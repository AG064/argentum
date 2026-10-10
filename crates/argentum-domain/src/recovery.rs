//! Durable local-host ownership and interrupted-run reconciliation metadata.

use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use uuid::Uuid;

use crate::{ProjectId, RunId, SessionId, Sha256Digest, TaskId, ToolCallId};

pub const RECOVERY_VERSION: u16 = 1;
pub const MAX_RECOVERY_RUNS: usize = 32;
pub const MAX_RECOVERY_REFERENCES: usize = 64;
pub const MAX_RECOVERY_REQUESTS: usize = 128;
pub const MAX_RECOVERY_APPROVAL_EVENTS: usize = 128;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OwnershipOutcome {
    NoActiveOwner,
    Live,
    ProvenAbandoned,
    OwnershipUnknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HostGenerationState {
    Active,
    RecoveryPending,
    Recovered,
    Retired,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HostOwnerEvidence {
    pub version: u16,
    pub supported_environment: bool,
    pub mechanism: String,
    pub canonical_identity_method: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub store_id: Option<Uuid>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub physical_identity_sha256: Option<Sha256Digest>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub generation: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub owner_id: Option<Uuid>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub process_id: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub process_start_key: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub state: Option<HostGenerationState>,
    pub outcome: OwnershipOutcome,
    pub reason: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RecoveryRunKind {
    Task,
    Compaction,
    SupervisedAttempt,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RecoveryExecutionState {
    Active,
    Complete,
    Failed,
    Cancelled,
    Interrupted,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RecoveryRequestState {
    None,
    PreparedNotDispatched,
    Dispatched,
    PartialResponse,
    ResponseReceived,
    Settled,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RecoveryEffectState {
    None,
    Admitted,
    Settled,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RecoveryAccountingState {
    NotAssociated,
    Settled,
    ProvenUndispatched,
    Unresolved,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecoveryReference {
    pub kind: String,
    pub id: Uuid,
    pub state: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RunRecoveryRecord {
    pub version: u16,
    pub run_id: RunId,
    pub project_id: ProjectId,
    pub session_id: SessionId,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub task_id: Option<TaskId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub invocation_id: Option<Uuid>,
    pub owner_generation: u64,
    pub kind: RecoveryRunKind,
    pub original_lifecycle: String,
    pub execution_state: RecoveryExecutionState,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub request_id: Option<Uuid>,
    pub request_purpose: String,
    pub request_state: RecoveryRequestState,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub effect_call_id: Option<ToolCallId>,
    pub effect_kind: String,
    pub effect_containment: String,
    pub effect_state: RecoveryEffectState,
    pub known_effects: Vec<RecoveryReference>,
    pub unresolved_effects: Vec<String>,
    pub accounting_state: RecoveryAccountingState,
    pub authority_invalidated: bool,
    pub admission_blocked: bool,
    pub created_at: OffsetDateTime,
    pub updated_at: OffsetDateTime,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub recovered_at: Option<OffsetDateTime>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub recovery_revision: Option<Uuid>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub interruption_reason: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub owner_reviewed_at: Option<OffsetDateTime>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LegacyOwnershipAmbiguity {
    pub run_id: RunId,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project_id: Option<ProjectId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_id: Option<SessionId>,
    pub last_lifecycle: String,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LegacyWorkAmbiguity {
    pub kind: String,
    pub id: Uuid,
    pub goal_id: Uuid,
    pub project_id: ProjectId,
    pub session_id: SessionId,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub run_id: Option<RunId>,
    pub state: String,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecoveryInspection {
    pub version: u16,
    pub ownership: HostOwnerEvidence,
    pub runs: Vec<RunRecoveryRecord>,
    pub runs_truncated: bool,
    pub legacy_ambiguities: Vec<LegacyOwnershipAmbiguity>,
    pub legacy_ambiguities_truncated: bool,
    pub legacy_work_ambiguities: Vec<LegacyWorkAmbiguity>,
    pub legacy_work_ambiguities_truncated: bool,
    pub mutation_performed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecoverySettlement {
    pub version: u16,
    pub ownership: HostOwnerEvidence,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub recovery_revision: Option<Uuid>,
    pub recovered_run_ids: Vec<RunId>,
    pub recovered_invocation_ids: Vec<Uuid>,
    pub incomplete: bool,
    pub mutation_performed: bool,
}
