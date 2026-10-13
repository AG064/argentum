#![cfg(all(windows, feature = "desktop-control"))]
//! Credential-free synthetic responses drive the actual CLI and real owned UIA.
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
#[ignore = "requires explicitly reviewed native adapter and owned synthetic app on interactive Windows"]
async fn actual_cli_native_observation_save_verification_feedback_and_teardown() {
    let adapter =
        PathBuf::from(std::env::var_os("ARGENTUM_NEXT02_ADAPTER").expect("reviewed adapter"));
    let app = PathBuf::from(
        std::env::var_os("ARGENTUM_NEXT02_SYNTHETIC_APP").expect("owned synthetic app"),
    );
    let root = tempfile::tempdir().unwrap();
    let ws = root.path().join("workspace");
    std::fs::create_dir(&ws).unwrap();
    std::fs::write(ws.join("input.txt"), "Native control\nOwner: sample\n").unwrap();
    std::fs::write(ws.join("keep.txt"), "UNCHANGED_NATIVE_CLI").unwrap();
    std::fs::create_dir(ws.join("src")).unwrap();
    std::fs::write(
        ws.join("Cargo.toml"),
        "[package]\nname=\"native_saved_control\"\nversion=\"0.1.0\"\nedition=\"2021\"\n",
    )
    .unwrap();
    std::fs::write(
        ws.join("Cargo.lock"),
        "version=4\n\n[[package]]\nname=\"native_saved_control\"\nversion=\"0.1.0\"\n",
    )
    .unwrap();
    let verifier_source = "#[test] fn saved_native_artifact() { let saved=std::fs::read_to_string(\"output.txt\").unwrap(); assert_eq!(saved.replace(\"\\r\\n\",\"\\n\"),\"Native control\\nOwner: reviewed\\n\"); assert_eq!(std::fs::read_to_string(\"input.txt\").unwrap(),\"Native control\\nOwner: sample\\n\"); }\n";
    std::fs::write(ws.join("src/lib.rs"), verifier_source).unwrap();
    std::fs::create_dir(root.path().join("images")).unwrap();
    let digest = |path: &std::path::Path| {
        Sha256Digest(
            argentum_execution::VerifiedFileLease::open(path, 256 * 1024 * 1024)
                .unwrap()
                .sha256()
                .into(),
        )
    };
    let campaign = argentum_domain::GoalId::new_v4().to_string();
    let app_sha = digest(&app);
    let config = root.path().join("adapter.json");
    let ledger = root.path().join("actions.bin");
    use sha2::Digest;
    let identity = sha2::Sha256::digest(
        format!(
            "NEXT02/actions/v1\n{campaign}\n{}\n{}",
            app_sha.0,
            ws.display()
        )
        .as_bytes(),
    );
    let mut ledger_bytes = identity.to_vec();
    ledger_bytes.extend([0u8; 8]);
    std::fs::write(&ledger, ledger_bytes).unwrap();
    std::fs::write(&config,serde_json::to_vec(&json!({"program":app,"sha256":app_sha,"working_directory":ws,"artifact_directory":root.path().join("images"),"action_ledger":ledger,"campaign_id":campaign,"arguments":[],"documents":["input.txt","output.txt"],"save_destinations":["output.txt"],"dialogs":["Save As"],"actions":["select_all","replace_selection","invoke","set_value"],"invokable_names":["Save As","Save"],"text_control_classes":["WindowsForms10.EDIT.app.0.34f5582_r6_ad1"],"targeted_text_input":true,"maximum_actions":128,"lifetime_seconds":180})).unwrap()).unwrap();
    let args = vec!["--config".to_owned(), config.display().to_string()];
    let package = root.path().join("package");
    std::fs::create_dir(&package).unwrap();
    std::fs::write(package.join("plugin.json"),serde_json::to_vec(&json!({"$schema":argentum_extensions::package::PLUGIN_SCHEMA,"name":"native-control","version":"1","description":"Owned native offline control","license":"LicenseRef-Argentum"})).unwrap()).unwrap();
    std::fs::copy(
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../LICENSE"),
        package.join("LICENSE"),
    )
    .unwrap();
    std::fs::write(package.join("mcp.json"),serde_json::to_vec(&json!({"$schema":argentum_extensions::package::MCP_SCHEMA,"mcpServers":{"stdio":{"type":"stdio","command":adapter,"args":args}}})).unwrap()).unwrap();
    let inspection = argentum_extensions::package::inspect(&package).unwrap();
    let definition = ExtensionDefinition {
        namespace: "native-control.stdio".into(),
        package_root: package.display().to_string(),
        package_sha256: Sha256Digest(inspection.digest),
        source_id: "first-party-owned-native-control".into(),
        source_revision: "v1".into(),
        transport: ExtensionTransport::Stdio {
            program: adapter.display().to_string(),
            program_sha256: digest(&adapter),
            args,
            cwd: ws.display().to_string(),
            environment: BTreeMap::new(),
            inputs: BTreeMap::from([
                (app.display().to_string(), digest(&app)),
                (config.display().to_string(), digest(&config)),
            ]),
        },
        allowed_tools: [
            "desktop_launch",
            "desktop_observe",
            "desktop_action",
            "desktop_close",
        ]
        .into_iter()
        .map(str::to_owned)
        .collect(),
        resource_prefixes: vec![],
        trust: ExtensionTrust::ReviewedWindowsApplication,
        image_scope: None,
    };
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let endpoint = format!("http://{}/v1/", listener.local_addr().unwrap());
    let db = root.path().join("store.sqlite3");
    let mut hc = HostConfig::in_memory(&ws);
    hc.database = Some(db.clone());
    let mut host = CommandHost::start_composed(hc).await.unwrap();
    let local = host.local_handle();
    local
        .dispatch(AppCommand::SaveProviderProfile {
            profile: ProviderProfile {
                id: "native-fixture".into(),
                label: "Offline native fixture".into(),
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
            provider_id: "native-fixture".into(),
        })
        .await
        .unwrap();
    local
        .dispatch(AppCommand::SelectExecutionProfile {
            profile_id: "verification".into(),
        })
        .await
        .unwrap();
    for capability_id in host
        .tool_descriptors()
        .into_iter()
        .map(|tool| tool.capability_id)
        .collect::<std::collections::BTreeSet<_>>()
    {
        if !matches!(
            capability_id.as_str(),
            "tool.read-text" | "verification.runner" | "extension.external"
        ) {
            local
                .dispatch(AppCommand::SetHarnessCapabilityEnabled {
                    capability_id,
                    enabled: false,
                })
                .await
                .unwrap();
        }
    }
    host.declare_model_budget("native-fixture", [204800, 196608, 4096, 4096, 1024])
        .unwrap();
    local.configure_extension(definition).await.unwrap();
    local
        .activate_extension("native-control.stdio")
        .await
        .unwrap();
    host.shutdown().await.unwrap();
    let saved_destination = ws.join("output.txt").display().to_string();
    let controller = std::thread::spawn(move || {
        let mut handles = BTreeMap::new();
        let mut state = Value::Null;
        let deadline = Instant::now() + Duration::from_secs(180);
        let mut round = 0;
        for request_index in 0..32 {
            let mut advance = true;
            let (mut stream, _) = loop {
                match listener.accept() {
                    Ok(x) => break x,
                    Err(_) if Instant::now() < deadline => {
                        std::thread::sleep(Duration::from_millis(5))
                    }
                    Err(e) => panic!("synthetic dispatch unavailable: {e}"),
                }
            };
            stream.set_nonblocking(false).unwrap();
            stream
                .set_read_timeout(Some(Duration::from_secs(10)))
                .unwrap();
            let request = http_support::read_request(&mut stream);
            assert!(!request["tools"].as_array().unwrap().iter().any(|tool| {
                matches!(
                    tool["function"]["name"].as_str(),
                    Some(
                        "write_text"
                            | "edit_text"
                            | "apply_patch"
                            | "execute_argv"
                            | "execute_shell"
                    )
                )
            }));
            let previous = if round == 0 {
                Value::Null
            } else {
                let last = request["messages"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .rev()
                    .find(|m| m["role"] == "tool")
                    .unwrap();
                serde_json::from_str::<Value>(last["content"].as_str().unwrap()).unwrap()
            };
            if round == 1 {
                for tool in previous["tools"]
                    .as_array()
                    .unwrap_or_else(|| panic!("native catalogue feedback: {previous}"))
                {
                    handles.insert(
                        tool["handle"]["name"].as_str().unwrap().to_owned(),
                        tool["handle"].clone(),
                    );
                }
            }
            if round == 2 {
                assert!(
                    previous["structuredContent"]["context_id"].is_string(),
                    "native launch feedback: {previous}"
                );
            }
            if matches!(round, 3 | 5 | 7 | 9 | 11 | 13) {
                state = previous["structuredContent"].clone();
                assert!(
                    state["elements"].is_array(),
                    "native observation feedback at round {round}: {previous}"
                );
            }
            if round == 7 {
                assert!(
                    state["elements"].as_array().unwrap().iter().any(|element| {
                        element["text"].as_str().is_some_and(|text| {
                            text.replace("\r\n", "\n") == "Native control\nOwner: reviewed\n"
                        })
                    }),
                    "actual UIA post-action readback: {state}"
                );
            }
            if round == 15 {
                assert!(
                    previous["structuredContent"]["closed"] == true,
                    "owned teardown feedback: {previous}"
                );
            }
            if round == 14 {
                assert_eq!(
                    previous[0]["state"], "passed",
                    "actual saved-artifact fixed verification: {previous}"
                );
                assert_eq!(previous[0]["exit_code"], 0);
            }
            let invoke = |name: &str, args: Value| json!({"action":"invoke","namespace":"native-control.stdio","handle":handles.get(name).expect("scoped handle"),"arguments":args});
            let op = match round {
                0 => Some(json!({"action":"discover_tools","namespace":"native-control.stdio"})),
                1 => Some(invoke("desktop_launch", json!({"document":"input.txt"}))),
                2 | 4 | 6 | 8 | 10 | 12 => {
                    Some(invoke("desktop_observe", json!({"text_limit":256})))
                }
                3 | 5 => {
                    let target = state["elements"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .find(|e| {
                            e["patterns"].as_array().is_some_and(|patterns| {
                                patterns.iter().any(|pattern| {
                                    pattern
                                        .as_str()
                                        .is_some_and(|name| name.contains("TextPattern"))
                                })
                            })
                        })
                        .expect("actual UIA document");
                    let mut args = json!({"generation":state["generation"],"reference":target["reference"],"action":if round==3 {"select_all"}else{"replace_selection"}});
                    if round == 5 {
                        args["value"] = json!("Native control\nOwner: reviewed\n");
                    }
                    Some(invoke("desktop_action", args))
                }
                7 | 11 => {
                    let name = if round == 7 { "Save As" } else { "Save" };
                    let target = state["elements"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .find(|element| {
                            element["name"] == name
                                && element["patterns"].as_array().is_some_and(|patterns| {
                                    patterns.iter().any(|pattern| {
                                        pattern
                                            .as_str()
                                            .is_some_and(|name| name.contains("InvokePattern"))
                                    })
                                })
                        });
                    if let Some(target) = target {
                        Some(invoke(
                            "desktop_action",
                            json!({"generation":state["generation"],"reference":target["reference"],"action":"invoke"}),
                        ))
                    } else {
                        let offset = state["next_offset"].as_u64().unwrap_or_else(|| {
                            panic!("observed native save control missing: {state}")
                        });
                        advance = false;
                        Some(invoke(
                            "desktop_observe",
                            json!({"offset":offset,"text_limit":256}),
                        ))
                    }
                }
                9 => {
                    let target = state["elements"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .find(|element| {
                            element["name"]
                                .as_str()
                                .is_some_and(|name| name.starts_with("File name"))
                                && element["role"] == "ControlType.Edit"
                                && element["patterns"].as_array().is_some_and(|patterns| {
                                    patterns.iter().any(|pattern| {
                                        pattern
                                            .as_str()
                                            .is_some_and(|name| name.contains("ValuePattern"))
                                    })
                                })
                        });
                    if let Some(target) = target {
                        Some(invoke(
                            "desktop_action",
                            json!({"generation":state["generation"],"reference":target["reference"],"action":"set_value","value":saved_destination}),
                        ))
                    } else {
                        let offset = state["next_offset"].as_u64().unwrap_or_else(|| {
                            panic!("observed native filename control missing: {state}")
                        });
                        advance = false;
                        Some(invoke(
                            "desktop_observe",
                            json!({"offset":offset,"text_limit":256}),
                        ))
                    }
                }
                13 => None,
                14 => Some(invoke("desktop_close", json!({}))),
                _ => None,
            };
            stream.write_all(b"HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nConnection: close\r\n\r\n").unwrap();
            let delta = if round == 13 {
                json!({"tool_calls":[{"index":0,"id":format!("native-{request_index}"),"type":"function","function":{"name":"verify","arguments":json!({"runners":["cargo-test-locked"]}).to_string()}}]})
            } else if let Some(op) = op {
                json!({"tool_calls":[{"index":0,"id":format!("native-{request_index}"),"type":"function","function":{"name":"extension","arguments":serde_json::to_string(&json!({"operation":op})).unwrap()}}]})
            } else {
                json!({"content":"The GUI-saved artifact passed current fixed verification. Input and protected files were preserved and the owned application was closed."})
            };
            let frame = json!({"choices":[{"index":0,"delta":delta,"finish_reason":null}]});
            write!(stream, "data: {frame}\n\n").unwrap();
            let terminal = json!({"choices":[{"index":0,"delta":{},"finish_reason":if round<15 {"tool_calls"}else{"stop"}}],"usage":{"prompt_tokens":10,"completion_tokens":10,"total_tokens":20}});
            write!(stream, "data: {terminal}\n\ndata: [DONE]\n\n").unwrap();
            if round == 15 {
                return;
            }
            if advance {
                round += 1;
            }
        }
        panic!("bounded synthetic native sequence did not settle");
    });
    let exe = std::env::var_os("ARGENTUM_NEXT02_CLI_EXE")
        .map(PathBuf::from)
        .unwrap_or_else(|| env!("CARGO_BIN_EXE_argentum").into());
    let output=Command::new(&exe).args(["run","--isolated","--workspace"]).arg(&ws).arg("--database").arg(&db).args(["--prompt","Use the admitted owned native control, save output.txt through its UI, verify the saved artifact with the fixed test runner, then close the owned app.","--no-color"]).stdin(Stdio::null()).output().unwrap();
    controller.join().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        std::fs::read_to_string(ws.join("keep.txt")).unwrap(),
        "UNCHANGED_NATIVE_CLI"
    );
    assert_eq!(
        std::fs::read_to_string(ws.join("input.txt")).unwrap(),
        "Native control\nOwner: sample\n"
    );
    assert_eq!(
        std::fs::read_to_string(ws.join("output.txt"))
            .unwrap()
            .replace("\r\n", "\n"),
        "Native control\nOwner: reviewed\n"
    );
    assert_eq!(
        std::fs::read_to_string(ws.join("src/lib.rs")).unwrap(),
        verifier_source
    );
    let receipt = Command::new(exe)
        .args(["extensions", "receipts", "--workspace"])
        .arg(&ws)
        .arg("--database")
        .arg(&db)
        .output()
        .unwrap();
    assert!(receipt.status.success());
    let receipt: Value = serde_json::from_slice(&receipt.stdout).unwrap();
    assert!(receipt["receipts"]
        .as_array()
        .unwrap()
        .iter()
        .any(|r| r["receipt"]["desktop_report"]["primitive_actions"]
            .as_u64()
            .unwrap_or(0)
            > 20));
    assert!(!receipt.to_string().contains("Owner: reviewed"));
}
