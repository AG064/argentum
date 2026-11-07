//! This install's device identity, the base for pairing a PC and a phone.
//!
//! Each install has one Ed25519 key pair. The private key never leaves the
//! system credential store (Windows Credential Manager, Android Keystore). The
//! public key identifies the device to a paired one, and its fingerprint is what
//! the owner compares on both screens when pairing.
//!
//! An identity grants nothing by itself. What a paired device may ask for is
//! decided by each device's own persisted workspace policy.

pub mod pairing;

use argentum_platform::{PlatformError, SecretStore};
use argentum_security::SecretValue;
use ring::rand::SystemRandom;
use ring::signature::{Ed25519KeyPair, KeyPair, UnparsedPublicKey, ED25519};
use sha2::{Digest, Sha256};

/// Where the private key is kept in the system credential store.
pub const IDENTITY_SECRET_KEY: &str = "device/identity-ed25519";

#[derive(Debug, thiserror::Error)]
pub enum DeviceError {
    #[error("the system credential store could not be used")]
    Store(#[from] PlatformError),
    #[error("the saved device identity is not valid")]
    InvalidIdentity,
    #[error("a device identity could not be created")]
    Generation,
}

/// This install's identity. The private key is held only while signing.
pub struct DeviceIdentity {
    key_pair: Ed25519KeyPair,
}

impl std::fmt::Debug for DeviceIdentity {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("DeviceIdentity")
            .field("fingerprint", &self.fingerprint())
            .finish_non_exhaustive()
    }
}

impl DeviceIdentity {
    /// The saved identity, or a new one saved now.
    pub fn load_or_create(store: &dyn SecretStore) -> Result<Self, DeviceError> {
        if let Some(saved) = store.get(IDENTITY_SECRET_KEY)? {
            let pkcs8 = decode_hex(saved.expose()).ok_or(DeviceError::InvalidIdentity)?;
            let key_pair =
                Ed25519KeyPair::from_pkcs8(&pkcs8).map_err(|_| DeviceError::InvalidIdentity)?;
            return Ok(Self { key_pair });
        }
        let pkcs8 = Ed25519KeyPair::generate_pkcs8(&SystemRandom::new())
            .map_err(|_| DeviceError::Generation)?;
        let key_pair =
            Ed25519KeyPair::from_pkcs8(pkcs8.as_ref()).map_err(|_| DeviceError::Generation)?;
        store.set(
            IDENTITY_SECRET_KEY,
            SecretValue::new(encode_hex(pkcs8.as_ref())),
        )?;
        Ok(Self { key_pair })
    }

    pub fn public_key(&self) -> [u8; 32] {
        let mut key = [0_u8; 32];
        key.copy_from_slice(self.key_pair.public_key().as_ref());
        key
    }

    /// Short, readable, and the same on both devices for the same key.
    pub fn fingerprint(&self) -> String {
        fingerprint(&self.public_key())
    }

    pub fn sign(&self, message: &[u8]) -> [u8; 64] {
        let mut signature = [0_u8; 64];
        signature.copy_from_slice(self.key_pair.sign(message).as_ref());
        signature
    }
}

/// Five groups of four hex digits from the SHA-256 of the public key, for
/// example `3f2a-91bc-07de-55a1-c004`.
pub fn fingerprint(public_key: &[u8; 32]) -> String {
    let digest = Sha256::digest(public_key);
    let hex = encode_hex(&digest[..10]);
    hex.as_bytes()
        .chunks(4)
        .map(|chunk| std::str::from_utf8(chunk).unwrap_or_default())
        .collect::<Vec<_>>()
        .join("-")
}

/// Whether `signature` is this public key's signature of `message`.
pub fn verify(public_key: &[u8; 32], message: &[u8], signature: &[u8; 64]) -> bool {
    UnparsedPublicKey::new(&ED25519, public_key)
        .verify(message, signature)
        .is_ok()
}

fn encode_hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn decode_hex(text: &str) -> Option<Vec<u8>> {
    if !text.len().is_multiple_of(2) {
        return None;
    }
    (0..text.len())
        .step_by(2)
        .map(|index| u8::from_str_radix(text.get(index..index + 2)?, 16).ok())
        .collect()
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::sync::Mutex;

    #[derive(Default)]
    pub(crate) struct MemoryStore(Mutex<HashMap<String, String>>);

    impl SecretStore for MemoryStore {
        fn get(&self, key: &str) -> Result<Option<SecretValue>, PlatformError> {
            Ok(self
                .0
                .lock()
                .unwrap()
                .get(key)
                .cloned()
                .map(SecretValue::new))
        }
        fn set(&self, key: &str, value: SecretValue) -> Result<(), PlatformError> {
            self.0
                .lock()
                .unwrap()
                .insert(key.to_owned(), value.expose().to_owned());
            Ok(())
        }
        fn delete(&self, key: &str) -> Result<(), PlatformError> {
            self.0.lock().unwrap().remove(key);
            Ok(())
        }
    }

    #[test]
    fn an_identity_is_created_once_and_then_reused() {
        let store = MemoryStore::default();
        let first = DeviceIdentity::load_or_create(&store).unwrap();
        let second = DeviceIdentity::load_or_create(&store).unwrap();
        assert_eq!(first.public_key(), second.public_key());
        assert_eq!(first.fingerprint().len(), 24);
        assert_eq!(first.fingerprint().matches('-').count(), 4);
        let other = DeviceIdentity::load_or_create(&MemoryStore::default()).unwrap();
        assert_ne!(first.fingerprint(), other.fingerprint());
    }

    #[test]
    fn signatures_verify_only_for_their_key_and_message() {
        let store = MemoryStore::default();
        let identity = DeviceIdentity::load_or_create(&store).unwrap();
        let other = DeviceIdentity::load_or_create(&MemoryStore::default()).unwrap();
        let signature = identity.sign(b"pairing challenge");
        assert!(verify(
            &identity.public_key(),
            b"pairing challenge",
            &signature
        ));
        assert!(!verify(
            &identity.public_key(),
            b"another message",
            &signature
        ));
        assert!(!verify(
            &other.public_key(),
            b"pairing challenge",
            &signature
        ));
    }

    #[test]
    fn a_damaged_saved_identity_is_refused_not_replaced() {
        let store = MemoryStore::default();
        store
            .set(IDENTITY_SECRET_KEY, SecretValue::new("not a key"))
            .unwrap();
        assert!(matches!(
            DeviceIdentity::load_or_create(&store),
            Err(DeviceError::InvalidIdentity)
        ));
        assert_eq!(
            store.get(IDENTITY_SECRET_KEY).unwrap().unwrap().expose(),
            "not a key"
        );
    }

    #[test]
    fn debug_output_shows_the_fingerprint_not_the_key() {
        let store = MemoryStore::default();
        let identity = DeviceIdentity::load_or_create(&store).unwrap();
        let shown = format!("{identity:?}");
        assert!(shown.contains(&identity.fingerprint()));
        let saved = store.get(IDENTITY_SECRET_KEY).unwrap().unwrap();
        assert!(!shown.contains(saved.expose()));
    }
}
