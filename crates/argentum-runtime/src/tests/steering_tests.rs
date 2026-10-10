use std::sync::{Arc, Mutex};

use argentum_domain::{
    AppEvent, SteeringEntryId, SteeringLifecycle, TaskLifecycle, ToolDescriptor, ToolResultState,
};
use argentum_providers::{
    ModelProvider, ModelRequest, ModelToolCall, ProviderError, ProviderEvent,
};
use argentum_security::{ApprovalPolicy, CapabilityBroker};
use argentum_tools::{AgentTool, ToolContext, ToolError, ToolRegistry, ToolRequest, ToolResult};
use argentum_workspaces::WorkspaceManager;
use async_trait::async_trait;
use serde_json::json;
use tokio::sync::{mpsc, Notify};

use super::*;

#[derive(Debug, Clone)]
struct Gate {
    entered: Arc<Notify>,
    release: Arc<Notify>,
}

impl Gate {
    fn new() -> Self {
        Self {
            entered: Arc::new(Notify::new()),
            release: Arc::new(Notify::new()),
        }
    }
}

#[derive(Debug)]
struct SteeringProvider {
    requests: Arc<Mutex<Vec<ModelRequest>>>,
    first_calls: Vec<ModelToolCall>,
    dispatch_gate: Option<Gate>,
}

#[derive(Debug)]
struct RetrySteeringProvider {
    requests: Arc<Mutex<Vec<ModelRequest>>>,
    first_attempt_gate: Gate,
}

#[async_trait]
impl ModelProvider for RetrySteeringProvider {
    fn id(&self) -> &'static str {
        "steering-retry-fixture"
    }

    fn status(&self) -> ProviderStatus {
        test_provider_status(self.id())
    }

    async fn stream(
        &self,
        request: ModelRequest,
        sender: mpsc::Sender<ProviderEvent>,
    ) -> Result<(), ProviderError> {
        let attempt = {
            let mut requests = self.requests.lock().unwrap();
            let attempt = requests.len();
            requests.push(request);
            attempt
        };
        match attempt {
            0 => {
                self.first_attempt_gate.entered.notify_one();
                self.first_attempt_gate.release.notified().await;
                Err(ProviderError::PreDispatchConnection)
            }
            1 => {
                sender
                    .send(ProviderEvent::ToolCall(call(
                        "stale-after-retry",
                        "create_directory",
                        json!({"path":"must-not-exist"}),
                    )))
                    .await
                    .map_err(|_| ProviderError::ConsumerClosed)?;
                sender
                    .send(ProviderEvent::Completed)
                    .await
                    .map_err(|_| ProviderError::ConsumerClosed)
            }
            _ => {
                sender
                    .send(ProviderEvent::Delta("Fresh retry-safe decision".into()))
                    .await
                    .map_err(|_| ProviderError::ConsumerClosed)?;
                sender
                    .send(ProviderEvent::Completed)
                    .await
                    .map_err(|_| ProviderError::ConsumerClosed)
            }
        }
    }
}

#[async_trait]
impl ModelProvider for SteeringProvider {
    fn id(&self) -> &'static str {
        "steering-fixture"
    }

    fn status(&self) -> ProviderStatus {
        test_provider_status(self.id())
    }

    async fn stream(
        &self,
        request: ModelRequest,
        sender: mpsc::Sender<ProviderEvent>,
    ) -> Result<(), ProviderError> {
        let round = {
            let mut requests = self.requests.lock().unwrap();
            let round = requests.len();
            requests.push(request);
            round
        };
        if round == 0 {
            if let Some(gate) = &self.dispatch_gate {
                gate.entered.notify_one();
                gate.release.notified().await;
            }
            sender
                .send(ProviderEvent::Delta("Stale first-round text".into()))
                .await
                .map_err(|_| ProviderError::ConsumerClosed)?;
            for call in self.first_calls.clone() {
                sender
                    .send(ProviderEvent::ToolCall(call))
                    .await
                    .map_err(|_| ProviderError::ConsumerClosed)?;
            }
        } else {
            sender
                .send(ProviderEvent::Delta("Fresh decision completed".into()))
                .await
                .map_err(|_| ProviderError::ConsumerClosed)?;
        }
        sender
            .send(ProviderEvent::Completed)
            .await
            .map_err(|_| ProviderError::ConsumerClosed)
    }
}

struct GatedDirectoryTool {
    delegate: ToolRegistry,
    descriptor: ToolDescriptor,
    gate: Gate,
}

#[async_trait]
impl AgentTool for GatedDirectoryTool {
    fn descriptor(&self) -> ToolDescriptor {
        self.descriptor.clone()
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

fn call(id: &str, name: &str, arguments: serde_json::Value) -> ModelToolCall {
    ModelToolCall {
        id: id.into(),
        name: name.into(),
        arguments: arguments.to_string(),
    }
}

async fn fixture(
    root: &std::path::Path,
    calls: Vec<ModelToolCall>,
    dispatch_gate: Option<Gate>,
    tool_gate: Option<Gate>,
    full_access: bool,
) -> (TestRuntime, Arc<Mutex<Vec<ModelRequest>>>) {
    let requests = Arc::new(Mutex::new(Vec::new()));
    let mut providers = ProviderRegistry::default();
    providers.register(SteeringProvider {
        requests: requests.clone(),
        first_calls: calls,
        dispatch_gate,
    });
    let broker = CapabilityBroker::new(root, ApprovalPolicy::default()).unwrap();
    let workspace = WorkspaceManager::new(broker);
    let mut tools = ToolRegistry::with_builtins(workspace.clone());
    if let Some(gate) = tool_gate {
        let descriptor = tools
            .descriptors()
            .into_iter()
            .find(|tool| tool.id == "create_directory")
            .unwrap();
        tools.register(GatedDirectoryTool {
            delegate: tools.clone(),
            descriptor,
            gate,
        });
    }
    let runtime = test_runtime(
        Store::open_in_memory().unwrap(),
        providers,
        tools,
        workspace,
    );
    configure_selected_provider(&runtime, "steering-fixture").await;
    if full_access {
        runtime
            .dispatch(AppCommand::SelectExecutionProfile {
                profile_id: "full-access".into(),
            })
            .await
            .unwrap();
    }
    (runtime, requests)
}

fn target(runtime: &TestRuntime, _root: &std::path::Path) -> SubmissionTarget {
    SubmissionTarget {
        project_id: runtime.project_id,
        session_id: runtime.session_id(),
        workspace: runtime.workspace.root().to_path_buf(),
    }
}

fn submission(runtime: &TestRuntime, root: &std::path::Path, run_id: RunId) -> TaskSubmission {
    TaskSubmission {
        id: argentum_domain::TaskId::new_v4(),
        run_id,
        target: target(runtime, root),
        prompt: "Use the available tools".into(),
        skill: None,
        answer_contract: None,
    }
}

async fn entered(gate: &Gate, running: &mut tokio::task::JoinHandle<TaskSubmissionResult>) {
    tokio::select! {
        _ = gate.entered.notified() => {}
        result = running => panic!("run settled before gate: {result:?}"),
        _ = tokio::time::sleep(std::time::Duration::from_secs(5)) => {
            panic!("gate was not entered")
        }
    }
}

#[tokio::test]
async fn steering_during_provider_request_skips_stale_calls_and_reaches_fresh_request() {
    let root = tempfile::tempdir().unwrap();
    let dispatch_gate = Gate::new();
    let (runtime, requests) = fixture(
        root.path(),
        vec![call(
            "stale-directory",
            "create_directory",
            json!({"path":"stale"}),
        )],
        Some(dispatch_gate.clone()),
        None,
        true,
    )
    .await;
    let mut events = runtime.subscribe();
    let run_id = RunId::new_v4();
    let task = submission(&runtime, root.path(), run_id);
    let service = runtime.runtime.clone();
    let mut running = tokio::spawn(async move { service.submit_bound_task(task).await });

    entered(&dispatch_gate, &mut running).await;
    let receipt = runtime
        .steer_active_run(SteeringSubmission {
            id: SteeringEntryId::new_v4(),
            run_id,
            target: target(&runtime, root.path()),
            text: "Do not create stale. Finish with a fresh decision.".into(),
        })
        .unwrap();
    assert_eq!(receipt.lifecycle, SteeringLifecycle::Pending);
    dispatch_gate.release.notify_one();
    let completion = running.await.unwrap();
    assert_eq!(
        completion.terminal_lifecycle,
        Some(TaskLifecycle::Complete),
        "{:?}",
        completion.result
    );
    completion.result.unwrap();
    assert!(!root.path().join("stale").exists());
    let conversation = runtime
        .conversation_snapshot(runtime.session_id())
        .expect("conversation snapshot");
    assert_eq!(
        conversation
            .messages
            .last()
            .expect("assistant message")
            .text,
        "Fresh decision completed"
    );

    let requests = requests.lock().unwrap();
    assert_eq!(requests.len(), 2);
    assert!(requests[1].prompt.contains("Do not create stale"));
    let exchange = requests[1]
        .history
        .iter()
        .find_map(|message| message.tool_exchange.as_ref())
        .expect("paired stale exchange retained in fresh request");
    assert_eq!(exchange.results.len(), 1);
    assert!(exchange.results[0].content.contains("not_executed"));
    drop(requests);

    let emitted = drain_events(&mut events);
    assert!(emitted.iter().any(|event| matches!(
        event,
        AppEvent::ToolFinished(trace) if trace.result == ToolResultState::NotExecuted
    )));
    assert!(emitted.iter().any(|event| matches!(
        event,
        AppEvent::SteeringChanged(status)
            if status.entry_id == receipt.id && status.lifecycle == SteeringLifecycle::Included
    )));
}

#[tokio::test]
async fn admitted_tool_settles_before_steering_skips_the_later_call() {
    let root = tempfile::tempdir().unwrap();
    let tool_gate = Gate::new();
    let (runtime, requests) = fixture(
        root.path(),
        vec![
            call("admitted", "create_directory", json!({"path":"kept"})),
            call("stale", "create_directory", json!({"path":"skipped"})),
        ],
        None,
        Some(tool_gate.clone()),
        true,
    )
    .await;
    let run_id = RunId::new_v4();
    let task = submission(&runtime, root.path(), run_id);
    let service = runtime.runtime.clone();
    let mut running = tokio::spawn(async move { service.submit_bound_task(task).await });

    entered(&tool_gate, &mut running).await;
    runtime
        .steer_active_run(SteeringSubmission {
            id: SteeringEntryId::new_v4(),
            run_id,
            target: target(&runtime, root.path()),
            text: "Keep admitted effects, but reconsider everything else.".into(),
        })
        .unwrap();
    tool_gate.release.notify_one();
    let completion = running.await.unwrap();
    assert_eq!(
        completion.terminal_lifecycle,
        Some(TaskLifecycle::Complete),
        "{:?}",
        completion.result
    );
    completion.result.unwrap();
    assert!(root.path().join("kept").is_dir());
    assert!(!root.path().join("skipped").exists());

    let requests = requests.lock().unwrap();
    assert_eq!(requests.len(), 2);
    let exchange = requests[1]
        .history
        .iter()
        .find_map(|message| message.tool_exchange.as_ref())
        .expect("tool exchange retained");
    assert_eq!(exchange.results.len(), 2);
    assert!(!exchange.results[0].content.contains("not_executed"));
    assert!(exchange.results[1].content.contains("not_executed"));
}

#[tokio::test]
async fn physical_retry_keeps_its_request_immutable_then_pending_steering_blocks_stale_calls() {
    let root = tempfile::tempdir().unwrap();
    let requests = Arc::new(Mutex::new(Vec::new()));
    let first_attempt_gate = Gate::new();
    let mut providers = ProviderRegistry::default();
    providers.register(RetrySteeringProvider {
        requests: requests.clone(),
        first_attempt_gate: first_attempt_gate.clone(),
    });
    let broker = CapabilityBroker::new(root.path(), ApprovalPolicy::default()).unwrap();
    let workspace = WorkspaceManager::new(broker);
    let tools = ToolRegistry::with_builtins(workspace.clone());
    let runtime = test_runtime(
        Store::open_in_memory().unwrap(),
        providers,
        tools,
        workspace,
    );
    configure_selected_provider(&runtime, "steering-retry-fixture").await;
    runtime
        .dispatch(AppCommand::SelectExecutionProfile {
            profile_id: "full-access".into(),
        })
        .await
        .unwrap();
    let run_id = RunId::new_v4();
    let task = submission(&runtime, root.path(), run_id);
    let service = runtime.runtime.clone();
    let mut running = tokio::spawn(async move { service.submit_bound_task(task).await });
    entered(&first_attempt_gate, &mut running).await;

    let steering_text = "Do not execute calls from the retried old response.";
    runtime
        .steer_active_run(SteeringSubmission {
            id: SteeringEntryId::new_v4(),
            run_id,
            target: target(&runtime, root.path()),
            text: steering_text.into(),
        })
        .unwrap();
    first_attempt_gate.release.notify_one();
    let completion = running.await.unwrap();
    assert_eq!(completion.terminal_lifecycle, Some(TaskLifecycle::Complete));
    completion.result.unwrap();
    assert!(!root.path().join("must-not-exist").exists());

    let requests = requests.lock().unwrap();
    assert_eq!(requests.len(), 3);
    assert_eq!(
        serde_json::to_vec(&requests[0]).unwrap(),
        serde_json::to_vec(&requests[1]).unwrap()
    );
    assert!(!requests[0].prompt.contains(steering_text));
    assert!(requests[2].prompt.contains(steering_text));
    let exchange = requests[2]
        .history
        .iter()
        .find_map(|message| message.tool_exchange.as_ref())
        .expect("retried stale response has a complete paired exchange");
    assert_eq!(exchange.results.len(), 1);
    assert!(exchange.results[0].content.contains("not_executed"));
}

#[tokio::test]
async fn steering_bounds_identity_and_terminal_rejection_are_explicit() {
    let root = tempfile::tempdir().unwrap();
    let dispatch_gate = Gate::new();
    let (runtime, requests) = fixture(
        root.path(),
        Vec::new(),
        Some(dispatch_gate.clone()),
        None,
        true,
    )
    .await;
    let run_id = RunId::new_v4();
    let task = submission(&runtime, root.path(), run_id);
    let service = runtime.runtime.clone();
    let mut running = tokio::spawn(async move { service.submit_bound_task(task).await });
    entered(&dispatch_gate, &mut running).await;

    let mut wrong_target = target(&runtime, root.path());
    wrong_target.session_id = argentum_domain::SessionId::new_v4();
    assert!(matches!(
        runtime.steer_active_run(SteeringSubmission {
            id: SteeringEntryId::new_v4(),
            run_id,
            target: wrong_target,
            text: "must remain bound to the admitted session".into(),
        }),
        Err(RuntimeError::SteeringTargetChanged)
    ));

    assert!(matches!(
        runtime.steer_active_run(SteeringSubmission {
            id: SteeringEntryId::new_v4(),
            run_id,
            target: target(&runtime, root.path()),
            text: "x".repeat(MAX_STEERING_ENTRY_BYTES + 1),
        }),
        Err(RuntimeError::SteeringEntryTooLarge)
    ));

    let duplicate_id = SteeringEntryId::new_v4();
    let first = SteeringSubmission {
        id: duplicate_id,
        run_id,
        target: target(&runtime, root.path()),
        text: "same text\r\nUnicode: näide".into(),
    };
    let original = runtime.steer_active_run(first.clone()).unwrap();
    assert_eq!(runtime.steer_active_run(first).unwrap(), original);
    assert!(matches!(
        runtime.steer_active_run(SteeringSubmission {
            id: duplicate_id,
            run_id,
            target: target(&runtime, root.path()),
            text: "changed text".into(),
        }),
        Err(RuntimeError::SteeringTargetChanged)
    ));
    let repeated_text = runtime
        .steer_active_run(SteeringSubmission {
            id: SteeringEntryId::new_v4(),
            run_id,
            target: target(&runtime, root.path()),
            text: "same text\r\nUnicode: näide".into(),
        })
        .unwrap();
    assert_ne!(repeated_text.id, original.id);
    assert_eq!(repeated_text.position, original.position + 1);
    for index in 2..MAX_STEERING_ENTRIES {
        runtime
            .steer_active_run(SteeringSubmission {
                id: SteeringEntryId::new_v4(),
                run_id,
                target: target(&runtime, root.path()),
                text: format!("entry {index}"),
            })
            .unwrap();
    }
    assert!(matches!(
        runtime.steer_active_run(SteeringSubmission {
            id: SteeringEntryId::new_v4(),
            run_id,
            target: target(&runtime, root.path()),
            text: "newest rejected".into(),
        }),
        Err(RuntimeError::SteeringCapacity(_))
    ));
    dispatch_gate.release.notify_one();
    running.await.unwrap().result.unwrap();
    assert_eq!(requests.lock().unwrap().len(), 2);
    assert!(matches!(
        runtime.steer_active_run(SteeringSubmission {
            id: SteeringEntryId::new_v4(),
            run_id,
            target: target(&runtime, root.path()),
            text: "too late".into(),
        }),
        Err(RuntimeError::NoActiveSteeringRun)
    ));
}

#[tokio::test]
async fn steering_outstanding_utf8_byte_budget_rejects_the_newest_entry() {
    let root = tempfile::tempdir().unwrap();
    let dispatch_gate = Gate::new();
    let (runtime, _) = fixture(
        root.path(),
        Vec::new(),
        Some(dispatch_gate.clone()),
        None,
        true,
    )
    .await;
    let run_id = RunId::new_v4();
    let task = submission(&runtime, root.path(), run_id);
    let service = runtime.runtime.clone();
    let mut running = tokio::spawn(async move { service.submit_bound_task(task).await });
    entered(&dispatch_gate, &mut running).await;

    for _ in 0..4 {
        runtime
            .steer_active_run(SteeringSubmission {
                id: SteeringEntryId::new_v4(),
                run_id,
                target: target(&runtime, root.path()),
                text: "x".repeat(MAX_STEERING_ENTRY_BYTES),
            })
            .unwrap();
    }
    assert!(matches!(
        runtime.steer_active_run(SteeringSubmission {
            id: SteeringEntryId::new_v4(),
            run_id,
            target: target(&runtime, root.path()),
            text: "y".into(),
        }),
        Err(RuntimeError::SteeringCapacity(_))
    ));
    dispatch_gate.release.notify_one();
    running.await.unwrap().result.unwrap();
}

#[tokio::test]
async fn goal_supervisor_run_rejects_steering_without_stranding_it() {
    let root = tempfile::tempdir().unwrap();
    let dispatch_gate = Gate::new();
    let (runtime, _) = fixture(
        root.path(),
        Vec::new(),
        Some(dispatch_gate.clone()),
        None,
        true,
    )
    .await;
    runtime
        .dispatch(AppCommand::SetGoal {
            objective: "Complete one bounded fixture run".into(),
            token_budget: Some(500_000),
            tool_budget: Some(8),
            time_budget_seconds: Some(60),
        })
        .await
        .unwrap();
    let goal = runtime.goal().unwrap().unwrap();
    let binding = argentum_domain::goal_accounting::GoalBinding {
        goal_id: goal.id,
        revision: goal.accounting.unwrap().revision,
    };
    let run_id = RunId::new_v4();
    let task = submission(&runtime, root.path(), run_id);
    let service = runtime.runtime.clone();
    let mut running =
        tokio::spawn(async move { service.submit_goal_bound_task(task, binding).await });
    entered(&dispatch_gate, &mut running).await;

    assert!(matches!(
        runtime.steer_active_run(SteeringSubmission {
            id: SteeringEntryId::new_v4(),
            run_id,
            target: target(&runtime, root.path()),
            text: "Do something different".into(),
        }),
        Err(RuntimeError::SteeringBusy(reason)) if reason.contains("goal supervision")
    ));
    dispatch_gate.release.notify_one();
    assert!(matches!(
        running.await.unwrap().result,
        Err(RuntimeError::GoalUsageUnresolved)
    ));
}

#[tokio::test]
async fn pending_owner_approval_rejects_steering_and_preserves_the_original_denial() {
    let root = tempfile::tempdir().unwrap();
    let (runtime, _) = fixture(
        root.path(),
        vec![call(
            "approval-write",
            "write_text",
            json!({"path":"approval.txt","content":"must not be written"}),
        )],
        None,
        None,
        false,
    )
    .await;
    let mut events = runtime.subscribe();
    let run_id = RunId::new_v4();
    let task = submission(&runtime, root.path(), run_id);
    let service = runtime.runtime.clone();
    let running = tokio::spawn(async move { service.submit_bound_task(task).await });
    let approval = tokio::time::timeout(std::time::Duration::from_secs(5), async {
        loop {
            if let AppEvent::ApprovalRequested(approval) = events.recv().await.unwrap() {
                break approval;
            }
        }
    })
    .await
    .expect("approval requested");

    assert!(matches!(
        runtime.steer_active_run(SteeringSubmission {
            id: SteeringEntryId::new_v4(),
            run_id,
            target: target(&runtime, root.path()),
            text: "Do not approve that write".into(),
        }),
        Err(RuntimeError::SteeringBusy(reason)) if reason.contains("approval")
    ));
    runtime
        .dispatch(AppCommand::RejectTool {
            approval_id: approval.id,
        })
        .await
        .unwrap();
    let completion = running.await.unwrap();
    assert!(matches!(
        completion.result,
        Err(RuntimeError::ApprovalDenied(_))
    ));
    assert!(!root.path().join("approval.txt").exists());
}

#[tokio::test]
async fn cancellation_marks_pending_steering_not_applied_without_redelivery() {
    let root = tempfile::tempdir().unwrap();
    let dispatch_gate = Gate::new();
    let (runtime, requests) = fixture(
        root.path(),
        Vec::new(),
        Some(dispatch_gate.clone()),
        None,
        true,
    )
    .await;
    let mut events = runtime.subscribe();
    let run_id = RunId::new_v4();
    let task = submission(&runtime, root.path(), run_id);
    let service = runtime.runtime.clone();
    let mut running = tokio::spawn(async move { service.submit_bound_task(task).await });
    entered(&dispatch_gate, &mut running).await;

    let receipt = runtime
        .steer_active_run(SteeringSubmission {
            id: SteeringEntryId::new_v4(),
            run_id,
            target: target(&runtime, root.path()),
            text: "This must not be redelivered after cancellation.".into(),
        })
        .unwrap();
    runtime
        .dispatch(AppCommand::CancelRun { run_id })
        .await
        .unwrap();
    dispatch_gate.release.notify_one();

    let completion = running.await.unwrap();
    assert_eq!(
        completion.terminal_lifecycle,
        Some(TaskLifecycle::Cancelled)
    );
    assert!(completion.result.is_ok(), "{:?}", completion.result);
    assert_eq!(requests.lock().unwrap().len(), 1);

    let emitted = drain_events(&mut events);
    assert!(emitted.iter().any(|event| matches!(
        event,
        AppEvent::SteeringChanged(status)
            if status.entry_id == receipt.id
                && status.lifecycle == SteeringLifecycle::NotApplied
                && status.reason.as_deref() == Some("run_cancelled_before_inclusion")
    )));
}
