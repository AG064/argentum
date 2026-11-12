// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 AG064

//! Devices the owner paired with this install. Only the owner's confirmation on
//! both devices adds one, and the owner can forget one at any time. Being paired
//! grants nothing by itself: what a paired device may ask for is decided by the
//! workspace policy.

use rusqlite::{params, OptionalExtension};

use super::{Store, StoreError};

const MAX_NAME_BYTES: usize = 64;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PairedDeviceRecord {
    /// Hex of the 32-byte Ed25519 public key.
    pub public_key: String,
    /// The fingerprint both screens showed, such as `3f2a-91bc-07de-55a1-c004`.
    pub fingerprint: String,
    pub name: String,
    pub paired_at: i64,
}

pub(crate) fn migrate(connection: &rusqlite::Connection) -> Result<(), StoreError> {
    connection.execute_batch(
        "CREATE TABLE IF NOT EXISTS paired_devices (
            public_key TEXT PRIMARY KEY CHECK(length(public_key) = 64),
            fingerprint TEXT NOT NULL UNIQUE CHECK(length(fingerprint) = 24),
            name TEXT NOT NULL CHECK(length(CAST(name AS BLOB)) BETWEEN 1 AND 64),
            paired_at INTEGER NOT NULL
        );",
    )?;
    Ok(())
}

fn valid(record: &PairedDeviceRecord) -> bool {
    let hex = |text: &str| text.bytes().all(|byte| byte.is_ascii_hexdigit());
    let fingerprint_ok = record.fingerprint.len() == 24
        && record
            .fingerprint
            .split('-')
            .all(|group| group.len() == 4 && hex(group));
    record.public_key.len() == 64
        && hex(&record.public_key)
        && fingerprint_ok
        && !record.name.trim().is_empty()
        && record.name.len() <= MAX_NAME_BYTES
        && !record.name.chars().any(char::is_control)
}

impl Store {
    /// Remembers a device the owner confirmed. Pairing the same device again
    /// updates its name.
    pub fn save_paired_device(&self, record: &PairedDeviceRecord) -> Result<(), StoreError> {
        if !valid(record) {
            return Err(StoreError::InvalidRecord("invalid paired device".into()));
        }
        let connection = self
            .connection
            .lock()
            .map_err(|_| StoreError::LockPoisoned)?;
        connection.execute(
            "INSERT INTO paired_devices (public_key, fingerprint, name, paired_at)
             VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(public_key) DO UPDATE SET name = excluded.name",
            params![
                record.public_key.to_ascii_lowercase(),
                record.fingerprint.to_ascii_lowercase(),
                record.name.trim(),
                record.paired_at
            ],
        )?;
        Ok(())
    }

    pub fn paired_devices(&self) -> Result<Vec<PairedDeviceRecord>, StoreError> {
        let connection = self
            .connection
            .lock()
            .map_err(|_| StoreError::LockPoisoned)?;
        let mut statement = connection.prepare(
            "SELECT public_key, fingerprint, name, paired_at FROM paired_devices
             ORDER BY paired_at DESC, fingerprint",
        )?;
        let rows = statement
            .query_map([], |row| {
                Ok(PairedDeviceRecord {
                    public_key: row.get(0)?,
                    fingerprint: row.get(1)?,
                    name: row.get(2)?,
                    paired_at: row.get(3)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    /// Forgets a paired device by fingerprint. Returns whether one was found.
    pub fn forget_paired_device(&self, fingerprint: &str) -> Result<bool, StoreError> {
        let connection = self
            .connection
            .lock()
            .map_err(|_| StoreError::LockPoisoned)?;
        let found = connection
            .query_row(
                "SELECT 1 FROM paired_devices WHERE fingerprint = ?1",
                [fingerprint.trim().to_ascii_lowercase()],
                |_| Ok(()),
            )
            .optional()?
            .is_some();
        if found {
            connection.execute(
                "DELETE FROM paired_devices WHERE fingerprint = ?1",
                [fingerprint.trim().to_ascii_lowercase()],
            )?;
        }
        Ok(found)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn record(fingerprint: &str, name: &str) -> PairedDeviceRecord {
        PairedDeviceRecord {
            public_key: format!("{:0>64}", fingerprint.replace('-', "")),
            fingerprint: fingerprint.into(),
            name: name.into(),
            paired_at: 1_800_000_000,
        }
    }

    #[test]
    fn paired_devices_are_saved_listed_renamed_and_forgotten() {
        let store = Store::open_in_memory().unwrap();
        let phone = record("3f2a-91bc-07de-55a1-c004", "Phone");
        store.save_paired_device(&phone).unwrap();
        store
            .save_paired_device(&record("3f2a-91bc-07de-55a1-c004", "Mi 13T Pro"))
            .unwrap();
        let listed = store.paired_devices().unwrap();
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].name, "Mi 13T Pro");

        assert!(store
            .forget_paired_device("3F2A-91BC-07DE-55A1-C004")
            .unwrap());
        assert!(!store
            .forget_paired_device("3f2a-91bc-07de-55a1-c004")
            .unwrap());
        assert!(store.paired_devices().unwrap().is_empty());
    }

    #[test]
    fn malformed_records_are_refused() {
        let store = Store::open_in_memory().unwrap();
        for bad in [
            record("not-a-fingerprint", "Phone"),
            record("3f2a-91bc-07de-55a1-c004", ""),
            record("3f2a-91bc-07de-55a1-c004", "line\nbreak"),
            PairedDeviceRecord {
                public_key: "zz".into(),
                ..record("3f2a-91bc-07de-55a1-c004", "Phone")
            },
        ] {
            assert!(store.save_paired_device(&bad).is_err());
        }
        assert!(store.paired_devices().unwrap().is_empty());
    }
}
