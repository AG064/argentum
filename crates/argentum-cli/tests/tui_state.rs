use std::path::PathBuf;

use argentum_cli::tui::{InputEvent, TuiAction, TuiState};
use argentum_domain::{
    AppCommand, AppEvent, ApprovalScope, ConversationSnapshot, WorkspaceSnapshot,
};

#[test]
fn two_followups_during_an_active_run_are_not_overwritten() {
    let mut state = initialized_state();
    state.apply_input(InputEvent::Text("A".into()));
    let TuiAction::Submit(first) = state.apply_input(InputEvent::Enter) else {
        panic!("first submission");
    };
    for prompt in ["queued B", "queued C"] {
        state.apply_input(InputEvent::Text(prompt.into()));
        state.apply_input(InputEvent::Enter);
    }
    state.apply_event(AppEvent::RunStatusChanged {
        run_id: first.run_id,
        lifecycle: argentum_domain::TaskLifecycle::Complete,
    });
    assert!(
        state.take_ready_submission().is_none(),
        "terminal event is not admission release"
    );
    state.submission_finished(argentum_runtime::TaskSubmissionResult {
        id: first.id,
        run_id: first.run_id,
        admitted: true,
        terminal_lifecycle: Some(argentum_domain::TaskLifecycle::Complete),
        result: Ok(()),
    });
    let second = state.take_ready_submission().expect("B");
    assert_eq!(second.prompt, "queued B");
    state.submission_finished(argentum_runtime::TaskSubmissionResult {
        id: second.id,
        run_id: second.run_id,
        admitted: true,
        terminal_lifecycle: Some(argentum_domain::TaskLifecycle::Complete),
        result: Ok(()),
    });
    assert!(
        state.take_ready_submission().is_none(),
        "reply is not a successful terminal event"
    );
    state.apply_event(AppEvent::RunStatusChanged {
        run_id: second.run_id,
        lifecycle: argentum_domain::TaskLifecycle::Complete,
    });
    assert_eq!(state.take_ready_submission().expect("C").prompt, "queued C");
}

fn initialized_state() -> TuiState {
    let mut state = TuiState::new(PathBuf::from("workspace"), true);
    let id = argentum_domain::SessionId::new_v4();
    state.apply_event(AppEvent::WorkspaceStateLoaded(WorkspaceSnapshot {
        project: argentum_domain::Project {
            id: argentum_domain::ProjectId::new_v4(),
            name: "fixture".into(),
            workspace_root: PathBuf::from("workspace"),
            created_at: argentum_domain::now(),
        },
        sessions: vec![argentum_domain::SessionSummary {
            id,
            title: "fixture".into(),
            created_at: argentum_domain::now(),
            updated_at: argentum_domain::now(),
        }],
        active_session_id: Some(id),
    }));
    state
}

#[test]
fn multiline_composer_dispatches_one_submit_command() {
    let mut state = initialized_state();

    state.apply_input(InputEvent::Text("first".into()));
    state.apply_input(InputEvent::NewLine);
    state.apply_input(InputEvent::Text("second".into()));

    let TuiAction::Submit(submission) = state.apply_input(InputEvent::Enter) else {
        panic!("submission");
    };
    assert_eq!(submission.prompt, "first\nsecond");
    assert!(state.composer().is_empty());
}

#[test]
fn history_and_slash_completion_are_local_editor_behaviors() {
    let mut state = initialized_state();
    state.apply_input(InputEvent::Text("first".into()));
    assert!(matches!(
        state.apply_input(InputEvent::Enter),
        TuiAction::Submit(_)
    ));

    state.apply_input(InputEvent::HistoryPrevious);
    assert_eq!(state.composer(), "first");

    state.clear_composer();
    state.apply_input(InputEvent::Text("/s".into()));
    state.apply_input(InputEvent::Complete);
    assert_eq!(state.composer(), "/sessions");
}

#[test]
fn domain_events_render_session_stream_tools_and_approvals() {
    let project_id = "00000000-0000-0000-0000-000000000101"
        .parse()
        .expect("project id");
    let session_id = "00000000-0000-0000-0000-000000000102"
        .parse()
        .expect("session id");
    let run_id = "00000000-0000-0000-0000-000000000103"
        .parse()
        .expect("run id");
    let call_id = "00000000-0000-0000-0000-000000000104"
        .parse()
        .expect("call id");
    let approval_id = "00000000-0000-0000-0000-000000000105"
        .parse()
        .expect("approval id");
    let now = argentum_domain::now();
    let mut state = TuiState::new(PathBuf::from("workspace"), true);

    state.apply_event(AppEvent::WorkspaceStateLoaded(WorkspaceSnapshot {
        project: argentum_domain::Project {
            id: project_id,
            name: "Workspace".into(),
            workspace_root: PathBuf::from("workspace"),
            created_at: now,
        },
        sessions: vec![argentum_domain::SessionSummary {
            id: session_id,
            title: "Working session".into(),
            created_at: now,
            updated_at: now,
        }],
        active_session_id: Some(session_id),
    }));
    state.apply_event(AppEvent::ConversationSnapshotLoaded(ConversationSnapshot {
        project_id,
        session_id,
        messages: Vec::new(),
    }));
    state.apply_event(AppEvent::ActiveRunsSnapshot {
        runs: vec![argentum_domain::ActiveRunState {
            session_id,
            run_id,
            lifecycle: argentum_domain::TaskLifecycle::Running,
        }],
    });
    state.apply_event(AppEvent::RunStatusChanged {
        run_id,
        lifecycle: argentum_domain::TaskLifecycle::Running,
    });
    state.apply_event(AppEvent::AssistantDelta {
        run_id,
        text: "streamed answer".into(),
    });
    state.apply_event(AppEvent::ToolStarted(argentum_domain::ToolTrace {
        id: call_id,
        run_id,
        tool_id: "read_text".into(),
        summary: "Read notes".into(),
        result: argentum_domain::ToolResultState::Running,
        duration_ms: None,
    }));
    state.apply_event(AppEvent::ApprovalRequested(
        argentum_domain::ApprovalRequest {
            id: approval_id,
            run_id,
            tool_id: "write_text".into(),
            action: "Write file".into(),
            target: "notes.txt".into(),
            binding: argentum_domain::Sha256Digest("0".repeat(64)),
            reason: "User requested the change".into(),
            capabilities: vec![argentum_domain::Capability::WriteFiles],
            expires_at: None,
        },
    ));

    let modal = state.render(80);
    assert!(modal.contains("Approval required") && modal.contains("notes.txt"));
    state.apply_input(InputEvent::Text("queued draft".into()));
    assert!(state.has_pending_approval());
    let rendered = state.render(80);
    assert!(rendered.contains("Working session"));
    assert!(rendered.contains("streamed answer"));
    assert!(rendered.contains("read_text"));
    assert!(rendered.contains("Approval required"));
    assert!(rendered.contains("notes.txt"));
}

#[test]
fn first_ctrl_c_interrupts_and_second_ctrl_c_exits() {
    let mut state = TuiState::new(PathBuf::from("workspace"), true);
    let run_id = "00000000-0000-0000-0000-000000000106"
        .parse()
        .expect("run id");
    state.apply_event(AppEvent::RunStatusChanged {
        run_id,
        lifecycle: argentum_domain::TaskLifecycle::Running,
    });

    assert_eq!(
        state.apply_input(InputEvent::CtrlC),
        TuiAction::Dispatch(AppCommand::CancelRun { run_id })
    );
    assert_eq!(state.apply_input(InputEvent::CtrlC), TuiAction::Exit);
}

#[test]
fn approval_reply_dispatches_the_existing_typed_approval_command() {
    let approval_id = "00000000-0000-0000-0000-000000000107"
        .parse()
        .expect("approval id");
    let run_id = "00000000-0000-0000-0000-000000000108"
        .parse()
        .expect("run id");
    let mut state = TuiState::new(PathBuf::from("workspace"), true);
    state.apply_event(AppEvent::ApprovalRequested(
        argentum_domain::ApprovalRequest {
            id: approval_id,
            run_id,
            tool_id: "write_text".into(),
            action: "Write file".into(),
            target: "notes.txt".into(),
            binding: argentum_domain::Sha256Digest("0".repeat(64)),
            reason: "User requested the change".into(),
            capabilities: vec![argentum_domain::Capability::WriteFiles],
            expires_at: None,
        },
    ));

    state.apply_input(InputEvent::Text("yes".into()));
    assert_eq!(
        state.apply_input(InputEvent::Enter),
        TuiAction::Dispatch(AppCommand::ApproveTool {
            approval_id,
            scope: ApprovalScope::Once,
        })
    );
}

#[test]
fn composer_limit_counts_utf8_bytes_without_splitting_characters() {
    let mut state = TuiState::new(PathBuf::from("workspace"), true);
    state.apply_input(InputEvent::Text("a".repeat(131_071)));
    state.apply_input(InputEvent::Text("é".into()));

    assert_eq!(state.composer().len(), 131_071);
}
