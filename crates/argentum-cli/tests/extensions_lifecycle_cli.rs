#![cfg(all(windows, feature = "extensions"))]
//! Actual CLI cancellation/restart controls with synthetic local services.
use argentum_cli::{
    protocol::{RequestEnvelope, ResponseEnvelope, ServerPayload},
    CommandHost, HostConfig,
};
use argentum_domain::{
    extensions::{ExtensionDefinition, ExtensionTransport, ExtensionTrust},
    AppCommand, AppEvent, ProviderKind, ProviderProfile, Sha256Digest, TaskLifecycle,
};
use serde_json::{json, Value};
use std::{
    collections::BTreeMap,
    io::{BufRead, BufReader, Read, Write},
    net::TcpListener,
    process::{Child, Command, Stdio},
    sync::mpsc,
    time::{Duration, Instant},
};
#[path = "support/http.rs"]
mod http_support;

struct OwnedCli(Child);
impl Drop for OwnedCli {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

#[tokio::test]
#[ignore = "requires the current owned synthetic MCP executable; exact qualification runs explicitly"]
async fn actual_cli_cancellation_revokes_uncertain_call_and_restart_has_no_replay() {
    let program = std::env::var_os("ARGENTUM_NEXT01_MCP_FIXTURE_EXE")
        .expect("owned synthetic fixture required");
    let exe = std::env::var_os("ARGENTUM_NEXT01_CLI_EXE")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| env!("CARGO_BIN_EXE_argentum").into());
    let root = tempfile::tempdir().unwrap();
    let ws = root.path().join("workspace");
    std::fs::create_dir(&ws).unwrap();
    std::fs::write(ws.join("keep.txt"), "PRESERVED_CANCEL_CONTROL").unwrap();
    let marker = root.path().join("actual-call-started.txt");
    let args = vec!["slow".into(), marker.to_string_lossy().into_owned()];
    let package = root.path().join("package");
    std::fs::create_dir(&package).unwrap();
    std::fs::write(package.join("plugin.json"),serde_json::to_vec(&json!({"$schema":argentum_extensions::package::PLUGIN_SCHEMA,"name":"lifecycle-control","version":"1","description":"Synthetic exact CLI cancellation","license":"MIT"})).unwrap()).unwrap();
    std::fs::write(package.join("LICENSE"), "Synthetic offline license").unwrap();
    std::fs::write(package.join("mcp.json"),serde_json::to_vec(&json!({"$schema":argentum_extensions::package::MCP_SCHEMA,"mcpServers":{"stdio":{"type":"stdio","command":program.to_string_lossy(),"args":args}}})).unwrap()).unwrap();
    let inspected = argentum_extensions::package::inspect(&package).unwrap();
    let lease = argentum_execution::VerifiedFileLease::open(
        std::path::PathBuf::from(&program),
        256 * 1024 * 1024,
    )
    .unwrap();
    let definition = ExtensionDefinition {
        namespace: "lifecycle-control.stdio".into(),
        package_root: package.to_string_lossy().into_owned(),
        package_sha256: Sha256Digest(inspected.digest),
        source_id: "synthetic-offline".into(),
        source_revision: "v1".into(),
        transport: ExtensionTransport::Stdio {
            program: program.to_string_lossy().into_owned(),
            program_sha256: Sha256Digest(lease.sha256().into()),
            args,
            cwd: ws.to_string_lossy().into_owned(),
            environment: BTreeMap::new(),
            inputs: BTreeMap::new(),
        },
        allowed_tools: vec!["echo".into()],
        resource_prefixes: vec![],
        trust: ExtensionTrust::ReviewedCurrentUserCode,
        image_scope: None,
    };
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let endpoint = format!("http://{}/v1/", listener.local_addr().unwrap());
    let db = root.path().join("store.sqlite3");
    let mut config = HostConfig::in_memory(&ws);
    config.database = Some(db.clone());
    let mut host = CommandHost::start_composed(config).await.unwrap();
    let local = host.local_handle();
    local
        .dispatch(AppCommand::SaveProviderProfile {
            profile: ProviderProfile {
                id: "lifecycle-fixture".into(),
                label: "Offline controlled responses".into(),
                kind: ProviderKind::OpenAiCompatible,
                endpoint,
                model: "fixture-model".into(),
                selected: true,
            },
        })
        .await
        .unwrap();
    local
        .dispatch(AppCommand::SelectProviderProfile {
            provider_id: "lifecycle-fixture".into(),
        })
        .await
        .unwrap();
    local
        .dispatch(AppCommand::SelectExecutionProfile {
            profile_id: "verification".into(),
        })
        .await
        .unwrap();
    host.declare_model_budget("lifecycle-fixture", [204800, 196608, 4096, 4096, 1024])
        .unwrap();
    local.configure_extension(definition).await.unwrap();
    local
        .activate_extension("lifecycle-control.stdio")
        .await
        .unwrap();
    host.shutdown().await.unwrap();
    let server = std::thread::spawn(move || {
        let deadline = Instant::now() + Duration::from_secs(40);
        for round in 0..2 {
            let (mut stream, _) = loop {
                match listener.accept() {
                    Ok(value) => break value,
                    Err(_) if Instant::now() < deadline => {
                        std::thread::sleep(Duration::from_millis(5))
                    }
                    Err(error) => panic!("controlled response deadline: {error}"),
                }
            };
            stream.set_nonblocking(false).unwrap();
            stream
                .set_read_timeout(Some(Duration::from_secs(10)))
                .unwrap();
            let request = http_support::read_request(&mut stream);
            let operation = if round == 0 {
                json!({"action":"discover_tools","namespace":"lifecycle-control.stdio"})
            } else {
                let message = request["messages"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .rev()
                    .find(|message| message["role"] == "tool")
                    .unwrap();
                let result: Value =
                    serde_json::from_str(message["content"].as_str().unwrap()).unwrap();
                json!({"action":"invoke","namespace":"lifecycle-control.stdio","handle":result["tools"][0]["handle"],"arguments":{"message":"controlled activity"}})
            };
            let frame = json!({"choices":[{"index":0,"delta":{"tool_calls":[{"index":0,"id":format!("lifecycle-{round}"),"type":"function","function":{"name":"extension","arguments":serde_json::to_string(&json!({"operation":operation})).unwrap()}}]},"finish_reason":null}]});
            let terminal = json!({"choices":[{"index":0,"delta":{},"finish_reason":"tool_calls"}],"usage":{"prompt_tokens":10,"completion_tokens":10,"total_tokens":20}});
            stream.write_all(b"HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nConnection: close\r\n\r\n").unwrap();
            write!(
                stream,
                "data: {frame}\n\ndata: {terminal}\n\ndata: [DONE]\n\n"
            )
            .unwrap();
        }
    });
    let mut cli = OwnedCli(
        Command::new(&exe)
            .args(["serve", "--isolated", "--workspace"])
            .arg(&ws)
            .arg("--database")
            .arg(&db)
            .env("RUST_LOG", "off")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap(),
    );
    let mut input = cli.0.stdin.take().unwrap();
    let stdout = cli.0.stdout.take().unwrap();
    let mut stderr = cli.0.stderr.take().unwrap();
    let errors = std::thread::spawn(move || {
        let mut value = String::new();
        stderr.read_to_string(&mut value).unwrap();
        value
    });
    let (sender, events) = mpsc::channel();
    let reader = std::thread::spawn(move || {
        for line in BufReader::new(stdout).lines() {
            let event: ResponseEnvelope = serde_json::from_str(&line.unwrap()).unwrap();
            if sender.send(event).is_err() {
                break;
            }
        }
    });
    writeln!(
        input,
        "{}",
        serde_json::to_string(&RequestEnvelope::command(
            "lifecycle-task",
            AppCommand::SubmitTask {
                prompt: "Discover and invoke the controlled slow tool.".into()
            }
        ))
        .unwrap()
    )
    .unwrap();
    input.flush().unwrap();
    let run = loop {
        let response = events.recv_timeout(Duration::from_secs(15)).unwrap();
        if let ServerPayload::Event { event } = response.payload {
            if let AppEvent::RunStatusChanged {
                run_id,
                lifecycle: TaskLifecycle::Running,
            } = *event
            {
                break run_id;
            }
        }
    };
    let deadline = Instant::now() + Duration::from_secs(15);
    while !marker.exists() {
        assert!(
            Instant::now() < deadline,
            "real external call did not start"
        );
        std::thread::sleep(Duration::from_millis(10));
    }
    let pid = std::fs::read_to_string(&marker)
        .unwrap()
        .parse::<u32>()
        .unwrap();
    assert_ne!(pid, std::process::id());
    use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle};
    let process = unsafe {
        windows_sys::Win32::System::Threading::OpenProcess(
            windows_sys::Win32::System::Threading::PROCESS_SYNCHRONIZE,
            0,
            pid,
        )
    };
    assert!(
        !process.is_null(),
        "retain the exact owned server before cancellation"
    );
    // Read-only synchronization handle, not PID-based authority or termination.
    let process = unsafe { OwnedHandle::from_raw_handle(process) };
    writeln!(
        input,
        "{}",
        serde_json::to_string(&RequestEnvelope::command(
            "lifecycle-cancel",
            AppCommand::CancelRun { run_id: run }
        ))
        .unwrap()
    )
    .unwrap();
    input.flush().unwrap();
    let deadline = Instant::now() + Duration::from_secs(15);
    let mut cancelled = false;
    while Instant::now() < deadline {
        let response = events.recv_timeout(Duration::from_secs(2)).unwrap();
        if let ServerPayload::Event { event } = response.payload {
            assert!(!matches!(
                *event,
                AppEvent::RunStatusChanged {
                    lifecycle: TaskLifecycle::Complete,
                    ..
                }
            ));
            if matches!(*event,AppEvent::RunStatusChanged {run_id,lifecycle:TaskLifecycle::Cancelled} if run_id==run)
            {
                cancelled = true;
                break;
            }
        }
    }
    assert!(cancelled);
    drop(input);
    let deadline = Instant::now() + Duration::from_secs(15);
    let status = loop {
        if let Some(status) = cli.0.try_wait().unwrap() {
            break status;
        }
        assert!(Instant::now() < deadline, "actual CLI shutdown must settle");
        std::thread::sleep(Duration::from_millis(10));
    };
    assert!(status.success(), "{}", errors.join().unwrap());
    reader.join().unwrap();
    server.join().unwrap();
    assert_eq!(
        unsafe {
            windows_sys::Win32::System::Threading::WaitForSingleObject(process.as_raw_handle(), 0)
        },
        windows_sys::Win32::Foundation::WAIT_OBJECT_0,
        "actual owned MCP process must be reaped before CLI shutdown returns"
    );
    let current = Command::new(&exe)
        .args(["extensions", "status", "--workspace"])
        .arg(&ws)
        .arg("--database")
        .arg(&db)
        .env("RUST_LOG", "off")
        .output()
        .unwrap();
    assert!(current.status.success());
    let text = String::from_utf8(current.stdout).unwrap();
    assert!(text.contains("\"active\": false"));
    assert!(text.contains("\"process_started\": false"));
    let remove = Command::new(&exe)
        .args([
            "extensions",
            "disable",
            "lifecycle-control.stdio",
            "--workspace",
        ])
        .arg(&ws)
        .arg("--database")
        .arg(&db)
        .output()
        .unwrap();
    assert!(remove.status.success());
    assert_eq!(
        std::fs::read_to_string(ws.join("keep.txt")).unwrap(),
        "PRESERVED_CANCEL_CONTROL"
    );
    // A fresh process status/disable cannot replay the interrupted call.
    assert_eq!(std::fs::read_to_string(&marker).unwrap(), pid.to_string());
}
