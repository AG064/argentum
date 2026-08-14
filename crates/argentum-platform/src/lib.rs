use std::path::PathBuf;

use argentum_security::SecretValue;
use directories::ProjectDirs;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum PlatformError {
    #[error("the operating system did not provide an application data directory")]
    MissingDataDirectory,
    #[error("secure storage is not available in this build")]
    SecureStorageUnavailable,
    #[error("secure storage operation failed: {0}")]
    SecureStorage(String),
}

#[derive(Debug, Clone)]
pub struct AppPaths {
    pub data_dir: PathBuf,
    pub database: PathBuf,
    pub logs_dir: PathBuf,
}

impl AppPaths {
    pub fn discover() -> Result<Self, PlatformError> {
        let dirs = ProjectDirs::from("com", "Argentum", "Argentum")
            .ok_or(PlatformError::MissingDataDirectory)?;
        let data_dir = dirs.data_dir().to_path_buf();
        Ok(Self {
            database: data_dir.join("argentum.db"),
            logs_dir: data_dir.join("logs"),
            data_dir,
        })
    }
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

pub fn default_secret_store(service: impl Into<String>) -> Box<dyn SecretStore> {
    #[cfg(any(
        target_os = "windows",
        target_os = "macos",
        target_os = "ios",
        target_os = "linux"
    ))]
    {
        Box::new(OsSecretStore::new(service))
    }

    #[cfg(not(any(
        target_os = "windows",
        target_os = "macos",
        target_os = "ios",
        target_os = "linux"
    )))]
    {
        let _ = service;
        Box::new(UnavailableSecretStore)
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
