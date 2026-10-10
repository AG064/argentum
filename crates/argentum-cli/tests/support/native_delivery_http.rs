//! Single native invocation controls, using the real CLI and streamed adapter.
use super::instructions_http::Server;
use super::*;

fn json_output(output: Output) -> Value {
    assert!(
        output.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).expect("settled JSON")
}

fn next(server: &Server) -> Value {
    let request = server
        .requests
        .recv_timeout(Duration::from_secs(120))
        .expect("native request watchdog");
    let tools = request["tools"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|tool| tool["function"]["name"].as_str().unwrap())
        .collect::<Vec<_>>();
    assert!(!tools.contains(&"execute_argv") && !tools.contains(&"execute_shell"));
    request
}

fn next_coding_after_reported_handoff(server: &Server) -> Value {
    loop {
        let request = next(server);
        if !request["messages"]
            .to_string()
            .contains("[native_handoff_feedback]")
        {
            return request;
        }
        assert!(request["tools"]
            .as_array()
            .map_or(true, |tools| tools.is_empty()));
        assert!(
            request["messages"]
                .as_array()
                .unwrap()
                .iter()
                .any(|m| m["role"] == "tool"),
            "handoff must deliver actual pending results, not only a generic notice"
        );
        text(
            server,
            r#"{"status":"continue","remaining":["Continue the public work using the completed observations."]}"#,
            true,
        );
    }
}

fn text(server: &Server, value: &str, usage: bool) {
    let mut response = json!({"choices":[{"delta":{"content":value},"finish_reason":"stop"}]});
    if usage {
        response["usage"] = json!({"total_tokens":10});
    }
    server.replies.send(response).unwrap();
}

fn tool(server: &Server, id: &str, name: &str, args: Value) {
    server.replies.send(json!({"choices":[{"delta":{"tool_calls":[{"index":0,"id":id,"type":"function","function":{"name":name,"arguments":args.to_string()}}]},"finish_reason":"tool_calls"}],"usage":{"total_tokens":10}})).unwrap();
}

fn prepare(fixture: &CliFixture, iterations: u32) -> String {
    fixture.allow_usability_tools_via_owner();
    json_output(fixture.run(&[
        "provider",
        "budget",
        "f01-fixture",
        "--limits",
        "204800,196608,4096,4096,1024",
        "--json",
    ]));
    std::fs::create_dir(fixture.workspace.join("src")).unwrap();
    std::fs::write(
        fixture.workspace.join("Cargo.toml"),
        "[package]\nname = \"native_delivery_control\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
    )
    .unwrap();
    std::fs::write(
        fixture.workspace.join("Cargo.lock"),
        "version = 4\n\n[[package]]\nname = \"native_delivery_control\"\nversion = \"0.1.0\"\n",
    )
    .unwrap();
    std::fs::write(fixture.workspace.join("src/lib.rs"), "pub fn value() -> u32 {\n    7\n}\n\n#[test]\nfn public_value() {\n    assert_eq!(value(), 8, \"MILESTONE_CURRENT_ASSERTION\");\n}\n").unwrap();
    let goal = json_output(fixture.run(&["goal", "set", "--objective", "Make the public value eight, preserving all other files. Complete the public requirement, not only a plan.", "--token-budget", "2000000", "--tool-budget", "32", "--time-budget", "600", "--json"]));
    let id = goal["goal"]["id"].as_str().unwrap().to_owned();
    json_output(fixture.run(&[
        "goal",
        "limits",
        "--goal-id",
        &id,
        "--limits",
        &format!("2000000,32,{iterations},32,600,1000000,32,16,300"),
        "--json",
    ]));
    json_output(fixture.run(&[
        "goal",
        "criteria",
        "--goal-id",
        &id,
        "--runners",
        "cargo-test-locked",
        "--json",
    ]));
    id
}

#[test]
fn milestone_native_handoff_delivers_redacted_search_feedback_and_rejects_new_effects() {
    use sha2::Digest;
    for forbidden_effect in [false, true] {
        let server = Server::start();
        let fixture = CliFixture::new(&server.endpoint);
        let id = prepare(&fixture, 8);
        tokio::runtime::Runtime::new().unwrap().block_on(async {
            let mut config = argentum_cli::HostConfig::in_memory(&fixture.workspace);
            config.database = Some(fixture.database.clone());
            let mut host = argentum_cli::CommandHost::start(config).unwrap();
            let capability = host
                .tool_descriptors()
                .into_iter()
                .find(|tool| tool.id == "search_text")
                .unwrap()
                .capability_id;
            host.local_handle()
                .dispatch(AppCommand::SetHarnessCapabilityEnabled {
                    capability_id: capability,
                    enabled: true,
                })
                .await
                .unwrap();
            host.shutdown().await.unwrap();
        });
        let path = fixture.workspace.join("src/lib.rs");
        let mut source = std::fs::read_to_string(&path).unwrap();
        source.push_str(
            "\n// api_key PUBLIC_EPHEMERAL_SEARCH is synthetic test text, not a credential.\n",
        );
        std::fs::write(&path, &source).unwrap();
        let digest = format!("{:x}", sha2::Sha256::digest(source.as_bytes()));
        json_output(fixture.run(&[
            "goal",
            "limits",
            "--goal-id",
            &id,
            "--limits",
            "2000000,32,8,32,600,1000000,4,16,300",
            "--json",
        ]));
        let output = thread::scope(|scope| {
            let child = scope.spawn(|| deliver(&fixture, &id, "8"));
            next(&server);
            tool(
                &server,
                "search-one",
                "search_text",
                json!({"path":"src/lib.rs","query":"api_key","max_results":2}),
            );
            next(&server);
            tool(
                &server,
                "search-two",
                "search_text",
                json!({"path":"src/lib.rs","query":"PUBLIC_EPHEMERAL_SEARCH","max_results":2}),
            );
            let report = next(&server);
            assert!(report["messages"]
                .to_string()
                .contains("[native_handoff_feedback]"));
            assert!(report["tools"]
                .as_array()
                .map_or(true, |tools| tools.is_empty()));
            assert!(report.get("tool_choice").is_none());
            assert!(tool_reply(&report,"search-two").to_string().contains("PUBLIC_EPHEMERAL_SEARCH"),"the actual fresh search body must reach the model before privacy-preserving handoff: {}",tool_reply(&report,"search-two"));
            if forbidden_effect {
                tool(
                    &server,
                    "forbidden-edit",
                    "edit_text",
                    json!({"path":"src/lib.rs","expected_sha256":digest,"old_text":"    7\n","new_text":"    8\n"}),
                );
            } else {
                text(
                    &server,
                    r#"{"status":"continue","remaining":["Read src/lib.rs and finish the public implementation."]}"#,
                    true,
                );
                let resumed = next(&server);
                assert!(!resumed["messages"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|m| m["role"] == "tool"
                        && m["content"]
                            .as_str()
                            .is_some_and(|text| text.contains("PUBLIC_EPHEMERAL_SEARCH"))));
                tool(
                    &server,
                    "fresh-read",
                    "read_text",
                    json!({"path":"src/lib.rs"}),
                );
                let current = next(&server);
                let read = tool_reply(&current, "fresh-read");
                tool(
                    &server,
                    "implement-after-feedback",
                    "edit_text",
                    json!({"path":"src/lib.rs","expected_sha256":read["sha256"],"old_text":"    7\n","new_text":"    8\n"}),
                );
                let ready = next(&server);
                assert!(ready["messages"]
                    .to_string()
                    .contains("[native_handoff_feedback]"));
                assert!(tool_reply(&ready, "implement-after-feedback")["sha256"].is_string());
                text(&server, r#"{"status":"ready","remaining":[]}"#, true);
                let verified = next(&server);
                assert!(verified["messages"]
                    .to_string()
                    .contains("native_verifier_feedback"));
                assert!(verified["messages"].to_string().contains("passed"));
                text(&server, r#"{"status":"ready","remaining":[]}"#, true);
            }
            child.join().unwrap()
        });
        let result = json_output(output);
        assert_eq!(result["task_success"], !forbidden_effect, "{result}");
        if forbidden_effect {
            assert_eq!(std::fs::read_to_string(&path).unwrap(), source);
        } else {
            assert!(std::fs::read_to_string(&path).unwrap().contains("    8\n"));
            assert_eq!(
                result["goal"]["accounting"]["counters"]["request_attempts"],
                7
            );
        }
        let db = rusqlite::Connection::open(&fixture.database).unwrap();
        let groups = db
            .prepare("SELECT payload FROM context_groups ORDER BY run_id,ordinal")
            .unwrap()
            .query_map([], |r| r.get::<_, String>(0))
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        assert!(
            groups
                .iter()
                .filter_map(|g| serde_json::from_str::<Value>(g).ok())
                .flat_map(|g| g["body"]["observations"]
                    .as_array()
                    .cloned()
                    .unwrap_or_default())
                .filter(|o| o["tool"] == "search_text")
                .all(|o| !o["result"].to_string().contains("PUBLIC_EPHEMERAL_SEARCH")),
            "the repair must not expand persistent search-body retention"
        );
        assert!(server.requests.try_recv().is_err());
    }
}

#[test]
fn milestone_delayed_deepseek_terminal_compaction_recovery_edit_verification() {
    use super::working_context_http::compaction_http::{context, summary_for};
    for mode in [
        "delayed_first",
        "useful_stream",
        "pretty_json",
        "oversized_terminal_usage",
        "wrong_reference_alias",
        "exhausted_parent",
    ] {
        println!("delayed-compaction-control stage=prepare mode={mode}");
        let server = Server::start();
        let fixture = CliFixture::new(&server.endpoint);
        let id = prepare(&fixture, 16);
        // Legitimate, tool-observed history, not synthetic context-store rows.
        std::fs::write(
            fixture.workspace.join("history.txt"),
            "observed public history\n".repeat(1200),
        )
        .unwrap();
        let large_reads = if mode == "pretty_json" { 3 } else { 2 };
        for n in 0..=large_reads {
            println!("delayed-compaction-control stage=seed-{n} mode={mode}");
            let output = thread::scope(|scope| {
                let child = scope.spawn(|| {
                    fixture.run(&[
                        "run",
                        "--prompt",
                        "Retain an actual read for a later native goal",
                        "--json",
                    ])
                });
                next(&server);
                tool(
                    &server,
                    &format!("retained-{n}"),
                    "read_text",
                    if n < large_reads {
                        json!({"path":"history.txt","start_byte":0,"end_byte":27600})
                    } else {
                        json!({"path":"fixture.txt"})
                    },
                );
                next(&server);
                text(
                    &server,
                    "Historical observation retained, no edit or current verification claim.",
                    true,
                );
                child.join().unwrap()
            });
            assert!(output.status.success());
        }
        // Bound only this disposable fixture. The real owner's limits do not change.
        json_output(fixture.run(&[
            "provider",
            "budget",
            "f01-fixture",
            "--limits",
            if mode == "pretty_json" {
                "204800,85000,4096,4096,1024"
            } else {
                "204800,70000,4096,4096,1024"
            },
            "--json",
        ]));
        let old = thread::scope(|scope| {
            println!("delayed-compaction-control stage=missing-summary mode={mode}");
            let child = scope.spawn(|| fixture.run(&["compact", "--goal", &id, "--json"]));
            let request = server
                .requests
                .recv_timeout(Duration::from_secs(30))
                .unwrap();
            assert!(request.get("tools").is_none());
            text(&server, "Unusable unaccounted summary", false);
            child.join().unwrap()
        });
        assert!(!old.status.success());
        let status = json_output(fixture.run(&["goal", "status", "--json"]));
        let session = status["goal"]["session_id"].as_str().unwrap();
        let charge = status["execution"]["requests"]
            .as_array()
            .unwrap()
            .iter()
            .find(|q| q["state"] == "unresolved")
            .unwrap();
        let request = charge["request_id"].as_str().unwrap();
        let inspection = json_output(fixture.run(&[
            "accounting",
            "inspect",
            "--session-id",
            session,
            "--goal-id",
            &id,
            "--request-id",
            request,
            "--json",
        ]));
        let amount = inspection["binding"]["reserved_tokens"]
            .as_u64()
            .unwrap()
            .to_string();
        let file = fixture.root.path().join("exact-summary-resolution.json");
        std::fs::write(&file,json!({"command_id":"one-known-summary-resolution","binding":inspection["binding"],"decision":{"kind":"owner_accept_reservation"}}).to_string()).unwrap();
        json_output(fixture.run(&[
            "accounting",
            "resolve",
            "--resolution",
            file.to_str().unwrap(),
            "--confirm-reservation",
            &amount,
            "--json",
        ]));
        assert!(context(&fixture)["summary_checkpoint_id"].is_null());
        if mode == "exhausted_parent" {
            let output = fixture.run(&[
                "goal",
                "deliver",
                "--goal-id",
                &id,
                "--iterations",
                "8",
                "--elapsed-seconds",
                "1",
                "--json",
            ]);
            let result: Value = serde_json::from_slice(&output.stdout).unwrap();
            assert_eq!(result["task_success"], false);
            let account = json_output(fixture.run(&["goal", "status", "--json"]));
            assert_eq!(
                account["execution"]["requests"].as_array().unwrap().len(),
                1,
                "insufficient drainage capacity must reject before a second physical request"
            );
            assert!(context(&fixture)["summary_checkpoint_id"].is_null());
            assert!(server.requests.try_recv().is_err());
            continue;
        }
        let ready = r#"{"status":"ready","remaining":[],"summary":"Current public requirement implemented; check it."}"#;
        let output = thread::scope(|scope| {
            println!("delayed-compaction-control stage=resume mode={mode}");
            let child = scope.spawn(|| deliver(&fixture, &id, "8"));
            let request = server
                .requests
                .recv_timeout(Duration::from_secs(30))
                .unwrap();
            assert!(
                request.get("tools").is_none(),
                "resume must really compact mandatory retained history"
            );
            assert_eq!(request["stream_options"]["include_usage"], true);
            let mut narrative = summary_for(&request);
            if mode == "wrong_reference_alias" {
                let first = narrative["source_references"][0]
                    .as_str()
                    .unwrap()
                    .to_owned();
                narrative["source_references"][0] = json!(format!("{first}_foreign"));
            }
            let summary = if mode == "pretty_json" {
                if let Some(mapping) = serde_json::from_str::<Value>(
                    request["messages"][1]["content"].as_str().unwrap(),
                )
                .unwrap()["source_reference_map"]
                    .as_array()
                {
                    narrative["source_references"] = json!(mapping
                        .iter()
                        .map(|entry| if entry.is_array() {
                            entry[0].clone()
                        } else {
                            entry["canonical_source"].clone()
                        })
                        .collect::<Vec<_>>());
                }
                narrative["objective"]=json!("Retained observations including \"quoted\" text and literal \\n remain historical.");
                for padding in 0..100 {
                    let row = format!(
                        "Historical observations are not current source or authorization.{}",
                        " ".repeat(padding)
                    );
                    for field in [
                        "decisions_constraints",
                        "observed_changes_verification",
                        "unresolved_work",
                    ] {
                        narrative[field] = json!(vec![row.clone(); 24]);
                    }
                    if narrative.to_string().len() >= 7900 {
                        break;
                    }
                }
                assert!(narrative.to_string().len() <= 8192);
                let pretty = serde_json::to_string_pretty(&narrative).unwrap();
                assert!(pretty.len() > 8192 && pretty.len() <= 16384);
                pretty
            } else if mode == "oversized_terminal_usage" {
                "x".repeat(9000)
            } else {
                narrative.to_string()
            };
            let mut frames = Vec::new();
            if mode == "useful_stream" {
                let length = summary.len().div_ceil(9);
                for part in summary.as_bytes().chunks(length) {
                    frames.push(json!({"choices":[{"index":0,"delta":{"content":std::str::from_utf8(part).unwrap()},"finish_reason":null}],"usage":null}));
                }
            } else {
                frames.push(json!({"choices":[{"index":0,"delta":{"content":summary},"finish_reason":null}],"usage":null}));
            }
            // DeepSeek documents usage ON its terminal content frame, not a
            // separate choices=[] usage-only frame. Explicit components/cache.
            frames.push(json!({"choices":[{"index":0,"delta":{"content":"","role":null},"finish_reason":"stop"}],"usage":{"prompt_tokens":17,"completion_tokens":9,"total_tokens":26,"prompt_cache_hit_tokens":2,"prompt_cache_miss_tokens":15,"prompt_tokens_details":{"cached_tokens":2}}}));
            server.replies.send(json!({"fixture_frames":frames,"fixture_first_event_delay_ms":if mode=="delayed_first"{12000}else{0},"fixture_frame_interval_ms":if mode=="useful_stream"{1200}else{0}})).unwrap();
            if matches!(mode, "oversized_terminal_usage" | "wrong_reference_alias") {
                return child.join().unwrap();
            }
            println!("delayed-compaction-control stage=await-coding mode={mode}");
            let coding = match server.requests.recv_timeout(Duration::from_secs(30)) {
                Ok(request) => request,
                Err(error) => {
                    let output = child.join().unwrap();
                    let db = rusqlite::Connection::open(&fixture.database).unwrap();
                    let reason:String=db.query_row("SELECT payload FROM event_log WHERE event_type='run_error' ORDER BY id DESC LIMIT 1",[],|row|row.get(0)).unwrap();
                    panic!(
                        "coding request missing after summary: {error}; {reason}; {}",
                        String::from_utf8_lossy(&output.stdout)
                    );
                }
            };
            assert!(coding["messages"]
                .to_string()
                .contains("historical_lossy_summary"));
            tool(
                &server,
                "new-observation",
                "read_text",
                json!({"path":"src/lib.rs"}),
            );
            let read = tool_reply(&next(&server), "new-observation");
            tool(
                &server,
                "new-valid-edit",
                "edit_text",
                json!({"path":"src/lib.rs","expected_sha256":read["sha256"],"old_text":"    7\n","new_text":"    8\n"}),
            );
            next(&server);
            text(&server, ready, true);
            let feedback = next(&server);
            assert!(feedback["messages"]
                .to_string()
                .contains("native_verifier_feedback"));
            assert!(feedback["messages"].to_string().contains("passed"));
            text(&server, ready, true);
            child.join().unwrap()
        });
        if matches!(mode, "oversized_terminal_usage" | "wrong_reference_alias") {
            let result: Value = serde_json::from_slice(&output.stdout).unwrap();
            assert_eq!(result["task_success"], false);
            let account = json_output(fixture.run(&["goal", "status", "--json"]));
            assert_eq!(
                account["goal"]["accounting"]["counters"]["unresolved_tokens"],
                0
            );
            assert_eq!(
                account["execution"]["requests"]
                    .as_array()
                    .unwrap()
                    .last()
                    .unwrap()["observed"]["total_tokens"],
                26
            );
            assert!(context(&fixture)["summary_checkpoint_id"].is_null());
            assert!(
                std::fs::read_to_string(fixture.workspace.join("src/lib.rs"))
                    .unwrap()
                    .contains("    7\n")
            );
            let db = rusqlite::Connection::open(&fixture.database).unwrap();
            let payload:String=db.query_row("SELECT payload FROM event_log WHERE event_type='summary_transport_observed' ORDER BY id DESC LIMIT 1",[],|row|row.get(0)).unwrap();
            let event: Value = serde_json::from_str(&payload).unwrap();
            let observation = &event["SummaryTransportObserved"]["observation"];
            assert_eq!(
                observation["primary_stop"],
                if mode == "wrong_reference_alias" {
                    "completed"
                } else {
                    "summary_content_limit"
                }
            );
            assert_eq!(observation["valid_usage_observed"], true);
            assert_eq!(observation["terminal_event_seen"], true);
            assert!(server.requests.try_recv().is_err());
            continue;
        }
        let result = json_output(output);
        assert_eq!(result["task_success"], true, "{result}");
        assert_eq!(result["goal"]["id"], id);
        let account = json_output(fixture.run(&["goal", "status", "--json"]));
        assert!(account["execution"]["requests"]
            .as_array()
            .unwrap()
            .iter()
            .any(|q| q["purpose"] == "compaction_tools_disabled"
                && q["observed"]["total_tokens"] == 26));
        let db = rusqlite::Connection::open(&fixture.database).unwrap();
        let payload:String=db.query_row("SELECT payload FROM event_log WHERE event_type='summary_transport_observed' ORDER BY id DESC LIMIT 1",[],|row|row.get(0)).unwrap();
        let observed: Value = serde_json::from_str(&payload).unwrap();
        let observation = &observed["SummaryTransportObserved"]["observation"];
        assert_eq!(observation["envelope_ms"], 120000);
        assert_eq!(observation["execution_budget_ms"], 115000);
        assert_eq!(observation["primary_stop"], "completed");
        assert_eq!(observation["valid_usage_observed"], true);
        if mode == "pretty_json" {
            assert!(observation["transport_content_bytes"].as_u64().unwrap() > 8192);
            assert!(observation["retained_content_bytes"].as_u64().unwrap() <= 8192);
        } else {
            assert!(observation["elapsed_ms"].as_u64().unwrap() > 10000);
        }
        assert!(server.requests.try_recv().is_err());
    }
}

fn deliver(fixture: &CliFixture, id: &str, iterations: &str) -> Output {
    let mut child = ChildGuard(
        fixture
            .command(&[
                "goal",
                "deliver",
                "--goal-id",
                id,
                "--iterations",
                iterations,
                "--elapsed-seconds",
                "300",
                "--json",
            ])
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap(),
    );
    let stdout = child.stdout.take().unwrap();
    let stderr = child.stderr.take().unwrap();
    let out = thread::spawn(move || read_output(stdout));
    let err = thread::spawn(move || read_output(stderr));
    let deadline = Instant::now() + Duration::from_secs(310);
    let status = loop {
        if let Some(status) = child.try_wait().unwrap() {
            break status;
        }
        assert!(Instant::now() < deadline, "native CLI watchdog");
        thread::sleep(Duration::from_millis(10));
    };
    let output = Output {
        status,
        stdout: out.join().unwrap(),
        stderr: err.join().unwrap(),
    };
    if let Some(root) = std::env::var_os("ARGENTUM_MILESTONE_EVIDENCE_ROOT") {
        let root = PathBuf::from(root);
        std::fs::create_dir_all(&root).unwrap();
        let name = fixture.root.path().file_name().unwrap().to_string_lossy();
        std::fs::write(root.join(format!("{name}-settled.json")), &output.stdout).unwrap();
        std::fs::write(root.join(format!("{name}-stderr.txt")), &output.stderr).unwrap();
        let db = rusqlite::Connection::open(&fixture.database).unwrap();
        let mut query=db.prepare("SELECT payload FROM event_log WHERE event_type IN ('summary_transport_observed','context_compaction_completed','run_status_changed','run_error') ORDER BY id").unwrap();
        let events = query
            .query_map([], |row| row.get::<_, String>(0))
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        std::fs::write(
            root.join(format!("{name}-diagnostics.json")),
            serde_json::to_vec(&events).unwrap(),
        )
        .unwrap();
    }
    output
}

#[test]
fn milestone_native_exact_cli_fail_guarded_correction_pass_feedback_restart() {
    let server = Server::start();
    let fixture = CliFixture::new(&server.endpoint);
    let id = prepare(&fixture, 8);
    let before = std::fs::read(fixture.workspace.join("fixture.txt")).unwrap();
    let ready = r#"{"status":"ready","remaining":[],"summary":"Public work ready for current verification."}"#;
    let mut requests = Vec::new();
    let output = thread::scope(|scope| {
        let child = scope.spawn(|| deliver(&fixture, &id, "8"));
        requests.push(next(&server));
        text(&server, ready, true);
        requests.push(next(&server));
        let diagnostic = requests.last().unwrap()["messages"].to_string();
        assert!(diagnostic.contains("native_verifier_feedback"));
        assert!(diagnostic.contains("MILESTONE_CURRENT_ASSERTION"));
        assert!(diagnostic.contains("failed"));
        tool(
            &server,
            "observe",
            "read_text",
            json!({"path":"src/lib.rs"}),
        );
        requests.push(next(&server));
        let read = tool_reply(requests.last().unwrap(), "observe");
        tool(
            &server,
            "correct",
            "edit_text",
            json!({"path":"src/lib.rs","expected_sha256":read["sha256"],"old_text":"    7\n","new_text":"    8\n"}),
        );
        requests.push(next(&server));
        assert!(tool_reply(requests.last().unwrap(), "correct")["sha256"].is_string());
        text(&server, ready, true);
        requests.push(next(&server));
        let messages = requests.last().unwrap()["messages"].to_string();
        assert!(messages.contains("feedback"));
        assert!(messages.contains("native_verifier_feedback"));
        assert!(messages.contains("passed"));
        text(&server, ready, true);
        child.join().unwrap()
    });
    let result = json_output(output);
    assert_eq!(result["task_success"], true, "{result}");
    assert_eq!(result["invocation"]["state"], "verified");
    let attempts = result["invocation"]["attempts"].as_array().unwrap();
    assert_eq!(attempts.len(), 5);
    assert_eq!(attempts[1]["outcome"], "criteria_failed");
    assert_eq!(attempts[4]["outcome"], "verified");
    assert_ne!(attempts[0]["run_id"], attempts[2]["run_id"]);
    assert_eq!(
        std::fs::read(fixture.workspace.join("fixture.txt")).unwrap(),
        before
    );
    assert_eq!(
        std::fs::read_to_string(fixture.workspace.join("src/lib.rs"))
            .unwrap()
            .matches("    8\n")
            .count(),
        1
    );
    let status = json_output(fixture.run(&["goal", "status", "--json"]));
    assert_eq!(status["goal"]["lifecycle"], "complete");
    assert_eq!(
        status["goal"]["accounting"]["counters"]["request_attempts"],
        5
    );
    assert_eq!(status["supervision"]["completion_current"], true);
    assert!(server.requests.try_recv().is_err());
    if let Some(root) = std::env::var_os("ARGENTUM_MILESTONE_EVIDENCE_ROOT") {
        let root = PathBuf::from(root);
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(
            root.join("native-next-requests.json"),
            serde_json::to_vec_pretty(&requests).unwrap(),
        )
        .unwrap();
        std::fs::write(
            root.join("native-result.json"),
            serde_json::to_vec_pretty(&result).unwrap(),
        )
        .unwrap();
        std::fs::write(
            root.join("native-restarted-status.json"),
            serde_json::to_vec_pretty(&status).unwrap(),
        )
        .unwrap();
    }
}

#[test]
fn milestone_partial_increment_continue_gets_native_failure_before_correction() {
    let server = Server::start();
    let fixture = CliFixture::new(&server.endpoint);
    let id = prepare(&fixture, 8);
    let before = json_output(fixture.run(&["goal", "status", "--json"]));
    let removed = json_output(fixture.run(&[
        "goal",
        "limits",
        "--goal-id",
        &id,
        "--remove-aggregate",
        "--json",
    ]));
    assert_eq!(removed["goal"]["id"], before["goal"]["id"]);
    assert_eq!(
        removed["goal"]["accounting"]["counters"],
        before["goal"]["accounting"]["counters"]
    );
    let fresh = json_output(fixture.run(&["goal", "status", "--json"]));
    assert!(fresh["goal"]["token_budget"].is_null());
    assert!(fresh["goal"]["accounting"]["request_limit"].is_null());
    assert_eq!(
        fresh["goal"]["accounting"]["per_run"],
        before["goal"]["accounting"]["per_run"]
    );
    for args in [
        vec![
            "goal",
            "limits",
            "--goal-id",
            &id,
            "--remove-aggregate",
            "--remove-aggregate",
            "--json",
        ],
        vec![
            "goal",
            "limits",
            "--goal-id",
            &id,
            "--remove-aggregate",
            "--limits",
            "2000000,32,8,32,600,1000000,32,16,300",
            "--json",
        ],
    ] {
        assert!(!fixture.run(&args).status.success());
    }
    let ready = r#"{"status":"ready","remaining":[]}"#;
    let output = thread::scope(|scope| {
        let child = scope.spawn(|| deliver(&fixture, &id, "8"));
        next(&server);
        tool(
            &server,
            "observe-partial",
            "read_text",
            json!({"path":"src/lib.rs"}),
        );
        let request = next(&server);
        let read = tool_reply(&request, "observe-partial");
        tool(
            &server,
            "partial-edit",
            "edit_text",
            json!({"path":"src/lib.rs","expected_sha256":read["sha256"],"old_text":"    7\n","new_text":"    6\n"}),
        );
        let request = next(&server);
        assert!(tool_reply(&request, "partial-edit")["sha256"].is_string());
        text(
            &server,
            r#"{"status":"continue","remaining":["Finish the public requirement after the partial increment."]}"#,
            true,
        );
        let request = next(&server);
        let messages = request["messages"].to_string();
        assert!(messages.contains("native_verifier_feedback"));
        assert!(messages.contains("MILESTONE_CURRENT_ASSERTION"));
        assert!(messages.contains("failed"));
        tool(
            &server,
            "fresh-after-check",
            "read_text",
            json!({"path":"src/lib.rs"}),
        );
        let request = next(&server);
        let read = tool_reply(&request, "fresh-after-check");
        tool(
            &server,
            "correct-partial",
            "edit_text",
            json!({"path":"src/lib.rs","expected_sha256":read["sha256"],"old_text":"    6\n","new_text":"    8\n"}),
        );
        next(&server);
        text(&server, ready, true);
        let request = next(&server);
        assert!(request["messages"].to_string().contains("passed"));
        text(&server, ready, true);
        child.join().unwrap()
    });
    let result = json_output(output);
    assert_eq!(result["task_success"], true, "{result}");
    let attempts = result["invocation"]["attempts"].as_array().unwrap();
    assert_eq!(attempts.len(), 5);
    assert_eq!(
        attempts[0]["stop_reason"],
        "scheduled_increment_verification"
    );
    assert_eq!(attempts[1]["outcome"], "criteria_failed");
    assert_eq!(attempts[4]["outcome"], "verified");
    assert_eq!(
        result["goal"]["accounting"]["counters"]["request_attempts"],
        7
    );
    assert!(server.requests.try_recv().is_err());
}

fn unapproved_native_control_runner(fixture: &CliFixture) {
    std::fs::create_dir(fixture.workspace.join(".agents")).unwrap();
    std::fs::write(fixture.workspace.join(".agents/verification.yaml"),"version: 1\nrunners:\n  - id: not-approved\n    label: Unapproved control only\n    program: cargo\n    args: [test, --locked, --offline, -j, '1']\n    cwd: .\n    timeout_ms: 120000\n    output_limit: 65536\n").unwrap();
}

#[test]
fn milestone_unapproved_model_verify_is_unexecuted_feedback_not_native_authority() {
    let server = Server::start();
    let fixture = CliFixture::new(&server.endpoint);
    let id = prepare(&fixture, 8);
    unapproved_native_control_runner(&fixture);
    let ready = r#"{"status":"ready","remaining":[]}"#;
    let output = thread::scope(|scope| {
        let child = scope.spawn(|| deliver(&fixture, &id, "8"));
        next(&server);
        tool(
            &server,
            "unapproved-selection",
            "verify",
            json!({"runners":["project:not-approved"]}),
        );
        let request = next(&server);
        let rejected = tool_reply(&request, "unapproved-selection");
        assert_eq!(rejected["executed"], false);
        assert_eq!(rejected["success"], false);
        assert_eq!(rejected["code"], "approval_required");
        assert!(!fixture.workspace.join("target").exists());
        tool(
            &server,
            "observe-authorized-work",
            "read_text",
            json!({"path":"src/lib.rs"}),
        );
        let request = next(&server);
        let read = tool_reply(&request, "observe-authorized-work");
        tool(
            &server,
            "authorized-edit",
            "edit_text",
            json!({"path":"src/lib.rs","expected_sha256":read["sha256"],"old_text":"    7\n","new_text":"    8\n"}),
        );
        next(&server);
        text(&server, ready, true);
        let request = next(&server);
        assert!(request["messages"].to_string().contains("passed"));
        text(&server, ready, true);
        child.join().unwrap()
    });
    let result = json_output(output);
    assert_eq!(result["task_success"], true, "{result}");
    let inspection = json_output(fixture.run(&["verification", "list", "--json"]));
    assert!(!inspection.to_string().contains("project:not-approved"));
    assert_eq!(
        result["goal"]["accounting"]["counters"]["request_attempts"],
        5
    );
    assert!(server.requests.try_recv().is_err());
}

#[test]
fn milestone_repeated_unapproved_model_verification_still_stops_without_effects() {
    let server = Server::start();
    let fixture = CliFixture::new(&server.endpoint);
    let id = prepare(&fixture, 8);
    unapproved_native_control_runner(&fixture);
    let output = thread::scope(|scope| {
        let child = scope.spawn(|| deliver(&fixture, &id, "8"));
        for index in 0..3 {
            let request = next(&server);
            if index > 0 {
                assert_eq!(
                    tool_reply(&request, &format!("blocked-{}", index - 1))["executed"],
                    false
                );
            }
            tool(
                &server,
                &format!("blocked-{index}"),
                "verify",
                json!({"runners":["project:not-approved"]}),
            );
        }
        child.join().unwrap()
    });
    let result = json_output(output);
    assert_eq!(result["task_success"], false);
    assert!(
        result["invocation"]["stop_reason"]
            .as_str()
            .unwrap()
            .contains("repeated_tool_rejection"),
        "{result}"
    );
    assert!(!fixture.workspace.join("target").exists());
    assert!(server.requests.try_recv().is_err());
}

#[test]
fn milestone_ordinary_headless_unapproved_verify_still_requires_owner() {
    let server = Server::start();
    let fixture = CliFixture::new(&server.endpoint);
    prepare(&fixture, 8);
    unapproved_native_control_runner(&fixture);
    let output = thread::scope(|scope| {
        let child = scope.spawn(|| {
            fixture.run(&[
                "run",
                "--final-result",
                "--prompt",
                "Inspect the public verification route without changing files.",
            ])
        });
        next(&server);
        tool(
            &server,
            "ordinary-unapproved",
            "verify",
            json!({"runners":["project:not-approved"]}),
        );
        child.join().unwrap()
    });
    assert!(!output.status.success());
    let diagnostic = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(diagnostic.contains("approval_required"), "{diagnostic}");
    assert!(!fixture.workspace.join("target").exists());
    assert!(server.requests.try_recv().is_err());
}

#[test]
fn milestone_cargo_check_missing_method_reaches_serialized_feedback() {
    let server = Server::start();
    let fixture = CliFixture::new(&server.endpoint);
    let id = prepare(&fixture, 8);
    std::fs::write(
        fixture.workspace.join("src/lib.rs"),
        "pub struct Observation;\npub fn value() -> u32 {\n    Observation.synthetic_missing_method()\n}\n",
    )
    .unwrap();
    json_output(fixture.run(&[
        "goal",
        "criteria",
        "--goal-id",
        &id,
        "--runners",
        "cargo-check-locked",
        "--json",
    ]));
    let preserved = std::fs::read(fixture.workspace.join("fixture.txt")).unwrap();
    let ready = r#"{"status":"ready","remaining":[]}"#;
    let mut requests = Vec::new();
    let output = thread::scope(|scope| {
        let child = scope.spawn(|| deliver(&fixture, &id, "8"));
        requests.push(next(&server));
        text(&server, ready, true);
        requests.push(next(&server));
        let feedback = requests.last().unwrap()["messages"].to_string();
        assert!(feedback.contains("native_verifier_feedback"));
        assert!(
            feedback.contains("E0599"),
            "missing compiler code: {feedback}"
        );
        assert!(feedback.contains("synthetic_missing_method"));
        assert!(feedback.contains("failed"));
        assert!(feedback.contains("Prioritize the concrete failure"));
        tool(
            &server,
            "fresh-compiler-read",
            "read_text",
            json!({"path":"src/lib.rs"}),
        );
        requests.push(next(&server));
        let read = tool_reply(requests.last().unwrap(), "fresh-compiler-read");
        tool(
            &server,
            "fresh-compiler-edit",
            "edit_text",
            json!({"path":"src/lib.rs","expected_sha256":read["sha256"],"old_text":"    Observation.synthetic_missing_method()\n","new_text":"    8\n"}),
        );
        requests.push(next(&server));
        assert!(tool_reply(requests.last().unwrap(), "fresh-compiler-edit")["sha256"].is_string());
        text(&server, ready, true);
        requests.push(next(&server));
        assert!(requests.last().unwrap()["messages"]
            .to_string()
            .contains("passed"));
        text(&server, ready, true);
        child.join().unwrap()
    });
    let result = json_output(output);
    assert_eq!(result["task_success"], true, "{result}");
    assert_eq!(
        std::fs::read(fixture.workspace.join("fixture.txt")).unwrap(),
        preserved
    );
    assert!(server.requests.try_recv().is_err());
    if let Some(root) = std::env::var_os("ARGENTUM_MILESTONE_EVIDENCE_ROOT") {
        let root = PathBuf::from(root);
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(
            root.join("cargo-check-next-requests.json"),
            serde_json::to_vec_pretty(&requests).unwrap(),
        )
        .unwrap();
        std::fs::write(
            root.join("cargo-check-result.json"),
            serde_json::to_vec_pretty(&result).unwrap(),
        )
        .unwrap();
    }
}

#[test]
fn milestone_restart_refreshes_failed_check_before_model_correction() {
    let server = Server::start();
    let fixture = CliFixture::new(&server.endpoint);
    let id = prepare(&fixture, 16);
    std::fs::write(fixture.workspace.join("src/lib.rs"), "pub struct Observation;\npub fn value() -> u32 {\n    Observation.synthetic_restart_method()\n}\n").unwrap();
    json_output(fixture.run(&[
        "goal",
        "criteria",
        "--goal-id",
        &id,
        "--runners",
        "cargo-check-locked",
        "--json",
    ]));
    let ready = r#"{"status":"ready","remaining":[]}"#;
    let first = thread::scope(|scope| {
        let child = scope.spawn(|| deliver(&fixture, &id, "2"));
        next(&server);
        text(&server, ready, true);
        child.join().unwrap()
    });
    let failed: Value = serde_json::from_slice(&first.stdout).unwrap();
    assert_eq!(failed["task_success"], false);
    let fresh = json_output(fixture.run(&["goal", "status", "--json"]));
    assert_eq!(
        fresh["supervision"]["current_criteria"][0]["state"],
        "failed"
    );
    // A source-preserving external observation still changes the snapshot.
    // The resumed invocation must re-observe, not replay old verifier text.
    let path = fixture.workspace.join("src/lib.rs");
    let mut source = std::fs::read_to_string(&path).unwrap();
    source.push_str("\n// Changed snapshot, same unresolved public compile requirement.\n");
    std::fs::write(path, source).unwrap();
    let stale = json_output(fixture.run(&["goal", "status", "--json"]));
    assert_eq!(
        stale["supervision"]["current_criteria"][0]["state"],
        "stale"
    );
    let mut initial = Value::Null;
    let second = thread::scope(|scope| {
        let child = scope.spawn(|| deliver(&fixture, &id, "8"));
        initial = next(&server);
        tool(
            &server,
            "restart-read",
            "read_text",
            json!({"path":"src/lib.rs"}),
        );
        let read = tool_reply(&next(&server), "restart-read");
        tool(
            &server,
            "restart-edit",
            "edit_text",
            json!({"path":"src/lib.rs","expected_sha256":read["sha256"],"old_text":"    Observation.synthetic_restart_method()\n","new_text":"    8\n"}),
        );
        next(&server);
        text(&server, ready, true);
        next(&server);
        text(&server, ready, true);
        child.join().unwrap()
    });
    let result = json_output(second);
    assert_eq!(result["task_success"], true, "{result}");
    let feedback = initial["messages"].to_string();
    assert!(
        feedback.contains("native_verifier_feedback"),
        "restart lacks actual failed-check feedback"
    );
    assert!(feedback.contains("E0599") && feedback.contains("synthetic_restart_method"));
    assert!(server.requests.try_recv().is_err());
}

#[test]
fn milestone_model_verifier_deadline_closes_host_without_recovery() {
    let server = Server::start();
    let fixture = CliFixture::new(&server.endpoint);
    let id = prepare(&fixture, 8);
    json_output(fixture.run(&[
        "goal",
        "limits",
        "--goal-id",
        &id,
        "--limits",
        "2000000,32,8,32,600,1000000,32,16,45",
        "--json",
    ]));
    std::fs::write(fixture.workspace.join("src/lib.rs"), "pub fn value() -> u32 { 8 }\n#[test]\nfn delayed_public_check() { std::thread::sleep(std::time::Duration::from_secs(60)); }\n").unwrap();
    let output = thread::scope(|scope| {
        let child = scope.spawn(|| deliver(&fixture, &id, "8"));
        next(&server);
        tool(
            &server,
            "bounded-deadline-check",
            "verify",
            json!({"runners":["cargo-test-locked"]}),
        );
        child.join().unwrap()
    });
    let status = fixture.run(&["goal", "status", "--json"]);
    assert!(
        status.status.success(),
        "ordinary verifier deadline left an abandoned host: {}",
        String::from_utf8_lossy(&status.stderr)
    );
    let status = json_output(status);
    assert_eq!(status["goal"]["id"], id);
    assert_eq!(
        status["goal"]["accounting"]["counters"]["unresolved_tokens"],
        0
    );
    assert_eq!(
        status["goal"]["accounting"]["counters"]["request_attempts"],
        1
    );
    assert_eq!(
        status["supervision"]["current_criteria"][0]["state"],
        "cancelled"
    );
    let settled = json_output(output);
    assert_eq!(settled["task_success"], false);
    assert_eq!(
        settled["invocation"]["stop_reason"],
        "goal_deadline_exceeded"
    );
    assert!(server.requests.try_recv().is_err());
}

#[test]
fn milestone_verification_policy_missing_read_and_search_return_typed_feedback() {
    let server = Server::start();
    let fixture = CliFixture::new(&server.endpoint);
    let id = prepare(&fixture, 8);
    // Match the existing real project's already enabled bounded search caps.
    // Aliased descriptors can otherwise disable a shared cap in fixture setup.
    tokio::runtime::Runtime::new().unwrap().block_on(async {
        let mut config = argentum_cli::HostConfig::in_memory(&fixture.workspace);
        config.database = Some(fixture.database.clone());
        let mut host = argentum_cli::CommandHost::start(config).unwrap();
        for capability in ["tool.search-text", "tool.search-filenames"] {
            host.local_handle()
                .dispatch(AppCommand::SetHarnessCapabilityEnabled {
                    capability_id: capability.into(),
                    enabled: true,
                })
                .await
                .unwrap();
        }
        host.shutdown().await.unwrap();
    });
    let ready = r#"{"status":"ready","remaining":[]}"#;
    let mut failed = Vec::new();
    let output = thread::scope(|scope| {
        let child = scope.spawn(|| deliver(&fixture, &id, "8"));
        next(&server);
        tool(
            &server,
            "missing-read",
            "read_text",
            json!({"path":"missing.rs"}),
        );
        let request = next(&server);
        failed.push(
            request["messages"]
                .as_array()
                .unwrap()
                .iter()
                .find(|message| message["tool_call_id"] == "missing-read")
                .unwrap()["content"]
                .clone(),
        );
        tool(
            &server,
            "missing-search",
            "search_text",
            json!({"path":"missing-directory","query":"value","max_results":4}),
        );
        let request = next(&server);
        failed.push(
            request["messages"]
                .as_array()
                .unwrap()
                .iter()
                .find(|message| message["tool_call_id"] == "missing-search")
                .unwrap()["content"]
                .clone(),
        );
        tool(
            &server,
            "exact-current-read",
            "read_text",
            json!({"path":"src/lib.rs"}),
        );
        let read = tool_reply(&next(&server), "exact-current-read");
        tool(
            &server,
            "exact-current-edit",
            "edit_text",
            json!({"path":"src/lib.rs","expected_sha256":read["sha256"],"old_text":"    7\n","new_text":"    8\n"}),
        );
        next(&server);
        text(&server, ready, true);
        next(&server);
        text(&server, ready, true);
        child.join().unwrap()
    });
    let result = json_output(output);
    assert_eq!(result["task_success"], true, "{result}");
    for content in failed {
        let body: Value = serde_json::from_str(content.as_str().unwrap())
            .expect("failure must be structured, not generic boundary prose");
        assert_eq!(body["success"], false);
        assert_eq!(body["code"], "not_found");
        assert!(body["detail"]
            .as_str()
            .unwrap()
            .contains("enabled bounded filename"));
        assert!(!content
            .as_str()
            .unwrap()
            .contains(&fixture.workspace.to_string_lossy().to_string()));
    }
    assert!(!fixture.workspace.join("missing.rs").exists());
    assert!(!fixture.workspace.join("missing-directory").exists());
    assert!(server.requests.try_recv().is_err());
}

#[test]
fn milestone_native_plan_does_not_complete_and_no_progress_stops() {
    let server = Server::start();
    let fixture = CliFixture::new(&server.endpoint);
    let id = prepare(&fixture, 8);
    // Initially passing source does not turn a plan-only answer into completion.
    let path = fixture.workspace.join("src/lib.rs");
    std::fs::write(
        &path,
        std::fs::read_to_string(&path)
            .unwrap()
            .replace("    7\n", "    8\n"),
    )
    .unwrap();
    let output = thread::scope(|scope| {
        let child = scope.spawn(|| deliver(&fixture, &id, "8"));
        for _ in 0..2 {
            next(&server);
            text(
                &server,
                "I will inspect and implement the public objective next.",
                true,
            );
        }
        child.join().unwrap()
    });
    let result = json_output(output);
    assert_eq!(result["task_success"], false);
    assert_eq!(result["invocation"]["stop_reason"], "insufficient_progress");
    assert_eq!(result["goal"]["lifecycle"], "active");
    assert!(server.requests.try_recv().is_err());
}

#[test]
fn milestone_native_unknown_usage_stops_without_automatic_retry() {
    let server = Server::start();
    let fixture = CliFixture::new(&server.endpoint);
    let id = prepare(&fixture, 8);
    let output = thread::scope(|scope| {
        let child = scope.spawn(|| deliver(&fixture, &id, "8"));
        next(&server);
        text(&server, r#"{"status":"ready","remaining":[]}"#, false);
        child.join().unwrap()
    });
    let result = json_output(output);
    assert_eq!(result["task_success"], false);
    assert_eq!(result["invocation"]["state"], "unresolved");
    assert!(server.requests.try_recv().is_err());
    json_output(fixture.run(&["goal", "status", "--json"]));
    assert!(server.requests.try_recv().is_err());
}

#[test]
fn milestone_native_same_goal_resumes_after_exact_owner_compaction_disposition_without_replay() {
    let server = Server::start();
    let fixture = CliFixture::new(&server.endpoint);
    let id = prepare(&fixture, 8);
    for _ in 0..2 {
        let output = thread::scope(|scope| {
            let child = scope.spawn(|| {
                fixture.run(&[
                    "run",
                    "--goal",
                    &id,
                    "--prompt",
                    "Synthetic public history, no tools or edits",
                    "--json",
                ])
            });
            next(&server);
            text(
                &server,
                &"Historical synthetic observation. ".repeat(100),
                true,
            );
            child.join().unwrap()
        });
        assert!(output.status.success());
    }
    let output = thread::scope(|scope| {
        let child = scope.spawn(|| fixture.run(&["compact", "--goal", &id, "--json"]));
        let request = server
            .requests
            .recv_timeout(Duration::from_secs(30))
            .unwrap();
        assert!(
            request["tools"].is_null() || request["tools"].as_array().is_some_and(Vec::is_empty)
        );
        assert_eq!(request["stream_options"]["include_usage"], true);
        text(&server, "unused synthetic summary", false);
        child.join().unwrap()
    });
    assert!(!output.status.success());
    let old = json_output(fixture.run(&["goal", "status", "--json"]));
    let session = old["goal"]["session_id"].as_str().unwrap();
    let request = old["execution"]["requests"]
        .as_array()
        .unwrap()
        .iter()
        .find(|q| q["state"] == "unresolved")
        .unwrap();
    assert_eq!(request["purpose"], "compaction_tools_disabled");
    let request_id = request["request_id"].as_str().unwrap();
    let inspection = json_output(fixture.run(&[
        "accounting",
        "inspect",
        "--goal-id",
        &id,
        "--session-id",
        session,
        "--request-id",
        request_id,
        "--json",
    ]));
    let amount = inspection["binding"]["reserved_tokens"]
        .as_u64()
        .unwrap()
        .to_string();
    let path = fixture.root.path().join("owner-resolution.json");
    std::fs::write(&path,json!({"command_id":"native-owner-compaction-disposition","binding":inspection["binding"],"decision":{"kind":"owner_accept_reservation"}}).to_string()).unwrap();
    json_output(fixture.run(&[
        "accounting",
        "resolve",
        "--resolution",
        path.to_str().unwrap(),
        "--confirm-reservation",
        &amount,
        "--json",
    ]));
    assert!(server.requests.try_recv().is_err());
    let ready = r#"{"status":"ready","remaining":[],"summary":"Synthetic source ready for current checks."}"#;
    let output = thread::scope(|scope| {
        let child = scope.spawn(|| deliver(&fixture, &id, "5"));
        let initial = next(&server);
        assert!(!initial["messages"]
            .to_string()
            .contains("bounded_context_compaction"));
        tool(
            &server,
            "fresh-read",
            "read_text",
            json!({"path":"src/lib.rs"}),
        );
        let observed = next(&server);
        let read = tool_reply(&observed, "fresh-read");
        tool(
            &server,
            "fresh-edit",
            "edit_text",
            json!({"path":"src/lib.rs","expected_sha256":read["sha256"],"old_text":"    7\n","new_text":"    8\n"}),
        );
        next(&server);
        text(&server, ready, true);
        let feedback = next(&server);
        assert!(feedback["messages"]
            .to_string()
            .contains("native_verifier_feedback"));
        assert!(feedback["messages"].to_string().contains("passed"));
        text(&server, ready, true);
        child.join().unwrap()
    });
    let result = json_output(output);
    assert_eq!(result["task_success"], true);
    assert_eq!(result["goal"]["id"], id);
    let after = json_output(fixture.run(&[
        "accounting",
        "inspect",
        "--goal-id",
        &id,
        "--session-id",
        session,
        "--request-id",
        request_id,
        "--json",
    ]));
    assert_eq!(after["charge"]["state"], "unresolved");
    assert_eq!(after["charge"]["observed"], Value::Null);
    assert_eq!(after["ownership"], inspection["ownership"]);
    assert_eq!(
        after["run"]["stop_reason"],
        inspection["run"]["stop_reason"]
    );
    assert!(server.requests.try_recv().is_err());
}

#[test]
fn milestone_native_missing_effective_provider_budget_cannot_dispatch() {
    let server = Server::start();
    let fixture = CliFixture::new(&server.endpoint);
    let id = prepare(&fixture, 8);
    json_output(fixture.run(&[
        "provider",
        "save",
        "unbudgeted",
        "--label",
        "Unbudgeted local control",
        "--kind",
        "openai-compatible",
        "--endpoint",
        &server.endpoint,
        "--model",
        "fixture-model",
        "--select",
        "--json",
    ]));
    let output = deliver(&fixture, &id, "8");
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("budget"));
    assert!(server.requests.try_recv().is_err());
    let status = json_output(fixture.run(&["goal", "status", "--json"]));
    assert!(status["supervision"]["invocation"].is_null());
}

#[test]
fn milestone_native_insufficient_aggregate_iterations_preserves_failed_check() {
    let server = Server::start();
    let fixture = CliFixture::new(&server.endpoint);
    let id = prepare(&fixture, 2);
    let output = thread::scope(|scope| {
        let child = scope.spawn(|| deliver(&fixture, &id, "2"));
        next(&server);
        text(&server, r#"{"status":"ready","remaining":[]}"#, true);
        child.join().unwrap()
    });
    let result = json_output(output);
    assert_eq!(result["task_success"], false);
    assert_eq!(result["invocation"]["stop_reason"], "iteration_limit");
    assert_eq!(
        result["invocation"]["attempts"].as_array().unwrap().len(),
        2
    );
    assert_eq!(
        result["supervision"]["current_criteria"][0]["state"],
        "failed"
    );
    assert!(server.requests.try_recv().is_err());
}

#[test]
fn milestone_native_planned_request_handoff_is_not_unlimited_correction() {
    let server = Server::start();
    let fixture = CliFixture::new(&server.endpoint);
    let id = prepare(&fixture, 8);
    json_output(fixture.run(&[
        "goal",
        "limits",
        "--goal-id",
        &id,
        "--limits",
        "2000000,32,8,32,600,1000000,4,16,300",
        "--json",
    ]));
    let output = thread::scope(|scope| {
        let child = scope.spawn(|| deliver(&fixture, &id, "8"));
        for i in 0..6 {
            next_coding_after_reported_handoff(&server);
            tool(
                &server,
                &format!("observe-{i}"),
                "read_text",
                json!({"path":"fixture.txt"}),
            );
        }
        let final_report = next(&server);
        assert!(final_report["messages"]
            .to_string()
            .contains("[native_handoff_feedback]"));
        assert!(final_report["tools"]
            .as_array()
            .map_or(true, |tools| tools.is_empty()));
        text(
            &server,
            r#"{"status":"continue","remaining":["The source still requires implementation."]}"#,
            true,
        );
        child.join().unwrap()
    });
    let result = json_output(output);
    assert_eq!(result["task_success"], false);
    assert_eq!(result["invocation"]["stop_reason"], "insufficient_progress");
    let attempts = result["invocation"]["attempts"].as_array().unwrap();
    assert_eq!(
        attempts
            .iter()
            .filter(|attempt| attempt["stop_reason"] == "scheduled_request_handoff")
            .count(),
        3
    );
    assert_eq!(
        result["goal"]["accounting"]["counters"]["request_attempts"],
        9
    );
    assert!(server.requests.try_recv().is_err());
}

#[test]
fn milestone_native_project_criterion_does_not_grant_execution() {
    let server = Server::start();
    let fixture = CliFixture::new(&server.endpoint);
    let id = prepare(&fixture, 8);
    std::fs::create_dir(fixture.workspace.join(".agents")).unwrap();
    std::fs::write(fixture.workspace.join(".agents/verification.yaml"), "version: 1\nrunners:\n  - id: native-tests\n    label: Native control tests\n    program: cargo\n    args: [test, --locked, --offline, -j, '1']\n    cwd: .\n    timeout_ms: 120000\n    output_limit: 65536\n").unwrap();
    json_output(fixture.run(&[
        "goal",
        "criteria",
        "--goal-id",
        &id,
        "--runners",
        "project:native-tests",
        "--json",
    ]));
    let output = thread::scope(|scope| {
        let child = scope.spawn(|| deliver(&fixture, &id, "8"));
        next(&server);
        text(&server, r#"{"status":"ready","remaining":[]}"#, true);
        child.join().unwrap()
    });
    let result = json_output(output);
    assert_eq!(result["task_success"], false);
    assert_eq!(result["invocation"]["stop_reason"], "approval_required");
    assert!(result["invocation"]["attempts"]
        .as_array()
        .unwrap()
        .iter()
        .all(|attempt| attempt["evidence_ids"].as_array().unwrap().is_empty()));
    assert!(!fixture.workspace.join("target").exists());
    assert!(server.requests.try_recv().is_err());
}

#[test]
fn milestone_native_orientation_handoffs_preserve_novel_reads_until_guarded_implementation() {
    let server = Server::start();
    let fixture = CliFixture::new(&server.endpoint);
    let id = prepare(&fixture, 8);
    json_output(fixture.run(&[
        "goal",
        "limits",
        "--goal-id",
        &id,
        "--limits",
        "2000000,32,8,32,600,1000000,4,16,300",
        "--json",
    ]));
    let output = thread::scope(|scope| {
        let child = scope.spawn(|| deliver(&fixture, &id, "8"));
        next_coding_after_reported_handoff(&server);
        tool(
            &server,
            "orient-fixture",
            "read_text",
            json!({"path":"fixture.txt"}),
        );
        next_coding_after_reported_handoff(&server);
        tool(
            &server,
            "orient-prefix",
            "read_text",
            json!({"path":"src/lib.rs","start_byte":0,"end_byte":12}),
        );
        next_coding_after_reported_handoff(&server);
        tool(
            &server,
            "orient-full",
            "read_text",
            json!({"path":"src/lib.rs"}),
        );
        next_coding_after_reported_handoff(&server);
        tool(
            &server,
            "orient-manifest",
            "read_text",
            json!({"path":"Cargo.toml"}),
        );
        next_coding_after_reported_handoff(&server);
        tool(
            &server,
            "observe-current",
            "read_text",
            json!({"path":"src/lib.rs"}),
        );
        let read_request = next(&server);
        let read = tool_reply(&read_request, "observe-current");
        tool(
            &server,
            "implement",
            "edit_text",
            json!({"path":"src/lib.rs","expected_sha256":read["sha256"],"old_text":"    7\n","new_text":"    8\n"}),
        );
        let ready = next(&server);
        assert!(tool_reply(&ready, "implement")["sha256"].is_string());
        text(&server, r#"{"status":"ready","remaining":[]}"#, true);
        let feedback = next(&server);
        assert!(feedback["messages"]
            .to_string()
            .contains("native_verifier_feedback"));
        text(&server, r#"{"status":"ready","remaining":[]}"#, true);
        child.join().unwrap()
    });
    let result = json_output(output);
    assert_eq!(result["task_success"], true, "{result}");
    let attempts = result["invocation"]["attempts"].as_array().unwrap();
    assert_eq!(attempts.len(), 5);
    assert!(attempts[..3]
        .iter()
        .all(|attempt| attempt["delivery_phase"] == "implementation"
            && attempt["evidence_ids"].as_array().unwrap().is_empty()));
    assert_eq!(
        result["goal"]["accounting"]["counters"]["request_attempts"],
        10
    );
    assert!(server.requests.try_recv().is_err());
}

#[test]
fn milestone_native_empty_continue_gets_model_visible_feedback_before_tools_and_completion() {
    let server = Server::start();
    let fixture = CliFixture::new(&server.endpoint);
    let id = prepare(&fixture, 8);
    let output = thread::scope(|scope| {
        let child = scope.spawn(|| deliver(&fixture, &id, "8"));
        next(&server);
        text(
            &server,
            r#"{"status":"continue","remaining":["Implement the public value"],"summary":"I will read the source next."}"#,
            true,
        );
        let corrective = next(&server);
        assert_eq!(corrective["tool_choice"], "required");
        assert!(corrective["messages"]
            .to_string()
            .contains("Native work-report feedback"));
        assert!(corrective["messages"].as_array().unwrap().iter().any(|message|message["role"]=="assistant"&&message["content"].as_str().is_some_and(|text|text.contains("I will read the source next."))),"correction must include the actual preceding empty report, not an identical task with a generic flag");
        tool(
            &server,
            "observe-after-feedback",
            "read_text",
            json!({"path":"src/lib.rs"}),
        );
        let current = next(&server);
        assert!(
            current.get("tool_choice").is_none(),
            "after a real tool result ordinary completion must remain available"
        );
        let read = tool_reply(&current, "observe-after-feedback");
        tool(
            &server,
            "guarded-after-feedback",
            "edit_text",
            json!({"path":"src/lib.rs","expected_sha256":read["sha256"],"old_text":"    7\n","new_text":"    8\n"}),
        );
        next(&server);
        text(&server, r#"{"status":"ready","remaining":[]}"#, true);
        let verified = next(&server);
        assert!(verified["messages"]
            .to_string()
            .contains("native_verifier_feedback"));
        text(&server, r#"{"status":"ready","remaining":[]}"#, true);
        child.join().unwrap()
    });
    let result = json_output(output);
    assert_eq!(result["task_success"], true, "{result}");
    assert_eq!(
        result["goal"]["accounting"]["counters"]["request_attempts"],
        5
    );
    assert!(server.requests.try_recv().is_err());
}

#[test]
fn milestone_native_multiple_criteria_use_distinct_verifier_runs_and_attestation_is_not_forged() {
    let server = Server::start();
    let fixture = CliFixture::new(&server.endpoint);
    let id = prepare(&fixture, 8);
    let path = fixture.workspace.join("src/lib.rs");
    std::fs::write(
        &path,
        std::fs::read_to_string(&path)
            .unwrap()
            .replace("    7\n", "    8\n"),
    )
    .unwrap();
    json_output(fixture.run(&[
        "goal",
        "criteria",
        "--goal-id",
        &id,
        "--runners",
        "cargo-fmt-check,cargo-test-locked",
        "--owner-attestation-required",
        "--json",
    ]));
    let output = thread::scope(|scope| {
        let child = scope.spawn(|| deliver(&fixture, &id, "8"));
        next(&server);
        text(&server, r#"{"status":"ready","remaining":[]}"#, true);
        let feedback = next(&server);
        assert!(feedback["messages"].to_string().contains("passed"));
        text(&server, r#"{"status":"ready","remaining":[]}"#, true);
        child.join().unwrap()
    });
    let result = json_output(output);
    assert_eq!(result["task_success"], false, "{result}");
    assert_eq!(
        result["invocation"]["stop_reason"],
        "owner_attestation_required"
    );
    assert!(result["goal"]["owner_attestation"].is_null());
    let attempts = result["invocation"]["attempts"].as_array().unwrap();
    assert!(attempts
        .iter()
        .any(|attempt| attempt["stop_reason"] == "scheduled_next_verifier"));
    assert_eq!(
        attempts
            .iter()
            .filter(|attempt| attempt["delivery_phase"] == "verification")
            .count(),
        2
    );
    assert!(server.requests.try_recv().is_err());
}
