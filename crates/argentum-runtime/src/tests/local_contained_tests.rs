use std::sync::{Arc, Mutex};

use argentum_android_inference::{
    ActivationApproval, AndroidInferenceModule, BackendKind, BackendReport, EngineFailure,
    EngineStreamOutcome, InferenceBounds, InferenceConfig, InferenceEngine, InferenceInputReport,
    InferenceRequest, PreparedModel, VerifiedModelDescriptor,
};
use argentum_domain::{AppCommand, AppEvent, ApprovalScope, ProviderKind, ProviderProfile};
use argentum_providers::ProviderRegistry;
use argentum_security::{ApprovalPolicy, CapabilityBroker};
use argentum_store::Store;
use argentum_tools::ToolRegistry;
use argentum_workspaces::WorkspaceManager;
use sha2::{Digest, Sha256};
use tokio_util::sync::CancellationToken;

use super::test_runtime;

struct ScriptedContainedEngine {
    outputs: Vec<String>,
    requests: Arc<Mutex<Vec<InferenceRequest>>>,
    next: usize,
}

impl InferenceEngine for ScriptedContainedEngine {
    fn load(&mut self, _: &PreparedModel) -> Result<BackendReport, EngineFailure> {
        BackendReport::new(BackendKind::Test, "runtime-contained-fixture")
            .map_err(|error| EngineFailure::new(error.to_string()))
    }

    fn stream(
        &mut self,
        request: &InferenceRequest,
        cancellation: &CancellationToken,
        emit: &mut dyn FnMut(&str) -> Result<(), EngineFailure>,
    ) -> Result<EngineStreamOutcome, EngineFailure> {
        self.requests.lock().unwrap().push(request.clone());
        let output = self
            .outputs
            .get(self.next)
            .ok_or_else(|| EngineFailure::new("fixture output exhausted"))?
            .clone();
        self.next += 1;
        for fragment in output.as_bytes().chunks(7) {
            if cancellation.is_cancelled() {
                return Ok(EngineStreamOutcome::Cancelled);
            }
            emit(
                std::str::from_utf8(fragment)
                    .map_err(|error| EngineFailure::new(error.to_string()))?,
            )?;
        }
        Ok(EngineStreamOutcome::Completed)
    }

    fn account(
        &mut self,
        request: &InferenceRequest,
    ) -> Result<InferenceInputReport, EngineFailure> {
        let estimated = request
            .messages()
            .iter()
            .map(|message| message.content().chars().count() as u64)
            .sum::<u64>()
            .div_ceil(4)
            .max(1);
        InferenceInputReport::new(
            estimated,
            4_096,
            request.max_generated_tokens(),
            "cd".repeat(32),
        )
        .map_err(|error| EngineFailure::new(error.to_string()))
    }

    fn unload(&mut self) -> Result<(), EngineFailure> {
        Ok(())
    }
}

fn tool_call(name: &str, arguments: &[(&str, serde_json::Value)]) -> String {
    let mut output = format!("<tool_call>\n<function={name}>\n");
    for (key, value) in arguments {
        output.push_str(&format!(
            "<parameter={key}>\n{}\n</parameter>\n",
            serde_json::to_string(value).unwrap()
        ));
    }
    output.push_str("</function>\n</tool_call>");
    output
}

#[tokio::test]
async fn contained_engine_drives_shared_read_edit_readback_without_touching_unrelated_or_index() {
    let workspace_dir = tempfile::tempdir().unwrap();
    let model_dir = tempfile::tempdir().unwrap();
    let target = workspace_dir.path().join("target.txt");
    let unrelated = workspace_dir.path().join("unrelated.txt");
    std::fs::write(&target, "before\n").unwrap();
    std::fs::write(&unrelated, "leave me\n").unwrap();
    for args in [
        ["init"].as_slice(),
        ["config", "user.email", "fixture@example.test"].as_slice(),
        ["config", "user.name", "Fixture"].as_slice(),
        ["add", "target.txt", "unrelated.txt"].as_slice(),
    ] {
        assert!(std::process::Command::new("git")
            .args(args)
            .current_dir(workspace_dir.path())
            .output()
            .unwrap()
            .status
            .success());
    }
    let index_before = std::fs::read(workspace_dir.path().join(".git/index")).unwrap();
    let target_digest = format!("{:x}", Sha256::digest(b"before\n"));
    let outputs = vec![
        tool_call("read_text", &[("path", serde_json::json!("target.txt"))]),
        tool_call(
            "edit_text",
            &[
                ("path", serde_json::json!("target.txt")),
                ("expected_sha256", serde_json::json!(target_digest)),
                ("old_text", serde_json::json!("before")),
                ("new_text", serde_json::json!("after")),
            ],
        ),
        tool_call("read_text", &[("path", serde_json::json!("target.txt"))]),
        "Readback confirms the requested change.".into(),
    ];
    let requests = Arc::new(Mutex::new(Vec::new()));
    let engine_requests = requests.clone();
    let model_name = "Qwen3.5-0.8B-Q4_0.gguf";
    let model_bytes = b"deterministic runtime fixture";
    std::fs::write(model_dir.path().join(model_name), model_bytes).unwrap();
    let descriptor = VerifiedModelDescriptor::new(
        model_name,
        model_name,
        model_bytes.len() as u64,
        &format!("{:x}", Sha256::digest(model_bytes)),
    )
    .unwrap();
    let (_, _, generation) = argentum_security::host_authority_channel().unwrap();
    let mut module = AndroidInferenceModule::with_engine_factory(
        InferenceConfig::new(model_dir.path(), InferenceBounds::default()).unwrap(),
        generation,
        Box::new(move || {
            Ok(Box::new(ScriptedContainedEngine {
                outputs,
                requests: engine_requests,
                next: 0,
            }) as Box<dyn InferenceEngine>)
        }),
    )
    .unwrap();
    let prepared = module.prepare(descriptor).await.unwrap();
    let mut activation = ActivationApproval::bind(&prepared);
    module.activate(prepared, &mut activation).await.unwrap();
    let module = Arc::new(tokio::sync::Mutex::new(module));
    let providers = ProviderRegistry::default().with_android_inference(module.clone());
    let workspace = WorkspaceManager::new(
        CapabilityBroker::new(workspace_dir.path(), ApprovalPolicy::default()).unwrap(),
    );
    let runtime = test_runtime(
        Store::open_in_memory().unwrap(),
        providers,
        ToolRegistry::with_builtins(workspace.clone()),
        workspace,
    );
    runtime
        .dispatch(AppCommand::SaveProviderProfile {
            profile: ProviderProfile {
                id: "llama-cpp".into(),
                label: "contained fixture".into(),
                kind: ProviderKind::LocalLlamaCpp,
                endpoint: "local://llama-cpp/".into(),
                model: model_name.into(),
                selected: true,
            },
        })
        .await
        .unwrap();
    runtime
        .declare_model_budget("llama-cpp", [4_096, 3_900, 512, 64, 256])
        .unwrap();
    runtime
        .dispatch(AppCommand::SelectExecutionProfile {
            profile_id: "full-access".into(),
        })
        .await
        .unwrap();
    runtime
        .dispatch(AppCommand::SetHarnessCapabilityEnabled {
            capability_id: "verification.runner".into(),
            enabled: false,
        })
        .await
        .unwrap();
    for descriptor in runtime.tool_descriptors() {
        if !matches!(descriptor.id.as_str(), "read_text" | "edit_text") {
            if descriptor.capability_id == "verification.runner" {
                continue;
            }
            runtime
                .dispatch(AppCommand::SetHarnessCapabilityEnabled {
                    capability_id: descriptor.capability_id,
                    enabled: false,
                })
                .await
                .unwrap();
        }
    }
    let mut events = runtime.subscribe();
    let submission = runtime.dispatch(AppCommand::SubmitTask {
        prompt: "Read target.txt, replace the unique word, then read it back.".into(),
    });
    tokio::pin!(submission);
    let mut exact_accounts = 0usize;
    loop {
        tokio::select! {
            result = &mut submission => {
                result.unwrap();
                break;
            }
            event = events.recv() => {
                match event.unwrap() {
                    AppEvent::ApprovalRequested(approval) => {
                        runtime.dispatch(AppCommand::ApproveTool {
                            approval_id: approval.id,
                            scope: ApprovalScope::Once,
                        }).await.unwrap();
                    }
                    AppEvent::ContextRequestAccounted { accounting, purpose, .. }
                        if purpose == "main_generation" =>
                    {
                        assert_eq!(
                            accounting.method,
                            "exact_loaded_native_tokenizer_final_chat_template_v1"
                        );
                        assert_eq!(accounting.uncertainty_allowance, 0);
                        exact_accounts += 1;
                    }
                    _ => {}
                }
            }
        }
    }
    while let Ok(event) = events.try_recv() {
        if let AppEvent::ContextRequestAccounted {
            accounting,
            purpose,
            ..
        } = event
        {
            if purpose == "main_generation" {
                assert_eq!(
                    accounting.method,
                    "exact_loaded_native_tokenizer_final_chat_template_v1"
                );
                assert_eq!(accounting.uncertainty_allowance, 0);
                exact_accounts += 1;
            }
        }
    }

    assert_eq!(std::fs::read_to_string(&target).unwrap(), "after\n");
    assert_eq!(std::fs::read_to_string(&unrelated).unwrap(), "leave me\n");
    assert_eq!(exact_accounts, 4);
    assert_eq!(
        std::fs::read(workspace_dir.path().join(".git/index")).unwrap(),
        index_before
    );
    {
        let captured = requests.lock().unwrap();
        assert_eq!(captured.len(), 4);
        assert!(captured
            .iter()
            .all(|request| request.max_generated_tokens() == 64));
        assert!(captured[1]
            .messages()
            .iter()
            .any(|message| message.content().contains("<tool_response>")));
        assert!(captured[3]
            .messages()
            .iter()
            .any(|message| message.content().contains("after")));
    }
    module.lock().await.shutdown().await.unwrap();
}
