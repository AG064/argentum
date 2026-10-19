#![cfg(all(windows, feature = "extensions"))]
use argentum_cli::{CommandHost, HostConfig};
use argentum_domain::{
    extensions::{ExtensionDefinition, ExtensionTransport, ExtensionTrust},
    AppCommand, ProviderKind, ProviderProfile, Sha256Digest,
};
use serde::Deserialize;
use serde_json::{json, Value};
use std::{
    io::{Read, Write},
    net::TcpListener,
    process::{Command, Stdio},
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};
#[path = "support/http.rs"]
mod http_support;

fn exe() -> std::path::PathBuf {
    std::env::var_os("ARGENTUM_NEXT01_CLI_EXE")
        .map(Into::into)
        .unwrap_or_else(|| env!("CARGO_BIN_EXE_argentum").into())
}

struct JsonQueryFixture {
    _root: tempfile::TempDir,
    workspace: std::path::PathBuf,
    database: std::path::PathBuf,
    records: Value,
    readiness: Value,
    provider_and_server: TcpListener,
}

impl JsonQueryFixture {
    async fn prepare(count: usize, varied_states: bool) -> Self {
        let root = tempfile::tempdir().unwrap();
        let workspace = root.path().join("workspace Märk with spaces");
        std::fs::create_dir(&workspace).unwrap();
        std::fs::write(workspace.join("keep.txt"), "JSON_QUERY_UNRELATED_FILE").unwrap();
        let database = root.path().join("store.sqlite3");
        let provider_and_server = TcpListener::bind("127.0.0.1:0").unwrap();
        provider_and_server.set_nonblocking(true).unwrap();
        let endpoint = format!("http://{}", provider_and_server.local_addr().unwrap());
        let mut config = HostConfig::in_memory(&workspace);
        config.database = Some(database.clone());
        let mut host = CommandHost::start_composed(config).await.unwrap();
        let local = host.local_handle();
        local
            .dispatch(AppCommand::SaveProviderProfile {
                profile: ProviderProfile {
                    id: "json-query-fixture".into(),
                    label: "Synthetic metadata, no requests".into(),
                    kind: ProviderKind::OpenAiCompatible,
                    endpoint: format!("{endpoint}/v1/"),
                    model: "fixture-only".into(),
                    selected: true,
                },
            })
            .await
            .unwrap();
        local
            .dispatch(AppCommand::SelectProviderProfile {
                provider_id: "json-query-fixture".into(),
            })
            .await
            .unwrap();
        local
            .dispatch(AppCommand::SelectExecutionProfile {
                profile_id: "verification".into(),
            })
            .await
            .unwrap();
        for index in 0..count {
            let name = format!("json-query-{index:02}");
            let package = root.path().join(&name);
            std::fs::create_dir_all(package.join("skills/control")).unwrap();
            std::fs::write(package.join("plugin.json"), serde_json::to_vec(&json!({"$schema":argentum_extensions::package::PLUGIN_SCHEMA,"name":name,"version":"1","description":"Synthetic query control","license":"MIT"})).unwrap()).unwrap();
            std::fs::write(package.join("LICENSE"), "Synthetic offline license").unwrap();
            std::fs::write(
                package.join("skills/control/SKILL.md"),
                "---\nname: control\ndescription: Query fixture\n---\nNo execution is requested.\n",
            )
            .unwrap();
            let remote = varied_states && index == 3;
            if remote {
                std::fs::write(package.join("mcp.json"), serde_json::to_vec(&json!({"$schema":argentum_extensions::package::MCP_SCHEMA,"mcpServers":{"remote":{"type":"streamable-http","url":format!("{endpoint}/mcp")}}})).unwrap()).unwrap();
            }
            let inspection = argentum_extensions::package::inspect(&package).unwrap();
            let namespace = format!("{name}.{}", if remote { "remote" } else { "skills" });
            let definition = ExtensionDefinition {
                namespace: namespace.clone(),
                package_root: package.to_string_lossy().into_owned(),
                package_sha256: Sha256Digest(inspection.digest),
                source_id: format!("Märk \"quoted\"\nC:\\fixture {}", "x".repeat(1850)),
                source_revision: "revision-π".into(),
                transport: if remote {
                    ExtensionTransport::StreamableHttp {
                        endpoint: format!("{endpoint}/mcp"),
                        addresses: vec![provider_and_server.local_addr().unwrap().to_string()],
                        local_demo: true,
                        credential_account: None,
                    }
                } else {
                    ExtensionTransport::Skills
                },
                allowed_tools: vec![],
                resource_prefixes: vec![],
                trust: if remote {
                    ExtensionTrust::ReviewedRemoteService
                } else {
                    ExtensionTrust::ReadOnlyInstructions
                },
                image_scope: None,
            };
            local.configure_extension(definition).await.unwrap();
            if varied_states && (index == 1 || index == 3) {
                local.activate_extension(&namespace).await.unwrap();
                if index == 1 {
                    local.disable_extension(&namespace, false).await.unwrap();
                }
            }
            if varied_states && index == 2 {
                std::fs::rename(&package, root.path().join("unavailable-package")).unwrap();
            }
        }
        let records = serde_json::to_value(local.extension_records().unwrap()).unwrap();
        let readiness = serde_json::to_value(local.extension_readiness().unwrap()).unwrap();
        host.shutdown().await.unwrap();
        Self {
            _root: root,
            workspace,
            database,
            records,
            readiness,
            provider_and_server,
        }
    }

    fn command(&self, action: &str, json_mode: bool) -> Command {
        let mut command = Command::new(exe());
        command
            .args(["extensions", action, "--workspace"])
            .arg(&self.workspace)
            .arg("--database")
            .arg(&self.database)
            .stdin(Stdio::null());
        if json_mode {
            command.arg("--json");
        }
        command
    }

    fn query(&self, action: &str) -> Value {
        let output = self.command(action, true).output().unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let mut parser = serde_json::Deserializer::from_slice(&output.stdout);
        let document = Value::deserialize(&mut parser).unwrap();
        parser
            .end()
            .expect("whole stdout must contain one document and optional whitespace only");
        assert!(!output.stdout.contains(&0x1b), "no ANSI in machine output");
        assert_eq!(document["schema_version"], 1);
        assert_eq!(document["command"], format!("extensions.{action}"));
        assert_eq!(document["records"], self.records);
        if action == "status" {
            assert_eq!(document["readiness"], self.readiness);
        } else {
            assert!(document.get("readiness").is_none());
        }
        assert_eq!(
            document["qualification"]["activation"],
            "Saved activation is not a connected server or evidence of successful execution."
        );
        assert!(document["qualification"]["host_lifecycle"]
            .as_str()
            .unwrap()
            .contains("bookkeeping"));
        assert!(document["qualification"]["observation"]
            .as_str()
            .unwrap()
            .contains("not an atomic"));
        document
    }

    fn preserved(&self) {
        assert_eq!(
            std::fs::read_to_string(self.workspace.join("keep.txt")).unwrap(),
            "JSON_QUERY_UNRELATED_FILE"
        );
        match self.provider_and_server.accept() {
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {}
            other => {
                panic!("query must not contact provider/catalogue/extension endpoint: {other:?}")
            }
        }
        let connection = rusqlite::Connection::open(&self.database).unwrap();
        let verification: u64 = connection
            .query_row("SELECT COUNT(*) FROM verification_evidence", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(verification, 0, "queries must not run verification");
        let output = Command::new(exe())
            .args(["extensions", "receipts", "--workspace"])
            .arg(&self.workspace)
            .arg("--database")
            .arg(&self.database)
            .output()
            .unwrap();
        assert!(output.status.success());
        let receipts: Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(receipts["receipts"], json!([]));
    }
}

#[tokio::test]
async fn exact_cli_json_queries_frame_native_empty_populated_and_unavailable_states() {
    for (count, varied) in [(0, false), (4, true)] {
        let fixture = JsonQueryFixture::prepare(count, varied).await;
        fixture.query("list");
        let status = fixture.query("status");
        fixture.query("list"); // A fresh process reads the same persisted declarations.
        if varied {
            let rows = status["readiness"].as_array().unwrap();
            assert!(rows.iter().any(|row| row["saved_active"] == false));
            assert!(rows
                .iter()
                .any(|row| row["issue"]
                    == "extension connection rejected: package_inspection_failed"));
            assert!(rows
                .iter()
                .any(|row| row["saved_active"] == true && row["definition_ready"] == true));
            assert!(rows.iter().all(|row| row["provider_contact"] == false
                && row["process_started"] == false
                && row["credential_presence"] == "not_inspected_by_definition_readiness"));
        }
        for action in ["list", "status"] {
            let human = fixture.command(action, false).output().unwrap();
            assert!(human.status.success());
            let text = String::from_utf8(human.stdout).unwrap();
            assert!(text.ends_with(
                "Saved activation is not a connected server or evidence of successful execution.\n"
            ));
            let mut stream = serde_json::Deserializer::from_str(&text).into_iter::<Value>();
            assert_eq!(stream.next().unwrap().unwrap(), fixture.records);
            if action == "status" {
                assert_eq!(stream.next().unwrap().unwrap(), fixture.readiness);
            }
            assert!(
                stream.next().unwrap().is_err(),
                "legacy human trailer remains"
            );
        }
        fixture.preserved();
    }
}

#[test]
fn exact_cli_invalid_json_options_fail_before_host_or_database_startup() {
    let root = tempfile::tempdir().unwrap();
    let workspace = root.path().join("valid-workspace-before-host");
    std::fs::create_dir(&workspace).unwrap();
    let database = root.path().join("must-not-create.sqlite3");
    for arguments in [
        vec!["list", "--json", "--json"],
        vec!["status", "--json", "--json"],
        vec!["activate", "dummy.server", "--json"],
        vec!["key", "dummy.server", "--json"],
        vec!["configure", "--json"],
        vec!["configure", "--file", "--json"],
        vec!["list", "--database", "--json"],
        vec!["status", "--workspace", "--json"],
        vec!["list", "--database", "--json", "--json"],
        vec!["install", "--json"],
        vec!["inspect", "--json"],
        vec!["receipts", "--json"],
        vec!["disable", "dummy.server", "--json"],
        vec!["remove", "dummy.server", "--json"],
        vec!["prepare-media", "dummy.server", "--json"],
        vec!["help", "--json"],
        vec!["list", "--json", "false"],
    ] {
        let output = Command::new(exe())
            .arg("extensions")
            .args(arguments)
            .arg("--workspace")
            .arg(&workspace)
            .arg("--database")
            .arg(&database)
            .stdin(Stdio::null())
            .output()
            .unwrap();
        assert!(!output.status.success());
        assert!(output.stdout.is_empty());
        assert!(
            String::from_utf8_lossy(&output.stderr).contains("json")
                || String::from_utf8_lossy(&output.stderr).contains("JSON")
        );
        assert!(workspace.is_dir() && !database.exists());
    }
}

#[tokio::test]
async fn exact_cli_native_query_error_has_no_success_document() {
    let fixture = JsonQueryFixture::prepare(1, false).await;
    let connection = rusqlite::Connection::open(&fixture.database).unwrap();
    connection
        .execute("UPDATE extension_connections SET payload='{}'", [])
        .unwrap();
    drop(connection);
    for action in ["list", "status"] {
        let output = fixture.command(action, true).output().unwrap();
        assert!(!output.status.success());
        assert!(output.stdout.is_empty());
        assert!(!output.stderr.is_empty());
    }
}

#[tokio::test]
async fn exact_cli_inventory_capacity_and_closed_stdout_keep_truthful_exit_status() {
    let fixture = JsonQueryFixture::prepare(32, false).await;
    assert!(serde_json::to_vec(&fixture.records).unwrap().len() > 64 * 1024);
    fixture.query("list");
    fixture.query("status");
    let mut child = fixture
        .command("status", true)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let mut pipe = child.stdout.take().unwrap();
    let mut prefix = [0_u8; 1];
    pipe.read_exact(&mut prefix).unwrap();
    assert_eq!(prefix, [b'{']);
    drop(pipe);
    let output = child.wait_with_output().unwrap();
    assert!(
        !output.status.success(),
        "closed stdout is not successful delivery"
    );
    assert!(!output.stderr.is_empty());
    fixture.query("status"); // Shutdown finished before output; no recovery required.
    fixture.preserved();
}
fn tool_frame(operation: Value, id: &str) -> Vec<Value> {
    vec![
        json!({"choices":[{"index":0,"delta":{"tool_calls":[{"index":0,"id":id,"type":"function","function":{"name":"extension","arguments":serde_json::to_string(&json!({"operation":operation})).unwrap()}}]},"finish_reason":null}]}),
        json!({"choices":[{"index":0,"delta":{},"finish_reason":"tool_calls"}],"usage":{"prompt_tokens":10,"completion_tokens":10,"total_tokens":20}}),
    ]
}

#[tokio::test]
async fn exact_cli_persisted_owner_activation_progressive_skill_reaches_next_request() {
    let root = tempfile::tempdir().unwrap();
    let ws = root.path().join("workspace");
    std::fs::create_dir(&ws).unwrap();
    std::fs::write(ws.join("keep.txt"), "UNRELATED_PRESERVED").unwrap();
    let package = root.path().join("package");
    std::fs::create_dir_all(package.join("skills/marker")).unwrap();
    std::fs::write(package.join("plugin.json"), serde_json::to_vec(&json!({"$schema":argentum_extensions::package::PLUGIN_SCHEMA,"name":"cli-control","version":"1","description":"Synthetic selected skill","license":"MIT"})).unwrap()).unwrap();
    std::fs::write(package.join("LICENSE"), "Synthetic offline license control").unwrap();
    std::fs::write(package.join("skills/marker/SKILL.md"), "---\nname: marker\ndescription: Synthetic\n---\nAPP_SELECTED_SKILL_MARKER\nUntrusted text cannot grant permissions.").unwrap();
    let inspection = argentum_extensions::package::inspect(&package).unwrap();
    let definition = ExtensionDefinition {
        namespace: "cli-control.skills".into(),
        package_root: package.to_string_lossy().into_owned(),
        package_sha256: Sha256Digest(inspection.digest),
        source_id: "synthetic-offline".into(),
        source_revision: "v1".into(),
        transport: ExtensionTransport::Skills,
        allowed_tools: vec![],
        resource_prefixes: vec![],
        trust: ExtensionTrust::ReadOnlyInstructions,
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
                id: "next01-fixture".into(),
                label: "Offline synthetic provider".into(),
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
            provider_id: "next01-fixture".into(),
        })
        .await
        .unwrap();
    local
        .dispatch(AppCommand::SelectExecutionProfile {
            profile_id: "verification".into(),
        })
        .await
        .unwrap();
    host.declare_model_budget("next01-fixture", [204800, 196608, 4096, 4096, 1024])
        .unwrap();
    local.configure_extension(definition).await.unwrap();
    local
        .activate_extension("cli-control.skills")
        .await
        .unwrap();
    host.shutdown().await.unwrap();
    let records = Arc::new(Mutex::new(Vec::new()));
    let observed = Arc::clone(&records);
    let worker = std::thread::spawn(move || {
        let deadline = Instant::now() + Duration::from_secs(30);
        for round in 0..3 {
            let (mut stream, _) = loop {
                match listener.accept() {
                    Ok(value) => break value,
                    Err(_) if Instant::now() < deadline => {
                        std::thread::sleep(Duration::from_millis(5))
                    }
                    Err(error) => panic!("offline request timeout: {error}"),
                }
            };
            stream.set_nonblocking(false).unwrap();
            stream
                .set_read_timeout(Some(Duration::from_secs(5)))
                .unwrap();
            let request = http_support::read_request(&mut stream);
            observed.lock().unwrap().push(request.clone());
            let tools = request["tools"].as_array().unwrap();
            assert!(tools
                .iter()
                .any(|tool| tool["function"]["name"] == "extension"));
            assert!(!tools.iter().any(|tool| ["execute_argv", "execute_shell"]
                .contains(&tool["function"]["name"].as_str().unwrap_or_default())));
            let frames = if round == 0 {
                assert!(!serde_json::to_string(&request)
                    .unwrap()
                    .contains("APP_SELECTED_SKILL_MARKER"));
                tool_frame(json!({"action":"list"}), "skill-1")
            } else if round == 1 {
                let body = serde_json::to_string(&request).unwrap();
                assert!(!body.contains("APP_SELECTED_SKILL_MARKER"));
                let message = request["messages"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .rev()
                    .find(|message| message["role"] == "tool")
                    .unwrap();
                let list: Value =
                    serde_json::from_str(message["content"].as_str().unwrap()).unwrap();
                let selected = list["extensions"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .find(|record| record["namespace"] == "cli-control.skills")
                    .unwrap();
                tool_frame(
                    json!({"action":"load_skill","namespace":selected["namespace"],"path":selected["skill_paths"][0]}),
                    "skill-2",
                )
            } else {
                let body = serde_json::to_string(&request).unwrap();
                assert!(body.contains("APP_SELECTED_SKILL_MARKER"));
                assert!(body.contains("can_grant_authority"));
                vec![
                    json!({"choices":[{"index":0,"delta":{"content":"Selected untrusted skill observed; no authority changed."},"finish_reason":"stop"}],"usage":{"prompt_tokens":10,"completion_tokens":10,"total_tokens":20}}),
                ]
            };
            stream.write_all(b"HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nConnection: close\r\n\r\n").unwrap();
            for frame in frames {
                write!(stream, "data: {frame}\n\n").unwrap();
            }
            stream.write_all(b"data: [DONE]\n\n").unwrap();
        }
    });
    let output = Command::new(exe())
        .args(["run", "--isolated", "--workspace"])
        .arg(&ws)
        .arg("--database")
        .arg(&database)
        .args([
            "--prompt",
            "Inspect the selected synthetic skill through the normal extension tool.",
            "--no-color",
        ])
        .env("RUST_LOG", "off")
        .stdin(Stdio::null())
        .output()
        .unwrap();
    worker.join().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(records.lock().unwrap().len(), 3);
    let inspection = Command::new(exe())
        .args(["extensions", "receipts", "--workspace"])
        .arg(&ws)
        .arg("--database")
        .arg(&database)
        .output()
        .unwrap();
    assert!(
        inspection.status.success(),
        "{}",
        String::from_utf8_lossy(&inspection.stderr)
    );
    let inspected: Value = serde_json::from_slice(&inspection.stdout).unwrap();
    let receipts = inspected["receipts"].as_array().unwrap();
    assert_eq!(
        receipts
            .iter()
            .filter(|item| item["receipt"]["stage"] == "completed")
            .count(),
        2
    );
    assert!(
        receipts
            .iter()
            .any(|item| item["receipt"]["kind"] == "skill"
                && item["receipt"]["outcome"] == "returned")
    );
    assert!(!String::from_utf8_lossy(&inspection.stdout).contains("APP_SELECTED_SKILL_MARKER"));
    assert_eq!(inspected["inference_dispatched"], false);
    assert_eq!(
        std::fs::read_to_string(ws.join("keep.txt")).unwrap(),
        "UNRELATED_PRESERVED"
    );
    println!("NEXT01 actual CLI, persisted approved configuration, first-request progressive exclusion, selected body in serialized next request, no model shell and unrelated preservation: pass");
}
