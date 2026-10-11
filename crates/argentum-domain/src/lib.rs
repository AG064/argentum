use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use uuid::Uuid;

mod authority;
pub mod context_budget;
pub mod context_summary;
pub mod extensions;
pub mod final_result;
pub mod goal_accounting;
pub mod goal_supervision;
pub mod process_job_contract;
pub mod recovery;
pub mod supplemental;
pub mod text_contract;
pub mod working_context;

pub use authority::{
    is_valid_capability_id, AuthorityProjectionError, PlatformAuthoritySnapshot,
    PlatformAuthorityState, PlatformAuthorityTier, PlatformCapabilityProbeSnapshot,
};

pub type ProjectId = Uuid;
pub type SessionId = Uuid;
pub type TaskId = Uuid;
pub type RunId = Uuid;
pub type MessageId = Uuid;
pub type ApprovalId = Uuid;
pub type ToolCallId = Uuid;
pub type GoalId = Uuid;
pub type ChangeSetId = Uuid;
pub type FileChangeId = Uuid;
pub type ChangeOperationId = Uuid;
pub type GrantId = Uuid;
pub type AuditId = Uuid;
pub type VerificationId = Uuid;

/// Continuation in the retained historical change journal, not file authority.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ChangePageCursor {
    pub project_id: ProjectId,
    pub session_id: SessionId,
    pub run_id: RunId,
    pub stamp: i64,
    pub sequence: i64,
    pub id: Uuid,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChangePageItem {
    pub id: Uuid,
    pub file_change_id: Option<FileChangeId>,
    pub operation_id: Option<ChangeOperationId>,
    pub path: String,
    pub kind: String,
    pub state: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChangePage {
    pub run_id: RunId,
    pub items: Vec<ChangePageItem>,
    pub next_cursor: Option<ChangePageCursor>,
    pub complete: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct Sha256Digest(pub String);

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SandboxScope {
    ReadOnly,
    #[default]
    WorkspaceWrite,
    Unrestricted,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ApprovalMode {
    #[default]
    Always,
    OnRisk,
    OnFailure,
    Never,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolRisk {
    ReadOnly,
    Mutation,
    Process,
    Shell,
    GitRead,
    GitMutation,
    Verification,
}

impl ToolRisk {
    pub const fn requires_risk_approval(self) -> bool {
        !matches!(self, Self::ReadOnly | Self::GitRead)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PatchFile {
    pub path: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expected_sha256: Option<Sha256Digest>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum GitOperation {
    Status {
        short: bool,
    },
    Diff {
        cached: bool,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        path: Option<String>,
    },
    Log {
        max_count: u16,
    },
    Show {
        revision: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        path: Option<String>,
    },
    BranchList,
    BranchCreate {
        name: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        start_point: Option<String>,
    },
    BranchSwitch {
        name: String,
    },
    WorktreeList,
    WorktreeCreate {
        path: String,
        branch: String,
        create_branch: bool,
    },
    Add {
        paths: Vec<String>,
    },
    Commit {
        message: String,
    },
    Restore {
        path: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        source: Option<String>,
    },
}

impl GitOperation {
    pub const fn risk(&self) -> ToolRisk {
        match self {
            Self::Status { .. }
            | Self::Diff { .. }
            | Self::Log { .. }
            | Self::Show { .. }
            | Self::BranchList
            | Self::WorktreeList => ToolRisk::GitRead,
            Self::BranchCreate { .. }
            | Self::BranchSwitch { .. }
            | Self::WorktreeCreate { .. }
            | Self::Add { .. }
            | Self::Commit { .. }
            | Self::Restore { .. } => ToolRisk::GitMutation,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum VerificationRunnerId {
    CargoFmtCheck,
    CargoCheckLocked,
    CargoTestLocked,
    CargoClippyStrict,
    Project(String),
}

impl Serialize for VerificationRunnerId {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&self.id())
    }
}

impl<'de> Deserialize<'de> for VerificationRunnerId {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        match value.as_str() {
            "cargo-fmt-check" => Ok(Self::CargoFmtCheck),
            "cargo-check-locked" => Ok(Self::CargoCheckLocked),
            "cargo-test-locked" => Ok(Self::CargoTestLocked),
            "cargo-clippy-strict" => Ok(Self::CargoClippyStrict),
            value
                if value
                    .strip_prefix("project:")
                    .is_some_and(|id| !id.is_empty()) =>
            {
                Ok(Self::Project(value[8..].to_owned()))
            }
            _ => Err(serde::de::Error::custom("unknown verification runner ID")),
        }
    }
}

impl VerificationRunnerId {
    pub fn id(&self) -> String {
        match self {
            Self::CargoFmtCheck => "cargo-fmt-check".into(),
            Self::CargoCheckLocked => "cargo-check-locked".into(),
            Self::CargoTestLocked => "cargo-test-locked".into(),
            Self::CargoClippyStrict => "cargo-clippy-strict".into(),
            Self::Project(id) => format!("project:{id}"),
        }
    }

    pub fn project_id(&self) -> Option<&str> {
        match self {
            Self::Project(id) => Some(id),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VerificationRunnerInspection {
    pub id: String,
    pub origin: String,
    pub available: bool,
    pub approval_required: bool,
    pub reason: Option<String>,
    pub definition_sha256: Option<Sha256Digest>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TextSearchMode {
    #[default]
    Literal,
    Regex,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ToolInput {
    ListDirectory {
        path: String,
        max_entries: u16,
    },
    FileMetadata {
        path: String,
    },
    ReadText {
        path: String,
        #[serde(default)]
        start_byte: Option<u64>,
        #[serde(default)]
        end_byte: Option<u64>,
        #[serde(default)]
        start_line: Option<u64>,
        #[serde(default)]
        line_count: Option<u16>,
        #[serde(default)]
        expected_sha256: Option<Sha256Digest>,
    },
    ReadBinaryMetadata {
        path: String,
    },
    SearchText {
        path: String,
        query: String,
        max_results: u16,
        #[serde(default)]
        mode: TextSearchMode,
        #[serde(default)]
        before_context: u8,
        #[serde(default)]
        after_context: u8,
    },
    SearchFilenames {
        path: String,
        pattern: String,
        max_results: u16,
    },
    CreateDirectory {
        path: String,
    },
    ApplyPatch {
        files: Vec<PatchFile>,
    },
    WriteText {
        path: String,
        content: String,
    },
    EditText {
        path: String,
        expected_sha256: Sha256Digest,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        old_text: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        new_text: Option<String>,
        #[serde(
            default,
            skip_serializing_if = "crate::text_contract::TextNewlineMode::is_exact"
        )]
        newline_mode: crate::text_contract::TextNewlineMode,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        start_line: Option<u64>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        line_count: Option<u16>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        replacement_lines: Option<Vec<String>>,
    },
    MoveFile {
        from: String,
        to: String,
    },
    CopyFile {
        from: String,
        to: String,
    },
    DeleteFile {
        path: String,
    },
    ExecuteArgv {
        program: String,
        args: Vec<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        cwd: Option<String>,
        timeout_ms: u64,
        #[serde(default)]
        environment: BTreeMap<String, String>,
    },
    ExecuteShell {
        command: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        cwd: Option<String>,
        timeout_ms: u64,
        #[serde(default)]
        environment: BTreeMap<String, String>,
    },
    Git {
        operation: GitOperation,
    },
    Verify {
        runners: Vec<VerificationRunnerId>,
    },
    StartVerification {
        runner: VerificationRunnerId,
    },
    CheckRust {
        package: String,
        #[serde(default)]
        profile: supplemental::RustCheckProfile,
        test_source: supplemental::RustCheckSource,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        replaces_check_id: Option<Uuid>,
    },
    PollProcessJob {
        job_id: String,
        stdout_cursor: u64,
        stderr_cursor: u64,
        wait_ms: u64,
    },
    CancelProcessJob {
        job_id: String,
    },
    Extension {
        operation: extensions::ExtensionOperation,
    },
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
    Interrupted,
    Cancelled,
    Failed,
}

impl TaskLifecycle {
    pub const fn is_terminal(self) -> bool {
        matches!(
            self,
            Self::Complete | Self::Interrupted | Self::Cancelled | Self::Failed
        )
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
            Self::Interrupted => "Interrupted",
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
    ExecuteShell,
    GitRead,
    GitWrite,
    Verify,
    Network,
    ExternalProcess,
    UseExtensions,
}

impl Capability {
    pub const fn label(self) -> &'static str {
        match self {
            Self::ReadFiles => "Read files",
            Self::WriteFiles => "Write files",
            Self::ExecuteCommands => "Execute commands",
            Self::ExecuteShell => "Execute shell commands",
            Self::GitRead => "Read Git state",
            Self::GitWrite => "Change Git state",
            Self::Verify => "Run verification",
            Self::Network => "Network",
            Self::ExternalProcess => "External process",
            Self::UseExtensions => "Use reviewed extension tools",
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

pub const DEFAULT_EXECUTION_PROFILE_ID: &str = "guided";
pub const CUSTOM_EXECUTION_PROFILE_ID: &str = "custom";
pub const LEGACY_READ_ONLY_EXECUTION_PROFILE_ID: &str = "read-only";
pub const LEGACY_GUIDED_EXECUTION_PROFILE_ID: &str = "confirm-before-changes";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HarnessExecutionPolicy {
    #[serde(default = "default_execution_profile_id")]
    pub profile_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_profile_id: Option<String>,
    #[serde(default)]
    pub sandbox_scope: SandboxScope,
    #[serde(default)]
    pub approval_mode: ApprovalMode,
    #[serde(default = "default_execution_capabilities")]
    pub capability_enabled: BTreeMap<String, bool>,
}

impl Default for HarnessExecutionPolicy {
    fn default() -> Self {
        Self {
            profile_id: default_execution_profile_id(),
            source_profile_id: Some(default_execution_profile_id()),
            sandbox_scope: SandboxScope::WorkspaceWrite,
            approval_mode: ApprovalMode::Always,
            capability_enabled: default_execution_capabilities(),
        }
    }
}

fn default_execution_profile_id() -> String {
    DEFAULT_EXECUTION_PROFILE_ID.to_owned()
}

fn default_execution_capabilities() -> BTreeMap<String, bool> {
    BTreeMap::from([
        ("tool.list-directory".to_owned(), true),
        ("tool.file-metadata".to_owned(), true),
        ("tool.read-text".to_owned(), true),
        ("tool.read-binary-metadata".to_owned(), true),
        ("tool.search-text".to_owned(), true),
        ("tool.search-filenames".to_owned(), true),
        ("tool.create-directory".to_owned(), true),
        ("tool.apply-patch".to_owned(), true),
        ("tool.write-text".to_owned(), true),
        ("tool.edit-text".to_owned(), true),
        ("tool.move-file".to_owned(), true),
        ("tool.copy-file".to_owned(), true),
        ("tool.delete-file".to_owned(), true),
        ("execution.argv".to_owned(), true),
        ("execution.shell".to_owned(), true),
        ("git.read".to_owned(), true),
        ("git.write".to_owned(), true),
        ("verification.runner".to_owned(), true),
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
    #[serde(default)]
    pub diagnostics: Option<DiagnosticSnapshot>,
    #[serde(default)]
    pub local_backend: Option<HarnessBackendState>,
    #[serde(default)]
    pub modules: Vec<HarnessModuleState>,
    pub selected_profile_id: String,
    pub profiles: Vec<HarnessProfileSummary>,
    #[serde(default = "default_execution_profile_id")]
    pub selected_execution_profile_id: String,
    #[serde(default)]
    pub execution_profiles: Vec<HarnessExecutionProfileSummary>,
    pub capabilities: Vec<HarnessCapabilityState>,
    pub surfaces: Vec<HarnessSurfaceState>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModulePreference {
    pub installed: bool,
    pub enabled: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiagnosticRecord {
    pub sequence: u64,
    pub occurred_at: OffsetDateTime,
    pub code: String,
    pub component: String,
    pub line: Option<u32>,
    pub level: String,
    pub run_id: Option<RunId>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiagnosticSnapshot {
    pub records: Vec<DiagnosticRecord>,
    pub recorded: u64,
    pub dropped: u64,
    pub evicted: u64,
    pub degraded: bool,
}

#[derive(Debug, Clone, Copy)]
pub enum ModelDiagnosticPhase {
    ArtifactPreparing,
    ArtifactReady,
    Preparing,
    Prepared,
    Loading,
    Ready,
    Unloading,
    Unloaded,
    Failed,
}

impl ModelDiagnosticPhase {
    pub const fn code(self) -> &'static str {
        match self {
            Self::ArtifactPreparing => "model_artifact_preparing",
            Self::ArtifactReady => "model_artifact_ready",
            Self::Preparing => "model_preparing",
            Self::Prepared => "model_prepared",
            Self::Loading => "model_loading",
            Self::Ready => "model_ready",
            Self::Unloading => "model_unloading",
            Self::Unloaded => "model_unloaded",
            Self::Failed => "model_failed",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HarnessBackendState {
    pub id: String,
    pub enabled: bool,
    pub changing: bool,
    pub recovery_pending: bool,
    pub toggle_supported: bool,
    pub detail: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HarnessModuleState {
    pub id: String,
    pub label: String,
    pub installed: bool,
    pub enabled: bool,
    pub recovery_pending: bool,
    pub detail: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ModuleAction {
    Install,
    Enable,
    Disable,
    Remove,
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

    pub const fn is_inspector(self) -> bool {
        matches!(self, Self::Changes | Self::Activity | Self::Trajectory)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Density {
    Compact,
    Comfortable,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct LayoutProfile {
    #[serde(default = "default_harness_profile_id")]
    pub harness_profile_id: String,
    pub density: Density,
    pub visible: BTreeMap<SurfaceId, bool>,
    pub widths: BTreeMap<SurfaceId, u32>,
    pub inspector_collapsed: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub active_inspector: Option<SurfaceId>,
}

#[derive(Deserialize)]
struct LayoutProfileWire {
    #[serde(default = "default_harness_profile_id")]
    harness_profile_id: String,
    density: Density,
    visible: BTreeMap<SurfaceId, bool>,
    widths: BTreeMap<SurfaceId, u32>,
    #[serde(default)]
    inspector_collapsed: Option<bool>,
    #[serde(default)]
    active_inspector: Option<SurfaceId>,
}

impl<'de> Deserialize<'de> for LayoutProfile {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let wire = LayoutProfileWire::deserialize(deserializer)?;
        if wire
            .active_inspector
            .is_some_and(|surface| !surface.is_inspector())
        {
            return Err(serde::de::Error::custom(
                "active_inspector must be Changes, Activity, or Trajectory",
            ));
        }

        let legacy = wire.inspector_collapsed.is_none();
        let active_inspector = if legacy {
            first_visible_inspector(&wire.visible)
        } else {
            wire.active_inspector
        };
        let inspector_collapsed = wire
            .inspector_collapsed
            .unwrap_or_else(|| active_inspector.is_none());

        Ok(Self {
            harness_profile_id: wire.harness_profile_id,
            density: wire.density,
            visible: wire.visible,
            widths: wire.widths,
            inspector_collapsed,
            active_inspector,
        })
    }
}

fn first_visible_inspector(visible: &BTreeMap<SurfaceId, bool>) -> Option<SurfaceId> {
    [
        SurfaceId::Changes,
        SurfaceId::Activity,
        SurfaceId::Trajectory,
    ]
    .into_iter()
    .find(|surface| visible.get(surface).copied().unwrap_or(false))
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
            inspector_collapsed: true,
            active_inspector: None,
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum SessionArchiveFilter {
    #[default]
    Active,
    Archived,
    All,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionListRow {
    pub id: SessionId,
    pub title: String,
    pub created_at: OffsetDateTime,
    pub last_activity_at: OffsetDateTime,
    pub pinned: bool,
    pub archived: bool,
    pub selected: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionListSnapshot {
    pub project_id: ProjectId,
    pub rows: Vec<SessionListRow>,
    pub has_more: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub next_cursor: Option<String>,
    pub query: String,
    pub filter: SessionArchiveFilter,
    pub limit: u16,
}

fn default_session_list_limit() -> u16 {
    20
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionListQuery {
    #[serde(default)]
    pub query: String,
    #[serde(default)]
    pub filter: SessionArchiveFilter,
    #[serde(default = "default_session_list_limit")]
    pub limit: u16,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cursor: Option<String>,
}

pub const SESSION_FORK_WARNING: &str =
    "This forks conversation context only. Workspace files are shared and are not restored or copied.";
pub const MAX_SESSION_FORK_SOURCES: usize = 128;
pub const MAX_SESSION_FORK_MESSAGES: usize = 200;
pub const MAX_SESSION_FORK_METADATA_BYTES: usize = 64 * 1024;
pub const MAX_SESSION_FORK_LINEAGE_DEPTH: u8 = 1;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum SessionForkSource {
    ContextGroup { run_id: RunId, ordinal: u32 },
    LegacyMessage { message_id: MessageId },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SessionForkCutoff {
    pub last_message_id: MessageId,
    pub last_source: SessionForkSource,
    pub source_count: usize,
    pub message_count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SessionForkPreview {
    pub project_id: ProjectId,
    pub source_session_id: SessionId,
    pub source_title: String,
    pub destination_title: String,
    pub workspace: PathBuf,
    pub cutoff: SessionForkCutoff,
    pub omitted_older_sources: u64,
    pub warning: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SessionForkSnapshot {
    pub version: u16,
    pub project_id: ProjectId,
    pub child_session_id: SessionId,
    pub source_session_id: SessionId,
    pub source_title_at_fork: String,
    pub destination_title: String,
    pub cutoff: SessionForkCutoff,
    pub sources: Vec<SessionForkSource>,
    pub source_message_ids: Vec<MessageId>,
    pub omitted_older_sources: u64,
    pub created_at: OffsetDateTime,
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
        matches!(self, Self::Paused | Self::Complete)
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
    #[serde(default)]
    pub accounting: Option<goal_accounting::GoalAccounting>,
    #[serde(default)]
    pub definition_revision: Uuid,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub acceptance: Option<goal_supervision::GoalAcceptanceDefinition>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub owner_attestation: Option<goal_supervision::GoalOwnerAttestation>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub completion: Option<goal_supervision::GoalCompletionReceipt>,
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

pub const ONBOARDING_SCHEMA_VERSION: u16 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GuidanceLevel {
    Guided,
    Expert,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OnboardingStep {
    Guidance,
    Provider,
    Model,
    Workspace,
    Permission,
    Review,
    Complete,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OnboardingCredentialState {
    NotRequired,
    Missing,
    Configured,
    Unavailable,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OnboardingConnectionState {
    NotTested,
    Testing,
    Reachable,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OnboardingSnapshot {
    pub version: u16,
    pub step: OnboardingStep,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub guidance: Option<GuidanceLevel>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workspace: Option<PathBuf>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub permission_profile_id: Option<String>,
    pub credential_state: OnboardingCredentialState,
    pub connection_state: OnboardingConnectionState,
    pub complete: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum OnboardingAction {
    Begin,
    SelectGuidance { level: GuidanceLevel },
    SelectProvider { provider_id: String },
    SelectModel { provider_id: String, model: String },
    SelectWorkspace { workspace: PathBuf },
    SelectPermission { profile_id: String },
    ConfirmReview,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum OnboardingError {
    #[error("onboarding step is {actual:?}; expected {expected:?}")]
    InvalidStep {
        actual: OnboardingStep,
        expected: OnboardingStep,
    },
    #[error("onboarding {field} must not be empty")]
    EmptyField { field: &'static str },
    #[error("onboarding {field} is too long")]
    ValueTooLong { field: &'static str },
    #[error("onboarding {field} contains a control character")]
    ControlCharacter { field: &'static str },
    #[error("onboarding provider does not match the selected provider")]
    ProviderMismatch,
    #[error("onboarding cannot be completed before the review step")]
    Incomplete,
    #[error("onboarding snapshot version is unsupported")]
    UnsupportedVersion,
}

impl OnboardingSnapshot {
    pub fn initial() -> Self {
        Self {
            version: ONBOARDING_SCHEMA_VERSION,
            step: OnboardingStep::Guidance,
            guidance: None,
            provider_id: None,
            model: None,
            workspace: None,
            permission_profile_id: None,
            credential_state: OnboardingCredentialState::NotRequired,
            connection_state: OnboardingConnectionState::NotTested,
            complete: false,
        }
    }

    pub fn legacy_complete(
        provider_id: impl Into<String>,
        model: impl Into<String>,
        workspace: PathBuf,
        permission_profile_id: impl Into<String>,
    ) -> Self {
        Self {
            version: ONBOARDING_SCHEMA_VERSION,
            step: OnboardingStep::Complete,
            guidance: Some(GuidanceLevel::Expert),
            provider_id: Some(provider_id.into()),
            model: Some(model.into()),
            workspace: Some(workspace),
            permission_profile_id: Some(permission_profile_id.into()),
            credential_state: OnboardingCredentialState::NotRequired,
            connection_state: OnboardingConnectionState::NotTested,
            complete: true,
        }
    }

    pub fn normalized(mut self) -> Result<Self, OnboardingError> {
        self.validate_values()?;
        if self.version != ONBOARDING_SCHEMA_VERSION {
            return Err(OnboardingError::UnsupportedVersion);
        }
        if self.complete
            && (self.guidance.is_none()
                || self.provider_id.is_none()
                || self.model.is_none()
                || self.workspace.is_none()
                || self.permission_profile_id.is_none()
                || matches!(
                    self.credential_state,
                    OnboardingCredentialState::Missing | OnboardingCredentialState::Unavailable
                ))
        {
            return Err(OnboardingError::Incomplete);
        }
        self.step = self.derived_step();
        Ok(self)
    }

    pub fn apply(&mut self, action: OnboardingAction) -> Result<(), OnboardingError> {
        match action {
            OnboardingAction::Begin => {
                *self = Self::initial();
            }
            OnboardingAction::SelectGuidance { level } => {
                self.require_step(OnboardingStep::Guidance)?;
                self.guidance = Some(level);
                self.provider_id = None;
                self.model = None;
                self.workspace = None;
                self.permission_profile_id = None;
                self.complete = false;
                self.credential_state = OnboardingCredentialState::NotRequired;
                self.connection_state = OnboardingConnectionState::NotTested;
                self.recompute_step();
            }
            OnboardingAction::SelectProvider { provider_id } => {
                self.require_step(OnboardingStep::Provider)?;
                validate_text("provider ID", &provider_id, 64)?;
                self.provider_id = Some(provider_id.trim().to_owned());
                self.model = None;
                self.workspace = None;
                self.permission_profile_id = None;
                self.complete = false;
                self.credential_state = OnboardingCredentialState::NotRequired;
                self.connection_state = OnboardingConnectionState::NotTested;
                self.recompute_step();
            }
            OnboardingAction::SelectModel { provider_id, model } => {
                self.require_step(OnboardingStep::Model)?;
                if self.provider_id.as_deref() != Some(provider_id.trim()) {
                    return Err(OnboardingError::ProviderMismatch);
                }
                validate_text("model", &model, 256)?;
                self.model = Some(model.trim().to_owned());
                self.workspace = None;
                self.permission_profile_id = None;
                self.complete = false;
                self.connection_state = OnboardingConnectionState::NotTested;
                self.recompute_step();
            }
            OnboardingAction::SelectWorkspace { workspace } => {
                self.require_step(OnboardingStep::Workspace)?;
                validate_path(&workspace)?;
                self.workspace = Some(workspace);
                self.permission_profile_id = None;
                self.complete = false;
                self.recompute_step();
            }
            OnboardingAction::SelectPermission { profile_id } => {
                self.require_step(OnboardingStep::Permission)?;
                validate_text("permission profile ID", &profile_id, 64)?;
                self.permission_profile_id = Some(profile_id.trim().to_owned());
                self.complete = false;
                self.recompute_step();
            }
            OnboardingAction::ConfirmReview => {
                self.require_step(OnboardingStep::Review)?;
                if self.provider_id.is_none()
                    || self.model.is_none()
                    || self.workspace.is_none()
                    || self.permission_profile_id.is_none()
                {
                    return Err(OnboardingError::Incomplete);
                }
                self.complete = true;
                self.step = OnboardingStep::Complete;
            }
        }
        self.validate_values()
    }

    pub fn derived_step(&self) -> OnboardingStep {
        if self.complete {
            OnboardingStep::Complete
        } else if self.guidance.is_none() {
            OnboardingStep::Guidance
        } else if self.provider_id.is_none() {
            OnboardingStep::Provider
        } else if self.model.is_none() {
            OnboardingStep::Model
        } else if self.workspace.is_none() {
            OnboardingStep::Workspace
        } else if self.permission_profile_id.is_none() {
            OnboardingStep::Permission
        } else {
            OnboardingStep::Review
        }
    }

    fn require_step(&self, expected: OnboardingStep) -> Result<(), OnboardingError> {
        if self.step == expected {
            Ok(())
        } else {
            Err(OnboardingError::InvalidStep {
                actual: self.step,
                expected,
            })
        }
    }

    fn recompute_step(&mut self) {
        self.step = self.derived_step();
    }

    fn validate_values(&self) -> Result<(), OnboardingError> {
        if let Some(provider_id) = &self.provider_id {
            validate_text("provider ID", provider_id, 64)?;
        }
        if let Some(model) = &self.model {
            validate_text("model", model, 256)?;
        }
        if let Some(workspace) = &self.workspace {
            validate_path(workspace)?;
        }
        if let Some(profile_id) = &self.permission_profile_id {
            validate_text("permission profile ID", profile_id, 64)?;
        }
        Ok(())
    }
}

fn validate_text(
    field: &'static str,
    value: &str,
    max_chars: usize,
) -> Result<(), OnboardingError> {
    if value.trim().is_empty() {
        return Err(OnboardingError::EmptyField { field });
    }
    if value.chars().count() > max_chars {
        return Err(OnboardingError::ValueTooLong { field });
    }
    if value.chars().any(char::is_control) {
        return Err(OnboardingError::ControlCharacter { field });
    }
    Ok(())
}

fn validate_path(path: &Path) -> Result<(), OnboardingError> {
    let value = path.to_string_lossy();
    if value.trim().is_empty() {
        return Err(OnboardingError::EmptyField { field: "workspace" });
    }
    if value.chars().count() > 4096 {
        return Err(OnboardingError::ValueTooLong { field: "workspace" });
    }
    if value.chars().any(char::is_control) {
        return Err(OnboardingError::ControlCharacter { field: "workspace" });
    }
    Ok(())
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
    pub capability_id: String,
    pub title: String,
    pub summary: String,
    pub capabilities: Vec<Capability>,
    pub risk: ToolRisk,
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
    NotExecuted,
}

pub type SteeringEntryId = Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SteeringLifecycle {
    Pending,
    Included,
    NotApplied,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SteeringStatus {
    pub version: u16,
    pub entry_id: SteeringEntryId,
    pub project_id: ProjectId,
    pub session_id: SessionId,
    pub run_id: RunId,
    pub position: u64,
    pub text_bytes: u32,
    pub lifecycle: SteeringLifecycle,
    pub reason: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ApprovalRequest {
    pub id: ApprovalId,
    pub run_id: RunId,
    pub tool_id: String,
    pub action: String,
    pub target: String,
    pub binding: Sha256Digest,
    pub reason: String,
    pub capabilities: Vec<Capability>,
    pub expires_at: Option<OffsetDateTime>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ApprovalScope {
    Once,
    Action,
    ToolCall,
    Run,
    Session,
    Workspace,
    PersistentProfile,
}

impl ApprovalScope {
    pub const fn grant_scope(self) -> GrantScope {
        match self {
            Self::Once | Self::ToolCall => GrantScope::ToolCall,
            Self::Action => GrantScope::Action,
            Self::Run => GrantScope::Run,
            Self::Session => GrantScope::Session,
            Self::Workspace => GrantScope::Workspace,
            Self::PersistentProfile => GrantScope::PersistentProfile,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GrantScope {
    Action,
    ToolCall,
    Run,
    Session,
    Workspace,
    PersistentProfile,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum GrantSubject {
    Action {
        tool_id: String,
        target: String,
        binding: Sha256Digest,
    },
    ToolCall {
        tool_call_id: ToolCallId,
    },
    Run {
        run_id: RunId,
    },
    Session {
        session_id: SessionId,
    },
    Workspace {
        project_id: ProjectId,
    },
    PersistentProfile {
        profile_id: String,
    },
}

impl GrantSubject {
    pub const fn scope(&self) -> GrantScope {
        match self {
            Self::Action { .. } => GrantScope::Action,
            Self::ToolCall { .. } => GrantScope::ToolCall,
            Self::Run { .. } => GrantScope::Run,
            Self::Session { .. } => GrantScope::Session,
            Self::Workspace { .. } => GrantScope::Workspace,
            Self::PersistentProfile { .. } => GrantScope::PersistentProfile,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GrantIssuer {
    User,
    Policy,
    System,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ApprovalGrantRecord {
    pub id: GrantId,
    pub audit_id: AuditId,
    pub capability: Capability,
    pub scope: GrantScope,
    pub subject: GrantSubject,
    pub created_at: OffsetDateTime,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<OffsetDateTime>,
    pub issuer: GrantIssuer,
    pub reason: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub revoked_at: Option<OffsetDateTime>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ApprovalGrantHistoryCoverage {
    pub limit: u64,
    pub returned: u64,
    pub total_count: u64,
    pub complete: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExecutionOutcome {
    Running,
    Succeeded,
    Failed,
    Cancelled,
    TimedOut,
    SpawnFailed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExecutionAuditRecord {
    pub id: AuditId,
    pub project_id: ProjectId,
    pub session_id: SessionId,
    pub run_id: RunId,
    pub tool_call_id: ToolCallId,
    pub tool_id: String,
    pub profile_id: String,
    pub sandbox_scope: SandboxScope,
    pub approval_mode: ApprovalMode,
    pub capabilities: Vec<Capability>,
    pub grant_ids: Vec<GrantId>,
    pub target: String,
    pub outcome: ExecutionOutcome,
    pub started_at: OffsetDateTime,
    pub finished_at: OffsetDateTime,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exit_code: Option<i32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stdout_sha256: Option<Sha256Digest>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stderr_sha256: Option<Sha256Digest>,
    pub stdout_bytes: u64,
    pub stderr_bytes: u64,
    pub stdout_truncated: bool,
    pub stderr_truncated: bool,
    pub changed_paths: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VerificationState {
    Passed,
    Failed,
    Cancelled,
    TimedOut,
    SpawnFailed,
    Stale,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VerificationEvidence {
    pub id: VerificationId,
    pub run_id: RunId,
    pub runner: VerificationRunnerId,
    pub registry_version: u16,
    #[serde(default)]
    pub program: String,
    #[serde(default)]
    pub args: Vec<String>,
    pub state: VerificationState,
    pub started_at: OffsetDateTime,
    pub duration_ms: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exit_code: Option<i32>,
    pub revision_before: Sha256Digest,
    pub revision_after: Sha256Digest,
    pub stdout_sha256: Sha256Digest,
    pub stderr_sha256: Sha256Digest,
    pub stdout_bytes: u64,
    pub stderr_bytes: u64,
    pub stdout_truncated: bool,
    pub stderr_truncated: bool,
    #[serde(default)]
    pub workspace_key: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub definition_sha256: Option<Sha256Digest>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub input_scope: Option<goal_supervision::VerificationInputScope>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub input_before: Option<goal_supervision::VerificationInputSnapshot>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub input_after: Option<goal_supervision::VerificationInputSnapshot>,
}

impl VerificationEvidence {
    pub fn is_fresh_for(&self, revision: &Sha256Digest) -> bool {
        self.state == VerificationState::Passed
            && self.revision_before == self.revision_after
            && &self.revision_after == revision
    }
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
#[serde(rename_all = "snake_case")]
pub enum FileChangeKind {
    Added,
    Modified,
    Deleted,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FileChangeOrigin {
    ToolCall,
    Restore,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileSnapshotRef {
    pub digest: Sha256Digest,
    pub size_bytes: u64,
    pub retained: bool,
}

/// Durability of a file change's recorded numeric line counts.
///
/// Consumers must read this instead of inferring meaning from the numeric
/// `additions` and `removals` values. A zero numeric count alone cannot
/// distinguish a genuine zero-line change from a change whose counts could
/// not be derived.
///
/// * `Computed` means the stored `additions` and `removals` were derived at
///   preparation time from the required admitted readable snapshots, and a
///   genuine zero result is included.
/// * `Unavailable` means a required existing image could not be read as text
///   because its bytes were not retained, it contained NUL bytes, or it was
///   not valid UTF-8. Real absence in creation or deletion is not missing
///   evidence and does not produce this state.
/// * `Unknown` means the provenance of the numeric counts was not recorded,
///   for example a record written before this metadata existed. It is the
///   safe default for missing serialized metadata, regardless of the numeric
///   values, today's files or current retention flags.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LineCountAvailability {
    Computed,
    Unavailable,
    #[default]
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileChangeRecord {
    pub id: FileChangeId,
    pub change_set_id: ChangeSetId,
    pub project_id: ProjectId,
    pub session_id: SessionId,
    pub run_id: RunId,
    pub revision: u64,
    pub relative_path: String,
    pub kind: FileChangeKind,
    pub origin: FileChangeOrigin,
    pub before: Option<FileSnapshotRef>,
    pub after: Option<FileSnapshotRef>,
    pub additions: u32,
    pub removals: u32,
    /// Provenance of the numeric `additions`/`removals` values. Additive and
    /// backward compatible: records written before this field existed decode
    /// as `Unknown` rather than inferring anything from the numbers.
    #[serde(default)]
    pub line_count_availability: LineCountAvailability,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub restored_from: Option<FileChangeId>,
    pub created_at: OffsetDateTime,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChangeSetSnapshot {
    pub id: ChangeSetId,
    pub project_id: ProjectId,
    pub session_id: SessionId,
    pub run_id: RunId,
    pub revision: u64,
    pub files: Vec<FileChangeRecord>,
    pub additions: u32,
    pub removals: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UnifiedDiffSnapshot {
    pub file_change_id: FileChangeId,
    pub relative_path: String,
    pub text: String,
    pub line_count: u32,
    pub byte_count: u64,
    pub truncated: bool,
    pub available: bool,
    pub unavailable_reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RestoreResult {
    pub run_id: RunId,
    pub source_file_change_id: FileChangeId,
    pub restore_file_change_id: FileChangeId,
    pub relative_path: String,
    pub deleted: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChangeRecoveryState {
    Prepared,
    Applied,
    Finalized,
    NotApplied,
    Conflicted,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChangeRecoveryEntry {
    pub operation_id: ChangeOperationId,
    pub relative_path: String,
    pub pre_image_digest: Option<Sha256Digest>,
    pub proposed_post_image_digest: Option<Sha256Digest>,
    pub current_digest: Option<Sha256Digest>,
    pub created_at: OffsetDateTime,
    pub state: ChangeRecoveryState,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChangeRecoverySnapshot {
    pub operations: Vec<ChangeRecoveryEntry>,
    pub recovery_required: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ProviderKind {
    OpenAiCompatible,
    Anthropic,
    LocalLmStudio,
    LocalLlamaCpp,
    Unknown,
}

pub const DEFAULT_PROVIDER_ID: &str = "llama-cpp";
pub const DEFAULT_LLAMA_CPP_ENDPOINT: &str = "local://llama-cpp/";
pub const DEFAULT_LLAMA_CPP_MODEL: &str = "Qwen3.5-0.8B-Q8_0.gguf";
pub const DEFAULT_LM_STUDIO_ENDPOINT: &str = "http://127.0.0.1:1234/v1/";
pub const DEFAULT_MODEL: &str = DEFAULT_LLAMA_CPP_MODEL;
pub const HOSTED_CREDENTIAL_PROVIDER_IDS: [&str; 3] = ["openai", "minimax", "deepseek"];

pub fn is_supported_hosted_credential_provider_id(profile_id: &str) -> bool {
    matches!(profile_id, "openai" | "minimax" | "deepseek")
}

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
    pub fn default_llama_cpp() -> Self {
        Self {
            id: DEFAULT_PROVIDER_ID.into(),
            label: "llama.cpp".into(),
            kind: ProviderKind::LocalLlamaCpp,
            endpoint: DEFAULT_LLAMA_CPP_ENDPOINT.into(),
            model: DEFAULT_LLAMA_CPP_MODEL.into(),
            selected: true,
        }
    }

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

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProviderCatalogSource {
    #[default]
    Live,
    Cached,
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
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum AppCommand {
    NewSession,
    ForkSession {
        source_session_id: SessionId,
        title: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        expected_last_message_id: Option<MessageId>,
    },
    SelectSession {
        session_id: SessionId,
    },
    ListSessions {
        #[serde(default)]
        query: String,
        #[serde(default)]
        filter: SessionArchiveFilter,
        #[serde(default = "default_session_list_limit")]
        limit: u16,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        cursor: Option<String>,
    },
    InspectSession {
        session_id: SessionId,
    },
    SetSessionTitle {
        session_id: SessionId,
        title: String,
    },
    SetSessionPinned {
        session_id: SessionId,
        pinned: bool,
    },
    SetSessionArchived {
        session_id: SessionId,
        archived: bool,
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
    AddProviderPreset {
        preset_id: String,
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
    LoadOnboarding,
    BeginOnboarding,
    SetOnboardingGuidance {
        guidance: GuidanceLevel,
    },
    SetOnboardingProvider {
        provider_id: String,
    },
    SetOnboardingModel {
        provider_id: String,
        model: String,
    },
    SetOnboardingWorkspace {
        workspace: PathBuf,
    },
    SetOnboardingPermission {
        profile_id: String,
    },
    CompleteOnboarding,
    ListHarnessState,
    SetLocalBackendEnabled {
        enabled: bool,
    },
    SetHarnessModule {
        module_id: String,
        action: ModuleAction,
    },
    SelectHarnessProfile {
        profile_id: String,
    },
    SetSurfaceVisibility {
        surface: SurfaceId,
        visible: bool,
    },
    SetInspectorSurface {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        surface: Option<SurfaceId>,
    },
    SelectExecutionProfile {
        profile_id: String,
    },
    SetHarnessCapabilityEnabled {
        capability_id: String,
        enabled: bool,
    },
    ListApprovalGrants,
    RevokeApprovalGrant {
        grant_id: GrantId,
    },
    ListExecutionAudit {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        run_id: Option<RunId>,
    },
    ListVerificationRunners,
    InspectVerificationRunners,
    ApproveProjectVerification {
        runner: VerificationRunnerId,
    },
    LoadVerificationEvidence {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        run_id: Option<RunId>,
    },
    LoadTrajectory {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        session_id: Option<SessionId>,
    },
    LoadChangeSet {
        run_id: RunId,
    },
    LoadUnifiedDiff {
        file_change_id: FileChangeId,
    },
    RequestRestore {
        file_change_id: FileChangeId,
    },
    LoadChangeRecovery,
    RetryChangeRecovery {
        operation_id: ChangeOperationId,
    },
    RequestRecoveryRestore {
        operation_id: ChangeOperationId,
    },
    SubmitTask {
        prompt: String,
    },
    SubmitTaskWithSkill {
        prompt: String,
        skill: String,
    },
    SubmitTaskWithAnswerSchema {
        prompt: String,
        answer_schema: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        expected_sha256: Option<Sha256Digest>,
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
    SummaryTransportObserved {
        session_id: SessionId,
        run_id: RunId,
        request_id: Uuid,
        observation: context_summary::SummaryTransportObservation,
    },
    ContextCompactionCompleted {
        session_id: SessionId,
        operation_id: RunId,
        result: Box<context_summary::CompactionResult>,
    },
    ContextCompactionProgress {
        session_id: SessionId,
        operation_id: RunId,
        phase: String,
    },
    GoalRequestAccounting {
        session_id: SessionId,
        run_id: RunId,
        charge: Box<goal_accounting::RequestCharge>,
        persistence: String,
    },
    ContextRequestAccounted {
        session_id: SessionId,
        run_id: RunId,
        request_id: Uuid,
        purpose: String,
        accounting: context_budget::RequestAccounting,
    },
    ModelRequestAttempt {
        session_id: SessionId,
        run_id: RunId,
        request_id: Uuid,
        purpose: String,
        attempt: u8,
        max_attempts: u8,
        outcome: String,
        error_class: Option<String>,
        status: Option<u16>,
        planned_delay_ms: Option<u64>,
        stop_reason: Option<String>,
    },
    SteeringChanged(SteeringStatus),
    PlatformAuthorityChanged(PlatformAuthoritySnapshot),
    WorkspaceStateLoaded(WorkspaceSnapshot),
    ConversationSnapshotLoaded(ConversationSnapshot),
    GoalSnapshotLoaded {
        session_id: SessionId,
        goal: Option<Box<Goal>>,
    },
    GoalInvocationSnapshot {
        session_id: SessionId,
        invocation: Option<goal_supervision::GoalInvocationStatus>,
    },
    ActiveRunsSnapshot {
        runs: Vec<ActiveRunState>,
    },
    ProjectCreated(Project),
    SessionCreated(Session),
    SessionForked(SessionForkSnapshot),
    SessionMetadataChanged {
        session_id: SessionId,
        title: String,
        pinned: bool,
        archived: bool,
    },
    SessionListLoaded(SessionListSnapshot),
    SessionRowUpdated(SessionListRow),
    TaskAccepted(Task),
    AnswerContractAccepted {
        run_id: RunId,
        source: final_result::AnswerContractSource,
        validator_version: u16,
    },
    AnswerContractEvaluated {
        run_id: RunId,
        evaluation: final_result::AnswerContractEvaluation,
    },
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
    ApprovalGrantIssued(ApprovalGrantRecord),
    ApprovalGrantRevoked {
        grant_id: GrantId,
        revoked_at: OffsetDateTime,
    },
    ApprovalGrantsSnapshot {
        grants: Vec<ApprovalGrantRecord>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        coverage: Option<ApprovalGrantHistoryCoverage>,
    },
    ExecutionAuditRecorded(ExecutionAuditRecord),
    ExecutionAuditSnapshot {
        records: Vec<ExecutionAuditRecord>,
    },
    ChangeSetReady(ChangeSet),
    FileChangeRecorded(FileChangeRecord),
    ChangeSetSnapshotLoaded(ChangeSetSnapshot),
    UnifiedDiffLoaded(UnifiedDiffSnapshot),
    RestoreCompleted(RestoreResult),
    ChangeRecoverySnapshotLoaded(ChangeRecoverySnapshot),
    ChangeRecoveryResolved {
        operation_id: ChangeOperationId,
        state: ChangeRecoveryState,
    },
    VerificationCompleted {
        run_id: RunId,
        passed: bool,
        summary: String,
    },
    VerificationEvidenceRecorded(VerificationEvidence),
    SupplementalCheckRecorded(supplemental::SupplementalCheckReceipt),
    VerificationEvidenceSnapshot {
        evidence: Vec<VerificationEvidence>,
    },
    VerificationRunnersSnapshot {
        runners: Vec<VerificationRunnerId>,
    },
    VerificationRunnerInspectionSnapshot {
        runners: Vec<VerificationRunnerInspection>,
    },
    ProviderStatus(ProviderStatus),
    ProviderCredentialStatus {
        provider_id: String,
        configured: bool,
    },
    ProviderProfilesSnapshot {
        profiles: Vec<ProviderProfile>,
    },
    ProviderModelsSnapshot {
        provider_id: String,
        models: Vec<ProviderModel>,
        selected_model: String,
        #[serde(default)]
        source: ProviderCatalogSource,
    },
    OnboardingSnapshotLoaded(OnboardingSnapshot),
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
            Self::ContextCompactionCompleted { .. } => "context_compaction_completed",
            Self::SummaryTransportObserved { .. } => "summary_transport_observed",
            Self::ContextCompactionProgress { .. } => "context_compaction_progress",
            Self::GoalRequestAccounting { .. } => "goal_request_accounting",
            Self::ContextRequestAccounted { .. } => "context_request_accounted",
            Self::ModelRequestAttempt { .. } => "model_request_attempt",
            Self::SteeringChanged(_) => "steering_changed",
            Self::PlatformAuthorityChanged(_) => "platform_authority_changed",
            Self::WorkspaceStateLoaded(_) => "workspace_state_loaded",
            Self::ConversationSnapshotLoaded(_) => "conversation_snapshot_loaded",
            Self::GoalSnapshotLoaded { .. } => "goal_snapshot_loaded",
            Self::GoalInvocationSnapshot { .. } => "goal_invocation_snapshot",
            Self::ActiveRunsSnapshot { .. } => "active_runs_snapshot",
            Self::ProjectCreated(_) => "project_created",
            Self::SessionCreated(_) => "session_created",
            Self::SessionForked(_) => "session_forked",
            Self::SessionMetadataChanged { .. } => "session_metadata_changed",
            Self::SessionListLoaded(_) => "session_list_loaded",
            Self::SessionRowUpdated(_) => "session_row_updated",
            Self::TaskAccepted(_) => "task_accepted",
            Self::AnswerContractAccepted { .. } => "answer_contract_accepted",
            Self::AnswerContractEvaluated { .. } => "answer_contract_evaluated",
            Self::PlanUpdated { .. } => "plan_updated",
            Self::RunStatusChanged { .. } => "run_status_changed",
            Self::AssistantDelta { .. } => "assistant_delta",
            Self::AssistantReasoningDelta { .. } => "assistant_reasoning_delta",
            Self::ModelUsageUpdated { .. } => "model_usage_updated",
            Self::ToolStarted(_) => "tool_started",
            Self::ToolFinished(_) => "tool_finished",
            Self::ApprovalRequested(_) => "approval_requested",
            Self::ApprovalResolved { .. } => "approval_resolved",
            Self::ApprovalGrantIssued(_) => "approval_grant_issued",
            Self::ApprovalGrantRevoked { .. } => "approval_grant_revoked",
            Self::ApprovalGrantsSnapshot { .. } => "approval_grants_snapshot",
            Self::ExecutionAuditRecorded(_) => "execution_audit_recorded",
            Self::ExecutionAuditSnapshot { .. } => "execution_audit_snapshot",
            Self::ChangeSetReady(_) => "change_set_ready",
            Self::FileChangeRecorded(_) => "file_change_recorded",
            Self::ChangeSetSnapshotLoaded(_) => "change_set_snapshot_loaded",
            Self::UnifiedDiffLoaded(_) => "unified_diff_loaded",
            Self::RestoreCompleted(_) => "restore_completed",
            Self::ChangeRecoverySnapshotLoaded(_) => "change_recovery_snapshot_loaded",
            Self::ChangeRecoveryResolved { .. } => "change_recovery_resolved",
            Self::VerificationCompleted { .. } => "verification_completed",
            Self::VerificationEvidenceRecorded(_) => "verification_evidence_recorded",
            Self::SupplementalCheckRecorded(_) => "supplemental_check_recorded",
            Self::VerificationEvidenceSnapshot { .. } => "verification_evidence_snapshot",
            Self::VerificationRunnersSnapshot { .. } => "verification_runners_snapshot",
            Self::VerificationRunnerInspectionSnapshot { .. } => {
                "verification_runner_inspection_snapshot"
            }
            Self::ProviderStatus(_) => "provider_status",
            Self::ProviderCredentialStatus { .. } => "provider_credential_status",
            Self::ProviderProfilesSnapshot { .. } => "provider_profiles_snapshot",
            Self::ProviderModelsSnapshot { .. } => "provider_models_snapshot",
            Self::OnboardingSnapshotLoaded(_) => "onboarding_snapshot_loaded",
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
    fn change_page_cursor_requires_typed_scope_and_rejects_overflow() {
        let cursor = ChangePageCursor {
            project_id: Uuid::new_v4(),
            session_id: Uuid::new_v4(),
            run_id: Uuid::new_v4(),
            stamp: 1000,
            sequence: 7,
            id: Uuid::new_v4(),
        };
        let text = serde_json::to_string(&cursor).unwrap();
        assert!(text.len() <= 512);
        assert_eq!(
            serde_json::from_str::<ChangePageCursor>(&text).unwrap(),
            cursor
        );
        let value = serde_json::to_value(&cursor).unwrap();
        for field in [
            "project_id",
            "session_id",
            "run_id",
            "stamp",
            "sequence",
            "id",
        ] {
            let mut missing = value.clone();
            missing.as_object_mut().unwrap().remove(field);
            assert!(serde_json::from_value::<ChangePageCursor>(missing).is_err());
        }
        let mut extra = value.clone();
        extra["workspace"] = serde_json::json!("foreign");
        assert!(serde_json::from_value::<ChangePageCursor>(extra).is_err());
        for field in ["stamp", "sequence"] {
            let mut overflow = value.clone();
            overflow[field] = serde_json::json!(u64::MAX);
            assert!(serde_json::from_value::<ChangePageCursor>(overflow).is_err());
        }
    }

    #[test]
    fn default_local_profile_uses_current_compact_qwen() {
        let profile = ProviderProfile::default_llama_cpp();

        assert_eq!(DEFAULT_LLAMA_CPP_MODEL, "Qwen3.5-0.8B-Q8_0.gguf");
        assert_eq!(profile.model, DEFAULT_LLAMA_CPP_MODEL);
        assert_eq!(profile.endpoint, "local://llama-cpp/");
    }

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
    fn provider_credential_status_event_serializes_without_a_secret_field() {
        let event = AppEvent::ProviderCredentialStatus {
            provider_id: "deepseek".into(),
            configured: true,
        };

        let json = serde_json::to_string(&event).expect("serialize status");

        assert!(json.contains("deepseek"));
        assert!(json.contains("\"configured\":true"));
        assert!(!json.contains("api_key"));
        assert!(!json.contains("credential_value"));
        assert!(!json.contains("deepseek-secret-fixture"));
    }

    #[test]
    fn legacy_change_set_ready_event_remains_decodable() {
        let run_id = Uuid::new_v4();
        let event: AppEvent = serde_json::from_value(serde_json::json!({
            "ChangeSetReady": {
                "run_id": run_id,
                "files_changed": 0,
                "additions": 0,
                "removals": 0,
                "verification_ready": false
            }
        }))
        .expect("legacy change summary event");

        assert!(matches!(
            event,
            AppEvent::ChangeSetReady(ChangeSet {
                run_id: stored_run_id,
                files_changed: 0,
                additions: 0,
                removals: 0,
                verification_ready: false,
            }) if stored_run_id == run_id
        ));
    }

    #[test]
    fn file_change_events_are_metadata_only() {
        let private_bytes = b"private-file-snapshot-should-never-serialize";
        let event = AppEvent::FileChangeRecorded(FileChangeRecord {
            id: Uuid::new_v4(),
            change_set_id: Uuid::new_v4(),
            project_id: Uuid::new_v4(),
            session_id: Uuid::new_v4(),
            run_id: Uuid::new_v4(),
            revision: 1,
            relative_path: "src/lib.rs".into(),
            kind: FileChangeKind::Modified,
            origin: FileChangeOrigin::ToolCall,
            before: Some(FileSnapshotRef {
                digest: Sha256Digest("11".repeat(32)),
                size_bytes: private_bytes.len() as u64,
                retained: true,
            }),
            after: Some(FileSnapshotRef {
                digest: Sha256Digest("22".repeat(32)),
                size_bytes: 3,
                retained: true,
            }),
            additions: 1,
            removals: 1,
            line_count_availability: LineCountAvailability::Unknown,
            restored_from: None,
            created_at: now(),
        });

        let encoded = serde_json::to_vec(&event).expect("file change event JSON");
        assert!(!encoded
            .windows(private_bytes.len())
            .any(|window| window == private_bytes));
        assert!(!String::from_utf8(encoded)
            .expect("event utf-8")
            .contains("blob_bytes"));
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
                source: ProviderCatalogSource::Live,
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
    fn inspector_surface_command_round_trips_additively() {
        for value in [
            serde_json::json!({
                "kind": "set_inspector_surface",
                "surface": "Trajectory"
            }),
            serde_json::json!({"kind": "set_inspector_surface"}),
        ] {
            let command = serde_json::from_value::<AppCommand>(value.clone())
                .expect("inspector surface command");
            let encoded = serde_json::to_value(command).expect("serialized command");
            assert_eq!(encoded, value);
        }
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
    fn legacy_layouts_migrate_the_first_visible_inspector_surface() {
        let layout: LayoutProfile = serde_json::from_value(serde_json::json!({
            "density": "Comfortable",
            "visible": {
                "Conversation": true,
                "Plan": true,
                "Changes": false,
                "Activity": true,
                "Trajectory": true
            },
            "widths": {}
        }))
        .expect("legacy layout");

        let migrated = serde_json::to_value(layout).expect("migrated layout");
        assert_eq!(migrated["inspector_collapsed"], false);
        assert_eq!(migrated["active_inspector"], "Activity");
    }

    #[test]
    fn persisted_layout_rejects_non_inspector_active_surface() {
        let layout = serde_json::from_value::<LayoutProfile>(serde_json::json!({
            "harness_profile_id": "custom",
            "density": "Comfortable",
            "visible": {
                "Conversation": true,
                "Plan": true,
                "Changes": true
            },
            "widths": {},
            "inspector_collapsed": false,
            "active_inspector": "Plan"
        }));

        assert!(layout.is_err());
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

    #[test]
    fn onboarding_normalization_rejects_incomplete_completed_snapshots() {
        let mut snapshot = OnboardingSnapshot::initial();
        snapshot.complete = true;
        snapshot.step = OnboardingStep::Complete;

        assert_eq!(snapshot.normalized(), Err(OnboardingError::Incomplete));

        let mut missing_credential = OnboardingSnapshot::legacy_complete(
            "openai",
            "gpt-test",
            PathBuf::from("A:\\workspace"),
            DEFAULT_EXECUTION_PROFILE_ID,
        );
        missing_credential.credential_state = OnboardingCredentialState::Missing;
        assert_eq!(
            missing_credential.normalized(),
            Err(OnboardingError::Incomplete)
        );
    }

    #[test]
    fn phase4_legacy_execution_policy_defaults_to_guided_security() {
        let policy: HarnessExecutionPolicy = serde_json::from_value(serde_json::json!({
            "profile_id": "confirm-before-changes",
            "capability_enabled": {
                "tool.read-text": true,
                "tool.write-text": true
            }
        }))
        .expect("legacy execution policy");

        assert_eq!(policy.sandbox_scope, SandboxScope::WorkspaceWrite);
        assert_eq!(policy.approval_mode, ApprovalMode::Always);
    }

    #[test]
    fn f12b_legacy_search_text_input_defaults_to_literal_without_context() {
        let input: ToolInput = serde_json::from_value(serde_json::json!({
            "kind": "search_text",
            "path": ".",
            "query": "a.b",
            "max_results": 8
        }))
        .expect("legacy search input");
        assert!(matches!(
            input,
            ToolInput::SearchText {
                mode: TextSearchMode::Literal,
                before_context: 0,
                after_context: 0,
                ..
            }
        ));
    }

    #[test]
    fn f12c_read_text_keeps_legacy_byte_inputs_and_adds_explicit_line_inputs() {
        let legacy: ToolInput = serde_json::from_value(serde_json::json!({
            "kind": "read_text",
            "path": "src/lib.rs",
            "start_byte": 0
        }))
        .expect("legacy byte read input");
        assert!(matches!(
            legacy,
            ToolInput::ReadText {
                start_byte: Some(0),
                end_byte: None,
                start_line: None,
                line_count: None,
                expected_sha256: None,
                ..
            }
        ));

        let historical_default: ToolInput = serde_json::from_value(serde_json::json!({
            "kind": "read_text",
            "path": "src/lib.rs"
        }))
        .expect("historical default read input");
        assert!(matches!(
            historical_default,
            ToolInput::ReadText {
                start_byte: None,
                end_byte: None,
                start_line: None,
                line_count: None,
                expected_sha256: None,
                ..
            }
        ));

        let line_input: ToolInput = serde_json::from_value(serde_json::json!({
            "kind": "read_text",
            "path": "src/lib.rs",
            "start_line": 4505,
            "line_count": 5,
            "expected_sha256": "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"
        }))
        .expect("line read input");
        assert!(matches!(
            line_input,
            ToolInput::ReadText {
                start_byte: None,
                end_byte: None,
                start_line: Some(4505),
                line_count: Some(5),
                expected_sha256: Some(_),
                ..
            }
        ));
    }

    #[test]
    fn phase4_policy_and_grant_contract_round_trip() {
        let created_at = now();
        let record = ApprovalGrantRecord {
            id: Uuid::new_v4(),
            audit_id: Uuid::new_v4(),
            capability: Capability::ExecuteCommands,
            scope: GrantScope::Run,
            subject: GrantSubject::Run {
                run_id: Uuid::new_v4(),
            },
            created_at,
            expires_at: Some(created_at + time::Duration::minutes(5)),
            issuer: GrantIssuer::User,
            reason: "Run the selected verification command".into(),
            revoked_at: None,
        };

        let encoded = serde_json::to_value(&record).expect("serialized grant");
        assert_eq!(encoded["scope"], "run");
        assert_eq!(encoded["capability"], "ExecuteCommands");
        assert_eq!(
            serde_json::from_value::<ApprovalGrantRecord>(encoded).expect("parsed grant"),
            record
        );
        assert_eq!(ApprovalScope::Once.grant_scope(), GrantScope::ToolCall);
        assert_eq!(ApprovalScope::Action.grant_scope(), GrantScope::Action);
    }

    #[test]
    fn phase4_git_input_is_fixed_and_rejects_publication_operations() {
        let input = ToolInput::Git {
            operation: GitOperation::Status { short: true },
        };
        let encoded = serde_json::to_value(input).expect("serialized Git input");
        assert_eq!(encoded["kind"], "git");
        assert_eq!(encoded["operation"]["kind"], "status");

        for forbidden in ["push", "force_push", "delete_branch", "tag", "remote"] {
            let value = serde_json::json!({
                "kind": "git",
                "operation": { "kind": forbidden }
            });
            assert!(
                serde_json::from_value::<ToolInput>(value).is_err(),
                "{forbidden} must not be representable"
            );
        }
    }

    #[test]
    fn phase4_verification_evidence_has_a_stable_audit_safe_shape() {
        let evidence = VerificationEvidence {
            id: Uuid::new_v4(),
            run_id: Uuid::new_v4(),
            runner: VerificationRunnerId::CargoTestLocked,
            registry_version: 1,
            program: "cargo".into(),
            args: vec!["test".into(), "--workspace".into(), "--locked".into()],
            state: VerificationState::Passed,
            started_at: now(),
            duration_ms: 42,
            exit_code: Some(0),
            revision_before: Sha256Digest("before".into()),
            revision_after: Sha256Digest("before".into()),
            stdout_sha256: Sha256Digest("stdout".into()),
            stderr_sha256: Sha256Digest("stderr".into()),
            stdout_bytes: 12,
            stderr_bytes: 0,
            stdout_truncated: false,
            stderr_truncated: false,
            workspace_key: String::new(),
            definition_sha256: None,
            input_scope: None,
            input_before: None,
            input_after: None,
        };

        let event = AppEvent::VerificationEvidenceRecorded(evidence.clone());
        let encoded = serde_json::to_value(&event).expect("serialized evidence event");
        assert!(!encoded.to_string().contains("stdout_text"));
        assert_eq!(event.kind(), "verification_evidence_recorded");
        assert_eq!(
            serde_json::from_value::<AppEvent>(encoded).expect("parsed evidence event"),
            AppEvent::VerificationEvidenceRecorded(evidence)
        );
    }
}
