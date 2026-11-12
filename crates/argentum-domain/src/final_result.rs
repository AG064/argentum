use crate::{
    FileChangeId, FileChangeKind, FileChangeOrigin, ModelUsage, ProjectId, RunId, SessionId,
    Sha256Digest, TaskId, TaskLifecycle, VerificationId, VerificationRunnerId, VerificationState,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

pub const FINAL_RESULT_SCHEMA_VERSION: u16 = 1;
pub const FINAL_RESULT_MAX_BYTES: usize = 512 * 1024;
pub const FINAL_RESULT_MAX_ANSWER_BYTES: usize = 128 * 1024;
pub const FINAL_RESULT_MAX_REFERENCE_ITEMS: usize = 128;
pub const FINAL_RESULT_MAX_USAGE_ITEMS: usize = 64;
pub const FINAL_RESULT_MAX_EVENT_RECORDS: usize = 512;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AnswerContractSource {
    pub relative_path: String,
    pub sha256: Sha256Digest,
    pub dialect: String,
    pub subset_version: u16,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AnswerContractStatus {
    Valid,
    Invalid,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AnswerContractEvaluation {
    pub source: AnswerContractSource,
    pub validator_version: u16,
    pub status: AnswerContractStatus,
    pub error_count: u16,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub error_codes: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parsed_value_sha256: Option<Sha256Digest>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FinalAnswerContract {
    pub source: AnswerContractSource,
    pub validator_version: u16,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<AnswerContractStatus>,
    #[serde(default)]
    pub error_count: u16,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub error_codes: Vec<String>,
    pub parsed_value_available: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parsed_model_value: Option<serde_json::Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unavailable_reason: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FinalRunState {
    NotFinal,
    Complete,
    Failed,
    Cancelled,
    Interrupted,
}

impl FinalRunState {
    pub const fn is_final(self) -> bool {
        !matches!(self, Self::NotFinal)
    }

    pub const fn from_lifecycle(lifecycle: TaskLifecycle) -> Option<Self> {
        match lifecycle {
            TaskLifecycle::Complete => Some(Self::Complete),
            TaskLifecycle::Failed => Some(Self::Failed),
            TaskLifecycle::Cancelled => Some(Self::Cancelled),
            TaskLifecycle::Interrupted => Some(Self::Interrupted),
            TaskLifecycle::Draft
            | TaskLifecycle::Queued
            | TaskLifecycle::Planning
            | TaskLifecycle::Running
            | TaskLifecycle::WaitingForApproval
            | TaskLifecycle::Reviewing
            | TaskLifecycle::Verifying
            | TaskLifecycle::Paused => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct FinalRunCorrelation {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project_id: Option<ProjectId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_id: Option<SessionId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub task_id: Option<TaskId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub run_id: Option<RunId>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FinalAnswer {
    pub authored_by: String,
    pub available: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    pub truncated: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unavailable_reason: Option<String>,
}

impl FinalAnswer {
    pub fn unavailable(reason: impl Into<String>) -> Self {
        Self {
            authored_by: "model".into(),
            available: false,
            text: None,
            truncated: false,
            unavailable_reason: Some(reason.into()),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FinalFailureKind {
    RunError,
    ApprovalRequired,
    Cancelled,
    Interrupted,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FinalFailure {
    pub kind: FinalFailureKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub recoverable: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FinalChangeReference {
    pub id: FileChangeId,
    pub change_set_id: Uuid,
    pub run_id: RunId,
    pub revision: u64,
    pub relative_path: String,
    pub kind: FileChangeKind,
    pub origin: FileChangeOrigin,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FinalVerificationReference {
    pub id: VerificationId,
    pub run_id: RunId,
    pub runner: VerificationRunnerId,
    pub state: VerificationState,
    pub revision_before: Sha256Digest,
    pub revision_after: Sha256Digest,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub definition_sha256: Option<Sha256Digest>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FinalUsageReference {
    pub profile_id: String,
    pub model: String,
    pub usage: ModelUsage,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FinalAccountingReference {
    pub request_id: Uuid,
    pub purpose: String,
    pub method: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub accounted_input: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub observed_total_tokens: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub charge_state: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FinalReferenceSet<T> {
    pub available: bool,
    pub complete: bool,
    pub items: Vec<T>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unavailable_reason: Option<String>,
}

impl<T> FinalReferenceSet<T> {
    pub fn unavailable(reason: impl Into<String>) -> Self {
        Self {
            available: false,
            complete: false,
            items: Vec::new(),
            unavailable_reason: Some(reason.into()),
        }
    }

    pub fn available(items: Vec<T>, complete: bool) -> Self {
        Self {
            available: true,
            complete,
            items,
            unavailable_reason: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FinalUsage {
    pub reported: FinalReferenceSet<FinalUsageReference>,
    pub client_accounting: FinalReferenceSet<FinalAccountingReference>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FinalResultCompleteness {
    pub event_history_complete: bool,
    pub conversation_answer_complete: bool,
    pub bounded_references_complete: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FinalRunResult {
    pub schema_version: u16,
    #[serde(rename = "final")]
    pub is_final: bool,
    pub run_state: FinalRunState,
    pub correlation: FinalRunCorrelation,
    pub answer: FinalAnswer,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub answer_contract: Option<FinalAnswerContract>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub failure: Option<FinalFailure>,
    pub changes: FinalReferenceSet<FinalChangeReference>,
    pub verification: FinalReferenceSet<FinalVerificationReference>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub supplemental_checks: Vec<crate::supplemental::SupplementalCheckReceipt>,
    pub usage: FinalUsage,
    pub completeness: FinalResultCompleteness,
}

impl FinalRunResult {
    pub fn unavailable_run(run_id: RunId, reason: impl Into<String>) -> Self {
        Self {
            schema_version: FINAL_RESULT_SCHEMA_VERSION,
            is_final: false,
            run_state: FinalRunState::NotFinal,
            correlation: FinalRunCorrelation {
                run_id: Some(run_id),
                ..FinalRunCorrelation::default()
            },
            answer: FinalAnswer::unavailable(reason),
            answer_contract: None,
            failure: None,
            changes: FinalReferenceSet::unavailable("run_not_settled"),
            verification: FinalReferenceSet::unavailable("run_not_settled"),
            supplemental_checks: Vec::new(),
            usage: FinalUsage {
                reported: FinalReferenceSet::unavailable("run_not_settled"),
                client_accounting: FinalReferenceSet::unavailable("run_not_settled"),
            },
            completeness: FinalResultCompleteness {
                event_history_complete: false,
                conversation_answer_complete: false,
                bounded_references_complete: false,
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unavailable_result_is_versioned_and_has_no_fabricated_answer() {
        let run_id = RunId::new_v4();
        let result = FinalRunResult::unavailable_run(run_id, "still_running");
        let value = serde_json::to_value(&result).expect("serializable result");

        assert_eq!(value["schema_version"], FINAL_RESULT_SCHEMA_VERSION);
        assert_eq!(value["final"], false);
        assert_eq!(value["run_state"], "not_final");
        assert_eq!(value["correlation"]["run_id"], run_id.to_string());
        assert_eq!(value["answer"]["available"], false);
        assert!(value["answer"].get("text").is_none());
    }

    #[test]
    fn model_text_serialization_preserves_utf8_and_escapes_controls() {
        let answer = FinalAnswer {
            authored_by: "model".into(),
            available: true,
            text: Some("line one\nline two\u{0000} utf8: ž".into()),
            truncated: false,
            unavailable_reason: None,
        };
        let encoded = serde_json::to_string(&answer).expect("serializable answer");

        assert!(encoded.contains("line one\\nline two\\u0000"));
        assert!(encoded.contains("ž"));
        assert!(!encoded.contains('\n'));
    }

    #[test]
    fn final_result_limits_are_bounded_and_explicit() {
        assert_eq!(FINAL_RESULT_SCHEMA_VERSION, 1);
        assert_eq!(FINAL_RESULT_MAX_BYTES, 512 * 1024);
        assert_eq!(FINAL_RESULT_MAX_ANSWER_BYTES, 128 * 1024);
        assert_eq!(FINAL_RESULT_MAX_REFERENCE_ITEMS, 128);
        assert_eq!(FINAL_RESULT_MAX_USAGE_ITEMS, 64);
        assert_eq!(FINAL_RESULT_MAX_EVENT_RECORDS, 512);
    }
}
