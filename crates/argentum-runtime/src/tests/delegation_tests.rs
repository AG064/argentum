use std::sync::{Arc, Mutex};

use argentum_domain::{AppCommand, AppEvent, TaskLifecycle};
use argentum_providers::{
    ModelProvider, ModelRequest, ModelToolCall, ModelUsage, ProviderError, ProviderEvent,
};
use argentum_security::{ApprovalPolicy, CapabilityBroker};
use argentum_tools::ToolRegistry;
use argentum_workspaces::WorkspaceManager;
use async_trait::async_trait;
use serde_json::{json, Value};
use tokio::sync::{mpsc, Notify};

use super::*;

fn tool_call(id: &str, name: &str, arguments: Value) -> ModelToolCall {
    ModelToolCall {
        id: id.into(),
        name: name.into(),
        arguments: arguments.to_string(),
    }
}

async fn emit_usage(sender: &mpsc::Sender<ProviderEvent>) -> Result<(), ProviderError> {
    sender
        .send(ProviderEvent::Usage(ModelUsage {
            input_tokens: 100,
            output_tokens: 20,
            total_tokens: 120,
            cached_input_tokens: None,
            reasoning_tokens: None,
            context_window_tokens: Some(2_000_000),
        }))
        .await
        .map_err(|_| ProviderError::ConsumerClosed)
}

#[derive(Debug)]
struct DelegationProvider {
    requests: Arc<Mutex<Vec<ModelRequest>>>,
    prohibited_child_tool: Option<&'static str>,
    omit_child_usage: bool,
    oversized_child_answer: bool,
    excess_child_calls: bool,
    repeat_parent_delegation: bool,
    duplicate_child_completion: bool,
    child_gate: Option<Arc<Notify>>,
    child_entered: Option<Arc<Notify>>,
}

#[async_trait]
impl ModelProvider for DelegationProvider {
    fn id(&self) -> &'static str {
        "delegation-fixture"
    }

    fn status(&self) -> ProviderStatus {
        test_provider_status(self.id())
    }

    async fn stream(
        &self,
        request: ModelRequest,
        sender: mpsc::Sender<ProviderEvent>,
    ) -> Result<(), ProviderError> {
        let child = request
            .system
            .as_deref()
            .is_some_and(|system| system.contains("\"delegated_investigation\":true"));
        let child_round = request.tool_exchanges.len();
        self.requests.lock().unwrap().push(request.clone());
        if !(child && self.omit_child_usage) {
            emit_usage(&sender).await?;
        }
        if child {
            if let Some(entered) = &self.child_entered {
                entered.notify_one();
            }
            if let Some(gate) = &self.child_gate {
                gate.notified().await;
            }
            if self.oversized_child_answer {
                sender
                    .send(ProviderEvent::Delta("x".repeat(9_000)))
                    .await
                    .map_err(|_| ProviderError::ConsumerClosed)?;
            } else if self.excess_child_calls {
                let count = if child_round == 0 { 5 } else { 4 };
                for index in 0..count {
                    sender
                        .send(ProviderEvent::ToolCall(tool_call(
                            &format!("child-read-{child_round}-{index}"),
                            "read_text",
                            json!({"path":"canary.txt"}),
                        )))
                        .await
                        .map_err(|_| ProviderError::ConsumerClosed)?;
                }
            } else if self.prohibited_child_tool == Some("foreign_read_fixture") {
                if child_round == 0 {
                    sender
                        .send(ProviderEvent::ToolCall(tool_call(
                            "child-foreign-read",
                            "read_text",
                            json!({"path":"C:\\Windows\\System32\\drivers\\etc\\hosts"}),
                        )))
                        .await
                        .map_err(|_| ProviderError::ConsumerClosed)?;
                } else {
                    sender
                        .send(ProviderEvent::Delta(
                            "No eligible repository evidence was found.".into(),
                        ))
                        .await
                        .map_err(|_| ProviderError::ConsumerClosed)?;
                }
            } else if let Some(name) = self.prohibited_child_tool {
                sender
                    .send(ProviderEvent::ToolCall(tool_call(
                        "child-prohibited",
                        name,
                        json!({"path":"must-not-exist.txt","content":"forbidden"}),
                    )))
                    .await
                    .map_err(|_| ProviderError::ConsumerClosed)?;
            } else {
                match child_round {
                    0 => {
                        sender
                            .send(ProviderEvent::ToolCall(tool_call(
                                "child-search",
                                "search_text",
                                json!({
                                    "path":".",
                                    "query":"^F19A_TOOL_ONLY_CANARY$",
                                    "max_results":8,
                                    "mode":"regex",
                                    "before_context":1,
                                    "after_context":1
                                }),
                            )))
                            .await
                            .map_err(|_| ProviderError::ConsumerClosed)?;
                    }
                    1 => {
                        let selection = request.tool_exchanges[0]
                            .results
                            .iter()
                            .find(|result| result.call_id == "child-search")
                            .expect("child search result before read")
                            .content
                            .parse::<Value>()
                            .expect("child search result JSON")["matches"][0]["read_selection"]
                            .clone();
                        sender
                            .send(ProviderEvent::ToolCall(tool_call(
                                "child-read",
                                "read_text",
                                selection,
                            )))
                            .await
                            .map_err(|_| ProviderError::ConsumerClosed)?;
                    }
                    2 => {
                        sender
                            .send(ProviderEvent::Delta(
                                "The canary is in canary.txt and the file read completed.".into(),
                            ))
                            .await
                            .map_err(|_| ProviderError::ConsumerClosed)?;
                    }
                    _ => return Err(ProviderError::InvalidResponse("extra child round".into())),
                }
            }
        } else if request.tool_exchanges.is_empty() {
            sender
                .send(ProviderEvent::ToolCall(tool_call(
                    "parent-delegate",
                    super::super::delegation::TOOL_ID,
                    json!({"brief":"Find the repository canary and report its file."}),
                )))
                .await
                .map_err(|_| ProviderError::ConsumerClosed)?;
        } else if self.repeat_parent_delegation && request.tool_exchanges.len() == 1 {
            sender
                .send(ProviderEvent::ToolCall(tool_call(
                    "parent-delegate-again",
                    super::super::delegation::TOOL_ID,
                    json!({"brief":"Try to start a second child."}),
                )))
                .await
                .map_err(|_| ProviderError::ConsumerClosed)?;
        } else {
            sender
                .send(ProviderEvent::Delta(
                    "Parent received the investigation once.".into(),
                ))
                .await
                .map_err(|_| ProviderError::ConsumerClosed)?;
        }
        sender
            .send(ProviderEvent::Completed)
            .await
            .map_err(|_| ProviderError::ConsumerClosed)?;
        if child && self.duplicate_child_completion {
            sender
                .send(ProviderEvent::Completed)
                .await
                .map_err(|_| ProviderError::ConsumerClosed)?;
        }
        Ok(())
    }
}

async fn fixture(
    provider: DelegationProvider,
) -> (
    tempfile::TempDir,
    TestRuntime,
    Arc<Mutex<Vec<ModelRequest>>>,
) {
    let requests = provider.requests.clone();
    let root = tempfile::tempdir().unwrap();
    std::fs::write(
        root.path().join("canary.txt"),
        "F19A_NEIGHBOR_BEFORE\nF19A_TOOL_ONLY_CANARY\nF19A_NEIGHBOR_AFTER\n",
    )
    .unwrap();
    let mut providers = ProviderRegistry::default();
    providers.register(provider);
    let broker = CapabilityBroker::new(root.path(), ApprovalPolicy::default()).unwrap();
    let workspace = WorkspaceManager::new(broker);
    let tools = ToolRegistry::with_builtins(workspace.clone());
    let runtime = test_runtime(
        Store::open_in_memory().unwrap(),
        providers,
        tools,
        workspace,
    );
    configure_selected_provider(&runtime, "delegation-fixture").await;
    (root, runtime, requests)
}

async fn enable_delegation(runtime: &TestRuntime) {
    runtime
        .dispatch(AppCommand::SetHarnessCapabilityEnabled {
            capability_id: super::super::delegation::CAPABILITY_ID.into(),
            enabled: true,
        })
        .await
        .unwrap();
}

fn submission(runtime: &TestRuntime, root: &std::path::Path, run_id: RunId) -> TaskSubmission {
    let _ = root;
    TaskSubmission {
        id: argentum_domain::TaskId::new_v4(),
        run_id,
        target: SubmissionTarget {
            project_id: runtime.project_id,
            session_id: runtime.session_id(),
            workspace: runtime.workspace.root().to_path_buf(),
        },
        prompt: "Repair the target. PARENT_PRIVATE_TEXT_MUST_NOT_ENTER_CHILD_HISTORY".into(),
        skill: None,
        answer_contract: None,
    }
}

#[tokio::test]
async fn delegation_is_disabled_by_default_and_enabled_only_for_compatible_http() {
    let requests = Arc::new(Mutex::new(Vec::new()));
    let provider = DelegationProvider {
        requests: requests.clone(),
        prohibited_child_tool: None,
        omit_child_usage: false,
        oversized_child_answer: false,
        excess_child_calls: false,
        repeat_parent_delegation: false,
        duplicate_child_completion: false,
        child_gate: None,
        child_entered: None,
    };
    let (_root, runtime, _) = fixture(provider).await;
    let initial = runtime.model_tool_definitions().unwrap();
    assert!(!initial
        .iter()
        .any(|tool| tool.name == super::super::delegation::TOOL_ID));
    assert!(runtime
        .harness_snapshot()
        .unwrap()
        .capabilities
        .iter()
        .any(|capability| {
            capability.id == super::super::delegation::CAPABILITY_ID
                && capability.configurable
                && !capability.enabled
        }));

    enable_delegation(&runtime).await;
    let enabled = runtime.model_tool_definitions().unwrap();
    assert_eq!(
        enabled
            .iter()
            .filter(|tool| tool.name == super::super::delegation::TOOL_ID)
            .count(),
        1
    );

    runtime
        .dispatch(AppCommand::SaveProviderProfile {
            profile: ProviderProfile::default_llama_cpp(),
        })
        .await
        .unwrap();
    assert!(!runtime
        .model_tool_definitions()
        .unwrap()
        .iter()
        .any(|tool| tool.name == super::super::delegation::TOOL_ID));
}

#[tokio::test]
async fn child_has_fresh_context_real_read_references_and_accounted_requests() {
    let requests = Arc::new(Mutex::new(Vec::new()));
    let provider = DelegationProvider {
        requests: requests.clone(),
        prohibited_child_tool: None,
        omit_child_usage: false,
        oversized_child_answer: false,
        excess_child_calls: false,
        repeat_parent_delegation: false,
        duplicate_child_completion: false,
        child_gate: None,
        child_entered: None,
    };
    let (root, runtime, _) = fixture(provider).await;
    enable_delegation(&runtime).await;
    let mut events = runtime.subscribe();
    let run_id = RunId::new_v4();
    let result = runtime
        .submit_bound_task(submission(&runtime, root.path(), run_id))
        .await;
    result.result.unwrap();
    assert_eq!(result.run_id, run_id);

    let requests = requests.lock().unwrap().clone();
    let child = requests
        .iter()
        .filter(|request| {
            request
                .system
                .as_deref()
                .is_some_and(|system| system.contains("\"delegated_investigation\":true"))
        })
        .collect::<Vec<_>>();
    let parent = requests
        .iter()
        .filter(|request| {
            request
                .system
                .as_deref()
                .is_none_or(|system| !system.contains("\"delegated_investigation\":true"))
        })
        .collect::<Vec<_>>();
    assert_eq!(child.len(), 3);
    assert_eq!(parent.len(), 2);
    assert!(child.iter().all(|request| {
        request.history.is_empty()
            && request.model == "test-model"
            && request.tools.iter().all(|tool| {
                matches!(
                    tool.name.as_str(),
                    "read_text" | "search_text" | "search_filenames"
                )
            })
            && !request
                .system
                .as_deref()
                .unwrap_or_default()
                .contains("PARENT_PRIVATE_TEXT_MUST_NOT_ENTER_CHILD_HISTORY")
            && !request
                .system
                .as_deref()
                .unwrap_or_default()
                .contains("ANSWER CONTRACT")
    }));
    let child_search_schema = child[0]
        .tools
        .iter()
        .find(|tool| tool.name == "search_text")
        .expect("F12B child search schema");
    assert_eq!(
        child_search_schema.parameters["properties"]["mode"]["enum"][1],
        "regex"
    );
    assert_eq!(
        child_search_schema.parameters["properties"]["before_context"]["maximum"],
        8
    );
    let child_read_schema = child[0]
        .tools
        .iter()
        .find(|tool| tool.name == "read_text")
        .expect("F12C child read schema");
    assert!(child_read_schema.description.contains("one-based"));
    assert_eq!(
        child_read_schema.parameters["properties"]["start_line"]["minimum"],
        1
    );
    assert_eq!(
        child_read_schema.parameters["properties"]["line_count"]["maximum"],
        200
    );
    let child_search = child[1].tool_exchanges[0]
        .results
        .iter()
        .find(|result| result.call_id == "child-search")
        .expect("F12B child search result")
        .content
        .parse::<Value>()
        .expect("F12B child search JSON");
    assert_eq!(child_search["matches_returned"], 1);
    assert_eq!(child_search["matches"][0]["path"], "canary.txt");
    assert_eq!(child_search["matches"][0]["line_number"], 2);
    assert_eq!(
        child_search["matches"][0]["read_selection"]["path"],
        "canary.txt"
    );
    assert_eq!(
        child_search["matches"][0]["read_selection"]["expected_sha256"]
            .as_str()
            .map(str::len),
        Some(64)
    );
    assert_eq!(
        child_search["matches"][0]["context"]["before"][0]["line"],
        "F19A_NEIGHBOR_BEFORE"
    );
    assert_eq!(
        child_search["matches"][0]["context"]["after"][0]["line"],
        "F19A_NEIGHBOR_AFTER"
    );
    let child_read_call = &child[2].tool_exchanges[1].calls[0];
    let child_read_arguments: Value =
        serde_json::from_str(&child_read_call.arguments).expect("F12C child read arguments");
    assert_eq!(
        child_read_arguments,
        child_search["matches"][0]["read_selection"]
    );
    let parent_result = parent[1].tool_exchanges[0].results[0]
        .content
        .parse::<Value>()
        .unwrap();
    assert_eq!(parent_result["disposition"], "completed");
    assert_eq!(parent_result["verification"], "not_verification");
    assert_eq!(parent_result["child_requests"], 3);
    assert_eq!(parent_result["child_tool_admissions"], 2);
    assert!(parent_result["references"]
        .as_array()
        .unwrap()
        .iter()
        .any(|reference| {
            reference["tool"] == "search_text"
                && reference["path"] == "canary.txt"
                && reference["complete"] == true
                && reference["matches_returned"] == 1
        }));
    assert!(parent_result["references"]
        .as_array()
        .unwrap()
        .iter()
        .any(|reference| {
            reference["tool"] == "read_text"
                && reference["path"] == "canary.txt"
                && reference["sha256"]
                    .as_str()
                    .is_some_and(|digest| digest.len() == 64)
        }));
    assert_eq!(
        parent[1].tool_exchanges[0]
            .results
            .iter()
            .filter(|result| result.call_id == "parent-delegate")
            .count(),
        1
    );

    let emitted = drain_events(&mut events);
    assert_eq!(
        emitted
            .iter()
            .filter(|event| matches!(event, AppEvent::ContextRequestAccounted { purpose, .. } if purpose == "delegated_readonly_generation"))
            .count(),
        3
    );
    assert!(emitted.iter().any(|event| matches!(
        event,
        AppEvent::RunStatusChanged { run_id: id, lifecycle: TaskLifecycle::Complete }
            if *id == run_id
    )));
    assert!(!emitted.iter().any(|event| matches!(
        event,
        AppEvent::AssistantDelta { text, .. } if text.contains("The canary is in")
    )));

    let historical = runtime
        .project_working_context(runtime.session_id(), None, 512 * 1024, true)
        .unwrap();
    let rendered = serde_json::to_string(&historical.messages).unwrap();
    assert!(rendered.contains(super::super::delegation::TOOL_ID));
    assert!(rendered.contains("The canary is in canary.txt"));
    assert!(!rendered.contains("F19A_TOOL_ONLY_CANARY"));
    runtime
        .dispatch(AppCommand::SetHarnessCapabilityEnabled {
            capability_id: super::super::delegation::CAPABILITY_ID.into(),
            enabled: false,
        })
        .await
        .unwrap();
    let filtered = runtime
        .project_working_context(runtime.session_id(), None, 512 * 1024, true)
        .unwrap();
    let filtered = serde_json::to_string(&filtered.messages).unwrap();
    assert!(!filtered.contains("The canary is in canary.txt"));
}

#[tokio::test]
async fn prohibited_child_tools_have_no_effect_and_return_failed_results() {
    for prohibited in [
        "write_text",
        "execute_argv",
        super::super::delegation::TOOL_ID,
    ] {
        let requests = Arc::new(Mutex::new(Vec::new()));
        let provider = DelegationProvider {
            requests: requests.clone(),
            prohibited_child_tool: Some(prohibited),
            omit_child_usage: false,
            oversized_child_answer: false,
            excess_child_calls: false,
            repeat_parent_delegation: false,
            duplicate_child_completion: false,
            child_gate: None,
            child_entered: None,
        };
        let (root, runtime, _) = fixture(provider).await;
        enable_delegation(&runtime).await;
        runtime
            .submit_bound_task(submission(&runtime, root.path(), RunId::new_v4()))
            .await
            .result
            .unwrap();
        assert!(!root.path().join("must-not-exist.txt").exists());
        let requests = requests.lock().unwrap();
        let parent_final = requests
            .iter()
            .find(|request| {
                !request.tool_exchanges.is_empty()
                    && request
                        .system
                        .as_deref()
                        .is_some_and(|system| !system.contains("\"delegated_investigation\":true"))
            })
            .unwrap();
        let result: Value =
            serde_json::from_str(&parent_final.tool_exchanges[0].results[0].content).unwrap();
        assert_eq!(result["disposition"], "failed");
        assert_eq!(result["answer"], "prohibited_child_tool");
        assert_eq!(result["child_tool_admissions"], 0);
    }
}

#[tokio::test]
async fn foreign_child_path_is_rejected_and_no_evidence_is_reported_honestly() {
    let requests = Arc::new(Mutex::new(Vec::new()));
    let provider = DelegationProvider {
        requests: requests.clone(),
        prohibited_child_tool: Some("foreign_read_fixture"),
        omit_child_usage: false,
        oversized_child_answer: false,
        excess_child_calls: false,
        repeat_parent_delegation: false,
        duplicate_child_completion: false,
        child_gate: None,
        child_entered: None,
    };
    let (root, runtime, _) = fixture(provider).await;
    enable_delegation(&runtime).await;
    runtime
        .submit_bound_task(submission(&runtime, root.path(), RunId::new_v4()))
        .await
        .result
        .unwrap();
    let requests = requests.lock().unwrap();
    assert_eq!(requests.len(), 4);
    let parent = requests.last().unwrap();
    let result: Value = serde_json::from_str(&parent.tool_exchanges[0].results[0].content).unwrap();
    assert_eq!(result["disposition"], "completed");
    assert_eq!(
        result["answer"],
        "No eligible repository evidence was found."
    );
    assert_eq!(result["references"], json!([]));
    assert_eq!(result["child_tool_admissions"], 1);
}

#[tokio::test]
async fn steering_is_rejected_while_owned_child_is_active_and_stop_drains_it() {
    let requests = Arc::new(Mutex::new(Vec::new()));
    let entered = Arc::new(Notify::new());
    let gate = Arc::new(Notify::new());
    let provider = DelegationProvider {
        requests,
        prohibited_child_tool: None,
        omit_child_usage: false,
        oversized_child_answer: false,
        excess_child_calls: false,
        repeat_parent_delegation: false,
        duplicate_child_completion: false,
        child_gate: Some(gate.clone()),
        child_entered: Some(entered.clone()),
    };
    let (root, runtime, _) = fixture(provider).await;
    enable_delegation(&runtime).await;
    let run_id = RunId::new_v4();
    let service = runtime.runtime.clone();
    let task = submission(&runtime, root.path(), run_id);
    let running = tokio::spawn(async move { service.submit_bound_task(task).await });
    tokio::time::timeout(std::time::Duration::from_secs(5), entered.notified())
        .await
        .unwrap();
    let steering = runtime.steer_active_run(SteeringSubmission {
        id: argentum_domain::SteeringEntryId::new_v4(),
        run_id,
        target: SubmissionTarget {
            project_id: runtime.project_id,
            session_id: runtime.session_id(),
            workspace: runtime.workspace.root().to_path_buf(),
        },
        text: "Retained draft".into(),
    });
    assert!(matches!(
        steering,
        Err(RuntimeError::SteeringBusy(
            "an approval or asynchronous process job is active"
        ))
    ));
    assert!(matches!(
        runtime
            .dispatch(AppCommand::SetHarnessCapabilityEnabled {
                capability_id: super::super::delegation::CAPABILITY_ID.into(),
                enabled: false,
            })
            .await,
        Err(RuntimeError::ExecutionPolicyBusy)
    ));
    assert!(matches!(
        runtime
            .dispatch(AppCommand::SelectExecutionProfile {
                profile_id: "observe".into(),
            })
            .await,
        Err(RuntimeError::ExecutionPolicyBusy)
    ));
    runtime
        .dispatch(AppCommand::CancelRun { run_id })
        .await
        .unwrap();
    gate.notify_waiters();
    let result = tokio::time::timeout(std::time::Duration::from_secs(5), running)
        .await
        .unwrap()
        .unwrap();
    assert!(result.result.is_ok());
    assert_eq!(result.terminal_lifecycle, Some(TaskLifecycle::Cancelled));
}

#[tokio::test]
async fn missing_child_usage_fails_the_parent_without_an_unaccounted_continuation() {
    let requests = Arc::new(Mutex::new(Vec::new()));
    let provider = DelegationProvider {
        requests: requests.clone(),
        prohibited_child_tool: None,
        omit_child_usage: true,
        oversized_child_answer: false,
        excess_child_calls: false,
        repeat_parent_delegation: false,
        duplicate_child_completion: false,
        child_gate: None,
        child_entered: None,
    };
    let (root, runtime, _) = fixture(provider).await;
    enable_delegation(&runtime).await;
    let result = runtime
        .submit_bound_task(submission(&runtime, root.path(), RunId::new_v4()))
        .await;
    assert!(matches!(
        result.result,
        Err(RuntimeError::DelegationUsageUnresolved)
    ));
    assert_eq!(requests.lock().unwrap().len(), 2);
}

#[tokio::test]
async fn exhausted_child_allowance_prevents_child_dispatch_and_parent_continuation() {
    let requests = Arc::new(Mutex::new(Vec::new()));
    let provider = DelegationProvider {
        requests: requests.clone(),
        prohibited_child_tool: None,
        omit_child_usage: false,
        oversized_child_answer: false,
        excess_child_calls: false,
        repeat_parent_delegation: false,
        duplicate_child_completion: false,
        child_gate: None,
        child_entered: None,
    };
    let (root, runtime, _) = fixture(provider).await;
    enable_delegation(&runtime).await;
    runtime
        .declare_model_budget(
            "delegation-fixture",
            [2_000_000, 1_900_000, 8_192, 1_024, 31_000],
        )
        .unwrap();
    let result = runtime
        .submit_bound_task(submission(&runtime, root.path(), RunId::new_v4()))
        .await;
    assert!(matches!(
        result.result,
        Err(RuntimeError::DelegationAllowanceExhausted)
    ));
    assert_eq!(requests.lock().unwrap().len(), 1);
}

#[tokio::test]
async fn oversized_child_answer_is_bounded_and_returned_as_a_failed_tool_result() {
    let requests = Arc::new(Mutex::new(Vec::new()));
    let provider = DelegationProvider {
        requests: requests.clone(),
        prohibited_child_tool: None,
        omit_child_usage: false,
        oversized_child_answer: true,
        excess_child_calls: false,
        repeat_parent_delegation: false,
        duplicate_child_completion: false,
        child_gate: None,
        child_entered: None,
    };
    let (root, runtime, _) = fixture(provider).await;
    enable_delegation(&runtime).await;
    runtime
        .submit_bound_task(submission(&runtime, root.path(), RunId::new_v4()))
        .await
        .result
        .unwrap();
    let requests = requests.lock().unwrap();
    assert_eq!(requests.len(), 3);
    let result: Value =
        serde_json::from_str(&requests[2].tool_exchanges[0].results[0].content).unwrap();
    assert_eq!(result["disposition"], "failed");
    assert_eq!(result["answer"], "child_answer_too_large");
}

#[tokio::test]
async fn repeated_child_completion_is_a_controlled_failure_and_does_not_repeat_effects() {
    let requests = Arc::new(Mutex::new(Vec::new()));
    let provider = DelegationProvider {
        requests: requests.clone(),
        prohibited_child_tool: None,
        omit_child_usage: false,
        oversized_child_answer: false,
        excess_child_calls: false,
        repeat_parent_delegation: false,
        duplicate_child_completion: true,
        child_gate: None,
        child_entered: None,
    };
    let (root, runtime, _) = fixture(provider).await;
    enable_delegation(&runtime).await;
    runtime
        .submit_bound_task(submission(&runtime, root.path(), RunId::new_v4()))
        .await
        .result
        .unwrap();
    let requests = requests.lock().unwrap();
    assert_eq!(requests.len(), 3);
    let result: Value =
        serde_json::from_str(&requests[2].tool_exchanges[0].results[0].content).unwrap();
    assert_eq!(result["disposition"], "failed");
    assert_eq!(result["answer"], "provider_request_failed");
    assert_eq!(result["child_tool_admissions"], 0);
}

#[tokio::test]
async fn excess_child_tool_calls_are_rejected_before_any_admission() {
    let requests = Arc::new(Mutex::new(Vec::new()));
    let provider = DelegationProvider {
        requests: requests.clone(),
        prohibited_child_tool: None,
        omit_child_usage: false,
        oversized_child_answer: false,
        excess_child_calls: true,
        repeat_parent_delegation: false,
        duplicate_child_completion: false,
        child_gate: None,
        child_entered: None,
    };
    let (root, runtime, _) = fixture(provider).await;
    enable_delegation(&runtime).await;
    runtime
        .submit_bound_task(submission(&runtime, root.path(), RunId::new_v4()))
        .await
        .result
        .unwrap();
    let requests = requests.lock().unwrap();
    assert_eq!(requests.len(), 4);
    let result: Value =
        serde_json::from_str(&requests[3].tool_exchanges[0].results[0].content).unwrap();
    assert_eq!(result["disposition"], "failed");
    assert_eq!(result["answer"], "child_tool_limit_reached");
    assert_eq!(result["child_tool_admissions"], 5);
}

#[tokio::test]
async fn a_second_delegation_in_the_same_parent_run_is_not_started() {
    let requests = Arc::new(Mutex::new(Vec::new()));
    let provider = DelegationProvider {
        requests: requests.clone(),
        prohibited_child_tool: None,
        omit_child_usage: false,
        oversized_child_answer: false,
        excess_child_calls: false,
        repeat_parent_delegation: true,
        duplicate_child_completion: false,
        child_gate: None,
        child_entered: None,
    };
    let (root, runtime, _) = fixture(provider).await;
    enable_delegation(&runtime).await;
    runtime
        .submit_bound_task(submission(&runtime, root.path(), RunId::new_v4()))
        .await
        .result
        .unwrap();
    let requests = requests.lock().unwrap();
    assert_eq!(requests.len(), 6);
    let second_result = requests[5]
        .tool_exchanges
        .iter()
        .flat_map(|exchange| &exchange.results)
        .find(|result| result.call_id == "parent-delegate-again")
        .unwrap();
    let result: Value = serde_json::from_str(&second_result.content).unwrap();
    assert_eq!(result["disposition"], "failed");
    assert_eq!(result["answer"], "delegation_limit_reached");
    assert_eq!(
        requests
            .iter()
            .filter(|request| request
                .system
                .as_deref()
                .is_some_and(|system| system.contains("\"delegated_investigation\":true")))
            .count(),
        3
    );
}
