use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use argentum_domain::{
    now, AppEvent, ConversationMessage, ConversationMessageStatus, ConversationRole,
    ConversationSnapshot, Goal, HarnessExecutionPolicy, LayoutProfile, ModelUsage, Project,
    ProjectId, ProviderKind, ProviderProfile, RunId, Session, SessionId, SessionSummary,
    WorkspaceSnapshot,
};
use directories::ProjectDirs;
use rusqlite::{params, Connection, OptionalExtension};
use thiserror::Error;
use url::Url;

#[derive(Debug, Error)]
pub enum StoreError {
    #[error("database error: {0}")]
    Database(#[from] rusqlite::Error),
    #[error("serialization error: {0}")]
    Serialization(#[from] serde_json::Error),
    #[error("database lock was poisoned")]
    LockPoisoned,
    #[error("no application data directory is available")]
    MissingDataDirectory,
    #[error("workspace path could not be resolved: {path}")]
    WorkspacePath {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("stored record is invalid: {0}")]
    InvalidRecord(String),
}

pub const EVENT_PAYLOAD_VERSION: u16 = 1;
pub const MAX_CONVERSATION_MESSAGE_BYTES: usize = 512 * 1024;
pub const MAX_CONVERSATION_SNAPSHOT_MESSAGES: usize = 200;
pub const MAX_CONVERSATION_SNAPSHOT_BYTES: usize = 2 * 1024 * 1024;
pub const MAX_GOAL_OBJECTIVE_BYTES: usize = 16 * 1024;
pub const MAX_GOAL_NEXT_ACTION_BYTES: usize = 4 * 1024;
pub const MAX_GOAL_VERIFICATION_HISTORY: usize = 64;
const MAX_GOAL_TOKEN_BUDGET: u64 = 10_000_000;
const MAX_GOAL_TOOL_BUDGET: u32 = 100_000;
const MAX_GOAL_TIME_BUDGET_SECONDS: u64 = 7 * 24 * 60 * 60;
const DATABASE_BUSY_TIMEOUT: Duration = Duration::from_secs(5);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EventScope {
    pub workspace_key: String,
    pub project_id: ProjectId,
    pub session_id: Option<SessionId>,
    pub run_id: Option<RunId>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkspaceResolution {
    pub workspace_key: String,
    pub snapshot: WorkspaceSnapshot,
}

#[derive(Clone)]
pub struct Store {
    connection: Arc<Mutex<Connection>>,
    path: Option<PathBuf>,
}

impl std::fmt::Debug for Store {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("Store")
            .field("path", &self.path)
            .finish_non_exhaustive()
    }
}

impl Store {
    pub fn open(path: impl AsRef<Path>) -> Result<Self, StoreError> {
        let path = path.as_ref().to_path_buf();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|error| {
                StoreError::Database(rusqlite::Error::ToSqlConversionFailure(Box::new(error)))
            })?;
        }
        let connection = Connection::open(&path)?;
        connection.busy_timeout(DATABASE_BUSY_TIMEOUT)?;
        let store = Self {
            connection: Arc::new(Mutex::new(connection)),
            path: Some(path),
        };
        store.migrate()?;
        Ok(store)
    }

    pub fn open_default() -> Result<Self, StoreError> {
        let project_dirs = ProjectDirs::from("com", "Argentum", "Argentum")
            .ok_or(StoreError::MissingDataDirectory)?;
        Self::open(project_dirs.data_dir().join("argentum.db"))
    }

    pub fn open_in_memory() -> Result<Self, StoreError> {
        let connection = Connection::open_in_memory()?;
        connection.busy_timeout(DATABASE_BUSY_TIMEOUT)?;
        let store = Self {
            connection: Arc::new(Mutex::new(connection)),
            path: None,
        };
        store.migrate()?;
        Ok(store)
    }

    pub fn path(&self) -> Option<&Path> {
        self.path.as_deref()
    }

    pub fn append_event(&self, event: &AppEvent) -> Result<(), StoreError> {
        self.append_event_values(None, None, None, None, event)
    }

    pub fn append_event_scoped(
        &self,
        scope: &EventScope,
        event: &AppEvent,
    ) -> Result<(), StoreError> {
        self.append_event_values(
            Some(&scope.workspace_key),
            Some(scope.project_id),
            scope.session_id,
            scope.run_id,
            event,
        )
    }

    fn append_event_values(
        &self,
        workspace_key: Option<&str>,
        project_id: Option<ProjectId>,
        session_id: Option<SessionId>,
        run_id: Option<RunId>,
        event: &AppEvent,
    ) -> Result<(), StoreError> {
        let connection = self
            .connection
            .lock()
            .map_err(|_| StoreError::LockPoisoned)?;
        append_event_on(
            &connection,
            workspace_key,
            project_id,
            session_id,
            run_id,
            event,
        )
    }

    pub fn events(&self) -> Result<Vec<AppEvent>, StoreError> {
        let connection = self
            .connection
            .lock()
            .map_err(|_| StoreError::LockPoisoned)?;
        let mut statement = connection.prepare("SELECT payload FROM event_log ORDER BY id ASC")?;
        let rows = statement.query_map([], |row| row.get::<_, String>(0))?;
        let mut events = Vec::new();
        for row in rows {
            events.push(serde_json::from_str(&row?)?);
        }
        Ok(events)
    }

    pub fn events_for_workspace(
        &self,
        workspace_root: impl AsRef<Path>,
    ) -> Result<Vec<AppEvent>, StoreError> {
        let (_, workspace_key) = canonical_workspace(workspace_root.as_ref())?;
        let connection = self
            .connection
            .lock()
            .map_err(|_| StoreError::LockPoisoned)?;
        let mut statement = connection.prepare(
            "SELECT payload FROM event_log
             WHERE workspace_key = ?1
             ORDER BY id ASC",
        )?;
        let rows = statement.query_map([workspace_key], |row| row.get::<_, String>(0))?;
        let mut events = Vec::new();
        for row in rows {
            events.push(serde_json::from_str(&row?)?);
        }
        Ok(events)
    }

    pub fn resolve_workspace(
        &self,
        workspace_root: impl AsRef<Path>,
    ) -> Result<WorkspaceResolution, StoreError> {
        self.resolve_workspace_with_default_provider(
            workspace_root,
            &ProviderProfile::default_lm_studio(),
        )
    }

    pub fn resolve_workspace_with_default_provider(
        &self,
        workspace_root: impl AsRef<Path>,
        default_provider: &ProviderProfile,
    ) -> Result<WorkspaceResolution, StoreError> {
        let (workspace_root, workspace_key) = canonical_workspace(workspace_root.as_ref())?;
        let mut connection = self
            .connection
            .lock()
            .map_err(|_| StoreError::LockPoisoned)?;
        let transaction = connection.transaction()?;
        let stored = transaction
            .query_row(
                "SELECT payload FROM projects WHERE workspace_key = ?1",
                [&workspace_key],
                |row| row.get::<_, String>(0),
            )
            .optional()?;

        let project = if let Some(payload) = stored {
            serde_json::from_str::<Project>(&payload)?
        } else {
            let project = Project {
                id: ProjectId::new_v4(),
                name: workspace_name(&workspace_root),
                workspace_root: workspace_root.clone(),
                created_at: now(),
            };
            transaction.execute(
                "INSERT INTO projects (id, workspace_key, payload, active_session_id)
                 VALUES (?1, ?2, ?3, NULL)",
                params![
                    project.id.to_string(),
                    workspace_key,
                    serde_json::to_string(&project)?,
                ],
            )?;
            project
        };

        let mut sessions = load_sessions(&transaction, project.id)?;
        let mut active_session_id = load_active_session_id(&transaction, project.id)?;
        if sessions.is_empty() {
            let session = new_session_record(project.id, "New session");
            insert_session(&transaction, &session)?;
            transaction.execute(
                "UPDATE projects SET active_session_id = ?1 WHERE id = ?2",
                params![session.id.to_string(), project.id.to_string()],
            )?;
            active_session_id = Some(session.id);
            sessions.push(SessionSummary::from(&session));
        } else if active_session_id.is_none()
            || !sessions
                .iter()
                .any(|session| Some(session.id) == active_session_id)
        {
            active_session_id = sessions.last().map(|session| session.id);
            transaction.execute(
                "UPDATE projects SET active_session_id = ?1 WHERE id = ?2",
                params![
                    active_session_id.map(|value| value.to_string()),
                    project.id.to_string(),
                ],
            )?;
        }
        ensure_default_provider_profile(&transaction, project.id, default_provider)?;
        transaction.commit()?;

        Ok(WorkspaceResolution {
            workspace_key,
            snapshot: WorkspaceSnapshot {
                project,
                sessions,
                active_session_id,
            },
        })
    }

    pub fn workspace_snapshot(
        &self,
        project_id: ProjectId,
    ) -> Result<WorkspaceSnapshot, StoreError> {
        let connection = self
            .connection
            .lock()
            .map_err(|_| StoreError::LockPoisoned)?;
        let payload = connection
            .query_row(
                "SELECT payload FROM projects WHERE id = ?1",
                [project_id.to_string()],
                |row| row.get::<_, String>(0),
            )
            .optional()?
            .ok_or_else(|| StoreError::InvalidRecord("project does not exist".into()))?;
        let project = serde_json::from_str::<Project>(&payload)?;
        let sessions = load_sessions(&connection, project_id)?;
        let active_session_id = load_active_session_id(&connection, project_id)?;
        Ok(WorkspaceSnapshot {
            project,
            sessions,
            active_session_id,
        })
    }

    pub fn create_session(
        &self,
        project_id: ProjectId,
        title: impl Into<String>,
    ) -> Result<Session, StoreError> {
        let mut connection = self
            .connection
            .lock()
            .map_err(|_| StoreError::LockPoisoned)?;
        let transaction = connection.transaction()?;
        let project_exists = transaction.query_row(
            "SELECT EXISTS(SELECT 1 FROM projects WHERE id = ?1)",
            [project_id.to_string()],
            |row| row.get::<_, bool>(0),
        )?;
        if !project_exists {
            return Err(StoreError::InvalidRecord("project does not exist".into()));
        }
        let session = new_session_record(project_id, title);
        insert_session(&transaction, &session)?;
        transaction.execute(
            "UPDATE projects SET active_session_id = ?1 WHERE id = ?2",
            params![session.id.to_string(), project_id.to_string()],
        )?;
        transaction.commit()?;
        Ok(session)
    }

    pub fn create_session_with_event(
        &self,
        workspace_key: &str,
        project_id: ProjectId,
        title: impl Into<String>,
    ) -> Result<Session, StoreError> {
        let mut connection = self
            .connection
            .lock()
            .map_err(|_| StoreError::LockPoisoned)?;
        let transaction = connection.transaction()?;
        let project_exists = transaction.query_row(
            "SELECT EXISTS(
                SELECT 1 FROM projects WHERE id = ?1 AND workspace_key = ?2
             )",
            params![project_id.to_string(), workspace_key],
            |row| row.get::<_, bool>(0),
        )?;
        if !project_exists {
            return Err(StoreError::InvalidRecord("project does not exist".into()));
        }
        let session = new_session_record(project_id, title);
        insert_session(&transaction, &session)?;
        transaction.execute(
            "UPDATE projects SET active_session_id = ?1 WHERE id = ?2",
            params![session.id.to_string(), project_id.to_string()],
        )?;
        append_event_on(
            &transaction,
            Some(workspace_key),
            Some(project_id),
            Some(session.id),
            None,
            &AppEvent::SessionCreated(session.clone()),
        )?;
        transaction.commit()?;
        Ok(session)
    }

    pub fn select_session(
        &self,
        project_id: ProjectId,
        session_id: SessionId,
    ) -> Result<WorkspaceSnapshot, StoreError> {
        let connection = self
            .connection
            .lock()
            .map_err(|_| StoreError::LockPoisoned)?;
        let belongs_to_project = connection.query_row(
            "SELECT EXISTS(
                SELECT 1 FROM sessions WHERE id = ?1 AND project_id = ?2
             )",
            params![session_id.to_string(), project_id.to_string()],
            |row| row.get::<_, bool>(0),
        )?;
        if !belongs_to_project {
            return Err(StoreError::InvalidRecord(
                "session does not belong to the current project".into(),
            ));
        }
        connection.execute(
            "UPDATE projects SET active_session_id = ?1 WHERE id = ?2",
            params![session_id.to_string(), project_id.to_string()],
        )?;
        drop(connection);
        self.workspace_snapshot(project_id)
    }

    pub fn goal(
        &self,
        project_id: ProjectId,
        session_id: SessionId,
    ) -> Result<Option<Goal>, StoreError> {
        let connection = self
            .connection
            .lock()
            .map_err(|_| StoreError::LockPoisoned)?;
        let payload = connection
            .query_row(
                "SELECT payload FROM goals
                 WHERE project_id = ?1 AND session_id = ?2",
                params![project_id.to_string(), session_id.to_string()],
                |row| row.get::<_, String>(0),
            )
            .optional()?;
        payload
            .map(|payload| {
                let goal = serde_json::from_str::<Goal>(&payload)?;
                validate_goal(&goal)?;
                if goal.project_id != project_id || goal.session_id != session_id {
                    return Err(StoreError::InvalidRecord(
                        "stored goal belongs to a different session".into(),
                    ));
                }
                Ok(goal)
            })
            .transpose()
    }

    pub fn save_goal(&self, goal: &Goal) -> Result<(), StoreError> {
        validate_goal(goal)?;
        let mut connection = self
            .connection
            .lock()
            .map_err(|_| StoreError::LockPoisoned)?;
        let transaction = connection.transaction()?;
        let belongs_to_project = transaction.query_row(
            "SELECT EXISTS(
                SELECT 1 FROM sessions WHERE id = ?1 AND project_id = ?2
             )",
            params![goal.session_id.to_string(), goal.project_id.to_string()],
            |row| row.get::<_, bool>(0),
        )?;
        if !belongs_to_project {
            return Err(StoreError::InvalidRecord(
                "goal session does not belong to its project".into(),
            ));
        }
        transaction.execute(
            "INSERT INTO goals (id, project_id, session_id, payload, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT(session_id) DO UPDATE SET
                 id = excluded.id,
                 project_id = excluded.project_id,
                 payload = excluded.payload,
                 updated_at = excluded.updated_at",
            params![
                goal.id.to_string(),
                goal.project_id.to_string(),
                goal.session_id.to_string(),
                serde_json::to_string(goal)?,
                goal.updated_at.unix_timestamp(),
            ],
        )?;
        transaction.commit()?;
        Ok(())
    }

    pub fn clear_goal(
        &self,
        project_id: ProjectId,
        session_id: SessionId,
    ) -> Result<bool, StoreError> {
        let mut connection = self
            .connection
            .lock()
            .map_err(|_| StoreError::LockPoisoned)?;
        let transaction = connection.transaction()?;
        let belongs_to_project = transaction.query_row(
            "SELECT EXISTS(
                SELECT 1 FROM sessions WHERE id = ?1 AND project_id = ?2
             )",
            params![session_id.to_string(), project_id.to_string()],
            |row| row.get::<_, bool>(0),
        )?;
        if !belongs_to_project {
            return Err(StoreError::InvalidRecord(
                "goal session does not belong to the current project".into(),
            ));
        }
        let payload = transaction
            .query_row(
                "SELECT payload FROM goals
                 WHERE project_id = ?1 AND session_id = ?2",
                params![project_id.to_string(), session_id.to_string()],
                |row| row.get::<_, String>(0),
            )
            .optional()?;
        let Some(payload) = payload else {
            transaction.commit()?;
            return Ok(false);
        };
        let goal = serde_json::from_str::<Goal>(&payload)?;
        validate_goal(&goal)?;
        transaction.execute(
            "INSERT INTO goal_history (goal_id, project_id, session_id, payload, cleared_at)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![
                goal.id.to_string(),
                project_id.to_string(),
                session_id.to_string(),
                payload,
                now().unix_timestamp(),
            ],
        )?;
        transaction.execute(
            "DELETE FROM goals WHERE project_id = ?1 AND session_id = ?2",
            params![project_id.to_string(), session_id.to_string()],
        )?;
        transaction.commit()?;
        Ok(true)
    }

    pub fn append_conversation_message(
        &self,
        message: &ConversationMessage,
    ) -> Result<(), StoreError> {
        validate_conversation_message_record(message)?;
        let usage = message
            .usage
            .as_ref()
            .map(serde_json::to_string)
            .transpose()?;
        let connection = self
            .connection
            .lock()
            .map_err(|_| StoreError::LockPoisoned)?;
        let session_project = connection
            .query_row(
                "SELECT project_id FROM sessions WHERE id = ?1",
                [message.session_id.to_string()],
                |row| row.get::<_, String>(0),
            )
            .optional()?;
        if session_project.as_deref() != Some(&message.project_id.to_string()) {
            return Err(StoreError::InvalidRecord(
                "conversation message session does not belong to its project".into(),
            ));
        }
        connection.execute(
            "INSERT INTO conversation_messages (
                id, project_id, session_id, run_id, role, status, created_at, text,
                reasoning, usage, profile_id, model
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
            params![
                message.id.to_string(),
                message.project_id.to_string(),
                message.session_id.to_string(),
                message.run_id.to_string(),
                conversation_role_name(message.role),
                conversation_status_name(message.status),
                message.created_at.unix_timestamp(),
                message.text,
                message.reasoning,
                usage,
                message.profile_id,
                message.model,
            ],
        )?;
        Ok(())
    }

    pub fn conversation_snapshot(
        &self,
        project_id: ProjectId,
        session_id: SessionId,
    ) -> Result<ConversationSnapshot, StoreError> {
        let connection = self
            .connection
            .lock()
            .map_err(|_| StoreError::LockPoisoned)?;
        let belongs_to_project = connection.query_row(
            "SELECT EXISTS(
                SELECT 1 FROM sessions WHERE id = ?1 AND project_id = ?2
             )",
            params![session_id.to_string(), project_id.to_string()],
            |row| row.get::<_, bool>(0),
        )?;
        if !belongs_to_project {
            return Err(StoreError::InvalidRecord(
                "session does not belong to the current project".into(),
            ));
        }
        let mut statement = connection.prepare(
            "SELECT id, run_id, role, status, created_at, text,
                    reasoning, usage, profile_id, model
             FROM conversation_messages
             WHERE project_id = ?1 AND session_id = ?2
             ORDER BY created_at DESC, rowid DESC
             LIMIT ?3",
        )?;
        let rows = statement.query_map(
            params![
                project_id.to_string(),
                session_id.to_string(),
                MAX_CONVERSATION_SNAPSHOT_MESSAGES as i64
            ],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, i64>(4)?,
                    row.get::<_, String>(5)?,
                    row.get::<_, String>(6)?,
                    row.get::<_, Option<String>>(7)?,
                    row.get::<_, String>(8)?,
                    row.get::<_, String>(9)?,
                ))
            },
        )?;
        let mut messages = Vec::new();
        let mut snapshot_bytes = 0usize;
        for row in rows {
            let (id, run_id, role, status, created_at, text, reasoning, usage, profile_id, model) =
                row?;
            let next_bytes = snapshot_bytes
                .saturating_add(text.len())
                .saturating_add(reasoning.len());
            if next_bytes > MAX_CONVERSATION_SNAPSHOT_BYTES {
                break;
            }
            snapshot_bytes = next_bytes;
            let message = ConversationMessage {
                id: id.parse().map_err(|_| {
                    StoreError::InvalidRecord("conversation message id is invalid".into())
                })?,
                project_id,
                session_id,
                run_id: run_id.parse().map_err(|_| {
                    StoreError::InvalidRecord("conversation message run id is invalid".into())
                })?,
                role: parse_conversation_role(&role)?,
                text,
                reasoning,
                usage: parse_stored_usage(usage.as_deref())?,
                profile_id,
                model,
                status: parse_conversation_status(&status)?,
                created_at: time::OffsetDateTime::from_unix_timestamp(created_at).map_err(
                    |_| {
                        StoreError::InvalidRecord(
                            "conversation message timestamp is invalid".into(),
                        )
                    },
                )?,
            };
            validate_conversation_message_record(&message)?;
            messages.push(message);
        }
        messages.reverse();
        Ok(ConversationSnapshot {
            project_id,
            session_id,
            messages,
        })
    }

    pub fn title_default_session_from_prompt(
        &self,
        project_id: ProjectId,
        session_id: SessionId,
        prompt: &str,
    ) -> Result<Option<WorkspaceSnapshot>, StoreError> {
        let connection = self
            .connection
            .lock()
            .map_err(|_| StoreError::LockPoisoned)?;
        let payload = connection
            .query_row(
                "SELECT payload FROM sessions WHERE id = ?1 AND project_id = ?2",
                params![session_id.to_string(), project_id.to_string()],
                |row| row.get::<_, String>(0),
            )
            .optional()?
            .ok_or_else(|| {
                StoreError::InvalidRecord("session does not belong to the current project".into())
            })?;
        let mut session = serde_json::from_str::<Session>(&payload)?;
        if session.title != "New session" {
            return Ok(None);
        }
        session.title = session_title_from_prompt(prompt);
        session.updated_at = now();
        connection.execute(
            "UPDATE sessions
             SET updated_at = ?1, payload = ?2
             WHERE id = ?3 AND project_id = ?4",
            params![
                session.updated_at.unix_timestamp(),
                serde_json::to_string(&session)?,
                session.id.to_string(),
                project_id.to_string(),
            ],
        )?;
        drop(connection);
        Ok(Some(self.workspace_snapshot(project_id)?))
    }

    pub fn provider_profiles(
        &self,
        project_id: ProjectId,
    ) -> Result<Vec<ProviderProfile>, StoreError> {
        let connection = self
            .connection
            .lock()
            .map_err(|_| StoreError::LockPoisoned)?;
        load_provider_profiles(&connection, project_id)
    }

    pub fn selected_provider_profile(
        &self,
        project_id: ProjectId,
    ) -> Result<Option<ProviderProfile>, StoreError> {
        let profiles = self.provider_profiles(project_id)?;
        Ok(profiles.into_iter().find(|profile| profile.selected))
    }

    pub fn save_provider_profile(
        &self,
        project_id: ProjectId,
        profile: &ProviderProfile,
    ) -> Result<Vec<ProviderProfile>, StoreError> {
        validate_storable_provider_profile(profile)?;
        let mut connection = self
            .connection
            .lock()
            .map_err(|_| StoreError::LockPoisoned)?;
        let transaction = connection.transaction()?;
        ensure_project_exists(&transaction, project_id)?;
        let existing_selected = transaction
            .query_row(
                "SELECT selected FROM provider_profiles
                 WHERE project_id = ?1 AND profile_id = ?2",
                params![project_id.to_string(), profile.id],
                |row| row.get::<_, bool>(0),
            )
            .optional()?;
        let selected = profile.selected || existing_selected.unwrap_or(false);
        if selected {
            transaction.execute(
                "UPDATE provider_profiles SET selected = 0 WHERE project_id = ?1",
                [project_id.to_string()],
            )?;
        }
        transaction.execute(
            "INSERT INTO provider_profiles (
                project_id, profile_id, label, provider_kind, endpoint, model, selected
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
             ON CONFLICT(project_id, profile_id) DO UPDATE SET
                label = excluded.label,
                provider_kind = excluded.provider_kind,
                endpoint = excluded.endpoint,
                model = excluded.model,
                selected = excluded.selected",
            params![
                project_id.to_string(),
                profile.id,
                profile.label,
                provider_kind_name(profile.kind),
                profile.endpoint,
                profile.model,
                selected,
            ],
        )?;
        let profiles = load_provider_profiles(&transaction, project_id)?;
        transaction.commit()?;
        Ok(profiles)
    }

    pub fn select_provider_profile(
        &self,
        project_id: ProjectId,
        provider_id: &str,
    ) -> Result<Vec<ProviderProfile>, StoreError> {
        let mut connection = self
            .connection
            .lock()
            .map_err(|_| StoreError::LockPoisoned)?;
        let transaction = connection.transaction()?;
        let exists = transaction.query_row(
            "SELECT EXISTS(
                SELECT 1 FROM provider_profiles
                WHERE project_id = ?1 AND profile_id = ?2
             )",
            params![project_id.to_string(), provider_id],
            |row| row.get::<_, bool>(0),
        )?;
        if !exists {
            return Err(StoreError::InvalidRecord(
                "provider profile does not belong to the current project".into(),
            ));
        }
        transaction.execute(
            "UPDATE provider_profiles SET selected = 0 WHERE project_id = ?1",
            [project_id.to_string()],
        )?;
        transaction.execute(
            "UPDATE provider_profiles
             SET selected = 1
             WHERE project_id = ?1 AND profile_id = ?2",
            params![project_id.to_string(), provider_id],
        )?;
        let profiles = load_provider_profiles(&transaction, project_id)?;
        transaction.commit()?;
        Ok(profiles)
    }

    pub fn select_provider_model(
        &self,
        project_id: ProjectId,
        provider_id: &str,
        model: &str,
    ) -> Result<Vec<ProviderProfile>, StoreError> {
        let model = model.trim();
        if !valid_storable_provider_id(provider_id) || !valid_storable_provider_model(model) {
            return Err(StoreError::InvalidRecord(
                "provider profile or model is invalid".into(),
            ));
        }
        let mut connection = self
            .connection
            .lock()
            .map_err(|_| StoreError::LockPoisoned)?;
        let transaction = connection.transaction()?;
        ensure_project_exists(&transaction, project_id)?;
        let updated = transaction.execute(
            "UPDATE provider_profiles
             SET model = ?1
             WHERE project_id = ?2 AND profile_id = ?3",
            params![model, project_id.to_string(), provider_id],
        )?;
        if updated != 1 {
            return Err(StoreError::InvalidRecord(
                "provider profile does not belong to the current project".into(),
            ));
        }
        let profiles = load_provider_profiles(&transaction, project_id)?;
        transaction.commit()?;
        Ok(profiles)
    }

    pub fn save_layout(&self, key: &str, profile: &LayoutProfile) -> Result<(), StoreError> {
        let payload = serde_json::to_string(profile)?;
        let connection = self
            .connection
            .lock()
            .map_err(|_| StoreError::LockPoisoned)?;
        connection.execute(
            "INSERT INTO layout_profiles (profile_key, payload) VALUES (?1, ?2)
             ON CONFLICT(profile_key) DO UPDATE SET payload = excluded.payload",
            params![key, payload],
        )?;
        Ok(())
    }

    pub fn load_layout(&self, key: &str) -> Result<Option<LayoutProfile>, StoreError> {
        let connection = self
            .connection
            .lock()
            .map_err(|_| StoreError::LockPoisoned)?;
        let mut statement =
            connection.prepare("SELECT payload FROM layout_profiles WHERE profile_key = ?1")?;
        let mut rows = statement.query(params![key])?;
        let Some(row) = rows.next()? else {
            return Ok(None);
        };
        let payload: String = row.get(0)?;
        Ok(Some(serde_json::from_str(&payload)?))
    }

    pub fn save_execution_policy(
        &self,
        project_id: ProjectId,
        policy: &HarnessExecutionPolicy,
    ) -> Result<(), StoreError> {
        validate_execution_policy(policy)?;
        let payload = serde_json::to_string(policy)?;
        let connection = self
            .connection
            .lock()
            .map_err(|_| StoreError::LockPoisoned)?;
        connection.execute(
            "INSERT INTO execution_policies (project_id, payload) VALUES (?1, ?2)
             ON CONFLICT(project_id) DO UPDATE SET payload = excluded.payload",
            params![project_id.to_string(), payload],
        )?;
        Ok(())
    }

    pub fn load_execution_policy(
        &self,
        project_id: ProjectId,
    ) -> Result<Option<HarnessExecutionPolicy>, StoreError> {
        let connection = self
            .connection
            .lock()
            .map_err(|_| StoreError::LockPoisoned)?;
        let payload = connection
            .query_row(
                "SELECT payload FROM execution_policies WHERE project_id = ?1",
                [project_id.to_string()],
                |row| row.get::<_, String>(0),
            )
            .optional()?;
        let Some(payload) = payload else {
            return Ok(None);
        };
        if payload.len() > 64 * 1024 {
            return Err(StoreError::InvalidRecord(
                "stored execution policy exceeds the size limit".into(),
            ));
        }
        let policy = serde_json::from_str(&payload)?;
        validate_execution_policy(&policy)?;
        Ok(Some(policy))
    }

    fn migrate(&self) -> Result<(), StoreError> {
        let connection = self
            .connection
            .lock()
            .map_err(|_| StoreError::LockPoisoned)?;
        connection.execute_batch(
            "PRAGMA journal_mode = WAL;
             PRAGMA foreign_keys = ON;
             CREATE TABLE IF NOT EXISTS event_log (
                 id INTEGER PRIMARY KEY AUTOINCREMENT,
                 event_type TEXT NOT NULL,
                 created_at INTEGER NOT NULL,
                 payload TEXT NOT NULL,
                 payload_version INTEGER NOT NULL DEFAULT 1,
                 workspace_key TEXT,
                 project_id TEXT,
                 session_id TEXT,
                 run_id TEXT
             );
             CREATE INDEX IF NOT EXISTS idx_event_log_type ON event_log(event_type);
             CREATE TABLE IF NOT EXISTS layout_profiles (
                 profile_key TEXT PRIMARY KEY,
                 payload TEXT NOT NULL
             );
             CREATE TABLE IF NOT EXISTS projects (
                 id TEXT PRIMARY KEY,
                 workspace_key TEXT NOT NULL UNIQUE,
                 payload TEXT NOT NULL,
                 active_session_id TEXT
             );
             CREATE TABLE IF NOT EXISTS sessions (
                 id TEXT PRIMARY KEY,
                 project_id TEXT NOT NULL,
                 created_at INTEGER NOT NULL,
                 updated_at INTEGER NOT NULL,
                 payload TEXT NOT NULL,
                 FOREIGN KEY(project_id) REFERENCES projects(id) ON DELETE CASCADE
             );
             CREATE TABLE IF NOT EXISTS provider_profiles (
                 project_id TEXT NOT NULL,
                 profile_id TEXT NOT NULL,
                 label TEXT NOT NULL,
                 provider_kind TEXT NOT NULL,
                 endpoint TEXT NOT NULL,
                 model TEXT NOT NULL,
                 selected INTEGER NOT NULL DEFAULT 0 CHECK(selected IN (0, 1)),
                 PRIMARY KEY(project_id, profile_id),
                 FOREIGN KEY(project_id) REFERENCES projects(id) ON DELETE CASCADE
             );
             CREATE TABLE IF NOT EXISTS conversation_messages (
                 id TEXT PRIMARY KEY,
                 project_id TEXT NOT NULL,
                 session_id TEXT NOT NULL,
                 run_id TEXT NOT NULL,
                 role TEXT NOT NULL CHECK(role IN ('user', 'assistant')),
                 status TEXT NOT NULL CHECK(status IN ('complete', 'interrupted', 'failed')),
                 created_at INTEGER NOT NULL,
                 text TEXT NOT NULL,
                 reasoning TEXT NOT NULL DEFAULT '',
                 usage TEXT,
                 profile_id TEXT NOT NULL DEFAULT '',
                 model TEXT NOT NULL DEFAULT '',
                 FOREIGN KEY(project_id) REFERENCES projects(id) ON DELETE CASCADE,
                 FOREIGN KEY(session_id) REFERENCES sessions(id) ON DELETE CASCADE
             );
             CREATE TABLE IF NOT EXISTS goals (
                 id TEXT PRIMARY KEY,
                 project_id TEXT NOT NULL,
                 session_id TEXT NOT NULL UNIQUE,
                 payload TEXT NOT NULL,
                 updated_at INTEGER NOT NULL,
                 FOREIGN KEY(project_id) REFERENCES projects(id) ON DELETE CASCADE,
                 FOREIGN KEY(session_id) REFERENCES sessions(id) ON DELETE CASCADE
             );
             CREATE TABLE IF NOT EXISTS goal_history (
                 id INTEGER PRIMARY KEY AUTOINCREMENT,
                 goal_id TEXT NOT NULL,
                 project_id TEXT NOT NULL,
                 session_id TEXT NOT NULL,
                 payload TEXT NOT NULL,
                 cleared_at INTEGER NOT NULL,
                 FOREIGN KEY(project_id) REFERENCES projects(id) ON DELETE CASCADE,
                 FOREIGN KEY(session_id) REFERENCES sessions(id) ON DELETE CASCADE
             );
             CREATE TABLE IF NOT EXISTS execution_policies (
                 project_id TEXT PRIMARY KEY,
                 payload TEXT NOT NULL,
                 FOREIGN KEY(project_id) REFERENCES projects(id) ON DELETE CASCADE
             );",
        )?;
        ensure_column(
            &connection,
            "event_log",
            "payload_version",
            "INTEGER NOT NULL DEFAULT 1",
        )?;
        ensure_column(&connection, "event_log", "workspace_key", "TEXT")?;
        ensure_column(&connection, "event_log", "project_id", "TEXT")?;
        ensure_column(&connection, "event_log", "session_id", "TEXT")?;
        ensure_column(&connection, "event_log", "run_id", "TEXT")?;
        ensure_column(
            &connection,
            "conversation_messages",
            "reasoning",
            "TEXT NOT NULL DEFAULT ''",
        )?;
        ensure_column(&connection, "conversation_messages", "usage", "TEXT")?;
        ensure_column(
            &connection,
            "conversation_messages",
            "profile_id",
            "TEXT NOT NULL DEFAULT ''",
        )?;
        ensure_column(
            &connection,
            "conversation_messages",
            "model",
            "TEXT NOT NULL DEFAULT ''",
        )?;
        connection.execute_batch(
            "CREATE INDEX IF NOT EXISTS idx_event_log_workspace_id
                 ON event_log(workspace_key, id);
             CREATE UNIQUE INDEX IF NOT EXISTS idx_provider_profiles_one_selected
                 ON provider_profiles(project_id) WHERE selected = 1;
             CREATE INDEX IF NOT EXISTS idx_conversation_messages_session
                 ON conversation_messages(project_id, session_id, created_at);
             CREATE UNIQUE INDEX IF NOT EXISTS idx_conversation_messages_run_role
                 ON conversation_messages(run_id, role);
             CREATE INDEX IF NOT EXISTS idx_goals_project_session
                 ON goals(project_id, session_id);
             CREATE INDEX IF NOT EXISTS idx_goal_history_session
                 ON goal_history(project_id, session_id, cleared_at);
             PRAGMA user_version = 7;",
        )?;
        Ok(())
    }
}

fn canonical_workspace(path: &Path) -> Result<(PathBuf, String), StoreError> {
    let canonical = std::fs::canonicalize(path).map_err(|source| StoreError::WorkspacePath {
        path: path.to_path_buf(),
        source,
    })?;
    let displayed = canonical.to_string_lossy().replace('\\', "/");
    #[cfg(target_os = "windows")]
    let key = displayed.to_ascii_lowercase();
    #[cfg(not(target_os = "windows"))]
    let key = displayed;
    Ok((canonical, key))
}

fn workspace_name(path: &Path) -> String {
    path.file_name()
        .and_then(|name| name.to_str())
        .filter(|name| !name.trim().is_empty())
        .unwrap_or("Workspace")
        .to_owned()
}

fn new_session_record(project_id: ProjectId, title: impl Into<String>) -> Session {
    let timestamp = now();
    Session {
        id: SessionId::new_v4(),
        project_id,
        title: title.into(),
        created_at: timestamp,
        updated_at: timestamp,
    }
}

fn session_title_from_prompt(prompt: &str) -> String {
    const MAX_TITLE_CHARS: usize = 56;
    let first_line = prompt
        .lines()
        .find(|line| !line.trim().is_empty())
        .unwrap_or_default();
    let normalized = first_line.split_whitespace().collect::<Vec<_>>().join(" ");
    let mut title = normalized.chars().take(MAX_TITLE_CHARS).collect::<String>();
    if title.is_empty() {
        title = "New session".into();
    }
    title
}

fn insert_session(connection: &Connection, session: &Session) -> Result<(), StoreError> {
    connection.execute(
        "INSERT INTO sessions (id, project_id, created_at, updated_at, payload)
         VALUES (?1, ?2, ?3, ?4, ?5)",
        params![
            session.id.to_string(),
            session.project_id.to_string(),
            session.created_at.unix_timestamp(),
            session.updated_at.unix_timestamp(),
            serde_json::to_string(session)?,
        ],
    )?;
    Ok(())
}

fn ensure_project_exists(connection: &Connection, project_id: ProjectId) -> Result<(), StoreError> {
    let exists = connection.query_row(
        "SELECT EXISTS(SELECT 1 FROM projects WHERE id = ?1)",
        [project_id.to_string()],
        |row| row.get::<_, bool>(0),
    )?;
    if exists {
        Ok(())
    } else {
        Err(StoreError::InvalidRecord("project does not exist".into()))
    }
}

fn ensure_default_provider_profile(
    connection: &Connection,
    project_id: ProjectId,
    profile: &ProviderProfile,
) -> Result<(), StoreError> {
    validate_storable_provider_profile(profile)?;
    let profile_count = connection.query_row(
        "SELECT COUNT(*) FROM provider_profiles WHERE project_id = ?1",
        [project_id.to_string()],
        |row| row.get::<_, u64>(0),
    )?;
    if profile_count == 0 {
        connection.execute(
            "INSERT INTO provider_profiles (
                project_id, profile_id, label, provider_kind, endpoint, model, selected
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, 1)",
            params![
                project_id.to_string(),
                profile.id,
                profile.label,
                provider_kind_name(profile.kind),
                profile.endpoint,
                profile.model,
            ],
        )?;
    }
    Ok(())
}

fn load_provider_profiles(
    connection: &Connection,
    project_id: ProjectId,
) -> Result<Vec<ProviderProfile>, StoreError> {
    let mut statement = connection.prepare(
        "SELECT profile_id, label, provider_kind, endpoint, model, selected
         FROM provider_profiles
         WHERE project_id = ?1
         ORDER BY selected DESC, profile_id ASC",
    )?;
    let rows = statement.query_map([project_id.to_string()], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
            row.get::<_, String>(3)?,
            row.get::<_, String>(4)?,
            row.get::<_, bool>(5)?,
        ))
    })?;
    let mut profiles = Vec::new();
    for row in rows {
        let (id, label, kind, endpoint, model, selected) = row?;
        let profile = ProviderProfile {
            id,
            label,
            kind: parse_provider_kind(&kind)?,
            endpoint,
            model,
            selected,
        };
        validate_storable_provider_profile(&profile)?;
        profiles.push(profile);
    }
    Ok(profiles)
}

fn provider_kind_name(kind: ProviderKind) -> &'static str {
    match kind {
        ProviderKind::OpenAiCompatible => "openai_compatible",
        ProviderKind::Anthropic => "anthropic",
        ProviderKind::LocalLmStudio => "local_lm_studio",
        ProviderKind::Unknown => "unknown",
    }
}

fn parse_provider_kind(kind: &str) -> Result<ProviderKind, StoreError> {
    match kind {
        "openai_compatible" => Ok(ProviderKind::OpenAiCompatible),
        "anthropic" => Ok(ProviderKind::Anthropic),
        "local_lm_studio" => Ok(ProviderKind::LocalLmStudio),
        "unknown" => Ok(ProviderKind::Unknown),
        _ => Err(StoreError::InvalidRecord(
            "provider profile kind is invalid".into(),
        )),
    }
}

fn conversation_role_name(role: ConversationRole) -> &'static str {
    match role {
        ConversationRole::User => "user",
        ConversationRole::Assistant => "assistant",
    }
}

fn parse_conversation_role(role: &str) -> Result<ConversationRole, StoreError> {
    match role {
        "user" => Ok(ConversationRole::User),
        "assistant" => Ok(ConversationRole::Assistant),
        _ => Err(StoreError::InvalidRecord(
            "conversation message role is invalid".into(),
        )),
    }
}

fn conversation_status_name(status: ConversationMessageStatus) -> &'static str {
    match status {
        ConversationMessageStatus::Complete => "complete",
        ConversationMessageStatus::Interrupted => "interrupted",
        ConversationMessageStatus::Failed => "failed",
    }
}

fn parse_conversation_status(status: &str) -> Result<ConversationMessageStatus, StoreError> {
    match status {
        "complete" => Ok(ConversationMessageStatus::Complete),
        "interrupted" => Ok(ConversationMessageStatus::Interrupted),
        "failed" => Ok(ConversationMessageStatus::Failed),
        _ => Err(StoreError::InvalidRecord(
            "conversation message status is invalid".into(),
        )),
    }
}

fn parse_stored_usage(raw: Option<&str>) -> Result<Option<ModelUsage>, StoreError> {
    raw.map(|value| {
        serde_json::from_str(value)
            .map_err(|_| StoreError::InvalidRecord("conversation message usage is invalid".into()))
    })
    .transpose()
}

fn validate_conversation_message_record(message: &ConversationMessage) -> Result<(), StoreError> {
    if message.text.len().saturating_add(message.reasoning.len()) > MAX_CONVERSATION_MESSAGE_BYTES {
        return Err(StoreError::InvalidRecord(
            "conversation message exceeded the byte limit".into(),
        ));
    }

    if message.role == ConversationRole::User {
        if !message.reasoning.is_empty()
            || message.usage.is_some()
            || !message.profile_id.is_empty()
            || !message.model.is_empty()
        {
            return Err(StoreError::InvalidRecord(
                "user conversation message contains assistant metadata".into(),
            ));
        }
        return Ok(());
    }

    if let Some(usage) = &message.usage {
        validate_model_usage(usage)?;
    }

    let has_profile = !message.profile_id.is_empty();
    let has_model = !message.model.is_empty();
    if has_profile != has_model {
        return Err(StoreError::InvalidRecord(
            "assistant conversation message identity is incomplete".into(),
        ));
    }
    if has_profile
        && (!valid_storable_provider_id(&message.profile_id)
            || !valid_storable_provider_model(&message.model))
    {
        return Err(StoreError::InvalidRecord(
            "assistant conversation message identity is invalid".into(),
        ));
    }
    if !has_profile && (!message.reasoning.is_empty() || message.usage.is_some()) {
        return Err(StoreError::InvalidRecord(
            "assistant conversation message metadata requires provider identity".into(),
        ));
    }
    Ok(())
}

fn validate_model_usage(usage: &ModelUsage) -> Result<(), StoreError> {
    let valid_total = usage
        .input_tokens
        .checked_add(usage.output_tokens)
        .is_some_and(|total| total == usage.total_tokens);
    let valid_cached = usage
        .cached_input_tokens
        .map_or(true, |tokens| tokens <= usage.input_tokens);
    let valid_reasoning = usage
        .reasoning_tokens
        .map_or(true, |tokens| tokens <= usage.output_tokens);
    let valid_context = usage
        .context_window_tokens
        .map_or(true, |tokens| tokens > 0 && usage.total_tokens <= tokens);
    if valid_total && valid_cached && valid_reasoning && valid_context {
        Ok(())
    } else {
        Err(StoreError::InvalidRecord(
            "conversation message usage is invalid".into(),
        ))
    }
}

fn validate_goal(goal: &Goal) -> Result<(), StoreError> {
    let objective = goal.objective.trim();
    if objective.is_empty() {
        return Err(StoreError::InvalidRecord(
            "goal objective must not be empty".into(),
        ));
    }
    if objective.len() > MAX_GOAL_OBJECTIVE_BYTES {
        return Err(StoreError::InvalidRecord(
            "goal objective exceeds the byte limit".into(),
        ));
    }
    if goal
        .objective
        .chars()
        .any(|character| character.is_control() && !matches!(character, '\n' | '\r' | '\t'))
    {
        return Err(StoreError::InvalidRecord(
            "goal objective contains unsupported control characters".into(),
        ));
    }
    if goal.next_action.len() > MAX_GOAL_NEXT_ACTION_BYTES {
        return Err(StoreError::InvalidRecord(
            "goal next action exceeds the byte limit".into(),
        ));
    }
    if goal
        .next_action
        .chars()
        .any(|character| character.is_control() && !matches!(character, '\n' | '\r' | '\t'))
    {
        return Err(StoreError::InvalidRecord(
            "goal next action contains unsupported control characters".into(),
        ));
    }
    if goal
        .token_budget
        .is_some_and(|budget| budget == 0 || budget > MAX_GOAL_TOKEN_BUDGET)
    {
        return Err(StoreError::InvalidRecord(
            "goal token budget is outside the supported range".into(),
        ));
    }
    if goal
        .tool_budget
        .is_some_and(|budget| budget == 0 || budget > MAX_GOAL_TOOL_BUDGET)
    {
        return Err(StoreError::InvalidRecord(
            "goal tool budget is outside the supported range".into(),
        ));
    }
    if goal
        .time_budget_seconds
        .is_some_and(|budget| budget == 0 || budget > MAX_GOAL_TIME_BUDGET_SECONDS)
    {
        return Err(StoreError::InvalidRecord(
            "goal time budget is outside the supported range".into(),
        ));
    }
    if goal
        .token_budget
        .is_some_and(|budget| goal.tokens_used > budget)
        || goal
            .tool_budget
            .is_some_and(|budget| goal.tools_used > budget)
    {
        return Err(StoreError::InvalidRecord(
            "goal usage exceeds its configured budget".into(),
        ));
    }
    if goal.verification_history.len() > MAX_GOAL_VERIFICATION_HISTORY {
        return Err(StoreError::InvalidRecord(
            "goal verification history exceeds the item limit".into(),
        ));
    }
    if goal.verification_history.iter().any(|verification| {
        verification.summary.len() > MAX_GOAL_NEXT_ACTION_BYTES
            || verification
                .summary
                .chars()
                .any(|character| character.is_control() && !matches!(character, '\n' | '\r' | '\t'))
    }) {
        return Err(StoreError::InvalidRecord(
            "goal verification summary exceeds the byte limit".into(),
        ));
    }
    Ok(())
}

fn validate_execution_policy(policy: &HarnessExecutionPolicy) -> Result<(), StoreError> {
    if !valid_harness_id(&policy.profile_id) || policy.capability_enabled.len() > 128 {
        return Err(StoreError::InvalidRecord(
            "execution policy contains invalid bounded fields".into(),
        ));
    }
    if policy
        .capability_enabled
        .keys()
        .any(|capability_id| !valid_harness_id(capability_id))
    {
        return Err(StoreError::InvalidRecord(
            "execution policy contains invalid bounded fields".into(),
        ));
    }
    Ok(())
}

fn valid_harness_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 64
        && value.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'.' | b'-')
        })
}

fn validate_storable_provider_profile(profile: &ProviderProfile) -> Result<(), StoreError> {
    let valid_id = valid_storable_provider_id(&profile.id);
    let valid_label = !profile.label.trim().is_empty()
        && profile.label.chars().count() <= 80
        && !profile.label.chars().any(char::is_control);
    let valid_model = valid_storable_provider_model(&profile.model);
    let valid_kind = profile.kind != ProviderKind::Unknown;
    let valid_endpoint = if profile.endpoint.len() > 2_048 {
        false
    } else if let Ok(endpoint) = Url::parse(&profile.endpoint) {
        matches!(endpoint.scheme(), "http" | "https")
            && endpoint.host_str().is_some()
            && endpoint.username().is_empty()
            && endpoint.password().is_none()
            && endpoint.query().is_none()
            && endpoint.fragment().is_none()
    } else {
        false
    };
    if valid_id && valid_label && valid_model && valid_kind && valid_endpoint {
        Ok(())
    } else {
        Err(StoreError::InvalidRecord(
            "provider profile contains invalid or secret-bearing fields".into(),
        ))
    }
}

fn valid_storable_provider_id(provider_id: &str) -> bool {
    !provider_id.is_empty()
        && provider_id.len() <= 64
        && provider_id.chars().all(|character| {
            character.is_ascii_alphanumeric() || matches!(character, '-' | '_' | '.')
        })
}

fn valid_storable_provider_model(model: &str) -> bool {
    !model.trim().is_empty() && model.chars().count() <= 256 && !model.chars().any(char::is_control)
}

fn append_event_on(
    connection: &Connection,
    workspace_key: Option<&str>,
    project_id: Option<ProjectId>,
    session_id: Option<SessionId>,
    run_id: Option<RunId>,
    event: &AppEvent,
) -> Result<(), StoreError> {
    let payload = serde_json::to_string(event)?;
    connection.execute(
        "INSERT INTO event_log (
            event_type, created_at, payload, payload_version,
            workspace_key, project_id, session_id, run_id
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        params![
            event.kind(),
            now().unix_timestamp(),
            payload,
            EVENT_PAYLOAD_VERSION,
            workspace_key,
            project_id.map(|value| value.to_string()),
            session_id.map(|value| value.to_string()),
            run_id.map(|value| value.to_string()),
        ],
    )?;
    Ok(())
}

fn load_sessions(
    connection: &Connection,
    project_id: ProjectId,
) -> Result<Vec<SessionSummary>, StoreError> {
    let mut statement = connection.prepare(
        "SELECT payload FROM sessions
         WHERE project_id = ?1
         ORDER BY created_at ASC, id ASC",
    )?;
    let rows = statement.query_map([project_id.to_string()], |row| row.get::<_, String>(0))?;
    let mut sessions = Vec::new();
    for row in rows {
        let session = serde_json::from_str::<Session>(&row?)?;
        sessions.push(SessionSummary::from(&session));
    }
    Ok(sessions)
}

fn load_active_session_id(
    connection: &Connection,
    project_id: ProjectId,
) -> Result<Option<SessionId>, StoreError> {
    let stored = connection
        .query_row(
            "SELECT active_session_id FROM projects WHERE id = ?1",
            [project_id.to_string()],
            |row| row.get::<_, Option<String>>(0),
        )
        .optional()?
        .flatten();
    stored
        .map(|value| {
            value
                .parse()
                .map_err(|_| StoreError::InvalidRecord("active session id is invalid".into()))
        })
        .transpose()
}

fn ensure_column(
    connection: &Connection,
    table: &str,
    column: &str,
    declaration: &str,
) -> Result<(), StoreError> {
    let mut statement = connection.prepare(&format!("PRAGMA table_info({table})"))?;
    let columns = statement.query_map([], |row| row.get::<_, String>(1))?;
    for existing in columns {
        if existing? == column {
            return Ok(());
        }
    }
    connection.execute_batch(&format!(
        "ALTER TABLE {table} ADD COLUMN {column} {declaration};"
    ))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use argentum_domain::{AppEvent, LayoutProfile};

    use super::*;

    fn conversation_store() -> (Store, tempfile::TempDir, ProjectId, SessionId) {
        let directory = tempfile::tempdir().expect("directory");
        let workspace = directory.path().join("workspace");
        std::fs::create_dir(&workspace).expect("workspace");
        let store = Store::open_in_memory().expect("store");
        let resolution = store.resolve_workspace(&workspace).expect("resolution");
        let project_id = resolution.snapshot.project.id;
        let session_id = resolution.snapshot.active_session_id.expect("session");
        (store, directory, project_id, session_id)
    }

    fn assistant_message(project_id: ProjectId, session_id: SessionId) -> ConversationMessage {
        ConversationMessage {
            id: RunId::new_v4(),
            project_id,
            session_id,
            run_id: RunId::new_v4(),
            role: ConversationRole::Assistant,
            text: "answer".into(),
            reasoning: "reasoning".into(),
            usage: Some(ModelUsage {
                input_tokens: 10,
                output_tokens: 5,
                total_tokens: 15,
                reasoning_tokens: Some(2),
                cached_input_tokens: Some(3),
                context_window_tokens: Some(32_768),
            }),
            profile_id: "hosted".into(),
            model: "hosted-model".into(),
            status: ConversationMessageStatus::Complete,
            created_at: now(),
        }
    }

    #[test]
    fn persists_events_and_layouts() {
        let store = Store::open_in_memory().expect("store");
        store
            .append_event(&AppEvent::Error {
                message: "test failure".into(),
                recoverable: true,
            })
            .expect("event");
        store
            .save_layout("default", &LayoutProfile::default())
            .expect("layout");

        assert_eq!(store.events().expect("events").len(), 1);
        assert!(store.load_layout("default").expect("layout read").is_some());
        let connection = store.connection.lock().expect("connection");
        let busy_timeout = connection
            .query_row("PRAGMA busy_timeout", [], |row| row.get::<_, u64>(0))
            .expect("busy timeout");
        assert_eq!(busy_timeout, DATABASE_BUSY_TIMEOUT.as_millis() as u64);
    }

    #[test]
    fn resolves_stable_project_and_active_session_after_reopen() {
        let directory = tempfile::tempdir().expect("directory");
        let workspace = directory.path().join("workspace");
        std::fs::create_dir(&workspace).expect("workspace");
        let database = directory.path().join("argentum.db");
        let store = Store::open(&database).expect("store");
        let first = store
            .resolve_workspace(&workspace)
            .expect("first resolution");
        let created = store
            .create_session(first.snapshot.project.id, "Second session")
            .expect("session");
        let project_id = first.snapshot.project.id;
        drop(store);

        let reopened = Store::open(&database).expect("reopened store");
        let second = reopened
            .resolve_workspace(&workspace)
            .expect("second resolution");

        assert_eq!(second.snapshot.project.id, project_id);
        assert_eq!(second.snapshot.active_session_id, Some(created.id));
        assert_eq!(second.snapshot.sessions.len(), 2);
    }

    #[test]
    fn workspace_event_queries_do_not_cross_workspace_boundaries() {
        let directory = tempfile::tempdir().expect("directory");
        let left_root = directory.path().join("left");
        let right_root = directory.path().join("right");
        std::fs::create_dir(&left_root).expect("left workspace");
        std::fs::create_dir(&right_root).expect("right workspace");
        let store = Store::open_in_memory().expect("store");
        let left = store
            .resolve_workspace(&left_root)
            .expect("left resolution");
        let right = store
            .resolve_workspace(&right_root)
            .expect("right resolution");
        let event = AppEvent::Error {
            message: "scoped".into(),
            recoverable: true,
        };
        store
            .append_event_scoped(
                &EventScope {
                    workspace_key: left.workspace_key.clone(),
                    project_id: left.snapshot.project.id,
                    session_id: left.snapshot.active_session_id,
                    run_id: None,
                },
                &event,
            )
            .expect("left event");

        assert_eq!(
            store.events_for_workspace(&left_root).expect("left events"),
            vec![event]
        );
        assert!(store
            .events_for_workspace(&right_root)
            .expect("right events")
            .is_empty());
        assert_ne!(left.snapshot.project.id, right.snapshot.project.id);
    }

    #[test]
    fn migrates_an_existing_event_log_without_losing_events() {
        let directory = tempfile::tempdir().expect("directory");
        let database = directory.path().join("legacy.db");
        let connection = Connection::open(&database).expect("legacy database");
        connection
            .execute_batch(
                "CREATE TABLE event_log (
                    id INTEGER PRIMARY KEY AUTOINCREMENT,
                    event_type TEXT NOT NULL,
                    created_at INTEGER NOT NULL,
                    payload TEXT NOT NULL
                 );
                 INSERT INTO event_log (event_type, created_at, payload)
                 VALUES ('error', 0, '{\"Error\":{\"message\":\"legacy\",\"recoverable\":true}}');",
            )
            .expect("legacy schema");
        drop(connection);

        let store = Store::open(&database).expect("migrated store");
        assert_eq!(store.events().expect("legacy events").len(), 1);
        let connection = store.connection.lock().expect("connection");
        let payload_version = connection
            .query_row(
                "SELECT payload_version FROM event_log WHERE id = 1",
                [],
                |row| row.get::<_, u16>(0),
            )
            .expect("payload version");
        let schema_version = connection
            .query_row("PRAGMA user_version", [], |row| row.get::<_, u16>(0))
            .expect("schema version");
        assert_eq!(payload_version, EVENT_PAYLOAD_VERSION);
        assert_eq!(schema_version, 7);
    }

    #[test]
    fn conversation_messages_survive_restart_in_stable_order() {
        let directory = tempfile::tempdir().expect("directory");
        let workspace = directory.path().join("workspace");
        std::fs::create_dir(&workspace).expect("workspace");
        let database = directory.path().join("argentum.db");
        let store = Store::open(&database).expect("store");
        let resolution = store.resolve_workspace(&workspace).expect("resolution");
        let project_id = resolution.snapshot.project.id;
        let session_id = resolution
            .snapshot
            .active_session_id
            .expect("active session");
        let run_id = RunId::new_v4();
        for (role, text) in [
            (ConversationRole::User, "First turn"),
            (ConversationRole::Assistant, "First answer"),
        ] {
            store
                .append_conversation_message(&ConversationMessage {
                    id: RunId::new_v4(),
                    project_id,
                    session_id,
                    run_id,
                    role,
                    text: text.into(),
                    reasoning: if role == ConversationRole::Assistant {
                        "First reasoning".into()
                    } else {
                        String::new()
                    },
                    usage: (role == ConversationRole::Assistant).then_some(
                        argentum_domain::ModelUsage {
                            input_tokens: 10,
                            output_tokens: 5,
                            total_tokens: 15,
                            reasoning_tokens: Some(2),
                            cached_input_tokens: None,
                            context_window_tokens: Some(32_768),
                        },
                    ),
                    profile_id: if role == ConversationRole::Assistant {
                        "hosted".into()
                    } else {
                        String::new()
                    },
                    model: if role == ConversationRole::Assistant {
                        "hosted-model".into()
                    } else {
                        String::new()
                    },
                    status: ConversationMessageStatus::Complete,
                    created_at: now(),
                })
                .expect("message");
        }
        drop(store);

        let reopened = Store::open(&database).expect("reopened store");
        let snapshot = reopened
            .conversation_snapshot(project_id, session_id)
            .expect("conversation");

        assert_eq!(snapshot.project_id, project_id);
        assert_eq!(snapshot.session_id, session_id);
        assert_eq!(snapshot.messages.len(), 2);
        assert_eq!(snapshot.messages[0].text, "First turn");
        assert_eq!(snapshot.messages[1].text, "First answer");
        assert_eq!(snapshot.messages[1].reasoning, "First reasoning");
        assert_eq!(snapshot.messages[1].profile_id, "hosted");
        assert_eq!(snapshot.messages[1].model, "hosted-model");
        assert_eq!(
            snapshot.messages[1]
                .usage
                .as_ref()
                .and_then(|usage| usage.context_window_tokens),
            Some(32_768)
        );
    }

    #[test]
    fn conversation_snapshots_keep_the_latest_bounded_messages_in_order() {
        let directory = tempfile::tempdir().expect("directory");
        let workspace = directory.path().join("workspace");
        std::fs::create_dir(&workspace).expect("workspace");
        let store = Store::open_in_memory().expect("store");
        let resolution = store.resolve_workspace(&workspace).expect("resolution");
        let project_id = resolution.snapshot.project.id;
        let session_id = resolution.snapshot.active_session_id.expect("session");

        for index in 0..(MAX_CONVERSATION_SNAPSHOT_MESSAGES + 5) {
            store
                .append_conversation_message(&ConversationMessage {
                    id: RunId::new_v4(),
                    project_id,
                    session_id,
                    run_id: RunId::new_v4(),
                    role: ConversationRole::User,
                    text: format!("turn-{index:03}"),
                    reasoning: String::new(),
                    usage: None,
                    profile_id: String::new(),
                    model: String::new(),
                    status: ConversationMessageStatus::Complete,
                    created_at: now(),
                })
                .expect("message");
        }

        let snapshot = store
            .conversation_snapshot(project_id, session_id)
            .expect("conversation");
        assert_eq!(snapshot.messages.len(), MAX_CONVERSATION_SNAPSHOT_MESSAGES);
        assert_eq!(snapshot.messages.first().expect("first").text, "turn-005");
        assert_eq!(snapshot.messages.last().expect("last").text, "turn-204");
    }

    #[test]
    fn conversation_message_and_snapshot_byte_limits_are_enforced() {
        let directory = tempfile::tempdir().expect("directory");
        let workspace = directory.path().join("workspace");
        std::fs::create_dir(&workspace).expect("workspace");
        let store = Store::open_in_memory().expect("store");
        let resolution = store.resolve_workspace(&workspace).expect("resolution");
        let project_id = resolution.snapshot.project.id;
        let session_id = resolution.snapshot.active_session_id.expect("session");
        let run_id = RunId::new_v4();

        let oversized = ConversationMessage {
            id: RunId::new_v4(),
            project_id,
            session_id,
            run_id,
            role: ConversationRole::User,
            text: "x".repeat(MAX_CONVERSATION_MESSAGE_BYTES + 1),
            reasoning: String::new(),
            usage: None,
            profile_id: String::new(),
            model: String::new(),
            status: ConversationMessageStatus::Complete,
            created_at: now(),
        };
        assert!(matches!(
            store.append_conversation_message(&oversized),
            Err(StoreError::InvalidRecord(message)) if message.contains("byte limit")
        ));

        let combined_oversized = ConversationMessage {
            role: ConversationRole::Assistant,
            text: "x".repeat(MAX_CONVERSATION_MESSAGE_BYTES),
            reasoning: "y".into(),
            profile_id: "hosted".into(),
            model: "hosted-model".into(),
            ..oversized.clone()
        };
        assert!(matches!(
            store.append_conversation_message(&combined_oversized),
            Err(StoreError::InvalidRecord(message)) if message.contains("byte limit")
        ));

        for index in 0..5 {
            store
                .append_conversation_message(&ConversationMessage {
                    id: RunId::new_v4(),
                    project_id,
                    session_id,
                    run_id: RunId::new_v4(),
                    role: ConversationRole::Assistant,
                    text: char::from(b'a' + index as u8)
                        .to_string()
                        .repeat(MAX_CONVERSATION_MESSAGE_BYTES),
                    reasoning: String::new(),
                    usage: None,
                    profile_id: String::new(),
                    model: String::new(),
                    status: ConversationMessageStatus::Complete,
                    created_at: now(),
                })
                .expect("bounded message");
        }

        let snapshot = store
            .conversation_snapshot(project_id, session_id)
            .expect("conversation");
        assert_eq!(snapshot.messages.len(), 4);
        assert_eq!(
            snapshot
                .messages
                .iter()
                .map(|message| message.text.len())
                .sum::<usize>(),
            MAX_CONVERSATION_SNAPSHOT_BYTES
        );
        assert!(snapshot.messages[0].text.starts_with('b'));
        assert!(snapshot.messages[3].text.starts_with('e'));
    }

    #[test]
    fn conversation_message_usage_invariants_are_enforced_before_append() {
        let (store, _directory, project_id, session_id) = conversation_store();
        let base = assistant_message(project_id, session_id);
        let invalid_usage = [
            ModelUsage {
                total_tokens: 14,
                ..base.usage.clone().expect("usage")
            },
            ModelUsage {
                input_tokens: u64::MAX,
                output_tokens: 1,
                total_tokens: u64::MAX,
                reasoning_tokens: None,
                cached_input_tokens: None,
                context_window_tokens: None,
            },
            ModelUsage {
                cached_input_tokens: Some(11),
                ..base.usage.clone().expect("usage")
            },
            ModelUsage {
                reasoning_tokens: Some(6),
                ..base.usage.clone().expect("usage")
            },
            ModelUsage {
                context_window_tokens: Some(0),
                ..base.usage.clone().expect("usage")
            },
            ModelUsage {
                context_window_tokens: Some(14),
                ..base.usage.clone().expect("usage")
            },
        ];

        for usage in invalid_usage {
            let mut message = base.clone();
            message.id = RunId::new_v4();
            message.run_id = RunId::new_v4();
            message.usage = Some(usage);
            assert!(matches!(
                store.append_conversation_message(&message),
                Err(StoreError::InvalidRecord(error))
                    if error == "conversation message usage is invalid"
            ));
        }
    }

    #[test]
    fn user_messages_reject_all_assistant_metadata() {
        let (store, _directory, project_id, session_id) = conversation_store();
        let base = ConversationMessage {
            id: RunId::new_v4(),
            project_id,
            session_id,
            run_id: RunId::new_v4(),
            role: ConversationRole::User,
            text: "question".into(),
            reasoning: String::new(),
            usage: None,
            profile_id: String::new(),
            model: String::new(),
            status: ConversationMessageStatus::Complete,
            created_at: now(),
        };
        let mut invalid = Vec::new();
        invalid.push(ConversationMessage {
            reasoning: "hidden".into(),
            ..base.clone()
        });
        invalid.push(ConversationMessage {
            usage: Some(ModelUsage {
                input_tokens: 1,
                output_tokens: 1,
                total_tokens: 2,
                reasoning_tokens: None,
                cached_input_tokens: None,
                context_window_tokens: None,
            }),
            ..base.clone()
        });
        invalid.push(ConversationMessage {
            profile_id: "hosted".into(),
            ..base.clone()
        });
        invalid.push(ConversationMessage {
            model: "hosted-model".into(),
            ..base
        });

        for message in invalid {
            assert!(matches!(
                store.append_conversation_message(&message),
                Err(StoreError::InvalidRecord(error))
                    if error == "user conversation message contains assistant metadata"
            ));
        }
    }

    #[test]
    fn malformed_or_invalid_stored_usage_is_rejected_without_serde_details() {
        let (store, _directory, project_id, session_id) = conversation_store();
        let message = assistant_message(project_id, session_id);
        store
            .append_conversation_message(&message)
            .expect("assistant message");

        {
            let connection = store.connection.lock().expect("connection");
            connection
                .execute(
                    "UPDATE conversation_messages SET usage = ?1 WHERE id = ?2",
                    params!["{not-json}", message.id.to_string()],
                )
                .expect("corrupt usage");
        }
        assert!(matches!(
            store.conversation_snapshot(project_id, session_id),
            Err(StoreError::InvalidRecord(error))
                if error == "conversation message usage is invalid"
        ));

        {
            let connection = store.connection.lock().expect("connection");
            connection
                .execute(
                    "UPDATE conversation_messages SET usage = ?1 WHERE id = ?2",
                    params![
                        r#"{"input_tokens":10,"output_tokens":5,"total_tokens":14}"#,
                        message.id.to_string()
                    ],
                )
                .expect("invalid usage");
        }
        assert!(matches!(
            store.conversation_snapshot(project_id, session_id),
            Err(StoreError::InvalidRecord(error))
                if error == "conversation message usage is invalid"
        ));
    }

    #[test]
    fn stored_user_messages_with_assistant_metadata_are_rejected() {
        let (store, _directory, project_id, session_id) = conversation_store();
        let message = ConversationMessage {
            id: RunId::new_v4(),
            project_id,
            session_id,
            run_id: RunId::new_v4(),
            role: ConversationRole::User,
            text: "question".into(),
            reasoning: String::new(),
            usage: None,
            profile_id: String::new(),
            model: String::new(),
            status: ConversationMessageStatus::Complete,
            created_at: now(),
        };
        store
            .append_conversation_message(&message)
            .expect("user message");
        {
            let connection = store.connection.lock().expect("connection");
            connection
                .execute(
                    "UPDATE conversation_messages SET reasoning = ?1 WHERE id = ?2",
                    params!["unexpected", message.id.to_string()],
                )
                .expect("corrupt metadata");
        }

        assert!(matches!(
            store.conversation_snapshot(project_id, session_id),
            Err(StoreError::InvalidRecord(error))
                if error == "user conversation message contains assistant metadata"
        ));
    }

    #[test]
    fn conversation_messages_are_isolated_by_project_and_session() {
        let directory = tempfile::tempdir().expect("directory");
        let left_root = directory.path().join("left");
        let right_root = directory.path().join("right");
        std::fs::create_dir(&left_root).expect("left");
        std::fs::create_dir(&right_root).expect("right");
        let store = Store::open_in_memory().expect("store");
        let left = store.resolve_workspace(&left_root).expect("left workspace");
        let right = store
            .resolve_workspace(&right_root)
            .expect("right workspace");
        let left_project = left.snapshot.project.id;
        let left_session = left.snapshot.active_session_id.expect("left session");
        store
            .append_conversation_message(&ConversationMessage {
                id: RunId::new_v4(),
                project_id: left_project,
                session_id: left_session,
                run_id: RunId::new_v4(),
                role: ConversationRole::User,
                text: "left only".into(),
                reasoning: String::new(),
                usage: None,
                profile_id: String::new(),
                model: String::new(),
                status: ConversationMessageStatus::Complete,
                created_at: now(),
            })
            .expect("left message");

        assert_eq!(
            store
                .conversation_snapshot(left_project, left_session)
                .expect("left conversation")
                .messages
                .len(),
            1
        );
        assert!(store
            .conversation_snapshot(
                right.snapshot.project.id,
                right.snapshot.active_session_id.expect("right session"),
            )
            .expect("right conversation")
            .messages
            .is_empty());
        assert!(matches!(
            store.conversation_snapshot(right.snapshot.project.id, left_session),
            Err(StoreError::InvalidRecord(message)) if message.contains("does not belong")
        ));
    }

    #[test]
    fn provider_profiles_persist_selection_across_restart() {
        let directory = tempfile::tempdir().expect("directory");
        let workspace = directory.path().join("workspace");
        std::fs::create_dir(&workspace).expect("workspace");
        let database = directory.path().join("argentum.db");
        let store = Store::open(&database).expect("store");
        let project_id = store
            .resolve_workspace(&workspace)
            .expect("workspace")
            .snapshot
            .project
            .id;
        let custom = ProviderProfile {
            id: "local-secondary".into(),
            label: "Secondary local".into(),
            kind: ProviderKind::LocalLmStudio,
            endpoint: "http://127.0.0.1:5678/v1/".into(),
            model: "secondary-model".into(),
            selected: true,
        };
        store
            .save_provider_profile(project_id, &custom)
            .expect("saved profile");
        drop(store);

        let reopened = Store::open(&database).expect("reopened store");
        let resolution = reopened
            .resolve_workspace(&workspace)
            .expect("reopened workspace");
        let profiles = reopened
            .provider_profiles(resolution.snapshot.project.id)
            .expect("profiles");

        assert_eq!(profiles.len(), 2);
        assert_eq!(
            profiles
                .iter()
                .find(|profile| profile.selected)
                .expect("selected profile"),
            &custom
        );
    }

    #[test]
    fn execution_policy_persists_and_is_isolated_by_project() {
        let directory = tempfile::tempdir().expect("directory");
        let left_root = directory.path().join("left");
        let right_root = directory.path().join("right");
        std::fs::create_dir(&left_root).expect("left workspace");
        std::fs::create_dir(&right_root).expect("right workspace");
        let database = directory.path().join("argentum.db");
        let store = Store::open(&database).expect("store");
        let left = store.resolve_workspace(&left_root).expect("left");
        let right = store.resolve_workspace(&right_root).expect("right");
        let policy = HarnessExecutionPolicy {
            profile_id: "read-only".into(),
            capability_enabled: BTreeMap::from([
                ("tool.read-text".into(), true),
                ("tool.write-text".into(), false),
            ]),
        };
        store
            .save_execution_policy(left.snapshot.project.id, &policy)
            .expect("saved policy");
        assert!(store
            .load_execution_policy(right.snapshot.project.id)
            .expect("right policy")
            .is_none());
        drop(store);

        let reopened = Store::open(&database).expect("reopened store");
        assert_eq!(
            reopened
                .load_execution_policy(left.snapshot.project.id)
                .expect("left policy"),
            Some(policy)
        );
        assert!(reopened
            .load_execution_policy(right.snapshot.project.id)
            .expect("right policy")
            .is_none());
    }

    #[test]
    fn execution_policy_rejects_unbounded_or_invalid_ids() {
        let workspace = tempfile::tempdir().expect("workspace");
        let store = Store::open_in_memory().expect("store");
        let project_id = store
            .resolve_workspace(workspace.path())
            .expect("workspace")
            .snapshot
            .project
            .id;
        let invalid = HarnessExecutionPolicy {
            profile_id: "Read Only".into(),
            capability_enabled: BTreeMap::new(),
        };
        assert!(matches!(
            store.save_execution_policy(project_id, &invalid),
            Err(StoreError::InvalidRecord(message))
                if message == "execution policy contains invalid bounded fields"
        ));
        assert!(store
            .load_execution_policy(project_id)
            .expect("policy remains absent")
            .is_none());
    }

    #[test]
    fn provider_selection_switches_between_existing_profiles_atomically() {
        let workspace = tempfile::tempdir().expect("workspace");
        let store = Store::open_in_memory().expect("store");
        let project_id = store
            .resolve_workspace(workspace.path())
            .expect("workspace")
            .snapshot
            .project
            .id;
        store
            .save_provider_profile(
                project_id,
                &ProviderProfile {
                    id: "hosted".into(),
                    label: "Hosted".into(),
                    kind: ProviderKind::OpenAiCompatible,
                    endpoint: "https://example.test/v1/".into(),
                    model: "hosted-model".into(),
                    selected: true,
                },
            )
            .expect("hosted profile");

        let profiles = store
            .select_provider_profile(project_id, "lm-studio")
            .expect("select default profile");

        assert_eq!(
            profiles.iter().filter(|profile| profile.selected).count(),
            1
        );
        assert!(profiles
            .iter()
            .any(|profile| profile.id == "lm-studio" && profile.selected));
    }

    #[test]
    fn provider_model_selection_updates_only_the_exact_profile() {
        let workspace = tempfile::tempdir().expect("workspace");
        let store = Store::open_in_memory().expect("store");
        let project_id = store
            .resolve_workspace(workspace.path())
            .expect("workspace")
            .snapshot
            .project
            .id;
        store
            .save_provider_profile(
                project_id,
                &ProviderProfile {
                    id: "hosted".into(),
                    label: "Hosted".into(),
                    kind: ProviderKind::OpenAiCompatible,
                    endpoint: "https://example.test/v1/".into(),
                    model: "hosted-old".into(),
                    selected: false,
                },
            )
            .expect("hosted profile");

        let profiles = store
            .select_provider_model(project_id, "hosted", " hosted-new ")
            .expect("selected model");

        assert!(profiles.iter().any(|profile| {
            profile.id == "hosted" && profile.model == "hosted-new" && !profile.selected
        }));
        assert!(profiles.iter().any(|profile| {
            profile.id == "lm-studio"
                && profile.model == argentum_domain::DEFAULT_MODEL
                && profile.selected
        }));
    }

    #[test]
    fn provider_model_selection_persists_and_is_project_scoped() {
        let directory = tempfile::tempdir().expect("directory");
        let left_root = directory.path().join("left");
        let right_root = directory.path().join("right");
        std::fs::create_dir(&left_root).expect("left workspace");
        std::fs::create_dir(&right_root).expect("right workspace");
        let database = directory.path().join("argentum.db");
        let store = Store::open(&database).expect("store");
        let left = store.resolve_workspace(&left_root).expect("left");
        let right = store.resolve_workspace(&right_root).expect("right");

        store
            .select_provider_model(left.snapshot.project.id, "lm-studio", "left-model")
            .expect("left model");
        assert!(matches!(
            store.select_provider_model(right.snapshot.project.id, "missing", "right-model"),
            Err(StoreError::InvalidRecord(message)) if message.contains("does not belong")
        ));
        drop(store);

        let reopened = Store::open(&database).expect("reopened store");
        let left_profiles = reopened
            .provider_profiles(left.snapshot.project.id)
            .expect("left profiles");
        let right_profiles = reopened
            .provider_profiles(right.snapshot.project.id)
            .expect("right profiles");
        assert!(left_profiles
            .iter()
            .any(|profile| profile.id == "lm-studio" && profile.model == "left-model"));
        assert!(right_profiles.iter().any(|profile| {
            profile.id == "lm-studio" && profile.model == argentum_domain::DEFAULT_MODEL
        }));
    }

    #[test]
    fn provider_model_selection_rejects_invalid_values_without_mutation() {
        let workspace = tempfile::tempdir().expect("workspace");
        let store = Store::open_in_memory().expect("store");
        let project_id = store
            .resolve_workspace(workspace.path())
            .expect("workspace")
            .snapshot
            .project
            .id;

        for model in ["", "bad\nmodel"] {
            assert!(matches!(
                store.select_provider_model(project_id, "lm-studio", model),
                Err(StoreError::InvalidRecord(message)) if message.contains("invalid")
            ));
        }
        assert_eq!(
            store
                .selected_provider_profile(project_id)
                .expect("selected profile")
                .expect("profile")
                .model,
            argentum_domain::DEFAULT_MODEL
        );
    }

    #[test]
    fn provider_profiles_are_isolated_by_workspace_project() {
        let directory = tempfile::tempdir().expect("directory");
        let left_root = directory.path().join("left");
        let right_root = directory.path().join("right");
        std::fs::create_dir(&left_root).expect("left workspace");
        std::fs::create_dir(&right_root).expect("right workspace");
        let store = Store::open_in_memory().expect("store");
        let left = store.resolve_workspace(&left_root).expect("left");
        let right = store.resolve_workspace(&right_root).expect("right");
        store
            .save_provider_profile(
                left.snapshot.project.id,
                &ProviderProfile {
                    id: "left-only".into(),
                    label: "Left only".into(),
                    kind: ProviderKind::OpenAiCompatible,
                    endpoint: "https://left.example.test/v1/".into(),
                    model: "left-model".into(),
                    selected: false,
                },
            )
            .expect("left profile");

        let left_profiles = store
            .provider_profiles(left.snapshot.project.id)
            .expect("left profiles");
        let right_profiles = store
            .provider_profiles(right.snapshot.project.id)
            .expect("right profiles");

        assert!(left_profiles
            .iter()
            .any(|profile| profile.id == "left-only"));
        assert!(!right_profiles
            .iter()
            .any(|profile| profile.id == "left-only"));
        assert!(matches!(
            store.select_provider_profile(right.snapshot.project.id, "left-only"),
            Err(StoreError::InvalidRecord(message)) if message.contains("does not belong")
        ));
    }

    #[test]
    fn store_rejects_secret_bearing_provider_endpoint() {
        let workspace = tempfile::tempdir().expect("workspace");
        let store = Store::open_in_memory().expect("store");
        let project_id = store
            .resolve_workspace(workspace.path())
            .expect("workspace")
            .snapshot
            .project
            .id;
        let profile = ProviderProfile {
            id: "unsafe".into(),
            label: "Unsafe".into(),
            kind: ProviderKind::OpenAiCompatible,
            endpoint: "https://example.test/v1?api_key=secret".into(),
            model: "test-model".into(),
            selected: false,
        };

        assert!(matches!(
            store.save_provider_profile(project_id, &profile),
            Err(StoreError::InvalidRecord(message))
                if message == "provider profile contains invalid or secret-bearing fields"
        ));
        assert!(!store
            .provider_profiles(project_id)
            .expect("profiles")
            .iter()
            .any(|saved| saved.id == "unsafe"));
    }

    #[test]
    fn default_session_title_uses_one_bounded_unicode_prompt_line() {
        let workspace = tempfile::tempdir().expect("workspace");
        let store = Store::open_in_memory().expect("store");
        let resolution = store
            .resolve_workspace(workspace.path())
            .expect("resolution");
        let session_id = resolution
            .snapshot
            .active_session_id
            .expect("active session");
        let first_line = "Ülevaade 東京 ".repeat(8);
        let prompt = format!("\n \n  {first_line}\t extra  \nignore this second line");

        let updated = store
            .title_default_session_from_prompt(resolution.snapshot.project.id, session_id, &prompt)
            .expect("title update")
            .expect("updated snapshot");
        let title = &updated.sessions[0].title;

        assert_eq!(title.chars().count(), 56);
        assert!(!title.contains("ignore"));
        assert!(title.starts_with("Ülevaade 東京"));
        assert!(!title.contains('\t'));
        assert!(!title.contains("  "));
        assert_eq!(updated.active_session_id, Some(session_id));
    }

    #[test]
    fn explicit_session_title_is_not_replaced_by_a_prompt() {
        let workspace = tempfile::tempdir().expect("workspace");
        let store = Store::open_in_memory().expect("store");
        let resolution = store
            .resolve_workspace(workspace.path())
            .expect("resolution");
        let explicit = store
            .create_session(resolution.snapshot.project.id, "Pinned title")
            .expect("explicit session");

        let updated = store
            .title_default_session_from_prompt(
                resolution.snapshot.project.id,
                explicit.id,
                "A different title",
            )
            .expect("title check");

        assert!(updated.is_none());
        let snapshot = store
            .workspace_snapshot(resolution.snapshot.project.id)
            .expect("snapshot");
        assert_eq!(
            snapshot
                .sessions
                .iter()
                .find(|session| session.id == explicit.id)
                .expect("explicit session")
                .title,
            "Pinned title"
        );
    }

    #[test]
    fn selected_session_must_belong_to_the_project() {
        let directory = tempfile::tempdir().expect("directory");
        let left_root = directory.path().join("left");
        let right_root = directory.path().join("right");
        std::fs::create_dir(&left_root).expect("left");
        std::fs::create_dir(&right_root).expect("right");
        let store = Store::open_in_memory().expect("store");
        let left = store.resolve_workspace(&left_root).expect("left workspace");
        let right = store
            .resolve_workspace(&right_root)
            .expect("right workspace");

        assert!(matches!(
            store.select_session(
                left.snapshot.project.id,
                right.snapshot.active_session_id.expect("right session"),
            ),
            Err(StoreError::InvalidRecord(message))
                if message.contains("does not belong")
        ));
    }

    #[test]
    fn prompt_derived_title_survives_store_reopen() {
        let directory = tempfile::tempdir().expect("directory");
        let workspace = directory.path().join("workspace");
        std::fs::create_dir(&workspace).expect("workspace");
        let database = directory.path().join("argentum.db");
        let store = Store::open(&database).expect("store");
        let resolution = store.resolve_workspace(&workspace).expect("resolution");
        store
            .title_default_session_from_prompt(
                resolution.snapshot.project.id,
                resolution
                    .snapshot
                    .active_session_id
                    .expect("active session"),
                "Durable session title\nsecond line",
            )
            .expect("title update")
            .expect("updated snapshot");
        drop(store);

        let reopened = Store::open(&database).expect("reopened store");
        let snapshot = reopened
            .resolve_workspace(&workspace)
            .expect("restored workspace")
            .snapshot;

        assert_eq!(snapshot.sessions[0].title, "Durable session title");
    }

    #[test]
    fn session_creation_rolls_back_when_its_event_cannot_be_persisted() {
        let workspace = tempfile::tempdir().expect("workspace");
        let store = Store::open_in_memory().expect("store");
        let resolution = store
            .resolve_workspace(workspace.path())
            .expect("resolution");
        let before = resolution.snapshot.clone();
        store
            .connection
            .lock()
            .expect("connection")
            .execute_batch(
                "CREATE TRIGGER fail_session_event
                 BEFORE INSERT ON event_log
                 WHEN NEW.event_type = 'session_created'
                 BEGIN
                     SELECT RAISE(FAIL, 'session event rejected');
                 END;",
            )
            .expect("failure trigger");

        assert!(store
            .create_session_with_event(
                &resolution.workspace_key,
                resolution.snapshot.project.id,
                "Must roll back",
            )
            .is_err());
        let after = store
            .workspace_snapshot(resolution.snapshot.project.id)
            .expect("snapshot after rollback");

        assert_eq!(after, before);
        assert!(store.events().expect("events").is_empty());
    }

    #[test]
    fn goals_persist_replace_and_clear_with_audit_history() {
        let directory = tempfile::tempdir().expect("directory");
        let workspace = directory.path().join("workspace");
        std::fs::create_dir(&workspace).expect("workspace");
        let database = directory.path().join("argentum.db");
        let store = Store::open(&database).expect("store");
        let resolution = store.resolve_workspace(&workspace).expect("resolution");
        let project_id = resolution.snapshot.project.id;
        let session_id = resolution.snapshot.active_session_id.expect("session");
        let timestamp = now();
        let goal = Goal {
            id: argentum_domain::GoalId::new_v4(),
            project_id,
            session_id,
            objective: "Ship the first verified slice".into(),
            lifecycle: argentum_domain::GoalLifecycle::Active,
            token_budget: Some(20_000),
            tool_budget: Some(12),
            time_budget_seconds: Some(3_600),
            tokens_used: 120,
            tools_used: 2,
            iteration: 1,
            next_action: "Inspect the current workspace".into(),
            verification_history: Vec::new(),
            created_at: timestamp,
            updated_at: timestamp,
        };
        store.save_goal(&goal).expect("save goal");
        assert_eq!(
            store.goal(project_id, session_id).expect("load goal"),
            Some(goal.clone())
        );

        let mut replacement = goal.clone();
        replacement.objective = "Ship the second verified slice".into();
        replacement.updated_at = now();
        store.save_goal(&replacement).expect("replace goal");
        assert_eq!(
            store
                .goal(project_id, session_id)
                .expect("load replacement"),
            Some(replacement)
        );

        assert!(store
            .clear_goal(project_id, session_id)
            .expect("clear goal"));
        assert_eq!(
            store.goal(project_id, session_id).expect("cleared goal"),
            None
        );
        let history_count: i64 = store
            .connection
            .lock()
            .expect("connection")
            .query_row("SELECT COUNT(*) FROM goal_history", [], |row| row.get(0))
            .expect("history count");
        assert_eq!(history_count, 1);

        drop(store);
        let reopened = Store::open(&database).expect("reopened store");
        assert_eq!(
            reopened
                .goal(project_id, session_id)
                .expect("reopened goal"),
            None
        );
    }

    #[test]
    fn goals_are_project_scoped_and_reject_invalid_budgets() {
        let directory = tempfile::tempdir().expect("directory");
        let left_root = directory.path().join("left");
        let right_root = directory.path().join("right");
        std::fs::create_dir(&left_root).expect("left workspace");
        std::fs::create_dir(&right_root).expect("right workspace");
        let store = Store::open_in_memory().expect("store");
        let left = store.resolve_workspace(&left_root).expect("left");
        let right = store.resolve_workspace(&right_root).expect("right");
        let left_session = left.snapshot.active_session_id.expect("left session");
        let right_session = right.snapshot.active_session_id.expect("right session");
        let timestamp = now();
        let goal = Goal {
            id: argentum_domain::GoalId::new_v4(),
            project_id: left.snapshot.project.id,
            session_id: left_session,
            objective: "Left workspace objective".into(),
            lifecycle: argentum_domain::GoalLifecycle::Active,
            token_budget: Some(100),
            tool_budget: None,
            time_budget_seconds: None,
            tokens_used: 0,
            tools_used: 0,
            iteration: 0,
            next_action: "Continue".into(),
            verification_history: Vec::new(),
            created_at: timestamp,
            updated_at: timestamp,
        };
        store.save_goal(&goal).expect("left goal");
        assert_eq!(
            store
                .goal(right.snapshot.project.id, right_session)
                .expect("right goal"),
            None
        );

        let mut invalid = goal;
        invalid.token_budget = Some(0);
        assert!(matches!(
            store.save_goal(&invalid),
            Err(StoreError::InvalidRecord(message)) if message.contains("token budget")
        ));
    }
}
