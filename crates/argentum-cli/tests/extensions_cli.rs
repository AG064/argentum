#![cfg(all(windows, feature = "extensions"))]
use argentum_cli::{CommandHost, HostConfig};
use argentum_domain::{
    extensions::{ExtensionDefinition, ExtensionTransport, ExtensionTrust},
    AppCommand, ProviderKind, ProviderProfile, Sha256Digest,
};
use serde_json::{json, Value};
use std::{
    io::Write,
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
        for round in 0..2 {
            let (mut stream, _) = loop {
                match listener.accept() {
                    Ok(value) => break value,
                    Err(_) if Instant::now() < deadline => {
                        std::thread::sleep(Duration::from_millis(5))
                    }
                    Err(error) => panic!("offline request timeout: {error}"),
                }
            };
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
                tool_frame(
                    json!({"action":"load_skill","namespace":"cli-control.skills","path":"skills/marker/SKILL.md"}),
                    "skill-1",
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
    assert_eq!(records.lock().unwrap().len(), 2);
    assert_eq!(
        std::fs::read_to_string(ws.join("keep.txt")).unwrap(),
        "UNRELATED_PRESERVED"
    );
    println!("NEXT01 actual CLI, persisted approved configuration, first-request progressive exclusion, selected body in serialized next request, no model shell and unrelated preservation: pass");
}
