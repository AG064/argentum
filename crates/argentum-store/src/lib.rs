use std::collections::BTreeMap;
use std::path::{Component, Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use argentum_domain::working_context::{ContextGroup, GroupBody};
use argentum_domain::{
    final_result::{
        AnswerContractEvaluation, AnswerContractSource, AnswerContractStatus,
        FinalAccountingReference, FinalAnswer, FinalAnswerContract, FinalChangeReference,
        FinalFailure, FinalFailureKind, FinalReferenceSet, FinalResultCompleteness,
        FinalRunCorrelation, FinalRunResult, FinalRunState, FinalUsage, FinalUsageReference,
        FinalVerificationReference, FINAL_RESULT_MAX_ANSWER_BYTES, FINAL_RESULT_MAX_EVENT_RECORDS,
        FINAL_RESULT_MAX_REFERENCE_ITEMS, FINAL_RESULT_MAX_USAGE_ITEMS,
        FINAL_RESULT_SCHEMA_VERSION,
    },
    now, AppEvent, ApprovalGrantRecord, ChangeOperationId, ChangeRecoveryState, ChangeSetId,
    ChangeSetSnapshot, ConversationMessage, ConversationMessageStatus, ConversationRole,
    ConversationSnapshot, ExecutionAuditRecord, FileChangeId, FileChangeKind, FileChangeOrigin,
    FileChangeRecord, FileSnapshotRef, Goal, GrantId, GrantScope, GrantSubject,
    HarnessExecutionPolicy, LayoutProfile, LineCountAvailability, MessageId, ModelUsage,
    OnboardingSnapshot, Project, ProjectId, ProviderKind, ProviderProfile, RestoreResult, RunId,
    Session, SessionArchiveFilter, SessionId, SessionListRow, SessionListSnapshot, SessionSummary,
    Sha256Digest, VerificationEvidence, WorkspaceSnapshot, DEFAULT_EXECUTION_PROFILE_ID,
};
use directories::ProjectDirs;
use rusqlite::{
    params, params_from_iter, types::Value, Connection, OpenFlags, OptionalExtension,
    TransactionBehavior,
};
use sha2::{Digest, Sha256};
use thiserror::Error;
use time::OffsetDateTime;
use url::Url;
use uuid::Uuid;

mod command_identity;
mod context_budget;
mod extensions;
mod grant_lookup;
mod history;
pub use grant_lookup::MAX_AUTHORIZATION_GRANTS;
mod review;
mod run_diagnostics;
mod run_outcomes;
mod session_forks;
pub use history::{HistoryMessage, HistoryPage};
pub use review::{ReviewCursor, ReviewRecord, ReviewRun, ReviewRuns};
pub use run_diagnostics::*;
pub use run_outcomes::RunOutcome;
pub use session_forks::{SessionForkPlan, SESSION_FORK_VERSION};
mod context_summary;
mod goal_accounting;
mod goal_supervision;
mod recovery;
mod working_context;
pub use command_identity::{
    validate_request_id as validate_command_request_id, CommandClaim, CommandClaimInput,
    CommandIdentityMatch, CommandIdentityScope, CommandIdentityState, CommandOutcomeKind,
    ForkSessionCommandClaim, NewSessionCommandClaim, StoredCommandIdentity,
    COMMAND_IDENTITY_VERSION, MAX_ACTIVE_COMMAND_IDENTITIES_PER_STORE, MAX_COMMAND_CANONICAL_BYTES,
    MAX_COMMAND_IDENTITIES_PER_PROJECT, MAX_COMMAND_IDENTITIES_PER_STORE,
    MAX_COMMAND_INSPECTION_LIMIT, MAX_COMMAND_REQUEST_ID_BYTES, MAX_COMMAND_RESULT_BYTES,
    MAX_RETAINED_COMMAND_RESULT_BODIES_PER_STORE,
};
pub use context_summary::{ContextCommitToken, SummarySnapshot};
pub use recovery::StoreHostOwner;
pub use working_context::{ContextSource, ContextSourceWindow};

#[derive(Debug, Error)]
pub enum StoreError {
    #[error("goal accounting: {0}")]
    GoalAccounting(&'static str),
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
    #[error("another writable Argentum host owns this database")]
    HostOwnershipLive,
    #[error("the prior writable host is proven abandoned; run the trusted recovery command")]
    HostRecoveryRequired,
    #[error("database ownership is unknown: {0}")]
    HostOwnershipUnknown(&'static str),
    #[error("this host generation has been fenced")]
    HostGenerationFenced,
    #[error("interrupted-run recovery is supported only for a persistent Windows local database")]
    RecoveryUnsupported,
    #[error("the recovery decision is stale")]
    RecoveryDecisionStale,
    #[error("recovery remains blocked: {0}")]
    RecoveryBlocked(&'static str),
    #[error("the command identity does not exist")]
    CommandIdentityMissing,
    #[error("the command identity is already bound to different input")]
    CommandIdentityConflict,
    #[error("the command identity quota is full")]
    CommandIdentityQuotaExceeded,
    #[error("the concurrent command admission limit is active")]
    CommandAdmissionLimit,
    #[error("the retained command result exceeds the size limit")]
    CommandResultTooLarge,
    #[error("the durable command execution snapshot changed")]
    CommandExecutionSnapshotChanged,
}

pub const EVENT_PAYLOAD_VERSION: u16 = 1;

fn validate_module_preferences(
    preferences: &BTreeMap<String, argentum_domain::ModulePreference>,
) -> Result<(), StoreError> {
    if preferences.len() > 128
        || preferences.iter().any(|(id, preference)| {
            id.is_empty()
                || id.len() > 64
                || !id
                    .bytes()
                    .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'.' || b == b'-')
                || (preference.enabled && !preference.installed)
        })
    {
        return Err(StoreError::InvalidRecord(
            "invalid module preferences".into(),
        ));
    }
    Ok(())
}
pub const MAX_CONVERSATION_MESSAGE_BYTES: usize = 512 * 1024;
pub const MAX_CONVERSATION_SNAPSHOT_MESSAGES: usize = 200;
pub const MAX_CONVERSATION_SNAPSHOT_BYTES: usize = 2 * 1024 * 1024;
pub const MAX_GOAL_OBJECTIVE_BYTES: usize = 16 * 1024;
pub const MAX_GOAL_NEXT_ACTION_BYTES: usize = 4 * 1024;
pub const MAX_GOAL_VERIFICATION_HISTORY: usize = 64;
pub const MAX_SESSION_EVENT_WINDOW: usize = 256;
pub const MAX_SESSION_LIST_QUERY_BYTES: usize = 256;
pub const DEFAULT_SESSION_LIST_LIMIT: usize = 20;
pub const MAX_SESSION_LIST_LIMIT: usize = 100;
pub const MAX_SESSION_TITLE_BYTES: usize = 256;
pub const MAX_CHANGE_SNAPSHOT_BYTES: usize = 1024 * 1024;
pub const MAX_CHANGE_RUN_SNAPSHOT_BYTES: usize = 16 * 1024 * 1024;
pub const MAX_ONBOARDING_SNAPSHOT_BYTES: usize = 64 * 1024;
// Aggregate allowance across linked runs, independent of the smaller run cap.
const MAX_GOAL_TOKEN_BUDGET: u64 = 20_000_000;
const MAX_GOAL_TOOL_BUDGET: u32 = 100_000;
const MAX_GOAL_TIME_BUDGET_SECONDS: u64 = 7 * 24 * 60 * 60;
const DATABASE_BUSY_TIMEOUT: Duration = Duration::from_secs(5);

fn bounded_result_text(value: &str, max_bytes: usize) -> (String, bool) {
    if value.len() <= max_bytes {
        return (value.to_owned(), false);
    }
    let mut end = max_bytes;
    while !value.is_char_boundary(end) {
        end = end.saturating_sub(1);
    }
    (value[..end].to_owned(), true)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EventScope {
    pub workspace_key: String,
    pub project_id: ProjectId,
    pub session_id: Option<SessionId>,
    pub run_id: Option<RunId>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredEventRecord {
    pub sequence: u64,
    pub occurred_at: OffsetDateTime,
    pub run_id: Option<RunId>,
    pub event: AppEvent,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionEventWindow {
    pub records: Vec<StoredEventRecord>,
    pub truncated: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkspaceResolution {
    pub workspace_key: String,
    pub snapshot: WorkspaceSnapshot,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreparedChangeOperation {
    pub operation_id: ChangeOperationId,
    pub workspace_key: String,
    pub project_id: ProjectId,
    pub session_id: SessionId,
    pub run_id: RunId,
    pub relative_path: String,
    pub kind: FileChangeKind,
    pub origin: FileChangeOrigin,
    pub before: Option<FileSnapshotRef>,
    pub after: Option<FileSnapshotRef>,
    pub before_bytes: Option<Vec<u8>>,
    pub after_bytes: Option<Vec<u8>>,
    pub additions: u32,
    pub removals: u32,
    /// Provenance of the numeric counts, admitted at preparation from the same
    /// snapshots used to derive them. Persisted verbatim so persistence never
    /// has to infer provenance from zero values, current files or retention.
    pub line_count_availability: LineCountAvailability,
    pub restored_from: Option<FileChangeId>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredChangeOperation {
    pub operation_id: ChangeOperationId,
    pub workspace_key: String,
    pub project_id: ProjectId,
    pub session_id: SessionId,
    pub run_id: RunId,
    pub relative_path: String,
    pub kind: FileChangeKind,
    pub origin: FileChangeOrigin,
    pub before: Option<FileSnapshotRef>,
    pub after: Option<FileSnapshotRef>,
    pub additions: u32,
    pub removals: u32,
    pub line_count_availability: LineCountAvailability,
    pub restored_from: Option<FileChangeId>,
    pub state: ChangeRecoveryState,
    pub current_digest: Option<Sha256Digest>,
    pub created_at: OffsetDateTime,
}

#[derive(Clone)]
pub struct Store {
    connection: Arc<Mutex<Connection>>,
    context_neutral_changes: Arc<std::sync::atomic::AtomicU64>,
    path: Option<PathBuf>,
    host_fence: Arc<Mutex<Option<Arc<recovery::ManagedHostFence>>>>,
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
            context_neutral_changes: Arc::new(std::sync::atomic::AtomicU64::new(0)),
            path: Some(path),
            host_fence: Arc::new(Mutex::new(None)),
        };
        store.migrate()?;
        Ok(store)
    }

    /// Open an already initialized store for metadata-only inspection. This
    /// path never creates directories, migrates schema, or acquires writable
    /// host ownership. Callers must not use the returned store for mutations.
    pub fn open_existing_read_only(path: impl AsRef<Path>) -> Result<Self, StoreError> {
        let path = path.as_ref().to_path_buf();
        let connection = Connection::open_with_flags(&path, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
        connection.busy_timeout(DATABASE_BUSY_TIMEOUT)?;
        Ok(Self {
            connection: Arc::new(Mutex::new(connection)),
            context_neutral_changes: Arc::new(std::sync::atomic::AtomicU64::new(0)),
            path: Some(path),
            host_fence: Arc::new(Mutex::new(None)),
        })
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
            context_neutral_changes: Arc::new(std::sync::atomic::AtomicU64::new(0)),
            path: None,
            host_fence: Arc::new(Mutex::new(None)),
        };
        store.migrate()?;
        Ok(store)
    }

    pub fn path(&self) -> Option<&Path> {
        self.path.as_deref()
    }

    pub fn append_event(&self, event: &AppEvent) -> Result<(), StoreError> {
        self.append_event_values(None, None, None, None, event)
            .map(|_| ())
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
        .map(|_| ())
    }

    pub fn append_event_scoped_record(
        &self,
        scope: &EventScope,
        event: &AppEvent,
    ) -> Result<StoredEventRecord, StoreError> {
        self.append_event_values(
            Some(&scope.workspace_key),
            Some(scope.project_id),
            scope.session_id,
            scope.run_id,
            event,
        )
    }

    pub fn append_context_neutral_event_scoped_record(
        &self,
        scope: &EventScope,
        event: &AppEvent,
    ) -> Result<StoredEventRecord, StoreError> {
        self.append_event_values_internal(
            Some(&scope.workspace_key),
            Some(scope.project_id),
            scope.session_id,
            scope.run_id,
            event,
            true,
        )
    }

    fn append_event_values(
        &self,
        workspace_key: Option<&str>,
        project_id: Option<ProjectId>,
        session_id: Option<SessionId>,
        run_id: Option<RunId>,
        event: &AppEvent,
    ) -> Result<StoredEventRecord, StoreError> {
        self.append_event_values_internal(
            workspace_key,
            project_id,
            session_id,
            run_id,
            event,
            false,
        )
    }

    fn append_event_values_internal(
        &self,
        workspace_key: Option<&str>,
        project_id: Option<ProjectId>,
        session_id: Option<SessionId>,
        run_id: Option<RunId>,
        event: &AppEvent,
        context_neutral: bool,
    ) -> Result<StoredEventRecord, StoreError> {
        let fence = self
            .host_fence
            .lock()
            .map_err(|_| StoreError::LockPoisoned)?
            .clone();
        let current_generation = match fence {
            Some(fence) if fence.active.load(std::sync::atomic::Ordering::Acquire) => {
                fence.generation
            }
            Some(_) => return Err(StoreError::HostGenerationFenced),
            None => None,
        };
        let mut connection = self
            .connection
            .lock()
            .map_err(|_| StoreError::LockPoisoned)?;
        let changes_before = connection.total_changes();
        let transaction = connection.transaction()?;
        if let Some(generation) = current_generation {
            recovery::validate_host_generation_for_event_on(&transaction, generation)?;
        }
        let (sequence, occurred_at) = append_event_on(
            &transaction,
            workspace_key,
            project_id,
            session_id,
            run_id,
            event,
        )?;
        recovery::update_run_claim_for_event_on(&transaction, current_generation, run_id, event)?;
        transaction.commit()?;
        if context_neutral {
            let committed_changes = connection
                .total_changes()
                .checked_sub(changes_before)
                .ok_or_else(|| {
                    StoreError::InvalidRecord("context change counter underflow".into())
                })?;
            self.context_neutral_changes.fetch_add(
                if matches!(event, AppEvent::SummaryTransportObserved { .. }) {
                    // Exclude only this exact inserted diagnostic. Trigger
                    // writes must still invalidate a context checkpoint.
                    1
                } else {
                    committed_changes
                },
                std::sync::atomic::Ordering::Relaxed,
            );
        }
        Ok(StoredEventRecord {
            sequence,
            occurred_at,
            run_id,
            event: event.clone(),
        })
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

    pub fn session_event_window(
        &self,
        project_id: ProjectId,
        session_id: SessionId,
        limit: usize,
    ) -> Result<SessionEventWindow, StoreError> {
        if limit == 0 || limit > MAX_SESSION_EVENT_WINDOW {
            return Err(StoreError::InvalidRecord(
                "session event window limit is invalid".into(),
            ));
        }
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

        let row_limit = i64::try_from(limit + 1).map_err(|_| {
            StoreError::InvalidRecord("session event window limit is invalid".into())
        })?;
        let mut statement = connection.prepare(
            "SELECT id, created_at, run_id, payload
             FROM event_log
             WHERE project_id = ?1 AND session_id = ?2
             ORDER BY id DESC
             LIMIT ?3",
        )?;
        let rows = statement.query_map(
            params![project_id.to_string(), session_id.to_string(), row_limit],
            |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, Option<String>>(2)?,
                    row.get::<_, String>(3)?,
                ))
            },
        )?;
        let mut records = Vec::with_capacity(limit + 1);
        for row in rows {
            let (sequence, created_at, run_id, payload) = row?;
            let sequence = u64::try_from(sequence)
                .map_err(|_| StoreError::InvalidRecord("event sequence is invalid".into()))?;
            let occurred_at = OffsetDateTime::from_unix_timestamp(created_at)
                .map_err(|_| StoreError::InvalidRecord("event timestamp is invalid".into()))?;
            let run_id = run_id
                .map(|value| {
                    value
                        .parse()
                        .map_err(|_| StoreError::InvalidRecord("event run id is invalid".into()))
                })
                .transpose()?;
            records.push(StoredEventRecord {
                sequence,
                occurred_at,
                run_id,
                event: serde_json::from_str(&payload)?,
            });
        }
        let truncated = records.len() > limit;
        records.truncate(limit);
        records.reverse();
        Ok(SessionEventWindow { records, truncated })
    }

    pub fn run_event_window(
        &self,
        project_id: ProjectId,
        run_id: RunId,
        limit: usize,
    ) -> Result<SessionEventWindow, StoreError> {
        if limit == 0 || limit > FINAL_RESULT_MAX_EVENT_RECORDS {
            return Err(StoreError::InvalidRecord(
                "run event window limit is invalid".into(),
            ));
        }
        let connection = self
            .connection
            .lock()
            .map_err(|_| StoreError::LockPoisoned)?;
        let row_limit = i64::try_from(limit + 1)
            .map_err(|_| StoreError::InvalidRecord("run event window limit is invalid".into()))?;
        let mut statement = connection.prepare(
            "SELECT id, created_at, run_id, payload
             FROM event_log
             WHERE project_id = ?1 AND run_id = ?2
             ORDER BY id DESC
             LIMIT ?3",
        )?;
        let rows = statement.query_map(
            params![project_id.to_string(), run_id.to_string(), row_limit],
            |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, Option<String>>(2)?,
                    row.get::<_, String>(3)?,
                ))
            },
        )?;
        let mut records = Vec::with_capacity(limit + 1);
        for row in rows {
            let (sequence, created_at, stored_run_id, payload) = row?;
            let sequence = u64::try_from(sequence)
                .map_err(|_| StoreError::InvalidRecord("event sequence is invalid".into()))?;
            let occurred_at = OffsetDateTime::from_unix_timestamp(created_at)
                .map_err(|_| StoreError::InvalidRecord("event timestamp is invalid".into()))?;
            let stored_run_id = stored_run_id
                .map(|value| {
                    value
                        .parse()
                        .map_err(|_| StoreError::InvalidRecord("event run id is invalid".into()))
                })
                .transpose()?;
            records.push(StoredEventRecord {
                sequence,
                occurred_at,
                run_id: stored_run_id,
                event: serde_json::from_str(&payload)?,
            });
        }
        let truncated = records.len() > limit;
        records.truncate(limit);
        records.reverse();
        Ok(SessionEventWindow { records, truncated })
    }

    pub fn final_run_result(
        &self,
        project_id: ProjectId,
        run_id: RunId,
    ) -> Result<Option<FinalRunResult>, StoreError> {
        let events = self.run_event_window(project_id, run_id, FINAL_RESULT_MAX_EVENT_RECORDS)?;
        if events.records.is_empty() {
            return Ok(None);
        }

        let mut task = None;
        let mut lifecycle = None;
        let mut run_error = None;
        let mut usage = Vec::new();
        let mut accounting = Vec::new();
        let mut answer_contract_source: Option<(AnswerContractSource, u16)> = None;
        let mut answer_contract_evaluation: Option<AnswerContractEvaluation> = None;
        let mut supplemental_checks = Vec::new();
        for record in &events.records {
            match &record.event {
                AppEvent::SupplementalCheckRecorded(check) if check.run_id == run_id => {
                    if supplemental_checks.len()
                        < argentum_domain::supplemental::RUST_CHECK_MAX_BATCHES
                    {
                        supplemental_checks.push(check.clone());
                    }
                }
                AppEvent::TaskAccepted(value) => {
                    task.get_or_insert_with(|| value.clone());
                }
                AppEvent::AnswerContractAccepted {
                    run_id: event_run_id,
                    source,
                    validator_version,
                } if *event_run_id == run_id => {
                    answer_contract_source = Some((source.clone(), *validator_version));
                }
                AppEvent::AnswerContractEvaluated {
                    run_id: event_run_id,
                    evaluation,
                } if *event_run_id == run_id => {
                    answer_contract_source =
                        Some((evaluation.source.clone(), evaluation.validator_version));
                    answer_contract_evaluation = Some(evaluation.clone());
                }
                AppEvent::RunStatusChanged {
                    run_id: event_run_id,
                    lifecycle: value,
                } if *event_run_id == run_id => lifecycle = Some(*value),
                AppEvent::RunError {
                    run_id: event_run_id,
                    message,
                    recoverable,
                    ..
                } if *event_run_id == run_id => {
                    run_error = Some((message.clone(), *recoverable));
                }
                AppEvent::ModelUsageUpdated {
                    run_id: event_run_id,
                    profile_id,
                    model,
                    usage: value,
                    ..
                } if *event_run_id == run_id => usage.push(FinalUsageReference {
                    profile_id: profile_id.clone(),
                    model: model.clone(),
                    usage: value.clone(),
                }),
                AppEvent::ContextRequestAccounted {
                    run_id: event_run_id,
                    request_id,
                    purpose,
                    accounting: value,
                    ..
                } if *event_run_id == run_id => accounting.push(FinalAccountingReference {
                    request_id: *request_id,
                    purpose: purpose.clone(),
                    method: value.method.clone(),
                    accounted_input: Some(value.accounted_input),
                    observed_total_tokens: None,
                    charge_state: None,
                }),
                AppEvent::GoalRequestAccounting {
                    run_id: event_run_id,
                    charge: value,
                    ..
                } if *event_run_id == run_id => accounting.push(FinalAccountingReference {
                    request_id: value.request_id,
                    purpose: value.purpose.clone(),
                    method: value.method.clone(),
                    accounted_input: None,
                    observed_total_tokens: value
                        .observed
                        .as_ref()
                        .map(|observed| observed.total_tokens),
                    charge_state: Some(format!("{:?}", value.state).to_ascii_lowercase()),
                }),
                _ => {}
            }
        }

        let task = task
            .ok_or_else(|| StoreError::InvalidRecord("run has no retained task identity".into()))?;
        let session_id = task.session_id;
        let run_state = lifecycle
            .and_then(FinalRunState::from_lifecycle)
            .unwrap_or(FinalRunState::NotFinal);
        let failure = match run_state {
            FinalRunState::Failed => Some(FinalFailure {
                kind: match run_error.as_ref().map(|(message, _)| message.as_str()) {
                    Some(message) if message.starts_with("approval_required") => {
                        FinalFailureKind::ApprovalRequired
                    }
                    Some(_) => FinalFailureKind::RunError,
                    None => FinalFailureKind::Unknown,
                },
                message: run_error.as_ref().map(|(message, _)| message.clone()),
                recoverable: run_error.as_ref().map(|(_, recoverable)| *recoverable),
            }),
            FinalRunState::Cancelled => Some(FinalFailure {
                kind: FinalFailureKind::Cancelled,
                message: None,
                recoverable: None,
            }),
            FinalRunState::Interrupted => Some(FinalFailure {
                kind: FinalFailureKind::Interrupted,
                message: None,
                recoverable: None,
            }),
            FinalRunState::Complete | FinalRunState::NotFinal => None,
        };

        let conversation = self.conversation_snapshot(project_id, session_id)?;
        let include_failed_contract_answer = answer_contract_evaluation
            .as_ref()
            .is_some_and(|evaluation| evaluation.status == AnswerContractStatus::Invalid);
        let answer = conversation
            .messages
            .iter()
            .rev()
            .find(|message| {
                message.run_id == run_id
                    && message.role == argentum_domain::ConversationRole::Assistant
                    && (message.status == argentum_domain::ConversationMessageStatus::Complete
                        || (include_failed_contract_answer
                            && message.status
                                == argentum_domain::ConversationMessageStatus::Failed))
            })
            .map(|message| {
                let (text, truncated) =
                    bounded_result_text(&message.text, FINAL_RESULT_MAX_ANSWER_BYTES);
                FinalAnswer {
                    authored_by: "model".into(),
                    available: true,
                    text: Some(text),
                    truncated,
                    unavailable_reason: None,
                }
            })
            .unwrap_or_else(|| FinalAnswer::unavailable("assistant_answer_unavailable"));
        let answer_complete = !answer.truncated;
        let answer_contract = answer_contract_source.map(|(source, validator_version)| {
            let evaluation = answer_contract_evaluation.as_ref();
            let parsed_model_value = evaluation
                .filter(|evaluation| evaluation.status == AnswerContractStatus::Valid)
                .and(answer.text.as_deref())
                .and_then(|text| serde_json::from_str::<serde_json::Value>(text).ok());
            let parsed_value_available = parsed_model_value.is_some();
            FinalAnswerContract {
                source,
                validator_version,
                status: evaluation.map(|evaluation| evaluation.status),
                error_count: evaluation.map_or(0, |evaluation| evaluation.error_count),
                error_codes: evaluation
                    .map(|evaluation| evaluation.error_codes.clone())
                    .unwrap_or_default(),
                parsed_value_available,
                parsed_model_value,
                unavailable_reason: (!parsed_value_available).then(|| {
                    if evaluation.is_none() {
                        "terminal_answer_not_evaluated".into()
                    } else if evaluation.is_some_and(|evaluation| {
                        evaluation.status == AnswerContractStatus::Invalid
                    }) {
                        "model_answer_invalid".into()
                    } else {
                        "parsed_model_value_unavailable".into()
                    }
                }),
            }
        });

        let changes = match self.change_set(project_id, run_id)? {
            Some(snapshot) => {
                let complete = snapshot.files.len() <= FINAL_RESULT_MAX_REFERENCE_ITEMS;
                let items = snapshot
                    .files
                    .into_iter()
                    .take(FINAL_RESULT_MAX_REFERENCE_ITEMS)
                    .map(|change| FinalChangeReference {
                        id: change.id,
                        change_set_id: change.change_set_id,
                        run_id: change.run_id,
                        revision: change.revision,
                        relative_path: change.relative_path,
                        kind: change.kind,
                        origin: change.origin,
                    })
                    .collect();
                FinalReferenceSet::available(items, complete)
            }
            None => FinalReferenceSet::available(Vec::new(), true),
        };
        let changes_complete = changes.complete;
        let stored_verification =
            self.verification_evidence(project_id, Some(run_id), FINAL_RESULT_MAX_REFERENCE_ITEMS)?;
        let verification_complete = stored_verification.len() < FINAL_RESULT_MAX_REFERENCE_ITEMS;
        let verification = FinalReferenceSet::available(
            stored_verification
                .into_iter()
                .take(FINAL_RESULT_MAX_REFERENCE_ITEMS)
                .map(|evidence| FinalVerificationReference {
                    id: evidence.id,
                    run_id: evidence.run_id,
                    runner: evidence.runner,
                    state: evidence.state,
                    revision_before: evidence.revision_before,
                    revision_after: evidence.revision_after,
                    definition_sha256: evidence.definition_sha256,
                })
                .collect(),
            verification_complete,
        );
        let verification_references_complete = verification.complete;

        let usage_complete = usage.len() < FINAL_RESULT_MAX_USAGE_ITEMS;
        usage.truncate(FINAL_RESULT_MAX_USAGE_ITEMS);
        let accounting_complete = accounting.len() < FINAL_RESULT_MAX_USAGE_ITEMS;
        accounting.truncate(FINAL_RESULT_MAX_USAGE_ITEMS);
        let result = FinalRunResult {
            schema_version: FINAL_RESULT_SCHEMA_VERSION,
            is_final: run_state.is_final(),
            run_state,
            correlation: FinalRunCorrelation {
                project_id: Some(project_id),
                session_id: Some(session_id),
                task_id: Some(task.id),
                run_id: Some(run_id),
            },
            answer,
            answer_contract,
            failure,
            changes,
            verification,
            supplemental_checks,
            usage: FinalUsage {
                reported: if usage.is_empty() {
                    FinalReferenceSet::unavailable("provider_usage_unavailable")
                } else {
                    FinalReferenceSet::available(usage, usage_complete)
                },
                client_accounting: if accounting.is_empty() {
                    FinalReferenceSet::unavailable("client_accounting_unavailable")
                } else {
                    FinalReferenceSet::available(accounting, accounting_complete)
                },
            },
            completeness: FinalResultCompleteness {
                event_history_complete: !events.truncated,
                conversation_answer_complete: answer_complete,
                bounded_references_complete: changes_complete && verification_references_complete,
            },
        };
        Ok(Some(result))
    }

    pub fn resolve_workspace(
        &self,
        workspace_root: impl AsRef<Path>,
    ) -> Result<WorkspaceResolution, StoreError> {
        self.resolve_workspace_with_default_provider(
            workspace_root,
            &ProviderProfile::default_llama_cpp(),
        )
    }

    /// Resolve an existing workspace without creating or repairing any row.
    pub fn resolve_existing_workspace(
        &self,
        workspace_root: impl AsRef<Path>,
    ) -> Result<WorkspaceResolution, StoreError> {
        let (workspace_root, workspace_key) = canonical_workspace(workspace_root.as_ref())?;
        let connection = self
            .connection
            .lock()
            .map_err(|_| StoreError::LockPoisoned)?;
        let payload = connection
            .query_row(
                "SELECT payload FROM projects WHERE workspace_key = ?1",
                [&workspace_key],
                |row| row.get::<_, String>(0),
            )
            .optional()?
            .ok_or_else(|| StoreError::InvalidRecord("workspace is not initialized".into()))?;
        let project = serde_json::from_str::<Project>(&payload)?;
        if project.id.is_nil() || project.workspace_root != workspace_root {
            return Err(StoreError::InvalidRecord(
                "stored workspace binding is invalid".into(),
            ));
        }
        let sessions = load_sessions(&connection, project.id)?;
        let active_session_id = load_active_session_id(&connection, project.id)?;
        if sessions.is_empty()
            || active_session_id.is_none()
            || !sessions
                .iter()
                .any(|session| Some(session.id) == active_session_id)
        {
            return Err(StoreError::InvalidRecord(
                "workspace has no valid active session".into(),
            ));
        }
        Ok(WorkspaceResolution {
            workspace_key,
            snapshot: WorkspaceSnapshot {
                project,
                sessions,
                active_session_id,
            },
        })
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
        let is_new_project = stored.is_none();

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
        ensure_onboarding_snapshot(
            &transaction,
            project.id,
            is_new_project,
            default_provider,
            &workspace_root,
        )?;
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

    pub fn load_onboarding(
        &self,
        project_id: ProjectId,
    ) -> Result<Option<OnboardingSnapshot>, StoreError> {
        let connection = self
            .connection
            .lock()
            .map_err(|_| StoreError::LockPoisoned)?;
        let payload = connection
            .query_row(
                "SELECT payload FROM project_setup WHERE project_id = ?1",
                [project_id.to_string()],
                |row| row.get::<_, String>(0),
            )
            .optional()?;
        let Some(payload) = payload else {
            return Ok(None);
        };
        if payload.len() > MAX_ONBOARDING_SNAPSHOT_BYTES {
            return Err(StoreError::InvalidRecord(
                "stored onboarding snapshot exceeds the size limit".into(),
            ));
        }
        let snapshot = serde_json::from_str::<OnboardingSnapshot>(&payload)?
            .normalized()
            .map_err(|error| StoreError::InvalidRecord(error.to_string()))?;
        Ok(Some(snapshot))
    }

    pub fn save_onboarding(
        &self,
        project_id: ProjectId,
        snapshot: &OnboardingSnapshot,
    ) -> Result<(), StoreError> {
        let snapshot = snapshot
            .clone()
            .normalized()
            .map_err(|error| StoreError::InvalidRecord(error.to_string()))?;
        let payload = serde_json::to_string(&snapshot)?;
        if payload.len() > MAX_ONBOARDING_SNAPSHOT_BYTES {
            return Err(StoreError::InvalidRecord(
                "onboarding snapshot exceeds the size limit".into(),
            ));
        }
        let mut connection = self
            .connection
            .lock()
            .map_err(|_| StoreError::LockPoisoned)?;
        let transaction = connection.transaction()?;
        ensure_project_exists(&transaction, project_id)?;
        let started_at = transaction
            .query_row(
                "SELECT started_at FROM project_setup WHERE project_id = ?1",
                [project_id.to_string()],
                |row| row.get::<_, i64>(0),
            )
            .optional()?
            .unwrap_or_else(|| now().unix_timestamp());
        let completed_at = snapshot.complete.then(|| now().unix_timestamp());
        transaction.execute(
            "INSERT INTO project_setup (
                project_id, version, started_at, completed_at, payload
             ) VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT(project_id) DO UPDATE SET
                version = excluded.version,
                completed_at = excluded.completed_at,
                payload = excluded.payload",
            params![
                project_id.to_string(),
                i64::from(snapshot.version),
                started_at,
                completed_at,
                payload,
            ],
        )?;
        transaction.commit()?;
        Ok(())
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
        let archived: bool = connection.query_row(
            "SELECT archived_at IS NOT NULL FROM sessions WHERE id = ?1 AND project_id = ?2",
            params![session_id.to_string(), project_id.to_string()],
            |row| row.get(0),
        )?;
        if archived {
            return Err(StoreError::InvalidRecord(
                "archived sessions must be unarchived before selection".into(),
            ));
        }
        connection.execute(
            "UPDATE projects SET active_session_id = ?1 WHERE id = ?2",
            params![session_id.to_string(), project_id.to_string()],
        )?;
        drop(connection);
        self.workspace_snapshot(project_id)
    }

    pub fn session_belongs_to_project(
        &self,
        project_id: ProjectId,
        session_id: SessionId,
    ) -> Result<bool, StoreError> {
        let connection = self
            .connection
            .lock()
            .map_err(|_| StoreError::LockPoisoned)?;
        Ok(connection.query_row(
            "SELECT EXISTS(
                 SELECT 1 FROM sessions WHERE id = ?1 AND project_id = ?2
             )",
            params![session_id.to_string(), project_id.to_string()],
            |row| row.get(0),
        )?)
    }

    pub fn list_sessions(
        &self,
        project_id: ProjectId,
        query: &str,
        filter: SessionArchiveFilter,
        limit: u16,
        cursor: Option<&str>,
    ) -> Result<SessionListSnapshot, StoreError> {
        validate_session_list_input(query, limit)?;
        let limit = usize::from(limit);
        let connection = self
            .connection
            .lock()
            .map_err(|_| StoreError::LockPoisoned)?;
        ensure_project_exists(&connection, project_id)?;
        let active_session_id = load_active_session_id(&connection, project_id)?;
        let cursor = cursor
            .map(|value| decode_session_cursor(value, project_id, query, filter, limit))
            .transpose()?;
        let mut sql = String::from(
            "SELECT id, title, created_at, last_activity_at, pinned_at, archived_at
             FROM sessions WHERE project_id = ?1",
        );
        let mut values = vec![Value::Text(project_id.to_string())];
        match filter {
            SessionArchiveFilter::Active => sql.push_str(" AND archived_at IS NULL"),
            SessionArchiveFilter::Archived => sql.push_str(" AND archived_at IS NOT NULL"),
            SessionArchiveFilter::All => {}
        }
        let escaped_query = escape_like_prefix(query);
        if !query.is_empty() {
            sql.push_str(" AND (title LIKE ? ESCAPE '\\' COLLATE NOCASE OR id LIKE ? ESCAPE '\\')");
            let pattern = format!("{escaped_query}%");
            values.push(Value::Text(pattern.clone()));
            values.push(Value::Text(pattern));
        }
        if let Some(cursor) = cursor {
            sql.push_str(
                " AND (
                    (CASE WHEN pinned_at IS NOT NULL THEN 1 ELSE 0 END) < ?
                    OR ((CASE WHEN pinned_at IS NOT NULL THEN 1 ELSE 0 END) = ?
                        AND last_activity_at < ?)
                    OR ((CASE WHEN pinned_at IS NOT NULL THEN 1 ELSE 0 END) = ?
                        AND last_activity_at = ? AND id > ?)
                )",
            );
            let pinned = i64::from(cursor.pinned);
            values.push(Value::Integer(pinned));
            values.push(Value::Integer(pinned));
            values.push(Value::Integer(cursor.last_activity_at));
            values.push(Value::Integer(pinned));
            values.push(Value::Integer(cursor.last_activity_at));
            values.push(Value::Text(cursor.id.to_string()));
        }
        sql.push_str(
            " ORDER BY (pinned_at IS NOT NULL) DESC, last_activity_at DESC, id ASC LIMIT ?",
        );
        values.push(Value::Integer((limit + 1) as i64));
        let mut statement = connection.prepare(&sql)?;
        let rows = statement.query_map(params_from_iter(values), |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, i64>(2)?,
                row.get::<_, i64>(3)?,
                row.get::<_, Option<i64>>(4)?,
                row.get::<_, Option<i64>>(5)?,
            ))
        })?;
        let mut loaded = Vec::with_capacity(limit + 1);
        for row in rows {
            let (id, title, created_at, last_activity_at, pinned_at, archived_at) = row?;
            let id = parse_uuid(&id, "session id")?;
            loaded.push(SessionListRow {
                id,
                title: validate_loaded_session_title(title)?,
                created_at: timestamp(created_at, "session created_at")?,
                last_activity_at: timestamp(last_activity_at, "session last_activity_at")?,
                pinned: pinned_at.is_some(),
                archived: archived_at.is_some(),
                selected: active_session_id == Some(id),
            });
        }
        let has_more = loaded.len() > limit;
        loaded.truncate(limit);
        let next_cursor = has_more
            .then(|| {
                loaded
                    .last()
                    .map(|row| encode_session_cursor(project_id, query, filter, limit, row))
            })
            .flatten();
        Ok(SessionListSnapshot {
            project_id,
            rows: loaded,
            has_more,
            next_cursor,
            query: query.to_owned(),
            filter,
            limit: limit as u16,
        })
    }

    pub fn inspect_session(
        &self,
        project_id: ProjectId,
        session_id: SessionId,
    ) -> Result<SessionListRow, StoreError> {
        let connection = self
            .connection
            .lock()
            .map_err(|_| StoreError::LockPoisoned)?;
        let active_session_id = load_active_session_id(&connection, project_id)?;
        load_session_row_on(&connection, project_id, session_id, active_session_id)
    }

    pub fn set_session_title_with_event(
        &self,
        workspace_key: &str,
        project_id: ProjectId,
        session_id: SessionId,
        title: &str,
    ) -> Result<SessionListRow, StoreError> {
        validate_session_title(title)?;
        self.mutate_session_metadata(
            workspace_key,
            project_id,
            session_id,
            Some(title),
            None,
            None,
        )
    }

    pub fn set_session_pinned_with_event(
        &self,
        workspace_key: &str,
        project_id: ProjectId,
        session_id: SessionId,
        pinned: bool,
    ) -> Result<SessionListRow, StoreError> {
        self.mutate_session_metadata(
            workspace_key,
            project_id,
            session_id,
            None,
            Some(pinned),
            None,
        )
    }

    pub fn set_session_archived_with_event(
        &self,
        workspace_key: &str,
        project_id: ProjectId,
        session_id: SessionId,
        archived: bool,
    ) -> Result<SessionListRow, StoreError> {
        self.mutate_session_metadata(
            workspace_key,
            project_id,
            session_id,
            None,
            None,
            Some(archived),
        )
    }

    pub fn session_is_archived(
        &self,
        project_id: ProjectId,
        session_id: SessionId,
    ) -> Result<bool, StoreError> {
        let connection = self
            .connection
            .lock()
            .map_err(|_| StoreError::LockPoisoned)?;
        connection
            .query_row(
                "SELECT archived_at IS NOT NULL FROM sessions WHERE id = ?1 AND project_id = ?2",
                params![session_id.to_string(), project_id.to_string()],
                |row| row.get(0),
            )
            .optional()?
            .ok_or_else(|| {
                StoreError::InvalidRecord("session does not belong to the current project".into())
            })
    }

    fn mutate_session_metadata(
        &self,
        workspace_key: &str,
        project_id: ProjectId,
        session_id: SessionId,
        title: Option<&str>,
        pinned: Option<bool>,
        archived: Option<bool>,
    ) -> Result<SessionListRow, StoreError> {
        let mut connection = self
            .connection
            .lock()
            .map_err(|_| StoreError::LockPoisoned)?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let (stored_title, pinned_at, archived_at) = transaction
            .query_row(
                "SELECT title, pinned_at, archived_at FROM sessions
                 WHERE id = ?1 AND project_id = ?2
                   AND EXISTS(SELECT 1 FROM projects WHERE id = ?2 AND workspace_key = ?3)",
                params![
                    session_id.to_string(),
                    project_id.to_string(),
                    workspace_key
                ],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, Option<i64>>(1)?,
                        row.get::<_, Option<i64>>(2)?,
                    ))
                },
            )
            .optional()?
            .ok_or_else(|| {
                StoreError::InvalidRecord("session does not belong to the current project".into())
            })?;
        if archived == Some(true) {
            let selected = load_active_session_id(&transaction, project_id)? == Some(session_id);
            if selected {
                return Err(StoreError::InvalidRecord(
                    "cannot archive the selected session; select another idle session first".into(),
                ));
            }
            let busy: bool = transaction.query_row(
                "SELECT EXISTS(
                    SELECT 1 FROM runtime_run_ownership
                    WHERE project_id = ?1 AND session_id = ?2 AND state = 'active'
                ) OR EXISTS(
                    SELECT 1 FROM command_identities
                    WHERE project_id = ?1 AND session_id = ?2
                      AND command_kind NOT IN (
                          'set_session_title', 'set_session_pinned',
                          'set_session_archived', 'select_session'
                      )
                      AND state IN ('accepted', 'executing')
                ) OR EXISTS(
                    SELECT 1 FROM goal_invocations
                    WHERE project_id = ?1 AND session_id = ?2
                      AND state IN ('active', 'review_required', 'unresolved')
                )",
                params![project_id.to_string(), session_id.to_string()],
                |row| row.get(0),
            )?;
            if busy {
                return Err(StoreError::InvalidRecord(
                    "cannot archive a session with admitted or pending work; stop or resolve it first".into(),
                ));
            }
        }
        let next_title = title.unwrap_or(&stored_title);
        let next_pinned = pinned.unwrap_or(pinned_at.is_some());
        let next_archived = archived.unwrap_or(archived_at.is_some());
        let at = now().unix_timestamp();
        let next_pinned_at = next_pinned.then_some(pinned_at.unwrap_or(at));
        let next_archived_at = next_archived.then_some(archived_at.unwrap_or(at));
        let payload = transaction.query_row(
            "SELECT payload FROM sessions WHERE id = ?1 AND project_id = ?2",
            params![session_id.to_string(), project_id.to_string()],
            |row| row.get::<_, String>(0),
        )?;
        let mut session = serde_json::from_str::<Session>(&payload)?;
        session.title = next_title.to_owned();
        transaction.execute(
            "UPDATE sessions SET title = ?1, pinned_at = ?2, archived_at = ?3, payload = ?4
             WHERE id = ?5 AND project_id = ?6",
            params![
                next_title,
                next_pinned_at,
                next_archived_at,
                serde_json::to_string(&session)?,
                session_id.to_string(),
                project_id.to_string(),
            ],
        )?;
        append_event_on(
            &transaction,
            Some(workspace_key),
            Some(project_id),
            Some(session_id),
            None,
            &AppEvent::SessionMetadataChanged {
                session_id,
                title: next_title.to_owned(),
                pinned: next_pinned,
                archived: next_archived,
            },
        )?;
        let active_session_id = load_active_session_id(&transaction, project_id)?;
        let row = load_session_row_on(&transaction, project_id, session_id, active_session_id)?;
        transaction.commit()?;
        Ok(row)
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
        let mut connection = self
            .connection
            .lock()
            .map_err(|_| StoreError::LockPoisoned)?;
        let transaction =
            connection.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        save_goal_on(&transaction, goal, None)?;
        transaction.commit()?;
        Ok(())
    }

    pub fn save_goal_if_current(
        &self,
        goal: &Goal,
        expected: &Option<Goal>,
    ) -> Result<(), StoreError> {
        let mut connection = self
            .connection
            .lock()
            .map_err(|_| StoreError::LockPoisoned)?;
        let transaction =
            connection.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        save_goal_on(&transaction, goal, Some(expected))?;
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
        let transaction =
            connection.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        let cleared = clear_goal_on(&transaction, project_id, session_id, None)?;
        transaction.commit()?;
        Ok(cleared)
    }

    pub fn clear_goal_if_current(
        &self,
        project_id: ProjectId,
        session_id: SessionId,
        expected: &Option<Goal>,
    ) -> Result<bool, StoreError> {
        let mut connection = self
            .connection
            .lock()
            .map_err(|_| StoreError::LockPoisoned)?;
        let transaction =
            connection.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        let cleared = clear_goal_on(&transaction, project_id, session_id, Some(expected))?;
        transaction.commit()?;
        Ok(cleared)
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
        connection.execute(
            "UPDATE sessions SET last_activity_at = ?1
             WHERE id = ?2 AND project_id = ?3",
            params![
                message.created_at.unix_timestamp(),
                message.session_id.to_string(),
                message.project_id.to_string(),
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
        let fork = session_forks::load_session_fork_on(&connection, project_id, session_id)?;
        let mut messages = Vec::new();
        if let Some(fork) = &fork {
            for source in &fork.sources {
                match source {
                    argentum_domain::SessionForkSource::LegacyMessage { message_id } => {
                        let row = connection
                            .query_row(
                                "SELECT id, run_id, role, status, created_at, text,
                                        reasoning, usage, profile_id, model
                                 FROM conversation_messages
                                 WHERE id = ?1 AND project_id = ?2 AND session_id = ?3",
                                params![
                                    message_id.to_string(),
                                    project_id.to_string(),
                                    fork.source_session_id.to_string()
                                ],
                                conversation_message_tuple,
                            )
                            .optional()?
                            .ok_or_else(|| {
                                StoreError::InvalidRecord(
                                    "fork source conversation is no longer available".into(),
                                )
                            })?;
                        let mut message = conversation_message_from_tuple(
                            project_id,
                            fork.source_session_id,
                            row,
                        )?;
                        message.text = format!(
                            "[Inherited from session {}, run {} at the settled fork cutoff. Historical only.]\n\n{}",
                            fork.source_session_id, message.run_id, message.text
                        );
                        messages.push(message);
                    }
                    argentum_domain::SessionForkSource::ContextGroup { run_id, ordinal } => {
                        let (payload, created_at) = connection
                            .query_row(
                                "SELECT g.payload, c.created_at
                                 FROM context_groups g
                                 JOIN context_runs r ON r.run_id = g.run_id
                                 JOIN conversation_messages c ON c.id = r.user_message_id
                                 WHERE r.project_id = ?1 AND r.session_id = ?2
                                   AND r.run_id = ?3 AND g.ordinal = ?4",
                                params![
                                    project_id.to_string(),
                                    fork.source_session_id.to_string(),
                                    run_id.to_string(),
                                    ordinal
                                ],
                                |row| Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?)),
                            )
                            .optional()?
                            .ok_or_else(|| {
                                StoreError::InvalidRecord(
                                    "fork source context group is no longer available".into(),
                                )
                            })?;
                        let group: ContextGroup = serde_json::from_str(&payload)?;
                        if group.validate().is_err()
                            || group.project_id != project_id
                            || group.session_id != fork.source_session_id
                            || group.run_id != *run_id
                            || group.ordinal != *ordinal
                        {
                            return Err(StoreError::InvalidRecord(
                                "fork source context group is invalid".into(),
                            ));
                        }
                        let (role, body) = match &group.body {
                            GroupBody::User { text } => (ConversationRole::User, text.clone()),
                            GroupBody::Steering {
                                entry_id,
                                position,
                                text,
                            } => (
                                ConversationRole::User,
                                format!(
                                    "Historical steering entry {entry_id} at position {position}. It grants no authority in this fork.\n\n{text}"
                                ),
                            ),
                            GroupBody::Assistant { text } => {
                                (ConversationRole::Assistant, text.clone())
                            }
                            GroupBody::AssistantOmitted { retention } => (
                                ConversationRole::Assistant,
                                format!(
                                    "Original assistant body was explicitly unavailable: {retention:?}."
                                ),
                            ),
                            GroupBody::Exchange {
                                text,
                                text_retention,
                                observations,
                            } => (
                                ConversationRole::Assistant,
                                format!(
                                    "{text}\n\nHistorical tool exchange. Assistant retention: {text_retention:?}. No tool is being executed.\n{}",
                                    serde_json::to_string_pretty(observations)?
                                ),
                            ),
                        };
                        let mut digest = Sha256::new();
                        digest.update(fork.child_session_id.as_bytes());
                        digest.update(run_id.as_bytes());
                        digest.update(ordinal.to_le_bytes());
                        let digest = digest.finalize();
                        let mut id = [0_u8; 16];
                        id.copy_from_slice(&digest[..16]);
                        let assistant = role == ConversationRole::Assistant;
                        messages.push(ConversationMessage {
                            id: MessageId::from_bytes(id),
                            project_id,
                            session_id: fork.source_session_id,
                            run_id: *run_id,
                            role,
                            text: format!(
                                "[Inherited from session {}, run {}, group {} at the settled fork cutoff. Historical only.]\n\n{}",
                                fork.source_session_id, run_id, ordinal, body
                            ),
                            reasoning: String::new(),
                            usage: None,
                            profile_id: if assistant {
                                "historical".into()
                            } else {
                                String::new()
                            },
                            model: if assistant {
                                "historical".into()
                            } else {
                                String::new()
                            },
                            status: ConversationMessageStatus::Complete,
                            created_at: time::OffsetDateTime::from_unix_timestamp(created_at)
                                .map_err(|_| {
                                    StoreError::InvalidRecord(
                                        "fork source timestamp is invalid".into(),
                                    )
                                })?,
                        });
                    }
                }
            }
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
        let mut direct = Vec::new();
        for row in rows {
            direct.push(conversation_message_from_tuple(
                project_id, session_id, row?,
            )?);
        }
        direct.reverse();
        messages.extend(direct);
        let mut selected = Vec::new();
        let mut snapshot_bytes = 0usize;
        for message in messages.into_iter().rev() {
            let next_bytes = snapshot_bytes
                .saturating_add(message.text.len())
                .saturating_add(message.reasoning.len());
            if selected.len() >= MAX_CONVERSATION_SNAPSHOT_MESSAGES
                || next_bytes > MAX_CONVERSATION_SNAPSHOT_BYTES
            {
                break;
            }
            snapshot_bytes = next_bytes;
            selected.push(message);
        }
        selected.reverse();
        Ok(ConversationSnapshot {
            project_id,
            session_id,
            messages: selected,
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
             SET updated_at = ?1, title = ?2, payload = ?3
             WHERE id = ?4 AND project_id = ?5",
            params![
                session.updated_at.unix_timestamp(),
                session.title,
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
        transaction.execute(
            "DELETE FROM model_budgets WHERE project_id=?1 AND profile_id=?2 AND EXISTS (
                SELECT 1 FROM provider_profiles WHERE project_id=?1 AND profile_id=?2
                AND (provider_kind<>?3 OR endpoint<>?4 OR model<>?5))",
            params![
                project_id.to_string(),
                profile.id,
                provider_kind_name(profile.kind),
                profile.endpoint,
                profile.model
            ],
        )?;
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

    /// Add a preset if absent and select it without overwriting saved edits.
    pub fn add_provider_preset(
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
        transaction.execute(
            "INSERT INTO provider_profiles
             (project_id, profile_id, label, provider_kind, endpoint, model, selected)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, 0)
             ON CONFLICT(project_id, profile_id) DO NOTHING",
            params![
                project_id.to_string(),
                profile.id,
                profile.label,
                provider_kind_name(profile.kind),
                profile.endpoint,
                profile.model
            ],
        )?;
        transaction.execute(
            "UPDATE provider_profiles SET selected = 0 WHERE project_id = ?1",
            [project_id.to_string()],
        )?;
        transaction.execute(
            "UPDATE provider_profiles SET selected = 1 WHERE project_id = ?1 AND profile_id = ?2",
            params![project_id.to_string(), profile.id],
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
        transaction.execute(
            "DELETE FROM model_budgets WHERE project_id=?1 AND profile_id=?2 AND EXISTS (
                SELECT 1 FROM provider_profiles WHERE project_id=?1 AND profile_id=?2 AND model<>?3)",
            params![project_id.to_string(), provider_id, model],
        )?;
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

    pub fn save_module_preferences(
        &self,
        project_id: ProjectId,
        preferences: &BTreeMap<String, argentum_domain::ModulePreference>,
    ) -> Result<(), StoreError> {
        validate_module_preferences(preferences)?;
        let payload = serde_json::to_string(preferences)?;
        let connection = self
            .connection
            .lock()
            .map_err(|_| StoreError::LockPoisoned)?;
        connection.execute(
            "INSERT INTO module_preferences (project_id, payload) VALUES (?1, ?2)
             ON CONFLICT(project_id) DO UPDATE SET payload = excluded.payload",
            params![project_id.to_string(), payload],
        )?;
        Ok(())
    }

    pub fn load_module_preferences(
        &self,
        project_id: ProjectId,
    ) -> Result<Option<BTreeMap<String, argentum_domain::ModulePreference>>, StoreError> {
        let connection = self
            .connection
            .lock()
            .map_err(|_| StoreError::LockPoisoned)?;
        let payload: Option<String> = connection
            .query_row(
                "SELECT payload FROM module_preferences WHERE project_id = ?1",
                [project_id.to_string()],
                |row| row.get(0),
            )
            .optional()?;
        let Some(payload) = payload else {
            return Ok(None);
        };
        if payload.len() > 32 * 1024 {
            return Err(StoreError::InvalidRecord(
                "module preferences exceed size limit".into(),
            ));
        }
        let preferences = serde_json::from_str(&payload)?;
        validate_module_preferences(&preferences)?;
        Ok(Some(preferences))
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

    pub fn save_approval_grant(
        &self,
        project_id: ProjectId,
        grant: &ApprovalGrantRecord,
    ) -> Result<(), StoreError> {
        validate_approval_grant(project_id, grant)?;
        let payload = serde_json::to_string(grant)?;
        let connection = self
            .connection
            .lock()
            .map_err(|_| StoreError::LockPoisoned)?;
        connection.execute(
            "INSERT INTO approval_grants (
                 id, project_id, scope, capability, created_at, revoked_at, payload
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
             ON CONFLICT(id) DO NOTHING",
            params![
                grant.id.to_string(),
                project_id.to_string(),
                grant_scope_name(grant.scope),
                format!("{:?}", grant.capability),
                grant.created_at.unix_timestamp(),
                grant.revoked_at.map(|value| value.unix_timestamp()),
                payload,
            ],
        )?;
        Ok(())
    }

    pub fn approval_grants(
        &self,
        project_id: ProjectId,
        limit: usize,
    ) -> Result<Vec<ApprovalGrantRecord>, StoreError> {
        validate_record_limit(limit)?;
        let connection = self
            .connection
            .lock()
            .map_err(|_| StoreError::LockPoisoned)?;
        let mut statement = connection.prepare(
            "SELECT payload
             FROM approval_grants
             WHERE project_id = ?1
             ORDER BY created_at ASC, id ASC
             LIMIT ?2",
        )?;
        let payloads = statement.query_map(
            params![
                project_id.to_string(),
                i64::try_from(limit).unwrap_or(i64::MAX)
            ],
            |row| row.get::<_, String>(0),
        )?;
        let mut grants = Vec::new();
        for payload in payloads {
            let payload = payload?;
            if payload.len() > 64 * 1024 {
                return Err(StoreError::InvalidRecord(
                    "stored approval grant exceeds the size limit".into(),
                ));
            }
            let grant = serde_json::from_str::<ApprovalGrantRecord>(&payload)?;
            validate_approval_grant(project_id, &grant)?;
            grants.push(grant);
        }
        Ok(grants)
    }

    pub fn revoke_approval_grant(
        &self,
        project_id: ProjectId,
        grant_id: GrantId,
        revoked_at: OffsetDateTime,
    ) -> Result<Option<ApprovalGrantRecord>, StoreError> {
        let mut connection = self
            .connection
            .lock()
            .map_err(|_| StoreError::LockPoisoned)?;
        let transaction = connection.transaction()?;
        let payload = transaction
            .query_row(
                "SELECT payload FROM approval_grants WHERE id = ?1 AND project_id = ?2",
                params![grant_id.to_string(), project_id.to_string()],
                |row| row.get::<_, String>(0),
            )
            .optional()?;
        let Some(payload) = payload else {
            return Ok(None);
        };
        let mut grant = serde_json::from_str::<ApprovalGrantRecord>(&payload)?;
        validate_approval_grant(project_id, &grant)?;
        if revoked_at < grant.created_at {
            return Err(StoreError::InvalidRecord(
                "approval grant revocation precedes creation".into(),
            ));
        }
        if grant.revoked_at.is_none() {
            grant.revoked_at = Some(revoked_at);
            let payload = serde_json::to_string(&grant)?;
            transaction.execute(
                "UPDATE approval_grants SET revoked_at = ?3, payload = ?4
                 WHERE id = ?1 AND project_id = ?2",
                params![
                    grant_id.to_string(),
                    project_id.to_string(),
                    revoked_at.unix_timestamp(),
                    payload,
                ],
            )?;
        }
        transaction.commit()?;
        Ok(Some(grant))
    }

    pub fn record_execution_audit(&self, record: &ExecutionAuditRecord) -> Result<(), StoreError> {
        validate_execution_audit(record)?;
        let payload = serde_json::to_string(record)?;
        let connection = self
            .connection
            .lock()
            .map_err(|_| StoreError::LockPoisoned)?;
        connection.execute(
            "INSERT INTO execution_audit (
                 id, project_id, session_id, run_id, tool_call_id, recorded_at, payload
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                record.id.to_string(),
                record.project_id.to_string(),
                record.session_id.to_string(),
                record.run_id.to_string(),
                record.tool_call_id.to_string(),
                record.finished_at.unix_timestamp(),
                payload,
            ],
        )?;
        Ok(())
    }

    pub fn execution_audit(
        &self,
        project_id: ProjectId,
        run_id: Option<RunId>,
        limit: usize,
    ) -> Result<Vec<ExecutionAuditRecord>, StoreError> {
        validate_record_limit(limit)?;
        let connection = self
            .connection
            .lock()
            .map_err(|_| StoreError::LockPoisoned)?;
        let sql = if run_id.is_some() {
            "SELECT payload FROM execution_audit
             WHERE project_id = ?1 AND run_id = ?2
             ORDER BY recorded_at ASC, rowid ASC LIMIT ?3"
        } else {
            "SELECT payload FROM execution_audit
             WHERE project_id = ?1 AND ?2 IS NULL
             ORDER BY recorded_at ASC, rowid ASC LIMIT ?3"
        };
        let mut statement = connection.prepare(sql)?;
        let run_value = run_id.map(|value| value.to_string());
        let payloads = statement.query_map(
            params![
                project_id.to_string(),
                run_value,
                i64::try_from(limit).unwrap_or(i64::MAX)
            ],
            |row| row.get::<_, String>(0),
        )?;
        let mut records = Vec::new();
        for payload in payloads {
            let payload = payload?;
            if payload.len() > 128 * 1024 {
                return Err(StoreError::InvalidRecord(
                    "stored execution audit exceeds the size limit".into(),
                ));
            }
            let record = serde_json::from_str::<ExecutionAuditRecord>(&payload)?;
            validate_execution_audit(&record)?;
            if record.project_id != project_id {
                return Err(StoreError::InvalidRecord(
                    "execution audit project scope is invalid".into(),
                ));
            }
            records.push(record);
        }
        Ok(records)
    }

    pub fn save_verification_evidence(
        &self,
        project_id: ProjectId,
        session_id: SessionId,
        evidence: &VerificationEvidence,
    ) -> Result<(), StoreError> {
        validate_verification_evidence(evidence)?;
        let payload = serde_json::to_string(evidence)?;
        let connection = self
            .connection
            .lock()
            .map_err(|_| StoreError::LockPoisoned)?;
        let belongs_to_project = connection.query_row(
            "SELECT EXISTS(SELECT 1 FROM sessions WHERE id = ?1 AND project_id = ?2)",
            params![session_id.to_string(), project_id.to_string()],
            |row| row.get::<_, bool>(0),
        )?;
        if !belongs_to_project {
            return Err(StoreError::InvalidRecord(
                "verification session does not belong to project".into(),
            ));
        }
        let inserted = connection.execute(
            "INSERT INTO verification_evidence (
                 id, project_id, session_id, run_id, runner_id, recorded_at, payload
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7) ON CONFLICT(id) DO NOTHING",
            params![
                evidence.id.to_string(),
                project_id.to_string(),
                session_id.to_string(),
                evidence.run_id.to_string(),
                evidence.runner.id(),
                evidence.started_at.unix_timestamp(),
                payload,
            ],
        )?;
        if inserted == 0 {
            let stored: (String, String, Option<String>) = connection.query_row(
                "SELECT project_id, session_id,
                 CASE WHEN length(CAST(payload AS BLOB))<=131072 THEN payload ELSE NULL END
                 FROM verification_evidence WHERE id = ?1",
                [evidence.id.to_string()],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )?;
            let same_receipt = stored.0 == project_id.to_string()
                && stored.1 == session_id.to_string()
                && stored.2.as_deref().is_some_and(|payload| {
                    serde_json::from_str::<VerificationEvidence>(payload)
                        .is_ok_and(|existing| existing == *evidence)
                });
            if !same_receipt {
                return Err(StoreError::InvalidRecord(
                    "verification evidence identity conflicts with an immutable receipt".into(),
                ));
            }
        }
        Ok(())
    }

    pub fn verification_evidence(
        &self,
        project_id: ProjectId,
        run_id: Option<RunId>,
        limit: usize,
    ) -> Result<Vec<VerificationEvidence>, StoreError> {
        validate_record_limit(limit)?;
        let connection = self
            .connection
            .lock()
            .map_err(|_| StoreError::LockPoisoned)?;
        let sql = if run_id.is_some() {
            "SELECT CASE WHEN length(CAST(payload AS BLOB))<=131072 THEN payload ELSE NULL END FROM verification_evidence
             WHERE project_id = ?1 AND run_id = ?2
             ORDER BY recorded_at ASC, id ASC LIMIT ?3"
        } else {
            "SELECT CASE WHEN length(CAST(payload AS BLOB))<=131072 THEN payload ELSE NULL END FROM verification_evidence
             WHERE project_id = ?1 AND ?2 IS NULL
             ORDER BY recorded_at ASC, id ASC LIMIT ?3"
        };
        let mut statement = connection.prepare(sql)?;
        let run_value = run_id.map(|value| value.to_string());
        let payloads = statement.query_map(
            params![
                project_id.to_string(),
                run_value,
                i64::try_from(limit).unwrap_or(i64::MAX)
            ],
            |row| row.get::<_, String>(0),
        )?;
        let mut evidence = Vec::new();
        for payload in payloads {
            let payload = payload?;
            if payload.len() > 128 * 1024 {
                return Err(StoreError::InvalidRecord(
                    "stored verification evidence exceeds the size limit".into(),
                ));
            }
            let record = serde_json::from_str::<VerificationEvidence>(&payload)?;
            validate_verification_evidence(&record)?;
            evidence.push(record);
        }
        Ok(evidence)
    }

    pub fn latest_verification_evidence(
        &self,
        project_id: ProjectId,
        limit: usize,
    ) -> Result<Vec<VerificationEvidence>, StoreError> {
        validate_record_limit(limit)?;
        let connection = self
            .connection
            .lock()
            .map_err(|_| StoreError::LockPoisoned)?;
        let mut statement = connection.prepare(
            "SELECT payload FROM verification_evidence
             WHERE project_id = ?1
             ORDER BY recorded_at DESC, rowid DESC LIMIT ?2",
        )?;
        let payloads = statement.query_map(
            params![
                project_id.to_string(),
                i64::try_from(limit).unwrap_or(i64::MAX)
            ],
            |row| row.get::<_, String>(0),
        )?;
        let mut evidence = Vec::new();
        for payload in payloads {
            let payload = payload?;
            if payload.len() > 128 * 1024 {
                return Err(StoreError::InvalidRecord(
                    "stored verification evidence exceeds the size limit".into(),
                ));
            }
            let record = serde_json::from_str::<VerificationEvidence>(&payload)?;
            validate_verification_evidence(&record)?;
            evidence.push(record);
        }
        Ok(evidence)
    }

    pub fn prepare_change_operation(
        &self,
        operation: &PreparedChangeOperation,
    ) -> Result<(), StoreError> {
        validate_prepared_change_operation(operation)?;
        let mut connection = self
            .connection
            .lock()
            .map_err(|_| StoreError::LockPoisoned)?;
        let transaction = connection.transaction()?;
        let belongs_to_project = transaction.query_row(
            "SELECT EXISTS(
                SELECT 1
                FROM sessions AS sessions
                JOIN projects AS projects ON projects.id = sessions.project_id
                WHERE sessions.id = ?1
                  AND sessions.project_id = ?2
                  AND projects.workspace_key = ?3
             )",
            params![
                operation.session_id.to_string(),
                operation.project_id.to_string(),
                operation.workspace_key,
            ],
            |row| row.get::<_, bool>(0),
        )?;
        if !belongs_to_project {
            return Err(StoreError::InvalidRecord(
                "change operation workspace and session do not belong to its project".into(),
            ));
        }

        let mut unique = BTreeMap::<String, u64>::new();
        let mut statement = transaction.prepare(
            "SELECT before_digest, before_size, before_retained,
                    after_digest, after_size, after_retained
             FROM change_operations
             WHERE project_id = ?1 AND run_id = ?2",
        )?;
        let rows = statement.query_map(
            params![
                operation.project_id.to_string(),
                operation.run_id.to_string()
            ],
            |row| {
                Ok((
                    row.get::<_, Option<String>>(0)?,
                    row.get::<_, Option<u64>>(1)?,
                    row.get::<_, Option<bool>>(2)?,
                    row.get::<_, Option<String>>(3)?,
                    row.get::<_, Option<u64>>(4)?,
                    row.get::<_, Option<bool>>(5)?,
                ))
            },
        )?;
        for row in rows {
            let (
                before_digest,
                before_size,
                before_retained,
                after_digest,
                after_size,
                after_retained,
            ) = row?;
            if let (Some(digest), Some(size), Some(true)) =
                (before_digest, before_size, before_retained)
            {
                unique.entry(digest).or_insert(size);
            }
            if let (Some(digest), Some(size), Some(true)) =
                (after_digest, after_size, after_retained)
            {
                unique.entry(digest).or_insert(size);
            }
        }
        drop(statement);
        for snapshot in [&operation.before, &operation.after]
            .into_iter()
            .flatten()
            .filter(|snapshot| snapshot.retained)
        {
            unique
                .entry(snapshot.digest.0.clone())
                .or_insert(snapshot.size_bytes);
        }
        let total = unique.values().try_fold(0_u64, |total, size| {
            total
                .checked_add(*size)
                .ok_or_else(|| StoreError::InvalidRecord("snapshot size overflow".into()))
        })?;
        if total > MAX_CHANGE_RUN_SNAPSHOT_BYTES as u64 {
            return Err(StoreError::InvalidRecord(
                "change run snapshot content exceeds the size limit".into(),
            ));
        }

        insert_change_blob(
            &transaction,
            operation.project_id,
            operation.before.as_ref(),
            operation.before_bytes.as_deref(),
        )?;
        insert_change_blob(
            &transaction,
            operation.project_id,
            operation.after.as_ref(),
            operation.after_bytes.as_deref(),
        )?;
        let created_at = now();
        transaction.execute(
            "INSERT INTO change_operations (
                id, workspace_key, project_id, session_id, run_id, relative_path,
                change_kind, origin, before_digest, before_size, before_retained,
                after_digest, after_size, after_retained, additions, removals,
                restored_from, state, current_digest, file_change_id, created_at, updated_at,
                line_count_availability
             ) VALUES (
                ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11,
                ?12, ?13, ?14, ?15, ?16, ?17, 'prepared', NULL, NULL, ?18, ?18, ?19
             )",
            params![
                operation.operation_id.to_string(),
                operation.workspace_key,
                operation.project_id.to_string(),
                operation.session_id.to_string(),
                operation.run_id.to_string(),
                operation.relative_path,
                file_change_kind_name(operation.kind),
                file_change_origin_name(operation.origin),
                operation
                    .before
                    .as_ref()
                    .map(|snapshot| snapshot.digest.0.as_str()),
                operation
                    .before
                    .as_ref()
                    .map(|snapshot| snapshot.size_bytes),
                operation.before.as_ref().map(|snapshot| snapshot.retained),
                operation
                    .after
                    .as_ref()
                    .map(|snapshot| snapshot.digest.0.as_str()),
                operation.after.as_ref().map(|snapshot| snapshot.size_bytes),
                operation.after.as_ref().map(|snapshot| snapshot.retained),
                operation.additions,
                operation.removals,
                operation.restored_from.map(|id| id.to_string()),
                created_at.unix_timestamp(),
                line_count_availability_name(operation.line_count_availability),
            ],
        )?;
        transaction.commit()?;
        Ok(())
    }

    pub fn mark_change_operation_applied(
        &self,
        operation_id: ChangeOperationId,
    ) -> Result<(), StoreError> {
        let connection = self
            .connection
            .lock()
            .map_err(|_| StoreError::LockPoisoned)?;
        let changed = connection.execute(
            "UPDATE change_operations
             SET state = 'applied', updated_at = ?2
             WHERE id = ?1 AND state IN ('prepared', 'conflicted')",
            params![operation_id.to_string(), now().unix_timestamp()],
        )?;
        if changed == 1 {
            Ok(())
        } else {
            Err(StoreError::InvalidRecord(
                "change operation cannot be marked applied".into(),
            ))
        }
    }

    pub fn finalize_change_operation(
        &self,
        operation_id: ChangeOperationId,
    ) -> Result<FileChangeRecord, StoreError> {
        self.finalize_change_operation_internal(operation_id, None)
            .map(|(record, _)| record)
    }

    pub fn finalize_restore_change_operation(
        &self,
        operation_id: ChangeOperationId,
        source_file_change_id: FileChangeId,
    ) -> Result<(FileChangeRecord, RestoreResult), StoreError> {
        let (record, result) =
            self.finalize_change_operation_internal(operation_id, Some(source_file_change_id))?;
        let result = result.ok_or_else(|| {
            StoreError::InvalidRecord("restore finalization did not create its event".into())
        })?;
        Ok((record, result))
    }

    fn finalize_change_operation_internal(
        &self,
        operation_id: ChangeOperationId,
        restore_source: Option<FileChangeId>,
    ) -> Result<(FileChangeRecord, Option<RestoreResult>), StoreError> {
        let mut connection = self
            .connection
            .lock()
            .map_err(|_| StoreError::LockPoisoned)?;
        let transaction = connection.transaction()?;
        let operation = load_change_operation_on(&transaction, operation_id)?
            .ok_or_else(|| StoreError::InvalidRecord("change operation does not exist".into()))?;
        if !matches!(
            operation.state,
            ChangeRecoveryState::Prepared | ChangeRecoveryState::Applied
        ) {
            return Err(StoreError::InvalidRecord(
                "change operation cannot be finalized from its current state".into(),
            ));
        }
        if let Some(source_file_change_id) = restore_source {
            let source_exists = transaction.query_row(
                "SELECT EXISTS(
                    SELECT 1 FROM file_changes
                    WHERE id = ?1 AND project_id = ?2 AND run_id = ?3
                 )",
                params![
                    source_file_change_id.to_string(),
                    operation.project_id.to_string(),
                    operation.run_id.to_string(),
                ],
                |row| row.get::<_, bool>(0),
            )?;
            if operation.origin != FileChangeOrigin::Restore
                || operation.restored_from != Some(source_file_change_id)
                || !source_exists
            {
                return Err(StoreError::InvalidRecord(
                    "restore operation is not bound to its source file change".into(),
                ));
            }
        }
        let existing = transaction
            .query_row(
                "SELECT id, revision FROM change_sets
                 WHERE project_id = ?1 AND run_id = ?2",
                params![
                    operation.project_id.to_string(),
                    operation.run_id.to_string()
                ],
                |row| Ok((row.get::<_, String>(0)?, row.get::<_, u64>(1)?)),
            )
            .optional()?;
        let had_existing = existing.is_some();
        let (change_set_id, revision) = match existing {
            Some((id, revision)) => (
                parse_uuid(&id, "change set id")?,
                revision
                    .checked_add(1)
                    .ok_or_else(|| StoreError::InvalidRecord("change revision overflow".into()))?,
            ),
            None => (ChangeSetId::new_v4(), 1),
        };
        if had_existing {
            transaction.execute(
                "UPDATE change_sets SET revision = ?1 WHERE id = ?2",
                params![revision, change_set_id.to_string()],
            )?;
        } else {
            transaction.execute(
                "INSERT INTO change_sets (
                    id, project_id, session_id, run_id, revision, created_at
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![
                    change_set_id.to_string(),
                    operation.project_id.to_string(),
                    operation.session_id.to_string(),
                    operation.run_id.to_string(),
                    revision,
                    operation.created_at.unix_timestamp(),
                ],
            )?;
        }
        let record = FileChangeRecord {
            id: FileChangeId::new_v4(),
            change_set_id,
            project_id: operation.project_id,
            session_id: operation.session_id,
            run_id: operation.run_id,
            revision,
            relative_path: operation.relative_path.clone(),
            kind: operation.kind,
            origin: operation.origin,
            before: operation.before.clone(),
            after: operation.after.clone(),
            additions: operation.additions,
            removals: operation.removals,
            line_count_availability: operation.line_count_availability,
            restored_from: operation.restored_from,
            created_at: now(),
        };
        transaction.execute(
            "INSERT INTO file_changes (
                id, change_set_id, project_id, session_id, run_id, revision,
                relative_path, payload, created_at
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![
                record.id.to_string(),
                record.change_set_id.to_string(),
                record.project_id.to_string(),
                record.session_id.to_string(),
                record.run_id.to_string(),
                record.revision,
                record.relative_path,
                serde_json::to_string(&record)?,
                record.created_at.unix_timestamp(),
            ],
        )?;
        transaction.execute(
            "UPDATE change_operations
             SET state = 'finalized', file_change_id = ?2, updated_at = ?3
             WHERE id = ?1",
            params![
                operation_id.to_string(),
                record.id.to_string(),
                now().unix_timestamp(),
            ],
        )?;
        append_event_on(
            &transaction,
            Some(&operation.workspace_key),
            Some(operation.project_id),
            Some(operation.session_id),
            Some(operation.run_id),
            &AppEvent::FileChangeRecorded(record.clone()),
        )?;
        let restore_result = restore_source.map(|source_file_change_id| RestoreResult {
            run_id: record.run_id,
            source_file_change_id,
            restore_file_change_id: record.id,
            relative_path: record.relative_path.clone(),
            deleted: record.kind == FileChangeKind::Deleted,
        });
        if let Some(result) = restore_result.as_ref() {
            append_event_on(
                &transaction,
                Some(&operation.workspace_key),
                Some(operation.project_id),
                Some(operation.session_id),
                Some(operation.run_id),
                &AppEvent::RestoreCompleted(result.clone()),
            )?;
        }
        transaction.commit()?;
        Ok((record, restore_result))
    }

    pub fn change_set(
        &self,
        project_id: ProjectId,
        run_id: RunId,
    ) -> Result<Option<ChangeSetSnapshot>, StoreError> {
        let connection = self
            .connection
            .lock()
            .map_err(|_| StoreError::LockPoisoned)?;
        let set = connection
            .query_row(
                "SELECT id, session_id, revision FROM change_sets
                 WHERE project_id = ?1 AND run_id = ?2",
                params![project_id.to_string(), run_id.to_string()],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, u64>(2)?,
                    ))
                },
            )
            .optional()?;
        let Some((id, session_id, revision)) = set else {
            return Ok(None);
        };
        let id = parse_uuid(&id, "change set id")?;
        let session_id = parse_uuid(&session_id, "change set session id")?;
        let mut statement = connection.prepare(
            "SELECT payload FROM file_changes
             WHERE change_set_id = ?1 AND project_id = ?2
             ORDER BY revision ASC, created_at ASC, id ASC",
        )?;
        let rows = statement.query_map(params![id.to_string(), project_id.to_string()], |row| {
            row.get::<_, String>(0)
        })?;
        let mut files = Vec::new();
        for row in rows {
            let record = serde_json::from_str::<FileChangeRecord>(&row?)?;
            if record.project_id != project_id
                || record.run_id != run_id
                || record.change_set_id != id
            {
                return Err(StoreError::InvalidRecord(
                    "stored file change has inconsistent scope".into(),
                ));
            }
            files.push(record);
        }
        let additions = files.iter().fold(0_u32, |total, record| {
            total.saturating_add(record.additions)
        });
        let removals = files
            .iter()
            .fold(0_u32, |total, record| total.saturating_add(record.removals));
        Ok(Some(ChangeSetSnapshot {
            id,
            project_id,
            session_id,
            run_id,
            revision,
            files,
            additions,
            removals,
        }))
    }

    pub fn file_change(
        &self,
        project_id: ProjectId,
        file_change_id: FileChangeId,
    ) -> Result<Option<FileChangeRecord>, StoreError> {
        let connection = self
            .connection
            .lock()
            .map_err(|_| StoreError::LockPoisoned)?;
        let payload = connection
            .query_row(
                "SELECT CASE WHEN length(CAST(payload AS BLOB))<=16384 THEN payload ELSE NULL END FROM file_changes WHERE id = ?1 AND project_id = ?2",
                params![file_change_id.to_string(), project_id.to_string()],
                |row| row.get::<_, Option<String>>(0),
            )
            .optional()?.flatten();
        payload
            .map(|payload| serde_json::from_str(&payload).map_err(StoreError::from))
            .transpose()
    }

    pub fn change_blob(
        &self,
        project_id: ProjectId,
        digest: &Sha256Digest,
    ) -> Result<Option<Vec<u8>>, StoreError> {
        let connection = self
            .connection
            .lock()
            .map_err(|_| StoreError::LockPoisoned)?;
        connection
            .query_row(
                "SELECT bytes FROM change_blobs WHERE project_id = ?1 AND digest = ?2",
                params![project_id.to_string(), digest.0],
                |row| row.get::<_, Vec<u8>>(0),
            )
            .optional()
            .map_err(StoreError::from)
    }

    pub fn unfinished_change_operations(
        &self,
        project_id: ProjectId,
    ) -> Result<Vec<StoredChangeOperation>, StoreError> {
        let connection = self
            .connection
            .lock()
            .map_err(|_| StoreError::LockPoisoned)?;
        let mut statement = connection.prepare(
            "SELECT id FROM change_operations
             WHERE project_id = ?1 AND state IN ('prepared', 'applied', 'conflicted')
             ORDER BY created_at ASC, id ASC",
        )?;
        let ids = statement.query_map([project_id.to_string()], |row| row.get::<_, String>(0))?;
        let mut operations = Vec::new();
        for id in ids {
            let id = parse_uuid(&id?, "change operation id")?;
            operations.push(
                load_change_operation_on(&connection, id)?.ok_or_else(|| {
                    StoreError::InvalidRecord("change operation disappeared".into())
                })?,
            );
        }
        Ok(operations)
    }

    pub fn change_operation(
        &self,
        project_id: ProjectId,
        operation_id: ChangeOperationId,
    ) -> Result<Option<StoredChangeOperation>, StoreError> {
        let connection = self
            .connection
            .lock()
            .map_err(|_| StoreError::LockPoisoned)?;
        let operation = load_change_operation_on(&connection, operation_id)?;
        Ok(operation.filter(|operation| operation.project_id == project_id))
    }

    pub fn resolve_change_operation(
        &self,
        operation_id: ChangeOperationId,
        state: ChangeRecoveryState,
        current_digest: Option<&Sha256Digest>,
    ) -> Result<(), StoreError> {
        if !matches!(
            state,
            ChangeRecoveryState::NotApplied | ChangeRecoveryState::Conflicted
        ) {
            return Err(StoreError::InvalidRecord(
                "unsupported recovery resolution state".into(),
            ));
        }
        let mut connection = self
            .connection
            .lock()
            .map_err(|_| StoreError::LockPoisoned)?;
        let transaction = connection.transaction()?;
        let operation = load_change_operation_on(&transaction, operation_id)?
            .ok_or_else(|| StoreError::InvalidRecord("change operation does not exist".into()))?;
        transaction.execute(
            "UPDATE change_operations
             SET state = ?2, current_digest = ?3, updated_at = ?4
             WHERE id = ?1",
            params![
                operation_id.to_string(),
                change_recovery_state_name(state),
                current_digest.map(|digest| digest.0.as_str()),
                now().unix_timestamp(),
            ],
        )?;
        if state == ChangeRecoveryState::NotApplied {
            append_event_on(
                &transaction,
                Some(&operation.workspace_key),
                Some(operation.project_id),
                Some(operation.session_id),
                Some(operation.run_id),
                &AppEvent::ChangeRecoveryResolved {
                    operation_id,
                    state,
                },
            )?;
        }
        transaction.commit()?;
        Ok(())
    }

    fn migrate(&self) -> Result<(), StoreError> {
        let connection = self
            .connection
            .lock()
            .map_err(|_| StoreError::LockPoisoned)?;
        connection.execute_batch(
            "PRAGMA journal_mode = WAL;
             PRAGMA foreign_keys = ON;",
        )?;
        // A fresh database otherwise commits every CREATE statement on its own.
        // Keep the base schema and its additive columns atomic and fsync once.
        let schema = connection.unchecked_transaction()?;
        schema.execute_batch(
            "CREATE TABLE IF NOT EXISTS event_log (
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
             CREATE TABLE IF NOT EXISTS module_preferences (
                 project_id TEXT PRIMARY KEY,
                 payload TEXT NOT NULL
             );
              CREATE TABLE IF NOT EXISTS projects (
                  id TEXT PRIMARY KEY,
                  workspace_key TEXT NOT NULL UNIQUE,
                  payload TEXT NOT NULL,
                  active_session_id TEXT
              );
              CREATE TABLE IF NOT EXISTS project_setup (
                  project_id TEXT PRIMARY KEY,
                  version INTEGER NOT NULL,
                  started_at INTEGER NOT NULL,
                  completed_at INTEGER,
                  payload TEXT NOT NULL,
                  FOREIGN KEY(project_id) REFERENCES projects(id) ON DELETE CASCADE
              );
             CREATE TABLE IF NOT EXISTS sessions (
                 id TEXT PRIMARY KEY,
                 project_id TEXT NOT NULL,
                 created_at INTEGER NOT NULL,
                 updated_at INTEGER NOT NULL,
                 last_activity_at INTEGER NOT NULL DEFAULT 0,
                 pinned_at INTEGER,
                 archived_at INTEGER,
                 title TEXT NOT NULL DEFAULT 'New session',
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
             );
             CREATE TABLE IF NOT EXISTS change_sets (
                 id TEXT PRIMARY KEY,
                 project_id TEXT NOT NULL,
                 session_id TEXT NOT NULL,
                 run_id TEXT NOT NULL,
                 revision INTEGER NOT NULL,
                 created_at INTEGER NOT NULL,
                 UNIQUE(project_id, run_id),
                 FOREIGN KEY(project_id) REFERENCES projects(id) ON DELETE CASCADE,
                 FOREIGN KEY(session_id) REFERENCES sessions(id) ON DELETE CASCADE
             );
             CREATE TABLE IF NOT EXISTS file_changes (
                 id TEXT PRIMARY KEY,
                 change_set_id TEXT NOT NULL,
                 project_id TEXT NOT NULL,
                 session_id TEXT NOT NULL,
                 run_id TEXT NOT NULL,
                 revision INTEGER NOT NULL,
                 relative_path TEXT NOT NULL,
                 payload TEXT NOT NULL,
                 created_at INTEGER NOT NULL,
                 FOREIGN KEY(change_set_id) REFERENCES change_sets(id) ON DELETE CASCADE,
                 FOREIGN KEY(project_id) REFERENCES projects(id) ON DELETE CASCADE,
                 FOREIGN KEY(session_id) REFERENCES sessions(id) ON DELETE CASCADE
             );
             CREATE TABLE IF NOT EXISTS change_blobs (
                 project_id TEXT NOT NULL,
                 digest TEXT NOT NULL,
                 size_bytes INTEGER NOT NULL CHECK(size_bytes >= 0 AND size_bytes <= 1048576),
                 bytes BLOB NOT NULL CHECK(length(bytes) <= 1048576),
                 PRIMARY KEY(project_id, digest),
                 FOREIGN KEY(project_id) REFERENCES projects(id) ON DELETE CASCADE
             );
             CREATE TABLE IF NOT EXISTS change_operations (
                 id TEXT PRIMARY KEY,
                 workspace_key TEXT NOT NULL,
                 project_id TEXT NOT NULL,
                 session_id TEXT NOT NULL,
                 run_id TEXT NOT NULL,
                 relative_path TEXT NOT NULL,
                 change_kind TEXT NOT NULL CHECK(change_kind IN ('added', 'modified', 'deleted')),
                 origin TEXT NOT NULL CHECK(origin IN ('tool_call', 'restore')),
                 before_digest TEXT,
                 before_size INTEGER,
                 before_retained INTEGER CHECK(before_retained IN (0, 1)),
                 after_digest TEXT,
                 after_size INTEGER,
                 after_retained INTEGER CHECK(after_retained IN (0, 1)),
                 additions INTEGER NOT NULL,
                 removals INTEGER NOT NULL,
                 restored_from TEXT,
                 state TEXT NOT NULL CHECK(state IN ('prepared', 'applied', 'finalized', 'not_applied', 'conflicted')),
                 current_digest TEXT,
                 file_change_id TEXT,
                 created_at INTEGER NOT NULL,
                 updated_at INTEGER NOT NULL,
                 FOREIGN KEY(project_id) REFERENCES projects(id) ON DELETE CASCADE,
                 FOREIGN KEY(session_id) REFERENCES sessions(id) ON DELETE CASCADE,
                 FOREIGN KEY(file_change_id) REFERENCES file_changes(id) ON DELETE SET NULL
             );
             CREATE TABLE IF NOT EXISTS approval_grants (
                 id TEXT PRIMARY KEY,
                 project_id TEXT NOT NULL,
                 scope TEXT NOT NULL CHECK(scope IN ('action', 'tool_call', 'run', 'session', 'workspace', 'persistent_profile')),
                 capability TEXT NOT NULL,
                 created_at INTEGER NOT NULL,
                 revoked_at INTEGER,
                 payload TEXT NOT NULL,
                 FOREIGN KEY(project_id) REFERENCES projects(id) ON DELETE CASCADE
             );
             CREATE TABLE IF NOT EXISTS execution_audit (
                 id TEXT PRIMARY KEY,
                 project_id TEXT NOT NULL,
                 session_id TEXT NOT NULL,
                 run_id TEXT NOT NULL,
                 tool_call_id TEXT NOT NULL,
                 recorded_at INTEGER NOT NULL,
                 payload TEXT NOT NULL,
                 FOREIGN KEY(project_id) REFERENCES projects(id) ON DELETE CASCADE,
                 FOREIGN KEY(session_id) REFERENCES sessions(id) ON DELETE CASCADE
             );
             CREATE TABLE IF NOT EXISTS verification_evidence (
                 id TEXT PRIMARY KEY,
                 project_id TEXT NOT NULL,
                 session_id TEXT NOT NULL,
                 run_id TEXT NOT NULL,
                 runner_id TEXT NOT NULL,
                 recorded_at INTEGER NOT NULL,
                 payload TEXT NOT NULL,
                 FOREIGN KEY(project_id) REFERENCES projects(id) ON DELETE CASCADE,
                 FOREIGN KEY(session_id) REFERENCES sessions(id) ON DELETE CASCADE
             );",
        )?;
        ensure_column(
            &schema,
            "event_log",
            "payload_version",
            "INTEGER NOT NULL DEFAULT 1",
        )?;
        ensure_column(&schema, "event_log", "workspace_key", "TEXT")?;
        ensure_column(&schema, "event_log", "project_id", "TEXT")?;
        ensure_column(&schema, "event_log", "session_id", "TEXT")?;
        ensure_column(&schema, "event_log", "run_id", "TEXT")?;
        ensure_column(
            &schema,
            "conversation_messages",
            "reasoning",
            "TEXT NOT NULL DEFAULT ''",
        )?;
        ensure_column(&schema, "conversation_messages", "usage", "TEXT")?;
        ensure_column(
            &schema,
            "conversation_messages",
            "profile_id",
            "TEXT NOT NULL DEFAULT ''",
        )?;
        ensure_column(
            &schema,
            "conversation_messages",
            "model",
            "TEXT NOT NULL DEFAULT ''",
        )?;
        ensure_column(
            &schema,
            "sessions",
            "last_activity_at",
            "INTEGER NOT NULL DEFAULT 0",
        )?;
        ensure_column(&schema, "sessions", "pinned_at", "INTEGER")?;
        ensure_column(&schema, "sessions", "archived_at", "INTEGER")?;
        ensure_column(
            &schema,
            "sessions",
            "title",
            "TEXT NOT NULL DEFAULT 'New session'",
        )?;
        schema.execute(
            "UPDATE sessions SET title = json_extract(payload, '$.title')
             WHERE json_extract(payload, '$.title') IS NOT NULL",
            [],
        )?;
        schema.execute(
            "UPDATE sessions SET last_activity_at = updated_at
             WHERE last_activity_at = 0",
            [],
        )?;
        schema.execute_batch(
            "CREATE INDEX IF NOT EXISTS idx_event_log_workspace_id
                  ON event_log(workspace_key, id);
              CREATE INDEX IF NOT EXISTS idx_project_setup_completion
                  ON project_setup(completed_at);
              CREATE UNIQUE INDEX IF NOT EXISTS idx_provider_profiles_one_selected
                 ON provider_profiles(project_id) WHERE selected = 1;
             CREATE INDEX IF NOT EXISTS idx_conversation_messages_session
                 ON conversation_messages(project_id, session_id, created_at);
             CREATE INDEX IF NOT EXISTS idx_sessions_organization
                 ON sessions(project_id, archived_at, pinned_at, last_activity_at DESC, id ASC);
             CREATE INDEX IF NOT EXISTS idx_sessions_title
                 ON sessions(project_id, title COLLATE NOCASE, id ASC);
             CREATE TABLE IF NOT EXISTS session_forks (
                 child_session_id TEXT PRIMARY KEY REFERENCES sessions(id) ON DELETE CASCADE,
                 project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
                 source_session_id TEXT NOT NULL REFERENCES sessions(id) ON DELETE RESTRICT,
                 created_at INTEGER NOT NULL,
                 payload TEXT NOT NULL CHECK(length(CAST(payload AS BLOB)) <= 65536),
                 CHECK(child_session_id <> source_session_id)
             );
             CREATE INDEX IF NOT EXISTS idx_session_forks_source
                 ON session_forks(project_id, source_session_id, created_at);
             CREATE UNIQUE INDEX IF NOT EXISTS idx_conversation_messages_run_role
                 ON conversation_messages(run_id, role);
             CREATE INDEX IF NOT EXISTS idx_goals_project_session
                 ON goals(project_id, session_id);
             CREATE INDEX IF NOT EXISTS idx_goal_history_session
                 ON goal_history(project_id, session_id, cleared_at);
             CREATE INDEX IF NOT EXISTS idx_change_sets_project_run
                 ON change_sets(project_id, run_id);
             CREATE INDEX IF NOT EXISTS idx_file_changes_set_revision
                 ON file_changes(change_set_id, revision);
             CREATE INDEX IF NOT EXISTS idx_change_operations_recovery
                 ON change_operations(project_id, state, created_at);
             CREATE INDEX IF NOT EXISTS idx_approval_grants_project_scope
                 ON approval_grants(project_id, scope, created_at);
             CREATE INDEX IF NOT EXISTS idx_approval_grants_action_exact
                 ON approval_grants(project_id, capability, json_extract(payload, '$.subject.tool_id'), json_extract(payload, '$.subject.target'), json_extract(payload, '$.subject.binding')) WHERE scope='action';
             CREATE INDEX IF NOT EXISTS idx_approval_grants_tool_call_exact
                 ON approval_grants(project_id, capability, json_extract(payload, '$.subject.tool_call_id')) WHERE scope='tool_call';
             CREATE INDEX IF NOT EXISTS idx_approval_grants_run_exact
                 ON approval_grants(project_id, capability, json_extract(payload, '$.subject.run_id')) WHERE scope='run';
             CREATE INDEX IF NOT EXISTS idx_approval_grants_session_exact
                 ON approval_grants(project_id, capability, json_extract(payload, '$.subject.session_id')) WHERE scope='session';
             CREATE INDEX IF NOT EXISTS idx_approval_grants_workspace_exact
                 ON approval_grants(project_id, capability, json_extract(payload, '$.subject.project_id')) WHERE scope='workspace';
             CREATE INDEX IF NOT EXISTS idx_approval_grants_profile_exact
                 ON approval_grants(project_id, capability, json_extract(payload, '$.subject.profile_id')) WHERE scope='persistent_profile';
             CREATE INDEX IF NOT EXISTS idx_execution_audit_project_run
                 ON execution_audit(project_id, run_id, recorded_at);
             CREATE INDEX IF NOT EXISTS idx_verification_evidence_project_run
                 ON verification_evidence(project_id, run_id, recorded_at);
             CREATE TABLE IF NOT EXISTS context_runs (
                 run_id TEXT PRIMARY KEY,
                 project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
                 session_id TEXT NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
                 task_id TEXT NOT NULL,
                 assistant_complete INTEGER NOT NULL DEFAULT 0,
                 next_ordinal INTEGER NOT NULL DEFAULT 0 CHECK(next_ordinal >= 0 AND next_ordinal <= 33),
                 user_message_id TEXT NOT NULL UNIQUE REFERENCES conversation_messages(id) ON DELETE CASCADE
             );
             CREATE INDEX IF NOT EXISTS idx_context_runs_scope
                 ON context_runs(project_id, session_id);
             CREATE TABLE IF NOT EXISTS context_groups (
                 run_id TEXT NOT NULL REFERENCES context_runs(run_id) ON DELETE CASCADE,
                 ordinal INTEGER NOT NULL CHECK(ordinal >= 0),
                 payload TEXT NOT NULL CHECK(length(CAST(payload AS BLOB)) <= 524288),
                 PRIMARY KEY(run_id, ordinal)
             );
             CREATE TABLE IF NOT EXISTS model_budgets (
                 project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
                 profile_id TEXT NOT NULL,
                 payload TEXT NOT NULL CHECK(length(CAST(payload AS BLOB)) <= 8192),
                 PRIMARY KEY(project_id, profile_id),
                 FOREIGN KEY(project_id, profile_id) REFERENCES provider_profiles(project_id, profile_id) ON DELETE CASCADE
             );
             CREATE TABLE IF NOT EXISTS context_checkpoints (
                 session_id TEXT PRIMARY KEY REFERENCES sessions(id) ON DELETE CASCADE,
                 project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
                 payload TEXT CHECK(payload IS NULL OR length(CAST(payload AS BLOB)) <= 98304)
             );
             CREATE TABLE IF NOT EXISTS goal_runs (
                 run_id TEXT PRIMARY KEY,
                 goal_id TEXT NOT NULL,
                 payload TEXT NOT NULL CHECK(length(CAST(payload AS BLOB)) <= 16384)
             );
             CREATE TABLE IF NOT EXISTS goal_requests (
                 request_id TEXT PRIMARY KEY,
                 run_id TEXT NOT NULL REFERENCES goal_runs(run_id),
                 payload TEXT NOT NULL CHECK(length(CAST(payload AS BLOB)) <= 16384)
             );
             CREATE TABLE IF NOT EXISTS goal_invocations (
                 id TEXT PRIMARY KEY,
                 goal_id TEXT NOT NULL,
                 project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
                 session_id TEXT NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
                 state TEXT NOT NULL CHECK(state IN ('active','verified','incomplete','failed','cancelled','review_required','unresolved')),
                 started_at INTEGER NOT NULL,
                 payload TEXT NOT NULL CHECK(length(CAST(payload AS BLOB)) <= 65536)
             );
             CREATE TABLE IF NOT EXISTS runtime_store_identity (
                 singleton INTEGER PRIMARY KEY CHECK(singleton = 1),
                 store_id TEXT NOT NULL,
                 created_at INTEGER NOT NULL
             );
             CREATE TABLE IF NOT EXISTS runtime_host_generations (
                 generation INTEGER PRIMARY KEY CHECK(generation > 0),
                 owner_id TEXT NOT NULL,
                 process_id INTEGER NOT NULL CHECK(process_id > 0),
                 process_start_key TEXT NOT NULL,
                 physical_identity TEXT NOT NULL,
                 physical_identity_sha256 TEXT NOT NULL,
                 state TEXT NOT NULL CHECK(state IN ('active','recovery_pending','recovered','retired')),
                 started_at INTEGER NOT NULL,
                 finished_at INTEGER,
                 recovery_revision TEXT,
                 reason TEXT NOT NULL DEFAULT ''
             );
             CREATE UNIQUE INDEX IF NOT EXISTS idx_runtime_host_one_unretired
                 ON runtime_host_generations((1))
                 WHERE state IN ('active','recovery_pending');
             CREATE TABLE IF NOT EXISTS runtime_run_ownership (
                 run_id TEXT PRIMARY KEY,
                 project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
                 session_id TEXT NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
                 owner_generation INTEGER NOT NULL REFERENCES runtime_host_generations(generation),
                 state TEXT NOT NULL CHECK(state IN ('active','complete','failed','cancelled','interrupted')),
                 updated_at INTEGER NOT NULL,
                 payload TEXT NOT NULL CHECK(length(CAST(payload AS BLOB)) <= 65536)
             );
             CREATE INDEX IF NOT EXISTS idx_runtime_run_owner_state
                 ON runtime_run_ownership(owner_generation, state, updated_at);
             CREATE TABLE IF NOT EXISTS command_identities (
                 project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
                 request_id TEXT NOT NULL CHECK(length(CAST(request_id AS BLOB)) BETWEEN 1 AND 128),
                 canonicalization_version INTEGER NOT NULL CHECK(canonicalization_version > 0),
                 fingerprint TEXT NOT NULL CHECK(length(fingerprint) = 64),
                 command_kind TEXT NOT NULL CHECK(length(CAST(command_kind AS BLOB)) BETWEEN 1 AND 64),
                 owner_generation INTEGER REFERENCES runtime_host_generations(generation),
                 state TEXT NOT NULL CHECK(state IN ('accepted','executing','settled','interrupted','unknown')),
                 session_id TEXT,
                 run_id TEXT,
                 task_id TEXT,
                 subject_id TEXT,
                 provider_profile_id TEXT CHECK(provider_profile_id IS NULL OR length(CAST(provider_profile_id AS BLOB)) BETWEEN 1 AND 64),
                 execution_snapshot_sha256 TEXT CHECK(execution_snapshot_sha256 IS NULL OR length(execution_snapshot_sha256) = 64),
                 definition_revision TEXT,
                 result_session_id TEXT,
                 outcome_kind TEXT CHECK(outcome_kind IS NULL OR outcome_kind IN ('success','failure','cancelled','denied','interrupted','unknown')),
                 result_payload TEXT CHECK(result_payload IS NULL OR length(CAST(result_payload AS BLOB)) <= 16384),
                 result_available INTEGER NOT NULL CHECK(result_available IN (0, 1)),
                 replay_allowed INTEGER NOT NULL CHECK(replay_allowed IN (0, 1)),
                 recovery_revision TEXT,
                 recovery_blocker TEXT CHECK(recovery_blocker IS NULL OR length(CAST(recovery_blocker AS BLOB)) <= 128),
                 accepted_at INTEGER NOT NULL,
                 updated_at INTEGER NOT NULL,
                 PRIMARY KEY(project_id, request_id)
             );
             CREATE INDEX IF NOT EXISTS idx_command_identities_owner_state
                 ON command_identities(owner_generation, state, updated_at);
             CREATE INDEX IF NOT EXISTS idx_command_identities_retention
                 ON command_identities(project_id, result_available, updated_at);
             CREATE INDEX IF NOT EXISTS idx_goal_invocations_scope
                 ON goal_invocations(project_id, session_id, goal_id, started_at);
              ",
        )?;
        schema.commit()?;
        // The legacy bounded-counter rebuild needs foreign keys disabled before
        // its own transaction, so it stays outside the schema transaction.
        working_context::upgrade_run_limit(&connection)?;
        // Legacy operations lack recorded provenance. A nullable additive column
        // leaves them unknown without rewriting their numbers or payloads.
        // The additive migrations below do not touch foreign-key topology, so
        // they can share one transaction and one commit as well.
        let additive = connection.unchecked_transaction()?;
        ensure_column(
            &additive,
            "change_operations",
            "line_count_availability",
            "TEXT CHECK(line_count_availability IS NULL OR line_count_availability IN ('computed', 'unavailable', 'unknown'))",
        )?;
        extensions::migrate(&additive)?;
        additive.execute_batch("PRAGMA user_version = 22;")?;
        ensure_column(&additive, "goal_invocations", "owner_generation", "INTEGER")?;
        // F09 development builds may already have created schema 18 before
        // execution-snapshot binding was added. Keep that interim database
        // readable without weakening the accepted identity.
        ensure_column(
            &additive,
            "command_identities",
            "provider_profile_id",
            "TEXT CHECK(provider_profile_id IS NULL OR length(CAST(provider_profile_id AS BLOB)) BETWEEN 1 AND 64)",
        )?;
        ensure_column(
            &additive,
            "command_identities",
            "execution_snapshot_sha256",
            "TEXT CHECK(execution_snapshot_sha256 IS NULL OR length(execution_snapshot_sha256) = 64)",
        )?;
        ensure_column(
            &additive,
            "command_identities",
            "definition_revision",
            "TEXT",
        )?;
        additive.execute(
            "INSERT OR IGNORE INTO runtime_store_identity (singleton, store_id, created_at)
             VALUES (1, ?1, ?2)",
            params![Uuid::new_v4().to_string(), now().unix_timestamp()],
        )?;
        // Legacy zeros are historical placeholders, not evidence of free budget.
        // The update is additive and restart-stable; no task history is inferred.
        let legacy =
            serde_json::to_string(&argentum_domain::goal_accounting::GoalAccounting::new(true))?;
        additive.execute("UPDATE goals SET payload = json_set(payload, '$.accounting', json(?1)) WHERE json_type(payload, '$.accounting') IS NULL OR json_type(payload, '$.accounting') = 'null'", [&legacy])?;
        additive.execute("UPDATE goal_history SET payload = json_set(payload, '$.accounting', json(?1)) WHERE json_type(payload, '$.accounting') IS NULL OR json_type(payload, '$.accounting') = 'null'", [&legacy])?;
        additive.commit()?;
        Ok(())
    }
}

fn save_goal_on(
    connection: &Connection,
    goal: &Goal,
    expected: Option<&Option<Goal>>,
) -> Result<(), StoreError> {
    validate_goal(goal)?;
    let mut goal = goal.clone();
    let existing: Option<String> = connection
        .query_row(
            "SELECT payload FROM goals WHERE project_id = ?1 AND session_id = ?2",
            params![goal.project_id.to_string(), goal.session_id.to_string()],
            |row| row.get(0),
        )
        .optional()?;
    let previous = existing
        .as_deref()
        .map(serde_json::from_str::<Goal>)
        .transpose()?;
    if let Some(previous) = previous.as_ref() {
        validate_goal(previous)?;
    }
    if expected.is_some_and(|expected| previous.as_ref() != expected.as_ref()) {
        return Err(StoreError::CommandExecutionSnapshotChanged);
    }
    if let Some(previous) = previous {
        if previous.id != goal.id {
            return Err(StoreError::GoalAccounting(
                "clear_goal_before_creating_distinct_identity",
            ));
        }
        if let Some(mut account) = previous.accounting.clone() {
            let definition_changed = previous.objective != goal.objective;
            let accounting_changed = definition_changed
                || previous.token_budget != goal.token_budget
                || previous.tool_budget != goal.tool_budget
                || previous.time_budget_seconds != goal.time_budget_seconds;
            if accounting_changed {
                account.revision = argentum_domain::GoalId::new_v4();
            }
            if definition_changed {
                goal.definition_revision = argentum_domain::GoalId::new_v4();
                goal.owner_attestation = None;
                goal.completion = None;
            }
            // Configuration callers cannot rewrite charged or unresolved work.
            goal.accounting = Some(account);
            goal.tokens_used = previous.tokens_used;
            goal.tools_used = previous.tools_used;
            goal.iteration = previous.iteration;
            goal.created_at = previous.created_at;
        }
    }
    let belongs_to_project = connection.query_row(
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
    connection.execute(
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
            serde_json::to_string(&goal)?,
            goal.updated_at.unix_timestamp(),
        ],
    )?;
    Ok(())
}

fn clear_goal_on(
    connection: &Connection,
    project_id: ProjectId,
    session_id: SessionId,
    expected: Option<&Option<Goal>>,
) -> Result<bool, StoreError> {
    let belongs_to_project = connection.query_row(
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
    let payload = connection
        .query_row(
            "SELECT payload FROM goals
             WHERE project_id = ?1 AND session_id = ?2",
            params![project_id.to_string(), session_id.to_string()],
            |row| row.get::<_, String>(0),
        )
        .optional()?;
    let current = payload
        .as_deref()
        .map(serde_json::from_str::<Goal>)
        .transpose()?;
    if let Some(current) = current.as_ref() {
        validate_goal(current)?;
    }
    if expected.is_some_and(|expected| current.as_ref() != expected.as_ref()) {
        return Err(StoreError::CommandExecutionSnapshotChanged);
    }
    let (Some(payload), Some(goal)) = (payload, current) else {
        return Ok(false);
    };
    connection.execute(
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
    connection.execute(
        "DELETE FROM goals WHERE project_id = ?1 AND session_id = ?2",
        params![project_id.to_string(), session_id.to_string()],
    )?;
    Ok(true)
}

fn validate_prepared_change_operation(
    operation: &PreparedChangeOperation,
) -> Result<(), StoreError> {
    let path = Path::new(&operation.relative_path);
    if operation.relative_path.is_empty()
        || operation.relative_path.len() > 4 * 1024
        || path
            .components()
            .any(|component| matches!(component, Component::ParentDir))
        || (!path.is_absolute()
            && path
                .components()
                .any(|component| matches!(component, Component::RootDir | Component::Prefix(_))))
    {
        return Err(StoreError::InvalidRecord(
            "change operation path is invalid".into(),
        ));
    }
    if operation.before.is_none() && operation.after.is_none() {
        return Err(StoreError::InvalidRecord(
            "change operation has no snapshots".into(),
        ));
    }
    for (snapshot, bytes) in [
        (operation.before.as_ref(), operation.before_bytes.as_deref()),
        (operation.after.as_ref(), operation.after_bytes.as_deref()),
    ] {
        match (snapshot, bytes) {
            (None, None) => {}
            (Some(snapshot), Some(bytes)) if snapshot.retained => {
                if bytes.len() > MAX_CHANGE_SNAPSHOT_BYTES
                    || snapshot.size_bytes != bytes.len() as u64
                    || snapshot.digest != digest_bytes(bytes)
                {
                    return Err(StoreError::InvalidRecord(
                        "retained change snapshot metadata is invalid".into(),
                    ));
                }
            }
            (Some(snapshot), None) if !snapshot.retained => {
                if snapshot.size_bytes <= MAX_CHANGE_SNAPSHOT_BYTES as u64 {
                    return Err(StoreError::InvalidRecord(
                        "unretained change snapshot size is invalid".into(),
                    ));
                }
            }
            _ => {
                return Err(StoreError::InvalidRecord(
                    "change snapshot retention metadata is inconsistent".into(),
                ));
            }
        }
    }
    Ok(())
}

fn insert_change_blob(
    connection: &Connection,
    project_id: ProjectId,
    snapshot: Option<&FileSnapshotRef>,
    bytes: Option<&[u8]>,
) -> Result<(), StoreError> {
    let (Some(snapshot), Some(bytes)) = (snapshot, bytes) else {
        return Ok(());
    };
    let existing = connection
        .query_row(
            "SELECT size_bytes, bytes FROM change_blobs
             WHERE project_id = ?1 AND digest = ?2",
            params![project_id.to_string(), snapshot.digest.0],
            |row| Ok((row.get::<_, u64>(0)?, row.get::<_, Vec<u8>>(1)?)),
        )
        .optional()?;
    if let Some((size_bytes, existing_bytes)) = existing {
        if size_bytes != snapshot.size_bytes || existing_bytes != bytes {
            return Err(StoreError::InvalidRecord(
                "stored change blob does not match its digest".into(),
            ));
        }
        return Ok(());
    }
    connection.execute(
        "INSERT INTO change_blobs (project_id, digest, size_bytes, bytes)
         VALUES (?1, ?2, ?3, ?4)",
        params![
            project_id.to_string(),
            snapshot.digest.0,
            snapshot.size_bytes,
            bytes,
        ],
    )?;
    Ok(())
}

fn load_change_operation_on(
    connection: &Connection,
    operation_id: ChangeOperationId,
) -> Result<Option<StoredChangeOperation>, StoreError> {
    // Read-only callers may inspect a pre-extension schema. Never migrate or
    // derive historical provenance while reading; a missing column is unknown.
    let has_availability = connection.query_row(
        "SELECT EXISTS(SELECT 1 FROM pragma_table_info('change_operations') WHERE name='line_count_availability')",
        [],
        |row| row.get::<_, bool>(0),
    )?;
    let availability_column = if has_availability {
        "line_count_availability"
    } else {
        "NULL"
    };
    let query = format!(
        "SELECT workspace_key, project_id, session_id, run_id, relative_path,
                change_kind, origin, before_digest, before_size, before_retained,
                after_digest, after_size, after_retained, additions, removals,
                restored_from, state, current_digest, created_at, {availability_column}
         FROM change_operations WHERE id = ?1"
    );
    let row = connection
        .query_row(&query, [operation_id.to_string()], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, String>(4)?,
                row.get::<_, String>(5)?,
                row.get::<_, String>(6)?,
                row.get::<_, Option<String>>(7)?,
                row.get::<_, Option<u64>>(8)?,
                row.get::<_, Option<bool>>(9)?,
                row.get::<_, Option<String>>(10)?,
                row.get::<_, Option<u64>>(11)?,
                row.get::<_, Option<bool>>(12)?,
                row.get::<_, u32>(13)?,
                row.get::<_, u32>(14)?,
                row.get::<_, Option<String>>(15)?,
                row.get::<_, String>(16)?,
                row.get::<_, Option<String>>(17)?,
                row.get::<_, i64>(18)?,
                row.get::<_, Option<String>>(19)?,
            ))
        })
        .optional()?;
    let Some((
        workspace_key,
        project_id,
        session_id,
        run_id,
        relative_path,
        kind,
        origin,
        before_digest,
        before_size,
        before_retained,
        after_digest,
        after_size,
        after_retained,
        additions,
        removals,
        restored_from,
        state,
        current_digest,
        created_at,
        line_count_availability,
    )) = row
    else {
        return Ok(None);
    };
    Ok(Some(StoredChangeOperation {
        operation_id,
        workspace_key,
        project_id: parse_uuid(&project_id, "change operation project id")?,
        session_id: parse_uuid(&session_id, "change operation session id")?,
        run_id: parse_uuid(&run_id, "change operation run id")?,
        relative_path,
        kind: parse_file_change_kind(&kind)?,
        origin: parse_file_change_origin(&origin)?,
        before: snapshot_ref(before_digest, before_size, before_retained)?,
        after: snapshot_ref(after_digest, after_size, after_retained)?,
        additions,
        removals,
        line_count_availability: parse_line_count_availability(line_count_availability.as_deref())?,
        restored_from: restored_from
            .map(|value| parse_uuid(&value, "restored file change id"))
            .transpose()?,
        state: parse_change_recovery_state(&state)?,
        current_digest: current_digest.map(Sha256Digest),
        created_at: OffsetDateTime::from_unix_timestamp(created_at).map_err(|_| {
            StoreError::InvalidRecord("change operation timestamp is invalid".into())
        })?,
    }))
}

fn snapshot_ref(
    digest: Option<String>,
    size: Option<u64>,
    retained: Option<bool>,
) -> Result<Option<FileSnapshotRef>, StoreError> {
    match (digest, size, retained) {
        (None, None, None) => Ok(None),
        (Some(digest), Some(size_bytes), Some(retained)) => Ok(Some(FileSnapshotRef {
            digest: Sha256Digest(digest),
            size_bytes,
            retained,
        })),
        _ => Err(StoreError::InvalidRecord(
            "stored change snapshot metadata is incomplete".into(),
        )),
    }
}

fn digest_bytes(bytes: &[u8]) -> Sha256Digest {
    let digest = Sha256::digest(bytes);
    Sha256Digest(format!("{digest:x}"))
}

fn parse_uuid(value: &str, label: &str) -> Result<ProjectId, StoreError> {
    value
        .parse()
        .map_err(|_| StoreError::InvalidRecord(format!("stored {label} is invalid")))
}

const fn line_count_availability_name(value: LineCountAvailability) -> &'static str {
    match value {
        LineCountAvailability::Computed => "computed",
        LineCountAvailability::Unavailable => "unavailable",
        LineCountAvailability::Unknown => "unknown",
    }
}

fn parse_line_count_availability(value: Option<&str>) -> Result<LineCountAvailability, StoreError> {
    match value {
        None | Some("unknown") => Ok(LineCountAvailability::Unknown),
        Some("computed") => Ok(LineCountAvailability::Computed),
        Some("unavailable") => Ok(LineCountAvailability::Unavailable),
        _ => Err(StoreError::InvalidRecord(
            "stored line-count availability is invalid".into(),
        )),
    }
}

const fn file_change_kind_name(kind: FileChangeKind) -> &'static str {
    match kind {
        FileChangeKind::Added => "added",
        FileChangeKind::Modified => "modified",
        FileChangeKind::Deleted => "deleted",
    }
}

fn parse_file_change_kind(value: &str) -> Result<FileChangeKind, StoreError> {
    match value {
        "added" => Ok(FileChangeKind::Added),
        "modified" => Ok(FileChangeKind::Modified),
        "deleted" => Ok(FileChangeKind::Deleted),
        _ => Err(StoreError::InvalidRecord(
            "stored file change kind is invalid".into(),
        )),
    }
}

const fn file_change_origin_name(origin: FileChangeOrigin) -> &'static str {
    match origin {
        FileChangeOrigin::ToolCall => "tool_call",
        FileChangeOrigin::Restore => "restore",
    }
}

fn parse_file_change_origin(value: &str) -> Result<FileChangeOrigin, StoreError> {
    match value {
        "tool_call" => Ok(FileChangeOrigin::ToolCall),
        "restore" => Ok(FileChangeOrigin::Restore),
        _ => Err(StoreError::InvalidRecord(
            "stored file change origin is invalid".into(),
        )),
    }
}

const fn change_recovery_state_name(state: ChangeRecoveryState) -> &'static str {
    match state {
        ChangeRecoveryState::Prepared => "prepared",
        ChangeRecoveryState::Applied => "applied",
        ChangeRecoveryState::Finalized => "finalized",
        ChangeRecoveryState::NotApplied => "not_applied",
        ChangeRecoveryState::Conflicted => "conflicted",
    }
}

fn parse_change_recovery_state(value: &str) -> Result<ChangeRecoveryState, StoreError> {
    match value {
        "prepared" => Ok(ChangeRecoveryState::Prepared),
        "applied" => Ok(ChangeRecoveryState::Applied),
        "finalized" => Ok(ChangeRecoveryState::Finalized),
        "not_applied" => Ok(ChangeRecoveryState::NotApplied),
        "conflicted" => Ok(ChangeRecoveryState::Conflicted),
        _ => Err(StoreError::InvalidRecord(
            "stored change recovery state is invalid".into(),
        )),
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
        "INSERT INTO sessions (
            id, project_id, created_at, updated_at, last_activity_at, title, payload
         ) VALUES (?1, ?2, ?3, ?4, ?4, ?5, ?6)",
        params![
            session.id.to_string(),
            session.project_id.to_string(),
            session.created_at.unix_timestamp(),
            session.updated_at.unix_timestamp(),
            session.title,
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

fn ensure_onboarding_snapshot(
    connection: &Connection,
    project_id: ProjectId,
    is_new_project: bool,
    default_provider: &ProviderProfile,
    workspace_root: &Path,
) -> Result<(), StoreError> {
    let exists = connection.query_row(
        "SELECT EXISTS(SELECT 1 FROM project_setup WHERE project_id = ?1)",
        [project_id.to_string()],
        |row| row.get::<_, bool>(0),
    )?;
    if exists {
        return Ok(());
    }

    let snapshot = if is_new_project {
        OnboardingSnapshot::initial()
    } else {
        OnboardingSnapshot::legacy_complete(
            &default_provider.id,
            &default_provider.model,
            workspace_root.to_path_buf(),
            DEFAULT_EXECUTION_PROFILE_ID,
        )
    };
    let payload = serde_json::to_string(&snapshot)?;
    let timestamp = now().unix_timestamp();
    connection.execute(
        "INSERT INTO project_setup (
            project_id, version, started_at, completed_at, payload
         ) VALUES (?1, ?2, ?3, ?4, ?5)",
        params![
            project_id.to_string(),
            i64::from(snapshot.version),
            timestamp,
            snapshot.complete.then_some(timestamp),
            payload,
        ],
    )?;
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
        ProviderKind::LocalLlamaCpp => "local_llama_cpp",
        ProviderKind::Unknown => "unknown",
    }
}

fn parse_provider_kind(kind: &str) -> Result<ProviderKind, StoreError> {
    match kind {
        "openai_compatible" => Ok(ProviderKind::OpenAiCompatible),
        "anthropic" => Ok(ProviderKind::Anthropic),
        "local_lm_studio" => Ok(ProviderKind::LocalLmStudio),
        "local_llama_cpp" => Ok(ProviderKind::LocalLlamaCpp),
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

type ConversationMessageTuple = (
    String,
    String,
    String,
    String,
    i64,
    String,
    String,
    Option<String>,
    String,
    String,
);

fn conversation_message_tuple(
    row: &rusqlite::Row<'_>,
) -> rusqlite::Result<ConversationMessageTuple> {
    Ok((
        row.get(0)?,
        row.get(1)?,
        row.get(2)?,
        row.get(3)?,
        row.get(4)?,
        row.get(5)?,
        row.get(6)?,
        row.get(7)?,
        row.get(8)?,
        row.get(9)?,
    ))
}

fn conversation_message_from_tuple(
    project_id: ProjectId,
    session_id: SessionId,
    row: ConversationMessageTuple,
) -> Result<ConversationMessage, StoreError> {
    let (id, run_id, role, status, created_at, text, reasoning, usage, profile_id, model) = row;
    let message = ConversationMessage {
        id: id
            .parse()
            .map_err(|_| StoreError::InvalidRecord("conversation message id is invalid".into()))?,
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
        created_at: time::OffsetDateTime::from_unix_timestamp(created_at).map_err(|_| {
            StoreError::InvalidRecord("conversation message timestamp is invalid".into())
        })?,
    };
    validate_conversation_message_record(&message)?;
    Ok(message)
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
        .is_none_or(|tokens| tokens <= usage.input_tokens);
    let valid_reasoning = usage
        .reasoning_tokens
        .is_none_or(|tokens| tokens <= usage.output_tokens);
    let valid_context = usage
        .context_window_tokens
        .is_none_or(|tokens| tokens > 0 && usage.total_tokens <= tokens);
    if valid_total && valid_cached && valid_reasoning && valid_context {
        Ok(())
    } else {
        Err(StoreError::InvalidRecord(
            "conversation message usage is invalid".into(),
        ))
    }
}

fn validate_goal(goal: &Goal) -> Result<(), StoreError> {
    if let Some(account) = &goal.accounting {
        if account.owner_removed_aggregate_limits {
            if goal.token_budget.is_some()
                || goal.tool_budget.is_some()
                || goal.time_budget_seconds.is_some()
                || account.request_limit.is_some()
                || account.iteration_limit.is_some()
                || account.per_run.token_limit.is_none()
                || account.per_run.request_limit.is_none()
                || account.per_run.tool_limit.is_none()
                || account.per_run.elapsed_seconds.is_none()
            {
                return Err(StoreError::InvalidRecord("owner aggregate amendment requires absent aggregate limits and finite run guards".into()));
            }
            account
                .per_run
                .validate()
                .map_err(|reason| StoreError::InvalidRecord(reason.into()))?;
        }
    }
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
    if let Some(acceptance) = &goal.acceptance {
        if goal.definition_revision.is_nil()
            || acceptance.version == 0
            || acceptance.criteria.is_empty()
            || acceptance.criteria.len()
                > argentum_domain::goal_supervision::MAX_ACCEPTANCE_CRITERIA
        {
            return Err(StoreError::InvalidRecord(
                "goal acceptance definition is invalid".into(),
            ));
        }
    } else if goal.owner_attestation.is_some() || goal.completion.is_some() {
        return Err(StoreError::InvalidRecord(
            "goal evidence requires an acceptance definition".into(),
        ));
    }
    if let Some(attestation) = &goal.owner_attestation {
        let acceptance = goal.acceptance.as_ref().ok_or_else(|| {
            StoreError::InvalidRecord("goal evidence requires an acceptance definition".into())
        })?;
        if attestation.id.is_nil()
            || attestation.definition_revision != goal.definition_revision
            || attestation.criteria_version != acceptance.version
            || !valid_audit_text(&attestation.summary, 4 * 1024)
        {
            return Err(StoreError::InvalidRecord(
                "goal owner attestation is invalid".into(),
            ));
        }
    }
    if let Some(completion) = &goal.completion {
        let acceptance = goal.acceptance.as_ref().ok_or_else(|| {
            StoreError::InvalidRecord("goal evidence requires an acceptance definition".into())
        })?;
        // An explicit owner resume can retain a previously valid completion as
        // historical evidence while the goal becomes active for revalidation.
        if completion.invocation_id.is_nil()
            || completion.definition_revision != goal.definition_revision
            || completion.criteria_version != acceptance.version
            || completion.evidence_ids.len() != acceptance.criteria.len()
            || !valid_audit_text(&completion.statement, 512)
        {
            return Err(StoreError::InvalidRecord(
                "goal completion receipt is invalid".into(),
            ));
        }
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
    if policy.sandbox_scope == argentum_domain::SandboxScope::Unrestricted
        && policy.profile_id != "full-access"
    {
        return Err(StoreError::InvalidRecord(
            "unrestricted execution policy requires Full Access".into(),
        ));
    }
    Ok(())
}

fn validate_record_limit(limit: usize) -> Result<(), StoreError> {
    if limit == 0 || limit > 1000 {
        Err(StoreError::InvalidRecord(
            "record query limit must be from 1 through 1000".into(),
        ))
    } else {
        Ok(())
    }
}

fn grant_scope_name(scope: GrantScope) -> &'static str {
    match scope {
        GrantScope::Action => "action",
        GrantScope::ToolCall => "tool_call",
        GrantScope::Run => "run",
        GrantScope::Session => "session",
        GrantScope::Workspace => "workspace",
        GrantScope::PersistentProfile => "persistent_profile",
    }
}

fn validate_approval_grant(
    project_id: ProjectId,
    grant: &ApprovalGrantRecord,
) -> Result<(), StoreError> {
    let valid_reason = !grant.reason.trim().is_empty()
        && grant.reason.len() <= 1024
        && !grant
            .reason
            .chars()
            .any(|character| character.is_control() && !matches!(character, '\n' | '\r' | '\t'));
    let valid_time = grant
        .expires_at
        .is_none_or(|expires_at| expires_at > grant.created_at)
        && grant
            .revoked_at
            .is_none_or(|revoked_at| revoked_at >= grant.created_at);
    let valid_subject = match &grant.subject {
        GrantSubject::Action {
            tool_id,
            target,
            binding,
        } => valid_tool_id(tool_id) && valid_audit_text(target, 4096) && valid_sha256(binding),
        GrantSubject::ToolCall { .. } | GrantSubject::Run { .. } | GrantSubject::Session { .. } => {
            true
        }
        GrantSubject::Workspace {
            project_id: subject_project_id,
        } => subject_project_id == &project_id,
        GrantSubject::PersistentProfile { profile_id } => valid_harness_id(profile_id),
    };
    if grant.scope == grant.subject.scope() && valid_reason && valid_time && valid_subject {
        Ok(())
    } else {
        Err(StoreError::InvalidRecord(
            "approval grant contains invalid bounded fields".into(),
        ))
    }
}

fn validate_execution_audit(record: &ExecutionAuditRecord) -> Result<(), StoreError> {
    let valid_times = record.finished_at >= record.started_at;
    let valid_ids = valid_tool_id(&record.tool_id) && valid_harness_id(&record.profile_id);
    let valid_target = valid_audit_text(&record.target, 4096);
    let valid_counts = record.capabilities.len() <= 16
        && record.grant_ids.len() <= 64
        && record.changed_paths.len() <= 64;
    let valid_paths = record
        .changed_paths
        .iter()
        .all(|path| valid_audit_text(path, 4096));
    let valid_digests = record.stdout_sha256.as_ref().is_none_or(valid_sha256)
        && record.stderr_sha256.as_ref().is_none_or(valid_sha256);
    if valid_times && valid_ids && valid_target && valid_counts && valid_paths && valid_digests {
        Ok(())
    } else {
        Err(StoreError::InvalidRecord(
            "execution audit contains invalid bounded fields".into(),
        ))
    }
}

fn validate_verification_evidence(evidence: &VerificationEvidence) -> Result<(), StoreError> {
    let valid_program = valid_audit_text(&evidence.program, 4096);
    let valid_args = evidence.args.len() <= 128
        && evidence.args.iter().map(String::len).sum::<usize>() <= 64 * 1024
        && evidence
            .args
            .iter()
            .all(|argument| valid_audit_text(argument, 8192));
    let valid_digests = [
        &evidence.revision_before,
        &evidence.revision_after,
        &evidence.stdout_sha256,
        &evidence.stderr_sha256,
    ]
    .into_iter()
    .all(valid_sha256);
    let valid_state = match evidence.state {
        argentum_domain::VerificationState::Passed => {
            evidence.exit_code == Some(0) && evidence.revision_before == evidence.revision_after
        }
        argentum_domain::VerificationState::Stale => {
            evidence.revision_before != evidence.revision_after
        }
        argentum_domain::VerificationState::Failed
        | argentum_domain::VerificationState::Cancelled
        | argentum_domain::VerificationState::TimedOut
        | argentum_domain::VerificationState::SpawnFailed => true,
    };
    let provenance = match (
        evidence.definition_sha256.as_ref(),
        evidence.input_scope.as_ref(),
        evidence.input_before.as_ref(),
        evidence.input_after.as_ref(),
    ) {
        (None, None, None, None) => {
            evidence.workspace_key.is_empty() && evidence.generated_outputs.is_none()
        }
        (Some(definition), Some(scope), Some(before), Some(after)) => {
            !evidence.workspace_key.is_empty()
                && evidence.workspace_key.len() <= 32 * 1024
                && valid_sha256(definition)
                && scope.validate().is_ok()
                && before.validate_for_scope(scope).is_ok()
                && after.validate_for_scope(scope).is_ok()
                && before.digest == evidence.revision_before
                && after.digest == evidence.revision_after
                && before.file_count <= scope.max_files
                && after.file_count <= scope.max_files
                && before.total_bytes <= scope.max_total_bytes
                && after.total_bytes <= scope.max_total_bytes
                && match (&scope.project_inputs, &evidence.generated_outputs) {
                    (None, None) => true,
                    (Some(_), Some(generated)) => generated
                        .validate_for_scope(
                            scope,
                            evidence.state == argentum_domain::VerificationState::Passed,
                        )
                        .is_ok(),
                    _ => false,
                }
        }
        _ => false,
    };
    if evidence.registry_version > 0
        && valid_program
        && valid_args
        && valid_digests
        && valid_state
        && provenance
    {
        Ok(())
    } else {
        Err(StoreError::InvalidRecord(
            "verification evidence contains invalid bounded fields".into(),
        ))
    }
}

fn valid_sha256(digest: &Sha256Digest) -> bool {
    digest.0.len() == 64 && digest.0.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn valid_tool_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 64
        && value.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'.' | b'-' | b'_')
        })
}

fn valid_audit_text(value: &str, limit: usize) -> bool {
    !value.is_empty()
        && value.len() <= limit
        && !value
            .chars()
            .any(|character| character.is_control() && !matches!(character, '\n' | '\r' | '\t'))
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
        let local_llama_endpoint = profile.kind == ProviderKind::LocalLlamaCpp
            && endpoint.scheme() == "local"
            && endpoint.host_str() == Some("llama-cpp")
            && endpoint.path() == "/";
        let http_endpoint = profile.kind != ProviderKind::LocalLlamaCpp
            && matches!(endpoint.scheme(), "http" | "https")
            && endpoint.host_str().is_some();
        (local_llama_endpoint || http_endpoint)
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
) -> Result<(u64, OffsetDateTime), StoreError> {
    let payload = serde_json::to_string(event)?;
    let occurred_at = now();
    connection.execute(
        "INSERT INTO event_log (
            event_type, created_at, payload, payload_version,
            workspace_key, project_id, session_id, run_id
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        params![
            event.kind(),
            occurred_at.unix_timestamp(),
            payload,
            EVENT_PAYLOAD_VERSION,
            workspace_key,
            project_id.map(|value| value.to_string()),
            session_id.map(|value| value.to_string()),
            run_id.map(|value| value.to_string()),
        ],
    )?;
    let sequence = u64::try_from(connection.last_insert_rowid())
        .map_err(|_| StoreError::InvalidRecord("event sequence is invalid".into()))?;
    Ok((sequence, occurred_at))
}

fn load_sessions(
    connection: &Connection,
    project_id: ProjectId,
) -> Result<Vec<SessionSummary>, StoreError> {
    let mut statement = connection.prepare(
        "SELECT payload, pinned_at, archived_at FROM sessions
         WHERE project_id = ?1
         ORDER BY created_at ASC, id ASC",
    )?;
    let rows = statement.query_map([project_id.to_string()], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, Option<i64>>(1)?,
            row.get::<_, Option<i64>>(2)?,
        ))
    })?;
    let mut sessions = Vec::new();
    for row in rows {
        let (payload, pinned_at, archived_at) = row?;
        let session = serde_json::from_str::<Session>(&payload)?;
        let mut summary = SessionSummary::from(&session);
        summary.pinned = pinned_at.is_some();
        summary.archived = archived_at.is_some();
        sessions.push(summary);
    }
    Ok(sessions)
}

#[derive(Debug, Clone, Copy)]
struct SessionCursor {
    pinned: bool,
    last_activity_at: i64,
    id: SessionId,
}

fn validate_session_list_input(query: &str, limit: u16) -> Result<(), StoreError> {
    if query.len() > MAX_SESSION_LIST_QUERY_BYTES || query.chars().any(char::is_control) {
        return Err(StoreError::InvalidRecord(
            "session search must be at most 256 UTF-8 bytes and contain no control characters"
                .into(),
        ));
    }
    let limit = usize::from(limit);
    if !(1..=MAX_SESSION_LIST_LIMIT).contains(&limit) {
        return Err(StoreError::InvalidRecord(format!(
            "session page size must be between 1 and {MAX_SESSION_LIST_LIMIT}"
        )));
    }
    Ok(())
}

fn validate_session_title(title: &str) -> Result<(), StoreError> {
    if title.trim().is_empty()
        || title.len() > MAX_SESSION_TITLE_BYTES
        || title.chars().any(char::is_control)
    {
        return Err(StoreError::InvalidRecord(
            "session title must be 1 to 256 UTF-8 bytes and contain no control characters".into(),
        ));
    }
    Ok(())
}

fn validate_loaded_session_title(title: String) -> Result<String, StoreError> {
    validate_session_title(&title)?;
    Ok(title)
}

fn timestamp(value: i64, label: &str) -> Result<OffsetDateTime, StoreError> {
    OffsetDateTime::from_unix_timestamp(value)
        .map_err(|_| StoreError::InvalidRecord(format!("{label} is invalid")))
}

fn escape_like_prefix(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('%', "\\%")
        .replace('_', "\\_")
}

fn session_filter_code(filter: SessionArchiveFilter) -> char {
    match filter {
        SessionArchiveFilter::Active => 'a',
        SessionArchiveFilter::Archived => 'r',
        SessionArchiveFilter::All => 'l',
    }
}

fn session_query_digest(query: &str) -> String {
    format!("{:x}", Sha256::digest(query.as_bytes()))
}

fn encode_session_cursor(
    project_id: ProjectId,
    query: &str,
    filter: SessionArchiveFilter,
    limit: usize,
    row: &SessionListRow,
) -> String {
    format!(
        "v1|{}|{}|{}|{}|{}|{}|{}",
        project_id,
        session_filter_code(filter),
        limit,
        session_query_digest(query),
        u8::from(row.pinned),
        row.last_activity_at.unix_timestamp(),
        row.id
    )
}

fn decode_session_cursor(
    value: &str,
    project_id: ProjectId,
    query: &str,
    filter: SessionArchiveFilter,
    limit: usize,
) -> Result<SessionCursor, StoreError> {
    let parts = value.split('|').collect::<Vec<_>>();
    if parts.len() != 8
        || parts[0] != "v1"
        || parts[1] != project_id.to_string()
        || !parts[2].starts_with(session_filter_code(filter))
        || parts[3].parse::<usize>().ok() != Some(limit)
        || parts[4] != session_query_digest(query)
        || !matches!(parts[5], "0" | "1")
    {
        return Err(StoreError::InvalidRecord(
            "session cursor is invalid or belongs to a different query".into(),
        ));
    }
    let pinned = parts[5] == "1";
    let last_activity_at = parts[6].parse::<i64>().map_err(|_| {
        StoreError::InvalidRecord("session cursor activity timestamp is invalid".into())
    })?;
    let id = parts[7]
        .parse()
        .map_err(|_| StoreError::InvalidRecord("session cursor session id is invalid".into()))?;
    Ok(SessionCursor {
        pinned,
        last_activity_at,
        id,
    })
}

fn load_session_row_on(
    connection: &Connection,
    project_id: ProjectId,
    session_id: SessionId,
    active_session_id: Option<SessionId>,
) -> Result<SessionListRow, StoreError> {
    let (title, created_at, last_activity_at, pinned_at, archived_at) = connection
        .query_row(
            "SELECT title, created_at, last_activity_at, pinned_at, archived_at
             FROM sessions WHERE id = ?1 AND project_id = ?2",
            params![session_id.to_string(), project_id.to_string()],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, i64>(2)?,
                    row.get::<_, Option<i64>>(3)?,
                    row.get::<_, Option<i64>>(4)?,
                ))
            },
        )
        .optional()?
        .ok_or_else(|| {
            StoreError::InvalidRecord("session does not belong to the current project".into())
        })?;
    Ok(SessionListRow {
        id: session_id,
        title: validate_loaded_session_title(title)?,
        created_at: timestamp(created_at, "session created_at")?,
        last_activity_at: timestamp(last_activity_at, "session last_activity_at")?,
        pinned: pinned_at.is_some(),
        archived: archived_at.is_some(),
        selected: active_session_id == Some(session_id),
    })
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

    use argentum_domain::{
        AppEvent, ApprovalGrantRecord, ApprovalMode, Capability, ExecutionAuditRecord,
        ExecutionOutcome, GrantIssuer, GrantScope, GrantSubject, LayoutProfile, SandboxScope,
        TaskLifecycle, VerificationEvidence, VerificationRunnerId, VerificationState,
    };

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
    fn new_projects_start_onboarding_and_existing_projects_migrate_complete() {
        let directory = tempfile::tempdir().expect("directory");
        let workspace = directory.path().join("workspace");
        std::fs::create_dir(&workspace).expect("workspace");
        let store = Store::open_in_memory().expect("store");
        let first = store.resolve_workspace(&workspace).expect("resolution");
        let initial = store
            .load_onboarding(first.snapshot.project.id)
            .expect("initial onboarding")
            .expect("initial row");
        assert_eq!(initial.step, argentum_domain::OnboardingStep::Guidance);
        assert!(!initial.complete);

        store
            .connection
            .lock()
            .expect("connection")
            .execute(
                "DELETE FROM project_setup WHERE project_id = ?1",
                [first.snapshot.project.id.to_string()],
            )
            .expect("remove setup row");
        let reopened = store.resolve_workspace(&workspace).expect("reopen");
        let migrated = store
            .load_onboarding(reopened.snapshot.project.id)
            .expect("migrated onboarding")
            .expect("migrated row");
        assert_eq!(migrated.step, argentum_domain::OnboardingStep::Complete);
        assert!(migrated.complete);
        assert_eq!(migrated.provider_id.as_deref(), Some("llama-cpp"));
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
    fn session_event_windows_are_bounded_ordered_and_project_scoped() {
        let directory = tempfile::tempdir().expect("directory");
        let left_root = directory.path().join("left");
        let right_root = directory.path().join("right");
        std::fs::create_dir(&left_root).expect("left workspace");
        std::fs::create_dir(&right_root).expect("right workspace");
        let store = Store::open_in_memory().expect("store");
        let left = store.resolve_workspace(&left_root).expect("left workspace");
        let right = store
            .resolve_workspace(&right_root)
            .expect("right workspace");
        let session_id = left.snapshot.active_session_id.expect("left session");
        let run_id = RunId::new_v4();
        let scope = EventScope {
            workspace_key: left.workspace_key.clone(),
            project_id: left.snapshot.project.id,
            session_id: Some(session_id),
            run_id: Some(run_id),
        };
        for lifecycle in [
            TaskLifecycle::Planning,
            TaskLifecycle::Running,
            TaskLifecycle::Complete,
        ] {
            store
                .append_event_scoped(&scope, &AppEvent::RunStatusChanged { run_id, lifecycle })
                .expect("scoped event");
        }

        let window = store
            .session_event_window(left.snapshot.project.id, session_id, 2)
            .expect("event window");
        assert!(window.truncated);
        assert_eq!(window.records.len(), 2);
        assert!(window.records[0].sequence < window.records[1].sequence);
        assert!(matches!(
            window.records[0].event,
            AppEvent::RunStatusChanged {
                lifecycle: TaskLifecycle::Running,
                ..
            }
        ));
        assert!(matches!(
            window.records[1].event,
            AppEvent::RunStatusChanged {
                lifecycle: TaskLifecycle::Complete,
                ..
            }
        ));
        assert!(matches!(
            store.session_event_window(left.snapshot.project.id, session_id, 0),
            Err(StoreError::InvalidRecord(_))
        ));
        let right_session = right.snapshot.active_session_id.expect("right session");
        assert!(matches!(
            store.session_event_window(left.snapshot.project.id, right_session, 2),
            Err(StoreError::InvalidRecord(_))
        ));
    }

    #[test]
    fn session_listing_and_organization_are_bounded_project_scoped_and_restart_safe() {
        let workspace = tempfile::tempdir().expect("workspace");
        let store = Store::open_in_memory().expect("store");
        let resolution = store
            .resolve_workspace(workspace.path())
            .expect("resolution");
        let project_id = resolution.snapshot.project.id;
        let workspace_key = resolution.workspace_key;
        let first = resolution
            .snapshot
            .active_session_id
            .expect("initial session");
        let second = store
            .create_session(project_id, "100% literal _ title")
            .expect("second session");
        let third = store
            .create_session(project_id, "Duplicate")
            .expect("third session");
        store
            .set_session_title_with_event(
                &workspace_key,
                project_id,
                second.id,
                "Pinned Unicode 你好",
            )
            .expect("title");
        store
            .set_session_pinned_with_event(&workspace_key, project_id, second.id, true)
            .expect("pin");
        let page = store
            .list_sessions(project_id, "", SessionArchiveFilter::Active, 1, None)
            .expect("first page");
        assert_eq!(page.rows.len(), 1);
        assert_eq!(page.rows[0].id, second.id);
        assert!(page.has_more);
        assert!(page.next_cursor.is_some());
        let mismatch = store.list_sessions(
            project_id,
            "Other",
            SessionArchiveFilter::Active,
            1,
            page.next_cursor.as_deref(),
        );
        assert!(
            matches!(mismatch, Err(StoreError::InvalidRecord(message)) if message.contains("cursor"))
        );
        let literal = store
            .list_sessions(project_id, "100%", SessionArchiveFilter::Active, 20, None)
            .expect("literal query");
        assert!(literal.rows.iter().all(|row| row.title.contains("100%")));
        store
            .set_session_archived_with_event(&workspace_key, project_id, first, true)
            .expect("archive");
        assert!(store
            .list_sessions(project_id, "", SessionArchiveFilter::Active, 20, None)
            .expect("active list")
            .rows
            .iter()
            .all(|row| row.id != first));
        let archived = store
            .inspect_session(project_id, first)
            .expect("archived inspect");
        assert!(archived.archived);
        assert_eq!(archived.id, first);
        assert!(store.select_session(project_id, first).is_err());
        store
            .set_session_archived_with_event(&workspace_key, project_id, first, false)
            .expect("unarchive");
        store
            .select_session(project_id, first)
            .expect("select after unarchive");
        let all = store
            .list_sessions(project_id, "", SessionArchiveFilter::All, 20, None)
            .expect("all list");
        assert!(all.rows.iter().any(|row| row.id == third.id));
        assert_eq!(
            store
                .inspect_session(project_id, second.id)
                .expect("restart-safe metadata")
                .title,
            "Pinned Unicode 你好"
        );
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
        assert_eq!(schema_version, 22);
    }

    #[test]
    fn fresh_schema_is_complete_and_reopen_preserves_identity() {
        let directory = tempfile::tempdir().expect("directory");
        let database = directory.path().join("fresh.db");
        let (tables, indexes, identity) = {
            let store = Store::open(&database).expect("fresh store");
            let connection = store.connection.lock().expect("connection");
            let version: u16 = connection
                .query_row("PRAGMA user_version", [], |row| row.get(0))
                .expect("schema version");
            assert_eq!(version, 22);
            let tables: i64 = connection
                .query_row(
                    "SELECT COUNT(*) FROM sqlite_master
                     WHERE type = 'table' AND name NOT LIKE 'sqlite_%'",
                    [],
                    |row| row.get(0),
                )
                .expect("table count");
            let indexes: i64 = connection
                .query_row(
                    "SELECT COUNT(*) FROM sqlite_master
                     WHERE type = 'index' AND sql IS NOT NULL",
                    [],
                    |row| row.get(0),
                )
                .expect("index count");
            let identity: String = connection
                .query_row(
                    "SELECT store_id FROM runtime_store_identity WHERE singleton = 1",
                    [],
                    |row| row.get(0),
                )
                .expect("store identity");
            (tables, indexes, identity)
        };
        assert_eq!((tables, indexes), (31, 29));
        let store = Store::open(&database).expect("reopened store");
        let connection = store.connection.lock().expect("connection");
        let reopened: String = connection
            .query_row(
                "SELECT store_id FROM runtime_store_identity WHERE singleton = 1",
                [],
                |row| row.get(0),
            )
            .expect("store identity after reopen");
        assert_eq!(reopened, identity);
    }

    #[test]
    fn failed_schema_migration_leaves_no_partial_schema() {
        let directory = tempfile::tempdir().expect("directory");
        let database = directory.path().join("atomic.db");
        let connection = Connection::open(&database).expect("database");
        connection
            .execute_batch("CREATE TABLE idx_event_log_type (value INTEGER);")
            .expect("conflicting object");
        drop(connection);

        assert!(
            Store::open(&database).is_err(),
            "conflicting index name must fail the migration"
        );

        let connection = Connection::open(&database).expect("database after failure");
        let version: u16 = connection
            .query_row("PRAGMA user_version", [], |row| row.get(0))
            .expect("schema version");
        assert_eq!(version, 0);
        let created: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master
                 WHERE type = 'table' AND name = 'event_log'",
                [],
                |row| row.get(0),
            )
            .expect("event_log count");
        assert_eq!(created, 0);
    }

    #[test]
    fn schema_v17_migration_preserves_existing_rows_and_adds_command_identities() {
        let directory = tempfile::tempdir().expect("directory");
        let database = directory.path().join("schema-17.db");
        let connection = Connection::open(&database).expect("schema 17 database");
        connection
            .execute_batch(
                "CREATE TABLE f08_preserved_evidence (
                     id INTEGER PRIMARY KEY,
                     value TEXT NOT NULL
                 );
                 INSERT INTO f08_preserved_evidence (id, value)
                 VALUES (1, 'preserve-f08');
                 PRAGMA user_version = 17;",
            )
            .expect("schema 17 fixture");
        drop(connection);

        let store = Store::open(&database).expect("schema 18 migration");
        let connection = store.connection.lock().expect("connection");
        assert_eq!(
            connection
                .query_row(
                    "SELECT value FROM f08_preserved_evidence WHERE id = 1",
                    [],
                    |row| row.get::<_, String>(0),
                )
                .expect("preserved evidence"),
            "preserve-f08"
        );
        assert_eq!(
            connection
                .query_row("PRAGMA user_version", [], |row| row.get::<_, u16>(0))
                .expect("schema version"),
            22
        );
        assert!(connection
            .query_row(
                "SELECT 1 FROM sqlite_master
                 WHERE type = 'table' AND name = 'command_identities'",
                [],
                |row| row.get::<_, u8>(0),
            )
            .is_ok());
    }

    #[test]
    fn interim_schema_v18_adds_execution_snapshot_columns_non_destructively() {
        let directory = tempfile::tempdir().expect("directory");
        let database = directory.path().join("schema-18-interim.db");
        let connection = Connection::open(&database).expect("interim schema database");
        connection
            .execute_batch(
                "CREATE TABLE command_identities (
                     project_id TEXT NOT NULL,
                     request_id TEXT NOT NULL,
                     canonicalization_version INTEGER NOT NULL,
                     fingerprint TEXT NOT NULL,
                     command_kind TEXT NOT NULL,
                     owner_generation INTEGER,
                     state TEXT NOT NULL,
                     session_id TEXT,
                     run_id TEXT,
                     task_id TEXT,
                     subject_id TEXT,
                     result_session_id TEXT,
                     outcome_kind TEXT,
                     result_payload TEXT,
                     result_available INTEGER NOT NULL,
                     replay_allowed INTEGER NOT NULL,
                     recovery_revision TEXT,
                     recovery_blocker TEXT,
                     accepted_at INTEGER NOT NULL,
                     updated_at INTEGER NOT NULL,
                     PRIMARY KEY(project_id, request_id)
                 );
                 INSERT INTO command_identities (
                     project_id, request_id, canonicalization_version, fingerprint,
                     command_kind, state, result_available, replay_allowed,
                     accepted_at, updated_at
                 ) VALUES (
                     'preserved-project', 'preserved-command', 1,
                     'aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa',
                     'pause_goal', 'interrupted', 0, 0, 1, 1
                 );
                 PRAGMA user_version = 18;",
            )
            .expect("interim schema fixture");
        drop(connection);

        let store = Store::open(&database).expect("interim schema migration");
        let connection = store.connection.lock().expect("connection");
        for column in [
            "provider_profile_id",
            "execution_snapshot_sha256",
            "definition_revision",
        ] {
            assert_eq!(
                connection
                    .query_row(
                        "SELECT COUNT(*) FROM pragma_table_info('command_identities')
                         WHERE name = ?1",
                        [column],
                        |row| row.get::<_, u8>(0),
                    )
                    .expect("column query"),
                1,
                "missing migrated column {column}"
            );
        }
        assert_eq!(
            connection
                .query_row(
                    "SELECT state FROM command_identities
                     WHERE project_id = 'preserved-project'
                       AND request_id = 'preserved-command'",
                    [],
                    |row| row.get::<_, String>(0),
                )
                .expect("preserved command identity"),
            "interrupted"
        );
    }

    #[test]
    fn schema_v10_prepares_private_blobs_then_finalizes_metadata_and_event_atomically() {
        let directory = tempfile::tempdir().expect("directory");
        let workspace = directory.path().join("workspace");
        std::fs::create_dir(&workspace).expect("workspace");
        let database = directory.path().join("argentum.db");
        let store = Store::open(&database).expect("store");
        let resolution = store.resolve_workspace(&workspace).expect("resolution");
        let project_id = resolution.snapshot.project.id;
        let session_id = resolution.snapshot.active_session_id.expect("session");
        let run_id = RunId::new_v4();
        let private_before = b"private-snapshot-content-491";
        let operation = test_change_operation(
            &resolution.workspace_key,
            project_id,
            session_id,
            run_id,
            private_before,
            b"public replacement",
        );

        store
            .prepare_change_operation(&operation)
            .expect("prepared journal");
        assert!(store
            .change_set(project_id, run_id)
            .expect("change set")
            .is_none());
        assert!(!store
            .events()
            .expect("events")
            .iter()
            .any(|event| matches!(event, AppEvent::FileChangeRecorded(_))));

        store
            .mark_change_operation_applied(operation.operation_id)
            .expect("applied");
        let record = store
            .finalize_change_operation(operation.operation_id)
            .expect("finalized");
        let snapshot = store
            .change_set(project_id, run_id)
            .expect("change set")
            .expect("stored change set");

        assert_eq!(snapshot.files, vec![record.clone()]);
        assert_eq!(snapshot.revision, 1);
        assert!(store.events().expect("events").iter().any(
            |event| matches!(event, AppEvent::FileChangeRecorded(stored) if stored == &record)
        ));
        assert_eq!(
            store
                .change_blob(project_id, &digest_bytes(private_before))
                .expect("private blob")
                .as_deref(),
            Some(private_before.as_slice())
        );
        assert!(!serde_json::to_vec(&record)
            .expect("record JSON")
            .windows(private_before.len())
            .any(|value| value == private_before));
    }

    #[test]
    fn unfinished_prepared_operation_survives_restart_for_visible_recovery() {
        let directory = tempfile::tempdir().expect("directory");
        let workspace = directory.path().join("workspace");
        std::fs::create_dir(&workspace).expect("workspace");
        let database = directory.path().join("argentum.db");
        let store = Store::open(&database).expect("store");
        let resolution = store.resolve_workspace(&workspace).expect("resolution");
        let project_id = resolution.snapshot.project.id;
        let session_id = resolution.snapshot.active_session_id.expect("session");
        let operation = test_change_operation(
            &resolution.workspace_key,
            project_id,
            session_id,
            RunId::new_v4(),
            b"before",
            b"after",
        );
        store
            .prepare_change_operation(&operation)
            .expect("prepared journal");
        drop(store);

        let reopened = Store::open(&database).expect("reopened");
        let unfinished = reopened
            .unfinished_change_operations(project_id)
            .expect("unfinished operations");

        assert_eq!(unfinished.len(), 1);
        assert_eq!(unfinished[0].operation_id, operation.operation_id);
        assert_eq!(
            unfinished[0].state,
            argentum_domain::ChangeRecoveryState::Prepared
        );
    }

    #[test]
    fn restore_finalization_commits_file_and_restore_events_together() {
        let directory = tempfile::tempdir().expect("directory");
        let workspace = directory.path().join("workspace");
        std::fs::create_dir(&workspace).expect("workspace");
        let store = Store::open_in_memory().expect("store");
        let resolution = store.resolve_workspace(&workspace).expect("resolution");
        let project_id = resolution.snapshot.project.id;
        let session_id = resolution.snapshot.active_session_id.expect("session");
        let run_id = RunId::new_v4();
        let source = test_change_operation(
            &resolution.workspace_key,
            project_id,
            session_id,
            run_id,
            b"before",
            b"after",
        );
        store
            .prepare_change_operation(&source)
            .expect("source prepared");
        store
            .mark_change_operation_applied(source.operation_id)
            .expect("source applied");
        let source_record = store
            .finalize_change_operation(source.operation_id)
            .expect("source finalized");

        let mut restore = test_change_operation(
            &resolution.workspace_key,
            project_id,
            session_id,
            run_id,
            b"after",
            b"before",
        );
        restore.origin = FileChangeOrigin::Restore;
        restore.restored_from = Some(source_record.id);
        store
            .prepare_change_operation(&restore)
            .expect("restore prepared");
        store
            .mark_change_operation_applied(restore.operation_id)
            .expect("restore applied");
        let (restore_record, result) = store
            .finalize_restore_change_operation(restore.operation_id, source_record.id)
            .expect("restore finalized");

        assert_eq!(result.restore_file_change_id, restore_record.id);
        assert_eq!(result.source_file_change_id, source_record.id);
        let events = store.events().expect("events");
        assert!(events.iter().any(
            |event| matches!(event, AppEvent::FileChangeRecorded(record) if record.id == restore_record.id)
        ));
        assert!(events
            .iter()
            .any(|event| matches!(event, AppEvent::RestoreCompleted(stored) if stored == &result)));
    }

    #[test]
    fn preparation_rejects_snapshot_content_over_the_per_run_limit() {
        let directory = tempfile::tempdir().expect("directory");
        let workspace = directory.path().join("workspace");
        std::fs::create_dir(&workspace).expect("workspace");
        let store = Store::open_in_memory().expect("store");
        let resolution = store.resolve_workspace(&workspace).expect("resolution");
        let project_id = resolution.snapshot.project.id;
        let session_id = resolution.snapshot.active_session_id.expect("session");
        let mut operation = test_change_operation(
            &resolution.workspace_key,
            project_id,
            session_id,
            RunId::new_v4(),
            b"before",
            b"after",
        );
        operation.before_bytes = Some(vec![b'x'; MAX_CHANGE_RUN_SNAPSHOT_BYTES + 1]);

        assert!(matches!(
            store.prepare_change_operation(&operation),
            Err(StoreError::InvalidRecord(message)) if message.contains("snapshot")
        ));
    }

    fn test_change_operation(
        workspace_key: &str,
        project_id: ProjectId,
        session_id: SessionId,
        run_id: RunId,
        before: &[u8],
        after: &[u8],
    ) -> PreparedChangeOperation {
        PreparedChangeOperation {
            operation_id: uuid::Uuid::new_v4(),
            workspace_key: workspace_key.into(),
            project_id,
            session_id,
            run_id,
            relative_path: "note.txt".into(),
            kind: argentum_domain::FileChangeKind::Modified,
            origin: argentum_domain::FileChangeOrigin::ToolCall,
            before: Some(argentum_domain::FileSnapshotRef {
                digest: digest_bytes(before),
                size_bytes: before.len() as u64,
                retained: true,
            }),
            after: Some(argentum_domain::FileSnapshotRef {
                digest: digest_bytes(after),
                size_bytes: after.len() as u64,
                retained: true,
            }),
            before_bytes: Some(before.to_vec()),
            after_bytes: Some(after.to_vec()),
            additions: 1,
            removals: 1,
            line_count_availability: LineCountAvailability::Unknown,
            restored_from: None,
        }
    }

    #[test]
    fn change_preparation_rejects_a_workspace_key_outside_the_project_scope() {
        let directory = tempfile::tempdir().expect("directory");
        let workspace = directory.path().join("workspace");
        std::fs::create_dir(&workspace).expect("workspace");
        let store = Store::open_in_memory().expect("store");
        let resolution = store.resolve_workspace(&workspace).expect("resolution");
        let mut operation = test_change_operation(
            "not-the-project-workspace",
            resolution.snapshot.project.id,
            resolution.snapshot.active_session_id.expect("session"),
            RunId::new_v4(),
            b"before",
            b"after",
        );
        operation.relative_path = "scoped.txt".into();

        assert!(matches!(
            store.prepare_change_operation(&operation),
            Err(StoreError::InvalidRecord(message)) if message.contains("workspace")
        ));
    }

    #[test]
    fn unretained_metadata_does_not_consume_the_unique_retained_run_budget() {
        let directory = tempfile::tempdir().expect("directory");
        let workspace = directory.path().join("workspace");
        std::fs::create_dir(&workspace).expect("workspace");
        let store = Store::open_in_memory().expect("store");
        let resolution = store.resolve_workspace(&workspace).expect("resolution");
        let project_id = resolution.snapshot.project.id;
        let session_id = resolution.snapshot.active_session_id.expect("session");
        let run_id = RunId::new_v4();
        let mut first = test_change_operation(
            &resolution.workspace_key,
            project_id,
            session_id,
            run_id,
            b"before",
            b"after-one",
        );
        first.before = Some(FileSnapshotRef {
            digest: Sha256Digest("11".repeat(32)),
            size_bytes: (MAX_CHANGE_RUN_SNAPSHOT_BYTES + 1) as u64,
            retained: false,
        });
        first.before_bytes = None;
        store
            .prepare_change_operation(&first)
            .expect("metadata-only operation");

        let mut second = test_change_operation(
            &resolution.workspace_key,
            project_id,
            session_id,
            run_id,
            b"before-two",
            b"after-two",
        );
        second.relative_path = "second.txt".into();
        store
            .prepare_change_operation(&second)
            .expect("retained content remains within budget");
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
            source_profile_id: Some("read-only".into()),
            sandbox_scope: SandboxScope::ReadOnly,
            approval_mode: ApprovalMode::Always,
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
            source_profile_id: None,
            sandbox_scope: SandboxScope::WorkspaceWrite,
            approval_mode: ApprovalMode::Always,
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
            .select_provider_profile(project_id, "llama-cpp")
            .expect("select default profile");

        assert_eq!(
            profiles.iter().filter(|profile| profile.selected).count(),
            1
        );
        assert!(profiles
            .iter()
            .any(|profile| profile.id == "llama-cpp" && profile.selected));
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
            profile.id == "llama-cpp"
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
            .select_provider_model(left.snapshot.project.id, "llama-cpp", "left-model")
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
            .any(|profile| profile.id == "llama-cpp" && profile.model == "left-model"));
        assert!(right_profiles.iter().any(|profile| {
            profile.id == "llama-cpp" && profile.model == argentum_domain::DEFAULT_MODEL
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
                store.select_provider_model(project_id, "llama-cpp", model),
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
            accounting: None,
            definition_revision: argentum_domain::GoalId::new_v4(),
            acceptance: None,
            owner_attestation: None,
            completion: None,
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

        let expected = Some(goal.clone());
        let mut concurrent = goal.clone();
        concurrent.next_action = "A newer owner update".into();
        concurrent.updated_at += time::Duration::seconds(1);
        store
            .save_goal(&concurrent)
            .expect("concurrent goal update");
        let mut stale = goal.clone();
        stale.lifecycle = argentum_domain::GoalLifecycle::Paused;
        stale.updated_at += time::Duration::seconds(2);
        assert!(matches!(
            store.save_goal_if_current(&stale, &expected),
            Err(StoreError::CommandExecutionSnapshotChanged)
        ));
        assert!(matches!(
            store.clear_goal_if_current(project_id, session_id, &expected),
            Err(StoreError::CommandExecutionSnapshotChanged)
        ));
        assert_eq!(
            store
                .goal(project_id, session_id)
                .expect("goal after rejected compare-and-set"),
            Some(concurrent)
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
            accounting: None,
            definition_revision: argentum_domain::GoalId::new_v4(),
            acceptance: None,
            owner_attestation: None,
            completion: None,
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

    #[test]
    fn phase4_grants_persist_restart_revoke_and_remain_project_scoped() {
        let directory = tempfile::tempdir().expect("directory");
        let database = directory.path().join("store.db");
        let left_root = directory.path().join("left");
        let right_root = directory.path().join("right");
        std::fs::create_dir(&left_root).expect("left workspace");
        std::fs::create_dir(&right_root).expect("right workspace");
        let store = Store::open(&database).expect("store");
        let left = store.resolve_workspace(&left_root).expect("left");
        let right = store.resolve_workspace(&right_root).expect("right");
        let run_id = RunId::new_v4();
        let created_at = now();
        let grant = ApprovalGrantRecord {
            id: argentum_domain::GrantId::new_v4(),
            audit_id: argentum_domain::AuditId::new_v4(),
            capability: Capability::ExecuteCommands,
            scope: GrantScope::Run,
            subject: GrantSubject::Run { run_id },
            created_at,
            expires_at: Some(created_at + time::Duration::minutes(5)),
            issuer: GrantIssuer::User,
            reason: "Run the requested checks".into(),
            revoked_at: None,
        };
        store
            .save_approval_grant(left.snapshot.project.id, &grant)
            .expect("save grant");
        assert_eq!(
            store
                .approval_grants(left.snapshot.project.id, 16)
                .expect("left grants"),
            vec![grant.clone()]
        );
        assert!(store
            .approval_grants(right.snapshot.project.id, 16)
            .expect("right grants")
            .is_empty());

        drop(store);
        let reopened = Store::open(&database).expect("reopened store");
        assert_eq!(
            reopened
                .approval_grants(left.snapshot.project.id, 16)
                .expect("reopened grants"),
            vec![grant.clone()]
        );
        let revoked_at = now();
        let revoked = reopened
            .revoke_approval_grant(left.snapshot.project.id, grant.id, revoked_at)
            .expect("revoke grant")
            .expect("revoked record");
        assert_eq!(revoked.revoked_at, Some(revoked_at));
    }

    #[test]
    fn scoped_receipt_schema_requires_complete_installed_and_generated_identities_after_reload() {
        use argentum_domain::goal_supervision::{
            VerificationInputScope, VerificationInputSnapshot,
        };
        let directory = tempfile::tempdir().unwrap();
        let workspace = directory.path().join("workspace");
        std::fs::create_dir(&workspace).unwrap();
        let database = directory.path().join("scoped-receipts.sqlite3");
        let store = Store::open(&database).unwrap();
        let resolution = store.resolve_workspace(&workspace).unwrap();
        let project = resolution.snapshot.project.id;
        let session = resolution.snapshot.active_session_id.unwrap();
        // Structural persistence control only. The CLI control observes real bytes.
        let scope:VerificationInputScope=serde_json::from_value(serde_json::json!({
            "method":argentum_domain::verification_inputs::PROJECT_VERIFICATION_METHOD,
            "excluded_components":[".git","target"],"max_files":16384,"max_total_bytes":67108864,"max_file_bytes":16777216,
            "program_path":std::env::current_exe().unwrap(),"project_inputs":{
                "program":{"sha256":"a".repeat(64),"bytes":1},"dependency_roots":["node_modules"],"generated_output_roots":["dist"],
                "dependency_limits":{"max_files":4,"max_total_bytes":4096,"max_file_bytes":4096},
                "generated_limits":{"max_files":4,"max_total_bytes":4096,"max_file_bytes":4096},
                "inventory_limits":{"max_entries":32,"max_depth":8}
            }
        })).unwrap();
        let source:VerificationInputSnapshot=serde_json::from_value(serde_json::json!({
            "method":scope.method,"digest":"c".repeat(64),"file_count":1,"total_bytes":1,"dependencies":[
                {"path":"node_modules","exists":true,"digest":"b".repeat(64),"file_count":1,"total_bytes":1},
                {"path":"[verifier_program]","exists":true,"digest":"a".repeat(64),"file_count":1,"total_bytes":1}
            ]
        })).unwrap();
        let receipt:VerificationEvidence=serde_json::from_value(serde_json::json!({
            "id":Uuid::new_v4(),"run_id":Uuid::new_v4(),"runner":"project:node-control","registry_version":1,
            "program":"node.exe","args":["test.js"],"state":"passed","started_at":now(),"duration_ms":1,"exit_code":0,
            "revision_before":source.digest,"revision_after":source.digest,"stdout_sha256":"d".repeat(64),"stderr_sha256":"e".repeat(64),
            "stdout_bytes":0,"stderr_bytes":0,"stdout_truncated":false,"stderr_truncated":false,"workspace_key":"ephemeral-workspace",
            "definition_sha256":"f".repeat(64),"input_scope":scope,"input_before":source,"input_after":source,
            "generated_outputs":{
                "before":[{"path":"dist","exists":false,"digest":"a".repeat(64),"file_count":0,"total_bytes":0}],
                "after":[{"path":"dist","exists":true,"digest":"b".repeat(64),"file_count":1,"total_bytes":1}]
            }
        })).unwrap();
        store
            .save_verification_evidence(project, session, &receipt)
            .unwrap();
        drop(store);
        let reopened = Store::open(&database).unwrap();
        assert_eq!(
            reopened
                .verification_evidence(project, Some(receipt.run_id), 4)
                .unwrap(),
            vec![receipt.clone()]
        );
        let mut wrong = receipt.clone();
        wrong.generated_outputs = None;
        assert!(validate_verification_evidence(&wrong).is_err());
        let mut wrong = receipt.clone();
        wrong.input_after.as_mut().unwrap().dependencies.pop();
        assert!(validate_verification_evidence(&wrong).is_err());
        let mut wrong = receipt.clone();
        wrong.generated_outputs.as_mut().unwrap().after[0].path = "other".into();
        assert!(validate_verification_evidence(&wrong).is_err());
        let mut wrong = receipt;
        wrong.generated_outputs.as_mut().unwrap().after[0].total_bytes = 4097;
        assert!(validate_verification_evidence(&wrong).is_err());
    }

    #[test]
    fn phase4_audit_and_verification_evidence_are_bounded_ordered_and_output_safe() {
        let directory = tempfile::tempdir().expect("directory");
        let workspace = directory.path().join("workspace");
        std::fs::create_dir(&workspace).expect("workspace");
        let database = directory.path().join("receipts.sqlite3");
        let store = Store::open(&database).expect("store");
        let resolution = store.resolve_workspace(&workspace).expect("resolution");
        let project_id = resolution.snapshot.project.id;
        let session_id = resolution
            .snapshot
            .active_session_id
            .expect("active session");
        let run_id = RunId::new_v4();
        let tool_call_id = argentum_domain::ToolCallId::new_v4();
        let started_at = now();
        let audit = ExecutionAuditRecord {
            id: argentum_domain::AuditId::new_v4(),
            project_id,
            session_id,
            run_id,
            tool_call_id,
            tool_id: "execute_argv".into(),
            profile_id: "developer".into(),
            sandbox_scope: SandboxScope::WorkspaceWrite,
            approval_mode: ApprovalMode::OnRisk,
            capabilities: vec![Capability::ExecuteCommands, Capability::ExternalProcess],
            grant_ids: Vec::new(),
            target: "cargo".into(),
            outcome: ExecutionOutcome::Succeeded,
            started_at,
            finished_at: started_at + time::Duration::milliseconds(20),
            exit_code: Some(0),
            stdout_sha256: Some(Sha256Digest("a".repeat(64))),
            stderr_sha256: Some(Sha256Digest("b".repeat(64))),
            stdout_bytes: 42,
            stderr_bytes: 0,
            stdout_truncated: false,
            stderr_truncated: false,
            changed_paths: vec!["src/lib.rs".into()],
        };
        store.record_execution_audit(&audit).expect("record audit");
        assert_eq!(
            store
                .execution_audit(project_id, Some(run_id), 16)
                .expect("audit records"),
            vec![audit.clone()]
        );

        let evidence = VerificationEvidence {
            id: argentum_domain::VerificationId::new_v4(),
            run_id,
            runner: VerificationRunnerId::CargoTestLocked,
            registry_version: 1,
            program: "cargo".into(),
            args: vec!["test".into(), "--workspace".into(), "--locked".into()],
            state: VerificationState::Passed,
            started_at,
            duration_ms: 20,
            exit_code: Some(0),
            revision_before: Sha256Digest("c".repeat(64)),
            revision_after: Sha256Digest("c".repeat(64)),
            stdout_sha256: Sha256Digest("d".repeat(64)),
            stderr_sha256: Sha256Digest("e".repeat(64)),
            stdout_bytes: 42,
            stderr_bytes: 0,
            stdout_truncated: false,
            stderr_truncated: false,
            workspace_key: String::new(),
            definition_sha256: None,
            input_scope: None,
            input_before: None,
            input_after: None,
            generated_outputs: None,
        };
        store
            .save_verification_evidence(project_id, session_id, &evidence)
            .expect("save evidence");
        store
            .save_verification_evidence(project_id, session_id, &evidence)
            .expect("same-owner terminal receipt redelivery");
        let mut conflicting = evidence.clone();
        conflicting.state = VerificationState::Failed;
        conflicting.exit_code = Some(1);
        assert!(matches!(
            store.save_verification_evidence(project_id, session_id, &conflicting),
            Err(StoreError::InvalidRecord(_))
        ));
        conflicting = evidence.clone();
        conflicting.run_id = RunId::new_v4();
        assert!(matches!(
            store.save_verification_evidence(project_id, session_id, &conflicting),
            Err(StoreError::InvalidRecord(_))
        ));
        let other_workspace = directory.path().join("other-workspace");
        std::fs::create_dir(&other_workspace).expect("other workspace");
        let other = store
            .resolve_workspace(&other_workspace)
            .expect("other project");
        assert!(matches!(
            store.save_verification_evidence(
                other.snapshot.project.id,
                other.snapshot.active_session_id.expect("other session"),
                &evidence,
            ),
            Err(StoreError::InvalidRecord(_))
        ));
        assert_eq!(
            store
                .verification_evidence(project_id, Some(run_id), 16)
                .expect("verification records"),
            vec![evidence.clone()]
        );

        drop(store);
        let store = Store::open(&database).expect("reloaded store");
        store
            .save_verification_evidence(project_id, session_id, &evidence)
            .expect("redelivery after store reload");
        assert_eq!(
            store
                .verification_evidence(project_id, Some(run_id), 16)
                .expect("retained receipt"),
            vec![evidence]
        );

        let connection = store.connection.lock().expect("connection");
        let audit_payload: String = connection
            .query_row("SELECT payload FROM execution_audit", [], |row| row.get(0))
            .expect("audit payload");
        let verification_payload: String = connection
            .query_row("SELECT payload FROM verification_evidence", [], |row| {
                row.get(0)
            })
            .expect("verification payload");
        assert!(!audit_payload.contains("stdout_text"));
        assert!(!verification_payload.contains("stdout_text"));
    }

    #[test]
    fn phase4_store_rejects_malformed_grant_and_audit_records() {
        let directory = tempfile::tempdir().expect("directory");
        let workspace = directory.path().join("workspace");
        std::fs::create_dir(&workspace).expect("workspace");
        let store = Store::open_in_memory().expect("store");
        let resolution = store.resolve_workspace(&workspace).expect("resolution");
        let project_id = resolution.snapshot.project.id;
        let created_at = now();
        let grant = ApprovalGrantRecord {
            id: argentum_domain::GrantId::new_v4(),
            audit_id: argentum_domain::AuditId::new_v4(),
            capability: Capability::WriteFiles,
            scope: GrantScope::Run,
            subject: GrantSubject::Workspace { project_id },
            created_at,
            expires_at: None,
            issuer: GrantIssuer::User,
            reason: "scope mismatch".into(),
            revoked_at: None,
        };
        assert!(matches!(
            store.save_approval_grant(project_id, &grant),
            Err(StoreError::InvalidRecord(_))
        ));

        let malformed_binding = ApprovalGrantRecord {
            id: argentum_domain::GrantId::new_v4(),
            audit_id: argentum_domain::AuditId::new_v4(),
            capability: Capability::ExecuteCommands,
            scope: GrantScope::Action,
            subject: GrantSubject::Action {
                tool_id: "execute_argv".into(),
                target: "cargo test".into(),
                binding: argentum_domain::Sha256Digest("not-a-sha256-digest".into()),
            },
            created_at,
            expires_at: None,
            issuer: GrantIssuer::User,
            reason: "exact action".into(),
            revoked_at: None,
        };
        assert!(matches!(
            store.save_approval_grant(project_id, &malformed_binding),
            Err(StoreError::InvalidRecord(_))
        ));
    }
}

#[cfg(test)]
mod line_count_provenance_tests {
    use super::*;

    fn record(store: &Store, workspace: &Path) -> (FileChangeRecord, ChangeOperationId) {
        let scope = store.resolve_workspace(workspace).unwrap();
        let before = b"one\n";
        let after = b"two\n";
        let snapshot = |bytes: &[u8]| FileSnapshotRef {
            digest: digest_bytes(bytes),
            size_bytes: bytes.len() as u64,
            retained: true,
        };
        let operation = PreparedChangeOperation {
            operation_id: ChangeOperationId::new_v4(),
            workspace_key: scope.workspace_key,
            project_id: scope.snapshot.project.id,
            session_id: scope.snapshot.active_session_id.unwrap(),
            run_id: RunId::new_v4(),
            relative_path: "recorded.txt".into(),
            kind: FileChangeKind::Modified,
            origin: FileChangeOrigin::ToolCall,
            before: Some(snapshot(before)),
            after: Some(snapshot(after)),
            before_bytes: Some(before.to_vec()),
            after_bytes: Some(after.to_vec()),
            additions: 1,
            removals: 1,
            line_count_availability: LineCountAvailability::Computed,
            restored_from: None,
        };
        store.prepare_change_operation(&operation).unwrap();
        store
            .mark_change_operation_applied(operation.operation_id)
            .unwrap();
        (
            store
                .finalize_change_operation(operation.operation_id)
                .unwrap(),
            operation.operation_id,
        )
    }

    #[test]
    fn recorded_provenance_survives_restart_pruning_and_readonly_inspection() {
        let root = tempfile::tempdir().unwrap();
        let workspace = root.path().join("workspace");
        std::fs::create_dir(&workspace).unwrap();
        let database = root.path().join("store.db");
        let store = Store::open(&database).unwrap();
        let (original, operation) = record(&store, &workspace);
        assert_eq!(
            original.line_count_availability,
            LineCountAvailability::Computed
        );
        drop(store);
        let connection = Connection::open(&database).unwrap();
        let payload_before: String = connection
            .query_row(
                "SELECT payload FROM file_changes WHERE id=?1",
                [original.id.to_string()],
                |r| r.get(0),
            )
            .unwrap();
        connection.execute("DELETE FROM change_blobs", []).unwrap();
        drop(connection);
        std::fs::write(
            workspace.join("recorded.txt"),
            b"later\nworkspace\ncontents\n",
        )
        .unwrap();
        let readonly = Store::open_existing_read_only(&database).unwrap();
        let loaded = readonly
            .file_change(original.project_id, original.id)
            .unwrap()
            .unwrap();
        assert_eq!(loaded, original);
        assert!(readonly
            .change_blob(
                original.project_id,
                &original.before.as_ref().unwrap().digest
            )
            .unwrap()
            .is_none());
        assert_eq!(
            readonly
                .change_set(original.project_id, original.run_id)
                .unwrap()
                .unwrap()
                .files[0],
            original
        );
        drop(readonly);
        let reopened = Store::open(&database).unwrap();
        assert_eq!(
            reopened
                .file_change(original.project_id, original.id)
                .unwrap()
                .unwrap(),
            original
        );
        let connection = reopened.connection.lock().unwrap();
        assert_eq!(
            load_change_operation_on(&connection, operation)
                .unwrap()
                .unwrap()
                .line_count_availability,
            LineCountAvailability::Computed
        );
        let payload_after: String = connection
            .query_row(
                "SELECT payload FROM file_changes WHERE id=?1",
                [original.id.to_string()],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(payload_before, payload_after);
    }

    #[test]
    fn legacy_payload_and_operation_remain_unknown_without_rewriting_numbers() {
        let root = tempfile::tempdir().unwrap();
        let workspace = root.path().join("workspace");
        std::fs::create_dir(&workspace).unwrap();
        let database = root.path().join("store.db");
        let store = Store::open(&database).unwrap();
        let (original, operation) = record(&store, &workspace);
        let mut legacy = serde_json::to_value(&original).unwrap();
        legacy
            .as_object_mut()
            .unwrap()
            .remove("line_count_availability");
        let encoded = legacy.to_string();
        {
            let connection = store.connection.lock().unwrap();
            connection
                .execute(
                    "UPDATE file_changes SET payload=?1 WHERE id=?2",
                    params![encoded, original.id.to_string()],
                )
                .unwrap();
            connection
                .execute(
                    "UPDATE change_operations SET line_count_availability=NULL WHERE id=?1",
                    [operation.to_string()],
                )
                .unwrap();
        }
        drop(store);
        let readonly = Store::open_existing_read_only(&database).unwrap();
        let loaded = readonly
            .file_change(original.project_id, original.id)
            .unwrap()
            .unwrap();
        assert_eq!(
            loaded.line_count_availability,
            LineCountAvailability::Unknown
        );
        assert_eq!(
            (loaded.id, loaded.additions, loaded.removals),
            (original.id, 1, 1)
        );
        let connection = readonly.connection.lock().unwrap();
        assert_eq!(
            load_change_operation_on(&connection, operation)
                .unwrap()
                .unwrap()
                .line_count_availability,
            LineCountAvailability::Unknown
        );
        let after: String = connection
            .query_row(
                "SELECT payload FROM file_changes WHERE id=?1",
                [original.id.to_string()],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(after, encoded);
    }
}
