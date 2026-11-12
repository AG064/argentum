use super::instructions_http::Server;
use super::*;

const MARKER: &str = "REAL06_PUBLIC_VALUE_ASSERTION";

fn request(server: &Server) -> Value {
    let value = server
        .requests
        .recv_timeout(Duration::from_secs(120))
        .expect("bounded verifier request watchdog");
    let names = value["tools"]
        .as_array()
        .unwrap()
        .iter()
        .map(|tool| tool["function"]["name"].as_str().unwrap())
        .collect::<Vec<_>>();
    for required in ["read_text", "edit_text", "verify"] {
        assert!(names.contains(&required));
    }
    assert!(!names.contains(&"execute_argv") && !names.contains(&"execute_shell"));
    value
}

fn run(fixture: &CliFixture) -> Output {
    let mut child=ChildGuard(fixture.command(&["run","--prompt","Synthetic offline REAL06 diagnostic control; observe source, run fixed verification, repair the observed value and verify again.","--json"]).stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped()).spawn().unwrap());
    let stdout = child.stdout.take().unwrap();
    let stderr = child.stderr.take().unwrap();
    let out = thread::spawn(move || read_output(stdout));
    let err = thread::spawn(move || read_output(stderr));
    let deadline = Instant::now() + Duration::from_secs(180);
    let status = loop {
        if let Some(status) = child.try_wait().unwrap() {
            break status;
        }
        if Instant::now() >= deadline {
            child.kill().unwrap();
            child.wait().unwrap();
            panic!("bounded REAL06 process watchdog");
        }
        thread::sleep(Duration::from_millis(5));
    };
    let output = Output {
        status,
        stdout: out.join().unwrap(),
        stderr: err.join().unwrap(),
    };
    if let Some(path) = std::env::var_os("ARGENTUM_REAL06_EVIDENCE_ROOT") {
        let path = PathBuf::from(path).join("settlement-debug");
        std::fs::create_dir_all(&path).unwrap();
        let id = fixture.root.path().file_name().unwrap().to_string_lossy();
        std::fs::write(path.join(format!("{id}-stdout.jsonl")), &output.stdout).unwrap();
        std::fs::write(path.join(format!("{id}-stderr.txt")), &output.stderr).unwrap();
    }
    output
}

#[test]
fn real06_exact_cli_failed_diagnostic_guarded_edit_and_current_pass() {
    let server = Server::start();
    let fixture = CliFixture::new(&server.endpoint);
    fixture.allow_usability_tools_via_owner();
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
    std::fs::create_dir(fixture.workspace.join("src")).unwrap();
    std::fs::create_dir(fixture.workspace.join("tests")).unwrap();
    std::fs::write(fixture.workspace.join("Cargo.toml"),"[package]\nname=\"real06_late_fixture\"\nversion=\"0.1.0\"\nedition=\"2021\"\n[[test]]\nname=\"public_contract\"\npath=\"tests/contract.rs\"\nharness=false\n").unwrap();
    std::fs::write(
        fixture.workspace.join("Cargo.lock"),
        "version = 4\n\n[[package]]\nname = \"real06_late_fixture\"\nversion = \"0.1.0\"\n",
    )
    .unwrap();
    std::fs::write(
        fixture.workspace.join("src/lib.rs"),
        "pub fn value()->u32{7}\n",
    )
    .unwrap();
    let test = r#"fn main(){
        for i in 0..240 { eprintln!("REAL06_BENIGN_SETUP_{i:04}: deterministic setup output, not a verification verdict. 01234567890123456789"); }
        assert_eq!(real06_late_fixture::value(),8,"REAL06_PUBLIC_VALUE_ASSERTION: expected public value 8");
    }"#;
    std::fs::write(fixture.workspace.join("tests/contract.rs"), test).unwrap();
    std::fs::write(
        fixture.workspace.join("unrelated.txt"),
        b"REAL06 untouched CRLF\r\n",
    )
    .unwrap();
    // No staging or commits, even in this fixture. Git index preservation is
    // independent of the application effects and uses an initialized empty index.
    assert!(Command::new("git")
        .args(["init", "--quiet"])
        .current_dir(&fixture.workspace)
        .output()
        .unwrap()
        .status
        .success());
    assert!(Command::new("git")
        .args(["read-tree", "--empty"])
        .current_dir(&fixture.workspace)
        .output()
        .unwrap()
        .status
        .success());
    let index = std::fs::read(fixture.workspace.join(".git/index")).unwrap();
    let baseline = std::env::var_os("ARGENTUM_REAL06_EXPECT_BASELINE").is_some();
    let mut recorded = Vec::new();
    let output = thread::scope(|scope| {
        let child = scope.spawn(|| run(&fixture));
        recorded.push(request(&server));
        server.tool("real06-read", "read_text", json!({"path":"src/lib.rs"}));
        recorded.push(request(&server));
        let read = tool_reply(recorded.last().unwrap(), "real06-read");
        assert!(read["sha256"].is_string());
        server.tool(
            "real06-before",
            "verify",
            json!({"runners":["cargo-test-locked"]}),
        );
        recorded.push(request(&server));
        let failed = tool_reply(recorded.last().unwrap(), "real06-before");
        assert_eq!(failed[0]["state"], "failed");
        assert_eq!(failed[0]["exit_code"], 101);
        assert!(failed.to_string().len() <= argentum_domain::text_contract::MAX_TEXT_RESULT_BYTES);
        assert!(!failed[0]["stderr_truncated"].as_bool().unwrap());
        assert_eq!(failed.to_string().contains(MARKER), !baseline);
        if baseline {
            server.replies.send(json!({"choices":[{"delta":{"content":"Synthetic control: Failed receipt preserved, late assertion unavailable in the preview."},"finish_reason":"stop"}]})).unwrap();
        } else {
            server.tool("real06-repair","edit_text",json!({"path":"src/lib.rs","expected_sha256":read["sha256"],"old_text":"pub fn value()->u32{7}","new_text":"pub fn value()->u32{8}"}));
            recorded.push(request(&server));
            let edit = tool_reply(recorded.last().unwrap(), "real06-repair");
            assert!(edit["sha256"].is_string());
            server.tool(
                "real06-after",
                "verify",
                json!({"runners":["cargo-test-locked"]}),
            );
            recorded.push(request(&server));
            let passed = tool_reply(recorded.last().unwrap(), "real06-after");
            assert_eq!(passed[0]["state"], "passed");
            assert_eq!(passed[0]["exit_code"], 0);
            assert_ne!(passed[0]["id"], failed[0]["id"]);
            assert_ne!(passed[0]["revision_before"], failed[0]["revision_before"]);
            assert_eq!(
                tool_reply(recorded.last().unwrap(), "real06-before")[0]["state"],
                "failed"
            );
            server.replies.send(json!({"choices":[{"delta":{"content":"Scripted offline control: one guarded edit and current fixed verification Passed; prior Failed receipt retained. Not live-model reliability."},"finish_reason":"stop"}]})).unwrap();
        }
        child.join().unwrap()
    });
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(recorded.len(), if baseline { 3 } else { 5 });
    assert_eq!(
        std::fs::read_to_string(fixture.workspace.join("src/lib.rs")).unwrap(),
        if baseline {
            "pub fn value()->u32{7}\n"
        } else {
            "pub fn value()->u32{8}\n"
        }
    );
    assert_eq!(
        std::fs::read_to_string(fixture.workspace.join("tests/contract.rs")).unwrap(),
        test
    );
    assert_eq!(
        std::fs::read(fixture.workspace.join("unrelated.txt")).unwrap(),
        b"REAL06 untouched CRLF\r\n"
    );
    assert_eq!(
        std::fs::read(fixture.workspace.join(".git/index")).unwrap(),
        index
    );
    let inspection = fixture.run(&["verification", "status", "--json"]);
    let state: Value = serde_json::from_slice(&inspection.stdout).unwrap();
    let receipts = state["evidence"].as_array().unwrap();
    assert_eq!(receipts.len(), if baseline { 1 } else { 2 });
    assert!(receipts.iter().any(|r| r["evidence"]["state"] == "failed"));
    if !baseline {
        assert!(receipts
            .iter()
            .any(|r| r["evidence"]["state"] == "passed" && r["fresh"] == true));
    }
    // Configuration reload is local and isolated; no keyring or live probe.
    let local = fixture.run(&["status", "--json"]);
    assert!(local.status.success());
    let config: Value = serde_json::from_slice(&local.stdout).unwrap();
    assert_eq!(config["coding_readiness"]["provider_contact"], false);
    assert_eq!(config["coding_readiness"]["inference_dispatched"], false);
    let db = rusqlite::Connection::open(&fixture.database).unwrap();
    let active: i64 = db
        .query_row(
            "SELECT COUNT(*) FROM runtime_host_generations WHERE state='active'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(active, 0);
    if let Some(path) = std::env::var_os("ARGENTUM_REAL06_EVIDENCE_ROOT") {
        let path = PathBuf::from(path);
        std::fs::create_dir_all(&path).unwrap();
        std::fs::write(
            path.join("actual-next-requests.json"),
            serde_json::to_vec_pretty(&recorded).unwrap(),
        )
        .unwrap();
        std::fs::write(path.join("application-events.jsonl"), &output.stdout).unwrap();
        std::fs::write(path.join("verification-status.json"), &inspection.stdout).unwrap();
        std::fs::write(path.join("report.json"),serde_json::to_vec_pretty(&json!({"phase":if baseline{"baseline"}else{"repaired"},"scripted_not_live_model":true,"candidate":std::env::var_os("ARGENTUM_F01_CLI_EXE").map(PathBuf::from),"loopback_requests":recorded.len(),"fixed_verifier_failed":true,"failure_detail_visible":!baseline,"guarded_edit_and_current_pass":!baseline,"prior_failure_retained":true,"unchanged_index_and_unrelated_source":true,"all_hosts_closed":true,"provider_contact":false,"live_generation_requests":0})).unwrap()).unwrap();
    }
}

#[test]
fn real06_owned_terminal_job_repeats_without_respawn_and_preserves_cursor_spans() {
    let server = Server::start();
    let fixture = CliFixture::new(&server.endpoint);
    fixture.allow_usability_tools_via_owner();
    std::fs::create_dir(fixture.workspace.join("tests")).unwrap();
    std::fs::write(fixture.workspace.join("Cargo.toml"),"[package]\nname=\"real06_poll_fixture\"\nversion=\"0.1.0\"\nedition=\"2021\"\n[[test]]\nname=\"public_contract\"\npath=\"tests/contract.rs\"\nharness=false\n").unwrap();
    std::fs::write(
        fixture.workspace.join("Cargo.lock"),
        "version = 4\n\n[[package]]\nname = \"real06_poll_fixture\"\nversion = \"0.1.0\"\n",
    )
    .unwrap();
    std::fs::write(fixture.workspace.join("tests/contract.rs"),r#"fn main(){for i in 0..240{eprintln!("REAL06_STDERR_PADDING_{i:04}: setup data, not a verdict. 0123456789012345678901234567890");}assert_eq!(7,8,"REAL06_PUBLIC_VALUE_ASSERTION");}"#).unwrap();
    let mut recorded = Vec::new();
    let output = thread::scope(|scope| {
        let child = scope.spawn(|| run(&fixture));
        recorded.push(request(&server));
        server.tool(
            "real06-job",
            "start_verification",
            json!({"runner":"cargo-test-locked"}),
        );
        recorded.push(request(&server));
        let started = tool_reply(recorded.last().unwrap(), "real06-job");
        let job = started["job_id"].as_str().unwrap().to_owned();
        let mut terminal = None;
        for index in 0..16 {
            let call = format!("real06-poll-{index}");
            // Repeat cursor zero deliberately. Observation is non-destructive.
            server.tool(
                &call,
                "poll_process_job",
                json!({"job_id":job,"stdout_cursor":0,"stderr_cursor":0,"wait_ms":30000}),
            );
            recorded.push(request(&server));
            let poll = tool_reply(recorded.last().unwrap(), &call);
            if poll["verification"].is_object() {
                terminal = Some(poll);
                break;
            }
        }
        let terminal = terminal.expect("real fixed Cargo job must settle within bounded polls");
        assert_eq!(terminal["verification"]["state"], "failed");
        assert_eq!(terminal["exit_code"], 101);
        assert!(terminal["diagnostics"]["stderr"]
            .to_string()
            .contains(MARKER));
        server.tool(
            "real06-repeat",
            "poll_process_job",
            json!({"job_id":job,"stdout_cursor":0,"stderr_cursor":0,"wait_ms":0}),
        );
        recorded.push(request(&server));
        let repeated = tool_reply(recorded.last().unwrap(), "real06-repeat");
        assert_eq!(repeated, terminal);
        for stream in ["stdout", "stderr"] {
            let part = &repeated[stream];
            let text = part["text"].as_str().unwrap();
            assert_eq!(
                part["to_cursor"].as_u64().unwrap() - part["from_cursor"].as_u64().unwrap(),
                text.len() as u64
            );
            assert_eq!(part["next_cursor"], part["to_cursor"]);
        }
        assert!(
            repeated.to_string().len() <= argentum_domain::text_contract::MAX_TEXT_RESULT_BYTES
        );
        server.replies.send(json!({"choices":[{"delta":{"content":"Scripted owned-job control received Failed detail twice without restart."},"finish_reason":"stop"}]})).unwrap();
        child.join().unwrap()
    });
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let messages = String::from_utf8(output.stdout.clone())
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str::<Value>(line).unwrap())
        .collect::<Vec<_>>();
    let starts = messages
        .iter()
        .filter(|value| value["event"]["ToolStarted"]["tool_id"] == "start_verification")
        .count();
    assert_eq!(starts, 1);
    let receipt_ids = messages
        .iter()
        .filter_map(|v| v["event"]["VerificationEvidenceRecorded"]["id"].as_str())
        .collect::<std::collections::BTreeSet<_>>();
    assert_eq!(
        receipt_ids.len(),
        1,
        "repeated polls may redeliver the same receipt, not create a new invocation"
    );
    // Fresh run ownership must reject the old handle, not expose retained text.
    let second = thread::scope(|scope| {
        let child = scope.spawn(|| run(&fixture));
        request(&server);
        let job = tool_reply(&recorded[1], "real06-job")["job_id"]
            .as_str()
            .unwrap()
            .to_owned();
        server.tool(
            "foreign-job",
            "poll_process_job",
            json!({"job_id":job,"stdout_cursor":0,"stderr_cursor":0,"wait_ms":0}),
        );
        let request = request(&server);
        let foreign = tool_reply(&request, "foreign-job");
        assert!(!foreign.to_string().contains(MARKER));
        server.replies.send(json!({"choices":[{"delta":{"content":"Old owned handle is unavailable to the new run."},"finish_reason":"stop"}]})).unwrap();
        child.join().unwrap()
    });
    assert!(second.status.success());
    if let Some(path) = std::env::var_os("ARGENTUM_REAL06_EVIDENCE_ROOT") {
        let path = PathBuf::from(path).join("owned-job");
        std::fs::create_dir_all(&path).unwrap();
        std::fs::write(
            path.join("requests.json"),
            serde_json::to_vec_pretty(&recorded).unwrap(),
        )
        .unwrap();
        std::fs::write(path.join("events.jsonl"), &output.stdout).unwrap();
        std::fs::write(path.join("report.json"),serde_json::to_vec_pretty(&json!({"scripted_not_live_model":true,"one_start":true,"one_receipt":true,"repeated_terminal_projection_identical":true,"cursor_spans_contiguous":true,"foreign_run_denied":true,"failed_detail_seen":true,"no_live_requests":true})).unwrap()).unwrap();
    }
}
