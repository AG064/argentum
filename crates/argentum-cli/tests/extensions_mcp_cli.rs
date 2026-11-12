#![cfg(all(windows, feature = "extensions"))]
use argentum_cli::{CommandHost, HostConfig};
use argentum_domain::{
    extensions::{ExtensionDefinition, ExtensionTransport, ExtensionTrust},
    AppCommand, ProviderKind, ProviderProfile, Sha256Digest,
};
use serde_json::{json, Value};
use std::{
    collections::BTreeMap,
    io::Write,
    net::TcpListener,
    process::{Command, Stdio},
    time::{Duration, Instant},
};
#[path = "support/http.rs"]
mod http_support;

#[tokio::test]
#[ignore = "requires pinned public SDK executable; exact application qualification runs explicitly"]
async fn actual_cli_discovers_public_mcp_invokes_and_returns_output_to_next_request() {
    let program = std::env::var_os("ARGENTUM_PUBLIC_MCP_CONTROL_EXE")
        .expect("pinned public executable required");
    let exe = std::env::var_os("ARGENTUM_NEXT01_CLI_EXE")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| env!("CARGO_BIN_EXE_argentum").into());
    let root = tempfile::tempdir().unwrap();
    let workspace = root.path().join("workspace");
    std::fs::create_dir(&workspace).unwrap();
    std::fs::write(workspace.join("keep.txt"), "UNCHANGED_ACTUAL_APP_CONTROL").unwrap();
    let package = root.path().join("package");
    std::fs::create_dir(&package).unwrap();
    std::fs::write(package.join("plugin.json"),serde_json::to_vec(&json!({"$schema":argentum_extensions::package::PLUGIN_SCHEMA,"name":"public-counter","version":"1","description":"Pinned licensed public SDK counter","license":"Apache-2.0 AND MIT"})).unwrap()).unwrap();
    std::fs::copy(
        std::env::var_os("ARGENTUM_PUBLIC_MCP_CONTROL_LICENSE")
            .expect("pinned public SDK license required"),
        package.join("LICENSE"),
    )
    .unwrap();
    std::fs::write(package.join("mcp.json"),serde_json::to_vec(&json!({"$schema":argentum_extensions::package::MCP_SCHEMA,"mcpServers":{"stdio":{"type":"stdio","command":program.to_string_lossy(),"args":[]}}})).unwrap()).unwrap();
    let inspected = argentum_extensions::package::inspect(&package).unwrap();
    let lease = argentum_execution::VerifiedFileLease::open(
        std::path::PathBuf::from(&program),
        256 * 1024 * 1024,
    )
    .unwrap();
    let definition = ExtensionDefinition {
        namespace: "public-counter.stdio".into(),
        package_root: package.to_string_lossy().into_owned(),
        package_sha256: Sha256Digest(inspected.digest),
        source_id: "https://github.com/modelcontextprotocol/rust-sdk".into(),
        source_revision: "79437f291b2c44053d00dcd5db969fd0cca7c887".into(),
        transport: ExtensionTransport::Stdio {
            program: program.to_string_lossy().into_owned(),
            program_sha256: Sha256Digest(lease.sha256().into()),
            args: vec![],
            cwd: workspace.to_string_lossy().into_owned(),
            environment: BTreeMap::new(),
            inputs: BTreeMap::new(),
        },
        allowed_tools: vec!["increment".into(), "get_value".into()],
        resource_prefixes: vec!["memo://".into()],
        trust: ExtensionTrust::ReviewedCurrentUserCode,
        image_scope: None,
    };
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let endpoint = format!("http://{}/v1/", listener.local_addr().unwrap());
    let database = root.path().join("store.sqlite3");
    let mut config = HostConfig::in_memory(&workspace);
    config.database = Some(database.clone());
    let mut host = CommandHost::start_composed(config).await.unwrap();
    let local = host.local_handle();
    local
        .dispatch(AppCommand::SaveProviderProfile {
            profile: ProviderProfile {
                id: "mcp-cli-fixture".into(),
                label: "Offline controlled provider".into(),
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
            provider_id: "mcp-cli-fixture".into(),
        })
        .await
        .unwrap();
    local
        .dispatch(AppCommand::SelectExecutionProfile {
            profile_id: "verification".into(),
        })
        .await
        .unwrap();
    host.declare_model_budget("mcp-cli-fixture", [204800, 196608, 4096, 4096, 1024])
        .unwrap();
    local.configure_extension(definition).await.unwrap();
    local
        .activate_extension("public-counter.stdio")
        .await
        .unwrap();
    host.shutdown().await.unwrap();
    let worker = std::thread::spawn(move || {
        let deadline = Instant::now() + Duration::from_secs(45);
        let mut handle = None;
        for round in 0..3 {
            let (mut stream, _) = loop {
                match listener.accept() {
                    Ok(value) => break value,
                    Err(_) if Instant::now() < deadline => {
                        std::thread::sleep(Duration::from_millis(5))
                    }
                    Err(error) => panic!("actual CLI synthetic provider timed out: {error}"),
                }
            };
            stream.set_nonblocking(false).unwrap();
            stream
                .set_read_timeout(Some(Duration::from_secs(5)))
                .unwrap();
            let request = http_support::read_request(&mut stream);
            let operation = if round == 0 {
                Some(json!({"action":"discover_tools","namespace":"public-counter.stdio"}))
            } else if round == 1 {
                let content = request["messages"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .rev()
                    .find(|message| message["role"] == "tool")
                    .unwrap()["content"]
                    .as_str()
                    .unwrap();
                let value: Value = serde_json::from_str(content).unwrap();
                let tool = value["tools"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .find(|tool| tool["handle"]["name"] == "increment")
                    .unwrap();
                handle = Some(tool["handle"].clone());
                Some(
                    json!({"action":"invoke","namespace":"public-counter.stdio","handle":handle.as_ref().unwrap(),"arguments":{}}),
                )
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
                assert_eq!(result["content"][0]["text"], "1");
                assert!(
                    result["_transport_accounting"]["sent_messages"]
                        .as_u64()
                        .unwrap()
                        >= 3
                );
                None
            };
            let frame = if let Some(operation) = operation {
                json!({"choices":[{"index":0,"delta":{"tool_calls":[{"index":0,"id":format!("mcp-{round}"),"type":"function","function":{"name":"extension","arguments":serde_json::to_string(&json!({"operation":operation})).unwrap()}}]},"finish_reason":null}]})
            } else {
                json!({"choices":[{"index":0,"delta":{"content":"Public scoped counter output observed through the actual application."},"finish_reason":null}]})
            };
            stream.write_all(b"HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nConnection: close\r\n\r\n").unwrap();
            write!(stream, "data: {frame}\n\n").unwrap();
            let terminal = json!({"choices":[{"index":0,"delta":{},"finish_reason":if round<2{"tool_calls"}else{"stop"}}],"usage":{"prompt_tokens":10,"completion_tokens":10,"total_tokens":20}});
            write!(stream, "data: {terminal}\n\ndata: [DONE]\n\n").unwrap();
        }
        handle
    });
    let output = Command::new(exe)
        .args(["run", "--isolated", "--workspace"])
        .arg(&workspace)
        .arg("--database")
        .arg(database)
        .args([
            "--prompt",
            "Inspect and invoke the scoped public counter through normal extension tools.",
            "--no-color",
        ])
        .env("RUST_LOG", "off")
        .stdin(Stdio::null())
        .output()
        .unwrap();
    assert!(worker.join().unwrap().is_some());
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        std::fs::read_to_string(workspace.join("keep.txt")).unwrap(),
        "UNCHANGED_ACTUAL_APP_CONTROL"
    );
}
