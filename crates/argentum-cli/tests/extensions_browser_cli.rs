#![cfg(all(windows, feature = "extensions"))]
//! Controlled model responses exercise the actual CLI, not an SDK-only route.
//! No real provider, account, session state or inference is used.
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
    path::PathBuf,
    process::{Command, Stdio},
    time::{Duration, Instant},
};
#[path = "support/http.rs"]
mod http_support;

#[tokio::test]
#[ignore = "requires reviewed pinned Node/Playwright and browser cache; qualification runs explicitly"]
async fn actual_cli_preview_observation_interaction_feedback_and_owned_teardown() {
    let node =
        PathBuf::from(std::env::var_os("ARGENTUM_NEXT01_NODE").expect("reviewed Node required"));
    let browser_cache =
        std::env::var("ARGENTUM_PLAYWRIGHT_BROWSERS_PATH").expect("pinned browser cache required");
    // Node's module entry loader does not accept Windows extended path syntax.
    // Use the absolute native spelling in argv, while VerifiedFileLease still
    // binds its canonical physical identity. Do not rewrite arbitrary model args.
    let adapter = std::env::var_os("ARGENTUM_NEXT01_ADAPTER")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .parent()
                .unwrap()
                .parent()
                .unwrap()
                .join("extensions/browser/adapter.mjs")
        });
    assert!(adapter.is_absolute() && adapter.is_file());
    let exe = std::env::var_os("ARGENTUM_NEXT01_CLI_EXE")
        .map(PathBuf::from)
        .unwrap_or_else(|| env!("CARGO_BIN_EXE_argentum").into());
    let root = tempfile::tempdir().unwrap();
    let ws = root.path().join("workspace");
    std::fs::create_dir(&ws).unwrap();
    std::fs::write(ws.join("index.html"), "<h1>Actual app control</h1><p id=count>0</p><button onclick=\"document.querySelector('#count').textContent='1'\">Increment</button>").unwrap();
    std::fs::write(ws.join("keep.txt"), "UNCHANGED_BROWSER_CLI").unwrap();
    let reservation = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = reservation.local_addr().unwrap().port();
    drop(reservation);
    let origin = format!("http://127.0.0.1:{port}");
    let config_path = root.path().join("browser.json");
    std::fs::write(&config_path, serde_json::to_vec(&json!({"allowedOrigins":[origin],"artifactDirectory":root.path().join("images"),"previewWorkspace":ws,"previewPort":port})).unwrap()).unwrap();
    let args = vec![
        adapter.to_string_lossy().into_owned(),
        config_path.to_string_lossy().into_owned(),
    ];
    let package = root.path().join("package");
    std::fs::create_dir(&package).unwrap();
    std::fs::write(package.join("plugin.json"), serde_json::to_vec(&json!({"$schema":argentum_extensions::package::PLUGIN_SCHEMA,"name":"browser-control","version":"1","description":"First-party controlled preview","license":"LicenseRef-Argentum"})).unwrap()).unwrap();
    std::fs::copy(
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../LICENSE"),
        package.join("LICENSE"),
    )
    .unwrap();
    std::fs::write(package.join("mcp.json"),serde_json::to_vec(&json!({"$schema":argentum_extensions::package::MCP_SCHEMA,"mcpServers":{"stdio":{"type":"stdio","command":node,"args":args}}})).unwrap()).unwrap();
    let inspected = argentum_extensions::package::inspect(&package).unwrap();
    let digest = |path: &std::path::Path| {
        Sha256Digest(
            argentum_execution::VerifiedFileLease::open(path, 256 * 1024 * 1024)
                .unwrap()
                .sha256()
                .into(),
        )
    };
    let definition = ExtensionDefinition {
        namespace: "browser-control.stdio".into(),
        package_root: package.to_string_lossy().into_owned(),
        package_sha256: Sha256Digest(inspected.digest),
        source_id: "first-party-offline-control".into(),
        source_revision: "v1".into(),
        transport: ExtensionTransport::Stdio {
            program: node.to_string_lossy().into_owned(),
            program_sha256: digest(&node),
            args,
            cwd: ws.to_string_lossy().into_owned(),
            environment: BTreeMap::from([(
                "ARGENTUM_PLAYWRIGHT_BROWSERS_PATH".into(),
                browser_cache,
            )]),
            inputs: BTreeMap::from([
                (adapter.to_string_lossy().into_owned(), digest(&adapter)),
                (
                    config_path.to_string_lossy().into_owned(),
                    digest(&config_path),
                ),
            ]),
        },
        allowed_tools: vec![
            "browser_preview_start",
            "browser_open",
            "browser_observe",
            "browser_interact",
            "browser_diagnostics",
            "browser_close",
        ]
        .into_iter()
        .map(str::to_owned)
        .collect(),
        resource_prefixes: vec![],
        trust: ExtensionTrust::ReviewedCurrentUserCode,
        image_scope: None,
    };
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let endpoint = format!("http://{}/v1/", listener.local_addr().unwrap());
    let database = root.path().join("store.sqlite3");
    let mut config = HostConfig::in_memory(&ws);
    config.database = Some(database.clone());
    let mut host = CommandHost::start_composed(config).await.unwrap();
    let local = host.local_handle();
    local
        .dispatch(AppCommand::SaveProviderProfile {
            profile: ProviderProfile {
                id: "browser-fixture".into(),
                label: "Offline browser fixture".into(),
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
            provider_id: "browser-fixture".into(),
        })
        .await
        .unwrap();
    local
        .dispatch(AppCommand::SelectExecutionProfile {
            profile_id: "verification".into(),
        })
        .await
        .unwrap();
    host.declare_model_budget("browser-fixture", [204800, 196608, 4096, 4096, 1024])
        .unwrap();
    local.configure_extension(definition).await.unwrap();
    local
        .activate_extension("browser-control.stdio")
        .await
        .unwrap();
    host.shutdown().await.unwrap();
    let observed_origin = origin.clone();
    let worker = std::thread::spawn(move || {
        let deadline = Instant::now() + Duration::from_secs(90);
        let mut handles = BTreeMap::new();
        let mut tab = Value::Null;
        let mut observation = Value::Null;
        for round in 0..10 {
            let (mut stream, _) = loop {
                match listener.accept() {
                    Ok(value) => break value,
                    Err(_) if Instant::now() < deadline => {
                        std::thread::sleep(Duration::from_millis(5))
                    }
                    Err(error) => panic!("controlled request deadline: {error}"),
                }
            };
            stream.set_nonblocking(false).unwrap();
            stream
                .set_read_timeout(Some(Duration::from_secs(10)))
                .unwrap();
            let request = http_support::read_request(&mut stream);
            let previous = if round > 0 {
                let message = request["messages"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .rev()
                    .find(|message| message["role"] == "tool")
                    .unwrap();
                serde_json::from_str::<Value>(message["content"].as_str().unwrap()).unwrap()
            } else {
                Value::Null
            };
            if round == 1 || round == 2 {
                for tool in previous["tools"]
                    .as_array()
                    .unwrap_or_else(|| panic!("round {round} discovery feedback: {previous}"))
                {
                    handles.insert(
                        tool["handle"]["name"].as_str().unwrap().to_owned(),
                        tool["handle"].clone(),
                    );
                }
            }
            if round == 4 {
                tab = previous["structuredContent"]["tab_id"].clone();
                assert!(tab.is_string());
            }
            if round == 5 {
                observation = previous["structuredContent"].clone();
                assert!(observation["accessibility"]
                    .as_str()
                    .unwrap()
                    .contains("Actual app control"));
            }
            if round == 7 {
                assert!(previous["structuredContent"]["accessibility"]
                    .as_str()
                    .unwrap()
                    .contains("1"));
            }
            let operation = match round {
                0 => Some(json!({"action":"discover_tools","namespace":"browser-control.stdio"})),
                1 => Some(
                    json!({"action":"discover_tools","namespace":"browser-control.stdio","cursor":previous["next_cursor"]}),
                ),
                2 => Some(invoke(&handles, "browser_preview_start", json!({}))),
                3 => Some(invoke(
                    &handles,
                    "browser_open",
                    json!({"url":observed_origin}),
                )),
                4 => Some(invoke(&handles, "browser_observe", json!({"tab_id":tab}))),
                5 => {
                    let target = observation["targets"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .find(|value| value["name"] == "Increment")
                        .unwrap();
                    Some(invoke(
                        &handles,
                        "browser_interact",
                        json!({"tab_id":tab,"revision":observation["revision"],"reference":target["reference"],"action":"click"}),
                    ))
                }
                6 => Some(invoke(&handles, "browser_observe", json!({"tab_id":tab}))),
                7 => Some(invoke(
                    &handles,
                    "browser_diagnostics",
                    json!({"tab_id":tab}),
                )),
                8 => Some(invoke(&handles, "browser_close", json!({}))),
                _ => {
                    assert_eq!(previous["structuredContent"]["closed"], true);
                    None
                }
            };
            stream.write_all(b"HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nConnection: close\r\n\r\n").unwrap();
            let frame = if let Some(operation) = operation {
                json!({"choices":[{"index":0,"delta":{"tool_calls":[{"index":0,"id":format!("browser-{round}"),"type":"function","function":{"name":"extension","arguments":serde_json::to_string(&json!({"operation":operation})).unwrap()}}]},"finish_reason":null}]})
            } else {
                json!({"choices":[{"index":0,"delta":{"content":"Actual CLI browser interaction and teardown observed."},"finish_reason":null}]})
            };
            write!(stream, "data: {frame}\n\n").unwrap();
            let terminal = json!({"choices":[{"index":0,"delta":{},"finish_reason":if round<9 {"tool_calls"}else{"stop"}}],"usage":{"prompt_tokens":10,"completion_tokens":10,"total_tokens":20}});
            write!(stream, "data: {terminal}\n\ndata: [DONE]\n\n").unwrap();
        }
    });
    let output=Command::new(exe).args(["run","--isolated","--workspace"]).arg(&ws).arg("--database").arg(database).args(["--prompt","Use the reviewed browser to inspect and exercise the controlled local preview, then close it.","--no-color"]).env("RUST_LOG","off").stdin(Stdio::null()).output().unwrap();
    worker.join().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        std::fs::read_to_string(ws.join("keep.txt")).unwrap(),
        "UNCHANGED_BROWSER_CLI"
    );
    assert!(
        std::net::TcpStream::connect(("127.0.0.1", port)).is_err(),
        "owned preview must settle"
    );
    let inspection = Command::new(
        std::env::var_os("ARGENTUM_NEXT01_CLI_EXE")
            .map(PathBuf::from)
            .unwrap_or_else(|| env!("CARGO_BIN_EXE_argentum").into()),
    )
    .args(["extensions", "receipts", "--workspace"])
    .arg(&ws)
    .arg("--database")
    .arg(root.path().join("store.sqlite3"))
    .output()
    .unwrap();
    assert!(
        inspection.status.success(),
        "{}",
        String::from_utf8_lossy(&inspection.stderr)
    );
    let inspected: Value = serde_json::from_slice(&inspection.stdout).unwrap();
    let receipts = inspected["receipts"].as_array().unwrap();
    let completed = receipts
        .iter()
        .filter(|item| item["receipt"]["stage"] == "completed")
        .collect::<Vec<_>>();
    assert_eq!(completed.len(), 9);
    assert!(completed
        .iter()
        .all(|item| item["receipt"]["application_condition_verified"] == false));
    assert!(completed
        .iter()
        .any(|item| item["receipt"]["transport"]["mcp_request_starts"]
            .as_u64()
            .unwrap_or(0)
            > 2));
    assert!(completed
        .iter()
        .any(|item| item["receipt"]["server_report"]["actions"]
            .as_u64()
            .unwrap_or(0)
            > 1));
    assert!(receipts
        .iter()
        .any(|item| item["receipt"]["stage"] == "teardown"
            && item["receipt"]["outcome"] == "drained"));
    assert!(!String::from_utf8_lossy(&inspection.stdout).contains("Actual app control"));
}
fn invoke(handles: &BTreeMap<String, Value>, name: &str, arguments: Value) -> Value {
    json!({"action":"invoke","namespace":"browser-control.stdio","handle":handles.get(name).expect("scoped discovered handle"),"arguments":arguments})
}
