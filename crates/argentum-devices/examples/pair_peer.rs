// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 AG064

//! A stand-in device for testing pairing. It keeps its identity in memory, so
//! it writes nothing to the credential store, and it saves nothing.
//!
//! `cargo run -p argentum-devices --example pair_peer -- HOST:PORT`

use std::collections::HashMap;
use std::net::TcpStream;
use std::sync::Mutex;
use std::time::Duration;

use argentum_devices::exchange::exchange;
use argentum_devices::DeviceIdentity;
use argentum_platform::{PlatformError, SecretStore};
use argentum_security::SecretValue;

#[derive(Default)]
struct MemoryStore(Mutex<HashMap<String, String>>);

impl SecretStore for MemoryStore {
    fn get(&self, key: &str) -> Result<Option<SecretValue>, PlatformError> {
        Ok(self
            .0
            .lock()
            .expect("store")
            .get(key)
            .cloned()
            .map(SecretValue::new))
    }
    fn set(&self, key: &str, value: SecretValue) -> Result<(), PlatformError> {
        self.0
            .lock()
            .expect("store")
            .insert(key.to_owned(), value.expose().to_owned());
        Ok(())
    }
    fn delete(&self, key: &str) -> Result<(), PlatformError> {
        self.0.lock().expect("store").remove(key);
        Ok(())
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let address = std::env::args()
        .nth(1)
        .ok_or("usage: pair_peer HOST:PORT")?;
    let identity = DeviceIdentity::load_or_create(&MemoryStore::default())?;
    println!("stand-in device: {}", identity.fingerprint());
    let stream = TcpStream::connect(&address)?;
    stream.set_read_timeout(Some(Duration::from_secs(60)))?;
    let outcome = exchange(&stream, &identity, true)?;
    println!("code: {}", outcome.comparison_code);
    println!("other device: {}", outcome.peer.fingerprint);
    Ok(())
}
