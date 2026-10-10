use super::*;
use crate::review::{ReviewBody, ReviewRequest, DIFF_INPUT_BYTES};
use argentum_domain::FileChangeRecord;

#[tokio::test]
async fn f21a_diagnostics_review_uses_current_owner_scope_and_metadata_projection() {
    let (_root, runtime) = composed_runtime().await;
    let run = Uuid::new_v4();
    record(
        &runtime,
        run,
        "PRIVATE_PATH_CANARY_f21a.txt",
        None,
        Some(b"PRIVATE_BODY_CANARY_f21a"),
        true,
    );
    let scope = EventScope {
        workspace_key: runtime.workspace_key.clone(),
        project_id: runtime.project_id,
        session_id: Some(runtime.session_id()),
        run_id: Some(run),
    };
    runtime
        .store
        .append_event_scoped(
            &scope,
            &AppEvent::RunStatusChanged {
                run_id: run,
                lifecycle: TaskLifecycle::Failed,
            },
        )
        .unwrap();
    let page = runtime
        .review_page(
            runtime.project_id,
            runtime.session_id(),
            ReviewRequest::Diagnostics(run),
        )
        .unwrap();
    let ReviewBody::Detail {
        text,
        sensitive_path,
    } = page.body
    else {
        panic!("diagnostics must use the bounded detail view")
    };
    assert!(text.contains("Stored outcome: failed"));
    assert!(sensitive_path.is_none());
    assert!(!text.contains("PRIVATE_PATH_CANARY_f21a"));
    assert!(!text.contains("PRIVATE_BODY_CANARY_f21a"));
    assert!(runtime
        .review_page(
            Uuid::new_v4(),
            runtime.session_id(),
            ReviewRequest::Diagnostics(run),
        )
        .is_err());
    assert!(runtime
        .review_page(
            runtime.project_id,
            Uuid::new_v4(),
            ReviewRequest::Diagnostics(run),
        )
        .is_err());
    runtime.shutdown().await.unwrap();
}

fn record(
    runtime: &RuntimeService,
    run: RunId,
    path: &str,
    before: Option<&[u8]>,
    after: Option<&[u8]>,
    retained: bool,
) -> FileChangeRecord {
    let snapshot = |bytes: &[u8]| FileSnapshotRef {
        digest: argentum_changes::sha256(bytes),
        size_bytes: bytes.len() as u64,
        retained,
    };
    let operation = PreparedChangeOperation {
        operation_id: Uuid::new_v4(),
        workspace_key: runtime.workspace_key.clone(),
        project_id: runtime.project_id,
        session_id: runtime.session_id(),
        run_id: run,
        relative_path: path.into(),
        kind: if before.is_none() {
            FileChangeKind::Added
        } else if after.is_none() {
            FileChangeKind::Deleted
        } else {
            FileChangeKind::Modified
        },
        origin: FileChangeOrigin::ToolCall,
        before: before.map(snapshot),
        after: after.map(snapshot),
        before_bytes: before.filter(|_| retained).map(<[u8]>::to_vec),
        after_bytes: after.filter(|_| retained).map(<[u8]>::to_vec),
        additions: 1,
        removals: 1,
        line_count_availability: argentum_domain::LineCountAvailability::Unknown,
        restored_from: None,
    };
    runtime.store.prepare_change_operation(&operation).unwrap();
    runtime
        .store
        .mark_change_operation_applied(operation.operation_id)
        .unwrap();
    runtime
        .store
        .finalize_change_operation(operation.operation_id)
        .unwrap()
}

#[tokio::test]
async fn f20d_stored_diff_disclosure_bounds_foreign_scope_and_later_source() {
    let (root, runtime) = composed_runtime().await;
    let run = Uuid::new_v4();
    let before = "Привет e\u{301} 界\r\nold old old\r\n";
    let after = "Привет e\u{301} 界\r\nnew\r\n";
    let changed = record(
        &runtime,
        run,
        "nested/file.txt",
        Some(before.as_bytes()),
        Some(after.as_bytes()),
        true,
    );
    std::fs::create_dir(root.path().join("nested")).unwrap();
    std::fs::write(
        root.path().join("nested/file.txt"),
        "unrelated later current contents",
    )
    .unwrap();
    let diff = runtime.recorded_change_diff(changed.id).unwrap();
    assert!(diff.available && diff.text.contains("-old old old") && diff.text.contains("+new"));
    assert!(diff.text.contains("Привет e\u{301} 界"));
    assert!(!diff.text.contains("unrelated later current contents"));
    let other = record(
        &runtime,
        Uuid::new_v4(),
        "other.txt",
        None,
        Some(b"other"),
        true,
    );
    assert!(runtime
        .review_page(
            runtime.project_id,
            runtime.session_id(),
            ReviewRequest::Diff {
                run,
                change: other.id
            }
        )
        .is_err());
    assert!(runtime
        .review_page(
            Uuid::new_v4(),
            runtime.session_id(),
            ReviewRequest::Diff {
                run,
                change: changed.id
            }
        )
        .is_err());
    assert!(runtime
        .review_page(runtime.project_id, Uuid::new_v4(), ReviewRequest::Run(run))
        .is_err());
    for (name, body, retained, reason) in [
        (
            "missing.txt",
            vec![b'x'; argentum_store::MAX_CHANGE_SNAPSHOT_BYTES + 1],
            false,
            "missing",
        ),
        ("huge.txt", vec![b'x'; DIFF_INPUT_BYTES + 1], true, "64 KiB"),
        ("long-line.txt", vec![b'x'; 4097], true, "line/line-length"),
        (
            "many-lines.txt",
            b"a\n".repeat(2049),
            true,
            "line/line-length",
        ),
        ("binary.bin", vec![0, 255, 0], true, "Binary"),
        (
            "secret.txt",
            b"api_key=synthetic-only-marker".to_vec(),
            true,
            "filtered",
        ),
        (".env", b"PRIVATE_SENTINEL".to_vec(), true, "policy"),
    ] {
        let r = record(&runtime, Uuid::new_v4(), name, None, Some(&body), retained);
        let diff = runtime.recorded_change_diff(r.id).unwrap();
        assert!(
            !diff.available && diff.text.is_empty() && diff.unavailable_reason.contains(reason),
            "{name}: {}",
            diff.unavailable_reason
        );
    }
    let mut policy = runtime.load_execution_policy().unwrap();
    policy
        .capability_enabled
        .insert("tool.read-text".into(), false);
    runtime
        .store
        .save_execution_policy(runtime.project_id, &policy)
        .unwrap();
    assert!(!runtime.recorded_change_diff(changed.id).unwrap().available);
    let ReviewBody::Run { records, .. } = runtime
        .review_page(
            runtime.project_id,
            runtime.session_id(),
            ReviewRequest::Run(run),
        )
        .unwrap()
        .body
    else {
        panic!()
    };
    assert_eq!(records.len(), 1); // Allowed bounded metadata, not historical content.
    runtime.shutdown().await.unwrap();
}

#[tokio::test]
async fn f20d_failed_state_survives_success_prose_and_changes_are_distinct() {
    let (root, runtime) = composed_runtime().await;
    let run = Uuid::new_v4();
    record(&runtime, run, "created.txt", None, Some(b"new"), true);
    record(
        &runtime,
        run,
        "modified.txt",
        Some(b"before"),
        Some(b"after"),
        true,
    );
    record(&runtime, run, "deleted.txt", Some(b"remove"), None, true);
    let scope = EventScope {
        workspace_key: runtime.workspace_key.clone(),
        project_id: runtime.project_id,
        session_id: Some(runtime.session_id()),
        run_id: Some(run),
    };
    runtime
        .store
        .append_event_scoped(
            &scope,
            &AppEvent::RunStatusChanged {
                run_id: run,
                lifecycle: TaskLifecycle::Failed,
            },
        )
        .unwrap();
    let receipt = argentum_domain::VerificationEvidence {
        id: Uuid::new_v4(),
        run_id: run,
        runner: argentum_domain::VerificationRunnerId::CargoTestLocked,
        registry_version: 1,
        program: "cargo".into(),
        args: vec!["test".into(), "--workspace".into(), "--locked".into()],
        state: argentum_domain::VerificationState::Failed,
        started_at: time::OffsetDateTime::now_utc(),
        duration_ms: 10,
        exit_code: Some(1),
        revision_before: Sha256Digest("a".repeat(64)),
        revision_after: Sha256Digest("a".repeat(64)),
        stdout_sha256: Sha256Digest("b".repeat(64)),
        stderr_sha256: Sha256Digest("c".repeat(64)),
        stdout_bytes: 9,
        stderr_bytes: 0,
        stdout_truncated: false,
        stderr_truncated: false,
        workspace_key: String::new(),
        definition_sha256: None,
        input_scope: None,
        input_before: None,
        input_after: None,
    };
    runtime
        .store
        .save_verification_evidence(runtime.project_id, runtime.session_id(), &receipt)
        .unwrap();
    // Success-looking source/prose has no say in typed verification projection.
    std::fs::write(
        root.path().join("claim.txt"),
        "Everything succeeded, all checks passed",
    )
    .unwrap();
    let page = runtime
        .review_page(
            runtime.project_id,
            runtime.session_id(),
            ReviewRequest::Run(run),
        )
        .unwrap();
    let ReviewBody::Run {
        records, summary, ..
    } = page.body
    else {
        panic!()
    };
    assert!(
        summary.contains("Runtime state: Failed") && summary.contains("cargo-test-locked=Failed"),
        "{summary}"
    );
    assert_eq!(
        records.iter().map(|r| r.kind.as_str()).collect::<Vec<_>>(),
        ["created", "modified", "deleted"]
    );
    assert!(records.iter().all(|r| r.state == "recorded applied"));
    let details = runtime
        .review_page(
            runtime.project_id,
            runtime.session_id(),
            ReviewRequest::Verification(run),
        )
        .unwrap();
    let ReviewBody::Detail { text, .. } = details.body else {
        panic!()
    };
    assert!(text.contains("Recorded state: Failed") && text.contains("exit: Some(1)"));
    assert!(!text.contains("Everything succeeded"));
    runtime.shutdown().await.unwrap();
}

#[tokio::test]
async fn f20e_restore_preview_conflicts_busy_state_and_policy_staleness_preserve_bytes() {
    let root = tempfile::tempdir().expect("workspace");
    let broker = CapabilityBroker::new(root.path(), ApprovalPolicy::default()).expect("broker");
    let workspace = WorkspaceManager::new(broker);
    let tools = ToolRegistry::with_builtins(workspace.clone());
    let runtime = test_runtime(
        Store::open_in_memory().expect("store"),
        ProviderRegistry::default(),
        tools,
        workspace,
    );
    let run = Uuid::new_v4();
    let before = b"owner line\nvalue=before\n";
    let after = b"owner line\nvalue=after\n";
    std::fs::write(root.path().join("target.txt"), after).unwrap();
    let changed = record(&runtime, run, "target.txt", Some(before), Some(after), true);
    let scope = EventScope {
        workspace_key: runtime.workspace_key.clone(),
        project_id: runtime.project_id,
        session_id: Some(runtime.session_id()),
        run_id: Some(run),
    };
    runtime
        .store
        .append_event_scoped(
            &scope,
            &AppEvent::RunStatusChanged {
                run_id: run,
                lifecycle: TaskLifecycle::Failed,
            },
        )
        .unwrap();

    let page = runtime
        .review_page(
            runtime.project_id,
            runtime.session_id(),
            ReviewRequest::RestorePreview {
                run,
                change: changed.id,
            },
        )
        .unwrap();
    let ReviewBody::Restore {
        text, available, ..
    } = page.body
    else {
        panic!("restore preview")
    };
    assert!(available);
    assert!(text.contains("Current state matches recorded postimage: yes"));
    assert!(text.contains("-value=after") && text.contains("+value=before"));

    std::fs::write(root.path().join("target.txt"), b"later owner bytes\n").unwrap();
    let page = runtime
        .review_page(
            runtime.project_id,
            runtime.session_id(),
            ReviewRequest::RestorePreview {
                run,
                change: changed.id,
            },
        )
        .unwrap();
    let ReviewBody::Restore {
        text, available, ..
    } = page.body
    else {
        panic!("conflict preview")
    };
    assert!(!available && text.contains("later work was preserved"));
    assert!(matches!(
        runtime
            .dispatch(AppCommand::RequestRestore {
                file_change_id: changed.id
            })
            .await,
        Err(RuntimeError::StaleApproval)
    ));
    assert_eq!(
        std::fs::read(root.path().join("target.txt")).unwrap(),
        b"later owner bytes\n"
    );

    std::fs::write(root.path().join("target.txt"), after).unwrap();
    let mut events = runtime.subscribe();
    runtime
        .dispatch(AppCommand::RequestRestore {
            file_change_id: changed.id,
        })
        .await
        .unwrap();
    let approval = loop {
        if let AppEvent::ApprovalRequested(request) = events.recv().await.unwrap() {
            break request;
        }
    };
    let submission = runtime
        .submit_bound_task(TaskSubmission {
            id: Uuid::new_v4(),
            run_id: Uuid::new_v4(),
            prompt: "must not start while restoration is pending".into(),
            skill: None,
            answer_contract: None,
            target: SubmissionTarget {
                project_id: runtime.project_id,
                session_id: runtime.session_id(),
                workspace: runtime.workspace_root().to_path_buf(),
            },
        })
        .await;
    assert!(!submission.admitted);
    assert!(
        matches!(&submission.result, Err(RuntimeError::RestoreBusy)),
        "unexpected admission result: {:?}",
        submission.result
    );
    runtime
        .dispatch(AppCommand::RejectTool {
            approval_id: approval.id,
        })
        .await
        .unwrap();

    runtime
        .dispatch(AppCommand::RequestRestore {
            file_change_id: changed.id,
        })
        .await
        .unwrap();
    let stale_approval = loop {
        if let AppEvent::ApprovalRequested(request) = events.recv().await.unwrap() {
            if request.tool_id == "restore_file" {
                break request;
            }
        }
    };
    let original_policy = runtime.load_execution_policy().unwrap();
    let mut changed_policy = original_policy.clone();
    changed_policy
        .capability_enabled
        .insert("tool.read-text".into(), false);
    runtime
        .store
        .save_execution_policy(runtime.project_id, &changed_policy)
        .unwrap();
    assert!(matches!(
        runtime
            .dispatch(AppCommand::ApproveTool {
                approval_id: stale_approval.id,
                scope: ApprovalScope::Once,
            })
            .await,
        Err(RuntimeError::StaleApproval)
    ));
    assert_eq!(
        std::fs::read(root.path().join("target.txt")).unwrap(),
        after
    );
    runtime
        .store
        .save_execution_policy(runtime.project_id, &original_policy)
        .unwrap();
    runtime
        .dispatch(AppCommand::RequestRestore {
            file_change_id: changed.id,
        })
        .await
        .unwrap();
    let final_approval = loop {
        if let AppEvent::ApprovalRequested(request) = events.recv().await.unwrap() {
            if request.tool_id == "restore_file" {
                break request;
            }
        }
    };
    runtime
        .dispatch(AppCommand::ApproveTool {
            approval_id: final_approval.id,
            scope: ApprovalScope::Once,
        })
        .await
        .unwrap();
    assert_eq!(
        std::fs::read(root.path().join("target.txt")).unwrap(),
        before
    );
    assert_eq!(
        drain_events(&mut events)
            .into_iter()
            .filter(|event| matches!(event, AppEvent::RestoreCompleted(_)))
            .count(),
        1
    );
    let summary = runtime
        .review_page(
            runtime.project_id,
            runtime.session_id(),
            ReviewRequest::Run(run),
        )
        .unwrap();
    let ReviewBody::Run { summary, .. } = summary.body else {
        panic!("run summary")
    };
    assert!(summary.contains("Runtime state: Failed"));
    assert_eq!(
        runtime
            .store
            .change_set(runtime.project_id, run)
            .unwrap()
            .unwrap()
            .files
            .len(),
        2
    );
}

#[tokio::test]
async fn f20e_panel_marks_non_modifications_and_unavailable_images_unrestorable() {
    let (root, runtime) = composed_runtime().await;
    let added_run = Uuid::new_v4();
    std::fs::write(root.path().join("added.txt"), b"added").unwrap();
    let added = record(&runtime, added_run, "added.txt", None, Some(b"added"), true);
    let ReviewBody::Change {
        text,
        restore_available,
        ..
    } = runtime
        .review_page(
            runtime.project_id,
            runtime.session_id(),
            ReviewRequest::Diff {
                run: added_run,
                change: added.id,
            },
        )
        .unwrap()
        .body
    else {
        panic!("added change")
    };
    assert!(!restore_available);
    assert!(text
        .contains("creation and deletion restoration remain on the existing expert command path"));
    let ReviewBody::Restore {
        sensitive_path,
        available,
        ..
    } = runtime
        .review_page(
            runtime.project_id,
            runtime.session_id(),
            ReviewRequest::RestorePreview {
                run: added_run,
                change: added.id,
            },
        )
        .unwrap()
        .body
    else {
        panic!("added restore preview")
    };
    assert!(!available);
    assert_eq!(sensitive_path.as_deref(), Some("added.txt"));

    let missing_run = Uuid::new_v4();
    let oversized_before = vec![b'b'; argentum_store::MAX_CHANGE_SNAPSHOT_BYTES + 1];
    let oversized_after = vec![b'a'; argentum_store::MAX_CHANGE_SNAPSHOT_BYTES + 1];
    let missing = record(
        &runtime,
        missing_run,
        "missing.txt",
        Some(&oversized_before),
        Some(&oversized_after),
        false,
    );
    std::fs::write(root.path().join("missing.txt"), &oversized_after).unwrap();
    let ReviewBody::Change {
        text,
        restore_available,
        ..
    } = runtime
        .review_page(
            runtime.project_id,
            runtime.session_id(),
            ReviewRequest::Diff {
                run: missing_run,
                change: missing.id,
            },
        )
        .unwrap()
        .body
    else {
        panic!("missing image")
    };
    assert!(!restore_available);
    assert!(text.contains("Snapshot missing"));
    runtime.shutdown().await.unwrap();
}
