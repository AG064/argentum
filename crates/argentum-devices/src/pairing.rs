// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 AG064

//! The pairing exchange between two installs, without the transport.
//!
//! 1. Each device sends its public key and a fresh random nonce.
//! 2. Each signs the transcript: a fixed label, the initiator's key and nonce,
//!    then the responder's key and nonce.
//! 3. Each verifies the other's signature over the same transcript.
//! 4. Both show a six-digit code derived from the transcript. The owner checks
//!    that the two screens match and confirms on both devices.
//!
//! Only the owner's confirmation pairs the devices. A device in the middle
//! cannot make both codes match without both private keys, and a signature
//! from a different key fails step 3. Nothing here grants any authority.

use ring::rand::{SecureRandom, SystemRandom};
use sha2::{Digest, Sha256};

use crate::{fingerprint, verify, DeviceError, DeviceIdentity};

const TRANSCRIPT_LABEL: &[u8] = b"argentum-pairing-v1";

/// What one device sends to the other first.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PairingHello {
    pub public_key: [u8; 32],
    pub nonce: [u8; 32],
}

impl PairingHello {
    pub fn new(identity: &DeviceIdentity) -> Result<Self, DeviceError> {
        let mut nonce = [0_u8; 32];
        SystemRandom::new()
            .fill(&mut nonce)
            .map_err(|_| DeviceError::Generation)?;
        Ok(Self {
            public_key: identity.public_key(),
            nonce,
        })
    }
}

/// The exchange as one device sees it, once both hellos are known.
#[derive(Debug, Clone)]
pub struct PairingTranscript {
    initiator: PairingHello,
    responder: PairingHello,
}

#[derive(Debug, PartialEq, Eq, thiserror::Error)]
pub enum PairingError {
    #[error("a device cannot pair with itself")]
    SameDevice,
    #[error("the other device's signature does not match this pairing")]
    BadSignature,
}

impl PairingTranscript {
    pub fn new(initiator: PairingHello, responder: PairingHello) -> Result<Self, PairingError> {
        if initiator.public_key == responder.public_key {
            return Err(PairingError::SameDevice);
        }
        Ok(Self {
            initiator,
            responder,
        })
    }

    fn bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(TRANSCRIPT_LABEL.len() + 128);
        bytes.extend_from_slice(TRANSCRIPT_LABEL);
        bytes.extend_from_slice(&self.initiator.public_key);
        bytes.extend_from_slice(&self.initiator.nonce);
        bytes.extend_from_slice(&self.responder.public_key);
        bytes.extend_from_slice(&self.responder.nonce);
        bytes
    }

    /// This device's signature over the transcript.
    pub fn sign(&self, identity: &DeviceIdentity) -> [u8; 64] {
        identity.sign(&self.bytes())
    }

    /// Checks the signature claimed to come from `peer_key`, which must be one
    /// of the two keys in this transcript.
    pub fn verify_peer(
        &self,
        peer_key: &[u8; 32],
        signature: &[u8; 64],
    ) -> Result<(), PairingError> {
        let in_transcript =
            *peer_key == self.initiator.public_key || *peer_key == self.responder.public_key;
        if in_transcript && verify(peer_key, &self.bytes(), signature) {
            Ok(())
        } else {
            Err(PairingError::BadSignature)
        }
    }

    /// The six digits both screens show for the owner to compare.
    pub fn comparison_code(&self) -> String {
        let digest = Sha256::digest(self.bytes());
        let number = u32::from_be_bytes([digest[0], digest[1], digest[2], digest[3]]) % 1_000_000;
        format!("{number:06}")
    }

    /// The other device as it would be remembered after the owner confirms.
    pub fn peer(&self, this_device: &[u8; 32]) -> PairedDevice {
        let key = if *this_device == self.initiator.public_key {
            self.responder.public_key
        } else {
            self.initiator.public_key
        };
        PairedDevice {
            public_key: key,
            fingerprint: fingerprint(&key),
        }
    }
}

/// A device the owner confirmed. Where it is kept, and what it may ask for,
/// belongs to the workspace policy, not to this record.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PairedDevice {
    pub public_key: [u8; 32],
    pub fingerprint: String,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::MemoryStore;

    fn identity() -> DeviceIdentity {
        DeviceIdentity::load_or_create(&MemoryStore::default()).unwrap()
    }

    #[test]
    fn two_devices_agree_on_the_code_and_verify_each_other() {
        let (pc, phone) = (identity(), identity());
        let (pc_hello, phone_hello) = (
            PairingHello::new(&pc).unwrap(),
            PairingHello::new(&phone).unwrap(),
        );
        // Each side builds the same transcript from the two hellos.
        let on_pc = PairingTranscript::new(pc_hello, phone_hello).unwrap();
        let on_phone = PairingTranscript::new(pc_hello, phone_hello).unwrap();
        assert_eq!(on_pc.comparison_code(), on_phone.comparison_code());
        assert_eq!(on_pc.comparison_code().len(), 6);

        let pc_signature = on_pc.sign(&pc);
        let phone_signature = on_phone.sign(&phone);
        assert_eq!(
            on_phone.verify_peer(&pc.public_key(), &pc_signature),
            Ok(())
        );
        assert_eq!(
            on_pc.verify_peer(&phone.public_key(), &phone_signature),
            Ok(())
        );
        assert_eq!(on_pc.peer(&pc.public_key()).public_key, phone.public_key());
        assert_eq!(
            on_phone.peer(&phone.public_key()).fingerprint,
            pc.fingerprint()
        );
    }

    #[test]
    fn a_device_in_the_middle_cannot_pass() {
        let (pc, phone, intruder) = (identity(), identity(), identity());
        let pc_hello = PairingHello::new(&pc).unwrap();
        let phone_hello = PairingHello::new(&phone).unwrap();
        let intruder_hello = PairingHello::new(&intruder).unwrap();

        // The intruder sits between them, so each side sees a different peer.
        let on_pc = PairingTranscript::new(pc_hello, intruder_hello).unwrap();
        let on_phone = PairingTranscript::new(intruder_hello, phone_hello).unwrap();
        assert_ne!(on_pc.comparison_code(), on_phone.comparison_code());

        // A signature from the real phone over its own view fails on the PC.
        let phone_signature = on_phone.sign(&phone);
        assert_eq!(
            on_pc.verify_peer(&phone.public_key(), &phone_signature),
            Err(PairingError::BadSignature)
        );
        // So does a signature over a replayed nonce.
        let replay = PairingTranscript::new(pc_hello, PairingHello::new(&phone).unwrap()).unwrap();
        let honest = PairingTranscript::new(pc_hello, phone_hello).unwrap();
        assert_eq!(
            honest.verify_peer(&phone.public_key(), &replay.sign(&phone)),
            Err(PairingError::BadSignature)
        );
    }

    #[test]
    fn a_device_cannot_pair_with_itself() {
        let pc = identity();
        let hello = PairingHello::new(&pc).unwrap();
        let again = PairingHello::new(&pc).unwrap();
        assert!(matches!(
            PairingTranscript::new(hello, again),
            Err(PairingError::SameDevice)
        ));
    }
}
