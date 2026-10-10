//! Synthetic usage through the exact executable and production text adapter.
use super::instructions_http::Server;
use super::*;

fn goal(fixture: &CliFixture) -> Value {
    let output = fixture.run(&["goal", "status", "--json"]);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice::<Value>(&output.stdout).unwrap()["goal"].clone()
}

fn set_goal(fixture: &CliFixture, budget: &str) -> String {
    let output = fixture.run(&[
        "goal",
        "set",
        "--objective",
        "F07A explicit scoped objective",
        "--token-budget",
        budget,
        "--json",
    ]);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    goal(fixture)["id"].as_str().unwrap().to_owned()
}

fn terminal(output: &Output) -> Value {
    serde_json::from_str(
        String::from_utf8_lossy(&output.stdout)
            .lines()
            .last()
            .unwrap(),
    )
    .unwrap()
}

fn limits(fixture: &CliFixture, id: &str, values: &str) {
    let output = fixture.run(&[
        "goal",
        "limits",
        "--goal-id",
        id,
        "--limits",
        values,
        "--json",
    ]);
    assert!(
        output.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn f07a_storage_failures_never_dispatch_without_reservation_or_replay_paid_work() {
    for after_consumption in [false, true] {
        let server = Server::start();
        let fixture = CliFixture::new(&server.endpoint);
        let id = set_goal(&fixture, "500000");
        let db = rusqlite::Connection::open(&fixture.database).unwrap();
        // Same SQLite trigger fault mechanism as the existing checkpoint tests.
        // Only disposable failure behavior is injected, never goal authority.
        if after_consumption {
            db.execute_batch("CREATE TRIGGER reject_settlement BEFORE UPDATE ON goal_requests WHEN json_extract(NEW.payload, '$.state') = 'settled' BEGIN SELECT RAISE(FAIL, 'synthetic settlement persistence failure'); END;").unwrap();
        } else {
            db.execute_batch("CREATE TRIGGER reject_reservation BEFORE INSERT ON goal_requests BEGIN SELECT RAISE(FAIL, 'synthetic reservation persistence failure'); END;").unwrap();
        }
        drop(db);
        let output = if after_consumption {
            thread::scope(|scope| {
                let process = scope.spawn(|| {
                    fixture.run(&[
                        "run",
                        "--goal",
                        &id,
                        "--prompt",
                        "Persist known consumption",
                        "--json",
                    ])
                });
                server.request();
                server.replies.send(json!({"choices":[{"delta":{"content":"paid response"},"finish_reason":"stop"}],"usage":{"total_tokens":40}})).unwrap();
                process.join().unwrap()
            })
        } else {
            fixture.run(&[
                "run",
                "--goal",
                &id,
                "--prompt",
                "Must reserve before dispatch",
                "--json",
            ])
        };
        assert!(!output.status.success());
        assert!(server.requests.try_recv().is_err());
        let stored = goal(&fixture);
        let counters = &stored["accounting"]["counters"];
        assert_eq!(counters["observed_tokens"], 0);
        if after_consumption {
            assert_eq!(terminal(&output)["code"], "goal_accounting_unresolved");
            assert!(counters["outstanding_tokens"].as_u64().unwrap() > 0);
            let events: Vec<Value> = String::from_utf8_lossy(&output.stdout)
                .lines()
                .map(|line| serde_json::from_str(line).unwrap())
                .collect();
            assert!(
                events
                    .iter()
                    .any(
                        |event| event["event"]["GoalRequestAccounting"]["charge"]["observed"]
                            ["total_tokens"]
                            == 40
                    ),
                "known usage evidence missing"
            );
            assert!(!fixture
                .run(&[
                    "run",
                    "--goal",
                    &id,
                    "--prompt",
                    "Do not replay lost settlement",
                    "--json"
                ])
                .status
                .success());
            assert!(server.requests.try_recv().is_err());
        } else {
            assert_eq!(counters["request_attempts"], 0);
            assert_eq!(counters["outstanding_tokens"], 0);
        }
    }
}

#[test]
fn f07a_truncation_settles_later_usage_without_retry_or_partial_tool_execution() {
    for reported in [false, true] {
        let server = Server::start();
        let fixture = CliFixture::new(&server.endpoint);
        let id = set_goal(&fixture, "500000");
        let output = thread::scope(|scope| {
            let process = scope.spawn(|| {
                fixture.run(&[
                    "run",
                    "--goal",
                    &id,
                    "--prompt",
                    "Account for a truncated response",
                    "--json",
                ])
            });
            server.request();
            let mut frames = vec![json!({"choices":[{
                "delta":{"tool_calls":[{"index":0,"id":"partial","type":"function",
                    "function":{"name":"write_text","arguments":"{\"path\":\"partial.txt\","}}]},
                "finish_reason":"length"
            }]})];
            if reported {
                frames.push(json!({"choices":[],"usage":{
                    "prompt_tokens":12,"completion_tokens":28,"total_tokens":40
                }}));
            }
            server
                .replies
                .send(json!({"fixture_frames":frames}))
                .unwrap();
            process.join().unwrap()
        });
        assert!(!output.status.success());
        assert!(terminal(&output)["message"]
            .as_str()
            .unwrap()
            .contains("response truncated"));
        assert!(
            server.requests.try_recv().is_err(),
            "truncation is not a retry"
        );
        assert!(!fixture.workspace.join("partial.txt").exists());
        let stored = goal(&fixture);
        let counters = &stored["accounting"]["counters"];
        assert_eq!(counters["request_attempts"], 1);
        assert_eq!(counters["tool_admissions"], 0);
        if reported {
            assert_ne!(terminal(&output)["code"], "goal_usage_unresolved");
            assert_eq!(counters["observed_tokens"], 40);
            assert_eq!(counters["outstanding_tokens"], 0);
        } else {
            assert_eq!(terminal(&output)["code"], "goal_usage_unresolved");
            assert_eq!(counters["observed_tokens"], 0);
            assert!(counters["unresolved_tokens"].as_u64().unwrap() > 0);
        }
    }
}

#[test]
fn f07a_provider_failure_with_unknown_usage_keeps_cause_and_reservation() {
    let server = Server::start();
    let fixture = CliFixture::new(&server.endpoint);
    let id = set_goal(&fixture, "500000");
    let output = thread::scope(|scope| {
        let process = scope.spawn(|| {
            fixture.run(&[
                "run",
                "--goal",
                &id,
                "--prompt",
                "Preserve the provider failure and unknown usage",
                "--json",
            ])
        });
        server.request();
        server
            .replies
            .send(json!({
                "fixture_http_failure": true,
                "fixture_http_body": {
                    "error": {
                        "code": "quota_exhausted",
                        "message": "Weekly quota exhausted",
                        "request_id": "req-goal-503"
                    },
                    "ignored": "not retained"
                }
            }))
            .unwrap();
        process.join().unwrap()
    });

    assert!(!output.status.success());
    let final_event = terminal(&output);
    assert_eq!(final_event["code"], "goal_usage_unresolved");
    assert!(final_event["message"]
        .as_str()
        .is_some_and(
            |message| message.contains("provider returned retryable HTTP 503")
                && message.contains("code=quota_exhausted")
                && message.contains("message=Weekly quota exhausted")
                && message.contains("request_id=req-goal-503")
                && !message.contains("ignored")
                && message.contains("goal_usage_unresolved")
        ));

    let events = String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(|line| serde_json::from_str::<Value>(line).unwrap())
        .collect::<Vec<_>>();
    assert!(events.iter().any(|event| {
        let attempt = &event["event"]["ModelRequestAttempt"];
        attempt["outcome"] == "stopped"
            && attempt["error_class"] == "transient_http_status"
            && attempt["status"] == 503
            && attempt["stop_reason"] == "usage_unresolved_after_provider_error"
    }));

    let stored = goal(&fixture);
    assert_eq!(stored["accounting"]["counters"]["request_attempts"], 1);
    assert_eq!(stored["accounting"]["counters"]["outstanding_tokens"], 0);
    assert!(stored["accounting"]["counters"]["unresolved_tokens"]
        .as_u64()
        .is_some_and(|tokens| tokens > 0));
    assert!(server.requests.try_recv().is_err());
}

#[test]
fn f07a_tool_admissions_are_bounded_and_guided_writes_remain_unapproved() {
    for denied in [false, true] {
        let server = Server::start();
        let fixture = CliFixture::new(&server.endpoint);
        let id = set_goal(&fixture, "500000");
        limits(&fixture, &id, "500000,100,100,1,600,500000,100,1,60");
        if !denied {
            // Policy elevation belongs to the trusted local owner, not CLI ingress.
            tokio::runtime::Runtime::new().unwrap().block_on(async {
                let mut config = argentum_cli::HostConfig::in_memory(&fixture.workspace);
                config.database = Some(fixture.database.clone());
                let mut host = argentum_cli::CommandHost::start(config).unwrap();
                host.local_handle()
                    .dispatch(AppCommand::SelectExecutionProfile {
                        profile_id: "automatic".into(),
                    })
                    .await
                    .unwrap();
                host.shutdown().await.unwrap();
            });
        }
        let output = thread::scope(|scope| {
            let process = scope.spawn(|| {
                fixture.run(&[
                    "run",
                    "--goal",
                    &id,
                    "--prompt",
                    "Exercise the authorized tool boundary",
                    "--json",
                ])
            });
            server.request();
            let (name, arguments) = if denied {
                (
                    "write_text",
                    json!({"path":"must-not-exist.txt","content":"not authorized"}),
                )
            } else {
                (
                    "write_text",
                    json!({"path":"permitted.txt","content":"earlier permitted effect"}),
                )
            };
            server.replies.send(json!({"choices":[{"delta":{"tool_calls":[{"index":0,"id":"first-budget-tool","type":"function","function":{"name":name,"arguments":arguments.to_string()}}]},"finish_reason":"tool_calls"}],"usage":{"total_tokens":40}})).unwrap();
            if !denied {
                let request = server.request();
                assert!(request.to_string().contains("first-budget-tool"));
                assert_eq!(
                    std::fs::read_to_string(fixture.workspace.join("permitted.txt")).unwrap(),
                    "earlier permitted effect"
                );
                server.replies.send(json!({"choices":[{"delta":{"tool_calls":[{"index":0,"id":"second-budget-tool","type":"function","function":{"name":"write_text","arguments":"{\"path\":\"must-not-exist.txt\",\"content\":\"must not execute\"}"}}]},"finish_reason":"tool_calls"}],"usage":{"total_tokens":40}})).unwrap();
            }
            process.join().unwrap()
        });
        assert!(!output.status.success());
        assert_eq!(
            terminal(&output)["code"],
            if denied {
                "approval_required"
            } else {
                "goal_tool_limit"
            },
            "{}",
            String::from_utf8_lossy(&output.stdout)
        );
        assert_eq!(goal(&fixture)["tools_used"], u32::from(!denied));
        assert_eq!(goal(&fixture)["tokens_used"], if denied { 40 } else { 80 });
        assert!(!fixture.workspace.join("must-not-exist.txt").exists());
        if !denied {
            assert_eq!(
                std::fs::read_to_string(fixture.workspace.join("permitted.txt")).unwrap(),
                "earlier permitted effect"
            );
            let db = rusqlite::Connection::open(&fixture.database).unwrap();
            let receipts: u32 = db
                .query_row("SELECT COUNT(*) FROM execution_audit", [], |row| row.get(0))
                .unwrap();
            assert_eq!(receipts, 1);
        }
        assert!(server.requests.try_recv().is_err());
    }
}

#[test]
fn f07a_stream_and_summary_deadlines_hold_unknown_usage_without_recharge() {
    use super::working_context_http::compaction_http::{seed, summary_request};
    for summary in [false, true] {
        let server = Server::start();
        let fixture = CliFixture::new(&server.endpoint);
        if summary {
            seed(&server, &fixture, 1800);
        }
        let id = set_goal(&fixture, "500000");
        limits(&fixture, &id, "500000,100,100,100,600,500000,100,100,1");
        let started = std::time::Instant::now();
        let output = thread::scope(|scope| {
            let process = scope.spawn(|| {
                if summary {
                    fixture.run(&["compact", "--goal", &id, "--json"])
                } else {
                    fixture.run(&[
                        "run",
                        "--goal",
                        &id,
                        "--prompt",
                        "Deadline during streaming",
                        "--json",
                    ])
                }
            });
            if summary {
                summary_request(&server);
            } else {
                server.request();
            }
            server
                .replies
                .send(json!({"fixture_partial":"unfinished"}))
                .unwrap();
            // The executable must settle while the server is still holding its
            // response. Fixture watchdog termination is never a passing result.
            let output = process.join().unwrap();
            server
                .replies
                .send(json!({"fixture_disconnect":true}))
                .unwrap();
            output
        });
        assert!(started.elapsed() < Duration::from_secs(10));
        assert!(!output.status.success());
        assert_eq!(
            terminal(&output)["code"],
            "goal_deadline_exceeded",
            "{}",
            String::from_utf8_lossy(&output.stdout)
        );
        let stored = goal(&fixture);
        assert_eq!(stored["tokens_used"], 0);
        assert!(
            stored["accounting"]["counters"]["unresolved_tokens"]
                .as_u64()
                .unwrap()
                > 0
        );
        assert_eq!(
            stored["accounting"]["stop_reason"],
            "goal_deadline_exceeded"
        );
        assert_eq!(goal(&fixture), stored);
        assert!(!fixture
            .run(&[
                "run",
                "--goal",
                &id,
                "--prompt",
                "Uncertain usage is not refunded",
                "--json"
            ])
            .status
            .success());
        assert!(server.requests.try_recv().is_err());
    }
}

#[test]
fn f07a_manual_and_automatic_compaction_are_charged_even_for_invalid_summary() {
    use super::working_context_http::compaction_http::{
        context, seed, summary_for, summary_request,
    };
    for automatic in [false, true] {
        let server = Server::start();
        let fixture = CliFixture::new(&server.endpoint);
        seed(&server, &fixture, 1800);
        let id = set_goal(&fixture, "500000");
        if automatic {
            let input = context(&fixture)["accounting"]["accounted_input"]
                .as_u64()
                .unwrap();
            let total = input + 4096 + 1024 - 1000;
            assert!(fixture
                .run(&[
                    "provider",
                    "budget",
                    "f01-fixture",
                    "--limits",
                    &format!("{total},{total},8192,4096,1024"),
                    "--json"
                ])
                .status
                .success());
        }
        let output = thread::scope(|scope| {
            let process = scope.spawn(|| {
                if automatic {
                    fixture.run(&[
                        "run",
                        "--goal",
                        &id,
                        "--prompt",
                        "Continue under the selected goal",
                        "--json",
                    ])
                } else {
                    fixture.run(&["compact", "--goal", &id, "--json"])
                }
            });
            let request = summary_request(&server);
            let summary = summary_for(&request);
            server.replies.send(json!({"choices":[{"delta":{"content":summary.to_string()},"finish_reason":"stop"}],"usage":{"total_tokens":75}})).unwrap();
            if automatic {
                let next = server.request();
                assert_eq!(next["max_tokens"], 4096);
                assert!(next.to_string().contains("explicit_goal_task_guidance"));
                server.replies.send(json!({"choices":[{"delta":{"content":"finished"},"finish_reason":"stop"}],"usage":{"total_tokens":25}})).unwrap();
            }
            process.join().unwrap()
        });
        assert!(
            output.status.success(),
            "automatic={automatic}: {}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(
            goal(&fixture)["tokens_used"],
            if automatic { 100 } else { 75 }
        );
        assert_eq!(goal(&fixture)["iteration"], u32::from(automatic));
        assert_eq!(
            goal(&fixture)["accounting"]["counters"]["request_attempts"],
            if automatic { 2 } else { 1 }
        );
        assert!(server.requests.try_recv().is_err());
        // A lower allowance preserves known consumption and cannot fund either
        // a new main call or an otherwise provider-backed summary.
        if automatic {
            assert!(fixture
                .run(&[
                    "provider",
                    "budget",
                    "f01-fixture",
                    "--limits",
                    "2000000,1900000,8192,4096,1024",
                    "--json"
                ])
                .status
                .success());
            for file in ["older.txt", "fixture.txt"] {
                let output = thread::scope(|scope| {
                    let process = scope.spawn(|| {
                        fixture.run(&[
                            "run",
                            "--goal",
                            &id,
                            "--prompt",
                            "Retain another goal-scoped observation",
                            "--json",
                        ])
                    });
                    assert_eq!(server.request()["max_tokens"], 4096);
                    let end = std::fs::metadata(fixture.workspace.join(file))
                        .unwrap()
                        .len();
                    server.replies.send(json!({"choices":[{"delta":{"tool_calls":[{"index":0,"id":"cumulative-read","type":"function","function":{"name":"read_text","arguments":json!({"path":file,"start_byte":0,"end_byte":end}).to_string()}}]},"finish_reason":"tool_calls"}],"usage":{"total_tokens":25}})).unwrap();
                    let next = server.request();
                    assert_eq!(next["max_tokens"], 4096);
                    assert!(tool_reply(&next, "cumulative-read")["sha256"].is_string());
                    server.replies.send(json!({"choices":[{"delta":{"content":"observation retained"},"finish_reason":"stop"}],"usage":{"total_tokens":25}})).unwrap();
                    process.join().unwrap()
                });
                assert!(
                    output.status.success(),
                    "{}",
                    String::from_utf8_lossy(&output.stderr)
                );
            }
            let output = thread::scope(|scope| {
                let process = scope.spawn(|| fixture.run(&["compact", "--goal", &id, "--json"]));
                let request = summary_request(&server);
                server.replies.send(json!({"choices":[{"delta":{"content":summary_for(&request).to_string()},"finish_reason":"stop"}],"usage":{"total_tokens":75}})).unwrap();
                process.join().unwrap()
            });
            assert!(
                output.status.success(),
                "{}",
                String::from_utf8_lossy(&output.stderr)
            );
            let cumulative = goal(&fixture);
            assert_eq!(cumulative["tokens_used"], 275);
            assert_eq!(cumulative["iteration"], 3);
            assert_eq!(cumulative["accounting"]["counters"]["request_attempts"], 7);
        }
        limits(&fixture, &id, "1,100,100,100,600,500000,100,100,60");
        assert!(!fixture
            .run(&[
                "run",
                "--goal",
                &id,
                "--prompt",
                "No free continuation",
                "--json"
            ])
            .status
            .success());
        assert!(!fixture
            .run(&["compact", "--goal", &id, "--json"])
            .status
            .success());
        assert!(server.requests.try_recv().is_err());
    }
    let server = Server::start();
    let fixture = CliFixture::new(&server.endpoint);
    seed(&server, &fixture, 1800);
    let id = set_goal(&fixture, "500000");
    let output = thread::scope(|scope| {
        let process = scope.spawn(|| fixture.run(&["compact", "--goal", &id, "--json"]));
        summary_request(&server);
        server.replies.send(json!({"choices":[{"delta":{"content":"invalid summary JSON"},"finish_reason":"stop"}],"usage":{"total_tokens":75}})).unwrap();
        process.join().unwrap()
    });
    assert!(!output.status.success());
    assert_eq!(goal(&fixture)["tokens_used"], 75);
    assert_eq!(
        goal(&fixture)["accounting"]["counters"]["unresolved_tokens"],
        0
    );
    assert!(context(&fixture)["summary_checkpoint_id"].is_null());
}

#[test]
fn f07a_one_token_goal_stops_before_http_but_ordinary_task_is_not_enrolled() {
    let server = Server::start();
    let fixture = CliFixture::new(&server.endpoint);
    let id = set_goal(&fixture, "1");
    let output = fixture.run(&[
        "run",
        "--goal",
        &id,
        "--prompt",
        "Explicit associated task",
        "--json",
    ]);
    assert_eq!(output.status.code(), Some(1));
    assert_eq!(
        terminal(&output)["code"],
        "goal_budget_exhausted",
        "{}",
        String::from_utf8_lossy(&output.stdout)
    );
    assert!(server.requests.try_recv().is_err());
    let accounting = goal(&fixture);
    assert_eq!(accounting["iteration"], 1);
    assert_eq!(accounting["accounting"]["counters"]["request_attempts"], 0);
    assert_eq!(accounting["tokens_used"], 0);
    let output = thread::scope(|scope| {
        let process = scope
            .spawn(|| fixture.run(&["run", "--prompt", "Ordinary unassociated task", "--json"]));
        let request = server.request();
        assert!(!request.to_string().contains("explicit_goal_task_guidance"));
        server.complete();
        process.join().unwrap()
    });
    assert!(output.status.success());
    assert_eq!(goal(&fixture)["iteration"], 1);
}

#[test]
fn f07a_funded_rounds_settle_real_counters_and_restart_without_recharge() {
    let server = Server::start();
    let fixture = CliFixture::new(&server.endpoint);
    let id = set_goal(&fixture, "500000");
    let output = thread::scope(|scope| {
        let process = scope.spawn(|| {
            fixture.run(&[
                "run",
                "--goal",
                &id,
                "--prompt",
                "Read the fixture once",
                "--json",
            ])
        });
        let first = server.request();
        assert_eq!(first["max_tokens"], 4096);
        assert!(first.to_string().contains("F07A explicit scoped objective"));
        server.replies.send(json!({"choices":[{"delta":{"tool_calls":[{"index":0,"id":"goal-read","type":"function","function":{"name":"read_text","arguments":"{\"path\":\"fixture.txt\"}"}}]},"finish_reason":"tool_calls"}],"usage":{"prompt_tokens":31,"completion_tokens":9,"total_tokens":40}})).unwrap();
        let second = server.request();
        assert_eq!(second["max_tokens"], 4096);
        server.replies.send(json!({"choices":[{"delta":{"content":"Read finished; objective completion is not asserted"},"finish_reason":"stop"}],"usage":{"prompt_tokens":43,"completion_tokens":7,"total_tokens":50}})).unwrap();
        process.join().unwrap()
    });
    assert!(
        output.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let first = goal(&fixture);
    assert_eq!(first["tokens_used"], 90);
    assert_eq!(first["tools_used"], 1);
    assert_eq!(first["iteration"], 1);
    assert_eq!(first["accounting"]["counters"]["request_attempts"], 2);
    assert_eq!(first["accounting"]["counters"]["outstanding_tokens"], 0);
    assert_eq!(first["accounting"]["counters"]["unresolved_tokens"], 0);
    assert_eq!(first["lifecycle"], "active");
    assert_eq!(goal(&fixture), first);
    assert!(server.requests.try_recv().is_err());
}

#[test]
fn f07a_usage_quality_is_not_free_allowance_or_repeated_snapshot_sum() {
    let done = json!({"choices":[{"delta":{"content":"finished"},"finish_reason":"stop"}]});
    let full = json!({"prompt_tokens":31,"completion_tokens":9,"total_tokens":40});
    let final_usage = json!({"choices":[],"usage":full});
    for (name, frames, expected, blocked) in [
        ("missing", vec![done.clone()], 0, true),
        (
            "zero",
            vec![
                json!({"choices":[{"delta":{"content":"finished"},"finish_reason":"stop"}],"usage":{"prompt_tokens":0,"completion_tokens":0,"total_tokens":0}}),
            ],
            0,
            false,
        ),
        (
            "total_only",
            vec![
                done.clone(),
                json!({"choices":[],"usage":{"total_tokens":40}}),
            ],
            40,
            false,
        ),
        (
            "duplicate",
            vec![done.clone(), final_usage.clone(), final_usage.clone()],
            40,
            false,
        ),
        (
            "cumulative",
            vec![
                json!({"choices":[{"delta":{"content":"partial"},"finish_reason":null}],"usage":{"prompt_tokens":31,"completion_tokens":1,"total_tokens":32}}),
                done.clone(),
                final_usage.clone(),
            ],
            40,
            false,
        ),
        (
            "conflict",
            vec![
                done.clone(),
                final_usage.clone(),
                json!({"choices":[],"usage":{"prompt_tokens":32,"completion_tokens":9,"total_tokens":41}}),
            ],
            0,
            true,
        ),
        (
            "impossible",
            vec![
                done.clone(),
                json!({"choices":[],"usage":{"prompt_tokens":31,"completion_tokens":9,"total_tokens":0}}),
            ],
            0,
            true,
        ),
        (
            "malformed",
            vec![
                done.clone(),
                json!({"choices":[],"usage":{"total_tokens":-1}}),
            ],
            0,
            true,
        ),
        (
            "overrun",
            vec![
                done.clone(),
                json!({"choices":[],"usage":{"total_tokens":600000}}),
            ],
            600000,
            true,
        ),
    ] {
        let server = Server::start();
        let fixture = CliFixture::new(&server.endpoint);
        let id = set_goal(&fixture, "500000");
        let output = thread::scope(|scope| {
            let process = scope.spawn(|| {
                fixture.run(&[
                    "run",
                    "--goal",
                    &id,
                    "--prompt",
                    "Check usage quality",
                    "--json",
                ])
            });
            server.request();
            server
                .replies
                .send(json!({"fixture_frames":frames}))
                .unwrap();
            process.join().unwrap()
        });
        assert_eq!(
            output.status.success(),
            !blocked,
            "{name}: {}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        let stored = goal(&fixture);
        assert_eq!(stored["tokens_used"], expected, "{name}");
        let counters = &stored["accounting"]["counters"];
        assert_eq!(counters["request_attempts"], 1);
        assert_eq!(counters["outstanding_tokens"], 0);
        if blocked && name != "overrun" {
            assert!(
                counters["unresolved_tokens"].as_u64().unwrap() > 0,
                "{name}"
            );
        } else {
            assert_eq!(counters["unresolved_tokens"], 0, "{name}");
        }
        let db = rusqlite::Connection::open(&fixture.database).unwrap();
        let charge: String = db
            .query_row("SELECT payload FROM goal_requests", [], |row| row.get(0))
            .unwrap();
        let charge: Value = serde_json::from_str(&charge).unwrap();
        if name == "missing" {
            assert_eq!(charge["usage_quality"], "missing_final_usage");
        }
        if matches!(name, "conflict" | "impossible" | "malformed") {
            assert_eq!(
                charge["usage_quality"],
                "invalid_or_conflicting_final_usage"
            );
        }
        if name == "zero" {
            assert_eq!(charge["usage_quality"], "reported_zero");
        }
        if name == "total_only" {
            assert!(charge["observed"]["input_tokens"].is_null());
            assert!(charge["observed"]["output_tokens"].is_null());
        }
        if name == "overrun" {
            assert_eq!(charge["observed"]["total_tokens"], 600000);
            assert!(charge["overrun_tokens"].as_u64().unwrap() > 0);
        }
        drop(db);
        if blocked {
            let next = fixture.run(&[
                "run",
                "--goal",
                &id,
                "--prompt",
                "Explicit new attempt still requires allowance",
                "--json",
            ]);
            assert!(!next.status.success(), "{name}");
            assert!(server.requests.try_recv().is_err(), "{name}");
            assert_eq!(
                goal(&fixture)["iteration"],
                1,
                "rejected admission is not an iteration: {name}"
            );
            if name == "missing" {
                let output = thread::scope(|scope| {
                    let ordinary = scope.spawn(|| {
                        fixture.run(&[
                            "run",
                            "--prompt",
                            "Ordinary work is outside the unresolved goal quota",
                            "--json",
                        ])
                    });
                    server.request();
                    server.complete();
                    ordinary.join().unwrap()
                });
                assert!(output.status.success());
                assert_eq!(goal(&fixture), stored);
            }
        }
    }
}
