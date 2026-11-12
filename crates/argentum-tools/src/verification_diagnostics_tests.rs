use super::*;
use argentum_security::{ApprovalPolicy, CapabilityBroker};

#[tokio::test]
async fn real06_late_assertion_uses_the_same_captured_output_for_projection() {
    let root = tempfile::tempdir().unwrap();
    std::fs::create_dir(root.path().join("src")).unwrap();
    std::fs::create_dir(root.path().join("tests")).unwrap();
    std::fs::write(root.path().join("Cargo.toml"), "[package]\nname=\"real06_late_fixture\"\nversion=\"0.1.0\"\nedition=\"2021\"\n[[test]]\nname=\"public_contract\"\npath=\"tests/contract.rs\"\nharness=false\n").unwrap();
    std::fs::write(
        root.path().join("Cargo.lock"),
        "version = 4\n\n[[package]]\nname = \"real06_late_fixture\"\nversion = \"0.1.0\"\n",
    )
    .unwrap();
    std::fs::write(root.path().join("src/lib.rs"), "pub fn value()->u32{7}\n").unwrap();
    std::fs::write(root.path().join("tests/contract.rs"), r#"fn main(){
        for i in 0..240 { eprintln!("REAL06_BENIGN_SETUP_{i:04}: deterministic setup output, not a verification verdict. 01234567890123456789"); }
        assert_eq!(real06_late_fixture::value(),8,"REAL06_PUBLIC_VALUE_ASSERTION: expected public value 8");
    }"#).unwrap();
    let workspace = WorkspaceManager::new(
        CapabilityBroker::new(root.path(), ApprovalPolicy::default()).unwrap(),
    );
    let verifier = VerificationRunner::new(workspace, VerificationRegistry::built_in());
    let executions = verifier
        .run_batch_with_output(
            argentum_domain::RunId::new_v4(),
            &[VerificationRunnerId::CargoTestLocked],
            CancellationToken::new(),
        )
        .await
        .unwrap();
    assert_eq!(executions.len(), 1);
    let actual = &executions[0];
    assert_eq!(
        actual.evidence.state,
        argentum_domain::VerificationState::Failed
    );
    assert_eq!(actual.evidence.exit_code, Some(101));
    assert!(!actual.evidence.stderr_truncated);
    let stderr = actual.stderr.as_deref().unwrap();
    let position = stderr.find("REAL06_PUBLIC_VALUE_ASSERTION").unwrap();
    assert!(
        position > 8192,
        "actual assertion must follow actual setup output"
    );
    let feedback = verification_feedback_output(&executions).unwrap();
    let encoded: serde_json::Value = serde_json::from_str(&feedback).unwrap();
    assert_eq!(encoded[0]["state"], "failed");
    assert!(feedback.len() <= argentum_domain::text_contract::MAX_TEXT_RESULT_BYTES);
    let visible = feedback.contains("REAL06_PUBLIC_VALUE_ASSERTION");
    let baseline = std::env::var_os("ARGENTUM_REAL06_EXPECT_BASELINE").is_some();
    assert_eq!(
        visible, !baseline,
        "projection visibility differs from the qualified phase"
    );
    let receipts: Vec<VerificationEvidence> = serde_json::from_str(&feedback).unwrap();
    assert_eq!(receipts[0], actual.evidence);
    if let Some(path) = std::env::var_os("ARGENTUM_REAL06_EVIDENCE_ROOT") {
        let path = PathBuf::from(path);
        std::fs::create_dir_all(&path).unwrap();
        std::fs::write(path.join("captured-stderr.txt"), stderr).unwrap();
        std::fs::write(
            path.join("captured-stdout.txt"),
            actual.stdout.as_deref().unwrap(),
        )
        .unwrap();
        std::fs::write(path.join("feedback.json"), &feedback).unwrap();
        std::fs::write(
            path.join("receipt.json"),
            serde_json::to_vec_pretty(&actual.evidence).unwrap(),
        )
        .unwrap();
        std::fs::write(path.join("visibility.json"),serde_json::to_vec_pretty(&serde_json::json!({"phase":if baseline{"baseline"}else{"repaired"},"same_invocation_capture_and_projection":true,"actual_assertion_utf8_position_in_captured_stderr":position,"visible":visible,"serialized_feedback_bytes":feedback.len(),"fixed_runner_unchanged":true,"process_failed":true,"no_sleep_or_test_order_assumption":true,"no_real_provider_requests":true})).unwrap()).unwrap();
    }
    if !baseline {
        // The actual captured invocation above supplies immutable receipt fields.
        // These synthetic projection controls do not establish process outcomes.
        let mut passed = actual.evidence.clone();
        passed.state = argentum_domain::VerificationState::Passed;
        passed.exit_code = Some(0);
        let fake_failure = vec![argentum_execution::VerificationRunOutput {
            evidence: passed.clone(),
            stdout: Some("error: FAKE_FAILURE_OUTPUT\n".into()),
            stderr: Some(String::new()),
        }];
        let value: serde_json::Value =
            serde_json::from_str(&verification_feedback_output(&fake_failure).unwrap()).unwrap();
        assert_eq!(value[0]["state"], "passed");
        assert_eq!(value[0]["exit_code"], 0);
        let failed_text = vec![argentum_execution::VerificationRunOutput {
            evidence: actual.evidence.clone(),
            stdout: Some("ALL_TESTS_PASSED_THIS_IS_FAKE\n".into()),
            stderr: Some(String::new()),
        }];
        let value: serde_json::Value =
            serde_json::from_str(&verification_feedback_output(&failed_text).unwrap()).unwrap();
        assert_eq!(value[0]["state"], "failed");
        let unavailable = vec![argentum_execution::VerificationRunOutput {
            evidence: actual.evidence.clone(),
            stdout: None,
            stderr: None,
        }];
        let unavailable: serde_json::Value =
            serde_json::from_str(&verification_feedback_output(&unavailable).unwrap()).unwrap();
        assert_eq!(unavailable[0]["state"], "failed");
        assert_eq!(unavailable[0]["feedback"]["output_available"], false);
        assert!(unavailable[0]["feedback"]["stdout"]["capture_complete"].is_null());
        assert!(unavailable[0]["feedback"]["stderr"]["capture_complete"].is_null());
        for status in [
            argentum_domain::VerificationState::Cancelled,
            argentum_domain::VerificationState::TimedOut,
            argentum_domain::VerificationState::Stale,
            argentum_domain::VerificationState::SpawnFailed,
        ] {
            let mut receipt = actual.evidence.clone();
            receipt.state = status;
            let outputs = vec![argentum_execution::VerificationRunOutput {
                evidence: receipt.clone(),
                stdout: None,
                stderr: None,
            }];
            let encoded = verification_feedback_output(&outputs).unwrap();
            let decoded: Vec<VerificationEvidence> = serde_json::from_str(&encoded).unwrap();
            assert_eq!(decoded[0], receipt);
        }
        let rendered = format!(
            "{}\nerror[E0308]: STDOUT_FAILURE_ONLY\n{}",
            "setup\n".repeat(5000),
            "tail\n".repeat(5000)
        );
        let stderr = format!(
            "{}\nthread 'main' panicked at here:\nassertion failed: STDERR_FAILURE_ONLY\n",
            "separate stderr\n".repeat(3000)
        );
        let stream = |kind, text: &str| argentum_execution::ProcessJobStream {
            stream: kind,
            from_cursor: 0,
            to_cursor: text.len() as u64,
            earliest_cursor: 0,
            next_cursor: text.len() as u64,
            available: true,
            gap: false,
            complete: true,
            text: text.into(),
        };
        let snapshot = argentum_execution::ProcessJobSnapshot {
            job_id: "synthetic-owned-projection".into(),
            state: argentum_execution::ProcessJobState::Exited,
            runner: VerificationRunnerId::CargoTestLocked,
            exit_code: Some(101),
            stdout: stream(argentum_execution::ProcessStream::Stdout, &rendered),
            stderr: stream(argentum_execution::ProcessStream::Stderr, &stderr),
            stdout_bytes: rendered.len() as u64,
            stderr_bytes: stderr.len() as u64,
            stdout_truncated: false,
            stderr_truncated: false,
            verification: Some(actual.evidence.clone()),
            detail: None,
        };
        let first = process_job_output(&snapshot).unwrap();
        assert!(first.len() <= argentum_domain::text_contract::MAX_TEXT_RESULT_BYTES);
        assert_eq!(first, process_job_output(&snapshot).unwrap());
        let decoded: serde_json::Value = serde_json::from_str(&first).unwrap();
        assert!(decoded["diagnostics"]["stdout"]
            .to_string()
            .contains("STDOUT_FAILURE_ONLY"));
        assert!(decoded["diagnostics"]["stderr"]
            .to_string()
            .contains("STDERR_FAILURE_ONLY"));
        for (name, original) in [("stdout", &snapshot.stdout), ("stderr", &snapshot.stderr)] {
            let projected = &decoded[name];
            let end = projected["next_cursor"].as_u64().unwrap();
            assert_eq!(projected["from_cursor"], 0);
            assert_eq!(projected["to_cursor"], end);
            assert_eq!(projected["text"], &original.text[..end as usize]);
            assert!(!projected["complete"].as_bool().unwrap());
            assert_eq!(decoded["poll_selection"][format!("{name}_cursor")], end);
        }
        assert_eq!(
            serde_json::from_value::<VerificationEvidence>(decoded["verification"].clone())
                .unwrap(),
            actual.evidence
        );
        let mut unresolved = snapshot.clone();
        unresolved.verification = None;
        unresolved.state = argentum_execution::ProcessJobState::Unresolved;
        let unresolved: serde_json::Value =
            serde_json::from_str(&process_job_output(&unresolved).unwrap()).unwrap();
        assert_eq!(unresolved["state"], "unresolved");
        assert!(unresolved["verification"].is_null());
        // No old process output enters retained receipt serialization.
        assert!(!serde_json::to_string(&actual.evidence)
            .unwrap()
            .contains("PUBLIC_VALUE_ASSERTION"));
        let truncated = argentum_execution::diagnostic_excerpt(
            "opaque incomplete",
            true,
            2048,
            Some(false),
            true,
        )
        .unwrap();
        assert_eq!(truncated.capture_complete, Some(false));
        assert!(truncated.source_gap);
        assert!(truncated.excerpted);
        let disclosure = ProcessJobManager::new();
        let secret = format!(
            "{}\nerror: api_key={}\n",
            "setup\n".repeat(5000),
            "SYNTHETIC_CANARY_".repeat(100)
        );
        let permitted = disclosure.disclose_process_text(&secret);
        assert!(!permitted.contains("SYNTHETIC_CANARY_"));
        let excerpt =
            argentum_execution::diagnostic_excerpt(&permitted, true, 4096, Some(true), false)
                .unwrap();
        assert!(!serde_json::to_string(&excerpt)
            .unwrap()
            .contains("SYNTHETIC_CANARY_"));
    }
}

#[test]
fn real06_same_retained_baseline_capture_projects_failure_without_respawn() {
    let Some(root) = std::env::var_os("ARGENTUM_REAL06_BASELINE_CAPTURE") else {
        return;
    };
    let root = PathBuf::from(root);
    let receipt: VerificationEvidence =
        serde_json::from_slice(&std::fs::read(root.join("receipt.json")).unwrap()).unwrap();
    let stdout = std::fs::read_to_string(root.join("captured-stdout.txt")).unwrap();
    let stderr = std::fs::read_to_string(root.join("captured-stderr.txt")).unwrap();
    let before = std::fs::read_to_string(root.join("feedback.json")).unwrap();
    assert!(!before.contains("REAL06_PUBLIC_VALUE_ASSERTION"));
    let executions = vec![argentum_execution::VerificationRunOutput {
        evidence: receipt.clone(),
        stdout: Some(stdout),
        stderr: Some(stderr),
    }];
    let after = verification_feedback_output(&executions).unwrap();
    assert!(after.contains("REAL06_PUBLIC_VALUE_ASSERTION"));
    assert!(after.len() <= argentum_domain::text_contract::MAX_TEXT_RESULT_BYTES);
    let receipts: Vec<VerificationEvidence> = serde_json::from_str(&after).unwrap();
    assert_eq!(receipts[0], receipt);
    if let Some(path) = std::env::var_os("ARGENTUM_REAL06_EVIDENCE_ROOT") {
        let path = PathBuf::from(path);
        std::fs::create_dir_all(&path).unwrap();
        std::fs::write(path.join("same-capture-repaired-feedback.json"), after).unwrap();
    }
}
