use std::path::PathBuf;

use argentum_domain::{
    MessageId, ProjectId, Session, SessionForkCutoff, SessionForkPreview, SessionForkSnapshot,
    SessionForkSource, SessionId, MAX_SESSION_FORK_MESSAGES, MAX_SESSION_FORK_METADATA_BYTES,
    MAX_SESSION_FORK_SOURCES, SESSION_FORK_WARNING,
};
use rusqlite::{params, Connection, OptionalExtension, TransactionBehavior};

use super::{
    append_event_on, insert_session, new_session_record, validate_session_title, Store, StoreError,
};

pub const SESSION_FORK_VERSION: u16 = 1;

#[derive(Debug, Clone)]
pub struct SessionForkPlan {
    pub project_id: ProjectId,
    pub source_session_id: SessionId,
    pub source_title: String,
    pub destination_title: String,
    pub cutoff: SessionForkCutoff,
    pub sources: Vec<SessionForkSource>,
    pub source_message_ids: Vec<MessageId>,
    pub omitted_older_sources: u64,
}

impl SessionForkPlan {
    pub fn preview(&self, workspace: PathBuf) -> SessionForkPreview {
        SessionForkPreview {
            project_id: self.project_id,
            source_session_id: self.source_session_id,
            source_title: self.source_title.clone(),
            destination_title: self.destination_title.clone(),
            workspace,
            cutoff: self.cutoff.clone(),
            omitted_older_sources: self.omitted_older_sources,
            warning: SESSION_FORK_WARNING.to_owned(),
        }
    }
}

impl Store {
    pub fn session_fork_plan(
        &self,
        project_id: ProjectId,
        source_session_id: SessionId,
        title: &str,
    ) -> Result<SessionForkPlan, StoreError> {
        let connection = self
            .connection
            .lock()
            .map_err(|_| StoreError::LockPoisoned)?;
        build_session_fork_plan_on(&connection, project_id, source_session_id, title)
    }

    pub fn create_session_fork_with_event(
        &self,
        workspace_key: &str,
        project_id: ProjectId,
        source_session_id: SessionId,
        title: &str,
        expected_last_message_id: Option<MessageId>,
    ) -> Result<(Session, SessionForkSnapshot), StoreError> {
        if workspace_key.is_empty() || workspace_key.len() > 32 * 1024 {
            return Err(StoreError::InvalidRecord(
                "invalid fork workspace identity".into(),
            ));
        }
        let mut connection = self
            .connection
            .lock()
            .map_err(|_| StoreError::LockPoisoned)?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let (session, snapshot) = create_session_fork_on(
            &transaction,
            workspace_key,
            project_id,
            source_session_id,
            title,
            expected_last_message_id,
        )?;
        transaction.commit()?;
        Ok((session, snapshot))
    }

    pub fn session_fork_snapshot(
        &self,
        project_id: ProjectId,
        child_session_id: SessionId,
    ) -> Result<Option<SessionForkSnapshot>, StoreError> {
        let connection = self
            .connection
            .lock()
            .map_err(|_| StoreError::LockPoisoned)?;
        load_session_fork_on(&connection, project_id, child_session_id)
    }
}

pub(crate) fn create_session_fork_on(
    connection: &Connection,
    workspace_key: &str,
    project_id: ProjectId,
    source_session_id: SessionId,
    title: &str,
    expected_last_message_id: Option<MessageId>,
) -> Result<(Session, SessionForkSnapshot), StoreError> {
    let plan = build_session_fork_plan_on(connection, project_id, source_session_id, title)?;
    if expected_last_message_id.is_some_and(|expected| expected != plan.cutoff.last_message_id) {
        return Err(StoreError::InvalidRecord(
            "fork source changed after preview; review the latest settled cutoff again".into(),
        ));
    }
    let session = new_session_record(project_id, title);
    let snapshot = SessionForkSnapshot {
        version: SESSION_FORK_VERSION,
        project_id,
        child_session_id: session.id,
        source_session_id,
        source_title_at_fork: plan.source_title,
        destination_title: plan.destination_title,
        cutoff: plan.cutoff,
        sources: plan.sources,
        source_message_ids: plan.source_message_ids,
        omitted_older_sources: plan.omitted_older_sources,
        created_at: session.created_at,
    };
    let payload = serde_json::to_string(&snapshot)?;
    if payload.len() > MAX_SESSION_FORK_METADATA_BYTES {
        return Err(StoreError::InvalidRecord(
            "fork lineage metadata exceeds its configured capacity".into(),
        ));
    }
    insert_session(connection, &session)?;
    connection.execute(
        "INSERT INTO session_forks (
            child_session_id, project_id, source_session_id, created_at, payload
         ) VALUES (?1, ?2, ?3, ?4, ?5)",
        params![
            session.id.to_string(),
            project_id.to_string(),
            source_session_id.to_string(),
            snapshot.created_at.unix_timestamp(),
            payload,
        ],
    )?;
    append_event_on(
        connection,
        Some(workspace_key),
        Some(project_id),
        Some(session.id),
        None,
        &argentum_domain::AppEvent::SessionForked(snapshot.clone()),
    )?;
    Ok((session, snapshot))
}

fn build_session_fork_plan_on(
    connection: &Connection,
    project_id: ProjectId,
    source_session_id: SessionId,
    title: &str,
) -> Result<SessionForkPlan, StoreError> {
    validate_session_title(title)?;
    let source = connection
        .query_row(
            "SELECT title, archived_at IS NOT NULL
             FROM sessions WHERE id = ?1 AND project_id = ?2",
            params![source_session_id.to_string(), project_id.to_string()],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, bool>(1)?)),
        )
        .optional()?
        .ok_or_else(|| {
            StoreError::InvalidRecord("fork source does not belong to the current project".into())
        })?;
    if source.1 {
        return Err(StoreError::InvalidRecord(
            "archived fork sources must be unarchived first".into(),
        ));
    }
    let nested: bool = connection.query_row(
        "SELECT EXISTS(SELECT 1 FROM session_forks WHERE child_session_id = ?1)",
        [source_session_id.to_string()],
        |row| row.get(0),
    )?;
    if nested {
        return Err(StoreError::InvalidRecord(
            "nested conversation forks are not available in this version".into(),
        ));
    }
    let durable_busy: bool = connection.query_row(
        "SELECT
            EXISTS(SELECT 1 FROM runtime_run_ownership
                   WHERE project_id = ?1 AND session_id = ?2 AND state = 'active')
            OR EXISTS(SELECT 1 FROM command_identities
                      WHERE project_id = ?1 AND session_id = ?2
                        AND state IN ('accepted', 'executing'))
            OR EXISTS(SELECT 1 FROM goal_invocations
                      WHERE project_id = ?1 AND session_id = ?2 AND state = 'active')
            OR EXISTS(SELECT 1 FROM change_operations
                      WHERE project_id = ?1 AND session_id = ?2 AND state = 'prepared')",
        params![project_id.to_string(), source_session_id.to_string()],
        |row| row.get(0),
    )?;
    if durable_busy {
        return Err(StoreError::InvalidRecord(
            "cannot fork a session with admitted, unresolved, or pending work".into(),
        ));
    }
    let incomplete_context: bool = connection.query_row(
        "SELECT EXISTS(
            SELECT 1 FROM context_runs r
            WHERE r.project_id = ?1 AND r.session_id = ?2
              AND (r.assistant_complete = 0
                   OR r.next_ordinal <> (SELECT COUNT(*) FROM context_groups g WHERE g.run_id = r.run_id)
                   OR r.next_ordinal <> (SELECT COALESCE(MAX(ordinal) + 1, 0) FROM context_groups g WHERE g.run_id = r.run_id))
        )",
        params![project_id.to_string(), source_session_id.to_string()],
        |row| row.get(0),
    )?;
    if incomplete_context {
        return Err(StoreError::InvalidRecord(
            "cannot fork before the source conversation reaches a settled boundary".into(),
        ));
    }

    let mut message_statement = connection.prepare(
        "SELECT id, role FROM conversation_messages
         WHERE project_id = ?1 AND session_id = ?2
         ORDER BY created_at ASC, rowid ASC
         LIMIT ?3",
    )?;
    let message_rows = message_statement.query_map(
        params![
            project_id.to_string(),
            source_session_id.to_string(),
            (MAX_SESSION_FORK_MESSAGES + 1) as i64
        ],
        |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
    )?;
    let mut source_message_ids = Vec::new();
    let mut last_role = String::new();
    for row in message_rows {
        let (id, role) = row?;
        source_message_ids.push(id.parse().map_err(|_| {
            StoreError::InvalidRecord("fork source message identity is invalid".into())
        })?);
        last_role = role;
    }
    if source_message_ids.is_empty() {
        return Err(StoreError::InvalidRecord(
            "the source conversation is empty; use a new session instead".into(),
        ));
    }
    if source_message_ids.len() > MAX_SESSION_FORK_MESSAGES {
        return Err(StoreError::InvalidRecord(
            "fork source message count exceeds its configured capacity".into(),
        ));
    }
    if last_role != "assistant" {
        return Err(StoreError::InvalidRecord(
            "cannot fork while the latest conversation exchange is unresolved".into(),
        ));
    }

    let source_sql = "SELECT c.rowid AS source_order, g.ordinal, 'group' AS kind,
            r.run_id AS source_id
        FROM context_runs r
        JOIN context_groups g ON g.run_id = r.run_id
        JOIN conversation_messages c ON c.id = r.user_message_id
        WHERE r.project_id = ?1 AND r.session_id = ?2
        UNION ALL
        SELECT c.rowid, 0, 'legacy', c.id
        FROM conversation_messages c
        WHERE c.project_id = ?1 AND c.session_id = ?2
          AND NOT EXISTS(SELECT 1 FROM context_runs r WHERE r.run_id = c.run_id)
        ORDER BY source_order ASC, ordinal ASC
        LIMIT ?3";
    let mut source_statement = connection.prepare(source_sql)?;
    let source_rows = source_statement.query_map(
        params![
            project_id.to_string(),
            source_session_id.to_string(),
            (MAX_SESSION_FORK_SOURCES + 1) as i64
        ],
        |row| {
            Ok((
                row.get::<_, u32>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
            ))
        },
    )?;
    let mut sources = Vec::new();
    for row in source_rows {
        let (ordinal, kind, source_id) = row?;
        sources.push(if kind == "group" {
            SessionForkSource::ContextGroup {
                run_id: source_id.parse().map_err(|_| {
                    StoreError::InvalidRecord("fork context run identity is invalid".into())
                })?,
                ordinal,
            }
        } else {
            SessionForkSource::LegacyMessage {
                message_id: source_id.parse().map_err(|_| {
                    StoreError::InvalidRecord("fork legacy message identity is invalid".into())
                })?,
            }
        });
    }
    if sources.is_empty() {
        return Err(StoreError::InvalidRecord(
            "the source conversation has no durable context; use a new session instead".into(),
        ));
    }
    if sources.len() > MAX_SESSION_FORK_SOURCES {
        return Err(StoreError::InvalidRecord(
            "fork source group count exceeds its configured capacity".into(),
        ));
    }
    let cutoff = SessionForkCutoff {
        last_message_id: *source_message_ids
            .last()
            .expect("non-empty source messages"),
        last_source: sources.last().expect("non-empty fork sources").clone(),
        source_count: sources.len(),
        message_count: source_message_ids.len(),
    };
    Ok(SessionForkPlan {
        project_id,
        source_session_id,
        source_title: source.0,
        destination_title: title.to_owned(),
        cutoff,
        sources,
        source_message_ids,
        omitted_older_sources: 0,
    })
}

pub(crate) fn load_session_fork_on(
    connection: &Connection,
    project_id: ProjectId,
    child_session_id: SessionId,
) -> Result<Option<SessionForkSnapshot>, StoreError> {
    let payload = connection
        .query_row(
            "SELECT CASE WHEN length(CAST(payload AS BLOB)) <= ?3 THEN payload END
             FROM session_forks WHERE project_id = ?1 AND child_session_id = ?2",
            params![
                project_id.to_string(),
                child_session_id.to_string(),
                MAX_SESSION_FORK_METADATA_BYTES as i64
            ],
            |row| row.get::<_, Option<String>>(0),
        )
        .optional()?
        .flatten();
    let Some(payload) = payload else {
        return Ok(None);
    };
    let snapshot: SessionForkSnapshot = serde_json::from_str(&payload)?;
    if snapshot.version != SESSION_FORK_VERSION
        || snapshot.project_id != project_id
        || snapshot.child_session_id != child_session_id
        || snapshot.sources.len() > MAX_SESSION_FORK_SOURCES
        || snapshot.source_message_ids.len() > MAX_SESSION_FORK_MESSAGES
        || snapshot.sources.len() != snapshot.cutoff.source_count
        || snapshot.source_message_ids.len() != snapshot.cutoff.message_count
    {
        return Err(StoreError::InvalidRecord(
            "stored session fork lineage is invalid".into(),
        ));
    }
    Ok(Some(snapshot))
}

#[cfg(test)]
mod tests {
    use argentum_domain::working_context::{
        ContextGroup, GroupBody, Observation, ObservationState, Retention, PROJECTION_VERSION,
    };
    use argentum_domain::{
        now, ConversationMessage, ConversationMessageStatus, ConversationRole, RunId,
    };

    use super::*;

    fn message(
        project_id: ProjectId,
        session_id: SessionId,
        run_id: RunId,
        role: ConversationRole,
        text: &str,
    ) -> ConversationMessage {
        ConversationMessage {
            id: MessageId::new_v4(),
            project_id,
            session_id,
            run_id,
            role,
            text: text.into(),
            reasoning: String::new(),
            usage: None,
            profile_id: if role == ConversationRole::Assistant {
                "fixture".into()
            } else {
                String::new()
            },
            model: if role == ConversationRole::Assistant {
                "fixture".into()
            } else {
                String::new()
            },
            status: ConversationMessageStatus::Complete,
            created_at: now(),
        }
    }

    fn settled_pair(
        store: &Store,
        project_id: ProjectId,
        session_id: SessionId,
        user: &str,
        assistant: &str,
    ) -> (MessageId, MessageId) {
        let run_id = RunId::new_v4();
        let user = message(project_id, session_id, run_id, ConversationRole::User, user);
        let assistant = message(
            project_id,
            session_id,
            run_id,
            ConversationRole::Assistant,
            assistant,
        );
        let ids = (user.id, assistant.id);
        store.append_conversation_message(&user).expect("user");
        store
            .append_conversation_message(&assistant)
            .expect("assistant");
        ids
    }

    #[test]
    fn fork_pins_the_settled_boundary_without_selecting_or_copying_later_messages() {
        let directory = tempfile::tempdir().expect("directory");
        let store = Store::open_in_memory().expect("store");
        let resolution = store
            .resolve_workspace(directory.path())
            .expect("workspace");
        let project_id = resolution.snapshot.project.id;
        let source_session_id = resolution.snapshot.active_session_id.expect("session");
        let (_, cutoff_message) = settled_pair(
            &store,
            project_id,
            source_session_id,
            "first user",
            "first assistant",
        );
        let preview = store
            .session_fork_plan(project_id, source_session_id, "Branch")
            .expect("preview");
        assert_eq!(preview.cutoff.last_message_id, cutoff_message);
        let (_, fork) = store
            .create_session_fork_with_event(
                &resolution.workspace_key,
                project_id,
                source_session_id,
                "Branch",
                Some(cutoff_message),
            )
            .expect("fork");
        assert_eq!(
            store
                .workspace_snapshot(project_id)
                .expect("snapshot")
                .active_session_id,
            Some(source_session_id)
        );
        settled_pair(
            &store,
            project_id,
            source_session_id,
            "later user",
            "later assistant",
        );
        let child = store
            .conversation_snapshot(project_id, fork.child_session_id)
            .expect("child conversation");
        let text = child
            .messages
            .iter()
            .map(|message| message.text.as_str())
            .collect::<Vec<_>>()
            .join("\n");
        assert!(text.contains("first user"));
        assert!(text.contains("first assistant"));
        assert!(!text.contains("later user"));
        assert!(!text.contains("later assistant"));
        assert!(text.contains("Historical only"));
    }

    #[test]
    fn fork_rejects_unsettled_archived_nested_and_stale_sources() {
        let directory = tempfile::tempdir().expect("directory");
        let store = Store::open_in_memory().expect("store");
        let resolution = store
            .resolve_workspace(directory.path())
            .expect("workspace");
        let project_id = resolution.snapshot.project.id;
        let source_session_id = resolution.snapshot.active_session_id.expect("session");
        let run_id = RunId::new_v4();
        store
            .append_conversation_message(&message(
                project_id,
                source_session_id,
                run_id,
                ConversationRole::User,
                "pending",
            ))
            .expect("pending user");
        assert!(store
            .session_fork_plan(project_id, source_session_id, "Rejected")
            .unwrap_err()
            .to_string()
            .contains("unresolved"));
        store
            .append_conversation_message(&message(
                project_id,
                source_session_id,
                run_id,
                ConversationRole::Assistant,
                "settled",
            ))
            .expect("settled assistant");
        let plan = store
            .session_fork_plan(project_id, source_session_id, "Child")
            .expect("plan");
        settled_pair(
            &store,
            project_id,
            source_session_id,
            "changed",
            "changed answer",
        );
        assert!(store
            .create_session_fork_with_event(
                &resolution.workspace_key,
                project_id,
                source_session_id,
                "Child",
                Some(plan.cutoff.last_message_id),
            )
            .unwrap_err()
            .to_string()
            .contains("changed after preview"));
        let latest = store
            .session_fork_plan(project_id, source_session_id, "Child")
            .expect("latest plan");
        let (_, fork) = store
            .create_session_fork_with_event(
                &resolution.workspace_key,
                project_id,
                source_session_id,
                "Child",
                Some(latest.cutoff.last_message_id),
            )
            .expect("child");
        assert!(store
            .session_fork_plan(project_id, fork.child_session_id, "Nested")
            .unwrap_err()
            .to_string()
            .contains("nested"));
        store
            .select_session(project_id, fork.child_session_id)
            .expect("select child");
        store
            .set_session_archived_with_event(
                &resolution.workspace_key,
                project_id,
                source_session_id,
                true,
            )
            .expect("archive");
        assert!(store
            .session_fork_plan(project_id, source_session_id, "Archived")
            .unwrap_err()
            .to_string()
            .contains("unarchived"));
    }

    #[test]
    fn fork_creation_rolls_back_if_its_event_cannot_be_persisted() {
        let directory = tempfile::tempdir().expect("directory");
        let store = Store::open_in_memory().expect("store");
        let resolution = store
            .resolve_workspace(directory.path())
            .expect("workspace");
        let project_id = resolution.snapshot.project.id;
        let source_session_id = resolution.snapshot.active_session_id.expect("session");
        settled_pair(&store, project_id, source_session_id, "user", "assistant");
        let before = store.workspace_snapshot(project_id).expect("before");
        store
            .connection
            .lock()
            .expect("connection")
            .execute_batch(
                "CREATE TRIGGER fail_fork_event
                 BEFORE INSERT ON event_log
                 WHEN NEW.event_type = 'session_forked'
                 BEGIN
                     SELECT RAISE(FAIL, 'fork event rejected');
                 END;",
            )
            .expect("trigger");
        assert!(store
            .create_session_fork_with_event(
                &resolution.workspace_key,
                project_id,
                source_session_id,
                "Must roll back",
                None,
            )
            .is_err());
        assert_eq!(store.workspace_snapshot(project_id).expect("after"), before);
    }

    #[test]
    fn fork_projects_exact_tool_groups_and_keeps_not_executed_history_historical() {
        let directory = tempfile::tempdir().expect("directory");
        let store = Store::open_in_memory().expect("store");
        let resolution = store
            .resolve_workspace(directory.path())
            .expect("workspace");
        let project_id = resolution.snapshot.project.id;
        let source_session_id = resolution.snapshot.active_session_id.expect("session");
        let task_id = argentum_domain::TaskId::new_v4();
        let run_id = RunId::new_v4();
        let user = message(
            project_id,
            source_session_id,
            run_id,
            ConversationRole::User,
            "use a tool",
        );
        store.append_conversation_message(&user).expect("user");
        for (ordinal, body) in [
            (
                0,
                GroupBody::User {
                    text: "use a tool".into(),
                },
            ),
            (
                1,
                GroupBody::Exchange {
                    text: "I will try the write".into(),
                    text_retention: Retention::Retained,
                    observations: vec![Observation {
                        provider_call_id: "call-1".into(),
                        runtime_call_id: None,
                        tool: "write_text".into(),
                        state: ObservationState::NotAttempted,
                        receipt: None,
                        change: None,
                        arguments: "{}".into(),
                        retention: Retention::MetadataOnly,
                        result: None,
                    }],
                },
            ),
            (
                2,
                GroupBody::Assistant {
                    text: "tool was not executed".into(),
                },
            ),
        ] {
            store
                .append_context_group(&ContextGroup {
                    version: PROJECTION_VERSION,
                    project_id,
                    session_id: source_session_id,
                    task_id,
                    run_id,
                    ordinal,
                    body,
                })
                .expect("context group");
        }
        store
            .append_conversation_message(&message(
                project_id,
                source_session_id,
                run_id,
                ConversationRole::Assistant,
                "tool was not executed",
            ))
            .expect("assistant");
        let plan = store
            .session_fork_plan(project_id, source_session_id, "Tool history")
            .expect("plan");
        let (_, fork) = store
            .create_session_fork_with_event(
                &resolution.workspace_key,
                project_id,
                source_session_id,
                "Tool history",
                Some(plan.cutoff.last_message_id),
            )
            .expect("fork");
        settled_pair(
            &store,
            project_id,
            source_session_id,
            "later parent",
            "later parent answer",
        );

        let window = store
            .context_sources(project_id, fork.child_session_id)
            .expect("fork context");
        assert_eq!(window.sources.len(), 3);
        assert!(window.sources.iter().all(|source| match source {
            crate::ContextSource::Group(group) => group.run_id == run_id,
            _ => false,
        }));
        let snapshot = store
            .conversation_snapshot(project_id, fork.child_session_id)
            .expect("fork transcript");
        let rendered = snapshot
            .messages
            .iter()
            .map(|message| message.text.as_str())
            .collect::<Vec<_>>()
            .join("\n");
        assert!(rendered.contains("Historical tool exchange"));
        assert!(rendered.contains("not_attempted"));
        assert!(rendered.contains("No tool is being executed"));
        assert!(!rendered.contains("later parent"));
    }
}
