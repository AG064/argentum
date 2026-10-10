use argentum_domain::working_context::{
    ContextGroup, GroupBody, MAX_GROUP_BYTES, MAX_SOURCE_GROUPS,
};
use argentum_domain::{ConversationRole, ProjectId, RunId, SessionForkSource, SessionId};
use rusqlite::{params, OptionalExtension};

use super::{Store, StoreError};

// Rebuild only the old bounded counter table, atomically, preserving its children.
// Foreign keys must be disabled before BEGIN or DROP would cascade into history.
pub(super) fn upgrade_run_limit(db: &rusqlite::Connection) -> Result<(), StoreError> {
    let schema: String = db.query_row(
        "SELECT sql FROM sqlite_master WHERE type='table' AND name='context_runs'",
        [],
        |row| row.get(0),
    )?;
    if !schema
        .split_whitespace()
        .collect::<String>()
        .contains("next_ordinal<=9)")
    {
        return Ok(());
    }
    db.execute_batch("PRAGMA foreign_keys = OFF;")?;
    let result = (|| {
        let tx = db.unchecked_transaction()?;
        tx.execute_batch("CREATE TABLE context_runs_v20 (
            run_id TEXT PRIMARY KEY,
            project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
            session_id TEXT NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
            task_id TEXT NOT NULL,
            assistant_complete INTEGER NOT NULL DEFAULT 0,
            next_ordinal INTEGER NOT NULL DEFAULT 0 CHECK(next_ordinal >= 0 AND next_ordinal <= 33),
            user_message_id TEXT NOT NULL UNIQUE REFERENCES conversation_messages(id) ON DELETE CASCADE);
            INSERT INTO context_runs_v20 SELECT run_id,project_id,session_id,task_id,assistant_complete,next_ordinal,user_message_id FROM context_runs;
            DROP TABLE context_runs;
            ALTER TABLE context_runs_v20 RENAME TO context_runs;
            CREATE INDEX idx_context_runs_scope ON context_runs(project_id,session_id);")?;
        let invalid: bool = tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM pragma_foreign_key_check)",
            [],
            |r| r.get(0),
        )?;
        if invalid {
            return Err(StoreError::InvalidRecord(
                "context migration foreign key check failed".into(),
            ));
        }
        tx.commit()?;
        Ok(())
    })();
    db.execute_batch("PRAGMA foreign_keys = ON;")?;
    result
}

#[derive(Clone)]
pub enum ContextSource {
    Group(ContextGroup),
    Legacy {
        session_id: SessionId,
        message_id: argentum_domain::MessageId,
        run_id: RunId,
        role: ConversationRole,
        text: String,
        complete: bool,
    },
    Unavailable {
        reason: &'static str,
        source: String,
    },
}

impl std::fmt::Debug for ContextSource {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Group(group) => formatter.debug_tuple("Group").field(group).finish(),
            Self::Legacy {
                session_id,
                message_id,
                run_id,
                role,
                text,
                complete,
            } => formatter
                .debug_struct("Legacy")
                .field("session_id", session_id)
                .field("message_id", message_id)
                .field("run_id", run_id)
                .field("role", role)
                .field("text_bytes", &text.len())
                .field("complete", complete)
                .finish(),
            Self::Unavailable { reason, source } => formatter
                .debug_struct("Unavailable")
                .field("reason", reason)
                .field("source", source)
                .finish(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use argentum_domain::working_context::{
        Observation, ObservationState, Retention, PROJECTION_VERSION,
    };
    use argentum_domain::{now, ConversationMessage, ConversationMessageStatus};
    use uuid::Uuid;

    fn start(store: &Store, root: &std::path::Path) -> ContextGroup {
        let state = store.resolve_workspace(root).unwrap().snapshot;
        let group = ContextGroup {
            version: PROJECTION_VERSION,
            project_id: state.project.id,
            session_id: state.active_session_id.unwrap(),
            task_id: Uuid::new_v4(),
            run_id: Uuid::new_v4(),
            ordinal: 0,
            body: GroupBody::User {
                text: "same prompt".into(),
            },
        };
        store
            .append_conversation_message(&ConversationMessage {
                id: Uuid::new_v4(),
                project_id: group.project_id,
                session_id: group.session_id,
                run_id: group.run_id,
                role: ConversationRole::User,
                text: "same prompt".into(),
                reasoning: String::new(),
                usage: None,
                profile_id: String::new(),
                model: String::new(),
                status: ConversationMessageStatus::Complete,
                created_at: now(),
            })
            .unwrap();
        group
    }

    #[test]
    fn reliability_old_counter_migration_preserves_history_and_foreign_keys() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("migration.db");
        let store = Store::open(&path).unwrap();
        let mut group = start(&store, root.path());
        store.append_context_group(&group).unwrap();
        {
            let db = store.connection.lock().unwrap();
            // Reproduce the precise prior schema in this disposable database.
            db.execute_batch("PRAGMA foreign_keys=OFF; BEGIN;
                CREATE TABLE old_context_runs (run_id TEXT PRIMARY KEY, project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,session_id TEXT NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,task_id TEXT NOT NULL,assistant_complete INTEGER NOT NULL DEFAULT 0,next_ordinal INTEGER NOT NULL DEFAULT 0 CHECK(next_ordinal >= 0 AND next_ordinal <= 9),user_message_id TEXT NOT NULL UNIQUE REFERENCES conversation_messages(id) ON DELETE CASCADE);
                INSERT INTO old_context_runs SELECT * FROM context_runs; DROP TABLE context_runs; ALTER TABLE old_context_runs RENAME TO context_runs;
                CREATE INDEX idx_context_runs_scope ON context_runs(project_id,session_id); PRAGMA user_version=19; COMMIT; PRAGMA foreign_keys=ON;") .unwrap();
        }
        drop(store);
        let store = Store::open(&path).unwrap();
        assert!(
            matches!(&store.context_sources(group.project_id,group.session_id).unwrap().sources[0],ContextSource::Group(g) if g==&group)
        );
        for ordinal in 1..32 {
            group.ordinal = ordinal;
            group.body = GroupBody::Exchange {
                text: "more".into(),
                text_retention: Retention::Retained,
                observations: vec![Observation {
                    provider_call_id: format!("call-{ordinal}"),
                    runtime_call_id: Some(Uuid::new_v4()),
                    tool: "read_text".into(),
                    arguments: "{}".into(),
                    retention: Retention::OmittedPolicy,
                    state: ObservationState::ResultAvailable,
                    receipt: None,
                    result: None,
                    change: None,
                }],
            };
            store.append_context_group(&group).unwrap();
        }
        group.ordinal = 32;
        group.body = GroupBody::Assistant {
            text: "finished".into(),
        };
        store.append_context_group(&group).unwrap();
        let db = store.connection.lock().unwrap();
        assert_eq!(
            db.query_row("SELECT COUNT(*) FROM context_groups", [], |r| r
                .get::<_, i64>(0))
                .unwrap(),
            33
        );
        assert_eq!(
            db.query_row("PRAGMA foreign_keys", [], |r| r.get::<_, i64>(0))
                .unwrap(),
            1
        );
        assert_eq!(
            db.query_row("SELECT COUNT(*) FROM pragma_foreign_key_check", [], |r| r
                .get::<_, i64>(
                0
            ))
            .unwrap(),
            0
        );
    }

    #[test]
    fn groups_restart_without_legacy_overlap_or_text_deduplication() {
        let root = tempfile::tempdir().unwrap();
        let db = root.path().join("context.db");
        let store = Store::open(&db).unwrap();
        let mut group = start(&store, root.path());
        store.append_context_group(&group).unwrap();
        store.append_context_group(&group).unwrap();
        group.ordinal = 1;
        group.body = GroupBody::Exchange {
            text: String::new(),
            text_retention: Retention::Retained,
            observations: vec![Observation {
                provider_call_id: "fixture-call".into(),
                runtime_call_id: Some(Uuid::new_v4()),
                tool: "read_text".into(),
                arguments: "{}".into(),
                retention: Retention::Retained,
                state: ObservationState::ResultAvailable,
                receipt: None,
                change: None,
                result: Some("original-only-canary".into()),
            }],
        };
        store.append_context_group(&group).unwrap();
        let next = start(&store, root.path());
        store.append_context_group(&next).unwrap();
        drop(store);
        let store = Store::open(&db).unwrap();
        for _ in 0..2 {
            let window = store
                .context_sources(group.project_id, group.session_id)
                .unwrap();
            assert_eq!(window.total_sources, 3);
            assert_eq!(window.sources.len(), 3);
            assert!(window
                .sources
                .iter()
                .all(|item| matches!(item, ContextSource::Group(_))));
            assert!(matches!(&window.sources[1], ContextSource::Group(value) if value == &group));
        }
        group.ordinal = 3;
        assert!(store.append_context_group(&group).is_err());
        assert!(store
            .context_sources(Uuid::new_v4(), group.session_id)
            .is_err());
    }

    #[test]
    fn corrupt_group_is_explicitly_unavailable_and_legacy_remains_legacy() {
        let root = tempfile::tempdir().unwrap();
        let store = Store::open_in_memory().unwrap();
        let group = start(&store, root.path());
        assert!(matches!(
            store
                .context_sources(group.project_id, group.session_id)
                .unwrap()
                .sources[0],
            ContextSource::Legacy { .. }
        ));
        store.append_context_group(&group).unwrap();
        store
            .connection
            .lock()
            .unwrap()
            .execute("UPDATE context_groups SET payload = '{}'", [])
            .unwrap();
        assert!(matches!(
            store
                .context_sources(group.project_id, group.session_id)
                .unwrap()
                .sources[0],
            ContextSource::Unavailable {
                reason: "invalid_context_group",
                ..
            }
        ));
    }

    #[test]
    fn conflicting_source_does_not_overwrite_committed_body() {
        let root = tempfile::tempdir().unwrap();
        let store = Store::open_in_memory().unwrap();
        let mut group = start(&store, root.path());
        store.append_context_group(&group).unwrap();
        group.body = GroupBody::User {
            text: "different".into(),
        };
        assert!(store.append_context_group(&group).is_err());
        let window = store
            .context_sources(group.project_id, group.session_id)
            .unwrap();
        assert!(
            matches!(&window.sources[0], ContextSource::Group(value) if matches!(&value.body, GroupBody::User { text } if text == "same prompt"))
        );
    }

    #[test]
    fn context_privacy_errors_do_not_echo_corrupt_receipt_fields() {
        let root = tempfile::tempdir().unwrap();
        let store = Store::open_in_memory().unwrap();
        let group = start(&store, root.path());
        let call_id = Uuid::new_v4();
        store.connection.lock().unwrap().execute(
            "INSERT INTO execution_audit(id, project_id, session_id, run_id, tool_call_id, recorded_at, payload)
             VALUES (?1, ?2, ?3, ?4, ?5, 0, ?6)",
            params![Uuid::new_v4().to_string(), group.project_id.to_string(), group.session_id.to_string(), group.run_id.to_string(), call_id.to_string(),
                "{\"outcome\":\"PRIVATE_DIAGNOSTIC_CANARY\"}"],
        ).unwrap();
        let message = store
            .context_execution_receipt(
                group.project_id,
                group.session_id,
                group.run_id,
                call_id,
                "read_text",
            )
            .unwrap_err()
            .to_string();
        assert!(!message.contains("PRIVATE_DIAGNOSTIC_CANARY"));
        store
            .connection
            .lock()
            .unwrap()
            .execute(
                "UPDATE conversation_messages SET text = 'PRIVATE_DIAGNOSTIC_CANARY'",
                [],
            )
            .unwrap();
        let sources = store
            .context_sources(group.project_id, group.session_id)
            .unwrap();
        assert!(!format!("{sources:?}").contains("PRIVATE_DIAGNOSTIC_CANARY"));
    }

    #[test]
    fn group_limit_rejection_is_atomic_and_session_deletion_cascades() {
        let root = tempfile::tempdir().unwrap();
        let store = Store::open_in_memory().unwrap();
        let mut group = start(&store, root.path());
        store.append_context_group(&group).unwrap();
        group.ordinal = 1;
        group.body = GroupBody::Exchange {
            text: String::new(),
            text_retention: Retention::Retained,
            observations: (0..8)
                .map(|index| Observation {
                    provider_call_id: format!("call-{index}"),
                    runtime_call_id: Some(Uuid::new_v4()),
                    tool: "read_text".into(),
                    arguments: "{}".into(),
                    retention: Retention::Retained,
                    result: Some("\"".repeat(64 * 1024)),
                    state: ObservationState::ResultAvailable,
                    receipt: None,
                    change: None,
                })
                .collect(),
        };
        assert!(
            group.validate().is_ok(),
            "individual fields fit, but serialized group does not"
        );
        assert!(store.append_context_group(&group).is_err());
        let window = store
            .context_sources(group.project_id, group.session_id)
            .unwrap();
        assert_eq!(window.total_sources, 1);
        assert_eq!(window.inconsistent_source_runs, 0);
        let connection = store.connection.lock().unwrap();
        connection
            .execute(
                "DELETE FROM sessions WHERE id = ?1",
                [group.session_id.to_string()],
            )
            .unwrap();
        for table in ["context_runs", "context_groups", "conversation_messages"] {
            assert_eq!(
                connection
                    .query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |row| row
                        .get::<_, u64>(0))
                    .unwrap(),
                0
            );
        }
    }

    #[test]
    fn oversized_legacy_body_is_not_allocated_and_checkpoint_loss_is_visible() {
        let root = tempfile::tempdir().unwrap();
        let store = Store::open_in_memory().unwrap();
        let group = start(&store, root.path());
        store
            .connection
            .lock()
            .unwrap()
            .execute(
                "UPDATE conversation_messages SET text = CAST(zeroblob(1048576) AS TEXT)",
                [],
            )
            .unwrap();
        let window = store
            .context_sources(group.project_id, group.session_id)
            .unwrap();
        assert!(matches!(
            &window.sources[0],
            ContextSource::Unavailable {
                reason: "source_body_limit",
                ..
            }
        ));
        store
            .connection
            .lock()
            .unwrap()
            .execute("UPDATE conversation_messages SET text = 'same prompt'", [])
            .unwrap();
        store.append_context_group(&group).unwrap();
        store
            .connection
            .lock()
            .unwrap()
            .execute("DELETE FROM context_groups", [])
            .unwrap();
        let window = store
            .context_sources(group.project_id, group.session_id)
            .unwrap();
        assert!(window.sources.is_empty());
        assert_eq!(window.inconsistent_source_runs, 1);
        assert_eq!(window.missing_checkpoint_groups, 1);
    }
}

#[derive(Debug, Clone)]
pub struct ContextSourceWindow {
    pub sources: Vec<ContextSource>,
    pub total_sources: u64,
    pub omitted_older: u64,
    pub runs_without_completed_assistant: u64,
    pub first_source: Option<String>,
    pub last_source: Option<String>,
    pub inconsistent_source_runs: u64,
    pub missing_checkpoint_groups: u64,
}

impl Store {
    pub fn context_edit_reference(
        &self,
        project_id: ProjectId,
        session_id: SessionId,
        run_id: RunId,
        path: &str,
        digest: &str,
        file_change_id: Option<argentum_domain::FileChangeId>,
    ) -> Result<Option<argentum_domain::working_context::ObservedChange>, StoreError> {
        let connection = self
            .connection
            .lock()
            .map_err(|_| StoreError::LockPoisoned)?;
        let payload: Option<String> = connection.query_row(
            "SELECT CASE WHEN length(CAST(payload AS BLOB)) <= 16384 THEN payload END FROM file_changes
             WHERE project_id = ?1 AND session_id = ?2 AND run_id = ?3 AND relative_path = ?4 AND (?5 IS NULL OR id = ?5)
             ORDER BY revision DESC, rowid DESC LIMIT 1",
            params![project_id.to_string(), session_id.to_string(), run_id.to_string(), path, file_change_id.map(|id| id.to_string())], |row| row.get(0),
        ).optional()?;
        let Some(payload) = payload else {
            return Ok(None);
        };
        let record: argentum_domain::FileChangeRecord = serde_json::from_str(&payload)
            .map_err(|_| StoreError::InvalidRecord("invalid context change reference".into()))?;
        if record.project_id != project_id
            || record.session_id != session_id
            || record.run_id != run_id
            || record.relative_path != path
        {
            return Err(StoreError::InvalidRecord(
                "context change reference scope mismatch".into(),
            ));
        }
        let Some(after) = record.after.filter(|after| after.digest.0 == digest) else {
            return Ok(None);
        };
        Ok(Some(argentum_domain::working_context::ObservedChange {
            file_change_id: record.id,
            change_set_id: record.change_set_id,
            revision: record.revision,
            before_sha256: record.before.map(|before| before.digest),
            after_sha256: after.digest,
        }))
    }

    pub fn context_verification_reference(
        &self,
        project_id: ProjectId,
        session_id: SessionId,
        run_id: RunId,
        id: argentum_domain::VerificationId,
    ) -> Result<Option<argentum_domain::VerificationEvidence>, StoreError> {
        let connection = self
            .connection
            .lock()
            .map_err(|_| StoreError::LockPoisoned)?;
        let payload: Option<String> = connection.query_row(
            "SELECT CASE WHEN length(CAST(payload AS BLOB)) <= 16384 THEN payload END FROM verification_evidence
             WHERE id = ?1 AND project_id = ?2 AND session_id = ?3 AND run_id = ?4",
            params![id.to_string(), project_id.to_string(), session_id.to_string(), run_id.to_string()], |row| row.get(0),
        ).optional()?;
        let Some(payload) = payload else {
            return Ok(None);
        };
        let record: argentum_domain::VerificationEvidence = serde_json::from_str(&payload)
            .map_err(|_| {
                StoreError::InvalidRecord("invalid context verification reference".into())
            })?;
        super::validate_verification_evidence(&record)?;
        if record.id != id || record.run_id != run_id {
            return Err(StoreError::InvalidRecord(
                "context verification scope mismatch".into(),
            ));
        }
        Ok(Some(record))
    }

    pub fn context_execution_receipt(
        &self,
        project_id: ProjectId,
        session_id: SessionId,
        run_id: RunId,
        call_id: argentum_domain::ToolCallId,
        tool: &str,
    ) -> Result<Option<argentum_domain::working_context::ObservationReceipt>, StoreError> {
        let connection = self
            .connection
            .lock()
            .map_err(|_| StoreError::LockPoisoned)?;
        let payload: Option<String> = connection.query_row(
            "SELECT CASE WHEN length(CAST(payload AS BLOB)) <= 131072 THEN payload END
             FROM execution_audit WHERE project_id = ?1 AND session_id = ?2 AND run_id = ?3 AND tool_call_id = ?4
             ORDER BY recorded_at DESC, rowid DESC LIMIT 1",
            params![project_id.to_string(), session_id.to_string(), run_id.to_string(), call_id.to_string()],
            |row| row.get(0),
        ).optional()?;
        let Some(payload) = payload else {
            return Ok(None);
        };
        let record: argentum_domain::ExecutionAuditRecord = serde_json::from_str(&payload)
            .map_err(|_| StoreError::InvalidRecord("invalid context execution receipt".into()))?;
        super::validate_execution_audit(&record)?;
        if record.project_id != project_id
            || record.session_id != session_id
            || record.run_id != run_id
            || record.tool_call_id != call_id
            || record.tool_id != tool
        {
            return Err(StoreError::InvalidRecord(
                "context execution receipt scope mismatch".into(),
            ));
        }
        Ok(Some(argentum_domain::working_context::ObservationReceipt {
            audit_id: record.id,
            outcome: record.outcome,
            exit_code: record.exit_code,
        }))
    }

    /// Commit a whole validated group. Exact source retries are harmless;
    /// conflicting payloads for a committed identity fail without overwriting it.
    pub fn append_context_group(&self, group: &ContextGroup) -> Result<(), StoreError> {
        group
            .validate()
            .map_err(|reason| StoreError::InvalidRecord(reason.into()))?;
        let payload = serde_json::to_string(group)?;
        if payload.len() > MAX_GROUP_BYTES {
            return Err(StoreError::InvalidRecord(
                "context group exceeds byte bound".into(),
            ));
        }
        let mut connection = self
            .connection
            .lock()
            .map_err(|_| StoreError::LockPoisoned)?;
        let transaction = connection.transaction()?;
        let user_id: Option<String> = transaction
            .query_row(
                "SELECT c.id FROM conversation_messages c JOIN sessions s ON s.id = c.session_id
             WHERE c.project_id = ?1 AND c.session_id = ?2 AND c.run_id = ?3
               AND c.role = 'user' AND s.project_id = c.project_id",
                params![
                    group.project_id.to_string(),
                    group.session_id.to_string(),
                    group.run_id.to_string()
                ],
                |row| row.get(0),
            )
            .optional()?;
        let user_id = user_id.ok_or_else(|| {
            StoreError::InvalidRecord("context run has no scoped user source".into())
        })?;
        if let GroupBody::User { text } = &group.body {
            let matches_source: bool = transaction.query_row(
                "SELECT text = ?2 FROM conversation_messages WHERE id = ?1",
                params![user_id, text],
                |row| row.get(0),
            )?;
            if !matches_source {
                return Err(StoreError::InvalidRecord(
                    "context user does not match its source".into(),
                ));
            }
            transaction.execute(
                "INSERT INTO context_runs(run_id, project_id, session_id, task_id, user_message_id)
                 VALUES (?1, ?2, ?3, ?4, ?5) ON CONFLICT(run_id) DO NOTHING",
                params![
                    group.run_id.to_string(),
                    group.project_id.to_string(),
                    group.session_id.to_string(),
                    group.task_id.to_string(),
                    user_id
                ],
            )?;
        }
        let bound: bool = transaction.query_row(
            "SELECT EXISTS(SELECT 1 FROM context_runs WHERE run_id = ?1 AND project_id = ?2 AND session_id = ?3 AND task_id = ?4)",
            params![group.run_id.to_string(), group.project_id.to_string(), group.session_id.to_string(), group.task_id.to_string()],
            |row| row.get(0),
        )?;
        if !bound {
            return Err(StoreError::InvalidRecord(
                "context source scope mismatch".into(),
            ));
        }
        let existing: Option<String> = transaction
            .query_row(
                "SELECT CASE WHEN length(CAST(payload AS BLOB)) <= ?3 THEN payload END
             FROM context_groups WHERE run_id = ?1 AND ordinal = ?2",
                params![
                    group.run_id.to_string(),
                    group.ordinal,
                    MAX_GROUP_BYTES as i64
                ],
                |row| row.get(0),
            )
            .optional()?;
        if let Some(existing) = existing {
            if existing != payload {
                return Err(StoreError::InvalidRecord(
                    "conflicting context source identity".into(),
                ));
            }
        } else {
            let (next, complete): (u32, bool) = transaction.query_row(
                "SELECT next_ordinal, assistant_complete FROM context_runs WHERE run_id = ?1",
                [group.run_id.to_string()],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )?;
            if next != group.ordinal || complete {
                return Err(StoreError::InvalidRecord(
                    "context group sequence gap or completed source".into(),
                ));
            }
            transaction.execute(
                "INSERT INTO context_groups(run_id, ordinal, payload) VALUES (?1, ?2, ?3)",
                params![group.run_id.to_string(), group.ordinal, payload],
            )?;
            transaction.execute(
                "UPDATE context_runs SET next_ordinal = ?2 WHERE run_id = ?1",
                params![group.run_id.to_string(), group.ordinal + 1],
            )?;
        }
        if matches!(
            group.body,
            GroupBody::Assistant { .. } | GroupBody::AssistantOmitted { .. }
        ) {
            transaction.execute(
                "UPDATE context_runs SET assistant_complete = 1 WHERE run_id = ?1",
                [group.run_id.to_string()],
            )?;
        }
        transaction.commit()?;
        Ok(())
    }

    /// Source-bounded reader shared by request construction and inspection.
    /// CASE bounds each body in SQLite before Rust allocates a String.
    pub fn context_sources(
        &self,
        project_id: ProjectId,
        session_id: SessionId,
    ) -> Result<ContextSourceWindow, StoreError> {
        self.context_sources_before_run(project_id, session_id, None)
    }

    pub fn context_sources_before_run(
        &self,
        project_id: ProjectId,
        session_id: SessionId,
        exclude_run: Option<RunId>,
    ) -> Result<ContextSourceWindow, StoreError> {
        if let Some(fork) = self.session_fork_snapshot(project_id, session_id)? {
            let inherited = self.context_sources_for_fork(&fork)?;
            let child =
                self.context_sources_direct_before_run(project_id, session_id, exclude_run)?;
            let mut sources = inherited.sources;
            sources.extend(child.sources);
            let excess = sources.len().saturating_sub(MAX_SOURCE_GROUPS);
            if excess > 0 {
                sources.drain(0..excess);
            }
            return Ok(ContextSourceWindow {
                total_sources: inherited.total_sources.saturating_add(child.total_sources),
                omitted_older: inherited
                    .omitted_older
                    .saturating_add(child.omitted_older)
                    .saturating_add(excess as u64),
                first_source: inherited.first_source.or(child.first_source),
                last_source: child.last_source.or(inherited.last_source),
                runs_without_completed_assistant: child.runs_without_completed_assistant,
                inconsistent_source_runs: inherited
                    .inconsistent_source_runs
                    .saturating_add(child.inconsistent_source_runs),
                missing_checkpoint_groups: inherited
                    .missing_checkpoint_groups
                    .saturating_add(child.missing_checkpoint_groups),
                sources,
            });
        }
        self.context_sources_direct_before_run(project_id, session_id, exclude_run)
    }

    fn context_sources_direct_before_run(
        &self,
        project_id: ProjectId,
        session_id: SessionId,
        exclude_run: Option<RunId>,
    ) -> Result<ContextSourceWindow, StoreError> {
        let connection = self
            .connection
            .lock()
            .map_err(|_| StoreError::LockPoisoned)?;
        // One read snapshot, including metadata counts and selected bodies.
        let connection = connection.unchecked_transaction()?;
        let belongs: bool = connection.query_row(
            "SELECT EXISTS(SELECT 1 FROM sessions WHERE id = ?1 AND project_id = ?2)",
            params![session_id.to_string(), project_id.to_string()],
            |row| row.get(0),
        )?;
        if !belongs {
            return Err(StoreError::InvalidRecord(
                "context session scope mismatch".into(),
            ));
        }
        let source_sql = "SELECT c.rowid AS source_order, g.ordinal AS ordinal, 'group' AS kind,
                CASE WHEN length(CAST(r.run_id AS BLOB)) = 36 THEN r.run_id ELSE '' END AS run_id, '' AS status,
                CASE WHEN length(CAST(r.task_id AS BLOB)) = 36 THEN r.task_id ELSE '' END AS task_id,
                CASE WHEN length(CAST(r.run_id AS BLOB)) = 36 THEN r.run_id ELSE '' END AS source_id,
                (r.next_ordinal = (SELECT COUNT(*) FROM context_groups x WHERE x.run_id = r.run_id)
                 AND r.next_ordinal = (SELECT COALESCE(MAX(ordinal) + 1, 0) FROM context_groups x WHERE x.run_id = r.run_id)) AS consistent
            FROM context_runs r JOIN context_groups g ON g.run_id = r.run_id
            JOIN conversation_messages c ON c.id = r.user_message_id
            WHERE r.project_id = ?1 AND r.session_id = ?2 AND (?3 IS NULL OR r.run_id <> ?3)
            UNION ALL
            SELECT c.rowid, 0, CASE c.role WHEN 'user' THEN 'user' WHEN 'assistant' THEN 'assistant' ELSE 'invalid' END,
                CASE WHEN length(CAST(c.run_id AS BLOB)) = 36 THEN c.run_id ELSE '' END,
                CASE c.status WHEN 'complete' THEN 'complete' WHEN 'failed' THEN 'failed' WHEN 'interrupted' THEN 'interrupted' ELSE 'invalid' END,
                '', CASE WHEN length(CAST(c.id AS BLOB)) = 36 THEN c.id ELSE '' END, 1
            FROM conversation_messages c
            WHERE c.project_id = ?1 AND c.session_id = ?2 AND (?3 IS NULL OR c.run_id <> ?3)
            AND NOT EXISTS(SELECT 1 FROM context_runs r WHERE r.run_id = c.run_id)";
        let total_sources: u64 = connection.query_row(
            &format!("SELECT COUNT(*) FROM ({source_sql})"),
            params![
                project_id.to_string(),
                session_id.to_string(),
                exclude_run.map(|id| id.to_string())
            ],
            |row| row.get(0),
        )?;
        let range_edge = |direction: &str| -> Result<Option<String>, StoreError> {
            Ok(connection
                .query_row(
                    &format!(
                        "SELECT source_id || ':' || ordinal || ':' || kind FROM ({source_sql})
                 ORDER BY source_order {direction}, ordinal {direction} LIMIT 1"
                    ),
                    params![
                        project_id.to_string(),
                        session_id.to_string(),
                        exclude_run.map(|id| id.to_string())
                    ],
                    |row| row.get(0),
                )
                .optional()?)
        };
        let first_source = range_edge("ASC")?;
        let last_source = range_edge("DESC")?;
        let mut statement = connection.prepare(&format!(
            "SELECT kind, run_id, status, source_id, task_id, ordinal, consistent
             FROM ({source_sql}) ORDER BY source_order DESC, ordinal DESC LIMIT ?4"
        ))?;
        let rows = statement.query_map(
            params![
                project_id.to_string(),
                session_id.to_string(),
                exclude_run.map(|id| id.to_string()),
                MAX_SOURCE_GROUPS as i64
            ],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, String>(4)?,
                    row.get::<_, u32>(5)?,
                    row.get::<_, bool>(6)?,
                ))
            },
        )?;
        let mut sources = Vec::new();
        let mut bytes = 0usize;
        for row in rows {
            let (kind, run, status, source_id, task_id, ordinal, consistent) = row?;
            let source_key = format!("{source_id}:{ordinal}:{kind}");
            if !consistent {
                sources.push(ContextSource::Unavailable {
                    reason: "context_source_checkpoint_gap",
                    source: source_key,
                });
                continue;
            }
            // Sorting and LIMIT above touch keys only, not historical payloads.
            let (size, body): (usize, Option<String>) = if kind == "group" {
                connection.query_row(
                    "SELECT length(CAST(payload AS BLOB)), CASE WHEN length(CAST(payload AS BLOB)) <= ?3 THEN payload END
                     FROM context_groups WHERE run_id = ?1 AND ordinal = ?2",
                    params![run, ordinal, MAX_GROUP_BYTES.saturating_sub(bytes) as i64],
                    |row| Ok((row.get(0)?, row.get(1)?)),
                )?
            } else {
                connection.query_row(
                    "SELECT length(CAST(text AS BLOB)), CASE WHEN length(CAST(text AS BLOB)) <= ?4 THEN text END
                     FROM conversation_messages WHERE id = ?1 AND project_id = ?2 AND session_id = ?3",
                    params![source_id, project_id.to_string(), session_id.to_string(), MAX_GROUP_BYTES.saturating_sub(bytes) as i64],
                    |row| Ok((row.get(0)?, row.get(1)?)),
                )?
            };
            let Some(body) = body else {
                if size > MAX_GROUP_BYTES {
                    sources.push(ContextSource::Unavailable {
                        reason: "source_body_limit",
                        source: source_key,
                    });
                }
                break;
            };
            bytes += body.len();
            let source = if kind == "group" {
                match serde_json::from_str::<ContextGroup>(&body) {
                    Ok(group)
                        if group.validate().is_ok()
                            && group.project_id == project_id
                            && group.session_id == session_id
                            && group.run_id.to_string() == run
                            && group.task_id.to_string() == task_id
                            && group.ordinal == ordinal =>
                    {
                        ContextSource::Group(group)
                    }
                    _ => ContextSource::Unavailable {
                        reason: "invalid_context_group",
                        source: source_key,
                    },
                }
            } else {
                let run_id = run
                    .parse()
                    .map_err(|_| StoreError::InvalidRecord("invalid legacy context run".into()))?;
                let message_id = source_id.parse().map_err(|_| {
                    StoreError::InvalidRecord("invalid legacy context message id".into())
                })?;
                let role = super::parse_conversation_role(&kind)?;
                let complete = super::parse_conversation_status(&status)?
                    == argentum_domain::ConversationMessageStatus::Complete;
                ContextSource::Legacy {
                    session_id,
                    message_id,
                    run_id,
                    role,
                    text: body,
                    complete,
                }
            };
            sources.push(source);
        }
        sources.reverse();
        let runs_without_completed_assistant = connection.query_row(
            "SELECT COUNT(*) FROM context_runs WHERE project_id = ?1 AND session_id = ?2 AND assistant_complete = 0 AND (?3 IS NULL OR run_id <> ?3)",
            params![project_id.to_string(), session_id.to_string(), exclude_run.map(|id| id.to_string())], |row| row.get(0),
        )?;
        let (inconsistent_source_runs, missing_checkpoint_groups) = connection.query_row(
            "SELECT COUNT(*), COALESCE(SUM(MAX(next_ordinal - actual, 0)), 0) FROM (
                SELECT r.next_ordinal, (SELECT COUNT(*) FROM context_groups g WHERE g.run_id = r.run_id) AS actual,
                    (SELECT COALESCE(MAX(ordinal) + 1, 0) FROM context_groups g WHERE g.run_id = r.run_id) AS last_ordinal
                FROM context_runs r WHERE project_id = ?1 AND session_id = ?2 AND (?3 IS NULL OR run_id <> ?3)
             ) WHERE next_ordinal <> actual OR next_ordinal <> last_ordinal",
            params![project_id.to_string(), session_id.to_string(), exclude_run.map(|id| id.to_string())],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )?;
        Ok(ContextSourceWindow {
            omitted_older: total_sources.saturating_sub(sources.len() as u64),
            sources,
            total_sources,
            runs_without_completed_assistant,
            first_source,
            last_source,
            inconsistent_source_runs,
            missing_checkpoint_groups,
        })
    }

    fn context_sources_for_fork(
        &self,
        fork: &argentum_domain::SessionForkSnapshot,
    ) -> Result<ContextSourceWindow, StoreError> {
        let connection = self
            .connection
            .lock()
            .map_err(|_| StoreError::LockPoisoned)?;
        let connection = connection.unchecked_transaction()?;
        let mut sources = Vec::with_capacity(fork.sources.len());
        let mut bytes = 0usize;
        for source in &fork.sources {
            let loaded = match source {
                SessionForkSource::ContextGroup { run_id, ordinal } => {
                    let payload = connection
                        .query_row(
                            "SELECT CASE WHEN length(CAST(g.payload AS BLOB)) <= ?5 THEN g.payload END
                             FROM context_groups g
                             JOIN context_runs r ON r.run_id = g.run_id
                             WHERE r.project_id = ?1 AND r.session_id = ?2
                               AND r.run_id = ?3 AND g.ordinal = ?4",
                            params![
                                fork.project_id.to_string(),
                                fork.source_session_id.to_string(),
                                run_id.to_string(),
                                ordinal,
                                MAX_GROUP_BYTES.saturating_sub(bytes) as i64
                            ],
                            |row| row.get::<_, Option<String>>(0),
                        )
                        .optional()?
                        .flatten();
                    match payload {
                        Some(payload) => {
                            bytes = bytes.saturating_add(payload.len());
                            match serde_json::from_str::<ContextGroup>(&payload) {
                                Ok(group)
                                    if group.validate().is_ok()
                                        && group.project_id == fork.project_id
                                        && group.session_id == fork.source_session_id
                                        && group.run_id == *run_id
                                        && group.ordinal == *ordinal =>
                                {
                                    ContextSource::Group(group)
                                }
                                _ => ContextSource::Unavailable {
                                    reason: "fork_source_invalid",
                                    source: format!("{}:{ordinal}:group", run_id),
                                },
                            }
                        }
                        None => ContextSource::Unavailable {
                            reason: "fork_source_unavailable",
                            source: format!("{}:{ordinal}:group", run_id),
                        },
                    }
                }
                SessionForkSource::LegacyMessage { message_id } => {
                    let row = connection
                        .query_row(
                            "SELECT run_id, role, status,
                                    CASE WHEN length(CAST(text AS BLOB)) <= ?4 THEN text END
                             FROM conversation_messages
                             WHERE id = ?1 AND project_id = ?2 AND session_id = ?3",
                            params![
                                message_id.to_string(),
                                fork.project_id.to_string(),
                                fork.source_session_id.to_string(),
                                MAX_GROUP_BYTES.saturating_sub(bytes) as i64
                            ],
                            |row| {
                                Ok((
                                    row.get::<_, String>(0)?,
                                    row.get::<_, String>(1)?,
                                    row.get::<_, String>(2)?,
                                    row.get::<_, Option<String>>(3)?,
                                ))
                            },
                        )
                        .optional()?;
                    match row {
                        Some((run_id, role, status, Some(text))) => {
                            bytes = bytes.saturating_add(text.len());
                            ContextSource::Legacy {
                                session_id: fork.source_session_id,
                                message_id: *message_id,
                                run_id: run_id.parse().map_err(|_| {
                                    StoreError::InvalidRecord(
                                        "fork legacy run identity is invalid".into(),
                                    )
                                })?,
                                role: super::parse_conversation_role(&role)?,
                                text,
                                complete: super::parse_conversation_status(&status)?
                                    == argentum_domain::ConversationMessageStatus::Complete,
                            }
                        }
                        _ => ContextSource::Unavailable {
                            reason: "fork_source_unavailable",
                            source: format!("{message_id}:0:legacy"),
                        },
                    }
                }
            };
            sources.push(loaded);
        }
        let first_source = fork.sources.first().map(fork_source_identity);
        let last_source = fork.sources.last().map(fork_source_identity);
        Ok(ContextSourceWindow {
            total_sources: fork.sources.len() as u64,
            omitted_older: fork.omitted_older_sources,
            first_source,
            last_source,
            runs_without_completed_assistant: 0,
            inconsistent_source_runs: 0,
            missing_checkpoint_groups: 0,
            sources,
        })
    }
}

fn fork_source_identity(source: &SessionForkSource) -> String {
    match source {
        SessionForkSource::ContextGroup { run_id, ordinal } => {
            format!("{run_id}:{ordinal}:group")
        }
        SessionForkSource::LegacyMessage { message_id } => {
            format!("{message_id}:0:legacy")
        }
    }
}
