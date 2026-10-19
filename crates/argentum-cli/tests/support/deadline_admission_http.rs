//! Parent-deadline admission controls. All provider traffic is synthetic loopback.
use super::*;

fn deliver_with_parent_seconds(fixture: &CliFixture, goal: &str, seconds: &str) -> Output {
    let mut child = ChildGuard(
        fixture
            .command(&[
                "goal",
                "deliver",
                "--goal-id",
                goal,
                "--iterations",
                "8",
                "--elapsed-seconds",
                seconds,
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
    let watchdog = Instant::now() + Duration::from_secs(seconds.parse::<u64>().unwrap() + 15);
    let status = loop {
        if let Some(status) = child.try_wait().unwrap() {
            break status;
        }
        assert!(
            Instant::now() < watchdog,
            "bounded parent-deadline control watchdog"
        );
        thread::sleep(Duration::from_millis(10));
    };
    Output {
        status,
        stdout: out.join().unwrap(),
        stderr: err.join().unwrap(),
    }
}

fn retain(label: &str, value: &Value) {
    if let Some(root) = std::env::var_os("ARGENTUM_DEADLINE_EVIDENCE_ROOT") {
        let root = PathBuf::from(root);
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(
            root.join(format!("{label}.json")),
            serde_json::to_vec_pretty(value).unwrap(),
        )
        .unwrap();
    }
}

#[test]
fn native_parent_deadline_tail_stops_before_another_implementation_and_resumes() {
    let server = Server::start();
    let fixture = CliFixture::new(&server.endpoint);
    let goal = prepare(&fixture, 8);
    json_output(fixture.run(&[
        "goal",
        "limits",
        "--goal-id",
        &goal,
        "--limits",
        "2000000,32,8,32,600,1000000,32,16,18",
        "--json",
    ]));
    let mut requests = Vec::new();
    let mut late_request = None;
    let first = thread::scope(|scope| {
        let child = scope.spawn(|| deliver_with_parent_seconds(&fixture, &goal, "18"));
        requests.push(next(&server));
        assert!(!requests[0]["tools"].as_array().unwrap().is_empty());
        thread::sleep(Duration::from_secs(13));
        tool(
            &server,
            "parent-tail-read",
            "read_text",
            json!({"path":"src/lib.rs"}),
        );
        let handoff = next(&server);
        assert!(handoff["messages"]
            .to_string()
            .contains("native_handoff_feedback"));
        assert!(handoff["tools"]
            .as_array()
            .is_none_or(|tools| tools.is_empty()));
        assert!(tool_reply(&handoff, "parent-tail-read")["sha256"].is_string());
        requests.push(handoff);
        thread::sleep(Duration::from_secs(2));
        text(
            &server,
            r#"{"status":"continue","remaining":["Complete the public implementation using fresh observations."]}"#,
            true,
        );
        if let Ok(request) = server.requests.recv_timeout(Duration::from_secs(5)) {
            // The preserved old executable starts a fresh coding stage in the
            // parent's short tail. Preserve that actual request and let its
            // existing parent deadline cancel a bounded synthetic response.
            late_request = Some(request.clone());
            requests.push(request);
            server.replies.send(json!({"fixture_first_event_delay_ms":5000,
                "fixture_frames":[{"choices":[{"delta":{"content":"{\"status\":\"continue\",\"remaining\":[\"Retained public work.\"]}"},"finish_reason":"stop"}],"usage":{"total_tokens":10}}]})).unwrap();
        }
        child.join().unwrap()
    });
    let first_result = json_output(first);
    let first_status = json_output(fixture.run(&["goal", "status", "--json"]));
    retain(
        "parent-tail-first",
        &json!({"requests":requests,"result":first_result,
        "fresh_status":first_status,"late_request_observed":late_request.is_some(),
        "fixture_root":fixture.root.path(),"real_provider_requests":0}),
    );
    if late_request.is_some() {
        let retained = fixture.root.keep();
        panic!("new implementation was dispatched inside the parent time reserve; retained offline fixture: {}", retained.display());
    }
    assert_eq!(first_result["task_success"], false);
    assert_eq!(first_result["invocation"]["state"], "incomplete");
    assert_eq!(
        first_result["invocation"]["stop_reason"],
        "invocation_time_reserve"
    );
    assert_eq!(
        first_result["invocation"]["attempts"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    assert_eq!(
        first_status["goal"]["accounting"]["counters"]["request_attempts"],
        2
    );
    assert_eq!(
        first_status["goal"]["accounting"]["counters"]["unresolved_tokens"],
        0
    );
    assert_eq!(
        first_status["goal"]["accounting"]["counters"]["outstanding_tokens"],
        0
    );
    assert!(first_result["invocation"]["delivery"]["remaining"]
        .to_string()
        .contains("Complete the public implementation"));

    // Reopening is a separate fresh invocation, with every prior request and
    // stage retained. No expired handle, digest, receipt or grant is replayed.
    let second = thread::scope(|scope| {
        let child = scope.spawn(|| deliver(&fixture, &goal, "8"));
        requests.push(next(&server));
        tool(
            &server,
            "resume-read",
            "read_text",
            json!({"path":"src/lib.rs"}),
        );
        let observed = next(&server);
        let read = tool_reply(&observed, "resume-read");
        requests.push(observed.clone());
        tool(
            &server,
            "resume-edit",
            "edit_text",
            json!({"path":"src/lib.rs",
            "expected_sha256":read["sha256"],"old_text":"    7\n","new_text":"    8\n"}),
        );
        requests.push(next(&server));
        text(&server, r#"{"status":"ready","remaining":[]}"#, true);
        let feedback = next(&server);
        assert!(feedback["messages"]
            .to_string()
            .contains("native_verifier_feedback"));
        assert!(feedback["messages"].to_string().contains("passed"));
        requests.push(feedback);
        text(
            &server,
            r#"{"status":"ready","remaining":[],"summary":"Actual current verification received."}"#,
            true,
        );
        child.join().unwrap()
    });
    let second_result = json_output(second);
    let final_status = json_output(fixture.run(&["goal", "status", "--json"]));
    retain(
        "parent-tail-resumed",
        &json!({"requests":requests,"result":second_result,
        "fresh_status":final_status,"fixture_root":fixture.root.path(),"real_provider_requests":0}),
    );
    let retained = fixture.root.keep();
    assert_eq!(
        second_result["task_success"],
        true,
        "{}; retained {}",
        second_result,
        retained.display()
    );
    assert_eq!(final_status["goal"]["id"], goal);
    assert_eq!(
        final_status["goal"]["accounting"]["counters"]["request_attempts"],
        6
    );
    assert_eq!(
        final_status["goal"]["accounting"]["counters"]["iterations"],
        4
    );
    assert_eq!(
        final_status["goal"]["accounting"]["counters"]["unresolved_tokens"],
        0
    );
    assert_eq!(final_status["supervision"]["completion_current"], true);
    assert!(server.requests.try_recv().is_err());
}
