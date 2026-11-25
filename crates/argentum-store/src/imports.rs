// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 AG064

//! MCP servers imported from other agents, kept per project as drafts.
//!
//! A draft is a description only. Storing one grants nothing, and the MCP
//! module turns a draft on only through its own owner action.

use argentum_domain::imports::ImportedMcpServer;
use rusqlite::{params, Connection};

use super::{ProjectId, Store, StoreError};

pub(super) fn migrate(connection: &Connection) -> Result<(), StoreError> {
    connection.execute_batch(
        "CREATE TABLE IF NOT EXISTS imported_mcp_servers (
             project_id TEXT NOT NULL,
             name TEXT NOT NULL,
             definition TEXT NOT NULL,
             imported_at INTEGER NOT NULL,
             PRIMARY KEY(project_id, name),
             FOREIGN KEY(project_id) REFERENCES projects(id) ON DELETE CASCADE
         );",
    )?;
    Ok(())
}

impl Store {
    /// Saves a draft, replacing one of the same name. The draft must be off.
    pub fn save_imported_mcp_server(
        &self,
        project_id: ProjectId,
        server: &ImportedMcpServer,
    ) -> Result<(), StoreError> {
        server
            .validate()
            .map_err(|reason| StoreError::InvalidRecord(reason.into()))?;
        let definition = serde_json::to_string(server)
            .map_err(|error| StoreError::InvalidRecord(error.to_string()))?;
        let connection = self
            .connection
            .lock()
            .map_err(|_| StoreError::LockPoisoned)?;
        connection.execute(
            "INSERT INTO imported_mcp_servers (project_id, name, definition, imported_at)
             VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(project_id, name) DO UPDATE SET definition = ?3, imported_at = ?4",
            params![
                project_id.to_string(),
                server.name,
                definition,
                time::OffsetDateTime::now_utc().unix_timestamp()
            ],
        )?;
        Ok(())
    }

    /// The project's drafts, by name. A stored row that no longer parses is
    /// left out.
    pub fn imported_mcp_servers(
        &self,
        project_id: ProjectId,
    ) -> Result<Vec<ImportedMcpServer>, StoreError> {
        let connection = self
            .connection
            .lock()
            .map_err(|_| StoreError::LockPoisoned)?;
        let mut statement = connection.prepare(
            "SELECT definition FROM imported_mcp_servers WHERE project_id = ?1 ORDER BY name",
        )?;
        let rows = statement.query_map(params![project_id.to_string()], |row| {
            row.get::<_, String>(0)
        })?;
        let mut servers = Vec::new();
        for row in rows {
            if let Ok(server) = serde_json::from_str::<ImportedMcpServer>(&row?) {
                if server.validate().is_ok() {
                    servers.push(server);
                }
            }
        }
        Ok(servers)
    }

    /// Removes a draft. Returns whether one was there.
    pub fn remove_imported_mcp_server(
        &self,
        project_id: ProjectId,
        name: &str,
    ) -> Result<bool, StoreError> {
        let connection = self
            .connection
            .lock()
            .map_err(|_| StoreError::LockPoisoned)?;
        Ok(connection.execute(
            "DELETE FROM imported_mcp_servers WHERE project_id = ?1 AND name = ?2",
            params![project_id.to_string(), name],
        )? > 0)
    }
}

#[cfg(test)]
mod tests {
    use argentum_domain::imports::{ImportOrigin, ImportedMcpTransport};

    use super::*;

    fn draft(name: &str) -> ImportedMcpServer {
        ImportedMcpServer {
            name: name.into(),
            origin: ImportOrigin::ClaudeCodeProject,
            source_path: ".mcp.json".into(),
            transport: ImportedMcpTransport::Stdio {
                command: "npx".into(),
                args: vec!["server".into()],
            },
            env_names: vec!["TOKEN".into()],
            header_names: vec![],
            enabled: false,
        }
    }

    #[test]
    fn drafts_round_trip_and_are_removed() {
        let directory = tempfile::tempdir().unwrap();
        let store = Store::open_in_memory().unwrap();
        let project = store
            .resolve_workspace(directory.path())
            .unwrap()
            .snapshot
            .project;
        store
            .save_imported_mcp_server(project.id, &draft("b"))
            .unwrap();
        store
            .save_imported_mcp_server(project.id, &draft("a"))
            .unwrap();
        let names: Vec<_> = store
            .imported_mcp_servers(project.id)
            .unwrap()
            .into_iter()
            .map(|server| server.name)
            .collect();
        assert_eq!(names, ["a", "b"]);
        assert!(store.remove_imported_mcp_server(project.id, "a").unwrap());
        assert!(!store.remove_imported_mcp_server(project.id, "a").unwrap());
        assert_eq!(store.imported_mcp_servers(project.id).unwrap().len(), 1);
    }

    #[test]
    fn a_draft_that_is_on_is_refused() {
        let directory = tempfile::tempdir().unwrap();
        let store = Store::open_in_memory().unwrap();
        let project = store
            .resolve_workspace(directory.path())
            .unwrap()
            .snapshot
            .project;
        let mut server = draft("a");
        server.enabled = true;
        assert!(store.save_imported_mcp_server(project.id, &server).is_err());
    }
}
