//! Exact-executable F06B checks using the production HTTP adapter and store.
use super::*;

const CANARY: &str = "F06B_TOOL_ONLY_ARCHIVED_9a71";

pub(crate) fn context(fixture: &CliFixture) -> Value {
    let output = fixture.run(&["context", "--json"]);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap()
}

fn observe(server: &Server, fixture: &CliFixture, file: &str, id: &str) -> Value {
    let mut observation = Value::Null;
    let output = thread::scope(|scope| {
        let process = scope.spawn(|| {
            fixture.run(&[
                "run",
                "--prompt",
                "Observe the requested record and retain its original revision",
                "--json",
            ])
        });
        let first = server.request();
        assert_eq!(first["max_tokens"], 4096);
        // Seed the intended observation explicitly. The production default is
        // a small preview, not the full compaction fixture's evidence.
        let length = std::fs::metadata(fixture.workspace.join(file))
            .unwrap()
            .len();
        server.tool(
            id,
            "read_text",
            json!({"path":file,"start_byte":0,"end_byte":length}),
        );
        observation = tool_reply(&server.request(), id);
        assert!(
            observation["sha256"].is_string(),
            "seed must retain an actual read, not a rejection"
        );
        server.complete();
        process.join().unwrap()
    });
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    observation
}

pub(crate) fn seed(server: &Server, fixture: &CliFixture, size: usize) -> Value {
    std::fs::write(
        fixture.workspace.join("older.txt"),
        format!("{CANARY}\n{}", "data é中 \"quoted\"\n".repeat(size)),
    )
    .unwrap();
    let observation = observe(server, fixture, "older.txt", "archived-read");
    observe(server, fixture, "fixture.txt", "recent-read");
    observation
}

pub(crate) fn summary_request(server: &Server) -> Value {
    let request = server
        .requests
        .recv_timeout(Duration::from_secs(15))
        .expect("compaction request watchdog");
    assert!(request.get("tools").is_none());
    assert_eq!(request["max_tokens"], 4096);
    let messages = request["messages"].as_array().unwrap();
    assert_eq!(messages.len(), 2);
    assert_eq!(messages[0]["role"], "system");
    assert_eq!(messages[1]["role"], "user");
    let facts: Value = serde_json::from_str(
        messages[0]["content"]
            .as_str()
            .unwrap()
            .lines()
            .nth(1)
            .unwrap(),
    )
    .unwrap();
    assert_eq!(facts["application"], "Argentum");
    assert_eq!(facts["mode"], "context compaction; tools disabled");
    assert_eq!(facts["model_tools"], json!([]));
    assert_eq!(facts["coding_available"], false);
    assert!(messages[0]["content"]
        .as_str()
        .unwrap()
        .contains("tools disabled"));
    request
}

pub(crate) fn summary_for(request: &Value) -> Value {
    let supplied: Value =
        serde_json::from_str(request["messages"][1]["content"].as_str().unwrap()).unwrap();
    assert_eq!(supplied["purpose"], "bounded_context_compaction");
    let sources = supplied["sources"].as_array().unwrap();
    let tool_text = sources
        .iter()
        .filter_map(|source| source["historical_message"]["tool_exchange"]["results"].as_array())
        .flatten()
        .map(|result| result["content"].clone())
        .filter_map(|result| result["content"].as_str().map(str::to_owned))
        .find(|text| text.contains(CANARY));
    // Replacement input may already contain a previous summary. It must still
    // provide original source references and a newly observed tool source.
    let observed = tool_text
        .map(|text| text.lines().next().unwrap().to_owned())
        .unwrap_or_else(|| {
            "Earlier observations remain historical and inspectable through host evidence".into()
        });
    json!({
        "objective":"Continue work using the retained observations without repeating effects",
        "decisions_constraints":["Original revisions and receipts remain authoritative; do not infer current file contents"],
        "observed_changes_verification":[format!("The supplied tool observation contained {observed}; no verification success is inferred")],
        "unresolved_work":["Inspect new work under current instructions and policy"],
        "source_references": supplied["source_references"],
    })
}

fn reply_summary(server: &Server, summary: &Value) {
    server
        .replies
        .send(json!({"choices":[{"delta":{"content":summary.to_string()},"finish_reason":"stop"}]}))
        .unwrap();
}

fn compact(server: &Server, fixture: &CliFixture) -> Value {
    let output = thread::scope(|scope| {
        let process = scope.spawn(|| fixture.run(&["compact", "--json"]));
        let request = summary_request(server);
        reply_summary(server, &summary_for(&request));
        process.join().unwrap()
    });
    assert!(
        output.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap()
}

fn originals(fixture: &CliFixture) -> Vec<(String, u32, String)> {
    let connection = rusqlite::Connection::open(&fixture.database).unwrap();
    let mut query = connection
        .prepare("SELECT run_id, ordinal, payload FROM context_groups ORDER BY rowid")
        .unwrap();
    query
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))
        .unwrap()
        .map(Result::unwrap)
        .collect()
}

#[test]
fn retained_search_prefix_automatically_compacts_at_unchanged_declared_limits() {
    let server = Server::start();
    let fixture = CliFixture::new(&server.endpoint);
    assert!(fixture
        .run(&[
            "provider",
            "budget",
            "f01-fixture",
            "--limits",
            "204800,196608,4096,4096,1024",
            "--json"
        ])
        .status
        .success());
    std::fs::write(
        fixture.workspace.join("search-anchor.txt"),
        "REAL05_SYNTHETIC_SEARCH_ANCHOR\n",
    )
    .unwrap();
    let output = thread::scope(|scope| {
        let process = scope.spawn(|| fixture.run(&["run", "--prompt", "Synthetic public objective: preserve unrelated files and retain recorded effect references.", "--json"]));
        server.request();
        server.tool(
            "prefix-search",
            "search_text",
            json!({"path":".","query":"REAL05_SYNTHETIC_SEARCH_ANCHOR","max_results":4}),
        );
        let observed = tool_reply(&server.request(), "prefix-search");
        assert_eq!(observed["matches_returned"], 1);
        server.complete();
        process.join().unwrap()
    });
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    std::fs::write(
        fixture.workspace.join("older.txt"),
        format!("{CANARY}\n{}", "data é中 \"quoted\"\n".repeat(5000)),
    )
    .unwrap();
    observe(&server, &fixture, "older.txt", "prefix-read");
    std::fs::write(
        fixture.workspace.join("fixture.txt"),
        "data é中 \"quoted\"\n".repeat(5000),
    )
    .unwrap();
    observe(&server, &fixture, "fixture.txt", "recent-read");
    let original_records = originals(&fixture);
    assert!(context(&fixture)["omitted_source_ids"]
        .as_array()
        .unwrap()
        .is_empty());
    std::fs::write(
        fixture.workspace.join("pressure.txt"),
        "required-current-data\n".repeat(2000),
    )
    .unwrap();
    let output = thread::scope(|scope| {
        let process = scope.spawn(|| {
            fixture.run(&[
                "run",
                "--prompt",
                "Continue the same synthetic public objective without repeating completed effects.",
                "--json",
            ])
        });
        let first = server.request();
        assert!(!first.to_string().contains("historical_lossy_summary"));
        server.tool(
            "fresh-pressure-read",
            "read_text",
            json!({"path":"pressure.txt","start_byte":0,"end_byte":42000}),
        );
        let summary = summary_request(&server);
        assert!(summary.to_string().contains("prefix-search"));
        assert!(summary.to_string().contains(CANARY));
        assert!(!summary.to_string().contains("required-current-data"));
        let mut narrative = summary_for(&summary);
        narrative["decisions_constraints"].as_array_mut().unwrap().push(json!("Synthetic public objective: preserve unrelated files and retain recorded effect references."));
        reply_summary(&server, &narrative);
        let admitted = server.request();
        assert!(admitted.to_string().contains("historical_lossy_summary"));
        assert!(admitted.to_string().contains("preserve unrelated files"));
        assert!(tool_reply(&admitted, "fresh-pressure-read")["content"]
            .as_str()
            .unwrap()
            .contains("required-current-data"));
        let input = serde_json::to_vec(&admitted).unwrap().len() as u64;
        assert!(input + 1024 <= 196608);
        assert!(input + 4096 + 1024 <= 204800);
        assert_eq!(admitted["max_tokens"], 4096);
        server.complete();
        process.join().unwrap()
    });
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        &originals(&fixture)[..original_records.len()],
        original_records.as_slice()
    );
    assert!(
        server.requests.try_recv().is_err(),
        "no prior tool/effect replay after compaction"
    );
    assert_eq!(
        std::fs::read_to_string(fixture.workspace.join("search-anchor.txt")).unwrap(),
        "REAL05_SYNTHETIC_SEARCH_ANCHOR\n"
    );
}

#[test]
fn manual_useful_compaction_restart_and_deactivation_preserve_original_tool_lineage() {
    let server = Server::start();
    let fixture = CliFixture::new(&server.endpoint);
    let original_read = seed(&server, &fixture, 1800);
    let before_records = originals(&fixture);
    let before_counts = history_counts(&fixture);
    let result = compact(&server, &fixture);
    assert_eq!(result["status"], "success");
    assert!(result["observed_usage"].is_null());
    assert_eq!(result["after"]["fits"], true);
    assert!(
        result["after"]["accounted_input"].as_u64().unwrap()
            < result["before"]["accounted_input"].as_u64().unwrap()
    );
    assert_eq!(originals(&fixture), before_records);
    let after_counts = history_counts(&fixture);
    assert_eq!(after_counts[0], before_counts[0] + 4);
    // One physical start, one safe transport diagnostic and one terminal
    // outcome. None is another inference request or historical source mutation.
    let db = rusqlite::Connection::open(&fixture.database).unwrap();
    let mut statement = db
        .prepare("SELECT payload FROM event_log ORDER BY id DESC LIMIT 4")
        .unwrap();
    let events = statement
        .query_map([], |row| row.get::<_, String>(0))
        .unwrap()
        .map(|row| serde_json::from_str::<Value>(&row.unwrap()).unwrap())
        .collect::<Vec<_>>();
    assert_eq!(
        events
            .iter()
            .filter(|event| event.get("SummaryTransportObserved").is_some())
            .count(),
        1
    );
    assert!(events
        .iter()
        .filter(|event| event.get("ModelRequestAttempt").is_some())
        .all(|event| event["ModelRequestAttempt"]["purpose"] == "compaction_tools_disabled"));
    for outcome in ["started", "transport_settled", "stopped"] {
        assert_eq!(
            events
                .iter()
                .filter(|event| event["ModelRequestAttempt"]["outcome"] == outcome)
                .count(),
            1
        );
    }
    assert_eq!(&after_counts[1..], &before_counts[1..]);
    let metadata = context(&fixture);
    assert_eq!(metadata["summary_checkpoint_id"], result["checkpoint_id"]);
    assert!(!metadata.to_string().contains(CANARY));
    assert!(server.requests.try_recv().is_err());
    std::fs::write(
        fixture.workspace.join("older.txt"),
        "F06B_NEW_CONTENT_NOT_HISTORICAL",
    )
    .unwrap();
    let output =
        thread::scope(|scope| {
            let process = scope.spawn(|| {
                fixture.run(&[
                    "run",
                    "--prompt",
                    "Continue without repeating historical tools",
                    "--json",
                ])
            });
            let request = server.request();
            let text = request.to_string();
            assert!(text.contains("historical_lossy_summary"));
            assert!(text.contains(CANARY));
            assert!(text.contains(original_read["sha256"].as_str().unwrap()));
            assert!(!text.contains("F06B_NEW_CONTENT_NOT_HISTORICAL"));
            assert_eq!(tool_reply(&request, "recent-read")["content"], FILE_CONTENT);
            assert!(!request["messages"]
                .as_array()
                .unwrap()
                .iter()
                .any(|message| message["role"] == "tool"
                    && message["tool_call_id"] == "archived-read"));
            server.complete();
            process.join().unwrap()
        });
    assert!(output.status.success());
    assert!(server.requests.try_recv().is_err());
    let deactivated = fixture.run(&["compact", "--deactivate", "--json"]);
    assert!(deactivated.status.success());
    assert_eq!(
        serde_json::from_slice::<Value>(&deactivated.stdout).unwrap()["status"],
        "deactivated"
    );
    assert!(context(&fixture)["summary_checkpoint_id"].is_null());
    let restored = originals(&fixture);
    assert_eq!(&restored[..before_records.len()], before_records.as_slice());
    println!("F06B manual: tool-only source summarized, useful reduction, original digest/receipts retained, clean restart without model or effect replay, local deactivation");
}

#[test]
fn estimated_wire_boundary_unknown_and_invalid_budget_fail_before_inference() {
    for mode in ["exact", "over", "unknown", "invalid"] {
        let server = Server::start();
        let fixture = CliFixture::new(&server.endpoint);
        let prompt = "é中\"\\ Unicode boundary";
        let preview = context(&fixture);
        let run_json = serde_json::to_string("00000000-0000-0000-0000-000000000000").unwrap();
        let scoped_run_wire_bytes = serde_json::to_vec(&run_json).unwrap().len() - 2;
        let no_run_wire_bytes = serde_json::to_vec("null").unwrap().len() - 2;
        let scoped_run_overhead = scoped_run_wire_bytes - no_run_wire_bytes;
        let input = preview["accounting"]["accounted_input"].as_u64().unwrap()
            + serde_json::to_string(prompt).unwrap().len() as u64
            - 2
            + scoped_run_overhead as u64;
        let total = input + 4096 + 1024 - u64::from(mode == "over");
        let declared = fixture.run(&[
            "provider",
            "budget",
            "f01-fixture",
            "--limits",
            &format!("{total},{total},8192,4096,1024"),
            "--json",
        ]);
        assert!(declared.status.success());
        if mode == "unknown" || mode == "invalid" {
            let connection = rusqlite::Connection::open(&fixture.database).unwrap();
            connection
                .execute_batch(if mode == "unknown" {
                    "DELETE FROM model_budgets"
                } else {
                    "UPDATE model_budgets SET payload='{}'"
                })
                .unwrap();
            let unavailable = context(&fixture);
            assert_eq!(unavailable["selected_profile"]["id"], "f01-fixture");
            assert_eq!(unavailable["selected_profile"]["model"], "fixture-model");
        }
        let output = thread::scope(|scope| {
            let process = scope.spawn(|| fixture.run(&["run", "--prompt", prompt, "--json"]));
            if mode == "exact" {
                let request = server.request();
                assert_eq!(request["max_tokens"], 4096);
                assert_eq!(serde_json::to_vec(&request).unwrap().len() as u64, input);
                server.complete();
            }
            process.join().unwrap()
        });
        assert_eq!(
            output.status.success(),
            mode == "exact",
            "{mode}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        if mode != "exact" {
            assert!(server.requests.try_recv().is_err());
        }
        let lines: Vec<Value> = String::from_utf8(output.stdout)
            .unwrap()
            .lines()
            .map(|line| serde_json::from_str(line).unwrap())
            .collect();
        if mode == "unknown" || mode == "invalid" {
            assert_eq!(lines.last().unwrap()["code"], "context_budget_invalid");
        }
    }
}

#[test]
fn automatic_pressure_between_tasks_and_settled_rounds_uses_one_admitted_summary() {
    for between_rounds in [false, true] {
        let server = Server::start();
        let fixture = CliFixture::new(&server.endpoint);
        seed(&server, &fixture, 1800);
        let originals_before = originals(&fixture);
        std::fs::write(
            fixture.workspace.join("pressure.txt"),
            "required-current-data\n".repeat(400),
        )
        .unwrap();
        let input = context(&fixture)["accounting"]["accounted_input"]
            .as_u64()
            .unwrap();
        let total = input + 4096 + 1024 + if between_rounds { 1000 } else { 0 }
            - if between_rounds { 0 } else { 1000 };
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
        let output = thread::scope(|scope| {
            let process = scope.spawn(|| {
                fixture.run(&[
                    "run",
                    "--prompt",
                    "Continue with all required context intact",
                    "--json",
                ])
            });
            if between_rounds {
                let first = server.request();
                assert!(!first.to_string().contains("historical_lossy_summary"));
                server.tool("pressure-read", "read_text", json!({"path":"pressure.txt"}));
            }
            let summary_input = summary_request(&server);
            assert!(summary_input.to_string().contains(CANARY));
            assert!(!summary_input.to_string().contains("required-current-data"));
            reply_summary(&server, &summary_for(&summary_input));
            let admitted = server.request();
            assert!(admitted.to_string().contains("historical_lossy_summary"));
            assert!(admitted.to_string().contains(CANARY));
            assert_eq!(
                tool_reply(&admitted, "recent-read")["content"],
                FILE_CONTENT
            );
            if between_rounds {
                assert!(tool_reply(&admitted, "pressure-read")["content"]
                    .as_str()
                    .unwrap()
                    .contains("required-current-data"));
            }
            assert!(serde_json::to_vec(&admitted).unwrap().len() as u64 + 4096 + 1024 <= total);
            server.complete();
            process.join().unwrap()
        });
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(
            &originals(&fixture)[..originals_before.len()],
            originals_before.as_slice()
        );
        assert!(server.requests.try_recv().is_err());
    }
}

fn stored_checkpoint(fixture: &CliFixture) -> Option<String> {
    use rusqlite::OptionalExtension;
    rusqlite::Connection::open(&fixture.database)
        .unwrap()
        .query_row(
            "SELECT payload FROM context_checkpoints WHERE payload IS NOT NULL",
            [],
            |row| row.get(0),
        )
        .optional()
        .unwrap()
}

#[test]
fn failed_initial_and_replacement_candidates_preserve_committed_projection() {
    for replacement in [false, true] {
        for failure in [
            "json",
            "json_usage",
            "incomplete",
            "truncated",
            "overlong",
            "tool",
            "references",
            "storage",
            "policy",
            "instructions",
            "source",
        ] {
            let server = Server::start();
            let fixture = CliFixture::new(&server.endpoint);
            seed(&server, &fixture, 1800);
            if replacement {
                compact(&server, &fixture);
                observe(&server, &fixture, "older.txt", "replacement-eligible-read");
                observe(&server, &fixture, "fixture.txt", "replacement-recent-read");
            }
            let old_checkpoint = stored_checkpoint(&fixture);
            let old_records = originals(&fixture);
            if failure == "storage" {
                rusqlite::Connection::open(&fixture.database).unwrap().execute_batch(
                    "CREATE TRIGGER block_checkpoint BEFORE INSERT ON context_checkpoints BEGIN SELECT RAISE(FAIL,'owned fixture storage failure'); END;
                     CREATE TRIGGER block_checkpoint_update BEFORE UPDATE ON context_checkpoints BEGIN SELECT RAISE(FAIL,'owned fixture storage failure'); END;").unwrap();
            }
            let output = thread::scope(|scope| {
                let process = scope.spawn(|| fixture.run(&["compact", "--json"]));
                let request = summary_request(&server);
                let mut summary = summary_for(&request);
                match failure {
                    "json" | "json_usage" => server.replies.send(json!({"choices":[{"delta":{"content":"not valid JSON"},"finish_reason":"stop"}],
                        "usage": if failure == "json_usage" { json!({"prompt_tokens":10,"completion_tokens":1,"total_tokens":11}) } else { Value::Null }})).unwrap(),
                    "incomplete" => {
                        server.replies.send(json!({"fixture_partial":"{\"objective\":\"unfinished"})).unwrap();
                        server.replies.send(json!({"fixture_disconnect":true})).unwrap();
                    }
                    "truncated" => server.replies.send(json!({"choices":[{"delta":{"content":"{"},"finish_reason":"length"}]})).unwrap(),
                    "overlong" => server.replies.send(json!({"choices":[{"delta":{"content":"x".repeat(8193)},"finish_reason":"stop"}]})).unwrap(),
                    "tool" => server.tool("forbidden-summary-write", "write_text", json!({"path":"must-not-exist.txt","content":"forbidden"})),
                    "references" => { summary["source_references"] = json!(["invented-source"]); reply_summary(&server, &summary); }
                    "policy" => { inject_external_policy_change(&fixture); reply_summary(&server, &summary); }
                    "instructions" => { std::fs::write(fixture.workspace.join("AGENTS.md"), "F06B_CHANGED_DURING_SUMMARY").unwrap(); reply_summary(&server, &summary); }
                    "source" => {
                        rusqlite::Connection::open(&fixture.database).unwrap().execute(
                            "UPDATE context_groups SET payload='{}' WHERE run_id=?1 AND ordinal=?2",
                            rusqlite::params![old_records[0].0, old_records[0].1]).unwrap();
                        reply_summary(&server, &summary);
                    }
                    _ => reply_summary(&server, &summary),
                }
                process.join().unwrap()
            });
            assert_eq!(
                output.status.code(),
                Some(1),
                "{failure}, replacement={replacement}: {}",
                String::from_utf8_lossy(&output.stdout)
            );
            assert_eq!(stored_checkpoint(&fixture), old_checkpoint);
            if failure == "source" {
                rusqlite::Connection::open(&fixture.database)
                    .unwrap()
                    .execute(
                        "UPDATE context_groups SET payload=?3 WHERE run_id=?1 AND ordinal=?2",
                        rusqlite::params![old_records[0].0, old_records[0].1, old_records[0].2],
                    )
                    .unwrap();
            }
            if failure == "json_usage" {
                let result: Value = serde_json::from_slice(&output.stdout).unwrap();
                assert_eq!(result["observed_usage"]["input_tokens"], 10);
                assert_eq!(result["observed_usage"]["total_tokens"], 11);
                assert!(!result["before"].is_null());
            }
            assert_eq!(originals(&fixture), old_records);
            assert!(!fixture.workspace.join("must-not-exist.txt").exists());
            assert!(server.requests.try_recv().is_err());
        }
    }
}

#[test]
fn summary_revocation_invalidates_transitive_coverage_and_model_switch_invalidates_budget() {
    let server = Server::start();
    let fixture = CliFixture::new(&server.endpoint);
    seed(&server, &fixture, 1800);
    compact(&server, &fixture);
    observe(&server, &fixture, "older.txt", "next-covered-read");
    observe(&server, &fixture, "fixture.txt", "next-recent-read");
    let replacement = compact(&server, &fixture);
    assert!(replacement["source_references"].as_array().unwrap().len() > 3);
    let raw = stored_checkpoint(&fixture).unwrap();
    let checkpoint: Value = serde_json::from_str(&raw).unwrap();
    let coverage = checkpoint["sources"].as_array().unwrap();
    let unique: std::collections::BTreeSet<_> = coverage
        .iter()
        .map(|source| source["identity"].as_str().unwrap())
        .collect();
    assert_eq!(coverage.len(), unique.len());
    let connection = rusqlite::Connection::open(&fixture.database).unwrap();
    connection
        .execute(
            "DELETE FROM context_groups WHERE run_id=?1 AND ordinal=?2",
            rusqlite::params![
                coverage[1]["identity"]
                    .as_str()
                    .unwrap()
                    .split(':')
                    .next()
                    .unwrap(),
                1
            ],
        )
        .unwrap();
    drop(connection);
    let metadata = context(&fixture);
    assert!(metadata["summary_checkpoint_id"].is_null());
    assert_eq!(
        metadata["compaction"],
        "summary_invalidated_source_eligibility_or_window"
    );
    let output = thread::scope(|scope| {
        let process = scope.spawn(|| {
            fixture.run(&[
                "run",
                "--prompt",
                "Use only admissible surviving evidence",
                "--json",
            ])
        });
        let request = server.request();
        assert!(!request.to_string().contains("historical_lossy_summary"));
        server.complete();
        process.join().unwrap()
    });
    assert!(output.status.success());
    assert!(fixture
        .run(&[
            "provider",
            "model",
            "f01-fixture",
            "--model",
            "changed-model",
            "--json"
        ])
        .status
        .success());
    let denied = fixture.run(&[
        "run",
        "--prompt",
        "Do not infer unknown model capacity",
        "--json",
    ]);
    assert_eq!(denied.status.code(), Some(1));
    assert!(String::from_utf8(denied.stdout)
        .unwrap()
        .contains("context_budget_invalid"));
    assert!(server.requests.try_recv().is_err());
}

#[test]
fn timeout_nonreducing_and_owned_process_interruption_do_not_activate_candidates() {
    for replacement in [false, true] {
        for mode in ["timeout", "nonreducing", "interrupted"] {
            let server = Server::start();
            let fixture = CliFixture::new(&server.endpoint);
            seed(
                &server,
                &fixture,
                if mode == "nonreducing" && !replacement {
                    1
                } else {
                    1800
                },
            );
            if replacement {
                compact(&server, &fixture);
                if mode == "nonreducing" {
                    std::fs::write(fixture.workspace.join("older.txt"), CANARY).unwrap();
                }
                observe(&server, &fixture, "older.txt", "failed-replacement-source");
                observe(
                    &server,
                    &fixture,
                    "fixture.txt",
                    "failed-replacement-suffix",
                );
            }
            let old_records = originals(&fixture);
            let old_checkpoint = stored_checkpoint(&fixture);
            if mode == "interrupted" {
                let mut child = ChildGuard(
                    fixture
                        .command(&["compact", "--json"])
                        .stdin(Stdio::null())
                        .stdout(Stdio::null())
                        .stderr(Stdio::null())
                        .spawn()
                        .unwrap(),
                );
                let request = summary_request(&server);
                let connection = rusqlite::Connection::open(&fixture.database).unwrap();
                connection.execute_batch("BEGIN IMMEDIATE").unwrap();
                reply_summary(&server, &summary_for(&request));
                thread::sleep(Duration::from_millis(250));
                assert!(
                    child.try_wait().unwrap().is_none(),
                    "owned child should be blocked behind the fixture transaction"
                );
                child.kill().unwrap();
                let injected_exit = child.wait().unwrap();
                assert!(
                    !injected_exit.success(),
                    "forced termination is an injection, never successful completion"
                );
                connection.execute_batch("ROLLBACK").unwrap();
                drop(connection);
                let settlement = argentum_store::Store::reconcile_recovery(&fixture.database)
                    .expect("explicitly reconcile the fault-injected abandoned compaction");
                assert!(settlement.mutation_performed);
            } else {
                let output = thread::scope(|scope| {
                    let process = scope.spawn(|| fixture.run(&["compact", "--json"]));
                    let request = summary_request(&server);
                    if mode == "nonreducing" {
                        reply_summary(&server, &summary_for(&request));
                    }
                    let output = process.join().unwrap();
                    if mode == "timeout" {
                        server
                            .replies
                            .send(json!({"fixture_disconnect":true}))
                            .unwrap();
                    }
                    output
                });
                assert_eq!(
                    output.status.code(),
                    Some(1),
                    "{mode}: {}",
                    String::from_utf8_lossy(&output.stdout)
                );
                let reason = String::from_utf8_lossy(&output.stdout);
                assert!(reason.contains(if mode == "timeout" {
                    "summary execution deadline reached"
                } else {
                    "reduce complete projection"
                }));
                if mode == "timeout" {
                    let db = rusqlite::Connection::open(&fixture.database).unwrap();
                    let payload:String=db.query_row("SELECT payload FROM event_log WHERE event_type='summary_transport_observed' ORDER BY id DESC LIMIT 1",[],|row|row.get(0)).unwrap();
                    let observed: Value = serde_json::from_str(&payload).unwrap();
                    let observation = &observed["SummaryTransportObserved"]["observation"];
                    assert_eq!(observation["primary_stop"], "summary_execution_deadline");
                    assert_eq!(observation["worker_settled"], true);
                    assert_eq!(observation["valid_usage_observed"], false);
                }
            }
            assert_eq!(stored_checkpoint(&fixture), old_checkpoint);
            assert_eq!(originals(&fixture), old_records);
            let expected_id = old_checkpoint
                .as_ref()
                .map(|payload| serde_json::from_str::<Value>(payload).unwrap()["id"].clone())
                .unwrap_or(Value::Null);
            assert_eq!(context(&fixture)["summary_checkpoint_id"], expected_id);
            assert!(server.requests.try_recv().is_err());
        }
    }
}

#[test]
fn failed_formatter_stays_historical_in_summary_and_old_policy_does_not_approve_write() {
    let server = Server::start();
    let fixture = CliFixture::new(&server.endpoint);
    owner_policy(&fixture, "full-access");
    std::fs::write(
        fixture.workspace.join("older.txt"),
        format!("{CANARY}\n{}", "older data\n".repeat(4000)),
    )
    .unwrap();
    observe(&server, &fixture, "older.txt", "summary-old-read");
    std::fs::create_dir(fixture.workspace.join("src")).unwrap();
    std::fs::write(
        fixture.workspace.join("Cargo.toml"),
        "[package]\nname=\"f06b-failure-fixture\"\nversion=\"0.1.0\"\nedition=\"2021\"\n",
    )
    .unwrap();
    std::fs::write(
        fixture.workspace.join("src/main.rs"),
        "fn main(){println!(\"unformatted\");}\n",
    )
    .unwrap();
    let mut verification = Value::Null;
    let verified = thread::scope(|scope| {
        let process = scope.spawn(|| {
            fixture.run(&[
                "run",
                "--prompt",
                "Record the real formatter outcome without relabeling it",
                "--json",
            ])
        });
        server.request();
        server.tool(
            "summary-format-check",
            "verify",
            json!({"runners":["cargo-fmt-check"]}),
        );
        verification = tool_reply(&server.request(), "summary-format-check");
        server.complete();
        process.join().unwrap()
    });
    assert!(verified.status.success());
    assert_eq!(
        verification[0]["state"], "failed",
        "fixture must actually fail formatting"
    );
    let recent = observe(&server, &fixture, "fixture.txt", "summary-recent-read");
    let old_records = originals(&fixture);
    let result = compact(&server, &fixture);
    assert_eq!(result["status"], "success");
    assert_eq!(originals(&fixture), old_records);
    let checkpoint: Value = serde_json::from_str(&stored_checkpoint(&fixture).unwrap()).unwrap();
    let anchors = checkpoint["evidence_anchors"].to_string();
    assert!(anchors.contains("cargo-fmt-check"));
    assert!(anchors.contains("failed"));
    std::fs::write(fixture.workspace.join("src/main.rs"), "fn main() {}\n").unwrap();
    owner_policy(&fixture, "guided");
    let output = thread::scope(|scope| {
        let process = scope.spawn(|| {
            fixture.run(&[
                "run",
                "--prompt",
                "Review historical verification and attempt the next change",
                "--json",
            ])
        });
        let request = server.request();
        assert!(request.to_string().contains("historical_lossy_summary"));
        assert!(request.to_string().contains("cargo-fmt-check"));
        assert!(request.to_string().contains("failed"));
        server.tool(
            "new-guided-edit",
            "edit_text",
            json!({"path":"fixture.txt", "expected_sha256":recent["sha256"],
            "old_text":FILE_CONTENT, "new_text":"must not be written"}),
        );
        process.join().unwrap()
    });
    assert_eq!(output.status.code(), Some(1));
    assert!(String::from_utf8(output.stdout)
        .unwrap()
        .contains("approval_required"));
    assert_eq!(
        std::fs::read_to_string(fixture.workspace.join("fixture.txt")).unwrap(),
        FILE_CONTENT
    );
    assert!(server.requests.try_recv().is_err());
    println!("F06B: actual cargo-fmt-check failed and remains failed in historical host anchors; subsequent Guided edit remains approval_required");
}

#[test]
fn summary_sources_exclude_process_payloads_private_files_and_automatic_instruction_bodies() {
    let server = Server::start();
    let fixture = CliFixture::new(&server.endpoint);
    owner_policy(&fixture, "full-access");
    std::fs::write(
        fixture.workspace.join("older.txt"),
        format!("{CANARY}\n{}", "historical source\n".repeat(2500)),
    )
    .unwrap();
    std::fs::write(fixture.workspace.join(".env"), "F06B_EXCLUDED_PRIVATE_FILE").unwrap();
    std::fs::write(
        fixture.workspace.join("AGENTS.md"),
        "F06B_AUTOMATIC_INSTRUCTION_OLD",
    )
    .unwrap();
    let output = thread::scope(|scope| {
        let process = scope.spawn(|| {
            fixture.run(&[
                "run",
                "--prompt",
                "Observe the controlled fixture sources",
                "--json",
            ])
        });
        server.request();
        server.tool(
            "privacy-older-read",
            "read_text",
            json!({"path":"older.txt","start_byte":0,"end_byte":std::fs::metadata(fixture.workspace.join("older.txt")).unwrap().len()}),
        );
        server.request();
        server.tool("privacy-excluded-read", "read_text", json!({"path":".env"}));
        assert!(server
            .request()
            .to_string()
            .contains("F06B_EXCLUDED_PRIVATE_FILE"));
        server.tool("privacy-process", "execute_shell", json!({"command":"Write-Output 'F06B_PRIVATE_PROCESS_BODY_AND_ARGUMENT'", "timeout_ms":5000}));
        assert!(server
            .request()
            .to_string()
            .contains("F06B_PRIVATE_PROCESS_BODY_AND_ARGUMENT"));
        server.complete();
        process.join().unwrap()
    });
    assert!(output.status.success());
    observe(&server, &fixture, "fixture.txt", "privacy-recent-read");
    let compacted = thread::scope(|scope| {
        let process = scope.spawn(|| fixture.run(&["compact", "--json"]));
        let request = summary_request(&server);
        let sources = request["messages"][1]["content"].as_str().unwrap();
        assert!(sources.contains(CANARY));
        assert!(sources.contains("metadata_only"));
        assert!(!sources.contains("F06B_PRIVATE_PROCESS_BODY_AND_ARGUMENT"));
        assert!(!sources.contains("F06B_EXCLUDED_PRIVATE_FILE"));
        assert!(!sources.contains("F06B_AUTOMATIC_INSTRUCTION_OLD"));
        reply_summary(&server, &summary_for(&request));
        process.join().unwrap()
    });
    assert!(
        compacted.status.success(),
        "{}",
        String::from_utf8_lossy(&compacted.stdout)
    );
    let saved = stored_checkpoint(&fixture).unwrap();
    for excluded in [
        "F06B_PRIVATE_PROCESS_BODY_AND_ARGUMENT",
        "F06B_EXCLUDED_PRIVATE_FILE",
        "F06B_AUTOMATIC_INSTRUCTION_OLD",
    ] {
        assert!(!saved.contains(excluded));
    }
    std::fs::write(
        fixture.workspace.join("AGENTS.md"),
        "F06B_AUTOMATIC_INSTRUCTION_NEW",
    )
    .unwrap();
    let next = thread::scope(|scope| {
        let process = scope.spawn(|| {
            fixture.run(&[
                "run",
                "--prompt",
                "Continue under fresh instructions",
                "--json",
            ])
        });
        let request = server.request();
        let text = request.to_string();
        assert!(text.contains("F06B_AUTOMATIC_INSTRUCTION_NEW"));
        assert!(text.contains("historical_lossy_summary"));
        for excluded in [
            "F06B_PRIVATE_PROCESS_BODY_AND_ARGUMENT",
            "F06B_EXCLUDED_PRIVATE_FILE",
            "F06B_AUTOMATIC_INSTRUCTION_OLD",
        ] {
            assert!(!text.contains(excluded));
        }
        server.complete();
        process.join().unwrap()
    });
    assert!(next.status.success());
    assert!(server.requests.try_recv().is_err());
}

#[test]
fn schema13_migration_and_malformed_checkpoint_recovery_are_provider_free() {
    let server = Server::start();
    let fixture = CliFixture::new(&server.endpoint);
    seed(&server, &fixture, 1800);
    let retained = originals(&fixture);
    let connection = rusqlite::Connection::open(&fixture.database).unwrap();
    connection
        .execute_batch(
            "DROP TABLE context_checkpoints; DROP TABLE model_budgets; PRAGMA user_version=13",
        )
        .unwrap();
    drop(connection);
    let migrated = context(&fixture);
    assert_eq!(migrated["schema_version"], 16);
    assert!(migrated["accounting_error"]
        .as_str()
        .unwrap()
        .contains("unknown"));
    assert_eq!(originals(&fixture), retained);
    assert!(server.requests.try_recv().is_err());
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
    compact(&server, &fixture);
    let connection = rusqlite::Connection::open(&fixture.database).unwrap();
    connection
        .execute("UPDATE context_checkpoints SET payload='{}'", [])
        .unwrap();
    drop(connection);
    let rejected = fixture.run(&["context", "--json"]);
    assert_eq!(rejected.status.code(), Some(1));
    assert_eq!(
        serde_json::from_slice::<Value>(&rejected.stdout).unwrap()["status"],
        "failed"
    );
    assert!(fixture
        .run(&["compact", "--deactivate", "--json"])
        .status
        .success());
    assert!(context(&fixture)["summary_checkpoint_id"].is_null());
    assert_eq!(originals(&fixture), retained);
    assert!(server.requests.try_recv().is_err());
}

#[test]
fn indivisible_current_exchange_overflow_fails_without_a_summarizer_or_tool_replay() {
    let server = Server::start();
    let fixture = CliFixture::new(&server.endpoint);
    seed(&server, &fixture, 1800);
    compact(&server, &fixture);
    let checkpoint = stored_checkpoint(&fixture);
    // Admit the initial current catalog/checkpoint, then reject the completed
    // large required exchange. A historical fixed number must not make this a
    // different test which starts summarization before the requested read.
    let initial_input = context(&fixture)["accounting"]["accounted_input"]
        .as_u64()
        .unwrap();
    let capacity = initial_input + 4096 + 1024 + 512;
    assert!(fixture
        .run(&[
            "provider",
            "budget",
            "f01-fixture",
            "--limits",
            &format!("{capacity},{capacity},8192,4096,1024"),
            "--json"
        ])
        .status
        .success());
    let output = thread::scope(|scope| {
        let process = scope.spawn(|| {
            fixture.run(&[
                "run",
                "--prompt",
                "Read the required indivisible current exchange",
                "--json",
            ])
        });
        server.request();
        server.tool(
            "required-overflow",
            "read_text",
            json!({"path":"older.txt","start_byte":0,"end_byte":std::fs::metadata(fixture.workspace.join("older.txt")).unwrap().len()}),
        );
        process.join().unwrap()
    });
    assert_eq!(output.status.code(), Some(1));
    assert!(String::from_utf8(output.stdout)
        .unwrap()
        .contains("context_token_capacity_exceeded"));
    assert_eq!(stored_checkpoint(&fixture), checkpoint);
    assert!(server.requests.try_recv().is_err());
    let occurrences = originals(&fixture)
        .iter()
        .filter(|(_, _, payload)| payload.contains("required-overflow"))
        .count();
    assert_eq!(
        occurrences, 1,
        "the completed tool group must be retained once, not replayed"
    );
}

#[test]
fn owned_process_stop_after_checkpoint_commit_reopens_the_new_complete_projection() {
    let server = Server::start();
    let fixture = CliFixture::new(&server.endpoint);
    seed(&server, &fixture, 1800);
    let retained = originals(&fixture);
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
    let mut child = ChildGuard(
        fixture
            .command(&[
                "run",
                "--prompt",
                "Continue after a committed summary",
                "--json",
            ])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap(),
    );
    let summary = summary_request(&server);
    reply_summary(&server, &summary_for(&summary));
    let next_main = server.request();
    assert!(next_main.to_string().contains("historical_lossy_summary"));
    let committed =
        stored_checkpoint(&fixture).expect("checkpoint must precede the admitted continuation");
    child.kill().unwrap();
    assert!(
        !child.wait().unwrap().success(),
        "injected termination is not a passing task completion"
    );
    server
        .replies
        .send(json!({"fixture_disconnect":true}))
        .unwrap();
    let settlement = argentum_store::Store::reconcile_recovery(&fixture.database)
        .expect("explicitly reconcile the fault-injected abandoned continuation");
    assert!(settlement.mutation_performed);
    let reopened = context(&fixture);
    let checkpoint: Value = serde_json::from_str(&committed).unwrap();
    assert_eq!(reopened["summary_checkpoint_id"], checkpoint["id"]);
    assert!(
        reopened["runs_without_completed_assistant"]
            .as_u64()
            .unwrap()
            >= 1
    );
    assert_eq!(&originals(&fixture)[..retained.len()], retained.as_slice());
    assert_eq!(stored_checkpoint(&fixture).unwrap(), committed);
    assert!(server.requests.try_recv().is_err());
    println!("F06B injected post-commit process stop: abandoned continuation reconciled and the new complete checkpoint reopened without replay");
}

#[test]
fn local_theoretical_window_cannot_enable_an_unsupported_accounting_route() {
    let server = Server::start();
    let fixture = CliFixture::new(&server.endpoint);
    assert!(fixture
        .run(&["provider", "select", "llama-cpp", "--json"])
        .status
        .success());
    assert!(fixture
        .run(&[
            "provider",
            "budget",
            "llama-cpp",
            "--limits",
            "1000000,1000000,8192,4096,1024",
            "--json"
        ])
        .status
        .success());
    let inspected = context(&fixture);
    assert!(inspected["accounting"].is_null());
    assert!(inspected["token_capacity"].is_null());
    assert!(inspected["accounting_error"]
        .as_str()
        .unwrap()
        .contains("context_accounting_unsupported"));
    assert!(server.requests.try_recv().is_err());
}
