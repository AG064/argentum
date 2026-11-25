// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 AG064

//! Project memory notes, stored per project with the workspace data.

use argentum_domain::memory::{MemoryAuthor, MemoryNote, MAX_MEMORY_NOTES};
use rusqlite::{params, Connection};

use super::{ProjectId, Store, StoreError};

pub(super) fn migrate(connection: &Connection) -> Result<(), StoreError> {
    connection.execute_batch(
        "CREATE TABLE IF NOT EXISTS project_memory_notes (
             id INTEGER PRIMARY KEY AUTOINCREMENT,
             project_id TEXT NOT NULL,
             text TEXT NOT NULL,
             author TEXT NOT NULL CHECK(author IN ('owner', 'agent')),
             created_at INTEGER NOT NULL,
             updated_at INTEGER NOT NULL,
             FOREIGN KEY(project_id) REFERENCES projects(id) ON DELETE CASCADE
         );
         CREATE INDEX IF NOT EXISTS idx_project_memory_notes_project
             ON project_memory_notes(project_id, updated_at);",
    )?;
    Ok(())
}

fn now() -> i64 {
    time::OffsetDateTime::now_utc().unix_timestamp()
}

impl Store {
    /// The project's notes, newest first.
    pub fn memory_notes(&self, project_id: ProjectId) -> Result<Vec<MemoryNote>, StoreError> {
        let connection = self
            .connection
            .lock()
            .map_err(|_| StoreError::LockPoisoned)?;
        let mut statement = connection.prepare(
            "SELECT id, text, author, created_at, updated_at FROM project_memory_notes
             WHERE project_id = ?1 ORDER BY updated_at DESC, id DESC",
        )?;
        let rows = statement.query_map(params![project_id.to_string()], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, i64>(3)?,
                row.get::<_, i64>(4)?,
            ))
        })?;
        let mut notes = Vec::new();
        for row in rows {
            let (id, text, author, created_at, updated_at) = row?;
            if let Some(author) = MemoryAuthor::parse(&author) {
                notes.push(MemoryNote {
                    id,
                    text,
                    author,
                    created_at,
                    updated_at,
                });
            }
        }
        Ok(notes)
    }

    /// Adds a note that the caller has already checked. Refuses past
    /// [`MAX_MEMORY_NOTES`], and returns the existing id for the same text.
    pub fn add_memory_note(
        &self,
        project_id: ProjectId,
        text: &str,
        author: MemoryAuthor,
    ) -> Result<i64, StoreError> {
        let connection = self
            .connection
            .lock()
            .map_err(|_| StoreError::LockPoisoned)?;
        let project = project_id.to_string();
        if let Ok(id) = connection.query_row(
            "SELECT id FROM project_memory_notes WHERE project_id = ?1 AND text = ?2",
            params![project, text],
            |row| row.get::<_, i64>(0),
        ) {
            return Ok(id);
        }
        let count: i64 = connection.query_row(
            "SELECT COUNT(*) FROM project_memory_notes WHERE project_id = ?1",
            params![project],
            |row| row.get(0),
        )?;
        if count >= MAX_MEMORY_NOTES as i64 {
            return Err(StoreError::InvalidRecord(
                "project memory is full; delete a note first".into(),
            ));
        }
        let now = now();
        connection.execute(
            "INSERT INTO project_memory_notes (project_id, text, author, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?4)",
            params![project, text, author.as_str(), now],
        )?;
        Ok(connection.last_insert_rowid())
    }

    /// Replaces a note's text. An edited note counts as the owner's.
    pub fn update_memory_note(
        &self,
        project_id: ProjectId,
        id: i64,
        text: &str,
    ) -> Result<bool, StoreError> {
        let connection = self
            .connection
            .lock()
            .map_err(|_| StoreError::LockPoisoned)?;
        Ok(connection.execute(
            "UPDATE project_memory_notes SET text = ?3, author = 'owner', updated_at = ?4
             WHERE project_id = ?1 AND id = ?2",
            params![project_id.to_string(), id, text, now()],
        )? > 0)
    }

    pub fn delete_memory_note(&self, project_id: ProjectId, id: i64) -> Result<bool, StoreError> {
        let connection = self
            .connection
            .lock()
            .map_err(|_| StoreError::LockPoisoned)?;
        Ok(connection.execute(
            "DELETE FROM project_memory_notes WHERE project_id = ?1 AND id = ?2",
            params![project_id.to_string(), id],
        )? > 0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> (tempfile::TempDir, Store, ProjectId) {
        let directory = tempfile::tempdir().unwrap();
        let store = Store::open_in_memory().unwrap();
        let project = store
            .resolve_workspace(directory.path())
            .unwrap()
            .snapshot
            .project
            .id;
        (directory, store, project)
    }

    #[test]
    fn notes_are_added_edited_and_deleted_per_project() {
        let (_directory, store, project) = fixture();
        let first = store
            .add_memory_note(project, "Use pnpm.", MemoryAuthor::Agent)
            .unwrap();
        let same = store
            .add_memory_note(project, "Use pnpm.", MemoryAuthor::Agent)
            .unwrap();
        assert_eq!(first, same);
        store
            .add_memory_note(project, "Tests live in tests/.", MemoryAuthor::Owner)
            .unwrap();
        assert_eq!(store.memory_notes(project).unwrap().len(), 2);
        assert!(store
            .update_memory_note(project, first, "Use pnpm, never npm.")
            .unwrap());
        let notes = store.memory_notes(project).unwrap();
        let edited = notes.iter().find(|note| note.id == first).unwrap();
        assert_eq!(edited.text, "Use pnpm, never npm.");
        assert_eq!(edited.author, MemoryAuthor::Owner);
        assert!(store.delete_memory_note(project, first).unwrap());
        assert!(!store.delete_memory_note(project, first).unwrap());
        assert_eq!(store.memory_notes(project).unwrap().len(), 1);
    }

    #[test]
    fn memory_stops_at_its_cap() {
        let (_directory, store, project) = fixture();
        for index in 0..MAX_MEMORY_NOTES {
            store
                .add_memory_note(project, &format!("note {index}"), MemoryAuthor::Owner)
                .unwrap();
        }
        assert!(store
            .add_memory_note(project, "one more", MemoryAuthor::Owner)
            .is_err());
    }
}
