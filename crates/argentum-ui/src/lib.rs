use std::collections::{HashMap, VecDeque};
use std::rc::Rc;
use std::sync::{Arc, Mutex, MutexGuard};

use argentum_domain::{
    ActiveRunState, AppCommand, AppEvent, ApprovalId, ConversationMessageStatus, ConversationRole,
    Goal, HarnessAvailability, HarnessCapabilityState, HarnessExecutionProfileSummary,
    HarnessProfileSummary, HarnessReadiness, HarnessSnapshot, HarnessSurfaceState, ProviderKind,
    ProviderModel, ProviderProfile, RunId, SessionId, SurfaceId, TaskLifecycle, ToolResultState,
    TrajectoryEntry, TrajectoryKind, TrajectorySnapshot, TrajectoryState,
};
use slint::{ComponentHandle, Model, ModelRc, SharedString, VecModel};
use time::macros::format_description;
use tracing::warn;

slint::include_modules!();

const MAX_ACTIVITY_ITEMS: usize = 50;
const MAX_TRAJECTORY_ITEMS: usize = 200;

#[derive(Clone)]
pub struct UiHandle {
    window: Rc<MainWindow>,
    projection: Arc<Mutex<ProjectionState>>,
}

#[derive(Clone)]
pub struct WeakUiHandle {
    window: slint::Weak<MainWindow>,
    projection: Arc<Mutex<ProjectionState>>,
}

#[derive(Default)]
struct ProjectionState {
    run_sessions: HashMap<RunId, SessionId>,
    pending_run_sessions: VecDeque<SessionId>,
    approval_sessions: HashMap<ApprovalId, SessionId>,
}

impl UiHandle {
    pub fn new() -> Result<Self, slint::PlatformError> {
        Ok(Self {
            window: Rc::new(MainWindow::new()?),
            projection: Arc::default(),
        })
    }

    pub fn from_window(window: MainWindow) -> Self {
        Self {
            window: Rc::new(window),
            projection: Arc::default(),
        }
    }

    pub fn window(&self) -> &MainWindow {
        &self.window
    }

    pub fn weak_window(&self) -> slint::Weak<MainWindow> {
        self.window.as_weak()
    }

    pub fn weak_handle(&self) -> WeakUiHandle {
        WeakUiHandle {
            window: self.window.as_weak(),
            projection: self.projection.clone(),
        }
    }

    pub fn show(&self) -> Result<(), slint::PlatformError> {
        self.window.show()
    }

    pub fn hide(&self) -> Result<(), slint::PlatformError> {
        self.window.hide()
    }

    fn projection(&self) -> MutexGuard<'_, ProjectionState> {
        self.projection
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    fn active_session_id(&self) -> Option<SessionId> {
        self.window.get_active_session_id().as_str().parse().ok()
    }

    fn session_for_run(&self, run_id: RunId) -> Option<SessionId> {
        let mut projection = self.projection();
        if let Some(session_id) = projection.run_sessions.get(&run_id) {
            return Some(*session_id);
        }
        let session_id = projection.pending_run_sessions.pop_front()?;
        projection.run_sessions.insert(run_id, session_id);
        Some(session_id)
    }

    fn run_targets_active_session(&self, run_id: RunId) -> bool {
        let Some(session_id) = self.session_for_run(run_id) else {
            return false;
        };
        self.active_session_id() == Some(session_id)
    }

    fn remember_approval_session(&self, approval_id: ApprovalId, run_id: RunId) {
        if let Some(session_id) = self.session_for_run(run_id) {
            self.projection()
                .approval_sessions
                .insert(approval_id, session_id);
        }
    }

    fn approval_targets_active_session(&self, approval_id: ApprovalId) -> bool {
        let session_id = self
            .projection()
            .approval_sessions
            .get(&approval_id)
            .copied();
        session_id.is_none() || session_id == self.active_session_id()
    }

    pub fn apply_event(&self, event: &AppEvent) {
        match event {
            AppEvent::WorkspaceStateLoaded(snapshot) => {
                let previous_active_session_id = self.window.get_active_session_id().to_string();
                self.window
                    .set_project_name(snapshot.project.name.clone().into());
                self.window.set_workspace_label(
                    snapshot.project.workspace_root.display().to_string().into(),
                );
                let active_session_id = snapshot
                    .active_session_id
                    .map(|session_id| session_id.to_string())
                    .unwrap_or_default();
                self.window
                    .set_active_session_id(active_session_id.clone().into());
                self.window.set_session_items(session_item_model(snapshot));
                if let Some(active_session_uuid) = snapshot.active_session_id {
                    if let Some(session) = snapshot
                        .sessions
                        .iter()
                        .find(|session| session.id == active_session_uuid)
                    {
                        self.window.set_session_title(session.title.clone().into());
                    }
                }
                if previous_active_session_id != active_session_id {
                    self.reset_session_view();
                }
            }
            AppEvent::ConversationSnapshotLoaded(snapshot) => {
                {
                    let mut projection = self.projection();
                    for message in &snapshot.messages {
                        projection
                            .run_sessions
                            .insert(message.run_id, message.session_id);
                    }
                }
                if self.active_session_id() != Some(snapshot.session_id) {
                    return;
                }
                self.window
                    .set_conversation_messages(conversation_message_model(&snapshot.messages));
                self.window.set_user_prompt(SharedString::default());
                self.window.set_transcript(SharedString::default());
                self.window.set_current_reasoning(SharedString::default());
                self.window.set_current_reasoning_available(false);
                self.window
                    .set_current_run_provider_label(SharedString::default());
                self.window.set_current_run_model(SharedString::default());
                self.reset_usage_projection();
                self.window.set_error_message(SharedString::default());
            }
            AppEvent::GoalSnapshotLoaded { session_id, goal } => {
                if self.active_session_id() != Some(*session_id) {
                    return;
                }
                self.apply_goal_snapshot(goal.as_ref());
            }
            AppEvent::ActiveRunsSnapshot { runs } => {
                self.apply_active_runs_snapshot(runs);
            }
            AppEvent::ProjectCreated(project) => {
                self.window.set_project_name(project.name.clone().into());
                self.window
                    .set_workspace_label(project.workspace_root.display().to_string().into());
                self.push_activity(
                    "Project created",
                    project.workspace_root.display().to_string(),
                    "neutral",
                );
            }
            AppEvent::SessionCreated(session) => {
                self.window.set_submission_pending(false);
                self.window
                    .set_active_session_id(session.id.to_string().into());
                self.window.set_session_title(session.title.clone().into());
                self.reset_session_view();
                self.push_activity("Session created", session.title.clone(), "neutral");
            }
            AppEvent::TaskAccepted(task) => {
                self.projection()
                    .pending_run_sessions
                    .push_back(task.session_id);
                if self.active_session_id() != Some(task.session_id) {
                    return;
                }
                self.window.set_submission_pending(false);
                self.window.set_user_prompt(task.prompt.clone().into());
                self.window.set_draft(SharedString::default());
                self.window.set_transcript(SharedString::default());
                self.window.set_current_reasoning(SharedString::default());
                self.window.set_current_reasoning_available(false);
                self.window
                    .set_current_run_provider_label(self.window.get_provider_label());
                self.window
                    .set_current_run_model(self.window.get_provider_model());
                self.reset_usage_projection();
                self.window.set_error_message(SharedString::default());
                self.window.set_run_state(task.lifecycle.label().into());
                self.window.set_running(is_active_lifecycle(task.lifecycle));
                self.window.set_plan_summary("No stages yet".into());
                self.window.set_plan_steps(empty_plan_model());
                self.reset_change_and_verification_state();
                self.close_approval();
                self.push_activity("Task submitted", task.prompt.clone(), "active");
            }
            AppEvent::PlanUpdated { run_id, steps } => {
                if !self.run_targets_active_session(*run_id) {
                    return;
                }
                self.window.set_active_run_id(run_id.to_string().into());
                self.window
                    .set_plan_summary(stage_count_label(steps.len()).into());
                self.window.set_plan_steps(plan_step_model(steps));
                self.push_activity(
                    "Run stages updated",
                    stage_count_label(steps.len()),
                    "active",
                );
            }
            AppEvent::AssistantDelta { run_id, text } => {
                if !self.run_targets_active_session(*run_id) {
                    return;
                }
                self.window.set_active_run_id(run_id.to_string().into());
                let mut transcript = self.window.get_transcript().to_string();
                transcript.push_str(text);
                self.window.set_transcript(transcript.into());
                self.window.set_error_message(SharedString::default());
            }
            AppEvent::AssistantReasoningDelta { run_id, text } => {
                if !self.run_targets_active_session(*run_id) {
                    return;
                }
                self.window.set_active_run_id(run_id.to_string().into());
                let mut reasoning = self.window.get_current_reasoning().to_string();
                reasoning.push_str(text);
                self.window
                    .set_current_reasoning(without_reasoning_markers(&reasoning).into());
                self.window.set_current_reasoning_available(true);
                self.window.set_error_message(SharedString::default());
            }
            AppEvent::ModelUsageUpdated {
                session_id,
                run_id,
                profile_id,
                model,
                usage,
            } => {
                if self.active_session_id() != Some(*session_id)
                    || !self.run_targets_active_session(*run_id)
                {
                    return;
                }
                self.window.set_active_run_id(run_id.to_string().into());
                self.window.set_usage_reported(true);
                self.window
                    .set_usage_input_tokens(saturating_i32_u64(usage.input_tokens));
                self.window
                    .set_usage_output_tokens(saturating_i32_u64(usage.output_tokens));
                self.window
                    .set_usage_total_tokens(saturating_i32_u64(usage.total_tokens));
                self.window
                    .set_context_window_reported(usage.context_window_tokens.is_some());
                self.window.set_context_window_tokens(
                    usage.context_window_tokens.map_or(0, saturating_i32_u64),
                );
                self.window.set_current_run_model(model.clone().into());
                if !profile_id.is_empty() {
                    let profiles = self.window.get_provider_profiles();
                    let provider_label = (0..profiles.row_count())
                        .filter_map(|index| profiles.row_data(index))
                        .find(|profile| profile.id.as_str() == profile_id)
                        .map_or_else(|| profile_id.clone(), |profile| profile.label.to_string());
                    self.window
                        .set_current_run_provider_label(provider_label.into());
                }
            }
            AppEvent::RunStatusChanged { run_id, lifecycle } => {
                if !self.run_targets_active_session(*run_id) {
                    return;
                }
                self.window.set_active_run_id(run_id.to_string().into());
                self.window.set_run_state(lifecycle.label().into());
                self.window.set_running(is_active_lifecycle(*lifecycle));
                if lifecycle.is_terminal() {
                    self.finish_stage_projection(*lifecycle);
                }
                self.push_activity(
                    lifecycle_activity_title(*lifecycle),
                    lifecycle.label(),
                    lifecycle_activity_state(*lifecycle),
                );
            }
            AppEvent::Error { message, .. } => {
                self.window.set_submission_pending(false);
                self.window.set_approval_response_pending(false);
                let consumes_provider_error = self.window.get_provider_error_consumes_next()
                    && message == self.window.get_provider_error().as_str();
                let provider_pending = self.window.get_provider_pending().to_string();
                let provider_models_pending = self.window.get_provider_models_pending().to_string();
                if consumes_provider_error {
                    self.window.set_provider_error_consumes_next(false);
                } else if !provider_pending.is_empty() || !provider_models_pending.is_empty() {
                    // The command-specific failure callback owns provider presentation. Profile
                    // and model requests can overlap, so a global error cannot safely identify
                    // which operation failed.
                } else {
                    self.window.set_error_message(present_error(message).into());
                    self.window.set_running(false);
                    let current_state = self.window.get_run_state().to_string();
                    if current_state != "Failed" && current_state != "Cancelled" {
                        self.window.set_run_state("Could not continue".into());
                    }
                    self.push_activity("Could not continue", present_error(message), "error");
                }
            }
            AppEvent::RunError {
                session_id,
                run_id,
                message,
                ..
            } => {
                {
                    self.projection().run_sessions.insert(*run_id, *session_id);
                }
                if self.active_session_id() != Some(*session_id) {
                    return;
                }
                let current_run_id = self.window.get_active_run_id().to_string();
                if !current_run_id.is_empty() && current_run_id != run_id.to_string() {
                    return;
                }
                self.window.set_submission_pending(false);
                self.window.set_approval_response_pending(false);
                self.window.set_active_run_id(run_id.to_string().into());
                self.window.set_error_message(present_error(message).into());
                self.window.set_run_state("Could not continue".into());
                self.window.set_running(false);
                self.push_activity("Run failed", present_error(message), "error");
            }
            AppEvent::ProviderStatus(status) => {
                let profile_id = provider_status_id(status);
                let current_id = self.window.get_provider_id().to_string();
                let has_profile_snapshot = self.window.get_provider_profiles().row_count() > 0;
                let targets_current =
                    current_id.is_empty() || current_id == profile_id || !has_profile_snapshot;
                if targets_current {
                    let was_testing = self.window.get_provider_testing()
                        && self.window.get_provider_pending().as_str() == "probe";
                    self.window.set_provider_label(status.label.clone().into());
                    self.window.set_provider_id(profile_id.into());
                    self.window
                        .set_provider_kind(provider_kind_id(status.kind).into());
                    self.window
                        .set_provider_kind_label(provider_kind_label(status.kind).into());
                    self.window
                        .set_provider_endpoint(status.endpoint.clone().into());
                    self.window.set_provider_connected(status.connected);
                    self.window.set_provider_testing(false);
                    self.window.set_provider_pending(SharedString::default());
                    self.window
                        .set_provider_detail(status.detail.clone().into());
                    if status.connected {
                        self.window.set_provider_probe_state("connected".into());
                        self.window.set_provider_error(SharedString::default());
                        self.window
                            .set_provider_notice("Connection verified.".into());
                        self.window.set_provider_error_consumes_next(false);
                    } else if was_testing {
                        self.window.set_provider_probe_state("failed".into());
                        self.window.set_provider_error(status.detail.clone().into());
                        self.window.set_provider_notice(SharedString::default());
                        self.window.set_provider_error_consumes_next(true);
                    }
                    self.push_activity(
                        if status.connected {
                            "Provider connected"
                        } else if was_testing {
                            "Connection failed"
                        } else {
                            "Provider configured"
                        },
                        status.detail.clone(),
                        if status.connected {
                            "success"
                        } else if was_testing {
                            "error"
                        } else {
                            "neutral"
                        },
                    );
                }
            }
            AppEvent::ProviderProfilesSnapshot { profiles } => {
                self.apply_provider_profiles(profiles);
            }
            AppEvent::ProviderModelsSnapshot {
                provider_id,
                models,
                selected_model,
            } => {
                self.apply_provider_models(provider_id, models, selected_model);
            }
            AppEvent::TrajectoryEntryRecorded { session_id, entry } => {
                self.append_trajectory_entry(*session_id, entry);
            }
            AppEvent::TrajectorySnapshotLoaded(snapshot) => {
                self.apply_trajectory_snapshot(snapshot);
            }
            AppEvent::HarnessSnapshotLoaded(snapshot) => {
                self.apply_harness_snapshot(snapshot);
            }
            AppEvent::ToolStarted(trace) => {
                if !self.run_targets_active_session(trace.run_id) {
                    return;
                }
                self.window
                    .set_active_run_id(trace.run_id.to_string().into());
                self.window
                    .set_run_state(TaskLifecycle::Running.label().into());
                self.window.set_running(true);
                self.push_activity(
                    format!("{} started", tool_label(&trace.tool_id)),
                    trace.summary.clone(),
                    "active",
                );
            }
            AppEvent::ToolFinished(trace) => {
                if !self.run_targets_active_session(trace.run_id) {
                    return;
                }
                self.window
                    .set_active_run_id(trace.run_id.to_string().into());
                let (verb, state) = tool_result_presentation(&trace.result);
                let title = format!("{} {verb}", tool_label(&trace.tool_id));
                self.push_activity(title, trace.summary.clone(), state);
            }
            AppEvent::ApprovalRequested(request) => {
                self.remember_approval_session(request.id, request.run_id);
                if !self.run_targets_active_session(request.run_id) {
                    return;
                }
                self.window.set_approval_response_pending(false);
                self.window
                    .set_active_run_id(request.run_id.to_string().into());
                self.window.set_approval_open(true);
                self.window.set_approval_id(request.id.to_string().into());
                self.window
                    .set_approval_action(request.action.clone().into());
                self.window
                    .set_approval_target(request.target.clone().into());
                self.window
                    .set_approval_reason(request.reason.clone().into());
                self.window
                    .set_run_state(TaskLifecycle::WaitingForApproval.label().into());
                self.window.set_running(true);
                self.push_activity(
                    "Approval requested",
                    format!("{}: {}", request.action, request.target),
                    "attention",
                );
            }
            AppEvent::ApprovalResolved {
                approval_id,
                approved,
            } => {
                if !self.approval_targets_active_session(*approval_id) {
                    return;
                }
                self.projection().approval_sessions.remove(approval_id);
                self.close_approval();
                self.push_activity(
                    if *approved {
                        "Action approved"
                    } else {
                        "Action rejected"
                    },
                    if *approved {
                        "The action can run once."
                    } else {
                        "The action was not run."
                    },
                    if *approved { "success" } else { "error" },
                );
                if !*approved {
                    self.window.set_running(false);
                    self.window.set_run_state("Cancelled".into());
                }
            }
            AppEvent::ChangeSetReady(change_set) => {
                if !self.run_targets_active_session(change_set.run_id) {
                    return;
                }
                self.window
                    .set_active_run_id(change_set.run_id.to_string().into());
                self.window
                    .set_changed_files(saturating_i32(change_set.files_changed));
                self.window
                    .set_additions(saturating_i32(change_set.additions));
                self.window
                    .set_removals(saturating_i32(change_set.removals));
                let summary = change_summary(
                    change_set.files_changed,
                    change_set.additions,
                    change_set.removals,
                );
                self.window.set_change_summary(summary.clone().into());
                self.window.set_verification_state(
                    if change_set.verification_ready {
                        "ready"
                    } else {
                        "pending"
                    }
                    .into(),
                );
                self.window.set_verification_summary(
                    if change_set.verification_ready {
                        "Ready for verification"
                    } else {
                        "No automated checks were run"
                    }
                    .into(),
                );
                self.push_activity("Changes recorded", summary, "neutral");
            }
            AppEvent::VerificationCompleted {
                run_id,
                passed,
                summary,
            } => {
                if !self.run_targets_active_session(*run_id) {
                    return;
                }
                self.window.set_active_run_id(run_id.to_string().into());
                self.window
                    .set_verification_state(if *passed { "passed" } else { "failed" }.into());
                self.window.set_verification_summary(summary.clone().into());
                self.push_activity(
                    if *passed {
                        "Verification passed"
                    } else {
                        "Verification failed"
                    },
                    summary.clone(),
                    if *passed { "success" } else { "error" },
                );
            }
            AppEvent::LayoutChanged(profile) => {
                self.window.set_inspector_open(
                    profile
                        .visible
                        .get(&SurfaceId::Changes)
                        .copied()
                        .unwrap_or(false),
                );
                let trajectory_visible = profile
                    .visible
                    .get(&SurfaceId::Trajectory)
                    .copied()
                    .unwrap_or(false);
                if trajectory_visible && !self.window.get_trajectory_open() {
                    self.window.set_trajectory_loading(true);
                    self.window.set_trajectory_error(SharedString::default());
                } else if !trajectory_visible {
                    self.window.set_trajectory_loading(false);
                }
                self.window.set_trajectory_open(trajectory_visible);
                self.window.set_activity_open(
                    profile
                        .visible
                        .get(&SurfaceId::Activity)
                        .copied()
                        .unwrap_or(false),
                );
            }
        }
    }

    pub fn apply_command_failure(&self, command: &AppCommand, message: &str) {
        match command {
            AppCommand::ListProviderProfiles
            | AppCommand::SaveProviderProfile { .. }
            | AppCommand::SelectProviderProfile { .. }
            | AppCommand::ProbeProvider { .. } => {
                let pending = self.window.get_provider_pending().to_string();
                let expected = match command {
                    AppCommand::ListProviderProfiles => "list",
                    AppCommand::SaveProviderProfile { .. } => "save",
                    AppCommand::SelectProviderProfile { .. } => "select",
                    AppCommand::ProbeProvider { .. } => "probe",
                    _ => unreachable!("provider profile command matched above"),
                };
                if pending != expected {
                    return;
                }
                let presented = present_error(message);
                self.window.set_provider_error(presented.clone().into());
                self.window.set_provider_notice(SharedString::default());
                self.window.set_provider_loading(false);
                self.window.set_provider_saving(false);
                self.window.set_provider_selecting(false);
                self.window.set_provider_testing(false);
                self.window.set_provider_pending(SharedString::default());
                if pending == "probe" {
                    self.window.set_provider_probe_state("failed".into());
                }
                self.push_activity(provider_error_title(&pending), presented, "error");
            }
            AppCommand::ListProviderModels { provider_id }
                if self.window.get_provider_models_provider_id().as_str() == provider_id
                    && self.window.get_provider_models_pending().as_str() == "list" =>
            {
                let presented = present_error(message);
                self.window.set_provider_models_error(presented.into());
                self.window.set_provider_models_loading(false);
                self.window
                    .set_provider_models_pending(SharedString::default());
            }
            AppCommand::SelectProviderModel { provider_id, .. }
                if self.window.get_provider_id().as_str() == provider_id
                    && self.window.get_provider_models_pending().as_str() == "select" =>
            {
                let presented = present_error(message);
                self.window.set_provider_models_error(presented.into());
                self.window.set_provider_model_selecting(false);
                self.window
                    .set_provider_models_pending(SharedString::default());
            }
            AppCommand::ListHarnessState
            | AppCommand::SelectHarnessProfile { .. }
            | AppCommand::SelectExecutionProfile { .. }
            | AppCommand::SetHarnessCapabilityEnabled { .. } => {
                self.window.set_harness_loading(false);
                self.window.set_harness_pending(SharedString::default());
                self.window.set_harness_error(present_error(message).into());
            }
            AppCommand::SetSurfaceVisibility { surface, visible } => {
                self.window.set_harness_loading(false);
                self.window.set_harness_pending(SharedString::default());
                self.window.set_harness_error(present_error(message).into());
                match surface {
                    SurfaceId::Changes => self.window.set_inspector_open(!visible),
                    SurfaceId::Activity => self.window.set_activity_open(!visible),
                    SurfaceId::Trajectory => {
                        self.window.set_trajectory_open(*visible);
                        self.window.set_trajectory_loading(false);
                        self.window
                            .set_trajectory_error(present_error(message).into());
                    }
                    SurfaceId::Conversation
                    | SurfaceId::Plan
                    | SurfaceId::Files
                    | SurfaceId::Terminal
                    | SurfaceId::Preview
                    | SurfaceId::Approvals => {}
                }
            }
            AppCommand::LoadTrajectory { session_id }
                if session_id.is_none() || *session_id == self.active_session_id() =>
            {
                self.window.set_trajectory_loading(false);
                self.window
                    .set_trajectory_error(present_error(message).into());
            }
            AppCommand::SubmitTask { .. } if self.window.get_submission_pending() => {
                self.window.set_submission_pending(false);
                self.window.set_error_message(present_error(message).into());
                self.window.set_run_state("Could not start".into());
                self.window.set_running(false);
            }
            AppCommand::ApproveTool { .. } | AppCommand::RejectTool { .. }
                if self.window.get_approval_response_pending() =>
            {
                self.window.set_approval_response_pending(false);
                self.window.set_error_message(present_error(message).into());
            }
            AppCommand::SetGoal { .. }
            | AppCommand::PauseGoal
            | AppCommand::ResumeGoal
            | AppCommand::ClearGoal
                if self.window.get_goal_action_pending() =>
            {
                self.window.set_goal_action_pending(false);
                self.window.set_error_message(present_error(message).into());
                self.push_activity("Goal action failed", present_error(message), "error");
            }
            _ => {}
        }
    }

    fn apply_active_runs_snapshot(&self, runs: &[ActiveRunState]) {
        {
            let mut projection = self.projection();
            projection.run_sessions.clear();
            projection.pending_run_sessions.clear();
            for run in runs {
                projection.run_sessions.insert(run.run_id, run.session_id);
            }
        }

        let active_session = self.active_session_id();
        if let Some(run) = runs
            .iter()
            .find(|run| Some(run.session_id) == active_session)
        {
            self.window.set_active_run_id(run.run_id.to_string().into());
            self.window.set_run_state(run.lifecycle.label().into());
            self.window.set_running(is_active_lifecycle(run.lifecycle));
            self.window.set_submission_pending(false);
        } else {
            self.window.set_active_run_id(SharedString::default());
            self.window.set_running(false);
            self.window.set_submission_pending(false);
            if is_active_run_state(self.window.get_run_state().as_str()) {
                self.window.set_run_state("Ready".into());
            }
        }
    }

    fn close_approval(&self) {
        self.window.set_approval_open(false);
        self.window.set_approval_response_pending(false);
        self.window.set_approval_id(SharedString::default());
        self.window.set_approval_action(SharedString::default());
        self.window.set_approval_target(SharedString::default());
        self.window.set_approval_reason(SharedString::default());
    }

    fn apply_provider_profiles(&self, profiles: &[ProviderProfile]) {
        let pending = self.window.get_provider_pending().to_string();
        let previous_id = self.window.get_provider_id().to_string();
        let previous_endpoint = self.window.get_provider_endpoint().to_string();
        let previous_model = self.window.get_provider_model().to_string();
        self.window
            .set_provider_profiles(provider_profile_model(profiles));

        if let Some(profile) = profiles.iter().find(|profile| profile.selected) {
            let execution_target_changed = previous_id != profile.id
                || previous_endpoint != profile.endpoint
                || previous_model != profile.model;
            let profile_changed = previous_id != profile.id
                || previous_endpoint != profile.endpoint
                || previous_model != profile.model
                || matches!(pending.as_str(), "save" | "select");
            self.window.set_provider_id(profile.id.clone().into());
            self.window.set_provider_label(profile.label.clone().into());
            self.window
                .set_provider_kind(provider_kind_id(profile.kind).into());
            self.window
                .set_provider_kind_label(provider_kind_label(profile.kind).into());
            self.window
                .set_provider_endpoint(profile.endpoint.clone().into());
            self.window.set_provider_model(profile.model.clone().into());
            self.window
                .set_provider_draft_label(profile.label.clone().into());
            self.window
                .set_provider_draft_endpoint(profile.endpoint.clone().into());
            self.window
                .set_provider_draft_model(profile.model.clone().into());
            if profile_changed {
                self.window.set_provider_connected(false);
                self.window.set_provider_probe_state("untested".into());
                self.window
                    .set_provider_detail(format!("Configured for {}", profile.model).into());
            }
            if execution_target_changed {
                self.reset_current_model_context();
            }
        } else {
            self.window.set_provider_id(SharedString::default());
            self.window.set_provider_label("No provider".into());
            self.window.set_provider_kind("unknown".into());
            self.window.set_provider_kind_label("Unavailable".into());
            self.window.set_provider_endpoint(SharedString::default());
            self.window.set_provider_model(SharedString::default());
            self.window
                .set_provider_draft_label(SharedString::default());
            self.window
                .set_provider_draft_endpoint(SharedString::default());
            self.window
                .set_provider_draft_model(SharedString::default());
            self.window.set_provider_connected(false);
            self.window.set_provider_probe_state("untested".into());
            self.window
                .set_provider_detail("No profile selected".into());
        }

        self.window
            .set_provider_label_error(SharedString::default());
        self.window
            .set_provider_endpoint_error(SharedString::default());
        self.window
            .set_provider_model_error(SharedString::default());
        self.window.set_provider_error(SharedString::default());
        self.window.set_provider_error_consumes_next(false);
        self.window.set_provider_notice(
            match pending.as_str() {
                "save" => "Profile saved. Test the connection to verify it.",
                "select" => "Profile selected. Test the connection before starting a task.",
                _ => "",
            }
            .into(),
        );
        match pending.as_str() {
            "list" => self.window.set_provider_loading(false),
            "save" => self.window.set_provider_saving(false),
            "select" => self.window.set_provider_selecting(false),
            _ => return,
        }
        self.window.set_provider_pending(SharedString::default());
    }

    fn apply_provider_models(
        &self,
        provider_id: &str,
        models: &[ProviderModel],
        selected_model: &str,
    ) {
        if self.window.get_provider_models_provider_id().as_str() != provider_id {
            return;
        }
        let pending = self.window.get_provider_models_pending().to_string();
        self.window
            .set_provider_models(provider_model_view_model(models, selected_model));
        match pending.as_str() {
            "list" => self.window.set_provider_models_loading(false),
            "select" => self.window.set_provider_model_selecting(false),
            _ => {}
        }
        self.window
            .set_provider_models_error(SharedString::default());

        if pending == "select" && self.window.get_provider_id().as_str() == provider_id {
            self.reset_current_model_context();
            self.window.set_provider_model(selected_model.into());
            self.window.set_provider_draft_model(selected_model.into());
            self.window.set_provider_connected(false);
            self.window.set_provider_probe_state("untested".into());
            self.window
                .set_provider_detail(format!("Configured for {selected_model}").into());
            self.window
                .set_provider_notice("Model updated for this profile.".into());
        }
        if !pending.is_empty() {
            self.window
                .set_provider_models_pending(SharedString::default());
        }
    }

    fn apply_harness_snapshot(&self, snapshot: &HarnessSnapshot) {
        self.window
            .set_harness_profile_id(snapshot.selected_profile_id.clone().into());
        self.window
            .set_harness_profiles(harness_profile_model(&snapshot.profiles));
        self.window
            .set_execution_profile_id(snapshot.selected_execution_profile_id.clone().into());
        self.window
            .set_harness_execution_profiles(harness_execution_profile_model(
                &snapshot.execution_profiles,
            ));
        let selected_label = snapshot
            .execution_profiles
            .iter()
            .find(|profile| profile.selected)
            .map(|profile| profile.label.as_str())
            .unwrap_or("Custom");
        self.window
            .set_execution_profile_label(selected_label.into());
        self.window
            .set_harness_surfaces(harness_surface_model(&snapshot.surfaces));
        self.window
            .set_harness_capabilities(harness_capability_model(&snapshot.capabilities));
        self.window.set_harness_loading(false);
        self.window.set_harness_pending(SharedString::default());
        self.window.set_harness_error(SharedString::default());
    }

    fn apply_trajectory_snapshot(&self, snapshot: &TrajectorySnapshot) {
        if self.active_session_id() != Some(snapshot.session_id) {
            return;
        }
        self.window
            .set_trajectory_items(trajectory_item_model(snapshot));
        self.window
            .set_trajectory_summary(trajectory_summary_label(snapshot.entries.len()).into());
        self.window.set_trajectory_loading(false);
        self.window.set_trajectory_truncated(snapshot.truncated);
        self.window.set_trajectory_error(SharedString::default());
    }

    fn append_trajectory_entry(&self, session_id: SessionId, entry: &TrajectoryEntry) {
        if self.active_session_id() != Some(session_id) {
            return;
        }
        let sequence = entry.sequence.to_string();
        let current = self.window.get_trajectory_items();
        if (0..current.row_count()).any(|row| {
            current
                .row_data(row)
                .is_some_and(|item| item.sequence.as_str() == sequence)
        }) {
            return;
        }
        let mut items = (0..current.row_count())
            .filter_map(|row| current.row_data(row))
            .collect::<Vec<_>>();
        items.push(trajectory_item_view(entry));
        items.sort_by_key(|item| item.sequence.as_str().parse::<u64>().unwrap_or(u64::MAX));
        let truncated =
            self.window.get_trajectory_truncated() || items.len() > MAX_TRAJECTORY_ITEMS;
        if items.len() > MAX_TRAJECTORY_ITEMS {
            items.drain(..items.len() - MAX_TRAJECTORY_ITEMS);
        }
        let count = items.len();
        self.window
            .set_trajectory_items(ModelRc::new(VecModel::from(items)));
        self.window
            .set_trajectory_summary(trajectory_summary_label(count).into());
        self.window.set_trajectory_truncated(truncated);
        self.window.set_trajectory_loading(false);
        self.window.set_trajectory_error(SharedString::default());
    }

    fn reset_session_view(&self) {
        self.window.set_user_prompt(SharedString::default());
        self.window
            .set_conversation_messages(empty_conversation_model());
        self.window.set_draft(SharedString::default());
        self.window.set_transcript(SharedString::default());
        self.window.set_current_reasoning(SharedString::default());
        self.window.set_current_reasoning_available(false);
        self.window
            .set_current_run_provider_label(SharedString::default());
        self.window.set_current_run_model(SharedString::default());
        self.reset_usage_projection();
        self.window.set_error_message(SharedString::default());
        self.window.set_run_state("Ready".into());
        self.window.set_active_run_id(SharedString::default());
        self.window.set_running(false);
        self.window.set_submission_pending(false);
        self.window.set_plan_summary("No stages yet".into());
        self.window.set_plan_steps(empty_plan_model());
        self.window.set_trajectory_items(empty_trajectory_model());
        self.window
            .set_trajectory_summary("No durable records".into());
        self.window.set_trajectory_loading(false);
        self.window.set_trajectory_truncated(false);
        self.window.set_trajectory_error(SharedString::default());
        self.reset_change_and_verification_state();
        self.apply_goal_snapshot(None);
        self.close_approval();
    }

    fn apply_goal_snapshot(&self, goal: Option<&Goal>) {
        self.window.set_goal_available(goal.is_some());
        self.window.set_goal_action_pending(false);
        if let Some(goal) = goal {
            self.window
                .set_goal_objective(goal.objective.clone().into());
            self.window.set_goal_draft(goal.objective.clone().into());
            self.window.set_goal_state(goal.lifecycle.label().into());
            self.window
                .set_goal_next_action(goal.next_action.clone().into());
            self.window
                .set_goal_iteration(saturating_i32_u64(u64::from(goal.iteration)));
            self.window
                .set_goal_tokens_used(saturating_i32_u64(goal.tokens_used));
            self.window
                .set_goal_token_budget_reported(goal.token_budget.is_some());
            self.window
                .set_goal_token_budget(goal.token_budget.map_or(0, saturating_i32_u64));
            self.window
                .set_goal_tools_used(saturating_i32_u64(u64::from(goal.tools_used)));
            self.window
                .set_goal_tool_budget_reported(goal.tool_budget.is_some());
            self.window.set_goal_tool_budget(
                goal.tool_budget
                    .map_or(0, |budget| saturating_i32_u64(u64::from(budget))),
            );
            self.window
                .set_goal_time_budget_reported(goal.time_budget_seconds.is_some());
            self.window.set_goal_time_budget_seconds(
                goal.time_budget_seconds.map_or(0, saturating_i32_u64),
            );
            self.window.set_goal_verification_count(
                goal.verification_history.len().min(i32::MAX as usize) as i32,
            );
        } else {
            self.window.set_goal_objective(SharedString::default());
            self.window.set_goal_draft(SharedString::default());
            self.window.set_goal_state(SharedString::default());
            self.window.set_goal_next_action(SharedString::default());
            self.window.set_goal_iteration(0);
            self.window.set_goal_tokens_used(0);
            self.window.set_goal_token_budget(0);
            self.window.set_goal_token_budget_reported(false);
            self.window.set_goal_tools_used(0);
            self.window.set_goal_tool_budget(0);
            self.window.set_goal_tool_budget_reported(false);
            self.window.set_goal_time_budget_seconds(0);
            self.window.set_goal_time_budget_reported(false);
            self.window.set_goal_verification_count(0);
        }
    }

    fn reset_usage_projection(&self) {
        self.window.set_usage_reported(false);
        self.window.set_usage_input_tokens(0);
        self.window.set_usage_output_tokens(0);
        self.window.set_usage_total_tokens(0);
        self.window.set_context_window_reported(false);
        self.window.set_context_window_tokens(0);
    }

    fn reset_current_model_context(&self) {
        self.window.set_current_reasoning(SharedString::default());
        self.window.set_current_reasoning_available(false);
        self.window
            .set_current_run_provider_label(SharedString::default());
        self.window.set_current_run_model(SharedString::default());
        self.reset_usage_projection();
    }

    fn reset_change_and_verification_state(&self) {
        self.window.set_changed_files(0);
        self.window.set_additions(0);
        self.window.set_removals(0);
        self.window.set_change_summary("No files changed".into());
        self.window.set_verification_state("pending".into());
        self.window
            .set_verification_summary("No automated checks were run".into());
    }

    fn finish_stage_projection(&self, lifecycle: TaskLifecycle) {
        let current = self.window.get_plan_steps();
        let rows = (0..current.row_count())
            .filter_map(|row| current.row_data(row))
            .map(|mut step| {
                if lifecycle == TaskLifecycle::Complete {
                    step.state = "complete".into();
                } else if step.state == "active" {
                    step.state = if lifecycle == TaskLifecycle::Failed {
                        "blocked"
                    } else {
                        "upcoming"
                    }
                    .into();
                }
                step
            })
            .collect::<Vec<_>>();
        self.window
            .set_plan_steps(ModelRc::new(VecModel::from(rows)));
    }

    fn push_activity(
        &self,
        title: impl Into<SharedString>,
        detail: impl Into<SharedString>,
        state: impl Into<SharedString>,
    ) {
        let current = self.window.get_activity_items();
        let start = current
            .row_count()
            .saturating_add(1)
            .saturating_sub(MAX_ACTIVITY_ITEMS);
        let mut items = (start..current.row_count())
            .filter_map(|row| current.row_data(row))
            .collect::<Vec<_>>();
        let title = title.into();
        self.window.set_activity_summary(title.clone());
        items.push(ActivityItemView {
            title,
            detail: detail.into(),
            state: state.into(),
        });
        self.window
            .set_activity_items(ModelRc::new(VecModel::from(items)));
    }
}

impl WeakUiHandle {
    pub fn upgrade(&self) -> Option<UiHandle> {
        self.window.upgrade().map(|window| UiHandle {
            window: Rc::new(window),
            projection: self.projection.clone(),
        })
    }
}

fn empty_plan_model() -> ModelRc<PlanStepView> {
    ModelRc::new(VecModel::from(Vec::<PlanStepView>::new()))
}

fn empty_conversation_model() -> ModelRc<ConversationMessageView> {
    ModelRc::new(VecModel::from(Vec::<ConversationMessageView>::new()))
}

fn empty_trajectory_model() -> ModelRc<TrajectoryItemView> {
    ModelRc::new(VecModel::from(Vec::<TrajectoryItemView>::new()))
}

fn trajectory_item_model(snapshot: &TrajectorySnapshot) -> ModelRc<TrajectoryItemView> {
    let rows = snapshot
        .entries
        .iter()
        .map(trajectory_item_view)
        .collect::<Vec<_>>();
    ModelRc::new(VecModel::from(rows))
}

fn trajectory_item_view(entry: &TrajectoryEntry) -> TrajectoryItemView {
    TrajectoryItemView {
        sequence: entry.sequence.to_string().into(),
        title: entry.title.clone().into(),
        detail: entry.detail.clone().into(),
        state: trajectory_state_id(entry.state).into(),
        kind: trajectory_kind_id(entry.kind).into(),
        timestamp: entry
            .occurred_at
            .format(format_description!(
                "[year]-[month]-[day] [hour]:[minute] UTC"
            ))
            .unwrap_or_else(|_| entry.occurred_at.unix_timestamp().to_string())
            .into(),
    }
}

fn trajectory_summary_label(count: usize) -> String {
    match count {
        0 => "No durable records".to_owned(),
        1 => "1 durable record".to_owned(),
        _ => format!("{count} durable records"),
    }
}

const fn trajectory_kind_id(kind: TrajectoryKind) -> &'static str {
    match kind {
        TrajectoryKind::Task => "task",
        TrajectoryKind::Plan => "plan",
        TrajectoryKind::Lifecycle => "lifecycle",
        TrajectoryKind::Tool => "tool",
        TrajectoryKind::Approval => "approval",
        TrajectoryKind::Usage => "usage",
        TrajectoryKind::Change => "change",
        TrajectoryKind::Verification => "verification",
        TrajectoryKind::Error => "error",
    }
}

const fn trajectory_state_id(state: TrajectoryState) -> &'static str {
    match state {
        TrajectoryState::Neutral => "neutral",
        TrajectoryState::Active => "active",
        TrajectoryState::Attention => "attention",
        TrajectoryState::Success => "success",
        TrajectoryState::Error => "error",
        TrajectoryState::Cancelled => "cancelled",
    }
}

fn conversation_message_model(
    messages: &[argentum_domain::ConversationMessage],
) -> ModelRc<ConversationMessageView> {
    let rows = messages
        .iter()
        .map(|message| {
            let usage = message.usage.as_ref();
            let (text, tagged_reasoning) = match message.role {
                ConversationRole::User => (message.text.clone(), String::new()),
                ConversationRole::Assistant => split_tagged_reasoning(&message.text),
            };
            let mut reasoning = without_reasoning_markers(&message.reasoning);
            if !tagged_reasoning.is_empty() && !reasoning.contains(&tagged_reasoning) {
                if !reasoning.is_empty() {
                    reasoning.push_str("\n\n");
                }
                reasoning.push_str(&tagged_reasoning);
            }
            ConversationMessageView {
                role: match message.role {
                    ConversationRole::User => "user",
                    ConversationRole::Assistant => "assistant",
                }
                .into(),
                text: text.into(),
                reasoning: reasoning.clone().into(),
                reasoning_available: message.role == ConversationRole::Assistant
                    && !reasoning.is_empty(),
                provider_label: message.profile_id.clone().into(),
                model: message.model.clone().into(),
                usage_reported: usage.is_some(),
                input_tokens: usage.map_or(0, |usage| saturating_i32_u64(usage.input_tokens)),
                output_tokens: usage.map_or(0, |usage| saturating_i32_u64(usage.output_tokens)),
                total_tokens: usage.map_or(0, |usage| saturating_i32_u64(usage.total_tokens)),
                context_window_reported: usage
                    .and_then(|usage| usage.context_window_tokens)
                    .is_some(),
                context_window_tokens: usage
                    .and_then(|usage| usage.context_window_tokens)
                    .map_or(0, saturating_i32_u64),
                state: match message.status {
                    ConversationMessageStatus::Complete => "Complete",
                    ConversationMessageStatus::Interrupted => "Interrupted",
                    ConversationMessageStatus::Failed => "Failed",
                }
                .into(),
            }
        })
        .collect::<Vec<_>>();
    ModelRc::new(VecModel::from(rows))
}

fn split_tagged_reasoning(text: &str) -> (String, String) {
    const OPEN: &str = "<think>";
    const CLOSE: &str = "</think>";

    let mut remaining = text;
    let mut visible = String::new();
    let mut reasoning = String::new();
    while let Some(open_index) = remaining.find(OPEN) {
        visible.push_str(&remaining[..open_index]);
        let hidden = &remaining[open_index + OPEN.len()..];
        if !reasoning.is_empty() {
            reasoning.push_str("\n\n");
        }
        let Some(close_index) = hidden.find(CLOSE) else {
            reasoning.push_str(hidden);
            remaining = "";
            break;
        };
        reasoning.push_str(&hidden[..close_index]);
        remaining = &hidden[close_index + CLOSE.len()..];
    }
    visible.push_str(&remaining.replace(CLOSE, ""));
    (visible, reasoning)
}

fn without_reasoning_markers(text: &str) -> String {
    text.replace("<think>", "").replace("</think>", "")
}

fn session_item_model(snapshot: &argentum_domain::WorkspaceSnapshot) -> ModelRc<SessionItemView> {
    let rows = snapshot
        .sessions
        .iter()
        .rev()
        .map(|session| {
            let active = Some(session.id) == snapshot.active_session_id;
            SessionItemView {
                id: session.id.to_string().into(),
                title: session.title.clone().into(),
                detail: if active { "Current" } else { "Saved session" }.into(),
                active,
            }
        })
        .collect::<Vec<_>>();
    ModelRc::new(VecModel::from(rows))
}

fn provider_profile_model(profiles: &[ProviderProfile]) -> ModelRc<ProviderProfileView> {
    let rows = profiles
        .iter()
        .map(|profile| ProviderProfileView {
            id: profile.id.clone().into(),
            label: profile.label.clone().into(),
            kind: provider_kind_id(profile.kind).into(),
            kind_label: provider_kind_label(profile.kind).into(),
            endpoint: profile.endpoint.clone().into(),
            model: profile.model.clone().into(),
            selected: profile.selected,
        })
        .collect::<Vec<_>>();
    ModelRc::new(VecModel::from(rows))
}

fn provider_model_view_model(
    models: &[ProviderModel],
    selected_model: &str,
) -> ModelRc<ProviderModelView> {
    let rows = models
        .iter()
        .map(|model| ProviderModelView {
            id: model.id.clone().into(),
            label: model.label.clone().into(),
            context_window_tokens: model.context_window_tokens.map_or(0, saturating_i32_u64),
            context_window_reported: model.context_window_tokens.is_some(),
            selected: model.id == selected_model,
        })
        .collect::<Vec<_>>();
    ModelRc::new(VecModel::from(rows))
}

fn harness_profile_model(profiles: &[HarnessProfileSummary]) -> ModelRc<HarnessProfileView> {
    let rows = profiles
        .iter()
        .map(|profile| HarnessProfileView {
            id: profile.id.clone().into(),
            label: profile.label.clone().into(),
            detail: profile.detail.clone().into(),
            selected: profile.selected,
            selectable: profile.selectable,
        })
        .collect::<Vec<_>>();
    ModelRc::new(VecModel::from(rows))
}

fn harness_execution_profile_model(
    profiles: &[HarnessExecutionProfileSummary],
) -> ModelRc<HarnessExecutionProfileView> {
    let rows = profiles
        .iter()
        .map(|profile| HarnessExecutionProfileView {
            id: profile.id.clone().into(),
            label: profile.label.clone().into(),
            detail: profile.detail.clone().into(),
            selected: profile.selected,
            selectable: profile.selectable,
        })
        .collect::<Vec<_>>();
    ModelRc::new(VecModel::from(rows))
}

fn harness_surface_model(surfaces: &[HarnessSurfaceState]) -> ModelRc<HarnessSurfaceView> {
    let rows = surfaces
        .iter()
        .map(|surface| HarnessSurfaceView {
            id: surface_id(surface.id).into(),
            label: surface.label.clone().into(),
            state: harness_surface_state(surface).into(),
            detail: if surface.unavailable_reason.is_empty() {
                surface.detail.clone()
            } else {
                surface.unavailable_reason.clone()
            }
            .into(),
            visible: surface.visible,
            configurable: surface.configurable,
        })
        .collect::<Vec<_>>();
    ModelRc::new(VecModel::from(rows))
}

fn harness_capability_model(
    capabilities: &[HarnessCapabilityState],
) -> ModelRc<HarnessCapabilityView> {
    let rows = capabilities
        .iter()
        .map(|capability| HarnessCapabilityView {
            id: capability.id.clone().into(),
            label: capability.label.clone().into(),
            state: harness_capability_state(capability).into(),
            detail: if capability.unavailable_reason.is_empty() {
                capability.detail.clone()
            } else {
                capability.unavailable_reason.clone()
            }
            .into(),
            enabled: capability.enabled,
            configurable: capability.configurable,
        })
        .collect::<Vec<_>>();
    ModelRc::new(VecModel::from(rows))
}

fn plan_step_model(steps: &[argentum_domain::PlanStep]) -> ModelRc<PlanStepView> {
    let active_index = steps
        .iter()
        .position(|step| !step.complete && !step.blocked);
    let rows = steps
        .iter()
        .enumerate()
        .map(|(index, step)| PlanStepView {
            index: i32::try_from(index.saturating_add(1)).unwrap_or(i32::MAX),
            title: step.title.clone().into(),
            detail: step.detail.clone().into(),
            state: if step.blocked {
                "blocked"
            } else if step.complete {
                "complete"
            } else if active_index == Some(index) {
                "active"
            } else {
                "upcoming"
            }
            .into(),
        })
        .collect::<Vec<_>>();
    ModelRc::new(VecModel::from(rows))
}

fn stage_count_label(count: usize) -> String {
    match count {
        0 => "No stages".into(),
        1 => "1 stage".into(),
        _ => format!("{count} stages"),
    }
}

const fn is_active_lifecycle(lifecycle: TaskLifecycle) -> bool {
    matches!(
        lifecycle,
        TaskLifecycle::Queued
            | TaskLifecycle::Planning
            | TaskLifecycle::Running
            | TaskLifecycle::WaitingForApproval
            | TaskLifecycle::Reviewing
            | TaskLifecycle::Verifying
    )
}

fn is_active_run_state(label: &str) -> bool {
    matches!(
        label,
        "Queued" | "Planning" | "Running" | "Waiting for approval" | "Reviewing" | "Verifying"
    )
}

const fn lifecycle_activity_state(lifecycle: TaskLifecycle) -> &'static str {
    match lifecycle {
        TaskLifecycle::Complete => "success",
        TaskLifecycle::Failed | TaskLifecycle::Cancelled => "error",
        TaskLifecycle::WaitingForApproval => "attention",
        TaskLifecycle::Queued
        | TaskLifecycle::Planning
        | TaskLifecycle::Running
        | TaskLifecycle::Reviewing
        | TaskLifecycle::Verifying => "active",
        TaskLifecycle::Draft | TaskLifecycle::Paused => "neutral",
    }
}

fn tool_result_presentation(result: &ToolResultState) -> (&'static str, &'static str) {
    match result {
        ToolResultState::Running => ("is running", "active"),
        ToolResultState::Succeeded => ("completed", "success"),
        ToolResultState::Failed => ("failed", "error"),
        ToolResultState::Cancelled => ("cancelled", "neutral"),
    }
}

fn saturating_i32(value: u32) -> i32 {
    i32::try_from(value).unwrap_or(i32::MAX)
}

fn saturating_i32_u64(value: u64) -> i32 {
    i32::try_from(value).unwrap_or(i32::MAX)
}

fn change_summary(files_changed: u32, additions: u32, removals: u32) -> String {
    if files_changed == 0 {
        return "No files changed".into();
    }
    let file_label = if files_changed == 1 { "file" } else { "files" };
    let _ = (additions, removals);
    format!("{files_changed} {file_label} changed")
}

const fn lifecycle_activity_title(lifecycle: TaskLifecycle) -> &'static str {
    match lifecycle {
        TaskLifecycle::Draft => "Task drafted",
        TaskLifecycle::Queued => "Task queued",
        TaskLifecycle::Planning => "Preparation started",
        TaskLifecycle::Running => "Run started",
        TaskLifecycle::WaitingForApproval => "Approval needed",
        TaskLifecycle::Reviewing => "Review started",
        TaskLifecycle::Verifying => "Checks started",
        TaskLifecycle::Complete => "Run completed",
        TaskLifecycle::Paused => "Run paused",
        TaskLifecycle::Cancelled => "Run cancelled",
        TaskLifecycle::Failed => "Run failed",
    }
}

fn tool_label(tool_id: &str) -> &str {
    match tool_id {
        "read_text" => "Read file",
        "write_text" => "Write file",
        _ => "Tool",
    }
}

fn present_error(message: &str) -> String {
    if message == "no provider is configured" {
        return "No model provider is configured.".into();
    }
    if message.contains("provider request failed")
        || message.contains("provider stream")
        || message.contains("connection")
    {
        return "Argentum could not reach the model provider. Check that it is running, then try again.".into();
    }
    if message.contains("outside the workspace boundary") {
        return "That path is outside the current workspace.".into();
    }

    let mut presented = message.trim().to_string();
    if presented.is_empty() {
        return "Argentum could not continue.".into();
    }
    if let Some(first) = presented.get_mut(0..1) {
        first.make_ascii_uppercase();
    }
    if !presented.ends_with(['.', '!', '?']) {
        presented.push('.');
    }
    presented
}

const fn provider_kind_id(kind: ProviderKind) -> &'static str {
    match kind {
        ProviderKind::OpenAiCompatible => "openai-compatible",
        ProviderKind::Anthropic => "anthropic",
        ProviderKind::LocalLmStudio => "lm-studio",
        ProviderKind::Unknown => "unknown",
    }
}

const fn provider_kind_label(kind: ProviderKind) -> &'static str {
    match kind {
        ProviderKind::OpenAiCompatible => "OpenAI compatible",
        ProviderKind::Anthropic => "Anthropic",
        ProviderKind::LocalLmStudio => "LM Studio",
        ProviderKind::Unknown => "Unavailable",
    }
}

fn provider_kind_from_id(kind: &str) -> Option<ProviderKind> {
    match kind {
        "openai-compatible" => Some(ProviderKind::OpenAiCompatible),
        "anthropic" => Some(ProviderKind::Anthropic),
        "lm-studio" => Some(ProviderKind::LocalLmStudio),
        _ => None,
    }
}

fn provider_status_id(status: &argentum_domain::ProviderStatus) -> String {
    let profile_id = status.profile_id.trim();
    if profile_id.is_empty() {
        provider_kind_id(status.kind).into()
    } else {
        profile_id.into()
    }
}

const fn surface_id(surface: SurfaceId) -> &'static str {
    match surface {
        SurfaceId::Conversation => "conversation",
        SurfaceId::Plan => "plan",
        SurfaceId::Changes => "changes",
        SurfaceId::Files => "files",
        SurfaceId::Terminal => "terminal",
        SurfaceId::Preview => "preview",
        SurfaceId::Activity => "activity",
        SurfaceId::Trajectory => "trajectory",
        SurfaceId::Approvals => "approvals",
    }
}

fn surface_from_id(surface: &str) -> Option<SurfaceId> {
    match surface {
        "conversation" => Some(SurfaceId::Conversation),
        "plan" => Some(SurfaceId::Plan),
        "changes" => Some(SurfaceId::Changes),
        "files" => Some(SurfaceId::Files),
        "terminal" => Some(SurfaceId::Terminal),
        "preview" => Some(SurfaceId::Preview),
        "activity" => Some(SurfaceId::Activity),
        "trajectory" => Some(SurfaceId::Trajectory),
        "approvals" => Some(SurfaceId::Approvals),
        _ => None,
    }
}

fn harness_surface_state(surface: &HarnessSurfaceState) -> &'static str {
    match surface.availability {
        HarnessAvailability::Unavailable => "unavailable",
        HarnessAvailability::Available if surface.visible => "visible",
        HarnessAvailability::Available if surface.configurable => "hidden",
        HarnessAvailability::Available => "automatic",
    }
}

fn harness_capability_state(capability: &HarnessCapabilityState) -> &'static str {
    match capability.availability {
        HarnessAvailability::Unavailable => "unavailable",
        HarnessAvailability::Available if !capability.enabled => "disabled",
        HarnessAvailability::Available => match capability.readiness {
            HarnessReadiness::Ready => "ready",
            HarnessReadiness::NeedsConfiguration => "needs configuration",
            HarnessReadiness::NotVerified => "not verified",
            HarnessReadiness::Blocked => "blocked",
            HarnessReadiness::Unavailable => "unavailable",
        },
    }
}

fn provider_error_title(operation: &str) -> &'static str {
    match operation {
        "list" => "Could not load providers",
        "save" => "Could not save provider",
        "select" => "Could not select provider",
        "probe" => "Connection failed",
        _ => "Provider command failed",
    }
}

fn validate_provider_label(value: &str) -> String {
    validate_printable_field(value, 80, "Enter a profile name.", "Profile name")
}

fn validate_provider_model(value: &str) -> String {
    validate_printable_field(value, 256, "Enter a model name.", "Model name")
}

fn validate_printable_field(
    value: &str,
    max_characters: usize,
    empty_message: &str,
    field_name: &str,
) -> String {
    let value = value.trim();
    if value.is_empty() {
        return empty_message.into();
    }
    if value.chars().any(char::is_control) || value.chars().count() > max_characters {
        return format!("{field_name} must use 1 to {max_characters} printable characters.");
    }
    String::new()
}

fn validate_provider_endpoint(value: &str) -> String {
    let value = value.trim();
    if value.is_empty() {
        return "Enter an endpoint.".into();
    }
    if value.chars().any(char::is_control) {
        return "Endpoint cannot contain control characters.".into();
    }
    let Ok(endpoint) = url::Url::parse(value) else {
        return "Enter a valid http or https endpoint.".into();
    };
    if !matches!(endpoint.scheme(), "http" | "https") || endpoint.host_str().is_none() {
        return "Enter a valid http or https endpoint.".into();
    }
    if !endpoint.username().is_empty()
        || endpoint.password().is_some()
        || endpoint.query().is_some()
        || endpoint.fragment().is_some()
    {
        return "Endpoint cannot include credentials, a query, or a fragment.".into();
    }
    String::new()
}

pub fn connect_commands<F>(window: &MainWindow, dispatch: F)
where
    F: Fn(AppCommand) + 'static,
{
    let dispatch = Rc::new(dispatch);

    let submit_dispatch = dispatch.clone();
    let submit_window = window.as_weak();
    window.on_submit_task(move |prompt| {
        let Some(window) = submit_window.upgrade() else {
            return;
        };
        if window.get_submission_pending() {
            return;
        }
        window.set_submission_pending(true);
        submit_dispatch(AppCommand::SubmitTask {
            prompt: prompt.to_string(),
        });
    });

    let cancel_dispatch = dispatch.clone();
    window.on_cancel_run(move |run_id| {
        let Ok(run_id) = run_id.to_string().parse() else {
            warn!("cancel requested without a valid run id");
            return;
        };
        cancel_dispatch(AppCommand::CancelRun { run_id });
    });

    let new_dispatch = dispatch.clone();
    window.on_new_session(move || {
        new_dispatch(AppCommand::NewSession);
    });

    let select_dispatch = dispatch.clone();
    window.on_select_session(move |session_id| {
        let Ok(session_id) = session_id.to_string().parse() else {
            warn!("session selection requested without a valid session id");
            return;
        };
        select_dispatch(AppCommand::SelectSession { session_id });
    });

    let set_goal_dispatch = dispatch.clone();
    let set_goal_window = window.as_weak();
    window.on_set_goal(
        move |objective, token_budget, tool_budget, time_budget_seconds| {
            let Some(window) = set_goal_window.upgrade() else {
                return;
            };
            if window.get_running() || window.get_goal_action_pending() {
                warn!("goal update ignored while a run or goal action is active");
                return;
            }
            let objective = objective.trim().to_string();
            if objective.is_empty() {
                window.set_error_message("Enter a goal objective.".into());
                return;
            }
            window.set_goal_action_pending(true);
            set_goal_dispatch(AppCommand::SetGoal {
                objective,
                token_budget: (token_budget >= 0).then_some(token_budget as u64),
                tool_budget: (tool_budget >= 0).then_some(tool_budget as u32),
                time_budget_seconds: (time_budget_seconds >= 0)
                    .then_some(time_budget_seconds as u64),
            });
        },
    );

    let pause_goal_dispatch = dispatch.clone();
    let pause_goal_window = window.as_weak();
    window.on_pause_goal(move || {
        let Some(window) = pause_goal_window.upgrade() else {
            return;
        };
        if window.get_running() || window.get_goal_action_pending() {
            return;
        }
        window.set_goal_action_pending(true);
        pause_goal_dispatch(AppCommand::PauseGoal);
    });

    let resume_goal_dispatch = dispatch.clone();
    let resume_goal_window = window.as_weak();
    window.on_resume_goal(move || {
        let Some(window) = resume_goal_window.upgrade() else {
            return;
        };
        if window.get_running() || window.get_goal_action_pending() {
            return;
        }
        window.set_goal_action_pending(true);
        resume_goal_dispatch(AppCommand::ResumeGoal);
    });

    let clear_goal_dispatch = dispatch.clone();
    let clear_goal_window = window.as_weak();
    window.on_clear_goal(move || {
        let Some(window) = clear_goal_window.upgrade() else {
            return;
        };
        if window.get_running() || window.get_goal_action_pending() {
            return;
        }
        window.set_goal_action_pending(true);
        clear_goal_dispatch(AppCommand::ClearGoal);
    });

    let probe_dispatch = dispatch.clone();
    let probe_window = window.as_weak();
    window.on_probe_provider(move |provider_id| {
        let Some(window) = probe_window.upgrade() else {
            return;
        };
        if window.get_running() {
            warn!("provider probe ignored while a run is active");
            return;
        }
        let provider_id = provider_id.trim().to_string();
        if provider_id.is_empty() {
            warn!("provider probe requested without a provider id");
            return;
        }
        probe_dispatch(AppCommand::ProbeProvider { provider_id });
    });

    let save_provider_dispatch = dispatch.clone();
    let save_provider_window = window.as_weak();
    window.on_save_provider_profile(move |provider_id, label, kind, endpoint, model, selected| {
        let Some(window) = save_provider_window.upgrade() else {
            return;
        };
        if window.get_running() {
            warn!("provider save ignored while a run is active");
            return;
        }
        let kind = provider_kind_from_id(kind.as_str()).unwrap_or(ProviderKind::Unknown);
        save_provider_dispatch(AppCommand::SaveProviderProfile {
            profile: ProviderProfile {
                id: provider_id.to_string(),
                label: label.to_string(),
                kind,
                endpoint: endpoint.to_string(),
                model: model.to_string(),
                selected,
            },
        });
    });

    let select_provider_dispatch = dispatch.clone();
    let select_provider_window = window.as_weak();
    window.on_select_provider_profile(move |provider_id| {
        let Some(window) = select_provider_window.upgrade() else {
            return;
        };
        if window.get_running() {
            warn!("provider selection ignored while a run is active");
            return;
        }
        select_provider_dispatch(AppCommand::SelectProviderProfile {
            provider_id: provider_id.to_string(),
        });
    });

    let refresh_models_dispatch = dispatch.clone();
    let refresh_models_window = window.as_weak();
    window.on_refresh_provider_models(move |provider_id| {
        let Some(window) = refresh_models_window.upgrade() else {
            return;
        };
        if window.get_running() {
            warn!("model catalog refresh ignored while a run is active");
            return;
        }
        let provider_id = provider_id.trim().to_string();
        if provider_id.is_empty() {
            warn!("model refresh requested without a provider id");
            return;
        }
        refresh_models_dispatch(AppCommand::ListProviderModels { provider_id });
    });

    let select_model_dispatch = dispatch.clone();
    let select_model_window = window.as_weak();
    window.on_select_provider_model(move |provider_id, model| {
        let Some(window) = select_model_window.upgrade() else {
            return;
        };
        if window.get_running() {
            warn!("model selection ignored while a run is active");
            return;
        }
        let provider_id = provider_id.trim().to_string();
        let model = model.trim().to_string();
        if provider_id.is_empty() || model.is_empty() {
            warn!("model selection requested without a provider id or model");
            return;
        }
        select_model_dispatch(AppCommand::SelectProviderModel { provider_id, model });
    });

    let refresh_harness_dispatch = dispatch.clone();
    let refresh_harness_window = window.as_weak();
    window.on_refresh_harness(move || {
        let Some(window) = refresh_harness_window.upgrade() else {
            return;
        };
        if window.get_harness_loading() || !window.get_harness_pending().is_empty() {
            return;
        }
        window.set_harness_loading(true);
        window.set_harness_pending("list".into());
        window.set_harness_error(SharedString::default());
        refresh_harness_dispatch(AppCommand::ListHarnessState);
    });

    let select_harness_dispatch = dispatch.clone();
    let select_harness_window = window.as_weak();
    window.on_select_harness_profile(move |profile_id| {
        let Some(window) = select_harness_window.upgrade() else {
            return;
        };
        let profile_id = profile_id.trim().to_string();
        if !window.get_harness_pending().is_empty()
            || profile_id.is_empty()
            || profile_id == "custom"
        {
            return;
        }
        window.set_harness_pending("profile".into());
        window.set_harness_error(SharedString::default());
        select_harness_dispatch(AppCommand::SelectHarnessProfile { profile_id });
    });

    let surface_visibility_dispatch = dispatch.clone();
    let surface_visibility_window = window.as_weak();
    window.on_set_harness_surface_visibility(move |surface, visible| {
        let Some(window) = surface_visibility_window.upgrade() else {
            return;
        };
        if !window.get_harness_pending().is_empty() {
            return;
        }
        let Some(surface) = surface_from_id(surface.trim()) else {
            warn!("harness surface visibility requested with an invalid surface id");
            window.set_harness_pending(SharedString::default());
            window.set_harness_error("Unknown harness surface.".into());
            return;
        };
        window.set_harness_pending("surface".into());
        window.set_harness_error(SharedString::default());
        surface_visibility_dispatch(AppCommand::SetSurfaceVisibility { surface, visible });
    });

    let execution_profile_dispatch = dispatch.clone();
    let execution_profile_window = window.as_weak();
    window.on_select_execution_profile(move |profile_id| {
        let Some(window) = execution_profile_window.upgrade() else {
            return;
        };
        let profile_id = profile_id.trim().to_string();
        if window.get_running()
            || window.get_approval_open()
            || !window.get_harness_pending().is_empty()
            || profile_id.is_empty()
            || profile_id == "custom"
        {
            return;
        }
        window.set_harness_pending("execution".into());
        window.set_harness_error(SharedString::default());
        execution_profile_dispatch(AppCommand::SelectExecutionProfile { profile_id });
    });

    let capability_dispatch = dispatch.clone();
    let capability_window = window.as_weak();
    window.on_set_harness_capability_enabled(move |capability_id, enabled| {
        let Some(window) = capability_window.upgrade() else {
            return;
        };
        let capability_id = capability_id.trim().to_string();
        if window.get_running()
            || window.get_approval_open()
            || !window.get_harness_pending().is_empty()
            || capability_id.is_empty()
        {
            return;
        }
        window.set_harness_pending("capability".into());
        window.set_harness_error(SharedString::default());
        capability_dispatch(AppCommand::SetHarnessCapabilityEnabled {
            capability_id,
            enabled,
        });
    });

    window.on_validate_provider_label(|value| validate_provider_label(value.as_str()).into());
    window.on_validate_provider_endpoint(|value| validate_provider_endpoint(value.as_str()).into());
    window.on_validate_provider_model(|value| validate_provider_model(value.as_str()).into());

    let drawer_dispatch = dispatch.clone();
    window.on_toggle_drawer(move || {
        let _ = &drawer_dispatch;
    });

    let inspector_dispatch = dispatch.clone();
    let inspector_window = window.as_weak();
    window.on_toggle_inspector(move || {
        let Some(window) = inspector_window.upgrade() else {
            return;
        };
        inspector_dispatch(AppCommand::SetSurfaceVisibility {
            surface: SurfaceId::Changes,
            visible: window.get_inspector_open(),
        });
    });

    let trajectory_dispatch = dispatch.clone();
    let trajectory_window = window.as_weak();
    window.on_toggle_trajectory(move || {
        let Some(window) = trajectory_window.upgrade() else {
            return;
        };
        trajectory_dispatch(AppCommand::SetSurfaceVisibility {
            surface: SurfaceId::Trajectory,
            visible: window.get_trajectory_open(),
        });
    });

    let refresh_trajectory_dispatch = dispatch.clone();
    let refresh_trajectory_window = window.as_weak();
    window.on_refresh_trajectory(move |session_id| {
        let Some(window) = refresh_trajectory_window.upgrade() else {
            return;
        };
        let Ok(session_id) = session_id.trim().parse::<SessionId>() else {
            window.set_trajectory_loading(false);
            window.set_trajectory_error("Invalid session identifier.".into());
            return;
        };
        refresh_trajectory_dispatch(AppCommand::LoadTrajectory {
            session_id: Some(session_id),
        });
    });

    let activity_dispatch = dispatch.clone();
    let activity_window = window.as_weak();
    window.on_toggle_activity(move || {
        let Some(window) = activity_window.upgrade() else {
            return;
        };
        activity_dispatch(AppCommand::SetSurfaceVisibility {
            surface: SurfaceId::Activity,
            visible: window.get_activity_open(),
        });
    });

    let approve_dispatch = dispatch.clone();
    let approve_window = window.as_weak();
    window.on_approve_tool(move |approval_id| {
        let Ok(approval_id) = approval_id.to_string().parse() else {
            warn!("approval requested without a valid approval id");
            return;
        };
        let Some(window) = approve_window.upgrade() else {
            return;
        };
        if window.get_approval_response_pending() {
            return;
        }
        window.set_approval_response_pending(true);
        approve_dispatch(AppCommand::ApproveTool {
            approval_id,
            scope: argentum_domain::ApprovalScope::Once,
        });
    });

    let reject_dispatch = dispatch.clone();
    let reject_window = window.as_weak();
    window.on_reject_tool(move |approval_id| {
        let Ok(approval_id) = approval_id.to_string().parse() else {
            warn!("rejection requested without a valid approval id");
            return;
        };
        let Some(window) = reject_window.upgrade() else {
            return;
        };
        if window.get_approval_response_pending() {
            return;
        }
        window.set_approval_response_pending(true);
        reject_dispatch(AppCommand::RejectTool { approval_id });
    });

    let settings_dispatch = dispatch.clone();
    let settings_window = window.as_weak();
    window.on_open_settings(move || {
        if let Some(window) = settings_window.upgrade() {
            window.set_harness_loading(true);
            window.set_harness_pending("list".into());
            window.set_harness_error(SharedString::default());
        }
        settings_dispatch(AppCommand::ListProviderProfiles);
        settings_dispatch(AppCommand::ListHarnessState);
    });
}

pub fn empty_string_model() -> ModelRc<SharedString> {
    ModelRc::new(VecModel::from(Vec::<SharedString>::new()))
}

#[cfg(test)]
mod tests {
    use argentum_domain::{
        now, ConversationMessage, ConversationMessageStatus, ConversationRole,
        ConversationSnapshot, Goal, GoalLifecycle, HarnessAvailability, HarnessCapabilityKind,
        HarnessCapabilityState, HarnessProfileSummary, HarnessReadiness, HarnessSnapshot,
        HarnessSurfaceState, PlanStep, Project, ProviderProfile, SessionSummary, Task,
        TaskLifecycle, ToolResultState, WorkspaceSnapshot,
    };
    use slint::Model;
    use std::cell::RefCell;
    use std::path::PathBuf;
    use std::rc::Rc;

    use super::*;

    #[test]
    fn plan_projection_marks_only_the_first_pending_step_active() {
        let model = plan_step_model(&[
            PlanStep {
                id: "done".into(),
                title: "Done".into(),
                detail: String::new(),
                complete: true,
                blocked: false,
            },
            PlanStep {
                id: "current".into(),
                title: "Current".into(),
                detail: String::new(),
                complete: false,
                blocked: false,
            },
            PlanStep {
                id: "later".into(),
                title: "Later".into(),
                detail: String::new(),
                complete: false,
                blocked: false,
            },
        ]);

        assert_eq!(model.row_data(0).expect("first step").state, "complete");
        assert_eq!(model.row_data(1).expect("second step").state, "active");
        assert_eq!(model.row_data(2).expect("third step").state, "upcoming");
    }

    #[test]
    fn trajectory_projection_preserves_exact_factual_labels() {
        let session_id = SessionId::new_v4();
        let snapshot = TrajectorySnapshot {
            session_id,
            entries: vec![argentum_domain::TrajectoryEntry {
                sequence: 4,
                run_id: Some(RunId::new_v4()),
                kind: TrajectoryKind::Usage,
                state: TrajectoryState::Neutral,
                title: "Model usage recorded".into(),
                detail: "deepseek | deepseek-chat | 12 input | 7 output | 19 total".into(),
                occurred_at: now(),
            }],
            truncated: false,
        };

        let model = trajectory_item_model(&snapshot);
        let row = model.row_data(0).expect("trajectory row");
        assert_eq!(row.kind, "usage");
        assert_eq!(row.state, "neutral");
        assert_eq!(row.title, "Model usage recorded");
        assert!(row.detail.contains("12 input"));
        assert!(row.timestamp.ends_with("UTC"));
    }

    #[test]
    fn session_projection_is_newest_first_and_marks_the_active_row() {
        let project_id = "00000000-0000-0000-0000-000000000081"
            .parse()
            .expect("project id");
        let older_id = "00000000-0000-0000-0000-000000000082"
            .parse()
            .expect("older id");
        let newer_id = "00000000-0000-0000-0000-000000000083"
            .parse()
            .expect("newer id");
        let timestamp = now();
        let snapshot = WorkspaceSnapshot {
            project: Project {
                id: project_id,
                name: "Workspace".into(),
                workspace_root: PathBuf::from("workspace"),
                created_at: timestamp,
            },
            sessions: vec![
                SessionSummary {
                    id: older_id,
                    title: "Older".into(),
                    created_at: timestamp,
                    updated_at: timestamp,
                },
                SessionSummary {
                    id: newer_id,
                    title: "Newer".into(),
                    created_at: timestamp,
                    updated_at: timestamp,
                },
            ],
            active_session_id: Some(older_id),
        };

        let model = session_item_model(&snapshot);

        assert_eq!(model.row_data(0).expect("newest session").title, "Newer");
        assert!(!model.row_data(0).expect("newest session").active);
        assert_eq!(model.row_data(1).expect("active session").title, "Older");
        assert!(model.row_data(1).expect("active session").active);
    }

    #[test]
    fn active_run_states_include_approval_and_verification() {
        assert!(is_active_lifecycle(TaskLifecycle::WaitingForApproval));
        assert!(is_active_lifecycle(TaskLifecycle::Verifying));
        assert!(!is_active_lifecycle(TaskLifecycle::Complete));
    }

    #[test]
    fn provider_kinds_map_to_stable_ids() {
        assert_eq!(provider_kind_id(ProviderKind::LocalLmStudio), "lm-studio");
        assert_eq!(
            provider_kind_id(ProviderKind::OpenAiCompatible),
            "openai-compatible"
        );
        assert_eq!(provider_kind_id(ProviderKind::Anthropic), "anthropic");
        assert_eq!(provider_kind_id(ProviderKind::Unknown), "unknown");
    }

    #[test]
    fn event_and_callback_projections_share_one_ui_instance() {
        let ui = UiHandle::new().expect("create UI projection");

        let goal_session_id: SessionId = "00000000-0000-0000-0000-0000000000c1"
            .parse()
            .expect("goal session id");
        ui.window()
            .set_active_session_id(goal_session_id.to_string().into());
        ui.apply_event(&AppEvent::GoalSnapshotLoaded {
            session_id: goal_session_id,
            goal: Some(Goal {
                id: "00000000-0000-0000-0000-0000000000c2"
                    .parse()
                    .expect("goal id"),
                project_id: "00000000-0000-0000-0000-0000000000c3"
                    .parse()
                    .expect("goal project id"),
                session_id: goal_session_id,
                objective: "Ship the bounded slice".into(),
                lifecycle: GoalLifecycle::Paused,
                token_budget: Some(4_000),
                tool_budget: Some(8),
                time_budget_seconds: Some(1_800),
                tokens_used: 640,
                tools_used: 2,
                iteration: 3,
                next_action: "Resume the goal when ready".into(),
                verification_history: Vec::new(),
                created_at: now(),
                updated_at: now(),
            }),
        });
        assert!(ui.window().get_goal_available());
        assert_eq!(ui.window().get_goal_objective(), "Ship the bounded slice");
        assert_eq!(ui.window().get_goal_state(), "Paused");
        assert_eq!(ui.window().get_goal_iteration(), 3);
        assert_eq!(ui.window().get_goal_token_budget(), 4_000);
        assert_eq!(ui.window().get_goal_tokens_used(), 640);
        ui.apply_event(&AppEvent::GoalSnapshotLoaded {
            session_id: goal_session_id,
            goal: None,
        });
        assert!(!ui.window().get_goal_available());

        // A provider probe failure is kept out of the task error surface.
        let detail = "LM Studio could not be reached. Start the service or check the endpoint.";
        ui.window().set_provider_testing(true);
        ui.window().set_provider_pending("probe".into());

        ui.apply_event(&AppEvent::ProviderStatus(argentum_domain::ProviderStatus {
            profile_id: "lm-studio".into(),
            kind: ProviderKind::LocalLmStudio,
            label: "LM Studio".into(),
            endpoint: "http://127.0.0.1:1234/v1/".into(),
            connected: false,
            detail: detail.into(),
        }));
        ui.apply_event(&AppEvent::Error {
            message: detail.into(),
            recoverable: true,
        });

        assert_eq!(ui.window().get_error_message(), "");
        assert_eq!(ui.window().get_run_state(), "Ready");
        assert_eq!(ui.window().get_provider_probe_state(), "failed");
        assert_eq!(ui.window().get_provider_error(), detail);

        // A rapid second submit is ignored until acceptance or error.
        let commands = Rc::new(RefCell::new(Vec::new()));
        let dispatched = commands.clone();
        connect_commands(ui.window(), move |command| {
            dispatched.borrow_mut().push(command);
        });
        ui.window()
            .invoke_submit_task("Inspect the workspace".into());
        ui.window()
            .invoke_submit_task("Inspect the workspace".into());
        assert!(ui.window().get_submission_pending());
        assert_eq!(commands.borrow().len(), 1);
        assert!(matches!(
            commands.borrow().first(),
            Some(AppCommand::SubmitTask { prompt }) if prompt == "Inspect the workspace"
        ));
        ui.apply_command_failure(
            &AppCommand::SubmitTask {
                prompt: "Inspect the workspace".into(),
            },
            "provider unavailable",
        );
        assert!(!ui.window().get_submission_pending());

        // Approval actions share one immediate response latch.
        commands.borrow_mut().clear();
        let approval_id = "00000000-0000-0000-0000-000000000091";
        ui.window().invoke_approve_tool(approval_id.into());
        ui.window().invoke_reject_tool(approval_id.into());
        assert!(ui.window().get_approval_response_pending());
        assert_eq!(commands.borrow().len(), 1);
        assert!(matches!(
            commands.borrow().first(),
            Some(AppCommand::ApproveTool { .. })
        ));
        ui.apply_event(&AppEvent::ApprovalResolved {
            approval_id: approval_id.parse().expect("approval id"),
            approved: true,
        });
        assert!(!ui.window().get_approval_response_pending());

        // A provider list failure clears every busy flag.
        ui.window().set_provider_loading(true);
        ui.window().set_provider_saving(true);
        ui.window().set_provider_selecting(true);
        ui.window().set_provider_testing(true);
        ui.window().set_provider_pending("list".into());
        ui.apply_command_failure(
            &AppCommand::ListProviderProfiles,
            "provider profile store unavailable",
        );
        assert!(!ui.window().get_provider_loading());
        assert!(!ui.window().get_provider_saving());
        assert!(!ui.window().get_provider_selecting());
        assert!(!ui.window().get_provider_testing());
        assert_eq!(ui.window().get_provider_pending(), "");
        assert_ne!(ui.window().get_provider_error(), "");

        commands.borrow_mut().clear();
        ui.window().invoke_refresh_provider_models("minimax".into());
        ui.window()
            .invoke_select_provider_model("minimax".into(), "MiniMax-M2.7".into());
        assert!(matches!(
            commands.borrow().first(),
            Some(AppCommand::ListProviderModels { provider_id }) if provider_id == "minimax"
        ));
        assert!(matches!(
            commands.borrow().get(1),
            Some(AppCommand::SelectProviderModel { provider_id, model })
                if provider_id == "minimax" && model == "MiniMax-M2.7"
        ));

        ui.window().set_run_state("Ready".into());
        ui.window().set_error_message(SharedString::default());
        ui.window().set_provider_models_loading(true);
        ui.window().set_provider_models_pending("list".into());
        ui.window()
            .set_provider_models_provider_id("minimax".into());
        ui.apply_command_failure(
            &AppCommand::ListProviderModels {
                provider_id: "minimax".into(),
            },
            "MiniMax could not return its model catalog. Check its settings.",
        );
        assert_eq!(ui.window().get_run_state(), "Ready");
        assert_eq!(ui.window().get_error_message(), "");
        assert_ne!(ui.window().get_provider_models_error(), "");

        // Harness state keeps presentation visibility separate from capability
        // availability and dispatches only typed profile or surface changes.
        commands.borrow_mut().clear();
        let harness_snapshot = HarnessSnapshot {
            selected_profile_id: "standard".into(),
            profiles: vec![HarnessProfileSummary {
                id: "review".into(),
                label: "Review".into(),
                detail: "Open the Changes summary.".into(),
                selected: false,
                selectable: true,
            }],
            selected_execution_profile_id: "confirm-before-changes".into(),
            execution_profiles: vec![
                HarnessExecutionProfileSummary {
                    id: "read-only".into(),
                    label: "Read Only".into(),
                    detail: "File changes are disabled.".into(),
                    selected: false,
                    selectable: true,
                },
                HarnessExecutionProfileSummary {
                    id: "confirm-before-changes".into(),
                    label: "Confirm Before Changes".into(),
                    detail: "Every write needs approval.".into(),
                    selected: true,
                    selectable: true,
                },
            ],
            capabilities: vec![
                HarnessCapabilityState {
                    id: "verification.runner".into(),
                    label: "Verification runners".into(),
                    kind: HarnessCapabilityKind::Review,
                    availability: HarnessAvailability::Unavailable,
                    readiness: HarnessReadiness::Unavailable,
                    enabled: false,
                    configurable: false,
                    detail: String::new(),
                    unavailable_reason: "No verification runner is registered.".into(),
                    dependencies: Vec::new(),
                },
                HarnessCapabilityState {
                    id: "tool.write-text".into(),
                    label: "Write text tool".into(),
                    kind: HarnessCapabilityKind::Tool,
                    availability: HarnessAvailability::Available,
                    readiness: HarnessReadiness::Ready,
                    enabled: true,
                    configurable: true,
                    detail: "Writes require approval.".into(),
                    unavailable_reason: String::new(),
                    dependencies: vec!["approval.write".into()],
                },
            ],
            surfaces: vec![HarnessSurfaceState {
                id: SurfaceId::Activity,
                label: "Activity".into(),
                availability: HarnessAvailability::Available,
                visible: false,
                configurable: true,
                detail: "Recent factual events.".into(),
                unavailable_reason: String::new(),
            }],
        };
        ui.apply_event(&AppEvent::HarnessSnapshotLoaded(harness_snapshot.clone()));
        assert_eq!(ui.window().get_harness_profile_id(), "standard");
        assert_eq!(
            ui.window().get_execution_profile_id(),
            "confirm-before-changes"
        );
        assert_eq!(
            ui.window().get_execution_profile_label(),
            "Confirm Before Changes"
        );
        assert_eq!(ui.window().get_harness_profiles().row_count(), 1);
        assert_eq!(
            ui.window()
                .get_harness_capabilities()
                .row_data(0)
                .expect("capability")
                .state,
            "unavailable"
        );
        ui.window()
            .invoke_select_execution_profile("read-only".into());
        assert!(matches!(
            commands.borrow().first(),
            Some(AppCommand::SelectExecutionProfile { profile_id })
                if profile_id == "read-only"
        ));
        let mut read_only_snapshot = harness_snapshot.clone();
        read_only_snapshot.selected_execution_profile_id = "read-only".into();
        read_only_snapshot.execution_profiles[0].selected = true;
        read_only_snapshot.execution_profiles[1].selected = false;
        read_only_snapshot.capabilities[1].enabled = false;
        ui.apply_event(&AppEvent::HarnessSnapshotLoaded(read_only_snapshot));
        assert_eq!(ui.window().get_execution_profile_label(), "Read Only");
        assert_eq!(
            ui.window()
                .get_harness_capabilities()
                .row_data(1)
                .expect("write capability")
                .state,
            "disabled"
        );
        commands.borrow_mut().clear();
        ui.window()
            .invoke_set_harness_capability_enabled("tool.write-text".into(), true);
        assert!(matches!(
            commands.borrow().first(),
            Some(AppCommand::SetHarnessCapabilityEnabled {
                capability_id,
                enabled: true,
            }) if capability_id == "tool.write-text"
        ));
        ui.apply_event(&AppEvent::HarnessSnapshotLoaded(harness_snapshot.clone()));
        commands.borrow_mut().clear();
        ui.window().invoke_select_harness_profile("review".into());
        assert!(matches!(
            commands.borrow().first(),
            Some(AppCommand::SelectHarnessProfile { profile_id }) if profile_id == "review"
        ));
        let mut review_snapshot = harness_snapshot;
        review_snapshot.selected_profile_id = "review".into();
        review_snapshot.profiles[0].selected = true;
        ui.apply_event(&AppEvent::HarnessSnapshotLoaded(review_snapshot));
        commands.borrow_mut().clear();
        ui.window()
            .invoke_set_harness_surface_visibility("activity".into(), true);
        assert!(matches!(
            commands.borrow().first(),
            Some(AppCommand::SetSurfaceVisibility {
                surface: SurfaceId::Activity,
                visible: true,
            })
        ));
        commands.borrow_mut().clear();
        ui.window().set_activity_open(false);
        ui.window().invoke_toggle_activity();
        ui.window().set_inspector_open(true);
        ui.window().invoke_toggle_inspector();
        ui.window().set_trajectory_open(true);
        ui.window().invoke_toggle_trajectory();
        assert!(matches!(
            commands.borrow().first(),
            Some(AppCommand::SetSurfaceVisibility {
                surface: SurfaceId::Activity,
                visible: false,
            })
        ));
        assert!(matches!(
            commands.borrow().get(1),
            Some(AppCommand::SetSurfaceVisibility {
                surface: SurfaceId::Changes,
                visible: true,
            })
        ));
        assert!(matches!(
            commands.borrow().get(2),
            Some(AppCommand::SetSurfaceVisibility {
                surface: SurfaceId::Trajectory,
                visible: true,
            })
        ));

        // Provider mutations and catalog refreshes are rejected at the Rust callback
        // boundary while a run is active, even if a caller bypasses Slint enabled state.
        commands.borrow_mut().clear();
        ui.window().set_running(true);
        ui.window().invoke_probe_provider("minimax".into());
        ui.window().invoke_save_provider_profile(
            "minimax".into(),
            "MiniMax".into(),
            "openai-compatible".into(),
            "https://api.minimax.io/v1/".into(),
            "MiniMax-M2.7".into(),
            true,
        );
        ui.window().invoke_select_provider_profile("minimax".into());
        ui.window().invoke_refresh_provider_models("minimax".into());
        ui.window()
            .invoke_select_provider_model("minimax".into(), "MiniMax-M2.7".into());
        ui.window()
            .invoke_select_execution_profile("read-only".into());
        ui.window()
            .invoke_set_harness_capability_enabled("tool.write-text".into(), false);
        assert!(commands.borrow().is_empty());
        ui.window().set_running(false);

        // Run events known to target another session cannot alter the active view.
        let active_session_id: SessionId = "00000000-0000-0000-0000-0000000000a1"
            .parse()
            .expect("active session id");
        let other_session_id: SessionId = "00000000-0000-0000-0000-0000000000a2"
            .parse()
            .expect("other session id");
        let run_id: RunId = "00000000-0000-0000-0000-0000000000a3"
            .parse()
            .expect("run id");
        ui.window()
            .set_active_session_id(active_session_id.to_string().into());
        ui.apply_event(&AppEvent::TrajectorySnapshotLoaded(TrajectorySnapshot {
            session_id: other_session_id,
            entries: vec![argentum_domain::TrajectoryEntry {
                sequence: 1,
                run_id: Some(run_id),
                kind: TrajectoryKind::Lifecycle,
                state: TrajectoryState::Active,
                title: "Background run".into(),
                detail: "Must stay hidden".into(),
                occurred_at: now(),
            }],
            truncated: false,
        }));
        assert_eq!(ui.window().get_trajectory_items().row_count(), 0);
        ui.apply_event(&AppEvent::TrajectorySnapshotLoaded(TrajectorySnapshot {
            session_id: active_session_id,
            entries: vec![argentum_domain::TrajectoryEntry {
                sequence: 2,
                run_id: Some(run_id),
                kind: TrajectoryKind::Lifecycle,
                state: TrajectoryState::Active,
                title: "Run started".into(),
                detail: "Run lifecycle changed.".into(),
                occurred_at: now(),
            }],
            truncated: true,
        }));
        assert_eq!(ui.window().get_trajectory_items().row_count(), 1);
        assert_eq!(ui.window().get_trajectory_summary(), "1 durable record");
        assert!(ui.window().get_trajectory_truncated());
        let recorded_entry = argentum_domain::TrajectoryEntry {
            sequence: 3,
            run_id: Some(run_id),
            kind: TrajectoryKind::Verification,
            state: TrajectoryState::Success,
            title: "Verification passed".into(),
            detail: "Exact check result.".into(),
            occurred_at: now(),
        };
        ui.apply_event(&AppEvent::TrajectoryEntryRecorded {
            session_id: active_session_id,
            entry: recorded_entry.clone(),
        });
        ui.apply_event(&AppEvent::TrajectoryEntryRecorded {
            session_id: active_session_id,
            entry: recorded_entry,
        });
        assert_eq!(ui.window().get_trajectory_items().row_count(), 2);
        assert_eq!(ui.window().get_trajectory_summary(), "2 durable records");
        ui.window().set_active_run_id(SharedString::default());
        ui.window().set_plan_steps(empty_plan_model());
        ui.window().set_user_prompt(SharedString::default());
        ui.window().set_transcript(SharedString::default());
        let unknown_run_id: RunId = "00000000-0000-0000-0000-0000000000a5"
            .parse()
            .expect("unknown run id");
        ui.apply_event(&AppEvent::AssistantDelta {
            run_id: unknown_run_id,
            text: "Must stay hidden".into(),
        });
        assert_eq!(ui.window().get_transcript(), "");
        ui.apply_event(&AppEvent::TaskAccepted(Task {
            id: "00000000-0000-0000-0000-0000000000a4"
                .parse()
                .expect("task id"),
            session_id: other_session_id,
            prompt: "Background task".into(),
            lifecycle: TaskLifecycle::Queued,
            created_at: now(),
        }));
        ui.apply_event(&AppEvent::PlanUpdated {
            run_id,
            steps: vec![PlanStep {
                id: "one".into(),
                title: "Should stay hidden".into(),
                detail: String::new(),
                complete: false,
                blocked: false,
            }],
        });
        assert_eq!(ui.window().get_active_run_id(), "");
        assert_eq!(ui.window().get_plan_steps().row_count(), 0);
        assert_eq!(ui.window().get_user_prompt(), "");

        // Conversation history is a multi-message projection for the active session.
        let project_id: argentum_domain::ProjectId = "00000000-0000-0000-0000-0000000000b1"
            .parse()
            .expect("project id");
        let session_id: SessionId = "00000000-0000-0000-0000-0000000000b2"
            .parse()
            .expect("session id");
        let run_id: RunId = "00000000-0000-0000-0000-0000000000b3"
            .parse()
            .expect("run id");
        ui.window()
            .set_active_session_id(session_id.to_string().into());
        ui.apply_event(&AppEvent::ConversationSnapshotLoaded(
            ConversationSnapshot {
                project_id,
                session_id,
                messages: vec![
                    ConversationMessage {
                        id: "00000000-0000-0000-0000-0000000000b4"
                            .parse()
                            .expect("message id"),
                        project_id,
                        session_id,
                        run_id,
                        role: ConversationRole::User,
                        text: "Question".into(),
                        reasoning: String::new(),
                        usage: None,
                        profile_id: String::new(),
                        model: String::new(),
                        status: ConversationMessageStatus::Complete,
                        created_at: now(),
                    },
                    ConversationMessage {
                        id: "00000000-0000-0000-0000-0000000000b5"
                            .parse()
                            .expect("message id"),
                        project_id,
                        session_id,
                        run_id,
                        role: ConversationRole::Assistant,
                        text: "Answer".into(),
                        reasoning: "Checked the available evidence.".into(),
                        usage: Some(argentum_domain::ModelUsage {
                            input_tokens: 12,
                            output_tokens: 7,
                            total_tokens: 19,
                            reasoning_tokens: Some(3),
                            cached_input_tokens: None,
                            context_window_tokens: Some(128_000),
                        }),
                        profile_id: "minimax".into(),
                        model: "MiniMax-M2.7".into(),
                        status: ConversationMessageStatus::Interrupted,
                        created_at: now(),
                    },
                ],
            },
        ));
        let messages = ui.window().get_conversation_messages();
        assert_eq!(messages.row_count(), 2);
        assert_eq!(messages.row_data(0).expect("user message").role, "user");
        assert_eq!(
            messages.row_data(1).expect("assistant message").text,
            "Answer"
        );
        assert_eq!(
            messages.row_data(1).expect("assistant message").state,
            "Interrupted"
        );
        let restored_assistant = messages.row_data(1).expect("assistant message");
        assert!(restored_assistant.reasoning_available);
        assert_eq!(
            restored_assistant.reasoning,
            "Checked the available evidence."
        );
        assert_eq!(restored_assistant.provider_label, "minimax");
        assert_eq!(restored_assistant.model, "MiniMax-M2.7");
        assert!(restored_assistant.usage_reported);
        assert_eq!(restored_assistant.input_tokens, 12);
        assert_eq!(restored_assistant.output_tokens, 7);
        assert_eq!(restored_assistant.total_tokens, 19);
        assert!(restored_assistant.context_window_reported);
        assert_eq!(restored_assistant.context_window_tokens, 128_000);

        ui.apply_event(&AppEvent::AssistantReasoningDelta {
            run_id,
            text: "<think>Live reasoning</think>".into(),
        });
        assert_eq!(ui.window().get_current_reasoning(), "Live reasoning");
        assert!(ui.window().get_current_reasoning_available());

        ui.apply_event(&AppEvent::ProviderProfilesSnapshot {
            profiles: vec![ProviderProfile {
                id: "minimax".into(),
                label: "MiniMax".into(),
                kind: ProviderKind::OpenAiCompatible,
                endpoint: "https://api.minimax.io/v1/".into(),
                model: "MiniMax-M2.7".into(),
                selected: true,
            }],
        });
        ui.window().set_current_reasoning("Live reasoning".into());
        ui.window().set_current_reasoning_available(true);
        ui.apply_event(&AppEvent::ModelUsageUpdated {
            session_id,
            run_id,
            profile_id: "minimax".into(),
            model: "MiniMax-M2.7".into(),
            usage: argentum_domain::ModelUsage {
                input_tokens: 40,
                output_tokens: 10,
                total_tokens: 50,
                reasoning_tokens: Some(6),
                cached_input_tokens: Some(4),
                context_window_tokens: Some(204_800),
            },
        });
        assert!(ui.window().get_usage_reported());
        assert_eq!(ui.window().get_usage_total_tokens(), 50);
        assert_eq!(ui.window().get_context_window_tokens(), 204_800);
        assert_eq!(ui.window().get_current_run_provider_label(), "MiniMax");

        // A failed provider switch preserves reasoning and usage from the valid
        // current execution target.
        ui.window().set_provider_selecting(true);
        ui.window().set_provider_pending("select".into());
        ui.apply_command_failure(
            &AppCommand::SelectProviderProfile {
                provider_id: "secondary".into(),
            },
            "provider selection failed",
        );
        assert_eq!(ui.window().get_current_reasoning(), "Live reasoning");
        assert!(ui.window().get_usage_reported());
        assert_eq!(ui.window().get_usage_total_tokens(), 50);
        ui.window().set_provider_model_selecting(true);
        ui.window().set_provider_models_pending("select".into());
        ui.apply_command_failure(
            &AppCommand::SelectProviderModel {
                provider_id: "minimax".into(),
                model: "MiniMax-M2.7-highspeed".into(),
            },
            "model selection failed",
        );
        assert_eq!(ui.window().get_current_reasoning(), "Live reasoning");
        assert!(ui.window().get_usage_reported());
        assert_eq!(ui.window().get_usage_total_tokens(), 50);

        // A target catalog may finish before the profile-selection snapshot. It
        // remains available and does not clear the independent profile operation.
        ui.window()
            .set_provider_models_provider_id("secondary".into());
        ui.window().set_provider_models_loading(true);
        ui.window().set_provider_models_pending("list".into());
        ui.window().set_provider_selecting(true);
        ui.window().set_provider_pending("select".into());
        ui.apply_event(&AppEvent::ProviderModelsSnapshot {
            provider_id: "secondary".into(),
            models: vec![ProviderModel {
                id: "secondary-model".into(),
                label: "Secondary model".into(),
                context_window_tokens: Some(32_768),
            }],
            selected_model: "secondary-model".into(),
        });
        let provider_models = ui.window().get_provider_models();
        assert_eq!(provider_models.row_count(), 1);
        let projected_model = provider_models.row_data(0).expect("provider model");
        assert!(projected_model.selected);
        assert_eq!(projected_model.context_window_tokens, 32_768);
        assert!(!ui.window().get_provider_models_loading());
        assert_eq!(ui.window().get_provider_models_pending(), "");
        assert_eq!(ui.window().get_provider_pending(), "select");
        assert_eq!(ui.window().get_provider_id(), "minimax");

        ui.apply_event(&AppEvent::ProviderProfilesSnapshot {
            profiles: vec![
                ProviderProfile {
                    id: "minimax".into(),
                    label: "MiniMax".into(),
                    kind: ProviderKind::OpenAiCompatible,
                    endpoint: "https://api.minimax.io/v1/".into(),
                    model: "MiniMax-M2.7".into(),
                    selected: false,
                },
                ProviderProfile {
                    id: "secondary".into(),
                    label: "Secondary".into(),
                    kind: ProviderKind::OpenAiCompatible,
                    endpoint: "https://secondary.example/v1/".into(),
                    model: "secondary-model".into(),
                    selected: true,
                },
            ],
        });
        assert_eq!(ui.window().get_provider_id(), "secondary");
        assert_eq!(ui.window().get_provider_models_provider_id(), "secondary");
        assert_eq!(ui.window().get_provider_models().row_count(), 1);
        assert!(!ui.window().get_provider_selecting());
        assert_eq!(ui.window().get_provider_pending(), "");
        assert_eq!(ui.window().get_current_reasoning(), "");
        assert!(!ui.window().get_usage_reported());

        // A concurrent model-catalog success cannot consume profile-list state.
        // The correlated list failure remains in Settings and does not become a
        // task failure.
        ui.window().set_run_state("Ready".into());
        ui.window().set_error_message(SharedString::default());
        ui.window().set_provider_error(SharedString::default());
        ui.window().set_provider_loading(true);
        ui.window().set_provider_pending("list".into());
        ui.window().set_provider_models_loading(true);
        ui.window().set_provider_models_pending("list".into());
        ui.window()
            .set_provider_models_provider_id("secondary".into());
        ui.apply_event(&AppEvent::ProviderModelsSnapshot {
            provider_id: "secondary".into(),
            models: vec![ProviderModel {
                id: "secondary-model".into(),
                label: "Secondary model".into(),
                context_window_tokens: Some(32_768),
            }],
            selected_model: "secondary-model".into(),
        });
        assert_eq!(ui.window().get_provider_pending(), "list");
        assert!(ui.window().get_provider_loading());
        ui.apply_event(&AppEvent::Error {
            message: "provider profile store unavailable".into(),
            recoverable: true,
        });
        assert_eq!(ui.window().get_error_message(), "");
        assert_eq!(ui.window().get_run_state(), "Ready");
        assert!(ui.window().get_provider_loading());
        ui.apply_command_failure(
            &AppCommand::ListProviderProfiles,
            "provider profile store unavailable",
        );
        assert!(!ui.window().get_provider_loading());
        assert_eq!(ui.window().get_provider_pending(), "");
        assert_ne!(ui.window().get_provider_error(), "");

        // Active-run resync restores the visible run and an empty snapshot clears it.
        let active_run_id: RunId = "00000000-0000-0000-0000-0000000000b6"
            .parse()
            .expect("active run id");
        ui.apply_event(&AppEvent::ActiveRunsSnapshot {
            runs: vec![ActiveRunState {
                session_id,
                run_id: active_run_id,
                lifecycle: TaskLifecycle::WaitingForApproval,
            }],
        });
        assert_eq!(ui.window().get_active_run_id(), active_run_id.to_string());
        assert_eq!(ui.window().get_run_state(), "Approval needed");
        assert!(ui.window().get_running());

        // Run errors for another session or run cannot alter the visible run.
        let background_session_id: SessionId = "00000000-0000-0000-0000-0000000000b7"
            .parse()
            .expect("background session id");
        let background_run_id: RunId = "00000000-0000-0000-0000-0000000000b8"
            .parse()
            .expect("background run id");
        for (error_session_id, error_run_id) in [
            (background_session_id, background_run_id),
            (session_id, background_run_id),
        ] {
            ui.apply_event(&AppEvent::RunError {
                session_id: error_session_id,
                run_id: error_run_id,
                message: "background failure".into(),
                recoverable: true,
            });
        }
        assert_eq!(ui.window().get_run_state(), "Approval needed");
        assert!(ui.window().get_running());

        ui.apply_event(&AppEvent::RunError {
            session_id,
            run_id: active_run_id,
            message: "provider request failed".into(),
            recoverable: true,
        });
        assert_eq!(ui.window().get_run_state(), "Could not continue");
        assert!(!ui.window().get_running());
        assert_ne!(ui.window().get_error_message(), "");

        ui.window().set_run_state("Running".into());
        ui.window().set_running(true);
        ui.apply_event(&AppEvent::ActiveRunsSnapshot { runs: Vec::new() });
        assert_eq!(ui.window().get_active_run_id(), "");
        assert_eq!(ui.window().get_run_state(), "Ready");
        assert!(!ui.window().get_running());
    }

    #[test]
    fn provider_profile_projection_preserves_selection_and_profile_identity() {
        let profiles = vec![
            ProviderProfile {
                id: "secondary".into(),
                label: "Secondary".into(),
                kind: ProviderKind::OpenAiCompatible,
                endpoint: "http://127.0.0.1:8000/v1/".into(),
                model: "small".into(),
                selected: false,
            },
            ProviderProfile {
                id: "local-main".into(),
                label: "Local main".into(),
                kind: ProviderKind::LocalLmStudio,
                endpoint: "http://127.0.0.1:1234/v1/".into(),
                model: "large".into(),
                selected: true,
            },
        ];

        let rows = provider_profile_model(&profiles);
        assert_eq!(rows.row_count(), 2);
        assert_eq!(rows.row_data(0).expect("first profile").id, "secondary");
        let selected = rows.row_data(1).expect("selected profile");
        assert_eq!(selected.id, "local-main");
        assert_eq!(selected.kind, "lm-studio");
        assert_eq!(selected.model, "large");
        assert!(selected.selected);
    }

    #[test]
    fn restored_assistant_copy_separates_tagged_reasoning_from_exact_visible_text() {
        assert_eq!(
            split_tagged_reasoning(
                "<think>private chain of thought</think>\nARGENTUM_MINIMAX_SMOKE_OK"
            ),
            (
                "\nARGENTUM_MINIMAX_SMOKE_OK".into(),
                "private chain of thought".into()
            )
        );
        assert_eq!(
            split_tagged_reasoning("ordinary answer"),
            ("ordinary answer".into(), String::new())
        );
        assert_eq!(
            split_tagged_reasoning("visible<think>unfinished private reasoning"),
            ("visible".into(), "unfinished private reasoning".into())
        );
        assert_eq!(
            split_tagged_reasoning("A<think>one</think>B<think>two</think>C"),
            ("ABC".into(), "one\n\ntwo".into())
        );
    }

    #[test]
    fn provider_validation_matches_safe_profile_contract() {
        assert_eq!(validate_provider_label(""), "Enter a profile name.");
        assert!(validate_provider_label("Local main").is_empty());
        assert_eq!(validate_provider_model("\n"), "Enter a model name.");
        assert!(validate_provider_model("mistral-small").is_empty());
        assert!(validate_provider_endpoint("http://127.0.0.1:1234/v1/").is_empty());
        assert_eq!(
            validate_provider_endpoint("https://user:secret@example.com/v1"),
            "Endpoint cannot include credentials, a query, or a fragment."
        );
        assert_eq!(
            validate_provider_endpoint("file:///tmp/provider"),
            "Enter a valid http or https endpoint."
        );
    }

    #[test]
    fn provider_status_uses_exact_profile_id_with_legacy_fallback() {
        let exact = argentum_domain::ProviderStatus {
            profile_id: "local-secondary".into(),
            kind: ProviderKind::LocalLmStudio,
            label: "Secondary".into(),
            endpoint: "http://127.0.0.1:1234/v1/".into(),
            connected: false,
            detail: "Configured".into(),
        };
        let mut legacy = exact.clone();
        legacy.profile_id.clear();

        assert_eq!(provider_status_id(&exact), "local-secondary");
        assert_eq!(provider_status_id(&legacy), "lm-studio");
    }

    #[test]
    fn tool_result_labels_are_honest() {
        assert_eq!(
            tool_result_presentation(&ToolResultState::Failed),
            ("failed", "error")
        );
        assert_eq!(
            tool_result_presentation(&ToolResultState::Cancelled),
            ("cancelled", "neutral")
        );
    }

    #[test]
    fn change_summary_reports_file_count_without_duplicate_metrics() {
        assert_eq!(change_summary(0, 0, 0), "No files changed");
        assert_eq!(change_summary(1, 12, 4), "1 file changed");
        assert_eq!(change_summary(3, 20, 7), "3 files changed");
    }

    #[test]
    fn error_presentation_is_actionable_and_never_empty() {
        assert_eq!(
            present_error("no provider is configured"),
            "No model provider is configured."
        );
        assert_eq!(present_error(""), "Argentum could not continue.");
    }
}
