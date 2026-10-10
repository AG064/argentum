use rusqlite::{params, Connection, OptionalExtension, TransactionBehavior};
use sha2::{Digest, Sha256};
use time::OffsetDateTime;
use uuid::Uuid;

use super::{
    append_event_on, ensure_project_exists, insert_session, new_session_record,
    session_forks::create_session_fork_on, Store, StoreError,
};

pub const COMMAND_IDENTITY_VERSION: u16 = 1;
pub const MAX_COMMAND_REQUEST_ID_BYTES: usize = 128;
pub const MAX_COMMAND_CANONICAL_BYTES: usize = 256 * 1024;
pub const MAX_COMMAND_RESULT_BYTES: usize = 16 * 1024;
pub const MAX_COMMAND_IDENTITIES_PER_PROJECT: usize = 4096;
pub const MAX_COMMAND_IDENTITIES_PER_STORE: usize = 16_384;
pub const MAX_ACTIVE_COMMAND_IDENTITIES_PER_PROJECT: usize = 32;
pub const MAX_ACTIVE_COMMAND_IDENTITIES_PER_STORE: usize = 128;
pub const MAX_RETAINED_COMMAND_RESULT_BODIES_PER_PROJECT: usize = 512;
pub const MAX_RETAINED_COMMAND_RESULT_BODIES_PER_STORE: usize = 2048;
pub const MAX_COMMAND_RECOVERY_BATCH: usize = 64;
pub const MAX_COMMAND_INSPECTION_LIMIT: usize = 100;

const FINGERPRINT_DOMAIN: &[u8] = b"argentum-jsonl-command-identity\0";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommandIdentityState {
    Accepted,
    Executing,
    Settled,
    Interrupted,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommandOutcomeKind {
    Success,
    Failure,
    Cancelled,
    Denied,
    Interrupted,
    Unknown,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CommandIdentityScope {
    pub session_id: Option<Uuid>,
    /// A run created by this command. A cancellation target belongs in
    /// `subject_id`, so recovery never mistakes it for this command's run.
    pub run_id: Option<Uuid>,
    pub task_id: Option<Uuid>,
    pub subject_id: Option<Uuid>,
    /// Non-secret profile reference captured with the execution snapshot.
    pub provider_profile_id: Option<String>,
    /// SHA-256 binding for relevant provider, policy, budget, or goal state.
    pub execution_snapshot_sha256: Option<String>,
    pub definition_revision: Option<Uuid>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandClaimInput {
    pub project_id: Uuid,
    pub request_id: String,
    pub canonicalization_version: u16,
    pub canonical_command: Vec<u8>,
    pub command_kind: String,
    pub scope: CommandIdentityScope,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredCommandIdentity {
    pub project_id: Uuid,
    pub request_id: String,
    pub canonicalization_version: u16,
    pub command_kind: String,
    pub owner_generation: Option<u64>,
    pub state: CommandIdentityState,
    pub scope: CommandIdentityScope,
    pub result_session_id: Option<Uuid>,
    pub outcome_kind: Option<CommandOutcomeKind>,
    pub result_payload: Option<String>,
    pub result_available: bool,
    pub replay_allowed: bool,
    pub recovery_revision: Option<Uuid>,
    pub recovery_blocker: Option<String>,
    pub accepted_at: OffsetDateTime,
    pub updated_at: OffsetDateTime,
    fingerprint: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CommandIdentityMatch {
    Missing,
    Equal(Box<StoredCommandIdentity>),
    Conflict,
    UnsupportedVersion { stored_version: u16 },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CommandClaim {
    Claimed(StoredCommandIdentity),
    Existing(StoredCommandIdentity),
    Conflict,
    UnsupportedVersion { stored_version: u16 },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NewSessionCommandClaim {
    Claimed {
        identity: StoredCommandIdentity,
        session: argentum_domain::Session,
    },
    Existing(StoredCommandIdentity),
    Conflict,
    UnsupportedVersion {
        stored_version: u16,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ForkSessionCommandClaim {
    Claimed {
        identity: StoredCommandIdentity,
        session: argentum_domain::Session,
        fork: Box<argentum_domain::SessionForkSnapshot>,
    },
    Existing(StoredCommandIdentity),
    Conflict,
    UnsupportedVersion {
        stored_version: u16,
    },
}

impl Store {
    pub fn match_command_identity(
        &self,
        project_id: Uuid,
        request_id: &str,
        canonicalization_version: u16,
        canonical_command: &[u8],
    ) -> Result<CommandIdentityMatch, StoreError> {
        validate_identity_material(
            project_id,
            request_id,
            canonicalization_version,
            canonical_command,
            "lookup",
        )?;
        let fingerprint = command_fingerprint(canonicalization_version, canonical_command);
        let connection = self
            .connection
            .lock()
            .map_err(|_| StoreError::LockPoisoned)?;
        match read_command_identity_on(&connection, project_id, request_id)? {
            None => Ok(CommandIdentityMatch::Missing),
            Some(record) if record.canonicalization_version != canonicalization_version => {
                Ok(CommandIdentityMatch::UnsupportedVersion {
                    stored_version: record.canonicalization_version,
                })
            }
            Some(record) if record.fingerprint == fingerprint => {
                Ok(CommandIdentityMatch::Equal(Box::new(record)))
            }
            Some(_) => Ok(CommandIdentityMatch::Conflict),
        }
    }

    pub fn command_identity(
        &self,
        project_id: Uuid,
        request_id: &str,
    ) -> Result<Option<StoredCommandIdentity>, StoreError> {
        validate_request_id(request_id)?;
        if project_id.is_nil() {
            return Err(StoreError::InvalidRecord(
                "invalid command project identity".into(),
            ));
        }
        let connection = self
            .connection
            .lock()
            .map_err(|_| StoreError::LockPoisoned)?;
        read_command_identity_on(&connection, project_id, request_id)
    }

    pub fn claim_command_identity(
        &self,
        input: &CommandClaimInput,
    ) -> Result<CommandClaim, StoreError> {
        validate_claim_input(input)?;
        let owner_generation = self.current_host_generation()?;
        let fingerprint =
            command_fingerprint(input.canonicalization_version, &input.canonical_command);
        let mut connection = self
            .connection
            .lock()
            .map_err(|_| StoreError::LockPoisoned)?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        validate_generation_on(&transaction, owner_generation)?;
        if let Some(existing) =
            read_command_identity_on(&transaction, input.project_id, &input.request_id)?
        {
            let claim = compare_existing(existing, input.canonicalization_version, &fingerprint);
            transaction.commit()?;
            return Ok(claim);
        }
        validate_scope_on(&transaction, input.project_id, &input.scope)?;
        enforce_admission_quotas_on(&transaction, input.project_id)?;
        let at = argentum_domain::now();
        insert_command_identity_on(
            &transaction,
            input,
            &fingerprint,
            CommandIdentityInsert {
                owner_generation,
                state: CommandIdentityState::Accepted,
                result_session_id: None,
                outcome_kind: None,
                result_payload: None,
                result_available: false,
                replay_allowed: true,
                at,
            },
        )?;
        transaction.commit()?;
        drop(connection);
        let record = self
            .command_identity(input.project_id, &input.request_id)?
            .ok_or_else(|| StoreError::InvalidRecord("command claim disappeared".into()))?;
        Ok(CommandClaim::Claimed(record))
    }

    /// The first NewSession claim, its session mutation, event, result
    /// reference, and settlement are one SQLite transaction.
    pub fn claim_new_session_command(
        &self,
        workspace_key: &str,
        input: &CommandClaimInput,
    ) -> Result<NewSessionCommandClaim, StoreError> {
        validate_claim_input(input)?;
        if input.command_kind != "new_session" || input.scope != CommandIdentityScope::default() {
            return Err(StoreError::InvalidRecord(
                "invalid new-session command binding".into(),
            ));
        }
        if workspace_key.is_empty() || workspace_key.len() > 32 * 1024 {
            return Err(StoreError::InvalidRecord(
                "invalid command workspace identity".into(),
            ));
        }
        let owner_generation = self.current_host_generation()?;
        let fingerprint =
            command_fingerprint(input.canonicalization_version, &input.canonical_command);
        let mut connection = self
            .connection
            .lock()
            .map_err(|_| StoreError::LockPoisoned)?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        validate_generation_on(&transaction, owner_generation)?;
        if let Some(existing) =
            read_command_identity_on(&transaction, input.project_id, &input.request_id)?
        {
            let answer =
                match compare_existing(existing, input.canonicalization_version, &fingerprint) {
                    CommandClaim::Existing(record) => NewSessionCommandClaim::Existing(record),
                    CommandClaim::Conflict => NewSessionCommandClaim::Conflict,
                    CommandClaim::UnsupportedVersion { stored_version } => {
                        NewSessionCommandClaim::UnsupportedVersion { stored_version }
                    }
                    CommandClaim::Claimed(_) => {
                        unreachable!("existing command cannot be newly claimed")
                    }
                };
            transaction.commit()?;
            return Ok(answer);
        }
        let project_exists = transaction.query_row(
            "SELECT EXISTS(
                 SELECT 1 FROM projects WHERE id = ?1 AND workspace_key = ?2
             )",
            params![input.project_id.to_string(), workspace_key],
            |row| row.get::<_, bool>(0),
        )?;
        if !project_exists {
            return Err(StoreError::InvalidRecord("project does not exist".into()));
        }
        enforce_admission_quotas_on(&transaction, input.project_id)?;
        let session = new_session_record(input.project_id, "New session");
        insert_session(&transaction, &session)?;
        transaction.execute(
            "UPDATE projects SET active_session_id = ?1 WHERE id = ?2",
            params![session.id.to_string(), input.project_id.to_string()],
        )?;
        append_event_on(
            &transaction,
            Some(workspace_key),
            Some(input.project_id),
            Some(session.id),
            None,
            &argentum_domain::AppEvent::SessionCreated(session.clone()),
        )?;
        let at = argentum_domain::now();
        insert_command_identity_on(
            &transaction,
            input,
            &fingerprint,
            CommandIdentityInsert {
                owner_generation,
                state: CommandIdentityState::Settled,
                result_session_id: Some(session.id),
                outcome_kind: Some(CommandOutcomeKind::Success),
                result_payload: None,
                result_available: true,
                replay_allowed: true,
                at,
            },
        )?;
        transaction.commit()?;
        drop(connection);
        let identity = self
            .command_identity(input.project_id, &input.request_id)?
            .ok_or_else(|| StoreError::InvalidRecord("command claim disappeared".into()))?;
        Ok(NewSessionCommandClaim::Claimed { identity, session })
    }

    /// The first fork claim, exact cutoff, child, lineage, event, result
    /// reference, and settlement are one SQLite transaction.
    pub fn claim_fork_session_command(
        &self,
        workspace_key: &str,
        input: &CommandClaimInput,
        title: &str,
        expected_last_message_id: Option<argentum_domain::MessageId>,
    ) -> Result<ForkSessionCommandClaim, StoreError> {
        validate_claim_input(input)?;
        let source_session_id = input.scope.session_id.ok_or_else(|| {
            StoreError::InvalidRecord("fork command has no source session".into())
        })?;
        let expected_scope = CommandIdentityScope {
            session_id: Some(source_session_id),
            subject_id: Some(source_session_id),
            ..CommandIdentityScope::default()
        };
        if input.command_kind != "fork_session" || input.scope != expected_scope {
            return Err(StoreError::InvalidRecord(
                "invalid fork-session command binding".into(),
            ));
        }
        if workspace_key.is_empty() || workspace_key.len() > 32 * 1024 {
            return Err(StoreError::InvalidRecord(
                "invalid command workspace identity".into(),
            ));
        }
        let owner_generation = self.current_host_generation()?;
        let fingerprint =
            command_fingerprint(input.canonicalization_version, &input.canonical_command);
        let mut connection = self
            .connection
            .lock()
            .map_err(|_| StoreError::LockPoisoned)?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        validate_generation_on(&transaction, owner_generation)?;
        if let Some(existing) =
            read_command_identity_on(&transaction, input.project_id, &input.request_id)?
        {
            let answer =
                match compare_existing(existing, input.canonicalization_version, &fingerprint) {
                    CommandClaim::Existing(record) => ForkSessionCommandClaim::Existing(record),
                    CommandClaim::Conflict => ForkSessionCommandClaim::Conflict,
                    CommandClaim::UnsupportedVersion { stored_version } => {
                        ForkSessionCommandClaim::UnsupportedVersion { stored_version }
                    }
                    CommandClaim::Claimed(_) => {
                        unreachable!("existing command cannot be newly claimed")
                    }
                };
            transaction.commit()?;
            return Ok(answer);
        }
        enforce_admission_quotas_on(&transaction, input.project_id)?;
        let (session, fork) = create_session_fork_on(
            &transaction,
            workspace_key,
            input.project_id,
            source_session_id,
            title,
            expected_last_message_id,
        )?;
        let at = argentum_domain::now();
        insert_command_identity_on(
            &transaction,
            input,
            &fingerprint,
            CommandIdentityInsert {
                owner_generation,
                state: CommandIdentityState::Settled,
                result_session_id: Some(session.id),
                outcome_kind: Some(CommandOutcomeKind::Success),
                result_payload: None,
                result_available: true,
                replay_allowed: true,
                at,
            },
        )?;
        transaction.commit()?;
        drop(connection);
        let identity = self
            .command_identity(input.project_id, &input.request_id)?
            .ok_or_else(|| StoreError::InvalidRecord("command claim disappeared".into()))?;
        Ok(ForkSessionCommandClaim::Claimed {
            identity,
            session,
            fork: Box::new(fork),
        })
    }

    pub fn mark_command_executing(
        &self,
        project_id: Uuid,
        request_id: &str,
        canonicalization_version: u16,
        canonical_command: &[u8],
    ) -> Result<StoredCommandIdentity, StoreError> {
        self.transition_command(
            project_id,
            request_id,
            canonicalization_version,
            canonical_command,
            |transaction, record, generation| {
                if record.owner_generation != generation
                    || !matches!(record.state, CommandIdentityState::Accepted)
                {
                    return Err(StoreError::HostGenerationFenced);
                }
                transaction.execute(
                    "UPDATE command_identities
                     SET state = 'executing', updated_at = ?3
                     WHERE project_id = ?1 AND request_id = ?2 AND state = 'accepted'",
                    params![
                        project_id.to_string(),
                        request_id,
                        argentum_domain::now().unix_timestamp()
                    ],
                )?;
                Ok(())
            },
        )
    }

    pub fn settle_command_identity(
        &self,
        project_id: Uuid,
        request_id: &str,
        canonicalization_version: u16,
        canonical_command: &[u8],
        outcome_kind: CommandOutcomeKind,
        result_payload: Option<&str>,
    ) -> Result<StoredCommandIdentity, StoreError> {
        if matches!(
            outcome_kind,
            CommandOutcomeKind::Interrupted | CommandOutcomeKind::Unknown
        ) {
            return Err(StoreError::InvalidRecord(
                "terminal command settlement has an unresolved outcome".into(),
            ));
        }
        if result_payload.is_some_and(|payload| payload.len() > MAX_COMMAND_RESULT_BYTES) {
            return Err(StoreError::CommandResultTooLarge);
        }
        self.transition_command(
            project_id,
            request_id,
            canonicalization_version,
            canonical_command,
            |transaction, record, generation| {
                if record.owner_generation != generation
                    || !matches!(
                        record.state,
                        CommandIdentityState::Accepted | CommandIdentityState::Executing
                    )
                {
                    return Err(StoreError::HostGenerationFenced);
                }
                let at = argentum_domain::now().unix_timestamp();
                if transaction.execute(
                    "UPDATE command_identities
                     SET state = 'settled', outcome_kind = ?3,
                         result_payload = CASE WHEN replay_allowed = 1 THEN ?4 ELSE NULL END,
                         result_available = CASE WHEN replay_allowed = 1 THEN 1 ELSE 0 END,
                         updated_at = ?5
                     WHERE project_id = ?1 AND request_id = ?2
                       AND state IN ('accepted','executing')",
                    params![
                        project_id.to_string(),
                        request_id,
                        outcome_kind_name(outcome_kind),
                        result_payload,
                        at,
                    ],
                )? != 1
                {
                    return Err(StoreError::HostGenerationFenced);
                }
                enforce_result_body_quota_on(transaction, project_id)?;
                Ok(())
            },
        )
    }

    fn transition_command<F>(
        &self,
        project_id: Uuid,
        request_id: &str,
        canonicalization_version: u16,
        canonical_command: &[u8],
        transition: F,
    ) -> Result<StoredCommandIdentity, StoreError>
    where
        F: FnOnce(&Connection, &StoredCommandIdentity, Option<u64>) -> Result<(), StoreError>,
    {
        validate_identity_material(
            project_id,
            request_id,
            canonicalization_version,
            canonical_command,
            "transition",
        )?;
        let fingerprint = command_fingerprint(canonicalization_version, canonical_command);
        let generation = self.current_host_generation()?;
        let mut connection = self
            .connection
            .lock()
            .map_err(|_| StoreError::LockPoisoned)?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        validate_generation_on(&transaction, generation)?;
        let record = read_command_identity_on(&transaction, project_id, request_id)?
            .ok_or(StoreError::CommandIdentityMissing)?;
        if record.canonicalization_version != canonicalization_version
            || record.fingerprint != fingerprint
        {
            return Err(StoreError::CommandIdentityConflict);
        }
        transition(&transaction, &record, generation)?;
        transaction.commit()?;
        drop(connection);
        self.command_identity(project_id, request_id)?
            .ok_or(StoreError::CommandIdentityMissing)
    }

    /// Remove bounded retained result bodies while preserving durable identity
    /// tombstones. This never makes an accepted request ID executable again.
    pub fn prune_command_result_bodies(
        &self,
        project_id: Uuid,
        batch_limit: usize,
    ) -> Result<usize, StoreError> {
        if batch_limit == 0 || batch_limit > MAX_COMMAND_RECOVERY_BATCH {
            return Err(StoreError::InvalidRecord(
                "invalid command pruning batch".into(),
            ));
        }
        let mut connection = self
            .connection
            .lock()
            .map_err(|_| StoreError::LockPoisoned)?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let changed = transaction.execute(
            "UPDATE command_identities
             SET result_payload = NULL, result_available = 0, updated_at = ?3
             WHERE rowid IN (
                 SELECT rowid FROM command_identities
                 WHERE project_id = ?1 AND result_payload IS NOT NULL
                 ORDER BY updated_at ASC, request_id ASC LIMIT ?2
             )",
            params![
                project_id.to_string(),
                i64::try_from(batch_limit).unwrap_or(i64::MAX),
                argentum_domain::now().unix_timestamp(),
            ],
        )?;
        transaction.commit()?;
        Ok(changed)
    }

    /// Revoke full result disclosure without deleting the identity binding.
    /// When `erase_body` is true, retained content is removed immediately and
    /// only the compact, non-executable tombstone remains.
    pub fn restrict_command_result(
        &self,
        project_id: Uuid,
        request_id: &str,
        erase_body: bool,
    ) -> Result<StoredCommandIdentity, StoreError> {
        validate_request_id(request_id)?;
        if project_id.is_nil() {
            return Err(StoreError::InvalidRecord(
                "invalid command project identity".into(),
            ));
        }
        let mut connection = self
            .connection
            .lock()
            .map_err(|_| StoreError::LockPoisoned)?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        if transaction.execute(
            "UPDATE command_identities
             SET replay_allowed = 0,
                 result_payload = CASE WHEN ?3 THEN NULL ELSE result_payload END,
                 result_available = CASE WHEN ?3 THEN 0 ELSE result_available END,
                 updated_at = ?4
             WHERE project_id = ?1 AND request_id = ?2",
            params![
                project_id.to_string(),
                request_id,
                erase_body,
                argentum_domain::now().unix_timestamp()
            ],
        )? != 1
        {
            return Err(StoreError::CommandIdentityMissing);
        }
        transaction.commit()?;
        drop(connection);
        self.command_identity(project_id, request_id)?
            .ok_or(StoreError::CommandIdentityMissing)
    }
}

pub(super) fn validate_claim_input(input: &CommandClaimInput) -> Result<(), StoreError> {
    validate_identity_material(
        input.project_id,
        &input.request_id,
        input.canonicalization_version,
        &input.canonical_command,
        &input.command_kind,
    )?;
    if input.command_kind.is_empty()
        || input.command_kind.len() > 64
        || !input
            .command_kind
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte == b'_')
        || [
            input.scope.session_id,
            input.scope.run_id,
            input.scope.task_id,
            input.scope.subject_id,
            input.scope.definition_revision,
        ]
        .into_iter()
        .flatten()
        .any(|id| id.is_nil())
        || input.scope.provider_profile_id.as_ref().is_some_and(|id| {
            id.is_empty()
                || id.len() > 64
                || !id
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b'_'))
        })
        || input
            .scope
            .execution_snapshot_sha256
            .as_ref()
            .is_some_and(|digest| {
                digest.len() != 64
                    || !digest
                        .bytes()
                        .all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f'))
            })
        || (input.scope.provider_profile_id.is_some() || input.scope.definition_revision.is_some())
            && input.scope.execution_snapshot_sha256.is_none()
    {
        return Err(StoreError::InvalidRecord(
            "invalid command identity binding".into(),
        ));
    }
    Ok(())
}

fn validate_identity_material(
    project_id: Uuid,
    request_id: &str,
    canonicalization_version: u16,
    canonical_command: &[u8],
    label: &str,
) -> Result<(), StoreError> {
    validate_request_id(request_id)?;
    if project_id.is_nil()
        || canonicalization_version != COMMAND_IDENTITY_VERSION
        || canonical_command.is_empty()
        || canonical_command.len() > MAX_COMMAND_CANONICAL_BYTES
        || label.is_empty()
    {
        return Err(StoreError::InvalidRecord(
            "invalid command identity material".into(),
        ));
    }
    Ok(())
}

pub fn validate_request_id(request_id: &str) -> Result<(), StoreError> {
    if request_id.is_empty()
        || request_id.len() > MAX_COMMAND_REQUEST_ID_BYTES
        || request_id.trim() != request_id
        || request_id.chars().any(char::is_control)
    {
        return Err(StoreError::InvalidRecord(
            "invalid command request identity".into(),
        ));
    }
    Ok(())
}

fn validate_generation_on(
    connection: &Connection,
    generation: Option<u64>,
) -> Result<(), StoreError> {
    if let Some(generation) = generation {
        super::recovery::validate_host_generation_on(connection, generation)?;
    }
    Ok(())
}

fn validate_scope_on(
    connection: &Connection,
    project_id: Uuid,
    scope: &CommandIdentityScope,
) -> Result<(), StoreError> {
    ensure_project_exists(connection, project_id)?;
    if let Some(session_id) = scope.session_id {
        let belongs: bool = connection.query_row(
            "SELECT EXISTS(
                 SELECT 1 FROM sessions WHERE id = ?1 AND project_id = ?2
             )",
            params![session_id.to_string(), project_id.to_string()],
            |row| row.get(0),
        )?;
        if !belongs {
            return Err(StoreError::InvalidRecord(
                "command session does not belong to the project".into(),
            ));
        }
    }
    Ok(())
}

pub(super) fn enforce_admission_quotas_on(
    connection: &Connection,
    project_id: Uuid,
) -> Result<(), StoreError> {
    let (project_total, project_active): (u64, u64) = connection.query_row(
        "SELECT COUNT(*),
                SUM(CASE WHEN state IN ('accepted','executing') THEN 1 ELSE 0 END)
         FROM command_identities WHERE project_id = ?1",
        [project_id.to_string()],
        |row| Ok((row.get(0)?, row.get::<_, Option<u64>>(1)?.unwrap_or(0))),
    )?;
    let (store_total, store_active): (u64, u64) = connection.query_row(
        "SELECT COUNT(*),
                SUM(CASE WHEN state IN ('accepted','executing') THEN 1 ELSE 0 END)
         FROM command_identities",
        [],
        |row| Ok((row.get(0)?, row.get::<_, Option<u64>>(1)?.unwrap_or(0))),
    )?;
    enforce_admission_quota_counts(project_total, project_active, store_total, store_active)
}

fn enforce_admission_quota_counts(
    project_total: u64,
    project_active: u64,
    store_total: u64,
    store_active: u64,
) -> Result<(), StoreError> {
    if project_total >= MAX_COMMAND_IDENTITIES_PER_PROJECT as u64
        || store_total >= MAX_COMMAND_IDENTITIES_PER_STORE as u64
    {
        return Err(StoreError::CommandIdentityQuotaExceeded);
    }
    if project_active >= MAX_ACTIVE_COMMAND_IDENTITIES_PER_PROJECT as u64
        || store_active >= MAX_ACTIVE_COMMAND_IDENTITIES_PER_STORE as u64
    {
        return Err(StoreError::CommandAdmissionLimit);
    }
    Ok(())
}

fn enforce_result_body_quota_on(
    connection: &Connection,
    project_id: Uuid,
) -> Result<(), StoreError> {
    let project_retained: u64 = connection.query_row(
        "SELECT COUNT(*) FROM command_identities
         WHERE project_id = ?1 AND result_payload IS NOT NULL",
        [project_id.to_string()],
        |row| row.get(0),
    )?;
    let project_excess =
        project_retained.saturating_sub(MAX_RETAINED_COMMAND_RESULT_BODIES_PER_PROJECT as u64);
    if project_excess > 0 {
        prune_oldest_result_bodies_on(connection, Some(project_id), project_excess)?;
    }

    let store_retained: u64 = connection.query_row(
        "SELECT COUNT(*) FROM command_identities WHERE result_payload IS NOT NULL",
        [],
        |row| row.get(0),
    )?;
    let store_excess =
        store_retained.saturating_sub(MAX_RETAINED_COMMAND_RESULT_BODIES_PER_STORE as u64);
    if store_excess > 0 {
        prune_oldest_result_bodies_on(connection, None, store_excess)?;
    }
    Ok(())
}

fn prune_oldest_result_bodies_on(
    connection: &Connection,
    project_id: Option<Uuid>,
    excess: u64,
) -> Result<(), StoreError> {
    if excess > MAX_COMMAND_RECOVERY_BATCH as u64 {
        return Err(StoreError::CommandIdentityQuotaExceeded);
    }
    let limit = excess;
    let (where_clause, project) = project_id.map_or(("", String::new()), |project_id| {
        ("project_id = ?1 AND", project_id.to_string())
    });
    connection.execute(
        &format!(
            "UPDATE command_identities
         SET result_payload = NULL, result_available = 0
         WHERE rowid IN (
             SELECT rowid FROM command_identities
             WHERE {where_clause} result_payload IS NOT NULL
             ORDER BY updated_at ASC, project_id ASC, request_id ASC LIMIT ?2
         )"
        ),
        params![project, limit],
    )?;
    Ok(())
}

pub(super) struct CommandIdentityInsert<'a> {
    pub(super) owner_generation: Option<u64>,
    pub(super) state: CommandIdentityState,
    pub(super) result_session_id: Option<Uuid>,
    pub(super) outcome_kind: Option<CommandOutcomeKind>,
    pub(super) result_payload: Option<&'a str>,
    pub(super) result_available: bool,
    pub(super) replay_allowed: bool,
    pub(super) at: OffsetDateTime,
}

pub(super) fn insert_command_identity_on(
    connection: &Connection,
    input: &CommandClaimInput,
    fingerprint: &str,
    insert: CommandIdentityInsert<'_>,
) -> Result<(), StoreError> {
    connection.execute(
        "INSERT INTO command_identities (
             project_id, request_id, canonicalization_version, fingerprint,
             command_kind, owner_generation, state, session_id, run_id,
             task_id, subject_id, provider_profile_id,
             execution_snapshot_sha256, definition_revision,
             result_session_id, outcome_kind, result_payload,
             result_available, replay_allowed,
             accepted_at, updated_at
          ) VALUES (
             ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12,
             ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20, ?20
          )",
        params![
            input.project_id.to_string(),
            input.request_id,
            input.canonicalization_version,
            fingerprint,
            input.command_kind,
            insert.owner_generation,
            state_name(insert.state),
            input.scope.session_id.map(|id| id.to_string()),
            input.scope.run_id.map(|id| id.to_string()),
            input.scope.task_id.map(|id| id.to_string()),
            input.scope.subject_id.map(|id| id.to_string()),
            input.scope.provider_profile_id,
            input.scope.execution_snapshot_sha256,
            input.scope.definition_revision.map(|id| id.to_string()),
            insert.result_session_id.map(|id| id.to_string()),
            insert.outcome_kind.map(outcome_kind_name),
            insert.result_payload,
            insert.result_available,
            insert.replay_allowed,
            insert.at.unix_timestamp(),
        ],
    )?;
    Ok(())
}

pub(super) fn compare_existing(
    record: StoredCommandIdentity,
    version: u16,
    fingerprint: &str,
) -> CommandClaim {
    if record.canonicalization_version != version {
        CommandClaim::UnsupportedVersion {
            stored_version: record.canonicalization_version,
        }
    } else if record.fingerprint == fingerprint {
        CommandClaim::Existing(record)
    } else {
        CommandClaim::Conflict
    }
}

pub(super) fn read_command_identity_on(
    connection: &Connection,
    project_id: Uuid,
    request_id: &str,
) -> Result<Option<StoredCommandIdentity>, StoreError> {
    let row = connection
        .query_row(
            "SELECT canonicalization_version, fingerprint, command_kind,
                    owner_generation, state, session_id, run_id, task_id,
                    subject_id, provider_profile_id, execution_snapshot_sha256,
                    definition_revision, result_session_id, outcome_kind,
                    result_payload, result_available, replay_allowed,
                    recovery_revision, recovery_blocker, accepted_at, updated_at
             FROM command_identities
             WHERE project_id = ?1 AND request_id = ?2",
            params![project_id.to_string(), request_id],
            |row| {
                Ok((
                    row.get::<_, u16>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, Option<u64>>(3)?,
                    row.get::<_, String>(4)?,
                    row.get::<_, Option<String>>(5)?,
                    row.get::<_, Option<String>>(6)?,
                    row.get::<_, Option<String>>(7)?,
                    row.get::<_, Option<String>>(8)?,
                    row.get::<_, Option<String>>(9)?,
                    row.get::<_, Option<String>>(10)?,
                    row.get::<_, Option<String>>(11)?,
                    row.get::<_, Option<String>>(12)?,
                    row.get::<_, Option<String>>(13)?,
                    row.get::<_, Option<String>>(14)?,
                    row.get::<_, bool>(15)?,
                    row.get::<_, bool>(16)?,
                    row.get::<_, Option<String>>(17)?,
                    row.get::<_, Option<String>>(18)?,
                    row.get::<_, i64>(19)?,
                    row.get::<_, i64>(20)?,
                ))
            },
        )
        .optional()?;
    let Some((
        canonicalization_version,
        fingerprint,
        command_kind,
        owner_generation,
        state,
        session_id,
        run_id,
        task_id,
        subject_id,
        provider_profile_id,
        execution_snapshot_sha256,
        definition_revision,
        result_session_id,
        outcome_kind,
        result_payload,
        result_available,
        replay_allowed,
        recovery_revision,
        recovery_blocker,
        accepted_at,
        updated_at,
    )) = row
    else {
        return Ok(None);
    };
    if fingerprint.len() != 64
        || !fingerprint.bytes().all(|byte| byte.is_ascii_hexdigit())
        || command_kind.is_empty()
        || command_kind.len() > 64
        || provider_profile_id.as_ref().is_some_and(|id| {
            id.is_empty()
                || id.len() > 64
                || !id
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b'_'))
        })
        || execution_snapshot_sha256.as_ref().is_some_and(|digest| {
            digest.len() != 64
                || !digest
                    .bytes()
                    .all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f'))
        })
        || (provider_profile_id.is_some() || definition_revision.is_some())
            && execution_snapshot_sha256.is_none()
        || result_payload
            .as_ref()
            .is_some_and(|payload| payload.len() > MAX_COMMAND_RESULT_BYTES)
        || result_available && outcome_kind.is_none()
    {
        return Err(StoreError::InvalidRecord(
            "stored command identity is invalid".into(),
        ));
    }
    let state = parse_state(&state)?;
    let outcome_kind = outcome_kind
        .as_deref()
        .map(parse_outcome_kind)
        .transpose()?;
    if matches!(state, CommandIdentityState::Settled) && outcome_kind.is_none() {
        return Err(StoreError::InvalidRecord(
            "settled command identity has no outcome".into(),
        ));
    }
    Ok(Some(StoredCommandIdentity {
        project_id,
        request_id: request_id.to_owned(),
        canonicalization_version,
        command_kind,
        owner_generation,
        state,
        scope: CommandIdentityScope {
            session_id: parse_optional_uuid(session_id, "command session identity")?,
            run_id: parse_optional_uuid(run_id, "command run identity")?,
            task_id: parse_optional_uuid(task_id, "command task identity")?,
            subject_id: parse_optional_uuid(subject_id, "command subject identity")?,
            provider_profile_id,
            execution_snapshot_sha256,
            definition_revision: parse_optional_uuid(
                definition_revision,
                "command definition revision",
            )?,
        },
        result_session_id: parse_optional_uuid(
            result_session_id,
            "command result session identity",
        )?,
        outcome_kind,
        result_payload,
        result_available,
        replay_allowed,
        recovery_revision: parse_optional_uuid(recovery_revision, "command recovery revision")?,
        recovery_blocker,
        accepted_at: OffsetDateTime::from_unix_timestamp(accepted_at).map_err(|_| {
            StoreError::InvalidRecord("stored command acceptance time is invalid".into())
        })?,
        updated_at: OffsetDateTime::from_unix_timestamp(updated_at).map_err(|_| {
            StoreError::InvalidRecord("stored command update time is invalid".into())
        })?,
        fingerprint,
    }))
}

fn parse_optional_uuid(value: Option<String>, label: &str) -> Result<Option<Uuid>, StoreError> {
    value
        .map(|value| Uuid::parse_str(&value).map_err(|_| StoreError::InvalidRecord(label.into())))
        .transpose()
}

pub(super) fn command_fingerprint(version: u16, canonical_command: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(FINGERPRINT_DOMAIN);
    hasher.update(version.to_be_bytes());
    hasher.update(canonical_command);
    format!("{:x}", hasher.finalize())
}

pub(crate) fn state_name(state: CommandIdentityState) -> &'static str {
    match state {
        CommandIdentityState::Accepted => "accepted",
        CommandIdentityState::Executing => "executing",
        CommandIdentityState::Settled => "settled",
        CommandIdentityState::Interrupted => "interrupted",
        CommandIdentityState::Unknown => "unknown",
    }
}

fn parse_state(value: &str) -> Result<CommandIdentityState, StoreError> {
    match value {
        "accepted" => Ok(CommandIdentityState::Accepted),
        "executing" => Ok(CommandIdentityState::Executing),
        "settled" => Ok(CommandIdentityState::Settled),
        "interrupted" => Ok(CommandIdentityState::Interrupted),
        "unknown" => Ok(CommandIdentityState::Unknown),
        _ => Err(StoreError::InvalidRecord(
            "stored command state is invalid".into(),
        )),
    }
}

pub(crate) fn outcome_kind_name(kind: CommandOutcomeKind) -> &'static str {
    match kind {
        CommandOutcomeKind::Success => "success",
        CommandOutcomeKind::Failure => "failure",
        CommandOutcomeKind::Cancelled => "cancelled",
        CommandOutcomeKind::Denied => "denied",
        CommandOutcomeKind::Interrupted => "interrupted",
        CommandOutcomeKind::Unknown => "unknown",
    }
}

fn parse_outcome_kind(value: &str) -> Result<CommandOutcomeKind, StoreError> {
    match value {
        "success" => Ok(CommandOutcomeKind::Success),
        "failure" => Ok(CommandOutcomeKind::Failure),
        "cancelled" => Ok(CommandOutcomeKind::Cancelled),
        "denied" => Ok(CommandOutcomeKind::Denied),
        "interrupted" => Ok(CommandOutcomeKind::Interrupted),
        "unknown" => Ok(CommandOutcomeKind::Unknown),
        _ => Err(StoreError::InvalidRecord(
            "stored command outcome is invalid".into(),
        )),
    }
}

pub(crate) fn active_command_count_on(
    connection: &Connection,
    generation: u64,
) -> Result<u64, StoreError> {
    Ok(connection.query_row(
        "SELECT COUNT(*) FROM command_identities
         WHERE owner_generation = ?1 AND state IN ('accepted','executing')",
        [generation],
        |row| row.get(0),
    )?)
}

pub(crate) fn reconcile_commands_on(
    connection: &Connection,
    generation: u64,
    recovery_revision: Uuid,
    at: OffsetDateTime,
) -> Result<(usize, bool), StoreError> {
    let limit = MAX_COMMAND_RECOVERY_BATCH + 1;
    let mut statement = connection.prepare(
        "SELECT project_id, request_id, run_id
         FROM command_identities
         WHERE owner_generation = ?1 AND state IN ('accepted','executing')
         ORDER BY accepted_at ASC, request_id ASC LIMIT ?2",
    )?;
    let rows = statement
        .query_map(params![generation, limit as u64], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, Option<String>>(2)?,
            ))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    let truncated = rows.len() > MAX_COMMAND_RECOVERY_BATCH;
    let mut reconciled = 0;
    for (project_id, request_id, run_id) in rows.into_iter().take(MAX_COMMAND_RECOVERY_BATCH) {
        let mut state = "interrupted";
        let mut outcome = "interrupted";
        let mut result_available = false;
        let mut blocker = "owner_process_interrupted";
        if let Some(run_id) = run_id {
            let run_state = connection
                .query_row(
                    "SELECT state FROM runtime_run_ownership WHERE run_id = ?1",
                    [run_id],
                    |row| row.get::<_, String>(0),
                )
                .optional()?;
            match run_state.as_deref() {
                Some("complete") => {
                    state = "settled";
                    outcome = "success";
                    result_available = true;
                    blocker = "";
                }
                Some("failed") => {
                    state = "settled";
                    outcome = "failure";
                    result_available = true;
                    blocker = "";
                }
                Some("cancelled") => {
                    state = "settled";
                    outcome = "cancelled";
                    result_available = true;
                    blocker = "";
                }
                Some("interrupted" | "active") | None => {}
                Some(_) => {
                    state = "unknown";
                    outcome = "unknown";
                    blocker = "linked_run_state_invalid";
                }
            }
        }
        if connection.execute(
            "UPDATE command_identities
              SET state = ?4, outcome_kind = ?5, result_payload = NULL,
                  result_available = CASE WHEN replay_allowed = 1 THEN ?6 ELSE 0 END,
                 recovery_revision = ?7, recovery_blocker = NULLIF(?8, ''),
                 updated_at = ?9
             WHERE project_id = ?1 AND request_id = ?2
               AND owner_generation = ?3 AND state IN ('accepted','executing')",
            params![
                project_id,
                request_id,
                generation,
                state,
                outcome,
                result_available,
                recovery_revision.to_string(),
                blocker,
                at.unix_timestamp(),
            ],
        )? != 1
        {
            return Err(StoreError::RecoveryDecisionStale);
        }
        reconciled += 1;
    }
    Ok((reconciled, truncated))
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Barrier};

    use argentum_domain::ProviderProfile;

    use super::*;

    fn fixture() -> (tempfile::TempDir, Store, Uuid) {
        let workspace = tempfile::tempdir().expect("workspace");
        let store = Store::open_in_memory().expect("store");
        let resolution = store
            .resolve_workspace_with_default_provider(
                workspace.path(),
                &ProviderProfile::default_llama_cpp(),
            )
            .expect("workspace resolution");
        (workspace, store, resolution.snapshot.project.id)
    }

    fn input(project_id: Uuid, request_id: &str, canonical: &[u8]) -> CommandClaimInput {
        CommandClaimInput {
            project_id,
            request_id: request_id.into(),
            canonicalization_version: COMMAND_IDENTITY_VERSION,
            canonical_command: canonical.to_vec(),
            command_kind: "set_goal".into(),
            scope: CommandIdentityScope::default(),
        }
    }

    #[test]
    fn equal_claims_reuse_one_identity_and_changed_input_conflicts() {
        let (_workspace, store, project_id) = fixture();
        let first = input(project_id, "stable-command", br#"{"kind":"set_goal"}"#);
        assert!(matches!(
            store.claim_command_identity(&first).unwrap(),
            CommandClaim::Claimed(_)
        ));
        assert!(matches!(
            store.claim_command_identity(&first).unwrap(),
            CommandClaim::Existing(_)
        ));
        let changed = input(
            project_id,
            "stable-command",
            br#"{"kind":"set_goal","objective":"different"}"#,
        );
        assert_eq!(
            store.claim_command_identity(&changed).unwrap(),
            CommandClaim::Conflict
        );

        let executing = store
            .mark_command_executing(
                project_id,
                &first.request_id,
                first.canonicalization_version,
                &first.canonical_command,
            )
            .unwrap();
        assert_eq!(executing.state, CommandIdentityState::Executing);
        let settled = store
            .settle_command_identity(
                project_id,
                &first.request_id,
                first.canonicalization_version,
                &first.canonical_command,
                CommandOutcomeKind::Failure,
                Some(r#"{"type":"command_failed","code":"definite","message":"safe","recoverable":false}"#),
            )
            .unwrap();
        assert_eq!(settled.state, CommandIdentityState::Settled);
        assert!(settled.result_available);
        assert!(settled.replay_allowed);
    }

    #[test]
    fn execution_snapshot_binding_round_trips_and_malformed_rows_stay_non_executable() {
        let (_workspace, store, project_id) = fixture();
        let definition_revision = Uuid::new_v4();
        let mut command = input(project_id, "snapshot-binding", b"snapshot-binding");
        command.scope = CommandIdentityScope {
            provider_profile_id: Some("bound-profile".into()),
            execution_snapshot_sha256: Some("a".repeat(64)),
            definition_revision: Some(definition_revision),
            ..CommandIdentityScope::default()
        };
        let claimed = store.claim_command_identity(&command).unwrap();
        let CommandClaim::Claimed(claimed) = claimed else {
            panic!("new command binding");
        };
        assert_eq!(claimed.scope, command.scope);
        assert_eq!(
            store
                .command_identity(project_id, &command.request_id)
                .unwrap()
                .expect("persisted command")
                .scope,
            command.scope
        );

        store
            .connection
            .lock()
            .unwrap()
            .execute(
                "UPDATE command_identities SET execution_snapshot_sha256 = NULL
                 WHERE project_id = ?1 AND request_id = ?2",
                params![project_id.to_string(), command.request_id],
            )
            .unwrap();
        assert!(matches!(
            store.command_identity(project_id, "snapshot-binding"),
            Err(StoreError::InvalidRecord(_))
        ));
        assert_eq!(
            store
                .connection
                .lock()
                .unwrap()
                .query_row(
                    "SELECT COUNT(*) FROM command_identities
                     WHERE project_id = ?1 AND request_id = 'snapshot-binding'",
                    [project_id.to_string()],
                    |row| row.get::<_, u64>(0),
                )
                .unwrap(),
            1
        );

        let mut invalid = input(project_id, "missing-snapshot", b"missing-snapshot");
        invalid.scope.provider_profile_id = Some("bound-profile".into());
        assert!(matches!(
            store.claim_command_identity(&invalid),
            Err(StoreError::InvalidRecord(_))
        ));
        assert!(store
            .command_identity(project_id, "missing-snapshot")
            .unwrap()
            .is_none());
    }

    #[test]
    fn new_session_mutation_and_command_settlement_are_atomic_and_replayed() {
        let (workspace, store, project_id) = fixture();
        let before = store.workspace_snapshot(project_id).unwrap();
        let mut command = input(project_id, "new-session-once", br#"{"kind":"new_session"}"#);
        command.command_kind = "new_session".into();
        let (_, workspace_key) =
            super::super::canonical_workspace(workspace.path()).expect("workspace key");

        let created = store
            .claim_new_session_command(&workspace_key, &command)
            .unwrap();
        let created_id = match created {
            NewSessionCommandClaim::Claimed { identity, session } => {
                assert_eq!(identity.state, CommandIdentityState::Settled);
                assert_eq!(identity.result_session_id, Some(session.id));
                session.id
            }
            other => panic!("unexpected claim: {other:?}"),
        };
        let replay = store
            .claim_new_session_command(&workspace_key, &command)
            .unwrap();
        assert!(matches!(
            replay,
            NewSessionCommandClaim::Existing(StoredCommandIdentity {
                result_session_id: Some(id),
                ..
            }) if id == created_id
        ));
        let after = store.workspace_snapshot(project_id).unwrap();
        assert_eq!(after.sessions.len(), before.sessions.len() + 1);
        assert_eq!(after.active_session_id, Some(created_id));

        let mut changed = command.clone();
        changed.canonical_command.extend_from_slice(b" ");
        assert_eq!(
            store
                .claim_new_session_command(&workspace_key, &changed)
                .unwrap(),
            NewSessionCommandClaim::Conflict
        );
        assert_eq!(
            store.workspace_snapshot(project_id).unwrap().sessions.len(),
            after.sessions.len()
        );
    }

    #[test]
    fn pruning_result_material_keeps_a_non_executable_identity_tombstone() {
        let (_workspace, store, project_id) = fixture();
        let command = input(project_id, "pruned-result", b"canonical");
        store.claim_command_identity(&command).unwrap();
        store
            .mark_command_executing(
                project_id,
                &command.request_id,
                command.canonicalization_version,
                &command.canonical_command,
            )
            .unwrap();
        store
            .settle_command_identity(
                project_id,
                &command.request_id,
                command.canonicalization_version,
                &command.canonical_command,
                CommandOutcomeKind::Failure,
                Some("retained failure"),
            )
            .unwrap();

        assert_eq!(store.prune_command_result_bodies(project_id, 1).unwrap(), 1);
        let tombstone = store
            .command_identity(project_id, &command.request_id)
            .unwrap()
            .unwrap();
        assert_eq!(tombstone.state, CommandIdentityState::Settled);
        assert_eq!(tombstone.outcome_kind, Some(CommandOutcomeKind::Failure));
        assert!(!tombstone.result_available);
        assert!(tombstone.result_payload.is_none());
        assert!(matches!(
            store.claim_command_identity(&command).unwrap(),
            CommandClaim::Existing(_)
        ));
    }

    #[test]
    fn disclosure_restriction_cannot_be_undone_by_late_settlement() {
        let (_workspace, store, project_id) = fixture();
        let active = input(project_id, "restricted-active", b"restricted-active");
        store.claim_command_identity(&active).unwrap();
        let restricted = store
            .restrict_command_result(project_id, &active.request_id, true)
            .unwrap();
        assert!(!restricted.replay_allowed);
        assert!(!restricted.result_available);
        let settled = store
            .settle_command_identity(
                project_id,
                &active.request_id,
                active.canonicalization_version,
                &active.canonical_command,
                CommandOutcomeKind::Failure,
                Some("PRIVATE_RESULT_CANARY"),
            )
            .unwrap();
        assert_eq!(settled.state, CommandIdentityState::Settled);
        assert!(!settled.replay_allowed);
        assert!(!settled.result_available);
        assert!(settled.result_payload.is_none());

        let retained = input(project_id, "restricted-settled", b"restricted-settled");
        store.claim_command_identity(&retained).unwrap();
        store
            .settle_command_identity(
                project_id,
                &retained.request_id,
                retained.canonicalization_version,
                &retained.canonical_command,
                CommandOutcomeKind::Failure,
                Some("PRIVATE_RESULT_CANARY"),
            )
            .unwrap();
        let tombstone = store
            .restrict_command_result(project_id, &retained.request_id, true)
            .unwrap();
        assert!(!tombstone.replay_allowed);
        assert!(!tombstone.result_available);
        assert!(tombstone.result_payload.is_none());
    }

    #[test]
    fn retained_result_body_quota_evicts_content_but_keeps_every_identity() {
        let (_workspace, store, project_id) = fixture();
        for index in 0..=MAX_RETAINED_COMMAND_RESULT_BODIES_PER_PROJECT {
            let request_id = format!("result-{index:04}");
            let command = input(project_id, &request_id, b"same-result-kind");
            store.claim_command_identity(&command).unwrap();
            store
                .settle_command_identity(
                    project_id,
                    &request_id,
                    command.canonicalization_version,
                    &command.canonical_command,
                    CommandOutcomeKind::Failure,
                    Some("bounded result"),
                )
                .unwrap();
        }
        let oldest = store
            .command_identity(project_id, "result-0000")
            .unwrap()
            .unwrap();
        assert_eq!(oldest.state, CommandIdentityState::Settled);
        assert!(!oldest.result_available);
        assert!(oldest.result_payload.is_none());
        let newest = store
            .command_identity(
                project_id,
                &format!(
                    "result-{:04}",
                    MAX_RETAINED_COMMAND_RESULT_BODIES_PER_PROJECT
                ),
            )
            .unwrap()
            .unwrap();
        assert!(newest.result_available);
        assert_eq!(newest.result_payload.as_deref(), Some("bounded result"));
        assert_eq!(
            store
                .connection
                .lock()
                .unwrap()
                .query_row(
                    "SELECT COUNT(*) FROM command_identities
                     WHERE project_id = ?1 AND result_payload IS NOT NULL",
                    [project_id.to_string()],
                    |row| row.get::<_, usize>(0),
                )
                .unwrap(),
            MAX_RETAINED_COMMAND_RESULT_BODIES_PER_PROJECT
        );
    }

    #[test]
    fn retained_result_body_quota_is_also_bounded_across_the_store() {
        let (workspace, store, first_project) = fixture();
        let mut project_ids = vec![first_project];
        for index in 0..4 {
            let path = workspace.path().join(format!("project-{index}"));
            std::fs::create_dir(&path).unwrap();
            let resolution = store
                .resolve_workspace_with_default_provider(
                    &path,
                    &ProviderProfile::default_llama_cpp(),
                )
                .unwrap();
            project_ids.push(resolution.snapshot.project.id);
        }

        for index in 0..=MAX_RETAINED_COMMAND_RESULT_BODIES_PER_STORE {
            let project_id = project_ids[index % project_ids.len()];
            let request_id = format!("store-result-{index:04}");
            let command = input(project_id, &request_id, b"same-store-result-kind");
            store.claim_command_identity(&command).unwrap();
            store
                .settle_command_identity(
                    project_id,
                    &request_id,
                    command.canonicalization_version,
                    &command.canonical_command,
                    CommandOutcomeKind::Failure,
                    Some("bounded result"),
                )
                .unwrap();
        }

        let (total, retained, unavailable) = store
            .connection
            .lock()
            .unwrap()
            .query_row(
                "SELECT COUNT(*),
                        SUM(CASE WHEN result_payload IS NOT NULL THEN 1 ELSE 0 END),
                        SUM(CASE WHEN result_available = 0 THEN 1 ELSE 0 END)
                 FROM command_identities WHERE request_id LIKE 'store-result-%'",
                [],
                |row| {
                    Ok((
                        row.get::<_, usize>(0)?,
                        row.get::<_, usize>(1)?,
                        row.get::<_, usize>(2)?,
                    ))
                },
            )
            .unwrap();
        assert_eq!(total, MAX_RETAINED_COMMAND_RESULT_BODIES_PER_STORE + 1);
        assert_eq!(retained, MAX_RETAINED_COMMAND_RESULT_BODIES_PER_STORE);
        assert_eq!(unavailable, 1);
    }

    #[test]
    fn concurrent_equal_claims_have_one_dispatch_owner() {
        let (_workspace, store, project_id) = fixture();
        let command = input(project_id, "concurrent", b"same-command");
        let start = Arc::new(Barrier::new(3));
        let workers = (0..2)
            .map(|_| {
                let store = store.clone();
                let command = command.clone();
                let start = Arc::clone(&start);
                std::thread::spawn(move || {
                    start.wait();
                    store.claim_command_identity(&command).unwrap()
                })
            })
            .collect::<Vec<_>>();
        start.wait();
        let outcomes = workers
            .into_iter()
            .map(|worker| worker.join().unwrap())
            .collect::<Vec<_>>();
        assert_eq!(
            outcomes
                .iter()
                .filter(|outcome| matches!(outcome, CommandClaim::Claimed(_)))
                .count(),
            1
        );
        assert_eq!(
            outcomes
                .iter()
                .filter(|outcome| matches!(outcome, CommandClaim::Existing(_)))
                .count(),
            1
        );
    }

    #[test]
    fn active_and_total_project_quotas_reject_new_ids_without_reusing_old_ones() {
        let (_workspace, store, project_id) = fixture();
        for index in 0..MAX_ACTIVE_COMMAND_IDENTITIES_PER_PROJECT {
            let command = input(project_id, &format!("active-{index}"), b"active");
            assert!(matches!(
                store.claim_command_identity(&command).unwrap(),
                CommandClaim::Claimed(_)
            ));
        }
        let overflow = input(project_id, "active-overflow", b"active");
        assert!(matches!(
            store.claim_command_identity(&overflow),
            Err(StoreError::CommandAdmissionLimit)
        ));

        for index in 0..MAX_ACTIVE_COMMAND_IDENTITIES_PER_PROJECT {
            let request_id = format!("active-{index}");
            let command = input(project_id, &request_id, b"active");
            store
                .settle_command_identity(
                    project_id,
                    &request_id,
                    command.canonicalization_version,
                    &command.canonical_command,
                    CommandOutcomeKind::Success,
                    None,
                )
                .unwrap();
        }
        for index in MAX_ACTIVE_COMMAND_IDENTITIES_PER_PROJECT..MAX_COMMAND_IDENTITIES_PER_PROJECT {
            let request_id = format!("retained-{index}");
            let command = input(project_id, &request_id, b"retained");
            store.claim_command_identity(&command).unwrap();
            store
                .settle_command_identity(
                    project_id,
                    &request_id,
                    command.canonicalization_version,
                    &command.canonical_command,
                    CommandOutcomeKind::Success,
                    None,
                )
                .unwrap();
        }
        let total_overflow = input(project_id, "total-overflow", b"retained");
        assert!(matches!(
            store.claim_command_identity(&total_overflow),
            Err(StoreError::CommandIdentityQuotaExceeded)
        ));
        assert!(matches!(
            store
                .claim_command_identity(&input(project_id, "active-0", b"active"))
                .unwrap(),
            CommandClaim::Existing(_)
        ));
    }

    #[test]
    fn aggregate_store_admission_quotas_are_enforced_independently_of_project_limits() {
        assert!(enforce_admission_quota_counts(
            1,
            1,
            (MAX_COMMAND_IDENTITIES_PER_STORE - 1) as u64,
            (MAX_ACTIVE_COMMAND_IDENTITIES_PER_STORE - 1) as u64,
        )
        .is_ok());
        assert!(matches!(
            enforce_admission_quota_counts(1, 1, MAX_COMMAND_IDENTITIES_PER_STORE as u64, 1,),
            Err(StoreError::CommandIdentityQuotaExceeded)
        ));
        assert!(matches!(
            enforce_admission_quota_counts(1, 1, 1, MAX_ACTIVE_COMMAND_IDENTITIES_PER_STORE as u64,),
            Err(StoreError::CommandAdmissionLimit)
        ));
    }

    #[test]
    fn aggregate_active_store_quota_rejects_a_transactional_claim() {
        let (workspace, store, first_project) = fixture();
        let mut project_ids = vec![first_project];
        for index in 0..4 {
            let path = workspace.path().join(format!("active-project-{index}"));
            std::fs::create_dir(&path).unwrap();
            let resolution = store
                .resolve_workspace_with_default_provider(
                    &path,
                    &ProviderProfile::default_llama_cpp(),
                )
                .unwrap();
            project_ids.push(resolution.snapshot.project.id);
        }

        for index in 0..MAX_ACTIVE_COMMAND_IDENTITIES_PER_STORE {
            let project_id = project_ids[index % project_ids.len()];
            let command = input(project_id, &format!("store-active-{index}"), b"active");
            assert!(matches!(
                store.claim_command_identity(&command).unwrap(),
                CommandClaim::Claimed(_)
            ));
        }
        let overflow = input(first_project, "store-active-overflow", b"active");
        assert!(matches!(
            store.claim_command_identity(&overflow),
            Err(StoreError::CommandAdmissionLimit)
        ));
        assert!(store
            .command_identity(first_project, &overflow.request_id)
            .unwrap()
            .is_none());
    }

    #[test]
    fn invalid_scope_does_not_leave_a_claim_and_old_versions_are_not_misses() {
        let (_workspace, store, project_id) = fixture();
        let mut invalid = input(project_id, "invalid-scope", b"scope");
        invalid.scope.session_id = Some(Uuid::new_v4());
        assert!(store.claim_command_identity(&invalid).is_err());
        assert!(store
            .command_identity(project_id, &invalid.request_id)
            .unwrap()
            .is_none());

        let command = input(project_id, "old-version", b"old");
        store.claim_command_identity(&command).unwrap();
        store
            .connection
            .lock()
            .unwrap()
            .execute(
                "UPDATE command_identities SET canonicalization_version = 2
                 WHERE project_id = ?1 AND request_id = ?2",
                params![project_id.to_string(), command.request_id],
            )
            .unwrap();
        assert_eq!(
            store
                .match_command_identity(
                    project_id,
                    "old-version",
                    COMMAND_IDENTITY_VERSION,
                    b"old",
                )
                .unwrap(),
            CommandIdentityMatch::UnsupportedVersion { stored_version: 2 }
        );

        let malformed = input(project_id, "malformed-retained", b"malformed");
        store.claim_command_identity(&malformed).unwrap();
        store
            .connection
            .lock()
            .unwrap()
            .execute(
                "UPDATE command_identities SET fingerprint = ?3
                 WHERE project_id = ?1 AND request_id = ?2",
                params![project_id.to_string(), malformed.request_id, "z".repeat(64)],
            )
            .unwrap();
        assert!(matches!(
            store.match_command_identity(
                project_id,
                "malformed-retained",
                COMMAND_IDENTITY_VERSION,
                b"malformed"
            ),
            Err(StoreError::InvalidRecord(_))
        ));
        assert_eq!(
            store
                .connection
                .lock()
                .unwrap()
                .query_row(
                    "SELECT COUNT(*) FROM command_identities
                     WHERE project_id = ?1 AND request_id = 'malformed-retained'",
                    [project_id.to_string()],
                    |row| row.get::<_, u64>(0),
                )
                .unwrap(),
            1
        );
    }
}
