use super::*;
use argentum_domain::context_summary::CompactionStatus;
use argentum_providers::ProviderRequestValidation;
use serde_json::{json, Value};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Mode {
    Valid,
    SearchHistory,
    FilenameSearchHistory,
    Oversize,
    MissingExactCount,
    RuntimeChanged,
    InstructionsChanged,
    Cancelled,
    InvalidJson,
}

#[derive(Debug)]
struct ExactCompactionProvider {
    requests: Arc<Mutex<Vec<ModelRequest>>>,
    rebuilt_validations: Arc<AtomicUsize>,
    mode: Mode,
    root: PathBuf,
    cancellation: CancellationToken,
}

fn summarizing(request: &ModelRequest) -> bool {
    serde_json::from_str::<Value>(&request.prompt)
        .is_ok_and(|v| v["purpose"] == "bounded_context_compaction")
}

#[async_trait]
impl ModelProvider for ExactCompactionProvider {
    fn id(&self) -> &'static str {
        "exact-compaction"
    }
    fn status(&self) -> ProviderStatus {
        test_provider_status(self.id())
    }
    async fn capability_runtime_evidence(&self) -> Result<Option<String>, ProviderError> {
        Ok(Some("fixture-loaded-runtime".into()))
    }
    async fn validate_request(
        &self,
        request: &ModelRequest,
    ) -> Result<ProviderRequestValidation, ProviderError> {
        let rebuilt = !summarizing(request)
            && request
                .history
                .iter()
                .any(|m| m.content.contains("historical_lossy_summary"));
        if rebuilt {
            self.rebuilt_validations.fetch_add(1, Ordering::SeqCst);
            if self.mode == Mode::InstructionsChanged {
                std::fs::write(
                    self.root.join("AGENTS.md"),
                    "New fixture instruction: preserve source boundaries.\n",
                )
                .unwrap();
            }
            if self.mode == Mode::Cancelled {
                self.cancellation.cancel();
            }
        }
        // Deterministic fixture tokenizer, not a measurement of a real model.
        let input = if summarizing(request) || request.history.is_empty() {
            1800
        } else if rebuilt {
            if self.mode == Mode::Oversize {
                9000
            } else {
                3500
            }
        } else {
            7600
        };
        Ok(ProviderRequestValidation {
            exact_input_tokens: (!(rebuilt && self.mode == Mode::MissingExactCount))
                .then_some(input),
            context_tokens: Some(8192),
            output_limit: request.output_limit,
            representation_fingerprint: Some("fixture-final-template".into()),
            runtime_fingerprint: Some(
                if rebuilt && self.mode == Mode::RuntimeChanged {
                    "different-loaded-runtime"
                } else {
                    "fixture-loaded-runtime"
                }
                .into(),
            ),
        })
    }
    async fn stream(
        &self,
        request: ModelRequest,
        sender: mpsc::Sender<ProviderEvent>,
    ) -> Result<(), ProviderError> {
        self.requests.lock().unwrap().push(request.clone());
        if matches!(self.mode, Mode::SearchHistory | Mode::FilenameSearchHistory)
            && request.prompt == "seed-search"
            && request.tool_exchanges.is_empty()
        {
            let search = if self.mode == Mode::FilenameSearchHistory {
                (
                    "retained-filename-search",
                    "search_filenames",
                    json!({"path":".","pattern":"*NEEDLE*","max_results":4}),
                )
            } else {
                (
                    "retained-search",
                    "search_text",
                    json!({"path":".","query":"REAL05_SYNTHETIC_NEEDLE","max_results":4}),
                )
            };
            for (id, name, arguments) in [
                search,
                ("paired-read", "read_text", json!({"path":"source.rs"})),
            ] {
                sender
                    .send(ProviderEvent::ToolCall(ModelToolCall {
                        id: id.into(),
                        name: name.into(),
                        arguments: arguments.to_string(),
                    }))
                    .await
                    .map_err(|_| ProviderError::ConsumerClosed)?;
            }
            sender
                .send(ProviderEvent::Completed)
                .await
                .map_err(|_| ProviderError::ConsumerClosed)?;
            return Ok(());
        }
        let text = if summarizing(&request) {
            assert!(request.tools.is_empty() && request.tool_exchanges.is_empty());
            if self.mode == Mode::InvalidJson {
                "not a JSON summary".into()
            } else {
                let supplied: Value = serde_json::from_str(&request.prompt).unwrap();
                json!({
                    "objective":"Continue the synthetic task",
                    "decisions_constraints":["Retain original evidence"],
                    "observed_changes_verification":[],
                    "unresolved_work":["Continue the remaining work"],
                    "source_references":supplied["source_references"],
                })
                .to_string()
            }
        } else if request.prompt.starts_with("seed") {
            "Synthetic historical discussion, not current authority. ".repeat(100)
        } else {
            "Continued after exact-accounted compaction".into()
        };
        sender
            .send(ProviderEvent::Delta(text))
            .await
            .map_err(|_| ProviderError::ConsumerClosed)?;
        sender
            .send(ProviderEvent::Completed)
            .await
            .map_err(|_| ProviderError::ConsumerClosed)
    }
}

struct Fixture {
    root: tempfile::TempDir,
    runtime: TestRuntime,
    requests: Arc<Mutex<Vec<ModelRequest>>>,
    rebuilt: Arc<AtomicUsize>,
    cancellation: CancellationToken,
}

impl Fixture {
    async fn new(mode: Mode) -> Self {
        let root = tempfile::tempdir().unwrap();
        let requests = Arc::new(Mutex::new(Vec::new()));
        let rebuilt = Arc::new(AtomicUsize::new(0));
        let cancellation = CancellationToken::new();
        let mut providers = ProviderRegistry::default();
        providers.register(ExactCompactionProvider {
            requests: requests.clone(),
            rebuilt_validations: rebuilt.clone(),
            mode,
            root: root.path().into(),
            cancellation: cancellation.clone(),
        });
        let workspace = WorkspaceManager::new(
            CapabilityBroker::new(root.path(), ApprovalPolicy::default()).unwrap(),
        );
        let tools = ToolRegistry::with_builtins(workspace.clone());
        let runtime = test_runtime(
            Store::open_in_memory().unwrap(),
            providers,
            tools,
            workspace,
        );
        configure_selected_provider(&runtime, "exact-compaction").await;
        runtime
            .declare_model_budget("exact-compaction", [8192, 8192, 2048, 512, 128])
            .unwrap();
        for i in 0..4 {
            runtime
                .dispatch(AppCommand::SubmitTask {
                    prompt: format!("seed {i}"),
                })
                .await
                .unwrap();
        }
        Self {
            root,
            runtime,
            requests,
            rebuilt,
            cancellation,
        }
    }
}

#[tokio::test]
async fn real_search_capture_projects_and_compacts_without_losing_its_paired_read() {
    let fixture = Fixture::new(Mode::SearchHistory).await;
    let source = "REAL05_SYNTHETIC_NEEDLE\nordinary fixture contents\n";
    std::fs::write(fixture.root.path().join("source.rs"), source).unwrap();
    fixture
        .runtime
        .dispatch(AppCommand::SubmitTask {
            prompt: "seed-search".into(),
        })
        .await
        .unwrap();
    for index in 0..2 {
        fixture
            .runtime
            .dispatch(AppCommand::SubmitTask {
                prompt: format!("seed later {index}"),
            })
            .await
            .unwrap();
    }
    let preview = fixture
        .runtime
        .context_preview(fixture.runtime.session_id())
        .unwrap();
    assert_eq!(preview.selected_exchanges, 1);
    assert!(
        preview.omitted_source_ids.is_empty(),
        "{:?}",
        preview.omissions
    );
    let recorded_count = fixture
        .runtime
        .store
        .context_sources(fixture.runtime.project_id, fixture.runtime.session_id())
        .unwrap()
        .total_sources;
    let before_calls = fixture.requests.lock().unwrap().len();
    let result = fixture
        .runtime
        .compact_context(fixture.runtime.session_id(), RunId::new_v4(), false)
        .await
        .unwrap();
    assert_eq!(
        result.status,
        CompactionStatus::Success,
        "{}",
        result.reason
    );
    let requests = fixture.requests.lock().unwrap();
    assert_eq!(
        requests.len(),
        before_calls + 1,
        "compaction must not replay tools"
    );
    let summary = requests.last().unwrap();
    assert!(summarizing(summary));
    let supplied: Value = serde_json::from_str(&summary.prompt).unwrap();
    let exchange = supplied["sources"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|s| s["historical_message"]["tool_exchange"].as_object())
        .find(|exchange| {
            exchange["calls"]
                .as_array()
                .unwrap()
                .iter()
                .any(|c| c["name"] == "search_text")
        })
        .unwrap();
    let calls = exchange["calls"].as_array().unwrap();
    let results = exchange["results"].as_array().unwrap();
    assert_eq!(calls.len(), 2);
    assert_eq!(results.len(), 2);
    assert_eq!(calls[0]["name"], "search_text");
    assert_eq!(calls[1]["name"], "read_text");
    assert_eq!(calls[0]["id"], results[0]["call_id"]);
    assert_eq!(calls[1]["id"], results[1]["call_id"]);
    assert_eq!(calls[0]["arguments"], json!({"path":"source.rs"}));
    assert_eq!(results[0]["content"].as_object().unwrap().len(), 6);
    drop(requests);
    assert_eq!(
        fixture
            .runtime
            .store
            .context_sources(fixture.runtime.project_id, fixture.runtime.session_id())
            .unwrap()
            .total_sources,
        recorded_count
    );
    assert_eq!(
        std::fs::read_to_string(fixture.root.path().join("source.rs")).unwrap(),
        source
    );
}

#[tokio::test]
async fn real_filename_search_capture_projects_and_compacts_without_a_gap() {
    let fixture = Fixture::new(Mode::FilenameSearchHistory).await;
    let matched = "REAL05_SYNTHETIC_NEEDLE.rs";
    std::fs::write(fixture.root.path().join(matched), "matched fixture file\n").unwrap();
    std::fs::write(
        fixture.root.path().join("source.rs"),
        "ordinary fixture contents\n",
    )
    .unwrap();
    fixture
        .runtime
        .dispatch(AppCommand::SubmitTask {
            prompt: "seed-search".into(),
        })
        .await
        .unwrap();
    for index in 0..2 {
        fixture
            .runtime
            .dispatch(AppCommand::SubmitTask {
                prompt: format!("seed later {index}"),
            })
            .await
            .unwrap();
    }
    let preview = fixture
        .runtime
        .context_preview(fixture.runtime.session_id())
        .unwrap();
    assert_eq!(preview.selected_exchanges, 1);
    assert!(
        preview.omitted_source_ids.is_empty(),
        "{:?}",
        preview.omissions
    );
    let recorded_count = fixture
        .runtime
        .store
        .context_sources(fixture.runtime.project_id, fixture.runtime.session_id())
        .unwrap()
        .total_sources;
    let before_calls = fixture.requests.lock().unwrap().len();
    let result = fixture
        .runtime
        .compact_context(fixture.runtime.session_id(), RunId::new_v4(), false)
        .await
        .unwrap();
    assert_eq!(
        result.status,
        CompactionStatus::Success,
        "{}",
        result.reason
    );
    let requests = fixture.requests.lock().unwrap();
    assert_eq!(
        requests.len(),
        before_calls + 1,
        "compaction must not replay tools"
    );
    let summary = requests.last().unwrap();
    assert!(summarizing(summary));
    let supplied: Value = serde_json::from_str(&summary.prompt).unwrap();
    let exchange = supplied["sources"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|s| s["historical_message"]["tool_exchange"].as_object())
        .find(|exchange| {
            exchange["calls"]
                .as_array()
                .unwrap()
                .iter()
                .any(|c| c["name"] == "search_filenames")
        })
        .expect("filename search exchange must survive projection");
    let calls = exchange["calls"].as_array().unwrap();
    let results = exchange["results"].as_array().unwrap();
    assert_eq!(calls.len(), 2);
    assert_eq!(results.len(), 2);
    assert_eq!(calls[0]["name"], "search_filenames");
    assert_eq!(calls[1]["name"], "read_text");
    assert_eq!(calls[0]["id"], results[0]["call_id"]);
    assert_eq!(calls[1]["id"], results[1]["call_id"]);
    // Arguments are normalized to the resolved path only; the raw query/pattern
    // and match bodies are never retained.
    assert_eq!(calls[0]["arguments"], json!({"path": matched}));
    let metadata = results[0]["content"].as_object().unwrap();
    assert_eq!(metadata.len(), 6);
    assert_eq!(
        metadata
            .keys()
            .map(String::as_str)
            .collect::<std::collections::BTreeSet<_>>(),
        [
            "complete",
            "search_complete",
            "output_truncated",
            "matches_observed",
            "matches_returned",
            "stop_reason",
        ]
        .into_iter()
        .collect::<std::collections::BTreeSet<_>>()
    );
    assert_eq!(metadata["complete"], json!(true));
    assert_eq!(metadata["search_complete"], json!(true));
    assert_eq!(metadata["output_truncated"], json!(false));
    assert_eq!(metadata["matches_observed"], json!(1));
    assert_eq!(metadata["matches_returned"], json!(1));
    assert_eq!(metadata["stop_reason"], json!("complete"));
    drop(requests);
    assert_eq!(
        fixture
            .runtime
            .store
            .context_sources(fixture.runtime.project_id, fixture.runtime.session_id())
            .unwrap()
            .total_sources,
        recorded_count
    );
    assert_eq!(
        std::fs::read_to_string(fixture.root.path().join(matched)).unwrap(),
        "matched fixture file\n"
    );
}

#[tokio::test]
async fn exact_rebuilt_token_count_accepts_summary_despite_catalog_bytes_exceeding_context() {
    let fixture = Fixture::new(Mode::Valid).await;
    let result = fixture
        .runtime
        .compact_context(fixture.runtime.session_id(), RunId::new_v4(), false)
        .await
        .unwrap();
    assert_eq!(
        result.status,
        CompactionStatus::Success,
        "{}",
        result.reason
    );
    let before = result.before.unwrap();
    let after = result.after.unwrap();
    assert_eq!(before.accounted_input, 7600);
    assert_eq!(after.accounted_input, 3500);
    assert!(after.serialized_catalog_bytes > after.usable_context);
    assert!(after.fits);
    assert_eq!(before.method, after.method);
    assert_eq!(
        after.method,
        "exact_loaded_native_tokenizer_final_chat_template_v1"
    );
    assert!(fixture.rebuilt.load(Ordering::SeqCst) > 0);
    assert!(fixture
        .runtime
        .store
        .summary_snapshot(fixture.runtime.project_id, fixture.runtime.session_id())
        .unwrap()
        .checkpoint
        .is_some());
}

#[tokio::test]
async fn rebuilt_validation_rejects_overflow_missing_counts_runtime_changes_instructions_and_cancel(
) {
    for mode in [
        Mode::Oversize,
        Mode::MissingExactCount,
        Mode::RuntimeChanged,
        Mode::InstructionsChanged,
        Mode::Cancelled,
    ] {
        let fixture = Fixture::new(mode).await;
        let result = fixture
            .runtime
            .compact_context_with_cancellation(
                fixture.runtime.session_id(),
                RunId::new_v4(),
                false,
                fixture.cancellation.clone(),
            )
            .await
            .unwrap();
        assert_eq!(result.status, CompactionStatus::Failed, "mode {mode:?}");
        assert!(
            fixture.rebuilt.load(Ordering::SeqCst) > 0,
            "mode {mode:?} skipped final validation"
        );
        assert!(fixture
            .runtime
            .store
            .summary_snapshot(fixture.runtime.project_id, fixture.runtime.session_id())
            .unwrap()
            .checkpoint
            .is_none());
        if mode == Mode::Oversize {
            assert_eq!(result.after.unwrap().accounted_input, 9000);
        }
        if mode == Mode::InstructionsChanged {
            assert!(fixture.root.path().join("AGENTS.md").exists());
        }
    }
}

#[tokio::test]
async fn automatic_compaction_resumes_only_after_exact_rebuilt_validation() {
    let fixture = Fixture::new(Mode::Valid).await;
    fixture
        .runtime
        .declare_model_budget("exact-compaction", [7000, 7000, 2048, 512, 128])
        .unwrap();
    fixture
        .runtime
        .dispatch(AppCommand::SubmitTask {
            prompt: "Continue the task".into(),
        })
        .await
        .unwrap();
    let requests = fixture.requests.lock().unwrap();
    assert_eq!(requests.iter().filter(|r| summarizing(r)).count(), 1);
    assert!(requests
        .last()
        .unwrap()
        .history
        .iter()
        .any(|m| m.content.contains("historical_lossy_summary")));
    assert_eq!(
        fixture
            .runtime
            .conversation_snapshot(fixture.runtime.session_id())
            .unwrap()
            .messages
            .last()
            .unwrap()
            .text,
        "Continued after exact-accounted compaction"
    );
}

#[tokio::test]
async fn automatic_failure_preserves_specific_reason_and_does_not_dispatch_main_request() {
    let fixture = Fixture::new(Mode::InvalidJson).await;
    fixture
        .runtime
        .declare_model_budget("exact-compaction", [7000, 7000, 2048, 512, 128])
        .unwrap();
    let mut events = fixture.runtime.subscribe();
    let error = fixture
        .runtime
        .dispatch(AppCommand::SubmitTask {
            prompt: "Continue the task".into(),
        })
        .await
        .unwrap_err();
    assert!(error.to_string().contains("invalid summary JSON"));
    assert!(error.to_string().contains("no oversized main request"));
    assert!(fixture
        .requests
        .lock()
        .unwrap()
        .last()
        .is_some_and(summarizing));
    let events = drain_events(&mut events);
    assert!(events.iter().any(
        |e| matches!(e, AppEvent::RunError {message,..} if message.contains("invalid summary JSON"))
    ));
}
