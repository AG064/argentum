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
