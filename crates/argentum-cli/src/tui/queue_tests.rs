use super::*;
use argentum_domain::{ProjectId, SessionId, TaskLifecycle};
use argentum_runtime::{ApprovalFailureContext, RuntimeError};

fn initialized() -> (TuiState, SubmissionTarget) {
    let mut state = TuiState::new(PathBuf::from("workspace"), true);
    let target = SubmissionTarget {
        project_id: ProjectId::new_v4(),
        session_id: SessionId::new_v4(),
        workspace: PathBuf::from("workspace"),
    };
    select(&mut state, &target);
    (state, target)
}

fn select(state: &mut TuiState, target: &SubmissionTarget) {
    state.apply_event(AppEvent::WorkspaceStateLoaded(WorkspaceSnapshot {
        project: argentum_domain::Project {
            id: target.project_id,
            name: "fixture".into(),
            workspace_root: target.workspace.clone(),
            created_at: argentum_domain::now(),
        },
        sessions: vec![SessionSummary {
            id: target.session_id,
            title: "fixture".into(),
            created_at: argentum_domain::now(),
            updated_at: argentum_domain::now(),
            pinned: false,
            archived: false,
        }],
        active_session_id: Some(target.session_id),
    }));
}

fn submit(state: &mut TuiState, prompt: &str) -> TuiAction {
    assert!(state.composer().is_empty());
    state.apply_input(InputEvent::Text(prompt.into()));
    state.apply_input(InputEvent::Enter)
}

fn first(state: &mut TuiState, prompt: &str) -> TaskSubmission {
    let TuiAction::Submit(submission) = submit(state, prompt) else {
        panic!("first submission");
    };
    *submission
}

fn reply(
    state: &mut TuiState,
    submission: &TaskSubmission,
    admitted: bool,
    result: Result<(), RuntimeError>,
) {
    state.submission_finished(TaskSubmissionResult {
        id: submission.id,
        run_id: submission.run_id,
        admitted,
        terminal_lifecycle: None,
        result,
    });
}

fn terminal(state: &mut TuiState, submission: &TaskSubmission, lifecycle: TaskLifecycle) {
    state.apply_event(AppEvent::RunStatusChanged {
        run_id: submission.run_id,
        lifecycle,
    });
}

#[test]
fn fifo_survives_three_followups_transition_arrivals_and_stale_notifications() {
    let (mut state, target) = initialized();
    let a = first(&mut state, "A");
    for prompt in ["B", "C", "D"] {
        assert!(matches!(submit(&mut state, prompt), TuiAction::Redraw));
    }
    terminal(&mut state, &a, TaskLifecycle::Complete);
    assert!(state.take_ready_submission().is_none());
    submit(&mut state, "E during admission-release transition");
    assert_eq!(
        state.followups.pending_prompts(&target),
        ["B", "C", "D", "E during admission-release transition"]
    );
    reply(&mut state, &a, true, Ok(()));
    for expected in ["B", "C", "D", "E during admission-release transition"] {
        let next = state.take_ready_submission().unwrap();
        assert_eq!(next.prompt, expected);
        assert_eq!(next.target, target);
        terminal(&mut state, &a, TaskLifecycle::Complete);
        reply(&mut state, &a, true, Ok(()));
        assert!(state.take_ready_submission().is_none());
        reply(&mut state, &next, true, Ok(()));
        assert!(state.take_ready_submission().is_none());
        terminal(&mut state, &next, TaskLifecycle::Complete);
    }
    assert_eq!(state.followups.totals(), (0, 0));
}

#[test]
fn count_limit_rejects_newest_and_keeps_draft_and_accepted_entries() {
    let (mut state, target) = initialized();
    let _a = first(&mut state, "0");
    for index in 1..MAX_QUEUE_ENTRIES {
        submit(&mut state, &index.to_string());
    }
    let before = state.followups.pending_ids(&target);
    submit(&mut state, "rejected newest draft");
    assert_eq!(state.composer(), "rejected newest draft");
    assert!(state.status.contains("16 retained"));
    assert_eq!(state.followups.pending_ids(&target), before);
    assert_eq!(state.followups.totals().0, MAX_QUEUE_ENTRIES);
    assert!(matches!(
        state.apply_input(InputEvent::Enter),
        TuiAction::Redraw
    ));
    assert_eq!(state.followups.pending_ids(&target), before);
}

#[test]
fn byte_limit_utf8_and_per_prompt_limit_are_exact_and_non_destructive() {
    let (mut state, _) = initialized();
    let maximum = "é".repeat(MAX_COMPOSER_BYTES / 2);
    let _a = first(&mut state, &maximum);
    submit(&mut state, &maximum);
    submit(&mut state, &maximum);
    submit(&mut state, &"é".repeat(MAX_COMPOSER_BYTES / 2 - 1));
    assert_eq!(state.followups.totals().1, MAX_QUEUE_BYTES - 2);
    submit(&mut state, "é");
    assert_eq!(state.followups.totals().1, MAX_QUEUE_BYTES);
    submit(&mut state, "x");
    assert_eq!(state.composer(), "x");
    assert!(state.status.contains("byte limit"));
    state.clear_composer();
    state.apply_input(InputEvent::Text("existing draft".into()));
    state.apply_input(InputEvent::Text("z".repeat(MAX_COMPOSER_BYTES + 1)));
    assert_eq!(state.composer(), "existing draft");
    assert_eq!(state.followups.totals().1, MAX_QUEUE_BYTES);
}

#[test]
fn repeated_enter_is_empty_but_identical_intent_has_distinct_identity() {
    let (mut state, target) = initialized();
    let a = first(&mut state, "same\r\né");
    assert!(matches!(
        state.apply_input(InputEvent::Enter),
        TuiAction::Redraw
    ));
    assert_eq!(state.followups.totals().0, 1);
    submit(&mut state, "same\r\né");
    submit(&mut state, "same\r\né");
    let ids = state.followups.pending_ids(&target);
    assert_eq!(ids.len(), 2);
    assert_ne!(a.id, ids[0]);
    assert_ne!(ids[0], ids[1]);
    terminal(&mut state, &a, TaskLifecycle::Complete);
    reply(&mut state, &a, true, Ok(()));
    let b = state.take_ready_submission().unwrap();
    assert_eq!(b.prompt, a.prompt);
    assert_eq!(b.id, ids[0]);
}

#[test]
fn pre_admission_rejection_retains_front_entry_and_new_attempt_identity() {
    let (mut state, target) = initialized();
    let a = first(&mut state, "A");
    submit(&mut state, "B");
    submit(&mut state, "C");
    reply(&mut state, &a, false, Err(RuntimeError::SessionRunActive));
    assert_eq!(state.followups.pending_prompts(&target), ["A", "B", "C"]);
    assert!(state.take_ready_submission().is_none());
    submit(&mut state, "/queue continue");
    let retried = state.take_ready_submission().unwrap();
    assert_eq!(retried.id, a.id);
    assert_eq!(retried.prompt, a.prompt);
    assert_ne!(retried.run_id, a.run_id);
    reply(&mut state, &a, false, Err(RuntimeError::SessionRunActive));
    terminal(&mut state, &a, TaskLifecycle::Complete);
    assert_eq!(state.followups.pending_prompts(&target), ["B", "C"]);
    assert!(state.take_ready_submission().is_none());
}

#[test]
fn failure_denial_approval_required_and_cancellation_pause_without_retrying_admitted_work() {
    for outcome in ["failure", "denial", "approval_required", "cancel"] {
        let (mut state, target) = initialized();
        let a = first(&mut state, "A");
        submit(&mut state, "B");
        submit(&mut state, "C");
        let ids = state.followups.pending_ids(&target);
        let context = ApprovalFailureContext {
            run_id: a.run_id,
            tool_call_id: None,
            approval_id: argentum_domain::ApprovalId::new_v4(),
        };
        let result = match outcome {
            "failure" => Err(RuntimeError::NoProvider),
            "denial" => Err(RuntimeError::ApprovalDenied(context)),
            "approval_required" => Err(RuntimeError::ApprovalRequired(context)),
            _ => Ok(()),
        };
        reply(&mut state, &a, true, result);
        assert!(state.take_ready_submission().is_none());
        terminal(
            &mut state,
            &a,
            if outcome == "cancel" {
                TaskLifecycle::Cancelled
            } else {
                TaskLifecycle::Failed
            },
        );
        assert_eq!(state.followups.pending_ids(&target), ids);
        assert!(state.take_ready_submission().is_none());
        terminal(&mut state, &a, TaskLifecycle::Complete);
        assert!(state.take_ready_submission().is_none());
        submit(&mut state, "/queue continue");
        assert_eq!(state.take_ready_submission().unwrap().prompt, "B");
    }
}

#[test]
fn uncertain_handoff_keeps_owned_prompt_and_refuses_blind_continuation() {
    let (mut state, target) = initialized();
    let _a = first(&mut state, "A");
    submit(&mut state, "B");
    state.followups.uncertain("uncertain handoff");
    submit(&mut state, "/queue continue");
    assert!(state.status.contains("uncertain"));
    assert!(state.take_ready_submission().is_none());
    assert!(state.followups.has_in_flight(&target));
    assert_eq!(state.followups.pending_prompts(&target), ["B"]);
}

#[test]
fn session_switch_and_discard_are_scoped_and_quota_is_global() {
    let (mut state, a_target) = initialized();
    let a = first(&mut state, "A");
    submit(&mut state, "A followup");
    let b_target = SubmissionTarget {
        session_id: SessionId::new_v4(),
        ..a_target.clone()
    };
    select(&mut state, &b_target);
    let _b = first(&mut state, "B");
    submit(&mut state, "B followup");
    assert_eq!(state.followups.totals().0, 4);
    submit(&mut state, "/queue discard");
    assert_eq!(state.followups.pending_prompts(&a_target), ["A followup"]);
    assert!(state.followups.pending_prompts(&b_target).is_empty());
    assert!(state.followups.has_in_flight(&b_target));
    terminal(&mut state, &a, TaskLifecycle::Complete);
    reply(&mut state, &a, true, Ok(()));
    select(&mut state, &a_target);
    assert!(state.take_ready_submission().is_none());
    submit(&mut state, "/queue continue");
    let resumed = state.take_ready_submission().unwrap();
    assert_eq!(resumed.target, a_target);
    assert_eq!(resumed.prompt, "A followup");
}

#[test]
fn quit_confirmation_eof_and_restart_account_for_volatile_pending_work() {
    let (mut state, target) = initialized();
    let a = first(&mut state, "A");
    submit(&mut state, "B");
    submit(&mut state, "C");
    assert!(matches!(submit(&mut state, "/quit"), TuiAction::Redraw));
    assert!(state.status.contains("2 pending"));
    terminal(&mut state, &a, TaskLifecycle::Complete);
    reply(&mut state, &a, true, Ok(()));
    assert!(state.take_ready_submission().is_none());
    assert!(matches!(
        state.apply_input(InputEvent::Eof),
        TuiAction::Exit
    ));
    assert!(state
        .shutdown_notice
        .as_deref()
        .unwrap()
        .contains("2 pending prompts were not executed"));
    assert!(state.take_ready_submission().is_none());
    submit(&mut state, "not accepted after shutdown");
    assert_eq!(state.composer(), "not accepted after shutdown");
    let mut restarted = TuiState::new(target.workspace.clone(), true);
    select(&mut restarted, &target);
    assert_eq!(restarted.followups.totals(), (0, 0));
}

#[test]
fn session_switch_keeps_approval_replies_bound_to_the_correct_request() {
    let (mut state, a_target) = initialized();
    let a = first(&mut state, "A");
    let make_approval = |run_id| ApprovalRequest {
        id: argentum_domain::ApprovalId::new_v4(),
        run_id,
        tool_id: "edit_text".into(),
        action: "Edit".into(),
        target: "fixture.txt".into(),
        binding: argentum_domain::Sha256Digest("a".repeat(64)),
        reason: "fixture".into(),
        capabilities: vec![argentum_domain::Capability::WriteFiles],
        scope: argentum_domain::ApprovalScope::Once,
        expires_at: None,
    };
    let approval_a = make_approval(a.run_id);
    state.apply_event(AppEvent::ApprovalRequested(approval_a.clone()));
    submit(&mut state, "queued while awaiting approval");
    assert_eq!(
        state.followups.pending_prompts(&a_target),
        ["queued while awaiting approval"]
    );
    let b_target = SubmissionTarget {
        session_id: SessionId::new_v4(),
        ..a_target.clone()
    };
    select(&mut state, &b_target);
    assert!(!state.has_pending_approval());
    let b = first(&mut state, "B");
    let approval_b = make_approval(b.run_id);
    state.apply_event(AppEvent::ApprovalRequested(approval_b.clone()));
    state.apply_event(AppEvent::ApprovalResolved {
        approval_id: approval_a.id,
        approved: false,
    });
    assert_eq!(state.pending_approval.as_ref().unwrap().id, approval_b.id);
    assert!(
        matches!(submit(&mut state, "yes"), TuiAction::Dispatch(AppCommand::ApproveTool { approval_id, .. }) if approval_id == approval_b.id)
    );
    select(&mut state, &a_target);
    assert!(!state.has_pending_approval());
    assert_eq!(
        state.followups.pending_prompts(&a_target),
        ["queued while awaiting approval"]
    );
}
