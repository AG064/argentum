use std::collections::BTreeMap;
use std::io::{self, IsTerminal, Write};
use std::path::PathBuf;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use std::thread;
use std::time::Duration;

use argentum_domain::{
    AppCommand, AppEvent, ApprovalRequest, ApprovalScope, ConversationRole,
    PlatformAuthoritySnapshot, ProviderProfile, RunId, SessionSummary, TaskLifecycle, ToolCallId,
    ToolResultState, WorkspaceSnapshot,
};
use crossterm::{
    cursor::Show,
    event::{
        self, DisableBracketedPaste, EnableBracketedPaste, Event, KeyCode, KeyEvent, KeyEventKind,
        KeyModifiers,
    },
    execute,
    terminal::{self, EnterAlternateScreen, LeaveAlternateScreen},
};
use thiserror::Error;
use tokio::io::{AsyncBufRead, AsyncBufReadExt, AsyncWrite, AsyncWriteExt};
use tokio::sync::mpsc;
use tokio::task::JoinSet;
use unicode_segmentation::UnicodeSegmentation;

use crate::{CommandHost, HostError, InProcessClient, LocalCommandHandle};
use argentum_runtime::{SubmissionTarget, TaskSubmission, TaskSubmissionResult};
mod credential_input;
mod local_text;
pub use credential_input::prompt_provider_secret;
mod menu;
mod queue;
mod review;
mod screen;
mod settings;
mod transcript;
pub use local_text::run_local_text_session;
mod terminal_diagnostic;
#[cfg(windows)]
mod windows_console;
pub use terminal_diagnostic::run_terminal_diagnostic;
#[cfg(test)]
mod f20c_tests;
#[cfg(test)]
mod queue_integration;
#[cfg(test)]
mod queue_tests;
pub use queue::{MAX_QUEUE_BYTES, MAX_QUEUE_ENTRIES};

const MAX_COMPOSER_BYTES: usize = 128 * 1024;
const MAX_HISTORY_ENTRIES: usize = 128;
const MAX_TIMELINE_ENTRIES: usize = 128;
const MAX_RENDERED_TEXT_CHARS: usize = 32 * 1024;
const DEFAULT_TERMINAL_WIDTH: u16 = 80;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TuiOptions {
    pub no_color: bool,
    pub force_non_interactive: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InputEvent {
    Help,
    Access,
    Palette,
    Escape,
    ConfirmAccess(bool),
    Text(String),
    NewLine,
    Enter,
    Backspace,
    Delete,
    Left,
    Right,
    Home,
    End,
    Scroll(i32),
    Latest,
    Paste(String),
    PasteClipboard,
    ClearInput,
    HistoryPrevious,
    HistoryNext,
    Complete,
    CtrlC,
    Eof,
    Resize(u16, u16),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TuiAction {
    Ui(menu::Request),
    HistoryPage {
        cursor: Option<argentum_domain::MessageId>,
        newer: bool,
        latest: bool,
    },
    LocalText(String),
    None,
    Redraw,
    Dispatch(AppCommand),
    Submit(Box<TaskSubmission>),
    Steer {
        submission: argentum_runtime::SteeringSubmission,
        draft: String,
    },
    InspectContext(argentum_domain::SessionId),
    CompactContext {
        session: argentum_domain::SessionId,
        operation: RunId,
        deactivate: bool,
    },
    Exit,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TimelineKind {
    User,
    Assistant,
    Tool,
    Approval,
    System,
}

#[derive(Debug, Clone)]
struct TimelineEntry {
    view_id: RunId,
    message_id: Option<argentum_domain::MessageId>,
    kind: TimelineKind,
    run_id: Option<RunId>,
    tool_call_id: Option<ToolCallId>,
    text: String,
    detail: String,
    state: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CommandKind {
    Access,
    Compact,
    SubmitTask,
    CancelRun,
    Session,
    Other,
}

#[derive(Debug, Error)]
pub enum TuiError {
    #[error(transparent)]
    Io(#[from] io::Error),
    #[error(transparent)]
    Host(#[from] HostError),
    #[error("terminal session failed: {session}; runtime shutdown also failed: {shutdown}")]
    SessionAndShutdown { session: String, shutdown: String },
    #[error("interactive command task failed: {0}")]
    CommandTask(String),
    #[error("terminal session failed: {session}; command cleanup also failed: {cleanup}")]
    SessionAndCommandCleanup { session: String, cleanup: String },
}

pub struct TuiState {
    settings_busy: bool,
    next_skill: Option<argentum_workspaces::SelectedSkill>,
    workspace: PathBuf,
    no_color: bool,
    project_name: String,
    project_id: Option<argentum_domain::ProjectId>,
    sessions: Vec<SessionSummary>,
    active_session_id: Option<argentum_domain::SessionId>,
    active_session_title: String,
    provider_label: String,
    model: String,
    permission_mode: String,
    goal_accounting_summary: Option<String>,
    platform_authority: Option<PlatformAuthoritySnapshot>,
    timeline: Vec<TimelineEntry>,
    composer: String,
    composer_cursor: usize,
    history: Vec<String>,
    history_index: Option<usize>,
    completion_index: usize,
    active_run_id: Option<RunId>,
    manual_compaction: Option<RunId>,
    manual_cancellation: Option<tokio_util::sync::CancellationToken>,
    pending_approval: Option<ApprovalRequest>,
    session_approvals: BTreeMap<argentum_domain::SessionId, ApprovalRequest>,
    followups: queue::FollowUps,
    pending_steering: BTreeMap<argentum_domain::SteeringEntryId, RunId>,
    observed_runs: BTreeMap<argentum_domain::SessionId, RunId>,
    session_transition: bool,
    quit_armed: bool,
    shutdown_notice: Option<String>,
    status: String,
    progress_tick: u8,
    interrupt_armed: bool,
    terminal_width: u16,
    coding_preflight_issue: Option<String>,
    diagnostic_active: bool,
    local_text_mode: bool,
    local_text_limit: usize,
    viewport: transcript::Viewport,
    overlay: Option<menu::Overlay>,
    access_review: Option<crate::LocalAccessReview>,
    access_pending: bool,
    queue_continue_requested: bool,
    editor_target: Option<SubmissionTarget>,
    drafts: BTreeMap<SubmissionTarget, (String, usize, Vec<String>)>,
    draft_order: std::collections::VecDeque<SubmissionTarget>,
    streams: BTreeMap<RunId, (argentum_domain::SessionId, TimelineEntry)>,
}

impl TuiState {
    pub fn new(workspace: PathBuf, no_color: bool) -> Self {
        Self {
            settings_busy: false,
            next_skill: None,
            workspace,
            no_color,
            project_name: String::new(),
            project_id: None,
            sessions: Vec::new(),
            active_session_id: None,
            active_session_title: String::new(),
            provider_label: "not configured".into(),
            model: "unknown".into(),
            permission_mode: "Guided".into(),
            goal_accounting_summary: None,
            platform_authority: None,
            timeline: Vec::new(),
            composer: String::new(),
            composer_cursor: 0,
            history: Vec::new(),
            history_index: None,
            completion_index: 0,
            active_run_id: None,
            manual_compaction: None,
            manual_cancellation: None,
            pending_approval: None,
            session_approvals: BTreeMap::new(),
            followups: queue::FollowUps::default(),
            pending_steering: BTreeMap::new(),
            observed_runs: BTreeMap::new(),
            session_transition: false,
            quit_armed: false,
            shutdown_notice: None,
            status: "Ready".into(),
            progress_tick: 0,
            interrupt_armed: false,
            terminal_width: DEFAULT_TERMINAL_WIDTH,
            coding_preflight_issue: None,
            diagnostic_active: false,
            local_text_mode: false,
            local_text_limit: 4096,
            viewport: transcript::Viewport::default(),
            overlay: None,
            access_review: None,
            access_pending: false,
            queue_continue_requested: false,
            editor_target: None,
            drafts: BTreeMap::new(),
            draft_order: std::collections::VecDeque::new(),
            streams: BTreeMap::new(),
        }
    }

    pub fn set_coding_preflight_issue(&mut self, issue: Option<String>) {
        self.coding_preflight_issue = issue;
    }

    pub fn composer(&self) -> &str {
        &self.composer
    }

    pub fn clear_composer(&mut self) {
        self.composer.clear();
        self.composer_cursor = 0;
        self.history_index = None;
        self.completion_index = 0;
    }

    pub fn active_run_id(&self) -> Option<RunId> {
        self.active_run_id
    }

    pub fn has_pending_approval(&self) -> bool {
        self.pending_approval.is_some()
    }

    pub fn submission_target(&self) -> Option<SubmissionTarget> {
        Some(SubmissionTarget {
            project_id: self.project_id?,
            session_id: self.active_session_id?,
            workspace: self.workspace.clone(),
        })
    }

    pub fn take_ready_submission(&mut self) -> Option<TaskSubmission> {
        if self.access_pending || self.settings_busy {
            return None;
        }
        if self.manual_compaction.is_some() {
            return None;
        }
        if self.session_transition || self.quit_armed || self.active_run_id.is_some() {
            return None;
        }
        self.followups.claim(&self.submission_target()?)
    }

    pub fn submission_finished(&mut self, completion: TaskSubmissionResult) {
        let selected = self.followups.target_for_run(completion.run_id).as_ref()
            == self.submission_target().as_ref();
        if let Some(notice) = self.followups.completed(completion) {
            if selected {
                self.status = bounded_text(&notice);
                self.append_system(&notice);
            }
        }
    }

    pub fn queue_description(&self) -> String {
        self.followups.describe()
    }

    fn begin_shutdown(&mut self) {
        self.overlay = None;
        if let Some(cancellation) = &self.manual_cancellation {
            cancellation.cancel();
        }
        if self.shutdown_notice.is_some() {
            return;
        }
        let (pending, current) = self.followups.stop();
        let steering = self.pending_steering.len();
        let notice = format!("Shutdown: {pending} pending prompts were not executed and are discarded (memory only); {steering} pending steering entries may be lost before inclusion; {current} current submissions remain owned by cancellation/drain.");
        self.status = notice.clone();
        self.shutdown_notice = Some(notice);
    }

    fn take_ready_cancel(&mut self) -> Option<AppCommand> {
        self.followups
            .take_cancel(&self.submission_target()?)
            .map(|run_id| AppCommand::CancelRun { run_id })
    }

    pub fn apply_input(&mut self, input: InputEvent) -> TuiAction {
        if self.overlay.is_some() {
            return self.overlay_input(input);
        }
        match input {
            InputEvent::PasteClipboard => match credential_input::read_clipboard() {
                Ok(text) => self.apply_input(InputEvent::Paste(normalize_paste(&text))),
                Err(_) => {
                    self.status = "Clipboard unavailable; use terminal paste.".into();
                    TuiAction::Redraw
                }
            },
            InputEvent::ClearInput => {
                self.clear_composer();
                TuiAction::Redraw
            }
            InputEvent::Help | InputEvent::Palette => {
                self.open_commands(false);
                TuiAction::Redraw
            }
            InputEvent::Access => TuiAction::Ui(menu::Request::Open(menu::View::Permissions)),
            InputEvent::Escape | InputEvent::ConfirmAccess(_) => TuiAction::Redraw,
            InputEvent::Scroll(rows) => self.scroll_rows(rows),
            InputEvent::Latest => self.jump_latest(),
            InputEvent::Paste(text) => {
                self.insert_text(&normalize_paste(&text));
                self.reset_interrupt();
                TuiAction::Redraw
            }
            InputEvent::Text(text) => {
                self.insert_text(&text);
                if self.composer == "/" {
                    self.open_commands(true);
                }
                self.reset_interrupt();
                TuiAction::Redraw
            }
            InputEvent::NewLine => {
                self.insert_text("\n");
                self.reset_interrupt();
                TuiAction::Redraw
            }
            InputEvent::Enter => {
                self.reset_interrupt();
                self.submit_composer()
            }
            InputEvent::Backspace => {
                let start = self.previous_cursor();
                self.composer.drain(start..self.composer_cursor);
                self.composer_cursor = start;
                self.history_index = None;
                self.completion_index = 0;
                self.reset_interrupt();
                TuiAction::Redraw
            }
            InputEvent::Delete => {
                let end = self.next_cursor();
                self.composer.drain(self.composer_cursor..end);
                self.reset_interrupt();
                TuiAction::Redraw
            }
            InputEvent::Left | InputEvent::Right | InputEvent::Home | InputEvent::End => {
                self.composer_cursor = match input {
                    InputEvent::Left => self.previous_cursor(),
                    InputEvent::Right => self.next_cursor(),
                    InputEvent::Home => self.composer[..self.composer_cursor]
                        .rfind('\n')
                        .map_or(0, |n| n + 1),
                    _ => self.composer[self.composer_cursor..]
                        .find('\n')
                        .map_or(self.composer.len(), |n| self.composer_cursor + n),
                };
                self.reset_interrupt();
                TuiAction::Redraw
            }
            InputEvent::HistoryPrevious => {
                self.history_previous();
                self.reset_interrupt();
                TuiAction::Redraw
            }
            InputEvent::HistoryNext => {
                self.history_next();
                self.reset_interrupt();
                TuiAction::Redraw
            }
            InputEvent::Complete => {
                self.complete_slash_command();
                self.reset_interrupt();
                TuiAction::Redraw
            }
            InputEvent::CtrlC => self.handle_ctrl_c(),
            InputEvent::Eof => {
                self.begin_shutdown();
                TuiAction::Exit
            }
            InputEvent::Resize(width, _) => {
                self.terminal_width = width.max(20);
                TuiAction::Redraw
            }
        }
    }

    pub fn apply_event(&mut self, event: AppEvent) {
        match event {
            AppEvent::GoalSnapshotLoaded {
                session_id,
                goal: Some(goal),
            } if self.active_session_id == Some(session_id) => {
                if let Some(account) = goal.accounting.as_ref() {
                    let counters = &account.counters;
                    let remaining = account.remaining(goal.token_budget);
                    self.goal_accounting_summary = Some(format!(
                        "Goal {} revision {}: admitted attempts {}, requests {}, tools {}; observed tokens {}, reserved {}, unresolved {}; estimated remaining {:?} ({}); stop {}. Goal completion is not verified.",
                        goal.id, account.revision, counters.iterations, counters.request_attempts,
                        counters.tool_admissions, counters.observed_tokens, counters.outstanding_tokens,
                        counters.unresolved_tokens, remaining.estimated_tokens, remaining.reason,
                        account.stop_reason.as_deref().unwrap_or("none")
                    ));
                } else {
                    self.goal_accounting_summary = Some(
                        "Goal accounting unavailable: legacy history is not a known zero.".into(),
                    );
                }
            }
            AppEvent::GoalSnapshotLoaded {
                session_id,
                goal: None,
            } if self.active_session_id == Some(session_id) => {
                self.goal_accounting_summary = None;
            }
            AppEvent::GoalInvocationSnapshot {
                session_id,
                invocation: Some(invocation),
            } => {
                self.followups.goal_invocation(session_id, &invocation);
                if self.active_session_id == Some(session_id) {
                    self.status = format!(
                        "Goal supervision {:?}: {}",
                        invocation.state,
                        invocation.stop_reason.as_deref().unwrap_or("active")
                    );
                }
            }
            AppEvent::ContextCompactionCompleted {
                session_id, result, ..
            } if self.active_session_id == Some(session_id) => {
                self.append_system(&format!(
                    "Context compaction {:?}: {}",
                    result.status, result.reason
                ));
            }
            AppEvent::ContextCompactionProgress {
                session_id, phase, ..
            } if self.active_session_id == Some(session_id) => {
                self.append_system(&format!("Context compaction: {phase}"));
            }
            AppEvent::PlatformAuthorityChanged(snapshot) => {
                self.platform_authority = Some(snapshot);
            }
            AppEvent::WorkspaceStateLoaded(snapshot) => self.apply_workspace(snapshot),
            AppEvent::ConversationSnapshotLoaded(snapshot) => {
                if self.active_session_id != Some(snapshot.session_id)
                    || self.project_id != Some(snapshot.project_id)
                {
                    return;
                }
                let incoming: Vec<_> = snapshot
                    .messages
                    .into_iter()
                    .map(|message| TimelineEntry {
                        view_id: message.id,
                        message_id: Some(message.id),
                        kind: match message.role {
                            ConversationRole::User => TimelineKind::User,
                            ConversationRole::Assistant => TimelineKind::Assistant,
                        },
                        run_id: Some(message.run_id),
                        tool_call_id: None,
                        text: bounded_text(&message.text),
                        detail: bounded_text(&message.reasoning),
                        state: format!("{:?}", message.status).to_lowercase(),
                    })
                    .collect();
                self.streams.retain(|run, _| {
                    !incoming.iter().any(|entry| {
                        entry.kind == TimelineKind::Assistant && entry.run_id == Some(*run)
                    })
                });
                if self.viewport.anchor.is_some() {
                    for entry in incoming {
                        if let Some(old) = self
                            .timeline
                            .iter_mut()
                            .find(|old| old.key() == entry.key())
                        {
                            *old = entry;
                        } else {
                            self.viewport.newer = true;
                        }
                    }
                } else {
                    self.replace_messages_preserving_tools(incoming);
                    self.merge_current_streams();
                    self.viewport.older = true;
                }
                self.transcript_changed();
                self.truncate_timeline();
            }
            AppEvent::ActiveRunsSnapshot { runs } => {
                self.observed_runs = runs
                    .into_iter()
                    .map(|run| (run.session_id, run.run_id))
                    .collect();
                self.active_run_id = self
                    .active_session_id
                    .and_then(|id| self.observed_runs.get(&id).copied());
                if self.active_run_id.is_some() {
                    self.status = "Work in progress".into();
                }
            }
            AppEvent::SessionCreated(session) => {
                self.bind_editor(self.project_id.map(|project_id| SubmissionTarget {
                    project_id,
                    session_id: session.id,
                    workspace: self.workspace.clone(),
                }));
                if self.active_session_id != Some(session.id) {
                    self.followups
                        .pause_all("Session changed; use /queue continue after returning");
                    self.active_run_id = self.observed_runs.get(&session.id).copied();
                }
                self.active_session_id = Some(session.id);
                self.pending_approval = self.session_approvals.get(&session.id).cloned();
                self.active_session_title = bounded_text(&session.title);
                self.status = "New session created".into();
            }
            AppEvent::TaskAccepted(task) => {
                self.followups.admitted(task.id, task.session_id);
                if self.active_session_id == Some(task.session_id) {
                    self.status = task.lifecycle.label().into();
                    if let Some(target) = self.submission_target() {
                        self.active_run_id =
                            self.followups.active_run(&target).or(self.active_run_id);
                    }
                }
            }
            AppEvent::RunStatusChanged { run_id, lifecycle } => {
                let target = self.followups.target_for_run(run_id);
                let belongs_here = target
                    .as_ref()
                    .is_some_and(|target| Some(target) == self.submission_target().as_ref())
                    || self.active_run_id == Some(run_id)
                    || self.active_session_id.is_none();
                if lifecycle.is_terminal() {
                    self.followups.terminal(run_id, lifecycle);
                    self.observed_runs.retain(|_, active| *active != run_id);
                    if self.active_run_id == Some(run_id) {
                        self.active_run_id = None;
                    }
                    if belongs_here {
                        self.status = lifecycle.label().into();
                        self.interrupt_armed = false;
                    }
                } else if belongs_here {
                    self.active_run_id = Some(run_id);
                    if let Some(session) = self.active_session_id {
                        self.observed_runs.insert(session, run_id);
                    }
                    self.status = if lifecycle == TaskLifecycle::Running {
                        "Waiting for model (answer or tool call)".into()
                    } else {
                        lifecycle.label().into()
                    };
                }
            }
            AppEvent::SteeringChanged(steering) => {
                let current = self.run_is_current(steering.run_id);
                match steering.lifecycle {
                    argentum_domain::SteeringLifecycle::Pending => {
                        self.pending_steering
                            .insert(steering.entry_id, steering.run_id);
                    }
                    argentum_domain::SteeringLifecycle::Included
                    | argentum_domain::SteeringLifecycle::NotApplied => {
                        self.pending_steering.remove(&steering.entry_id);
                    }
                }
                if !current {
                    return;
                }
                let message = match steering.lifecycle {
                    argentum_domain::SteeringLifecycle::Pending => format!(
                        "Steering {} pending at position {} ({} bytes)",
                        steering.entry_id, steering.position, steering.text_bytes
                    ),
                    argentum_domain::SteeringLifecycle::Included => format!(
                        "Steering {} included in a model request at position {}. Inclusion does not prove model obedience.",
                        steering.entry_id, steering.position
                    ),
                    argentum_domain::SteeringLifecycle::NotApplied => format!(
                        "Steering {} was not applied: {}",
                        steering.entry_id,
                        steering.reason.as_deref().unwrap_or("run settled")
                    ),
                };
                self.status = bounded_text(&message);
                self.append_system(&message);
            }
            AppEvent::AssistantDelta { run_id, text } => {
                self.remember_stream(run_id, &text);
                if self.run_is_current(run_id) {
                    review::note_output(self);
                    self.status = "Receiving response".into();
                    self.append_assistant_delta(run_id, &text);
                }
            }
            AppEvent::AssistantReasoningDelta { run_id, text } => {
                if self.active_run_id != Some(run_id) && self.active_session_id.is_some() {
                    return;
                }
                self.transcript_changed();
                if let Some(entry) = self.timeline.iter_mut().rev().find(|entry| {
                    entry.run_id == Some(run_id) && entry.kind == TimelineKind::Assistant
                }) {
                    append_bounded(&mut entry.detail, &text);
                } else {
                    self.timeline.push(TimelineEntry {
                        view_id: RunId::new_v4(),
                        message_id: None,
                        kind: TimelineKind::Assistant,
                        run_id: Some(run_id),
                        tool_call_id: None,
                        text: String::new(),
                        detail: bounded_text(&text),
                        state: "reasoning".into(),
                    });
                    self.truncate_timeline();
                }
            }
            AppEvent::ModelUsageUpdated {
                session_id, usage, ..
            } if self.active_session_id == Some(session_id) => {
                if self.active_run_id.is_none() {
                    self.status = format!("Used {} tokens", usage.total_tokens);
                }
            }
            AppEvent::ToolStarted(trace) => {
                if self.run_is_current(trace.run_id) {
                    self.status = format!("Executing {}", trace.tool_id);
                    self.append_tool(trace, "running");
                }
            }
            AppEvent::ToolFinished(trace) => {
                if self.run_is_current(trace.run_id) {
                    review::note_output(self);
                    self.status = "Waiting for model (answer or tool call)".into();
                    self.finish_tool(trace);
                }
            }
            AppEvent::ApprovalRequested(request) => {
                let review_owns_approval = review::approval(self, &request);
                let origin = self
                    .followups
                    .target_for_run(request.run_id)
                    .map(|target| target.session_id)
                    .or_else(|| {
                        self.observed_runs
                            .iter()
                            .find_map(|(session, run)| (*run == request.run_id).then_some(*session))
                    });
                if let Some(session) = origin {
                    self.session_approvals.insert(session, request.clone());
                    if self.active_session_id != Some(session) {
                        return;
                    }
                }
                self.pending_approval = Some(request.clone());
                self.status = "Approval required".into();
                self.timeline.push(TimelineEntry {
                    view_id: request.id,
                    message_id: None,
                    kind: TimelineKind::Approval,
                    run_id: Some(request.run_id),
                    tool_call_id: None,
                    text: "Approval required".into(),
                    detail: bounded_text(&format!(
                        "{} on {}. {}",
                        request.action, request.target, request.reason
                    )),
                    state: "waiting".into(),
                });
                self.truncate_timeline();
                if !review_owns_approval {
                    settings::approval(self, &request);
                }
            }
            AppEvent::ApprovalResolved {
                approval_id,
                approved,
            } => {
                review::approval_resolved(self, approval_id, approved);
                self.session_approvals
                    .retain(|_, request| request.id != approval_id);
                if self.pending_approval.as_ref().map(|request| request.id) != Some(approval_id) {
                    return;
                }
                self.pending_approval = None;
                self.status = if approved {
                    "Approval granted".into()
                } else {
                    "Approval rejected".into()
                };
            }
            AppEvent::RestoreCompleted(result) => {
                if review::restore_completed(self, &result) {
                    self.status = format!("Restored {}", bounded_text(&result.relative_path));
                }
            }
            AppEvent::ProviderProfilesSnapshot { profiles } => {
                self.apply_provider_profiles(&profiles);
            }
            AppEvent::HarnessSnapshotLoaded(snapshot) => {
                self.permission_mode = snapshot
                    .execution_profiles
                    .iter()
                    .find(|profile| profile.selected)
                    .map(|profile| profile.label.clone())
                    .unwrap_or_else(|| snapshot.selected_execution_profile_id.clone());
            }
            AppEvent::Error { message, .. } => {
                self.status = format!("Error: {}", bounded_text(&message));
                self.append_system(&format!("Error: {message}"));
            }
            AppEvent::RunError {
                session_id,
                message,
                ..
            } if self.active_session_id == Some(session_id) => {
                self.status = format!("Run failed: {}", bounded_text(&message));
                self.append_system(&format!("Run failed: {message}"));
            }
            _ => {}
        }
    }

    pub fn command_started(&mut self, command: &AppCommand) {
        match command {
            AppCommand::SubmitTask { .. } => {
                self.status = "Submitting".into();
            }
            AppCommand::CancelRun { .. } => {
                self.status = "Interrupt requested".into();
            }
            AppCommand::NewSession | AppCommand::SelectSession { .. } => {
                self.session_transition = true;
                self.followups
                    .pause_all("Session switch requested; use /queue continue to resume");
                self.status = "Switching session".into();
            }
            _ => {}
        }
    }

    fn command_finished(&mut self, kind: CommandKind, result: &Result<(), String>) {
        if kind == CommandKind::Access {
            self.access_pending = false;
            self.overlay = None;
            self.access_review = None;
        }
        if kind == CommandKind::Compact {
            if self.active_run_id == self.manual_compaction {
                self.active_run_id = None;
            }
            self.manual_compaction = None;
            self.manual_cancellation = None;
        }
        if kind == CommandKind::Session {
            self.session_transition = false;
        }
        if let Err(message) = result {
            review::command_failed(self);
            self.status = format!("Command failed: {}", bounded_text(message));
            self.append_system(&format!("Command failed: {message}"));
        }
    }

    fn body_lines(&self, width: u16) -> Vec<String> {
        let width = usize::from(
            if width == 0 {
                self.terminal_width
            } else {
                width
            }
            .max(1),
        );
        let mut lines = Vec::new();
        let _ = self.no_color;
        push_wrapped(&mut lines, "ARGENTUM", width);
        if !self.project_name.is_empty() {
            push_wrapped(
                &mut lines,
                &format!("project: {}", self.project_name),
                width,
            );
        }
        push_wrapped(
            &mut lines,
            &format!("workspace: {}", self.workspace.display()),
            width,
        );
        push_wrapped(
            &mut lines,
            &format!("provider: {}  model: {}", self.provider_label, self.model),
            width,
        );
        push_wrapped(
            &mut lines,
            &format!("permission: {}", self.permission_mode),
            width,
        );
        if let Some(summary) = &self.goal_accounting_summary {
            push_wrapped(&mut lines, summary, width);
        }
        if let Some(authority) = self.platform_authority.as_ref() {
            push_wrapped(
                &mut lines,
                &format!(
                    "authority: {} {:?} generation {}",
                    authority.platform(),
                    authority.state(),
                    authority.generation()
                ),
                width,
            );
        }
        let session = if self.active_session_title.is_empty() {
            self.active_session_id
                .map(|id| id.to_string())
                .unwrap_or_else(|| "none".into())
        } else {
            self.active_session_title.clone()
        };
        push_wrapped(&mut lines, &format!("session: {session}"), width);
        push_wrapped(&mut lines, &format!("status: {}", self.status), width);
        lines.push(String::new());

        for entry in &self.timeline {
            let prefix = match entry.kind {
                TimelineKind::User => "you",
                TimelineKind::Assistant => "argentum",
                TimelineKind::Tool => "tool",
                TimelineKind::Approval => "approval",
                TimelineKind::System => "info",
            };
            push_wrapped(
                &mut lines,
                &format!("{prefix}: {}", bounded_text(&entry.text)),
                width,
            );
            if !entry.detail.is_empty() {
                push_wrapped(
                    &mut lines,
                    &format!("  {}", bounded_text(&entry.detail)),
                    width,
                );
            }
            if !entry.state.is_empty() && entry.kind == TimelineKind::Tool {
                push_wrapped(&mut lines, &format!("  state: {}", entry.state), width);
            }
        }

        let (retained, bytes) = self.followups.totals();
        if retained > 0 {
            push_wrapped(&mut lines, &format!("queue: {} pending, {retained}/{MAX_QUEUE_ENTRIES} retained, {bytes}/{MAX_QUEUE_BYTES} bytes; memory only. /queue inspect", self.followups.pending_count()), width);
            if let Some(reason) = self
                .submission_target()
                .as_ref()
                .and_then(|target| self.followups.pause_reason(target))
            {
                push_wrapped(
                    &mut lines,
                    &format!("queue paused: {}", bounded_text(reason)),
                    width,
                );
            }
        }
        if let Some(request) = &self.pending_approval {
            push_wrapped(
                &mut lines,
                &format!(
                    "Approval required: {} on {}. Reply y or n.",
                    request.action, request.target
                ),
                width,
            );
        }

        lines
    }

    pub fn render(&self, width: u16) -> String {
        if let Some(rows) = self.overlay_rows(usize::from(width.max(1)), 24) {
            return rows.join("\n");
        }
        let mut lines = self.body_lines(width);
        let width = usize::from(width.max(1));
        lines.push(String::new());
        let mut composer_lines = self.composer.split('\n');
        if let Some(first) = composer_lines.next() {
            push_wrapped(&mut lines, &format!("> {first}"), width);
        }
        for line in composer_lines {
            push_wrapped(&mut lines, &format!("  {line}"), width);
        }
        push_wrapped(
            &mut lines,
            "Enter submit  Shift+Enter newline  Tab complete  Ctrl+C interrupt",
            width,
        );
        if self.interrupt_armed {
            push_wrapped(&mut lines, "Press Ctrl+C again to exit.", width);
        }
        if self.quit_armed {
            push_wrapped(
                &mut lines,
                "Quit confirmation pending: repeat /quit, or /queue continue to keep working.",
                width,
            );
        }

        let mut output = lines.join("\n");
        output.push('\n');
        output
    }

    fn insert_text(&mut self, text: &str) {
        let available = MAX_COMPOSER_BYTES.saturating_sub(self.composer.len());
        if text.len() > available {
            self.status = "Input exceeds 128 KiB; insertion rejected, draft unchanged".into();
            return;
        }
        self.composer.insert_str(self.composer_cursor, text);
        self.composer_cursor += text.len();
        self.history_index = None;
        self.completion_index = 0;
    }

    fn previous_cursor(&self) -> usize {
        self.composer[..self.composer_cursor]
            .grapheme_indices(true)
            .next_back()
            .map_or(0, |(i, _)| i)
    }

    fn next_cursor(&self) -> usize {
        self.composer[self.composer_cursor..]
            .graphemes(true)
            .next()
            .map_or(self.composer_cursor, |g| self.composer_cursor + g.len())
    }

    fn submit_composer(&mut self) -> TuiAction {
        let prompt = self.composer.clone();
        if prompt.trim().is_empty() {
            self.status = "Enter a message or /help.".into();
            return TuiAction::Redraw;
        }
        if prompt.starts_with('/') {
            let name = prompt.split_whitespace().next().unwrap_or_default();
            if let Some(c) = menu::COMMANDS.iter().find(|c| c.name == name) {
                if let Some(reason) = self.command_unavailable(c) {
                    self.status = reason.into();
                    return TuiAction::Redraw;
                }
                if prompt.split_whitespace().count() == 1 {
                    if let Some(view) = c.view {
                        self.clear_composer();
                        return TuiAction::Ui(menu::Request::Open(view));
                    }
                }
            }
        }
        if self.local_text_mode {
            if prompt.starts_with('/') {
                self.clear_composer();
                if prompt.trim() == "/quit" {
                    return TuiAction::Exit;
                }
                if prompt.trim() == "/stop" {
                    return self.handle_ctrl_c();
                }
                self.append_system("Local text-only chat: /quit exits, Ctrl+C cancels. Coding tools and task commands are unavailable. Messages are kept only in this session.");
                return TuiAction::Redraw;
            }
            if self.settings_busy || self.coding_preflight_issue.is_some() {
                self.status = format!(
                    "Draft retained: {}",
                    self.coding_preflight_issue
                        .as_deref()
                        .unwrap_or("wait for settings to finish")
                );
                return TuiAction::Redraw;
            }
            if self.active_run_id.is_some() || prompt.len() > self.local_text_limit {
                self.status = format!("Draft retained: wait or Ctrl+C to cancel. Local text allowance is {} UTF-8 bytes after host context.",self.local_text_limit);
                return TuiAction::Redraw;
            }
            self.clear_composer();
            self.record_user_prompt(&prompt);
            return TuiAction::LocalText(prompt);
        }
        if let Some(request) = self.pending_approval.clone() {
            let approval = match prompt.trim().to_ascii_lowercase().as_str() {
                "y" | "yes" => Some(TuiAction::Dispatch(AppCommand::ApproveTool {
                    approval_id: request.id,
                    scope: ApprovalScope::Once,
                })),
                "n" | "no" => Some(TuiAction::Dispatch(AppCommand::RejectTool {
                    approval_id: request.id,
                })),
                _ => None,
            };
            if let Some(action) = approval {
                self.clear_composer();
                return action;
            }
        }

        if prompt
            .split_whitespace()
            .next()
            .is_some_and(|command| command.eq_ignore_ascii_case("/steer"))
        {
            let text = prompt
                .split_once(char::is_whitespace)
                .map_or("", |(_, text)| text)
                .trim()
                .to_owned();
            return self.submit_steering(text);
        }
        if prompt.starts_with('/') {
            self.clear_composer();
            return self.handle_slash_command(&prompt);
        }
        self.submit_prompt(prompt)
    }

    fn submit_prompt(&mut self, prompt: String) -> TuiAction {
        self.submit_scoped_prompt(prompt, None)
    }

    fn submit_steering(&mut self, text: String) -> TuiAction {
        if self.local_text_mode {
            self.status = "Steering is unavailable in local text-only chat; draft retained.".into();
            return TuiAction::Redraw;
        }
        if text.trim().is_empty() {
            self.status = "Steering text is required; draft retained.".into();
            return TuiAction::Redraw;
        }
        let Some(run_id) = self.active_run_id else {
            self.status = "No active ordinary run is eligible for steering; draft retained.".into();
            return TuiAction::Redraw;
        };
        let Some(target) = self.submission_target() else {
            self.status = "Workspace/session identity is not loaded; draft retained.".into();
            return TuiAction::Redraw;
        };
        TuiAction::Steer {
            submission: argentum_runtime::SteeringSubmission {
                id: RunId::new_v4(),
                run_id,
                target,
                text,
            },
            draft: self.composer.clone(),
        }
    }

    fn submit_scoped_prompt(
        &mut self,
        prompt: String,
        goal_id: Option<argentum_domain::GoalId>,
    ) -> TuiAction {
        if self.settings_busy {
            self.status = "Settings are being applied; draft retained.".into();
            return TuiAction::Redraw;
        }
        if goal_id.is_none() && self.coding_preflight_issue.is_some() {
            let issue = self
                .coding_preflight_issue
                .as_deref()
                .unwrap_or("the selected provider does not support the coding workflow");
            self.status = format!(
                "Coding workflow unavailable: {issue}. Draft retained; use /setup to choose a supported provider."
            );
            return TuiAction::Redraw;
        }
        if self.manual_compaction.is_some() {
            self.status = "Manual compaction is active; draft retained. Wait or use /stop.".into();
            return TuiAction::Redraw;
        }
        let Some(target) = self.submission_target() else {
            self.status = "Workspace/session identity is not loaded; draft retained".into();
            return TuiAction::Redraw;
        };
        let queued = self.followups.enqueue_with_skill(
            target.clone(),
            prompt.clone(),
            goal_id,
            self.next_skill.clone(),
        );
        match queued {
            Ok(id) => {
                self.next_skill = None;
                self.clear_composer();
                self.record_user_prompt(&prompt);
                if self
                    .active_run_id
                    .is_some_and(|run| self.followups.target_for_run(run).is_none())
                {
                    self.followups.pause(&target, "Waiting behind an untracked run; use /queue continue after checking its settlement");
                }
                self.status = format!(
                    "Accepted {id}; {} pending (memory only)",
                    self.followups.pending_count()
                );
                self.take_ready_submission()
                    .map_or(TuiAction::Redraw, |submission| {
                        TuiAction::Submit(Box::new(submission))
                    })
            }
            Err(reason) => {
                self.status = reason.into();
                self.append_system(reason);
                TuiAction::Redraw
            }
        }
    }

    fn handle_slash_command(&mut self, prompt: &str) -> TuiAction {
        let mut parts = prompt.split_whitespace();
        let command = parts.next().unwrap_or_default().to_ascii_lowercase();
        if prompt.split_whitespace().count() == 1 {
            if let Some(view) = menu::COMMANDS
                .iter()
                .find(|c| c.name == command)
                .and_then(|c| c.view)
            {
                return TuiAction::Ui(menu::Request::Open(view));
            }
        }
        match command.as_str() {
            "/steer" => {
                let text = prompt
                    .split_once(char::is_whitespace)
                    .map_or("", |(_, text)| text)
                    .trim()
                    .to_owned();
                self.submit_steering(text)
            }
            "/goal-run" => {
                let mut arguments = prompt.splitn(3, char::is_whitespace);
                arguments.next();
                let id = arguments
                    .next()
                    .and_then(|value| value.parse::<argentum_domain::GoalId>().ok());
                let task = arguments.next().unwrap_or_default().trim();
                if let Some(id) = id.filter(|_| !task.is_empty()) {
                    self.submit_scoped_prompt(task.to_owned(), Some(id))
                } else {
                    self.append_system("Usage: /goal-run GOAL_ID TASK. Association applies only to this queue entry.");
                    TuiAction::Redraw
                }
            }
            "/help" => {
                self.append_system(
                    "Commands: /help /setup /new /fork /sessions /resume SESSION_ID /queue [continue|discard] /steer TEXT /stop /retry /quit",
                );
                TuiAction::Redraw
            }
            "/setup" => {
                self.append_system("Run 'argentum setup' to review or change onboarding.");
                TuiAction::Redraw
            }
            "/new" => {
                if self.active_run_id.is_some()
                    || self
                        .submission_target()
                        .as_ref()
                        .is_some_and(|target| self.followups.has_in_flight(target))
                {
                    self.status = "Wait for the current run before switching sessions".into();
                    TuiAction::Redraw
                } else {
                    TuiAction::Dispatch(AppCommand::NewSession)
                }
            }
            "/sessions" => {
                if self.sessions.is_empty() {
                    self.append_system("No sessions are available.");
                } else {
                    let sessions = self
                        .sessions
                        .iter()
                        .map(|session| {
                            let marker = if Some(session.id) == self.active_session_id {
                                "*"
                            } else {
                                " "
                            };
                            format!("{marker} {}  {}", session.id, session.title)
                        })
                        .collect::<Vec<_>>()
                        .join("\n");
                    self.append_system(&sessions);
                }
                TuiAction::Redraw
            }
            "/resume" => {
                let Some(value) = parts.next() else {
                    self.append_system("Usage: /resume SESSION_ID");
                    return TuiAction::Redraw;
                };
                match value.parse() {
                    Ok(session_id) => TuiAction::Dispatch(AppCommand::SelectSession { session_id }),
                    Err(_) => {
                        self.append_system("Session ID is not valid.");
                        TuiAction::Redraw
                    }
                }
            }
            "/stop" => {
                if let Some(cancellation) = &self.manual_cancellation {
                    cancellation.cancel();
                }
                if let Some(target) = self.submission_target() {
                    self.followups.request_cancel(&target);
                }
                if let Some(run_id) = self.active_run_id.or(self.manual_compaction) {
                    if let Some(target) = self.submission_target() {
                        let _ = self.followups.take_cancel(&target);
                    }
                    TuiAction::Dispatch(AppCommand::CancelRun { run_id })
                } else {
                    self.status = if self
                        .submission_target()
                        .as_ref()
                        .is_some_and(|target| self.followups.has_in_flight(target))
                    {
                        "Cancellation requested for the current handoff; later items paused".into()
                    } else {
                        "No run is active; pending items remain paused".into()
                    };
                    TuiAction::Redraw
                }
            }
            "/retry" => {
                let Some(prompt) = self.history.last().cloned() else {
                    self.append_system("There is no previous prompt to retry.");
                    return TuiAction::Redraw;
                };
                self.submit_prompt(prompt)
            }
            "/queue" => {
                match parts.next() {
                    None | Some("inspect") => {
                        for line in self.followups.describe().lines() {
                            self.append_system(line);
                        }
                    }
                    Some("continue") => {
                        if let Some(target) = self.submission_target() {
                            match self.followups.continue_selected(&target) {
                                Ok(()) => {
                                    self.quit_armed = false;
                                    self.queue_continue_requested = true;
                                    self.status = "Selected queue continued; waiting for any current submission to settle".into();
                                }
                                Err(reason) => self.status = reason.into(),
                            }
                        }
                    }
                    Some("discard") => {
                        if let Some(target) = self.submission_target() {
                            let count = self.followups.discard_pending(&target);
                            self.status = format!("Discarded {count} pending prompts; admitted work was not cancelled");
                            self.append_system(&self.status.clone());
                        }
                    }
                    _ => self.status = "Usage: /queue [inspect|continue|discard]".into(),
                }
                TuiAction::Redraw
            }
            "/context" => {
                if parts.next().is_some() {
                    self.append_system("Usage: /context");
                    TuiAction::Redraw
                } else {
                    self.active_session_id
                        .map_or(TuiAction::Redraw, TuiAction::InspectContext)
                }
            }
            "/compact" => {
                let argument = parts.next();
                if !matches!(argument, None | Some("deactivate")) || parts.next().is_some() {
                    self.append_system("Usage: /compact [deactivate]");
                    return TuiAction::Redraw;
                }
                if self.active_run_id.is_some()
                    || self.manual_compaction.is_some()
                    || self.pending_approval.is_some()
                    || self
                        .submission_target()
                        .as_ref()
                        .is_some_and(|target| self.followups.has_in_flight(target))
                {
                    self.append_system("Session is busy; compaction did not start.");
                    return TuiAction::Redraw;
                }
                let Some(session) = self.active_session_id else {
                    return TuiAction::Redraw;
                };
                let operation = RunId::new_v4();
                self.manual_compaction = Some(operation);
                self.manual_cancellation = Some(tokio_util::sync::CancellationToken::new());
                self.append_system("Compaction is selected-model work and may consume usage. Tools are disabled; original evidence is retained.");
                TuiAction::CompactContext {
                    session,
                    operation,
                    deactivate: argument.is_some(),
                }
            }
            "/quit" | "/exit" => {
                if self.followups.pending_count() > 0 && !self.quit_armed {
                    self.quit_armed = true;
                    self.followups.pause_all(
                        "Quit requested; repeat /quit to discard pending work, or /queue continue",
                    );
                    self.status = format!(
                        "{} pending prompts are not persisted. Repeat /quit to discard and exit",
                        self.followups.pending_count()
                    );
                    TuiAction::Redraw
                } else {
                    self.begin_shutdown();
                    TuiAction::Exit
                }
            }
            command if menu::COMMANDS.iter().any(|c| c.name == command) => {
                self.append_system(&format!(
                    "Use {command} without arguments, or open /help for its supported syntax."
                ));
                TuiAction::Redraw
            }
            _ => {
                self.append_system(&format!("Unknown command: {command}"));
                TuiAction::Redraw
            }
        }
    }

    fn handle_ctrl_c(&mut self) -> TuiAction {
        if let Some(cancellation) = &self.manual_cancellation {
            cancellation.cancel();
        }
        if self.interrupt_armed {
            self.begin_shutdown();
            return TuiAction::Exit;
        }
        self.interrupt_armed = true;
        if let Some(target) = self.submission_target() {
            self.followups.request_cancel(&target);
        }
        if let Some(run_id) = self.active_run_id.or(self.manual_compaction) {
            if let Some(target) = self.submission_target() {
                let _ = self.followups.take_cancel(&target);
            }
            self.status = "Interrupt requested".into();
            TuiAction::Dispatch(AppCommand::CancelRun { run_id })
        } else if self
            .submission_target()
            .as_ref()
            .is_some_and(|target| self.followups.has_in_flight(target))
        {
            self.status =
                "Cancellation requested for the current handoff; later items paused".into();
            TuiAction::Redraw
        } else {
            self.clear_composer();
            self.status = "Input cleared. Press Ctrl+C again to exit.".into();
            TuiAction::Redraw
        }
    }

    fn history_previous(&mut self) {
        let Some(last) = self.history.len().checked_sub(1) else {
            return;
        };
        let index = self
            .history_index
            .map_or(last, |index| index.saturating_sub(1));
        self.history_index = Some(index);
        self.composer = self.history[index].clone();
        self.composer_cursor = self.composer.len();
    }

    fn history_next(&mut self) {
        let Some(index) = self.history_index else {
            return;
        };
        if index + 1 >= self.history.len() {
            self.history_index = None;
            self.composer.clear();
        } else {
            self.history_index = Some(index + 1);
            self.composer = self.history[index + 1].clone();
        }
        self.composer_cursor = self.composer.len();
    }

    fn complete_slash_command(&mut self) {
        if !self.composer.starts_with('/') || self.composer.contains(char::is_whitespace) {
            return;
        }
        let candidates = menu::COMMANDS
            .iter()
            .map(|c| c.name)
            .filter(|command| command.starts_with(&self.composer))
            .collect::<Vec<_>>();
        if candidates.is_empty() {
            return;
        }
        self.composer = candidates[self.completion_index % candidates.len()].into();
        self.composer_cursor = self.composer.len();
        self.completion_index = self.completion_index.saturating_add(1);
    }

    fn record_user_prompt(&mut self, prompt: &str) {
        self.transcript_changed();
        if self.history.last().map_or(true, |last| last != prompt) {
            self.history.push(prompt.to_owned());
            if self.history.len() > MAX_HISTORY_ENTRIES {
                self.history.remove(0);
            }
            while self.history.iter().map(String::len).sum::<usize>() > 64 * 1024 {
                self.history.remove(0);
            }
        }
        self.timeline.push(TimelineEntry {
            view_id: RunId::new_v4(),
            message_id: None,
            kind: TimelineKind::User,
            run_id: None,
            tool_call_id: None,
            text: bounded_text(prompt),
            detail: String::new(),
            state: "submitted".into(),
        });
        self.truncate_timeline();
    }

    fn append_assistant_delta(&mut self, run_id: RunId, text: &str) {
        self.transcript_changed();
        if self.viewport.anchor.is_some()
            && self.viewport.newer
            && !self.timeline.iter().any(|e| e.run_id == Some(run_id))
            && !self.local_text_mode
        {
            return;
        }
        if let Some(entry) = self
            .timeline
            .iter_mut()
            .rev()
            .find(|entry| entry.run_id == Some(run_id) && entry.kind == TimelineKind::Assistant)
        {
            if entry
                .text
                .chars()
                .count()
                .saturating_add(text.chars().count())
                > MAX_RENDERED_TEXT_CHARS
            {
                entry.detail =
                    "Display preview is limited; this is not a model output-token limit.".into();
            }
            append_bounded(&mut entry.text, text);
        } else {
            self.timeline.push(TimelineEntry {
                view_id: RunId::new_v4(),
                message_id: None,
                kind: TimelineKind::Assistant,
                run_id: Some(run_id),
                tool_call_id: None,
                text: bounded_text(text),
                detail: String::new(),
                state: "streaming".into(),
            });
            self.truncate_timeline();
        }
    }

    fn append_tool(&mut self, trace: argentum_domain::ToolTrace, state: &str) {
        self.transcript_changed();
        self.timeline.push(TimelineEntry {
            view_id: RunId::new_v4(),
            message_id: None,
            kind: TimelineKind::Tool,
            run_id: Some(trace.run_id),
            tool_call_id: Some(trace.id),
            text: bounded_text(&format!("{}: {}", trace.tool_id, trace.summary)),
            detail: String::new(),
            state: state.into(),
        });
        self.truncate_timeline();
    }

    fn finish_tool(&mut self, trace: argentum_domain::ToolTrace) {
        self.transcript_changed();
        if let Some(entry) = self
            .timeline
            .iter_mut()
            .rev()
            .find(|entry| entry.tool_call_id == Some(trace.id))
        {
            entry.text = bounded_text(&format!("{}: {}", trace.tool_id, trace.summary));
            entry.state = tool_result_label(trace.result).into();
            entry.detail = trace
                .duration_ms
                .map_or_else(String::new, |duration| format!("duration: {duration} ms"));
        } else {
            let state = tool_result_label(trace.result.clone());
            self.append_tool(trace, state);
        }
    }

    fn append_system(&mut self, text: &str) {
        self.transcript_changed();
        self.timeline.push(TimelineEntry {
            view_id: RunId::new_v4(),
            message_id: None,
            kind: TimelineKind::System,
            run_id: None,
            tool_call_id: None,
            text: bounded_text(text),
            detail: String::new(),
            state: String::new(),
        });
        self.truncate_timeline();
    }

    fn apply_workspace(&mut self, snapshot: WorkspaceSnapshot) {
        self.bind_editor(
            snapshot
                .active_session_id
                .map(|session_id| SubmissionTarget {
                    project_id: snapshot.project.id,
                    session_id,
                    workspace: snapshot.project.workspace_root.clone(),
                }),
        );
        let changed = self.project_id.is_some()
            && (self.project_id != Some(snapshot.project.id)
                || self.active_session_id != snapshot.active_session_id
                || self.workspace != snapshot.project.workspace_root);
        if changed {
            self.timeline.clear();
            self.viewport = transcript::Viewport::default();
            self.goal_accounting_summary = None;
            self.followups
                .pause_all("Session/workspace changed; use /queue continue after returning");
        }
        self.project_id = Some(snapshot.project.id);
        self.project_name = bounded_text(&snapshot.project.name);
        self.workspace = snapshot.project.workspace_root;
        self.sessions = snapshot.sessions;
        self.active_session_id = snapshot.active_session_id;
        self.pending_approval = self
            .active_session_id
            .and_then(|session| self.session_approvals.get(&session).cloned());
        self.active_run_id = self
            .active_session_id
            .and_then(|session| self.observed_runs.get(&session).copied())
            .or_else(|| {
                self.submission_target()
                    .as_ref()
                    .and_then(|target| self.followups.active_run(target))
            });
        self.active_session_title = self
            .active_session_id
            .and_then(|id| self.sessions.iter().find(|session| session.id == id))
            .map(|session| bounded_text(&session.title))
            .unwrap_or_default();
    }

    fn apply_provider_profiles(&mut self, profiles: &[ProviderProfile]) {
        if let Some(profile) = profiles.iter().find(|profile| {
            if self.local_text_mode {
                profile.id == "llama-cpp"
            } else {
                profile.selected
            }
        }) {
            self.provider_label = if self.local_text_mode {
                "llama.cpp (text-only)".into()
            } else {
                bounded_text(&profile.label)
            };
            self.model = bounded_text(&profile.model);
        }
    }

    fn truncate_timeline(&mut self) {
        while self.timeline.len() > MAX_TIMELINE_ENTRIES
            || self
                .timeline
                .iter()
                .map(|e| e.text.len() + e.detail.len())
                .sum::<usize>()
                > transcript::CACHE_BYTES
        {
            if self.viewport.drop_newer {
                self.timeline.pop();
                self.viewport.newer = true;
            } else {
                self.timeline.remove(0);
                self.viewport.older = true;
            }
            self.viewport.notice =
                "Bounded history cache; older/newer pages load on demand.".into();
        }
        self.viewport.revision = self.viewport.revision.wrapping_add(1);
    }

    fn reset_interrupt(&mut self) {
        self.interrupt_armed = false;
    }

    fn run_is_current(&self, run: RunId) -> bool {
        self.active_session_id.is_none()
            || self.active_run_id == Some(run)
            || self
                .followups
                .target_for_run(run)
                .is_some_and(|target| Some(target.session_id) == self.active_session_id)
            || self
                .active_session_id
                .and_then(|id| self.observed_runs.get(&id).copied())
                == Some(run)
    }

    fn bind_editor(&mut self, target: Option<SubmissionTarget>) {
        if self.editor_target == target {
            return;
        }
        self.next_skill = None;
        if let Some(old) = self.editor_target.take() {
            self.draft_order.retain(|key| *key != old);
            self.draft_order.push_back(old.clone());
            self.drafts.insert(
                old,
                (
                    std::mem::take(&mut self.composer),
                    self.composer_cursor,
                    std::mem::take(&mut self.history),
                ),
            );
        }
        if let Some(key) = &target {
            self.draft_order.retain(|saved| saved != key);
        }
        let (draft, cursor, history) = target
            .as_ref()
            .and_then(|id| self.drafts.remove(id))
            .unwrap_or_default();
        self.composer = draft;
        self.composer_cursor = cursor;
        self.history = history;
        self.history_index = None;
        self.completion_index = 0;
        self.editor_target = target;
        self.timeline.clear();
        self.viewport = transcript::Viewport::default();
        self.overlay = None;
        self.access_review = None;
        while self.drafts.len() > 8 {
            let key = self.draft_order.pop_front().unwrap();
            self.drafts.remove(&key);
            self.status =
                "Oldest cached draft evicted; drafts are memory-only (8 sessions).".into();
        }
    }

    fn remember_stream(&mut self, run: RunId, text: &str) {
        let session = self
            .followups
            .target_for_run(run)
            .map(|t| t.session_id)
            .or_else(|| {
                self.observed_runs
                    .iter()
                    .find_map(|(s, r)| (*r == run).then_some(*s))
            })
            .or_else(|| {
                (self.active_run_id == Some(run))
                    .then_some(self.active_session_id)
                    .flatten()
            });
        let Some(session) = session else {
            return;
        };
        let item = self.streams.entry(run).or_insert_with(|| {
            (
                session,
                TimelineEntry {
                    view_id: run,
                    message_id: None,
                    kind: TimelineKind::Assistant,
                    run_id: Some(run),
                    tool_call_id: None,
                    text: String::new(),
                    detail: String::new(),
                    state: "streaming".into(),
                },
            )
        });
        append_bounded(&mut item.1.text, text);
        while self.streams.len() > 8 {
            let id = *self.streams.keys().next().unwrap();
            self.streams.remove(&id);
        }
    }
    fn merge_current_streams(&mut self) {
        for (_, entry) in self
            .streams
            .values()
            .filter(|(s, _)| Some(*s) == self.active_session_id)
        {
            if !self.timeline.iter().any(|e| e.key() == entry.key()) {
                self.timeline.push(entry.clone());
            }
        }
    }
}

fn command_kind(command: &AppCommand) -> CommandKind {
    match command {
        AppCommand::SubmitTask { .. } => CommandKind::SubmitTask,
        AppCommand::CancelRun { .. } => CommandKind::CancelRun,
        AppCommand::NewSession | AppCommand::SelectSession { .. } => CommandKind::Session,
        _ => CommandKind::Other,
    }
}

impl TuiState {
    fn progress_active(&self) -> bool {
        (self.active_run_id.is_some() || self.settings_busy)
            && self.pending_approval.is_none()
            && self.shutdown_notice.is_none()
    }

    fn display_status(&self) -> String {
        if self.progress_active() {
            let marker = ['|', '/', '-', '\\'][usize::from(self.progress_tick % 4)];
            format!("[{marker}] {}", self.status)
        } else {
            self.status.clone()
        }
    }
}

fn tool_result_label(result: ToolResultState) -> &'static str {
    match result {
        ToolResultState::Running => "running",
        ToolResultState::Succeeded => "succeeded",
        ToolResultState::Failed => "failed",
        ToolResultState::Cancelled => "cancelled",
        ToolResultState::NotExecuted => "not executed",
    }
}

fn bounded_text(text: &str) -> String {
    text.chars().take(MAX_RENDERED_TEXT_CHARS).collect()
}

fn append_bounded(target: &mut String, text: &str) {
    let available = MAX_RENDERED_TEXT_CHARS.saturating_sub(target.chars().count());
    target.extend(text.chars().take(available));
}

fn push_wrapped(lines: &mut Vec<String>, text: &str, width: usize) {
    lines.extend(screen::wrap(text, width));
}

pub async fn run_interactive(mut host: CommandHost, options: TuiOptions) -> Result<(), TuiError> {
    if options.force_non_interactive || !io::stdin().is_terminal() || !io::stdout().is_terminal() {
        return run_line_session(
            host,
            tokio::io::BufReader::new(tokio::io::stdin()),
            tokio::io::stdout(),
            options,
        )
        .await;
    }
    let session_result = run_tty_session(&mut host, options).await;
    finish_session_shutdown(session_result, host.shutdown().await)
}

pub async fn run_line_session<R, W>(
    mut host: CommandHost,
    reader: R,
    writer: W,
    options: TuiOptions,
) -> Result<(), TuiError>
where
    R: AsyncBufRead + Unpin,
    W: AsyncWrite + Unpin,
{
    // This fallback dispatches through the unprivileged client and has no live
    // owner approval loop. A pipe is not an approval responder.
    host.disconnect_approval_responder();
    let session_result = run_line_session_inner(&host, reader, writer, options).await;
    finish_session_shutdown(session_result, host.shutdown().await)
}

async fn run_line_session_inner<R, W>(
    host: &CommandHost,
    mut reader: R,
    mut writer: W,
    options: TuiOptions,
) -> Result<(), TuiError>
where
    R: AsyncBufRead + Unpin,
    W: AsyncWrite + Unpin,
{
    let mut state = TuiState::new(host.workspace_root().to_path_buf(), options.no_color);
    let mut events = host.subscribe();
    host.publish_initial_state()?;
    drain_events(&mut events, &mut state);
    write_line_render(&mut writer, &state).await?;

    let client = host.client();
    let mut line = String::new();
    loop {
        line.clear();
        if reader.read_line(&mut line).await? == 0 {
            break;
        }
        state.set_coding_preflight_issue(host.selected_provider_coding_issue()?);
        let mut input = line.trim_end_matches(['\r', '\n']).to_owned();
        let continuation = input.ends_with('\\');
        if continuation {
            input.pop();
        }

        if input.is_empty() && !continuation && !state.composer().is_empty() {
            handle_line_action(
                &mut state,
                &client,
                &mut events,
                &mut writer,
                InputEvent::Enter,
            )
            .await?;
        } else {
            handle_line_action(
                &mut state,
                &client,
                &mut events,
                &mut writer,
                InputEvent::Text(input),
            )
            .await?;
            if continuation {
                handle_line_action(
                    &mut state,
                    &client,
                    &mut events,
                    &mut writer,
                    InputEvent::NewLine,
                )
                .await?;
            } else {
                handle_line_action(
                    &mut state,
                    &client,
                    &mut events,
                    &mut writer,
                    InputEvent::Enter,
                )
                .await?;
            }
        }

        if state.status == "Exiting" {
            break;
        }
    }
    Ok(())
}

async fn handle_line_action<W>(
    state: &mut TuiState,
    client: &InProcessClient,
    events: &mut tokio::sync::broadcast::Receiver<AppEvent>,
    writer: &mut W,
    input: InputEvent,
) -> Result<(), TuiError>
where
    W: AsyncWrite + Unpin,
{
    let action = state.apply_input(input);
    match action {
        TuiAction::Ui(_) => state.append_system("Interactive views are available in a native terminal. Use the corresponding argentum inspection command here."),
        TuiAction::HistoryPage { .. } => state.append_system("History paging is available in the interactive terminal."),
        TuiAction::InspectContext(session_id) => {
            append_context_preview(state, client.context_preview(session_id));
        }
        TuiAction::CompactContext {
            session,
            operation,
            deactivate,
        } => {
            let cancellation = state.manual_cancellation.clone().unwrap_or_default();
            let result = client
                .compact_context_with_cancellation(session, operation, deactivate, cancellation)
                .await;
            state.manual_compaction = None;
            state.manual_cancellation = None;
            match result {
                Ok(result) => state.append_system(
                    &serde_json::to_string_pretty(&result)
                        .unwrap_or_else(|_| "Compaction result unavailable".into()),
                ),
                Err(error) => state.append_system(&format!("Compaction failed: {error}")),
            }
        }
        TuiAction::Submit(submission) => {
            let completion = dispatch_bound_and_observe(client, events, state, *submission).await;
            state.submission_finished(completion);
        }
        TuiAction::Steer { submission, draft } => match client.steer_active_run(submission) {
            Ok(receipt) => {
                state.pending_steering.insert(receipt.id, receipt.run_id);
                if state.composer == draft {
                    state.clear_composer();
                }
                state.status = format!(
                    "Steering {} accepted at position {}; pending the next safe request boundary",
                    receipt.id, receipt.position
                );
            }
            Err(error) => {
                state.status = format!("Steering rejected: {error}");
                state.append_system(&format!("Steering rejected: {error}"));
            }
        },
        TuiAction::Dispatch(command) => {
            let kind = command_kind(&command);
            state.command_started(&command);
            let result = dispatch_and_observe(client, events, state, command).await;
            state.command_finished(kind, &result);
        }
        TuiAction::Exit => state.status = "Exiting".into(),
        TuiAction::LocalText(_) | TuiAction::None | TuiAction::Redraw => {}
    }
    drain_events(events, state);
    while let Some(submission) = state.take_ready_submission() {
        let completion = dispatch_bound_and_observe(client, events, state, submission).await;
        state.submission_finished(completion);
        drain_events(events, state);
    }
    writer
        .write_all(state.render(DEFAULT_TERMINAL_WIDTH).as_bytes())
        .await?;
    writer.flush().await?;
    Ok(())
}

async fn dispatch_and_observe(
    client: &InProcessClient,
    events: &mut tokio::sync::broadcast::Receiver<AppEvent>,
    state: &mut TuiState,
    command: AppCommand,
) -> Result<(), String> {
    let dispatch = client.dispatch(command);
    tokio::pin!(dispatch);
    loop {
        tokio::select! {
            result = &mut dispatch => {
                return result.map_err(|error| error.to_string());
            }
            event = events.recv() => {
                match event {
                    Ok(event) => state.apply_event(event),
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(skipped)) => {
                        state.append_system(&format!("Skipped {skipped} events."));
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => {}
                }
            }
        }
    }
}

async fn dispatch_bound_and_observe(
    client: &InProcessClient,
    events: &mut tokio::sync::broadcast::Receiver<AppEvent>,
    state: &mut TuiState,
    submission: TaskSubmission,
) -> TaskSubmissionResult {
    if state.followups.goal_for_run(submission.run_id).is_some() {
        // The scripted transport has no trusted local goal-control handle.
        // Never silently downgrade an explicitly associated entry.
        return TaskSubmissionResult {
            id: submission.id,
            run_id: submission.run_id,
            admitted: false,
            terminal_lifecycle: None,
            result: Err(argentum_runtime::RuntimeError::SubmissionTargetChanged),
        };
    }
    let dispatch = client.submit_bound_task(submission);
    tokio::pin!(dispatch);
    loop {
        tokio::select! {
            result = &mut dispatch => return result,
            event = events.recv() => match event {
                Ok(event) => state.apply_event(event),
                Err(error) => state.followups.uncertain(&format!("Host event stream incomplete: {error}; queue paused")),
            }
        }
    }
}

fn finish_session_shutdown(
    session_result: Result<(), TuiError>,
    shutdown_result: Result<(), HostError>,
) -> Result<(), TuiError> {
    match (session_result, shutdown_result) {
        (Ok(()), Ok(())) => Ok(()),
        (Err(session_error), Ok(())) => Err(session_error),
        (Ok(()), Err(shutdown_error)) => Err(shutdown_error.into()),
        (Err(session_error), Err(shutdown_error)) => Err(TuiError::SessionAndShutdown {
            session: session_error.to_string(),
            shutdown: shutdown_error.to_string(),
        }),
    }
}

fn drain_events(events: &mut tokio::sync::broadcast::Receiver<AppEvent>, state: &mut TuiState) {
    while let Ok(event) = events.try_recv() {
        state.apply_event(event);
    }
}

async fn write_line_render<W>(writer: &mut W, state: &TuiState) -> io::Result<()>
where
    W: AsyncWrite + Unpin,
{
    writer
        .write_all(state.render(DEFAULT_TERMINAL_WIDTH).as_bytes())
        .await?;
    writer.flush().await
}

async fn run_tty_session(host: &mut CommandHost, options: TuiOptions) -> Result<(), TuiError> {
    let mut state = TuiState::new(host.workspace_root().to_path_buf(), options.no_color);
    let mut events = host.subscribe();
    host.publish_initial_state()?;
    drain_events(&mut events, &mut state);
    let local = host.local_handle();
    state.set_coding_preflight_issue(local.selected_provider_coding_issue()?);
    if let Some(session) = state.active_session_id {
        if let Ok(page) = local.history_page(session, None, false) {
            state.apply_history_page(page, false, true);
        }
    }

    let mut terminal_guard = TerminalGuard::enter()?;
    let mut renderer = TtyRenderer::stdout();
    renderer.render(&state)?;
    let (mut input_receiver, stop_input, input_thread) = spawn_input_reader(None);

    let mut command_tasks = JoinSet::new();

    let session_result = run_tty_event_loop(
        &mut state,
        &mut events,
        &mut input_receiver,
        &local,
        &mut command_tasks,
        &mut |state| renderer.render(state).map_err(TuiError::from),
    )
    .await;

    state.begin_shutdown();

    stop_input.store(true, Ordering::Release);
    let _ = input_thread.join();
    let restore_result = terminal_guard.restore().map_err(TuiError::from);
    if let Some(notice) = &state.shutdown_notice {
        let _ = writeln!(io::stderr(), "{notice}");
    }

    let shutdown_request = host.request_shutdown().map_err(TuiError::from);
    let task_cleanup = drain_command_tasks(&mut command_tasks).await;
    let cleanup_result = combine_tty_results(shutdown_request, task_cleanup);
    combine_tty_results(
        combine_tty_results(session_result, restore_result),
        cleanup_result,
    )
}

async fn run_tty_event_loop<F>(
    state: &mut TuiState,
    events: &mut tokio::sync::broadcast::Receiver<AppEvent>,
    input_receiver: &mut mpsc::UnboundedReceiver<InputEvent>,
    local: &LocalCommandHandle,
    command_tasks: &mut JoinSet<CommandCompletion>,
    render: &mut F,
) -> Result<(), TuiError>
where
    F: FnMut(&TuiState) -> Result<(), TuiError>,
{
    let frame_period = tokio::time::Duration::from_millis(33);
    let mut progress_clock = tokio::time::interval(Duration::from_millis(250));
    progress_clock.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    let mut dirty = false;
    let mut next_frame = tokio::time::Instant::now() + frame_period;
    loop {
        if dirty && tokio::time::Instant::now() >= next_frame {
            review::revalidate(state, local);
            render(state)?;
            dirty = false;
            next_frame = tokio::time::Instant::now() + frame_period;
        }
        tokio::select! {
            _ = progress_clock.tick(), if state.progress_active() => {
                state.progress_tick = state.progress_tick.wrapping_add(1);
                dirty = true;
            }
            input = input_receiver.recv() => {
                let Some(input) = input else {
                    state.begin_shutdown();
                    render(state)?;
                    return Ok(());
                };
                if matches!(input,InputEvent::Enter) && state.overlay.is_none(){settings::refresh_preflight(state,local).await?;}
                let action = state.apply_input(input);
                let drives_queue = matches!(&action, TuiAction::Submit(_))
                    || matches!(&action, TuiAction::Dispatch(command) if !matches!(command, AppCommand::RequestRestore { .. } | AppCommand::ApproveTool { .. } | AppCommand::RejectTool { .. }))
                    || std::mem::take(&mut state.queue_continue_requested);
                let should_exit = dispatch_tui_action(
                    state,
                    local,
                    command_tasks,
                    action,
                );
                if should_exit {
                    state.begin_shutdown();
                    render(state)?;
                    return Ok(());
                }
                if drives_queue {dispatch_ready(state, local, command_tasks);}
                dirty = true;
            }
            event = events.recv() => {
                match event {
                    Ok(event) => {
                        state.apply_event(event);
                        if let Some(command) = review::take_command(state) {
                            dispatch_tui_action(
                                state,
                                local,
                                command_tasks,
                                TuiAction::Dispatch(command),
                            );
                        }
                        if review::take_refresh(state) {
                            review::schedule(state, local, command_tasks);
                        }
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(skipped)) => {
                        state.append_system(&format!("Skipped {skipped} events."));
                        state.followups.uncertain("Host events were lost; queue paused without retry");
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                        state.begin_shutdown();
                        render(state)?;
                        return Err(TuiError::CommandTask("Host event stream closed".into()));
                    }
                }
                dispatch_ready(state, local, command_tasks);
                dirty = true;
            }
            completed = command_tasks.join_next(), if !command_tasks.is_empty() => {
                match completed {
                    Some(Ok(completion)) => {
                        if let Some(result)=completion.settings {if let Some(next)=settings::finish(state,local,result).await{settings::dispatch(state,local,command_tasks,next);}dirty=true;continue;}
                        if let Some(review_result)=completion.review {review::finish(state,local,review_result);dirty=true;continue;}
                        let access_change=completion.kind==CommandKind::Access;
                        if let Some(detail) = &completion.compaction_detail { state.append_system(detail); }
                        if let Some(submission) = completion.submission { state.submission_finished(submission); }
                        else { state.command_finished(completion.kind, &completion.result); }
                        if access_change && completion.result.is_ok(){let _=menu::open_view(state,local,menu::View::Status);}
                        dispatch_ready(state, local, command_tasks);
                        dirty = true;
                    }
                    Some(Err(error)) => {
                        state.followups.uncertain("Command handoff is uncertain; queued instructions retained without retry");
                        state.append_system(&format!("Command task failed: {error}. Queue paused; no blind resubmission."));
                        dirty = true;
                    }
                    None => {}
                }
            }
            _ = tokio::time::sleep_until(next_frame), if dirty => {
                review::revalidate(state, local);
                render(state)?;
                dirty = false;
                next_frame = tokio::time::Instant::now() + frame_period;
            }
        }
    }
}

async fn drain_command_tasks(
    command_tasks: &mut JoinSet<CommandCompletion>,
) -> Result<(), TuiError> {
    let mut first_error = None;
    while let Some(result) = command_tasks.join_next().await {
        match result {
            Ok(completion) => {
                if let Err(error) = completion.result {
                    first_error.get_or_insert(error);
                }
            }
            Err(error) => {
                first_error.get_or_insert_with(|| error.to_string());
            }
        }
    }
    first_error.map_or(Ok(()), |error| Err(TuiError::CommandTask(error)))
}

fn combine_tty_results(
    session_result: Result<(), TuiError>,
    cleanup_result: Result<(), TuiError>,
) -> Result<(), TuiError> {
    match (session_result, cleanup_result) {
        (Ok(()), Ok(())) => Ok(()),
        (Err(error), Ok(())) | (Ok(()), Err(error)) => Err(error),
        (Err(session), Err(cleanup)) => Err(TuiError::SessionAndCommandCleanup {
            session: session.to_string(),
            cleanup: cleanup.to_string(),
        }),
    }
}

struct CommandCompletion {
    settings: Option<settings::Completion>,
    review: Option<review::Completion>,
    kind: CommandKind,
    result: Result<(), String>,
    submission: Option<TaskSubmissionResult>,
    compaction_detail: Option<String>,
}

fn dispatch_tui_action(
    state: &mut TuiState,
    local: &LocalCommandHandle,
    command_tasks: &mut JoinSet<CommandCompletion>,
    action: TuiAction,
) -> bool {
    match action {
        TuiAction::Ui(request) => {
            match request {
                menu::Request::Settings(action) => {
                    settings::dispatch(state, local, command_tasks, action)
                }
                menu::Request::Open(menu::View::Changes) => {
                    review::begin(state);
                    review::schedule(state, local, command_tasks);
                }
                menu::Request::Review => review::schedule(state, local, command_tasks),
                menu::Request::Open(view) => {
                    if let Err(error) = menu::open_view(state, local, view) {
                        state.info_overlay(view, error.to_string());
                    }
                    if view == menu::View::Model {
                        if let Ok(profiles) = local.provider_profiles() {
                            if let Some(p) = profiles.into_iter().find(|p| {
                                p.selected && p.kind != argentum_domain::ProviderKind::LocalLlamaCpp
                            }) {
                                settings::dispatch(
                                    state,
                                    local,
                                    command_tasks,
                                    settings::Action::Refresh(p.id),
                                );
                            }
                        }
                    }
                }
                menu::Request::ReviewAccess(id) => {
                    if let Err(error) = menu::review_access(state, local, &id) {
                        state.info_overlay(menu::View::Permissions, error.to_string());
                    }
                }
                menu::Request::CommitAccess => {
                    if let Some(review) = state.access_review.take() {
                        if review.review.busy
                            || state.settings_busy
                            || state.active_run_id.is_some()
                            || state.pending_approval.is_some()
                            || state.session_transition
                        {
                            state.info_overlay(
                                menu::View::AccessReview,
                                "Work or approval is active. Cancel and drain, then reopen access."
                                    .into(),
                            );
                            return false;
                        }
                        state.access_pending = true;
                        state.followups.pause_all("Access review completed; /queue continue is required before retained work resumes");
                        let local = local.clone();
                        command_tasks.spawn(async move {
                            CommandCompletion {
                                settings: None,
                                kind: CommandKind::Access,
                                review: None,
                                result: local
                                    .apply_access_review(review)
                                    .await
                                    .map_err(|e| e.to_string()),
                                submission: None,
                                compaction_detail: None,
                            }
                        });
                    }
                }
            }
            false
        }
        TuiAction::HistoryPage {
            cursor,
            newer,
            latest,
        } => {
            if let Some(session) = state.active_session_id {
                match local.history_page(session, cursor, newer) {
                    Ok(page) => state.apply_history_page(page, newer, latest),
                    Err(error) => {
                        state.viewport.notice = format!("History unavailable: {error}");
                        state.viewport.older = false;
                        state.viewport.newer = false;
                    }
                }
            }
            false
        }
        TuiAction::InspectContext(session_id) => {
            append_context_preview(state, local.context_preview(session_id));
            false
        }
        TuiAction::CompactContext {
            session,
            operation,
            deactivate,
        } => {
            let local = local.clone();
            let cancellation = state.manual_cancellation.clone().unwrap_or_default();
            command_tasks.spawn(async move {
                let outcome = local
                    .compact_context_with_cancellation(session, operation, deactivate, cancellation)
                    .await;
                let detail = outcome
                    .as_ref()
                    .ok()
                    .and_then(|result| serde_json::to_string_pretty(result).ok());
                let result = match outcome {
                    Ok(result)
                        if result.status
                            == argentum_domain::context_summary::CompactionStatus::Failed =>
                    {
                        Err(result.reason)
                    }
                    Ok(_) => Ok(()),
                    Err(error) => Err(error.to_string()),
                };
                CommandCompletion {
                    settings: None,
                    kind: CommandKind::Compact,
                    review: None,
                    result,
                    submission: None,
                    compaction_detail: detail,
                }
            });
            false
        }
        TuiAction::Submit(submission) => {
            let local = local.clone();
            let goal_id = state.followups.goal_for_run(submission.run_id);
            let submission = *submission;
            command_tasks.spawn(async move {
                let completion = if let Some(goal_id) = goal_id {
                    local.submit_goal_entry(submission, goal_id).await
                } else {
                    local.submit_bound_task(submission).await
                };
                let result = completion
                    .result
                    .as_ref()
                    .map(|_| ())
                    .map_err(|error| error.to_string());
                CommandCompletion {
                    settings: None,
                    kind: CommandKind::SubmitTask,
                    review: None,
                    result,
                    submission: Some(completion),
                    compaction_detail: None,
                }
            });
            false
        }
        TuiAction::Steer { submission, draft } => {
            match local.steer_active_run(submission) {
                Ok(receipt) => {
                    state.pending_steering.insert(receipt.id, receipt.run_id);
                    if state.composer == draft {
                        state.clear_composer();
                    }
                    state.status = format!(
                        "Steering {} accepted at position {}; pending the next safe request boundary",
                        receipt.id, receipt.position
                    );
                }
                Err(error) => {
                    state.status = format!("Steering rejected: {error}");
                    state.append_system(&format!("Steering rejected: {error}"));
                }
            }
            false
        }
        TuiAction::Dispatch(AppCommand::SubmitTask { prompt }) => {
            let action = state.submit_prompt(prompt);
            dispatch_tui_action(state, local, command_tasks, action)
        }
        TuiAction::Dispatch(AppCommand::SetSessionArchived {
            session_id,
            archived: true,
        }) => {
            let retained = state.followups.retained_count_for_session(session_id);
            if let Err(error) = local.set_retained_session_work(session_id, retained) {
                state.status = format!("Command failed: {}", bounded_text(&error.to_string()));
                state.append_system(&format!("Command failed: {error}"));
                return false;
            }
            if retained > 0 {
                let message = format!(
                    "Cannot archive session {session_id}: {retained} retained queue item(s); use /queue continue or /queue discard first"
                );
                state.status = message.clone();
                state.append_system(&message);
                return false;
            }
            let command = AppCommand::SetSessionArchived {
                session_id,
                archived: true,
            };
            let kind = command_kind(&command);
            state.command_started(&command);
            let local = local.clone();
            command_tasks.spawn(async move {
                let result = local
                    .dispatch(command)
                    .await
                    .map_err(|error| error.to_string());
                CommandCompletion {
                    settings: None,
                    kind,
                    result,
                    submission: None,
                    compaction_detail: None,
                    review: None,
                }
            });
            false
        }
        TuiAction::Dispatch(command) => {
            let kind = command_kind(&command);
            state.command_started(&command);
            let local = local.clone();
            command_tasks.spawn(async move {
                let result = local
                    .dispatch(command)
                    .await
                    .map_err(|error| error.to_string());
                CommandCompletion {
                    settings: None,
                    kind,
                    result,
                    submission: None,
                    compaction_detail: None,
                    review: None,
                }
            });
            false
        }
        TuiAction::Exit => {
            state.begin_shutdown();
            true
        }
        TuiAction::LocalText(_) | TuiAction::None | TuiAction::Redraw => false,
    }
}

fn append_context_preview(
    state: &mut TuiState,
    result: Result<argentum_runtime::working_context::ContextPreview, HostError>,
) {
    match result {
        Ok(preview) => match serde_json::to_string_pretty(&preview) {
            Ok(text) => state.append_system(&text),
            Err(_) => state.append_system("Context preview serialization failed."),
        },
        Err(error) => state.append_system(&format!("Context preview failed: {error}")),
    }
}

fn dispatch_ready(
    state: &mut TuiState,
    local: &LocalCommandHandle,
    commands: &mut JoinSet<CommandCompletion>,
) {
    if let Some(command) = state.take_ready_cancel() {
        dispatch_tui_action(state, local, commands, TuiAction::Dispatch(command));
    }
    if let Some(submission) = state.take_ready_submission() {
        dispatch_tui_action(
            state,
            local,
            commands,
            TuiAction::Submit(Box::new(submission)),
        );
    }
}

struct TerminalGuard {
    restored: bool,
    alternate_entered: bool,
    #[cfg(windows)]
    modes: windows_console::Modes,
}

impl TerminalGuard {
    fn enter() -> io::Result<Self> {
        #[cfg(windows)]
        let modes = windows_console::Modes::capture()?;
        terminal::enable_raw_mode()?;
        let mut guard = Self {
            restored: false,
            alternate_entered: false,
            #[cfg(windows)]
            modes,
        };
        #[cfg(windows)]
        guard.modes.enable_output()?;
        let mut stdout = io::stdout();
        execute!(stdout, EnterAlternateScreen)?;
        guard.alternate_entered = true;
        if let Err(error) = execute!(
            stdout,
            EnableBracketedPaste,
            event::EnableMouseCapture,
            Show
        ) {
            let _ = guard.restore();
            return Err(error);
        }
        #[cfg(windows)]
        guard.modes.enable_output()?;
        Ok(guard)
    }

    fn restore(&mut self) -> io::Result<()> {
        if self.restored {
            return Ok(());
        }
        let mut stdout = io::stdout();
        let output = if self.alternate_entered {
            execute!(
                stdout,
                Show,
                event::DisableMouseCapture,
                DisableBracketedPaste,
                LeaveAlternateScreen
            )
        } else {
            Ok(())
        };
        let raw = terminal::disable_raw_mode();
        #[cfg(windows)]
        let modes = self.modes.restore();
        #[cfg(not(windows))]
        let modes: io::Result<()> = Ok(());
        let flush = stdout.flush();
        self.restored = true;
        output.and(raw).and(modes).and(flush)
    }
}

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        let _ = self.restore();
    }
}

struct TtyRenderer<W: Write> {
    writer: W,
    previous: Option<screen::Frame>,
    width: Option<u16>,
    height: Option<u16>,
    stats: RenderStats,
}

#[derive(Default, Debug, Clone, serde::Serialize)]
struct RenderStats {
    frames: u64,
    full_clears: u64,
    changed_rows: u64,
    bytes: u64,
    flushes: u64,
}

impl TtyRenderer<io::Stdout> {
    fn stdout() -> Self {
        Self::new(io::stdout())
    }
}

impl<W: Write> TtyRenderer<W> {
    fn new(writer: W) -> Self {
        Self {
            writer,
            previous: None,
            width: None,
            height: None,
            stats: RenderStats::default(),
        }
    }

    fn render(&mut self, state: &TuiState) -> io::Result<()> {
        let (width, height) = terminal::size().unwrap_or((DEFAULT_TERMINAL_WIDTH, 24));
        self.render_at(state, width, height)
    }

    fn render_at(&mut self, state: &TuiState, width: u16, height: u16) -> io::Result<()> {
        if width == 0 || height == 0 {
            return Ok(());
        }
        let frame = screen::frame(state, width, height);
        let full_redraw =
            self.previous.is_none() || self.width != Some(width) || self.height != Some(height);
        let mut bytes = Vec::new();
        if full_redraw {
            bytes.extend_from_slice(b"\x1b[2J");
            self.stats.full_clears += 1;
        }
        for (row, line) in frame.rows.iter().enumerate() {
            if full_redraw || self.previous.as_ref().and_then(|old| old.rows.get(row)) != Some(line)
            {
                write!(bytes, "\x1b[{};1H\x1b[2K{}", row + 1, line)?;
                self.stats.changed_rows += 1;
            }
        }
        if !bytes.is_empty() || self.previous.as_ref().map(|old| old.cursor) != Some(frame.cursor) {
            write!(bytes, "\x1b[{};{}H", frame.cursor.1 + 1, frame.cursor.0 + 1)?;
            // One coherent write and flush, including cursor-only frames.
            self.writer.write_all(&bytes)?;
            self.writer.flush()?;
            self.stats.frames += 1;
            self.stats.bytes += bytes.len() as u64;
            self.stats.flushes += 1;
        }
        self.previous = Some(frame);
        self.width = Some(width);
        self.height = Some(height);
        Ok(())
    }
}

fn spawn_input_reader(
    trace: Option<terminal_diagnostic::Trace>,
) -> (
    mpsc::UnboundedReceiver<InputEvent>,
    Arc<AtomicBool>,
    thread::JoinHandle<()>,
) {
    let (sender, receiver) = mpsc::unbounded_channel();
    let stop = Arc::new(AtomicBool::new(false));
    let thread_stop = Arc::clone(&stop);
    let handle = thread::spawn(move || {
        let mut mapper = KeyMapper::default();
        let mut sequence = 0u64;
        while !thread_stop.load(Ordering::Acquire) {
            match event::poll(Duration::from_millis(100)) {
                Ok(true) => match event::read() {
                    Ok(event) => {
                        sequence += 1;
                        let input = match &event {
                            Event::Key(key)
                                if trace.is_some()
                                    && key.code == KeyCode::F(10)
                                    && key.kind == KeyEventKind::Press =>
                            {
                                Some(InputEvent::Eof)
                            }
                            Event::Key(key) => mapper.map(*key),
                            Event::Resize(w, h) => Some(InputEvent::Resize(*w, *h)),
                            Event::Paste(text) => Some(InputEvent::Paste(normalize_paste(text))),
                            Event::Mouse(mouse) => match mouse.kind {
                                event::MouseEventKind::Down(event::MouseButton::Right) => {
                                    Some(InputEvent::PasteClipboard)
                                }
                                event::MouseEventKind::ScrollUp => Some(InputEvent::Scroll(-3)),
                                event::MouseEventKind::ScrollDown => Some(InputEvent::Scroll(3)),
                                _ => None,
                            },
                            _ => None,
                        };
                        if let Some(trace) = &trace {
                            trace.native(sequence, &event, input.as_ref());
                        }
                        if let Some(input) = input {
                            if sender.send(input).is_err() {
                                break;
                            }
                        }
                    }
                    Err(_) => break,
                },
                Ok(false) => {}
                Err(_) => break,
            }
        }
    });
    (receiver, stop, handle)
}

fn normalize_paste(text: &str) -> String {
    text.replace("\r\n", "\n").replace('\r', "\n")
}

fn is_control_key(key: KeyEvent, letter: char) -> bool {
    if key.modifiers.contains(KeyModifiers::ALT) {
        return false;
    }
    let KeyCode::Char(ch) = key.code else {
        return false;
    };
    if ch as u32 == letter.to_ascii_lowercase() as u32 - 'a' as u32 + 1 {
        return true;
    }
    if !key.modifiers.contains(KeyModifiers::CONTROL) {
        return false;
    }
    if ch.eq_ignore_ascii_case(&letter) {
        return true;
    }
    #[cfg(windows)]
    {
        windows_console::control_key_matches(ch, letter)
    }
    #[cfg(not(windows))]
    {
        false
    }
}

fn map_key_event(key: KeyEvent) -> Option<InputEvent> {
    if matches!(key.kind, KeyEventKind::Release) {
        return None;
    }
    if is_control_key(key, 'c') {
        return (key.kind == KeyEventKind::Press).then_some(InputEvent::CtrlC);
    }
    if is_control_key(key, 'p') {
        return Some(InputEvent::Palette);
    }
    if is_control_key(key, 'v')
        || key.code == KeyCode::Insert && key.modifiers.contains(KeyModifiers::SHIFT)
    {
        return Some(InputEvent::PasteClipboard);
    }
    if is_control_key(key, 'u') {
        return Some(InputEvent::ClearInput);
    }
    match key.code {
        KeyCode::F(1) if key.kind == KeyEventKind::Press => Some(InputEvent::Help),
        KeyCode::F(2) if key.kind == KeyEventKind::Press => Some(InputEvent::Access),
        KeyCode::F(8) if key.kind == KeyEventKind::Press => {
            #[cfg(windows)]
            let physical = windows_console::access_confirmation_down();
            #[cfg(not(windows))]
            let physical = false;
            Some(InputEvent::ConfirmAccess(physical))
        }
        KeyCode::Char(character)
            if !key.modifiers.contains(KeyModifiers::CONTROL)
                || key.modifiers.contains(KeyModifiers::ALT) =>
        {
            Some(InputEvent::Text(character.to_string()))
        }
        KeyCode::Enter if key.modifiers.contains(KeyModifiers::SHIFT) => Some(InputEvent::NewLine),
        KeyCode::Enter if key.kind == KeyEventKind::Press => Some(InputEvent::Enter),
        KeyCode::Backspace => Some(InputEvent::Backspace),
        KeyCode::Delete => Some(InputEvent::Delete),
        KeyCode::Left => Some(InputEvent::Left),
        KeyCode::Right => Some(InputEvent::Right),
        KeyCode::Home => Some(InputEvent::Home),
        KeyCode::End if key.modifiers.contains(KeyModifiers::CONTROL) => Some(InputEvent::Latest),
        KeyCode::End => Some(InputEvent::End),
        KeyCode::PageUp => Some(InputEvent::Scroll(-16)),
        KeyCode::PageDown => Some(InputEvent::Scroll(16)),
        KeyCode::Up => Some(InputEvent::HistoryPrevious),
        KeyCode::Down => Some(InputEvent::HistoryNext),
        KeyCode::Tab => Some(InputEvent::Complete),
        KeyCode::Esc if key.kind == KeyEventKind::Press => Some(InputEvent::Escape),
        _ => None,
    }
}

#[derive(Default)]
struct KeyMapper {
    controls_down: Vec<KeyCode>,
}

impl KeyMapper {
    fn map(&mut self, key: KeyEvent) -> Option<InputEvent> {
        self.map_with_release_records(key, cfg!(windows))
    }

    fn map_with_release_records(&mut self, key: KeyEvent, releases: bool) -> Option<InputEvent> {
        // The locked Windows ReadConsoleInput backend labels every key-down as
        // Press. Latch only discrete controls there, never editing characters.
        // VT backends without release records rely on explicit Repeat kinds.
        let control = matches!(key.code, KeyCode::Enter | KeyCode::Esc) || is_control_key(key, 'c');
        if key.kind == KeyEventKind::Release {
            self.controls_down.retain(|code| *code != key.code);
            return None;
        }
        if releases && control {
            if self.controls_down.contains(&key.code) {
                return None;
            }
            self.controls_down.push(key.code);
        }
        map_key_event(key)
    }
}

#[cfg(test)]
mod tests {
    mod feedback_tests;
    use argentum_domain::ProviderStatus;
    use argentum_providers::{ModelProvider, ModelRequest, ProviderEvent, ProviderRegistry};
    use argentum_runtime::RuntimeService;
    use argentum_security::{ApprovalPolicy, CapabilityBroker};
    use argentum_store::Store;
    use argentum_tools::ToolRegistry;
    use argentum_workspaces::WorkspaceManager;
    use async_trait::async_trait;
    use tokio::sync::mpsc;

    use super::*;
    use crate::HostConfig;

    fn key(code: KeyCode, modifiers: KeyModifiers, kind: KeyEventKind) -> KeyEvent {
        KeyEvent::new_with_kind(code, modifiers, kind)
    }

    #[test]
    fn key_releases_are_ignored_and_repeats_are_preserved() {
        assert_eq!(
            map_key_event(key(
                KeyCode::Char('b'),
                KeyModifiers::NONE,
                KeyEventKind::Press,
            )),
            Some(InputEvent::Text("b".into()))
        );
        assert_eq!(
            map_key_event(key(
                KeyCode::Char('b'),
                KeyModifiers::NONE,
                KeyEventKind::Repeat,
            )),
            Some(InputEvent::Text("b".into()))
        );
        assert_eq!(
            map_key_event(key(
                KeyCode::Char('b'),
                KeyModifiers::NONE,
                KeyEventKind::Release,
            )),
            None
        );
        assert_eq!(
            map_key_event(key(
                KeyCode::Enter,
                KeyModifiers::NONE,
                KeyEventKind::Release
            )),
            None
        );
    }

    #[test]
    fn pasted_crlf_is_normalized_to_multiline_input() {
        assert_eq!(
            normalize_paste("first\r\nsecond\rthird"),
            "first\nsecond\nthird"
        );
    }

    #[test]
    fn native_control_latch_keeps_editing_repeats_and_distinct_taps() {
        let mut mapper = KeyMapper::default();
        let mut state = TuiState::new(PathBuf::from("fixture"), true);
        for ch in "abc bookkeeper Привет é".chars() {
            for kind in [KeyEventKind::Press, KeyEventKind::Release] {
                if let Some(input) = mapper.map_with_release_records(
                    key(KeyCode::Char(ch), KeyModifiers::NONE, kind),
                    true,
                ) {
                    state.apply_input(input);
                }
            }
        }
        assert_eq!(state.composer(), "abc bookkeeper Привет é");
        let press = key(KeyCode::Enter, KeyModifiers::NONE, KeyEventKind::Press);
        assert_eq!(
            mapper.map_with_release_records(press, true),
            Some(InputEvent::Enter)
        );
        for _ in 0..20 {
            assert_eq!(mapper.map_with_release_records(press, true), None);
        }
        mapper.map_with_release_records(
            key(KeyCode::Enter, KeyModifiers::NONE, KeyEventKind::Release),
            true,
        );
        assert_eq!(
            mapper.map_with_release_records(press, true),
            Some(InputEvent::Enter)
        );
        let mut vt_mapper = KeyMapper::default();
        assert_eq!(
            vt_mapper.map_with_release_records(press, false),
            Some(InputEvent::Enter)
        );
        assert_eq!(
            vt_mapper.map_with_release_records(press, false),
            Some(InputEvent::Enter)
        );
        for _ in 0..2 {
            state.apply_input(
                mapper
                    .map_with_release_records(
                        key(KeyCode::Backspace, KeyModifiers::NONE, KeyEventKind::Repeat),
                        true,
                    )
                    .unwrap(),
            );
        }
        assert_eq!(state.composer(), "abc bookkeeper Привет");
        state.apply_input(InputEvent::Left);
        state.apply_input(InputEvent::Text("é".into()));
        assert_eq!(state.composer(), "abc bookkeeper Привеéт");
    }

    #[test]
    fn composer_cursor_edits_graphemes_and_preserves_literal_paste_sequences() {
        let mut state = TuiState::new(PathBuf::from("fixture"), true);
        state.apply_input(InputEvent::Text("é e\u{301}界".into()));
        state.apply_input(InputEvent::Left);
        state.apply_input(InputEvent::Backspace);
        assert_eq!(state.composer(), "é 界");
        state.apply_input(InputEvent::Delete);
        assert_eq!(state.composer(), "é ");
        state.apply_input(InputEvent::Text(normalize_paste(
            "abc\r\nbookkeeper \\r\\n",
        )));
        assert_eq!(state.composer(), "é abc\nbookkeeper \\r\\n");
        assert_eq!(state.followups.totals(), (0, 0));
    }

    #[test]
    fn unsupported_coding_preflight_preserves_the_draft_before_submission() {
        let mut state = TuiState::new(PathBuf::from("fixture"), true);
        state.set_coding_preflight_issue(Some(
            "the configured llama.cpp sidecar has no implemented tool-template and parser contract"
                .into(),
        ));
        for character in "bookkeeper".chars() {
            state.apply_input(InputEvent::Text(character.to_string()));
        }
        let action = state.apply_input(InputEvent::Enter);
        assert!(matches!(action, TuiAction::Redraw));
        assert_eq!(state.composer(), "bookkeeper");
    }

    #[test]
    fn renderer_writes_only_changed_lines_and_does_not_flush_unchanged_frames() {
        let mut state = TuiState::new(PathBuf::from("fixture"), true);
        let mut renderer = TtyRenderer::new(Vec::new());
        renderer.render_at(&state, 80, 24).expect("initial frame");
        let first_len = renderer.writer.len();
        renderer.render_at(&state, 80, 24).expect("unchanged frame");
        assert_eq!(renderer.writer.len(), first_len);

        state.apply_input(InputEvent::Text("x".into()));
        renderer.render_at(&state, 80, 24).expect("changed frame");
        let output = String::from_utf8(renderer.writer.clone()).expect("terminal bytes");
        assert_eq!(output.matches("\x1b[2J").count(), 1);
        assert!(output.contains("\x1b[1;"));
    }

    #[test]
    fn manual_stop_signal_exists_before_the_host_future_is_polled() {
        let mut state = TuiState::new(PathBuf::from("fixture"), true);
        state.active_session_id = Some(argentum_domain::SessionId::new_v4());
        let action = state.handle_slash_command("/compact");
        assert!(matches!(action, TuiAction::CompactContext { .. }));
        let signal = state.manual_cancellation.clone().unwrap();
        assert!(!signal.is_cancelled());
        state.handle_slash_command("/stop");
        assert!(signal.is_cancelled());
        assert!(state.take_ready_submission().is_none());
    }

    #[tokio::test]
    async fn line_session_shutdown_unmounts_the_composed_runtime() {
        let workspace = tempfile::tempdir().expect("workspace");
        let host = CommandHost::start_composed(HostConfig::in_memory(workspace.path()))
            .await
            .expect("composed host");
        let observer = host.lifecycle_observer();

        run_line_session(
            host,
            tokio::io::BufReader::new(tokio::io::empty()),
            tokio::io::sink(),
            TuiOptions {
                no_color: true,
                force_non_interactive: true,
            },
        )
        .await
        .expect("line session shutdown");

        assert!(observer
            .harness_lifecycle_snapshot()
            .await
            .expect("line lifecycle")
            .module_ids()
            .is_empty());
    }

    #[tokio::test]
    async fn tty_cleanup_waits_for_spawned_commands_before_shutdown() {
        let (_workspace, mut host) = blocking_composed_host().await;
        let local = host.local_handle();
        let mut events = host.subscribe();
        let mut state = TuiState::new(host.workspace_root().to_path_buf(), true);
        state.apply_event(AppEvent::WorkspaceStateLoaded(
            host.workspace_snapshot().expect("workspace snapshot"),
        ));
        let mut command_tasks = tokio::task::JoinSet::new();

        assert!(!dispatch_tui_action(
            &mut state,
            &local,
            &mut command_tasks,
            TuiAction::Dispatch(AppCommand::SubmitTask {
                prompt: "wait for TTY cleanup".into(),
            }),
        ));
        loop {
            if matches!(
                events.recv().await.expect("run event"),
                AppEvent::AssistantDelta { .. }
            ) {
                break;
            }
        }

        host.request_shutdown().expect("shutdown request");
        drain_command_tasks(&mut command_tasks)
            .await
            .expect("command task cleanup");
        host.shutdown().await.expect("runtime shutdown");
        assert!(host
            .harness_lifecycle_snapshot()
            .await
            .expect("TTY lifecycle")
            .module_ids()
            .is_empty());
    }

    #[tokio::test]
    async fn tty_cleanup_reports_a_late_command_failure() {
        let workspace = tempfile::tempdir().expect("workspace");
        let mut host = CommandHost::start_composed(HostConfig::in_memory(workspace.path()))
            .await
            .expect("composed host");
        host.request_shutdown().expect("shutdown request");
        let local = host.local_handle();
        let mut state = TuiState::new(host.workspace_root().to_path_buf(), true);
        let mut command_tasks = tokio::task::JoinSet::new();

        assert!(!dispatch_tui_action(
            &mut state,
            &local,
            &mut command_tasks,
            TuiAction::Dispatch(AppCommand::NewSession),
        ));
        let error = drain_command_tasks(&mut command_tasks)
            .await
            .expect_err("late command failure");

        assert!(error.to_string().contains("runtime is shutting down"));
        host.shutdown().await.expect("runtime shutdown");
    }

    #[tokio::test]
    async fn retained_nonselected_queue_blocks_archive_and_preserves_controller_state() {
        let workspace = tempfile::tempdir().expect("workspace");
        let mut host = CommandHost::start_composed(HostConfig::in_memory(workspace.path()))
            .await
            .expect("composed host");
        let local = host.local_handle();
        let mut state = TuiState::new(host.workspace_root().to_path_buf(), true);
        let initial = host.workspace_snapshot().expect("initial snapshot");
        let original = initial.active_session_id.expect("original session");
        state.apply_event(AppEvent::WorkspaceStateLoaded(initial));

        local
            .dispatch(AppCommand::NewSession)
            .await
            .expect("new session");
        let candidate = host
            .workspace_snapshot()
            .expect("new session snapshot")
            .active_session_id
            .expect("candidate session");
        local
            .dispatch(AppCommand::SelectSession {
                session_id: original,
            })
            .await
            .expect("select original");
        state.apply_event(AppEvent::WorkspaceStateLoaded(
            host.workspace_snapshot()
                .expect("selected original snapshot"),
        ));

        let target = state.submission_target().expect("selected target");
        let candidate_target = SubmissionTarget {
            session_id: candidate,
            ..target
        };
        let first = state
            .followups
            .enqueue(candidate_target.clone(), "first retained prompt".into())
            .expect("first queue item");
        let second = state
            .followups
            .enqueue(candidate_target.clone(), "second retained prompt".into())
            .expect("second queue item");
        state.followups.pause(&candidate_target, "F04 paused");
        let ids_before = state.followups.pending_ids(&candidate_target);
        let prompts_before = state
            .followups
            .pending_prompts(&candidate_target)
            .into_iter()
            .map(str::to_owned)
            .collect::<Vec<_>>();
        let pause_before = state
            .followups
            .pause_reason(&candidate_target)
            .map(str::to_owned);
        assert_eq!(ids_before, [first, second]);
        assert_eq!(
            prompts_before,
            ["first retained prompt", "second retained prompt"]
        );
        assert_eq!(pause_before.as_deref(), Some("F04 paused"));

        let mut command_tasks = tokio::task::JoinSet::new();
        assert!(!dispatch_tui_action(
            &mut state,
            &local,
            &mut command_tasks,
            TuiAction::Dispatch(AppCommand::SetSessionArchived {
                session_id: candidate,
                archived: true,
            }),
        ));
        assert!(command_tasks.is_empty());
        assert_eq!(state.followups.pending_ids(&candidate_target), ids_before);
        assert_eq!(
            state
                .followups
                .pending_prompts(&candidate_target)
                .into_iter()
                .map(str::to_owned)
                .collect::<Vec<_>>(),
            prompts_before
        );
        assert_eq!(
            state
                .followups
                .pause_reason(&candidate_target)
                .map(str::to_owned),
            pause_before
        );
        assert!(
            !host
                .inspect_session(candidate)
                .expect("candidate row")
                .archived
        );

        let host_error = local
            .dispatch(AppCommand::SetSessionArchived {
                session_id: candidate,
                archived: true,
            })
            .await
            .expect_err("host archive guard");
        assert!(host_error.to_string().contains("admitted or pending work"));
        host.shutdown().await.expect("runtime shutdown");
    }

    async fn blocking_composed_host() -> (tempfile::TempDir, CommandHost) {
        let workspace = tempfile::tempdir().expect("workspace");
        let broker =
            CapabilityBroker::new(workspace.path(), ApprovalPolicy::default()).expect("broker");
        let workspace_manager = WorkspaceManager::new(broker);
        let tools = ToolRegistry::with_builtins(workspace_manager.clone());
        let mut providers = ProviderRegistry::default();
        providers.register(TtyBlockingProvider);
        let runtime = RuntimeService::new_composed(
            Store::open_in_memory().expect("store"),
            providers,
            tools,
            workspace_manager,
        )
        .await
        .expect("composed runtime");
        runtime
            .dispatch(AppCommand::SaveProviderProfile {
                profile: ProviderProfile {
                    id: "tty-blocking".into(),
                    label: "TTY blocking".into(),
                    kind: argentum_domain::ProviderKind::OpenAiCompatible,
                    endpoint: "https://tty-blocking.example.test/v1/".into(),
                    model: "test-model".into(),
                    selected: true,
                },
            })
            .await
            .expect("selected blocking provider");
        runtime
            .declare_model_budget("tty-blocking", [2_000_000, 1_900_000, 8192, 4096, 1024])
            .unwrap();
        (workspace, CommandHost::from_runtime(runtime))
    }

    #[derive(Debug)]
    struct TtyBlockingProvider;

    #[async_trait]
    impl ModelProvider for TtyBlockingProvider {
        fn id(&self) -> &'static str {
            "tty-blocking"
        }

        fn status(&self) -> ProviderStatus {
            ProviderStatus {
                profile_id: self.id().into(),
                kind: argentum_domain::ProviderKind::OpenAiCompatible,
                label: "TTY blocking".into(),
                endpoint: "local".into(),
                connected: true,
                detail: "test".into(),
            }
        }

        async fn stream(
            &self,
            _request: ModelRequest,
            sender: mpsc::Sender<ProviderEvent>,
        ) -> Result<(), argentum_providers::ProviderError> {
            sender
                .send(ProviderEvent::Delta("started".into()))
                .await
                .map_err(|_| argentum_providers::ProviderError::ConsumerClosed)?;
            std::future::pending().await
        }
    }

    #[test]
    fn slash_completion_cycles_only_for_a_command_prefix() {
        let mut state = TuiState::new(PathBuf::from("workspace"), true);
        state.apply_input(InputEvent::Text("/s".into()));
        state.apply_input(InputEvent::Complete);
        assert_eq!(state.composer(), "/sessions");

        state.clear_composer();
        state.apply_input(InputEvent::Text("plain".into()));
        state.apply_input(InputEvent::Complete);
        assert_eq!(state.composer(), "plain");
    }

    #[test]
    fn line_width_wrapping_preserves_unicode_boundaries() {
        let state = TuiState::new(PathBuf::from("workspace"), true);
        let rendered = state.render(20);
        assert!(rendered.lines().all(|line| line.chars().count() <= 20));
    }

    #[test]
    fn tools_command_is_local_inspection_and_unknown_commands_do_not_submit() {
        let mut state = TuiState::new(PathBuf::from("workspace"), true);
        state.apply_input(InputEvent::Text("/tools".into()));
        assert_eq!(
            state.apply_input(InputEvent::Enter),
            TuiAction::Ui(menu::Request::Open(menu::View::Tools))
        );
        state.apply_input(InputEvent::Text("/not-a-command".into()));
        assert_eq!(state.apply_input(InputEvent::Enter), TuiAction::Redraw);
        assert!(state.render(80).contains("Unknown command"));
    }

    #[test]
    fn first_ctrl_c_without_a_run_clears_and_arms_exit() {
        let mut state = TuiState::new(PathBuf::from("workspace"), true);
        state.apply_input(InputEvent::Text("draft".into()));

        assert_eq!(state.apply_input(InputEvent::CtrlC), TuiAction::Redraw);
        assert!(state.composer().is_empty());
        assert_eq!(state.apply_input(InputEvent::CtrlC), TuiAction::Exit);
    }

    #[test]
    fn steer_command_rejects_without_an_active_run_and_preserves_the_draft() {
        let mut state = TuiState::new(PathBuf::from("workspace"), true);
        state.apply_input(InputEvent::Text("/steer revise the answer".into()));

        assert_eq!(state.apply_input(InputEvent::Enter), TuiAction::Redraw);
        assert_eq!(state.composer(), "/steer revise the answer");
        assert!(state.status.contains("No active ordinary run"));
    }

    #[test]
    fn steer_command_binds_the_exact_active_run_without_entering_the_fifo_queue() {
        let mut state = TuiState::new(PathBuf::from("workspace"), true);
        state.project_id = Some(argentum_domain::ProjectId::new_v4());
        state.active_session_id = Some(argentum_domain::SessionId::new_v4());
        state.active_run_id = Some(RunId::new_v4());
        state.apply_input(InputEvent::Text("/steer revise the answer".into()));

        let action = state.apply_input(InputEvent::Enter);
        let TuiAction::Steer { submission, draft } = action else {
            panic!("expected steering action");
        };
        assert_eq!(submission.run_id, state.active_run_id.unwrap());
        assert_eq!(submission.text, "revise the answer");
        assert_eq!(draft, "/steer revise the answer");
        assert_eq!(state.followups.pending_count(), 0);
        assert_eq!(state.composer(), "/steer revise the answer");
    }

    #[test]
    fn command_palette_steers_the_unsent_draft_and_escape_keeps_it() {
        let mut state = TuiState::new(PathBuf::from("workspace"), true);
        state.project_id = Some(argentum_domain::ProjectId::new_v4());
        state.active_session_id = Some(argentum_domain::SessionId::new_v4());
        state.active_run_id = Some(RunId::new_v4());
        state.apply_input(InputEvent::Text("unsent draft".into()));
        state.open_commands(false);
        assert_eq!(state.overlay_input(InputEvent::Escape), TuiAction::Redraw);
        assert_eq!(state.composer(), "unsent draft");

        state.open_commands(false);
        state.overlay_input(InputEvent::Text("steer".into()));
        let action = state.overlay_input(InputEvent::Enter);
        let TuiAction::Steer { submission, draft } = action else {
            panic!("expected steering action");
        };
        assert_eq!(submission.text, "unsent draft");
        assert_eq!(draft, "unsent draft");
        assert_eq!(state.composer(), "unsent draft");
    }
}
