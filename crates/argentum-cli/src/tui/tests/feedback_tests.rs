use super::*;

#[test]
fn final_message_refresh_keeps_tool_paths_once_and_drops_unrelated_runs() {
    use argentum_domain::ConversationMessageStatus;
    use argentum_store::{HistoryMessage, HistoryPage};
    let mut state = TuiState::new(PathBuf::from("fixture"), true);
    let project = RunId::new_v4();
    let session = RunId::new_v4();
    let run = RunId::new_v4();
    state.project_id = Some(project);
    state.active_session_id = Some(session);
    state.active_run_id = Some(run);
    state.append_tool(
        trace(
            RunId::new_v4(),
            RunId::new_v4(),
            "other session file",
            ToolResultState::Succeeded,
        ),
        "succeeded",
    );
    state.append_tool(
        trace(
            run,
            RunId::new_v4(),
            "Created \"new.py\"",
            ToolResultState::Succeeded,
        ),
        "succeeded",
    );
    let page = HistoryPage {
        project_id: project,
        session_id: session,
        has_older: false,
        has_newer: false,
        messages: [ConversationRole::User, ConversationRole::Assistant]
            .into_iter()
            .map(|role| HistoryMessage {
                id: RunId::new_v4(),
                run_id: run,
                role,
                status: ConversationMessageStatus::Complete,
                text: "message".into(),
                truncated: false,
            })
            .collect(),
    };
    state.apply_history_page(page.clone(), false, true);
    state.apply_history_page(page, false, true);
    assert_eq!(state.timeline.len(), 3);
    assert_eq!(state.timeline[1].kind, TimelineKind::Tool);
    let rows = state.transcript_rows(100, 16).join("\n");
    assert!(rows.contains("Created \"new.py\""));
    assert!(!rows.contains("other session file"));
}

fn trace(
    run_id: RunId,
    id: RunId,
    summary: &str,
    result: ToolResultState,
) -> argentum_domain::ToolTrace {
    argentum_domain::ToolTrace {
        id,
        run_id,
        tool_id: "apply_patch".into(),
        summary: summary.into(),
        result,
        duration_ms: None,
    }
}

#[test]
fn completed_messages_hide_internal_recording_state_and_failed_answers_are_clear() {
    let mut state = TuiState::new(PathBuf::from("fixture"), true);
    for kind in [TimelineKind::User, TimelineKind::Assistant] {
        state.timeline.push(TimelineEntry {
            view_id: RunId::new_v4(),
            message_id: None,
            kind,
            run_id: None,
            tool_call_id: None,
            text: "normal message".into(),
            detail: String::new(),
            state: "complete".into(),
        });
    }
    state.transcript_changed();
    let rows = state.transcript_rows(100, 12).join("\n");
    assert!(rows.contains("you: normal message"));
    assert!(rows.contains("argentum: normal message"));
    assert!(!rows.contains("recorded") && !rows.contains("complete"));
    state.timeline.last_mut().unwrap().state = "failed".into();
    state.transcript_changed();
    let rows = state.transcript_rows(100, 12).join("\n");
    assert!(rows.contains("response failed"));
    assert!(!rows.contains("recorded"));
}

#[test]
fn tool_rows_replace_planned_paths_with_actual_completed_changes() {
    let mut state = TuiState::new(PathBuf::from("fixture"), true);
    let run = RunId::new_v4();
    let id = RunId::new_v4();
    state.apply_event(AppEvent::RunStatusChanged {
        run_id: run,
        lifecycle: TaskLifecycle::Running,
    });
    state.apply_event(AppEvent::ToolStarted(trace(
        run,
        id,
        "Create \"new.py\"\nModify \"requirements.txt\"\nDelete \"old.py\"",
        ToolResultState::Running,
    )));
    assert!(state.display_status().contains("Executing apply_patch"));
    let rows = state.transcript_rows(100, 16).join("\n");
    assert!(rows.contains("Create \"new.py\"") && rows.contains("result: running"));
    state.apply_event(AppEvent::ToolFinished(trace(
        run,
        id,
        "Created \"new.py\"\nModified \"requirements.txt\"\nDeleted \"old.py\"",
        ToolResultState::Succeeded,
    )));
    let rows = state.transcript_rows(100, 16).join("\n");
    for text in [
        "Created \"new.py\"",
        "Modified \"requirements.txt\"",
        "Deleted \"old.py\"",
        "result: succeeded",
    ] {
        assert!(rows.contains(text), "missing {text}");
    }
    assert!(!rows.contains("recorded"));
    assert_eq!(state.timeline.len(), 1);
    assert!(state.display_status().contains("Waiting for model"));
}

#[test]
fn failed_tool_does_not_claim_completed_changes() {
    let mut state = TuiState::new(PathBuf::from("fixture"), true);
    let run = RunId::new_v4();
    let id = RunId::new_v4();
    state.active_run_id = Some(run);
    state.apply_event(AppEvent::ToolStarted(trace(
        run,
        id,
        "Create \"new.py\"",
        ToolResultState::Running,
    )));
    state.apply_event(AppEvent::ToolFinished(trace(
        run,
        id,
        "Create \"new.py\"",
        ToolResultState::Failed,
    )));
    let rows = state.transcript_rows(100, 10).join("\n");
    assert!(rows.contains("result: failed"));
    assert!(!rows.contains("Created"));
}

#[test]
fn progress_is_visible_while_waiting_and_text_streams_before_completion() {
    let mut state = TuiState::new(PathBuf::from("fixture"), true);
    assert!(!state.progress_active());
    assert_eq!(state.display_status(), "Ready");
    let run = RunId::new_v4();
    state.apply_event(AppEvent::RunStatusChanged {
        run_id: run,
        lifecycle: TaskLifecycle::Running,
    });
    let first = state.display_status();
    assert!(first.contains("Waiting for model"));
    state.progress_tick += 1;
    assert_ne!(state.display_status(), first);
    state.apply_event(AppEvent::AssistantDelta {
        run_id: run,
        text: "partial ".into(),
    });
    assert!(state.display_status().contains("Receiving response"));
    assert!(state.transcript_rows(80, 8).join("\n").contains("partial"));
    state.apply_event(AppEvent::AssistantDelta {
        run_id: run,
        text: "answer".into(),
    });
    assert!(state
        .transcript_rows(80, 8)
        .join("\n")
        .contains("partial answer"));
    state.apply_event(AppEvent::RunStatusChanged {
        run_id: run,
        lifecycle: TaskLifecycle::Complete,
    });
    assert!(!state.progress_active());
    assert_eq!(state.display_status(), "Completed");
}
