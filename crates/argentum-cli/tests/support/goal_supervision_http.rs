//! F07B through the rebuilt CLI, loopback HTTP adapter, and real Cargo formatter.
use super::instructions_http::Server;
use super::*;

fn parse_json(output: &Output) -> Value {
    assert!(
        output.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).expect("JSON output")
}

fn select_owner_full_access(fixture: &CliFixture) {
    tokio::runtime::Runtime::new().unwrap().block_on(async {
        let mut config = argentum_cli::HostConfig::in_memory(&fixture.workspace);
        config.database = Some(fixture.database.clone());
        let mut host = argentum_cli::CommandHost::start(config).expect("owner command host");
        host.local_handle()
            .dispatch(AppCommand::SelectExecutionProfile {
                profile_id: "full-access".into(),
            })
            .await
            .expect("owner-selected full access fixture policy");
        host.shutdown().await.expect("owner host shutdown");
    });
}

fn reply_text_with_usage(server: &Server, text: &str, total_tokens: u64) {
    server
        .replies
        .send(json!({
            "choices":[{"delta":{"content":text},"finish_reason":"stop"}],
            "usage":{"total_tokens":total_tokens}
        }))
        .expect("text reply");
}

#[test]
fn f07b_ordinary_requests_refresh_passed_then_stale_fixed_verification() {
    let server = Server::start();
    let fixture = CliFixture::new(&server.endpoint);
    std::fs::create_dir(fixture.workspace.join("src")).unwrap();
    std::fs::write(
        fixture.workspace.join("Cargo.toml"),
        "[package]\nname = \"current-evidence-fixture\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
    )
    .unwrap();
    let original = "pub fn answer() -> u32 {\n    42\n}\n";
    std::fs::write(fixture.workspace.join("src/lib.rs"), original).unwrap();
    select_owner_full_access(&fixture);
    let set = parse_json(&fixture.run(&[
        "goal",
        "set",
        "--objective",
        "Observe the current fixed check",
        "--token-budget",
        "500000",
        "--json",
    ]));
    let id = set["goal"]["id"].as_str().unwrap().to_owned();
    parse_json(&fixture.run(&[
        "goal",
        "criteria",
        "--goal-id",
        &id,
        "--runners",
        "cargo-fmt-check",
        "--json",
    ]));
    let mut guidance = Vec::new();
    let output = thread::scope(|scope| {
        let process = scope.spawn(|| {
            fixture.run(&[
                "run",
                "--goal",
                &id,
                "--prompt",
                "Observe check freshness",
                "--json",
            ])
        });
        for round in 0..4 {
            let request = server.request();
            let current = request["messages"]
                .as_array()
                .unwrap()
                .iter()
                .filter_map(|message| {
                    serde_json::from_str::<Value>(message["content"].as_str()?).ok()
                })
                .find(|value| value["kind"] == "explicit_goal_task_guidance");
            guidance.push(current);
            if round == 3 {
                reply_text_with_usage(
                    &server,
                    "The declared fixed check passed on the final revision.",
                    10,
                );
            } else {
                let (name, arguments) = if round == 1 {
                    (
                        "write_text",
                        json!({"path":"src/lib.rs","content":format!("{original}// Changed after the earlier check.\n")}),
                    )
                } else {
                    ("verify", json!({"runners":["cargo-fmt-check"]}))
                };
                server
                    .replies
                    .send(json!({"choices":[{"delta":{"tool_calls":[{
                    "index":0,"id":format!("check-{round}"),"type":"function",
                    "function":{"name":name,"arguments":arguments.to_string()}
                }]},"finish_reason":"tool_calls"}],"usage":{"total_tokens":10}}))
                    .unwrap();
            }
        }
        process.join().unwrap()
    });
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stdout)
    );
    let states: Vec<_> = guidance
        .iter()
        .map(|value| {
            value.as_ref().unwrap()["verification"]["current_criteria"][0]["state"].clone()
        })
        .collect();
    assert_eq!(
        states,
        vec![
            json!("unknown"),
            json!("passed"),
            json!("stale"),
            json!("passed")
        ]
    );
    assert!(guidance
        .iter()
        .all(|value| value.as_ref().unwrap()["verification"]["scope"]
            == "declared_fixed_checks_only"));
    assert!(server.requests.try_recv().is_err());
}

#[test]
fn f07b_public_cli_runs_real_fail_repair_pass_and_reopens_as_current_status() {
    let server = Server::start();
    let fixture = CliFixture::new(&server.endpoint);
    std::fs::create_dir(fixture.workspace.join("src")).expect("source directory");
    std::fs::write(
        fixture.workspace.join("Cargo.toml"),
        "[package]\nname = \"f07b-cli-fixture\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
    )
    .expect("manifest");
    std::fs::write(
        fixture.workspace.join("src/lib.rs"),
        "pub fn answer()->u32{42}\n",
    )
    .expect("unformatted source");
    std::fs::write(fixture.workspace.join("unrelated.txt"), "preserve me\n")
        .expect("unrelated source");
    let git = |arguments: &[&str]| {
        Command::new("git")
            .args(arguments)
            .current_dir(&fixture.workspace)
            .output()
            .expect("git fixture command")
    };
    assert!(git(&["init", "--quiet"]).status.success());
    assert!(git(&["add", "fixture.txt", "unrelated.txt"])
        .status
        .success());
    let index_before = std::fs::read(fixture.workspace.join(".git/index")).expect("index");

    select_owner_full_access(&fixture);
    let set = parse_json(&fixture.run(&[
        "goal",
        "set",
        "--objective",
        "Format src/lib.rs with the declared Cargo formatter",
        "--token-budget",
        "1000000",
        "--tool-budget",
        "8",
        "--time-budget",
        "600",
        "--json",
    ]));
    let goal_id = set["goal"]["id"]
        .as_str()
        .expect("goal identity")
        .to_owned();
    let criteria = parse_json(&fixture.run(&[
        "goal",
        "criteria",
        "--goal-id",
        &goal_id,
        "--runners",
        "cargo-fmt-check",
        "--json",
    ]));
    assert_eq!(criteria["command_status"], "accepted");
    assert_eq!(
        criteria["goal"]["acceptance"]["criteria"][0]["runner"],
        "cargo-fmt-check"
    );

    let output = thread::scope(|scope| {
        let process = scope.spawn(|| {
            fixture.run(&[
                "goal",
                "supervise",
                "--goal-id",
                &goal_id,
                "--iterations",
                "3",
                "--elapsed-seconds",
                "120",
                "--json",
            ])
        });

        let first = server.request();
        assert!(first["messages"]
            .to_string()
            .contains("Format src/lib.rs with the declared Cargo formatter"));
        reply_text_with_usage(&server, "The formatting objective is complete.", 6);

        let continuation = server.request();
        let continuation_messages = continuation["messages"].to_string();
        assert!(continuation_messages.contains("The formatting objective is complete."));
        assert!(continuation_messages.contains("bounded_goal_continuation"));
        assert!(continuation_messages.contains("current_failure"));
        server
            .replies
            .send(json!({
                "choices":[{"delta":{"tool_calls":[{
                    "index":0,
                    "id":"f07b-format-source",
                    "type":"function",
                    "function":{
                        "name":"write_text",
                        "arguments":json!({
                            "path":"src/lib.rs",
                            "content":"pub fn answer() -> u32 {\n    42\n}\n"
                        }).to_string()
                    }
                }]},"finish_reason":"tool_calls"}],
                "usage":{"total_tokens":6}
            }))
            .expect("repair tool reply");

        let after_tool = server.request();
        assert!(after_tool["messages"]
            .as_array()
            .expect("messages")
            .iter()
            .any(|message| message["role"] == "tool"
                && message["tool_call_id"] == "f07b-format-source"));
        reply_text_with_usage(&server, "Formatting repair is settled.", 6);
        process.join().expect("supervision process")
    });

    let settled = parse_json(&output);
    assert_eq!(settled["command_status"], "accepted_and_settled");
    assert_eq!(settled["task_success"], true);
    assert!(settled.get("owner_claim_id").is_none());
    assert!(settled["invocation"].get("owner_claim_id").is_none());
    assert_eq!(settled["invocation"]["state"], "verified");
    let first_invocation_id = settled["invocation"]["id"]
        .as_str()
        .expect("first invocation identity")
        .to_owned();
    assert_eq!(
        settled["invocation"]["stop_reason"],
        "verified_against_declared_criteria"
    );
    let attempts = settled["invocation"]["attempts"]
        .as_array()
        .expect("attempts");
    assert_eq!(attempts.len(), 2);
    assert_eq!(attempts[0]["outcome"], "criteria_failed");
    assert_eq!(attempts[1]["outcome"], "verified");
    assert_ne!(attempts[0]["run_id"], attempts[1]["run_id"]);
    assert_eq!(
        std::fs::read_to_string(fixture.workspace.join("src/lib.rs")).expect("source"),
        "pub fn answer() -> u32 {\n    42\n}\n"
    );
    assert_eq!(
        std::fs::read_to_string(fixture.workspace.join("unrelated.txt")).expect("unrelated"),
        "preserve me\n"
    );
    assert_eq!(
        std::fs::read(fixture.workspace.join(".git/index")).expect("index"),
        index_before
    );

    let status = parse_json(&fixture.run(&["goal", "status", "--json"]));
    assert_eq!(status["goal"]["lifecycle"], "complete");
    assert_eq!(status["goal"]["accounting"]["counters"]["iterations"], 2);
    assert_eq!(
        status["goal"]["accounting"]["counters"]["request_attempts"],
        3
    );
    assert_eq!(
        status["goal"]["accounting"]["counters"]["tool_admissions"],
        3
    );
    assert_eq!(status["supervision"]["completion_current"], true);
    assert_eq!(
        status["supervision"]["completion_status"],
        "verified_against_declared_criteria"
    );
    assert!(!status.to_string().contains("owner_claim_id"));
    assert!(server.requests.try_recv().is_err());

    let resumed = parse_json(&fixture.run(&["goal", "resume", "--json"]));
    assert_eq!(resumed["goal"]["lifecycle"], "active");
    assert!(server.requests.try_recv().is_err());

    let revalidated = parse_json(&fixture.run(&[
        "goal",
        "supervise",
        "--goal-id",
        &goal_id,
        "--iterations",
        "1",
        "--elapsed-seconds",
        "60",
        "--json",
    ]));
    assert_eq!(revalidated["command_status"], "accepted_and_settled");
    assert_eq!(revalidated["task_success"], true);
    assert_eq!(revalidated["invocation"]["state"], "verified");
    assert_ne!(revalidated["invocation"]["id"], first_invocation_id);
    assert_eq!(
        revalidated["invocation"]["attempts"]
            .as_array()
            .expect("revalidation attempts")
            .len(),
        0
    );
    assert!(server.requests.try_recv().is_err());

    let restarted_status = parse_json(&fixture.run(&["goal", "status", "--json"]));
    assert_eq!(restarted_status["goal"]["lifecycle"], "complete");
    assert_eq!(
        restarted_status["goal"]["accounting"]["counters"]["iterations"],
        2
    );
    assert_eq!(
        restarted_status["goal"]["accounting"]["counters"]["request_attempts"],
        3
    );
    assert_eq!(
        restarted_status["goal"]["accounting"]["counters"]["tool_admissions"],
        3
    );
    assert_eq!(
        restarted_status["supervision"]["invocation"]["id"],
        revalidated["invocation"]["id"]
    );
    assert_eq!(restarted_status["supervision"]["completion_current"], true);
    assert!(server.requests.try_recv().is_err());

    let database = rusqlite::Connection::open(&fixture.database).expect("read-only evidence check");
    let mut statement = database
        .prepare("SELECT payload FROM verification_evidence ORDER BY rowid")
        .expect("evidence statement");
    let evidence = statement
        .query_map([], |row| row.get::<_, String>(0))
        .expect("evidence rows")
        .map(|payload| serde_json::from_str::<Value>(&payload.expect("payload")).expect("evidence"))
        .collect::<Vec<_>>();
    assert_eq!(evidence.len(), 2);
    assert_eq!(evidence[0]["state"], "failed");
    assert_eq!(evidence[1]["state"], "passed");
    assert_ne!(evidence[0]["id"], evidence[1]["id"]);
}

#[test]
fn f07b_supervised_automatic_compaction_is_charged_and_cannot_invent_verification() {
    use super::working_context_http::compaction_http::{
        context, seed, summary_for, summary_request,
    };

    let server = Server::start();
    let fixture = CliFixture::new(&server.endpoint);
    std::fs::create_dir(fixture.workspace.join("src")).expect("source directory");
    std::fs::write(
        fixture.workspace.join("Cargo.toml"),
        "[package]\nname = \"f07b-compaction-fixture\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
    )
    .expect("manifest");
    std::fs::write(
        fixture.workspace.join("src/lib.rs"),
        "pub fn answer()->u32{42}\n",
    )
    .expect("unformatted source");
    seed(&server, &fixture, 1_800);
    select_owner_full_access(&fixture);
    let set = parse_json(&fixture.run(&[
        "goal",
        "set",
        "--objective",
        "Format src/lib.rs with the declared Cargo formatter",
        "--token-budget",
        "500000",
        "--tool-budget",
        "8",
        "--time-budget",
        "600",
        "--json",
    ]));
    let goal_id = set["goal"]["id"]
        .as_str()
        .expect("goal identity")
        .to_owned();
    parse_json(&fixture.run(&[
        "goal",
        "criteria",
        "--goal-id",
        &goal_id,
        "--runners",
        "cargo-fmt-check",
        "--json",
    ]));
    let input = context(&fixture)["accounting"]["accounted_input"]
        .as_u64()
        .expect("accounted input");
    let total = input + 4096 + 1024 - 1000;
    parse_json(&fixture.run(&[
        "provider",
        "budget",
        "f01-fixture",
        "--limits",
        &format!("{total},{total},8192,4096,1024"),
        "--json",
    ]));

    let output = thread::scope(|scope| {
        let process = scope.spawn(|| {
            fixture.run(&[
                "goal",
                "supervise",
                "--goal-id",
                &goal_id,
                "--iterations",
                "1",
                "--elapsed-seconds",
                "120",
                "--json",
            ])
        });
        let summary = summary_request(&server);
        server
            .replies
            .send(json!({
                "choices":[{"delta":{"content":summary_for(&summary).to_string()},"finish_reason":"stop"}],
                "usage":{"total_tokens":75}
            }))
            .expect("summary reply");
        let main = server.request();
        assert!(main.to_string().contains("bounded_goal_continuation"));
        reply_text_with_usage(&server, "Model prose does not establish verification.", 25);
        process.join().expect("supervision process")
    });
    let settled = parse_json(&output);
    assert_eq!(settled["task_success"], false);
    assert_eq!(settled["invocation"]["state"], "incomplete");
    assert_eq!(settled["invocation"]["stop_reason"], "iteration_limit");
    assert_eq!(
        settled["invocation"]["evaluated_criteria"][0]["state"],
        "failed"
    );
    assert_eq!(settled["goal"]["accounting"]["counters"]["iterations"], 1);
    assert_eq!(
        settled["goal"]["accounting"]["counters"]["request_attempts"],
        2
    );
    assert_eq!(
        settled["goal"]["accounting"]["counters"]["observed_tokens"],
        100
    );
    assert_eq!(
        settled["goal"]["accounting"]["counters"]["tool_admissions"],
        1
    );
    assert!(settled["goal"]["completion"].is_null());
    assert!(server.requests.try_recv().is_err());
}

#[test]
fn f07b_empty_or_unsupported_criteria_fail_without_removing_required_checks() {
    let server = Server::start();
    let fixture = CliFixture::new(&server.endpoint);
    let set = parse_json(&fixture.run(&[
        "goal",
        "set",
        "--objective",
        "Formatting requires declared typed evidence",
        "--token-budget",
        "500000",
        "--tool-budget",
        "8",
        "--time-budget",
        "600",
        "--json",
    ]));
    let goal_id = set["goal"]["id"]
        .as_str()
        .expect("goal identity")
        .to_owned();
    let unconfigured = parse_json(&fixture.run(&["goal", "status", "--json"]));
    assert_eq!(
        unconfigured["supervision"]["completion_status"],
        "acceptance_criteria_missing"
    );
    assert_eq!(unconfigured["supervision"]["current_criteria"], json!([]));
    let missing = fixture.run(&[
        "goal",
        "supervise",
        "--goal-id",
        &goal_id,
        "--iterations",
        "1",
        "--elapsed-seconds",
        "60",
        "--json",
    ]);
    assert!(!missing.status.success());
    assert!(String::from_utf8_lossy(&missing.stderr).contains("acceptance_criteria_missing"));
    for runners in ["", "arbitrary-shell"] {
        let rejected = fixture.run(&[
            "goal",
            "criteria",
            "--goal-id",
            &goal_id,
            "--runners",
            runners,
            "--json",
        ]);
        assert!(!rejected.status.success());
    }
    parse_json(&fixture.run(&[
        "goal",
        "criteria",
        "--goal-id",
        &goal_id,
        "--runners",
        "cargo-fmt-check",
        "--json",
    ]));
    let removal = fixture.run(&[
        "goal",
        "criteria",
        "--goal-id",
        &goal_id,
        "--runners",
        "",
        "--json",
    ]);
    assert!(!removal.status.success());
    let status = parse_json(&fixture.run(&["goal", "status", "--json"]));
    assert_eq!(
        status["goal"]["acceptance"]["criteria"]
            .as_array()
            .expect("criteria")
            .len(),
        1
    );
    assert_eq!(
        status["goal"]["acceptance"]["criteria"][0]["runner"],
        "cargo-fmt-check"
    );
    assert!(server.requests.try_recv().is_err());
}
