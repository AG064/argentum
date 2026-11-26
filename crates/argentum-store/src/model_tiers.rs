// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 AG064

//! The owner's model tiers, one settings record per project.

use argentum_domain::model_tiers::ModelTierSettings;
use rusqlite::{params, Connection, OptionalExtension};

use super::{ProjectId, Store, StoreError};

pub(super) fn migrate(connection: &Connection) -> Result<(), StoreError> {
    connection.execute_batch(
        "CREATE TABLE IF NOT EXISTS project_model_tiers (
             project_id TEXT PRIMARY KEY NOT NULL,
             settings TEXT NOT NULL,
             FOREIGN KEY(project_id) REFERENCES projects(id) ON DELETE CASCADE
         );",
    )?;
    Ok(())
}

impl Store {
    /// The project's tiers. Missing or unreadable settings read as none.
    pub fn model_tiers(&self, project_id: ProjectId) -> Result<ModelTierSettings, StoreError> {
        let connection = self
            .connection
            .lock()
            .map_err(|_| StoreError::LockPoisoned)?;
        let stored: Option<String> = connection
            .query_row(
                "SELECT settings FROM project_model_tiers WHERE project_id = ?1",
                params![project_id.to_string()],
                |row| row.get(0),
            )
            .optional()?;
        Ok(stored
            .and_then(|text| serde_json::from_str::<ModelTierSettings>(&text).ok())
            .filter(|settings| settings.validate().is_ok())
            .unwrap_or_default())
    }

    pub fn save_model_tiers(
        &self,
        project_id: ProjectId,
        settings: &ModelTierSettings,
    ) -> Result<(), StoreError> {
        settings
            .validate()
            .map_err(|reason| StoreError::InvalidRecord(reason.into()))?;
        let text = serde_json::to_string(settings)
            .map_err(|error| StoreError::InvalidRecord(error.to_string()))?;
        let connection = self
            .connection
            .lock()
            .map_err(|_| StoreError::LockPoisoned)?;
        connection.execute(
            "INSERT INTO project_model_tiers (project_id, settings) VALUES (?1, ?2)
             ON CONFLICT(project_id) DO UPDATE SET settings = ?2",
            params![project_id.to_string(), text],
        )?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use argentum_domain::model_tiers::{ModelStep, ModelTier};

    use super::*;

    #[test]
    fn tiers_are_kept_per_project_and_checked() {
        let directory = tempfile::tempdir().unwrap();
        let store = Store::open_in_memory().unwrap();
        let project = store
            .resolve_workspace(directory.path())
            .unwrap()
            .snapshot
            .project
            .id;
        assert_eq!(
            store.model_tiers(project).unwrap(),
            ModelTierSettings::default()
        );
        let settings = ModelTierSettings {
            tiers: vec![ModelTier {
                name: "cheap".into(),
                profile_id: "deepseek".into(),
                model: "deepseek-chat".into(),
            }],
            steps: BTreeMap::from([(ModelStep::Summaries, "cheap".into())]),
        };
        store.save_model_tiers(project, &settings).unwrap();
        assert_eq!(store.model_tiers(project).unwrap(), settings);
        let mut broken = settings.clone();
        broken.steps.insert(ModelStep::Planning, "missing".into());
        assert!(store.save_model_tiers(project, &broken).is_err());
        assert_eq!(store.model_tiers(project).unwrap(), settings);
    }
}
