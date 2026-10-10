//! One request's owner disposition, in the existing accounting transaction.
use super::*;
use crate::command_identity::{self, CommandIdentityInsert};
use argentum_domain::recovery::{RecoveryEffectState, RecoveryExecutionState, RunRecoveryRecord};

const MAX_RECOVERY_PAYLOAD: usize = 16 * 1024;

fn reviewed_provider_only_interruption(owner: &RunRecoveryRecord) -> bool {
    owner.execution_state == RecoveryExecutionState::Interrupted
        && owner.recovered_at.is_some()
        && owner.recovery_revision.is_some()
        && owner.owner_reviewed_at.is_some()
        && owner.authority_invalidated
        && !owner.admission_blocked
        && owner.effect_state == RecoveryEffectState::None
        && owner.known_effects.is_empty()
        && owner.unresolved_effects.as_slice() == ["provider_request_outcome_unknown"]
}

fn digest(value: &impl serde::Serialize) -> Result<Sha256Digest, StoreError> {
    Ok(Sha256Digest(format!(
        "{:x}",
        Sha256::digest(serde_json::to_vec(value)?)
    )))
}

fn bounded<T: serde::de::DeserializeOwned>(
    db: &Connection,
    table: &str,
    key: &str,
    id: Uuid,
) -> Result<T, StoreError> {
    // Table and key are internal constants, never caller input. Bound before decode.
    let sql = format!("SELECT CASE WHEN length(CAST(payload AS BLOB))<=?2 THEN payload END FROM {table} WHERE {key}=?1");
    let payload: Option<String> = db
        .query_row(&sql, params![id.to_string(), MAX_RECOVERY_PAYLOAD], |r| {
            r.get(0)
        })
        .optional()?
        .flatten();
    serde_json::from_str(
        &payload.ok_or_else(|| rejected("accounting_recovery_record_unavailable"))?,
    )
    .map_err(|_| rejected("accounting_recovery_record_invalid"))
}

fn inspect_on(
    db: &Connection,
    physical: &Sha256Digest,
    workspace_key: &str,
    session: Uuid,
    goal_id: Uuid,
    request_id: Uuid,
) -> Result<AccountingRecoveryInspection, StoreError> {
    if [session, goal_id, request_id].iter().any(Uuid::is_nil) || workspace_key.len() > 32 * 1024 {
        return Err(rejected("goal_scope_mismatch"));
    }
    let charge: RequestCharge = bounded(db, "goal_requests", "request_id", request_id)?;
    let run: RunAccounting = bounded(db, "goal_runs", "run_id", charge.run_id)?;
    let goal: Goal = bounded(db, "goals", "id", goal_id)?;
    let owner: RunRecoveryRecord = bounded(db, "runtime_run_ownership", "run_id", charge.run_id)?;
    let project_matches: bool = db.query_row(
        "SELECT EXISTS(SELECT 1 FROM projects WHERE id=?1 AND workspace_key=?2)",
        params![goal.project_id.to_string(), workspace_key],
        |r| r.get(0),
    )?;
    if !project_matches
        || charge.request_id != request_id
        || charge.binding != run.binding
        || charge.binding.goal_id != goal_id
        || run.run_id != charge.run_id
        || run.session_id != session
        || goal.session_id != session
        || owner.run_id != run.run_id
        || owner.project_id != goal.project_id
        || owner.session_id != session
        || owner.request_id != Some(request_id)
        || owner.request_purpose != charge.purpose
        || run.project_id != goal.project_id
    {
        return Err(rejected("goal_scope_mismatch"));
    }
    let (owner_id, host_state): (String, String) = db.query_row(
        "SELECT owner_id,state FROM runtime_host_generations WHERE generation=?1",
        [owner.owner_generation],
        |r| Ok((r.get(0)?, r.get(1)?)),
    )?;
    let account = goal
        .accounting
        .as_ref()
        .ok_or_else(|| rejected("legacy_usage_history_unavailable"))?;
    let session_held = match crate::recovery::ensure_session_recovery_admissible_on(
        db,
        goal.project_id,
        session,
    ) {
        Ok(()) => false,
        Err(StoreError::RecoveryBlocked(_) | StoreError::HostOwnershipUnknown(_)) => true,
        Err(error) => return Err(error),
    };
    let provider_only = reviewed_provider_only_interruption(&owner);
    let other_active: bool = db.query_row("SELECT EXISTS(SELECT 1 FROM runtime_run_ownership WHERE project_id=?1 AND (state='active' OR (session_id=?2 AND (json_array_length(json_extract(payload,'$.unresolved_effects'))>0 OR json_extract(payload,'$.effect_state')='admitted') AND NOT (run_id=?3 AND ?4=1)))) OR EXISTS(SELECT 1 FROM goal_invocations WHERE project_id=?1 AND json_extract(payload,'$.owner_claim_retired')=0)",params![goal.project_id.to_string(),session.to_string(),owner.run_id.to_string(),provider_only], |r| r.get(0))?;
    let blocker = if owner.execution_state == RecoveryExecutionState::Active
        || !matches!(host_state.as_str(), "retired" | "recovered")
        || run.finished_at.is_none()
        || other_active
    {
        Some("accounting_recovery_owner_unsettled")
    } else if owner.effect_state == RecoveryEffectState::Admitted
        || (!owner.unresolved_effects.is_empty() && !provider_only)
        || owner.admission_blocked
    {
        Some("accounting_recovery_effects_unresolved")
    } else if session_held {
        Some("accounting_recovery_session_held")
    } else if !account.supported_format() || account.incomplete_history {
        Some("legacy_usage_history_unavailable")
    } else if !matches!(
        charge.state,
        ChargeState::Unresolved | ChargeState::Dispatched
    ) || charge.observed.is_some()
        || charge.reserved_tokens == 0
    {
        Some("accounting_recovery_request_not_unresolved")
    } else {
        None
    };
    let binding = AccountingRecoveryBinding {
        store_id: crate::recovery::read_store_id_on(db)?,
        physical_store_sha256: physical.clone(),
        project_id: goal.project_id,
        workspace_key: workspace_key.into(),
        session_id: session,
        goal_id,
        request_id,
        run_id: run.run_id,
        owner_generation: owner.owner_generation,
        owner_id: Uuid::parse_str(&owner_id)
            .map_err(|_| rejected("accounting_recovery_owner_invalid"))?,
        accounting_revision: account.revision,
        accounting_snapshot_sha256: digest(&(&goal, &charge, &run, &owner))?,
        reserved_tokens: charge.reserved_tokens,
    };
    Ok(AccountingRecoveryInspection {
        binding,
        provider_usage_complete: charge.observed.is_some()
            || charge.disposition.as_ref().is_some_and(|d| {
                d.later_observed_usage.is_some() && d.conflicting_later_usage.is_empty()
            }),
        charge,
        run,
        goal_counters: account.counters.clone(),
        ownership: owner,
        recoverable: blocker.is_none(),
        blocker: blocker.map(str::to_owned),
        mutation_performed: false,
    })
}

impl Store {
    /// Exact read-only inspection. No host admission, migration or active selection.
    pub fn inspect_accounting_recovery(
        path: impl AsRef<Path>,
        workspace: impl AsRef<Path>,
        session: Uuid,
        goal: Uuid,
        request: Uuid,
    ) -> Result<AccountingRecoveryInspection, StoreError> {
        #[cfg(not(windows))]
        {
            let _ = (path, workspace, session, goal, request);
            Err(StoreError::RecoveryUnsupported)
        }
        #[cfg(windows)]
        {
            let (_file, physical) =
                crate::recovery::pinned_physical_identity(path.as_ref(), false, false)?;
            let db = Connection::open_with_flags(
                &physical.canonical_path,
                OpenFlags::SQLITE_OPEN_READ_ONLY,
            )?;
            db.busy_timeout(DATABASE_BUSY_TIMEOUT)?;
            let (_, key) = canonical_workspace(workspace.as_ref())?;
            let tx = db.unchecked_transaction()?;
            inspect_on(&tx, &physical.digest, &key, session, goal, request)
        }
    }

    /// The runtime must supply an exact consumed owner proof before calling.
    pub fn resolve_accounting_recovery(
        &self,
        resolution: &AccountingResolution,
    ) -> Result<RequestCharge, StoreError> {
        self.ensure_runtime_host_guard()?;
        let generation = self
            .current_host_generation()?
            .ok_or_else(|| rejected("accounting_recovery_requires_owned_store"))?;
        let path = self
            .path
            .as_ref()
            .ok_or_else(|| rejected("accounting_recovery_requires_owned_store"))?;
        #[cfg(not(windows))]
        {
            let _ = (path, generation, resolution);
            Err(StoreError::RecoveryUnsupported)
        }
        #[cfg(windows)]
        {
            let (_file, physical) = crate::recovery::pinned_physical_identity(path, false, false)?;
            let bytes = serde_json::to_vec(resolution)?;
            if bytes.len() > MAX_RECOVERY_PAYLOAD {
                return Err(rejected("accounting_recovery_input_too_large"));
            }
            if let AccountingDecision::AuthoritativeUsage { usage, .. } = &resolution.decision {
                usage.validate().map_err(rejected)?;
            }
            let b = &resolution.binding;
            let input = CommandClaimInput {
                project_id: b.project_id,
                request_id: resolution.command_id.clone(),
                canonicalization_version: COMMAND_IDENTITY_VERSION,
                canonical_command: bytes,
                command_kind: "accounting_recovery".into(),
                scope: CommandIdentityScope {
                    session_id: Some(b.session_id),
                    subject_id: Some(b.request_id),
                    execution_snapshot_sha256: Some(b.accounting_snapshot_sha256.0.clone()),
                    ..Default::default()
                },
            };
            command_identity::validate_claim_input(&input)?;
            let fingerprint = command_identity::command_fingerprint(
                COMMAND_IDENTITY_VERSION,
                &input.canonical_command,
            );
            let mut connection = self
                .connection
                .lock()
                .map_err(|_| StoreError::LockPoisoned)?;
            let tx = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
            crate::recovery::validate_host_generation_on(&tx, generation)?;
            if crate::recovery::read_store_id_on(&tx)? != b.store_id
                || physical.digest != b.physical_store_sha256
            {
                return Err(rejected("accounting_recovery_store_mismatch"));
            }
            if let Some(existing) =
                command_identity::read_command_identity_on(&tx, b.project_id, &input.request_id)?
            {
                match command_identity::compare_existing(
                    existing,
                    COMMAND_IDENTITY_VERSION,
                    &fingerprint,
                ) {
                    CommandClaim::Existing(record)
                        if record.state == CommandIdentityState::Settled
                            && record.outcome_kind == Some(CommandOutcomeKind::Success) =>
                    {
                        let charge: RequestCharge =
                            bounded(&tx, "goal_requests", "request_id", b.request_id)?;
                        if charge.disposition.is_some() {
                            return Ok(charge);
                        }
                    }
                    _ => {}
                }
                return Err(rejected("accounting_recovery_command_conflict"));
            }
            let current = inspect_on(
                &tx,
                &physical.digest,
                &b.workspace_key,
                b.session_id,
                b.goal_id,
                b.request_id,
            )?;
            if current.binding != *b {
                return Err(rejected("accounting_recovery_snapshot_changed"));
            }
            if let Some(blocker) = current.blocker {
                return Err(StoreError::InvalidRecord(blocker));
            }
            let provider_only = reviewed_provider_only_interruption(&current.ownership);
            if provider_only
                && !matches!(
                    resolution.decision,
                    AccountingDecision::OwnerAcceptReservation
                )
            {
                return Err(rejected(
                    "interrupted_provider_requires_full_reservation_retention",
                ));
            }
            if let AccountingDecision::AuthoritativeUsage { event_id, usage } = &resolution.decision
            {
                if *event_id == 0 || *event_id > i64::MAX as u64 {
                    return Err(rejected("accounting_usage_evidence_unavailable"));
                }
                let payload:Option<String>=tx.query_row("SELECT CASE WHEN length(CAST(payload AS BLOB))<=16384 THEN payload END FROM event_log WHERE id=?1 AND project_id=?2 AND session_id=?3 AND run_id=?4 AND event_type='goal_request_accounting'",params![event_id,b.project_id.to_string(),b.session_id.to_string(),b.run_id.to_string()],|row|row.get(0)).optional()?.flatten();
                let event: AppEvent = serde_json::from_str(
                    &payload.ok_or_else(|| rejected("accounting_usage_evidence_unavailable"))?,
                )
                .map_err(|_| rejected("accounting_usage_evidence_invalid"))?;
                if !matches!(event,AppEvent::GoalRequestAccounting{charge,ref persistence,session_id,run_id} if session_id==b.session_id && run_id==b.run_id && charge.request_id==b.request_id && charge.run_id==b.run_id && charge.binding==current.charge.binding && charge.limit_revision==current.charge.limit_revision && charge.profile_id==current.charge.profile_id && charge.model_budget_revision==current.charge.model_budget_revision && charge.purpose==current.charge.purpose && charge.method==current.charge.method && charge.goal_token_limit==current.charge.goal_token_limit && charge.run_token_limit==current.charge.run_token_limit && charge.reserved_tokens==b.reserved_tokens && charge.observed.as_ref()==Some(usage) && persistence=="observed_settlement_pending" && charge.usage_quality==usage.quality_label())
                {
                    return Err(rejected("accounting_usage_evidence_mismatch"));
                }
            } else {
                let known:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM event_log WHERE project_id=?1 AND session_id=?2 AND run_id=?3 AND event_type='goal_request_accounting' AND CASE WHEN length(CAST(payload AS BLOB))<=16384 THEN (json_extract(payload,'$.GoalRequestAccounting.charge.request_id')=?4 AND json_extract(payload,'$.GoalRequestAccounting.charge.observed') IS NOT NULL) ELSE 1 END)",params![b.project_id.to_string(),b.session_id.to_string(),b.run_id.to_string(),b.request_id.to_string()],|row|row.get(0))?;
                if known {
                    return Err(rejected(
                        "accounting_usage_evidence_requires_reconciliation",
                    ));
                }
            }
            command_identity::enforce_admission_quotas_on(&tx, b.project_id)?;
            let mut charge = current.charge;
            if charge.disposition.is_some() {
                let AccountingDecision::AuthoritativeUsage { usage, .. } = &resolution.decision
                else {
                    return Err(rejected("accounting_recovery_already_disposed"));
                };
                let conflict = record_later_usage_on(&tx, &mut charge, usage.clone())?;
                command_identity::insert_command_identity_on(
                    &tx,
                    &input,
                    &fingerprint,
                    CommandIdentityInsert {
                        owner_generation: Some(generation),
                        state: CommandIdentityState::Settled,
                        result_session_id: Some(b.session_id),
                        outcome_kind: Some(if conflict {
                            CommandOutcomeKind::Failure
                        } else {
                            CommandOutcomeKind::Success
                        }),
                        result_payload: None,
                        result_available: false,
                        replay_allowed: true,
                        at: now(),
                    },
                )?;
                tx.commit()?;
                return if conflict {
                    Err(rejected("goal_settlement_conflict"))
                } else {
                    Ok(charge)
                };
            }
            let mut run = current.run;
            let mut goal = goal_by_id(&tx, b.goal_id)?;
            let account = goal
                .accounting
                .as_mut()
                .ok_or_else(|| rejected("legacy_usage_history_unavailable"))?;
            let actual = match &resolution.decision {
                AccountingDecision::AuthoritativeUsage { usage, .. } => Some(usage.clone()),
                _ => None,
            };
            let charged = actual
                .as_ref()
                .map_or(b.reserved_tokens, |usage| usage.total_tokens);
            let overrun = charged.saturating_sub(b.reserved_tokens);
            for counters in [&mut run.counters, &mut account.counters] {
                let held = if charge.state == ChargeState::Unresolved {
                    &mut counters.unresolved_tokens
                } else {
                    &mut counters.outstanding_tokens
                };
                *held = held
                    .checked_sub(b.reserved_tokens)
                    .ok_or_else(|| rejected("accounting_underflow"))?;
                if actual.is_some() {
                    counters.observed_tokens = add(counters.observed_tokens, charged)?;
                } else {
                    counters.owner_accepted_tokens = add(counters.owner_accepted_tokens, charged)?;
                }
                counters.overrun_tokens = add(counters.overrun_tokens, overrun)?;
            }
            charge.disposition = Some(AccountingDisposition {
                resolution: resolution.clone(),
                recorded_at: now(),
                resolving_owner_generation: generation,
                client_charge_tokens: charged,
                later_observed_usage: actual,
                conflicting_later_usage: Vec::new(),
            });
            account.revision = Uuid::new_v4();
            account.version = ACCOUNTING_VERSION;
            account.disposition_format = Some(1);
            // Original request state, usage quality, failed run and recovery
            // evidence remain unchanged. This is a separate admission decision.
            if account.counters.unresolved_tokens == 0
                && account.stop_reason.as_deref() == Some("goal_usage_unresolved")
            {
                account.stop_reason = Some("owner_accounting_disposition".into());
            }
            if overrun > 0 {
                account.stop_reason = Some("goal_usage_overrun".into());
            }
            tx.execute(
                "UPDATE goal_requests SET payload=?2 WHERE request_id=?1",
                params![b.request_id.to_string(), serde_json::to_string(&charge)?],
            )?;
            write_run(&tx, &run)?;
            write_goal(&tx, &mut goal)?;
            if provider_only {
                // This exact owner decision settles only the client reservation.
                // The interrupted execution, dispatched request, absent observed
                // usage and original recovery evidence remain historical facts.
                let mut ownership = current.ownership;
                ownership.unresolved_effects.clear();
                ownership.accounting_state =
                    argentum_domain::recovery::RecoveryAccountingState::Settled;
                ownership.updated_at = now();
                if tx.execute(
                    "UPDATE runtime_run_ownership SET updated_at=?3,payload=?4 WHERE run_id=?1 AND owner_generation=?2 AND state='interrupted'",
                    params![ownership.run_id.to_string(), ownership.owner_generation,
                        ownership.updated_at.unix_timestamp(), serde_json::to_string(&ownership)?],
                )? != 1 {
                    return Err(StoreError::RecoveryDecisionStale);
                }
            }
            command_identity::insert_command_identity_on(
                &tx,
                &input,
                &fingerprint,
                CommandIdentityInsert {
                    owner_generation: Some(generation),
                    state: CommandIdentityState::Settled,
                    result_session_id: Some(b.session_id),
                    outcome_kind: Some(CommandOutcomeKind::Success),
                    result_payload: None,
                    result_available: false,
                    replay_allowed: true,
                    at: now(),
                },
            )?;
            tx.commit()?;
            Ok(charge)
        }
    }
}

#[cfg(all(test, windows))]
#[path = "accounting_recovery_tests.rs"]
mod tests;

pub(super) fn settle_later_usage(
    store: &Store,
    tx: Transaction<'_>,
    mut charge: RequestCharge,
    next: ChargeState,
    observed: Option<ObservedUsage>,
    invalid: bool,
) -> Result<RequestCharge, StoreError> {
    if next != ChargeState::Settled || invalid || observed.is_none() {
        return Err(rejected("goal_settlement_conflict"));
    }
    let usage = observed.ok_or_else(|| rejected("goal_settlement_conflict"))?;
    let conflict = record_later_usage_on(&tx, &mut charge, usage)?;
    tx.commit()?;
    store
        .context_neutral_changes
        .fetch_add(3, std::sync::atomic::Ordering::Relaxed);
    if conflict {
        Err(rejected("goal_settlement_conflict"))
    } else {
        Ok(charge)
    }
}

fn record_later_usage_on(
    tx: &Transaction<'_>,
    charge: &mut RequestCharge,
    usage: ObservedUsage,
) -> Result<bool, StoreError> {
    let disposition = charge
        .disposition
        .as_mut()
        .ok_or_else(|| rejected("goal_settlement_conflict"))?;
    if disposition.later_observed_usage.as_ref() == Some(&usage) {
        return Ok(false);
    }
    let mut run = read_run(tx, charge.run_id)?;
    let mut goal = goal_by_id(tx, charge.binding.goal_id)?;
    let account = goal
        .accounting
        .as_mut()
        .ok_or_else(|| rejected("legacy_usage_history_unavailable"))?;
    if disposition.later_observed_usage.is_some() {
        if disposition.conflicting_later_usage.contains(&usage) {
            return Err(rejected("goal_settlement_conflict"));
        }
        if !disposition.conflicting_later_usage.is_empty() {
            return Err(rejected("goal_settlement_conflict"));
        }
        disposition.conflicting_later_usage.push(usage);
        // A retained conflict blocks admission even after restart, without
        // inventing a combined total or overwriting the first observation.
        account.stop_reason = Some("goal_accounting_unresolved".into());
    } else {
        let excess = usage
            .total_tokens
            .saturating_sub(disposition.client_charge_tokens);
        for counters in [&mut run.counters, &mut account.counters] {
            counters.later_usage_excess_tokens = add(counters.later_usage_excess_tokens, excess)?;
            counters.overrun_tokens = add(counters.overrun_tokens, excess)?;
        }
        disposition.later_observed_usage = Some(usage);
        if excess > 0 {
            account.stop_reason = Some("goal_usage_overrun".into());
        }
    }
    let conflict = !disposition.conflicting_later_usage.is_empty();
    account.revision = Uuid::new_v4();
    tx.execute(
        "UPDATE goal_requests SET payload=?2 WHERE request_id=?1",
        params![
            charge.request_id.to_string(),
            serde_json::to_string(&charge)?
        ],
    )?;
    write_run(tx, &run)?;
    write_goal(tx, &mut goal)?;
    Ok(conflict)
}
