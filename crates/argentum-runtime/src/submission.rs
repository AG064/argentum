use super::*;

/// In-process destination binding, not a queue or authority protocol.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct SubmissionTarget {
    pub project_id: argentum_domain::ProjectId,
    pub session_id: SessionId,
    pub workspace: PathBuf,
}

#[derive(Clone, PartialEq, Eq)]
pub struct TaskSubmission {
    pub id: argentum_domain::TaskId,
    pub run_id: RunId,
    pub target: SubmissionTarget,
    pub prompt: String,
    pub skill: Option<SelectedSkill>,
    pub answer_contract: Option<AcceptedAnswerContract>,
}

impl std::fmt::Debug for TaskSubmission {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TaskSubmission")
            .field("id", &self.id)
            .field("run_id", &self.run_id)
            .field("target", &self.target)
            .field("prompt_bytes", &self.prompt.len())
            .field("skill", &self.skill.as_ref().map(|skill| &skill.name))
            .field(
                "answer_contract",
                &self
                    .answer_contract
                    .as_ref()
                    .map(|contract| &contract.source),
            )
            .finish()
    }
}

/// Returned only after the admitted run's existing cleanup released admission.
/// `Ok(())` alone is not successful task completion: cancellation also uses it.
#[derive(Debug)]
pub struct TaskSubmissionResult {
    pub id: argentum_domain::TaskId,
    pub run_id: RunId,
    pub admitted: bool,
    /// The terminal run lifecycle observed before admission was released.
    /// This distinguishes a cancelled run from `Ok(())`, which only means
    /// that runtime cleanup itself completed successfully.
    pub terminal_lifecycle: Option<TaskLifecycle>,
    pub result: Result<(), RuntimeError>,
}

impl RuntimeService {
    pub async fn submit_bound_task(&self, submission: TaskSubmission) -> TaskSubmissionResult {
        use tracing::instrument::WithSubscriber;
        self.diagnostics
            .record("command_started", "argentum.runtime", None, "INFO", None);
        let operation = self.submit_task_attempt(submission);
        let result = if self.diagnostics.enabled() {
            operation
                .with_subscriber(self.diagnostics.dispatcher())
                .await
        } else {
            operation.await
        };
        self.diagnostics.record(
            if result.result.is_ok() {
                "command_succeeded"
            } else {
                "command_failed"
            },
            "argentum.runtime",
            None,
            if result.result.is_ok() {
                "INFO"
            } else {
                "ERROR"
            },
            None,
        );
        result
    }

    pub async fn submit_durable_bound_task(
        &self,
        submission: TaskSubmission,
        scope: &CommandIdentityScope,
    ) -> TaskSubmissionResult {
        use tracing::instrument::WithSubscriber;
        self.diagnostics
            .record("command_started", "argentum.runtime", None, "INFO", None);
        let execution =
            self.durable_submission_execution_for_scope(scope, submission.answer_contract.as_ref());
        let result = match execution {
            Ok(execution) => {
                let operation = self.submit_task_attempt_scoped(submission, None, Some(execution));
                if self.diagnostics.enabled() {
                    operation
                        .with_subscriber(self.diagnostics.dispatcher())
                        .await
                } else {
                    operation.await
                }
            }
            Err(error) => TaskSubmissionResult {
                id: submission.id,
                run_id: submission.run_id,
                admitted: false,
                terminal_lifecycle: None,
                result: Err(error),
            },
        };
        self.diagnostics.record(
            if result.result.is_ok() {
                "command_succeeded"
            } else {
                "command_failed"
            },
            "argentum.runtime",
            None,
            if result.result.is_ok() {
                "INFO"
            } else {
                "ERROR"
            },
            None,
        );
        result
    }

    pub(super) async fn submit_task_attempt(
        &self,
        submission: TaskSubmission,
    ) -> TaskSubmissionResult {
        self.submit_task_attempt_scoped(submission, None, None)
            .await
    }

    pub async fn submit_goal_bound_task(
        &self,
        submission: TaskSubmission,
        binding: argentum_domain::goal_accounting::GoalBinding,
    ) -> TaskSubmissionResult {
        self.submit_task_attempt_scoped(submission, Some(binding), None)
            .await
    }

    async fn submit_task_attempt_scoped(
        &self,
        submission: TaskSubmission,
        goal: Option<argentum_domain::goal_accounting::GoalBinding>,
        durable_execution: Option<DurableSubmissionExecution>,
    ) -> TaskSubmissionResult {
        let TaskSubmission {
            id,
            run_id,
            target,
            prompt,
            skill,
            answer_contract,
        } = submission;
        let admission = (|| {
            if id.is_nil() || run_id.is_nil() {
                return Err(RuntimeError::SubmissionTargetChanged);
            }
            if self.shutdown_cancellation.is_cancelled() {
                return Err(RuntimeError::ShuttingDown);
            }
            if prompt.trim().is_empty() {
                return Err(RuntimeError::EmptyPrompt);
            }
            if prompt.len() > MAX_PROMPT_BYTES {
                return Err(
                    ProviderError::InvalidRequest("task input exceeded the safety limit").into(),
                );
            }
            if let Some(contract) = answer_contract.as_ref() {
                contract.verify_current(&self.workspace)?;
            }
            let target_key = target.workspace.to_string_lossy().replace('\\', "/");
            #[cfg(target_os = "windows")]
            let target_key = target_key.to_ascii_lowercase();
            if target.project_id != self.project_id || target_key != self.workspace_key {
                return Err(RuntimeError::SubmissionTargetChanged);
            }
            let _organization_guard = self
                .session_organization_admission
                .lock()
                .map_err(|_| RuntimeError::StateLockPoisoned)?;
            // Selection and admission share the selection lock. No dispatch-time
            // fallback to a different session is permitted.
            let selected = self
                .session_id
                .lock()
                .map_err(|_| RuntimeError::StateLockPoisoned)?;
            if *selected != target.session_id {
                return Err(RuntimeError::SubmissionTargetChanged);
            }
            if self
                .store
                .session_is_archived(self.project_id, target.session_id)?
            {
                return Err(RuntimeError::SubmissionTargetChanged);
            }
            if !self
                .pending_restores
                .lock()
                .map_err(|_| RuntimeError::StateLockPoisoned)?
                .is_empty()
            {
                return Err(RuntimeError::RestoreBusy);
            }
            // Reject unusable configuration before accepting the draft, creating
            // run ownership or appending conversation history. Request preparation
            // rechecks the same persisted declaration at its own boundary.
            self.selected_coding_readiness()?;
            self.acquire_session_run(target.session_id, run_id)?;
            if let Err(error) = self.store.claim_run_ownership(
                run_id,
                self.project_id,
                target.session_id,
                Some(id),
                None,
                argentum_domain::recovery::RecoveryRunKind::Task,
            ) {
                self.clear_run_state(run_id, target.session_id);
                return Err(error.into());
            }
            if let Some(binding) = goal {
                if let Err(error) = self.admit_goal_scope(target.session_id, run_id, binding, false)
                {
                    let _ = self.store.abandon_unadmitted_run_claim(run_id);
                    self.clear_run_state(run_id, target.session_id);
                    return Err(error);
                }
            }
            if let Err(error) = self.register_steering_run(&target, run_id) {
                let _ = self.store.abandon_unadmitted_run_claim(run_id);
                self.clear_run_state(run_id, target.session_id);
                return Err(error);
            }
            Ok(())
        })();
        if let Err(error) = admission {
            return TaskSubmissionResult {
                id,
                run_id,
                admitted: false,
                terminal_lifecycle: None,
                result: Err(error),
            };
        }
        let result = self
            .submit_task_for_run(
                target.session_id,
                run_id,
                id,
                prompt,
                skill,
                answer_contract,
                durable_execution,
            )
            .await;
        let terminal_lifecycle = self
            .run_lifecycles
            .lock()
            .ok()
            .and_then(|lifecycles| lifecycles.get(&run_id).copied())
            .filter(|lifecycle| lifecycle.is_terminal());
        let accounting = self.finish_goal_scope(run_id, &result);
        let retirement = if matches!(
            &result,
            Err(RuntimeError::ApprovalRequired(_) | RuntimeError::ApprovalDenied(_))
        ) {
            self.retire_run_approval_grants(run_id)
        } else {
            Ok(())
        };
        let (execution_state, ownership_outcome) = match terminal_lifecycle {
            Some(TaskLifecycle::Cancelled) => (
                argentum_domain::recovery::RecoveryExecutionState::Cancelled,
                "cancelled",
            ),
            Some(TaskLifecycle::Complete) if result.is_ok() => (
                argentum_domain::recovery::RecoveryExecutionState::Complete,
                "complete",
            ),
            _ => (
                argentum_domain::recovery::RecoveryExecutionState::Failed,
                "failed",
            ),
        };
        let steering = self.settle_steering(
            run_id,
            match terminal_lifecycle {
                Some(TaskLifecycle::Complete) if result.is_ok() => "run_completed_before_inclusion",
                Some(TaskLifecycle::Cancelled) => "run_cancelled_before_inclusion",
                _ => "run_failed_before_inclusion",
            },
        );
        let ownership =
            self.store
                .settle_active_run_ownership(run_id, execution_state, ownership_outcome);
        self.clear_run_state(run_id, target.session_id);
        TaskSubmissionResult {
            id,
            run_id,
            admitted: true,
            terminal_lifecycle,
            result: result
                .and(retirement)
                .and(accounting)
                .and(steering)
                .and(ownership.map_err(RuntimeError::from)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use argentum_security::{ApprovalPolicy, CapabilityBroker};

    fn runtime() -> (tempfile::TempDir, RuntimeService) {
        let directory = tempfile::tempdir().unwrap();
        let workspace = WorkspaceManager::new(
            CapabilityBroker::new(directory.path(), ApprovalPolicy::default()).unwrap(),
        );
        let service = RuntimeService::new(
            Store::open_in_memory().unwrap(),
            ProviderRegistry::default(),
            ToolRegistry::with_builtins(workspace.clone()),
            workspace,
        )
        .unwrap();
        // These identity tests need an admissible coding configuration. The
        // missing credential still fails the run without any network dispatch.
        service
            .store
            .save_provider_profile(
                service.project_id,
                &ProviderProfile {
                    id: "minimax".into(),
                    label: "Submission fixture".into(),
                    kind: ProviderKind::OpenAiCompatible,
                    endpoint: "https://api.minimax.io/v1/".into(),
                    model: "synthetic-test-model".into(),
                    selected: true,
                },
            )
            .unwrap();
        service
            .declare_model_budget("minimax", [204800, 196608, 4096, 4096, 1024])
            .unwrap();
        (directory, service)
    }

    #[tokio::test]
    async fn bound_submission_rejects_changed_destination_before_admission_and_keeps_exact_identity(
    ) {
        let (directory, service) = runtime();
        let target = SubmissionTarget {
            project_id: service.project_id,
            session_id: service.session_id(),
            workspace: service.workspace.root().to_path_buf(),
        };
        let id = Uuid::new_v4();
        service.dispatch(AppCommand::NewSession).await.unwrap();
        let rejected = service
            .submit_bound_task(TaskSubmission {
                id,
                run_id: Uuid::new_v4(),
                target: target.clone(),
                prompt: "bound original input\r\n".into(),
                skill: None,
                answer_contract: None,
            })
            .await;
        assert!(!rejected.admitted);
        assert!(matches!(
            rejected.result,
            Err(RuntimeError::SubmissionTargetChanged)
        ));
        assert!(service.active_session_runs.lock().unwrap().is_empty());
        assert!(!service
            .store
            .events_for_workspace(directory.path())
            .unwrap()
            .iter()
            .any(|event| matches!(event, AppEvent::TaskAccepted(_))));
        service
            .dispatch(AppCommand::SelectSession {
                session_id: target.session_id,
            })
            .await
            .unwrap();
        let run_id = Uuid::new_v4();
        let admitted = service
            .submit_bound_task(TaskSubmission {
                id,
                run_id,
                target: target.clone(),
                prompt: "bound original input\r\n".into(),
                skill: None,
                answer_contract: None,
            })
            .await;
        assert!(admitted.admitted);
        assert!(
            admitted.result.is_err(),
            "no provider credential configured in this fixture"
        );
        assert!(service.active_session_runs.lock().unwrap().is_empty());
        assert!(service.cancellation_tokens.lock().unwrap().is_empty());
        let events = service
            .store
            .events_for_workspace(directory.path())
            .unwrap();
        let tasks = events
            .iter()
            .filter_map(|event| {
                if let AppEvent::TaskAccepted(task) = event {
                    Some(task)
                } else {
                    None
                }
            })
            .collect::<Vec<_>>();
        assert_eq!(tasks.len(), 1);
        assert_eq!(tasks[0].id, id);
        assert_eq!(tasks[0].session_id, target.session_id);
        assert_eq!(tasks[0].prompt, "bound original input\r\n");
        assert!(events.iter().any(|event| matches!(event, AppEvent::RunStatusChanged { run_id: actual, lifecycle: TaskLifecycle::Failed } if *actual == run_id)));
    }

    #[tokio::test]
    async fn bound_submission_rejects_an_archived_session_without_admission() {
        let (_directory, service) = runtime();
        let archived = SubmissionTarget {
            project_id: service.project_id,
            session_id: service.session_id(),
            workspace: service.workspace.root().to_path_buf(),
        };
        service.dispatch(AppCommand::NewSession).await.unwrap();
        service
            .dispatch(AppCommand::SetSessionArchived {
                session_id: archived.session_id,
                archived: true,
            })
            .await
            .expect("archive nonselected session");

        let result = service
            .submit_bound_task(TaskSubmission {
                id: Uuid::new_v4(),
                run_id: Uuid::new_v4(),
                target: archived,
                prompt: "must not reach an archived session".into(),
                skill: None,
                answer_contract: None,
            })
            .await;
        assert!(!result.admitted);
        assert!(matches!(
            result.result,
            Err(RuntimeError::SubmissionTargetChanged)
        ));
        assert!(service.active_session_runs.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn bound_submission_rejects_foreign_project_workspace_and_active_run_identity() {
        let (_directory, service) = runtime();
        let target = SubmissionTarget {
            project_id: service.project_id,
            session_id: service.session_id(),
            workspace: service.workspace.root().to_path_buf(),
        };
        for bad_target in [
            SubmissionTarget {
                project_id: Uuid::new_v4(),
                ..target.clone()
            },
            SubmissionTarget {
                workspace: target.workspace.join("other"),
                ..target.clone()
            },
        ] {
            let result = service
                .submit_bound_task(TaskSubmission {
                    id: Uuid::new_v4(),
                    run_id: Uuid::new_v4(),
                    target: bad_target,
                    prompt: "not admitted".into(),
                    skill: None,
                    answer_contract: None,
                })
                .await;
            assert!(!result.admitted);
            assert!(matches!(
                result.result,
                Err(RuntimeError::SubmissionTargetChanged)
            ));
        }
        let existing_run = Uuid::new_v4();
        #[cfg(target_os = "windows")]
        {
            let same_workspace = SubmissionTarget {
                workspace: PathBuf::from(target.workspace.to_string_lossy().to_ascii_lowercase()),
                ..target.clone()
            };
            let result = service
                .submit_bound_task(TaskSubmission {
                    id: Uuid::new_v4(),
                    run_id: Uuid::new_v4(),
                    target: same_workspace,
                    prompt: "same canonical workspace identity".into(),
                    skill: None,
                    answer_contract: None,
                })
                .await;
            assert!(
                result.admitted,
                "Windows casing must follow the existing workspace identity key"
            );
        }
        service
            .acquire_session_run(target.session_id, existing_run)
            .unwrap();
        service.dispatch(AppCommand::NewSession).await.unwrap();
        let result = service
            .submit_bound_task(TaskSubmission {
                id: Uuid::new_v4(),
                run_id: existing_run,
                target: SubmissionTarget {
                    session_id: service.session_id(),
                    ..target.clone()
                },
                prompt: "duplicate active run identity".into(),
                skill: None,
                answer_contract: None,
            })
            .await;
        assert!(!result.admitted);
        assert!(matches!(result.result, Err(RuntimeError::SessionRunActive)));
        service.clear_run_state(existing_run, target.session_id);
    }
}
