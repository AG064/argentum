#![cfg(windows)]

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use argentum_domain::{
    now, ConversationMessage, ConversationMessageStatus, ConversationRole, MessageId, RunId,
};
use argentum_store::Store;
use serde_json::{json, Value};

fn executable() -> PathBuf {
    std::env::var_os("ARGENTUM_F09_CLI_EXE")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(env!("CARGO_BIN_EXE_argentum")))
}

fn command_request(request_id: &str, command: Value) -> String {
    serde_json::to_string(&json!({
        "protocol_version": 1,
        "request_id": request_id,
        "type": "command",
        "command": command,
    }))
    .unwrap()
}

fn run_server(workspace: &Path, database: &Path, requests: &[String]) -> Vec<Value> {
    let mut child = Command::new(executable())
        .args([
            "serve",
            "--workspace",
            workspace.to_str().unwrap(),
            "--database",
            database.to_str().unwrap(),
            "--isolated",
            "--jsonl",
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("start JSONL executable");
    {
        let mut input = child.stdin.take().expect("server stdin");
        for request in requests {
            writeln!(input, "{request}").expect("write request");
        }
    }
    let output = child.wait_with_output().expect("wait for JSONL executable");
    assert!(
        output.status.success(),
        "server failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout)
        .expect("UTF-8 JSONL")
        .lines()
        .map(|line| serde_json::from_str(line).expect("JSONL response"))
        .collect()
}

fn run_server_discarding_delivery(workspace: &Path, database: &Path, request: &str) {
    let mut child = Command::new(executable())
        .args([
            "serve",
            "--workspace",
            workspace.to_str().unwrap(),
            "--database",
            database.to_str().unwrap(),
            "--isolated",
            "--jsonl",
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .expect("start JSONL executable");
    writeln!(child.stdin.take().unwrap(), "{request}").expect("write request");
    let output = child.wait_with_output().expect("wait for JSONL executable");
    assert!(
        output.status.success(),
        "server failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

fn sessions(workspace: &Path, database: &Path) -> Value {
    let output = Command::new(executable())
        .args([
            "sessions",
            "--workspace",
            workspace.to_str().unwrap(),
            "--database",
            database.to_str().unwrap(),
            "--isolated",
            "--json",
        ])
        .output()
        .expect("sessions command");
    assert!(
        output.status.success(),
        "sessions failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).expect("sessions JSON")
}

fn select_session(workspace: &Path, database: &Path, session_id: &str) {
    let output = Command::new(executable())
        .args([
            "session",
            "select",
            session_id,
            "--workspace",
            workspace.to_str().unwrap(),
            "--database",
            database.to_str().unwrap(),
            "--isolated",
            "--json",
        ])
        .output()
        .expect("select session");
    assert!(
        output.status.success(),
        "session selection failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

fn seed_settled_source(workspace: &Path, database: &Path) -> (String, String) {
    let store = Store::open(database).expect("store");
    let resolution = store.resolve_workspace(workspace).expect("workspace");
    let project_id = resolution.snapshot.project.id;
    let session_id = resolution.snapshot.active_session_id.expect("session");
    let run_id = RunId::new_v4();
    let mut cutoff = None;
    for (role, text) in [
        (ConversationRole::User, "source request"),
        (ConversationRole::Assistant, "settled source response"),
    ] {
        let id = MessageId::new_v4();
        store
            .append_conversation_message(&ConversationMessage {
                id,
                project_id,
                session_id,
                run_id,
                role,
                text: text.into(),
                reasoning: String::new(),
                usage: None,
                profile_id: if role == ConversationRole::Assistant {
                    "fixture".into()
                } else {
                    String::new()
                },
                model: if role == ConversationRole::Assistant {
                    "fixture".into()
                } else {
                    String::new()
                },
                status: ConversationMessageStatus::Complete,
                created_at: now(),
            })
            .expect("message");
        cutoff = Some(id);
    }
    (session_id.to_string(), cutoff.expect("cutoff").to_string())
}

#[test]
fn lost_committed_new_session_delivery_replays_after_restart_without_reselection() {
    let fixture = tempfile::tempdir().expect("fixture");
    let workspace = fixture.path().join("workspace");
    std::fs::create_dir(&workspace).expect("workspace directory");
    std::fs::write(workspace.join("unrelated.txt"), b"staged baseline\r\n")
        .expect("unrelated baseline");
    for args in [&["init", "--quiet"][..], &["add", "--", "unrelated.txt"]] {
        assert!(Command::new("git")
            .args(args)
            .current_dir(&workspace)
            .output()
            .expect("git fixture")
            .status
            .success());
    }
    std::fs::write(workspace.join("unrelated.txt"), b"preserved user bytes\r\n")
        .expect("unrelated user edit");
    let index_before = std::fs::read(workspace.join(".git/index")).expect("git index");
    let unrelated_before = std::fs::read(workspace.join("unrelated.txt")).expect("unrelated bytes");
    let database = fixture.path().join("state.db");
    let request = command_request("lost-new-session", json!({"kind": "new_session"}));

    run_server_discarding_delivery(&workspace, &database, &request);
    let first_state = sessions(&workspace, &database);
    let first_sessions = first_state["rows"].as_array().unwrap();
    assert_eq!(first_sessions.len(), 2);
    let created_session = first_sessions
        .iter()
        .find(|session| session["selected"] == true)
        .and_then(|session| session["id"].as_str())
        .unwrap()
        .to_owned();
    let initial_session = first_sessions
        .iter()
        .map(|session| session["id"].as_str().unwrap())
        .find(|session_id| *session_id != created_session)
        .unwrap()
        .to_owned();

    select_session(&workspace, &database, &initial_session);
    let responses = run_server(&workspace, &database, &[request]);
    let replay = responses
        .iter()
        .find(|response| {
            response["request_id"] == "lost-new-session"
                && response["type"] == "command_completed"
                && response["delivery"]["disposition"] == "replayed"
        })
        .expect("replayed completion");
    assert_eq!(
        replay["delivery"]["command"]["result_session_id"],
        created_session
    );

    let final_state = sessions(&workspace, &database);
    assert_eq!(final_state["rows"].as_array().unwrap().len(), 2);
    assert!(final_state["rows"]
        .as_array()
        .unwrap()
        .iter()
        .any(|session| { session["id"] == initial_session && session["selected"] == true }));
    assert_eq!(
        std::fs::read(workspace.join(".git/index")).expect("final git index"),
        index_before
    );
    assert_eq!(
        std::fs::read(workspace.join("unrelated.txt")).expect("final unrelated bytes"),
        unrelated_before
    );
}

#[test]
fn same_request_id_is_isolated_by_project_and_changed_input_conflicts() {
    let fixture = tempfile::tempdir().expect("fixture");
    let workspace_a = fixture.path().join("workspace-a");
    let workspace_b = fixture.path().join("workspace-b");
    std::fs::create_dir(&workspace_a).expect("workspace A");
    std::fs::create_dir(&workspace_b).expect("workspace B");
    let database = fixture.path().join("state.db");
    let new_session = command_request("project-local", json!({"kind": "new_session"}));

    let first = run_server(&workspace_a, &database, std::slice::from_ref(&new_session));
    assert!(first.iter().any(|response| {
        response["request_id"] == "project-local"
            && response["type"] == "command_completed"
            && response["delivery"]["disposition"] == "original"
    }));
    let conflict = command_request(
        "project-local",
        json!({"kind": "submit_task", "prompt": "private changed input"}),
    );
    let conflict_responses = run_server(&workspace_a, &database, &[conflict]);
    let conflict = conflict_responses
        .iter()
        .find(|response| response["code"] == "command_identity_conflict")
        .expect("identity conflict");
    assert_eq!(conflict["delivery"]["disposition"], "conflict");
    assert!(!conflict.to_string().contains("private changed input"));

    let independent = run_server(&workspace_b, &database, &[new_session]);
    assert!(independent.iter().any(|response| {
        response["request_id"] == "project-local"
            && response["type"] == "command_completed"
            && response["delivery"]["disposition"] == "original"
    }));
    assert_eq!(
        sessions(&workspace_a, &database)["rows"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
    assert_eq!(
        sessions(&workspace_b, &database)["rows"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
}

#[test]
fn lost_fork_delivery_replays_the_original_child_after_parent_advances() {
    let fixture = tempfile::tempdir().expect("fixture");
    let workspace = fixture.path().join("workspace");
    std::fs::create_dir(&workspace).expect("workspace");
    let database = fixture.path().join("state.db");
    let (source_session_id, original_cutoff) = seed_settled_source(&workspace, &database);
    let request = command_request(
        "lost-fork",
        json!({
            "kind": "fork_session",
            "source_session_id": source_session_id.clone(),
            "title": "Focused branch"
        }),
    );

    run_server_discarding_delivery(&workspace, &database, &request);
    let first_state = sessions(&workspace, &database);
    let first_rows = first_state["rows"].as_array().expect("session rows");
    assert_eq!(first_rows.len(), 2);
    assert!(first_rows
        .iter()
        .any(|row| row["id"] == source_session_id && row["selected"] == true));
    let child_session_id = first_rows
        .iter()
        .find(|row| row["id"] != source_session_id)
        .and_then(|row| row["id"].as_str())
        .expect("fork child")
        .to_owned();

    let store = Store::open(&database).expect("reopen store");
    let resolution = store.resolve_workspace(&workspace).expect("workspace");
    let project_id = resolution.snapshot.project.id;
    let source = source_session_id.parse().expect("source id");
    let run_id = RunId::new_v4();
    for (role, text) in [
        (ConversationRole::User, "later parent request"),
        (ConversationRole::Assistant, "later parent response"),
    ] {
        store
            .append_conversation_message(&ConversationMessage {
                id: MessageId::new_v4(),
                project_id,
                session_id: source,
                run_id,
                role,
                text: text.into(),
                reasoning: String::new(),
                usage: None,
                profile_id: if role == ConversationRole::Assistant {
                    "fixture".into()
                } else {
                    String::new()
                },
                model: if role == ConversationRole::Assistant {
                    "fixture".into()
                } else {
                    String::new()
                },
                status: ConversationMessageStatus::Complete,
                created_at: now(),
            })
            .expect("later message");
    }
    drop(store);

    let responses = run_server(&workspace, &database, std::slice::from_ref(&request));
    let replay = responses
        .iter()
        .find(|response| {
            response["request_id"] == "lost-fork"
                && response["type"] == "command_completed"
                && response["delivery"]["disposition"] == "replayed"
        })
        .expect("replayed fork");
    assert_eq!(
        replay["delivery"]["command"]["result_session_id"],
        child_session_id
    );
    assert_eq!(
        sessions(&workspace, &database)["rows"]
            .as_array()
            .unwrap()
            .len(),
        2
    );

    let conflict = command_request(
        "lost-fork",
        json!({
            "kind": "fork_session",
            "source_session_id": source_session_id,
            "title": "Changed private title"
        }),
    );
    let conflict_responses = run_server(&workspace, &database, &[conflict]);
    let conflict = conflict_responses
        .iter()
        .find(|response| response["code"] == "command_identity_conflict")
        .expect("fork identity conflict");
    assert!(!conflict.to_string().contains("Changed private title"));

    let connection = rusqlite::Connection::open_with_flags(
        &database,
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
    )
    .expect("read-only database");
    assert_eq!(
        connection
            .query_row(
                "SELECT count(*) FROM event_log WHERE event_type = 'session_forked'",
                [],
                |row| row.get::<_, u64>(0)
            )
            .expect("fork event count"),
        1
    );
    let payload: String = connection
        .query_row(
            "SELECT payload FROM session_forks WHERE child_session_id = ?1",
            [&child_session_id],
            |row| row.get(0),
        )
        .expect("fork lineage payload");
    assert_eq!(
        serde_json::from_str::<Value>(&payload).expect("lineage JSON")["cutoff"]["last_message_id"],
        original_cutoff
    );
}
