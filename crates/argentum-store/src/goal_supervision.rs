use std::collections::BTreeSet;

use argentum_domain::goal_supervision::{
    CriterionEvaluation, GoalAcceptanceDefinition, GoalCompletionReceipt, GoalCriterion,
    GoalInvocation, GoalInvocationAttempt, GoalInvocationState, GoalOwnerAttestation,
    InvocationAttemptOutcome, ProgressCheckpoint, SupervisionRequest, VerificationInputSnapshot,
    MAX_ACCEPTANCE_CRITERIA, MAX_INVOCATION_ITERATIONS, MAX_INVOCATION_SECONDS,
    MAX_PROGRESS_CHECKPOINTS, SUPERVISION_VERSION,
};
use argentum_domain::{
    now, Goal, GoalLifecycle, ProjectId, SessionId, VerificationEvidence, VerificationState,
};
use rusqlite::{OptionalExtension, Transaction, TransactionBehavior};
use time::Duration;
use uuid::Uuid;

use super::goal_accounting::{eligible, goal_by_id, write_goal};
use super::*;

const MAX_ATTESTATION_BYTES: usize = 4 * 1024;

fn rejected(reason: &'static str) -> StoreError {
    StoreError::GoalAccounting(reason)
}

fn state_name(state: GoalInvocationState) -> &'static str {
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

fn valid_digest(value: &argentum_domain::Sha256Digest) -> bool {
    value.0.len() == 64
        && value
            .0
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
}

fn validate_snapshot(snapshot: &VerificationInputSnapshot) -> Result<(), StoreError> {
    if snapshot.method != "workspace_revision_v1"
        || !valid_digest(&snapshot.digest)
        || snapshot.file_count > 16_384
        || snapshot.total_bytes > 64 * 1024 * 1024
    {
        return Err(rejected("invalid_verification_input_snapshot"));
    }
    Ok(())
}

fn validate_criteria(criteria: &[GoalCriterion]) -> Result<(), StoreError> {
    if criteria.is_empty() || criteria.len() > MAX_ACCEPTANCE_CRITERIA {
        return Err(rejected("invalid_acceptance_criteria"));
    }
    let mut ids = BTreeSet::new();
    let mut runners = BTreeSet::new();
    for criterion in criteria {
        if criterion.id.is_nil()
            || criterion.registry_version == 0
            || !valid_digest(&criterion.definition_sha256)
            || criterion.input_scope.method != "workspace_revision_v1"
            || criterion.input_scope.excluded_components != [".git", "target"]
            || criterion.input_scope.max_files != 16_384
            || criterion.input_scope.max_total_bytes != 64 * 1024 * 1024
            || criterion.input_scope.max_file_bytes != 16 * 1024 * 1024
            || !ids.insert(criterion.id)
            || !runners.insert(criterion.runner.clone())
        {
            return Err(rejected("invalid_acceptance_criteria"));
        }
    }
    Ok(())
}

fn validate_evaluations(
    criteria: &[GoalCriterion],
    evaluations: &[CriterionEvaluation],
) -> Result<(), StoreError> {
    if criteria.len() != evaluations.len() {
        return Err(rejected("incomplete_criterion_evaluation"));
    }
    for criterion in criteria {
        let matches = evaluations
            .iter()
            .filter(|evaluation| {
                evaluation.criterion_id == criterion.id && evaluation.runner == criterion.runner
            })
            .count();
        if matches != 1 {
            return Err(rejected("incomplete_criterion_evaluation"));
        }
    }
    if evaluations
        .iter()
        .any(|evaluation| evaluation.detail.len() > 512)
    {
        return Err(rejected("invalid_criterion_evaluation"));
    }
    Ok(())
}

fn validate_evidence_ids(
    evidence_ids: &[Uuid],
    evaluations: &[CriterionEvaluation],
) -> Result<(), StoreError> {
    let expected = evaluations
        .iter()
        .filter_map(|evaluation| evaluation.evidence_id)
        .collect::<Vec<_>>();
    let unique = expected.iter().copied().collect::<BTreeSet<_>>();
    if unique.len() != expected.len() || evidence_ids != expected {
        return Err(rejected("criterion_evidence_reference_mismatch"));
    }
    Ok(())
}

fn read_invocation(tx: &Transaction<'_>, id: Uuid) -> Result<GoalInvocation, StoreError> {
    let payload: String = tx.query_row(
        "SELECT payload FROM goal_invocations WHERE id = ?1",
        [id.to_string()],
        |row| row.get(0),
    )?;
    let invocation: GoalInvocation = serde_json::from_str(&payload)?;
    if invocation.id != id || invocation.version != SUPERVISION_VERSION {
        return Err(rejected("goal_invocation_scope_mismatch"));
    }
    Ok(invocation)
}

fn write_invocation(tx: &Transaction<'_>, invocation: &GoalInvocation) -> Result<(), StoreError> {
    if tx.execute(
        "UPDATE goal_invocations
         SET state = ?2, payload = ?3, owner_generation = ?4
         WHERE id = ?1",
        params![
            invocation.id.to_string(),
            state_name(invocation.state),
            serde_json::to_string(invocation)?,
            invocation.owner_generation,
        ],
    )? != 1
    {
        return Err(rejected("goal_invocation_scope_mismatch"));
    }
    Ok(())
}

fn validate_claim(invocation: &GoalInvocation, owner_claim_id: Uuid) -> Result<(), StoreError> {
    if owner_claim_id.is_nil() || invocation.owner_claim_id != owner_claim_id {
        return Err(rejected("goal_invocation_owner_mismatch"));
    }
    if invocation.state != GoalInvocationState::Active || invocation.owner_claim_retired {
        return Err(rejected("goal_invocation_settled"));
    }
    Ok(())
}

fn validate_owner_generation(
    invocation: &GoalInvocation,
    current_generation: Option<u64>,
) -> Result<(), StoreError> {
    if invocation.owner_generation == current_generation {
        Ok(())
    } else {
        Err(rejected("goal_invocation_owner_generation_changed"))
    }
}

fn validate_goal_binding(goal: &Goal, invocation: &GoalInvocation) -> Result<(), StoreError> {
    if goal.id != invocation.goal_id
        || goal.project_id != invocation.project_id
        || goal.session_id != invocation.session_id
        || goal.definition_revision != invocation.definition_revision
    {
        return Err(rejected("goal_definition_changed"));
    }
    let acceptance = goal
        .acceptance
        .as_ref()
        .ok_or_else(|| rejected("acceptance_criteria_missing"))?;
    validate_criteria(&acceptance.criteria)?;
    if acceptance.version != invocation.criteria_version {
        return Err(rejected("goal_definition_changed"));
    }
    if goal.accounting.as_ref().map(|account| account.revision)
        != Some(invocation.accounting_revision)
    {
        return Err(rejected("goal_accounting_revision_changed"));
    }
    Ok(())
}

impl Store {
    pub fn validate_goal_invocation_claim(
        &self,
        invocation_id: Uuid,
        owner_claim_id: Uuid,
        workspace_key: &str,
    ) -> Result<(Goal, GoalInvocation), StoreError> {
        let current_generation = self.current_host_generation()?;
        let mut connection = self
            .connection
            .lock()
            .map_err(|_| StoreError::LockPoisoned)?;
        let tx = connection.transaction_with_behavior(TransactionBehavior::Deferred)?;
        let invocation = read_invocation(&tx, invocation_id)?;
        validate_owner_generation(&invocation, current_generation)?;
        validate_claim(&invocation, owner_claim_id)?;
        if invocation.workspace_key != workspace_key {
            return Err(rejected("goal_invocation_workspace_changed"));
        }
        let goal = goal_by_id(&tx, invocation.goal_id)?;
        validate_goal_binding(&goal, &invocation)?;
        tx.commit()?;
        Ok((goal, invocation))
    }

    pub fn configure_goal_acceptance(
        &self,
        project_id: ProjectId,
        session_id: SessionId,
        goal_id: Uuid,
        criteria: Vec<GoalCriterion>,
        owner_attestation_required: bool,
    ) -> Result<Goal, StoreError> {
        validate_criteria(&criteria)?;
        let mut connection = self
            .connection
            .lock()
            .map_err(|_| StoreError::LockPoisoned)?;
        let tx = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let mut goal = goal_by_id(&tx, goal_id)?;
        if goal.project_id != project_id || goal.session_id != session_id {
            return Err(rejected("goal_scope_mismatch"));
        }
        let active_claim: bool = tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM goal_invocations WHERE goal_id = ?1 AND state = 'active')",
            [goal_id.to_string()],
            |row| row.get(0),
        )?;
        if active_claim {
            return Err(rejected("goal_invocation_active"));
        }
        let version = goal.acceptance.as_ref().map_or(Ok(1), |definition| {
            definition
                .version
                .checked_add(1)
                .ok_or_else(|| rejected("goal_definition_overflow"))
        })?;
        goal.definition_revision = Uuid::new_v4();
        goal.acceptance = Some(GoalAcceptanceDefinition {
            version,
            criteria,
            owner_attestation_required,
            configured_at: now(),
        });
        goal.owner_attestation = None;
        goal.completion = None;
        if goal.lifecycle == GoalLifecycle::Complete {
            goal.lifecycle = GoalLifecycle::Active;
        }
        write_goal(&tx, &mut goal)?;
        tx.commit()?;
        Ok(goal)
    }

    pub fn attest_goal_acceptance(
        &self,
        project_id: ProjectId,
        session_id: SessionId,
        goal_id: Uuid,
        summary: &str,
    ) -> Result<Goal, StoreError> {
        let summary = summary.trim();
        if summary.is_empty()
            || summary.len() > MAX_ATTESTATION_BYTES
            || summary.chars().any(char::is_control)
        {
            return Err(rejected("invalid_owner_attestation"));
        }
        let mut connection = self
            .connection
            .lock()
            .map_err(|_| StoreError::LockPoisoned)?;
        let tx = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let mut goal = goal_by_id(&tx, goal_id)?;
        if goal.project_id != project_id || goal.session_id != session_id {
            return Err(rejected("goal_scope_mismatch"));
        }
        let acceptance = goal
            .acceptance
            .as_ref()
            .ok_or_else(|| rejected("acceptance_criteria_missing"))?;
        goal.owner_attestation = Some(GoalOwnerAttestation {
            id: Uuid::new_v4(),
            definition_revision: goal.definition_revision,
            criteria_version: acceptance.version,
            summary: summary.into(),
            recorded_at: now(),
        });
        write_goal(&tx, &mut goal)?;
        tx.commit()?;
        Ok(goal)
    }

    pub fn start_goal_invocation(
        &self,
        project_id: ProjectId,
        session_id: SessionId,
        workspace_key: &str,
        owner_claim_id: Uuid,
        request: &SupervisionRequest,
    ) -> Result<GoalInvocation, StoreError> {
        self.start_goal_invocation_mode(
            project_id,
            session_id,
            workspace_key,
            owner_claim_id,
            request,
            None,
        )
    }

    pub fn start_goal_invocation_mode(
        &self,
        project_id: ProjectId,
        session_id: SessionId,
        workspace_key: &str,
        owner_claim_id: Uuid,
        request: &SupervisionRequest,
        native_configuration: Option<Sha256Digest>,
    ) -> Result<GoalInvocation, StoreError> {
        let owner_generation = self.current_host_generation()?;
        self.ensure_session_recovery_admissible(project_id, session_id)?;
        let goal_id = request.goal_id;
        let iteration_limit = request.iteration_limit;
        let elapsed_seconds = request.elapsed_seconds;
        if workspace_key.is_empty()
            || native_configuration
                .as_ref()
                .is_some_and(|digest| !valid_digest(digest))
            || workspace_key.len() > 32 * 1024
            || owner_claim_id.is_nil()
            || !request.pause_on_session_switch
            || iteration_limit == 0
            || iteration_limit > MAX_INVOCATION_ITERATIONS
            || elapsed_seconds == 0
            || elapsed_seconds > MAX_INVOCATION_SECONDS
        {
            return Err(rejected("invalid_goal_invocation_limits"));
        }
        let mut connection = self
            .connection
            .lock()
            .map_err(|_| StoreError::LockPoisoned)?;
        let tx = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        if let Some(generation) = owner_generation {
            super::recovery::validate_host_generation_on(&tx, generation)?;
        }
        super::recovery::ensure_session_recovery_admissible_on(&tx, project_id, session_id)?;
        let goal = goal_by_id(&tx, goal_id)?;
        if goal.project_id != project_id || goal.session_id != session_id {
            return Err(rejected("goal_scope_mismatch"));
        }
        eligible(&goal)?;
        if native_configuration.is_some() {
            let account = goal
                .accounting
                .as_ref()
                .ok_or_else(|| rejected("legacy_usage_history_unavailable"))?;
            if (!account.owner_removed_aggregate_limits
                && (goal.token_budget.is_none()
                    || goal.tool_budget.is_none()
                    || goal.time_budget_seconds.is_none()
                    || account.request_limit.is_none()
                    || account.iteration_limit.is_none()))
                || account.per_run.token_limit.is_none()
                || account.per_run.request_limit.is_none()
                || account.per_run.tool_limit.is_none()
                || account.per_run.elapsed_seconds.is_none()
            {
                return Err(rejected("native_delivery_finite_allowance_required"));
            }
        }
        if goal.definition_revision.is_nil() {
            return Err(rejected("acceptance_criteria_missing"));
        }
        let acceptance = goal
            .acceptance
            .as_ref()
            .ok_or_else(|| rejected("acceptance_criteria_missing"))?;
        let accounting_revision = goal
            .accounting
            .as_ref()
            .ok_or_else(|| rejected("legacy_usage_history_unavailable"))?
            .revision;
        validate_criteria(&acceptance.criteria)?;
        let active_claim: bool = tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM goal_invocations WHERE goal_id = ?1 AND state = 'active')",
            [goal_id.to_string()],
            |row| row.get(0),
        )?;
        if active_claim {
            return Err(rejected("goal_invocation_active"));
        }
        let unfinished_run: bool = tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM goal_runs WHERE goal_id = ?1 AND json_extract(payload, '$.finished_at') IS NULL)",
            [goal_id.to_string()],
            |row| row.get(0),
        )?;
        if unfinished_run {
            return Err(rejected("goal_unfinished_run"));
        }
        let started_at = now();
        let seconds = i64::try_from(elapsed_seconds)
            .map_err(|_| rejected("invalid_goal_invocation_limits"))?;
        let delivery = if let Some(configuration) = native_configuration {
            let previous: Option<String> = tx.query_row(
                "SELECT payload FROM goal_invocations WHERE project_id=?1 AND session_id=?2 AND goal_id=?3 ORDER BY started_at DESC, rowid DESC LIMIT 1",
                params![project_id.to_string(),session_id.to_string(),goal_id.to_string()], |row| row.get(0),
            ).optional()?;
            let remaining = previous
                .map(|payload| serde_json::from_str::<GoalInvocation>(&payload))
                .transpose()?
                .filter(|old| {
                    old.definition_revision == goal.definition_revision
                        && old.criteria_version == acceptance.version
                        && old.workspace_key == workspace_key
                })
                .and_then(|old| old.delivery)
                .filter(|old| old.configuration_sha256.as_ref() == Some(&configuration))
                .map(|old| old.remaining)
                .unwrap_or_default();
            Some(argentum_domain::goal_supervision::DeliveryProgress {
                configuration_sha256: Some(configuration),
                remaining,
                ..Default::default()
            })
        } else {
            None
        };
        let invocation = GoalInvocation {
            version: SUPERVISION_VERSION,
            id: Uuid::new_v4(),
            goal_id,
            project_id,
            session_id,
            workspace_key: workspace_key.into(),
            owner_claim_id,
            owner_generation,
            owner_claim_retired: false,
            definition_revision: goal.definition_revision,
            criteria_version: acceptance.version,
            accounting_revision,
            iteration_limit,
            elapsed_seconds,
            started_at,
            deadline_at: started_at + Duration::seconds(seconds),
            state: GoalInvocationState::Active,
            attempts: Vec::new(),
            evaluated_criteria: Vec::new(),
            progress: Vec::new(),
            delivery,
            stop_reason: None,
            finished_at: None,
        };
        tx.execute(
            "INSERT INTO goal_invocations (
                 id, goal_id, project_id, session_id, state, started_at,
                 owner_generation, payload
             ) VALUES (?1, ?2, ?3, ?4, 'active', ?5, ?6, ?7)",
            params![
                invocation.id.to_string(),
                goal_id.to_string(),
                project_id.to_string(),
                session_id.to_string(),
                started_at.unix_timestamp(),
                owner_generation,
                serde_json::to_string(&invocation)?,
            ],
        )?;
        tx.commit()?;
        Ok(invocation)
    }

    pub fn begin_goal_invocation_attempt(
        &self,
        invocation_id: Uuid,
        owner_claim_id: Uuid,
        task_id: Uuid,
        run_id: Uuid,
        source_before: VerificationInputSnapshot,
        criteria_before: Vec<CriterionEvaluation>,
    ) -> Result<GoalInvocation, StoreError> {
        validate_snapshot(&source_before)?;
        let current_generation = self.current_host_generation()?;
        let mut connection = self
            .connection
            .lock()
            .map_err(|_| StoreError::LockPoisoned)?;
        let tx = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let mut invocation = read_invocation(&tx, invocation_id)?;
        validate_owner_generation(&invocation, current_generation)?;
        validate_claim(&invocation, owner_claim_id)?;
        let goal = goal_by_id(&tx, invocation.goal_id)?;
        validate_goal_binding(&goal, &invocation)?;
        let acceptance = goal
            .acceptance
            .as_ref()
            .ok_or_else(|| rejected("acceptance_criteria_missing"))?;
        validate_evaluations(&acceptance.criteria, &criteria_before)?;
        if now() >= invocation.deadline_at {
            return Err(rejected("goal_invocation_deadline"));
        }
        if invocation.attempts.len() >= invocation.iteration_limit as usize {
            return Err(rejected("goal_invocation_iteration_limit"));
        }
        if task_id.is_nil()
            || run_id.is_nil()
            || invocation
                .attempts
                .iter()
                .any(|attempt| attempt.task_id == task_id || attempt.run_id == run_id)
        {
            return Err(rejected("goal_invocation_attempt_conflict"));
        }
        let ordinal = u32::try_from(invocation.attempts.len())
            .map_err(|_| rejected("goal_invocation_iteration_limit"))?
            .checked_add(1)
            .ok_or_else(|| rejected("goal_invocation_iteration_limit"))?;
        invocation.attempts.push(GoalInvocationAttempt {
            ordinal,
            task_id,
            run_id,
            source_before,
            criteria_before,
            outcome: InvocationAttemptOutcome::Admitted,
            delivery_phase: invocation.delivery.as_ref().map(|delivery| delivery.phase),
            evidence_ids: Vec::new(),
            started_at: now(),
            finished_at: None,
            stop_reason: None,
        });
        write_invocation(&tx, &invocation)?;
        tx.commit()?;
        Ok(invocation)
    }

    #[allow(clippy::too_many_arguments)]
    pub fn finish_goal_invocation_attempt(
        &self,
        invocation_id: Uuid,
        owner_claim_id: Uuid,
        run_id: Uuid,
        outcome: InvocationAttemptOutcome,
        evidence_ids: Vec<Uuid>,
        evaluations: Vec<CriterionEvaluation>,
        checkpoint: ProgressCheckpoint,
        stop_reason: &str,
    ) -> Result<GoalInvocation, StoreError> {
        if outcome == InvocationAttemptOutcome::Admitted || !valid_stop_reason(stop_reason) {
            return Err(rejected("invalid_goal_invocation_transition"));
        }
        validate_snapshot(&checkpoint.source)?;
        if checkpoint
            .observation_progress_sha256
            .as_ref()
            .is_some_and(|digest| !valid_digest(digest))
        {
            return Err(rejected("invalid_progress_checkpoint"));
        }
        if !valid_digest(&checkpoint.normalized_criteria_sha256) || checkpoint.repeated_count > 32 {
            return Err(rejected("invalid_progress_checkpoint"));
        }
        let current_generation = self.current_host_generation()?;
        let mut connection = self
            .connection
            .lock()
            .map_err(|_| StoreError::LockPoisoned)?;
        let tx = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let mut invocation = read_invocation(&tx, invocation_id)?;
        validate_owner_generation(&invocation, current_generation)?;
        validate_claim(&invocation, owner_claim_id)?;
        let goal = goal_by_id(&tx, invocation.goal_id)?;
        validate_goal_binding(&goal, &invocation)?;
        let acceptance = goal
            .acceptance
            .as_ref()
            .ok_or_else(|| rejected("acceptance_criteria_missing"))?;
        validate_evaluations(&acceptance.criteria, &evaluations)?;
        validate_evidence_ids(&evidence_ids, &evaluations)?;
        let attempt = invocation
            .attempts
            .last_mut()
            .filter(|attempt| attempt.run_id == run_id)
            .ok_or_else(|| rejected("goal_invocation_attempt_conflict"))?;
        if attempt.finished_at.is_some() {
            return Err(rejected("goal_invocation_attempt_conflict"));
        }
        attempt.outcome = outcome;
        attempt.evidence_ids = evidence_ids;
        attempt.finished_at = Some(now());
        attempt.stop_reason = Some(stop_reason.into());
        invocation.evaluated_criteria = evaluations;
        // A planned stage boundary is not another implementation checkpoint.
        // Native verification/correction cycles still use the same stall guard.
        if outcome != InvocationAttemptOutcome::ScheduledHandoff {
            invocation.progress.push(checkpoint);
        }
        if invocation.progress.len() > MAX_PROGRESS_CHECKPOINTS {
            let remove = invocation.progress.len() - MAX_PROGRESS_CHECKPOINTS;
            invocation.progress.drain(0..remove);
        }
        write_invocation(&tx, &invocation)?;
        tx.commit()?;
        Ok(invocation)
    }

    pub fn update_goal_delivery_progress(
        &self,
        invocation_id: Uuid,
        owner_claim_id: Uuid,
        expected_phase: argentum_domain::goal_supervision::DeliveryPhase,
        delivery: argentum_domain::goal_supervision::DeliveryProgress,
    ) -> Result<GoalInvocation, StoreError> {
        if let Some(source) = &delivery.reported_source {
            validate_snapshot(source)?;
        }
        let generation = self.current_host_generation()?;
        let mut connection = self
            .connection
            .lock()
            .map_err(|_| StoreError::LockPoisoned)?;
        let tx = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let mut invocation = read_invocation(&tx, invocation_id)?;
        validate_owner_generation(&invocation, generation)?;
        validate_claim(&invocation, owner_claim_id)?;
        let goal = goal_by_id(&tx, invocation.goal_id)?;
        validate_goal_binding(&goal, &invocation)?;
        let old = invocation
            .delivery
            .as_ref()
            .ok_or_else(|| rejected("delivery_mode_missing"))?;
        if old.phase != expected_phase {
            return Err(rejected("delivery_stage_changed"));
        }
        use argentum_domain::goal_supervision::DeliveryPhase;
        let transition_valid = match expected_phase {
            DeliveryPhase::Implementation => {
                matches!(
                    delivery.phase,
                    DeliveryPhase::Implementation | DeliveryPhase::Verification
                ) && !delivery.feedback_acknowledged
            }
            DeliveryPhase::Feedback => {
                matches!(
                    delivery.phase,
                    DeliveryPhase::Feedback
                        | DeliveryPhase::Verification
                        | DeliveryPhase::Implementation
                ) && (!delivery.feedback_acknowledged
                    || (delivery.phase == DeliveryPhase::Feedback && delivery.implementation_ready))
            }
            DeliveryPhase::Verification => {
                (match delivery.phase {
                    DeliveryPhase::Feedback => {
                        old.implementation_ready
                            && delivery.implementation_ready
                            && delivery.reported_source == old.reported_source
                    }
                    DeliveryPhase::Implementation => !delivery.implementation_ready,
                    DeliveryPhase::Verification => false,
                }) && !delivery.feedback_acknowledged
            }
        };
        if !transition_valid
            || delivery.configuration_sha256 != old.configuration_sha256
            || delivery.remaining.len() > 8
            || delivery
                .remaining
                .iter()
                .any(|item| item.trim().is_empty() || item.len() > 1024)
            || (delivery.implementation_ready
                && (!delivery.remaining.is_empty() || delivery.reported_source.is_none()))
        {
            return Err(rejected("delivery_progress_invalid"));
        }
        invocation.delivery = Some(delivery);
        write_invocation(&tx, &invocation)?;
        tx.commit()?;
        Ok(invocation)
    }

    pub fn settle_goal_invocation(
        &self,
        invocation_id: Uuid,
        owner_claim_id: Uuid,
        state: GoalInvocationState,
        stop_reason: &str,
    ) -> Result<GoalInvocation, StoreError> {
        if !state.is_terminal() || !valid_stop_reason(stop_reason) {
            return Err(rejected("invalid_goal_invocation_transition"));
        }
        let current_generation = self.current_host_generation()?;
        let mut connection = self
            .connection
            .lock()
            .map_err(|_| StoreError::LockPoisoned)?;
        let tx = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let mut invocation = read_invocation(&tx, invocation_id)?;
        if invocation.state.is_terminal() {
            if invocation.state == state && invocation.stop_reason.as_deref() == Some(stop_reason) {
                return Ok(invocation);
            }
            return Err(rejected("goal_invocation_settled"));
        }
        validate_owner_generation(&invocation, current_generation)?;
        validate_claim(&invocation, owner_claim_id)?;
        invocation.state = state;
        invocation.owner_claim_retired = true;
        invocation.stop_reason = Some(stop_reason.into());
        invocation.finished_at = Some(now());
        write_invocation(&tx, &invocation)?;
        tx.commit()?;
        Ok(invocation)
    }

    pub fn settle_goal_invocation_after_attempt_error(
        &self,
        invocation_id: Uuid,
        owner_claim_id: Uuid,
        run_id: Uuid,
        outcome: InvocationAttemptOutcome,
        state: GoalInvocationState,
        stop_reason: &str,
    ) -> Result<GoalInvocation, StoreError> {
        if outcome == InvocationAttemptOutcome::Admitted
            || !state.is_terminal()
            || !valid_stop_reason(stop_reason)
        {
            return Err(rejected("invalid_goal_invocation_transition"));
        }
        let current_generation = self.current_host_generation()?;
        let mut connection = self
            .connection
            .lock()
            .map_err(|_| StoreError::LockPoisoned)?;
        let tx = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let mut invocation = read_invocation(&tx, invocation_id)?;
        validate_owner_generation(&invocation, current_generation)?;
        validate_claim(&invocation, owner_claim_id)?;
        let attempt = invocation
            .attempts
            .last_mut()
            .filter(|attempt| attempt.run_id == run_id && attempt.finished_at.is_none())
            .ok_or_else(|| rejected("goal_invocation_attempt_conflict"))?;
        attempt.outcome = outcome;
        attempt.finished_at = Some(now());
        attempt.stop_reason = Some(stop_reason.into());
        invocation.state = state;
        invocation.owner_claim_retired = true;
        invocation.stop_reason = Some(stop_reason.into());
        invocation.finished_at = Some(now());
        write_invocation(&tx, &invocation)?;
        tx.commit()?;
        Ok(invocation)
    }

    #[allow(clippy::too_many_arguments)]
    pub fn complete_goal_invocation(
        &self,
        invocation_id: Uuid,
        owner_claim_id: Uuid,
        run_id: Option<Uuid>,
        source: VerificationInputSnapshot,
        evaluations: Vec<CriterionEvaluation>,
        checkpoint: ProgressCheckpoint,
    ) -> Result<(Goal, GoalInvocation), StoreError> {
        validate_snapshot(&source)?;
        validate_snapshot(&checkpoint.source)?;
        if source != checkpoint.source {
            return Err(rejected("completion_source_changed"));
        }
        let current_generation = self.current_host_generation()?;
        let mut connection = self
            .connection
            .lock()
            .map_err(|_| StoreError::LockPoisoned)?;
        let tx = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let mut invocation = read_invocation(&tx, invocation_id)?;
        validate_owner_generation(&invocation, current_generation)?;
        validate_claim(&invocation, owner_claim_id)?;
        let mut goal = goal_by_id(&tx, invocation.goal_id)?;
        validate_goal_binding(&goal, &invocation)?;
        if goal.lifecycle != GoalLifecycle::Active {
            return Err(rejected("goal_not_active"));
        }
        if now() >= invocation.deadline_at {
            return Err(rejected("goal_invocation_deadline"));
        }
        let acceptance = goal
            .acceptance
            .as_ref()
            .ok_or_else(|| rejected("acceptance_criteria_missing"))?;
        validate_evaluations(&acceptance.criteria, &evaluations)?;
        if evaluations.iter().any(|evaluation| {
            evaluation.state != argentum_domain::goal_supervision::CriterionEvidenceState::Passed
        }) {
            return Err(rejected("completion_criteria_not_passed"));
        }
        if invocation.delivery.as_ref().is_some_and(|delivery| {
            !delivery.implementation_ready
                || !delivery.feedback_acknowledged
                || !delivery.remaining.is_empty()
                || delivery.reported_source.as_ref() != Some(&source)
        }) {
            return Err(rejected("delivery_required_work_unresolved"));
        }
        if invocation.delivery.is_some() {
            eligible(&goal)?;
            let unfinished: bool = tx.query_row("SELECT EXISTS(SELECT 1 FROM goal_runs WHERE goal_id=?1 AND json_extract(payload,'$.finished_at') IS NULL)", [goal.id.to_string()], |row| row.get(0))?;
            if unfinished {
                return Err(rejected("completion_accounting_unsettled"));
            }
        }
        let mut evidence_ids = Vec::with_capacity(acceptance.criteria.len());
        for criterion in &acceptance.criteria {
            let evaluation = evaluations
                .iter()
                .find(|evaluation| evaluation.criterion_id == criterion.id)
                .ok_or_else(|| rejected("incomplete_criterion_evaluation"))?;
            let evidence_id = evaluation
                .evidence_id
                .ok_or_else(|| rejected("completion_evidence_missing"))?;
            let payload: String = tx
                .query_row(
                    "SELECT payload FROM verification_evidence WHERE id = ?1 AND project_id = ?2 AND session_id = ?3",
                    params![evidence_id.to_string(), invocation.project_id.to_string(), invocation.session_id.to_string()],
                    |row| row.get(0),
                )
                .optional()?
                .ok_or_else(|| rejected("completion_evidence_missing"))?;
            let evidence: VerificationEvidence = serde_json::from_str(&payload)?;
            if evidence.id != evidence_id
                || evidence.runner != criterion.runner
                || evidence.registry_version != criterion.registry_version
                || evidence.state != VerificationState::Passed
                || evidence.workspace_key != invocation.workspace_key
                || evidence.definition_sha256.as_ref() != Some(&criterion.definition_sha256)
                || evidence.input_scope.as_ref() != Some(&criterion.input_scope)
                || evidence.input_before.as_ref() != Some(&source)
                || evidence.input_after.as_ref() != Some(&source)
            {
                return Err(rejected("completion_evidence_incompatible"));
            }
            evidence_ids.push(evidence_id);
        }
        let owner_attestation_id = if acceptance.owner_attestation_required {
            let attestation = goal
                .owner_attestation
                .as_ref()
                .filter(|attestation| {
                    attestation.definition_revision == goal.definition_revision
                        && attestation.criteria_version == acceptance.version
                })
                .ok_or_else(|| rejected("owner_attestation_required"))?;
            Some(attestation.id)
        } else {
            None
        };
        if let Some(run_id) = run_id {
            let attempt = invocation
                .attempts
                .last_mut()
                .filter(|attempt| attempt.run_id == run_id && attempt.finished_at.is_none())
                .ok_or_else(|| rejected("goal_invocation_attempt_conflict"))?;
            attempt.outcome = InvocationAttemptOutcome::Verified;
            attempt.evidence_ids = evidence_ids.clone();
            attempt.finished_at = Some(now());
            attempt.stop_reason = Some("verified_against_declared_criteria".into());
        } else if invocation
            .attempts
            .last()
            .is_some_and(|attempt| attempt.finished_at.is_none())
        {
            return Err(rejected("goal_invocation_attempt_conflict"));
        }
        invocation.evaluated_criteria = evaluations;
        invocation.progress.push(checkpoint);
        if invocation.progress.len() > MAX_PROGRESS_CHECKPOINTS {
            let remove = invocation.progress.len() - MAX_PROGRESS_CHECKPOINTS;
            invocation.progress.drain(0..remove);
        }
        invocation.state = GoalInvocationState::Verified;
        invocation.owner_claim_retired = true;
        invocation.stop_reason = Some("verified_against_declared_criteria".into());
        invocation.finished_at = Some(now());
        goal.lifecycle = GoalLifecycle::Complete;
        goal.next_action = "Verified against the declared criteria".into();
        goal.completion = Some(GoalCompletionReceipt {
            invocation_id,
            definition_revision: goal.definition_revision,
            criteria_version: acceptance.version,
            source,
            evidence_ids,
            owner_attestation_id,
            statement: "verified against the declared criteria".into(),
            recorded_at: now(),
        });
        write_goal(&tx, &mut goal)?;
        write_invocation(&tx, &invocation)?;
        tx.commit()?;
        Ok((goal, invocation))
    }

    pub fn goal_invocation(&self, id: Uuid) -> Result<Option<GoalInvocation>, StoreError> {
        let mut connection = self
            .connection
            .lock()
            .map_err(|_| StoreError::LockPoisoned)?;
        let tx = connection.transaction_with_behavior(TransactionBehavior::Deferred)?;
        let exists = tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM goal_invocations WHERE id = ?1)",
            [id.to_string()],
            |row| row.get::<_, bool>(0),
        )?;
        let invocation = exists.then(|| read_invocation(&tx, id)).transpose()?;
        tx.commit()?;
        Ok(invocation)
    }

    pub fn latest_goal_invocation(
        &self,
        project_id: ProjectId,
        session_id: SessionId,
        goal_id: Uuid,
    ) -> Result<Option<GoalInvocation>, StoreError> {
        let connection = self
            .connection
            .lock()
            .map_err(|_| StoreError::LockPoisoned)?;
        let payload = connection
            .query_row(
                "SELECT payload FROM goal_invocations WHERE project_id = ?1 AND session_id = ?2 AND goal_id = ?3 ORDER BY started_at DESC, rowid DESC LIMIT 1",
                params![project_id.to_string(), session_id.to_string(), goal_id.to_string()],
                |row| row.get::<_, String>(0),
            )
            .optional()?;
        payload
            .map(|payload| serde_json::from_str(&payload).map_err(StoreError::from))
            .transpose()
    }
}

fn valid_stop_reason(reason: &str) -> bool {
    !reason.is_empty()
        && reason.len() <= 128
        && reason
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte == b'_')
}

#[cfg(test)]
mod tests {
    use argentum_domain::goal_accounting::GoalAccounting;
    use argentum_domain::goal_supervision::{CriterionEvidenceState, VerificationInputScope};
    use argentum_domain::{Sha256Digest, VerificationRunnerId};

    use super::*;

    fn criterion(id: Uuid) -> GoalCriterion {
        GoalCriterion {
            id,
            runner: VerificationRunnerId::CargoFmtCheck,
            registry_version: 1,
            definition_sha256: Sha256Digest("a".repeat(64)),
            input_scope: VerificationInputScope {
                method: "workspace_revision_v1".into(),
                excluded_components: vec![".git".into(), "target".into()],
                max_files: 16_384,
                max_total_bytes: 64 * 1024 * 1024,
                max_file_bytes: 16 * 1024 * 1024,
            },
        }
    }

    fn snapshot(byte: char) -> VerificationInputSnapshot {
        VerificationInputSnapshot {
            method: "workspace_revision_v1".into(),
            digest: Sha256Digest(byte.to_string().repeat(64)),
            file_count: 2,
            total_bytes: 128,
        }
    }

    fn fixture(store: &Store, root: &std::path::Path) -> (ProjectId, SessionId, Goal) {
        std::fs::create_dir_all(root).expect("workspace");
        let resolution = store.resolve_workspace(root).expect("workspace resolution");
        let project_id = resolution.snapshot.project.id;
        let session_id = resolution.snapshot.active_session_id.expect("session");
        let timestamp = now();
        let goal = Goal {
            id: Uuid::new_v4(),
            project_id,
            session_id,
            objective: "Verify the declared formatting criterion".into(),
            lifecycle: GoalLifecycle::Active,
            token_budget: Some(100_000),
            tool_budget: Some(20),
            time_budget_seconds: Some(600),
            tokens_used: 0,
            tools_used: 0,
            iteration: 0,
            next_action: "Configure acceptance".into(),
            verification_history: Vec::new(),
            created_at: timestamp,
            updated_at: timestamp,
            accounting: Some(GoalAccounting::new(false)),
            definition_revision: Uuid::new_v4(),
            acceptance: None,
            owner_attestation: None,
            completion: None,
        };
        store.save_goal(&goal).expect("goal");
        (project_id, session_id, goal)
    }

    fn supervision_request(
        goal_id: Uuid,
        iteration_limit: u32,
        elapsed_seconds: u64,
    ) -> SupervisionRequest {
        SupervisionRequest {
            goal_id,
            iteration_limit,
            elapsed_seconds,
            pause_on_session_switch: true,
        }
    }

    #[test]
    fn acceptance_is_nonempty_bounded_and_competing_claim_persists_across_restart() {
        let directory = tempfile::tempdir().expect("fixture root");
        let database = directory.path().join("argentum.db");
        let workspace = directory.path().join("workspace");
        let store = Store::open(&database).expect("store");
        let (project_id, session_id, goal) = fixture(&store, &workspace);

        assert!(matches!(
            store.configure_goal_acceptance(project_id, session_id, goal.id, Vec::new(), false),
            Err(StoreError::GoalAccounting("invalid_acceptance_criteria"))
        ));
        let duplicate = criterion(Uuid::new_v4());
        assert!(matches!(
            store.configure_goal_acceptance(
                project_id,
                session_id,
                goal.id,
                vec![duplicate.clone(), duplicate],
                false,
            ),
            Err(StoreError::GoalAccounting("invalid_acceptance_criteria"))
        ));
        let configured = store
            .configure_goal_acceptance(
                project_id,
                session_id,
                goal.id,
                vec![criterion(Uuid::new_v4())],
                false,
            )
            .expect("acceptance");
        assert_ne!(configured.definition_revision, goal.definition_revision);
        let claim = Uuid::new_v4();
        let invocation = store
            .start_goal_invocation(
                project_id,
                session_id,
                "workspace-key",
                claim,
                &supervision_request(goal.id, 2, 60),
            )
            .expect("invocation");
        assert!(matches!(
            store.start_goal_invocation(
                project_id,
                session_id,
                "workspace-key",
                Uuid::new_v4(),
                &supervision_request(goal.id, 2, 60),
            ),
            Err(StoreError::HostOwnershipUnknown(
                "legacy_session_work_has_no_owner_evidence"
            ))
        ));
        drop(store);

        let reopened = Store::open(&database).expect("reopened store");
        let stored = reopened
            .latest_goal_invocation(project_id, session_id, goal.id)
            .expect("stored invocation")
            .expect("invocation");
        assert_eq!(stored.id, invocation.id);
        assert_eq!(stored.state, GoalInvocationState::Active);
        assert!(reopened
            .start_goal_invocation(
                project_id,
                session_id,
                "workspace-key",
                Uuid::new_v4(),
                &supervision_request(goal.id, 1, 60),
            )
            .is_err());
    }

    #[test]
    fn native_delivery_stages_are_bound_and_reload_without_completion() {
        use argentum_domain::goal_supervision::{DeliveryPhase, DeliveryProgress};
        let directory = tempfile::tempdir().unwrap();
        let database = directory.path().join("native.db");
        let store = Store::open(&database).unwrap();
        let (project, session, goal) = fixture(&store, &directory.path().join("workspace"));
        store
            .configure_goal_acceptance(
                project,
                session,
                goal.id,
                vec![criterion(Uuid::new_v4())],
                false,
            )
            .unwrap();
        let claim = Uuid::new_v4();
        let digest = Sha256Digest("d".repeat(64));
        assert!(matches!(
            store.start_goal_invocation_mode(
                project,
                session,
                "workspace-key",
                claim,
                &supervision_request(goal.id, 8, 60),
                Some(digest.clone())
            ),
            Err(StoreError::GoalAccounting(
                "native_delivery_finite_allowance_required"
            ))
        ));
        store
            .configure_goal_limits(
                project,
                session,
                goal.id,
                [100_000, 32, 8, 32, 600, 100_000, 32, 16, 300],
            )
            .unwrap();
        let removed = store
            .remove_goal_aggregate_limits(project, session, goal.id)
            .unwrap();
        assert!(
            removed
                .accounting
                .as_ref()
                .unwrap()
                .owner_removed_aggregate_limits
        );
        // An old writer loses the explicit opt-in: absence is not permission.
        let mut old_writer = removed.clone();
        old_writer
            .accounting
            .as_mut()
            .unwrap()
            .owner_removed_aggregate_limits = false;
        store.save_goal(&old_writer).unwrap();
        // Current configuration callers cannot rewrite accounting or the marker.
        assert!(
            store
                .goal(project, session)
                .unwrap()
                .unwrap()
                .accounting
                .unwrap()
                .owner_removed_aggregate_limits
        );
        // Disposable persistence control: simulate an older writer that drops
        // the new field, without modifying any owner store or authority record.
        let mut payload = serde_json::to_value(&removed).unwrap();
        payload["accounting"]
            .as_object_mut()
            .unwrap()
            .remove("owner_removed_aggregate_limits");
        store
            .connection
            .lock()
            .unwrap()
            .execute(
                "UPDATE goals SET payload=?2 WHERE id=?1",
                params![
                    goal.id.to_string(),
                    serde_json::to_string(&payload).unwrap()
                ],
            )
            .unwrap();
        assert!(matches!(
            store.start_goal_invocation_mode(
                project,
                session,
                "workspace-key",
                claim,
                &supervision_request(goal.id, 8, 60),
                Some(digest.clone())
            ),
            Err(StoreError::GoalAccounting(
                "native_delivery_finite_allowance_required"
            ))
        ));
        store
            .connection
            .lock()
            .unwrap()
            .execute(
                "UPDATE goals SET payload=?2 WHERE id=?1",
                params![
                    goal.id.to_string(),
                    serde_json::to_string(&removed).unwrap()
                ],
            )
            .unwrap();
        let invocation = store
            .start_goal_invocation_mode(
                project,
                session,
                "workspace-key",
                claim,
                &supervision_request(goal.id, 8, 60),
                Some(digest.clone()),
            )
            .unwrap();
        let mut progress = DeliveryProgress {
            configuration_sha256: Some(digest.clone()),
            phase: DeliveryPhase::Feedback,
            implementation_ready: true,
            reported_source: Some(snapshot('a')),
            ..Default::default()
        };
        assert!(store
            .update_goal_delivery_progress(
                invocation.id,
                claim,
                DeliveryPhase::Implementation,
                progress.clone()
            )
            .is_err());
        progress.phase = DeliveryPhase::Verification;
        progress.remaining = vec!["Required behavior is unfinished".into()];
        assert!(store
            .update_goal_delivery_progress(
                invocation.id,
                claim,
                DeliveryPhase::Implementation,
                progress.clone()
            )
            .is_err());
        progress.remaining.clear();
        progress.configuration_sha256 = Some(Sha256Digest("e".repeat(64)));
        assert!(store
            .update_goal_delivery_progress(
                invocation.id,
                claim,
                DeliveryPhase::Implementation,
                progress.clone()
            )
            .is_err());
        progress.configuration_sha256 = Some(digest);
        store
            .update_goal_delivery_progress(
                invocation.id,
                claim,
                DeliveryPhase::Implementation,
                progress.clone(),
            )
            .unwrap();
        progress.phase = DeliveryPhase::Feedback;
        progress.feedback_acknowledged = true;
        assert!(store
            .update_goal_delivery_progress(
                invocation.id,
                claim,
                DeliveryPhase::Verification,
                progress.clone()
            )
            .is_err());
        progress.feedback_acknowledged = false;
        store
            .update_goal_delivery_progress(
                invocation.id,
                claim,
                DeliveryPhase::Verification,
                progress.clone(),
            )
            .unwrap();
        drop(store);
        let reopened = Store::open(&database).unwrap();
        let retained = reopened.goal_invocation(invocation.id).unwrap().unwrap();
        assert_eq!(retained.delivery, Some(progress));
        assert_eq!(retained.state, GoalInvocationState::Active);
        assert!(retained.attempts.is_empty());
        assert_eq!(
            reopened.goal(project, session).unwrap().unwrap().lifecycle,
            GoalLifecycle::Active
        );
    }

    #[test]
    fn invocation_start_revalidates_host_generation_inside_its_transaction() {
        let directory = tempfile::tempdir().expect("fixture root");
        let database = directory.path().join("argentum.db");
        let workspace = directory.path().join("workspace");
        let (store, _owner) = Store::open_for_host(&database).expect("owned store");
        let (project_id, session_id, goal) = fixture(&store, &workspace);
        store
            .configure_goal_acceptance(
                project_id,
                session_id,
                goal.id,
                vec![criterion(Uuid::new_v4())],
                false,
            )
            .expect("acceptance");
        let generation = store
            .current_host_generation()
            .expect("host generation")
            .expect("persistent host generation");
        rusqlite::Connection::open(&database)
            .expect("fence connection")
            .execute(
                "UPDATE runtime_host_generations SET state = 'recovered' WHERE generation = ?1",
                [generation],
            )
            .expect("fence host generation");

        assert!(matches!(
            store.start_goal_invocation(
                project_id,
                session_id,
                "workspace-key",
                Uuid::new_v4(),
                &supervision_request(goal.id, 2, 60),
            ),
            Err(StoreError::HostGenerationFenced)
        ));
        let count: usize = rusqlite::Connection::open(&database)
            .expect("invocation count connection")
            .query_row("SELECT COUNT(*) FROM goal_invocations", [], |row| {
                row.get(0)
            })
            .expect("invocation count");
        assert_eq!(count, 0);
    }

    #[test]
    fn attempt_evidence_references_must_match_the_validated_evaluations() {
        let directory = tempfile::tempdir().expect("fixture root");
        let store = Store::open_in_memory().expect("store");
        let (project_id, session_id, goal) = fixture(&store, directory.path());
        let criterion = criterion(Uuid::new_v4());
        store
            .configure_goal_acceptance(
                project_id,
                session_id,
                goal.id,
                vec![criterion.clone()],
                false,
            )
            .expect("acceptance");
        let invocation = store
            .start_goal_invocation(
                project_id,
                session_id,
                "workspace-key",
                Uuid::new_v4(),
                &supervision_request(goal.id, 2, 60),
            )
            .expect("invocation");
        let run_id = Uuid::new_v4();
        let evaluation = CriterionEvaluation {
            criterion_id: criterion.id,
            runner: criterion.runner.clone(),
            state: CriterionEvidenceState::Unknown,
            evidence_id: None,
            detail: "not_checked".into(),
        };
        store
            .begin_goal_invocation_attempt(
                invocation.id,
                invocation.owner_claim_id,
                Uuid::new_v4(),
                run_id,
                snapshot('a'),
                vec![evaluation.clone()],
            )
            .expect("attempt");
        assert!(matches!(
            store.finish_goal_invocation_attempt(
                invocation.id,
                invocation.owner_claim_id,
                run_id,
                InvocationAttemptOutcome::CriteriaFailed,
                vec![Uuid::new_v4()],
                vec![evaluation],
                ProgressCheckpoint {
                    source: snapshot('a'),
                    normalized_criteria_sha256: Sha256Digest("b".repeat(64)),
                    observation_progress_sha256: None,
                    repeated_count: 1,
                    recorded_at: now(),
                },
                "criteria_failed",
            ),
            Err(StoreError::GoalAccounting(
                "criterion_evidence_reference_mismatch"
            ))
        ));
        let stored = store
            .goal_invocation(invocation.id)
            .expect("stored invocation")
            .expect("invocation");
        assert_eq!(
            stored.attempts[0].outcome,
            InvocationAttemptOutcome::Admitted
        );
        assert!(stored.attempts[0].finished_at.is_none());
    }

    #[test]
    fn completion_requires_an_exact_current_passing_typed_receipt() {
        let directory = tempfile::tempdir().expect("fixture root");
        let store = Store::open_in_memory().expect("store");
        let (project_id, session_id, goal) = fixture(&store, directory.path());
        let criterion = criterion(Uuid::new_v4());
        let configured = store
            .configure_goal_acceptance(
                project_id,
                session_id,
                goal.id,
                vec![criterion.clone()],
                false,
            )
            .expect("acceptance");
        let invocation = store
            .start_goal_invocation(
                project_id,
                session_id,
                "workspace-key",
                Uuid::new_v4(),
                &supervision_request(goal.id, 2, 60),
            )
            .expect("invocation");
        let current = snapshot('b');
        let failed = VerificationEvidence {
            id: Uuid::new_v4(),
            run_id: Uuid::new_v4(),
            runner: criterion.runner.clone(),
            registry_version: criterion.registry_version,
            program: "cargo".into(),
            args: vec!["fmt".into()],
            state: VerificationState::Failed,
            started_at: now(),
            duration_ms: 1,
            exit_code: Some(1),
            revision_before: current.digest.clone(),
            revision_after: current.digest.clone(),
            stdout_sha256: Sha256Digest("c".repeat(64)),
            stderr_sha256: Sha256Digest("d".repeat(64)),
            stdout_bytes: 1,
            stderr_bytes: 1,
            stdout_truncated: false,
            stderr_truncated: false,
            workspace_key: "workspace-key".into(),
            definition_sha256: Some(criterion.definition_sha256.clone()),
            input_scope: Some(criterion.input_scope.clone()),
            input_before: Some(current.clone()),
            input_after: Some(current.clone()),
        };
        store
            .save_verification_evidence(project_id, session_id, &failed)
            .expect("failed evidence");
        let failed_evaluation = CriterionEvaluation {
            criterion_id: criterion.id,
            runner: criterion.runner.clone(),
            state: CriterionEvidenceState::Failed,
            evidence_id: Some(failed.id),
            detail: "current_failure".into(),
        };
        let failed_checkpoint = ProgressCheckpoint {
            source: current.clone(),
            normalized_criteria_sha256: Sha256Digest("e".repeat(64)),
            observation_progress_sha256: None,
            repeated_count: 1,
            recorded_at: now(),
        };
        assert!(matches!(
            store.complete_goal_invocation(
                invocation.id,
                invocation.owner_claim_id,
                None,
                current.clone(),
                vec![failed_evaluation],
                failed_checkpoint,
            ),
            Err(StoreError::GoalAccounting("completion_criteria_not_passed"))
        ));

        let mut passed = failed.clone();
        passed.id = Uuid::new_v4();
        passed.run_id = Uuid::new_v4();
        passed.state = VerificationState::Passed;
        passed.exit_code = Some(0);
        passed.started_at = now();
        store
            .save_verification_evidence(project_id, session_id, &passed)
            .expect("passing evidence");
        let passed_evaluation = CriterionEvaluation {
            criterion_id: criterion.id,
            runner: criterion.runner.clone(),
            state: CriterionEvidenceState::Passed,
            evidence_id: Some(passed.id),
            detail: "current_pass".into(),
        };
        let checkpoint = ProgressCheckpoint {
            source: current.clone(),
            normalized_criteria_sha256: Sha256Digest("f".repeat(64)),
            observation_progress_sha256: None,
            repeated_count: 1,
            recorded_at: now(),
        };
        let changed_before_publication = snapshot('9');
        assert!(matches!(
            store.complete_goal_invocation(
                invocation.id,
                invocation.owner_claim_id,
                None,
                changed_before_publication.clone(),
                vec![passed_evaluation.clone()],
                ProgressCheckpoint {
                    source: changed_before_publication,
                    normalized_criteria_sha256: Sha256Digest("9".repeat(64)),
                    observation_progress_sha256: None,
                    repeated_count: 1,
                    recorded_at: now(),
                },
            ),
            Err(StoreError::GoalAccounting(
                "completion_evidence_incompatible"
            ))
        ));
        {
            let mut connection = store.connection.lock().expect("store lock");
            let tx = connection.transaction().expect("pause transaction");
            let mut stored_goal = goal_by_id(&tx, goal.id).expect("stored goal");
            stored_goal.lifecycle = GoalLifecycle::Paused;
            write_goal(&tx, &mut stored_goal).expect("pause goal");
            tx.commit().expect("commit pause");
        }
        assert!(matches!(
            store.complete_goal_invocation(
                invocation.id,
                invocation.owner_claim_id,
                None,
                current.clone(),
                vec![passed_evaluation.clone()],
                checkpoint.clone(),
            ),
            Err(StoreError::GoalAccounting("goal_not_active"))
        ));
        {
            let mut connection = store.connection.lock().expect("store lock");
            let tx = connection.transaction().expect("expiry transaction");
            let mut stored_goal = goal_by_id(&tx, goal.id).expect("stored goal");
            stored_goal.lifecycle = GoalLifecycle::Active;
            write_goal(&tx, &mut stored_goal).expect("resume goal");
            let mut expired = read_invocation(&tx, invocation.id).expect("stored invocation");
            expired.deadline_at = now() - Duration::seconds(1);
            write_invocation(&tx, &expired).expect("expire invocation");
            tx.commit().expect("commit expiry");
        }
        assert!(matches!(
            store.complete_goal_invocation(
                invocation.id,
                invocation.owner_claim_id,
                None,
                current.clone(),
                vec![passed_evaluation.clone()],
                checkpoint.clone(),
            ),
            Err(StoreError::GoalAccounting("goal_invocation_deadline"))
        ));
        {
            let mut connection = store.connection.lock().expect("store lock");
            let tx = connection.transaction().expect("deadline transaction");
            let mut current_invocation =
                read_invocation(&tx, invocation.id).expect("stored invocation");
            current_invocation.deadline_at = now() + Duration::seconds(60);
            write_invocation(&tx, &current_invocation).expect("restore deadline");
            tx.commit().expect("commit deadline");
        }
        let (completed_goal, completed_invocation) = store
            .complete_goal_invocation(
                invocation.id,
                invocation.owner_claim_id,
                None,
                current,
                vec![passed_evaluation],
                checkpoint,
            )
            .expect("verified completion");
        assert_eq!(completed_goal.lifecycle, GoalLifecycle::Complete);
        assert_eq!(completed_invocation.state, GoalInvocationState::Verified);
        assert_eq!(
            completed_goal
                .completion
                .as_ref()
                .expect("completion")
                .evidence_ids,
            [passed.id]
        );
        let history = store
            .latest_verification_evidence(project_id, 8)
            .expect("evidence history");
        assert!(history.iter().any(
            |evidence| evidence.id == failed.id && evidence.state == VerificationState::Failed
        ));
        assert_eq!(configured.acceptance.expect("acceptance").version, 1);
    }

    #[test]
    fn accounting_configuration_invalidates_an_active_invocation_without_resetting_usage() {
        let directory = tempfile::tempdir().expect("fixture root");
        let store = Store::open_in_memory().expect("store");
        let (project_id, session_id, goal) = fixture(&store, directory.path());
        let configured = store
            .configure_goal_acceptance(
                project_id,
                session_id,
                goal.id,
                vec![criterion(Uuid::new_v4())],
                false,
            )
            .expect("acceptance");
        let invocation = store
            .start_goal_invocation(
                project_id,
                session_id,
                "workspace-key",
                Uuid::new_v4(),
                &supervision_request(goal.id, 2, 60),
            )
            .expect("invocation");
        let before = configured.accounting.expect("accounting");
        let changed = store
            .configure_goal_limits(
                project_id,
                session_id,
                goal.id,
                [200_000, 20, 10, 20, 600, 100_000, 10, 10, 300],
            )
            .expect("changed limits");
        let after = changed.accounting.expect("changed accounting");
        assert_ne!(after.revision, before.revision);
        assert_eq!(after.counters, before.counters);
        assert!(matches!(
            store.validate_goal_invocation_claim(
                invocation.id,
                invocation.owner_claim_id,
                "workspace-key"
            ),
            Err(StoreError::GoalAccounting(
                "goal_accounting_revision_changed"
            ))
        ));
        assert_eq!(
            store
                .latest_goal_invocation(project_id, session_id, goal.id)
                .expect("invocation query")
                .expect("invocation")
                .state,
            GoalInvocationState::Active
        );
    }

    #[test]
    fn owner_definition_change_settles_the_known_attempt_without_reusing_its_claim() {
        let directory = tempfile::tempdir().expect("fixture root");
        let store = Store::open_in_memory().expect("store");
        let (project_id, session_id, goal) = fixture(&store, directory.path());
        let configured = store
            .configure_goal_acceptance(
                project_id,
                session_id,
                goal.id,
                vec![criterion(Uuid::new_v4())],
                false,
            )
            .expect("acceptance");
        let invocation = store
            .start_goal_invocation(
                project_id,
                session_id,
                "workspace-key",
                Uuid::new_v4(),
                &supervision_request(goal.id, 2, 60),
            )
            .expect("invocation");
        let run_id = Uuid::new_v4();
        store
            .begin_goal_invocation_attempt(
                invocation.id,
                invocation.owner_claim_id,
                Uuid::new_v4(),
                run_id,
                snapshot('b'),
                vec![CriterionEvaluation {
                    criterion_id: configured.acceptance.as_ref().expect("acceptance").criteria[0]
                        .id,
                    runner: VerificationRunnerId::CargoFmtCheck,
                    state: CriterionEvidenceState::Unknown,
                    evidence_id: None,
                    detail: "no_typed_receipt".into(),
                }],
            )
            .expect("attempt");

        let mut changed = store
            .goal(project_id, session_id)
            .expect("goal query")
            .expect("goal");
        changed.objective = "Owner supplied a different definition".into();
        store.save_goal(&changed).expect("owner definition edit");
        assert!(matches!(
            store.validate_goal_invocation_claim(
                invocation.id,
                invocation.owner_claim_id,
                "workspace-key"
            ),
            Err(StoreError::GoalAccounting("goal_definition_changed"))
        ));

        let settled = store
            .settle_goal_invocation_after_attempt_error(
                invocation.id,
                invocation.owner_claim_id,
                run_id,
                InvocationAttemptOutcome::RuntimeFailed,
                GoalInvocationState::Incomplete,
                "goal_definition_changed",
            )
            .expect("known invalidated attempt settles");
        assert_eq!(settled.state, GoalInvocationState::Incomplete);
        assert_eq!(
            settled.attempts[0].outcome,
            InvocationAttemptOutcome::RuntimeFailed
        );
        assert!(settled.attempts[0].finished_at.is_some());
        assert!(matches!(
            store.settle_goal_invocation_after_attempt_error(
                invocation.id,
                invocation.owner_claim_id,
                run_id,
                InvocationAttemptOutcome::RuntimeFailed,
                GoalInvocationState::Incomplete,
                "goal_definition_changed",
            ),
            Err(StoreError::GoalAccounting("goal_invocation_settled"))
        ));
    }

    #[test]
    fn injected_transition_failure_leaves_one_conservative_unresolved_claim() {
        let directory = tempfile::tempdir().expect("fixture root");
        let store = Store::open_in_memory().expect("store");
        let (project_id, session_id, goal) = fixture(&store, directory.path());
        let configured = store
            .configure_goal_acceptance(
                project_id,
                session_id,
                goal.id,
                vec![criterion(Uuid::new_v4())],
                false,
            )
            .expect("acceptance");
        let invocation = store
            .start_goal_invocation(
                project_id,
                session_id,
                "workspace-key",
                Uuid::new_v4(),
                &supervision_request(goal.id, 2, 60),
            )
            .expect("invocation");
        let run_id = Uuid::new_v4();
        store
            .begin_goal_invocation_attempt(
                invocation.id,
                invocation.owner_claim_id,
                Uuid::new_v4(),
                run_id,
                snapshot('b'),
                vec![CriterionEvaluation {
                    criterion_id: configured.acceptance.as_ref().expect("acceptance").criteria[0]
                        .id,
                    runner: VerificationRunnerId::CargoFmtCheck,
                    state: CriterionEvidenceState::Unknown,
                    evidence_id: None,
                    detail: "no_typed_receipt".into(),
                }],
            )
            .expect("attempt");
        {
            let connection = store.connection.lock().expect("connection");
            connection
                .execute_batch(
                    "CREATE TEMP TRIGGER fail_goal_invocation_update
                     BEFORE UPDATE ON goal_invocations
                     BEGIN SELECT RAISE(ABORT, 'injected transition failure'); END;",
                )
                .expect("fault trigger");
        }
        assert!(store
            .settle_goal_invocation_after_attempt_error(
                invocation.id,
                invocation.owner_claim_id,
                run_id,
                InvocationAttemptOutcome::RuntimeFailed,
                GoalInvocationState::Failed,
                "storage_failure",
            )
            .is_err());
        {
            let connection = store.connection.lock().expect("connection");
            connection
                .execute_batch("DROP TRIGGER fail_goal_invocation_update;")
                .expect("remove fault trigger");
        }
        let unresolved = store
            .goal_invocation(invocation.id)
            .expect("invocation query")
            .expect("invocation");
        assert_eq!(unresolved.state, GoalInvocationState::Active);
        assert!(unresolved.attempts[0].finished_at.is_none());
        assert!(matches!(
            store.start_goal_invocation(
                project_id,
                session_id,
                "workspace-key",
                Uuid::new_v4(),
                &supervision_request(goal.id, 1, 60),
            ),
            Err(StoreError::HostOwnershipUnknown(
                "legacy_session_work_has_no_owner_evidence"
            ))
        ));
    }
}
