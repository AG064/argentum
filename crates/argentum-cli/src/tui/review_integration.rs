use super::*;
use argentum_runtime::review::{ReviewBody, ReviewRequest};
use std::path::Path;

async fn request(server: &mut Server) -> Value {
    tokio::time::timeout(Duration::from_secs(60), server.requests.recv())
        .await
        .unwrap()
        .unwrap()
}
fn feedback(request: &Value) -> Value {
    let message = request["messages"]
        .as_array()
        .unwrap()
        .iter()
        .rev()
        .find(|m| m["role"] == "tool")
        .unwrap();
    let text = message["content"].as_str().unwrap();
    serde_json::from_str(text).unwrap_or_else(|_| Value::String(text.to_owned()))
}
fn git(root: &Path, args: &[&str]) -> Vec<u8> {
    let out = std::process::Command::new("git")
        .args(args)
        .current_dir(root)
        .output()
        .unwrap();
    assert!(out.status.success(), "fixture git command failed");
    out.stdout
}
fn database_snapshot(path: &Path) -> Vec<String> {
    // Test-only read-only connection. Production review uses its owning host.
    let db =
        rusqlite::Connection::open_with_flags(path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
            .unwrap();
    let names: Vec<String> = db
        .prepare("SELECT name FROM sqlite_master WHERE type='table' ORDER BY name")
        .unwrap()
        .query_map([], |row| row.get(0))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();
    let mut records = Vec::new();
    for name in names {
        let mut statement = db
            .prepare(&format!("SELECT * FROM \"{}\"", name.replace('"', "\"\"")))
            .unwrap();
        let count = statement.column_count();
        let mut rows = statement.query([]).unwrap();
        while let Some(row) = rows.next().unwrap() {
            records.push(format!(
                "{name}:{:?}",
                (0..count)
                    .map(|i| format!("{:?}", row.get_ref(i).unwrap()))
                    .collect::<Vec<_>>()
            ));
        }
    }
    records.sort();
    records
}
fn assert_unchanged(before: &[String], after: &[String]) {
    let changed: Vec<_> = before
        .iter()
        .chain(after)
        .filter(|row| !before.contains(row) || !after.contains(row))
        .filter_map(|row| row.split(':').next())
        .collect();
    assert!(changed.is_empty(), "review changed table rows: {changed:?}");
}
async fn open(h: &mut Harness) -> View {
    h.input.send(InputEvent::Help).unwrap();
    h.input.send(InputEvent::Text("changes".into())).unwrap();
    h.input.send(InputEvent::Enter).unwrap();
    h.view(|v| v.review.contains("latest recorded run")).await;
    h.input.send(InputEvent::Enter).unwrap();
    h.view(|v| v.review.contains("Runtime state:")).await
}
async fn back(h: &mut Harness, marker: &str) {
    h.input.send(InputEvent::Escape).unwrap();
    h.view(|v| v.review.contains(marker)).await;
}

async fn next_restore_approval(h: &mut Harness) -> ApprovalRequest {
    tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            if let AppEvent::ApprovalRequested(request) = h.events.recv().await.unwrap() {
                if request.tool_id == "restore_file" {
                    break request;
                }
            }
        }
    })
    .await
    .expect("restore approval event")
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn f20e_owner_review_restores_one_real_modified_change_once() {
    let parent = tempfile::tempdir().unwrap();
    let workspace = tempfile::Builder::new()
        .prefix("workspace-")
        .tempdir_in(parent.path())
        .unwrap();
    let database = parent.path().join("state.db");
    let mut server = Server::start();
    let mut h =
        Harness::start_configured(&server.endpoint, workspace, Some(database.clone())).await;
    let root = h._directory.path().to_path_buf();
    let before = b"owner note remains\nvalue = 1\n";
    let after = b"owner note remains\nvalue = 2\n";
    std::fs::write(root.join("target.txt"), before).unwrap();
    std::fs::write(root.join("unrelated.txt"), b"staged unrelated\n").unwrap();
    std::fs::write(root.join("staged-sentinel.txt"), b"staged sentinel\n").unwrap();
    git(
        &root,
        &["add", "target.txt", "unrelated.txt", "staged-sentinel.txt"],
    );
    std::fs::write(root.join("unrelated.txt"), b"later unrelated owner bytes\n").unwrap();
    let unrelated = std::fs::read(root.join("unrelated.txt")).unwrap();
    let index = git(&root, &["ls-files", "--stage"]);
    h.control
        .apply_access_review(h.control.access_review("verification").unwrap())
        .await
        .unwrap();

    h.submit("Read target.txt and change only value = 1 to value = 2 using its SHA.");
    request(&mut server).await;
    h.view(|v| v.current_a).await;
    server
        .replies
        .send(Reply::Call("read_text", json!({"path":"target.txt"})))
        .unwrap();
    let read = feedback(&request(&mut server).await);
    server
        .replies
        .send(Reply::Call(
            "edit_text",
            json!({"path":"target.txt","expected_sha256":read["sha256"],"old_text":"value = 1","new_text":"value = 2"}),
        ))
        .unwrap();
    feedback(&request(&mut server).await);
    server.replies.send(Reply::Complete).unwrap();
    h.view(|v| !v.current_a).await;
    assert_eq!(std::fs::read(root.join("target.txt")).unwrap(), after);

    let scope = h.control.runtime.workspace_snapshot().unwrap();
    let project = scope.project.id;
    let session = h.a;
    let ReviewBody::Runs(runs) = h
        .control
        .review_page(project, session, ReviewRequest::Runs { before: None })
        .unwrap()
        .body
    else {
        panic!("run page")
    };
    let run = runs.items[0].run_id;
    let ReviewBody::Run { records, .. } = h
        .control
        .review_page(project, session, ReviewRequest::Run(run))
        .unwrap()
        .body
    else {
        panic!("run detail")
    };
    let change = records[0].file_change_id.unwrap();

    h.input
        .send(InputEvent::Text("draft survives restore review".into()))
        .unwrap();
    let draft = h.view(|v| v.draft.contains("survives")).await;
    open(&mut h).await;
    h.input.send(InputEvent::Enter).unwrap();
    h.view(|v| v.review.contains("Restore this change..."))
        .await;
    h.input.send(InputEvent::Enter).unwrap();
    let preview = h
        .view(|v| {
            v.review.contains("Proposed restoration")
                && v.review
                    .contains("Current state matches recorded postimage: yes")
        })
        .await;
    assert_eq!(preview.draft, draft.draft);
    assert!(preview.review.contains("-value = 2") && preview.review.contains("+value = 1"));

    h.input.send(InputEvent::Escape).unwrap();
    h.view(|v| v.review.contains("Original change diff")).await;
    assert_eq!(std::fs::read(root.join("target.txt")).unwrap(), after);
    h.input.send(InputEvent::Enter).unwrap();
    h.view(|v| v.review.contains("Proposed restoration")).await;
    h.input.send(InputEvent::Enter).unwrap();
    h.view(|v| v.review.contains("Exact owner approval required"))
        .await;
    let approval = next_restore_approval(&mut h).await;
    assert_eq!(approval.run_id, run);
    assert_eq!(approval.target, "target.txt");
    h.input.send(InputEvent::HistoryNext).unwrap();
    h.input.send(InputEvent::Enter).unwrap();
    tokio::time::timeout(Duration::from_secs(10), async {
        while std::fs::read(root.join("target.txt")).unwrap() != before {
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .expect("restored bytes");
    let refreshed = h
        .view(|v| {
            v.review.contains("Runtime state: Complete")
                && !v.review.contains("owner approval required")
        })
        .await;
    assert_eq!(refreshed.draft, draft.draft);
    assert_eq!(refreshed.anchor, draft.anchor);
    assert_eq!(
        std::fs::read(root.join("unrelated.txt")).unwrap(),
        unrelated
    );
    assert_eq!(git(&root, &["ls-files", "--stage"]), index);

    h.control
        .dispatch(AppCommand::ApproveTool {
            approval_id: approval.id,
            scope: ApprovalScope::Once,
        })
        .await
        .unwrap();
    assert_eq!(std::fs::read(root.join("target.txt")).unwrap(), before);
    let db = rusqlite::Connection::open_with_flags(
        &database,
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
    )
    .unwrap();
    let payloads: Vec<String> = db
        .prepare("SELECT payload FROM file_changes WHERE run_id=?1 ORDER BY revision")
        .unwrap()
        .query_map([run.to_string()], |row| row.get(0))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();
    let records: Vec<argentum_domain::FileChangeRecord> = payloads
        .iter()
        .map(|payload| serde_json::from_str(payload).unwrap())
        .collect();
    assert_eq!(records.len(), 2);
    assert_eq!(records[0].id, change);
    assert_eq!(records[1].restored_from, Some(change));
    assert_eq!(
        records[1].origin,
        argentum_domain::FileChangeOrigin::Restore
    );
    let restore_events: i64 = db
        .query_row(
            "SELECT COUNT(*) FROM event_log WHERE run_id=?1 AND event_type='restore_completed'",
            [run.to_string()],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(restore_events, 1);
    drop(db);
    h.input.send(InputEvent::Eof).unwrap();
    let (_, result) = tokio::time::timeout(WATCHDOG, h.task)
        .await
        .unwrap()
        .unwrap();
    result.unwrap();
    drop(h.control);

    let mut config = crate::HostConfig::in_memory(&root);
    config.database = Some(database.clone());
    let mut restarted = CommandHost::start_composed(config).await.unwrap();
    let page = restarted
        .local_handle()
        .review_page(project, session, ReviewRequest::Run(run))
        .unwrap();
    let ReviewBody::Run {
        records, summary, ..
    } = page.body
    else {
        panic!("restart review")
    };
    assert_eq!(records.len(), 2);
    assert!(summary.contains("Runtime state: Complete"));
    assert_eq!(std::fs::read(root.join("target.txt")).unwrap(), before);
    restarted.shutdown().await.unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn f20d_real_read_edit_verify_review_stream_restart_and_read_only() {
    let parent = tempfile::tempdir().unwrap();
    let workspace = tempfile::Builder::new()
        .prefix("workspace-")
        .tempdir_in(parent.path())
        .unwrap();
    let database = parent.path().join("state.db");
    let mut server = Server::start();
    let mut h =
        Harness::start_configured(&server.endpoint, workspace, Some(database.clone())).await;
    let root = h._directory.path().to_path_buf();
    std::fs::create_dir(root.join("src")).unwrap();
    std::fs::write(
        root.join("Cargo.toml"),
        "[package]\nname=\"f20d-fixture\"\nversion=\"0.1.0\"\nedition=\"2021\"\n",
    )
    .unwrap();
    std::fs::write(
        root.join("Cargo.lock"),
        "version = 4\n[[package]]\nname = \"f20d-fixture\"\nversion = \"0.1.0\"\n",
    )
    .unwrap();
    let before =
        "pub fn answer() -> u32 { 41 }\n#[test] fn check() { assert_eq!(answer(), 42); }\n";
    std::fs::write(root.join("src/lib.rs"), before).unwrap();
    std::fs::write(root.join("unrelated.txt"), "index version\n").unwrap();
    std::fs::write(root.join("sentinel.txt"), "staged sentinel\n").unwrap();
    git(
        &root,
        &[
            "add",
            "Cargo.toml",
            "Cargo.lock",
            "src/lib.rs",
            "unrelated.txt",
            "sentinel.txt",
        ],
    );
    std::fs::write(root.join("unrelated.txt"), "user dirty version\n").unwrap();
    let index = git(&root, &["ls-files", "--stage"]);
    h.control
        .apply_access_review(h.control.access_review("verification").unwrap())
        .await
        .unwrap();
    h.submit("Read src/lib.rs, fix answer with its SHA, and run the fixed Cargo verifier.");
    let first = request(&mut server).await;
    h.view(|v| v.current_a).await;
    assert!(!first["tools"]
        .as_array()
        .unwrap()
        .iter()
        .any(|t| t["function"]["name"] == "execute_argv"));
    server
        .replies
        .send(Reply::Call("read_text", json!({"path":"src/lib.rs"})))
        .unwrap();
    let read = feedback(&request(&mut server).await);
    let sha = read["sha256"].as_str().expect("real read SHA");
    server.replies.send(Reply::Call("edit_text",json!({"path":"src/lib.rs","expected_sha256":sha,"old_text":"{ 41 }","new_text":"{ 42 }"}))).unwrap();
    let edit = feedback(&request(&mut server).await);
    assert!(edit.to_string().contains("src/lib.rs"));
    server.replies.send(Reply::Verify).unwrap();
    let verified = feedback(&request(&mut server).await);
    assert!(verified.to_string().contains("passed"), "{verified}");
    server.replies.send(Reply::Complete).unwrap();
    h.view(|v| !v.current_a && !v.paused_a).await;
    let snapshot = h.control.runtime.workspace_snapshot().unwrap();
    let project = snapshot.project.id;
    let session = h.a;
    let page = h
        .control
        .review_page(project, session, ReviewRequest::Runs { before: None })
        .unwrap();
    let ReviewBody::Runs(runs) = page.body else {
        panic!()
    };
    let run = runs.items[0].run_id;
    assert_eq!(runs.items.len(), 1);
    let page = h
        .control
        .review_page(project, session, ReviewRequest::Run(run))
        .unwrap();
    let ReviewBody::Run {
        records, summary, ..
    } = page.body
    else {
        panic!()
    };
    assert_eq!(records.len(), 1);
    assert_eq!(records[0].kind, "modified");
    assert!(
        summary.contains("Complete") && summary.contains("Passed"),
        "{summary}"
    );
    let change = records[0].file_change_id.unwrap();
    let baseline = database_snapshot(&database);
    h.input
        .send(InputEvent::Text(
            "unsent Cyrillic: Привет e\u{301} 界".into(),
        ))
        .unwrap();
    h.input.send(InputEvent::Scroll(4)).unwrap();
    let draft = h.view(|v| v.draft.contains("Привет")).await;
    open(&mut h).await;
    h.input.send(InputEvent::Enter).unwrap();
    let diff = h
        .view(|v| v.review.contains("-pub fn answer()") && v.review.contains("+pub fn answer()"))
        .await;
    assert_eq!(diff.draft, draft.draft);
    assert!(!diff.review.contains("user dirty version") && !diff.review.contains("sentinel.txt"));
    assert!(diff.review.contains("{ 41 }") && diff.review.contains("{ 42 }"));
    back(&mut h, "Runtime state:").await;
    h.input.send(InputEvent::HistoryNext).unwrap();
    h.input.send(InputEvent::Enter).unwrap();
    let verification = h
        .view(|v| v.review.contains("Recorded state: Passed"))
        .await;
    assert!(
        verification.review.contains("cargo-test-locked")
            && verification.review.contains("origin: fixed")
    );
    assert!(
        verification.review.contains("Revision before:")
            && verification
                .review
                .contains("current workspace not checked")
    );
    let db = rusqlite::Connection::open_with_flags(
        &database,
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
    )
    .unwrap();
    let payload: String = db
        .query_row(
            "SELECT payload FROM verification_evidence WHERE run_id=?1",
            [run.to_string()],
            |row| row.get(0),
        )
        .unwrap();
    let stored: argentum_domain::VerificationEvidence = serde_json::from_str(&payload).unwrap();
    drop(db);
    assert!(verification
        .review
        .replace('\n', "")
        .contains(&stored.revision_after.0));
    back(&mut h, "Runtime state:").await;
    back(&mut h, "latest recorded run").await;
    h.input.send(InputEvent::Escape).unwrap();
    let closed = h.view(|v| v.review.trim().is_empty()).await;
    assert_eq!(closed.draft, draft.draft);
    assert_eq!(closed.anchor, draft.anchor);
    assert_unchanged(&baseline, &database_snapshot(&database));
    assert_eq!(index, git(&root, &["ls-files", "--stage"]));
    assert_eq!(
        std::fs::read_to_string(root.join("unrelated.txt")).unwrap(),
        "user dirty version\n"
    );
    assert_eq!(
        std::fs::read_to_string(root.join("sentinel.txt")).unwrap(),
        "staged sentinel\n"
    );

    // Start a later response, then review the prior run while its stream is held.
    h.input.send(InputEvent::Home).unwrap();
    // Ctrl+U is not required: use supported composer deletion to clear the draft.
    for _ in draft.draft.chars() {
        h.input.send(InputEvent::Delete).unwrap();
    }
    h.submit("Later stream remains active during review.");
    request(&mut server).await;
    h.view(|v| v.current_a).await;
    h.input
        .send(InputEvent::Text("keep this second draft".into()))
        .unwrap();
    open(&mut h).await;
    h.input.send(InputEvent::Enter).unwrap();
    h.view(|v| v.review.contains("Stored preimage")).await;
    h.input.send(InputEvent::Scroll(3)).unwrap();
    h.input.send(InputEvent::Resize(43, 24)).unwrap();
    let position = h
        .view(|v| v.width == 43 && v.review_position.is_some())
        .await
        .review_position;
    server.replies.send(Reply::HeldDelta).unwrap();
    let live = h.view(|v| v.review.contains("new output:")).await;
    assert!(live.current_a);
    assert_eq!(live.review_position, position);
    assert_eq!(live.selected, Some(session));
    assert_eq!(live.draft, "keep this second draft");
    assert!(live.review.contains("panel owns navigation"));
    server.replies.send(Reply::Complete).unwrap();
    h.view(|v| !v.current_a && v.review.contains("new output:"))
        .await;
    h.input.send(InputEvent::Resize(100, 32)).unwrap();
    back(&mut h, "Runtime state:").await;
    back(&mut h, "latest recorded run").await;
    h.input.send(InputEvent::Escape).unwrap();
    assert_eq!(
        h.view(|v| v.review.trim().is_empty()).await.draft,
        "keep this second draft"
    );
    assert!(server.requests.try_recv().is_err());
    h.input.send(InputEvent::Eof).unwrap();
    let (_, result) = tokio::time::timeout(WATCHDOG, h.task)
        .await
        .unwrap()
        .unwrap();
    result.unwrap();
    drop(h.control);

    // Clean restart, no provider request or execution. The same production command opens it.
    let mut config = crate::HostConfig::in_memory(&root);
    config.database = Some(database.clone());
    let mut restarted = CommandHost::start_composed(config).await.unwrap();
    let local = restarted.local_handle();
    let mut state = TuiState::new(root.clone(), true);
    let mut events = restarted.subscribe();
    restarted.publish_initial_state().unwrap();
    drain_events(&mut events, &mut state);
    let baseline = database_snapshot(&database);
    state.apply_input(InputEvent::Text("/changes".into()));
    assert_eq!(
        state.apply_input(InputEvent::Enter),
        TuiAction::Ui(menu::Request::Open(menu::View::Changes))
    );
    // Slash interception produces the same owner review request without a SubmitTask.
    menu::open_view(&mut state, &local, menu::View::Changes).unwrap();
    assert!(state
        .overlay_rows(100, 30)
        .unwrap()
        .join("\n")
        .contains(&run.to_string()));
    assert_eq!(
        state.apply_input(InputEvent::Enter),
        TuiAction::Ui(menu::Request::Review)
    );
    review::load_sync(&mut state, &local).unwrap();
    assert_eq!(
        state.apply_input(InputEvent::Enter),
        TuiAction::Ui(menu::Request::Review)
    );
    review::load_sync(&mut state, &local).unwrap();
    let shown = screen::frame(&state, 100, 32).rows.join("\n");
    assert!(shown.contains("{ 41 }") && shown.contains("{ 42 }"));
    assert_unchanged(&baseline, &database_snapshot(&database));
    assert!(local
        .review_page(project, SessionId::new_v4(), ReviewRequest::Run(run))
        .is_err());
    assert!(local
        .review_page(
            RunId::new_v4(),
            session,
            ReviewRequest::Diff { run, change }
        )
        .is_err());
    assert!(server.requests.try_recv().is_err());
    let retain = std::env::var("ARGENTUM_F20D_RETAIN_FIXTURE").as_deref() == Ok("1");
    if retain {
        // Optional disposable fixture for copied-executable review, no provider call.
        for command in [
            AppCommand::BeginOnboarding,
            AppCommand::SetOnboardingGuidance {
                guidance: argentum_domain::GuidanceLevel::Expert,
            },
            AppCommand::SetOnboardingProvider {
                provider_id: "f04-fixture".into(),
            },
            AppCommand::SetOnboardingModel {
                provider_id: "f04-fixture".into(),
                model: "fixture".into(),
            },
            AppCommand::SetOnboardingWorkspace {
                workspace: root.clone(),
            },
            AppCommand::SetOnboardingPermission {
                profile_id: "verification".into(),
            },
            AppCommand::CompleteOnboarding,
        ] {
            local.dispatch(command).await.unwrap();
        }
    }
    restarted.shutdown().await.unwrap();
    if retain {
        let kept = h._directory.keep();
        let parent = parent.keep();
        eprintln!(
            "F20D retained disposable fixture: workspace={} database={} run={} change={} parent={}",
            kept.display(),
            database.display(),
            run,
            change,
            parent.display()
        );
    }
    eprintln!("F20D real read/SHA edit/fixed Passed; stored diff; index/dirty preserved; active-stream draft/focus; restart; all-table read-only comparison passed");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn f20d_real_failed_verifier_overrides_success_prose_and_failed_edit_run_reviewable() {
    let mut server = Server::start();
    let mut h = Harness::start(&server.endpoint).await;
    let root = h._directory.path();
    std::fs::create_dir(root.join("src")).unwrap();
    std::fs::write(
        root.join("Cargo.toml"),
        "[package]\nname=\"f20d-negative\"\nversion=\"0.1.0\"\nedition=\"2021\"\n",
    )
    .unwrap();
    std::fs::write(
        root.join("Cargo.lock"),
        "version = 4\n[[package]]\nname = \"f20d-negative\"\nversion = \"0.1.0\"\n",
    )
    .unwrap();
    std::fs::write(
        root.join("src/lib.rs"),
        "#[test] fn check() { panic!(\"intentional fixture failure\"); }\n",
    )
    .unwrap();
    h.control
        .apply_access_review(h.control.access_review("verification").unwrap())
        .await
        .unwrap();
    h.submit("Run the declared test.");
    request(&mut server).await;
    h.view(|v| v.current_a).await;
    server.replies.send(Reply::Verify).unwrap();
    let receipt = feedback(&request(&mut server).await);
    assert!(receipt.to_string().contains("failed"));
    server.replies.send(Reply::Complete).unwrap();
    h.view(|v| !v.current_a).await;
    let v = open(&mut h).await;
    assert!(v.review.contains("cargo-test-locked=Failed"));
    assert!(v.review.contains("Runtime state: Complete"));
    h.input.send(InputEvent::Enter).unwrap();
    h.view(|v| v.review.contains("Recorded state: Failed") && v.review.contains("exit: Some(101)"))
        .await;
    back(&mut h, "Runtime state:").await;
    back(&mut h, "latest recorded run").await;
    h.input.send(InputEvent::Escape).unwrap();
    h.view(|v| v.review.trim().is_empty()).await;
    h.submit("Create an edit before the provider fails.");
    request(&mut server).await;
    h.view(|v| v.current_a).await;
    server
        .replies
        .send(Reply::Call("read_text", json!({"path":"src/lib.rs"})))
        .unwrap();
    let read = feedback(&request(&mut server).await);
    server.replies.send(Reply::Call("edit_text",json!({"path":"src/lib.rs","expected_sha256":read["sha256"],"old_text":"intentional fixture failure","new_text":"Completed edit before failure"}))).unwrap();
    let written = feedback(&request(&mut server).await);
    assert!(
        std::fs::read_to_string(h._directory.path().join("src/lib.rs"))
            .unwrap()
            .contains("Completed edit before failure"),
        "edit feedback: {written}"
    );
    server.replies.send(Reply::Fail).unwrap();
    for _ in 0..2 {
        request(&mut server).await;
        server.replies.send(Reply::Fail).unwrap();
    }
    h.view(|v| !v.current_a && v.status.contains("failed"))
        .await;
    let v = open(&mut h).await;
    assert!(
        v.review.contains("Runtime state: Failed")
            && v.review.contains("src/lib.rs")
            && v.review.contains("modified"),
        "failed-run review: {}",
        v.review
    );
    h.input.send(InputEvent::Enter).unwrap();
    h.view(|v| v.review.contains("Completed edit before failure") && v.review.contains("+#[test]"))
        .await;
    back(&mut h, "Runtime state:").await;
    back(&mut h, "latest recorded run").await;
    h.input.send(InputEvent::HistoryNext).unwrap();
    h.input.send(InputEvent::Enter).unwrap();
    let earlier = h
        .view(|v| v.review.contains("cargo-test-locked=Failed"))
        .await;
    assert!(earlier.review.contains("Runtime state: Complete"));
    assert_eq!(earlier.selected, Some(h.a));
    h.finish().await;
}
