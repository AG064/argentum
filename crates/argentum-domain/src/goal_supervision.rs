//! Bounded owner-started foreground supervision and declared acceptance evidence.
use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use uuid::Uuid;

use crate::{
    ProjectId, RunId, SessionId, Sha256Digest, TaskId, VerificationId, VerificationRunnerId,
};

pub const SUPERVISION_VERSION: u16 = 1;
pub const MAX_ACCEPTANCE_CRITERIA: usize = 4;
pub const MAX_INVOCATION_ITERATIONS: u32 = 16;
pub const MAX_INVOCATION_SECONDS: u64 = 86_400;
pub const MAX_PROGRESS_CHECKPOINTS: usize = 8;
pub const NO_PROGRESS_STOP_THRESHOLD: u8 = 2;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VerificationInputScope {
    pub method: String,
    pub excluded_components: Vec<String>,
    pub max_files: u64,
    pub max_total_bytes: u64,
    pub max_file_bytes: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project_inputs: Option<VerificationProjectInputs>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub program_path: Option<String>,
}

/// Explicit owner-reviewed resource bounds, not inferred package-manager defaults.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VerificationInputLimits {
    pub max_files: u64,
    pub max_total_bytes: u64,
    pub max_file_bytes: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VerificationInventoryLimits {
    pub max_entries: u64,
    pub max_depth: u32,
}

/// Source remains the workspace outside these separately observed roots.
/// Directory roles, bounds and interpretation are part of the approved runner.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VerificationProjectInputs {
    pub program: VerificationProgramIdentity,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub fixed_files: Vec<VerificationFixedFile>,
    pub dependency_roots: Vec<String>,
    pub generated_output_roots: Vec<String>,
    pub dependency_limits: VerificationInputLimits,
    pub generated_limits: VerificationInputLimits,
    pub inventory_limits: VerificationInventoryLimits,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VerificationProgramIdentity {
    pub sha256: Sha256Digest,
    pub bytes: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VerificationFixedFile {
    pub path: String,
    pub sha256: Sha256Digest,
    pub bytes: u64,
}

/// Content identity observed at an endpoint, not proof against transient changes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VerificationTreeIdentity {
    pub path: String,
    pub exists: bool,
    pub digest: Sha256Digest,
    pub file_count: u64,
    pub total_bytes: u64,
}

/// Existing generated files are recorded before execution as existing inputs.
/// After identities bind later output use without calling output changes source edits.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VerificationGeneratedOutputs {
    pub before: Vec<VerificationTreeIdentity>,
    pub after: Vec<VerificationTreeIdentity>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VerificationInputSnapshot {
    pub method: String,
    pub digest: Sha256Digest,
    /// Remaining source only; installed inputs have their own counted identities.
    pub file_count: u64,
    pub total_bytes: u64,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub dependencies: Vec<VerificationTreeIdentity>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GoalCriterion {
    pub id: Uuid,
    pub runner: VerificationRunnerId,
    pub registry_version: u16,
    pub definition_sha256: Sha256Digest,
    pub input_scope: VerificationInputScope,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GoalAcceptanceDefinition {
    pub version: u32,
    pub criteria: Vec<GoalCriterion>,
    pub owner_attestation_required: bool,
    pub configured_at: OffsetDateTime,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GoalOwnerAttestation {
    pub id: Uuid,
    pub definition_revision: Uuid,
    pub criteria_version: u32,
    pub summary: String,
    pub recorded_at: OffsetDateTime,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CriterionEvidenceState {
    Passed,
    Failed,
    Cancelled,
    Unavailable,
    Unknown,
    Stale,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CriterionEvaluation {
    pub criterion_id: Uuid,
    pub runner: VerificationRunnerId,
    pub state: CriterionEvidenceState,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub evidence_id: Option<VerificationId>,
    pub detail: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InvocationAttemptOutcome {
    Admitted,
    ScheduledHandoff,
    WorkIncomplete,
    CriteriaFailed,
    Verified,
    RuntimeFailed,
    Cancelled,
    Unresolved,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DeliveryPhase {
    #[default]
    Implementation,
    Verification,
    Feedback,
}

/// Native stage metadata, not execution authority or natural-language proof.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeliveryProgress {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub configuration_sha256: Option<Sha256Digest>,
    #[serde(default)]
    pub remaining: Vec<String>,
    pub phase: DeliveryPhase,
    pub implementation_ready: bool,
    pub feedback_acknowledged: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reported_source: Option<VerificationInputSnapshot>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GoalInvocationAttempt {
    pub ordinal: u32,
    pub task_id: TaskId,
    pub run_id: RunId,
    pub source_before: VerificationInputSnapshot,
    pub criteria_before: Vec<CriterionEvaluation>,
    pub outcome: InvocationAttemptOutcome,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub delivery_phase: Option<DeliveryPhase>,
    pub evidence_ids: Vec<VerificationId>,
    pub started_at: OffsetDateTime,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub finished_at: Option<OffsetDateTime>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stop_reason: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProgressCheckpoint {
    pub source: VerificationInputSnapshot,
    pub normalized_criteria_sha256: Sha256Digest,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub observation_progress_sha256: Option<Sha256Digest>,
    pub repeated_count: u8,
    pub recorded_at: OffsetDateTime,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GoalInvocationState {
    Active,
    Verified,
    Incomplete,
    Failed,
    Interrupted,
    Cancelled,
    ReviewRequired,
    Unresolved,
}

impl GoalInvocationState {
    pub const fn is_terminal(self) -> bool {
        !matches!(self, Self::Active)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GoalInvocation {
    pub version: u16,
    pub id: Uuid,
    pub goal_id: Uuid,
    pub project_id: ProjectId,
    pub session_id: SessionId,
    pub workspace_key: String,
    pub owner_claim_id: Uuid,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub owner_generation: Option<u64>,
    #[serde(default)]
    pub owner_claim_retired: bool,
    pub definition_revision: Uuid,
    pub criteria_version: u32,
    pub accounting_revision: Uuid,
    pub iteration_limit: u32,
    pub elapsed_seconds: u64,
    pub started_at: OffsetDateTime,
    pub deadline_at: OffsetDateTime,
    pub state: GoalInvocationState,
    pub attempts: Vec<GoalInvocationAttempt>,
    pub evaluated_criteria: Vec<CriterionEvaluation>,
    pub progress: Vec<ProgressCheckpoint>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub delivery: Option<DeliveryProgress>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stop_reason: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub finished_at: Option<OffsetDateTime>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GoalInvocationStatus {
    pub version: u16,
    pub id: Uuid,
    pub goal_id: Uuid,
    pub project_id: ProjectId,
    pub session_id: SessionId,
    pub workspace_key: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub owner_generation: Option<u64>,
    pub owner_claim_retired: bool,
    pub definition_revision: Uuid,
    pub criteria_version: u32,
    pub accounting_revision: Uuid,
    pub iteration_limit: u32,
    pub elapsed_seconds: u64,
    pub started_at: OffsetDateTime,
    pub deadline_at: OffsetDateTime,
    pub state: GoalInvocationState,
    pub attempts: Vec<GoalInvocationAttempt>,
    pub evaluated_criteria: Vec<CriterionEvaluation>,
    pub progress: Vec<ProgressCheckpoint>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub delivery: Option<DeliveryProgress>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stop_reason: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub finished_at: Option<OffsetDateTime>,
}

impl From<&GoalInvocation> for GoalInvocationStatus {
    fn from(invocation: &GoalInvocation) -> Self {
        Self {
            version: invocation.version,
            id: invocation.id,
            goal_id: invocation.goal_id,
            project_id: invocation.project_id,
            session_id: invocation.session_id,
            workspace_key: invocation.workspace_key.clone(),
            owner_generation: invocation.owner_generation,
            owner_claim_retired: invocation.owner_claim_retired,
            definition_revision: invocation.definition_revision,
            criteria_version: invocation.criteria_version,
            accounting_revision: invocation.accounting_revision,
            iteration_limit: invocation.iteration_limit,
            elapsed_seconds: invocation.elapsed_seconds,
            started_at: invocation.started_at,
            deadline_at: invocation.deadline_at,
            state: invocation.state,
            attempts: invocation.attempts.clone(),
            evaluated_criteria: invocation.evaluated_criteria.clone(),
            progress: invocation.progress.clone(),
            delivery: invocation.delivery.clone(),
            stop_reason: invocation.stop_reason.clone(),
            finished_at: invocation.finished_at,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GoalCompletionReceipt {
    pub invocation_id: Uuid,
    pub definition_revision: Uuid,
    pub criteria_version: u32,
    pub source: VerificationInputSnapshot,
    pub evidence_ids: Vec<VerificationId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub owner_attestation_id: Option<Uuid>,
    pub statement: String,
    pub recorded_at: OffsetDateTime,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GoalSupervisionInspection {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub invocation: Option<GoalInvocationStatus>,
    pub current_criteria: Vec<CriterionEvaluation>,
    pub completion_current: bool,
    pub completion_status: String,
    pub interrupted_claim_blocks_reentry: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SupervisionRequest {
    pub goal_id: Uuid,
    pub iteration_limit: u32,
    pub elapsed_seconds: u64,
    pub pause_on_session_switch: bool,
}
