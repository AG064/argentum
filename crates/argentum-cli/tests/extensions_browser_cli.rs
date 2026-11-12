#![cfg(all(windows, feature = "extensions"))]
//! Controlled model responses exercise the actual CLI, not an SDK-only route.
//! No real provider, account, session state or inference is used.
use argentum_cli::{CommandHost, HostConfig};
use argentum_domain::{
    extensions::{ExtensionDefinition, ExtensionTransport, ExtensionTrust},
    AppCommand, ProviderKind, ProviderProfile, Sha256Digest,
};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
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
    browser_cli_case(false).await;
}

#[tokio::test]
#[ignore = "requires reviewed pinned Node/Playwright, browser cache and fixed Cargo; qualification runs explicitly"]
async fn actual_cli_reviewed_stylesheet_activation_native_feedback_and_owned_teardown() {
    browser_cli_case(true).await;
}

async fn browser_cli_case(reviewed: bool) {
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
    let stylesheet = "/* synthetic reviewed stylesheet */\n#count { color: rgb(17, 34, 51); }\n";
    if reviewed {
        std::fs::create_dir(ws.join("styles")).unwrap();
        std::fs::write(ws.join("styles/tokens.css"), stylesheet).unwrap();
        std::fs::write(ws.join("index.html"), "<link rel=stylesheet href=/styles/tokens.css><h1>Actual app control</h1><p id=count>0</p><button onclick=\"document.querySelector('#count').textContent='1'\">Increment</button>").unwrap();
        std::fs::create_dir(ws.join("src")).unwrap();
        std::fs::write(
            ws.join("Cargo.toml"),
            "[package]\nname=\"reviewed_preview_control\"\nversion=\"0.1.0\"\nedition=\"2021\"\n",
        )
        .unwrap();
        std::fs::write(ws.join("Cargo.lock"), "version = 4\n\n[[package]]\nname = \"reviewed_preview_control\"\nversion = \"0.1.0\"\n").unwrap();
        std::fs::write(ws.join("src/lib.rs"), format!("#[test]\nfn reviewed_public_css_is_current() {{\n    assert_eq!(std::fs::read_to_string(\"styles/tokens.css\").unwrap(), {stylesheet:?});\n}}\n")).unwrap();
    }
    let reservation = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = reservation.local_addr().unwrap().port();
    drop(reservation);
    let origin = format!("http://127.0.0.1:{port}");
    let config_path = root.path().join("browser.json");
    let mut browser_config = json!({"allowedOrigins":[origin],"artifactDirectory":root.path().join("images"),"previewWorkspace":ws,"previewPort":port});
    if reviewed {
        browser_config["reviewedStylesheet"] = json!({"path":"styles/tokens.css","sha256":format!("{:x}",Sha256::digest(stylesheet.as_bytes())),"bytes":stylesheet.len()});
    }
    std::fs::write(&config_path, serde_json::to_vec(&browser_config).unwrap()).unwrap();
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
    let mut definition = ExtensionDefinition {
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
    local.configure_extension(definition.clone()).await.unwrap();
    let activated = local
        .activate_extension("browser-control.stdio")
        .await
        .unwrap();
    host.shutdown().await.unwrap();
    if reviewed {
        // Owner fixture preparation only. A byte-different configuration must
        // revoke the prior exact activation, even when its JSON meaning is equal.
        std::fs::write(
            &config_path,
            serde_json::to_vec_pretty(&browser_config).unwrap(),
        )
        .unwrap();
        if let ExtensionTransport::Stdio { inputs, .. } = &mut definition.transport {
            inputs.insert(
                config_path.to_string_lossy().into_owned(),
                digest(&config_path),
            );
        }
        let mut setup = HostConfig::in_memory(&ws);
        setup.database = Some(database.clone());
        let mut owner_host = CommandHost::start_composed(setup).await.unwrap();
        let updated = owner_host
            .local_handle()
            .configure_extension(definition)
            .await
            .unwrap();
        assert!(!updated.active);
        assert_ne!(updated.revision, activated.revision);
        owner_host.shutdown().await.unwrap();
        let inactive = fixture_command(&exe, &ws, &database, &["extensions", "list"])
            .output()
            .unwrap();
        assert!(
            inactive.status.success(),
            "{}",
            String::from_utf8_lossy(&inactive.stderr)
        );
        // The administrative list includes an explanatory trailing text line.
        let listed: Value = serde_json::Deserializer::from_slice(&inactive.stdout)
            .into_iter::<Value>()
            .next()
            .unwrap()
            .unwrap();
        assert!(listed
            .as_array()
            .unwrap()
            .iter()
            .any(
                |row| row["definition"]["namespace"] == "browser-control.stdio"
                    && row["active"] == false
            ));
        let mut setup = HostConfig::in_memory(&ws);
        setup.database = Some(database.clone());
        let mut owner_host = CommandHost::start_composed(setup).await.unwrap();
        let reactivated = owner_host
            .local_handle()
            .activate_extension("browser-control.stdio")
            .await
            .unwrap();
        assert!(reactivated.active);
        assert_ne!(reactivated.approval_binding, activated.approval_binding);
        retain_preview(
            "activation",
            &json!({"previous_revision":activated.revision,"updated_inactive_revision":updated.revision,"fresh_cli_list":listed,"reactivated_revision":reactivated.revision,"bindings_changed":true,"fixture_preparation":"test-built owner CommandHost, not owner production activation"}),
        );
        owner_host.shutdown().await.unwrap();
    }
    let goal = reviewed.then(|| {
        let created = fixture_json(&exe, &ws, &database, &["goal","set","--objective","Inspect the reviewed local preview, close owned browser work, and require the current public Rust test before completing.","--token-budget","2000000","--tool-budget","64","--time-budget","300","--isolated","--json"]);
        let id = created["goal"]["id"].as_str().unwrap().to_owned();
        fixture_json(&exe, &ws, &database, &["goal","limits","--goal-id",&id,"--limits","2000000,32,8,64,300,1000000,32,32,180","--isolated","--json"]);
        fixture_json(&exe, &ws, &database, &["goal","criteria","--goal-id",&id,"--runners","cargo-test-locked","--isolated","--json"]);
        id
    });
    let observed_origin = origin.clone();
    let worker = std::thread::spawn(move || {
        let deadline = Instant::now() + Duration::from_secs(if reviewed { 180 } else { 90 });
        let mut handles = BTreeMap::new();
        let mut tab = Value::Null;
        let mut observation = Value::Null;
        for round in 0..if reviewed { 11 } else { 10 } {
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
            if reviewed {
                assert!(request["tools"]
                    .as_array()
                    .is_none_or(|tools| !tools.iter().any(|tool| matches!(
                        tool["function"]["name"].as_str(),
                        Some("execute_argv" | "execute_shell")
                    ))));
            }
            if reviewed && round == 10 {
                retain_preview("post-preview-request", &request);
                let feedback = request["messages"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .filter(|message| message["role"] == "system")
                    .filter_map(|message| message["content"].as_str())
                    .flat_map(str::lines)
                    .find_map(|line| {
                        // Instruction projection follows host guidance without
                        // guaranteeing a newline after the feedback object.
                        serde_json::Deserializer::from_str(line)
                            .into_iter::<Value>()
                            .next()
                            .and_then(Result::ok)
                            .filter(|value| value.get("native_verifier_feedback").is_some())
                    })
                    .expect("current native verifier feedback JSON must reach the next request");
                assert!(
                    feedback["native_verifier_feedback"]["output"]
                        .to_string()
                        .contains("passed"),
                    "{feedback}"
                );
                assert_eq!(
                    feedback["native_verifier_feedback"]["current_source_matches"],
                    true
                );
                assert!(feedback["native_verifier_feedback"]["verifier_run_id"].is_string());
                retain_preview("native-verifier-feedback", &request);
            }
            let previous = if round > 0 && round < 10 {
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
                9 => {
                    assert_eq!(previous["structuredContent"]["closed"], true);
                    None
                }
                _ => None,
            };
            stream.write_all(b"HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nConnection: close\r\n\r\n").unwrap();
            let frame = if let Some(operation) = operation {
                json!({"choices":[{"index":0,"delta":{"tool_calls":[{"index":0,"id":format!("browser-{round}"),"type":"function","function":{"name":"extension","arguments":serde_json::to_string(&json!({"operation":operation})).unwrap()}}]},"finish_reason":null}]})
            } else {
                let answer = if reviewed && round == 10 {
                    "{\"status\":\"ready\",\"remaining\":[],\"summary\":\"Current native verification feedback and owned browser closure observed.\"}"
                } else if reviewed {
                    "{\"status\":\"ready\",\"remaining\":[],\"summary\":\"Reviewed preview interaction and owned closure observed; current native verification is still required.\"}"
                } else {
                    "Actual CLI browser interaction and teardown observed."
                };
                json!({"choices":[{"index":0,"delta":{"content":answer},"finish_reason":null}]})
            };
            write!(stream, "data: {frame}\n\n").unwrap();
            let terminal = json!({"choices":[{"index":0,"delta":{},"finish_reason":if round<9 {"tool_calls"}else{"stop"}}],"usage":{"prompt_tokens":10,"completion_tokens":10,"total_tokens":20}});
            write!(stream, "data: {terminal}\n\ndata: [DONE]\n\n").unwrap();
        }
    });
    let output = if let Some(goal) = &goal {
        fixture_command(
            &exe,
            &ws,
            &database,
            &[
                "goal",
                "deliver",
                "--goal-id",
                goal,
                "--iterations",
                "8",
                "--elapsed-seconds",
                "180",
                "--isolated",
                "--json",
                "--no-color",
            ],
        )
        .output()
        .unwrap()
    } else {
        fixture_command(&exe, &ws, &database, &["run","--isolated","--prompt","Use the reviewed browser to inspect and exercise the controlled local preview, then close it.","--no-color"]).output().unwrap()
    };
    if reviewed {
        retain_preview(
            "cli-output",
            &json!({"success":output.status.success(),"stdout":String::from_utf8_lossy(&output.stdout),"stderr":String::from_utf8_lossy(&output.stderr)}),
        );
    }
    worker.join().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    if reviewed {
        let result: Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(result["task_success"], true, "{result}");
        assert_eq!(result["invocation"]["state"], "verified");
        let status = fixture_json(
            &exe,
            &ws,
            &database,
            &["goal", "status", "--isolated", "--json"],
        );
        assert_eq!(
            status["supervision"]["completion_current"], true,
            "{status}"
        );
        retain_preview("native-result", &result);
        retain_preview("native-restarted-status", &status);
        let verification = fixture_json(
            &exe,
            &ws,
            &database,
            &["verification", "status", "--isolated", "--json"],
        );
        assert!(
            verification["evidence"]
                .as_array()
                .unwrap()
                .iter()
                .any(|row| row["fresh"] == true && row["evidence"]["state"] == "passed"),
            "{verification}"
        );
        retain_preview("current-native-verification", &verification);
        assert_eq!(
            std::fs::read_to_string(ws.join("styles/tokens.css")).unwrap(),
            stylesheet
        );
    }
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
    if reviewed {
        retain_preview("extension-receipts", &inspected);
    }
}
fn fixture_command(
    exe: &std::path::Path,
    workspace: &std::path::Path,
    database: &std::path::Path,
    args: &[&str],
) -> Command {
    let mut command = Command::new(exe);
    command
        .args(args)
        .arg("--workspace")
        .arg(workspace)
        .arg("--database")
        .arg(database)
        .env("RUST_LOG", "off")
        .stdin(Stdio::null());
    command
}
fn fixture_json(
    exe: &std::path::Path,
    workspace: &std::path::Path,
    database: &std::path::Path,
    args: &[&str],
) -> Value {
    let output = fixture_command(exe, workspace, database, args)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap()
}
fn retain_preview(label: &str, value: &Value) {
    if let Some(directory) = std::env::var_os("ARGENTUM_PREVIEW_NATIVE_EVIDENCE") {
        let directory = PathBuf::from(directory);
        assert!(directory.is_absolute());
        std::fs::create_dir_all(&directory).unwrap();
        std::fs::write(
            directory.join(format!("{label}.json")),
            serde_json::to_vec_pretty(value).unwrap(),
        )
        .unwrap();
    }
}
fn invoke(handles: &BTreeMap<String, Value>, name: &str, arguments: Value) -> Value {
    json!({"action":"invoke","namespace":"browser-control.stdio","handle":handles.get(name).expect("scoped discovered handle"),"arguments":arguments})
}
