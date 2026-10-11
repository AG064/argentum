use std::collections::BTreeMap;
use std::io::{self, Write};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use argentum_domain::{
    goal_supervision::{GoalCriterion, VerificationInputSnapshot},
    now,
    process_job_contract::{PollSelection, POLL_WAIT_MS},
    ApprovalGrantRecord, Capability, ExecutionOutcome, GrantScope, GrantSubject,
    HarnessExecutionPolicy, PatchFile, ProjectId, SandboxScope, SessionId, Sha256Digest,
    ToolDescriptor, ToolRisk, VerificationEvidence, VerificationRunnerId,
};
pub use argentum_domain::{ToolInput, ToolRequest};
pub use argentum_execution::{
    default_revision_limits, rust_check_configuration_digest, ProcessJobOwner,
};
use argentum_execution::{
    project_verification_binding, verification_input_scope, GitError, GitExecutor, ProcessError,
    ProcessJobError, ProcessJobManager, ProcessRunner, ProcessSpec, ProcessState,
    VerificationError, VerificationRegistry, VerificationRunner,
};
use argentum_security::{ApprovalGrant, ScopedPath, SecurityError};
use argentum_workspaces::{
    PreparedWorkspacePatch, WorkspaceError, WorkspaceManager, WorkspaceTextSearchOptions,
    MAX_SEARCH_CONTEXT_LINES,
};
use async_trait::async_trait;
use serde::ser::SerializeMap;
use serde::{Serialize, Serializer};
use thiserror::Error;
use tokio_util::sync::CancellationToken;

// A path-only read is an orientation preview. Explicit ranges and line
// selections retain the existing larger bound; no full provider state is lost.
const DEFAULT_MODEL_READ_RESULT_BYTES: usize = 8 * 1024;

#[cfg(test)]
mod verification_diagnostics_tests;

#[derive(Debug, Error)]
pub enum ToolError {
    #[error("tool is not registered: {0}")]
    NotRegistered(String),
    #[error("tool input is invalid: {0}")]
    InvalidInput(String),
    #[error(transparent)]
    Workspace(#[from] WorkspaceError),
    #[error(transparent)]
    Security(#[from] SecurityError),
    #[error(transparent)]
    Process(#[from] ProcessError),
    #[error(transparent)]
    Git(#[from] GitError),
    #[error(transparent)]
    Verification(#[from] VerificationError),
    #[error(transparent)]
    ProcessJob(#[from] ProcessJobError),
    #[error("tool mutation was not prepared by the runtime")]
    MissingPreparedMutation,
    #[error("tool execution was cancelled")]
    Cancelled,
    #[error("tool execution failed: {0}")]
    Execution(String),
    #[error("extension failed: {code}; effects_unknown={effects_unknown}")]
    ExtensionFailure {
        code: &'static str,
        effects_unknown: bool,
    },
    #[error("external tool reported failure; its detail is untrusted")]
    ExtensionReportedFailure(String),
}

impl ToolError {
    pub const fn execution_outcome(&self) -> ExecutionOutcome {
        match self {
            Self::Cancelled => ExecutionOutcome::Cancelled,
            Self::Process(ProcessError::Spawn(_) | ProcessError::Containment(_))
            | Self::Git(GitError::Process(ProcessError::Spawn(_) | ProcessError::Containment(_))) => {
                ExecutionOutcome::SpawnFailed
            }
            Self::NotRegistered(_)
            | Self::InvalidInput(_)
            | Self::Workspace(_)
            | Self::Security(_)
            | Self::Process(_)
            | Self::Git(_)
            | Self::Verification(_)
            | Self::ProcessJob(_)
            | Self::MissingPreparedMutation
            | Self::Execution(_)
            | Self::ExtensionFailure { .. }
            | Self::ExtensionReportedFailure(_) => ExecutionOutcome::Failed,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolExecutionEvidence {
    pub outcome: ExecutionOutcome,
    pub exit_code: Option<i32>,
    pub stdout_sha256: Option<Sha256Digest>,
    pub stderr_sha256: Option<Sha256Digest>,
    pub stdout_bytes: u64,
    pub stderr_bytes: u64,
    pub stdout_truncated: bool,
    pub stderr_truncated: bool,
    pub changed_paths: Vec<String>,
    pub verification: Vec<VerificationEvidence>,
    pub process_job: Option<argentum_execution::ProcessJobSnapshot>,
    pub supplemental_check: Option<argentum_domain::supplemental::SupplementalCheckReceipt>,
}

impl Default for ToolExecutionEvidence {
    fn default() -> Self {
        Self {
            outcome: ExecutionOutcome::Succeeded,
            exit_code: None,
            stdout_sha256: None,
            stderr_sha256: None,
            stdout_bytes: 0,
            stderr_bytes: 0,
            stdout_truncated: false,
            stderr_truncated: false,
            changed_paths: Vec::new(),
            verification: Vec::new(),
            process_job: None,
            supplemental_check: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolResult {
    pub summary: String,
    pub output: String,
    pub evidence: ToolExecutionEvidence,
}

#[derive(Clone)]
pub struct ToolContext {
    pub extension_executor: Option<Arc<dyn ExtensionExecutor>>,
    pub workspace: WorkspaceManager,
    pub approval: ApprovalGrant,
    pub policy: HarnessExecutionPolicy,
    pub grants: Vec<ApprovalGrantRecord>,
    pub cancellation: CancellationToken,
    pub prepared_patch: Option<PreparedWorkspacePatch>,
    pub process_jobs: Option<Arc<ProcessJobManager>>,
    pub process_job_owner: Option<(ProjectId, SessionId, String)>,
    pub remaining_execution_time: Option<Duration>,
}

impl ToolContext {
    pub fn new(
        workspace: WorkspaceManager,
        approval: ApprovalGrant,
        policy: HarnessExecutionPolicy,
        cancellation: CancellationToken,
    ) -> Self {
        Self {
            extension_executor: None,
            workspace,
            approval,
            policy,
            grants: Vec::new(),
            cancellation,
            prepared_patch: None,
            process_jobs: None,
            process_job_owner: None,
            remaining_execution_time: None,
        }
    }

    pub fn with_grants(mut self, grants: Vec<ApprovalGrantRecord>) -> Self {
        self.grants = grants;
        self
    }

    pub fn with_prepared_patch(mut self, prepared_patch: PreparedWorkspacePatch) -> Self {
        self.prepared_patch = Some(prepared_patch);
        self
    }

    pub fn with_process_jobs(
        mut self,
        process_jobs: Arc<ProcessJobManager>,
        project_id: ProjectId,
        session_id: SessionId,
        workspace_key: String,
    ) -> Self {
        self.process_jobs = Some(process_jobs);
        self.process_job_owner = Some((project_id, session_id, workspace_key));
        self
    }
}

#[async_trait]
pub trait AgentTool: Send + Sync {
    fn descriptor(&self) -> ToolDescriptor;

    async fn execute(
        &self,
        request: ToolRequest,
        context: ToolContext,
    ) -> Result<ToolResult, ToolError>;
}

#[async_trait]
pub trait ExtensionExecutor: Send + Sync {
    async fn execute(
        &self,
        operation: argentum_domain::extensions::ExtensionOperation,
        request: &ToolRequest,
        context: &ToolContext,
    ) -> Result<ToolResult, ToolError>;
}

#[derive(Clone)]
pub struct ToolRegistry {
    tools: Arc<BTreeMap<String, Arc<dyn AgentTool>>>,
    process_jobs: Arc<ProcessJobManager>,
}

impl Default for ToolRegistry {
    fn default() -> Self {
        Self {
            tools: Arc::new(BTreeMap::new()),
            process_jobs: Arc::new(ProcessJobManager::new()),
        }
    }
}

impl std::fmt::Debug for ToolRegistry {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ToolRegistry")
            .field("tools", &self.tools.keys().collect::<Vec<_>>())
            .finish()
    }
}

impl ToolRegistry {
    pub fn with_builtins(workspace: WorkspaceManager) -> Self {
        let mut registry = Self::default();
        for &tool in BuiltinTool::ALL {
            registry.register(tool);
        }
        let _ = workspace;
        registry
    }

    pub fn process_jobs(&self) -> Arc<ProcessJobManager> {
        Arc::clone(&self.process_jobs)
    }

    pub fn register<T>(&mut self, tool: T)
    where
        T: AgentTool + 'static,
    {
        let descriptor = tool.descriptor();
        let mut tools = (*self.tools).clone();
        tools.insert(descriptor.id, Arc::new(tool));
        self.tools = Arc::new(tools);
    }

    pub fn descriptors(&self) -> Vec<ToolDescriptor> {
        self.tools.values().map(|tool| tool.descriptor()).collect()
    }

    pub async fn execute(
        &self,
        request: ToolRequest,
        context: ToolContext,
    ) -> Result<ToolResult, ToolError> {
        let tool_id = tool_id_for_input(&request.input);
        let tool = self
            .tools
            .get(tool_id)
            .ok_or_else(|| ToolError::NotRegistered(tool_id.to_owned()))?;
        tool.execute(request, context).await
    }

    pub fn descriptor_for_input(&self, input: &ToolInput) -> Result<ToolDescriptor, ToolError> {
        let tool_id = tool_id_for_input(input);
        self.tools
            .get(tool_id)
            .map(|tool| {
                let mut descriptor = tool.descriptor();
                if matches!(
                    input,
                    ToolInput::Extension {
                        operation: argentum_domain::extensions::ExtensionOperation::List
                            | argentum_domain::extensions::ExtensionOperation::LoadSkill { .. }
                    }
                ) {
                    descriptor.capabilities = vec![Capability::ReadFiles];
                    descriptor.risk = ToolRisk::ReadOnly;
                    descriptor.requires_approval = false;
                }
                descriptor
            })
            .ok_or_else(|| ToolError::NotRegistered(tool_id.to_owned()))
    }

    pub fn prepare_mutations(
        &self,
        request: &ToolRequest,
        workspace: &WorkspaceManager,
        scope: SandboxScope,
    ) -> Result<Option<PreparedWorkspacePatch>, ToolError> {
        let patch = match &request.input {
            ToolInput::EditText {
                path,
                expected_sha256,
                old_text,
                new_text,
                newline_mode,
                start_line,
                line_count,
                replacement_lines,
            } => {
                let prepared = match (old_text.as_deref(), new_text.as_deref(), start_line, line_count, replacement_lines.as_deref()) {
                    (Some(old), Some(new), None, None, None) => workspace.prepare_text_edit_with_mode_and_scope(
                        path, expected_sha256, old, new, *newline_mode, scope,
                    )?,
                    (None, None, Some(start), Some(count), Some(lines)) if newline_mode.is_exact() =>
                        workspace.prepare_text_line_edit_with_scope(
                            path, expected_sha256, *start, *count, lines, scope,
                        )?,
                    _ => return Err(ToolError::InvalidInput(
                        "edit_text requires either old_text/new_text or start_line/line_count/replacement_lines, never both".into(),
                    )),
                };
                Some(prepared)
            }
            ToolInput::ApplyPatch { files } => {
                Some(workspace.prepare_patch_with_scope(files, scope)?)
            }
            ToolInput::WriteText { path, content } => Some(workspace.prepare_patch_with_scope(
                &[PatchFile {
                    path: path.clone(),
                    expected_sha256: None,
                    content: Some(content.clone()),
                }],
                scope,
            )?),
            ToolInput::MoveFile { from, to } => {
                Some(workspace.prepare_move_file_with_scope(from, to, scope)?)
            }
            ToolInput::CopyFile { from, to } => {
                Some(workspace.prepare_copy_file_with_scope(from, to, scope)?)
            }
            ToolInput::DeleteFile { path } => {
                Some(workspace.prepare_delete_file_with_scope(path, scope)?)
            }
            ToolInput::ListDirectory { .. }
            | ToolInput::FileMetadata { .. }
            | ToolInput::ReadText { .. }
            | ToolInput::ReadBinaryMetadata { .. }
            | ToolInput::SearchText { .. }
            | ToolInput::SearchFilenames { .. }
            | ToolInput::CreateDirectory { .. }
            | ToolInput::ExecuteArgv { .. }
            | ToolInput::ExecuteShell { .. }
            | ToolInput::Git { .. }
            | ToolInput::Verify { .. }
            | ToolInput::StartVerification { .. }
            | ToolInput::CheckRust { .. }
            | ToolInput::PollProcessJob { .. }
            | ToolInput::CancelProcessJob { .. }
            | ToolInput::Extension { .. } => None,
        };
        Ok(patch)
    }

    pub fn verification_runners(&self) -> Vec<VerificationRunnerId> {
        VerificationRegistry::built_in()
            .runners()
            .into_iter()
            .map(|runner| runner.id)
            .collect()
    }

    pub fn verification_inspection(
        &self,
        workspace: &WorkspaceManager,
    ) -> Result<Vec<argentum_domain::VerificationRunnerInspection>, ToolError> {
        Ok(VerificationRegistry::for_workspace(workspace.root())?.inspect())
    }

    pub fn verification_binding(
        &self,
        workspace: &WorkspaceManager,
        runners: &[VerificationRunnerId],
    ) -> Result<Sha256Digest, ToolError> {
        Ok(project_verification_binding(workspace, runners)?)
    }

    pub fn goal_criteria(
        &self,
        runners: &[VerificationRunnerId],
    ) -> Result<Vec<GoalCriterion>, ToolError> {
        let registry = VerificationRegistry::built_in();
        self.goal_criteria_from_registry(&registry, runners)
    }

    pub fn goal_criteria_for_workspace(
        &self,
        workspace: &WorkspaceManager,
        runners: &[VerificationRunnerId],
    ) -> Result<Vec<GoalCriterion>, ToolError> {
        let registry = VerificationRegistry::for_workspace(workspace.root())?;
        self.goal_criteria_from_registry(&registry, runners)
    }

    fn goal_criteria_from_registry(
        &self,
        registry: &VerificationRegistry,
        runners: &[VerificationRunnerId],
    ) -> Result<Vec<GoalCriterion>, ToolError> {
        registry.validate_batch(runners)?;
        runners
            .iter()
            .map(|runner| {
                Ok(GoalCriterion {
                    id: argentum_domain::GoalId::new_v4(),
                    runner: runner.clone(),
                    registry_version: registry.version(),
                    definition_sha256: registry.definition_sha256(runner)?,
                    input_scope: verification_input_scope(),
                })
            })
            .collect()
    }

    pub fn validate_goal_criteria(&self, criteria: &[GoalCriterion]) -> Result<(), ToolError> {
        let registry = VerificationRegistry::built_in();
        self.validate_goal_criteria_from_registry(&registry, criteria)
    }

    pub fn validate_goal_criteria_for_workspace(
        &self,
        workspace: &WorkspaceManager,
        criteria: &[GoalCriterion],
    ) -> Result<(), ToolError> {
        let registry = VerificationRegistry::for_workspace(workspace.root())?;
        self.validate_goal_criteria_from_registry(&registry, criteria)
    }

    fn validate_goal_criteria_from_registry(
        &self,
        registry: &VerificationRegistry,
        criteria: &[GoalCriterion],
    ) -> Result<(), ToolError> {
        let runners = criteria
            .iter()
            .map(|criterion| criterion.runner.clone())
            .collect::<Vec<_>>();
        registry.validate_batch(&runners)?;
        for criterion in criteria {
            if criterion.registry_version != registry.version()
                || criterion.definition_sha256 != registry.definition_sha256(&criterion.runner)?
                || criterion.input_scope != verification_input_scope()
            {
                return Err(ToolError::InvalidInput(
                    "verification criterion does not match the current registry".into(),
                ));
            }
        }
        Ok(())
    }

    pub fn verification_input_snapshot(
        &self,
        workspace: &WorkspaceManager,
    ) -> Result<VerificationInputSnapshot, ToolError> {
        let revision = workspace.revision_digest(default_revision_limits())?;
        Ok(VerificationInputSnapshot {
            method: "workspace_revision_v1".into(),
            digest: revision.digest,
            file_count: revision.file_count,
            total_bytes: revision.total_bytes,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BuiltinTool {
    ListDirectory,
    FileMetadata,
    ReadText,
    ReadBinaryMetadata,
    SearchText,
    SearchFilenames,
    CreateDirectory,
    ApplyPatch,
    WriteText,
    EditText,
    MoveFile,
    CopyFile,
    DeleteFile,
    ExecuteArgv,
    ExecuteShell,
    GitRead,
    GitWrite,
    Verify,
    StartVerification,
    CheckRust,
    PollProcessJob,
    CancelProcessJob,
    #[cfg(feature = "extensions")]
    Extension,
}

impl BuiltinTool {
    const ALL: &'static [Self] = &[
        Self::ListDirectory,
        Self::FileMetadata,
        Self::ReadText,
        Self::ReadBinaryMetadata,
        Self::SearchText,
        Self::SearchFilenames,
        Self::CreateDirectory,
        Self::ApplyPatch,
        Self::WriteText,
        Self::EditText,
        Self::MoveFile,
        Self::CopyFile,
        Self::DeleteFile,
        Self::ExecuteArgv,
        Self::ExecuteShell,
        Self::GitRead,
        Self::GitWrite,
        Self::Verify,
        Self::StartVerification,
        Self::CheckRust,
        Self::PollProcessJob,
        Self::CancelProcessJob,
        #[cfg(feature = "extensions")]
        Self::Extension,
    ];

    fn descriptor(self) -> ToolDescriptor {
        let (id, capability_id, title, summary, capabilities, risk) = match self {
            #[cfg(feature = "extensions")]
            Self::Extension => ("extension", "extension.external", "Reviewed extensions", "Discover selected pinned skills/MCP tools, then use current handles. Installation and descriptions do not grant authority.", vec![Capability::UseExtensions], ToolRisk::Mutation),
            Self::ListDirectory => (
                "list_directory",
                "tool.list-directory",
                "List directory",
                "List bounded immediate entries inside the active path",
                vec![Capability::ReadFiles],
                ToolRisk::ReadOnly,
            ),
            Self::FileMetadata => (
                "file_metadata",
                "tool.file-metadata",
                "File metadata",
                "Read bounded metadata without returning file content",
                vec![Capability::ReadFiles],
                ToolRisk::ReadOnly,
            ),
            Self::ReadText => (
                "read_text",
                "tool.read-text",
                "Read text",
                "Read raw UTF-8 by byte offsets or paired one-based lines. Unspecified reads return a bounded orientation preview. Copy search read_selection for a same-snapshot read. To refresh a stale observation omit expected_sha256; nonzero byte reads first need a new digest from a byte-zero/line read. Returned edit_selection describes the observed window, not an inferred replacement target. Select intended edit lines separately. Explicit ranges keep the existing bound and truthful EOF metadata.",
                vec![Capability::ReadFiles],
                ToolRisk::ReadOnly,
            ),
            Self::EditText => (
                "edit_text",
                "tool.edit-text",
                "Edit guarded text",
                "Make a localized SHA-bound edit. Prefer old_text/new_text for one unique match; explicit newline_mode=preserve maps physical LF/CRLF. Never mix forms. For line replacement first read exactly the lines to replace, then copy that edit_selection start_line and line_count. line_count counts OLD source lines, not replacement_lines. A broad read is observation, not permission to overwrite surrounding code or comments. JSON strings are decoded once: encode source backslashes using normal JSON string escaping, keeping source escapes distinct from physical control characters. Logical replacement line bodies have no physical CR/LF. 16 KiB edit limit, 1 MiB file limit; authorization and stale checks are unchanged.",
                vec![Capability::WriteFiles],
                ToolRisk::Mutation,
            ),
            Self::ReadBinaryMetadata => (
                "read_binary_metadata",
                "tool.read-binary-metadata",
                "Binary metadata",
                "Read bounded size and digest metadata without returning binary content",
                vec![Capability::ReadFiles],
                ToolRisk::ReadOnly,
            ),
            Self::SearchText => (
                "search_text",
                "tool.search-text",
                "Search text",
                "Search CONTENT in an existing workspace file or directory; . is the root. Send path, nonempty query and max_results (1..1000). mode defaults to literal; choose regex explicitly. Optional context counts are 0..8. Use search_filenames for names. Match line numbers are one-based and read_selection is for observation, not replacement.",
                vec![Capability::ReadFiles],
                ToolRisk::ReadOnly,
            ),
            Self::SearchFilenames => (
                "search_filenames",
                "tool.search-filenames",
                "Search filenames",
                "Search NAMES with path, nonempty pattern and max_results (1..1000). Put simple * or ? wildcards in pattern, not path; these are not regex. Use search_text for contents. Existing ignore, traversal, authority and truthful completeness bounds remain unchanged.",
                vec![Capability::ReadFiles],
                ToolRisk::ReadOnly,
            ),
            Self::CreateDirectory => (
                "create_directory",
                "tool.create-directory",
                "Create directory",
                "Create a directory inside the authorized path scope",
                vec![Capability::WriteFiles],
                ToolRisk::Mutation,
            ),
            Self::ApplyPatch => (
                "apply_patch",
                "tool.apply-patch",
                "Apply patch",
                "Apply a stale-safe all-or-rollback exact-image patch",
                vec![Capability::WriteFiles],
                ToolRisk::Mutation,
            ),
            Self::WriteText => (
                "write_text",
                "tool.write-text",
                "Write text",
                "Write a prepared bounded UTF-8 file",
                vec![Capability::WriteFiles],
                ToolRisk::Mutation,
            ),
            Self::MoveFile => (
                "move_file",
                "tool.move-file",
                "Move file",
                "Move one prepared bounded file",
                vec![Capability::WriteFiles],
                ToolRisk::Mutation,
            ),
            Self::CopyFile => (
                "copy_file",
                "tool.copy-file",
                "Copy file",
                "Copy one prepared bounded file",
                vec![Capability::WriteFiles],
                ToolRisk::Mutation,
            ),
            Self::DeleteFile => (
                "delete_file",
                "tool.delete-file",
                "Delete file",
                "Delete one prepared bounded file with restoration data",
                vec![Capability::WriteFiles],
                ToolRisk::Mutation,
            ),
            Self::ExecuteArgv => (
                "execute_argv",
                "execution.argv",
                "Execute arguments",
                "Execute a contained process without an implicit shell",
                vec![Capability::ExecuteCommands, Capability::ExternalProcess],
                ToolRisk::Process,
            ),
            Self::ExecuteShell => (
                "execute_shell",
                "execution.shell",
                "Execute shell",
                "Execute explicit shell text in a contained process",
                vec![Capability::ExecuteShell, Capability::ExternalProcess],
                ToolRisk::Shell,
            ),
            Self::GitRead => (
                "git_read",
                "git.read",
                "Read Git state",
                "Run one fixed read-only Git operation",
                vec![Capability::GitRead, Capability::ExternalProcess],
                ToolRisk::GitRead,
            ),
            Self::GitWrite => (
                "git_write",
                "git.write",
                "Change Git state",
                "Run one fixed local Git mutation without publication operations",
                vec![Capability::GitWrite, Capability::ExternalProcess],
                ToolRisk::GitMutation,
            ),
            Self::Verify => (
                "verify",
                "verification.runner",
                "Run verification",
                "Run fixed Rust checks, returning revision-bound receipts and bounded current stdout/stderr diagnostics. Failed checks need source correction, not a success claim. A prior Passed receipt becomes stale after relevant edits.",
                vec![
                    Capability::Verify,
                    Capability::ExecuteCommands,
                    Capability::ExternalProcess,
                ],
                ToolRisk::Verification,
            ),
            Self::StartVerification => (
                "start_verification",
                "verification.runner",
                "Start verification job",
                "Start one fixed registered Rust verifier owned by this run; this is not a detached server, and the job must be polled and settled before final task completion",
                vec![Capability::Verify, Capability::ExecuteCommands, Capability::ExternalProcess],
                ToolRisk::Verification,
            ),
            Self::PollProcessJob => (
                "poll_process_job",
                "verification.runner",
                "Poll process job",
                "Read non-destructive bounded stdout and stderr cursor spans from a verification job owned by this run; handles do not survive run settlement or host restart",
                vec![Capability::Verify],
                ToolRisk::Verification,
            ),
            Self::CheckRust => (
                "check_rust",
                "verification.supplemental-rust",
                "Supplemental Rust check",
                "Execute model-authored integration assertions against an admitted byte-copy of a supported Rust library, under Windows LPAC isolation; this is separate from the requested project suite and not independent proof of all requirements",
                vec![Capability::Verify,Capability::ExecuteCommands,Capability::ExternalProcess],
                ToolRisk::Verification,
            ),
            Self::CancelProcessJob => (
                "cancel_process_job",
                "verification.runner",
                "Cancel process job",
                "Cancel and drain a verification job owned by this run; cancellation does not restart it or undo completed effects",
                vec![Capability::Verify],
                ToolRisk::Verification,
            ),
        };
        ToolDescriptor {
            id: id.into(),
            capability_id: capability_id.into(),
            title: title.into(),
            summary: summary.into(),
            capabilities,
            risk,
            requires_approval: risk.requires_risk_approval(),
        }
    }
}

#[async_trait]
impl AgentTool for BuiltinTool {
    fn descriptor(&self) -> ToolDescriptor {
        (*self).descriptor()
    }

    async fn execute(
        &self,
        request: ToolRequest,
        context: ToolContext,
    ) -> Result<ToolResult, ToolError> {
        if self.descriptor().id != tool_id_for_input(&request.input) {
            return Err(ToolError::InvalidInput(
                "tool input does not match descriptor".into(),
            ));
        }
        execute_builtin(request, context).await
    }
}

fn tool_id_for_input(input: &ToolInput) -> &'static str {
    match input {
        ToolInput::ListDirectory { .. } => "list_directory",
        ToolInput::FileMetadata { .. } => "file_metadata",
        ToolInput::ReadText { .. } => "read_text",
        ToolInput::ReadBinaryMetadata { .. } => "read_binary_metadata",
        ToolInput::SearchText { .. } => "search_text",
        ToolInput::SearchFilenames { .. } => "search_filenames",
        ToolInput::CreateDirectory { .. } => "create_directory",
        ToolInput::ApplyPatch { .. } => "apply_patch",
        ToolInput::WriteText { .. } => "write_text",
        ToolInput::EditText { .. } => "edit_text",
        ToolInput::MoveFile { .. } => "move_file",
        ToolInput::CopyFile { .. } => "copy_file",
        ToolInput::DeleteFile { .. } => "delete_file",
        ToolInput::ExecuteArgv { .. } => "execute_argv",
        ToolInput::ExecuteShell { .. } => "execute_shell",
        ToolInput::Git { operation } if operation.risk() == ToolRisk::GitRead => "git_read",
        ToolInput::Git { .. } => "git_write",
        ToolInput::Verify { .. } => "verify",
        ToolInput::StartVerification { .. } => "start_verification",
        ToolInput::CheckRust { .. } => "check_rust",
        ToolInput::PollProcessJob { .. } => "poll_process_job",
        ToolInput::CancelProcessJob { .. } => "cancel_process_job",
        ToolInput::Extension { .. } => "extension",
    }
}

async fn execute_builtin(
    request: ToolRequest,
    mut context: ToolContext,
) -> Result<ToolResult, ToolError> {
    if let ToolInput::Extension { operation } = &request.input {
        let executor = context.extension_executor.as_ref().ok_or_else(|| {
            ToolError::NotRegistered("extension support is not configured in this build".into())
        })?;
        return executor
            .execute(operation.clone(), &request, &context)
            .await;
    }
    let edit_newline_mode = match &request.input {
        ToolInput::EditText {
            newline_mode,
            replacement_lines,
            ..
        } => Some((*newline_mode, replacement_lines.is_some())),
        _ => None,
    };
    match request.input {
        ToolInput::ListDirectory { path, max_entries } => {
            if max_entries == 0 || max_entries > 1024 {
                return Err(ToolError::InvalidInput(
                    "directory entry limit is invalid".into(),
                ));
            }
            let entries = context.workspace.list_directory_bounded_with_scope(
                path,
                usize::from(max_entries),
                context.policy.sandbox_scope,
            )?;
            let output = serde_json::to_string(
                &entries
                    .iter()
                    .map(|entry| {
                        serde_json::json!({
                            "path": entry.path,
                            "name": entry.name,
                            "kind": format!("{:?}", entry.kind).to_ascii_lowercase(),
                            "size_bytes": entry.size_bytes,
                        })
                    })
                    .collect::<Vec<_>>(),
            )
            .map_err(|error_value| ToolError::Execution(error_value.to_string()))?;
            Ok(success("Listed directory", output))
        }
        ToolInput::FileMetadata { path } => {
            let metadata = context
                .workspace
                .file_metadata_with_scope(path, context.policy.sandbox_scope)?;
            let output = serde_json::json!({
                "path": metadata.path,
                "kind": format!("{:?}", metadata.kind).to_ascii_lowercase(),
                "size_bytes": metadata.size_bytes,
                "readonly": metadata.readonly,
            })
            .to_string();
            Ok(success("Read file metadata", output))
        }
        ToolInput::ReadText {
            path,
            start_byte,
            end_byte,
            start_line,
            line_count,
            expected_sha256,
        } => {
            if context.cancellation.is_cancelled() {
                return Err(ToolError::Cancelled);
            }
            let line_selector_present = start_line.is_some() || line_count.is_some();
            if line_selector_present && (start_byte.is_some() || end_byte.is_some()) {
                return Err(ToolError::InvalidInput(
                    "read_text accepts either byte selectors or the start_line/line_count pair, not both"
                        .into(),
                ));
            }
            let read = match (start_line, line_count) {
                (Some(start_line), Some(line_count)) => {
                    context.workspace.read_text_lines_with_scope(
                        &path,
                        start_line,
                        line_count,
                        expected_sha256.as_ref(),
                        context.policy.sandbox_scope,
                    )
                }
                (None, None) => context
                    .workspace
                    .read_text_range_with_scope_and_output_limit(
                        &path,
                        start_byte.unwrap_or(0),
                        end_byte,
                        expected_sha256.as_ref(),
                        context.policy.sandbox_scope,
                        if end_byte.is_none() {
                            DEFAULT_MODEL_READ_RESULT_BYTES
                        } else {
                            argentum_domain::text_contract::MAX_TEXT_RESULT_BYTES
                        },
                    ),
                _ => {
                    return Err(ToolError::InvalidInput(
                        "read_text line addressing requires start_line and line_count together"
                            .into(),
                    ))
                }
            }
            .map_err(|error_value| match error_value {
                WorkspaceError::InvalidText(reason) => ToolError::InvalidInput(reason.to_owned()),
                error_value => ToolError::Workspace(error_value),
            })?;
            let output = serde_json::to_string(&read).map_err(|_| {
                ToolError::Execution("could not encode truthful text result".into())
            })?;
            if output.len() > argentum_domain::text_contract::MAX_TEXT_RESULT_BYTES {
                return Err(ToolError::Execution(
                    "truthful text result exceeded output budget; no content returned".into(),
                ));
            }
            Ok(success("Read file range (text protocol v1)", output))
        }
        ToolInput::ReadBinaryMetadata { path } => {
            let metadata = context.workspace.binary_metadata_bounded_with_scope(
                path,
                16 * 1024 * 1024,
                context.policy.sandbox_scope,
            )?;
            Ok(success(
                "Read binary metadata",
                serde_json::json!({
                    "path": metadata.path,
                    "size_bytes": metadata.size_bytes,
                    "sha256": metadata.sha256.0,
                    "binary": metadata.binary,
                })
                .to_string(),
            ))
        }
        ToolInput::SearchText {
            path,
            query,
            max_results,
            mode,
            before_context,
            after_context,
        } => {
            if query.is_empty()
                || query.len() > 4096
                || max_results == 0
                || max_results > 1000
                || before_context > MAX_SEARCH_CONTEXT_LINES
                || after_context > MAX_SEARCH_CONTEXT_LINES
            {
                return Err(ToolError::InvalidInput(
                    "text search bounds are invalid".into(),
                ));
            }
            let search_cancellation = context.cancellation.clone();
            let report = context
                .workspace
                .search_text_bounded_report_with_options_and_scope_and_cancellation(
                    path,
                    &query,
                    usize::from(max_results),
                    WorkspaceTextSearchOptions {
                        mode,
                        before_context,
                        after_context,
                    },
                    context.policy.sandbox_scope,
                    move || search_cancellation.is_cancelled(),
                )
                .map_err(|error_value| match error_value {
                    WorkspaceError::InvalidText(reason) => {
                        ToolError::InvalidInput(reason.to_owned())
                    }
                    error_value => ToolError::Workspace(error_value),
                })?;
            let matches = report
                .matches
                .iter()
                .map(|item| {
                    let before = item
                        .before_context
                        .iter()
                        .map(|line| {
                            serde_json::json!({
                                "kind": "context",
                                "line_number": line.line_number,
                                "line": line.line,
                                "line_truncated": line.truncated,
                            })
                        })
                        .collect::<Vec<_>>();
                    let after = item
                        .after_context
                        .iter()
                        .map(|line| {
                            serde_json::json!({
                                "kind": "context",
                                "line_number": line.line_number,
                                "line": line.line,
                                "line_truncated": line.truncated,
                            })
                        })
                        .collect::<Vec<_>>();
                    serde_json::json!({
                        "kind": "match",
                        "path": item.path,
                        "line_number": item.line_number,
                        "line": item.line,
                        "line_truncated": item.line_truncated,
                        "read_selection": {
                            "path": item.read_selection.path,
                            "start_line": item.read_selection.start_line,
                            "line_count": item.read_selection.line_count,
                            "expected_sha256": item.read_selection.expected_sha256,
                        },
                        "context": {
                            "before_requested": item.before_context_requested,
                            "after_requested": item.after_context_requested,
                            "before": before,
                            "after": after,
                            "complete": item.context_complete,
                        },
                    })
                })
                .collect::<Vec<_>>();
            let output_cancellation = context.cancellation.clone();
            let output = bounded_search_output(&report, matches, move || {
                output_cancellation.is_cancelled()
            })?;
            Ok(success("Searched text", output))
        }
        ToolInput::SearchFilenames {
            path,
            pattern,
            max_results,
        } => {
            if pattern.is_empty() || pattern.len() > 512 || max_results == 0 || max_results > 1000 {
                return Err(ToolError::InvalidInput(
                    "filename search bounds are invalid".into(),
                ));
            }
            let search_cancellation = context.cancellation.clone();
            let report = context
                .workspace
                .search_filenames_bounded_report_with_scope_and_cancellation(
                    path,
                    &pattern,
                    usize::from(max_results),
                    context.policy.sandbox_scope,
                    move || search_cancellation.is_cancelled(),
                )?;
            let matches = report
                .matches
                .iter()
                .map(|item| serde_json::json!({ "path": item.path }))
                .collect::<Vec<_>>();
            let output_cancellation = context.cancellation.clone();
            let output = bounded_search_output(&report, matches, move || {
                output_cancellation.is_cancelled()
            })?;
            Ok(success("Searched filenames", output))
        }
        ToolInput::CreateDirectory { path } => {
            context
                .workspace
                .create_directory_with_scope_and_grant_cancellable(
                    &path,
                    &context.approval,
                    context.policy.sandbox_scope,
                    || context.cancellation.is_cancelled(),
                )
                .map_err(|error_value| match error_value {
                    WorkspaceError::Cancelled => ToolError::Cancelled,
                    error_value => ToolError::Workspace(error_value),
                })?;
            let mut result = success("Created directory", path.clone());
            result.evidence.changed_paths.push(path);
            Ok(result)
        }
        ToolInput::ApplyPatch { .. }
        | ToolInput::EditText { .. }
        | ToolInput::WriteText { .. }
        | ToolInput::MoveFile { .. }
        | ToolInput::CopyFile { .. }
        | ToolInput::DeleteFile { .. } => {
            let patch = context
                .prepared_patch
                .take()
                .ok_or(ToolError::MissingPreparedMutation)?;
            let changed_paths = patch
                .mutations
                .iter()
                .map(|mutation| mutation.relative_path.clone())
                .collect::<Vec<_>>();
            context
                .workspace
                .apply_prepared_patch_with_scope_and_grant_cancellable(
                    &patch,
                    &context.approval,
                    context.policy.sandbox_scope,
                    || context.cancellation.is_cancelled(),
                )
                .map_err(|error_value| match error_value {
                    WorkspaceError::Cancelled => ToolError::Cancelled,
                    error_value => ToolError::Workspace(error_value),
                })?;
            let mut result = success(
                "Applied workspace mutation",
                if let Some((newline_mode, line_form)) = edit_newline_mode {
                    let mutation = &patch.mutations[0];
                    let mut output = serde_json::json!({
                        "path": mutation.relative_path,
                        "sha256": mutation.after_sha256,
                        "replaced_matches": 1,
                        "observation_state": "prior_read_selections_are_historical",
                    });
                    if !newline_mode.is_exact() {
                        output["newline_mode"] = serde_json::json!(newline_mode);
                    }
                    if line_form {
                        output["edit_form"] = serde_json::json!("lines");
                    }
                    output.to_string()
                } else {
                    format!("{} file changes applied", changed_paths.len())
                },
            );
            result.evidence.changed_paths = changed_paths;
            Ok(result)
        }
        ToolInput::ExecuteArgv {
            program,
            args,
            cwd,
            timeout_ms,
            environment,
        } => {
            let cwd = resolve_cwd(&context.workspace, context.policy.sandbox_scope, cwd)?;
            let spec = ProcessSpec::argv(program, args, cwd)?
                .with_timeout(Duration::from_millis(timeout_ms))?
                .with_environment(environment)?;
            let result = ProcessRunner.run(spec, context.cancellation).await?;
            Ok(process_tool_result("Executed argument vector", result))
        }
        ToolInput::ExecuteShell {
            command,
            cwd,
            timeout_ms,
            environment,
        } => {
            let cwd = resolve_cwd(&context.workspace, context.policy.sandbox_scope, cwd)?;
            let spec = ProcessSpec::shell(command, cwd)?
                .with_timeout(Duration::from_millis(timeout_ms))?
                .with_environment(environment)?;
            let result = ProcessRunner.run(spec, context.cancellation).await?;
            Ok(process_tool_result("Executed explicit shell", result))
        }
        ToolInput::Git { operation } => {
            let result = GitExecutor::default()
                .execute(&operation, context.workspace.root(), context.cancellation)
                .await?;
            Ok(process_tool_result("Executed Git operation", result))
        }
        ToolInput::Verify { runners } => {
            let registry = if runners.iter().any(|runner| runner.project_id().is_some()) {
                VerificationRegistry::for_workspace(context.workspace.root())?
            } else {
                VerificationRegistry::built_in()
            };
            let binding = project_verification_binding(&context.workspace, &runners)?;
            if runners.iter().any(|runner| runner.project_id().is_some())
                && !context.grants.iter().any(|grant| {
                    grant.capability == Capability::ExternalProcess
                        && grant.scope == GrantScope::Action
                        && grant.created_at <= now()
                        && grant.expires_at.is_none_or(|expires_at| expires_at > now())
                        && grant.revoked_at.is_none_or(|revoked_at| revoked_at > now())
                        && matches!(
                            &grant.subject,
                            GrantSubject::Action { tool_id, binding: grant_binding, .. }
                                if tool_id == "verify" && *grant_binding == binding
                        )
                })
            {
                return Err(ToolError::Verification(
                    VerificationError::ProjectApprovalRequired,
                ));
            }
            if registry.runners().iter().any(|definition| {
                runners.contains(&definition.id) && definition.unavailable_reason.is_some()
            }) {
                return Err(ToolError::InvalidInput(
                    "selected project verification runner is unavailable".into(),
                ));
            }
            let verifier = VerificationRunner::new(context.workspace, registry);
            let mut executions = verifier
                .run_batch_with_output(request.run_id, &runners, context.cancellation)
                .await?;
            let disclosure = context
                .process_jobs
                .clone()
                .unwrap_or_else(|| Arc::new(ProcessJobManager::new()));
            for execution in &mut executions {
                for (text, truncated) in [
                    (&mut execution.stdout, execution.evidence.stdout_truncated),
                    (&mut execution.stderr, execution.evidence.stderr_truncated),
                ] {
                    if let Some(text) = text {
                        *text = disclosure.disclose_captured_process_text(text, truncated);
                    }
                }
            }
            let output = verification_feedback_output(&executions).unwrap_or_else(|_| {
                // Formatting cannot discard completed verification evidence.
                // Original receipts still reach storage and freshness checks.
                serde_json::json!({
                    "verification_feedback_unavailable": "model_visible_output_limit",
                    "executed": true,
                    "verification": executions.iter().map(|execution| serde_json::json!({
                        "id": execution.evidence.id,
                        "runner": execution.evidence.runner,
                        "state": execution.evidence.state,
                        "exit_code": execution.evidence.exit_code,
                    })).collect::<Vec<_>>(),
                })
                .to_string()
            });
            let evidence = executions
                .into_iter()
                .map(|execution| execution.evidence)
                .collect::<Vec<_>>();
            let passed = !evidence.is_empty()
                && evidence
                    .iter()
                    .all(|item| item.state == argentum_domain::VerificationState::Passed);
            let outcome = if passed {
                ExecutionOutcome::Succeeded
            } else if evidence
                .iter()
                .any(|item| item.state == argentum_domain::VerificationState::Cancelled)
            {
                ExecutionOutcome::Cancelled
            } else if evidence
                .iter()
                .any(|item| item.state == argentum_domain::VerificationState::TimedOut)
            {
                ExecutionOutcome::TimedOut
            } else {
                ExecutionOutcome::Failed
            };
            Ok(ToolResult {
                summary: "Ran verification".into(),
                output,
                evidence: ToolExecutionEvidence {
                    outcome,
                    verification: evidence,
                    ..ToolExecutionEvidence::default()
                },
            })
        }
        ToolInput::StartVerification { runner } => {
            let jobs = context
                .process_jobs
                .ok_or_else(|| ToolError::InvalidInput("process jobs are unavailable".into()))?;
            let (project_id, session_id, workspace_key) =
                context.process_job_owner.ok_or_else(|| {
                    ToolError::InvalidInput("process job owner is unavailable".into())
                })?;
            let owner = ProcessJobOwner {
                host_generation: jobs.host_generation(),
                project_id,
                workspace_key,
                session_id,
                run_id: request.run_id,
            };
            let verifier =
                VerificationRunner::new(context.workspace, VerificationRegistry::built_in());
            let job_id =
                jobs.start_verification(owner, verifier, runner.clone(), context.cancellation)?;
            Ok(ToolResult {
                summary: "Started verification job".into(),
                output: serde_json::json!({
                    "job_id": job_id,
                    "state": "starting",
                    "spawned": true,
                    "runner": runner.id(),
                    "poll_selection": PollSelection {
                        job_id: job_id.clone(),
                        stdout_cursor: 0,
                        stderr_cursor: 0,
                        wait_ms: POLL_WAIT_MS,
                    },
                })
                .to_string(),
                evidence: ToolExecutionEvidence {
                    outcome: ExecutionOutcome::Running,
                    ..ToolExecutionEvidence::default()
                },
            })
        }
        ToolInput::CheckRust {
            package,
            profile,
            test_source,
            replaces_check_id,
        } => {
            let jobs = context
                .process_jobs
                .ok_or_else(|| ToolError::InvalidInput("owned check manager unavailable".into()))?;
            let (project_id, session_id, workspace_key) = context
                .process_job_owner
                .ok_or_else(|| ToolError::InvalidInput("check owner unavailable".into()))?;
            let owner = ProcessJobOwner {
                host_generation: jobs.host_generation(),
                project_id,
                session_id,
                run_id: request.run_id,
                workspace_key,
            };
            let timeout = context
                .remaining_execution_time
                .unwrap_or(Duration::from_secs(60))
                .min(Duration::from_secs(60));
            let result = jobs
                .run_rust_check(
                    owner,
                    context.workspace,
                    argentum_execution::RustCheckRequest {
                        package: package.clone(),
                        profile,
                        source: test_source.clone(),
                        replaces: replaces_check_id,
                    },
                    context.cancellation,
                    timeout,
                )
                .await?;
            let passed = result.receipt.state
                == argentum_domain::supplemental::SupplementalCheckState::ReportedPass;
            Ok(ToolResult{summary:format!("Supplemental Rust check: {:?}",result.receipt.state),
                output:serde_json::json!({"supplemental_check":result.receipt,"feedback":{"stdout":result.stdout,"stderr":result.stderr,"stdout_prefix_truncated":result.stdout_prefix_truncated,"stderr_prefix_truncated":result.stderr_prefix_truncated,"diagnostic_prefix_limit_per_stream":4096},"project_suite_ran":false,"independent_correctness_proof":false}).to_string(),
                evidence:ToolExecutionEvidence{outcome:if passed{ExecutionOutcome::Succeeded}else{ExecutionOutcome::Failed},supplemental_check:Some(result.receipt),..ToolExecutionEvidence::default()}})
        }
        ToolInput::PollProcessJob {
            job_id,
            stdout_cursor,
            stderr_cursor,
            wait_ms,
        } => {
            let jobs = context
                .process_jobs
                .ok_or_else(|| ToolError::InvalidInput("process jobs are unavailable".into()))?;
            let (project_id, session_id, workspace_key) =
                context.process_job_owner.ok_or_else(|| {
                    ToolError::InvalidInput("process job owner is unavailable".into())
                })?;
            let owner = ProcessJobOwner {
                host_generation: jobs.host_generation(),
                project_id,
                workspace_key,
                session_id,
                run_id: request.run_id,
            };
            let snapshot = jobs
                .poll(
                    &owner,
                    &job_id,
                    stdout_cursor,
                    stderr_cursor,
                    Duration::from_millis(wait_ms),
                )
                .await?;
            let outcome = match snapshot.state {
                argentum_execution::ProcessJobState::Exited
                    if snapshot.verification.as_ref().is_some_and(|evidence| {
                        evidence.state == argentum_domain::VerificationState::Passed
                    }) =>
                {
                    ExecutionOutcome::Succeeded
                }
                argentum_execution::ProcessJobState::Cancelled => ExecutionOutcome::Cancelled,
                argentum_execution::ProcessJobState::TimedOut => ExecutionOutcome::TimedOut,
                state if state.is_terminal() => ExecutionOutcome::Failed,
                _ => ExecutionOutcome::Running,
            };
            let output = process_job_output(&snapshot)?;
            Ok(ToolResult {
                summary: "Polled verification job".into(),
                output,
                evidence: ToolExecutionEvidence {
                    outcome,
                    exit_code: snapshot.exit_code,
                    verification: snapshot.verification.clone().into_iter().collect(),
                    process_job: Some(snapshot),
                    ..ToolExecutionEvidence::default()
                },
            })
        }
        ToolInput::Extension { .. } => Err(ToolError::NotRegistered(
            "extension executor is unavailable".into(),
        )),
        ToolInput::CancelProcessJob { job_id } => {
            let jobs = context
                .process_jobs
                .ok_or_else(|| ToolError::InvalidInput("process jobs are unavailable".into()))?;
            let (project_id, session_id, workspace_key) =
                context.process_job_owner.ok_or_else(|| {
                    ToolError::InvalidInput("process job owner is unavailable".into())
                })?;
            let owner = ProcessJobOwner {
                host_generation: jobs.host_generation(),
                project_id,
                workspace_key,
                session_id,
                run_id: request.run_id,
            };
            let snapshot = jobs.cancel(&owner, &job_id)?;
            Ok(ToolResult {
                summary: "Cancelled verification job".into(),
                output: process_job_output(&snapshot)?,
                evidence: ToolExecutionEvidence {
                    outcome: ExecutionOutcome::Cancelled,
                    process_job: Some(snapshot),
                    ..ToolExecutionEvidence::default()
                },
            })
        }
    }
}

fn process_job_output(
    snapshot: &argentum_execution::ProcessJobSnapshot,
) -> Result<String, ToolError> {
    let limit = argentum_domain::text_contract::MAX_TEXT_RESULT_BYTES;
    let failed = snapshot
        .verification
        .as_ref()
        .is_some_and(|receipt| receipt.state == argentum_domain::VerificationState::Failed);
    let mut projected = snapshot.clone();
    projected.stdout.text.clear();
    projected.stderr.text.clear();
    let metadata = serde_json::to_vec(&projected)
        .map_err(|_| ToolError::Execution("could not encode job metadata".into()))?
        .len();
    let per_stream = limit
        .saturating_sub(metadata + 2048)
        .checked_div(2)
        .unwrap_or(0)
        .min(8 * 1024);
    let mut diagnostics = serde_json::Map::new();
    let mut unread = false;
    for (name, original, projected, raw_truncated) in [
        (
            "stdout",
            &snapshot.stdout,
            &mut projected.stdout,
            snapshot.verification.as_ref().map(|r| r.stdout_truncated),
        ),
        (
            "stderr",
            &snapshot.stderr,
            &mut projected.stderr,
            snapshot.verification.as_ref().map(|r| r.stderr_truncated),
        ),
    ] {
        let excerpt = if failed {
            argentum_execution::diagnostic_excerpt(
                &original.text,
                true,
                per_stream * 3 / 4,
                raw_truncated.map(|truncated| !truncated && original.complete),
                original.gap,
            )
        } else {
            None
        };
        let excerpt_bytes = excerpt
            .as_ref()
            .and_then(|v| serde_json::to_vec(v).ok())
            .map_or(0, |v| v.len() + 128);
        let prefix_end = argentum_execution::diagnostic_prefix_end(
            &original.text,
            per_stream.saturating_sub(excerpt_bytes),
        );
        projected.text = original.text[..prefix_end].to_owned();
        projected.to_cursor = original
            .from_cursor
            .checked_add(prefix_end as u64)
            .ok_or_else(|| ToolError::Execution("job cursor overflow".into()))?;
        projected.next_cursor = projected.to_cursor;
        projected.complete = original.complete && prefix_end == original.text.len();
        unread |= prefix_end < original.text.len();
        if let Some(excerpt) = excerpt {
            diagnostics.insert(
                name.into(),
                serde_json::json!({"source_cursor_base":original.from_cursor,"excerpt":excerpt}),
            );
        }
    }
    let mut value = serde_json::to_value(&projected)
        .map_err(|error_value| ToolError::Execution(error_value.to_string()))?;
    if let Some(selection) = projected.poll_selection().or_else(|| {
        unread.then(|| PollSelection {
            job_id: projected.job_id.clone(),
            stdout_cursor: projected.stdout.next_cursor,
            stderr_cursor: projected.stderr.next_cursor,
            wait_ms: 0,
        })
    }) {
        value["poll_selection"] = serde_json::to_value(selection)
            .map_err(|error_value| ToolError::Execution(error_value.to_string()))?;
    }
    if !diagnostics.is_empty() {
        value["diagnostics"] = serde_json::Value::Object(diagnostics);
    }
    let output = serde_json::to_string(&value)
        .map_err(|error_value| ToolError::Execution(error_value.to_string()))?;
    if output.len() > limit {
        return Err(ToolError::Execution(
            "job feedback exceeded the model-visible byte limit".into(),
        ));
    }
    Ok(output)
}

/// Preserve the existing array of typed receipts, adding only bounded current
/// process feedback. Deserializing the receipts drops this transient field.
fn verification_feedback_output(
    executions: &[argentum_execution::VerificationRunOutput],
) -> Result<String, ToolError> {
    let limit = argentum_domain::text_contract::MAX_TEXT_RESULT_BYTES;
    let encode_error = |_| ToolError::Execution("could not encode verification feedback".into());
    let mut values = executions
        .iter()
        .map(|execution| {
            let mut value = serde_json::to_value(&execution.evidence).map_err(encode_error)?;
            value["feedback"] = serde_json::json!({
                "output_available": execution.stdout.is_some() && execution.stderr.is_some(),
                "stdout": {"text": "", "prefix_utf8_bytes": execution.stdout.as_ref().map_or(0, String::len), "truncated": execution.evidence.stdout_truncated},
                "stderr": {"text": "", "prefix_utf8_bytes": execution.stderr.as_ref().map_or(0, String::len), "truncated": execution.evidence.stderr_truncated},
            });
            if execution.evidence.state == argentum_domain::VerificationState::Failed {
                for (name,complete) in [("stdout",execution.stdout.as_ref().map(|_| !execution.evidence.stdout_truncated)),("stderr",execution.stderr.as_ref().map(|_| !execution.evidence.stderr_truncated))] {
                    value["feedback"][name]["diagnostic_excerpt"] = serde_json::Value::Null;
                    value["feedback"][name]["capture_complete"] = serde_json::json!(complete);
                }
            }
            Ok(value)
        })
        .collect::<Result<Vec<_>, ToolError>>()?;
    let metadata_bytes = serde_json::to_vec(&values).map_err(encode_error)?.len();
    let remaining = limit.checked_sub(metadata_bytes).ok_or_else(|| {
        ToolError::Execution("verification metadata exceeded the model-visible byte limit".into())
    })?;
    // String quotes are already reserved in metadata. Budget JSON-encoded
    // content, not raw UTF-8; repeated controls cannot escape this bound.
    let per_stream = (remaining / (2 * executions.len().max(1))).min(8 * 1024);
    for (value, execution) in values.iter_mut().zip(executions) {
        for (name, captured) in [
            ("stdout", execution.stdout.as_deref()),
            ("stderr", execution.stderr.as_deref()),
        ] {
            let Some(captured) = captured else { continue };
            let capture_complete = match name {
                "stdout" => !execution.evidence.stdout_truncated,
                _ => !execution.evidence.stderr_truncated,
            };
            let failed = execution.evidence.state == argentum_domain::VerificationState::Failed;
            let complete_prefix = argentum_execution::diagnostic_prefix_end(captured, per_stream);
            let needs_selection = failed && complete_prefix < captured.len();
            let prefix_budget = if needs_selection {
                per_stream / 4
            } else {
                per_stream
            };
            let excerpt = needs_selection
                .then(|| {
                    argentum_execution::diagnostic_excerpt(
                        captured,
                        true,
                        per_stream - prefix_budget,
                        Some(capture_complete),
                        false,
                    )
                })
                .flatten();
            // Legacy text remains a real contiguous prefix, never a stitched
            // excerpt. The new labelled selection uses the SAME stream budget.
            let end = argentum_execution::diagnostic_prefix_end(captured, prefix_budget);
            value["feedback"][name]["text"] = serde_json::json!(&captured[..end]);
            value["feedback"][name]["prefix_utf8_bytes"] = serde_json::json!(end);
            if end < captured.len() {
                value["feedback"][name]["truncated"] = serde_json::json!(true);
            }
            if failed {
                value["feedback"][name]["diagnostic_excerpt"] =
                    serde_json::to_value(excerpt).map_err(encode_error)?;
                value["feedback"][name]["capture_complete"] = serde_json::json!(capture_complete);
            }
        }
    }
    let output = serde_json::to_string(&values).map_err(encode_error)?;
    if output.len() > limit {
        return Err(ToolError::Execution(
            "verification feedback exceeded the model-visible byte limit".into(),
        ));
    }
    Ok(output)
}

fn resolve_cwd(
    workspace: &WorkspaceManager,
    scope: SandboxScope,
    cwd: Option<String>,
) -> Result<PathBuf, ToolError> {
    let Some(cwd) = cwd else {
        return Ok(workspace.root().to_path_buf());
    };
    Ok(match workspace.resolve_path_with_scope(cwd, scope)? {
        ScopedPath::Workspace { absolute, .. } | ScopedPath::Ambient(absolute) => absolute,
    })
}

fn success(summary: impl Into<String>, output: impl Into<String>) -> ToolResult {
    ToolResult {
        summary: summary.into(),
        output: output.into(),
        evidence: ToolExecutionEvidence::default(),
    }
}

struct SearchOutputEnvelope<'a, T> {
    report: &'a argentum_workspaces::WorkspaceSearchReport<T>,
    matches: &'a [serde_json::Value],
    output_truncated: bool,
}

impl<T> Serialize for SearchOutputEnvelope<'_, T> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut map = serializer.serialize_map(Some(26))?;
        map.serialize_entry("binary_files_skipped", &self.report.binary_files_skipped)?;
        map.serialize_entry("bytes_read", &self.report.bytes_read)?;
        map.serialize_entry(
            "complete",
            &(self.report.complete && !self.output_truncated),
        )?;
        map.serialize_entry("directories_opened", &self.report.directories_opened)?;
        map.serialize_entry("entries_visited", &self.report.entries_visited)?;
        map.serialize_entry("files_considered", &self.report.files_considered)?;
        map.serialize_entry("files_read", &self.report.files_read)?;
        map.serialize_entry("ignore_bytes_read", &self.report.ignore_bytes_read)?;
        map.serialize_entry("ignore_files_loaded", &self.report.ignore_files_loaded)?;
        map.serialize_entry("ignore_rules_loaded", &self.report.ignore_rules_loaded)?;
        map.serialize_entry(
            "ignored_directories_pruned",
            &self.report.ignored_directories_pruned,
        )?;
        map.serialize_entry("ignored_files_skipped", &self.report.ignored_files_skipped)?;
        map.serialize_entry("incomplete_reasons", &self.report.incomplete_reasons)?;
        map.serialize_entry("invalid_files_skipped", &self.report.invalid_files_skipped)?;
        map.serialize_entry("matches", self.matches)?;
        map.serialize_entry("matches_observed", &self.report.matches_observed)?;
        map.serialize_entry("matches_returned", &self.matches.len())?;
        map.serialize_entry("output_truncated", &self.output_truncated)?;
        map.serialize_entry(
            "oversized_files_skipped",
            &self.report.oversized_files_skipped,
        )?;
        map.serialize_entry(
            "policy_directories_pruned",
            &self.report.policy_directories_pruned,
        )?;
        map.serialize_entry("policy_files_skipped", &self.report.policy_files_skipped)?;
        map.serialize_entry("search_complete", &self.report.complete)?;
        map.serialize_entry(
            "stop_reason",
            &if self.output_truncated {
                "output_limit"
            } else {
                self.report.stop_reason.as_str()
            },
        )?;
        map.serialize_entry("symlinks_skipped", &self.report.symlinks_skipped)?;
        map.serialize_entry(
            "unreadable_directories",
            &self.report.unreadable_directories,
        )?;
        map.serialize_entry("unreadable_files", &self.report.unreadable_files)?;
        map.end()
    }
}

struct LimitedJsonWriter {
    bytes: Vec<u8>,
    limit: usize,
    limit_exceeded: bool,
}

impl LimitedJsonWriter {
    fn new(limit: usize) -> Self {
        Self {
            bytes: Vec::with_capacity(limit.min(64 * 1024)),
            limit,
            limit_exceeded: false,
        }
    }
}

impl Write for LimitedJsonWriter {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        let Some(next) = self.bytes.len().checked_add(bytes.len()) else {
            self.limit_exceeded = true;
            return Err(io::Error::other("search output limit exceeded"));
        };
        if next > self.limit {
            self.limit_exceeded = true;
            return Err(io::Error::other("search output limit exceeded"));
        }
        self.bytes.extend_from_slice(bytes);
        Ok(bytes.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

struct CountingJsonWriter {
    bytes: usize,
    limit: usize,
    limit_exceeded: bool,
}

impl CountingJsonWriter {
    const fn new(limit: usize) -> Self {
        Self {
            bytes: 0,
            limit,
            limit_exceeded: false,
        }
    }
}

impl Write for CountingJsonWriter {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        let Some(next) = self.bytes.checked_add(bytes.len()) else {
            self.limit_exceeded = true;
            return Err(io::Error::other("search output limit exceeded"));
        };
        if next > self.limit {
            self.limit_exceeded = true;
            return Err(io::Error::other("search output limit exceeded"));
        }
        self.bytes = next;
        Ok(bytes.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct BoundedSearchOutputWork {
    envelope_encodings: usize,
    size_probes: usize,
}

fn encode_search_output_bounded<T>(
    envelope: &SearchOutputEnvelope<'_, T>,
    limit: usize,
) -> Result<Option<String>, ToolError> {
    let mut writer = LimitedJsonWriter::new(limit);
    match serde_json::to_writer(&mut writer, envelope) {
        Ok(()) => String::from_utf8(writer.bytes)
            .map(Some)
            .map_err(|error_value| ToolError::Execution(error_value.to_string())),
        Err(_) if writer.limit_exceeded => Ok(None),
        Err(error_value) => Err(ToolError::Execution(error_value.to_string())),
    }
}

fn search_output_fits<T>(
    envelope: &SearchOutputEnvelope<'_, T>,
    limit: usize,
) -> Result<bool, ToolError> {
    let mut writer = CountingJsonWriter::new(limit);
    match serde_json::to_writer(&mut writer, envelope) {
        Ok(()) => Ok(true),
        Err(_) if writer.limit_exceeded => Ok(false),
        Err(error_value) => Err(ToolError::Execution(error_value.to_string())),
    }
}

fn bounded_search_output<T, C: FnMut() -> bool>(
    report: &argentum_workspaces::WorkspaceSearchReport<T>,
    matches: Vec<serde_json::Value>,
    cancelled: C,
) -> Result<String, ToolError> {
    bounded_search_output_with_limit(
        report,
        matches,
        argentum_domain::text_contract::MAX_TEXT_RESULT_BYTES,
        cancelled,
    )
    .map(|(output, _)| output)
}

fn bounded_search_output_with_limit<T, C: FnMut() -> bool>(
    report: &argentum_workspaces::WorkspaceSearchReport<T>,
    matches: Vec<serde_json::Value>,
    limit: usize,
    mut cancelled: C,
) -> Result<(String, BoundedSearchOutputWork), ToolError> {
    let mut work = BoundedSearchOutputWork::default();
    if cancelled() {
        return Err(ToolError::Cancelled);
    }

    let full = SearchOutputEnvelope {
        report,
        matches: &matches,
        output_truncated: false,
    };
    work.envelope_encodings = work.envelope_encodings.saturating_add(1);
    if let Some(output) = encode_search_output_bounded(&full, limit)? {
        return Ok((output, work));
    }
    if matches.is_empty() {
        return Err(ToolError::Execution(
            "search metadata exceeded the output budget".into(),
        ));
    }

    let mut low = 0_usize;
    let mut high = matches.len() - 1;
    let mut best = None;
    // Within the truncated family, every candidate changes only by appending one
    // complete match and updating matches_returned. JSON escaping and count digit
    // transitions are encoded by serde itself, so encoded size remains monotonic.
    // The differently flagged full representation was tested separately above.
    while low <= high {
        if cancelled() {
            return Err(ToolError::Cancelled);
        }
        let midpoint = low + (high - low) / 2;
        let candidate = SearchOutputEnvelope {
            report,
            matches: &matches[..midpoint],
            output_truncated: true,
        };
        work.envelope_encodings = work.envelope_encodings.saturating_add(1);
        work.size_probes = work.size_probes.saturating_add(1);
        if search_output_fits(&candidate, limit)? {
            best = Some(midpoint);
            low = midpoint.saturating_add(1);
        } else if midpoint == 0 {
            break;
        } else {
            high = midpoint - 1;
        }
    }

    let retained = best
        .ok_or_else(|| ToolError::Execution("search metadata exceeded the output budget".into()))?;
    if cancelled() {
        return Err(ToolError::Cancelled);
    }
    let selected = SearchOutputEnvelope {
        report,
        matches: &matches[..retained],
        output_truncated: true,
    };
    work.envelope_encodings = work.envelope_encodings.saturating_add(1);
    let output = encode_search_output_bounded(&selected, limit)?.ok_or_else(|| {
        ToolError::Execution("selected search output exceeded the output budget".into())
    })?;
    Ok((output, work))
}

fn process_tool_result(summary: &str, result: argentum_execution::ProcessResult) -> ToolResult {
    let outcome = match result.state {
        ProcessState::Exited if result.exit_code == Some(0) => ExecutionOutcome::Succeeded,
        ProcessState::Exited => ExecutionOutcome::Failed,
        ProcessState::TimedOut => ExecutionOutcome::TimedOut,
        ProcessState::Cancelled => ExecutionOutcome::Cancelled,
    };
    let output = match (result.stdout.is_empty(), result.stderr.is_empty()) {
        (false, true) => result.stdout.clone(),
        (true, false) => result.stderr.clone(),
        (false, false) => format!("stdout:\n{}\nstderr:\n{}", result.stdout, result.stderr),
        (true, true) => String::new(),
    };
    ToolResult {
        summary: summary.into(),
        output,
        evidence: ToolExecutionEvidence {
            outcome,
            exit_code: result.exit_code,
            stdout_sha256: Some(result.stdout_sha256),
            stderr_sha256: Some(result.stderr_sha256),
            stdout_bytes: result.stdout_bytes,
            stderr_bytes: result.stderr_bytes,
            stdout_truncated: result.stdout_truncated,
            stderr_truncated: result.stderr_truncated,
            changed_paths: Vec::new(),
            verification: Vec::new(),
            process_job: None,
            supplemental_check: None,
        },
    }
}

#[cfg(test)]
mod tests {
    use std::collections::{BTreeMap, BTreeSet};
    use std::fs;
    use std::process::Command;

    use argentum_domain::{
        ApprovalMode, GitOperation, GrantIssuer, GrantScope, GrantSubject, HarnessExecutionPolicy,
        PatchFile, SandboxScope, ToolRisk, VerificationRunnerId,
    };
    use argentum_security::{ApprovalPolicy, CapabilityBroker};
    use sha2::{Digest, Sha256};
    use tokio_util::sync::CancellationToken;
    use uuid::Uuid;

    use super::*;

    fn workspace() -> (tempfile::TempDir, WorkspaceManager) {
        let directory = tempfile::tempdir().expect("workspace");
        let broker = CapabilityBroker::new(directory.path(), ApprovalPolicy::default())
            .expect("capability broker");
        (directory, WorkspaceManager::new(broker))
    }

    fn context(workspace: WorkspaceManager) -> ToolContext {
        ToolContext::new(
            workspace,
            ApprovalGrant::for_capabilities([
                Capability::WriteFiles,
                Capability::ExecuteCommands,
                Capability::ExecuteShell,
                Capability::GitRead,
                Capability::GitWrite,
                Capability::Verify,
                Capability::ExternalProcess,
            ]),
            HarnessExecutionPolicy {
                profile_id: "developer".into(),
                source_profile_id: Some("developer".into()),
                sandbox_scope: SandboxScope::WorkspaceWrite,
                approval_mode: ApprovalMode::OnRisk,
                capability_enabled: BTreeMap::new(),
            },
            CancellationToken::new(),
        )
    }

    fn search_report<T>(
        matches: Vec<T>,
        complete: bool,
        stop_reason: argentum_workspaces::WorkspaceSearchStopReason,
    ) -> argentum_workspaces::WorkspaceSearchReport<T> {
        argentum_workspaces::WorkspaceSearchReport {
            matches_observed: matches.len() as u64,
            matches,
            complete,
            stop_reason,
            entries_visited: 23,
            directories_opened: 3,
            files_considered: 17,
            files_read: 11,
            bytes_read: 65_432,
            ignored_directories_pruned: 2,
            ignored_files_skipped: 4,
            policy_directories_pruned: 5,
            policy_files_skipped: 6,
            symlinks_skipped: 7,
            oversized_files_skipped: 8,
            binary_files_skipped: 9,
            invalid_files_skipped: 10,
            unreadable_directories: 11,
            unreadable_files: 12,
            ignore_files_loaded: 2,
            ignore_bytes_read: 321,
            ignore_rules_loaded: 13,
            incomplete_reasons: if complete {
                Vec::new()
            } else {
                vec![stop_reason.as_str().to_owned(), "byte_limit".into()]
            },
        }
    }

    fn linear_search_output_reference<T>(
        report: &argentum_workspaces::WorkspaceSearchReport<T>,
        matches: Vec<serde_json::Value>,
        limit: usize,
    ) -> Result<String, ToolError> {
        let mut retained = matches.len();
        let mut output_truncated = false;
        loop {
            let envelope = SearchOutputEnvelope {
                report,
                matches: &matches[..retained],
                output_truncated,
            };
            let output = serde_json::to_string(&envelope)
                .map_err(|error_value| ToolError::Execution(error_value.to_string()))?;
            if output.len() <= limit {
                return Ok(output);
            }
            if retained == 0 {
                return Err(ToolError::Execution(
                    "search metadata exceeded the output budget".into(),
                ));
            }
            retained -= 1;
            output_truncated = true;
        }
    }

    fn comparable_output(
        result: Result<(String, BoundedSearchOutputWork), ToolError>,
    ) -> Result<String, String> {
        result
            .map(|(output, _)| output)
            .map_err(|error_value| error_value.to_string())
    }

    fn comparable_reference(result: Result<String, ToolError>) -> Result<String, String> {
        result.map_err(|error_value| error_value.to_string())
    }

    fn truncated_output_size<T>(
        report: &argentum_workspaces::WorkspaceSearchReport<T>,
        matches: &[serde_json::Value],
        retained: usize,
    ) -> usize {
        serde_json::to_vec(&SearchOutputEnvelope {
            report,
            matches: &matches[..retained],
            output_truncated: true,
        })
        .expect("truncated search output")
        .len()
    }

    #[test]
    fn phase4_registry_registers_unique_complete_tool_families() {
        let (_directory, workspace) = workspace();
        let registry = ToolRegistry::with_builtins(workspace);
        let descriptors = registry.descriptors();
        let ids = descriptors
            .iter()
            .map(|descriptor| descriptor.id.as_str())
            .collect::<BTreeSet<_>>();
        assert_eq!(ids.len(), descriptors.len());
        for expected in [
            "list_directory",
            "file_metadata",
            "read_text",
            "read_binary_metadata",
            "search_text",
            "search_filenames",
            "create_directory",
            "apply_patch",
            "write_text",
            "move_file",
            "copy_file",
            "delete_file",
            "execute_argv",
            "execute_shell",
            "git_read",
            "git_write",
            "verify",
        ] {
            assert!(ids.contains(expected), "missing {expected}");
        }
        assert!(!ids.contains("network"));
        assert!(!ids.contains("persistent_terminal"));
        assert!(descriptors.iter().all(|descriptor| {
            !descriptor.capability_id.is_empty() && !descriptor.capabilities.is_empty()
        }));
    }

    #[test]
    fn phase4_registry_maps_dynamic_git_risk_to_distinct_descriptors() {
        let (_directory, workspace) = workspace();
        let registry = ToolRegistry::with_builtins(workspace);
        let read = registry
            .descriptor_for_input(&ToolInput::Git {
                operation: GitOperation::Status { short: true },
            })
            .expect("Git read descriptor");
        assert_eq!(read.id, "git_read");
        assert_eq!(read.risk, ToolRisk::GitRead);
        let mutation = registry
            .descriptor_for_input(&ToolInput::Git {
                operation: GitOperation::Add {
                    paths: vec!["one.txt".into()],
                },
            })
            .expect("Git write descriptor");
        assert_eq!(mutation.id, "git_write");
        assert_eq!(mutation.risk, ToolRisk::GitMutation);
    }

    #[tokio::test]
    async fn phase4_filesystem_tools_use_prepared_patch_and_return_changed_paths() {
        let (directory, workspace) = workspace();
        fs::write(directory.path().join("note.txt"), "before\nneedle\n").expect("fixture");
        let registry = ToolRegistry::with_builtins(workspace.clone());
        let request = ToolRequest {
            call_id: Uuid::new_v4(),
            run_id: Uuid::new_v4(),
            input: ToolInput::ApplyPatch {
                files: vec![PatchFile {
                    path: "note.txt".into(),
                    expected_sha256: None,
                    content: Some("after\nneedle\n".into()),
                }],
            },
        };
        let prepared = registry
            .prepare_mutations(&request, &workspace, SandboxScope::WorkspaceWrite)
            .expect("prepared mutation")
            .expect("patch mutation");
        let result = registry
            .execute(
                request,
                context(workspace.clone()).with_prepared_patch(prepared),
            )
            .await
            .expect("patch tool");
        assert_eq!(result.evidence.changed_paths, ["note.txt"]);
        assert_eq!(
            fs::read_to_string(directory.path().join("note.txt")).expect("updated read"),
            "after\nneedle\n"
        );

        let search = registry
            .execute(
                ToolRequest {
                    call_id: Uuid::new_v4(),
                    run_id: Uuid::new_v4(),
                    input: ToolInput::SearchText {
                        path: ".".into(),
                        query: "needle".into(),
                        max_results: 4,
                        mode: argentum_domain::TextSearchMode::Literal,
                        before_context: 0,
                        after_context: 0,
                    },
                },
                context(workspace.clone()),
            )
            .await
            .expect("search tool");
        let output: serde_json::Value = serde_json::from_str(&search.output).expect("search JSON");
        assert!(output["complete"].as_bool().expect("search completeness"));
        assert_eq!(output["matches_returned"], 1);
        assert_eq!(output["matches"][0]["path"], "note.txt");
        assert!(output["matches"][0]["line"]
            .as_str()
            .is_some_and(|line| line.contains("needle")));
        assert_eq!(output["matches"][0]["line_number"], 2);
        let selection = output["matches"][0]["read_selection"].clone();
        assert_eq!(selection["path"], "note.txt");
        assert_eq!(selection["start_line"], 1);
        assert_eq!(selection["line_count"], 2);
        assert_eq!(
            selection["expected_sha256"],
            format!("{:x}", Sha256::digest(b"after\nneedle\n"))
        );
        let mut read_value = selection;
        read_value["kind"] = serde_json::json!("read_text");
        let read_input: ToolInput =
            serde_json::from_value(read_value).expect("directly usable read selection");
        let read = registry
            .execute(
                ToolRequest {
                    call_id: Uuid::new_v4(),
                    run_id: Uuid::new_v4(),
                    input: read_input.clone(),
                },
                context(workspace.clone()),
            )
            .await
            .expect("search-led line read");
        let read_output: serde_json::Value =
            serde_json::from_str(&read.output).expect("line-read JSON");
        assert_eq!(read_output["content"], "after\nneedle\n");
        assert_eq!(read_output["requested_start_line"], 1);
        assert_eq!(read_output["returned_line_count"], 2);

        fs::write(directory.path().join("note.txt"), "changed\nneedle\n").expect("changed fixture");
        let stale = registry
            .execute(
                ToolRequest {
                    call_id: Uuid::new_v4(),
                    run_id: Uuid::new_v4(),
                    input: read_input,
                },
                context(workspace),
            )
            .await;
        assert!(matches!(
            stale,
            Err(ToolError::Workspace(
                WorkspaceError::ExpectedDigestMismatch(_)
            ))
        ));
    }

    #[tokio::test]
    async fn read_text_rejects_mixed_line_byte_modes_and_honors_cancellation() {
        let (directory, workspace) = workspace();
        fs::write(directory.path().join("note.txt"), "one\r\ntwo\r\n").expect("fixture");
        let registry = ToolRegistry::with_builtins(workspace.clone());
        let mixed = registry
            .execute(
                ToolRequest {
                    call_id: Uuid::new_v4(),
                    run_id: Uuid::new_v4(),
                    input: ToolInput::ReadText {
                        path: "note.txt".into(),
                        start_byte: Some(0),
                        end_byte: None,
                        start_line: Some(1),
                        line_count: Some(1),
                        expected_sha256: None,
                    },
                },
                context(workspace.clone()),
            )
            .await;
        assert!(matches!(mixed, Err(ToolError::InvalidInput(_))));

        let cancelled = context(workspace);
        cancelled.cancellation.cancel();
        let result = registry
            .execute(
                ToolRequest {
                    call_id: Uuid::new_v4(),
                    run_id: Uuid::new_v4(),
                    input: ToolInput::ReadText {
                        path: "note.txt".into(),
                        start_byte: None,
                        end_byte: None,
                        start_line: Some(1),
                        line_count: Some(1),
                        expected_sha256: None,
                    },
                },
                cancelled,
            )
            .await;
        assert!(matches!(result, Err(ToolError::Cancelled)));
    }

    #[tokio::test]
    async fn phase4_search_output_limit_preserves_truthful_metadata_and_json() {
        let (directory, workspace) = workspace();
        let content = (0..900)
            .map(|index| {
                format!(
                    "PRESSURE_{index:04} abcdefghijklmnopqrstuvwxyz0123456789abcdefghijklmnopqrstuvwxyz0123456789\n"
                )
            })
            .collect::<String>();
        fs::write(directory.path().join("many.txt"), content).expect("many search matches");
        let registry = ToolRegistry::with_builtins(workspace.clone());
        let result = registry
            .execute(
                ToolRequest {
                    call_id: Uuid::new_v4(),
                    run_id: Uuid::new_v4(),
                    input: ToolInput::SearchText {
                        path: ".".into(),
                        query: "^PRESSURE".into(),
                        max_results: 1_000,
                        mode: argentum_domain::TextSearchMode::Regex,
                        before_context: 2,
                        after_context: 2,
                    },
                },
                context(workspace),
            )
            .await
            .expect("output-bounded search tool");
        let output: serde_json::Value =
            serde_json::from_str(&result.output).expect("bounded search JSON");
        assert_eq!(output["complete"], false);
        assert_eq!(output["search_complete"], true);
        assert_eq!(output["output_truncated"], true);
        assert_eq!(output["stop_reason"], "output_limit");
        assert!(output["matches_returned"]
            .as_u64()
            .is_some_and(|count| count < 900));
        assert_eq!(output["matches"][0]["kind"], "match");
        assert_eq!(output["matches"][0]["context"]["before_requested"], 2);
        assert_eq!(output["matches"][0]["context"]["after_requested"], 2);
        assert!(result.output.len() <= argentum_domain::text_contract::MAX_TEXT_RESULT_BYTES);
        let returned = output["matches_returned"]
            .as_u64()
            .expect("returned match count");
        assert_eq!(
            returned as usize,
            output["matches"].as_array().unwrap().len()
        );
        assert!(returned > 0);
        assert_eq!(output["matches"][0]["path"], "many.txt");
        assert_eq!(output["matches"][0]["line_number"], 1);
        let last = &output["matches"][(returned - 1) as usize];
        assert_eq!(last["path"], "many.txt");
        assert_eq!(last["line_number"], returned);
        assert_eq!(last["read_selection"]["path"], "many.txt");
        assert_eq!(
            last["read_selection"]["expected_sha256"]
                .as_str()
                .map(str::len),
            Some(64)
        );
    }

    #[test]
    fn perf02_bounded_selector_matches_linear_reference_on_small_cases() {
        let escaped = (0..16)
            .map(|index| {
                serde_json::json!({
                    "kind": "match",
                    "path": format!("folder/quoted_\"{index}\\name.txt"),
                    "line_number": index + 1,
                    "line": format!("tab\t nul\u{0} snowman ☃ {index}"),
                    "line_truncated": index % 2 == 0,
                    "context": {
                        "before_requested": 8,
                        "after_requested": 8,
                        "before": [{"kind": "context", "line_number": index, "line": "crlf-derived", "line_truncated": false}],
                        "after": [{"kind": "context", "line_number": index + 2, "line": "overlap\\\"", "line_truncated": true}],
                        "complete": index % 3 != 0,
                    },
                })
            })
            .collect::<Vec<_>>();
        let cases = [
            Vec::new(),
            vec![serde_json::json!({"path": "one.txt"})],
            escaped,
        ];

        for (case_index, matches) in cases.into_iter().enumerate() {
            for complete in [true, false] {
                let stop_reason = if complete {
                    argentum_workspaces::WorkspaceSearchStopReason::Complete
                } else {
                    argentum_workspaces::WorkspaceSearchStopReason::ResultLimit
                };
                let report = search_report(vec![(); matches.len()], complete, stop_reason);
                let full_size = serde_json::to_vec(&SearchOutputEnvelope {
                    report: &report,
                    matches: &matches,
                    output_truncated: false,
                })
                .expect("full output")
                .len();
                let zero_size = truncated_output_size(&report, &matches, 0);
                let mut limits = BTreeSet::from([
                    0,
                    zero_size.saturating_sub(1),
                    zero_size,
                    zero_size.saturating_add(1),
                    full_size.saturating_sub(1),
                    full_size,
                    full_size.saturating_add(1),
                ]);
                if matches.len() > 1 {
                    let middle = truncated_output_size(&report, &matches, matches.len() / 2);
                    limits.extend([middle.saturating_sub(1), middle, middle.saturating_add(1)]);
                }

                for limit in limits {
                    let expected = comparable_reference(linear_search_output_reference(
                        &report,
                        matches.clone(),
                        limit,
                    ));
                    let actual = comparable_output(bounded_search_output_with_limit(
                        &report,
                        matches.clone(),
                        limit,
                        || false,
                    ));
                    assert_eq!(
                        actual, expected,
                        "case {case_index}, complete {complete}, limit {limit}"
                    );
                }
            }
        }
    }

    #[test]
    fn perf02_exact_limit_and_decimal_transitions_select_maximal_prefix() {
        let one_match_report = search_report(
            vec![()],
            true,
            argentum_workspaces::WorkspaceSearchStopReason::Complete,
        );
        for target_size in [65_535_usize, 65_536, 65_537] {
            let empty = vec![serde_json::json!({"line": ""})];
            let base_size = serde_json::to_vec(&SearchOutputEnvelope {
                report: &one_match_report,
                matches: &empty,
                output_truncated: false,
            })
            .expect("base output")
            .len();
            let matches = vec![serde_json::json!({"line": "x".repeat(target_size - base_size)})];
            let encoded_size = serde_json::to_vec(&SearchOutputEnvelope {
                report: &one_match_report,
                matches: &matches,
                output_truncated: false,
            })
            .expect("sized output")
            .len();
            assert_eq!(encoded_size, target_size);

            let expected = comparable_reference(linear_search_output_reference(
                &one_match_report,
                matches.clone(),
                65_536,
            ));
            let actual = comparable_output(bounded_search_output_with_limit(
                &one_match_report,
                matches,
                65_536,
                || false,
            ));
            assert_eq!(actual, expected, "target size {target_size}");
            let output: serde_json::Value =
                serde_json::from_str(actual.as_ref().expect("bounded output"))
                    .expect("bounded JSON");
            assert_eq!(
                output["matches_returned"],
                if target_size <= 65_536 { 1 } else { 0 }
            );
        }

        let matches = (0..101)
            .map(|index| serde_json::json!({"path": format!("file-{index:03}.txt")}))
            .collect::<Vec<_>>();
        let report = search_report(
            vec![(); matches.len()],
            true,
            argentum_workspaces::WorkspaceSearchStopReason::Complete,
        );
        for retained in [9_usize, 10, 99, 100] {
            let limit = truncated_output_size(&report, &matches, retained);
            let (output, _) =
                bounded_search_output_with_limit(&report, matches.clone(), limit, || false)
                    .expect("decimal-boundary output");
            let output: serde_json::Value =
                serde_json::from_str(&output).expect("decimal-boundary JSON");
            assert_eq!(output["matches_returned"], retained);
            assert_eq!(
                output["matches"][retained - 1]["path"],
                matches[retained - 1]["path"]
            );
        }
    }

    #[test]
    fn perf02_metadata_overflow_and_incomplete_traversal_remain_truthful() {
        let empty_report = search_report(
            Vec::<()>::new(),
            true,
            argentum_workspaces::WorkspaceSearchStopReason::Complete,
        );
        let error = bounded_search_output_with_limit(&empty_report, Vec::new(), 1, || false)
            .expect_err("metadata-only overflow");
        assert!(matches!(error, ToolError::Execution(_)));
        assert!(error
            .to_string()
            .contains("search metadata exceeded the output budget"));

        let matches = (0..20)
            .map(|index| serde_json::json!({"line": "x".repeat(512), "index": index}))
            .collect::<Vec<_>>();
        let report = search_report(
            vec![(); matches.len()],
            false,
            argentum_workspaces::WorkspaceSearchStopReason::ResultLimit,
        );
        let limit = truncated_output_size(&report, &matches, 3);
        let (output, _) = bounded_search_output_with_limit(&report, matches, limit, || false)
            .expect("incomplete bounded output");
        let output: serde_json::Value = serde_json::from_str(&output).expect("incomplete JSON");
        assert_eq!(output["search_complete"], false);
        assert_eq!(output["complete"], false);
        assert_eq!(output["output_truncated"], true);
        assert_eq!(output["stop_reason"], "output_limit");
        assert_eq!(output["incomplete_reasons"][0], "result_limit");
        assert_eq!(output["incomplete_reasons"][1], "byte_limit");
        assert_eq!(output["matches_observed"], 20);
        assert_eq!(output["matches_returned"], 3);
        assert_eq!(output["binary_files_skipped"], 9);
        assert_eq!(output["policy_files_skipped"], 6);
    }

    #[test]
    fn perf02_probe_work_is_logarithmic_and_cancellable() {
        let matches = (0..900)
            .map(|index| {
                serde_json::json!({
                    "path": "many.txt",
                    "line_number": index + 1,
                    "line": format!("needle {index:04} {}", "x".repeat(64)),
                })
            })
            .collect::<Vec<_>>();
        let report = search_report(
            vec![(); matches.len()],
            true,
            argentum_workspaces::WorkspaceSearchStopReason::Complete,
        );
        let (output, work) = bounded_search_output_with_limit(
            &report,
            matches.clone(),
            argentum_domain::text_contract::MAX_TEXT_RESULT_BYTES,
            || false,
        )
        .expect("pressure output");
        assert!(output.len() <= argentum_domain::text_contract::MAX_TEXT_RESULT_BYTES);
        assert!(work.size_probes <= 10, "unexpected probes: {work:?}");
        assert!(work.envelope_encodings <= 12, "unexpected work: {work:?}");

        let small_report = search_report(
            vec![()],
            true,
            argentum_workspaces::WorkspaceSearchStopReason::Complete,
        );
        let (_, fast_work) = bounded_search_output_with_limit(
            &small_report,
            vec![serde_json::json!({"path": "fits.txt"})],
            argentum_domain::text_contract::MAX_TEXT_RESULT_BYTES,
            || false,
        )
        .expect("fast-path output");
        assert_eq!(
            fast_work,
            BoundedSearchOutputWork {
                envelope_encodings: 1,
                size_probes: 0,
            }
        );

        let mut cancellation_checks = 0;
        let error = bounded_search_output_with_limit(
            &report,
            matches,
            argentum_domain::text_contract::MAX_TEXT_RESULT_BYTES,
            || {
                cancellation_checks += 1;
                cancellation_checks >= 3
            },
        )
        .expect_err("cancelled between probes");
        assert!(matches!(error, ToolError::Cancelled));
        assert_eq!(cancellation_checks, 3);
    }

    #[tokio::test]
    async fn perf02_filename_search_caller_uses_the_same_bounded_contract() {
        let (directory, workspace) = workspace();
        for index in 0..450 {
            let name = format!("item-{index:03}-{}.txt", "x".repeat(140));
            fs::write(directory.path().join(name), "fixture").expect("filename fixture");
        }
        let registry = ToolRegistry::with_builtins(workspace.clone());
        let result = registry
            .execute(
                ToolRequest {
                    call_id: Uuid::new_v4(),
                    run_id: Uuid::new_v4(),
                    input: ToolInput::SearchFilenames {
                        path: ".".into(),
                        pattern: "*.txt".into(),
                        max_results: 1_000,
                    },
                },
                context(workspace),
            )
            .await
            .expect("bounded filename search");
        let output: serde_json::Value =
            serde_json::from_str(&result.output).expect("filename search JSON");
        assert_eq!(output["search_complete"], true);
        assert_eq!(output["output_truncated"], true);
        assert_eq!(output["stop_reason"], "output_limit");
        assert_eq!(output["matches_observed"], 450);
        assert!(output["matches_returned"]
            .as_u64()
            .is_some_and(|count| count < 450));
        assert_eq!(
            output["matches"][0]["path"],
            format!("item-000-{}.txt", "x".repeat(140))
        );
        assert!(result.output.len() <= argentum_domain::text_contract::MAX_TEXT_RESULT_BYTES);
    }

    #[tokio::test]
    async fn f12b_regex_context_is_typed_and_invalid_patterns_do_not_traverse() {
        let (directory, workspace) = workspace();
        fs::write(
            directory.path().join("context.txt"),
            "before\r\nDEFECT_42\r\nafter\r\nother DEFECT_x\r\n",
        )
        .expect("regex context fixture");
        let registry = ToolRegistry::with_builtins(workspace.clone());
        let result = registry
            .execute(
                ToolRequest {
                    call_id: Uuid::new_v4(),
                    run_id: Uuid::new_v4(),
                    input: ToolInput::SearchText {
                        path: ".".into(),
                        query: r"^DEFECT_[0-9]+$".into(),
                        max_results: 8,
                        mode: argentum_domain::TextSearchMode::Regex,
                        before_context: 1,
                        after_context: 1,
                    },
                },
                context(workspace.clone()),
            )
            .await
            .expect("typed regex context search");
        let output: serde_json::Value =
            serde_json::from_str(&result.output).expect("regex context JSON");
        assert_eq!(output["matches_returned"], 1);
        assert_eq!(output["matches"][0]["line_number"], 2);
        assert_eq!(output["matches"][0]["kind"], "match");
        assert_eq!(
            output["matches"][0]["context"]["before"][0]["line"],
            "before"
        );
        assert_eq!(output["matches"][0]["context"]["after"][0]["line"], "after");
        assert_eq!(output["matches"][0]["context"]["complete"], true);
        assert_eq!(output["matches_observed"], 1);

        for (query, before_context) in [("(", 0), ("value", 9)] {
            let error = registry
                .execute(
                    ToolRequest {
                        call_id: Uuid::new_v4(),
                        run_id: Uuid::new_v4(),
                        input: ToolInput::SearchText {
                            path: ".".into(),
                            query: query.into(),
                            max_results: 8,
                            mode: argentum_domain::TextSearchMode::Regex,
                            before_context,
                            after_context: 0,
                        },
                    },
                    context(workspace.clone()),
                )
                .await
                .expect_err("invalid regex input");
            assert!(matches!(error, ToolError::InvalidInput(_)));
        }
    }

    #[tokio::test]
    async fn phase4_pre_cancelled_file_tool_does_not_apply_its_prepared_patch() {
        let (directory, workspace) = workspace();
        fs::write(directory.path().join("note.txt"), "before").expect("fixture");
        let registry = ToolRegistry::with_builtins(workspace.clone());
        let request = ToolRequest {
            call_id: Uuid::new_v4(),
            run_id: Uuid::new_v4(),
            input: ToolInput::WriteText {
                path: "note.txt".into(),
                content: "after".into(),
            },
        };
        let prepared = registry
            .prepare_mutations(&request, &workspace, SandboxScope::WorkspaceWrite)
            .expect("prepared mutation")
            .expect("write mutation");
        let tool_context = context(workspace).with_prepared_patch(prepared);
        tool_context.cancellation.cancel();

        let result = registry.execute(request, tool_context).await;

        assert!(matches!(result, Err(ToolError::Cancelled)));
        assert_eq!(
            fs::read_to_string(directory.path().join("note.txt")).expect("unchanged file"),
            "before"
        );
    }

    #[tokio::test]
    async fn phase4_full_access_inspection_and_directory_creation_use_unrestricted_scope() {
        let fixture = tempfile::tempdir().expect("fixture root");
        let workspace_root = fixture.path().join("workspace");
        let ambient_root = fixture.path().join("ambient");
        fs::create_dir_all(&workspace_root).expect("workspace directory");
        fs::create_dir_all(&ambient_root).expect("ambient directory");
        fs::write(ambient_root.join("note.txt"), "ambient needle").expect("ambient fixture");
        let broker = CapabilityBroker::new(&workspace_root, ApprovalPolicy::default())
            .expect("capability broker");
        let workspace = WorkspaceManager::new(broker);
        let registry = ToolRegistry::with_builtins(workspace.clone());
        let mut full_access = context(workspace.clone());
        full_access.policy.sandbox_scope = SandboxScope::Unrestricted;

        let read = registry
            .execute(
                ToolRequest {
                    call_id: Uuid::new_v4(),
                    run_id: Uuid::new_v4(),
                    input: ToolInput::ReadText {
                        path: ambient_root.join("note.txt").to_string_lossy().into_owned(),
                        start_byte: Some(0),
                        end_byte: None,
                        start_line: None,
                        line_count: None,
                        expected_sha256: None,
                    },
                },
                full_access.clone(),
            )
            .await
            .expect("ambient read");
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(&read.output).unwrap()["content"],
            "ambient needle"
        );

        let search = registry
            .execute(
                ToolRequest {
                    call_id: Uuid::new_v4(),
                    run_id: Uuid::new_v4(),
                    input: ToolInput::SearchFilenames {
                        path: ambient_root.to_string_lossy().into_owned(),
                        pattern: "*.txt".into(),
                        max_results: 8,
                    },
                },
                full_access.clone(),
            )
            .await
            .expect("ambient search");
        assert!(search.output.contains("note.txt"));

        let created_path = ambient_root.join("created");
        let create = registry
            .execute(
                ToolRequest {
                    call_id: Uuid::new_v4(),
                    run_id: Uuid::new_v4(),
                    input: ToolInput::CreateDirectory {
                        path: created_path.to_string_lossy().into_owned(),
                    },
                },
                full_access,
            )
            .await
            .expect("ambient directory creation");
        assert_eq!(
            create.evidence.changed_paths,
            [created_path.to_string_lossy().into_owned()]
        );
        assert!(created_path.is_dir());
    }

    #[cfg(windows)]
    #[tokio::test]
    async fn phase4_execution_and_git_tools_return_bounded_process_evidence() {
        let (directory, workspace) = workspace();
        let registry = ToolRegistry::with_builtins(workspace.clone());
        let process = registry
            .execute(
                ToolRequest {
                    call_id: Uuid::new_v4(),
                    run_id: Uuid::new_v4(),
                    input: ToolInput::ExecuteArgv {
                        program: "powershell.exe".into(),
                        args: vec![
                            "-NoLogo".into(),
                            "-NoProfile".into(),
                            "-NonInteractive".into(),
                            "-Command".into(),
                            "[Console]::Out.Write('safe'); exit 3".into(),
                        ],
                        cwd: None,
                        timeout_ms: 5_000,
                        environment: BTreeMap::new(),
                    },
                },
                context(workspace.clone()),
            )
            .await
            .expect("argv tool");
        assert_eq!(process.output, "safe");
        assert_eq!(process.evidence.exit_code, Some(3));
        assert_eq!(
            process
                .evidence
                .stdout_sha256
                .as_ref()
                .expect("hash")
                .0
                .len(),
            64
        );

        for arguments in [
            vec!["init", "--quiet"],
            vec!["config", "user.name", "Argentum Test"],
            vec!["config", "user.email", "argentum-test@example.invalid"],
        ] {
            assert!(Command::new("git")
                .args(arguments)
                .current_dir(directory.path())
                .status()
                .expect("Git setup")
                .success());
        }
        fs::write(directory.path().join("one.txt"), "one").expect("Git fixture");
        let git = registry
            .execute(
                ToolRequest {
                    call_id: Uuid::new_v4(),
                    run_id: Uuid::new_v4(),
                    input: ToolInput::Git {
                        operation: GitOperation::Status { short: true },
                    },
                },
                context(workspace),
            )
            .await
            .expect("Git tool");
        assert!(git.output.contains("one.txt"));
        assert_eq!(git.evidence.exit_code, Some(0));
    }

    #[test]
    fn phase4_registry_exposes_the_fixed_verification_runner_ids() {
        let (_directory, workspace) = workspace();
        let registry = ToolRegistry::with_builtins(workspace);
        assert_eq!(
            registry.verification_runners(),
            [
                VerificationRunnerId::CargoFmtCheck,
                VerificationRunnerId::CargoCheckLocked,
                VerificationRunnerId::CargoTestLocked,
                VerificationRunnerId::CargoClippyStrict,
            ]
        );
    }

    #[tokio::test]
    async fn model_default_read_is_small_and_explicit_ranges_keep_the_existing_bound() {
        let (directory, workspace) = workspace();
        let source = "read-preview-data\n".repeat(10000);
        fs::write(directory.path().join("large.txt"), &source).unwrap();
        let registry = ToolRegistry::with_builtins(workspace.clone());
        for (end_byte, maximum) in [
            (None, DEFAULT_MODEL_READ_RESULT_BYTES),
            (
                Some(source.len() as u64),
                argentum_domain::text_contract::MAX_TEXT_RESULT_BYTES,
            ),
        ] {
            let result = registry
                .execute(
                    ToolRequest {
                        call_id: Uuid::new_v4(),
                        run_id: Uuid::new_v4(),
                        input: ToolInput::ReadText {
                            path: "large.txt".into(),
                            start_byte: None,
                            end_byte,
                            start_line: None,
                            line_count: None,
                            expected_sha256: None,
                        },
                    },
                    context(workspace.clone()),
                )
                .await
                .unwrap();
            assert!(result.output.len() <= maximum);
            let read: argentum_domain::text_contract::TextReadResult =
                serde_json::from_str(&result.output).unwrap();
            assert_eq!(read.requested_end_byte, source.len() as u64);
            assert!(!read.range_complete);
            assert_eq!(read.content, source[..read.returned_end_byte as usize]);
            if end_byte.is_some() {
                assert!(read.content.len() > DEFAULT_MODEL_READ_RESULT_BYTES);
            }
        }
    }

    #[cfg(windows)]
    #[tokio::test]
    async fn synchronous_verify_returns_real_diagnostics_without_changing_receipts() {
        let (directory, workspace) = workspace();
        fs::create_dir(directory.path().join("src")).unwrap();
        fs::write(
            directory.path().join("Cargo.toml"),
            "[package]\nname=\"verify_feedback_fixture\"\nversion=\"0.1.0\"\nedition=\"2021\"\n",
        )
        .unwrap();
        fs::write(
            directory.path().join("Cargo.lock"),
            "version = 4\n\n[[package]]\nname = \"verify_feedback_fixture\"\nversion = \"0.1.0\"\n",
        )
        .unwrap();
        fs::write(
            directory.path().join("src/lib.rs"),
            "pub fn value() -> bool { true } }\n",
        )
        .unwrap();
        let registry = ToolRegistry::with_builtins(workspace.clone());
        let result = registry
            .execute(
                ToolRequest {
                    call_id: Uuid::new_v4(),
                    run_id: Uuid::new_v4(),
                    input: ToolInput::Verify {
                        runners: vec![VerificationRunnerId::CargoCheckLocked],
                    },
                },
                context(workspace),
            )
            .await
            .unwrap();
        assert_eq!(result.evidence.outcome, ExecutionOutcome::Failed);
        let value: serde_json::Value = serde_json::from_str(&result.output).unwrap();
        assert_eq!(value[0]["state"], "failed");
        assert!(value[0]["feedback"]["stderr"]["text"]
            .as_str()
            .unwrap()
            .contains("unexpected closing delimiter"));
        assert_eq!(value[0]["feedback"]["output_available"], true);
        let receipts: Vec<VerificationEvidence> = serde_json::from_str(&result.output).unwrap();
        assert_eq!(receipts, result.evidence.verification);
        assert!(!serde_json::to_string(&receipts)
            .unwrap()
            .contains("unexpected closing delimiter"));

        let captured = format!(
            "SYNTHETIC_OUTPUT_CANARY{}",
            "æ\\\"\n\r\t\u{0001}".repeat(20000)
        );
        let executions = (0..4)
            .map(|_| {
                let mut evidence = receipts[0].clone();
                evidence.id = Uuid::new_v4();
                argentum_execution::VerificationRunOutput {
                    evidence,
                    stdout: Some(captured.clone()),
                    stderr: Some(captured.clone()),
                }
            })
            .collect::<Vec<_>>();
        assert!(!format!("{:?}", executions[0]).contains("SYNTHETIC_OUTPUT_CANARY"));
        let bounded = verification_feedback_output(&executions).unwrap();
        assert!(bounded.len() <= argentum_domain::text_contract::MAX_TEXT_RESULT_BYTES);
        let parsed: serde_json::Value = serde_json::from_str(&bounded).unwrap();
        assert_eq!(parsed.as_array().unwrap().len(), 4);
        assert!(parsed
            .as_array()
            .unwrap()
            .iter()
            .all(|record| record["feedback"]["stdout"]["truncated"] == true
                && record["feedback"]["stderr"]["truncated"] == true));
        let metadata: Vec<VerificationEvidence> = serde_json::from_str(&bounded).unwrap();
        assert_eq!(
            metadata,
            executions
                .iter()
                .map(|execution| execution.evidence.clone())
                .collect::<Vec<_>>()
        );
    }

    #[tokio::test]
    async fn project_verification_requires_exact_grant_and_runs_after_binding() {
        let (directory, workspace) = workspace();
        fs::create_dir_all(directory.path().join(".agents")).expect("agents directory");
        fs::write(
            directory.path().join(".agents/verification.yaml"),
            "version: 1\nrunners:\n  - id: cargo-version\n    label: Cargo version\n    program: cargo\n    args: [--version]\n    cwd: .\n    timeout_ms: 30000\n    output_limit: 65536\n",
        )
        .expect("project verification config");
        let runner = VerificationRunnerId::Project("cargo-version".into());
        let request = ToolRequest {
            call_id: Uuid::new_v4(),
            run_id: Uuid::new_v4(),
            input: ToolInput::Verify {
                runners: vec![runner.clone()],
            },
        };
        let registry = ToolRegistry::with_builtins(workspace.clone());
        let criteria = registry
            .goal_criteria_for_workspace(&workspace, std::slice::from_ref(&runner))
            .expect("project runner criterion");
        registry
            .validate_goal_criteria_for_workspace(&workspace, &criteria)
            .expect("same canonical registry");
        assert!(registry
            .goal_criteria(std::slice::from_ref(&runner))
            .is_err());
        assert!(registry.validate_goal_criteria(&criteria).is_err());
        let denied = registry
            .execute(request.clone(), context(workspace.clone()))
            .await
            .expect_err("unapproved project verification");
        assert!(matches!(
            denied,
            ToolError::Verification(VerificationError::ProjectApprovalRequired)
        ));

        let binding = project_verification_binding(&workspace, std::slice::from_ref(&runner))
            .expect("project verification binding");
        let grant = ApprovalGrantRecord {
            id: Uuid::new_v4(),
            audit_id: Uuid::new_v4(),
            capability: Capability::ExternalProcess,
            scope: GrantScope::Action,
            subject: GrantSubject::Action {
                tool_id: "verify".into(),
                target: "fixed runners: project:cargo-version".into(),
                binding,
            },
            created_at: now(),
            expires_at: None,
            issuer: GrantIssuer::User,
            reason: "test owner approval".into(),
            revoked_at: None,
        };
        let passed = registry
            .execute(request, context(workspace.clone()).with_grants(vec![grant]))
            .await
            .expect("approved project verification");
        assert_eq!(passed.evidence.verification.len(), 1);
        assert_eq!(
            passed.evidence.verification[0].state,
            argentum_domain::VerificationState::Passed
        );
        assert_eq!(passed.evidence.verification[0].runner, runner);
        fs::write(directory.path().join(".agents/verification.yaml"), "version: 1\nrunners:\n  - id: cargo-version\n    label: Cargo changed\n    program: cargo\n    args: [--version, --verbose]\n    cwd: .\n    timeout_ms: 30000\n    output_limit: 65536\n").expect("changed runner definition");
        assert!(registry
            .validate_goal_criteria_for_workspace(&workspace, &criteria)
            .is_err());
    }
}
