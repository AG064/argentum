use crate::{Store, StoreError};
use argentum_domain::extensions::ExtensionRecord;
use argentum_domain::ProjectId;
use rusqlite::{params, OptionalExtension};

pub(crate) fn migrate(connection: &rusqlite::Connection) -> Result<(), StoreError> {
    connection.execute_batch(
        "CREATE TABLE IF NOT EXISTS extension_connections (
        project_id TEXT NOT NULL REFERENCES projects(id),
        namespace TEXT NOT NULL,
        schema_version INTEGER NOT NULL CHECK(schema_version=1),
        revision TEXT NOT NULL,
        payload TEXT NOT NULL CHECK(length(payload)<=131072),
        PRIMARY KEY(project_id, namespace)
    );",
    )?;
    Ok(())
}

impl Store {
    /// The runtime has consumed a current exact owner proof. Commit activation
    /// and its capability policy together, or leave both unchanged.
    pub fn save_extension_activation(
        &self,
        project: ProjectId,
        record: &ExtensionRecord,
        expected_revision: uuid::Uuid,
        policy: &argentum_domain::HarnessExecutionPolicy,
    ) -> Result<(), StoreError> {
        crate::validate_execution_policy(policy)?;
        let payload = serde_json::to_string(record)?;
        let policy_payload = serde_json::to_string(policy)?;
        if !record.active
            || record.approval_binding.is_none()
            || record.revision.is_nil()
            || record.revision == expected_revision
            || payload.len() > 131072
        {
            return Err(StoreError::InvalidRecord(
                "invalid extension activation".into(),
            ));
        }
        let mut connection = self
            .connection
            .lock()
            .map_err(|_| StoreError::LockPoisoned)?;
        let transaction =
            connection.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        let changed = transaction.execute("UPDATE extension_connections SET revision=?1,payload=?2 WHERE project_id=?3 AND namespace=?4 AND revision=?5",
            params![record.revision.to_string(),payload,project.to_string(),record.definition.namespace,expected_revision.to_string()])?;
        if changed != 1 {
            return Err(StoreError::InvalidRecord(
                "extension configuration revision changed".into(),
            ));
        }
        transaction.execute("INSERT INTO execution_policies(project_id,payload) VALUES(?1,?2) ON CONFLICT(project_id) DO UPDATE SET payload=excluded.payload",
            params![project.to_string(),policy_payload])?;
        transaction.commit()?;
        Ok(())
    }
    pub fn extension_records(
        &self,
        project: ProjectId,
    ) -> Result<Vec<ExtensionRecord>, StoreError> {
        let connection = self
            .connection
            .lock()
            .map_err(|_| StoreError::LockPoisoned)?;
        let mut query = connection.prepare("SELECT payload FROM extension_connections WHERE project_id=?1 ORDER BY namespace LIMIT 33")?;
        let rows = query.query_map(params![project.to_string()], |row| row.get::<_, String>(0))?;
        let mut records = Vec::new();
        for row in rows {
            records.push(serde_json::from_str::<ExtensionRecord>(&row?)?);
        }
        if records.len() > 32 {
            return Err(StoreError::InvalidRecord(
                "extension connection capacity exceeded".into(),
            ));
        }
        Ok(records)
    }
    pub fn extension_record(
        &self,
        project: ProjectId,
        namespace: &str,
    ) -> Result<Option<ExtensionRecord>, StoreError> {
        let connection = self
            .connection
            .lock()
            .map_err(|_| StoreError::LockPoisoned)?;
        let body: Option<String> = connection
            .query_row(
                "SELECT payload FROM extension_connections WHERE project_id=?1 AND namespace=?2",
                params![project.to_string(), namespace],
                |row| row.get(0),
            )
            .optional()?;
        body.map(|body| serde_json::from_str(&body).map_err(StoreError::from))
            .transpose()
    }
    /// Called only after the runtime consumes an exact local owner proof.
    /// The database contains no credential or executable authority material.
    pub fn save_extension_record(
        &self,
        project: ProjectId,
        record: &ExtensionRecord,
        expected_revision: Option<uuid::Uuid>,
    ) -> Result<(), StoreError> {
        let payload = serde_json::to_string(record)?;
        if payload.len() > 131072
            || record.definition.namespace.is_empty()
            || record.definition.namespace.len() > 129
            || record.revision.is_nil()
        {
            return Err(StoreError::InvalidRecord("invalid extension record".into()));
        }
        let mut connection = self
            .connection
            .lock()
            .map_err(|_| StoreError::LockPoisoned)?;
        let transaction =
            connection.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        let current: Option<String> = transaction
            .query_row(
                "SELECT revision FROM extension_connections WHERE project_id=?1 AND namespace=?2",
                params![project.to_string(), record.definition.namespace],
                |row| row.get(0),
            )
            .optional()?;
        if current.as_deref()
            != expected_revision
                .as_ref()
                .map(uuid::Uuid::to_string)
                .as_deref()
        {
            return Err(StoreError::InvalidRecord(
                "extension configuration revision changed".into(),
            ));
        }
        if current.is_none() {
            let count: u64 = transaction.query_row(
                "SELECT count(*) FROM extension_connections WHERE project_id=?1",
                params![project.to_string()],
                |row| row.get(0),
            )?;
            if count >= 32 {
                return Err(StoreError::InvalidRecord(
                    "extension connection capacity exceeded".into(),
                ));
            }
        }
        transaction.execute("INSERT INTO extension_connections(project_id,namespace,schema_version,revision,payload) VALUES(?1,?2,1,?3,?4)
            ON CONFLICT(project_id,namespace) DO UPDATE SET revision=excluded.revision,payload=excluded.payload", params![project.to_string(), record.definition.namespace, record.revision.to_string(), payload])?;
        transaction.commit()?;
        Ok(())
    }
    pub fn remove_extension_record(
        &self,
        project: ProjectId,
        namespace: &str,
        expected_revision: uuid::Uuid,
    ) -> Result<(), StoreError> {
        let connection = self
            .connection
            .lock()
            .map_err(|_| StoreError::LockPoisoned)?;
        let removed = connection.execute("DELETE FROM extension_connections WHERE project_id=?1 AND namespace=?2 AND revision=?3", params![project.to_string(), namespace, expected_revision.to_string()])?;
        if removed != 1 {
            return Err(StoreError::InvalidRecord(
                "extension configuration revision changed".into(),
            ));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use argentum_domain::{
        extensions::{ExtensionDefinition, ExtensionTransport, ExtensionTrust},
        ApprovalMode, HarnessExecutionPolicy, SandboxScope, Sha256Digest,
    };
    #[test]
    fn activation_is_atomic_revision_bound_and_survives_reopen_without_cross_project_access() {
        let root = tempfile::tempdir().unwrap();
        let ws = root.path().join("workspace");
        std::fs::create_dir(&ws).unwrap();
        let other = root.path().join("other");
        std::fs::create_dir(&other).unwrap();
        let db = root.path().join("store.sqlite3");
        let store = Store::open(&db).unwrap();
        let project = store.resolve_workspace(&ws).unwrap().snapshot.project.id;
        let other = store.resolve_workspace(&other).unwrap().snapshot.project.id;
        let original = ExtensionRecord {
            definition: ExtensionDefinition {
                namespace: "control.skills".into(),
                package_root: ws.to_string_lossy().into_owned(),
                package_sha256: Sha256Digest("a".repeat(64)),
                source_id: "synthetic".into(),
                source_revision: "v1".into(),
                transport: ExtensionTransport::Skills,
                allowed_tools: vec![],
                resource_prefixes: vec![],
                trust: ExtensionTrust::ReadOnlyInstructions,
                image_scope: None,
            },
            revision: uuid::Uuid::new_v4(),
            active: false,
            approval_binding: None,
        };
        store
            .save_extension_record(project, &original, None)
            .unwrap();
        let policy = HarnessExecutionPolicy {
            profile_id: "custom".into(),
            source_profile_id: None,
            sandbox_scope: SandboxScope::WorkspaceWrite,
            approval_mode: ApprovalMode::Always,
            capability_enabled: std::collections::BTreeMap::from([(
                "extension.external".into(),
                true,
            )]),
        };
        let mut active = original.clone();
        active.revision = uuid::Uuid::new_v4();
        active.active = true;
        active.approval_binding = Some(Sha256Digest("b".repeat(64)));
        assert!(store
            .save_extension_activation(project, &active, uuid::Uuid::new_v4(), &policy)
            .is_err());
        assert_eq!(
            store.extension_record(project, "control.skills").unwrap(),
            Some(original.clone())
        );
        assert!(store.load_execution_policy(project).unwrap().is_none());
        store
            .save_extension_activation(project, &active, original.revision, &policy)
            .unwrap();
        drop(store);
        let store = Store::open(&db).unwrap();
        assert_eq!(
            store.extension_record(project, "control.skills").unwrap(),
            Some(active.clone())
        );
        assert_eq!(store.load_execution_policy(project).unwrap(), Some(policy));
        assert!(store
            .extension_record(other, "control.skills")
            .unwrap()
            .is_none());
        let previous = active.revision;
        active.active = false;
        active.approval_binding = None;
        active.revision = uuid::Uuid::new_v4();
        store
            .save_extension_record(project, &active, Some(previous))
            .unwrap();
        drop(store);
        let store = Store::open(&db).unwrap();
        assert_eq!(
            store.extension_record(project, "control.skills").unwrap(),
            Some(active)
        );
    }
}
