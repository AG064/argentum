use std::collections::BTreeMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use uuid::Uuid;

pub type ProjectId = Uuid;
pub type SessionId = Uuid;
pub type TaskId = Uuid;
pub type RunId = Uuid;
pub type MessageId = Uuid;
pub type ApprovalId = Uuid;
pub type ToolCallId = Uuid;
pub type GoalId = Uuid;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ToolInput {
    ReadText { path: String },
    WriteText { path: String, content: String },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolRequest {
    pub call_id: ToolCallId,
    pub run_id: RunId,
    pub input: ToolInput,
}

pub fn now() -> OffsetDateTime {
    OffsetDateTime::now_utc()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TaskLifecycle {
    Draft,
    Queued,
    Planning,
    Running,
    WaitingForApproval,
    Reviewing,
    Verifying,
    Complete,
    Paused,
    Cancelled,
    Failed,
}

impl TaskLifecycle {
    pub const fn is_terminal(self) -> bool {
        matches!(self, Self::Complete | Self::Cancelled | Self::Failed)
    }

    pub const fn label(self) -> &'static str {
        match self {
            Self::Draft => "Draft",
            Self::Queued => "Queued",
            Self::Planning => "Planning",
            Self::Running => "Running",
            Self::WaitingForApproval => "Approval needed",
            Self::Reviewing => "Reviewing",
            Self::Verifying => "Verifying",
            Self::Complete => "Completed",
            Self::Paused => "Paused",
            Self::Cancelled => "Cancelled",
            Self::Failed => "Failed",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum Capability {
    ReadFiles,
    WriteFiles,
    ExecuteCommands,
    Network,
    ExternalProcess,
}

impl Capability {
    pub const fn label(self) -> &'static str {
        match self {
            Self::ReadFiles => "Read files",
            Self::WriteFiles => "Write files",
            Self::ExecuteCommands => "Execute commands",
            Self::Network => "Network",
            Self::ExternalProcess => "External process",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum SurfaceId {
    Conversation,
    Plan,
    Changes,
    Files,
    Terminal,
    Preview,
    Activity,
    Trajectory,
    Approvals,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HarnessCapabilityKind {
    Agent,
    Provider,
    Session,
    Tool,
    Security,
    Review,
    Execution,
    Integration,
    Extension,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HarnessAvailability {
    Available,
    Unavailable,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HarnessReadiness {
    Ready,
    NeedsConfiguration,
    NotVerified,
    Blocked,
    Unavailable,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HarnessCapabilityState {
    pub id: String,
    pub label: String,
    pub kind: HarnessCapabilityKind,
    pub availability: HarnessAvailability,
    pub readiness: HarnessReadiness,
    pub enabled: bool,
    pub configurable: bool,
    pub detail: String,
    pub unavailable_reason: String,
    pub dependencies: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HarnessSurfaceState {
    pub id: SurfaceId,
    pub label: String,
    pub availability: HarnessAvailability,
    pub visible: bool,
    pub configurable: bool,
    pub detail: String,
    pub unavailable_reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HarnessProfileSummary {
    pub id: String,
    pub label: String,
    pub detail: String,
    pub selected: bool,
    pub selectable: bool,
}

pub const DEFAULT_EXECUTION_PROFILE_ID: &str = "confirm-before-changes";
pub const CUSTOM_EXECUTION_PROFILE_ID: &str = "custom";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HarnessExecutionPolicy {
    #[serde(default = "default_execution_profile_id")]
    pub profile_id: String,
    #[serde(default = "default_execution_capabilities")]
    pub capability_enabled: BTreeMap<String, bool>,
}

impl Default for HarnessExecutionPolicy {
    fn default() -> Self {
        Self {
            profile_id: default_execution_profile_id(),
            capability_enabled: default_execution_capabilities(),
        }
    }
}

fn default_execution_profile_id() -> String {
    DEFAULT_EXECUTION_PROFILE_ID.to_owned()
}

fn default_execution_capabilities() -> BTreeMap<String, bool> {
    BTreeMap::from([
        ("tool.read-text".to_owned(), true),
        ("tool.write-text".to_owned(), true),
    ])
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HarnessExecutionProfileSummary {
    pub id: String,
    pub label: String,
    pub detail: String,
    pub selected: bool,
    pub selectable: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HarnessSnapshot {
    pub selected_profile_id: String,
    pub profiles: Vec<HarnessProfileSummary>,
    #[serde(default = "default_execution_profile_id")]
    pub selected_execution_profile_id: String,
    #[serde(default)]
    pub execution_profiles: Vec<HarnessExecutionProfileSummary>,
    pub capabilities: Vec<HarnessCapabilityState>,
    pub surfaces: Vec<HarnessSurfaceState>,
}

impl SurfaceId {
    pub const fn label(self) -> &'static str {
        match self {
            Self::Conversation => "Conversation",
            Self::Plan => "Plan",
            Self::Changes => "Changes",
            Self::Files => "Files",
            Self::Terminal => "Terminal",
            Self::Preview => "Preview",
            Self::Activity => "Activity",
            Self::Trajectory => "Trajectory",
            Self::Approvals => "Approvals",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Density {
    Compact,
    Comfortable,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LayoutProfile {
    #[serde(default = "default_harness_profile_id")]
    pub harness_profile_id: String,
    pub density: Density,
    pub visible: BTreeMap<SurfaceId, bool>,
    pub widths: BTreeMap<SurfaceId, u32>,
}

impl Default for LayoutProfile {
    fn default() -> Self {
        let mut visible = BTreeMap::new();
        visible.insert(SurfaceId::Conversation, true);
        visible.insert(SurfaceId::Plan, true);
        visible.insert(SurfaceId::Changes, false);
        visible.insert(SurfaceId::Files, false);
        visible.insert(SurfaceId::Terminal, false);
        visible.insert(SurfaceId::Preview, false);
        visible.insert(SurfaceId::Activity, false);
        visible.insert(SurfaceId::Trajectory, false);
        visible.insert(SurfaceId::Approvals, false);

        let mut widths = BTreeMap::new();
        widths.insert(SurfaceId::Plan, 320);
        widths.insert(SurfaceId::Changes, 400);
        widths.insert(SurfaceId::Files, 360);
        widths.insert(SurfaceId::Terminal, 420);
        widths.insert(SurfaceId::Preview, 420);
        widths.insert(SurfaceId::Activity, 360);
        widths.insert(SurfaceId::Trajectory, 400);
        widths.insert(SurfaceId::Approvals, 380);

        Self {
            harness_profile_id: default_harness_profile_id(),
            density: Density::Comfortable,
            visible,
            widths,
        }
    }
}

fn default_harness_profile_id() -> String {
    "standard".to_owned()
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Project {
    pub id: ProjectId,
    pub name: String,
    pub workspace_root: PathBuf,
    pub created_at: OffsetDateTime,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Session {
    pub id: SessionId,
    pub project_id: ProjectId,
    pub title: String,
    pub created_at: OffsetDateTime,
    pub updated_at: OffsetDateTime,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionSummary {
    pub id: SessionId,
    pub title: String,
    pub created_at: OffsetDateTime,
    pub updated_at: OffsetDateTime,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GoalLifecycle {
    Active,
    Paused,
    BudgetLimited,
    Complete,
}

impl GoalLifecycle {
    pub const fn label(self) -> &'static str {
        match self {
            Self::Active => "Active",
            Self::Paused => "Paused",
            Self::BudgetLimited => "Budget limited",
            Self::Complete => "Complete",
        }
    }

    pub const fn can_resume(self) -> bool {
        matches!(self, Self::Paused)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GoalVerification {
    pub run_id: RunId,
    pub passed: bool,
    pub summary: String,
    pub recorded_at: OffsetDateTime,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Goal {
    pub id: GoalId,
    pub project_id: ProjectId,
    pub session_id: SessionId,
    pub objective: String,
    pub lifecycle: GoalLifecycle,
    pub token_budget: Option<u64>,
    pub tool_budget: Option<u32>,
    pub time_budget_seconds: Option<u64>,
    pub tokens_used: u64,
    pub tools_used: u32,
    pub iteration: u32,
    pub next_action: String,
    pub verification_history: Vec<GoalVerification>,
    pub created_at: OffsetDateTime,
    pub updated_at: OffsetDateTime,
}

impl From<&Session> for SessionSummary {
    fn from(session: &Session) -> Self {
        Self {
            id: session.id,
            title: session.title.clone(),
            created_at: session.created_at,
            updated_at: session.updated_at,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkspaceSnapshot {
    pub project: Project,
    pub sessions: Vec<SessionSummary>,
    pub active_session_id: Option<SessionId>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConversationRole {
    User,
    Assistant,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConversationMessageStatus {
    Complete,
    Interrupted,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConversationMessage {
    pub id: MessageId,
    pub project_id: ProjectId,
    pub session_id: SessionId,
    pub run_id: RunId,
    pub role: ConversationRole,
    pub text: String,
    #[serde(default)]
    pub reasoning: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub usage: Option<ModelUsage>,
    #[serde(default)]
    pub profile_id: String,
    #[serde(default)]
    pub model: String,
    pub status: ConversationMessageStatus,
    pub created_at: OffsetDateTime,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConversationSnapshot {
    pub project_id: ProjectId,
    pub session_id: SessionId,
    pub messages: Vec<ConversationMessage>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Task {
    pub id: TaskId,
    pub session_id: SessionId,
    pub prompt: String,
    pub lifecycle: TaskLifecycle,
    pub created_at: OffsetDateTime,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlanStep {
    pub id: String,
    pub title: String,
    pub detail: String,
    pub complete: bool,
    pub blocked: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Run {
    pub id: RunId,
    pub task_id: TaskId,
    pub lifecycle: TaskLifecycle,
    pub steps: Vec<PlanStep>,
    pub started_at: Option<OffsetDateTime>,
    pub finished_at: Option<OffsetDateTime>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ActiveRunState {
    pub session_id: SessionId,
    pub run_id: RunId,
    pub lifecycle: TaskLifecycle,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolDescriptor {
    pub id: String,
    pub title: String,
    pub summary: String,
    pub capabilities: Vec<Capability>,
    pub requires_approval: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolTrace {
    pub id: ToolCallId,
    pub run_id: RunId,
    pub tool_id: String,
    pub summary: String,
    pub result: ToolResultState,
    pub duration_ms: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ToolResultState {
    Running,
    Succeeded,
    Failed,
    Cancelled,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ApprovalRequest {
    pub id: ApprovalId,
    pub run_id: RunId,
    pub tool_id: String,
    pub action: String,
    pub target: String,
    pub reason: String,
    pub capabilities: Vec<Capability>,
    pub expires_at: Option<OffsetDateTime>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ApprovalScope {
    Once,
    Session,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChangeSet {
    pub run_id: RunId,
    pub files_changed: u32,
    pub additions: u32,
    pub removals: u32,
    pub verification_ready: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ProviderKind {
    OpenAiCompatible,
    Anthropic,
    LocalLmStudio,
    Unknown,
}

pub const DEFAULT_PROVIDER_ID: &str = "lm-studio";
pub const DEFAULT_LM_STUDIO_ENDPOINT: &str = "http://127.0.0.1:1234/v1/";
pub const DEFAULT_MODEL: &str = "local-model";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProviderProfile {
    pub id: String,
    pub label: String,
    pub kind: ProviderKind,
    pub endpoint: String,
    pub model: String,
    pub selected: bool,
}

impl ProviderProfile {
    pub fn default_lm_studio() -> Self {
        Self {
            id: DEFAULT_PROVIDER_ID.into(),
            label: "LM Studio".into(),
            kind: ProviderKind::LocalLmStudio,
            endpoint: DEFAULT_LM_STUDIO_ENDPOINT.into(),
            model: DEFAULT_MODEL.into(),
            selected: true,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProviderStatus {
    #[serde(default)]
    pub profile_id: String,
    pub kind: ProviderKind,
    pub label: String,
    pub endpoint: String,
    pub connected: bool,
    pub detail: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProviderModel {
    pub id: String,
    pub label: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context_window_tokens: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModelUsage {
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub total_tokens: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reasoning_tokens: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cached_input_tokens: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context_window_tokens: Option<u64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TrajectoryKind {
    Task,
    Plan,
    Lifecycle,
    Tool,
    Approval,
    Usage,
    Change,
    Verification,
    Error,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TrajectoryState {
    Neutral,
    Active,
    Attention,
    Success,
    Error,
    Cancelled,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TrajectoryEntry {
    pub sequence: u64,
    pub run_id: Option<RunId>,
    pub kind: TrajectoryKind,
    pub state: TrajectoryState,
    pub title: String,
    pub detail: String,
    pub occurred_at: OffsetDateTime,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TrajectorySnapshot {
    pub session_id: SessionId,
    pub entries: Vec<TrajectoryEntry>,
    pub truncated: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum AppCommand {
    NewSession,
    SelectSession {
        session_id: SessionId,
    },
    SetGoal {
        objective: String,
        token_budget: Option<u64>,
        tool_budget: Option<u32>,
        time_budget_seconds: Option<u64>,
    },
    PauseGoal,
    ResumeGoal,
    ClearGoal,
    ProbeProvider {
        provider_id: String,
    },
    ListProviderProfiles,
    SaveProviderProfile {
        profile: ProviderProfile,
    },
    SelectProviderProfile {
        provider_id: String,
    },
    ListProviderModels {
        provider_id: String,
    },
    SelectProviderModel {
        provider_id: String,
        model: String,
    },
    ListHarnessState,
    SelectHarnessProfile {
        profile_id: String,
    },
    SetSurfaceVisibility {
        surface: SurfaceId,
        visible: bool,
    },
    SelectExecutionProfile {
        profile_id: String,
    },
    SetHarnessCapabilityEnabled {
        capability_id: String,
        enabled: bool,
    },
    LoadTrajectory {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        session_id: Option<SessionId>,
    },
    SubmitTask {
        prompt: String,
    },
    RequestTool {
        request: ToolRequest,
    },
    CancelRun {
        run_id: RunId,
    },
    ApproveTool {
        approval_id: ApprovalId,
        scope: ApprovalScope,
    },
    RejectTool {
        approval_id: ApprovalId,
    },
    ToggleSurface {
        surface: SurfaceId,
    },
    SetLayout {
        profile: LayoutProfile,
    },
    ResetLayout,
}

#[deprecated(note = "use AppCommand; commands are shared by every frontend")]
pub type UiCommand = AppCommand;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum AppEvent {
    WorkspaceStateLoaded(WorkspaceSnapshot),
    ConversationSnapshotLoaded(ConversationSnapshot),
    GoalSnapshotLoaded {
        session_id: SessionId,
        goal: Option<Goal>,
    },
    ActiveRunsSnapshot {
        runs: Vec<ActiveRunState>,
    },
    ProjectCreated(Project),
    SessionCreated(Session),
    TaskAccepted(Task),
    PlanUpdated {
        run_id: RunId,
        steps: Vec<PlanStep>,
    },
    RunStatusChanged {
        run_id: RunId,
        lifecycle: TaskLifecycle,
    },
    AssistantDelta {
        run_id: RunId,
        text: String,
    },
    AssistantReasoningDelta {
        run_id: RunId,
        text: String,
    },
    ModelUsageUpdated {
        session_id: SessionId,
        run_id: RunId,
        profile_id: String,
        model: String,
        usage: ModelUsage,
    },
    ToolStarted(ToolTrace),
    ToolFinished(ToolTrace),
    ApprovalRequested(ApprovalRequest),
    ApprovalResolved {
        approval_id: ApprovalId,
        approved: bool,
    },
    ChangeSetReady(ChangeSet),
    VerificationCompleted {
        run_id: RunId,
        passed: bool,
        summary: String,
    },
    ProviderStatus(ProviderStatus),
    ProviderProfilesSnapshot {
        profiles: Vec<ProviderProfile>,
    },
    ProviderModelsSnapshot {
        provider_id: String,
        models: Vec<ProviderModel>,
        selected_model: String,
    },
    TrajectoryEntryRecorded {
        session_id: SessionId,
        entry: TrajectoryEntry,
    },
    TrajectorySnapshotLoaded(TrajectorySnapshot),
    HarnessSnapshotLoaded(HarnessSnapshot),
    LayoutChanged(LayoutProfile),
    Error {
        message: String,
        recoverable: bool,
    },
    RunError {
        session_id: SessionId,
        run_id: RunId,
        message: String,
        recoverable: bool,
    },
}

impl AppEvent {
    pub const fn kind(&self) -> &'static str {
        match self {
            Self::WorkspaceStateLoaded(_) => "workspace_state_loaded",
            Self::ConversationSnapshotLoaded(_) => "conversation_snapshot_loaded",
            Self::GoalSnapshotLoaded { .. } => "goal_snapshot_loaded",
            Self::ActiveRunsSnapshot { .. } => "active_runs_snapshot",
            Self::ProjectCreated(_) => "project_created",
            Self::SessionCreated(_) => "session_created",
            Self::TaskAccepted(_) => "task_accepted",
            Self::PlanUpdated { .. } => "plan_updated",
            Self::RunStatusChanged { .. } => "run_status_changed",
            Self::AssistantDelta { .. } => "assistant_delta",
            Self::AssistantReasoningDelta { .. } => "assistant_reasoning_delta",
            Self::ModelUsageUpdated { .. } => "model_usage_updated",
            Self::ToolStarted(_) => "tool_started",
            Self::ToolFinished(_) => "tool_finished",
            Self::ApprovalRequested(_) => "approval_requested",
            Self::ApprovalResolved { .. } => "approval_resolved",
            Self::ChangeSetReady(_) => "change_set_ready",
            Self::VerificationCompleted { .. } => "verification_completed",
            Self::ProviderStatus(_) => "provider_status",
            Self::ProviderProfilesSnapshot { .. } => "provider_profiles_snapshot",
            Self::ProviderModelsSnapshot { .. } => "provider_models_snapshot",
            Self::TrajectoryEntryRecorded { .. } => "trajectory_entry_recorded",
            Self::TrajectorySnapshotLoaded(_) => "trajectory_snapshot_loaded",
            Self::HarnessSnapshotLoaded(_) => "harness_snapshot_loaded",
            Self::LayoutChanged(_) => "layout_changed",
            Self::Error { .. } => "error",
            Self::RunError { .. } => "run_error",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn provider_status_deserializes_legacy_protocol_without_profile_id() {
        let status: ProviderStatus = serde_json::from_value(serde_json::json!({
            "kind": "LocalLmStudio",
            "label": "LM Studio",
            "endpoint": "http://127.0.0.1:1234/v1/",
            "connected": false,
            "detail": "Model: local-model"
        }))
        .expect("legacy provider status");

        assert!(status.profile_id.is_empty());
    }

    #[test]
    fn conversation_message_deserializes_without_additive_model_metadata() {
        let message: ConversationMessage = serde_json::from_value(serde_json::json!({
            "id": Uuid::new_v4(),
            "project_id": Uuid::new_v4(),
            "session_id": Uuid::new_v4(),
            "run_id": Uuid::new_v4(),
            "role": "assistant",
            "text": "legacy answer",
            "status": "complete",
            "created_at": now(),
        }))
        .expect("legacy conversation message");

        assert!(message.reasoning.is_empty());
        assert!(message.usage.is_none());
        assert!(message.profile_id.is_empty());
        assert!(message.model.is_empty());
    }

    #[test]
    fn additive_run_events_preserve_session_and_run_identity() {
        let session_id = Uuid::new_v4();
        let run_id = Uuid::new_v4();
        let events = [
            AppEvent::ActiveRunsSnapshot {
                runs: vec![ActiveRunState {
                    session_id,
                    run_id,
                    lifecycle: TaskLifecycle::WaitingForApproval,
                }],
            },
            AppEvent::RunError {
                session_id,
                run_id,
                message: "bounded failure".into(),
                recoverable: true,
            },
        ];

        for event in events {
            let encoded = serde_json::to_vec(&event).expect("serialize event");
            let decoded: AppEvent = serde_json::from_slice(&encoded).expect("deserialize event");
            assert_eq!(decoded, event);
        }
    }

    #[test]
    fn model_catalog_reasoning_and_usage_events_are_additive_and_profile_scoped() {
        let session_id = Uuid::new_v4();
        let run_id = Uuid::new_v4();
        let events = [
            AppEvent::ProviderModelsSnapshot {
                provider_id: "deepseek".into(),
                models: vec![ProviderModel {
                    id: "deepseek-chat".into(),
                    label: "DeepSeek Chat".into(),
                    context_window_tokens: Some(65_536),
                }],
                selected_model: "deepseek-chat".into(),
            },
            AppEvent::AssistantReasoningDelta {
                run_id,
                text: "bounded reasoning".into(),
            },
            AppEvent::ModelUsageUpdated {
                session_id,
                run_id,
                profile_id: "deepseek".into(),
                model: "deepseek-chat".into(),
                usage: ModelUsage {
                    input_tokens: 10,
                    output_tokens: 5,
                    total_tokens: 15,
                    reasoning_tokens: Some(2),
                    cached_input_tokens: Some(3),
                    context_window_tokens: Some(65_536),
                },
            },
        ];

        for event in &events {
            let value = serde_json::to_value(event).expect("serialized event");
            let parsed = serde_json::from_value::<AppEvent>(value).expect("parsed event");
            assert_eq!(&parsed, event);
        }
        assert_eq!(events[0].kind(), "provider_models_snapshot");
        assert_eq!(events[1].kind(), "assistant_reasoning_delta");
        assert_eq!(events[2].kind(), "model_usage_updated");
    }

    #[test]
    fn trajectory_command_and_snapshot_are_additive_and_session_scoped() {
        let session_id = Uuid::new_v4();
        let run_id = Uuid::new_v4();
        let command = AppCommand::LoadTrajectory {
            session_id: Some(session_id),
        };
        let event = AppEvent::TrajectorySnapshotLoaded(TrajectorySnapshot {
            session_id,
            entries: vec![TrajectoryEntry {
                sequence: 7,
                run_id: Some(run_id),
                kind: TrajectoryKind::Lifecycle,
                state: TrajectoryState::Success,
                title: "Complete".into(),
                detail: "Run lifecycle changed.".into(),
                occurred_at: now(),
            }],
            truncated: false,
        });

        let command_value = serde_json::to_value(&command).expect("serialized command");
        assert_eq!(command_value["kind"], "load_trajectory");
        assert_eq!(command_value["session_id"], session_id.to_string());
        assert_eq!(
            serde_json::from_value::<AppCommand>(command_value).expect("parsed command"),
            command
        );

        let event_value = serde_json::to_value(&event).expect("serialized event");
        assert_eq!(
            serde_json::from_value::<AppEvent>(event_value).expect("parsed event"),
            event
        );
        assert_eq!(event.kind(), "trajectory_snapshot_loaded");
    }

    #[test]
    fn legacy_layouts_default_to_the_standard_harness_profile() {
        let value = serde_json::json!({
            "density": "Comfortable",
            "visible": {
                "Conversation": true,
                "Plan": true,
                "Changes": false,
                "Files": false,
                "Terminal": false,
                "Preview": false,
                "Activity": false,
                "Approvals": false
            },
            "widths": {}
        });
        let layout: LayoutProfile = serde_json::from_value(value).expect("legacy layout");
        assert_eq!(layout.harness_profile_id, "standard");
    }

    #[test]
    fn legacy_harness_snapshots_default_to_confirm_before_changes() {
        let snapshot: HarnessSnapshot = serde_json::from_value(serde_json::json!({
            "selected_profile_id": "standard",
            "profiles": [],
            "capabilities": [],
            "surfaces": []
        }))
        .expect("legacy harness snapshot");

        assert_eq!(
            snapshot.selected_execution_profile_id,
            DEFAULT_EXECUTION_PROFILE_ID
        );
        assert!(snapshot.execution_profiles.is_empty());
    }
}
