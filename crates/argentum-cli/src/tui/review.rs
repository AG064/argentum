use super::*;
use argentum_runtime::review::{ReviewBody, ReviewPage, ReviewRequest};

#[derive(Clone)]
struct Navigation {
    request: ReviewRequest,
    selected: usize,
    offset: usize,
}
pub(super) struct State {
    navigation: Navigation,
    back: Vec<Navigation>,
    page: Option<ReviewPage>,
    ticket: RunId,
    cancel: tokio_util::sync::CancellationToken,
    notice: String,
    pub updates: u16,
    max_offset: std::cell::Cell<usize>,
    restore: Option<RestoreFlow>,
    pending_command: Option<AppCommand>,
    refresh: bool,
    restore_enabled: bool,
}

#[derive(Clone, Copy)]
enum RestoreStage {
    Requesting,
    CancellingRequest,
    Awaiting(argentum_domain::ApprovalId),
    Rejecting(argentum_domain::ApprovalId),
    Executing,
}

#[derive(Clone)]
struct RestoreFlow {
    run: RunId,
    change: argentum_domain::FileChangeId,
    target: String,
    binding: argentum_domain::Sha256Digest,
    stage: RestoreStage,
}

enum Choice {
    Open(ReviewRequest),
    Command(AppCommand),
}
impl Drop for State {
    fn drop(&mut self) {
        self.cancel.cancel();
    }
}
pub(super) struct Completion {
    ticket: RunId,
    page: Result<ReviewPage, String>,
}

fn state(tui: &TuiState) -> Option<&State> {
    tui.overlay.as_ref()?.review.as_ref()
}
fn state_mut(tui: &mut TuiState) -> Option<&mut State> {
    tui.overlay.as_mut()?.review.as_mut()
}

#[cfg(test)]
pub(super) fn position(tui: &TuiState) -> Option<(ReviewRequest, usize, usize)> {
    let n = &state(tui)?.navigation;
    Some((n.request.clone(), n.selected, n.offset))
}

pub(super) fn begin(tui: &mut TuiState) {
    let mut overlay = menu::Overlay::new(menu::View::Changes);
    overlay.review = Some(State {
        navigation: Navigation {
            request: ReviewRequest::Runs { before: None },
            selected: 0,
            offset: 0,
        },
        back: Vec::new(),
        page: None,
        ticket: RunId::new_v4(),
        cancel: tokio_util::sync::CancellationToken::new(),
        notice: "Loading recorded review...".into(),
        updates: 0,
        max_offset: std::cell::Cell::new(0),
        restore: None,
        pending_command: None,
        refresh: false,
        restore_enabled: !tui.local_text_mode,
    });
    tui.overlay = Some(overlay);
}

pub(super) fn schedule(
    tui: &mut TuiState,
    local: &LocalCommandHandle,
    tasks: &mut JoinSet<CommandCompletion>,
) {
    let (Some(project), Some(session)) = (tui.project_id, tui.active_session_id) else {
        return;
    };
    let Some(review) = state_mut(tui) else {
        return;
    };
    review.cancel.cancel();
    review.cancel = tokio_util::sync::CancellationToken::new();
    review.ticket = RunId::new_v4();
    review.page = None;
    if tasks.len() >= 8 {
        review.notice = "Review busy; close and reopen after pending inspection settles.".into();
        return;
    }
    review.notice = "Loading recorded review...".into();
    let ticket = review.ticket;
    let cancel = review.cancel.clone();
    let request = review.navigation.request.clone();
    let local = local.clone();
    tasks.spawn(async move {
        tokio::task::yield_now().await;
        let page = if cancel.is_cancelled() {
            Err("Obsolete review request".into())
        } else {
            local.review_page(project, session, request).map_err(|_| {
                "Review unavailable: scope, record, or read-policy check failed.".into()
            })
        };
        CommandCompletion {
            settings: None,
            kind: CommandKind::Other,
            result: Ok(()),
            submission: None,
            compaction_detail: None,
            review: Some(Completion { ticket, page }),
        }
    });
}

pub(super) fn load_sync(tui: &mut TuiState, local: &LocalCommandHandle) -> Result<(), HostError> {
    let (Some(project), Some(session)) = (tui.project_id, tui.active_session_id) else {
        return Ok(());
    };
    let Some(review) = state(tui) else {
        return Ok(());
    };
    let ticket = review.ticket;
    let page = local
        .review_page(project, session, review.navigation.request.clone())
        .map_err(|_| "Review unavailable: scope, record, or read-policy check failed.".into());
    finish(tui, local, Completion { ticket, page });
    Ok(())
}

pub(super) fn finish(tui: &mut TuiState, local: &LocalCommandHandle, completion: Completion) {
    if !state(tui)
        .is_some_and(|review| review.ticket == completion.ticket && !review.cancel.is_cancelled())
    {
        return;
    }
    let page = completion.page.and_then(|page| {
        if Some(page.project) != tui.project_id
            || Some(page.session) != tui.active_session_id
            || local.review_policy_stamp().ok().as_ref() != Some(&page.policy)
        {
            Err("Review scope or access changed; reopen review.".into())
        } else {
            Ok(page)
        }
    });
    let review = state_mut(tui).unwrap();
    match page {
        Ok(page) => {
            review.page = Some(page);
            review.notice.clear();
        }
        Err(error) => {
            review.page = None;
            review.notice = error;
        }
    }
}

pub(super) fn revalidate(tui: &mut TuiState, local: &LocalCommandHandle) {
    let Some(review) = state(tui) else {
        return;
    };
    let Some(page) = &review.page else {
        return;
    };
    let allowed = Some(page.project) == tui.project_id
        && Some(page.session) == tui.active_session_id
        && local.review_policy_stamp().ok().as_ref() == Some(&page.policy)
        && match &page.body {
            ReviewBody::Detail {
                sensitive_path: Some(path),
                ..
            }
            | ReviewBody::Change {
                sensitive_path: Some(path),
                ..
            }
            | ReviewBody::Restore {
                sensitive_path: Some(path),
                ..
            } => local.review_body_allowed(path).unwrap_or(false),
            _ => true,
        };
    if !allowed {
        let review = state_mut(tui).unwrap();
        review.cancel.cancel();
        review.page = None;
        review.notice="Review body unavailable: current scope or content access changed. Close and reopen to inspect permitted records.".into();
    }
}

pub(super) fn note_output(tui: &mut TuiState) {
    if let Some(review) = state_mut(tui) {
        review.updates = review.updates.saturating_add(1).min(999);
    }
}

fn choices(review: &State) -> Vec<(String, Choice)> {
    if let Some(flow) = review.restore.as_ref() {
        return match flow.stage {
            RestoreStage::Awaiting(approval_id) => vec![
                (
                    "Reject restoration".into(),
                    Choice::Command(AppCommand::RejectTool { approval_id }),
                ),
                (
                    "Approve this exact restoration once".into(),
                    Choice::Command(AppCommand::ApproveTool {
                        approval_id,
                        scope: ApprovalScope::Once,
                    }),
                ),
            ],
            RestoreStage::Requesting
            | RestoreStage::CancellingRequest
            | RestoreStage::Rejecting(_)
            | RestoreStage::Executing => Vec::new(),
        };
    }
    match review.page.as_ref().map(|page| &page.body) {
        Some(ReviewBody::Runs(runs)) => {
            let mut choices: Vec<_> = runs
                .items
                .iter()
                .enumerate()
                .map(|(i, run)| {
                    (
                        format!(
                            "{}{}",
                            run.run_id,
                            if i == 0
                                && matches!(
                                    review.navigation.request,
                                    ReviewRequest::Runs { before: None }
                                )
                            {
                                " [latest recorded run]"
                            } else {
                                ""
                            }
                        ),
                        Choice::Open(ReviewRequest::Run(run.run_id)),
                    )
                })
                .collect();
            if let Some(next) = &runs.next {
                choices.push((
                    "Older recorded runs...".into(),
                    Choice::Open(ReviewRequest::Runs {
                        before: Some(next.clone()),
                    }),
                ));
            }
            choices
        }
        Some(ReviewBody::Run { id, records, .. }) => {
            let mut choices: Vec<_> = records
                .iter()
                .map(|record| {
                    (
                        format!(
                            "{} | {} | {} | id={} | transaction={}",
                            record.path,
                            record.kind,
                            record.state,
                            record.id,
                            record
                                .operation_id
                                .map_or_else(|| "unavailable".into(), |id| id.to_string())
                        ),
                        Choice::Open(match record.file_change_id {
                            Some(change) => ReviewRequest::Diff { run: *id, change },
                            None => ReviewRequest::Operation {
                                run: *id,
                                operation: record.operation_id.unwrap_or(record.id),
                            },
                        }),
                    )
                })
                .collect();
            choices.push((
                "Verification receipt details...".into(),
                Choice::Open(ReviewRequest::Verification(*id)),
            ));
            choices.push((
                "Run diagnostics...".into(),
                Choice::Open(ReviewRequest::Diagnostics(*id)),
            ));
            choices
        }
        Some(ReviewBody::Change {
            run,
            change,
            restore_available: true,
            ..
        }) if review.restore_enabled => vec![(
            "Restore this change...".into(),
            Choice::Open(ReviewRequest::RestorePreview {
                run: *run,
                change: *change,
            }),
        )],
        Some(ReviewBody::Restore {
            run: _,
            change,
            available: true,
            ..
        }) if review.restore_enabled => vec![(
            "Continue to exact owner confirmation...".into(),
            Choice::Command(AppCommand::RequestRestore {
                file_change_id: *change,
            }),
        )],
        _ => Vec::new(),
    }
}

pub(super) fn rows(tui: &TuiState, width: usize, height: usize) -> Option<Vec<String>> {
    let review = state(tui)?;
    let header = [
        format!(
            "Changes / Review | panel owns navigation{}",
            if review.updates > 0 {
                format!(" | new output: {}", review.updates)
            } else {
                String::new()
            }
        ),
        "Arrows select/scroll; PgUp/PgDn scroll; Enter opens/selects; Esc cancels/back/closes. Typing returns to draft."
            .into(),
    ];
    let mut source = Vec::new();
    if !review.notice.is_empty() {
        source.push(review.notice.clone());
    }
    if let Some(page) = &review.page {
        match &page.body {
            ReviewBody::Runs(runs) => {
                source.push("Recorded runs in the current workspace and session. Selection does not change execution session.".into());
                if runs.items.is_empty() {
                    source.push("No recorded changes or verification receipts.".into());
                }
            }
            ReviewBody::Run { summary, .. } => source.extend(summary.lines().map(str::to_owned)),
            ReviewBody::Detail { text, .. }
            | ReviewBody::Change { text, .. }
            | ReviewBody::Restore { text, .. } => source.extend(text.lines().map(str::to_owned)),
        }
        if !review.restore_enabled {
            source.push(
                "Text-only mode keeps restoration unavailable; this panel is inspection-only."
                    .into(),
            );
        }
    }
    let options = choices(review);
    if !options.is_empty() {
        for (i, (label, _)) in options.iter().enumerate().skip(review.navigation.selected) {
            source.push(format!(
                "{} {label}",
                if i == review.navigation.selected {
                    ">"
                } else {
                    " "
                }
            ));
        }
    }
    let mut rows: Vec<_> = header
        .iter()
        .flat_map(|line| screen::wrap(line, width))
        .take(height.saturating_sub(1))
        .collect();
    let remaining = height.saturating_sub(rows.len());
    let content: Vec<_> = source
        .iter()
        .flat_map(|line| screen::wrap(line, width))
        .collect();
    let max = content.len().saturating_sub(remaining.max(1));
    review.max_offset.set(max);
    rows.extend(
        content
            .into_iter()
            .skip(review.navigation.offset.min(max))
            .take(remaining),
    );
    rows.resize(height, String::new());
    Some(rows)
}

pub(super) fn input(tui: &mut TuiState, input: InputEvent) -> TuiAction {
    // Do not discard ordinary typing or a paste when review owns navigation.
    // Return it to the existing composer path without synthesizing Enter.
    if matches!(
        input,
        InputEvent::Text(_)
            | InputEvent::Paste(_)
            | InputEvent::NewLine
            | InputEvent::Backspace
            | InputEvent::Delete
            | InputEvent::Left
            | InputEvent::Right
            | InputEvent::Home
            | InputEvent::End
    ) {
        tui.overlay = None;
        return tui.apply_input(input);
    }
    if let InputEvent::Resize(width, _) = input {
        tui.terminal_width = width.max(1);
        return TuiAction::Redraw;
    }
    if input == InputEvent::Eof {
        return TuiAction::Exit;
    }
    if input == InputEvent::CtrlC {
        tui.overlay = None;
        return tui.handle_ctrl_c();
    }
    if input == InputEvent::Escape {
        let review = state_mut(tui).unwrap();
        if let Some(flow) = review.restore.as_mut() {
            match flow.stage {
                RestoreStage::Requesting => {
                    flow.stage = RestoreStage::CancellingRequest;
                    review.notice = "Cancellation requested; the exact restoration approval will be rejected when preparation settles.".into();
                    return TuiAction::Redraw;
                }
                RestoreStage::CancellingRequest => {
                    review.notice =
                        "Cancellation is pending; no restoration approval will be granted.".into();
                    return TuiAction::Redraw;
                }
                RestoreStage::Awaiting(approval_id) => {
                    flow.stage = RestoreStage::Rejecting(approval_id);
                    review.notice = "Rejecting the exact restoration approval...".into();
                    return TuiAction::Dispatch(AppCommand::RejectTool { approval_id });
                }
                RestoreStage::Rejecting(_) => {
                    review.notice =
                        "Restoration rejection is settling; no approval will be granted.".into();
                    return TuiAction::Redraw;
                }
                RestoreStage::Executing => {
                    review.notice = "The exact restoration was already approved and is settling; wait for its recorded result.".into();
                    return TuiAction::Redraw;
                }
            }
        }
        if let Some(previous) = review.back.pop() {
            review.navigation = previous;
            return TuiAction::Ui(menu::Request::Review);
        }
        tui.overlay = None;
        return TuiAction::Redraw;
    }
    let review = state_mut(tui).unwrap();
    let options = choices(review);
    match input {
        InputEvent::HistoryPrevious if !options.is_empty() => {
            review.navigation.selected = review.navigation.selected.saturating_sub(1);
            review.navigation.offset = 0;
        }
        InputEvent::HistoryNext if !options.is_empty() => {
            review.navigation.selected = (review.navigation.selected + 1).min(options.len() - 1);
            review.navigation.offset = 0;
        }
        InputEvent::HistoryPrevious => {
            review.navigation.offset = review
                .navigation
                .offset
                .min(review.max_offset.get())
                .saturating_sub(1)
        }
        InputEvent::HistoryNext => {
            review.navigation.offset = (review.navigation.offset + 1).min(review.max_offset.get())
        }
        InputEvent::Scroll(n) => {
            review.navigation.offset =
                (review.navigation.offset.min(review.max_offset.get()) as i64 + i64::from(n))
                    .clamp(0, review.max_offset.get() as i64) as usize
        }
        InputEvent::Enter => {
            if let Some((_, choice)) = options.get(review.navigation.selected) {
                match choice {
                    Choice::Open(request) => {
                        if review.back.len() >= 64 {
                            review.notice = "Navigation limit reached. Esc returns to earlier pages; close and reopen for latest records.".into();
                            return TuiAction::Redraw;
                        }
                        review.back.push(review.navigation.clone());
                        review.navigation = Navigation {
                            request: request.clone(),
                            selected: 0,
                            offset: 0,
                        };
                        return TuiAction::Ui(menu::Request::Review);
                    }
                    Choice::Command(AppCommand::RequestRestore { file_change_id }) => {
                        let ReviewRequest::RestorePreview { run, change } =
                            review.navigation.request
                        else {
                            return TuiAction::Redraw;
                        };
                        if change != *file_change_id {
                            return TuiAction::Redraw;
                        }
                        let Some(ReviewBody::Restore {
                            sensitive_path: Some(target),
                            approval_binding: Some(binding),
                            available: true,
                            ..
                        }) = review.page.as_ref().map(|page| &page.body)
                        else {
                            review.notice =
                                "Restoration binding is unavailable; refresh the review.".into();
                            return TuiAction::Redraw;
                        };
                        review.restore = Some(RestoreFlow {
                            run,
                            change,
                            target: target.clone(),
                            binding: binding.clone(),
                            stage: RestoreStage::Requesting,
                        });
                        review.navigation.selected = 0;
                        review.notice = "Preparing the exact restoration approval...".into();
                        tui.followups.pause_all(
                            "Restoration review started; use /queue continue after it settles",
                        );
                        return TuiAction::Dispatch(AppCommand::RequestRestore {
                            file_change_id: *file_change_id,
                        });
                    }
                    Choice::Command(AppCommand::RejectTool { approval_id }) => {
                        if !matches!(
                            review.restore.as_ref().map(|flow| flow.stage),
                            Some(RestoreStage::Awaiting(id)) if id == *approval_id
                        ) {
                            return TuiAction::Redraw;
                        }
                        review.restore.as_mut().unwrap().stage =
                            RestoreStage::Rejecting(*approval_id);
                        review.notice = "Rejecting the exact restoration approval...".into();
                        return TuiAction::Dispatch(AppCommand::RejectTool {
                            approval_id: *approval_id,
                        });
                    }
                    Choice::Command(AppCommand::ApproveTool { approval_id, scope }) => {
                        if !matches!(
                            review.restore.as_ref().map(|flow| flow.stage),
                            Some(RestoreStage::Awaiting(id)) if id == *approval_id
                        ) {
                            return TuiAction::Redraw;
                        }
                        review.restore.as_mut().unwrap().stage = RestoreStage::Executing;
                        review.notice =
                            "Submitting the exact one-time owner restoration approval...".into();
                        return TuiAction::Dispatch(AppCommand::ApproveTool {
                            approval_id: *approval_id,
                            scope: scope.clone(),
                        });
                    }
                    Choice::Command(command) => return TuiAction::Dispatch(command.clone()),
                }
            }
        }
        _ => {}
    }
    TuiAction::Redraw
}

pub(super) fn approval(tui: &mut TuiState, request: &ApprovalRequest) -> bool {
    let Some(review) = state_mut(tui) else {
        return false;
    };
    let Some(flow) = review.restore.as_mut() else {
        return false;
    };
    if request.tool_id != "restore_file"
        || request.run_id != flow.run
        || request.target != flow.target
        || request.binding != flow.binding
    {
        return false;
    }
    let cancellation_pending = matches!(flow.stage, RestoreStage::CancellingRequest);
    if !cancellation_pending && !matches!(flow.stage, RestoreStage::Requesting) {
        return false;
    }
    flow.stage = if cancellation_pending {
        RestoreStage::Rejecting(request.id)
    } else {
        RestoreStage::Awaiting(request.id)
    };
    review.navigation.selected = 0;
    if cancellation_pending {
        review.pending_command = Some(AppCommand::RejectTool {
            approval_id: request.id,
        });
        review.notice =
            "Cancellation matched the exact restoration approval; rejecting it now.".into();
    } else {
        review.notice = format!(
            "Exact owner approval required for {}. Reject is selected by default.",
            request.target
        );
    }
    true
}

pub(super) fn take_command(tui: &mut TuiState) -> Option<AppCommand> {
    state_mut(tui)?.pending_command.take()
}

pub(super) fn approval_resolved(
    tui: &mut TuiState,
    approval_id: argentum_domain::ApprovalId,
    approved: bool,
) -> bool {
    let Some(review) = state_mut(tui) else {
        return false;
    };
    let Some(flow) = review.restore.as_mut() else {
        return false;
    };
    if !matches!(flow.stage, RestoreStage::Awaiting(id) | RestoreStage::Rejecting(id) if id == approval_id)
    {
        return false;
    }
    if approved {
        flow.stage = RestoreStage::Executing;
        review.notice = "Owner approval accepted; applying the bound restoration...".into();
    } else {
        review.restore = None;
        review.notice = "Restoration rejected; no file change was made.".into();
    }
    true
}

pub(super) fn restore_completed(
    tui: &mut TuiState,
    result: &argentum_domain::RestoreResult,
) -> bool {
    let Some(review) = state_mut(tui) else {
        return false;
    };
    let Some(flow) = review.restore.clone() else {
        return false;
    };
    if flow.run != result.run_id || flow.change != result.source_file_change_id {
        return false;
    }
    review.restore = None;
    review.navigation = Navigation {
        request: ReviewRequest::Run(flow.run),
        selected: 0,
        offset: 0,
    };
    review
        .back
        .retain(|navigation| matches!(navigation.request, ReviewRequest::Runs { .. }));
    review.page = None;
    review.notice = format!(
        "Restoration recorded as change {}. Refreshing the original run history.",
        result.restore_file_change_id
    );
    review.refresh = true;
    true
}

pub(super) fn take_refresh(tui: &mut TuiState) -> bool {
    let Some(review) = state_mut(tui) else {
        return false;
    };
    std::mem::take(&mut review.refresh)
}

pub(super) fn command_failed(tui: &mut TuiState) {
    let Some(review) = state_mut(tui) else {
        return;
    };
    if review.restore.take().is_some() {
        review.notice =
            "Restoration did not complete. Current bytes and recorded history were preserved; reopen the review for a fresh proposal.".into();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn f21a_selected_run_opens_diagnostics_without_losing_draft_or_navigation() {
        let root = tempfile::tempdir().unwrap();
        let mut tui = TuiState::new(root.path().into(), true);
        tui.composer = "draft remains".into();
        tui.composer_cursor = tui.composer.len();
        begin(&mut tui);
        let run = RunId::new_v4();
        let review = state_mut(&mut tui).unwrap();
        review.page = Some(ReviewPage {
            project: RunId::new_v4(),
            session: RunId::new_v4(),
            policy: "fixture".into(),
            body: ReviewBody::Run {
                id: run,
                summary: "fixture".into(),
                records: Vec::new(),
            },
        });
        review.navigation.request = ReviewRequest::Run(run);
        review.navigation.selected = 1;
        review.navigation.offset = 7;

        assert_eq!(
            input(&mut tui, InputEvent::Enter),
            TuiAction::Ui(menu::Request::Review)
        );
        assert!(matches!(
            position(&tui),
            Some((ReviewRequest::Diagnostics(id), 0, 0)) if id == run
        ));
        assert_eq!(tui.composer, "draft remains");
        assert_eq!(
            input(&mut tui, InputEvent::Escape),
            TuiAction::Ui(menu::Request::Review)
        );
        assert!(matches!(
            position(&tui),
            Some((ReviewRequest::Run(id), 1, 7)) if id == run
        ));
        assert_eq!(tui.composer, "draft remains");
    }

    #[test]
    fn f20e_escape_during_restore_preparation_rejects_the_late_exact_approval() {
        let root = tempfile::tempdir().unwrap();
        let mut tui = TuiState::new(root.path().into(), true);
        begin(&mut tui);
        let run = RunId::new_v4();
        let change = argentum_domain::FileChangeId::new_v4();
        state_mut(&mut tui).unwrap().restore = Some(RestoreFlow {
            run,
            change,
            target: "fixture.txt".into(),
            binding: argentum_domain::Sha256Digest("a".repeat(64)),
            stage: RestoreStage::Requesting,
        });

        assert_eq!(input(&mut tui, InputEvent::Escape), TuiAction::Redraw);
        assert!(matches!(
            state(&tui).unwrap().restore.as_ref().unwrap().stage,
            RestoreStage::CancellingRequest
        ));

        let request = ApprovalRequest {
            id: argentum_domain::ApprovalId::new_v4(),
            run_id: run,
            tool_id: "restore_file".into(),
            action: "Restore".into(),
            target: "fixture.txt".into(),
            binding: argentum_domain::Sha256Digest("a".repeat(64)),
            reason: "fixture".into(),
            capabilities: vec![argentum_domain::Capability::WriteFiles],
            scope: argentum_domain::ApprovalScope::Once,
            expires_at: None,
        };
        let mut wrong_request = request.clone();
        wrong_request.binding = argentum_domain::Sha256Digest("b".repeat(64));
        assert!(!approval(&mut tui, &wrong_request));
        assert_eq!(take_command(&mut tui), None);
        assert!(approval(&mut tui, &request));
        assert_eq!(
            take_command(&mut tui),
            Some(AppCommand::RejectTool {
                approval_id: request.id
            })
        );
        assert!(approval_resolved(&mut tui, request.id, false));
        assert!(state(&tui).unwrap().restore.is_none());
    }

    #[test]
    fn f20e_escape_cannot_claim_an_approved_restore_was_cancelled() {
        let root = tempfile::tempdir().unwrap();
        let mut tui = TuiState::new(root.path().into(), true);
        begin(&mut tui);
        state_mut(&mut tui).unwrap().restore = Some(RestoreFlow {
            run: RunId::new_v4(),
            change: argentum_domain::FileChangeId::new_v4(),
            target: "fixture.txt".into(),
            binding: argentum_domain::Sha256Digest("a".repeat(64)),
            stage: RestoreStage::Executing,
        });

        assert_eq!(input(&mut tui, InputEvent::Escape), TuiAction::Redraw);
        assert!(matches!(
            state(&tui).unwrap().restore.as_ref().unwrap().stage,
            RestoreStage::Executing
        ));
        assert!(state(&tui)
            .unwrap()
            .notice
            .contains("already approved and is settling"));
    }

    #[tokio::test]
    async fn f20d_review_obsolete_foreign_revoked_and_hostile_rendering() {
        let root = tempfile::tempdir().unwrap();
        let mut host = CommandHost::start_composed(crate::HostConfig::in_memory(root.path()))
            .await
            .unwrap();
        let local = host.local_handle();
        let mut tui = TuiState::new(root.path().into(), true);
        let scope = host.workspace_snapshot().unwrap();
        tui.project_id = Some(scope.project.id);
        tui.active_session_id = scope.active_session_id;
        tui.composer = "draft stays e\u{301} 界".into();
        tui.composer_cursor = tui.composer.len();
        begin(&mut tui);
        let old = state(&tui).unwrap().ticket;
        let page = local
            .review_page(
                scope.project.id,
                scope.active_session_id.unwrap(),
                ReviewRequest::Runs { before: None },
            )
            .unwrap();
        begin(&mut tui);
        let current = state(&tui).unwrap().ticket;
        finish(
            &mut tui,
            &local,
            Completion {
                ticket: old,
                page: Ok(page.clone()),
            },
        );
        assert!(state(&tui).unwrap().page.is_none());
        let mut foreign = page.clone();
        foreign.session = RunId::new_v4();
        finish(
            &mut tui,
            &local,
            Completion {
                ticket: current,
                page: Ok(foreign),
            },
        );
        assert!(state(&tui).unwrap().page.is_none());
        let mut hostile = page.clone();
        hostile.body = ReviewBody::Detail {
            text:
                "Привет e\u{301} 界\r\n\u{1b}]52;c;INERT\u{7}\u{1b}[2J\nrepeated repeated repeated"
                    .into(),
            sensitive_path: Some("note.txt".into()),
        };
        finish(
            &mut tui,
            &local,
            Completion {
                ticket: current,
                page: Ok(hostile),
            },
        );
        for width in [1, 2, 9, 43, 100] {
            tui.apply_input(InputEvent::Resize(width, 24));
            let frame = screen::frame(&tui, width, 24);
            assert_eq!(frame.rows.len(), 24);
            assert!(frame
                .rows
                .iter()
                .all(|r| !r.contains('\u{1b}') && !r.contains('\u{7}') && !r.contains('\r')));
            assert!(frame
                .rows
                .iter()
                .all(|r| unicode_width::UnicodeWidthStr::width(r.as_str())
                    <= usize::from(width.saturating_sub(1))));
            if width == 100 {
                assert!(frame.rows.join("\n").contains("Привет e\u{301} 界"));
            }
        }
        // A shorter replacement must not retain old terminal content in its frame.
        let mut short = page.clone();
        short.body = ReviewBody::Detail {
            text: "short".into(),
            sensitive_path: None,
        };
        finish(
            &mut tui,
            &local,
            Completion {
                ticket: current,
                page: Ok(short),
            },
        );
        assert!(!screen::frame(&tui, 100, 24)
            .rows
            .join("\n")
            .contains("INERT"));
        let mut body = page;
        body.body = ReviewBody::Detail {
            text: "historical body".into(),
            sensitive_path: Some("note.txt".into()),
        };
        finish(
            &mut tui,
            &local,
            Completion {
                ticket: current,
                page: Ok(body),
            },
        );
        local
            .dispatch(AppCommand::SetHarnessCapabilityEnabled {
                capability_id: "tool.read-text".into(),
                enabled: false,
            })
            .await
            .unwrap();
        revalidate(&mut tui, &local);
        let frame = screen::frame(&tui, 100, 24).rows.join("\n");
        assert!(!frame.contains("historical body"));
        assert!(frame.contains("unavailable"));
        tui.apply_input(InputEvent::Escape);
        assert!(tui.overlay.is_none());
        assert_eq!(tui.composer, "draft stays e\u{301} 界");
        begin(&mut tui);
        assert_eq!(
            tui.apply_input(InputEvent::Paste(" /quit\nrepeated repeated".into())),
            TuiAction::Redraw
        );
        assert!(tui.overlay.is_none() && tui.composer.ends_with(" /quit\nrepeated repeated"));
        host.shutdown().await.unwrap();
    }
}
