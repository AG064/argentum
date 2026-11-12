use super::*;
use serde_json::{json, Value};
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::time::{Duration, Instant};

fn read_request(stream: &mut TcpStream) -> Value {
    stream.set_nonblocking(false).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(40)))
        .unwrap();
    let mut bytes = Vec::new();
    let mut buffer = [0_u8; 4096];
    let (offset, length) = loop {
        let count = stream.read(&mut buffer).unwrap();
        assert_ne!(count, 0, "synthetic HTTP ended before headers");
        bytes.extend_from_slice(&buffer[..count]);
        assert!(bytes.len() <= 512 * 1024);
        if let Some(offset) = bytes.windows(4).position(|value| value == b"\r\n\r\n") {
            let headers = std::str::from_utf8(&bytes[..offset]).unwrap();
            assert!(!headers.to_ascii_lowercase().contains("authorization:"));
            let length = headers
                .lines()
                .find_map(|line| {
                    let (key, value) = line.split_once(':')?;
                    key.eq_ignore_ascii_case("content-length")
                        .then(|| value.trim().parse::<usize>().unwrap())
                })
                .unwrap();
            break (offset + 4, length);
        }
    };
    while bytes.len() < offset + length {
        let count = stream.read(&mut buffer).unwrap();
        assert_ne!(count, 0);
        bytes.extend_from_slice(&buffer[..count]);
        assert!(bytes.len() <= 512 * 1024);
    }
    serde_json::from_slice(&bytes[offset..offset + length]).unwrap()
}

fn tool_result(request: &Value, id: &str) -> Value {
    let message = request["messages"]
        .as_array()
        .unwrap()
        .iter()
        .find(|message| message["role"] == "tool" && message["tool_call_id"] == id)
        .unwrap();
    serde_json::from_str(message["content"].as_str().unwrap()).unwrap()
}

fn response(round: usize, calls: Vec<(&str, &str, Value)>) -> String {
    let finish = if calls.is_empty() {
        "stop"
    } else {
        "tool_calls"
    };
    let tools: Vec<_> = calls.into_iter().enumerate().map(|(index, (id, name, arguments))| {
        json!({"index":index,"id":id,"type":"function","function":{"name":name,"arguments":arguments.to_string()}})
    }).collect();
    let event = json!({"id":format!("synthetic-response-{round}"), "model":"MiniMax-M2.7", "choices":[{
        "delta":{"role":"assistant","content":if finish=="stop" {"Synthetic completed"}else{"Synthetic progress"},
        "reasoning_details":[{"type":"reasoning.text","id":format!("synthetic-private-id-{round}"),
            "format":"MiniMax-response-v1","index":0,"text":format!("SYNTHETIC_REASONING_{round}")}],"tool_calls":tools},
        "finish_reason":finish
    }]});
    let body = format!("data: {event}\n\ndata: [DONE]\n\n");
    format!("HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len())
}

#[tokio::test]
async fn actual_minimax_runtime_chain_pairs_parallel_rejection_edit_verification_retry_and_fresh_task(
) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let endpoint = format!("http://{}/v1/", listener.local_addr().unwrap());
    let requests = Arc::new(Mutex::new(Vec::new()));
    let recorded = requests.clone();
    let worker = std::thread::spawn(move || {
        let deadline = Instant::now() + Duration::from_secs(120);
        for round in 0..8 {
            let (mut stream, _) = loop {
                match listener.accept() {
                    Ok(connection) => break connection,
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        assert!(
                            Instant::now() < deadline,
                            "synthetic server foreground deadline"
                        );
                        std::thread::sleep(Duration::from_millis(5));
                    }
                    Err(error) => panic!("synthetic accept failed: {error}"),
                }
            };
            let request = read_request(&mut stream);
            assert_eq!(request["reasoning_split"], true);
            let serialized = request.to_string();
            if round == 0 {
                assert!(serialized.contains("SYNTHETIC_ORIGINAL_AGENTS"));
            }
            if round > 0 && round < 7 {
                let expected_groups = round.min(5);
                let groups: Vec<_> = request["messages"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .filter(|message| message.get("reasoning_details").is_some())
                    .collect();
                assert_eq!(groups.len(), expected_groups);
                for (index, group) in groups.iter().enumerate() {
                    assert_eq!(
                        group["reasoning_details"][0]["id"],
                        format!("synthetic-private-id-{index}")
                    );
                    assert_eq!(
                        group["reasoning_details"][0]["text"],
                        format!("SYNTHETIC_REASONING_{index}")
                    );
                }
            }
            let result = match round {
                0 => response(
                    round,
                    vec![
                        ("search", "search_text", json!({"query":"pub fn answer"})),
                        (
                            "rejected",
                            "read_text",
                            json!({"path":"../not-allowed.txt"}),
                        ),
                    ],
                ),
                1 => {
                    let rejected = tool_result(&request, "rejected");
                    assert_eq!(rejected["executed"], false);
                    assert!(rejected.to_string().len() < 4096);
                    let first = request["messages"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .find(|message| message["role"] == "assistant")
                        .unwrap();
                    assert_eq!(first["tool_calls"][0]["id"], "search");
                    assert_eq!(first["tool_calls"][0]["index"], 0);
                    assert_eq!(first["tool_calls"][1]["id"], "rejected");
                    assert_eq!(first["tool_calls"][1]["index"], 1);
                    response(
                        round,
                        vec![("read", "read_text", json!({"path":"src/lib.rs"}))],
                    )
                }
                2 => response(
                    round,
                    vec![(
                        "verify-failed",
                        "verify",
                        json!({"runners":["cargo-test-locked"]}),
                    )],
                ),
                3 => {
                    let failed = tool_result(&request, "verify-failed");
                    assert_eq!(failed[0]["state"], "failed");
                    assert_ne!(failed[0]["exit_code"], 0);
                    response(
                        round,
                        vec![(
                            "edit",
                            "edit_text",
                            json!({
                                "path":"src/lib.rs", "expected_sha256":tool_result(&request,"read")["sha256"],
                                "old_text":"pub fn answer() -> u32 { 1 }", "new_text":"pub fn answer() -> u32 { 2 }"
                            }),
                        )],
                    )
                }
                4 => response(
                    round,
                    vec![(
                        "verify-passed",
                        "verify",
                        json!({"runners":["cargo-test-locked"]}),
                    )],
                ),
                5 => {
                    let actual = tool_result(&request, "verify-passed");
                    assert_eq!(
                        actual[0]["state"], "passed",
                        "synthetic verifier evidence: {actual}"
                    );
                    "HTTP/1.1 529 Overloaded\r\nContent-Length: 0\r\nRetry-After: 0\r\nConnection: close\r\n\r\n".into()
                }
                6 => {
                    assert_eq!(request, recorded.lock().unwrap()[5]);
                    response(round, Vec::new())
                }
                7 => {
                    assert!(!serialized.contains("SYNTHETIC_REASONING"));
                    assert!(!serialized.contains("synthetic-private-id"));
                    response(round, Vec::new())
                }
                _ => unreachable!(),
            };
            recorded.lock().unwrap().push(request);
            stream.write_all(result.as_bytes()).unwrap();
        }
    });
    let root = tempfile::tempdir().unwrap();
    std::fs::create_dir(root.path().join("src")).unwrap();
    std::fs::create_dir(root.path().join("tests")).unwrap();
    std::fs::write(root.path().join("Cargo.toml"), "[package]\nname = \"minimax_continuation_fixture\"\nversion = \"0.1.0\"\nedition = \"2021\"\n").unwrap();
    std::fs::write(root.path().join("Cargo.lock"), "version = 4\n\n[[package]]\nname = \"minimax_continuation_fixture\"\nversion = \"0.1.0\"\n").unwrap();
    std::fs::write(
        root.path().join("src/lib.rs"),
        "pub fn answer() -> u32 { 1 }\n",
    )
    .unwrap();
    std::fs::write(
        root.path().join("tests/behavior.rs"),
        "#[test] fn actual_behavior() { assert_eq!(minimax_continuation_fixture::answer(), 2); }\n",
    )
    .unwrap();
    std::fs::write(root.path().join("AGENTS.md"), "SYNTHETIC_ORIGINAL_AGENTS\nChange only src/lib.rs. Use the fixed cargo-test-locked verifier.\n").unwrap();
    std::fs::write(root.path().join("unrelated.txt"), "preserved\r\n").unwrap();
    let mut providers = ProviderRegistry::default();
    providers.register(
        argentum_providers::OpenAiCompatibleProvider::minimax_loopback_for_test(
            &endpoint,
            "MiniMax-M2.7",
        )
        .unwrap(),
    );
    let broker = CapabilityBroker::new(root.path(), ApprovalPolicy::default()).unwrap();
    let workspace = WorkspaceManager::new(broker);
    let tools = ToolRegistry::with_builtins(workspace.clone());
    let runtime = test_runtime(
        Store::open_in_memory().unwrap(),
        providers,
        tools,
        workspace,
    );
    runtime
        .dispatch(AppCommand::SaveProviderProfile {
            profile: ProviderProfile {
                id: "openai-compatible".into(),
                label: "Synthetic MiniMax".into(),
                kind: ProviderKind::OpenAiCompatible,
                endpoint: "https://api.minimax.io/v1/".into(),
                model: "MiniMax-M2.7".into(),
                selected: true,
            },
        })
        .await
        .unwrap();
    runtime
        .declare_model_budget("openai-compatible", [204800, 196608, 4096, 4096, 1024])
        .unwrap();
    runtime
        .dispatch(AppCommand::SelectExecutionProfile {
            profile_id: "verification".into(),
        })
        .await
        .unwrap();
    for capability_id in ["tool.search-text", "tool.search-filenames"] {
        runtime
            .dispatch(AppCommand::SetHarnessCapabilityEnabled {
                capability_id: capability_id.into(),
                enabled: true,
            })
            .await
            .unwrap();
    }
    let policy = runtime.load_execution_policy().unwrap();
    assert!(!runtime
        .model_tool_definitions()
        .unwrap()
        .iter()
        .any(|tool| tool.name == "execute_argv" || tool.name == "shell"));
    let mut events = runtime.subscribe();
    runtime
        .dispatch(AppCommand::SubmitTask {
            prompt: "Repair the fixture and verify the actual behavior".into(),
        })
        .await
        .unwrap();
    assert_eq!(
        std::fs::read_to_string(root.path().join("src/lib.rs")).unwrap(),
        "pub fn answer() -> u32 { 2 }\n"
    );
    assert_eq!(
        std::fs::read(root.path().join("unrelated.txt")).unwrap(),
        b"preserved\r\n"
    );
    runtime
        .dispatch(AppCommand::SubmitTask {
            prompt: "A fresh task, without the previous private provider state".into(),
        })
        .await
        .unwrap();
    assert_eq!(runtime.load_execution_policy().unwrap(), policy);
    worker.join().unwrap();
    let emitted = drain_events(&mut events);
    let starts: Vec<_> = emitted
        .iter()
        .filter_map(|event| match event {
            AppEvent::ModelRequestAttempt {
                request_id,
                attempt,
                outcome,
                ..
            } if outcome == "started" => Some((*request_id, *attempt)),
            _ => None,
        })
        .collect();
    assert_eq!(starts.len(), 8);
    assert_eq!(
        starts
            .iter()
            .collect::<std::collections::BTreeSet<_>>()
            .len(),
        8
    );
    assert_ne!(
        starts[5].0, starts[6].0,
        "retry physical request identities"
    );
    assert!(emitted.iter().any(|event| matches!(
        event,
        AppEvent::ModelRequestAttempt { status: Some(529), outcome, .. }
            if outcome == "retry_scheduled"
    )));
    let verification: Vec<_> = emitted
        .iter()
        .filter_map(|event| match event {
            AppEvent::VerificationEvidenceRecorded(evidence) => Some(evidence.state),
            _ => None,
        })
        .collect();
    assert!(verification.contains(&argentum_domain::VerificationState::Failed));
    assert!(verification.contains(&argentum_domain::VerificationState::Passed));
    assert_eq!(emitted.iter().filter(|event| matches!(event,AppEvent::ToolFinished(trace) if trace.tool_id == "edit_text")).count(), 1);
    assert!(emitted.iter().any(|event| matches!(
        event,
        AppEvent::RunStatusChanged {
            lifecycle: TaskLifecycle::Complete,
            ..
        }
    )));
}
