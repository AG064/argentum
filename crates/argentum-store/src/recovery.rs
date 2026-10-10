use std::collections::{BTreeMap, BTreeSet};
use std::fs::File;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

use argentum_domain::goal_accounting::{ChargeState, RequestCharge, RunAccounting};
use argentum_domain::goal_supervision::{
    GoalInvocation, GoalInvocationState, InvocationAttemptOutcome,
};
use argentum_domain::recovery::{
    HostGenerationState, HostOwnerEvidence, LegacyOwnershipAmbiguity, LegacyWorkAmbiguity,
    OwnershipOutcome, RecoveryAccountingState, RecoveryEffectState, RecoveryExecutionState,
    RecoveryInspection, RecoveryReference, RecoveryRequestState, RecoveryRunKind,
    RecoverySettlement, RunRecoveryRecord, MAX_RECOVERY_APPROVAL_EVENTS, MAX_RECOVERY_REFERENCES,
    MAX_RECOVERY_REQUESTS, MAX_RECOVERY_RUNS, RECOVERY_VERSION,
};
use argentum_domain::{now, AppEvent, GoalLifecycle, GrantScope, GrantSubject, TaskLifecycle};
use rusqlite::{params, Connection, OpenFlags, OptionalExtension, TransactionBehavior};
use sha2::{Digest, Sha256};
use time::OffsetDateTime;
use uuid::Uuid;

use super::goal_accounting::{goal_by_id, write_goal};
use super::{append_event_on, Store, StoreError, DATABASE_BUSY_TIMEOUT};

const OWNERSHIP_MECHANISM: &str = "windows_named_mutex_lifetime_guard_with_pinned_database_file";
const IDENTITY_METHOD: &str = "windows_volume_serial_and_file_index";

#[derive(Debug, Clone)]
pub(super) struct PhysicalStoreIdentity {
    pub(super) canonical_path: std::path::PathBuf,
    value: String,
    pub(super) digest: argentum_domain::Sha256Digest,
}

#[derive(Debug, Clone)]
struct HostGenerationRecord {
    store_id: Uuid,
    generation: u64,
    owner_id: Uuid,
    process_id: u32,
    process_start_key: u64,
    physical_identity: String,
    physical_identity_sha256: argentum_domain::Sha256Digest,
    state: HostGenerationState,
}

pub(crate) struct ManagedHostFence {
    pub(crate) generation: Option<u64>,
    pub(crate) active: AtomicBool,
    #[cfg(windows)]
    guard: Option<WindowsMutexGuard>,
    #[cfg(windows)]
    _database_file: Option<File>,
}

impl std::fmt::Debug for ManagedHostFence {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ManagedHostFence")
            .field("generation", &self.generation)
            .field("active", &self.active.load(Ordering::Acquire))
            .finish_non_exhaustive()
    }
}

pub struct StoreHostOwner {
    store: Store,
    fence: Arc<ManagedHostFence>,
    retired: bool,
}

impl std::fmt::Debug for StoreHostOwner {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("StoreHostOwner")
            .field("generation", &self.fence.generation)
            .field("retired", &self.retired)
            .finish_non_exhaustive()
    }
}

impl StoreHostOwner {
    pub fn generation(&self) -> Option<u64> {
        self.fence.generation
    }

    pub fn retire(&mut self) -> Result<(), StoreError> {
        if self.retired {
            return Ok(());
        }
        let Some(generation) = self.fence.generation else {
            self.fence.active.store(false, Ordering::Release);
            self.retired = true;
            return Ok(());
        };
        let mut connection = self
            .store
            .connection
            .lock()
            .map_err(|_| StoreError::LockPoisoned)?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        validate_host_generation_on(&transaction, generation)?;
        let active_runs: bool = transaction.query_row(
            "SELECT EXISTS(
                 SELECT 1 FROM runtime_run_ownership
                 WHERE owner_generation = ?1 AND state = 'active'
             )",
            [generation],
            |row| row.get(0),
        )?;
        let active_invocations: bool = transaction.query_row(
            "SELECT EXISTS(
                 SELECT 1 FROM goal_invocations
                 WHERE owner_generation = ?1
                   AND (json_extract(payload, '$.owner_claim_retired') IS NULL
                        OR json_extract(payload, '$.owner_claim_retired') = 0)
             )",
            [generation],
            |row| row.get(0),
        )?;
        let active_commands =
            super::command_identity::active_command_count_on(&transaction, generation)? > 0;
        if active_runs || active_invocations || active_commands {
            return Err(StoreError::RecoveryBlocked("active_work_remains"));
        }
        if transaction.execute(
            "UPDATE runtime_host_generations
             SET state = 'retired', finished_at = ?2, reason = 'clean_shutdown'
             WHERE generation = ?1 AND state = 'active'",
            params![generation, now().unix_timestamp()],
        )? != 1
        {
            return Err(StoreError::HostGenerationFenced);
        }
        transaction.commit()?;
        self.fence.active.store(false, Ordering::Release);
        #[cfg(windows)]
        if let Some(guard) = self.fence.guard.as_ref() {
            guard.release();
        }
        self.retired = true;
        Ok(())
    }
}

impl Drop for StoreHostOwner {
    fn drop(&mut self) {
        let _ = self.retire();
    }
}

impl Store {
    pub fn ensure_runtime_host_guard(&self) -> Result<(), StoreError> {
        if self.path.is_none() {
            return Ok(());
        }
        match self.current_host_generation()? {
            Some(_) => Ok(()),
            None => Err(StoreError::HostOwnershipUnknown(
                "persistent_runtime_requires_writable_host_guard",
            )),
        }
    }

    pub fn open_for_host(path: impl AsRef<Path>) -> Result<(Self, StoreHostOwner), StoreError> {
        let path = path.as_ref().to_path_buf();
        #[cfg(not(windows))]
        {
            let _ = path;
            return Err(StoreError::RecoveryUnsupported);
        }
        #[cfg(windows)]
        {
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent).map_err(io_as_database_error)?;
            }
            let (database_file, physical) = pinned_physical_identity(&path, true, true)?;
            let guard = acquire_named_mutex(&physical.digest)?;
            let connection = Connection::open(&physical.canonical_path)?;
            connection.busy_timeout(DATABASE_BUSY_TIMEOUT)?;
            let store = Self {
                connection: Arc::new(Mutex::new(connection)),
                context_neutral_changes: Arc::new(std::sync::atomic::AtomicU64::new(0)),
                path: Some(path),
                host_fence: Arc::new(Mutex::new(None)),
            };
            store.migrate()?;
            let generation = store.begin_host_generation(&physical)?;
            let fence = Arc::new(ManagedHostFence {
                generation: Some(generation),
                active: AtomicBool::new(true),
                guard: Some(guard),
                _database_file: Some(database_file),
            });
            *store
                .host_fence
                .lock()
                .map_err(|_| StoreError::LockPoisoned)? = Some(Arc::clone(&fence));
            let owner = StoreHostOwner {
                store: store.clone(),
                fence,
                retired: false,
            };
            Ok((store, owner))
        }
    }

    pub fn open_in_memory_for_host() -> Result<(Self, StoreHostOwner), StoreError> {
        let store = Self::open_in_memory()?;
        let fence = Arc::new(ManagedHostFence {
            generation: None,
            active: AtomicBool::new(true),
            #[cfg(windows)]
            guard: None,
            #[cfg(windows)]
            _database_file: None,
        });
        *store
            .host_fence
            .lock()
            .map_err(|_| StoreError::LockPoisoned)? = Some(Arc::clone(&fence));
        let owner = StoreHostOwner {
            store: store.clone(),
            fence,
            retired: false,
        };
        Ok((store, owner))
    }

    pub(crate) fn current_host_generation(&self) -> Result<Option<u64>, StoreError> {
        let fence = self
            .host_fence
            .lock()
            .map_err(|_| StoreError::LockPoisoned)?
            .clone();
        let Some(fence) = fence else {
            return Ok(None);
        };
        if !fence.active.load(Ordering::Acquire) {
            return Err(StoreError::HostGenerationFenced);
        }
        let Some(generation) = fence.generation else {
            return Ok(None);
        };
        let connection = self
            .connection
            .lock()
            .map_err(|_| StoreError::LockPoisoned)?;
        validate_host_generation_on(&connection, generation)?;
        Ok(Some(generation))
    }

    pub fn claim_run_ownership(
        &self,
        run_id: Uuid,
        project_id: Uuid,
        session_id: Uuid,
        task_id: Option<Uuid>,
        invocation_id: Option<Uuid>,
        kind: RecoveryRunKind,
    ) -> Result<(), StoreError> {
        let Some(generation) = self.current_host_generation()? else {
            return Ok(());
        };
        if run_id.is_nil()
            || project_id.is_nil()
            || session_id.is_nil()
            || task_id.is_some_and(|id| id.is_nil())
            || invocation_id.is_some_and(|id| id.is_nil())
        {
            return Err(StoreError::InvalidRecord(
                "invalid run ownership binding".into(),
            ));
        }
        let mut connection = self
            .connection
            .lock()
            .map_err(|_| StoreError::LockPoisoned)?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        validate_host_generation_on(&transaction, generation)?;
        ensure_session_recovery_admissible_on(&transaction, project_id, session_id)?;
        let at = now();
        let claim = RunRecoveryRecord {
            version: RECOVERY_VERSION,
            run_id,
            project_id,
            session_id,
            task_id,
            invocation_id,
            owner_generation: generation,
            kind,
            original_lifecycle: "queued".into(),
            execution_state: RecoveryExecutionState::Active,
            request_id: None,
            request_purpose: String::new(),
            request_state: RecoveryRequestState::None,
            effect_call_id: None,
            effect_kind: String::new(),
            effect_containment: String::new(),
            effect_state: RecoveryEffectState::None,
            known_effects: Vec::new(),
            unresolved_effects: Vec::new(),
            accounting_state: RecoveryAccountingState::NotAssociated,
            authority_invalidated: false,
            admission_blocked: false,
            created_at: at,
            updated_at: at,
            recovered_at: None,
            recovery_revision: None,
            interruption_reason: None,
            owner_reviewed_at: None,
        };
        transaction.execute(
            "INSERT INTO runtime_run_ownership (
                 run_id, project_id, session_id, owner_generation, state, updated_at, payload
             ) VALUES (?1, ?2, ?3, ?4, 'active', ?5, ?6)",
            params![
                run_id.to_string(),
                project_id.to_string(),
                session_id.to_string(),
                generation,
                at.unix_timestamp(),
                serde_json::to_string(&claim)?,
            ],
        )?;
        transaction.commit()?;
        Ok(())
    }

    pub fn abandon_unadmitted_run_claim(&self, run_id: Uuid) -> Result<(), StoreError> {
        self.finish_run_ownership(run_id, RecoveryExecutionState::Failed, "not_admitted")
    }

    pub fn finish_run_ownership(
        &self,
        run_id: Uuid,
        state: RecoveryExecutionState,
        lifecycle: &str,
    ) -> Result<(), StoreError> {
        if state == RecoveryExecutionState::Active || !bounded_label(lifecycle, 64) {
            return Err(StoreError::InvalidRecord(
                "invalid run ownership settlement".into(),
            ));
        }
        let Some(generation) = self.current_host_generation()? else {
            return Ok(());
        };
        let mut connection = self
            .connection
            .lock()
            .map_err(|_| StoreError::LockPoisoned)?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        validate_host_generation_on(&transaction, generation)?;
        let mut claim =
            read_run_claim_on(&transaction, run_id)?.ok_or(StoreError::HostGenerationFenced)?;
        if claim.owner_generation != generation {
            return Err(StoreError::HostGenerationFenced);
        }
        if claim.execution_state != RecoveryExecutionState::Active && claim.execution_state != state
        {
            return Err(StoreError::HostGenerationFenced);
        }
        claim.execution_state = state;
        claim.original_lifecycle = lifecycle.into();
        claim.accounting_state = observed_accounting_state_on(&transaction, run_id)?;
        claim.updated_at = now();
        write_run_claim_on(&transaction, &claim)?;
        transaction.commit()?;
        Ok(())
    }

    pub fn settle_active_run_ownership(
        &self,
        run_id: Uuid,
        fallback_state: RecoveryExecutionState,
        fallback_lifecycle: &str,
    ) -> Result<(), StoreError> {
        if fallback_state == RecoveryExecutionState::Active
            || !bounded_label(fallback_lifecycle, 64)
        {
            return Err(StoreError::InvalidRecord(
                "invalid run ownership settlement".into(),
            ));
        }
        let Some(generation) = self.current_host_generation()? else {
            return Ok(());
        };
        let mut connection = self
            .connection
            .lock()
            .map_err(|_| StoreError::LockPoisoned)?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        validate_host_generation_on(&transaction, generation)?;
        let mut claim =
            read_run_claim_on(&transaction, run_id)?.ok_or(StoreError::HostGenerationFenced)?;
        if claim.owner_generation != generation {
            return Err(StoreError::HostGenerationFenced);
        }
        if claim.execution_state == RecoveryExecutionState::Active {
            claim.execution_state = fallback_state;
            claim.original_lifecycle = fallback_lifecycle.into();
        }
        claim.accounting_state = observed_accounting_state_on(&transaction, run_id)?;
        claim.updated_at = now();
        write_run_claim_on(&transaction, &claim)?;
        transaction.commit()?;
        Ok(())
    }

    pub fn ensure_run_generation(&self, run_id: Uuid) -> Result<(), StoreError> {
        let Some(generation) = self.current_host_generation()? else {
            return Ok(());
        };
        let connection = self
            .connection
            .lock()
            .map_err(|_| StoreError::LockPoisoned)?;
        validate_run_generation_on(&connection, run_id, generation)
    }

    pub(crate) fn ensure_session_recovery_admissible(
        &self,
        project_id: Uuid,
        session_id: Uuid,
    ) -> Result<(), StoreError> {
        let connection = self
            .connection
            .lock()
            .map_err(|_| StoreError::LockPoisoned)?;
        ensure_session_recovery_admissible_on(&connection, project_id, session_id)
    }

    pub fn mark_run_request(
        &self,
        run_id: Uuid,
        request_id: Uuid,
        purpose: &str,
        state: RecoveryRequestState,
    ) -> Result<(), StoreError> {
        if request_id.is_nil()
            || purpose.is_empty()
            || purpose.len() > 64
            || !purpose
                .bytes()
                .all(|byte| byte.is_ascii_lowercase() || byte == b'_')
        {
            return Err(StoreError::InvalidRecord(
                "invalid recovery request boundary".into(),
            ));
        }
        self.update_active_run_claim(run_id, |claim| {
            if claim.request_id == Some(request_id) && claim.request_purpose != purpose {
                return Err(StoreError::InvalidRecord(
                    "recovery request purpose changed".into(),
                ));
            }
            if claim.request_id == Some(request_id)
                && request_state_rank(state) < request_state_rank(claim.request_state)
            {
                return Err(StoreError::InvalidRecord(
                    "recovery request boundary regressed".into(),
                ));
            }
            if claim.request_id != Some(request_id)
                && (state != RecoveryRequestState::PreparedNotDispatched
                    || (claim.request_id.is_some()
                        && claim.request_state != RecoveryRequestState::Settled))
            {
                return Err(StoreError::InvalidRecord(
                    "recovery request identity changed".into(),
                ));
            }
            claim.request_id = Some(request_id);
            claim.request_purpose = purpose.into();
            claim.request_state = state;
            Ok(())
        })
    }

    pub fn mark_run_effect_admitted(
        &self,
        run_id: Uuid,
        call_id: Uuid,
        kind: &str,
        containment: &str,
    ) -> Result<(), StoreError> {
        if call_id.is_nil()
            || !bounded_label(kind, 64)
            || (!containment.is_empty() && !bounded_label(containment, 96))
        {
            return Err(StoreError::InvalidRecord(
                "invalid recovery effect boundary".into(),
            ));
        }
        self.update_active_run_claim(run_id, |claim| {
            if claim.effect_state == RecoveryEffectState::Admitted {
                return Err(StoreError::InvalidRecord(
                    "another effect is already admitted".into(),
                ));
            }
            claim.effect_call_id = Some(call_id);
            claim.effect_kind = kind.into();
            claim.effect_containment = containment.into();
            claim.effect_state = RecoveryEffectState::Admitted;
            Ok(())
        })
    }

    pub fn mark_run_effect_settled(&self, run_id: Uuid, call_id: Uuid) -> Result<(), StoreError> {
        self.update_active_run_claim(run_id, |claim| {
            if claim.effect_call_id != Some(call_id)
                || claim.effect_state != RecoveryEffectState::Admitted
            {
                return Err(StoreError::InvalidRecord(
                    "recovery effect settlement does not match admission".into(),
                ));
            }
            claim.effect_state = RecoveryEffectState::Settled;
            Ok(())
        })
    }

    fn update_active_run_claim(
        &self,
        run_id: Uuid,
        update: impl FnOnce(&mut RunRecoveryRecord) -> Result<(), StoreError>,
    ) -> Result<(), StoreError> {
        let Some(generation) = self.current_host_generation()? else {
            return Ok(());
        };
        let mut connection = self
            .connection
            .lock()
            .map_err(|_| StoreError::LockPoisoned)?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        validate_host_generation_on(&transaction, generation)?;
        let mut claim =
            read_run_claim_on(&transaction, run_id)?.ok_or(StoreError::HostGenerationFenced)?;
        if claim.owner_generation != generation
            || claim.execution_state != RecoveryExecutionState::Active
        {
            return Err(StoreError::HostGenerationFenced);
        }
        update(&mut claim)?;
        claim.updated_at = now();
        write_run_claim_on(&transaction, &claim)?;
        transaction.commit()?;
        // Request and effect boundary metadata does not alter the context source
        // snapshot. Count the one committed ownership-row update so compaction's
        // optimistic token still detects every other local database mutation.
        self.context_neutral_changes
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        Ok(())
    }

    fn begin_host_generation(&self, physical: &PhysicalStoreIdentity) -> Result<u64, StoreError> {
        let mut connection = self
            .connection
            .lock()
            .map_err(|_| StoreError::LockPoisoned)?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let store_id = read_store_id_on(&transaction)?;
        if let Some(active) = read_active_host_on(&transaction, store_id)? {
            if active.physical_identity != physical.value {
                return Err(StoreError::HostOwnershipUnknown(
                    "active_owner_belongs_to_a_different_physical_store_copy",
                ));
            }
            return Err(StoreError::HostRecoveryRequired);
        }
        let (legacy, truncated) = legacy_ambiguities_on(&transaction, None, None)?;
        let (legacy_work, legacy_work_truncated) =
            legacy_work_ambiguities_on(&transaction, None, None, false)?;
        if !legacy.is_empty() || truncated || !legacy_work.is_empty() || legacy_work_truncated {
            return Err(StoreError::HostOwnershipUnknown(
                "legacy_nonterminal_work_has_no_lifetime_owner_evidence",
            ));
        }
        let last: Option<u64> = transaction.query_row(
            "SELECT MAX(generation) FROM runtime_host_generations",
            [],
            |row| row.get(0),
        )?;
        let generation = last
            .unwrap_or(0)
            .checked_add(1)
            .ok_or_else(|| StoreError::InvalidRecord("host generation exhausted".into()))?;
        let owner_id = Uuid::new_v4();
        let process_id = std::process::id();
        let process_start_key = current_process_start_key()?;
        transaction.execute(
            "INSERT INTO runtime_host_generations (
                 generation, owner_id, process_id, process_start_key,
                 physical_identity, physical_identity_sha256, state, started_at
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, 'active', ?7)",
            params![
                generation,
                owner_id.to_string(),
                process_id,
                process_start_key.to_string(),
                physical.value,
                physical.digest.0,
                now().unix_timestamp(),
            ],
        )?;
        transaction.commit()?;
        Ok(generation)
    }
}

fn bounded_label(value: &str, max: usize) -> bool {
    !value.is_empty()
        && value.len() <= max
        && value
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte == b'_' || byte == b'-')
}

fn request_state_rank(state: RecoveryRequestState) -> u8 {
    match state {
        RecoveryRequestState::None => 0,
        RecoveryRequestState::PreparedNotDispatched => 1,
        RecoveryRequestState::Dispatched => 2,
        RecoveryRequestState::PartialResponse => 3,
        RecoveryRequestState::ResponseReceived => 4,
        RecoveryRequestState::Settled => 5,
    }
}

pub(super) fn read_store_id_on(connection: &Connection) -> Result<Uuid, StoreError> {
    let value: String = connection.query_row(
        "SELECT store_id FROM runtime_store_identity WHERE singleton = 1",
        [],
        |row| row.get(0),
    )?;
    Uuid::parse_str(&value)
        .map_err(|_| StoreError::InvalidRecord("runtime store identity is invalid".into()))
}

fn read_active_host_on(
    connection: &Connection,
    store_id: Uuid,
) -> Result<Option<HostGenerationRecord>, StoreError> {
    let mut statement = connection.prepare(
        "SELECT generation, owner_id, process_id, process_start_key,
                physical_identity, physical_identity_sha256, state
         FROM runtime_host_generations
         WHERE state IN ('active','recovery_pending')
         ORDER BY generation DESC LIMIT 2",
    )?;
    let rows = statement
        .query_map([], |row| {
            Ok((
                row.get::<_, u64>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, u32>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, String>(4)?,
                row.get::<_, String>(5)?,
                row.get::<_, String>(6)?,
            ))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    if rows.len() > 1 {
        return Err(StoreError::InvalidRecord(
            "multiple unretired host generations exist".into(),
        ));
    }
    rows.into_iter()
        .next()
        .map(
            |(generation, owner_id, process_id, process_start_key, physical, digest, state)| {
                Ok(HostGenerationRecord {
                    store_id,
                    generation,
                    owner_id: Uuid::parse_str(&owner_id).map_err(|_| {
                        StoreError::InvalidRecord("host owner identity is invalid".into())
                    })?,
                    process_id,
                    process_start_key: process_start_key.parse().map_err(|_| {
                        StoreError::InvalidRecord("host process start key is invalid".into())
                    })?,
                    physical_identity: physical,
                    physical_identity_sha256: argentum_domain::Sha256Digest(digest),
                    state: parse_host_state(&state)?,
                })
            },
        )
        .transpose()
}

fn parse_host_state(value: &str) -> Result<HostGenerationState, StoreError> {
    match value {
        "active" => Ok(HostGenerationState::Active),
        "recovery_pending" => Ok(HostGenerationState::RecoveryPending),
        "recovered" => Ok(HostGenerationState::Recovered),
        "retired" => Ok(HostGenerationState::Retired),
        _ => Err(StoreError::InvalidRecord(
            "host generation state is invalid".into(),
        )),
    }
}

pub(super) fn validate_host_generation_on(
    connection: &Connection,
    generation: u64,
) -> Result<(), StoreError> {
    let valid: bool = connection.query_row(
        "SELECT EXISTS(
             SELECT 1 FROM runtime_host_generations
             WHERE generation = ?1 AND state = 'active'
         )",
        [generation],
        |row| row.get(0),
    )?;
    if valid {
        Ok(())
    } else {
        Err(StoreError::HostGenerationFenced)
    }
}

pub(super) fn validate_host_generation_for_event_on(
    connection: &Connection,
    generation: u64,
) -> Result<(), StoreError> {
    validate_host_generation_on(connection, generation)
}

fn validate_run_generation_on(
    connection: &Connection,
    run_id: Uuid,
    generation: u64,
) -> Result<(), StoreError> {
    validate_host_generation_on(connection, generation)?;
    let valid: bool = connection.query_row(
        "SELECT EXISTS(
             SELECT 1 FROM runtime_run_ownership
             WHERE run_id = ?1 AND owner_generation = ?2 AND state = 'active'
         )",
        params![run_id.to_string(), generation],
        |row| row.get(0),
    )?;
    if valid {
        Ok(())
    } else {
        Err(StoreError::HostGenerationFenced)
    }
}

pub(super) fn read_run_claim_on(
    connection: &Connection,
    run_id: Uuid,
) -> Result<Option<RunRecoveryRecord>, StoreError> {
    let payload: Option<String> = connection
        .query_row(
            "SELECT payload FROM runtime_run_ownership WHERE run_id = ?1",
            [run_id.to_string()],
            |row| row.get(0),
        )
        .optional()?;
    payload
        .map(|payload| {
            let claim: RunRecoveryRecord = serde_json::from_str(&payload)?;
            validate_run_claim(&claim, run_id)?;
            Ok(claim)
        })
        .transpose()
}

fn write_run_claim_on(
    connection: &Connection,
    claim: &RunRecoveryRecord,
) -> Result<(), StoreError> {
    validate_run_claim(claim, claim.run_id)?;
    let state = match claim.execution_state {
        RecoveryExecutionState::Active => "active",
        RecoveryExecutionState::Complete => "complete",
        RecoveryExecutionState::Failed => "failed",
        RecoveryExecutionState::Cancelled => "cancelled",
        RecoveryExecutionState::Interrupted => "interrupted",
    };
    if connection.execute(
        "UPDATE runtime_run_ownership
         SET state = ?2, updated_at = ?3, payload = ?4
         WHERE run_id = ?1 AND owner_generation = ?5",
        params![
            claim.run_id.to_string(),
            state,
            claim.updated_at.unix_timestamp(),
            serde_json::to_string(claim)?,
            claim.owner_generation,
        ],
    )? != 1
    {
        return Err(StoreError::HostGenerationFenced);
    }
    Ok(())
}

fn validate_run_claim(claim: &RunRecoveryRecord, run_id: Uuid) -> Result<(), StoreError> {
    if claim.version != RECOVERY_VERSION
        || claim.run_id != run_id
        || claim.run_id.is_nil()
        || claim.project_id.is_nil()
        || claim.session_id.is_nil()
        || claim.owner_generation == 0
        || claim.known_effects.len() > MAX_RECOVERY_REFERENCES
        || claim.unresolved_effects.len() > MAX_RECOVERY_REFERENCES
        || claim.original_lifecycle.len() > 64
        || claim.request_purpose.len() > 64
        || claim.effect_kind.len() > 64
        || claim.effect_containment.len() > 96
    {
        return Err(StoreError::InvalidRecord(
            "run ownership record is invalid".into(),
        ));
    }
    Ok(())
}

fn observed_accounting_state_on(
    connection: &Connection,
    run_id: Uuid,
) -> Result<RecoveryAccountingState, StoreError> {
    let run_payload: Option<String> = connection
        .query_row(
            "SELECT payload FROM goal_runs WHERE run_id = ?1",
            [run_id.to_string()],
            |row| row.get(0),
        )
        .optional()?;
    let Some(run_payload) = run_payload else {
        return Ok(RecoveryAccountingState::NotAssociated);
    };
    let run: RunAccounting = serde_json::from_str(&run_payload)?;
    if run.run_id != run_id {
        return Err(StoreError::InvalidRecord(
            "goal run recovery binding is invalid".into(),
        ));
    }
    let mut statement = connection.prepare(
        "SELECT request_id, payload FROM goal_requests
         WHERE run_id = ?1 ORDER BY rowid ASC LIMIT 129",
    )?;
    let rows = statement
        .query_map([run_id.to_string()], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    if rows.len() > MAX_RECOVERY_REQUESTS {
        return Err(StoreError::RecoveryBlocked(
            "accounting_request_limit_exceeded",
        ));
    }
    let mut saw_settled = false;
    let mut saw_not_dispatched = false;
    for (id, payload) in rows {
        let request_id = parse_uuid(id, "goal request identity")?;
        let charge: RequestCharge = serde_json::from_str(&payload)?;
        if charge.request_id != request_id || charge.run_id != run_id {
            return Err(StoreError::InvalidRecord(
                "goal request recovery binding is invalid".into(),
            ));
        }
        match charge.state {
            ChargeState::Reserved | ChargeState::Dispatched | ChargeState::Unresolved => {
                return Ok(RecoveryAccountingState::Unresolved);
            }
            ChargeState::Settled => saw_settled = true,
            ChargeState::NotDispatched => saw_not_dispatched = true,
        }
    }
    Ok(if saw_settled || !saw_not_dispatched {
        RecoveryAccountingState::Settled
    } else {
        RecoveryAccountingState::ProvenUndispatched
    })
}

fn execution_state_for_lifecycle(lifecycle: TaskLifecycle) -> RecoveryExecutionState {
    match lifecycle {
        TaskLifecycle::Complete => RecoveryExecutionState::Complete,
        TaskLifecycle::Cancelled => RecoveryExecutionState::Cancelled,
        TaskLifecycle::Failed => RecoveryExecutionState::Failed,
        TaskLifecycle::Interrupted => RecoveryExecutionState::Interrupted,
        TaskLifecycle::Draft
        | TaskLifecycle::Queued
        | TaskLifecycle::Planning
        | TaskLifecycle::Running
        | TaskLifecycle::WaitingForApproval
        | TaskLifecycle::Reviewing
        | TaskLifecycle::Verifying
        | TaskLifecycle::Paused => RecoveryExecutionState::Active,
    }
}

pub(super) fn update_run_claim_for_event_on(
    connection: &Connection,
    current_generation: Option<u64>,
    run_id: Option<Uuid>,
    event: &AppEvent,
) -> Result<(), StoreError> {
    let Some(run_id) = run_id else {
        return Ok(());
    };
    if let AppEvent::RunStatusChanged {
        run_id: event_run_id,
        ..
    } = event
    {
        if *event_run_id != run_id {
            return Err(StoreError::InvalidRecord(
                "run status event scope is invalid".into(),
            ));
        }
    }
    let Some(mut claim) = read_run_claim_on(connection, run_id)? else {
        return Ok(());
    };
    let Some(current_generation) = current_generation else {
        return Ok(());
    };
    validate_host_generation_on(connection, current_generation)?;
    if let AppEvent::RunStatusChanged { lifecycle, .. } = event {
        if claim.owner_generation != current_generation {
            if *lifecycle == TaskLifecycle::Reviewing {
                return Ok(());
            }
            return Err(StoreError::HostGenerationFenced);
        }
        let next_state = execution_state_for_lifecycle(*lifecycle);
        if claim.execution_state != RecoveryExecutionState::Active {
            if next_state == RecoveryExecutionState::Active {
                if *lifecycle == TaskLifecycle::Reviewing {
                    return Ok(());
                }
                return Err(StoreError::HostGenerationFenced);
            }
            if claim.execution_state != next_state {
                return Err(StoreError::HostGenerationFenced);
            }
        } else {
            claim.original_lifecycle = lifecycle_name(*lifecycle).into();
            claim.execution_state = next_state;
            if lifecycle.is_terminal() {
                claim.accounting_state = observed_accounting_state_on(connection, run_id)?;
            }
            claim.updated_at = now();
            write_run_claim_on(connection, &claim)?;
        }
    }
    Ok(())
}

fn lifecycle_name(lifecycle: TaskLifecycle) -> &'static str {
    match lifecycle {
        TaskLifecycle::Draft => "draft",
        TaskLifecycle::Queued => "queued",
        TaskLifecycle::Planning => "planning",
        TaskLifecycle::Running => "running",
        TaskLifecycle::WaitingForApproval => "waiting_for_approval",
        TaskLifecycle::Reviewing => "reviewing",
        TaskLifecycle::Verifying => "verifying",
        TaskLifecycle::Complete => "complete",
        TaskLifecycle::Paused => "paused",
        TaskLifecycle::Interrupted => "interrupted",
        TaskLifecycle::Cancelled => "cancelled",
        TaskLifecycle::Failed => "failed",
    }
}

pub(super) fn ensure_session_recovery_admissible_on(
    connection: &Connection,
    project_id: Uuid,
    session_id: Uuid,
) -> Result<(), StoreError> {
    let blocked: bool = connection.query_row(
        "SELECT EXISTS(
             SELECT 1 FROM runtime_run_ownership
             WHERE project_id = ?1 AND session_id = ?2
               AND json_extract(payload, '$.admission_blocked') = 1
         )",
        params![project_id.to_string(), session_id.to_string()],
        |row| row.get(0),
    )?;
    if blocked {
        return Err(StoreError::RecoveryBlocked("owner_review_required"));
    }
    let (legacy, truncated) =
        legacy_ambiguities_on(connection, Some(project_id), Some(session_id))?;
    let (legacy_work, legacy_work_truncated) =
        legacy_work_ambiguities_on(connection, Some(project_id), Some(session_id), false)?;
    if !legacy.is_empty() || truncated || !legacy_work.is_empty() || legacy_work_truncated {
        return Err(StoreError::HostOwnershipUnknown(
            "legacy_session_work_has_no_owner_evidence",
        ));
    }
    Ok(())
}

fn legacy_ambiguities_on(
    connection: &Connection,
    project_id: Option<Uuid>,
    session_id: Option<Uuid>,
) -> Result<(Vec<LegacyOwnershipAmbiguity>, bool), StoreError> {
    let mut sql = String::from(
        "SELECT latest.run_id, latest.project_id, latest.session_id, latest.payload
         FROM event_log latest
         WHERE latest.event_type = 'run_status_changed'
           AND latest.run_id IS NOT NULL
           AND NOT EXISTS (
               SELECT 1 FROM event_log newer
               WHERE newer.run_id = latest.run_id
                 AND newer.event_type = 'run_status_changed'
                 AND newer.id > latest.id
           )
           AND NOT EXISTS (
               SELECT 1 FROM runtime_run_ownership owned
               WHERE owned.run_id = latest.run_id
           )
           AND CASE
               WHEN json_valid(latest.payload) = 0 THEN 1
               WHEN json_type(latest.payload, '$.RunStatusChanged') != 'object' THEN 1
               WHEN length(latest.run_id) != 36
                    OR substr(latest.run_id, 9, 1) != '-'
                    OR substr(latest.run_id, 14, 1) != '-'
                    OR substr(latest.run_id, 19, 1) != '-'
                    OR substr(latest.run_id, 24, 1) != '-'
                    OR length(replace(latest.run_id, '-', '')) != 32
                    OR lower(replace(latest.run_id, '-', '')) GLOB '*[^0-9a-f]*'
                    THEN 1
               WHEN json_type(latest.payload, '$.RunStatusChanged.run_id') IS NOT 'text'
                    THEN 1
               WHEN lower(json_extract(latest.payload, '$.RunStatusChanged.run_id'))
                    != lower(latest.run_id) THEN 1
               WHEN json_type(latest.payload, '$.RunStatusChanged.lifecycle') IS NOT 'text'
                    THEN 1
               WHEN json_extract(latest.payload, '$.RunStatusChanged.lifecycle')
                    IN ('Complete','Interrupted','Cancelled','Failed') THEN 0
               ELSE 1
           END = 1",
    );
    if project_id.is_some() {
        sql.push_str(" AND latest.project_id = ?1");
    }
    if session_id.is_some() {
        sql.push_str(if project_id.is_some() {
            " AND latest.session_id = ?2"
        } else {
            " AND latest.session_id = ?1"
        });
    }
    sql.push_str(" ORDER BY latest.id DESC LIMIT 33");
    let mut statement = connection.prepare(&sql)?;
    let mut rows = match (project_id, session_id) {
        (Some(project), Some(session)) => {
            statement.query(params![project.to_string(), session.to_string()])?
        }
        (Some(project), None) => statement.query([project.to_string()])?,
        (None, Some(session)) => statement.query([session.to_string()])?,
        (None, None) => statement.query([])?,
    };
    let mut ambiguities = Vec::new();
    while let Some(row) = rows.next()? {
        let run_id = parse_uuid(row.get::<_, String>(0)?, "legacy run identity")?;
        let project_id = row
            .get::<_, Option<String>>(1)?
            .map(|value| parse_uuid(value, "legacy project identity"))
            .transpose()?;
        let session_id = row
            .get::<_, Option<String>>(2)?
            .map(|value| parse_uuid(value, "legacy session identity"))
            .transpose()?;
        let event: AppEvent = serde_json::from_str(&row.get::<_, String>(3)?)?;
        let AppEvent::RunStatusChanged {
            run_id: event_run_id,
            lifecycle,
        } = event
        else {
            return Err(StoreError::InvalidRecord(
                "legacy status event is malformed".into(),
            ));
        };
        if event_run_id != run_id || lifecycle.is_terminal() {
            return Err(StoreError::InvalidRecord(
                "legacy status event binding is invalid".into(),
            ));
        }
        ambiguities.push(LegacyOwnershipAmbiguity {
            run_id,
            project_id,
            session_id,
            last_lifecycle: lifecycle_name(lifecycle).into(),
            reason: "legacy_nonterminal_run_without_lifetime_owner_evidence".into(),
        });
    }
    let truncated = ambiguities.len() > MAX_RECOVERY_RUNS;
    ambiguities.truncate(MAX_RECOVERY_RUNS);
    Ok((ambiguities, truncated))
}

fn table_exists_on(connection: &Connection, table: &str) -> Result<bool, StoreError> {
    Ok(connection.query_row(
        "SELECT EXISTS(SELECT 1 FROM sqlite_schema WHERE type = 'table' AND name = ?1)",
        [table],
        |row| row.get(0),
    )?)
}

fn legacy_work_ambiguities_on(
    connection: &Connection,
    project_id: Option<Uuid>,
    session_id: Option<Uuid>,
    pre17: bool,
) -> Result<(Vec<LegacyWorkAmbiguity>, bool), StoreError> {
    if pre17
        && (!table_exists_on(connection, "goal_invocations")?
            || !table_exists_on(connection, "goal_runs")?
            || !table_exists_on(connection, "goals")?)
    {
        return Ok((Vec::new(), false));
    }
    let invocation_owner_filter = if pre17 {
        ""
    } else {
        "AND (gi.owner_generation IS NULL OR NOT EXISTS (
             SELECT 1 FROM runtime_host_generations host
             WHERE host.generation = gi.owner_generation
               AND host.state IN ('active','recovery_pending')
         ))"
    };
    let run_owner_filter = if pre17 {
        ""
    } else {
        "AND NOT EXISTS (
             SELECT 1 FROM runtime_run_ownership owned
             WHERE owned.run_id = gr.run_id
         )"
    };
    let mut sql = format!(
        "SELECT ambiguous.kind, ambiguous.id, ambiguous.goal_id,
                ambiguous.project_id, ambiguous.session_id, ambiguous.state,
                ambiguous.payload
         FROM (
             SELECT 'goal_invocation' AS kind, gi.id, gi.goal_id,
                    gi.project_id, gi.session_id, gi.state, gi.payload,
                    gi.started_at AS ordering_key
             FROM goal_invocations gi
             WHERE gi.state = 'active'
               {invocation_owner_filter}
             UNION ALL
             SELECT 'goal_run' AS kind, gr.run_id AS id, gr.goal_id,
                    goal.project_id, goal.session_id, 'unfinished' AS state,
                    gr.payload, gr.rowid AS ordering_key
             FROM goal_runs gr
             LEFT JOIN goals goal ON goal.id = gr.goal_id
             WHERE CASE
                 WHEN json_valid(gr.payload) = 0 THEN 1
                 WHEN json_extract(gr.payload, '$.finished_at') IS NULL THEN 1
                 ELSE 0
             END = 1
               {run_owner_filter}
         ) ambiguous
         WHERE 1 = 1"
    );
    if project_id.is_some() {
        sql.push_str(" AND ambiguous.project_id = ?1");
    }
    if session_id.is_some() {
        sql.push_str(if project_id.is_some() {
            " AND ambiguous.session_id = ?2"
        } else {
            " AND ambiguous.session_id = ?1"
        });
    }
    sql.push_str(" ORDER BY ambiguous.ordering_key DESC, ambiguous.id ASC LIMIT 33");
    let mut statement = connection.prepare(&sql)?;
    let mut rows = match (project_id, session_id) {
        (Some(project), Some(session)) => {
            statement.query(params![project.to_string(), session.to_string()])?
        }
        (Some(project), None) => statement.query([project.to_string()])?,
        (None, Some(session)) => statement.query([session.to_string()])?,
        (None, None) => statement.query([])?,
    };
    let mut records = Vec::new();
    while let Some(row) = rows.next()? {
        let kind: String = row.get(0)?;
        let id = parse_uuid(row.get(1)?, "legacy work identity")?;
        let goal_id = parse_uuid(row.get(2)?, "legacy work goal identity")?;
        let project_id = parse_uuid(row.get(3)?, "legacy work project identity")?;
        let session_id = parse_uuid(row.get(4)?, "legacy work session identity")?;
        let state: String = row.get(5)?;
        let payload: String = row.get(6)?;
        let (run_id, reason) = match kind.as_str() {
            "goal_invocation" => {
                let invocation: GoalInvocation = serde_json::from_str(&payload)?;
                if invocation.id != id
                    || invocation.goal_id != goal_id
                    || invocation.project_id != project_id
                    || invocation.session_id != session_id
                    || invocation.state != GoalInvocationState::Active
                    || invocation.owner_claim_retired
                    || state != "active"
                {
                    return Err(StoreError::InvalidRecord(
                        "legacy goal invocation binding is invalid".into(),
                    ));
                }
                (
                    None,
                    "legacy_active_goal_invocation_without_lifetime_owner_evidence",
                )
            }
            "goal_run" => {
                let run: RunAccounting = serde_json::from_str(&payload)?;
                if run.run_id != id
                    || run.binding.goal_id != goal_id
                    || run.project_id != project_id
                    || run.session_id != session_id
                    || run.finished_at.is_some()
                    || state != "unfinished"
                {
                    return Err(StoreError::InvalidRecord(
                        "legacy goal run binding is invalid".into(),
                    ));
                }
                (
                    Some(id),
                    "legacy_unfinished_goal_run_without_lifetime_owner_evidence",
                )
            }
            _ => {
                return Err(StoreError::InvalidRecord(
                    "legacy work kind is invalid".into(),
                ));
            }
        };
        records.push(LegacyWorkAmbiguity {
            kind,
            id,
            goal_id,
            project_id,
            session_id,
            run_id,
            state,
            reason: reason.into(),
        });
    }
    let truncated = records.len() > MAX_RECOVERY_RUNS;
    records.truncate(MAX_RECOVERY_RUNS);
    Ok((records, truncated))
}

fn parse_uuid(value: String, field: &str) -> Result<Uuid, StoreError> {
    Uuid::parse_str(&value).map_err(|_| StoreError::InvalidRecord(format!("{field} is invalid")))
}

impl Store {
    pub fn inspect_recovery(path: impl AsRef<Path>) -> Result<RecoveryInspection, StoreError> {
        let path = path.as_ref();
        #[cfg(not(windows))]
        {
            let _ = path;
            return Err(StoreError::RecoveryUnsupported);
        }
        #[cfg(windows)]
        {
            let (_file, physical) = pinned_physical_identity(path, false, false)?;
            let connection = Connection::open_with_flags(
                &physical.canonical_path,
                OpenFlags::SQLITE_OPEN_READ_ONLY,
            )?;
            connection.busy_timeout(DATABASE_BUSY_TIMEOUT)?;
            let schema: u16 = connection.query_row("PRAGMA user_version", [], |row| row.get(0))?;
            if schema < 17 {
                let (legacy_ambiguities, legacy_ambiguities_truncated) =
                    legacy_ambiguities_pre17_on(&connection)?;
                let (legacy_work_ambiguities, legacy_work_ambiguities_truncated) =
                    legacy_work_ambiguities_on(&connection, None, None, true)?;
                return Ok(RecoveryInspection {
                    version: RECOVERY_VERSION,
                    ownership: unsupported_owner_evidence(
                        OwnershipOutcome::OwnershipUnknown,
                        "legacy_schema_without_durable_owner_generation",
                    ),
                    runs: Vec::new(),
                    runs_truncated: false,
                    legacy_ambiguities,
                    legacy_ambiguities_truncated,
                    legacy_work_ambiguities,
                    legacy_work_ambiguities_truncated,
                    mutation_performed: false,
                });
            }
            let store_id = read_store_id_on(&connection)?;
            let active = read_active_host_on(&connection, store_id)?;
            let ownership = inspect_owner_evidence(active.as_ref(), store_id, &physical);
            let (runs, runs_truncated) = recovery_runs_on(&connection)?;
            let (legacy_ambiguities, legacy_ambiguities_truncated) =
                legacy_ambiguities_on(&connection, None, None)?;
            let (legacy_work_ambiguities, legacy_work_ambiguities_truncated) =
                legacy_work_ambiguities_on(&connection, None, None, false)?;
            Ok(RecoveryInspection {
                version: RECOVERY_VERSION,
                ownership,
                runs,
                runs_truncated,
                legacy_ambiguities,
                legacy_ambiguities_truncated,
                legacy_work_ambiguities,
                legacy_work_ambiguities_truncated,
                mutation_performed: false,
            })
        }
    }

    pub fn reconcile_recovery(path: impl AsRef<Path>) -> Result<RecoverySettlement, StoreError> {
        let path = path.as_ref();
        #[cfg(not(windows))]
        {
            let _ = path;
            return Err(StoreError::RecoveryUnsupported);
        }
        #[cfg(windows)]
        {
            let (_file, physical) = pinned_physical_identity(path, true, false)?;
            let _guard = acquire_named_mutex(&physical.digest)?;
            let mut connection = Connection::open(&physical.canonical_path)?;
            connection.busy_timeout(DATABASE_BUSY_TIMEOUT)?;
            let schema: u16 = connection.query_row("PRAGMA user_version", [], |row| row.get(0))?;
            if schema < 17 {
                return Err(StoreError::HostOwnershipUnknown(
                    "legacy_schema_without_durable_owner_generation",
                ));
            }
            let transaction =
                connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
            let store_id = read_store_id_on(&transaction)?;
            let Some(active) = read_active_host_on(&transaction, store_id)? else {
                transaction.commit()?;
                return Ok(RecoverySettlement {
                    version: RECOVERY_VERSION,
                    ownership: no_owner_evidence(Some(store_id), Some(&physical)),
                    recovery_revision: None,
                    recovered_run_ids: Vec::new(),
                    recovered_invocation_ids: Vec::new(),
                    incomplete: false,
                    mutation_performed: false,
                });
            };
            if active.physical_identity != physical.value {
                return Err(StoreError::HostOwnershipUnknown(
                    "active_owner_belongs_to_a_different_physical_store_copy",
                ));
            }
            let recovery_revision = Uuid::new_v4();
            let at = now();
            let (claims, run_batch_truncated) =
                active_run_claims_on(&transaction, active.generation, MAX_RECOVERY_RUNS + 1)?;
            let mut recovered_run_ids = Vec::new();
            for mut claim in claims.into_iter().take(MAX_RECOVERY_RUNS) {
                reconcile_run_on(&transaction, &mut claim, recovery_revision, at)?;
                recovered_run_ids.push(claim.run_id);
            }
            let (invocations, invocation_batch_truncated) =
                recoverable_invocations_on(&transaction, active.generation, MAX_RECOVERY_RUNS + 1)?;
            let mut recovered_invocation_ids = Vec::new();
            for mut invocation in invocations.into_iter().take(MAX_RECOVERY_RUNS) {
                reconcile_invocation_on(
                    &transaction,
                    &mut invocation,
                    active.generation,
                    recovery_revision,
                    at,
                )?;
                recovered_invocation_ids.push(invocation.id);
            }
            // Run claims are reconciled first. A command linked to a run can be
            // reconstructed as successful, failed, or cancelled only when that
            // authoritative run already has the matching durable terminal state.
            // Every other unfinished command remains bound and becomes
            // interrupted or unknown without redispatch.
            let command_batch_truncated = if schema >= 18 {
                super::command_identity::reconcile_commands_on(
                    &transaction,
                    active.generation,
                    recovery_revision,
                    at,
                )?
                .1
            } else {
                false
            };
            let run_remaining: bool = transaction.query_row(
                "SELECT EXISTS(
                     SELECT 1 FROM runtime_run_ownership
                     WHERE owner_generation = ?1 AND state = 'active'
                 )",
                [active.generation],
                |row| row.get(0),
            )?;
            let invocation_remaining: bool = transaction.query_row(
                "SELECT EXISTS(
                     SELECT 1 FROM goal_invocations
                     WHERE owner_generation = ?1
                       AND (json_extract(payload, '$.owner_claim_retired') IS NULL
                            OR json_extract(payload, '$.owner_claim_retired') = 0)
                 )",
                [active.generation],
                |row| row.get(0),
            )?;
            let command_remaining = schema >= 18
                && super::command_identity::active_command_count_on(
                    &transaction,
                    active.generation,
                )? > 0;
            let incomplete = run_batch_truncated
                || invocation_batch_truncated
                || command_batch_truncated
                || run_remaining
                || invocation_remaining
                || command_remaining;
            let next_state = if incomplete {
                "recovery_pending"
            } else {
                "recovered"
            };
            let reason = if incomplete {
                "bounded_recovery_batch_incomplete"
            } else {
                "proven_abandoned_owner_reconciled"
            };
            if transaction.execute(
                "UPDATE runtime_host_generations
                 SET state = ?2, finished_at = ?3, recovery_revision = ?4, reason = ?5
                 WHERE generation = ?1 AND state IN ('active','recovery_pending')",
                params![
                    active.generation,
                    next_state,
                    at.unix_timestamp(),
                    recovery_revision.to_string(),
                    reason,
                ],
            )? != 1
            {
                return Err(StoreError::RecoveryDecisionStale);
            }
            transaction.commit()?;
            let mut ownership =
                owner_evidence_from_record(&active, OwnershipOutcome::ProvenAbandoned, reason);
            ownership.state = Some(if incomplete {
                HostGenerationState::RecoveryPending
            } else {
                HostGenerationState::Recovered
            });
            Ok(RecoverySettlement {
                version: RECOVERY_VERSION,
                ownership,
                recovery_revision: Some(recovery_revision),
                recovered_run_ids,
                recovered_invocation_ids,
                incomplete,
                mutation_performed: true,
            })
        }
    }

    pub fn review_recovery(
        path: impl AsRef<Path>,
        run_id: Uuid,
        expected_revision: Uuid,
    ) -> Result<RunRecoveryRecord, StoreError> {
        let path = path.as_ref();
        #[cfg(not(windows))]
        {
            let _ = (path, run_id, expected_revision);
            return Err(StoreError::RecoveryUnsupported);
        }
        #[cfg(windows)]
        {
            if run_id.is_nil() || expected_revision.is_nil() {
                return Err(StoreError::RecoveryDecisionStale);
            }
            let (_file, physical) = pinned_physical_identity(path, true, false)?;
            let _guard = acquire_named_mutex(&physical.digest)?;
            let mut connection = Connection::open(&physical.canonical_path)?;
            connection.busy_timeout(DATABASE_BUSY_TIMEOUT)?;
            let transaction =
                connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
            let store_id = read_store_id_on(&transaction)?;
            if let Some(active) = read_active_host_on(&transaction, store_id)? {
                if active.physical_identity != physical.value {
                    return Err(StoreError::HostOwnershipUnknown(
                        "active_owner_belongs_to_a_different_physical_store_copy",
                    ));
                }
                return Err(StoreError::HostRecoveryRequired);
            }
            let mut claim = read_run_claim_on(&transaction, run_id)?
                .ok_or(StoreError::RecoveryDecisionStale)?;
            if claim.execution_state != RecoveryExecutionState::Interrupted
                || claim.recovery_revision != Some(expected_revision)
                || !claim.admission_blocked
                || claim.owner_reviewed_at.is_some()
            {
                return Err(StoreError::RecoveryDecisionStale);
            }
            let owner_physical: String = transaction.query_row(
                "SELECT physical_identity FROM runtime_host_generations WHERE generation = ?1",
                [claim.owner_generation],
                |row| row.get(0),
            )?;
            if owner_physical != physical.value {
                return Err(StoreError::HostOwnershipUnknown(
                    "recovery_record_belongs_to_a_different_physical_store_copy",
                ));
            }
            if claim
                .unresolved_effects
                .iter()
                .any(|reason| reason == "uncontained_process_may_still_be_running")
            {
                return Err(StoreError::RecoveryBlocked(
                    "uncontained_process_may_still_be_running",
                ));
            }
            claim.owner_reviewed_at = Some(now());
            claim.admission_blocked = false;
            claim.updated_at = now();
            write_run_claim_on(&transaction, &claim)?;
            transaction.commit()?;
            Ok(claim)
        }
    }
}

fn unsupported_owner_evidence(outcome: OwnershipOutcome, reason: &str) -> HostOwnerEvidence {
    HostOwnerEvidence {
        version: RECOVERY_VERSION,
        supported_environment: cfg!(windows),
        mechanism: OWNERSHIP_MECHANISM.into(),
        canonical_identity_method: IDENTITY_METHOD.into(),
        store_id: None,
        physical_identity_sha256: None,
        generation: None,
        owner_id: None,
        process_id: None,
        process_start_key: None,
        state: None,
        outcome,
        reason: reason.into(),
    }
}

fn no_owner_evidence(
    store_id: Option<Uuid>,
    physical: Option<&PhysicalStoreIdentity>,
) -> HostOwnerEvidence {
    HostOwnerEvidence {
        store_id,
        physical_identity_sha256: physical.map(|identity| identity.digest.clone()),
        ..unsupported_owner_evidence(OwnershipOutcome::NoActiveOwner, "no_unretired_owner")
    }
}

fn owner_evidence_from_record(
    record: &HostGenerationRecord,
    outcome: OwnershipOutcome,
    reason: &str,
) -> HostOwnerEvidence {
    HostOwnerEvidence {
        version: RECOVERY_VERSION,
        supported_environment: cfg!(windows),
        mechanism: OWNERSHIP_MECHANISM.into(),
        canonical_identity_method: IDENTITY_METHOD.into(),
        store_id: Some(record.store_id),
        physical_identity_sha256: Some(record.physical_identity_sha256.clone()),
        generation: Some(record.generation),
        owner_id: Some(record.owner_id),
        process_id: Some(record.process_id),
        process_start_key: Some(record.process_start_key),
        state: Some(record.state),
        outcome,
        reason: reason.into(),
    }
}

#[cfg(windows)]
fn inspect_owner_evidence(
    active: Option<&HostGenerationRecord>,
    store_id: Uuid,
    physical: &PhysicalStoreIdentity,
) -> HostOwnerEvidence {
    let Some(active) = active else {
        return no_owner_evidence(Some(store_id), Some(physical));
    };
    if active.physical_identity != physical.value {
        return owner_evidence_from_record(
            active,
            OwnershipOutcome::OwnershipUnknown,
            "active_owner_belongs_to_a_different_physical_store_copy",
        );
    }
    match acquire_named_mutex(&physical.digest) {
        Ok(_guard) => owner_evidence_from_record(
            active,
            OwnershipOutcome::ProvenAbandoned,
            "lifetime_guard_absent_and_durable_generation_matches",
        ),
        Err(StoreError::HostOwnershipLive) => {
            owner_evidence_from_record(active, OwnershipOutcome::Live, "lifetime_guard_is_held")
        }
        Err(_) => owner_evidence_from_record(
            active,
            OwnershipOutcome::OwnershipUnknown,
            "lifetime_guard_could_not_be_inspected",
        ),
    }
}

fn recovery_runs_on(connection: &Connection) -> Result<(Vec<RunRecoveryRecord>, bool), StoreError> {
    let mut statement = connection.prepare(
        "SELECT run_id, payload FROM runtime_run_ownership
         ORDER BY updated_at DESC, run_id ASC LIMIT 33",
    )?;
    let rows = statement
        .query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    let truncated = rows.len() > MAX_RECOVERY_RUNS;
    let mut records = Vec::new();
    for (id, payload) in rows.into_iter().take(MAX_RECOVERY_RUNS) {
        let run_id = parse_uuid(id, "run recovery identity")?;
        let record: RunRecoveryRecord = serde_json::from_str(&payload)?;
        validate_run_claim(&record, run_id)?;
        records.push(record);
    }
    Ok((records, truncated))
}

fn active_run_claims_on(
    connection: &Connection,
    generation: u64,
    limit: usize,
) -> Result<(Vec<RunRecoveryRecord>, bool), StoreError> {
    let limit = i64::try_from(limit)
        .map_err(|_| StoreError::InvalidRecord("recovery run limit is invalid".into()))?;
    let mut statement = connection.prepare(
        "SELECT run_id, payload FROM runtime_run_ownership
         WHERE owner_generation = ?1 AND state = 'active'
         ORDER BY updated_at ASC, run_id ASC LIMIT ?2",
    )?;
    let rows = statement
        .query_map(params![generation, limit], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    let truncated = rows.len() > MAX_RECOVERY_RUNS;
    let mut records = Vec::new();
    for (id, payload) in rows {
        let run_id = parse_uuid(id, "active recovery run identity")?;
        let record: RunRecoveryRecord = serde_json::from_str(&payload)?;
        validate_run_claim(&record, run_id)?;
        if record.owner_generation != generation
            || record.execution_state != RecoveryExecutionState::Active
        {
            return Err(StoreError::InvalidRecord(
                "active recovery run binding is inconsistent".into(),
            ));
        }
        records.push(record);
    }
    Ok((records, truncated))
}

fn recoverable_invocations_on(
    connection: &Connection,
    generation: u64,
    limit: usize,
) -> Result<(Vec<GoalInvocation>, bool), StoreError> {
    let limit = i64::try_from(limit)
        .map_err(|_| StoreError::InvalidRecord("recovery invocation limit is invalid".into()))?;
    let mut statement = connection.prepare(
        "SELECT payload FROM goal_invocations
         WHERE owner_generation = ?1
           AND (json_extract(payload, '$.owner_claim_retired') IS NULL
                OR json_extract(payload, '$.owner_claim_retired') = 0)
         ORDER BY started_at ASC, id ASC LIMIT ?2",
    )?;
    let rows = statement
        .query_map(params![generation, limit], |row| row.get::<_, String>(0))?
        .collect::<Result<Vec<_>, _>>()?;
    let truncated = rows.len() > MAX_RECOVERY_RUNS;
    let mut invocations = Vec::new();
    for payload in rows {
        let invocation: GoalInvocation = serde_json::from_str(&payload)?;
        if invocation.owner_generation != Some(generation) {
            return Err(StoreError::InvalidRecord(
                "goal invocation owner generation is inconsistent".into(),
            ));
        }
        invocations.push(invocation);
    }
    Ok((invocations, truncated))
}

fn reconcile_invocation_on(
    transaction: &Connection,
    invocation: &mut GoalInvocation,
    generation: u64,
    recovery_revision: Uuid,
    at: OffsetDateTime,
) -> Result<(), StoreError> {
    if invocation.owner_generation != Some(generation) || invocation.owner_claim_retired {
        return Err(StoreError::RecoveryDecisionStale);
    }
    invocation.owner_claim_retired = true;
    if invocation.state == GoalInvocationState::Active {
        if let Some(attempt) = invocation
            .attempts
            .last_mut()
            .filter(|attempt| attempt.finished_at.is_none())
        {
            attempt.outcome = InvocationAttemptOutcome::Unresolved;
            attempt.finished_at = Some(at);
            attempt.stop_reason = Some("owner_process_interrupted".into());
        }
        invocation.state = GoalInvocationState::Interrupted;
        invocation.stop_reason = Some("owner_process_interrupted".into());
        invocation.finished_at = Some(at);
        let mut goal = goal_by_id(transaction, invocation.goal_id)?;
        if goal.lifecycle != GoalLifecycle::Complete {
            goal.lifecycle = GoalLifecycle::Paused;
            goal.next_action = format!(
                "Review interrupted invocation {} at recovery revision {}",
                invocation.id, recovery_revision
            );
            goal.updated_at = at;
            write_goal(transaction, &mut goal)?;
        }
    }
    let state = invocation_state_name(invocation.state);
    if transaction.execute(
        "UPDATE goal_invocations
         SET state = ?2, payload = ?3
         WHERE id = ?1 AND owner_generation = ?4",
        params![
            invocation.id.to_string(),
            state,
            serde_json::to_string(invocation)?,
            generation,
        ],
    )? != 1
    {
        return Err(StoreError::RecoveryDecisionStale);
    }
    Ok(())
}

fn invocation_state_name(state: GoalInvocationState) -> &'static str {
    match state {
        GoalInvocationState::Active => "active",
        GoalInvocationState::Verified => "verified",
        GoalInvocationState::Incomplete => "incomplete",
        GoalInvocationState::Failed => "failed",
        GoalInvocationState::Interrupted => "unresolved",
        GoalInvocationState::Cancelled => "cancelled",
        GoalInvocationState::ReviewRequired => "review_required",
        GoalInvocationState::Unresolved => "unresolved",
    }
}

fn reconcile_run_on(
    transaction: &Connection,
    claim: &mut RunRecoveryRecord,
    recovery_revision: Uuid,
    at: OffsetDateTime,
) -> Result<(), StoreError> {
    if claim.execution_state != RecoveryExecutionState::Active {
        return Err(StoreError::RecoveryDecisionStale);
    }
    let workspace_key: String = transaction.query_row(
        "SELECT workspace_key FROM projects WHERE id = ?1",
        [claim.project_id.to_string()],
        |row| row.get(0),
    )?;
    if workspace_key.is_empty() || workspace_key.len() > 32 * 1024 {
        return Err(StoreError::InvalidRecord(
            "recovery workspace identity is invalid".into(),
        ));
    }
    let (known_effects, mut unresolved_effects) = collect_effect_references_on(
        transaction,
        claim.run_id,
        claim.effect_state,
        &claim.effect_kind,
        &claim.effect_containment,
    )?;
    let pending_approvals = retire_approval_authority_on(transaction, claim, &workspace_key, at)?;
    if pending_approvals {
        unresolved_effects.push("approval_claimed_without_settled_effect".into());
    }
    if matches!(
        claim.request_state,
        RecoveryRequestState::Dispatched | RecoveryRequestState::PartialResponse
    ) {
        unresolved_effects.push("provider_request_outcome_unknown".into());
    }
    unresolved_effects.sort();
    unresolved_effects.dedup();
    if unresolved_effects.len() > MAX_RECOVERY_REFERENCES {
        return Err(StoreError::RecoveryBlocked(
            "unresolved_effect_reference_limit_exceeded",
        ));
    }
    let accounting_state = reconcile_accounting_on(transaction, claim, at)?;
    claim.known_effects = known_effects;
    claim.unresolved_effects = unresolved_effects;
    claim.accounting_state = accounting_state;
    claim.authority_invalidated = true;
    claim.execution_state = RecoveryExecutionState::Interrupted;
    claim.recovered_at = Some(at);
    claim.updated_at = at;
    claim.recovery_revision = Some(recovery_revision);
    claim.interruption_reason = Some("owner_process_interrupted".into());
    claim.admission_blocked = true;
    append_event_on(
        transaction,
        Some(&workspace_key),
        Some(claim.project_id),
        Some(claim.session_id),
        Some(claim.run_id),
        &AppEvent::RunStatusChanged {
            run_id: claim.run_id,
            lifecycle: TaskLifecycle::Interrupted,
        },
    )?;
    write_run_claim_on(transaction, claim)
}

fn collect_effect_references_on(
    connection: &Connection,
    run_id: Uuid,
    effect_state: RecoveryEffectState,
    effect_kind: &str,
    containment: &str,
) -> Result<(Vec<RecoveryReference>, Vec<String>), StoreError> {
    let mut references = Vec::new();
    let mut unresolved = Vec::new();
    let mut changes = connection.prepare(
        "SELECT id, state FROM change_operations
         WHERE run_id = ?1 ORDER BY created_at ASC, id ASC LIMIT 65",
    )?;
    let change_rows = changes
        .query_map([run_id.to_string()], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    if change_rows.len() > MAX_RECOVERY_REFERENCES {
        return Err(StoreError::RecoveryBlocked(
            "change_reference_limit_exceeded",
        ));
    }
    for (id, state) in change_rows {
        let id = parse_uuid(id, "change operation identity")?;
        match state.as_str() {
            "applied" | "finalized" => references.push(RecoveryReference {
                kind: "change_operation".into(),
                id,
                state,
            }),
            "prepared" => unresolved.push("prepared_file_change_requires_recovery".into()),
            "conflicted" => unresolved.push("file_change_conflict_requires_review".into()),
            "not_applied" => {}
            _ => {
                return Err(StoreError::InvalidRecord(
                    "change recovery state is invalid".into(),
                ))
            }
        }
    }
    let mut audits = connection.prepare(
        "SELECT id, payload FROM execution_audit
         WHERE run_id = ?1 ORDER BY recorded_at ASC, id ASC LIMIT 65",
    )?;
    let audit_rows = audits
        .query_map([run_id.to_string()], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    if references.len().saturating_add(audit_rows.len()) > MAX_RECOVERY_REFERENCES {
        return Err(StoreError::RecoveryBlocked(
            "effect_reference_limit_exceeded",
        ));
    }
    for (id, payload) in audit_rows {
        let id = parse_uuid(id, "execution audit identity")?;
        let audit: argentum_domain::ExecutionAuditRecord = serde_json::from_str(&payload)?;
        references.push(RecoveryReference {
            kind: "execution_audit".into(),
            id,
            state: format!("{:?}", audit.outcome).to_ascii_lowercase(),
        });
    }
    if effect_state == RecoveryEffectState::Admitted {
        match effect_kind {
            "workspace_change" if containment == "workspace_change_journal" => {
                if references.is_empty() && unresolved.is_empty() {
                    unresolved.push("workspace_effect_outcome_unknown".into());
                }
            }
            "workspace_change" => unresolved.push("workspace_effect_outcome_unknown".into()),
            "external_process" if containment == "windows_job_kill_on_close" => {
                unresolved.push("contained_effect_outcome_unknown".into());
            }
            "external_process" => {
                unresolved.push("uncontained_process_may_still_be_running".into());
            }
            _ => unresolved.push("effect_outcome_unknown".into()),
        }
    }
    Ok((references, unresolved))
}

fn retire_approval_authority_on(
    connection: &Connection,
    claim: &RunRecoveryRecord,
    workspace_key: &str,
    at: OffsetDateTime,
) -> Result<bool, StoreError> {
    let mut statement = connection.prepare(
        "SELECT payload FROM event_log
         WHERE run_id = ?1
           AND event_type IN ('approval_requested','approval_resolved','tool_finished')
         ORDER BY id ASC LIMIT 129",
    )?;
    let payloads = statement
        .query_map([claim.run_id.to_string()], |row| row.get::<_, String>(0))?
        .collect::<Result<Vec<_>, _>>()?;
    if payloads.len() > MAX_RECOVERY_APPROVAL_EVENTS {
        return Err(StoreError::RecoveryBlocked("approval_event_limit_exceeded"));
    }
    let mut requests = BTreeMap::new();
    let mut resolved = BTreeMap::new();
    let mut finished_calls = BTreeSet::new();
    for payload in payloads {
        match serde_json::from_str::<AppEvent>(&payload)? {
            AppEvent::ApprovalRequested(request) => {
                requests.insert(request.id, request);
            }
            AppEvent::ApprovalResolved {
                approval_id,
                approved,
            } => {
                resolved.insert(approval_id, approved);
            }
            AppEvent::ToolFinished(trace) => {
                finished_calls.insert(trace.id);
            }
            _ => {}
        }
    }
    let mut claimed_without_settlement = false;
    for (approval_id, request) in requests {
        match resolved.get(&approval_id).copied() {
            None => {
                append_event_on(
                    connection,
                    Some(workspace_key),
                    Some(claim.project_id),
                    Some(claim.session_id),
                    Some(claim.run_id),
                    &AppEvent::ApprovalResolved {
                        approval_id,
                        approved: false,
                    },
                )?;
            }
            Some(true)
                if claim
                    .effect_call_id
                    .is_some_and(|id| !finished_calls.contains(&id)) =>
            {
                let _ = request;
                claimed_without_settlement = true;
            }
            Some(_) => {}
        }
    }
    let mut grants = connection.prepare(
        "SELECT id, payload FROM approval_grants
         WHERE project_id = ?1 AND scope = 'run' AND revoked_at IS NULL
           AND instr(payload, ?2) > 0
         ORDER BY created_at ASC, id ASC LIMIT 129",
    )?;
    let grant_rows = grants
        .query_map(
            params![claim.project_id.to_string(), claim.run_id.to_string()],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
        )?
        .collect::<Result<Vec<_>, _>>()?;
    if grant_rows.len() > MAX_RECOVERY_APPROVAL_EVENTS {
        return Err(StoreError::RecoveryBlocked("run_grant_limit_exceeded"));
    }
    for (id, payload) in grant_rows {
        let id = parse_uuid(id, "approval grant identity")?;
        let mut grant: argentum_domain::ApprovalGrantRecord = serde_json::from_str(&payload)?;
        if grant.scope != GrantScope::Run
            || !matches!(grant.subject, GrantSubject::Run { run_id } if run_id == claim.run_id)
        {
            continue;
        }
        grant.revoked_at = Some(at);
        connection.execute(
            "UPDATE approval_grants SET revoked_at = ?2, payload = ?3
             WHERE id = ?1 AND revoked_at IS NULL",
            params![
                id.to_string(),
                at.unix_timestamp(),
                serde_json::to_string(&grant)?,
            ],
        )?;
        append_event_on(
            connection,
            Some(workspace_key),
            Some(claim.project_id),
            Some(claim.session_id),
            Some(claim.run_id),
            &AppEvent::ApprovalGrantRevoked {
                grant_id: id,
                revoked_at: at,
            },
        )?;
    }
    Ok(claimed_without_settlement)
}

fn reconcile_accounting_on(
    connection: &Connection,
    claim: &RunRecoveryRecord,
    at: OffsetDateTime,
) -> Result<RecoveryAccountingState, StoreError> {
    let run_payload: Option<String> = connection
        .query_row(
            "SELECT payload FROM goal_runs WHERE run_id = ?1",
            [claim.run_id.to_string()],
            |row| row.get(0),
        )
        .optional()?;
    let Some(run_payload) = run_payload else {
        return Ok(RecoveryAccountingState::NotAssociated);
    };
    let mut run: RunAccounting = serde_json::from_str(&run_payload)?;
    if run.run_id != claim.run_id {
        return Err(StoreError::InvalidRecord(
            "goal run recovery binding is invalid".into(),
        ));
    }
    let mut goal = goal_by_id(connection, run.binding.goal_id)?;
    let account = goal.accounting.as_mut().ok_or_else(|| {
        StoreError::InvalidRecord("goal accounting is unavailable during recovery".into())
    })?;
    let mut statement = connection.prepare(
        "SELECT request_id, payload FROM goal_requests
         WHERE run_id = ?1 ORDER BY rowid ASC LIMIT 129",
    )?;
    let request_rows = statement
        .query_map([claim.run_id.to_string()], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    if request_rows.len() > MAX_RECOVERY_REQUESTS {
        return Err(StoreError::RecoveryBlocked(
            "accounting_request_limit_exceeded",
        ));
    }
    let mut proven_undispatched = false;
    let mut unresolved = false;
    for (id, payload) in request_rows {
        let request_id = parse_uuid(id, "goal request identity")?;
        let mut charge: RequestCharge = serde_json::from_str(&payload)?;
        if charge.request_id != request_id || charge.run_id != claim.run_id {
            return Err(StoreError::InvalidRecord(
                "goal request recovery binding is invalid".into(),
            ));
        }
        let next = match charge.state {
            ChargeState::Reserved
                if claim.request_id == Some(request_id)
                    && claim.request_state == RecoveryRequestState::PreparedNotDispatched =>
            {
                proven_undispatched = true;
                Some(ChargeState::NotDispatched)
            }
            ChargeState::Reserved | ChargeState::Dispatched => {
                unresolved = true;
                Some(ChargeState::Unresolved)
            }
            ChargeState::Unresolved => {
                unresolved = true;
                None
            }
            ChargeState::Settled | ChargeState::NotDispatched => None,
        };
        if let Some(next) = next {
            for counters in [&mut account.counters, &mut run.counters] {
                counters.outstanding_tokens = counters
                    .outstanding_tokens
                    .checked_sub(charge.reserved_tokens)
                    .ok_or_else(|| {
                        StoreError::InvalidRecord("goal recovery accounting underflow".into())
                    })?;
                if next == ChargeState::Unresolved {
                    counters.unresolved_tokens = counters
                        .unresolved_tokens
                        .checked_add(charge.reserved_tokens)
                        .ok_or_else(|| {
                            StoreError::InvalidRecord("goal recovery accounting overflow".into())
                        })?;
                }
            }
            charge.state = next;
            charge.usage_quality = if next == ChargeState::NotDispatched {
                "proven_not_dispatched"
            } else {
                "missing_final_usage_after_interruption"
            }
            .into();
            connection.execute(
                "UPDATE goal_requests SET payload = ?2 WHERE request_id = ?1",
                params![request_id.to_string(), serde_json::to_string(&charge)?],
            )?;
        }
    }
    if run.finished_at.is_none() {
        run.finished_at = Some(at);
        run.stop_reason = Some("goal_accounting_unresolved".into());
        account.stop_reason = Some(
            if unresolved {
                "goal_usage_unresolved"
            } else {
                "goal_accounting_unresolved"
            }
            .into(),
        );
        connection.execute(
            "UPDATE goal_runs SET payload = ?2 WHERE run_id = ?1",
            params![claim.run_id.to_string(), serde_json::to_string(&run)?],
        )?;
        if goal.lifecycle != GoalLifecycle::Complete {
            goal.lifecycle = GoalLifecycle::Paused;
            goal.next_action = "Review interrupted goal accounting before new goal work".into();
            goal.updated_at = at;
        }
        write_goal(connection, &mut goal)?;
        unresolved = true;
    }
    Ok(if unresolved {
        RecoveryAccountingState::Unresolved
    } else if proven_undispatched {
        RecoveryAccountingState::ProvenUndispatched
    } else {
        RecoveryAccountingState::Settled
    })
}

fn legacy_ambiguities_pre17_on(
    connection: &Connection,
) -> Result<(Vec<LegacyOwnershipAmbiguity>, bool), StoreError> {
    let mut statement = connection.prepare(
        "SELECT latest.run_id, latest.project_id, latest.session_id, latest.payload
         FROM event_log latest
         WHERE latest.event_type = 'run_status_changed'
           AND latest.run_id IS NOT NULL
           AND NOT EXISTS (
               SELECT 1 FROM event_log newer
               WHERE newer.run_id = latest.run_id
                 AND newer.event_type = 'run_status_changed'
                 AND newer.id > latest.id
           )
           AND CASE
               WHEN json_valid(latest.payload) = 0 THEN 1
               WHEN json_type(latest.payload, '$.RunStatusChanged') != 'object' THEN 1
               WHEN length(latest.run_id) != 36
                    OR substr(latest.run_id, 9, 1) != '-'
                    OR substr(latest.run_id, 14, 1) != '-'
                    OR substr(latest.run_id, 19, 1) != '-'
                    OR substr(latest.run_id, 24, 1) != '-'
                    OR length(replace(latest.run_id, '-', '')) != 32
                    OR lower(replace(latest.run_id, '-', '')) GLOB '*[^0-9a-f]*'
                    THEN 1
               WHEN json_type(latest.payload, '$.RunStatusChanged.run_id') IS NOT 'text'
                    THEN 1
               WHEN lower(json_extract(latest.payload, '$.RunStatusChanged.run_id'))
                    != lower(latest.run_id) THEN 1
               WHEN json_type(latest.payload, '$.RunStatusChanged.lifecycle') IS NOT 'text'
                    THEN 1
               WHEN json_extract(latest.payload, '$.RunStatusChanged.lifecycle')
                    IN ('Complete','Interrupted','Cancelled','Failed') THEN 0
               ELSE 1
           END = 1
         ORDER BY latest.id DESC LIMIT 33",
    )?;
    let rows = statement
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, Option<String>>(1)?,
                row.get::<_, Option<String>>(2)?,
                row.get::<_, String>(3)?,
            ))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    let mut records = Vec::new();
    for (run, project, session, payload) in rows {
        let event: AppEvent = serde_json::from_str(&payload)?;
        let run_id = parse_uuid(run, "legacy run identity")?;
        let AppEvent::RunStatusChanged {
            run_id: event_run_id,
            lifecycle,
        } = event
        else {
            return Err(StoreError::InvalidRecord(
                "legacy status event is malformed".into(),
            ));
        };
        if event_run_id != run_id || lifecycle.is_terminal() {
            return Err(StoreError::InvalidRecord(
                "legacy status event binding is invalid".into(),
            ));
        }
        records.push(LegacyOwnershipAmbiguity {
            run_id,
            project_id: project
                .map(|value| parse_uuid(value, "legacy project identity"))
                .transpose()?,
            session_id: session
                .map(|value| parse_uuid(value, "legacy session identity"))
                .transpose()?,
            last_lifecycle: lifecycle_name(lifecycle).into(),
            reason: "legacy_nonterminal_run_without_lifetime_owner_evidence".into(),
        });
    }
    let truncated = records.len() > MAX_RECOVERY_RUNS;
    records.truncate(MAX_RECOVERY_RUNS);
    Ok((records, truncated))
}

#[cfg(windows)]
fn io_as_database_error(error: std::io::Error) -> StoreError {
    StoreError::Database(rusqlite::Error::ToSqlConversionFailure(Box::new(error)))
}

#[cfg(windows)]
pub(super) fn pinned_physical_identity(
    path: &Path,
    writable: bool,
    create: bool,
) -> Result<(File, PhysicalStoreIdentity), StoreError> {
    use std::os::windows::fs::OpenOptionsExt;
    use std::os::windows::io::AsRawHandle;
    use windows_sys::Win32::Storage::FileSystem::{
        GetFileInformationByHandle, BY_HANDLE_FILE_INFORMATION, FILE_SHARE_READ, FILE_SHARE_WRITE,
    };

    let file = std::fs::OpenOptions::new()
        .read(true)
        .write(writable)
        .create(create)
        .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE)
        .open(path)
        .map_err(io_as_database_error)?;
    let canonical_path = std::fs::canonicalize(path).map_err(io_as_database_error)?;
    ensure_supported_local_volume(&canonical_path)?;
    let mut information = BY_HANDLE_FILE_INFORMATION::default();
    if unsafe { GetFileInformationByHandle(file.as_raw_handle(), &mut information) } == 0 {
        return Err(StoreError::HostOwnershipUnknown(
            "database_physical_identity_unavailable",
        ));
    }
    let volume = information.dwVolumeSerialNumber;
    let index =
        (u64::from(information.nFileIndexHigh) << 32) | u64::from(information.nFileIndexLow);
    let value = format!("{volume:08x}:{index:016x}");
    let digest = argentum_domain::Sha256Digest(format!("{:x}", Sha256::digest(value.as_bytes())));
    Ok((
        file,
        PhysicalStoreIdentity {
            canonical_path,
            value,
            digest,
        },
    ))
}

#[cfg(windows)]
fn ensure_supported_local_volume(path: &Path) -> Result<(), StoreError> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Storage::FileSystem::{GetDriveTypeW, GetVolumePathNameW};
    use windows_sys::Win32::System::WindowsProgramming::{
        DRIVE_FIXED, DRIVE_RAMDISK, DRIVE_REMOTE, DRIVE_REMOVABLE,
    };

    let path = path
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect::<Vec<_>>();
    let mut volume_path = vec![0_u16; 32_768];
    if unsafe {
        GetVolumePathNameW(
            path.as_ptr(),
            volume_path.as_mut_ptr(),
            u32::try_from(volume_path.len()).expect("fixed volume-path buffer length"),
        )
    } == 0
    {
        return Err(StoreError::HostOwnershipUnknown(
            "database_volume_identity_unavailable",
        ));
    }
    match unsafe { GetDriveTypeW(volume_path.as_ptr()) } {
        DRIVE_FIXED | DRIVE_REMOVABLE | DRIVE_RAMDISK => Ok(()),
        DRIVE_REMOTE => Err(StoreError::HostOwnershipUnknown(
            "remote_database_filesystem_not_supported",
        )),
        _ => Err(StoreError::HostOwnershipUnknown(
            "database_volume_type_unsupported_or_unknown",
        )),
    }
}

#[cfg(not(windows))]
fn current_process_start_key() -> Result<u64, StoreError> {
    Err(StoreError::RecoveryUnsupported)
}

#[cfg(windows)]
fn current_process_start_key() -> Result<u64, StoreError> {
    use windows_sys::Win32::Foundation::FILETIME;
    use windows_sys::Win32::System::Threading::{GetCurrentProcess, GetProcessTimes};

    let mut created = FILETIME::default();
    let mut exited = FILETIME::default();
    let mut kernel = FILETIME::default();
    let mut user = FILETIME::default();
    let ok = unsafe {
        GetProcessTimes(
            GetCurrentProcess(),
            &mut created,
            &mut exited,
            &mut kernel,
            &mut user,
        )
    };
    if ok == 0 {
        return Err(StoreError::HostOwnershipUnknown(
            "process_start_identity_unavailable",
        ));
    }
    Ok((u64::from(created.dwHighDateTime) << 32) | u64::from(created.dwLowDateTime))
}

#[cfg(windows)]
fn mutex_registry() -> &'static Mutex<BTreeSet<String>> {
    static REGISTRY: OnceLock<Mutex<BTreeSet<String>>> = OnceLock::new();
    REGISTRY.get_or_init(|| Mutex::new(BTreeSet::new()))
}

#[cfg(windows)]
struct WindowsMutexGuard {
    name: String,
    release_sender: Mutex<Option<std::sync::mpsc::Sender<()>>>,
    owner_thread: Mutex<Option<std::thread::JoinHandle<()>>>,
    released: AtomicBool,
}

#[cfg(windows)]
#[derive(Clone, Copy)]
enum MutexAcquireResult {
    Acquired,
    Live,
    Unknown,
}

#[cfg(windows)]
impl WindowsMutexGuard {
    fn release(&self) {
        if !self.released.swap(true, Ordering::AcqRel) {
            if let Ok(mut sender) = self.release_sender.lock() {
                if let Some(sender) = sender.take() {
                    let _ = sender.send(());
                }
            }
            if let Ok(mut owner_thread) = self.owner_thread.lock() {
                if let Some(owner_thread) = owner_thread.take() {
                    let _ = owner_thread.join();
                }
            }
            if let Ok(mut registry) = mutex_registry().lock() {
                registry.remove(&self.name);
            }
        }
    }
}

#[cfg(windows)]
impl Drop for WindowsMutexGuard {
    fn drop(&mut self) {
        self.release();
    }
}

#[cfg(windows)]
fn mutex_name(digest: &argentum_domain::Sha256Digest) -> String {
    format!("Global\\Argentum.Store.{}", digest.0)
}

#[cfg(windows)]
fn acquire_named_mutex(
    digest: &argentum_domain::Sha256Digest,
) -> Result<WindowsMutexGuard, StoreError> {
    use std::os::windows::ffi::OsStrExt;

    let name = mutex_name(digest);
    {
        let mut registry = mutex_registry()
            .lock()
            .map_err(|_| StoreError::LockPoisoned)?;
        if !registry.insert(name.clone()) {
            return Err(StoreError::HostOwnershipLive);
        }
    }
    let wide = std::ffi::OsStr::new(&name)
        .encode_wide()
        .chain(Some(0))
        .collect::<Vec<_>>();
    let (result_sender, result_receiver) = std::sync::mpsc::sync_channel(1);
    let (release_sender, release_receiver) = std::sync::mpsc::channel();
    let owner_thread = match std::thread::Builder::new()
        .name("argentum-store-owner".into())
        .spawn(move || {
            use windows_sys::Win32::Foundation::{
                CloseHandle, GetLastError, ERROR_ALREADY_EXISTS, WAIT_ABANDONED, WAIT_OBJECT_0,
                WAIT_TIMEOUT,
            };
            use windows_sys::Win32::System::Threading::{
                CreateMutexW, ReleaseMutex, WaitForSingleObject,
            };

            let handle = unsafe { CreateMutexW(std::ptr::null(), 1, wide.as_ptr()) };
            if handle.is_null() {
                let _ = result_sender.send(MutexAcquireResult::Unknown);
                return;
            }
            let existed = unsafe { GetLastError() } == ERROR_ALREADY_EXISTS;
            let result = if !existed {
                MutexAcquireResult::Acquired
            } else {
                match unsafe { WaitForSingleObject(handle, 0) } {
                    WAIT_OBJECT_0 | WAIT_ABANDONED => MutexAcquireResult::Acquired,
                    WAIT_TIMEOUT => MutexAcquireResult::Live,
                    _ => MutexAcquireResult::Unknown,
                }
            };
            if !matches!(result, MutexAcquireResult::Acquired) {
                unsafe {
                    CloseHandle(handle);
                }
                let _ = result_sender.send(result);
                return;
            }
            if result_sender.send(MutexAcquireResult::Acquired).is_err() {
                unsafe {
                    ReleaseMutex(handle);
                    CloseHandle(handle);
                }
                return;
            }
            let _ = release_receiver.recv();
            unsafe {
                ReleaseMutex(handle);
                CloseHandle(handle);
            }
        }) {
        Ok(thread) => thread,
        Err(_) => {
            mutex_registry()
                .lock()
                .map_err(|_| StoreError::LockPoisoned)?
                .remove(&name);
            return Err(StoreError::HostOwnershipUnknown(
                "named_mutex_owner_thread_failed",
            ));
        }
    };
    let outcome = result_receiver
        .recv()
        .unwrap_or(MutexAcquireResult::Unknown);
    if !matches!(outcome, MutexAcquireResult::Acquired) {
        let _ = owner_thread.join();
        mutex_registry()
            .lock()
            .map_err(|_| StoreError::LockPoisoned)?
            .remove(&name);
        return match outcome {
            MutexAcquireResult::Live => Err(StoreError::HostOwnershipLive),
            MutexAcquireResult::Unknown => {
                Err(StoreError::HostOwnershipUnknown("named_mutex_probe_failed"))
            }
            MutexAcquireResult::Acquired => unreachable!(),
        };
    }
    Ok(WindowsMutexGuard {
        name,
        release_sender: Mutex::new(Some(release_sender)),
        owner_thread: Mutex::new(Some(owner_thread)),
        released: AtomicBool::new(false),
    })
}
