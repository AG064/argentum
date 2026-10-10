use super::*;
use argentum_domain::goal_accounting::{GoalBinding, ObservedUsage, RequestCharge};

#[derive(Clone)]
pub(super) struct LiveGoalRun {
    pub(super) started: tokio::time::Instant,
    pub(super) deadline: Option<tokio::time::Instant>,
    pub(super) binding: GoalBinding,
    pub(super) session: SessionId,
    pub(super) objective: String,
}

impl RuntimeService {
    pub fn accounting_resolution_action(
        &self,
        resolution: &argentum_domain::goal_accounting::AccountingResolution,
    ) -> Result<argentum_security::LocalAuthorityAction, RuntimeError> {
        if resolution.binding.project_id != self.project_id
            || resolution.binding.workspace_key != self.workspace_key
        {
            return Err(StoreError::GoalAccounting("goal_scope_mismatch").into());
        }
        let bytes = serde_json::to_vec(resolution).map_err(|_| RuntimeError::ContextRecording)?;
        if bytes.len() > 16 * 1024 {
            return Err(StoreError::GoalAccounting("accounting_recovery_input_too_large").into());
        }
        Ok(
            argentum_security::LocalAuthorityAction::ResolveGoalAccounting {
                binding: argentum_domain::Sha256Digest(format!(
                    "{:x}",
                    sha2::Sha256::digest(bytes)
                )),
            },
        )
    }

    pub fn resolve_accounting_recovery(
        &self,
        resolution: &argentum_domain::goal_accounting::AccountingResolution,
        ticket: argentum_security::LocalAuthorityTicket,
    ) -> Result<RequestCharge, RuntimeError> {
        let action = self.accounting_resolution_action(resolution)?;
        let proof = self.verify_local_authority_ticket(ticket, &action)?;
        let generation = self.current_local_authority_generation()?;
        proof.authorize(&action, &generation)?;
        let result = self.store.resolve_accounting_recovery(resolution)?;
        self.publish_goal_scope(resolution.binding.session_id)?;
        Ok(result)
    }

    pub fn inspect_goal_accounting(
        &self,
    ) -> Result<argentum_domain::goal_accounting::GoalInspection, RuntimeError> {
        let session = self.session_id();
        let Some(goal) = self.store.goal(self.project_id, session)? else {
            return Ok(Default::default());
        };
        Ok(self
            .store
            .inspect_goal_accounting(self.project_id, session, goal.id)?)
    }
    pub fn configure_goal_limits(
        &self,
        goal_id: argentum_domain::GoalId,
        limits: [u64; 9],
    ) -> Result<Goal, RuntimeError> {
        let goal = self.store.configure_goal_limits(
            self.project_id,
            self.session_id(),
            goal_id,
            limits,
        )?;
        self.publish_goal_scope(goal.session_id)?;
        Ok(goal)
    }
    pub fn remove_goal_aggregate_limits(
        &self,
        goal_id: argentum_domain::GoalId,
    ) -> Result<Goal, RuntimeError> {
        let goal =
            self.store
                .remove_goal_aggregate_limits(self.project_id, self.session_id(), goal_id)?;
        self.publish_goal_scope(goal.session_id)?;
        Ok(goal)
    }
    pub(super) fn admit_goal_scope(
        &self,
        session: SessionId,
        run: RunId,
        binding: GoalBinding,
        maintenance: bool,
    ) -> Result<(), RuntimeError> {
        let started = tokio::time::Instant::now();
        let goal = self
            .store
            .goal(self.project_id, session)?
            .ok_or(RuntimeError::GoalMissing)?;
        let mut scopes = self
            .goal_runs
            .lock()
            .map_err(|_| RuntimeError::StateLockPoisoned)?;
        let admitted =
            self.store
                .admit_goal_run(self.project_id, session, binding, run, maintenance)?;
        scopes.insert(
            run,
            LiveGoalRun {
                started,
                deadline: admitted
                    .limits
                    .elapsed_seconds
                    .map(|seconds| started + std::time::Duration::from_secs(seconds)),
                binding,
                session,
                objective: goal.objective,
            },
        );
        drop(scopes);
        // Admission is already durable. A snapshot read failure cannot turn it
        // into a rejected attempt or leave it outside normal run settlement.
        let _ = self.publish_goal_scope(session);
        Ok(())
    }

    pub(super) fn goal_scope(&self, run: RunId) -> Result<Option<LiveGoalRun>, RuntimeError> {
        Ok(self
            .goal_runs
            .lock()
            .map_err(|_| RuntimeError::StateLockPoisoned)?
            .get(&run)
            .cloned())
    }

    pub(super) fn goal_guidance(&self, run: RunId) -> Result<Option<String>, RuntimeError> {
        let Some(scope) = self.goal_scope(run)? else {
            return Ok(None);
        };
        // Use the same bounded source/definition-bound evaluator as owner status.
        // A selected-session change cannot substitute another goal's evidence.
        let inspection =
            self.inspect_goal_supervision_for_session(scope.session, scope.binding.goal_id)?;
        let mut guidance = serde_json::json!({
            "kind": "explicit_goal_task_guidance",
            "goal_id": scope.binding.goal_id,
            "revision": scope.binding.revision,
            "objective": scope.objective,
            "precedence": "Current explicit user directions and runtime policy take precedence. This objective grants no authority and is not a completion assertion."
        });
        if !inspection.current_criteria.is_empty() {
            guidance["verification"] = serde_json::json!({
                "scope": "declared_fixed_checks_only",
                "current_criteria": inspection.current_criteria,
                "completion_current": inspection.completion_current,
                "completion_status": inspection.completion_status,
                "instruction": "Use current evidence for the final revision. Failed, missing, unknown or stale checks are not passing evidence. After relevant edits, an earlier pass is stale. Passing fixed checks does not prove every task requirement or preservation outside the requested edit scope; compare the actual edit with the user's constraints and report only established facts."
            });
        }
        serde_json::to_string(&guidance)
            .map(Some)
            .map_err(|_| RuntimeError::ContextRecording)
    }

    pub(super) fn check_goal_deadline(&self, run: RunId) -> Result<(), RuntimeError> {
        if self
            .goal_scope(run)?
            .and_then(|scope| scope.deadline)
            .is_some_and(|deadline| tokio::time::Instant::now() >= deadline)
        {
            self.cancellation_for_run(run)?.cancel();
            return Err(RuntimeError::GoalDeadline);
        }
        Ok(())
    }

    pub(super) fn reserve_goal_inference(
        &self,
        run: RunId,
        request: Uuid,
        purpose: &str,
        estimate: &argentum_domain::context_budget::RequestAccounting,
    ) -> Result<Option<RequestCharge>, RuntimeError> {
        self.check_goal_deadline(run)?;
        if self.goal_scope(run)?.is_none() {
            return Ok(None);
        }
        if self.cancellation_for_run(run)?.is_cancelled() {
            return Err(ProviderError::RequestCancelled.into());
        }
        let charge = self
            .store
            .reserve_goal_request(run, request, purpose, estimate)?;
        Ok(Some(charge))
    }

    pub(super) fn mark_goal_dispatch(
        &self,
        charge: Option<&RequestCharge>,
    ) -> Result<(), RuntimeError> {
        if let Some(charge) = charge {
            if let Err(error) = self.check_goal_deadline(charge.run_id) {
                self.store.goal_request_not_dispatched(charge.request_id)?;
                return Err(error);
            }
            if self.cancellation_for_run(charge.run_id)?.is_cancelled() {
                self.store.goal_request_not_dispatched(charge.request_id)?;
                return Err(ProviderError::RequestCancelled.into());
            }
            // Durable conservative dispatch intent immediately before polling the
            // adapter. A crash here remains unresolved; it never creates a refund.
            self.store.goal_request_dispatched(charge.request_id)?;
        }
        Ok(())
    }

    pub(super) fn settle_goal_inference(
        &self,
        charge: Option<&RequestCharge>,
        usage: Option<ObservedUsage>,
        invalid_usage: bool,
    ) -> Result<(), RuntimeError> {
        let Some(charge) = charge else {
            return Ok(());
        };
        let scope = self
            .goal_scope(charge.run_id)?
            .ok_or(RuntimeError::GoalAccountingUnresolved)?;
        let mut evidence = charge.clone();
        evidence.observed = usage.clone();
        evidence.state = argentum_domain::goal_accounting::ChargeState::Dispatched;
        evidence.usage_quality = if invalid_usage {
            "invalid_or_conflicting_final_usage"
        } else if let Some(usage) = usage.as_ref() {
            usage.quality_label()
        } else {
            "missing_final_usage"
        }
        .into();
        let pending = AppEvent::GoalRequestAccounting {
            session_id: scope.session,
            run_id: charge.run_id,
            charge: Box::new(evidence),
            persistence: "observed_settlement_pending".into(),
        };
        self.publish_transient(pending.clone());
        let settlement = if invalid_usage {
            self.store.settle_goal_invalid_usage(charge.request_id)
        } else {
            self.store.settle_goal_request(charge.request_id, usage)
        };
        let settled = match settlement {
            Ok(settled) => settled,
            Err(_) => {
                // An independent event write may retain the observation when a
                // charge-table write fails. Failure never triggers inference again.
                let _ = self.emit_for_run(pending, charge.run_id);
                return Err(RuntimeError::GoalAccountingUnresolved);
            }
        };
        self.publish_transient(AppEvent::GoalRequestAccounting {
            session_id: scope.session,
            run_id: charge.run_id,
            charge: Box::new(settled.clone()),
            persistence: "stored".into(),
        });
        if let Some(scope) = self.goal_scope(charge.run_id)? {
            self.publish_goal_scope(scope.session)?;
        }
        if settled.observed.is_none() {
            return Err(RuntimeError::GoalUsageUnresolved);
        }
        if settled.overrun_tokens > 0 {
            return Err(RuntimeError::GoalUsageOverrun);
        }
        Ok(())
    }

    pub(super) fn finish_goal_scope(
        &self,
        run: RunId,
        result: &Result<(), RuntimeError>,
    ) -> Result<(), RuntimeError> {
        if self.goal_scope(run)?.is_none() {
            return Ok(());
        }
        let reason = if self.check_goal_deadline(run).is_err() {
            "goal_deadline_exceeded"
        } else {
            match result {
                Ok(()) if self.cancellation_for_run(run)?.is_cancelled() => "cancelled",
                Ok(()) => "task_finished_goal_not_verified",
                Err(
                    RuntimeError::GoalUsageUnresolved
                    | RuntimeError::ProviderUsageUnresolved { .. },
                ) => "goal_usage_unresolved",
                Err(RuntimeError::GoalUsageOverrun) => "goal_usage_overrun",
                Err(RuntimeError::Store(StoreError::GoalAccounting(code))) => code,
                Err(_) => "failed",
            }
        };
        self.finish_goal_scope_reason(run, reason)
    }

    pub(super) fn finish_goal_scope_reason(
        &self,
        run: RunId,
        reason: &str,
    ) -> Result<(), RuntimeError> {
        let Some(scope) = self.goal_scope(run)? else {
            return Ok(());
        };
        let elapsed = u64::try_from(scope.started.elapsed().as_millis())
            .map_err(|_| StoreError::GoalAccounting("accounting_overflow"))?;
        self.store
            .finish_goal_run(run, elapsed, reason)
            .map_err(|_| RuntimeError::GoalAccountingUnresolved)?;
        self.publish_goal_scope(scope.session)?;
        self.goal_runs
            .lock()
            .map_err(|_| RuntimeError::StateLockPoisoned)?
            .remove(&run);
        Ok(())
    }

    fn publish_goal_scope(&self, session: SessionId) -> Result<(), RuntimeError> {
        self.publish_transient(AppEvent::GoalSnapshotLoaded {
            session_id: session,
            goal: self.store.goal(self.project_id, session)?.map(Box::new),
        });
        Ok(())
    }
}
