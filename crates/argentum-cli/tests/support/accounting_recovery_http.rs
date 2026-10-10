//! Exact executable controls. Trusted local owner commands, no physical-input claims.
use super::instructions_http::Server;
use super::*;

fn parsed(output: Output) -> Value {
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap()
}
fn missing(f: &CliFixture, s: &Server) -> (String, String, String) {
    let goal = parsed(f.run(&[
        "goal",
        "set",
        "--objective",
        "Synthetic scoped recovery only",
        "--token-budget",
        "2000000",
        "--json",
    ]));
    let id = goal["goal"]["id"].as_str().unwrap().to_owned();
    let output = thread::scope(|scope| {
        let run = scope.spawn(|| {
            f.run(&[
                "run",
                "--goal",
                &id,
                "--prompt",
                "Create an accounting uncertainty without tools",
                "--json",
            ])
        });
        let request = s.request();
        assert_eq!(request["stream_options"]["include_usage"], true);
        s.replies.send(json!({"choices":[{"delta":{"content":"Unverified synthetic response"},"finish_reason":"stop"}]})).unwrap();
        run.join().unwrap()
    });
    assert!(!output.status.success());
    let status = parsed(f.run(&["goal", "status", "--json"]));
    assert_eq!(status["execution"]["requests"].as_array().unwrap().len(), 1);
    let charge = &status["execution"]["requests"][0];
    assert_eq!(charge["state"], "unresolved");
    (
        id,
        status["goal"]["session_id"].as_str().unwrap().into(),
        charge["request_id"].as_str().unwrap().into(),
    )
}
fn inspect(f: &CliFixture, goal: &str, session: &str, request: &str) -> Value {
    parsed(f.run(&[
        "accounting",
        "inspect",
        "--goal-id",
        goal,
        "--session-id",
        session,
        "--request-id",
        request,
        "--json",
    ]))
}

#[test]
fn accounting_recovery_exact_cli_no_dispatch_restart_idempotence_and_explicit_same_goal_resume() {
    let server = Server::start();
    let f = CliFixture::new(&server.endpoint);
    let (goal, session, request) = missing(&f, &server);
    let before = inspect(&f, &goal, &session, &request);
    assert_eq!(before["recoverable"], true);
    assert_eq!(before["mutation_performed"], false);
    let bytes = std::fs::read(&f.database).unwrap();
    assert_eq!(inspect(&f, &goal, &session, &request), before);
    assert_eq!(bytes, std::fs::read(&f.database).unwrap());
    let reservation = before["binding"]["reserved_tokens"].as_u64().unwrap();
    let amount = reservation.to_string();
    let decision = json!({"command_id":"exact-owner-resolution","binding":before["binding"],"decision":{"kind":"owner_accept_reservation"}});
    let path = f.root.path().join("resolution.json");
    std::fs::write(&path, decision.to_string()).unwrap();
    let path = path.to_str().unwrap();
    assert!(!f
        .run(&["accounting", "resolve", "--resolution", path, "--json"])
        .status
        .success());
    assert!(!f
        .run(&[
            "accounting",
            "resolve",
            "--resolution",
            path,
            "--confirm-reservation",
            "1",
            "--json"
        ])
        .status
        .success());
    for _ in 0..3 {
        let result = parsed(f.run(&[
            "accounting",
            "resolve",
            "--resolution",
            path,
            "--confirm-reservation",
            &amount,
            "--json",
        ]));
        assert_eq!(result["charge"]["state"], "unresolved");
        assert_eq!(result["charge"]["observed"], Value::Null);
        assert_eq!(
            result["charge"]["disposition"]["client_charge_tokens"],
            reservation
        );
        assert_eq!(result["inference_dispatched"], false);
        assert_eq!(result["verifier_dispatched"], false);
    }
    let after = inspect(&f, &goal, &session, &request);
    assert_eq!(after["ownership"], before["ownership"]);
    assert_eq!(after["run"]["stop_reason"], "goal_usage_unresolved");
    assert_eq!(after["goal_counters"]["owner_accepted_tokens"], reservation);
    assert_eq!(after["goal_counters"]["request_attempts"], 1);
    assert!(server.requests.try_recv().is_err());
    if let Some(old) = std::env::var_os("ARGENTUM_MILESTONE_OLD_CLI_EXE") {
        let template = f.command(&[
            "run",
            "--goal",
            &goal,
            "--prompt",
            "Old executable must not ignore a policy charge",
            "--json",
        ]);
        let mut command = Command::new(old);
        command
            .args(template.get_args())
            .current_dir(f.root.path())
            .env_clear();
        for name in ["SystemRoot", "WINDIR", "SystemDrive", "PATH", "TEMP", "TMP"] {
            if let Some(value) = std::env::var_os(name) {
                command.env(name, value);
            }
        }
        let output = command.output().unwrap();
        assert!(
            !output.status.success(),
            "older executable admitted unsupported policy-charge format"
        );
        assert!(server.requests.try_recv().is_err());
        assert_eq!(
            inspect(&f, &goal, &session, &request)["charge"],
            after["charge"]
        );
    }
    let output = thread::scope(|scope| {
        let run = scope.spawn(|| {
            f.run(&[
                "run",
                "--goal",
                &goal,
                "--prompt",
                "Explicit new task on the same goal, not compaction replay",
                "--json",
            ])
        });
        let request = server.request();
        assert!(request
            .to_string()
            .contains("Explicit new task on the same goal"));
        server.replies.send(json!({"choices":[{"delta":{"content":"new task only"},"finish_reason":"stop"}],"usage":{"total_tokens":40}})).unwrap();
        run.join().unwrap()
    });
    assert!(output.status.success());
    let status = parsed(f.run(&["goal", "status", "--json"]));
    assert_eq!(status["goal"]["id"], goal);
    assert_eq!(
        status["goal"]["accounting"]["counters"]["request_attempts"],
        2
    );
    assert_eq!(
        status["allowance"]["effective_client_charge_tokens"],
        reservation + 40
    );
    assert_eq!(
        status["allowance"]["provider_usage_coverage_complete"],
        false
    );
}

#[test]
fn accounting_recovery_exact_cli_wrong_binding_and_atomic_failure_never_charge_or_dispatch() {
    let server = Server::start();
    let f = CliFixture::new(&server.endpoint);
    let (goal, session, request) = missing(&f, &server);
    let before = inspect(&f, &goal, &session, &request);
    let amount = before["binding"]["reserved_tokens"]
        .as_u64()
        .unwrap()
        .to_string();
    let path = f.root.path().join("resolution.json");
    for key in [
        "store_id",
        "project_id",
        "session_id",
        "goal_id",
        "request_id",
        "run_id",
        "accounting_revision",
        "owner_id",
    ] {
        let mut binding = before["binding"].clone();
        binding[key] = json!("00000000-0000-0000-0000-000000000001");
        std::fs::write(&path,json!({"command_id":"wrong-scope","binding":binding,"decision":{"kind":"owner_accept_reservation"}}).to_string()).unwrap();
        assert!(!f
            .run(&[
                "accounting",
                "resolve",
                "--resolution",
                path.to_str().unwrap(),
                "--confirm-reservation",
                &amount,
                "--json"
            ])
            .status
            .success());
    }
    let db = rusqlite::Connection::open(&f.database).unwrap();
    db.execute_batch("CREATE TRIGGER reject_owner_disposition BEFORE INSERT ON command_identities WHEN NEW.command_kind='accounting_recovery' BEGIN SELECT RAISE(FAIL,'synthetic atomic failure');END;").unwrap();
    drop(db);
    std::fs::write(&path,json!({"command_id":"atomic-failure","binding":before["binding"],"decision":{"kind":"owner_accept_reservation"}}).to_string()).unwrap();
    assert!(!f
        .run(&[
            "accounting",
            "resolve",
            "--resolution",
            path.to_str().unwrap(),
            "--confirm-reservation",
            &amount,
            "--json"
        ])
        .status
        .success());
    assert_eq!(inspect(&f, &goal, &session, &request), before);
    assert!(server.requests.try_recv().is_err());
}

#[test]
fn accounting_recovery_exact_cli_terminal_separate_usage_eof_interruption_invalid_and_conflict() {
    for mode in ["separate", "eof", "interrupted", "malformed", "conflicting"] {
        let server = Server::start();
        let f = CliFixture::new(&server.endpoint);
        let goal = parsed(f.run(&[
            "goal",
            "set",
            "--objective",
            "Transport accounting only",
            "--token-budget",
            "2000000",
            "--json",
        ]));
        let id = goal["goal"]["id"].as_str().unwrap();
        let output = thread::scope(|scope| {
            let run = scope.spawn(|| {
                f.run(&[
                    "run",
                    "--goal",
                    id,
                    "--prompt",
                    "Observe final transport metadata",
                    "--json",
                ])
            });
            let request = server.request();
            assert_eq!(request["stream_options"]["include_usage"], true);
            let mut frames =
                vec![json!({"choices":[{"delta":{"content":"content"},"finish_reason":"stop"}]})];
            match mode {
                "separate"=>frames.push(json!({"choices":[],"usage":{"prompt_tokens":20,"completion_tokens":3,"total_tokens":23}})),
                "conflicting"=>{frames.push(json!({"choices":[],"usage":{"total_tokens":23}}));frames.push(json!({"choices":[],"usage":{"total_tokens":24}}));},
                "malformed"=>frames.push(json!({"choices":[],"usage":{"prompt_tokens":20,"completion_tokens":3,"total_tokens":2}})),
                "interrupted"=>{server.replies.send(json!({"fixture_partial":"partial"})).unwrap();thread::sleep(Duration::from_millis(20));server.replies.send(json!({"fixture_disconnect":true})).unwrap();return run.join().unwrap();},_=>{}
            }
            server
                .replies
                .send(json!({"fixture_frames":frames}))
                .unwrap();
            run.join().unwrap()
        });
        let status = parsed(f.run(&["goal", "status", "--json"]));
        let charge = &status["execution"]["requests"][0];
        if mode == "separate" {
            assert!(output.status.success());
            assert_eq!(charge["observed"]["total_tokens"], 23);
        } else {
            assert!(!output.status.success());
            assert_eq!(charge["state"], "unresolved");
            assert_eq!(charge["observed"], Value::Null);
            assert!(
                status["goal"]["accounting"]["counters"]["unresolved_tokens"]
                    .as_u64()
                    .unwrap()
                    > 0
            );
        }
        assert!(server.requests.try_recv().is_err());
    }
}

#[test]
fn accounting_recovery_exact_cli_reconciles_only_matching_retained_provider_evidence() {
    let server = Server::start();
    let f = CliFixture::new(&server.endpoint);
    let goal = parsed(f.run(&[
        "goal",
        "set",
        "--objective",
        "Synthetic persistence failure",
        "--token-budget",
        "2000000",
        "--json",
    ]));
    let id = goal["goal"]["id"].as_str().unwrap();
    let session = goal["goal"]["session_id"].as_str().unwrap();
    let db = rusqlite::Connection::open(&f.database).unwrap();
    db.execute_batch("CREATE TRIGGER reject_actual_settlement BEFORE UPDATE ON goal_requests WHEN json_extract(NEW.payload,'$.state')='settled' BEGIN SELECT RAISE(FAIL,'synthetic persistence failure');END;").unwrap();
    drop(db);
    let output = thread::scope(|scope| {
        let run = scope.spawn(|| {
            f.run(&[
                "run",
                "--goal",
                id,
                "--prompt",
                "Retain terminal usage without replaying work",
                "--json",
            ])
        });
        server.request();
        server.replies.send(json!({"fixture_frames":[{"choices":[{"delta":{"content":"terminal content"},"finish_reason":"stop"}]},{"choices":[],"usage":{"prompt_tokens":37,"completion_tokens":3,"total_tokens":40}}]})).unwrap();
        run.join().unwrap()
    });
    assert!(!output.status.success());
    let db = rusqlite::Connection::open(&f.database).unwrap();
    let (event_id,payload):(u64,String)=db.query_row("SELECT id,payload FROM event_log WHERE event_type='goal_request_accounting' ORDER BY id DESC LIMIT 1",[],|r|Ok((r.get(0)?,r.get(1)?))).unwrap();
    let pending: Value = serde_json::from_str(&payload).unwrap();
    let request = pending["GoalRequestAccounting"]["charge"]["request_id"]
        .as_str()
        .unwrap();
    assert_eq!(
        pending["GoalRequestAccounting"]["charge"]["observed"]["total_tokens"],
        40
    );
    let before = inspect(&f, id, session, request);
    let mut decision = json!({"command_id":"owner-evidence-reconciliation","binding":before["binding"],"decision":{"kind":"authoritative_usage","event_id":event_id,"usage":pending["GoalRequestAccounting"]["charge"]["observed"]}});
    let path = f.root.path().join("evidence.json");
    decision["decision"]["event_id"] = json!(event_id + 1);
    std::fs::write(&path, decision.to_string()).unwrap();
    assert!(!f
        .run(&[
            "accounting",
            "resolve",
            "--resolution",
            path.to_str().unwrap(),
            "--json"
        ])
        .status
        .success());
    decision["decision"]["event_id"] = json!(event_id);
    decision["decision"]["usage"]["total_tokens"] = json!(41);
    std::fs::write(&path, decision.to_string()).unwrap();
    assert!(!f
        .run(&[
            "accounting",
            "resolve",
            "--resolution",
            path.to_str().unwrap(),
            "--json"
        ])
        .status
        .success());
    decision["decision"]["usage"]["total_tokens"] = json!(40);
    std::fs::write(&path, decision.to_string()).unwrap();
    let settled = parsed(f.run(&[
        "accounting",
        "resolve",
        "--resolution",
        path.to_str().unwrap(),
        "--json",
    ]));
    assert_eq!(settled["charge"]["observed"], Value::Null);
    assert_eq!(
        settled["charge"]["disposition"]["later_observed_usage"]["total_tokens"],
        40
    );
    let after = inspect(&f, id, session, request);
    assert_eq!(after["goal_counters"]["observed_tokens"], 40);
    assert_eq!(after["goal_counters"]["owner_accepted_tokens"], 0);
    assert_eq!(after["goal_counters"]["outstanding_tokens"], 0);
    assert_eq!(after["ownership"], before["ownership"]);
    assert!(server.requests.try_recv().is_err());
}
