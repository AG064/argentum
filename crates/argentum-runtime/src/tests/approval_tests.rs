use super::*;
use argentum_domain::SandboxScope;
use argentum_tools::{AgentTool, ToolContext, ToolError, ToolResult};
use std::time::Duration;

const WATCHDOG: Duration = Duration::from_secs(5);

#[tokio::test]
async fn f07a_goal_edit_publishes_the_committed_revision() {
    let fixture = Fixture::new(false).await;
    let mut events = fixture.owner.subscribe();
    for budget in [1000, 2000] {
        fixture
            .owner
            .dispatch(AppCommand::SetGoal {
                objective: "Preserve the authoritative goal revision".into(),
                token_budget: Some(budget),
                tool_budget: None,
                time_budget_seconds: None,
            })
            .await
            .unwrap();
        let stored = fixture.owner.runtime.goal().unwrap().unwrap();
        let published = drain_events(&mut events)
            .into_iter()
            .find_map(|event| match event {
                AppEvent::GoalSnapshotLoaded {
                    goal: Some(goal), ..
                } => Some(goal),
                _ => None,
            })
            .unwrap();
        assert_eq!(*published, stored);
    }
    assert!(fixture.requests.lock().unwrap().is_empty());
}

#[tokio::test]
async fn f07a_tool_limit_after_owner_approval_stops_without_another_model_request() {
    let fixture = Fixture::new(false).await;
    fixture
        .report_usage
        .store(true, std::sync::atomic::Ordering::Release);
    fixture.script.lock().unwrap().insert(
        0,
        call(
            "first-read",
            "read_text",
            serde_json::json!({"path":"fixture.txt"}),
        ),
    );
    fixture
        .owner
        .dispatch(AppCommand::SetGoal {
            objective: "A tool admission failure must stop this run".into(),
            token_budget: Some(500000),
            tool_budget: Some(1),
            time_budget_seconds: Some(60),
        })
        .await
        .unwrap();
    let goal = fixture.owner.runtime.goal().unwrap().unwrap();
    let runtime = fixture.owner.runtime.clone();
    let submission = TaskSubmission {
        id: Uuid::new_v4(),
        run_id: Uuid::new_v4(),
        target: SubmissionTarget {
            project_id: goal.project_id,
            session_id: goal.session_id,
            workspace: fixture.owner.runtime.workspace_root().to_path_buf(),
        },
        prompt: "Read the file, then request an edit".into(),
        skill: None,
        answer_contract: None,
    };
    let binding = argentum_domain::goal_accounting::GoalBinding {
        goal_id: goal.id,
        revision: goal.accounting.unwrap().revision,
    };
    let mut events = fixture.owner.subscribe();
    let run =
        tokio::spawn(async move { runtime.submit_goal_bound_task(submission, binding).await });
    let approval = Fixture::approval(&mut events).await;
    fixture
        .owner
        .dispatch(AppCommand::ApproveTool {
            approval_id: approval.id,
            scope: ApprovalScope::Once,
        })
        .await
        .unwrap();
    let result = tokio::time::timeout(WATCHDOG, run).await.unwrap().unwrap();
    assert!(
        matches!(
            result.result,
            Err(RuntimeError::Store(StoreError::GoalAccounting(
                "goal_tool_limit"
            )))
        ),
        "unexpected terminal result: {:?}",
        result.result
    );
    fixture.assert_settled(TaskLifecycle::Failed);
    assert_eq!(fixture.requests.lock().unwrap().len(), 1);
    assert_eq!(
        std::fs::read(fixture.directory.path().join("fixture.txt")).unwrap(),
        b"before\r\n"
    );
    let goal = fixture.owner.runtime.goal().unwrap().unwrap();
    assert_eq!(goal.tokens_used, 40);
    assert_eq!(goal.tools_used, 1);
    assert_eq!(
        goal.accounting.unwrap().stop_reason.as_deref(),
        Some("goal_tool_limit")
    );
}

#[tokio::test]
async fn f07a_deadline_retires_unclaimed_approval_and_drains_claimed_execution() {
    for claimed in [false, true] {
        let fixture = Fixture::new(true).await;
        fixture
            .report_usage
            .store(true, std::sync::atomic::Ordering::Release);
        fixture
            .owner
            .dispatch(AppCommand::SetGoal {
                objective: "Deadline constrained explicit work".into(),
                token_budget: Some(500000),
                tool_budget: Some(10),
                time_budget_seconds: Some(60),
            })
            .await
            .unwrap();
        let goal = fixture.owner.runtime.goal().unwrap().unwrap();
        let goal = fixture
            .owner
            .runtime
            .configure_goal_limits(goal.id, [500000, 100, 100, 10, 60, 500000, 100, 10, 1])
            .unwrap();
        let runtime = fixture.owner.runtime.clone();
        let submission = TaskSubmission {
            id: Uuid::new_v4(),
            run_id: Uuid::new_v4(),
            target: SubmissionTarget {
                project_id: goal.project_id,
                session_id: goal.session_id,
                workspace: fixture.owner.runtime.workspace_root().to_path_buf(),
            },
            prompt: "Perform only the explicitly approved edit".into(),
            skill: None,
            answer_contract: None,
        };
        let run_id = submission.run_id;
        let binding = argentum_domain::goal_accounting::GoalBinding {
            goal_id: goal.id,
            revision: goal.accounting.unwrap().revision,
        };
        let mut events = fixture.owner.subscribe();
        let run =
            tokio::spawn(async move { runtime.submit_goal_bound_task(submission, binding).await });
        let approval = Fixture::approval(&mut events).await;
        let cancellation = fixture.owner.runtime.cancellation_for_run(run_id).unwrap();
        let approving = if claimed {
            let generation = fixture
                .owner
                .platform_authority
                .current_generation()
                .unwrap();
            let ticket = fixture
                .owner
                .issuer
                .issue(
                    LocalAuthorityAction::ApproveTool {
                        approval_id: approval.id,
                        scope: ApprovalScope::Once,
                    },
                    generation,
                )
                .unwrap();
            let runtime = fixture.owner.runtime.clone();
            let approving = tokio::spawn(async move {
                runtime
                    .dispatch_with_local_authority(
                        AppCommand::ApproveTool {
                            approval_id: approval.id,
                            scope: ApprovalScope::Once,
                        },
                        ticket,
                    )
                    .await
            });
            tokio::time::timeout(WATCHDOG, fixture.gate.entered.notified())
                .await
                .unwrap();
            Some(approving)
        } else {
            None
        };
        tokio::time::timeout(WATCHDOG, cancellation.cancelled())
            .await
            .unwrap();
        if let Some(approving) = approving {
            assert!(
                !run.is_finished(),
                "claimed execution must drain before terminal settlement"
            );
            fixture.gate.release.notify_one();
            tokio::time::timeout(WATCHDOG, approving)
                .await
                .unwrap()
                .unwrap()
                .unwrap();
        } else {
            fixture
                .owner
                .dispatch(AppCommand::ApproveTool {
                    approval_id: approval.id,
                    scope: ApprovalScope::Once,
                })
                .await
                .unwrap();
        }
        let result = tokio::time::timeout(WATCHDOG, run).await.unwrap().unwrap();
        assert!(result.admitted);
        assert!(matches!(result.result, Err(RuntimeError::GoalDeadline)));
        fixture.assert_settled(TaskLifecycle::Failed);
        assert_eq!(
            std::fs::read(fixture.directory.path().join("fixture.txt")).unwrap(),
            b"before\r\n"
        );
        assert_eq!(fixture.requests.lock().unwrap().len(), 1);
        let goal = fixture.owner.runtime.goal().unwrap().unwrap();
        assert_eq!(goal.tokens_used, 40);
        assert_eq!(goal.tools_used, u32::from(claimed));
        assert_eq!(
            goal.accounting.unwrap().stop_reason.as_deref(),
            Some("goal_deadline_exceeded")
        );
    }
}

struct ScriptProvider {
    requests: Arc<Mutex<Vec<ModelRequest>>>,
    script: Arc<Mutex<Vec<ModelToolCall>>>,
    report_usage: Arc<std::sync::atomic::AtomicBool>,
}

#[async_trait]
impl ModelProvider for ScriptProvider {
    fn id(&self) -> &'static str {
        "f03-script"
    }
    fn status(&self) -> ProviderStatus {
        test_provider_status(self.id())
    }
    async fn stream(
        &self,
        request: ModelRequest,
        sender: mpsc::Sender<ProviderEvent>,
    ) -> Result<(), ProviderError> {
        let first = request.tool_exchanges.is_empty();
        self.requests.lock().unwrap().push(request);
        if self.report_usage.load(std::sync::atomic::Ordering::Acquire) {
            sender
                .send(ProviderEvent::UsageObservation(
                    argentum_domain::goal_accounting::ObservedUsage {
                        input_tokens: None,
                        output_tokens: None,
                        total_tokens: 40,
                        cached_input_tokens: None,
                        reasoning_tokens: None,
                    },
                ))
                .await
                .map_err(|_| ProviderError::ConsumerClosed)?;
        }
        let calls = if first {
            self.script.lock().unwrap().clone()
        } else {
            Vec::new()
        };
        for call in calls {
            sender
                .send(ProviderEvent::ToolCall(call))
                .await
                .map_err(|_| ProviderError::ConsumerClosed)?;
        }
        sender
            .send(ProviderEvent::Completed)
            .await
            .map_err(|_| ProviderError::ConsumerClosed)
    }
}

#[derive(Clone)]
struct Gate {
    entered: Arc<Notify>,
    release: Arc<Notify>,
}

struct GatedEdit {
    delegate: ToolRegistry,
    gate: Gate,
}

#[async_trait]
impl AgentTool for GatedEdit {
    fn descriptor(&self) -> ToolDescriptor {
        self.delegate
            .descriptors()
            .into_iter()
            .find(|tool| tool.id == "edit_text")
            .unwrap()
    }
    async fn execute(
        &self,
        request: ToolRequest,
        context: ToolContext,
    ) -> Result<ToolResult, ToolError> {
        self.gate.entered.notify_one();
        self.gate.release.notified().await;
        self.delegate.execute(request, context).await
    }
}

struct Fixture {
    directory: tempfile::TempDir,
    owner: TestRuntime,
    requests: Arc<Mutex<Vec<ModelRequest>>>,
    script: Arc<Mutex<Vec<ModelToolCall>>>,
    gate: Gate,
    report_usage: Arc<std::sync::atomic::AtomicBool>,
}

impl Fixture {
    async fn new(gated: bool) -> Self {
        let directory = tempfile::tempdir().unwrap();
        std::fs::write(directory.path().join("fixture.txt"), "before\r\n").unwrap();
        let workspace = WorkspaceManager::new(
            CapabilityBroker::new(directory.path(), ApprovalPolicy::default()).unwrap(),
        );
        let digest = workspace
            .read_text_range_with_scope("fixture.txt", 0, None, None, SandboxScope::WorkspaceWrite)
            .unwrap()
            .sha256;
        let requests = Arc::new(Mutex::new(Vec::new()));
        let script = Arc::new(Mutex::new(vec![ModelToolCall { id: "f03-edit".into(), name: "edit_text".into(),
            arguments: serde_json::json!({"path":"fixture.txt","expected_sha256":digest,"old_text":"before","new_text":"after"}).to_string() }]));
        let mut providers = ProviderRegistry::default();
        let report_usage = Arc::new(std::sync::atomic::AtomicBool::new(false));
        providers.register(ScriptProvider {
            requests: requests.clone(),
            script: script.clone(),
            report_usage: report_usage.clone(),
        });
        let mut tools = ToolRegistry::with_builtins(workspace.clone());
        let gate = Gate {
            entered: Arc::new(Notify::new()),
            release: Arc::new(Notify::new()),
        };
        if gated {
            tools.register(GatedEdit {
                delegate: tools.clone(),
                gate: gate.clone(),
            });
        }
        let owner = test_runtime(
            Store::open_in_memory().unwrap(),
            providers,
            tools,
            workspace,
        );
        configure_selected_provider(&owner, "f03-script").await;
        Self {
            directory,
            owner,
            requests,
            script,
            gate,
            report_usage,
        }
    }

    fn start(&self) -> tokio::task::JoinHandle<Result<(), RuntimeError>> {
        let runtime = self.owner.runtime.clone();
        tokio::spawn(async move {
            runtime
                .dispatch(AppCommand::SubmitTask {
                    prompt: "F03 fixture".into(),
                })
                .await
        })
    }

    fn assert_settled(&self, expected: TaskLifecycle) -> RunId {
        assert!(self.owner.pending_approvals.lock().unwrap().is_empty());
        assert!(self.owner.pending_restores.lock().unwrap().is_empty());
        assert!(self.owner.active_session_runs.lock().unwrap().is_empty());
        assert!(self.owner.run_sessions.lock().unwrap().is_empty());
        assert!(self.owner.run_lifecycles.lock().unwrap().is_empty());
        assert!(self.owner.cancellation_tokens.lock().unwrap().is_empty());
        assert!(self.owner.module_run_leases.lock().unwrap().is_empty());
        let events = self
            .owner
            .store
            .events_for_workspace(self.directory.path())
            .unwrap();
        let terminal = events
            .iter()
            .filter_map(|event| match event {
                AppEvent::RunStatusChanged { run_id, lifecycle } if lifecycle.is_terminal() => {
                    Some((*run_id, *lifecycle))
                }
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(terminal.len(), 1);
        assert_eq!(terminal[0].1, expected);
        assert!(!self.directory.path().join("later.txt").exists());
        terminal[0].0
    }

    async fn approval(events: &mut broadcast::Receiver<AppEvent>) -> ApprovalRequest {
        tokio::time::timeout(WATCHDOG, async {
            loop {
                if let AppEvent::ApprovalRequested(approval) = events.recv().await.unwrap() {
                    break approval;
                }
            }
        })
        .await
        .expect("approval event watchdog")
    }

    async fn finish(
        run: tokio::task::JoinHandle<Result<(), RuntimeError>>,
    ) -> Result<(), RuntimeError> {
        tokio::time::timeout(WATCHDOG, run)
            .await
            .expect("run settlement watchdog")
            .unwrap()
    }
}

fn call(id: &str, name: &str, args: serde_json::Value) -> ModelToolCall {
    ModelToolCall {
        id: id.into(),
        name: name.into(),
        arguments: args.to_string(),
    }
}

#[tokio::test]
async fn no_responder_is_terminal_without_waiters_or_subsequent_effects() {
    let mut fixture = Fixture::new(false).await;
    drop(fixture.owner.approval_responder.take());
    fixture.script.lock().unwrap().push(call(
        "later",
        "write_text",
        serde_json::json!({"path":"later.txt","content":"must not run"}),
    ));
    assert!(matches!(
        Fixture::finish(fixture.start()).await,
        Err(RuntimeError::ApprovalRequired(_))
    ));
    fixture.assert_settled(TaskLifecycle::Failed);
    assert_eq!(
        std::fs::read(fixture.directory.path().join("fixture.txt")).unwrap(),
        b"before\r\n"
    );
    assert_eq!(fixture.requests.lock().unwrap().len(), 1);
}

#[tokio::test]
async fn live_owner_approves_once_and_wrong_or_late_replies_do_not_execute() {
    let fixture = Fixture::new(false).await;
    let mut events = fixture.owner.subscribe();
    let run = fixture.start();
    let approval = Fixture::approval(&mut events).await;
    let generation = fixture
        .owner
        .platform_authority
        .current_generation()
        .unwrap();
    let wrong = fixture
        .owner
        .issuer
        .issue(
            LocalAuthorityAction::ApproveTool {
                approval_id: Uuid::new_v4(),
                scope: ApprovalScope::Once,
            },
            generation,
        )
        .unwrap();
    assert!(fixture
        .owner
        .runtime
        .dispatch_with_local_authority(
            AppCommand::ApproveTool {
                approval_id: approval.id,
                scope: ApprovalScope::Once
            },
            wrong
        )
        .await
        .is_err());
    assert_eq!(fixture.owner.pending_approvals.lock().unwrap().len(), 1);
    assert_eq!(
        std::fs::read(fixture.directory.path().join("fixture.txt")).unwrap(),
        b"before\r\n"
    );
    fixture
        .owner
        .dispatch(AppCommand::ApproveTool {
            approval_id: approval.id,
            scope: ApprovalScope::Once,
        })
        .await
        .unwrap();
    Fixture::finish(run).await.unwrap();
    fixture
        .owner
        .dispatch(AppCommand::ApproveTool {
            approval_id: approval.id,
            scope: ApprovalScope::Once,
        })
        .await
        .unwrap();
    let run_id = fixture.assert_settled(TaskLifecycle::Complete);
    assert_eq!(
        fixture
            .owner
            .store
            .change_set(fixture.owner.project_id, run_id)
            .unwrap()
            .unwrap()
            .files
            .len(),
        1
    );
    assert_eq!(
        std::fs::read(fixture.directory.path().join("fixture.txt")).unwrap(),
        b"after\r\n"
    );
    assert_eq!(fixture.requests.lock().unwrap().len(), 2);
}

#[tokio::test]
async fn f05_changed_instruction_during_owner_approval_defers_before_effect() {
    let fixture = Fixture::new(false).await;
    let instruction = fixture.directory.path().join("AGENTS.md");
    std::fs::write(&instruction, "F05_APPROVAL_OLD").unwrap();
    let mut events = fixture.owner.subscribe();
    let run = fixture.start();
    let approval = Fixture::approval(&mut events).await;
    std::fs::write(&instruction, "F05_APPROVAL_NEW").unwrap();
    fixture
        .owner
        .dispatch(AppCommand::ApproveTool {
            approval_id: approval.id,
            scope: ApprovalScope::Once,
        })
        .await
        .unwrap();
    Fixture::finish(run).await.unwrap();
    fixture.assert_settled(TaskLifecycle::Complete);
    assert_eq!(
        std::fs::read(fixture.directory.path().join("fixture.txt")).unwrap(),
        b"before\r\n"
    );
    let requests = fixture.requests.lock().unwrap();
    assert_eq!(requests.len(), 2);
    assert!(requests[0]
        .system
        .as_ref()
        .unwrap()
        .contains("F05_APPROVAL_OLD"));
    assert!(requests[1]
        .system
        .as_ref()
        .unwrap()
        .contains("F05_APPROVAL_NEW"));
    assert!(!requests[1]
        .system
        .as_ref()
        .unwrap()
        .contains("F05_APPROVAL_OLD"));
    assert!(requests[1].tool_exchanges[0].results[0]
        .content
        .contains("instruction_context_refresh_required"));
    let durable = serde_json::to_string(
        &fixture
            .owner
            .store
            .events_for_workspace(fixture.directory.path())
            .unwrap(),
    )
    .unwrap();
    assert!(!durable.contains("F05_APPROVAL_"));
}

#[tokio::test]
async fn f05_discovered_scopes_do_not_leak_to_later_sessions_or_workspaces() {
    let fixture = Fixture::new(false).await;
    std::fs::create_dir(fixture.directory.path().join("nested")).unwrap();
    std::fs::write(
        fixture.directory.path().join("AGENTS.md"),
        "F05_SHARED_ROOT",
    )
    .unwrap();
    std::fs::write(
        fixture.directory.path().join("nested/AGENTS.md"),
        "F05_RUN_LOCAL_SCOPE",
    )
    .unwrap();
    std::fs::write(fixture.directory.path().join("nested/file.txt"), "readable").unwrap();
    *fixture.script.lock().unwrap() = vec![ModelToolCall {
        id: "nested-read".into(),
        name: "read_text".into(),
        arguments: r#"{"path":"nested/file.txt"}"#.into(),
    }];
    Fixture::finish(fixture.start()).await.unwrap();
    assert!(fixture.requests.lock().unwrap()[1]
        .system
        .as_ref()
        .unwrap()
        .contains("F05_RUN_LOCAL_SCOPE"));
    fixture.script.lock().unwrap().clear();
    fixture
        .owner
        .dispatch(AppCommand::NewSession)
        .await
        .unwrap();
    Fixture::finish(fixture.start()).await.unwrap();
    {
        let requests = fixture.requests.lock().unwrap();
        assert_eq!(requests.len(), 3);
        assert!(requests[2]
            .system
            .as_ref()
            .unwrap()
            .contains("F05_SHARED_ROOT"));
        assert!(!requests[2]
            .system
            .as_ref()
            .unwrap()
            .contains("F05_RUN_LOCAL_SCOPE"));
    }
    let foreign = Fixture::new(false).await;
    foreign.script.lock().unwrap().clear();
    Fixture::finish(foreign.start()).await.unwrap();
    assert!(!foreign.requests.lock().unwrap()[0]
        .system
        .as_ref()
        .unwrap()
        .contains("F05_SHARED_ROOT"));
}

#[tokio::test]
async fn f05_observe_guidance_and_context_access_follow_effective_policy() {
    let fixture = Fixture::new(false).await;
    fixture.script.lock().unwrap().clear();
    fixture
        .owner
        .dispatch(AppCommand::SelectExecutionProfile {
            profile_id: "observe".into(),
        })
        .await
        .unwrap();
    Fixture::finish(fixture.start()).await.unwrap();
    {
        let requests = fixture.requests.lock().unwrap();
        assert_eq!(requests[0].tools.len(), 7);
        assert!(requests[0]
            .system
            .as_ref()
            .unwrap()
            .contains("sandbox: ReadOnly"));
        assert!(!requests[0]
            .tools
            .iter()
            .any(|tool| tool.name == "edit_text"));
    }
    fixture
        .owner
        .dispatch(AppCommand::SetHarnessCapabilityEnabled {
            capability_id: "tool.read-text".into(),
            enabled: false,
        })
        .await
        .unwrap();
    assert!(matches!(
        fixture.owner.instruction_preview(""),
        Err(RuntimeError::Instructions(_))
    ));
    assert!(matches!(
        Fixture::finish(fixture.start()).await,
        Err(RuntimeError::Instructions(_))
    ));
    assert_eq!(fixture.requests.lock().unwrap().len(), 1);
}

#[tokio::test]
async fn rejection_closure_and_cancellation_are_distinct_and_retire_approval() {
    for outcome in ["deny", "close", "cancel"] {
        let mut fixture = Fixture::new(false).await;
        let mut events = fixture.owner.subscribe();
        let run = fixture.start();
        let approval = Fixture::approval(&mut events).await;
        match outcome {
            "deny" => fixture
                .owner
                .dispatch(AppCommand::RejectTool {
                    approval_id: approval.id,
                })
                .await
                .unwrap(),
            "cancel" => {
                fixture
                    .owner
                    .dispatch(AppCommand::CancelRun {
                        run_id: approval.run_id,
                    })
                    .await
                    .unwrap();
                drop(fixture.owner.approval_responder.take());
            }
            _ => drop(fixture.owner.approval_responder.take()),
        }
        let result = Fixture::finish(run).await;
        match outcome {
            "deny" => assert!(matches!(result, Err(RuntimeError::ApprovalDenied(_)))),
            "close" => assert!(matches!(result, Err(RuntimeError::ApprovalRequired(_)))),
            _ => assert!(result.is_ok()),
        }
        fixture
            .owner
            .dispatch(AppCommand::ApproveTool {
                approval_id: approval.id,
                scope: ApprovalScope::Once,
            })
            .await
            .unwrap();
        fixture.assert_settled(if outcome == "cancel" {
            TaskLifecycle::Cancelled
        } else {
            TaskLifecycle::Failed
        });
        assert_eq!(
            std::fs::read(fixture.directory.path().join("fixture.txt")).unwrap(),
            b"before\r\n"
        );
        assert_eq!(fixture.requests.lock().unwrap().len(), 1);
    }
}

#[tokio::test]
async fn approval_claim_is_drained_before_close_or_cancel_can_settle_run() {
    for cancel in [false, true] {
        let mut fixture = Fixture::new(true).await;
        let mut events = fixture.owner.subscribe();
        let run = fixture.start();
        let approval = Fixture::approval(&mut events).await;
        let generation = fixture
            .owner
            .platform_authority
            .current_generation()
            .unwrap();
        let ticket = fixture
            .owner
            .issuer
            .issue(
                LocalAuthorityAction::ApproveTool {
                    approval_id: approval.id,
                    scope: ApprovalScope::Once,
                },
                generation,
            )
            .unwrap();
        let runtime = fixture.owner.runtime.clone();
        let approving = tokio::spawn(async move {
            runtime
                .dispatch_with_local_authority(
                    AppCommand::ApproveTool {
                        approval_id: approval.id,
                        scope: ApprovalScope::Once,
                    },
                    ticket,
                )
                .await
        });
        tokio::time::timeout(WATCHDOG, fixture.gate.entered.notified())
            .await
            .unwrap();
        if cancel {
            fixture
                .owner
                .dispatch(AppCommand::CancelRun {
                    run_id: approval.run_id,
                })
                .await
                .unwrap();
        }
        drop(fixture.owner.approval_responder.take());
        assert!(
            !run.is_finished(),
            "run settled while the claimed effect was still owned"
        );
        fixture.gate.release.notify_one();
        tokio::time::timeout(WATCHDOG, approving)
            .await
            .unwrap()
            .unwrap()
            .unwrap();
        let result = Fixture::finish(run).await;
        if cancel {
            assert!(result.is_ok());
        } else {
            assert!(matches!(result, Err(RuntimeError::ApprovalRequired(_))));
        }
        fixture.assert_settled(if cancel {
            TaskLifecycle::Cancelled
        } else {
            TaskLifecycle::Failed
        });
        let expected = if cancel {
            b"before\r\n".as_slice()
        } else {
            b"after\r\n".as_slice()
        };
        assert_eq!(
            std::fs::read(fixture.directory.path().join("fixture.txt")).unwrap(),
            expected
        );
        assert_eq!(fixture.requests.lock().unwrap().len(), 1);
    }
}

#[tokio::test]
async fn automatic_effect_is_preserved_when_next_process_requires_unavailable_approval() {
    let mut fixture = Fixture::new(false).await;
    fixture
        .owner
        .dispatch(AppCommand::SelectExecutionProfile {
            profile_id: "automatic".into(),
        })
        .await
        .unwrap();
    drop(fixture.owner.approval_responder.take());
    *fixture.script.lock().unwrap() = vec![
        call(
            "earlier",
            "write_text",
            serde_json::json!({"path":"earlier.txt","content":"authorized earlier effect"}),
        ),
        call(
            "process",
            "execute_shell",
            serde_json::json!({"command":"echo F03 > child-marker.txt","timeout_ms":1000}),
        ),
        call(
            "later",
            "write_text",
            serde_json::json!({"path":"later.txt","content":"must not run"}),
        ),
    ];
    assert!(matches!(
        Fixture::finish(fixture.start()).await,
        Err(RuntimeError::ApprovalRequired(_))
    ));
    let run_id = fixture.assert_settled(TaskLifecycle::Failed);
    assert_eq!(
        std::fs::read(fixture.directory.path().join("earlier.txt")).unwrap(),
        b"authorized earlier effect"
    );
    assert!(!fixture.directory.path().join("child-marker.txt").exists());
    assert_eq!(
        fixture
            .owner
            .store
            .change_set(fixture.owner.project_id, run_id)
            .unwrap()
            .unwrap()
            .files
            .len(),
        1
    );
    assert!(fixture
        .owner
        .store
        .approval_grants(fixture.owner.project_id, 1000)
        .unwrap()
        .iter()
        .filter(|grant| grant.scope == GrantScope::Run)
        .all(|grant| grant.revoked_at.is_some()));
}

#[tokio::test]
async fn policy_denial_is_not_misclassified_as_unavailable_approval() {
    let mut fixture = Fixture::new(false).await;
    fixture
        .owner
        .dispatch(AppCommand::SelectExecutionProfile {
            profile_id: "observe".into(),
        })
        .await
        .unwrap();
    drop(fixture.owner.approval_responder.take());
    let input = ToolInput::ExecuteShell {
        command: "echo F03 > child-marker.txt".into(),
        cwd: None,
        timeout_ms: 1000,
        environment: BTreeMap::new(),
    };
    let error = fixture
        .owner
        .request_tool(ToolRequest {
            call_id: Uuid::new_v4(),
            run_id: Uuid::new_v4(),
            input,
        })
        .await
        .unwrap_err();
    assert!(matches!(error, RuntimeError::CapabilityDisabled(_)));
    assert!(fixture.owner.pending_approvals.lock().unwrap().is_empty());
    assert!(!fixture.directory.path().join("child-marker.txt").exists());
}

#[tokio::test]
async fn unavailable_restore_does_not_rewrite_a_historical_run_or_file() {
    for close_before in [false, true] {
        let mut fixture = Fixture::new(false).await;
        let mut events = fixture.owner.subscribe();
        let run = fixture.start();
        let approval = Fixture::approval(&mut events).await;
        fixture
            .owner
            .dispatch(AppCommand::ApproveTool {
                approval_id: approval.id,
                scope: ApprovalScope::Once,
            })
            .await
            .unwrap();
        Fixture::finish(run).await.unwrap();
        let change = fixture
            .owner
            .store
            .change_set(fixture.owner.project_id, approval.run_id)
            .unwrap()
            .unwrap()
            .files
            .remove(0);
        if close_before {
            drop(fixture.owner.approval_responder.take());
        }
        let restore = fixture
            .owner
            .dispatch(AppCommand::RequestRestore {
                file_change_id: change.id,
            })
            .await;
        if close_before {
            assert!(matches!(restore, Err(RuntimeError::ApprovalRequired(_))));
        } else {
            restore.unwrap();
            drop(fixture.owner.approval_responder.take());
        }
        fixture.assert_settled(TaskLifecycle::Complete);
        assert_eq!(
            std::fs::read(fixture.directory.path().join("fixture.txt")).unwrap(),
            b"after\r\n"
        );
    }
}
