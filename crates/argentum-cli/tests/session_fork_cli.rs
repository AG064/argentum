#![cfg(windows)]

use std::path::{Path, PathBuf};
use std::process::Command;

use argentum_domain::{
    now, ConversationMessage, ConversationMessageStatus, ConversationRole, MessageId, ProjectId,
    RunId, SessionForkSnapshot, SessionId,
};
use argentum_store::Store;

fn executable() -> PathBuf {
    std::env::var_os("ARGENTUM_F17B_CLI_EXE")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(env!("CARGO_BIN_EXE_argentum")))
}

fn append_pair(
    store: &Store,
    project_id: ProjectId,
    session_id: SessionId,
    user_text: &str,
    assistant_text: &str,
) -> MessageId {
    let run_id = RunId::new_v4();
    let mut last_message_id = None;
    for (role, text) in [
        (ConversationRole::User, user_text),
        (ConversationRole::Assistant, assistant_text),
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
            .expect("conversation message");
        last_message_id = Some(id);
    }
    last_message_id.expect("assistant message")
}

fn cli_args(workspace: &Path, database: &Path) -> Vec<String> {
    vec![
        "--isolated".into(),
        "--workspace".into(),
        workspace.to_string_lossy().into_owned(),
        "--database".into(),
        database.to_string_lossy().into_owned(),
        "--json".into(),
        "--no-color".into(),
    ]
}

#[test]
fn public_fork_command_pins_history_without_selecting_or_changing_workspace() {
    let fixture = tempfile::tempdir().expect("fixture");
    let workspace = fixture.path().join("workspace");
    std::fs::create_dir(&workspace).expect("workspace");
    let observed_file = workspace.join("observed.txt");
    let original_file = b"f17b-original-file-canary\r\n";
    std::fs::write(&observed_file, original_file).expect("observed file");
    let database = fixture.path().join("state.db");

    let store = Store::open(&database).expect("store");
    let resolution = store
        .resolve_workspace(&workspace)
        .expect("workspace resolution");
    let project_id = resolution.snapshot.project.id;
    let source_session_id = resolution
        .snapshot
        .active_session_id
        .expect("source session");
    let cutoff = append_pair(
        &store,
        project_id,
        source_session_id,
        "read observed.txt",
        "The read completed. The file body is not repeated here.",
    );
    drop(store);

    let mut arguments = vec![
        "session".into(),
        "fork".into(),
        source_session_id.to_string(),
        "--title".into(),
        "Focused branch".into(),
    ];
    arguments.extend(cli_args(&workspace, &database));
    let output = Command::new(executable())
        .args(&arguments)
        .output()
        .expect("public fork command");
    assert!(
        output.status.success(),
        "fork failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let fork: SessionForkSnapshot =
        serde_json::from_slice(&output.stdout).expect("fork snapshot JSON");
    assert_eq!(fork.source_session_id, source_session_id);
    assert_eq!(fork.cutoff.last_message_id, cutoff);
    assert_ne!(fork.child_session_id, source_session_id);
    assert_eq!(
        std::fs::read(&observed_file).expect("file after fork"),
        original_file
    );

    let store = Store::open(&database).expect("reopen store");
    assert_eq!(
        store
            .workspace_snapshot(project_id)
            .expect("workspace snapshot")
            .active_session_id,
        Some(source_session_id)
    );
    append_pair(
        &store,
        project_id,
        source_session_id,
        "later parent request",
        "later-parent-only-canary",
    );
    let child_before_restart = store
        .conversation_snapshot(project_id, fork.child_session_id)
        .expect("child conversation");
    let inherited = child_before_restart
        .messages
        .iter()
        .map(|message| message.text.as_str())
        .collect::<Vec<_>>()
        .join("\n");
    assert!(inherited.contains("read observed.txt"));
    assert!(inherited.contains("Historical only"));
    assert!(!inherited.contains("later-parent-only-canary"));
    drop(store);

    let reopened = Store::open(&database).expect("clean restart store");
    let stored_fork = reopened
        .session_fork_snapshot(project_id, fork.child_session_id)
        .expect("stored fork")
        .expect("fork lineage");
    assert_eq!(stored_fork, fork);
    assert_eq!(
        reopened
            .conversation_snapshot(project_id, fork.child_session_id)
            .expect("restarted child conversation"),
        child_before_restart
    );
    drop(reopened);

    let mut select_arguments = vec![
        "session".into(),
        "select".into(),
        fork.child_session_id.to_string(),
    ];
    select_arguments.extend(cli_args(&workspace, &database));
    let selected = Command::new(executable())
        .args(&select_arguments)
        .output()
        .expect("explicit child selection");
    assert!(
        selected.status.success(),
        "selection failed: {}",
        String::from_utf8_lossy(&selected.stderr)
    );
    let reopened = Store::open(&database).expect("selected store");
    assert_eq!(
        reopened
            .workspace_snapshot(project_id)
            .expect("selected snapshot")
            .active_session_id,
        Some(fork.child_session_id)
    );
    assert_eq!(
        std::fs::read(&observed_file).expect("final file"),
        original_file
    );
}
