// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 AG064

//! Owner settings of agent modules, per project. A setting chooses how a module
//! works, such as which search address it uses. It never grants authority: the
//! module's switch is its capability in the execution policy, and each call
//! still needs its own approval.

use std::collections::BTreeMap;

use argentum_domain::modules::validate_setting;
use argentum_domain::ProjectId;
use rusqlite::{params, OptionalExtension};

use super::{Store, StoreError};

pub type ModuleSettings = BTreeMap<String, BTreeMap<String, String>>;

const MAX_PAYLOAD_BYTES: usize = 32 * 1024;

pub(crate) fn migrate(connection: &rusqlite::Connection) -> Result<(), StoreError> {
    connection.execute_batch(
        "CREATE TABLE IF NOT EXISTS module_settings (
            project_id TEXT PRIMARY KEY,
            payload TEXT NOT NULL
        );",
    )?;
    Ok(())
}

fn validate(settings: &ModuleSettings) -> Result<(), StoreError> {
    for (module, values) in settings {
        for (key, value) in values {
            if validate_setting(module, key, value).as_deref() != Ok(value.as_str()) {
                return Err(StoreError::InvalidRecord(
                    "module setting is not valid".into(),
                ));
            }
        }
    }
    Ok(())
}

impl Store {
    pub fn load_module_settings(
        &self,
        project_id: ProjectId,
    ) -> Result<ModuleSettings, StoreError> {
        let connection = self
            .connection
            .lock()
            .map_err(|_| StoreError::LockPoisoned)?;
        let payload: Option<String> = connection
            .query_row(
                "SELECT payload FROM module_settings WHERE project_id = ?1",
                [project_id.to_string()],
                |row| row.get(0),
            )
            .optional()?;
        let Some(payload) = payload else {
            return Ok(ModuleSettings::new());
        };
        if payload.len() > MAX_PAYLOAD_BYTES {
            return Err(StoreError::InvalidRecord(
                "module settings exceed size limit".into(),
            ));
        }
        let settings: ModuleSettings = serde_json::from_str(&payload)?;
        validate(&settings)?;
        Ok(settings)
    }

    /// Set one owner setting. The value is checked against the module's
    /// manifest and stored in its normal form. An empty value removes it.
    pub fn save_module_setting(
        &self,
        project_id: ProjectId,
        module: &str,
        key: &str,
        value: &str,
    ) -> Result<ModuleSettings, StoreError> {
        let value = validate_setting(module, key, value).map_err(StoreError::InvalidRecord)?;
        let mut settings = self.load_module_settings(project_id)?;
        let entry = settings.entry(module.to_owned()).or_default();
        if value.is_empty() {
            entry.remove(key);
        } else {
            entry.insert(key.to_owned(), value);
        }
        settings.retain(|_, values| !values.is_empty());
        let payload = serde_json::to_string(&settings)?;
        if payload.len() > MAX_PAYLOAD_BYTES {
            return Err(StoreError::InvalidRecord(
                "module settings exceed size limit".into(),
            ));
        }
        let connection = self
            .connection
            .lock()
            .map_err(|_| StoreError::LockPoisoned)?;
        connection.execute(
            "INSERT INTO module_settings (project_id, payload) VALUES (?1, ?2)
             ON CONFLICT(project_id) DO UPDATE SET payload = excluded.payload",
            params![project_id.to_string(), payload],
        )?;
        Ok(settings)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settings_are_validated_stored_and_removed() {
        let store = Store::open_in_memory().expect("store");
        let project = uuid::Uuid::new_v4();
        assert!(store
            .load_module_settings(project)
            .expect("empty")
            .is_empty());
        let saved = store
            .save_module_setting(
                project,
                "web-search",
                "search-address",
                " https://s.example/q ",
            )
            .expect("saved");
        assert_eq!(saved["web-search"]["search-address"], "https://s.example/q");
        assert!(store
            .save_module_setting(project, "web-search", "search-address", "file:///x")
            .is_err());
        assert!(store
            .save_module_setting(project, "web-search", "unknown", "x")
            .is_err());
        assert_eq!(store.load_module_settings(project).expect("load"), saved);
        store
            .save_module_setting(project, "web-search", "search-address", "")
            .expect("removed");
        assert!(store
            .load_module_settings(project)
            .expect("load")
            .is_empty());
        assert!(store
            .load_module_settings(uuid::Uuid::new_v4())
            .expect("other project")
            .is_empty());
    }
}
