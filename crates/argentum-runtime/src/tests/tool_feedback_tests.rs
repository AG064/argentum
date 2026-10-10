use super::*;
use serde_json::{json, Value};

// Explicitly launched fixture for the public process-capture regressions below.
// This helper is not itself a semantic acceptance test.
#[test]
#[ignore = "subprocess byte writer; invoked by the capture regressions"]
fn real09_fragmented_byte_writer() {
    use std::io::Write;
    let bytes = std::env::var("ARGENTUM_REAL09_BYTES")
        .unwrap()
        .split(',')
        .map(|value| u8::from_str_radix(value, 16).unwrap())
        .collect::<Vec<_>>();
    let stderr = std::env::var("ARGENTUM_REAL09_STREAM").unwrap() == "stderr";
    let mut output: Box<dyn Write> = if stderr {
        Box::new(std::io::stderr())
    } else {
        Box::new(std::io::stdout())
    };
    output.write_all(b"P:").unwrap();
    output.flush().unwrap();
    std::thread::sleep(std::time::Duration::from_millis(100));
    for byte in bytes {
        output.write_all(&[byte]).unwrap();
        output.flush().unwrap();
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
    if std::env::var("ARGENTUM_REAL09_SUFFIX").unwrap() == "yes" {
        output.write_all(b":E").unwrap();
        output.flush().unwrap();
    }
}

fn real09_writer_spec(
    directory: &std::path::Path,
    bytes: &[u8],
    stderr: bool,
    suffix: bool,
    limit: usize,
) -> argentum_execution::ProcessSpec {
    let module = module_path!().split_once("::").unwrap().1;
    let test = format!("{module}::real09_fragmented_byte_writer");
    let environment = std::collections::BTreeMap::from([
        (
            "ARGENTUM_REAL09_BYTES".into(),
            bytes
                .iter()
                .map(|byte| format!("{byte:02x}"))
                .collect::<Vec<_>>()
                .join(","),
        ),
        (
            "ARGENTUM_REAL09_STREAM".into(),
            if stderr { "stderr" } else { "stdout" }.into(),
        ),
        (
            "ARGENTUM_REAL09_SUFFIX".into(),
            if suffix { "yes" } else { "no" }.into(),
        ),
    ]);
    argentum_execution::ProcessSpec::argv(
        std::env::current_exe()
            .unwrap()
            .to_string_lossy()
            .into_owned(),
        vec![
            "--exact".into(),
            test,
            "--ignored".into(),
            "--nocapture".into(),
            "--test-threads=1".into(),
        ],
        directory,
    )
    .unwrap()
    .with_environment(environment)
    .unwrap()
    .with_timeout(std::time::Duration::from_secs(10))
    .unwrap()
    .with_output_limit(limit)
    .unwrap()
}

async fn real09_capture_check(managed: bool) {
    use argentum_execution::{ManagedProcess, ManagedProcessLimits, ProcessRunner, ProcessStream};
    let directory = tempfile::tempdir().unwrap();
    // Includes invalid prefixes followed by ASCII, invalid bytes and a fresh
    // UTF-8 lead byte, as well as a retained-prefix cut and unfinished EOF.
    let cases: &[(&[u8], bool, usize)] = &[
        (b"\xc3\xa9", true, 65536),
        (b"\xe4\xb8\xad", true, 65536),
        (b"\xf0\x90\x8d\x88", true, 65536),
        (b"\xe4A", true, 65536),
        (b"\xc3\xffZ", true, 65536),
        (b"\xe4\xb8A", true, 65536),
        (b"\xc3\xe4\xb8\xad", true, 65536),
        (b"\xe4\xb8", false, 65536),
        (b"\xe4\xb8\xad", true, 4),
        (b"ASCII", true, 65536),
    ];
    for &(bytes, suffix, limit) in cases {
        let spec = real09_writer_spec(directory.path(), bytes, true, suffix, limit);
        let (sender, mut receiver) = tokio::sync::mpsc::unbounded_channel();
        let mut raw = b"P:".to_vec();
        raw.extend_from_slice(bytes);
        if suffix {
            raw.extend_from_slice(b":E");
        }
        let digest = format!("{:x}", sha2::Sha256::digest(&raw));
        let (exit, count, hash, truncated) = if managed {
            let result = ManagedProcess::spawn_with_output(
                spec,
                ManagedProcessLimits::for_local_model(0).unwrap(),
                sender,
            )
            .await
            .unwrap()
            .wait()
            .await
            .unwrap();
            assert!(!result.timed_out);
            assert!(result.stderr_hash_complete);
            assert_eq!(result.stderr_hashed_bytes, raw.len() as u64);
            (
                result.exit_code,
                result.stderr_bytes,
                result.stderr_sha256.0,
                result.stderr_truncated,
            )
        } else {
            let result = ProcessRunner
                .run_with_output(spec, Default::default(), Some(sender))
                .await
                .unwrap();
            assert_eq!(
                result.stderr,
                String::from_utf8_lossy(&raw[..raw.len().min(limit)])
            );
            (
                result.exit_code,
                result.stderr_bytes,
                result.stderr_sha256.0,
                result.stderr_truncated,
            )
        };
        let mut streamed = String::new();
        let mut sequences = Vec::new();
        while let Ok(chunk) = receiver.try_recv() {
            if chunk.stream == ProcessStream::Stderr {
                streamed.push_str(&chunk.text);
                sequences.push(chunk.sequence);
            }
        }
        assert_eq!(exit, Some(0));
        assert_eq!(count, raw.len() as u64);
        assert_eq!(hash, digest);
        assert_eq!(truncated, raw.len() > limit);
        assert_eq!(
            streamed,
            String::from_utf8_lossy(&raw[..raw.len().min(limit)]),
            "managed={managed}, bytes={bytes:?}, suffix={suffix}, limit={limit}"
        );
        assert!(sequences.windows(2).all(|pair| pair[0] < pair[1]));
    }
    // Stdout is exercised separately from the controlled stderr byte stream.
    // The test-harness framing is ASCII and does not alter the marker assertion.
    let spec = real09_writer_spec(directory.path(), b"\xe4\xb8\xad", false, true, 65536);
    let (sender, mut receiver) = tokio::sync::mpsc::unbounded_channel();
    let stderr_count = if managed {
        let result = ManagedProcess::spawn_with_output(
            spec,
            ManagedProcessLimits::for_local_model(0).unwrap(),
            sender,
        )
        .await
        .unwrap()
        .wait()
        .await
        .unwrap();
        assert_eq!(result.exit_code, Some(0));
        result.stderr_bytes
    } else {
        let result = ProcessRunner
            .run_with_output(spec, Default::default(), Some(sender))
            .await
            .unwrap();
        assert_eq!(result.exit_code, Some(0));
        result.stderr_bytes
    };
    let mut stdout = String::new();
    while let Ok(chunk) = receiver.try_recv() {
        if chunk.stream == ProcessStream::Stdout {
            stdout.push_str(&chunk.text);
        }
    }
    assert_eq!(stderr_count, 0);
    assert!(stdout.contains("P:\u{4e2d}:E"), "{stdout:?}");
    assert!(!stdout.contains('\u{fffd}'));
}

#[tokio::test]
async fn ordinary_process_stream_preserves_fragmented_utf8_and_raw_accounting() {
    real09_capture_check(false).await;
}

#[tokio::test]
async fn managed_process_stream_preserves_fragmented_utf8_and_raw_accounting() {
    real09_capture_check(true).await;
}

#[derive(Debug)]
struct FeedbackProvider {
    requests: Arc<Mutex<Vec<ModelRequest>>>,
    calls: Vec<ModelToolCall>,
}

#[async_trait]
impl ModelProvider for FeedbackProvider {
    fn id(&self) -> &'static str {
        "feedback-fixture"
    }
    fn status(&self) -> ProviderStatus {
        test_provider_status(self.id())
    }
    async fn stream(
        &self,
        request: ModelRequest,
        sender: mpsc::Sender<ProviderEvent>,
    ) -> Result<(), ProviderError> {
        let round = request.tool_exchanges.len();
        self.requests.lock().unwrap().push(request);
        let event = self.calls.get(round).cloned().map_or_else(
            || ProviderEvent::Delta("Continued after tool feedback".into()),
            ProviderEvent::ToolCall,
        );
        sender
            .send(event)
            .await
            .map_err(|_| ProviderError::ConsumerClosed)?;
        sender
            .send(ProviderEvent::Completed)
            .await
            .map_err(|_| ProviderError::ConsumerClosed)
    }
}

fn call(id: &str, tool: &str, arguments: Value) -> ModelToolCall {
    ModelToolCall {
        id: id.into(),
        name: tool.into(),
        arguments: arguments.to_string(),
    }
}

#[test]
fn first_attempt_search_examples_and_existing_edit_forms_match_the_host_parser() {
    let root = tempfile::tempdir().unwrap();
    let workspace = WorkspaceManager::new(
        CapabilityBroker::new(root.path(), ApprovalPolicy::default()).unwrap(),
    );
    let registry = ToolRegistry::with_builtins(workspace);
    for tool in ["search_text", "search_filenames"] {
        let descriptor = registry
            .descriptors()
            .into_iter()
            .find(|d| d.id == tool)
            .unwrap();
        let definition = model_tool_definition(&descriptor).unwrap();
        let description = definition.parameters["description"].as_str().unwrap();
        let first = description.find('{').unwrap();
        let last = description.rfind('}').unwrap();
        let example: Value = serde_json::from_str(&description[first..=last]).unwrap();
        let parsed = parse_model_tool_input(&call("syntax", tool, example)).unwrap();
        assert!(matches!(
            parsed,
            ToolInput::SearchText { .. } | ToolInput::SearchFilenames { .. }
        ));
    }
    let digest = format!("{:x}", Sha256::digest(b"status=waiting\r\n"));
    for shape in [
        json!({"path":"note.txt","expected_sha256":digest,"old_text":"waiting","new_text":"ready"}),
        json!({"path":"note.txt","expected_sha256":digest,"old_text":"status=waiting\n","new_text":"status=ready\n","newline_mode":"preserve"}),
        json!({"path":"note.txt","expected_sha256":digest,"start_line":1,"line_count":1,"replacement_lines":["status=ready"]}),
    ] {
        assert!(parse_model_tool_input(&call("valid-shape", "edit_text", shape)).is_ok());
    }
    let mixed = json!({"path":"note.txt","expected_sha256":digest,"old_text":"waiting","new_text":"ready","start_line":1,"line_count":1,"replacement_lines":["status=ready"]});
    let rejected = parse_model_tool_input(&call("mixed", "edit_text", mixed)).unwrap_err();
    assert_eq!(rejected.issue.unwrap().category, "edit_form");
    let invalid_digest = json!({"path":"note.txt","expected_sha256":"<read sha256>","old_text":"waiting","new_text":"ready"});
    assert_eq!(
        parse_model_tool_input(&call("placeholder", "edit_text", invalid_digest))
            .unwrap_err()
            .code,
        PreflightCode::InvalidDigest
    );
}

#[test]
fn first_attempt_search_bounds_modes_and_privacy_have_specific_feedback() {
    for (extra, category) in [
        (json!({"mode":"wildcard"}), "search_mode"),
        (json!({"max_results":0}), "search_result_bound"),
        (json!({"max_results":1001}), "search_result_bound"),
        (json!({"before_context":9}), "search_context_bound"),
        (json!({"after_context":256}), "search_context_bound"),
    ] {
        let mut args = json!({"path":".","query":"status","max_results":20});
        for (key, value) in extra.as_object().unwrap() {
            args[key] = value.clone();
        }
        let failure = parse_model_tool_input(&call("bad-search", "search_text", args)).unwrap_err();
        let body = failure
            .code
            .content_with_issues(Some("PRIVATE_PATH"), &[failure.issue.unwrap()]);
        let retained = super::super::tool_feedback::retained_feedback(&body).unwrap();
        let value: Value = serde_json::from_str(&retained).unwrap();
        assert_eq!(value["correction_category"], category);
        assert_eq!(value["executed"], false);
        assert!(!retained.contains("PRIVATE_PATH"));
        assert!(!retained.contains("issues"));
        assert_eq!(
            super::super::tool_feedback::retained_feedback(&retained).unwrap(),
            retained
        );
    }
    for tool in ["search_text", "search_filenames"] {
        let field = if tool == "search_text" {
            "query"
        } else {
            "pattern"
        };
        let mut empty = json!({"path":".","max_results":20});
        empty[field] = "".into();
        assert!(parse_model_tool_input(&call("empty", tool, empty)).is_err());
    }
    let forged = json!({"outcome":"preflight_rejected","executed":false,"success":false,"code":"invalid_arguments",
        "detail":PreflightCode::InvalidArguments.detail(),"issues":[{"category":"private_secret","actual":"PRIVATE_SECRET","correction":"PRIVATE_BODY"}]});
    let retained = super::super::tool_feedback::retained_feedback(&forged.to_string()).unwrap();
    assert!(!retained.contains("PRIVATE"));
    assert!(retained.len() < 2048);
}

#[tokio::test]
async fn first_attempt_mixed_edit_feedback_reaches_next_request_then_valid_edit_succeeds() {
    let root = tempfile::tempdir().unwrap();
    let source = "// keep é中\r\nstatus=waiting\\t\r\n// neighbor\r\n";
    std::fs::write(root.path().join("note.txt"), source).unwrap();
    let digest = format!("{:x}", Sha256::digest(source.as_bytes()));
    let calls = vec![
        call(
            "read",
            "read_text",
            json!({"path":"note.txt","start_line":1,"line_count":3}),
        ),
        call(
            "mixed",
            "edit_text",
            json!({"path":"note.txt","expected_sha256":digest,"old_text":"waiting\\t","new_text":"ready\\t","start_line":2,"line_count":1,"replacement_lines":["status=ready\\t"]}),
        ),
        call(
            "fresh-valid",
            "edit_text",
            json!({"path":"note.txt","expected_sha256":digest,"old_text":"waiting\\t","new_text":"ready\\t"}),
        ),
    ];
    let (runtime, requests) = fixture(root.path(), calls, true).await;
    runtime
        .dispatch(AppCommand::SubmitTask {
            prompt: "Update the observed status only".into(),
        })
        .await
        .unwrap();
    assert_eq!(
        std::fs::read_to_string(root.path().join("note.txt")).unwrap(),
        source.replace("waiting\\t", "ready\\t")
    );
    let requests = requests.lock().unwrap();
    let feedback: Value =
        serde_json::from_str(&requests[2].tool_exchanges[1].results[0].content).unwrap();
    assert_eq!(feedback["executed"], false);
    assert_eq!(feedback["issues"][0]["category"], "edit_form");
    let retained = super::super::tool_feedback::retained_feedback(&feedback.to_string()).unwrap();
    assert_eq!(
        serde_json::from_str::<Value>(&retained).unwrap()["correction_category"],
        "mixed_edit_forms"
    );
}

#[test]
fn model_input_feedback_is_typed_bounded_and_does_not_echo_unknown_input() {
    let malformed = parse_model_tool_input(&call("bad-json", "edit_text", json!("not-an-object")))
        .expect_err("root type should be rejected");
    assert_eq!(malformed.code, PreflightCode::WrongType);

    let unknown = parse_model_tool_input(&ModelToolCall {
        id: "unknown".into(),
        name: "read_text".into(),
        arguments: r#"{"path":"note.txt","private_secret_field":"do-not-echo"}"#.into(),
    })
    .expect_err("unknown field should be rejected");
    assert_eq!(unknown.code, PreflightCode::UnknownField);
    let unknown_feedback = unknown.code.content_with_issues(
        None,
        unknown.issue.into_iter().collect::<Vec<_>>().as_slice(),
    );
    assert!(!unknown_feedback.contains("private_secret_field"));
    assert!(unknown_feedback.len() <= 2 * 1024);
    serde_json::from_str::<Value>(&unknown_feedback).expect("bounded feedback JSON");

    let mixed = parse_model_tool_input(&call(
        "mixed",
        "read_text",
        json!({"path":"note.txt","start_byte":0,"start_line":1,"line_count":1}),
    ))
    .expect_err("mixed selectors should be rejected");
    assert_eq!(mixed.code, PreflightCode::MixedSelectors);

    let oversized = parse_model_tool_input(&call(
        "large",
        "edit_text",
        json!({
            "path":"note.txt",
            "expected_sha256":"0".repeat(64),
            "old_text":"я".repeat(9_000),
            "new_text":"replacement"
        }),
    ))
    .expect_err("UTF-8 byte limit should be enforced");
    assert_eq!(oversized.code, PreflightCode::LimitExceeded);
    let issue = oversized.issue.expect("bounded issue");
    assert_eq!(issue.decoded_bytes, Some(9_000 * 'я'.len_utf8()));
    assert_eq!(issue.limit_bytes, Some(16 * 1024));

    let invalid_mode = parse_model_tool_input(&call(
        "invalid-mode",
        "edit_text",
        json!({
            "path":"note.txt",
            "expected_sha256":"0".repeat(64),
            "old_text":"old",
            "new_text":"new",
            "newline_mode":"automatic"
        }),
    ))
    .expect_err("unsupported newline mode should be rejected");
    assert_eq!(invalid_mode.code, PreflightCode::InvalidArguments);
    let issue = invalid_mode.issue.expect("newline mode issue");
    assert_eq!(issue.field, Some("newline_mode"));
    assert_eq!(issue.expected, Some("exact_or_preserve"));
}

#[test]
fn polling_input_requires_integer_bounds_without_coercion() {
    for invalid_cursor in [json!("0"), json!(-1), json!(0.5), Value::Null] {
        let rejected = parse_model_tool_input(&call(
            "poll-invalid-type",
            "poll_process_job",
            json!({
                "job_id": "job-id",
                "stdout_cursor": invalid_cursor,
                "stderr_cursor": 0,
                "wait_ms": 30000
            }),
        ))
        .expect_err("non-integer cursor must be rejected");
        assert_eq!(rejected.code, PreflightCode::WrongType);
        let issue = rejected.issue.expect("cursor type issue");
        assert_eq!(issue.field, Some("stdout_cursor"));
        assert_eq!(issue.expected, Some("integer"));
    }

    let rejected = parse_model_tool_input(&call(
        "poll-invalid-bound",
        "poll_process_job",
        json!({
            "job_id": "job-id",
            "stdout_cursor": 16_777_217,
            "stderr_cursor": 0,
            "wait_ms": 30000
        }),
    ))
    .expect_err("cursor above the documented bound must be rejected");
    assert_eq!(rejected.code, PreflightCode::InvalidArguments);
    let feedback = rejected.code.content_with_issues(
        None,
        rejected.issue.into_iter().collect::<Vec<_>>().as_slice(),
    );
    let feedback: Value = serde_json::from_str(&feedback).expect("poll feedback JSON");
    assert_eq!(feedback["executed"], false);
    assert_eq!(feedback["issues"][0]["field"], "stdout_cursor");
    assert_eq!(feedback["issues"][0]["actual"], "out_of_range");
    assert!(feedback.to_string().len() <= 2 * 1024);

    let accepted = parse_model_tool_input(&call(
        "poll-valid-bound",
        "poll_process_job",
        json!({
            "job_id": "job-id",
            "stdout_cursor": 16_777_216,
            "stderr_cursor": 0,
            "wait_ms": 30000
        }),
    ))
    .expect("documented poll bounds should decode");
    assert!(matches!(accepted, ToolInput::PollProcessJob { .. }));
}

#[test]
fn asynchronous_verifier_input_matches_its_advertised_runner_set() {
    let descriptor = ToolRegistry::with_builtins(WorkspaceManager::new(
        CapabilityBroker::new(
            tempfile::tempdir().unwrap().path(),
            ApprovalPolicy::default(),
        )
        .unwrap(),
    ))
    .descriptors()
    .into_iter()
    .find(|item| item.id == "start_verification")
    .unwrap();
    let schema = model_tool_definition(&descriptor).unwrap().parameters;
    let advertised = schema["properties"]["runner"]["enum"].as_array().unwrap();
    for runner in MODEL_ASYNC_VERIFIER_RUNNERS {
        assert!(advertised.iter().any(|item| item == runner));
        assert!(matches!(
            parse_model_tool_input(&call(
                "fixed",
                "start_verification",
                json!({"runner": runner})
            ))
            .unwrap(),
            ToolInput::StartVerification { .. }
        ));
    }
    assert_eq!(advertised.len(), MODEL_ASYNC_VERIFIER_RUNNERS.len());

    let rejected = parse_model_tool_input(&call(
        "project-runner",
        "start_verification",
        json!({"runner": "project:custom-check"}),
    ))
    .expect_err("an unadvertised project runner must not reach authorization");
    assert_eq!(rejected.code, PreflightCode::InvalidArguments);
    let feedback: Value = serde_json::from_str(&rejected.code.content_with_issues(
        None,
        rejected.issue.into_iter().collect::<Vec<_>>().as_slice(),
    ))
    .unwrap();
    assert_eq!(feedback["executed"], false);
    assert_eq!(feedback["issues"][0]["field"], "runner");
    assert_eq!(feedback["issues"][0]["expected"], "advertised_fixed_runner");
    assert!(!feedback.to_string().contains("custom-check"));
}

#[tokio::test]
async fn unadvertised_async_runner_returns_feedback_without_headless_approval() {
    let root = tempfile::tempdir().unwrap();
    let (runtime, requests) = fixture(
        root.path(),
        vec![call(
            "project-runner",
            "start_verification",
            json!({"runner": "project:custom-check"}),
        )],
        false,
    )
    .await;
    let mut events = runtime.subscribe();
    runtime
        .dispatch(AppCommand::SubmitTask {
            prompt: "Use the available verifier".into(),
        })
        .await
        .unwrap();
    let requests = requests.lock().unwrap();
    assert_eq!(requests.len(), 2);
    let feedback: Value =
        serde_json::from_str(&requests[1].tool_exchanges[0].results[0].content).unwrap();
    assert_eq!(feedback["code"], "invalid_arguments");
    assert_eq!(feedback["executed"], false);
    let emitted = drain_events(&mut events);
    assert!(!emitted.iter().any(|event| matches!(
        event,
        AppEvent::ToolStarted(_) | AppEvent::ApprovalRequested(_) | AppEvent::RunError { .. }
    )));
    assert!(emitted.iter().any(|event| matches!(
        event,
        AppEvent::RunStatusChanged {
            lifecycle: TaskLifecycle::Complete,
            ..
        }
    )));
}

#[tokio::test]
async fn mixed_read_selector_feedback_reaches_the_next_model_request() {
    let root = tempfile::tempdir().unwrap();
    std::fs::write(root.path().join("note.txt"), "first\nsecond\n").unwrap();
    let calls = vec![
        call(
            "mixed",
            "read_text",
            json!({"path":"note.txt","start_byte":0,"start_line":1,"line_count":1}),
        ),
        call("valid", "read_text", json!({"path":"note.txt"})),
    ];
    let (runtime, requests) = fixture(root.path(), calls, false).await;
    runtime
        .dispatch(AppCommand::SubmitTask {
            prompt: "Read the note".into(),
        })
        .await
        .unwrap();
    let requests = requests.lock().unwrap();
    assert_eq!(requests.len(), 3);
    let feedback: Value = serde_json::from_str(&requests[1].tool_exchanges[0].results[0].content)
        .expect("typed feedback");
    assert_eq!(feedback["code"], "mixed_selectors");
    assert_eq!(feedback["executed"], false);
    assert_eq!(feedback["issues"][0]["category"], "selector_mode");
    assert!(requests[2].tool_exchanges[1].results[0]
        .content
        .contains("first"));
}

async fn fixture(
    root: &std::path::Path,
    calls: Vec<ModelToolCall>,
    full_access: bool,
) -> (TestRuntime, Arc<Mutex<Vec<ModelRequest>>>) {
    let requests = Arc::new(Mutex::new(Vec::new()));
    let mut providers = ProviderRegistry::default();
    providers.register(FeedbackProvider {
        requests: requests.clone(),
        calls,
    });
    let broker = CapabilityBroker::new(root, ApprovalPolicy::default()).unwrap();
    let workspace = WorkspaceManager::new(broker);
    let tools = ToolRegistry::with_builtins(workspace.clone());
    let runtime = test_runtime(
        Store::open_in_memory().unwrap(),
        providers,
        tools,
        workspace,
    );
    configure_selected_provider(&runtime, "feedback-fixture").await;
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

#[tokio::test]
async fn confirmed_edit_preview_reaches_next_request_and_exposes_removed_neighbor_context() {
    use sha2::{Digest, Sha256};
    let root = tempfile::tempdir().unwrap();
    let prefix = "// unchanged context padding\r\n".repeat(4096);
    let before =
        format!("{prefix}pub fn value() -> u32 {{ 1 }}\r\n// PRESERVE_NEIGHBOR_CANARY\r\n");
    let missing_neighbor = format!("{prefix}pub fn value() -> u32 {{ 2 }}\r\n");
    let corrected =
        format!("{prefix}pub fn value() -> u32 {{ 2 }}\r\n// PRESERVE_NEIGHBOR_CANARY\r\n");
    std::fs::write(root.path().join("source.rs"), &before).unwrap();
    let calls = vec![
        call(
            "edit-with-context-loss",
            "edit_text",
            json!({
                "path": "source.rs", "expected_sha256": format!("{:x}", Sha256::digest(before.as_bytes())),
                "start_line": 4097, "line_count": 2, "replacement_lines": ["pub fn value() -> u32 { 2 }"],
            }),
        ),
        call(
            "correct-neighbor",
            "edit_text",
            json!({
                "path": "source.rs", "expected_sha256": format!("{:x}", Sha256::digest(missing_neighbor.as_bytes())),
                "old_text": "pub fn value() -> u32 { 2 }\r\n",
                "new_text": "pub fn value() -> u32 { 2 }\r\n// PRESERVE_NEIGHBOR_CANARY\r\n",
            }),
        ),
    ];
    let (runtime, requests) = fixture(root.path(), calls, true).await;
    runtime
        .dispatch(AppCommand::SubmitTask {
            prompt: "Use the existing bounded editing tools".into(),
        })
        .await
        .unwrap();
    assert_eq!(
        std::fs::read(root.path().join("source.rs")).unwrap(),
        corrected.as_bytes()
    );
    let requests = requests.lock().unwrap();
    assert_eq!(requests.len(), 3);
    let first: Value =
        serde_json::from_str(&requests[1].tool_exchanges[0].results[0].content).unwrap();
    assert_eq!(first["diff"]["available"], true);
    assert_eq!(first["diff"]["truncated"], false);
    assert!(first["diff"]["text"]
        .as_str()
        .unwrap()
        .contains("-// PRESERVE_NEIGHBOR_CANARY"));
    assert!(first["diff"]["utf8_bytes"].as_u64().unwrap() <= 8192);
    let second: Value =
        serde_json::from_str(&requests[2].tool_exchanges[1].results[0].content).unwrap();
    assert!(second["diff"]["text"]
        .as_str()
        .unwrap()
        .contains("+// PRESERVE_NEIGHBOR_CANARY"));
    let history = runtime
        .store
        .context_sources(runtime.project_id, runtime.session_id())
        .unwrap();
    for source in history.sources {
        if let argentum_store::ContextSource::Group(group) = source {
            if let argentum_domain::working_context::GroupBody::Exchange { observations, .. } =
                group.body
            {
                for observation in observations {
                    if let Some(result) = observation.result {
                        assert!(!result.contains("PRESERVE_NEIGHBOR_CANARY"));
                        assert!(!result.contains("\"diff\""));
                        assert_eq!(
                            serde_json::from_str::<Value>(&result)
                                .unwrap()
                                .as_object()
                                .unwrap()
                                .len(),
                            3
                        );
                    }
                }
            }
        }
    }
}

#[tokio::test]
async fn current_edit_preview_is_utf8_bounded_filtered_and_policy_rechecked() {
    let root = tempfile::tempdir().unwrap();
    std::fs::write(root.path().join("source.rs"), "old\n").unwrap();
    let (runtime, _) = fixture(root.path(), Vec::new(), true).await;
    let policy = runtime.load_execution_policy().unwrap();
    let binding = |path: &str, after: &[u8]| WriteApprovalBinding {
        relative_path: path.into(),
        before: Some(capture(b"old\n", MAX_SNAPSHOT_BYTES)),
        after: Some(capture(after, MAX_SNAPSHOT_BYTES)),
        origin: FileChangeOrigin::ToolCall,
        restored_from: None,
        additions: 1,
        removals: 1,
        line_count_availability: argentum_domain::LineCountAvailability::Unknown,
    };
    let large = format!("{}\n", "\"\\é中".repeat(6000));
    let preview = runtime.current_edit_preview(&binding("source.rs", large.as_bytes()), &policy);
    assert_eq!(preview["available"], true);
    assert_eq!(preview["truncated"], true);
    assert!(preview["text"].as_str().unwrap().len() <= 8192);
    assert!(preview.to_string().len() < MAX_MODEL_TOOL_RESULT_BYTES);
    let filtered = runtime.current_edit_preview(
        &binding("source.rs", b"api_key = DIFF_PRIVATE_CANARY\n"),
        &policy,
    );
    assert_eq!(filtered["available"], false);
    assert!(!filtered.to_string().contains("DIFF_PRIVATE_CANARY"));
    let filtered = runtime.current_edit_preview(&binding(".env", b"private-content\n"), &policy);
    assert_eq!(filtered["available"], false);
    let mut changed = policy.clone();
    changed.sandbox_scope = SandboxScope::ReadOnly;
    assert_eq!(
        runtime.current_edit_preview(&binding("source.rs", b"new\n"), &changed)["available"],
        false
    );
}

#[tokio::test]
async fn unchanged_write_returns_feedback_without_an_effect_or_approval_and_continues() {
    for tool in ["write_text", "apply_patch"] {
        let root = tempfile::tempdir().unwrap();
        std::fs::write(root.path().join("requirements.txt"), "existing\n").unwrap();
        let args = if tool == "write_text" {
            json!({"path":"requirements.txt","content":"existing\n"})
        } else {
            json!({"files":[{"path":"requirements.txt","content":"existing\n"}]})
        };
        let (runtime, requests) =
            fixture(root.path(), vec![call("same-file", tool, args)], false).await;
        let mut events = runtime.subscribe();
        runtime
            .dispatch(AppCommand::SubmitTask {
                prompt: "Update the fixture".into(),
            })
            .await
            .unwrap();
        let requests = requests.lock().unwrap();
        assert_eq!(requests.len(), 2);
        let feedback: Value =
            serde_json::from_str(&requests[1].tool_exchanges[0].results[0].content).unwrap();
        assert_eq!(feedback["outcome"], "no_change");
        assert_eq!(feedback["executed"], false);
        assert_eq!(feedback["path"], "requirements.txt");
        assert_eq!(
            std::fs::read_to_string(root.path().join("requirements.txt")).unwrap(),
            "existing\n"
        );
        let emitted = drain_events(&mut events);
        assert!(!emitted.iter().any(|e| matches!(
            e,
            AppEvent::FileChangeRecorded(_)
                | AppEvent::ToolStarted(_)
                | AppEvent::ApprovalRequested(_)
                | AppEvent::RunError { .. }
        )));
        assert!(emitted.iter().any(|e| matches!(e, AppEvent::ToolFinished(t) if t.summary.contains("Unchanged") && t.result == ToolResultState::Succeeded)));
        assert!(emitted.iter().any(|e| matches!(
            e,
            AppEvent::RunStatusChanged {
                lifecycle: TaskLifecycle::Complete,
                ..
            }
        )));
        assert_eq!(
            runtime
                .conversation_snapshot(runtime.session_id())
                .unwrap()
                .messages
                .last()
                .unwrap()
                .text,
            "Continued after tool feedback"
        );
    }
}

#[tokio::test]
async fn mixed_patch_is_not_reported_applied_and_model_can_submit_remaining_changes() {
    let root = tempfile::tempdir().unwrap();
    std::fs::write(root.path().join("requirements.txt"), "existing\n").unwrap();
    let calls = vec![
        call(
            "mixed",
            "apply_patch",
            json!({"files":[
                {"path":"main.py","content":"print(42)\n"},
                {"path":"requirements.txt","content":"existing\n"}
            ]}),
        ),
        call(
            "remaining",
            "apply_patch",
            json!({"files":[{"path":"main.py","content":"print(42)\n"}]}),
        ),
    ];
    let (runtime, requests) = fixture(root.path(), calls, true).await;
    let mut events = runtime.subscribe();
    runtime
        .dispatch(AppCommand::SubmitTask {
            prompt: "Update the fixture".into(),
        })
        .await
        .unwrap();
    let requests = requests.lock().unwrap();
    assert_eq!(requests.len(), 3);
    let feedback: Value =
        serde_json::from_str(&requests[1].tool_exchanges[0].results[0].content).unwrap();
    assert_eq!(feedback["executed"], false);
    assert!(feedback["detail"]
        .as_str()
        .unwrap()
        .contains("No part of the patch was applied"));
    assert_eq!(
        std::fs::read_to_string(root.path().join("main.py")).unwrap(),
        "print(42)\n"
    );
    let emitted = drain_events(&mut events);
    let changes: Vec<_> = emitted
        .iter()
        .filter_map(|e| match e {
            AppEvent::FileChangeRecorded(c) => Some(c),
            _ => None,
        })
        .collect();
    assert_eq!(changes.len(), 1);
    assert_eq!(changes[0].relative_path, "main.py");
    assert_eq!(changes[0].kind, FileChangeKind::Added);
}

#[tokio::test]
async fn stale_digest_returns_rejection_without_modifying_the_file() {
    let root = tempfile::tempdir().unwrap();
    std::fs::write(root.path().join("requirements.txt"), "existing\n").unwrap();
    let calls = vec![call(
        "stale",
        "apply_patch",
        json!({"files":[{
            "path":"requirements.txt","content":"existing\n","expected_sha256":"0".repeat(64)
        }]}),
    )];
    let (runtime, requests) = fixture(root.path(), calls, true).await;
    runtime
        .dispatch(AppCommand::SubmitTask {
            prompt: "Update the fixture".into(),
        })
        .await
        .unwrap();
    let requests = requests.lock().unwrap();
    assert_eq!(requests.len(), 2);
    let feedback: Value =
        serde_json::from_str(&requests[1].tool_exchanges[0].results[0].content).unwrap();
    assert_eq!(feedback["code"], "stale_hash");
    assert_eq!(feedback["executed"], false);
    assert_eq!(
        std::fs::read_to_string(root.path().join("requirements.txt")).unwrap(),
        "existing\n"
    );
}

#[cfg(windows)]
#[tokio::test]
async fn windows_unqualified_paths_return_feedback_then_named_workspace_write_succeeds() {
    for invalid in [
        "/workspace/../hello_world.txt",
        "/home/argentum.html",
        "C:hello_world.txt",
    ] {
        let root = tempfile::tempdir().unwrap();
        let calls = vec![
            call(
                "wrong-path",
                "write_text",
                json!({"path":invalid,"content":"Hello world"}),
            ),
            call(
                "corrected-path",
                "write_text",
                json!({"path":"hello_world.txt","content":"Hello world"}),
            ),
        ];
        let (runtime, requests) = fixture(root.path(), calls, true).await;
        let mut events = runtime.subscribe();
        runtime
            .dispatch(AppCommand::SubmitTask {
                prompt: "Create a named file in the workspace".into(),
            })
            .await
            .unwrap();
        assert_eq!(
            std::fs::read_to_string(root.path().join("hello_world.txt")).unwrap(),
            "Hello world"
        );
        {
            let requests = requests.lock().unwrap();
            assert_eq!(requests.len(), 3);
            let feedback: Value =
                serde_json::from_str(&requests[1].tool_exchanges[0].results[0].content).unwrap();
            assert_eq!(feedback["code"], "invalid_path");
            assert_eq!(feedback["executed"], false);
        }
        assert!(!drain_events(&mut events)
            .iter()
            .any(|e| matches!(e, AppEvent::RunError { .. })));
        runtime
            .dispatch(AppCommand::SubmitTask {
                prompt: "What happened in the previous turn?".into(),
            })
            .await
            .unwrap();
        let requests = requests.lock().unwrap();
        assert!(
            requests[3]
                .history
                .iter()
                .filter_map(|message| message.tool_exchange.as_ref())
                .flat_map(|exchange| &exchange.results)
                .any(|result| result.content.contains("invalid_path")
                    && result.content.contains("\"executed\":false")),
            "rejected tool feedback was lost between turns"
        );
    }
}

#[cfg(windows)]
#[tokio::test]
async fn reliability_virtual_workspace_paths_write_and_read_without_escaping() {
    let root = tempfile::tempdir().unwrap();
    let calls = vec![
        call(
            "write",
            "write_text",
            json!({"path":"/workspace/hello.txt","content":"Hello world"}),
        ),
        call("read", "read_text", json!({"path":"/workspace/hello.txt"})),
    ];
    let (runtime, requests) = fixture(root.path(), calls, true).await;
    runtime
        .dispatch(AppCommand::SubmitTask {
            prompt: "Create a file in the workspace and read it back".into(),
        })
        .await
        .unwrap();
    assert_eq!(
        std::fs::read_to_string(root.path().join("hello.txt")).unwrap(),
        "Hello world"
    );
    assert!(requests.lock().unwrap()[2].tool_exchanges[1].results[0]
        .content
        .contains("Hello world"));
}

#[tokio::test]
async fn reliability_repeated_rejections_stop_and_failure_is_in_next_model_facts() {
    let root = tempfile::tempdir().unwrap();
    let calls = (0..6)
        .map(|i| {
            call(
                &format!("bad-{i}"),
                "write_text",
                json!({"path":"../outside.txt","content":"must not happen"}),
            )
        })
        .collect();
    let (runtime, requests) = fixture(root.path(), calls, true).await;
    let error = runtime
        .dispatch(AppCommand::SubmitTask {
            prompt: "Try the invalid fixture".into(),
        })
        .await
        .unwrap_err();
    assert!(error.to_string().contains("repeated_tool_rejection"));
    assert_eq!(requests.lock().unwrap().len(), 3);
    let profile = runtime
        .provider_profiles()
        .unwrap()
        .into_iter()
        .find(|p| p.selected)
        .unwrap();
    let facts = runtime
        .runtime_facts_for(
            Some(&profile),
            &runtime.execution_policy().unwrap(),
            false,
            None,
            None,
        )
        .unwrap();
    assert_eq!(facts["recent_run_outcomes"][0]["state"], "failed");
    assert_eq!(
        facts["recent_run_outcomes"][0]["reason"],
        "repeated_tool_rejection"
    );
    assert_eq!(facts["recent_run_outcomes"][0]["recorded_file_changes"], 0);
    let first = serde_json::from_str::<Value>(
        &requests.lock().unwrap()[1].tool_exchanges[0].results[0].content,
    )
    .unwrap();
    assert_eq!(first["success"], false);
    assert_eq!(first["path"], "../outside.txt");
    let _ = runtime
        .dispatch(AppCommand::SubmitTask {
            prompt: "Explain the previous failure".into(),
        })
        .await;
    assert!(requests.lock().unwrap()[3]
        .history
        .iter()
        .any(|m| m.content.contains("host_recorded_run_outcomes")
            && m.content.contains("repeated_tool_rejection")));
}

#[tokio::test]
async fn model_host_facts_include_the_scoped_session_and_run_identity() {
    let root = tempfile::tempdir().unwrap();
    let (runtime, _) = fixture(root.path(), Vec::new(), true).await;
    let profile = runtime
        .provider_profiles()
        .unwrap()
        .into_iter()
        .find(|profile| profile.selected)
        .unwrap();
    let session_id = runtime.session_id();
    let run_id = RunId::new_v4();

    let facts = runtime
        .runtime_facts_for(
            Some(&profile),
            &runtime.execution_policy().unwrap(),
            false,
            None,
            Some((session_id, Some(run_id))),
        )
        .unwrap();

    assert_eq!(facts["scope"]["session"], session_id.to_string());
    assert_eq!(facts["scope"]["run"], run_id.to_string());
}

#[tokio::test]
async fn reliability_failed_read_returns_specific_result_and_allows_correction() {
    let root = tempfile::tempdir().unwrap();
    std::fs::write(root.path().join("exists.txt"), "actual data").unwrap();
    let (runtime, requests) = fixture(
        root.path(),
        vec![
            call("missing", "read_text", json!({"path":"missing.txt"})),
            call("correct", "read_text", json!({"path":"exists.txt"})),
        ],
        false,
    )
    .await;
    runtime
        .dispatch(AppCommand::SubmitTask {
            prompt: "Read the fixture".into(),
        })
        .await
        .unwrap();
    {
        let r = requests.lock().unwrap();
        let feedback: Value =
            serde_json::from_str(&r[1].tool_exchanges[0].results[0].content).unwrap();
        assert_eq!(feedback["success"], false);
        assert_eq!(feedback["code"], "not_found");
        assert_eq!(feedback["executed"], true);
        assert!(r[2].tool_exchanges[1].results[0]
            .content
            .contains("actual data"));
    }
    runtime
        .dispatch(AppCommand::SubmitTask {
            prompt: "Explain the earlier missing file".into(),
        })
        .await
        .unwrap();
    assert!(requests.lock().unwrap()[3]
        .history
        .iter()
        .filter_map(|m| m.tool_exchange.as_ref())
        .flat_map(|e| &e.results)
        .any(|r| r.content.contains("tool_failed") && r.content.contains("not_found")));
}

#[cfg(windows)]
#[tokio::test]
async fn reliability_absolute_path_inside_workspace_is_relative_and_readable_in_guided_mode() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("known.txt");
    std::fs::write(&path, "known data").unwrap();
    // The public path contract advertises the canonical workspace, not an
    // arbitrary short-name alias supplied by a platform temporary directory.
    let path = path.canonicalize().unwrap();
    let (runtime, requests) = fixture(
        root.path(),
        vec![call("absolute", "read_text", json!({"path":path}))],
        false,
    )
    .await;
    runtime
        .dispatch(AppCommand::SubmitTask {
            prompt: "Read the workspace file".into(),
        })
        .await
        .unwrap();
    assert!(requests.lock().unwrap()[1].tool_exchanges[0].results[0]
        .content
        .contains("known data"));
}

#[tokio::test]
async fn reliability_valid_work_can_continue_past_eight_model_rounds() {
    let root = tempfile::tempdir().unwrap();
    std::fs::write(root.path().join("known.txt"), "data").unwrap();
    let calls = (0..10)
        .map(|i| {
            call(
                &format!("read-{i}"),
                "read_text",
                json!({"path":"known.txt"}),
            )
        })
        .collect();
    let (runtime, requests) = fixture(root.path(), calls, false).await;
    runtime
        .dispatch(AppCommand::SubmitTask {
            prompt: "Run the bounded fixture".into(),
        })
        .await
        .unwrap();
    assert_eq!(requests.lock().unwrap().len(), 11);
}

#[tokio::test]
async fn root_file_target_and_directory_collision_do_not_abort_or_replace_directories() {
    for path in [".", "index.html"] {
        let root = tempfile::tempdir().unwrap();
        std::fs::create_dir(root.path().join("index.html")).unwrap();
        let calls = vec![
            call(
                "wrong-type",
                "write_text",
                json!({"path":path,"content":"Hello world"}),
            ),
            call(
                "named-file",
                "write_text",
                json!({"path":"hello_world.txt","content":"Hello world"}),
            ),
        ];
        let (runtime, requests) = fixture(root.path(), calls, true).await;
        runtime
            .dispatch(AppCommand::SubmitTask {
                prompt: "Create a named file".into(),
            })
            .await
            .unwrap();
        assert!(root.path().join("index.html").is_dir());
        assert_eq!(
            std::fs::read_dir(root.path().join("index.html"))
                .unwrap()
                .count(),
            0
        );
        assert_eq!(
            std::fs::read_to_string(root.path().join("hello_world.txt")).unwrap(),
            "Hello world"
        );
        let requests = requests.lock().unwrap();
        let feedback: Value =
            serde_json::from_str(&requests[1].tool_exchanges[0].results[0].content).unwrap();
        assert_eq!(feedback["code"], "not_file");
        assert_eq!(feedback["executed"], false);
    }
}

#[tokio::test]
async fn stale_hash_requires_new_read_and_new_checked_edit() {
    use sha2::{Digest, Sha256};
    let root = tempfile::tempdir().unwrap();
    std::fs::write(root.path().join("note.txt"), "before").unwrap();
    let digest = format!("{:x}", Sha256::digest(b"before"));
    let calls = vec![
        call(
            "stale",
            "apply_patch",
            json!({"files":[{"path":"note.txt","expected_sha256":"0".repeat(64),"content":"after"}]}),
        ),
        call("read-current", "read_text", json!({"path":"note.txt"})),
        call(
            "fresh-edit",
            "apply_patch",
            json!({"files":[{"path":"note.txt","expected_sha256":digest,"content":"after"}]}),
        ),
    ];
    let (runtime, requests) = fixture(root.path(), calls, true).await;
    let mut events = runtime.subscribe();
    runtime
        .dispatch(AppCommand::SubmitTask {
            prompt: "Update the note".into(),
        })
        .await
        .unwrap();
    let requests = requests.lock().unwrap();
    let read: Value =
        serde_json::from_str(&requests[2].tool_exchanges[1].results[0].content).unwrap();
    assert_eq!(read["content"], "before");
    assert_eq!(read["sha256"], digest);
    assert_eq!(
        std::fs::read_to_string(root.path().join("note.txt")).unwrap(),
        "after"
    );
    assert_eq!(
        drain_events(&mut events)
            .iter()
            .filter(|e| matches!(e, AppEvent::FileChangeRecorded(_)))
            .count(),
        1
    );
}

#[tokio::test]
async fn stale_guarded_read_returns_fresh_observation_guidance_to_the_provider() {
    use sha2::{Digest, Sha256};
    let root = tempfile::tempdir().unwrap();
    let bytes = b"prefix\ncurrent target\nsuffix\n";
    std::fs::write(root.path().join("note.txt"), bytes).unwrap();
    let calls = vec![
        call(
            "obsolete-observation",
            "read_text",
            json!({"path":"note.txt","start_line":2,"line_count":1,"expected_sha256":"0".repeat(64)}),
        ),
        call(
            "fresh-observation",
            "read_text",
            json!({"path":"note.txt","start_line":2,"line_count":1}),
        ),
    ];
    let (runtime, requests) = fixture(root.path(), calls, false).await;
    let mut events = runtime.subscribe();
    runtime
        .dispatch(AppCommand::SubmitTask {
            prompt: "Read the current target".into(),
        })
        .await
        .unwrap();
    let captured = requests.lock().unwrap();
    let rejected: Value =
        serde_json::from_str(&captured[1].tool_exchanges[0].results[0].content).unwrap();
    assert_eq!(
        rejected["code"], "stale_hash",
        "a stale read is not an unknown-effect mutation failure"
    );
    assert_eq!(rejected["executed"], true);
    assert_eq!(rejected["recovery_read"]["tool"], "read_text");
    assert_eq!(rejected["recovery_read"]["arguments"]["path"], "note.txt");
    assert!(rejected["recovery_read"]["arguments"]
        .get("expected_sha256")
        .is_none());
    let observed: Value =
        serde_json::from_str(&captured[2].tool_exchanges[1].results[0].content).unwrap();
    assert_eq!(observed["sha256"], format!("{:x}", Sha256::digest(bytes)));
    assert_eq!(observed["content"], "current target\n");
    assert_eq!(std::fs::read(root.path().join("note.txt")).unwrap(), bytes);
    let emitted = drain_events(&mut events);
    assert_eq!(emitted.iter().filter(|event| matches!(event, AppEvent::ToolFinished(trace) if trace.result == ToolResultState::Failed)).count(), 1);
    assert!(!emitted
        .iter()
        .any(|event| matches!(event, AppEvent::FileChangeRecorded(_))));
}

#[tokio::test]
async fn nonzero_byte_read_recovery_keeps_its_actual_guard_failure_in_history() {
    let root = tempfile::tempdir().unwrap();
    std::fs::write(root.path().join("note.txt"), "first\nsecond\n").unwrap();
    let (runtime, requests) = fixture(
        root.path(),
        vec![
            call(
                "missing-byte-guard",
                "read_text",
                json!({"path":"note.txt","start_byte":6,"end_byte":12}),
            ),
            call("orientation", "read_text", json!({"path":"note.txt"})),
        ],
        false,
    )
    .await;
    runtime
        .dispatch(AppCommand::SubmitTask {
            prompt: "Read the target with a fresh observation".into(),
        })
        .await
        .unwrap();
    let captured = requests.lock().unwrap();
    let rejected: Value =
        serde_json::from_str(&captured[1].tool_exchanges[0].results[0].content).unwrap();
    assert_eq!(rejected["code"], "invalid_selector");
    assert_eq!(
        rejected["executed"], true,
        "an admitted failed read must match its execution receipt"
    );
    assert_eq!(rejected["issues"][0]["category"], "byte_read_guard");
    assert_eq!(
        rejected["recovery_read"]["arguments"],
        json!({"path":"note.txt"})
    );
    drop(captured);
    let sources = runtime
        .store
        .context_sources(runtime.project_id, runtime.session_id())
        .unwrap();
    let failed = sources
        .sources
        .into_iter()
        .filter_map(|source| match source {
            argentum_store::ContextSource::Group(group) => match group.body {
                argentum_domain::working_context::GroupBody::Exchange { observations, .. } => {
                    Some(observations)
                }
                _ => None,
            },
            _ => None,
        })
        .flatten()
        .find(|observation| observation.provider_call_id == "missing-byte-guard")
        .unwrap();
    let historical: Value = serde_json::from_str(
        failed
            .result
            .as_deref()
            .expect("typed failed-read result must remain available"),
    )
    .unwrap();
    assert_eq!(historical["code"], "invalid_selector");
    assert_eq!(historical["executed"], true);
    assert!(!historical.to_string().contains("note.txt"));
    assert!(!historical.to_string().contains("recovery_read"));
    assert_eq!(failed.arguments, "{}");
    assert_eq!(
        std::fs::read_to_string(root.path().join("note.txt")).unwrap(),
        "first\nsecond\n"
    );
}

#[derive(Debug)]
struct SelectionRecoveryProvider {
    requests: Arc<Mutex<Vec<ModelRequest>>>,
    root: std::path::PathBuf,
    initial_digest: String,
    target_line: u64,
    newline: &'static str,
    moved: bool,
    second_race: bool,
    malformed: bool,
}

fn recovery_function(number: u8) -> String {
    format!("pub fn answer() -> u32 {{ let _literal = r\"keep\\n\\t\"; {number} }}")
}

fn last_recovery_result(request: &ModelRequest) -> Value {
    serde_json::from_str(&request.tool_exchanges.last().unwrap().results[0].content).unwrap()
}

#[async_trait]
impl ModelProvider for SelectionRecoveryProvider {
    fn id(&self) -> &'static str {
        "selection-recovery-fixture"
    }
    fn status(&self) -> ProviderStatus {
        test_provider_status(self.id())
    }
    async fn stream(
        &self,
        request: ModelRequest,
        sender: mpsc::Sender<ProviderEvent>,
    ) -> Result<(), ProviderError> {
        let round = self.requests.lock().unwrap().len();
        self.requests.lock().unwrap().push(request.clone());
        let next = if self.malformed {
            match round {
                0 => Some(call(
                    "missing-line-count",
                    "read_text",
                    json!({"path":"source.rs","start_line":self.target_line}),
                )),
                1 => {
                    let feedback = last_recovery_result(&request);
                    assert_eq!(feedback["code"], "invalid_selector");
                    assert_eq!(
                        feedback["recovery_read"]["selection_role"],
                        "observation_window_only"
                    );
                    Some(call(
                        "corrected-line-read",
                        "read_text",
                        feedback["recovery_read"]["arguments"].clone(),
                    ))
                }
                2 => {
                    let observed = last_recovery_result(&request);
                    assert!(observed.get("sha256").is_some());
                    Some(call(
                        "mixed-edit",
                        "edit_text",
                        json!({"path":"source.rs","expected_sha256":observed["sha256"],
                        "old_text":recovery_function(1),"new_text":recovery_function(3),
                        "start_line":self.target_line,"line_count":1,"replacement_lines":[recovery_function(3)]}),
                    ))
                }
                3 => {
                    let feedback = last_recovery_result(&request);
                    assert_eq!(feedback["code"], "invalid_arguments");
                    assert_eq!(feedback["issues"][0]["category"], "edit_form");
                    Some(call(
                        "observe-after-mixed-edit",
                        "read_text",
                        feedback["recovery_read"]["arguments"].clone(),
                    ))
                }
                4 => {
                    let observed = last_recovery_result(&request);
                    let selection = &observed["edit_selection"];
                    assert_eq!(selection["line_count"], 1);
                    Some(call(
                        "new-explicit-valid-edit",
                        "edit_text",
                        json!({"path":"source.rs", "expected_sha256":observed["sha256"],
                        "start_line":selection["start_line"],"line_count":selection["line_count"],
                        "replacement_lines":[recovery_function(3)]}),
                    ))
                }
                _ => None,
            }
        } else {
            match round {
                0 => Some(call(
                    "wide-observation-A",
                    "read_text",
                    json!({"path":"source.rs","start_line":self.target_line-1,"line_count":3}),
                )),
                1 => {
                    let observed = last_recovery_result(&request);
                    assert_eq!(observed["edit_selection"]["line_count"], 3);
                    Some(call(
                        "selected-narrow-A",
                        "read_text",
                        json!({"path":"source.rs","start_line":self.target_line,"line_count":1,"expected_sha256":observed["sha256"]}),
                    ))
                }
                2 => {
                    let observed = last_recovery_result(&request);
                    Some(call(
                        "valid-A-to-B",
                        "edit_text",
                        json!({"path":"source.rs","expected_sha256":observed["sha256"],
                        "start_line":observed["edit_selection"]["start_line"],"line_count":1,"replacement_lines":[recovery_function(2)]}),
                    ))
                }
                3 => {
                    let receipt = last_recovery_result(&request);
                    assert_eq!(
                        receipt["observation_state"],
                        "prior_read_selections_are_historical"
                    );
                    if self.moved {
                        let path = self.root.join("source.rs");
                        let text = std::fs::read_to_string(&path).unwrap();
                        let inserted = format!(
                            "// fixture-owned movement{}// preserved new neighbor{}{}",
                            self.newline,
                            self.newline,
                            recovery_function(2)
                        );
                        std::fs::write(path, text.replacen(&recovery_function(2), &inserted, 1))
                            .unwrap();
                    }
                    Some(call(
                        "old-A-edit",
                        "edit_text",
                        json!({"path":"source.rs","expected_sha256":self.initial_digest,
                        "old_text":recovery_function(1),"new_text":recovery_function(4)}),
                    ))
                }
                4 => {
                    let feedback = last_recovery_result(&request);
                    assert_eq!(feedback["code"], "stale_hash");
                    assert_eq!(feedback["executed"], false);
                    assert!(feedback["recovery_read"]["arguments"]
                        .get("expected_sha256")
                        .is_none());
                    assert_eq!(
                        serde_json::from_str::<Value>(
                            &request.tool_exchanges[3].calls[0].arguments
                        )
                        .unwrap()["expected_sha256"],
                        self.initial_digest
                    );
                    Some(call(
                        "fresh-B-orientation",
                        "read_text",
                        feedback["recovery_read"]["arguments"].clone(),
                    ))
                }
                5 => {
                    let observed = last_recovery_result(&request);
                    assert_ne!(observed["sha256"], self.initial_digest);
                    if self.target_line > 200 {
                        assert_eq!(observed["is_complete_file"], false);
                    }
                    Some(call(
                        "obsolete-A-read-guard",
                        "read_text",
                        json!({"path":"source.rs","start_line":self.target_line,
                        "line_count":1,"expected_sha256":self.initial_digest}),
                    ))
                }
                6 => {
                    let feedback = last_recovery_result(&request);
                    assert_eq!(feedback["code"], "stale_hash");
                    assert_eq!(feedback["executed"], true);
                    assert_eq!(
                        feedback["recovery_read"]["coordinate_basis"],
                        "previous_coordinates_unverified"
                    );
                    assert!(feedback["recovery_read"]["arguments"]
                        .get("expected_sha256")
                        .is_none());
                    Some(call(
                        "inspect-current-at-old-coordinate",
                        "read_text",
                        feedback["recovery_read"]["arguments"].clone(),
                    ))
                }
                7 => {
                    let observed = last_recovery_result(&request);
                    if self.moved {
                        assert!(!observed["content"]
                            .as_str()
                            .unwrap()
                            .contains("pub fn answer"));
                    }
                    Some(call(
                        "explicit-current-target-search",
                        "search_text",
                        json!({"path":"source.rs","query":"pub fn answer","max_results":4}),
                    ))
                }
                8 => {
                    let search = last_recovery_result(&request);
                    assert_eq!(
                        search["matches"]
                            .as_array()
                            .expect("authorized search result")
                            .len(),
                        1
                    );
                    if self.moved {
                        assert_eq!(search["matches"][0]["line_number"], self.target_line + 2);
                    }
                    Some(call(
                        "fresh-narrow-target-B",
                        "read_text",
                        search["matches"][0]["read_selection"].clone(),
                    ))
                }
                9 => {
                    let window = last_recovery_result(&request);
                    let search: Value =
                        serde_json::from_str(&request.tool_exchanges[7].results[0].content)
                            .unwrap();
                    assert!(
                        window["edit_selection"]["line_count"].as_u64().unwrap() > 1,
                        "a search handoff is context, not a selected replacement span"
                    );
                    Some(call(
                        "explicitly-selected-target-line",
                        "read_text",
                        json!({
                            "path":"source.rs", "start_line":search["matches"][0]["line_number"],
                            "line_count":1, "expected_sha256":window["sha256"]
                        }),
                    ))
                }
                10 => {
                    let observed = last_recovery_result(&request);
                    assert_eq!(observed["edit_selection"]["line_count"], 1);
                    if self.second_race {
                        let path = self.root.join("source.rs");
                        let mut bytes = std::fs::read(&path).unwrap();
                        bytes.extend(
                            format!("// fixture-owned second race{}", self.newline).as_bytes(),
                        );
                        std::fs::write(path, bytes).unwrap();
                    }
                    Some(call(
                        "new-current-explicit-edit",
                        "edit_text",
                        json!({"path":"source.rs","expected_sha256":observed["sha256"],
                        "start_line":observed["edit_selection"]["start_line"],"line_count":1,"replacement_lines":[recovery_function(3)]}),
                    ))
                }
                _ => {
                    let last = last_recovery_result(&request);
                    if self.second_race {
                        assert_eq!(last["code"], "stale_hash");
                        assert_eq!(last["executed"], false);
                    }
                    None
                }
            }
        };
        sender
            .send(next.map_or_else(
                || ProviderEvent::Delta("Synthetic recovery control settled".into()),
                ProviderEvent::ToolCall,
            ))
            .await
            .map_err(|_| ProviderError::ConsumerClosed)?;
        sender
            .send(ProviderEvent::Completed)
            .await
            .map_err(|_| ProviderError::ConsumerClosed)
    }
}

#[tokio::test]
async fn selection_recovery_provider_path_observes_new_revision_and_keeps_narrow_scope() {
    use sha2::{Digest, Sha256};
    for (large, moved, second_race, malformed) in [
        (false, false, false, false),
        (true, false, false, false),
        (true, true, false, false),
        (false, false, true, false),
        (false, false, false, true),
    ] {
        let root = tempfile::tempdir().unwrap();
        let newline = if large { "\r\n" } else { "\n" };
        let prefix = if large {
            format!("// UTF-8 neighbor μ \\n \\t{newline}").repeat(10_000)
        } else {
            format!("// UTF-8 neighbor μ \\n \\t{newline}")
        };
        let target_line = if large { 10_001 } else { 2 };
        let suffix = format!("{newline}pub const KEEP: &str = r\"literal\\n\\t\";{newline}");
        let initial = format!("{prefix}{}{suffix}", recovery_function(1));
        std::fs::write(root.path().join("source.rs"), initial.as_bytes()).unwrap();
        let requests = Arc::new(Mutex::new(Vec::new()));
        let mut providers = ProviderRegistry::default();
        providers.register(SelectionRecoveryProvider {
            requests: requests.clone(),
            root: root.path().into(),
            initial_digest: format!("{:x}", Sha256::digest(initial.as_bytes())),
            target_line,
            newline,
            moved,
            second_race,
            malformed,
        });
        let workspace = WorkspaceManager::new(
            CapabilityBroker::new(root.path(), ApprovalPolicy::default()).unwrap(),
        );
        let runtime = test_runtime(
            Store::open_in_memory().unwrap(),
            providers,
            ToolRegistry::with_builtins(workspace.clone()),
            workspace,
        );
        configure_selected_provider(&runtime, "selection-recovery-fixture").await;
        runtime
            .dispatch(AppCommand::SelectExecutionProfile {
                profile_id: "verification".into(),
            })
            .await
            .unwrap();
        runtime
            .dispatch(AppCommand::SetHarnessCapabilityEnabled {
                capability_id: "tool.search-text".into(),
                enabled: true,
            })
            .await
            .unwrap();
        let mut events = runtime.subscribe();
        runtime
            .dispatch(AppCommand::SubmitTask {
                prompt: "Update only the explicitly selected fixture target".into(),
            })
            .await
            .unwrap();
        let added_prefix = if moved {
            format!("// fixture-owned movement{newline}// preserved new neighbor{newline}")
        } else {
            String::new()
        };
        let race_suffix = if second_race {
            format!("// fixture-owned second race{newline}")
        } else {
            String::new()
        };
        let expected = format!(
            "{prefix}{added_prefix}{}{suffix}{race_suffix}",
            recovery_function(if second_race { 2 } else { 3 })
        );
        assert_eq!(
            std::fs::read(root.path().join("source.rs")).unwrap(),
            expected.as_bytes(),
            "raw prefix/suffix, UTF-8, newline and literal backslash preservation"
        );
        let changes = drain_events(&mut events)
            .into_iter()
            .filter(|e| matches!(e, AppEvent::FileChangeRecorded(_)))
            .count();
        assert_eq!(changes, if malformed || second_race { 1 } else { 2 });
        assert_eq!(
            requests.lock().unwrap().len(),
            if malformed { 6 } else { 12 }
        );
        let sources = runtime
            .store
            .context_sources(runtime.project_id, runtime.session_id())
            .unwrap();
        for source in sources.sources {
            if let argentum_store::ContextSource::Group(group) = source {
                if let argentum_domain::working_context::GroupBody::Exchange {
                    observations, ..
                } = group.body
                {
                    for observed in observations {
                        if observed.provider_call_id == "wide-observation-A" {
                            let original: Value =
                                serde_json::from_str(observed.result.as_ref().unwrap()).unwrap();
                            assert_eq!(
                                original["sha256"],
                                format!("{:x}", Sha256::digest(initial.as_bytes()))
                            );
                            assert_eq!(original["edit_selection"]["line_count"], 3);
                        }
                        if let Some(body) = observed.result {
                            assert!(!body.contains("recovery_read"), "private recovery coordinates are not persisted as new current facts");
                        }
                    }
                }
            }
        }
    }
}

#[tokio::test]
async fn selection_recovery_does_not_propose_foreign_missing_or_disabled_reads() {
    for (path, disabled, expected_code) in [
        ("../outside.txt", false, "invalid_path"),
        ("missing.txt", false, "not_found"),
        ("note.txt", true, "capability_disabled"),
    ] {
        let root = tempfile::tempdir().unwrap();
        std::fs::write(root.path().join("note.txt"), "unchanged\n").unwrap();
        let (runtime, requests) = fixture(
            root.path(),
            vec![call(
                "denied-observation",
                "read_text",
                json!({"path":path}),
            )],
            false,
        )
        .await;
        if disabled {
            runtime
                .dispatch(AppCommand::SetHarnessCapabilityEnabled {
                    capability_id: "tool.read-text".into(),
                    enabled: false,
                })
                .await
                .unwrap();
        }
        let mut events = runtime.subscribe();
        let result = runtime
            .dispatch(AppCommand::SubmitTask {
                prompt: "Check only the permitted fixture observation".into(),
            })
            .await;
        let captured = requests.lock().unwrap();
        if disabled {
            assert!(matches!(result, Err(RuntimeError::Instructions(_))));
            assert!(
                captured.is_empty(),
                "denied instruction/read access stops before provider dispatch"
            );
            assert!(!drain_events(&mut events).iter().any(|event| matches!(
                event,
                AppEvent::ToolStarted(_) | AppEvent::FileChangeRecorded(_)
            )));
            continue;
        }
        result.unwrap();
        let feedback: Value =
            serde_json::from_str(&captured[1].tool_exchanges[0].results[0].content).unwrap();
        assert_eq!(feedback["code"], expected_code);
        assert!(feedback.get("recovery_read").is_none());
        assert_eq!(
            std::fs::read_to_string(root.path().join("note.txt")).unwrap(),
            "unchanged\n"
        );
        assert!(!drain_events(&mut events)
            .iter()
            .any(|event| matches!(event, AppEvent::FileChangeRecorded(_))));
    }
}

#[tokio::test]
async fn selection_recovery_invalid_coordinates_reset_to_orientation_without_a_guard() {
    let root = tempfile::tempdir().unwrap();
    std::fs::write(root.path().join("note.txt"), "one\ntwo\n").unwrap();
    let (runtime, requests) = fixture(
        root.path(),
        vec![
            call(
                "past-EOF",
                "read_text",
                json!({"path":"note.txt","start_line":200,"line_count":1}),
            ),
            call("fresh-orientation", "read_text", json!({"path":"note.txt"})),
        ],
        false,
    )
    .await;
    runtime
        .dispatch(AppCommand::SubmitTask {
            prompt: "Observe valid current bounds".into(),
        })
        .await
        .unwrap();
    let captured = requests.lock().unwrap();
    let feedback: Value =
        serde_json::from_str(&captured[1].tool_exchanges[0].results[0].content).unwrap();
    assert_eq!(feedback["code"], "invalid_selector");
    assert_eq!(
        feedback["recovery_read"]["arguments"],
        json!({"path":"note.txt"})
    );
    assert!(captured[2].tool_exchanges[1].results[0]
        .content
        .contains("one\\ntwo\\n"));
}

#[tokio::test]
async fn selection_recovery_multibyte_foreign_path_feedback_stays_bounded() {
    let root = tempfile::tempdir().unwrap();
    let path = format!("/{}", "\u{10400}".repeat(512));
    let (runtime, requests) = fixture(
        root.path(),
        vec![call("foreign-unicode", "read_text", json!({"path":path}))],
        false,
    )
    .await;
    runtime
        .dispatch(AppCommand::SubmitTask {
            prompt: "Reject the foreign fixture without observation".into(),
        })
        .await
        .unwrap();
    let captured = requests.lock().unwrap();
    let body = &captured[1].tool_exchanges[0].results[0].content;
    assert!(body.len() <= 2048);
    let feedback: Value = serde_json::from_str(body).unwrap();
    assert_eq!(feedback["code"], "invalid_path");
    assert_eq!(feedback["executed"], false);
    assert!(feedback.get("recovery_read").is_none());
}

#[tokio::test]
async fn actual_unreadable_instruction_source_remains_fatal_before_write() {
    let root = tempfile::tempdir().unwrap();
    std::fs::create_dir(root.path().join("nested")).unwrap();
    std::fs::write(root.path().join("nested/AGENTS.md"), [0xff, 0xfe, 0xff]).unwrap();
    let (runtime, requests) = fixture(
        root.path(),
        vec![call(
            "target",
            "write_text",
            json!({"path":"nested/hello.txt","content":"Hello"}),
        )],
        true,
    )
    .await;
    let error = runtime
        .dispatch(AppCommand::SubmitTask {
            prompt: "Create the file".into(),
        })
        .await
        .unwrap_err();
    assert!(error.to_string().contains("instruction_loading_failed"));
    assert_eq!(requests.lock().unwrap().len(), 1);
    assert!(!root.path().join("nested/hello.txt").exists());
}

#[tokio::test]
async fn list_directory_still_accepts_workspace_root() {
    let root = tempfile::tempdir().unwrap();
    std::fs::write(root.path().join("note.txt"), "unchanged").unwrap();
    let (runtime, requests) = fixture(
        root.path(),
        vec![call(
            "list-root",
            "list_directory",
            json!({"path":".","max_entries":10}),
        )],
        false,
    )
    .await;
    runtime
        .dispatch(AppCommand::SubmitTask {
            prompt: "List the workspace".into(),
        })
        .await
        .unwrap();
    assert!(requests.lock().unwrap()[1].tool_exchanges[0].results[0]
        .content
        .contains("note.txt"));
}

#[tokio::test]
async fn mutation_trace_uses_verified_creation_modification_and_deletion_paths() {
    let root = tempfile::tempdir().unwrap();
    std::fs::write(root.path().join("modify.txt"), "before").unwrap();
    std::fs::write(root.path().join("delete.txt"), "remove").unwrap();
    let calls = vec![call(
        "three-files",
        "apply_patch",
        json!({"files":[
            {"path":"create.txt","content":"new"},
            {"path":"modify.txt","content":"after"},
            {"path":"delete.txt","content":null}
        ]}),
    )];
    let (runtime, _) = fixture(root.path(), calls, true).await;
    let mut events = runtime.subscribe();
    runtime
        .dispatch(AppCommand::SubmitTask {
            prompt: "Update the fixture".into(),
        })
        .await
        .unwrap();
    let emitted = drain_events(&mut events);
    let started = emitted
        .iter()
        .find_map(|e| {
            if let AppEvent::ToolStarted(t) = e {
                Some(t)
            } else {
                None
            }
        })
        .unwrap();
    let finished = emitted
        .iter()
        .find_map(|e| {
            if let AppEvent::ToolFinished(t) = e {
                Some(t)
            } else {
                None
            }
        })
        .unwrap();
    assert_eq!(
        started.summary,
        "Create \"create.txt\"\nModify \"modify.txt\"\nDelete \"delete.txt\""
    );
    assert_eq!(
        finished.summary,
        "Created \"create.txt\"\nModified \"modify.txt\"\nDeleted \"delete.txt\""
    );
    assert_eq!(
        std::fs::read_to_string(root.path().join("create.txt")).unwrap(),
        "new"
    );
    assert_eq!(
        std::fs::read_to_string(root.path().join("modify.txt")).unwrap(),
        "after"
    );
    assert!(!root.path().join("delete.txt").exists());
}
