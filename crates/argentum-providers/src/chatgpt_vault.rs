//! Versioned OS-secret-store records. No bearer state is stored in SQLite or files.
use argentum_platform::SecretStore;
use argentum_security::SecretValue;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use zeroize::Zeroizing;

use crate::chatgpt_auth::{AuthError, Credentials};
const INDEX: &str = "chatgpt/accounts";
const HOST: &str = "chatgpt/host";
const PENDING: &str = "chatgpt/pending-registration";
const CHUNK: usize = 1000;
const MAX: usize = 64 * 1024;

#[derive(Default, Serialize, Deserialize)]
pub(crate) struct Index {
    pub selected: Option<String>,
    pub accounts: Vec<Account>,
}
#[derive(Clone, Serialize, Deserialize)]
pub(crate) struct Account {
    pub id: String,
    pub client: String,
    pub subject: String,
}
#[derive(Serialize, Deserialize)]
struct Revision {
    id: uuid::Uuid,
    chunks: usize,
    sha: String,
}
fn read(store: &dyn SecretStore, key: &str) -> Result<Option<Zeroizing<String>>, AuthError> {
    store
        .get(key)
        .map_err(|_| AuthError::Storage)
        .map(|s| s.map(|s| Zeroizing::new(s.expose().to_owned())))
}
fn write<T: Serialize>(store: &dyn SecretStore, key: &str, value: &T) -> Result<(), AuthError> {
    let data = Zeroizing::new(serde_json::to_string(value).map_err(|_| AuthError::Storage)?);
    store
        .set(key, SecretValue::new(data.to_string()))
        .map_err(|_| AuthError::Storage)
}
pub(crate) fn host(store: &dyn SecretStore) -> Result<String, AuthError> {
    if let Some(value) = read(store, HOST)? {
        let canonical = super::chatgpt_auth::canonical_host_id(&value)?;
        if canonical.as_str() != value.as_str() {
            store
                .set(HOST, SecretValue::new(canonical.clone()))
                .map_err(|_| AuthError::Storage)?;
        }
        return Ok(canonical);
    }
    let value = uuid::Uuid::new_v4().urn().to_string();
    store
        .set(HOST, SecretValue::new(value.clone()))
        .map_err(|_| AuthError::Storage)?;
    Ok(value)
}
pub(crate) fn pending(store: &dyn SecretStore) -> Result<Option<String>, AuthError> {
    read(store, PENDING)?
        .map(|value| {
            if super::chatgpt_auth::valid_client(&value) {
                Ok(value.to_string())
            } else {
                Err(AuthError::Storage)
            }
        })
        .transpose()
}
pub(crate) fn retain_registration(store: &dyn SecretStore, client: &str) -> Result<(), AuthError> {
    if !super::chatgpt_auth::valid_client(client) {
        return Err(AuthError::Identity);
    }
    store
        .set(PENDING, SecretValue::new(client.to_owned()))
        .map_err(|_| AuthError::Storage)
}
pub(crate) fn retire_pending(store: &dyn SecretStore, client: &str) -> Result<(), AuthError> {
    if pending(store)?.as_deref() == Some(client) {
        store.delete(PENDING).map_err(|_| AuthError::Storage)?;
    }
    Ok(())
}
pub(crate) fn index(store: &dyn SecretStore) -> Result<Index, AuthError> {
    let Some(data) = read(store, INDEX)? else {
        return Ok(Index::default());
    };
    if data.len() > 1000 {
        return Err(AuthError::Storage);
    }
    let result: Index = serde_json::from_str(&data).map_err(|_| AuthError::Storage)?;
    if result.accounts.len() > 2
        || result.accounts.iter().any(|a| {
            !valid_id(&a.id)
                || !super::chatgpt_auth::valid_client(&a.client)
                || a.subject.is_empty()
        })
        || result
            .selected
            .as_ref()
            .is_some_and(|id| !result.accounts.iter().any(|a| &a.id == id))
    {
        return Err(AuthError::Storage);
    }
    Ok(result)
}
pub(crate) fn select(store: &dyn SecretStore, id: &str) -> Result<(), AuthError> {
    let mut value = index(store)?;
    if !value.accounts.iter().any(|a| a.id == id) {
        return Err(AuthError::Storage);
    }
    value.selected = Some(id.into());
    write(store, INDEX, &value)
}
fn valid_id(id: &str) -> bool {
    id.len() == 64 && id.bytes().all(|b| b.is_ascii_hexdigit())
}
fn identity(credentials: &Credentials) -> String {
    let mut hash = Sha256::new();
    hash.update(credentials.client_id.as_bytes());
    hash.update([0]);
    hash.update(credentials.subject.as_bytes());
    format!("{:x}", hash.finalize())
}
fn pointer(id: &str) -> String {
    format!("chatgpt/{id}/current")
}
fn revision(store: &dyn SecretStore, id: &str) -> Result<Option<Revision>, AuthError> {
    if !valid_id(id) {
        return Err(AuthError::Storage);
    }
    read(store, &pointer(id))?
        .map(|v| {
            let value: Revision = serde_json::from_str(&v).map_err(|_| AuthError::Storage)?;
            if value.id.is_nil()
                || value.chunks == 0
                || value.chunks > MAX.div_ceil(CHUNK)
                || value.sha.len() != 64
            {
                return Err(AuthError::Storage);
            }
            Ok(value)
        })
        .transpose()
}
fn cleanup(store: &dyn SecretStore, id: &str, revision: &Revision) {
    for i in 0..revision.chunks {
        let _ = store.delete(&format!("chatgpt/{id}/{}/{i}", revision.id));
    }
}
pub(crate) fn load(store: &dyn SecretStore, id: &str) -> Result<Option<Credentials>, AuthError> {
    let Some(rev) = revision(store, id)? else {
        return Ok(None);
    };
    let mut bytes = Zeroizing::new(String::new());
    for i in 0..rev.chunks {
        let chunk =
            read(store, &format!("chatgpt/{id}/{}/{i}", rev.id))?.ok_or(AuthError::Storage)?;
        if chunk.len() > CHUNK || bytes.len() + chunk.len() > MAX {
            return Err(AuthError::Storage);
        }
        bytes.push_str(&chunk);
    }
    if format!("{:x}", Sha256::digest(bytes.as_bytes())) != rev.sha {
        return Err(AuthError::Storage);
    }
    let credentials: Credentials = serde_json::from_str(&bytes).map_err(|_| AuthError::Storage)?;
    if identity(&credentials) != id {
        return Err(AuthError::Storage);
    }
    Ok(Some(credentials))
}
pub(crate) fn save(
    store: &dyn SecretStore,
    credentials: &Credentials,
) -> Result<String, AuthError> {
    let id = identity(credentials);
    let old = revision(store, &id)?;
    let mut catalogue = index(store)?;
    if !catalogue.accounts.iter().any(|a| a.id == id) && catalogue.accounts.len() >= 2 {
        return Err(AuthError::Storage);
    }
    if !catalogue.accounts.iter().any(|a| a.id == id) {
        catalogue.accounts.push(Account {
            id: id.clone(),
            client: credentials.client_id.clone(),
            subject: credentials.subject.clone(),
        });
    }
    catalogue.selected = Some(id.clone());
    if serde_json::to_vec(&catalogue).map_or(true, |v| v.len() > 1000) {
        return Err(AuthError::Storage);
    }
    // Escape non-ASCII before chunking so UTF-16 OS credential limits are bounded.
    let raw = Zeroizing::new(serde_json::to_string(credentials).map_err(|_| AuthError::Storage)?);
    let mut data = Zeroizing::new(String::new());
    for c in raw.chars() {
        if c.is_ascii() {
            data.push(c);
        } else {
            for u in c.encode_utf16(&mut [0; 2]) {
                data.push_str(&format!("\\u{u:04x}"));
            }
        }
    }
    if data.len() > MAX {
        return Err(AuthError::Storage);
    }
    let rev = Revision {
        id: uuid::Uuid::new_v4(),
        chunks: data.len().div_ceil(CHUNK),
        sha: format!("{:x}", Sha256::digest(data.as_bytes())),
    };
    for (i, chunk) in data.as_bytes().chunks(CHUNK).enumerate() {
        if store
            .set(
                &format!("chatgpt/{id}/{}/{i}", rev.id),
                SecretValue::new(
                    String::from_utf8(chunk.to_vec()).map_err(|_| AuthError::Storage)?,
                ),
            )
            .is_err()
        {
            cleanup(store, &id, &rev);
            return Err(AuthError::Storage);
        }
    }
    if write(store, &pointer(&id), &rev).is_err() {
        cleanup(store, &id, &rev);
        return Err(AuthError::Storage);
    }
    write(store, INDEX, &catalogue)?;
    if let Some(old) = old {
        cleanup(store, &id, &old);
    }
    Ok(id)
}
pub(crate) fn clear(store: &dyn SecretStore, id: &str) -> Result<(), AuthError> {
    if let Some(rev) = revision(store, id)? {
        store.delete(&pointer(id)).map_err(|_| AuthError::Storage)?;
        cleanup(store, id, &rev);
    }
    Ok(())
}

/// The owner thread keeps mutex ownership valid across async thread migration.
#[cfg(windows)]
pub(crate) struct Lock {
    release: Option<std::sync::mpsc::Sender<()>>,
    thread: Option<std::thread::JoinHandle<()>>,
}
#[cfg(windows)]
impl Lock {
    pub fn acquire() -> Result<Self, AuthError> {
        use windows_sys::Win32::{
            Foundation::{CloseHandle, WAIT_ABANDONED, WAIT_OBJECT_0},
            System::Threading::{CreateMutexW, ReleaseMutex, WaitForSingleObject},
        };
        let (ready_tx, ready_rx) = std::sync::mpsc::sync_channel(1);
        let (release, rx) = std::sync::mpsc::channel();
        let thread = std::thread::spawn(move || {
            let name: Vec<u16> = "Global\\Argentum.ChatGPT.Credentials.v1"
                .encode_utf16()
                .chain(Some(0))
                .collect();
            let handle = unsafe { CreateMutexW(std::ptr::null(), 0, name.as_ptr()) };
            if handle.is_null() {
                let _ = ready_tx.send(false);
                return;
            }
            let acquired = matches!(
                unsafe { WaitForSingleObject(handle, 30_000) },
                WAIT_OBJECT_0 | WAIT_ABANDONED
            );
            if ready_tx.send(acquired).is_ok() && acquired {
                let _ = rx.recv();
            }
            unsafe {
                if acquired {
                    ReleaseMutex(handle);
                }
                CloseHandle(handle);
            }
        });
        if ready_rx.recv().unwrap_or(false) {
            Ok(Self {
                release: Some(release),
                thread: Some(thread),
            })
        } else {
            let _ = thread.join();
            Err(AuthError::Storage)
        }
    }
}
#[cfg(windows)]
impl Drop for Lock {
    fn drop(&mut self) {
        self.release.take();
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}
// Do not pretend a process-local mutex serializes rotating tokens on other hosts.
#[cfg(not(windows))]
pub(crate) struct Lock;
#[cfg(not(windows))]
impl Lock {
    pub fn acquire() -> Result<Self, AuthError> {
        Err(AuthError::Storage)
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use std::sync::{
        atomic::{AtomicBool, Ordering},
        Mutex,
    };
    #[derive(Default)]
    pub(crate) struct MemoryStore {
        data: Mutex<std::collections::BTreeMap<String, String>>,
        fail_pointer: AtomicBool,
        fail_host: AtomicBool,
    }
    impl SecretStore for MemoryStore {
        fn get(&self, key: &str) -> Result<Option<SecretValue>, argentum_platform::PlatformError> {
            Ok(self
                .data
                .lock()
                .unwrap()
                .get(key)
                .cloned()
                .map(SecretValue::new))
        }
        fn set(
            &self,
            key: &str,
            value: SecretValue,
        ) -> Result<(), argentum_platform::PlatformError> {
            if key == HOST && self.fail_host.load(Ordering::SeqCst) {
                return Err(argentum_platform::PlatformError::SecureStorageUnavailable);
            }
            if key.ends_with("/current") && self.fail_pointer.load(Ordering::SeqCst) {
                return Err(argentum_platform::PlatformError::SecureStorageUnavailable);
            }
            assert!(value.expose().encode_utf16().count() <= 1000);
            self.data
                .lock()
                .unwrap()
                .insert(key.into(), value.expose().into());
            Ok(())
        }
        fn delete(&self, key: &str) -> Result<(), argentum_platform::PlatformError> {
            self.data.lock().unwrap().remove(key);
            Ok(())
        }
    }
    pub(crate) fn synthetic() -> Credentials {
        Credentials {
            subject: "subject".into(),
            client_id: "oaiapp_test".into(),
            access_token: "SYNTHETIC_ACCESS".repeat(400),
            refresh_token: "SYNTHETIC_REFRESH".into(),
            id_token: "SYNTHETIC_ID".into(),
            scopes: "chatgpt.tokens.use.direct".into(),
            expires_at: u64::MAX,
            earliest_refresh_at: 0,
        }
    }
    #[test]
    fn chunked_rotation_is_atomic_isolated_and_redacted() {
        let store = MemoryStore::default();
        let mut credentials = synthetic();
        let id = save(&store, &credentials).unwrap();
        assert_eq!(
            load(&store, &id).unwrap().unwrap().access_token,
            credentials.access_token
        );
        let host_id = host(&store).unwrap();
        assert_eq!(host(&store).unwrap(), host_id);
        store.fail_pointer.store(true, Ordering::SeqCst);
        credentials.refresh_token = "ROTATED_SYNTHETIC".into();
        assert!(save(&store, &credentials).is_err());
        assert_eq!(
            load(&store, &id).unwrap().unwrap().refresh_token,
            "SYNTHETIC_REFRESH"
        );
        store.fail_pointer.store(false, Ordering::SeqCst);
        save(&store, &credentials).unwrap();
        assert_eq!(
            load(&store, &id).unwrap().unwrap().refresh_token,
            "ROTATED_SYNTHETIC"
        );
        credentials.subject = "other".into();
        let other = save(&store, &credentials).unwrap();
        assert_ne!(id, other);
        select(&store, &id).unwrap();
        assert_eq!(index(&store).unwrap().selected, Some(id.clone()));
        clear(&store, &id).unwrap();
        assert!(load(&store, &id).unwrap().is_none());
        assert!(load(&store, &other).unwrap().is_some());
        assert_eq!(index(&store).unwrap().accounts.len(), 2);
        assert!(!format!("{credentials:?}").contains("SYNTHETIC"));
    }
    #[test]
    fn corrupted_or_missing_chunk_fails_closed() {
        let store = MemoryStore::default();
        let id = save(&store, &synthetic()).unwrap();
        let rev = revision(&store, &id).unwrap().unwrap();
        store
            .set(
                &format!("chatgpt/{id}/{}/0", rev.id),
                SecretValue::new("corrupted"),
            )
            .unwrap();
        assert!(load(&store, &id).is_err());
    }

    #[test]
    fn legacy_host_uuid_is_migrated_without_rotation_or_credential_changes() {
        let store = MemoryStore::default();
        let uuid = uuid::Uuid::new_v4();
        store.set(HOST, SecretValue::new(uuid.to_string())).unwrap();
        let id = save(&store, &synthetic()).unwrap();
        let before = store.data.lock().unwrap().clone();
        assert_eq!(host(&store).unwrap(), uuid.urn().to_string());
        assert_eq!(host(&store).unwrap(), uuid.urn().to_string());
        let after = store.data.lock().unwrap().clone();
        for (key, value) in before {
            if key != HOST {
                assert_eq!(after.get(&key), Some(&value));
            }
        }
        assert_eq!(
            load(&store, &id).unwrap().unwrap().refresh_token,
            "SYNTHETIC_REFRESH"
        );
        assert_eq!(index(&store).unwrap().selected, Some(id));
    }

    #[test]
    fn corrupt_host_identity_is_not_silently_replaced() {
        for invalid in [
            "not-a-host",
            "urn:uuid:00000000-0000-0000-0000-000000000000",
            "urn:uuid:6ba7b810-9dad-11d1-80b4-00c04fd430c8",
        ] {
            let store = MemoryStore::default();
            store.set(HOST, SecretValue::new(invalid)).unwrap();
            assert!(host(&store).is_err());
            assert_eq!(store.get(HOST).unwrap().unwrap().expose(), invalid);
        }
    }

    #[test]
    fn failed_host_migration_keeps_legacy_identity_and_does_not_continue() {
        let store = MemoryStore::default();
        let legacy = uuid::Uuid::new_v4().to_string();
        store.set(HOST, SecretValue::new(legacy.clone())).unwrap();
        store.fail_host.store(true, Ordering::SeqCst);
        assert!(host(&store).is_err());
        assert_eq!(store.get(HOST).unwrap().unwrap().expose(), legacy);
    }
    #[cfg(windows)]
    #[test]
    #[ignore = "invoked only by the isolated parent test with synthetic credentials"]
    fn os_store_fresh_process_worker() {
        let service = std::env::var("ARGENTUM_TEST_CHATGPT_SERVICE").unwrap();
        assert!(service.starts_with("com.argentum.test.chatgpt."));
        let store = argentum_platform::OsSecretStore::new(service);
        let _lock = Lock::acquire().unwrap();
        let catalogue = index(&store).unwrap();
        let id = catalogue.selected.unwrap();
        let credentials = load(&store, &id).unwrap().unwrap();
        assert!(credentials.access_token.starts_with("SYNTHETIC_ACCESS"));
        assert_eq!(credentials.refresh_token, "SYNTHETIC_REFRESH");
        assert!(host(&store).unwrap().starts_with("urn:uuid:"));
        assert_eq!(host(&store).unwrap().len(), 45);
        assert_eq!(
            host(&store).unwrap(),
            std::env::var("ARGENTUM_TEST_CHATGPT_HOST").unwrap()
        );
    }
    #[cfg(windows)]
    #[test]
    fn os_store_persistence_survives_fresh_process_without_plaintext_files() {
        let service = format!("com.argentum.test.chatgpt.{}", uuid::Uuid::new_v4());
        let store = argentum_platform::OsSecretStore::new(service.clone());
        let legacy = uuid::Uuid::new_v4();
        let id = {
            let _lock = Lock::acquire().unwrap();
            store
                .set(HOST, SecretValue::new(legacy.to_string()))
                .unwrap();
            assert_eq!(host(&store).unwrap(), legacy.urn().to_string());
            save(&store, &synthetic()).unwrap()
        };
        let status = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "chatgpt_vault::tests::os_store_fresh_process_worker",
                "--ignored",
                "--test-threads=1",
            ])
            .env("ARGENTUM_TEST_CHATGPT_SERVICE", service)
            .env("ARGENTUM_TEST_CHATGPT_HOST", legacy.urn().to_string())
            .output()
            .unwrap();
        // Remove only this uniquely named, synthetic test service.
        {
            let _lock = Lock::acquire().unwrap();
            clear(&store, &id).unwrap();
            store.delete(INDEX).unwrap();
            store.delete(HOST).unwrap();
        }
        assert!(
            status.status.success(),
            "fresh-process synthetic secure-store test failed"
        );
    }
}
