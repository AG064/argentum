use super::*;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApprovalFailureContext {
    pub run_id: RunId,
    pub tool_call_id: Option<argentum_domain::ToolCallId>,
    pub approval_id: argentum_domain::ApprovalId,
}

impl ApprovalFailureContext {
    pub(super) fn new(
        approval: &ApprovalRequest,
        tool_call_id: Option<argentum_domain::ToolCallId>,
    ) -> Self {
        Self {
            run_id: approval.run_id,
            tool_call_id,
            approval_id: approval.id,
        }
    }
}

pub(super) struct ApprovalChannel {
    pub closed: CancellationToken,
}

/// Lifetime of an existing trusted interactive owner responder, not an approval
/// grant or a transport. Only trusted host wiring creates this nonserializable
/// guard. Dropping it retires unclaimed approvals and wakes their continuations.
pub struct InteractiveApprovalResponder {
    runtime: RuntimeService,
    channel: Arc<ApprovalChannel>,
}

impl Drop for InteractiveApprovalResponder {
    fn drop(&mut self) {
        self.channel.closed.cancel();
        let pending = match self.runtime.pending_approvals.lock() {
            Ok(mut entries) => {
                let ids = entries
                    .iter()
                    .filter_map(|(id, entry)| {
                        Arc::ptr_eq(&entry.responder, &self.channel).then_some(*id)
                    })
                    .collect::<Vec<_>>();
                ids.into_iter()
                    .filter_map(|id| entries.remove(&id))
                    .collect::<Vec<_>>()
            }
            Err(_) => return,
        };
        for entry in pending {
            // Cleanup still signals the waiter if writing an event fails.
            let _ = self.runtime.settle_unavailable_approval(entry);
        }
        let restores = match self.runtime.pending_restores.lock() {
            Ok(mut entries) => {
                let ids = entries
                    .iter()
                    .filter_map(|(id, entry)| {
                        Arc::ptr_eq(&entry.responder, &self.channel).then_some(*id)
                    })
                    .collect::<Vec<_>>();
                ids.into_iter()
                    .filter_map(|id| entries.remove(&id).map(|entry| (id, entry.source_run_id)))
                    .collect::<Vec<_>>()
            }
            Err(_) => return,
        };
        for (approval_id, run_id) in restores {
            // Restores refer to historical runs. Do not rewrite that run's outcome.
            let _ = self.runtime.emit_for_run(
                AppEvent::ApprovalResolved {
                    approval_id,
                    approved: false,
                },
                run_id,
            );
        }
    }
}

pub(super) fn unavailable(
    approval: &ApprovalRequest,
    call_id: Option<argentum_domain::ToolCallId>,
) -> RuntimeError {
    RuntimeError::ApprovalRequired(ApprovalFailureContext::new(approval, call_id))
}

impl RuntimeService {
    pub(super) fn register_pending_restore(
        &self,
        approval: ApprovalRequest,
        pending: PendingRestore,
    ) -> Result<(), RuntimeError> {
        let mut entries = self
            .pending_restores
            .lock()
            .map_err(|_| RuntimeError::StateLockPoisoned)?;
        if pending.responder.closed.is_cancelled() {
            return Err(unavailable(&approval, None));
        }
        let run_id = approval.run_id;
        entries.insert(approval.id, pending);
        self.emit_for_run(AppEvent::ApprovalRequested(approval), run_id)
    }
    pub(super) fn register_pending_approval(
        &self,
        pending: PendingTool,
        model: bool,
    ) -> Result<(), RuntimeError> {
        let mut entries = self
            .pending_approvals
            .lock()
            .map_err(|_| RuntimeError::StateLockPoisoned)?;
        if pending.responder.closed.is_cancelled() {
            return Err(unavailable(
                &pending.approval,
                Some(pending.request.call_id),
            ));
        }
        let approval = pending.approval.clone();
        let run_id = approval.run_id;
        entries.insert(approval.id, pending);
        // Publication precedes any close/approve taking this continuation. This
        // keeps Requested/Waiting before Resolved/terminal events in close races.
        self.emit(AppEvent::ApprovalRequested(approval))?;
        if model {
            self.emit(AppEvent::RunStatusChanged {
                run_id,
                lifecycle: TaskLifecycle::WaitingForApproval,
            })?;
        }
        Ok(())
    }

    /// Trusted host wiring only. This declares a live interactive responder; it
    /// does not mint authority and is deliberately absent from AppCommand/JSONL.
    /// Headless entry points leave the responder disconnected.
    pub fn connect_interactive_approval_responder(
        &self,
    ) -> Result<InteractiveApprovalResponder, RuntimeError> {
        let mut current = self
            .approval_responder
            .lock()
            .map_err(|_| RuntimeError::StateLockPoisoned)?;
        if current
            .as_ref()
            .is_some_and(|channel| !channel.closed.is_cancelled())
        {
            return Err(RuntimeError::ExecutionPolicyBusy);
        }
        let channel = Arc::new(ApprovalChannel {
            closed: CancellationToken::new(),
        });
        *current = Some(channel.clone());
        Ok(InteractiveApprovalResponder {
            runtime: self.clone(),
            channel,
        })
    }

    pub(super) fn require_approval_responder(
        &self,
        approval: &ApprovalRequest,
        call_id: Option<argentum_domain::ToolCallId>,
    ) -> Result<Arc<ApprovalChannel>, RuntimeError> {
        self.approval_responder
            .lock()
            .map_err(|_| RuntimeError::StateLockPoisoned)?
            .as_ref()
            .filter(|channel| !channel.closed.is_cancelled())
            .cloned()
            .ok_or_else(|| unavailable(approval, call_id))
    }

    pub(super) fn settle_unavailable_approval(
        &self,
        pending: PendingTool,
    ) -> Result<(), RuntimeError> {
        let PendingTool {
            request,
            approval,
            continuation,
            ..
        } = pending;
        let cancelled = self
            .cancellation_tokens
            .lock()
            .map_err(|_| RuntimeError::StateLockPoisoned)?
            .get(&request.run_id)
            .is_some_and(CancellationToken::is_cancelled);
        let context = ApprovalFailureContext::new(&approval, Some(request.call_id));
        let mut first_error = None;
        for event in [
            AppEvent::ApprovalResolved {
                approval_id: approval.id,
                approved: false,
            },
            AppEvent::ToolFinished(ToolTrace {
                id: request.call_id,
                run_id: request.run_id,
                tool_id: approval.tool_id,
                summary: "Approval responder unavailable".into(),
                result: if cancelled {
                    ToolResultState::Cancelled
                } else {
                    ToolResultState::Failed
                },
                duration_ms: Some(0),
            }),
        ] {
            if let Err(error) = self.emit_for_run(event, request.run_id) {
                first_error.get_or_insert(error);
            }
        }
        match continuation {
            PendingToolContinuation::Model { sender, .. } => {
                let _ = sender.send(if cancelled {
                    ModelToolStep::Cancelled
                } else {
                    ModelToolStep::Failed(RuntimeError::ApprovalRequired(context))
                });
            }
            PendingToolContinuation::Supervisor { sender, .. } => {
                let error = if cancelled {
                    ProviderError::RequestCancelled.into()
                } else {
                    RuntimeError::ApprovalRequired(context)
                };
                let _ = sender.send(Err(error));
            }
            PendingToolContinuation::Manual => {
                let result = if cancelled {
                    self.emit(AppEvent::RunStatusChanged {
                        run_id: request.run_id,
                        lifecycle: TaskLifecycle::Cancelled,
                    })
                } else {
                    self.fail_run(request.run_id, &RuntimeError::ApprovalRequired(context))
                };
                if let Err(error) = result {
                    first_error.get_or_insert(error);
                }
                if let Err(error) = self.retire_run_approval_grants(request.run_id) {
                    first_error.get_or_insert(error);
                }
                self.clear_run_state(
                    request.run_id,
                    self.session_for_run(request.run_id)
                        .unwrap_or_else(|| self.session_id()),
                );
            }
        }
        first_error.map_or(Ok(()), Err)
    }

    pub(super) fn retire_run_approval_grants(&self, run_id: RunId) -> Result<(), RuntimeError> {
        for grant in self.store.run_grants(self.project_id, run_id)?.into_iter().filter(|grant|
            grant.revoked_at.is_none() && grant.scope == GrantScope::Run
            && matches!(&grant.subject, GrantSubject::Run { run_id: subject } if *subject == run_id)) {
            let at = now();
            if self.store.revoke_approval_grant(self.project_id, grant.id, at)?.is_some() {
                self.emit_for_run(AppEvent::ApprovalGrantRevoked { grant_id: grant.id, revoked_at: at }, run_id)?;
            }
        }
        Ok(())
    }
}
