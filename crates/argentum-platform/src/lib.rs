use std::collections::BTreeMap;
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use argentum_security::SecretValue;
use argon2::{Algorithm, Argon2, Params, Version};
use chacha20poly1305::aead::{Aead, KeyInit, Payload};
use chacha20poly1305::{XChaCha20Poly1305, XNonce};
use directories::ProjectDirs;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use zeroize::{Zeroize, ZeroizeOnDrop, Zeroizing};

#[cfg(any(target_os = "android", test))]
mod android_authority;
mod android_secret_store;
mod android_workspace;
mod authority;
pub use android_secret_store::AndroidSecretStore;
pub use android_workspace::android_app_private_workspace;
#[cfg(target_os = "android")]
pub use android_workspace::android_app_private_workspace_for_app;
pub use android_workspace::ValidatedAndroidAppPaths;
#[cfg(windows)]
pub use authority::WindowsCurrentUserBridge;
pub use authority::{
    AuthorityDependentService, PlatformAuthorityBridge, PlatformAuthorityBroker,
    PlatformAuthorityFuture, PlatformAuthorityLease, PlatformCapabilityProbe,
};

const MAX_WORKSPACE_CONFIG_BYTES: u64 = 8 * 1024;
pub const SECRET_SERVICE: &str = "com.argentum.Argentum";

#[derive(Debug, Error)]
pub enum PlatformError {
    #[error("the operating system did not provide an application data directory")]
    MissingDataDirectory,
    #[error("secure storage is not available in this build")]
    SecureStorageUnavailable,
    #[error("secure storage operation failed: {0}")]
    SecureStorage(String),
    #[error("the saved workspace configuration is invalid")]
    InvalidWorkspaceConfiguration,
    #[error("the selected workspace is not an accessible directory")]
    WorkspaceUnavailable,
    #[error("the provider credential profile ID is invalid")]
    InvalidProviderCredentialProfile,
    #[error("the operating system credential prompt is not available in this build")]
    CredentialPromptUnavailable,
    #[error("the operating system credential prompt was cancelled")]
    CredentialPromptCancelled,
    #[error("the Android secret vault is unavailable")]
    VaultUnavailable,
    #[error("the Android secret vault is corrupt or uses an unsupported format")]
    VaultCorrupt,
    #[error("the Android secret vault is too large")]
    VaultTooLarge,
    #[error("the Android secret vault passphrase is invalid")]
    VaultPassphraseInvalid,
    #[error("the Android secret vault could not authenticate")]
    VaultAuthenticationFailed,
    #[error("the Android secret vault key is invalid")]
    VaultKeyInvalid,
    #[error("the Android secret vault value is invalid")]
    VaultValueInvalid,
    #[error("the platform authority probe is invalid")]
    InvalidAuthorityProbe,
    #[error("the operating system authority identity probe failed")]
    AuthorityIdentityProbeFailed,
    #[error("the initial authority generation must have ordinal 1")]
    InvalidInitialAuthorityGeneration,
    #[error("platform authority identifier generation failed")]
    AuthorityIdentifierUnavailable,
    #[error("the authority-dependent service ID is invalid")]
    InvalidAuthorityServiceId,
    #[error("the authority-dependent service ID does not match the mounted ID")]
    AuthorityDependentIdMismatch,
    #[error("the authority-dependent service is already mounted")]
    DuplicateAuthorityDependent,
    #[error("platform authority admission is closed")]
    AuthorityAdmissionClosed,
    #[error("platform authority is unavailable")]
    AuthorityUnavailable,
    #[error("platform authority lifecycle operation failed")]
    AuthorityLifecycleFailed,
    #[error("platform authority release was cancelled")]
    AuthorityReleaseCancelled,
    #[error("platform authority release failed")]
    AuthorityReleaseFailed,
    #[error("authority-dependent cleanup failed for '{service_id}'")]
    AuthorityDependentCleanup { service_id: String },
}

#[derive(Debug, Clone)]
pub struct AppPaths {
    pub data_dir: PathBuf,
    pub database: PathBuf,
    pub logs_dir: PathBuf,
    pub workspace_config: PathBuf,
}

impl AppPaths {
    pub fn discover() -> Result<Self, PlatformError> {
        let dirs = ProjectDirs::from("com", "Argentum", "Argentum")
            .ok_or(PlatformError::MissingDataDirectory)?;
        Ok(Self::from_data_dir(dirs.data_dir()))
    }

    pub fn from_data_dir(data_dir: impl Into<PathBuf>) -> Self {
        let data_dir = data_dir.into();
        Self {
            database: data_dir.join("argentum.db"),
            logs_dir: data_dir.join("logs"),
            workspace_config: data_dir.join("workspace.json"),
            data_dir,
        }
    }

    pub fn load_workspace(&self) -> Result<Option<PathBuf>, PlatformError> {
        let metadata = match fs::symlink_metadata(&self.workspace_config) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(_) => return Err(PlatformError::InvalidWorkspaceConfiguration),
        };
        if metadata_is_path_redirect(&metadata)
            || !metadata.is_file()
            || metadata.len() > MAX_WORKSPACE_CONFIG_BYTES
        {
            return Err(PlatformError::InvalidWorkspaceConfiguration);
        }
        let file = File::open(&self.workspace_config)
            .map_err(|_| PlatformError::InvalidWorkspaceConfiguration)?;
        let mut payload = String::new();
        file.take(MAX_WORKSPACE_CONFIG_BYTES + 1)
            .read_to_string(&mut payload)
            .map_err(|_| PlatformError::InvalidWorkspaceConfiguration)?;
        if payload.len() as u64 > MAX_WORKSPACE_CONFIG_BYTES {
            return Err(PlatformError::InvalidWorkspaceConfiguration);
        }
        let config = serde_json::from_str::<WorkspaceConfig>(&payload)
            .map_err(|_| PlatformError::InvalidWorkspaceConfiguration)?;
        validate_workspace(&config.path).map(Some)
    }

    pub fn save_workspace(&self, workspace: impl AsRef<Path>) -> Result<PathBuf, PlatformError> {
        self.save_workspace_with_operation_hooks(workspace, None, || {}, || {})
    }

    pub(crate) fn save_workspace_with_operation_hooks<F, G>(
        &self,
        workspace: impl AsRef<Path>,
        operation_marker: Option<&str>,
        after_canonicalize: F,
        after_commit: G,
    ) -> Result<PathBuf, PlatformError>
    where
        F: FnOnce(),
        G: FnOnce(),
    {
        let canonical = validate_workspace(workspace.as_ref())?;
        after_canonicalize();
        fs::create_dir_all(&self.data_dir)
            .map_err(|_| PlatformError::InvalidWorkspaceConfiguration)?;
        let temporary = self.workspace_config.with_file_name(format!(
            "{}.{}.{}.tmp",
            self.workspace_config
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("workspace.json"),
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|duration| duration.as_nanos())
                .unwrap_or_default()
        ));
        let payload = serde_json::to_vec(&WorkspaceConfig {
            path: canonical.clone(),
            operation_marker: operation_marker.map(str::to_owned),
        })
        .map_err(|_| PlatformError::InvalidWorkspaceConfiguration)?;
        let mut file = OpenOptions::new()
            .create(true)
            .create_new(true)
            .truncate(true)
            .write(true)
            .open(&temporary)
            .map_err(|_| PlatformError::InvalidWorkspaceConfiguration)?;
        if file
            .write_all(&payload)
            .and_then(|_| file.sync_all())
            .is_err()
        {
            drop(file);
            let _ = fs::remove_file(&temporary);
            return Err(PlatformError::InvalidWorkspaceConfiguration);
        }
        drop(file);
        if let Ok(metadata) = fs::symlink_metadata(&self.workspace_config) {
            if metadata_is_path_redirect(&metadata) || !metadata.is_file() {
                let _ = fs::remove_file(&temporary);
                return Err(PlatformError::InvalidWorkspaceConfiguration);
            }
            fs::remove_file(&self.workspace_config).map_err(|_| {
                let _ = fs::remove_file(&temporary);
                PlatformError::InvalidWorkspaceConfiguration
            })?;
        }
        if fs::rename(&temporary, &self.workspace_config).is_err() {
            let _ = fs::remove_file(&temporary);
            return Err(PlatformError::InvalidWorkspaceConfiguration);
        }
        after_commit();
        Ok(canonical)
    }

    pub fn clear_workspace(&self) -> Result<(), PlatformError> {
        if let Ok(metadata) = fs::symlink_metadata(&self.workspace_config) {
            if metadata_is_path_redirect(&metadata) || !metadata.is_file() {
                return Err(PlatformError::InvalidWorkspaceConfiguration);
            }
        }
        match fs::remove_file(&self.workspace_config) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(_) => Err(PlatformError::InvalidWorkspaceConfiguration),
        }
    }
}

fn metadata_is_path_redirect(metadata: &fs::Metadata) -> bool {
    metadata.file_type().is_symlink() || metadata_is_reparse_point(metadata)
}

#[cfg(windows)]
fn metadata_is_reparse_point(metadata: &fs::Metadata) -> bool {
    use std::os::windows::fs::MetadataExt;

    const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x0400;
    metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0
}

#[cfg(not(windows))]
fn metadata_is_reparse_point(_metadata: &fs::Metadata) -> bool {
    false
}

#[derive(Debug, Serialize, Deserialize)]
struct WorkspaceConfig {
    path: PathBuf,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    operation_marker: Option<String>,
}

fn validate_workspace(path: &Path) -> Result<PathBuf, PlatformError> {
    let canonical = fs::canonicalize(path).map_err(|_| PlatformError::WorkspaceUnavailable)?;
    if !canonical.is_dir() {
        return Err(PlatformError::WorkspaceUnavailable);
    }
    Ok(canonical)
}

pub fn provider_credential_key(profile_id: &str) -> Result<String, PlatformError> {
    let profile_id = profile_id.trim();
    if profile_id.is_empty()
        || profile_id.len() > 64
        || !profile_id.chars().all(|character| {
            character.is_ascii_alphanumeric() || matches!(character, '-' | '_' | '.')
        })
    {
        return Err(PlatformError::InvalidProviderCredentialProfile);
    }
    Ok(format!("provider/{profile_id}"))
}

pub fn credential_prompt_target(profile_id: &str) -> Result<String, PlatformError> {
    let key = provider_credential_key(profile_id)?;
    let target = format!("{SECRET_SERVICE}/{key}");
    if target.len() > 256 {
        return Err(PlatformError::InvalidProviderCredentialProfile);
    }
    Ok(target)
}

#[cfg(windows)]
pub fn prompt_provider_credential(profile_id: &str) -> Result<SecretValue, PlatformError> {
    use std::mem::size_of;
    use std::ptr::{null, null_mut};

    use windows_sys::Win32::Foundation::ERROR_CANCELLED;
    use windows_sys::Win32::Security::Credentials::{
        CredUIPromptForCredentialsW, CREDUI_FLAGS_DO_NOT_PERSIST, CREDUI_FLAGS_GENERIC_CREDENTIALS,
        CREDUI_FLAGS_PASSWORD_ONLY_OK, CREDUI_INFOW,
    };
    use zeroize::Zeroizing;

    const USERNAME_CAPACITY: usize = 256;
    const PASSWORD_CAPACITY: usize = 16 * 1024;
    const CREDENTIAL_PROMPT_FLAGS: u32 = CREDUI_FLAGS_GENERIC_CREDENTIALS
        | CREDUI_FLAGS_PASSWORD_ONLY_OK
        | CREDUI_FLAGS_DO_NOT_PERSIST;

    let target = credential_prompt_target(profile_id)?;
    let mut target_wide = target.encode_utf16().collect::<Vec<_>>();
    target_wide.push(0);
    let mut username = Zeroizing::new(vec![0u16; USERNAME_CAPACITY]);
    let mut profile_wide = profile_id.encode_utf16().collect::<Vec<_>>();
    if profile_wide.len() + 1 > username.len() {
        return Err(PlatformError::InvalidProviderCredentialProfile);
    }
    username[..profile_wide.len()].copy_from_slice(&profile_wide);
    let mut password = Zeroizing::new(vec![0u16; PASSWORD_CAPACITY]);
    let mut save = 0;
    let prompt_info = CREDUI_INFOW {
        cbSize: size_of::<CREDUI_INFOW>() as u32,
        hwndParent: null_mut(),
        pszMessageText: null(),
        pszCaptionText: null(),
        hbmBanner: null_mut(),
    };
    let result = unsafe {
        CredUIPromptForCredentialsW(
            &prompt_info,
            target_wide.as_ptr(),
            null(),
            0,
            username.as_mut_ptr(),
            username.len() as u32,
            password.as_mut_ptr(),
            password.len() as u32,
            &mut save,
            CREDENTIAL_PROMPT_FLAGS,
        )
    };
    profile_wide.zeroize();
    if result == ERROR_CANCELLED {
        return Err(PlatformError::CredentialPromptCancelled);
    }
    if result != 0 {
        return Err(PlatformError::SecureStorage(format!(
            "credential prompt failed with Windows error {result}"
        )));
    }
    let username_end = username
        .iter()
        .position(|character| *character == 0)
        .unwrap_or(username.len());
    if String::from_utf16(&username[..username_end])
        .ok()
        .as_deref()
        != Some(profile_id)
    {
        return Err(PlatformError::SecureStorage(
            "credential prompt returned an unexpected profile ID".into(),
        ));
    }
    let password_end = password
        .iter()
        .position(|character| *character == 0)
        .unwrap_or(password.len());
    let value = Zeroizing::new(String::from_utf16(&password[..password_end]).map_err(|_| {
        PlatformError::SecureStorage("credential prompt returned invalid text".into())
    })?);
    if value.trim().is_empty() {
        return Err(PlatformError::SecureStorage(
            "credential prompt returned an empty credential".into(),
        ));
    }
    Ok(SecretValue::new(value.as_str()))
}

#[cfg(not(windows))]
pub fn prompt_provider_credential(_profile_id: &str) -> Result<SecretValue, PlatformError> {
    Err(PlatformError::CredentialPromptUnavailable)
}

pub trait SecretStore: Send + Sync {
    fn get(&self, key: &str) -> Result<Option<SecretValue>, PlatformError>;
    fn set(&self, key: &str, value: SecretValue) -> Result<(), PlatformError>;
    fn delete(&self, key: &str) -> Result<(), PlatformError>;
}

#[cfg(any(
    target_os = "windows",
    target_os = "macos",
    target_os = "ios",
    target_os = "linux"
))]
#[derive(Debug, Clone)]
pub struct OsSecretStore {
    service: String,
}

#[cfg(any(
    target_os = "windows",
    target_os = "macos",
    target_os = "ios",
    target_os = "linux"
))]
impl OsSecretStore {
    pub fn new(service: impl Into<String>) -> Self {
        Self {
            service: service.into(),
        }
    }

    fn entry(&self, key: &str) -> Result<keyring::Entry, PlatformError> {
        keyring::Entry::new(&self.service, key)
            .map_err(|error| PlatformError::SecureStorage(error.to_string()))
    }
}

#[cfg(any(
    target_os = "windows",
    target_os = "macos",
    target_os = "ios",
    target_os = "linux"
))]
impl SecretStore for OsSecretStore {
    fn get(&self, key: &str) -> Result<Option<SecretValue>, PlatformError> {
        match self.entry(key)?.get_password() {
            Ok(value) => Ok(Some(SecretValue::new(value))),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(error) => Err(PlatformError::SecureStorage(error.to_string())),
        }
    }

    fn set(&self, key: &str, value: SecretValue) -> Result<(), PlatformError> {
        self.entry(key)?
            .set_password(value.expose())
            .map_err(|error| PlatformError::SecureStorage(error.to_string()))
    }

    fn delete(&self, key: &str) -> Result<(), PlatformError> {
        match self.entry(key)?.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(error) => Err(PlatformError::SecureStorage(error.to_string())),
        }
    }
}

pub fn default_secret_store(service: impl Into<String>) -> Arc<dyn SecretStore> {
    #[cfg(any(
        target_os = "windows",
        target_os = "macos",
        target_os = "ios",
        target_os = "linux"
    ))]
    {
        Arc::new(OsSecretStore::new(service))
    }

    #[cfg(target_os = "android")]
    {
        let _ = service;
        let Ok(passphrase) = std::env::var("ARGENTUM_VAULT_PASSPHRASE") else {
            return Arc::new(UnavailableSecretStore);
        };
        let passphrase = SecretValue::new(passphrase);
        let Ok(paths) = AppPaths::discover() else {
            return Arc::new(UnavailableSecretStore);
        };
        match AndroidVaultSecretStore::new(paths.data_dir.join("secrets.vault"), passphrase) {
            Ok(store) => Arc::new(store),
            Err(_) => Arc::new(UnavailableSecretStore),
        }
    }

    #[cfg(not(any(
        target_os = "windows",
        target_os = "macos",
        target_os = "ios",
        target_os = "linux",
        target_os = "android"
    )))]
    {
        let _ = service;
        Arc::new(UnavailableSecretStore)
    }
}

#[derive(Debug, Default, Clone, Copy)]
pub struct UnavailableSecretStore;

impl SecretStore for UnavailableSecretStore {
    fn get(&self, _key: &str) -> Result<Option<SecretValue>, PlatformError> {
        Err(PlatformError::SecureStorageUnavailable)
    }

    fn set(&self, _key: &str, _value: SecretValue) -> Result<(), PlatformError> {
        Err(PlatformError::SecureStorageUnavailable)
    }

    fn delete(&self, _key: &str) -> Result<(), PlatformError> {
        Err(PlatformError::SecureStorageUnavailable)
    }
}

const VAULT_MAGIC: &[u8; 8] = b"AGVAULT1";
const VAULT_VERSION: u8 = 1;
const VAULT_SALT_BYTES: usize = 16;
const VAULT_NONCE_BYTES: usize = 24;
const VAULT_LENGTH_BYTES: usize = 4;
const VAULT_HEADER_BYTES: usize =
    VAULT_MAGIC.len() + 1 + VAULT_SALT_BYTES + VAULT_NONCE_BYTES + VAULT_LENGTH_BYTES;
const VAULT_AAD: &[u8] = b"com.argentum.Argentum/android-vault/v1";
const MAX_VAULT_BYTES: u64 = 8 * 1024 * 1024;
const MAX_VAULT_PLAINTEXT_BYTES: usize = 4 * 1024 * 1024;
const MAX_VAULT_ENTRIES: usize = 128;
const MAX_VAULT_KEY_BYTES: usize = 256;
const MAX_VAULT_VALUE_BYTES: usize = 16 * 1024;

#[derive(Clone)]
pub struct AndroidVaultSecretStore {
    path: PathBuf,
    passphrase: SecretValue,
    values: Arc<Mutex<BTreeMap<String, SecretValue>>>,
}

impl std::fmt::Debug for AndroidVaultSecretStore {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("AndroidVaultSecretStore")
            .field("path", &self.path)
            .field(
                "entry_count",
                &self
                    .values
                    .lock()
                    .map(|values| values.len())
                    .unwrap_or_default(),
            )
            .finish_non_exhaustive()
    }
}

impl AndroidVaultSecretStore {
    pub fn new(path: impl AsRef<Path>, passphrase: SecretValue) -> Result<Self, PlatformError> {
        let path = validate_vault_path(path.as_ref())?;
        validate_vault_passphrase(&passphrase)?;
        let values = load_vault_values(&path, &passphrase)?;
        Ok(Self {
            path,
            passphrase,
            values: Arc::new(Mutex::new(values)),
        })
    }

    fn persist(&self, values: &BTreeMap<String, SecretValue>) -> Result<(), PlatformError> {
        let encoded = encode_vault(values, &self.passphrase)?;
        write_vault_file(&self.path, &encoded)
    }
}

impl SecretStore for AndroidVaultSecretStore {
    fn get(&self, key: &str) -> Result<Option<SecretValue>, PlatformError> {
        validate_vault_key(key)?;
        let values = self
            .values
            .lock()
            .map_err(|_| PlatformError::VaultUnavailable)?;
        Ok(values.get(key).cloned())
    }

    fn set(&self, key: &str, value: SecretValue) -> Result<(), PlatformError> {
        validate_vault_key(key)?;
        validate_vault_value(&value)?;
        let mut updated = self
            .values
            .lock()
            .map_err(|_| PlatformError::VaultUnavailable)?
            .clone();
        updated.insert(key.to_owned(), value);
        self.persist(&updated)?;
        *self
            .values
            .lock()
            .map_err(|_| PlatformError::VaultUnavailable)? = updated;
        Ok(())
    }

    fn delete(&self, key: &str) -> Result<(), PlatformError> {
        validate_vault_key(key)?;
        let mut updated = self
            .values
            .lock()
            .map_err(|_| PlatformError::VaultUnavailable)?
            .clone();
        if updated.remove(key).is_none() {
            return Ok(());
        }
        self.persist(&updated)?;
        *self
            .values
            .lock()
            .map_err(|_| PlatformError::VaultUnavailable)? = updated;
        Ok(())
    }
}

#[derive(Debug, Serialize, Deserialize, Zeroize, ZeroizeOnDrop)]
struct VaultEntry {
    key: String,
    value: String,
}

fn validate_vault_path(path: &Path) -> Result<PathBuf, PlatformError> {
    if !path.is_absolute()
        || path.as_os_str().to_string_lossy().len() > 2_048
        || path.file_name().is_none()
    {
        return Err(PlatformError::VaultUnavailable);
    }
    if let Ok(metadata) = fs::symlink_metadata(path) {
        if metadata.file_type().is_symlink() || !metadata.is_file() {
            return Err(PlatformError::VaultCorrupt);
        }
    }
    Ok(path.to_path_buf())
}

fn validate_vault_passphrase(passphrase: &SecretValue) -> Result<(), PlatformError> {
    let value = passphrase.expose();
    if value.trim().is_empty() || value.len() < 8 || value.len() > 4 * 1024 {
        return Err(PlatformError::VaultPassphraseInvalid);
    }
    Ok(())
}

fn validate_vault_key(key: &str) -> Result<(), PlatformError> {
    if key.is_empty()
        || key.len() > MAX_VAULT_KEY_BYTES
        || key.contains("//")
        || key
            .split('/')
            .any(|part| part.is_empty() || part == "." || part == "..")
        || !key.chars().all(|character| {
            character.is_ascii_alphanumeric() || matches!(character, '/' | '-' | '_' | '.')
        })
    {
        return Err(PlatformError::VaultKeyInvalid);
    }
    Ok(())
}

fn validate_vault_value(value: &SecretValue) -> Result<(), PlatformError> {
    if value.expose().is_empty()
        || value.expose().len() > MAX_VAULT_VALUE_BYTES
        || value.expose().chars().any(char::is_control)
    {
        return Err(PlatformError::VaultValueInvalid);
    }
    Ok(())
}

fn load_vault_values(
    path: &Path,
    passphrase: &SecretValue,
) -> Result<BTreeMap<String, SecretValue>, PlatformError> {
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(BTreeMap::new()),
        Err(_) => return Err(PlatformError::VaultUnavailable),
    };
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err(PlatformError::VaultCorrupt);
    }
    if metadata.len() > MAX_VAULT_BYTES {
        return Err(PlatformError::VaultTooLarge);
    }
    let mut bytes = Vec::new();
    let file = File::open(path).map_err(|_| PlatformError::VaultUnavailable)?;
    file.take(MAX_VAULT_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| PlatformError::VaultUnavailable)?;
    if bytes.len() as u64 > MAX_VAULT_BYTES {
        return Err(PlatformError::VaultTooLarge);
    }
    decode_vault(&bytes, passphrase)
}

fn decode_vault(
    bytes: &[u8],
    passphrase: &SecretValue,
) -> Result<BTreeMap<String, SecretValue>, PlatformError> {
    if bytes.len() < VAULT_HEADER_BYTES || &bytes[..VAULT_MAGIC.len()] != VAULT_MAGIC {
        return Err(PlatformError::VaultCorrupt);
    }
    let version_offset = VAULT_MAGIC.len();
    if bytes[version_offset] != VAULT_VERSION {
        return Err(PlatformError::VaultCorrupt);
    }
    let salt_start = version_offset + 1;
    let nonce_start = salt_start + VAULT_SALT_BYTES;
    let length_start = nonce_start + VAULT_NONCE_BYTES;
    let ciphertext_start = length_start + VAULT_LENGTH_BYTES;
    let ciphertext_len = u32::from_le_bytes(
        bytes[length_start..ciphertext_start]
            .try_into()
            .map_err(|_| PlatformError::VaultCorrupt)?,
    ) as usize;
    if ciphertext_len < 16
        || ciphertext_len > MAX_VAULT_PLAINTEXT_BYTES.saturating_add(16)
        || ciphertext_start.saturating_add(ciphertext_len) != bytes.len()
    {
        return Err(PlatformError::VaultCorrupt);
    }
    let mut key = derive_vault_key(passphrase, &bytes[salt_start..nonce_start])?;
    let cipher =
        XChaCha20Poly1305::new_from_slice(&key).map_err(|_| PlatformError::VaultCorrupt)?;
    let plaintext = cipher
        .decrypt(
            XNonce::from_slice(&bytes[nonce_start..length_start]),
            Payload {
                msg: &bytes[ciphertext_start..],
                aad: VAULT_AAD,
            },
        )
        .map_err(|_| PlatformError::VaultAuthenticationFailed);
    key.zeroize();
    let plaintext = Zeroizing::new(plaintext?);
    if plaintext.len() > MAX_VAULT_PLAINTEXT_BYTES {
        return Err(PlatformError::VaultTooLarge);
    }
    let entries = serde_json::from_slice::<Vec<VaultEntry>>(&plaintext)
        .map_err(|_| PlatformError::VaultCorrupt)?;
    if entries.len() > MAX_VAULT_ENTRIES {
        return Err(PlatformError::VaultCorrupt);
    }
    let mut values = BTreeMap::new();
    for entry in entries {
        let key = entry.key.clone();
        let value = SecretValue::new(entry.value.clone());
        validate_vault_key(&key)?;
        validate_vault_value(&value)?;
        if values.insert(key, value).is_some() {
            return Err(PlatformError::VaultCorrupt);
        }
    }
    Ok(values)
}

fn encode_vault(
    values: &BTreeMap<String, SecretValue>,
    passphrase: &SecretValue,
) -> Result<Vec<u8>, PlatformError> {
    if values.len() > MAX_VAULT_ENTRIES {
        return Err(PlatformError::VaultTooLarge);
    }
    let entries = values
        .iter()
        .map(|(key, value)| {
            validate_vault_key(key)?;
            validate_vault_value(value)?;
            Ok(VaultEntry {
                key: key.clone(),
                value: value.expose().to_owned(),
            })
        })
        .collect::<Result<Vec<_>, PlatformError>>()?;
    let plaintext =
        Zeroizing::new(serde_json::to_vec(&entries).map_err(|_| PlatformError::VaultUnavailable)?);
    if plaintext.len() > MAX_VAULT_PLAINTEXT_BYTES {
        return Err(PlatformError::VaultTooLarge);
    }
    let mut salt = [0u8; VAULT_SALT_BYTES];
    let mut nonce = [0u8; VAULT_NONCE_BYTES];
    getrandom::fill(&mut salt).map_err(|_| PlatformError::VaultUnavailable)?;
    getrandom::fill(&mut nonce).map_err(|_| PlatformError::VaultUnavailable)?;
    let mut key = derive_vault_key(passphrase, &salt)?;
    let cipher =
        XChaCha20Poly1305::new_from_slice(&key).map_err(|_| PlatformError::VaultUnavailable)?;
    let ciphertext = cipher
        .encrypt(
            XNonce::from_slice(&nonce),
            Payload {
                msg: &plaintext,
                aad: VAULT_AAD,
            },
        )
        .map_err(|_| PlatformError::VaultUnavailable);
    key.zeroize();
    let ciphertext = ciphertext?;
    if ciphertext.len() > u32::MAX as usize
        || VAULT_HEADER_BYTES.saturating_add(ciphertext.len()) as u64 > MAX_VAULT_BYTES
    {
        return Err(PlatformError::VaultTooLarge);
    }
    let mut output = Vec::with_capacity(VAULT_HEADER_BYTES + ciphertext.len());
    output.extend_from_slice(VAULT_MAGIC);
    output.push(VAULT_VERSION);
    output.extend_from_slice(&salt);
    output.extend_from_slice(&nonce);
    output.extend_from_slice(&(ciphertext.len() as u32).to_le_bytes());
    output.extend_from_slice(&ciphertext);
    Ok(output)
}

fn derive_vault_key(passphrase: &SecretValue, salt: &[u8]) -> Result<[u8; 32], PlatformError> {
    let params =
        Params::new(19 * 1024, 2, 1, Some(32)).map_err(|_| PlatformError::VaultUnavailable)?;
    let argon = Argon2::new(Algorithm::Argon2id, Version::V0x13, params);
    let mut key = [0u8; 32];
    argon
        .hash_password_into(passphrase.expose().as_bytes(), salt, &mut key)
        .map_err(|_| PlatformError::VaultUnavailable)?;
    Ok(key)
}

fn write_vault_file(path: &Path, bytes: &[u8]) -> Result<(), PlatformError> {
    if bytes.len() as u64 > MAX_VAULT_BYTES {
        return Err(PlatformError::VaultTooLarge);
    }
    let parent = path.parent().ok_or(PlatformError::VaultUnavailable)?;
    fs::create_dir_all(parent).map_err(|_| PlatformError::VaultUnavailable)?;
    if let Ok(metadata) = fs::symlink_metadata(path) {
        if metadata.file_type().is_symlink() || !metadata.is_file() {
            return Err(PlatformError::VaultCorrupt);
        }
    }
    let temporary = path.with_file_name(format!(
        ".{}.{}.{}.tmp",
        path.file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("vault"),
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or_default()
    ));
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temporary)
        .map_err(|_| PlatformError::VaultUnavailable)?;
    let write_result = file.write_all(bytes).and_then(|_| file.sync_all());
    if write_result.is_err() {
        drop(file);
        let _ = fs::remove_file(&temporary);
        return Err(PlatformError::VaultUnavailable);
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if fs::set_permissions(&temporary, fs::Permissions::from_mode(0o600)).is_err() {
            drop(file);
            let _ = fs::remove_file(&temporary);
            return Err(PlatformError::VaultUnavailable);
        }
    }
    drop(file);
    if replace_vault_file(&temporary, path).is_err() {
        let _ = fs::remove_file(&temporary);
        return Err(PlatformError::VaultUnavailable);
    }
    Ok(())
}

#[cfg(not(windows))]
fn replace_vault_file(temporary: &Path, path: &Path) -> std::io::Result<()> {
    fs::rename(temporary, path)
}

#[cfg(windows)]
fn replace_vault_file(temporary: &Path, path: &Path) -> std::io::Result<()> {
    use std::os::windows::ffi::OsStrExt;
    use std::ptr::null;

    use windows_sys::Win32::Storage::FileSystem::{
        MoveFileExW, ReplaceFileW, MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH,
        REPLACEFILE_WRITE_THROUGH,
    };

    let mut temporary_wide = temporary.as_os_str().encode_wide().collect::<Vec<_>>();
    let mut path_wide = path.as_os_str().encode_wide().collect::<Vec<_>>();
    temporary_wide.push(0);
    path_wide.push(0);
    let result = unsafe {
        if path.exists() {
            ReplaceFileW(
                path_wide.as_ptr(),
                temporary_wide.as_ptr(),
                null(),
                REPLACEFILE_WRITE_THROUGH,
                null(),
                null(),
            )
        } else {
            MoveFileExW(
                temporary_wide.as_ptr(),
                path_wide.as_ptr(),
                MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
            )
        }
    };
    if result == 0 {
        Err(std::io::Error::last_os_error())
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn workspace_selection_persists_as_a_bounded_validated_path() {
        let data = tempfile::tempdir().expect("data directory");
        let workspace = tempfile::tempdir().expect("workspace");
        let paths = AppPaths {
            data_dir: data.path().to_path_buf(),
            database: data.path().join("argentum.db"),
            logs_dir: data.path().join("logs"),
            workspace_config: data.path().join("workspace.json"),
        };

        assert_eq!(paths.load_workspace().expect("empty config"), None);
        let saved = paths
            .save_workspace(workspace.path())
            .expect("save workspace");
        assert_eq!(
            saved,
            std::fs::canonicalize(workspace.path()).expect("canonical")
        );
        assert_eq!(paths.load_workspace().expect("load workspace"), Some(saved));
    }

    #[test]
    fn workspace_config_rejects_missing_or_malformed_paths() {
        let data = tempfile::tempdir().expect("data directory");
        let paths = AppPaths {
            data_dir: data.path().to_path_buf(),
            database: data.path().join("argentum.db"),
            logs_dir: data.path().join("logs"),
            workspace_config: data.path().join("workspace.json"),
        };
        std::fs::write(&paths.workspace_config, "not json").expect("malformed config");
        assert!(matches!(
            paths.load_workspace(),
            Err(PlatformError::InvalidWorkspaceConfiguration)
        ));
    }

    #[test]
    fn workspace_config_redirect_is_rejected_without_touching_its_target() {
        let data = tempfile::tempdir().expect("data directory");
        let workspace = tempfile::tempdir().expect("workspace");
        let outside = tempfile::tempdir().expect("outside directory");
        let target = outside.path().join("outside-config.json");
        std::fs::write(&target, b"unchanged").expect("outside marker");
        let paths = AppPaths::from_data_dir(data.path());
        if !create_workspace_config_link(&target, &paths.workspace_config) {
            return;
        }

        assert!(paths.load_workspace().is_err());
        assert!(paths.save_workspace(workspace.path()).is_err());
        assert!(paths.clear_workspace().is_err());
        assert_eq!(
            std::fs::read(&target).expect("outside marker remains"),
            b"unchanged"
        );
    }

    #[cfg(windows)]
    fn create_workspace_config_link(target: &Path, link: &Path) -> bool {
        std::os::windows::fs::symlink_file(target, link).is_ok()
    }

    #[cfg(unix)]
    fn create_workspace_config_link(target: &Path, link: &Path) -> bool {
        std::os::unix::fs::symlink(target, link).is_ok()
    }

    #[cfg(not(any(windows, unix)))]
    fn create_workspace_config_link(_target: &Path, _link: &Path) -> bool {
        false
    }

    #[test]
    fn provider_credential_keys_are_scoped_to_safe_profile_ids() {
        assert_eq!(
            provider_credential_key("minimax").expect("credential key"),
            "provider/minimax"
        );
        assert!(provider_credential_key("../secret").is_err());
        assert!(provider_credential_key("provider/key").is_err());
    }

    #[test]
    fn credential_prompt_target_is_scoped_and_bounded() {
        assert_eq!(
            credential_prompt_target("minimax").expect("prompt target"),
            "com.argentum.Argentum/provider/minimax"
        );
        for profile_id in ["", "../secret", "provider/key"] {
            assert!(credential_prompt_target(profile_id).is_err());
        }
        let long_profile_id = "x".repeat(65);
        assert!(credential_prompt_target(&long_profile_id).is_err());
    }

    #[test]
    fn android_vault_round_trips_and_hides_plaintext() {
        let directory = tempfile::tempdir().expect("vault directory");
        let path = directory.path().join("vault.bin");
        let secret = "vault-secret-value";
        let vault =
            AndroidVaultSecretStore::new(&path, SecretValue::new("passphrase")).expect("new vault");
        vault
            .set("provider/openai", SecretValue::new(secret))
            .expect("write vault");
        assert_eq!(
            vault
                .get("provider/openai")
                .expect("read vault")
                .expect("secret")
                .expose(),
            secret
        );
        let bytes = std::fs::read(&path).expect("vault bytes");
        assert!(!bytes
            .windows(secret.len())
            .any(|window| window == secret.as_bytes()));

        drop(vault);
        let reopened = AndroidVaultSecretStore::new(&path, SecretValue::new("passphrase"))
            .expect("reopen vault");
        assert_eq!(
            reopened
                .get("provider/openai")
                .expect("read reopened vault")
                .expect("reopened secret")
                .expose(),
            secret
        );
    }

    #[test]
    fn android_vault_rejects_wrong_passphrase_corruption_and_oversized_files() {
        let directory = tempfile::tempdir().expect("vault directory");
        let path = directory.path().join("vault.bin");
        let vault =
            AndroidVaultSecretStore::new(&path, SecretValue::new("passphrase")).expect("new vault");
        vault
            .set("provider/deepseek", SecretValue::new("secret"))
            .expect("write vault");
        drop(vault);

        assert!(matches!(
            AndroidVaultSecretStore::new(&path, SecretValue::new("wrong-passphrase")),
            Err(PlatformError::VaultAuthenticationFailed)
        ));
        std::fs::write(&path, b"corrupt").expect("corrupt vault");
        assert!(matches!(
            AndroidVaultSecretStore::new(&path, SecretValue::new("passphrase")),
            Err(PlatformError::VaultCorrupt)
        ));
        let oversized = directory.path().join("oversized.bin");
        let file = File::create(&oversized).expect("oversized vault");
        file.set_len(MAX_VAULT_BYTES + 1).expect("set file size");
        drop(file);
        assert!(matches!(
            AndroidVaultSecretStore::new(&oversized, SecretValue::new("passphrase")),
            Err(PlatformError::VaultTooLarge)
        ));
    }

    #[test]
    fn android_vault_delete_and_replacement_leave_one_valid_file() {
        let directory = tempfile::tempdir().expect("vault directory");
        let path = directory.path().join("vault.bin");
        let vault =
            AndroidVaultSecretStore::new(&path, SecretValue::new("passphrase")).expect("new vault");
        vault
            .set("provider/minimax", SecretValue::new("first"))
            .expect("first write");
        vault
            .set("provider/minimax", SecretValue::new("second"))
            .expect("replacement write");
        vault.delete("provider/minimax").expect("delete");
        assert!(vault
            .get("provider/minimax")
            .expect("read deleted value")
            .is_none());
        let temporary_files = std::fs::read_dir(directory.path())
            .expect("vault directory entries")
            .filter_map(Result::ok)
            .filter(|entry| entry.file_name().to_string_lossy().contains(".tmp"))
            .count();
        assert_eq!(temporary_files, 0);
    }
}
