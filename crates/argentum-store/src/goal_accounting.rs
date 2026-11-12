use super::*;
use argentum_domain::goal_accounting::*;
use argentum_domain::GoalLifecycle;
use argentum_domain::ProjectId as Uuid;
use rusqlite::{Transaction, TransactionBehavior};

#[path = "accounting_recovery.rs"]
mod accounting_recovery;
#[cfg(test)]
#[path = "goal_accounting_tests.rs"]
mod tests;

fn rejected(reason: &'static str) -> StoreError {
    StoreError::GoalAccounting(reason)
}

fn add(left: u64, right: u64) -> Result<u64, StoreError> {
    left.checked_add(right)
        .ok_or_else(|| rejected("accounting_overflow"))
}

fn increment(value: &mut u32) -> Result<(), StoreError> {
    *value = value
        .checked_add(1)
        .ok_or_else(|| rejected("accounting_overflow"))?;
    Ok(())
}

pub(super) fn goal_by_id(tx: &Connection, id: Uuid) -> Result<Goal, StoreError> {
    let payload: String = tx.query_row(
        "SELECT payload FROM goals WHERE id = ?1 UNION ALL SELECT payload FROM goal_history WHERE goal_id = ?1 LIMIT 1",
        [id.to_string()], |row| row.get(0),
    ).optional()?.ok_or_else(|| rejected("goal_scope_unavailable"))?;
    let goal: Goal = serde_json::from_str(&payload)?;
    validate_goal(&goal)?;
    if goal.id != id {
        return Err(rejected("goal_scope_mismatch"));
    }
    Ok(goal)
}

pub(super) fn write_goal(tx: &Connection, goal: &mut Goal) -> Result<(), StoreError> {
    let accounting = goal
        .accounting
        .as_ref()
        .ok_or_else(|| rejected("legacy_usage_history_unavailable"))?;
    if !accounting.incomplete_history {
        goal.tokens_used = accounting.counters.effective_tokens();
        goal.tools_used = accounting.counters.tool_admissions;
        goal.iteration = accounting.counters.iterations;
    }
    goal.updated_at = now();
    let payload = serde_json::to_string(goal)?;
    let changed = tx.execute(
        "UPDATE goals SET payload = ?2, updated_at = ?3 WHERE id = ?1",
        params![
            goal.id.to_string(),
            payload,
            goal.updated_at.unix_timestamp()
        ],
    )?;
    if changed == 0 {
        let changed = tx.execute(
            "UPDATE goal_history SET payload = ?2 WHERE goal_id = ?1",
            params![goal.id.to_string(), payload],
        )?;
        if changed != 1 {
            return Err(rejected("goal_scope_unavailable"));
        }
    }
    Ok(())
}

pub(super) fn eligible(goal: &Goal) -> Result<(), StoreError> {
    let account = goal
        .accounting
        .as_ref()
        .ok_or_else(|| rejected("legacy_usage_history_unavailable"))?;
    if !account.supported_format() || account.incomplete_history {
        return Err(rejected("legacy_usage_history_unavailable"));
    }
    if account.stop_reason.as_deref() == Some("goal_accounting_unresolved") {
        return Err(rejected("goal_accounting_unresolved"));
    }
    if account.counters.unresolved_tokens > 0 || account.counters.outstanding_tokens > 0 {
        return Err(rejected("goal_usage_unresolved"));
    }
    if account.counters.overrun_tokens > 0 {
        return Err(rejected("goal_usage_overrun"));
    }
    if goal.lifecycle != GoalLifecycle::Active {
        return Err(rejected("goal_not_active"));
    }
    if goal
        .token_budget
        .is_some_and(|limit| account.counters.effective_tokens() >= limit)
    {
        return Err(rejected("goal_budget_exhausted"));
    }
    Ok(())
}

fn read_run(tx: &Transaction<'_>, id: Uuid) -> Result<RunAccounting, StoreError> {
    let payload: String = tx.query_row(
        "SELECT payload FROM goal_runs WHERE run_id = ?1",
        [id.to_string()],
        |row| row.get(0),
    )?;
    let run: RunAccounting = serde_json::from_str(&payload)?;
    if run.run_id != id {
        return Err(rejected("goal_scope_mismatch"));
    }
    Ok(run)
}

fn write_run(tx: &Transaction<'_>, run: &RunAccounting) -> Result<(), StoreError> {
    if tx.execute(
        "UPDATE goal_runs SET payload = ?2 WHERE run_id = ?1",
        params![run.run_id.to_string(), serde_json::to_string(run)?],
    )? != 1
    {
        return Err(rejected("goal_scope_unavailable"));
    }
    Ok(())
}

impl Store {
    pub fn inspect_goal_accounting(
        &self,
        project: ProjectId,
        session: SessionId,
        goal_id: Uuid,
    ) -> Result<GoalInspection, StoreError> {
        let mut connection = self
            .connection
            .lock()
            .map_err(|_| StoreError::LockPoisoned)?;
        let tx = connection.transaction()?;
        let goal = goal_by_id(&tx, goal_id)?;
        if goal.project_id != project || goal.session_id != session {
            return Err(rejected("goal_scope_mismatch"));
        }
        let mut runs = tx.prepare(
            "SELECT payload FROM goal_runs WHERE goal_id = ?1 ORDER BY rowid DESC LIMIT 17",
        )?;
        let mut recent_runs = runs
            .query_map([goal_id.to_string()], |row| row.get::<_, String>(0))?
            .map(|value| Ok(serde_json::from_str::<RunAccounting>(&value?)?))
            .collect::<Result<Vec<_>, StoreError>>()?;
        let runs_truncated = recent_runs.len() > 16;
        recent_runs.truncate(16);
        let mut charges = tx.prepare("SELECT q.payload FROM goal_requests q JOIN goal_runs r ON q.run_id = r.run_id WHERE r.goal_id = ?1 ORDER BY (json_extract(q.payload, '$.state') IN ('reserved','dispatched','unresolved')) DESC, q.rowid DESC LIMIT 129")?;
        let mut requests = charges
            .query_map([goal_id.to_string()], |row| row.get::<_, String>(0))?
            .map(|value| Ok(serde_json::from_str::<RequestCharge>(&value?)?))
            .collect::<Result<Vec<_>, StoreError>>()?;
        let requests_truncated = requests.len() > 128;
        requests.truncate(128);
        Ok(GoalInspection {
            recent_runs,
            requests,
            runs_truncated,
            requests_truncated,
        })
    }
    pub fn configure_goal_limits(
        &self,
        project: ProjectId,
        session: SessionId,
        goal_id: Uuid,
        limits: [u64; 9],
    ) -> Result<Goal, StoreError> {
        if limits.contains(&0) {
            return Err(rejected("invalid_finite_goal_limits"));
        }
        let requests =
            u32::try_from(limits[1]).map_err(|_| rejected("invalid_finite_goal_limits"))?;
        let iterations =
            u32::try_from(limits[2]).map_err(|_| rejected("invalid_finite_goal_limits"))?;
        if requests > 100_000 || iterations > 100_000 {
            return Err(rejected("invalid_finite_goal_limits"));
        }
        let per_run = RunLimits {
            token_limit: Some(limits[5]),
            request_limit: Some(
                u32::try_from(limits[6]).map_err(|_| rejected("invalid_finite_goal_limits"))?,
            ),
            tool_limit: Some(
                u32::try_from(limits[7]).map_err(|_| rejected("invalid_finite_goal_limits"))?,
            ),
            elapsed_seconds: Some(limits[8]),
        };
        per_run.validate().map_err(rejected)?;
        let mut connection = self
            .connection
            .lock()
            .map_err(|_| StoreError::LockPoisoned)?;
        let tx = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let mut goal = goal_by_id(&tx, goal_id)?;
        if goal.project_id != project || goal.session_id != session {
            return Err(rejected("goal_scope_mismatch"));
        }
        let active: bool = tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM goals WHERE id = ?1)",
            [goal_id.to_string()],
            |row| row.get(0),
        )?;
        if !active {
            return Err(rejected("goal_not_active"));
        }
        goal.token_budget = Some(limits[0]);
        goal.tool_budget =
            Some(u32::try_from(limits[3]).map_err(|_| rejected("invalid_finite_goal_limits"))?);
        goal.time_budget_seconds = Some(limits[4]);
        validate_goal(&goal)?;
        let account = goal
            .accounting
            .as_mut()
            .ok_or_else(|| rejected("legacy_usage_history_unavailable"))?;
        account.revision = Uuid::new_v4();
        account.request_limit = Some(requests);
        account.iteration_limit = Some(iterations);
        account.per_run = per_run;
        account.owner_removed_aggregate_limits = false;
        write_goal(&tx, &mut goal)?;
        tx.commit()?;
        // Owner edits deliberately invalidate context checkpoint snapshots.
        Ok(goal)
    }
    /// Explicit owner amendment. Keep accumulated charges and finite run guards.
    pub fn remove_goal_aggregate_limits(
        &self,
        project: ProjectId,
        session: SessionId,
        goal_id: Uuid,
    ) -> Result<Goal, StoreError> {
        let mut connection = self
            .connection
            .lock()
            .map_err(|_| StoreError::LockPoisoned)?;
        let tx = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let mut goal = goal_by_id(&tx, goal_id)?;
        if goal.project_id != project || goal.session_id != session {
            return Err(rejected("goal_scope_mismatch"));
        }
        let active: bool = tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM goals WHERE id = ?1)",
            [goal_id.to_string()],
            |row| row.get(0),
        )?;
        if !active {
            return Err(rejected("goal_not_active"));
        }
        let account = goal
            .accounting
            .as_mut()
            .ok_or_else(|| rejected("legacy_usage_history_unavailable"))?;
        if !account.supported_format() || account.incomplete_history {
            return Err(rejected("legacy_usage_history_unavailable"));
        }
        let run = &account.per_run;
        if run.token_limit.is_none()
            || run.request_limit.is_none()
            || run.tool_limit.is_none()
            || run.elapsed_seconds.is_none()
        {
            return Err(rejected("finite_per_run_limits_required"));
        }
        run.validate().map_err(rejected)?;
        account.request_limit = None;
        account.iteration_limit = None;
        account.owner_removed_aggregate_limits = true;
        account.revision = Uuid::new_v4();
        goal.token_budget = None;
        goal.tool_budget = None;
        goal.time_budget_seconds = None;
        validate_goal(&goal)?;
        write_goal(&tx, &mut goal)?;
        tx.commit()?;
        Ok(goal)
    }
    /// Explicit admitted attempts only. Resuming a goal never calls this method.
    pub fn admit_goal_run(
        &self,
        project: ProjectId,
        session: SessionId,
        binding: GoalBinding,
        run_id: RunId,
        maintenance: bool,
    ) -> Result<RunAccounting, StoreError> {
        let mut connection = self
            .connection
            .lock()
            .map_err(|_| StoreError::LockPoisoned)?;
        let tx = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let mut goal = goal_by_id(&tx, binding.goal_id)?;
        if goal.project_id != project || goal.session_id != session {
            return Err(rejected("goal_scope_mismatch"));
        }
        let active: bool = tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM goals WHERE id = ?1)",
            [binding.goal_id.to_string()],
            |row| row.get(0),
        )?;
        if !active {
            return Err(rejected("goal_not_active"));
        }
        eligible(&goal)?;
        let account = goal
            .accounting
            .as_mut()
            .ok_or_else(|| rejected("legacy_usage_history_unavailable"))?;
        if account.revision != binding.revision {
            return Err(rejected("goal_revision_changed"));
        }
        account.per_run.validate().map_err(rejected)?;
        if !maintenance
            && account
                .iteration_limit
                .is_some_and(|limit| account.counters.iterations >= limit)
        {
            return Err(rejected("goal_iteration_limit"));
        }
        if goal.time_budget_seconds.is_some_and(|limit| {
            account.counters.elapsed_milliseconds >= limit.saturating_mul(1000)
        }) {
            return Err(rejected("goal_deadline_exceeded"));
        }
        // The existing session owner admits at most one live run. Across hosts,
        // unfinished intervals are conservatively blocked pending F08, not resumed.
        let unfinished: bool = tx.query_row("SELECT EXISTS(SELECT 1 FROM goal_runs WHERE goal_id = ?1 AND json_extract(payload, '$.finished_at') IS NULL)", [binding.goal_id.to_string()], |row| row.get(0))?;
        if unfinished {
            return Err(rejected("goal_unfinished_run"));
        }
        if !maintenance {
            increment(&mut account.counters.iterations)?;
        }
        let mut limits = account.per_run.clone();
        if let Some(seconds) = goal.time_budget_seconds {
            let remaining_ms = seconds
                .checked_mul(1000)
                .ok_or_else(|| rejected("accounting_overflow"))?
                .saturating_sub(account.counters.elapsed_milliseconds);
            // Round down rather than grant an extra fractional second.
            let remaining = remaining_ms / 1000;
            if remaining == 0 {
                return Err(rejected("goal_deadline_exceeded"));
            }
            limits.elapsed_seconds = Some(
                limits
                    .elapsed_seconds
                    .map_or(remaining, |limit| limit.min(remaining)),
            );
        }
        let run = RunAccounting {
            run_id,
            project_id: project,
            session_id: session,
            binding,
            limits,
            counters: Counters {
                iterations: u32::from(!maintenance),
                ..Counters::default()
            },
            admitted_at: now(),
            finished_at: None,
            stop_reason: None,
            maintenance,
        };
        tx.execute(
            "INSERT INTO goal_runs (run_id, goal_id, payload) VALUES (?1, ?2, ?3)",
            params![
                run_id.to_string(),
                binding.goal_id.to_string(),
                serde_json::to_string(&run)?
            ],
        )?;
        write_goal(&tx, &mut goal)?;
        tx.commit()?;
        self.context_neutral_changes
            .fetch_add(2, std::sync::atomic::Ordering::Relaxed);
        Ok(run)
    }

    pub fn reserve_goal_request(
        &self,
        run_id: RunId,
        request_id: Uuid,
        purpose: &str,
        estimate: &argentum_domain::context_budget::RequestAccounting,
    ) -> Result<RequestCharge, StoreError> {
        if !estimate.fits
            || request_id.is_nil()
            || !matches!(purpose, "main_generation" | "compaction_tools_disabled")
        {
            return Err(rejected("invalid_request_reservation"));
        }
        let reserved = add(
            add(estimate.accounted_input, u64::from(estimate.output_reserve))?,
            estimate.uncertainty_allowance,
        )?;
        let mut connection = self
            .connection
            .lock()
            .map_err(|_| StoreError::LockPoisoned)?;
        let tx = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let mut run = read_run(&tx, run_id)?;
        if run.finished_at.is_some() {
            return Err(rejected("goal_run_finished"));
        }
        let mut goal = goal_by_id(&tx, run.binding.goal_id)?;
        eligible(&goal)?;
        let account = goal
            .accounting
            .as_mut()
            .ok_or_else(|| rejected("legacy_usage_history_unavailable"))?;
        // A changed declaration cannot transfer an already admitted scope.
        // Current limits may narrow it; the original run limits remain ceilings.
        if account
            .request_limit
            .is_some_and(|n| account.counters.request_attempts >= n)
            || run
                .limits
                .request_limit
                .is_some_and(|n| run.counters.request_attempts >= n)
            || account
                .per_run
                .request_limit
                .is_some_and(|n| run.counters.request_attempts >= n)
        {
            return Err(rejected("goal_request_limit"));
        }
        let goal_next = add(account.counters.effective_tokens(), reserved)?;
        let run_next = add(run.counters.effective_tokens(), reserved)?;
        if goal.token_budget.is_some_and(|n| goal_next > n)
            || run.limits.token_limit.is_some_and(|n| run_next > n)
            || account.per_run.token_limit.is_some_and(|n| run_next > n)
        {
            return Err(rejected("goal_budget_exhausted"));
        }
        increment(&mut account.counters.request_attempts)?;
        increment(&mut run.counters.request_attempts)?;
        account.counters.outstanding_tokens = add(account.counters.outstanding_tokens, reserved)?;
        run.counters.outstanding_tokens = add(run.counters.outstanding_tokens, reserved)?;
        let run_token_limit = match (run.limits.token_limit, account.per_run.token_limit) {
            (Some(a), Some(b)) => Some(a.min(b)),
            (a, b) => a.or(b),
        };
        let charge = RequestCharge {
            request_id,
            run_id,
            binding: run.binding,
            limit_revision: account.revision,
            goal_token_limit: goal.token_budget,
            run_token_limit,
            profile_id: estimate.profile_id.clone(),
            model_budget_revision: estimate.budget_revision,
            purpose: purpose.into(),
            method: estimate.method.clone(),
            reserved_tokens: reserved,
            state: ChargeState::Reserved,
            usage_quality: "not_dispatched".into(),
            observed: None,
            overrun_tokens: 0,
            disposition: None,
        };
        tx.execute(
            "INSERT INTO goal_requests (request_id, run_id, payload) VALUES (?1, ?2, ?3)",
            params![
                request_id.to_string(),
                run_id.to_string(),
                serde_json::to_string(&charge)?
            ],
        )?;
        write_goal(&tx, &mut goal)?;
        write_run(&tx, &run)?;
        tx.commit()?;
        self.context_neutral_changes
            .fetch_add(3, std::sync::atomic::Ordering::Relaxed);
        Ok(charge)
    }

    pub fn goal_request_dispatched(&self, request: Uuid) -> Result<(), StoreError> {
        self.change_goal_charge(request, ChargeState::Dispatched, None, false)
            .map(|_| ())
    }

    /// The caller must prove no transport dispatch to release an unused charge.
    pub fn goal_request_not_dispatched(&self, request: Uuid) -> Result<(), StoreError> {
        self.change_goal_charge(request, ChargeState::NotDispatched, None, false)
            .map(|_| ())
    }

    pub fn settle_goal_request(
        &self,
        request: Uuid,
        usage: Option<ObservedUsage>,
    ) -> Result<RequestCharge, StoreError> {
        self.change_goal_charge(
            request,
            if usage.is_some() {
                ChargeState::Settled
            } else {
                ChargeState::Unresolved
            },
            usage,
            false,
        )
    }

    pub fn settle_goal_invalid_usage(&self, request: Uuid) -> Result<RequestCharge, StoreError> {
        self.change_goal_charge(request, ChargeState::Unresolved, None, true)
    }

    fn change_goal_charge(
        &self,
        request: Uuid,
        next: ChargeState,
        observed: Option<ObservedUsage>,
        invalid_usage: bool,
    ) -> Result<RequestCharge, StoreError> {
        if let Some(usage) = &observed {
            usage.validate().map_err(rejected)?;
        }
        let mut connection = self
            .connection
            .lock()
            .map_err(|_| StoreError::LockPoisoned)?;
        let tx = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let payload: String = tx.query_row(
            "SELECT payload FROM goal_requests WHERE request_id = ?1",
            [request.to_string()],
            |row| row.get(0),
        )?;
        let mut charge: RequestCharge = serde_json::from_str(&payload)?;
        if charge.request_id != request {
            return Err(rejected("goal_scope_mismatch"));
        }
        if charge.disposition.is_some() {
            return accounting_recovery::settle_later_usage(
                self,
                tx,
                charge,
                next,
                observed,
                invalid_usage,
            );
        }
        let quality = if invalid_usage {
            "invalid_or_conflicting_final_usage"
        } else if next == ChargeState::Dispatched {
            "awaiting_final_usage"
        } else if next == ChargeState::NotDispatched {
            "proven_not_dispatched"
        } else if let Some(usage) = observed.as_ref() {
            usage.quality_label()
        } else {
            "missing_final_usage"
        };
        if charge.state == next && charge.observed == observed && charge.usage_quality == quality {
            return Ok(charge);
        }
        if matches!(
            charge.state,
            ChargeState::Settled | ChargeState::NotDispatched
        ) || (next == ChargeState::NotDispatched && charge.state != ChargeState::Reserved)
            || (next == ChargeState::Dispatched && charge.state != ChargeState::Reserved)
            || (matches!(next, ChargeState::Settled | ChargeState::Unresolved)
                && !matches!(
                    charge.state,
                    ChargeState::Dispatched | ChargeState::Unresolved
                ))
        {
            return Err(rejected("goal_settlement_conflict"));
        }
        let mut run = read_run(&tx, charge.run_id)?;
        if run.binding != charge.binding {
            return Err(rejected("goal_scope_mismatch"));
        }
        let mut goal = goal_by_id(&tx, charge.binding.goal_id)?;
        let account = goal
            .accounting
            .as_mut()
            .ok_or_else(|| rejected("legacy_usage_history_unavailable"))?;
        let overrun = if let Some(usage) = &observed {
            let goal_after = add(account.counters.effective_tokens(), usage.total_tokens)?;
            let run_after = add(run.counters.effective_tokens(), usage.total_tokens)?;
            usage
                .total_tokens
                .saturating_sub(charge.reserved_tokens)
                .max(
                    goal.token_budget
                        .map_or(0, |limit| goal_after.saturating_sub(limit)),
                )
                .max(
                    run.limits
                        .token_limit
                        .map_or(0, |limit| run_after.saturating_sub(limit)),
                )
                .max(
                    account
                        .per_run
                        .token_limit
                        .map_or(0, |limit| run_after.saturating_sub(limit)),
                )
        } else {
            0
        };
        if next != ChargeState::Dispatched {
            for counters in [&mut account.counters, &mut run.counters] {
                let held = if charge.state == ChargeState::Unresolved {
                    &mut counters.unresolved_tokens
                } else {
                    &mut counters.outstanding_tokens
                };
                *held = held
                    .checked_sub(charge.reserved_tokens)
                    .ok_or_else(|| rejected("accounting_underflow"))?;
                if next == ChargeState::Unresolved {
                    counters.unresolved_tokens =
                        add(counters.unresolved_tokens, charge.reserved_tokens)?;
                }
                if let Some(usage) = &observed {
                    counters.observed_tokens = add(counters.observed_tokens, usage.total_tokens)?;
                    counters.overrun_tokens = add(counters.overrun_tokens, overrun)?;
                }
            }
        }
        charge.overrun_tokens = overrun;
        charge.observed = observed;
        charge.state = next;
        charge.usage_quality = quality.into();
        if next == ChargeState::Unresolved {
            account.stop_reason = Some("goal_usage_unresolved".into());
        }
        if charge.overrun_tokens > 0
            || goal
                .token_budget
                .is_some_and(|n| account.counters.effective_tokens() > n)
            || run
                .limits
                .token_limit
                .is_some_and(|n| run.counters.effective_tokens() > n)
        {
            account.stop_reason = Some("goal_usage_overrun".into());
        }
        tx.execute(
            "UPDATE goal_requests SET payload = ?2 WHERE request_id = ?1",
            params![request.to_string(), serde_json::to_string(&charge)?],
        )?;
        write_run(&tx, &run)?;
        write_goal(&tx, &mut goal)?;
        tx.commit()?;
        self.context_neutral_changes
            .fetch_add(3, std::sync::atomic::Ordering::Relaxed);
        Ok(charge)
    }

    pub fn admit_goal_tool(&self, run_id: RunId) -> Result<(), StoreError> {
        let mut connection = self
            .connection
            .lock()
            .map_err(|_| StoreError::LockPoisoned)?;
        let tx = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let mut run = read_run(&tx, run_id)?;
        if run.finished_at.is_some() {
            return Err(rejected("goal_run_finished"));
        }
        let mut goal = goal_by_id(&tx, run.binding.goal_id)?;
        eligible(&goal)?;
        let account = goal
            .accounting
            .as_mut()
            .ok_or_else(|| rejected("legacy_usage_history_unavailable"))?;
        if goal
            .tool_budget
            .is_some_and(|n| account.counters.tool_admissions >= n)
            || run
                .limits
                .tool_limit
                .is_some_and(|n| run.counters.tool_admissions >= n)
            || account
                .per_run
                .tool_limit
                .is_some_and(|n| run.counters.tool_admissions >= n)
        {
            return Err(rejected("goal_tool_limit"));
        }
        increment(&mut account.counters.tool_admissions)?;
        increment(&mut run.counters.tool_admissions)?;
        write_run(&tx, &run)?;
        write_goal(&tx, &mut goal)?;
        tx.commit()?;
        self.context_neutral_changes
            .fetch_add(2, std::sync::atomic::Ordering::Relaxed);
        Ok(())
    }

    /// Synchronous pre-invocation failure only. The runtime must call this once
    /// before any tool future has been created or polled, never for a tool error.
    pub fn release_uninvoked_goal_tool(&self, run_id: RunId) -> Result<(), StoreError> {
        let mut connection = self
            .connection
            .lock()
            .map_err(|_| StoreError::LockPoisoned)?;
        let tx = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let mut run = read_run(&tx, run_id)?;
        if run.finished_at.is_some() {
            return Err(rejected("goal_run_finished"));
        }
        let mut goal = goal_by_id(&tx, run.binding.goal_id)?;
        let account = goal
            .accounting
            .as_mut()
            .ok_or_else(|| rejected("legacy_usage_history_unavailable"))?;
        for count in [
            &mut run.counters.tool_admissions,
            &mut account.counters.tool_admissions,
        ] {
            *count = count
                .checked_sub(1)
                .ok_or_else(|| rejected("accounting_underflow"))?;
        }
        write_run(&tx, &run)?;
        write_goal(&tx, &mut goal)?;
        tx.commit()?;
        self.context_neutral_changes
            .fetch_add(2, std::sync::atomic::Ordering::Relaxed);
        Ok(())
    }

    pub fn finish_goal_run(
        &self,
        run_id: RunId,
        elapsed_ms: u64,
        stop_reason: &str,
    ) -> Result<RunAccounting, StoreError> {
        if stop_reason.len() > 128
            || !stop_reason
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b == b'_')
        {
            return Err(rejected("invalid_stop_reason"));
        }
        let mut connection = self
            .connection
            .lock()
            .map_err(|_| StoreError::LockPoisoned)?;
        let tx = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let mut run = read_run(&tx, run_id)?;
        if run.finished_at.is_some() {
            return Ok(run);
        }
        let mut goal = goal_by_id(&tx, run.binding.goal_id)?;
        let account = goal
            .accounting
            .as_mut()
            .ok_or_else(|| rejected("legacy_usage_history_unavailable"))?;
        run.counters.elapsed_milliseconds = elapsed_ms;
        run.finished_at = Some(now());
        run.stop_reason = Some(stop_reason.into());
        account.counters.elapsed_milliseconds =
            add(account.counters.elapsed_milliseconds, elapsed_ms)?;
        account.stop_reason = Some(stop_reason.into());
        // Task success does not assert goal achievement.
        write_run(&tx, &run)?;
        write_goal(&tx, &mut goal)?;
        tx.commit()?;
        self.context_neutral_changes
            .fetch_add(2, std::sync::atomic::Ordering::Relaxed);
        Ok(run)
    }

    pub fn goal_run_accounting(&self, run_id: RunId) -> Result<Option<RunAccounting>, StoreError> {
        let connection = self
            .connection
            .lock()
            .map_err(|_| StoreError::LockPoisoned)?;
        let payload: Option<String> = connection
            .query_row(
                "SELECT payload FROM goal_runs WHERE run_id = ?1",
                [run_id.to_string()],
                |row| row.get(0),
            )
            .optional()?;
        payload
            .map(|payload| serde_json::from_str(&payload).map_err(StoreError::from))
            .transpose()
    }
}
