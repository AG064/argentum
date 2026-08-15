use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use argentum_security::SecretValue;
use directories::ProjectDirs;
use serde::{Deserialize, Serialize};
use thiserror::Error;

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
        let data_dir = dirs.data_dir().to_path_buf();
        Ok(Self {
            database: data_dir.join("argentum.db"),
            logs_dir: data_dir.join("logs"),
            workspace_config: data_dir.join("workspace.json"),
            data_dir,
        })
    }

    pub fn load_workspace(&self) -> Result<Option<PathBuf>, PlatformError> {
        let metadata = match fs::symlink_metadata(&self.workspace_config) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(_) => return Err(PlatformError::InvalidWorkspaceConfiguration),
        };
        if metadata.file_type().is_symlink()
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
        let canonical = validate_workspace(workspace.as_ref())?;
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
            if metadata.file_type().is_symlink() || !metadata.is_file() {
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
        Ok(canonical)
    }

    pub fn clear_workspace(&self) -> Result<(), PlatformError> {
        match fs::remove_file(&self.workspace_config) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(_) => Err(PlatformError::InvalidWorkspaceConfiguration),
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
struct WorkspaceConfig {
    path: PathBuf,
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

    #[cfg(not(any(
        target_os = "windows",
        target_os = "macos",
        target_os = "ios",
        target_os = "linux"
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
    fn provider_credential_keys_are_scoped_to_safe_profile_ids() {
        assert_eq!(
            provider_credential_key("minimax").expect("credential key"),
            "provider/minimax"
        );
        assert!(provider_credential_key("../secret").is_err());
        assert!(provider_credential_key("provider/key").is_err());
    }
}
