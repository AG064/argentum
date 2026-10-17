#![cfg(all(windows, feature = "artifacts"))]
//! Actual CLI stdio and verifier controls. Synthetic sources; no real provider.
use argentum_artifacts::{
    artifact::{Configuration, Input},
    sha256,
};
use argentum_execution::{ManagedProcessLimits, ProcessSpec};
use argentum_extensions::connection::McpSession;
use serde_json::{json, Value};
use std::{collections::BTreeSet, path::PathBuf, process::Command, time::Duration};
use tokio_util::sync::CancellationToken;
#[path = "support/http.rs"]
mod http_support;

#[tokio::test]
async fn actual_model_tool_path_returns_guarded_markdown_correction_to_next_request() {
    use argentum_cli::{CommandHost, HostConfig};
    use argentum_domain::{AppCommand, ProviderKind, ProviderProfile};
    use std::{io::Write, net::TcpListener, process::Stdio, time::Instant};
    let root = tempfile::tempdir().unwrap();
    let ws = root.path().join("workspace");
    std::fs::create_dir(&ws).unwrap();
    let before = "# Brief\n\nState: draft.\n";
    let after = "# Brief\n\nState: reviewed.\n";
    std::fs::write(ws.join("brief.md"), before).unwrap();
    std::fs::write(ws.join("keep.txt"), "UNRELATED\n").unwrap();
    let digest = sha256(before.as_bytes());
    let new_digest = sha256(after.as_bytes());
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let endpoint = format!("http://{}/v1/", listener.local_addr().unwrap());
    let db = root.path().join("store.sqlite3");
    let mut config = HostConfig::in_memory(&ws);
    config.database = Some(db.clone());
    let mut host = CommandHost::start_composed(config).await.unwrap();
    let owner = host.local_handle();
    owner
        .dispatch(AppCommand::SaveProviderProfile {
            profile: ProviderProfile {
                id: "artifact-control".into(),
                label: "Synthetic local artifact correction".into(),
                kind: ProviderKind::OpenAiCompatible,
                endpoint,
                model: "fixture-model".into(),
                selected: true,
            },
        })
        .await
        .unwrap();
    owner
        .dispatch(AppCommand::SelectProviderProfile {
            provider_id: "artifact-control".into(),
        })
        .await
        .unwrap();
    owner
        .dispatch(AppCommand::SelectExecutionProfile {
            profile_id: "verification".into(),
        })
        .await
        .unwrap();
    host.declare_model_budget("artifact-control", [204800, 196608, 4096, 4096, 1024])
        .unwrap();
    host.shutdown().await.unwrap();
    let controller = std::thread::spawn(move || {
        let deadline = Instant::now() + Duration::from_secs(90);
        for round in 0..3 {
            let (mut stream, _) = loop {
                match listener.accept() {
                    Ok(value) => break value,
                    Err(_) if Instant::now() < deadline => {
                        std::thread::sleep(Duration::from_millis(5))
                    }
                    Err(e) => panic!("synthetic dispatch deadline: {e}"),
                }
            };
            stream.set_nonblocking(false).unwrap();
            stream
                .set_read_timeout(Some(Duration::from_secs(10)))
                .unwrap();
            let request = http_support::read_request(&mut stream);
            let body = request["messages"].to_string();
            assert!(!request["tools"]
                .as_array()
                .unwrap()
                .iter()
                .any(|t| matches!(
                    t["function"]["name"].as_str(),
                    Some("execute_argv" | "execute_shell")
                )));
            if round == 1 {
                assert!(
                    body.contains(&digest),
                    "read SHA must reach the actual next request"
                );
            }
            if round == 2 {
                assert!(
                    body.contains(&new_digest),
                    "guarded edit postimage must reach the model"
                );
            }
            let response = match round {
                0 => {
                    json!({"choices":[{"delta":{"tool_calls":[{"index":0,"id":"read-current","type":"function","function":{"name":"read_text","arguments":json!({"path":"brief.md"}).to_string()}}]},"finish_reason":"tool_calls"}],"usage":{"total_tokens":10}})
                }
                1 => {
                    json!({"choices":[{"delta":{"tool_calls":[{"index":0,"id":"guarded-correction","type":"function","function":{"name":"edit_text","arguments":json!({"path":"brief.md","expected_sha256":digest,"old_text":"State: draft.","new_text":"State: reviewed."}).to_string()}}]},"finish_reason":"tool_calls"}],"usage":{"total_tokens":10}})
                }
                _ => {
                    json!({"choices":[{"delta":{"content":"The guarded Markdown correction is recorded. This control makes no visual or native-goal completion claim."},"finish_reason":"stop"}],"usage":{"total_tokens":10}})
                }
            };
            stream.write_all(b"HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nConnection: close\r\n\r\n").unwrap();
            write!(stream, "data: {response}\n\ndata: [DONE]\n\n").unwrap();
        }
    });
    let exe = std::env::var_os("ARGENTUM_TEST_ARTIFACT_CLI_EXE")
        .map(PathBuf::from)
        .unwrap_or_else(|| env!("CARGO_BIN_EXE_argentum").into());
    let output=Command::new(exe).args(["run","--isolated","--workspace"]).arg(&ws).arg("--database").arg(&db)
        .args(["--prompt","Apply the public review correction to the local Markdown through normal guarded tools.","--no-color"])
        .env("RUST_LOG","off").stdin(Stdio::null()).output().unwrap();
    controller.join().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(std::fs::read_to_string(ws.join("brief.md")).unwrap(), after);
    assert_eq!(std::fs::read(ws.join("keep.txt")).unwrap(), b"UNRELATED\n");
}

#[tokio::test]
#[ignore = "requires private pinned assets and worker executable; exact packaged qualification runs explicitly"]
async fn actual_cli_mcp_render_current_images_and_verifier_feedback() {
    let exe = std::env::var_os("ARGENTUM_TEST_ARTIFACT_CLI_EXE")
        .map(PathBuf::from)
        .unwrap_or_else(|| env!("CARGO_BIN_EXE_argentum").into());
    let worker = PathBuf::from(
        std::env::var_os("ARGENTUM_TEST_ARTIFACT_WORKER_EXE").expect("exact worker required"),
    );
    let root = tempfile::tempdir().unwrap();
    let ws = root.path().join("workspace");
    std::fs::create_dir(&ws).unwrap();
    for (name,body) in [("a.txt","Approved source A\n"),("b.txt","Approved source B\n"),("keep.txt","UNRELATED_PRESERVATION\n"),("brief.md","# Maintainer brief\n\nRésumé for Pärnu.\n\n| Fact | Value |\n|---|---|\n| State | Review required |\n")]{std::fs::write(ws.join(name),body).unwrap();}
    let config = Configuration {
        schema_version: 1,
        workspace: ws.clone(),
        inputs: vec![
            Input {
                path: "a.txt".into(),
                sha256: sha256(b"Approved source A\n"),
            },
            Input {
                path: "b.txt".into(),
                sha256: sha256(b"Approved source B\n"),
            },
        ],
        markdown: "brief.md".into(),
        output_directory: "out".into(),
        media_directory: root.path().join("media"),
        worker_directory: root.path().join("workers"),
        worker: worker.clone(),
        worker_sha256: sha256(&std::fs::read(&worker).unwrap()),
        pdfium: PathBuf::from(
            std::env::var_os("ARGENTUM_TEST_PDFIUM_DLL").expect("pinned DLL required"),
        ),
        font: PathBuf::from(
            std::env::var_os("ARGENTUM_TEST_ARTIFACT_FONT").expect("pinned font required"),
        ),
    };
    for path in [&config.media_directory, &config.worker_directory] {
        argentum_execution::ensure_owner_only_directory(path).unwrap();
    }
    let config_file = root.path().join("config.json");
    std::fs::write(&config_file, serde_json::to_vec(&config).unwrap()).unwrap();
    let spec = ProcessSpec::argv(
        exe.to_string_lossy(),
        vec![
            "artifacts".into(),
            "serve".into(),
            "--config".into(),
            config_file.display().to_string(),
        ],
        &ws,
    )
    .unwrap()
    .with_timeout(Duration::from_secs(90))
    .unwrap();
    // Test-owned direct component launch, not a human activation or production
    // grant. The ordinary runtime activation/denial paths are tested separately.
    let mut session = McpSession::connect_stdio(
        spec,
        ManagedProcessLimits::new(4, 50, 65536, 1024 * 1024 * 1024, 1536 * 1024 * 1024).unwrap(),
        BTreeSet::from_iter(
            ["artifact_open", "artifact_render", "artifact_page"].map(str::to_owned),
        ),
        CancellationToken::new(),
        || true,
    )
    .await
    .unwrap();
    let tools = session.discover().await.unwrap();
    assert_eq!(tools.len(), 3);
    let handle = |name: &str| {
        tools
            .iter()
            .find(|t| t.handle.name == name)
            .unwrap()
            .handle
            .clone()
    };
    let opened = session
        .invoke(&handle("artifact_open"), json!({}))
        .await
        .unwrap();
    let sha = opened["structuredContent"]["markdown_sha256"]
        .as_str()
        .unwrap();
    let bad = session
        .invoke(
            &handle("artifact_render"),
            json!({"source_sha256":"0".repeat(64),"expected_artifact":null}),
        )
        .await
        .unwrap();
    assert_eq!(bad["isError"], true);
    assert!(!ws.join("out").exists());
    let result = session
        .invoke(
            &handle("artifact_render"),
            json!({"source_sha256":sha,"expected_artifact":null}),
        )
        .await
        .unwrap();
    assert_ne!(result["isError"], true, "{result}");
    let rendered = &result["structuredContent"];
    let verify = || {
        Command::new(&exe)
            .args(["artifacts", "verify", "--config"])
            .arg(&config_file)
            .output()
            .unwrap()
    };
    let failed = verify();
    assert!(!failed.status.success());
    assert!(String::from_utf8_lossy(&failed.stderr).contains("not every current page tile"));
    for tile in rendered["tiles"].as_array().unwrap() {
        let reply=session.invoke(&handle("artifact_page"),json!({"artifact_id":rendered["artifact_id"],"page":tile["page"],"tile":tile["tile"]})).await.unwrap();
        assert_eq!(reply["structuredContent"]["sha256"], tile["sha256"]);
        assert_eq!(
            reply["structuredContent"]["context_id"],
            rendered["context_id"]
        );
    }
    let passed = verify();
    assert!(
        passed.status.success(),
        "{}",
        String::from_utf8_lossy(&passed.stderr)
    );
    let proof: Value = serde_json::from_slice(&passed.stdout).unwrap();
    assert_eq!(proof["fresh_pixels_match"], "passed");
    session.retire().await.unwrap();
    native_goal_returns_current_artifact_failure_and_pass(&exe, &ws, &config_file, root.path())
        .await;
    std::fs::write(ws.join("brief.md"), "# Changed after verification\n").unwrap();
    assert!(
        !verify().status.success(),
        "a changed draft cannot retain current verification"
    );
    assert_eq!(
        std::fs::read(ws.join("a.txt")).unwrap(),
        b"Approved source A\n"
    );
    assert_eq!(
        std::fs::read(ws.join("b.txt")).unwrap(),
        b"Approved source B\n"
    );
    assert_eq!(
        std::fs::read(ws.join("keep.txt")).unwrap(),
        b"UNRELATED_PRESERVATION\n"
    );
}

// Composite offline control: actual artifact producer/render above, followed by
// the actual goal driver and approved project runner. The loopback text fixture
// deliberately makes no claim of provider image transport or visual judgement.
async fn native_goal_returns_current_artifact_failure_and_pass(
    exe: &std::path::Path,
    ws: &std::path::Path,
    configuration: &std::path::Path,
    root: &std::path::Path,
) {
    use argentum_cli::{CommandHost, HostConfig};
    use argentum_domain::{AppCommand, ProviderKind, ProviderProfile, VerificationRunnerId};
    use std::{io::Write, net::TcpListener, process::Stdio, time::Instant};
    let before = std::fs::read_to_string(ws.join("brief.md")).unwrap();
    let defective = before.replace("Review required", "Review pending");
    assert_ne!(defective, before);
    std::fs::write(ws.join("brief.md"), &defective).unwrap();
    std::fs::create_dir(ws.join(".agents")).unwrap();
    let runner = json!({"version":1,"runners":[{"id":"local-artifact","label":"Current raw artifact verification",
        "program":exe.file_name().unwrap().to_str().unwrap(),"args":["artifacts","verify","--config",configuration],
        "cwd":".","timeout_ms":90000,"output_limit":65536}]});
    std::fs::write(
        ws.join(".agents/verification.yaml"),
        serde_json::to_vec(&runner).unwrap(),
    )
    .unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let endpoint = format!("http://{}/v1/", listener.local_addr().unwrap());
    let db = root.join("native-store.sqlite3");
    let mut config = HostConfig::in_memory(ws);
    config.database = Some(db.clone());
    let mut host = CommandHost::start_composed(config).await.unwrap();
    let owner = host.local_handle();
    owner
        .dispatch(AppCommand::SaveProviderProfile {
            profile: ProviderProfile {
                id: "artifact-control".into(),
                label: "Synthetic native artifact verification".into(),
                kind: ProviderKind::OpenAiCompatible,
                endpoint,
                model: "fixture-model".into(),
                selected: true,
            },
        })
        .await
        .unwrap();
    owner
        .dispatch(AppCommand::SelectProviderProfile {
            provider_id: "artifact-control".into(),
        })
        .await
        .unwrap();
    owner
        .dispatch(AppCommand::SelectExecutionProfile {
            profile_id: "verification".into(),
        })
        .await
        .unwrap();
    host.declare_model_budget("artifact-control", [204800, 196608, 4096, 4096, 1024])
        .unwrap();
    // In-process test-owned authority, never production owner confirmation.
    owner
        .dispatch(AppCommand::ApproveProjectVerification {
            runner: VerificationRunnerId::Project("local-artifact".into()),
        })
        .await
        .unwrap();
    host.shutdown().await.unwrap();
    let command = |args: &[&str]| {
        Command::new(exe)
            .args(args)
            .args(["--isolated", "--workspace"])
            .arg(ws)
            .arg("--database")
            .arg(&db)
            .arg("--no-color")
            .stdin(Stdio::null())
            .output()
            .unwrap()
    };
    let set = command(&["goal","set","--objective","Correct the public draft and receive current native artifact verification before completion.","--token-budget","2000000","--tool-budget","32","--time-budget","295","--json"]);
    assert!(
        set.status.success(),
        "{}",
        String::from_utf8_lossy(&set.stderr)
    );
    let goal: Value = serde_json::from_slice(&set.stdout).unwrap();
    let id = goal["goal"]["id"].as_str().unwrap();
    let limits = command(&[
        "goal",
        "limits",
        "--goal-id",
        id,
        "--limits",
        "2000000,32,8,32,295,1000000,32,16,295",
        "--json",
    ]);
    assert!(
        limits.status.success(),
        "{}",
        String::from_utf8_lossy(&limits.stderr)
    );
    let criteria = command(&[
        "goal",
        "criteria",
        "--goal-id",
        id,
        "--runners",
        "project:local-artifact",
        "--json",
    ]);
    assert!(
        criteria.status.success(),
        "{}",
        String::from_utf8_lossy(&criteria.stderr)
    );
    let controller = std::thread::spawn(move || {
        let deadline = Instant::now() + Duration::from_secs(140);
        for round in 0..5 {
            let (mut stream, _) = loop {
                match listener.accept() {
                    Ok(v) => break v,
                    Err(_) if Instant::now() < deadline => {
                        std::thread::sleep(Duration::from_millis(5))
                    }
                    Err(e) => panic!("native artifact dispatch deadline: {e}"),
                }
            };
            stream.set_nonblocking(false).unwrap();
            stream
                .set_read_timeout(Some(Duration::from_secs(10)))
                .unwrap();
            let request = http_support::read_request(&mut stream);
            let body = request["messages"].to_string();
            if round == 1 {
                assert!(
                    body.contains("native_verifier_feedback") && body.contains("failed"),
                    "current failed verifier feedback missing"
                );
            }
            if round == 3 {
                assert!(
                    body.contains(&sha256(before.as_bytes())),
                    "current guarded postimage missing"
                );
            }
            if round == 4 {
                assert!(
                    body.contains("native_verifier_feedback") && body.contains("passed"),
                    "current passing verifier feedback missing"
                );
            }
            let response = match round {
                1 => {
                    json!({"choices":[{"delta":{"tool_calls":[{"index":0,"id":"observe-current","type":"function","function":{"name":"read_text","arguments":json!({"path":"brief.md"}).to_string()}}]},"finish_reason":"tool_calls"}],"usage":{"total_tokens":10}})
                }
                2 => {
                    json!({"choices":[{"delta":{"tool_calls":[{"index":0,"id":"correct-current","type":"function","function":{"name":"edit_text","arguments":json!({"path":"brief.md","expected_sha256":sha256(defective.as_bytes()),"old_text":"Review pending","new_text":"Review required"}).to_string()}}]},"finish_reason":"tool_calls"}],"usage":{"total_tokens":10}})
                }
                _ => {
                    json!({"choices":[{"delta":{"content":"{\"status\":\"ready\",\"remaining\":[],\"summary\":\"Current native artifact feedback acknowledged.\"}"},"finish_reason":"stop"}],"usage":{"total_tokens":10}})
                }
            };
            stream.write_all(b"HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nConnection: close\r\n\r\n").unwrap();
            write!(stream, "data: {response}\n\ndata: [DONE]\n\n").unwrap();
        }
    });
    let output = command(&[
        "goal",
        "deliver",
        "--goal-id",
        id,
        "--iterations",
        "8",
        "--elapsed-seconds",
        "295",
        "--json",
    ]);
    controller.join().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let result: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(result["task_success"], true, "{result}");
    assert_eq!(result["invocation"]["state"], "verified");
    let reopened = command(&["goal", "status", "--json"]);
    assert!(reopened.status.success());
    let status: Value = serde_json::from_slice(&reopened.stdout).unwrap();
    assert_eq!(status["goal"]["lifecycle"], "complete");
    assert_eq!(status["supervision"]["completion_current"], true);
    assert_eq!(
        status["goal"]["accounting"]["counters"]["request_attempts"],
        5
    );
}
