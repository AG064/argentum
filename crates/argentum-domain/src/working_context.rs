//! Data only. These records cannot confer execution authority.
use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

use crate::{AuditId, ExecutionOutcome, ProjectId, RunId, SessionId, TaskId, ToolCallId};

pub const PROJECTION_VERSION: u16 = 1;
pub const MAX_RECORD_BYTES: usize = 128 * 1024;
pub const MAX_RESULT_BYTES: usize = crate::text_contract::MAX_TEXT_RESULT_BYTES;
pub const MAX_GROUP_BYTES: usize = 512 * 1024;
pub const MAX_HISTORY_BYTES: usize = 512 * 1024;
pub const MAX_HISTORY_MESSAGES: usize = 128;
pub const MAX_GROUP_CALLS: usize = 8;
pub const MAX_SOURCE_GROUPS: usize = 128;
pub const MAX_RUN_GROUPS: u32 = 33;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Retention {
    Retained,
    Redacted,
    MetadataOnly,
    OmittedPolicy,
    OmittedLimit,
    Unavailable,
}

impl Retention {
    pub const fn label(self) -> &'static str {
        match self {
            Self::Retained => "retained",
            Self::Redacted => "redacted",
            Self::MetadataOnly => "metadata_only",
            Self::OmittedPolicy => "omitted_policy",
            Self::OmittedLimit => "omitted_limit",
            Self::Unavailable => "unavailable",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ObservationState {
    ResultAvailable,
    Rejected,
    Deferred,
    NotAttempted,
    Unresolved,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ObservationReceipt {
    pub audit_id: AuditId,
    pub outcome: ExecutionOutcome,
    pub exit_code: Option<i32>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ObservedChange {
    pub file_change_id: crate::FileChangeId,
    pub change_set_id: crate::ChangeSetId,
    pub revision: u64,
    pub before_sha256: Option<crate::Sha256Digest>,
    pub after_sha256: crate::Sha256Digest,
}

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Observation {
    pub provider_call_id: String,
    pub runtime_call_id: Option<ToolCallId>,
    pub tool: String,
    pub state: ObservationState,
    pub receipt: Option<ObservationReceipt>,
    pub change: Option<ObservedChange>,
    /// Allowlisted historical arguments, never approval material.
    pub arguments: String,
    pub retention: Retention,
    /// Complete observed result, or None. A placeholder is created at projection.
    pub result: Option<String>,
}

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum GroupBody {
    User {
        text: String,
    },
    Steering {
        entry_id: crate::SteeringEntryId,
        position: u64,
        text: String,
    },
    Assistant {
        text: String,
    },
    AssistantOmitted {
        retention: Retention,
    },
    Exchange {
        text: String,
        text_retention: Retention,
        observations: Vec<Observation>,
    },
}

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ContextGroup {
    pub version: u16,
    pub project_id: ProjectId,
    pub session_id: SessionId,
    pub task_id: TaskId,
    pub run_id: RunId,
    pub ordinal: u32,
    pub body: GroupBody,
}

impl std::fmt::Debug for ContextGroup {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let (kind, text_bytes, observations) = match &self.body {
            GroupBody::User { text } => ("user", text.len(), 0),
            GroupBody::Steering { text, .. } => ("steering", text.len(), 0),
            GroupBody::Assistant { text } => ("assistant", text.len(), 0),
            GroupBody::AssistantOmitted { .. } => ("assistant_omitted", 0, 0),
            GroupBody::Exchange {
                text, observations, ..
            } => ("exchange", text.len(), observations.len()),
        };
        formatter
            .debug_struct("ContextGroup")
            .field("project_id", &self.project_id)
            .field("session_id", &self.session_id)
            .field("run_id", &self.run_id)
            .field("ordinal", &self.ordinal)
            .field("kind", &kind)
            .field("text_bytes", &text_bytes)
            .field("observations", &observations)
            .finish_non_exhaustive()
    }
}

impl ContextGroup {
    pub fn message_count(&self) -> usize {
        match &self.body {
            GroupBody::Exchange { observations, .. } => 1 + observations.len(),
            _ => 1,
        }
    }

    /// Validate untrusted storage before it can become protocol messages.
    pub fn validate(&self) -> Result<(), &'static str> {
        if self.version != PROJECTION_VERSION
            || self.project_id.is_nil()
            || self.session_id.is_nil()
            || self.task_id.is_nil()
            || self.run_id.is_nil()
            || self.ordinal >= MAX_RUN_GROUPS
        {
            return Err("unsupported context version or invalid scope");
        }
        if self.ordinal == 0 && !matches!(self.body, GroupBody::User { .. }) {
            return Err("context must start with its user source");
        }
        let text = match &self.body {
            GroupBody::User { text } => {
                if self.ordinal != 0 {
                    return Err("user context must start a run");
                }
                text
            }
            GroupBody::Steering {
                entry_id,
                position,
                text,
            } => {
                if self.ordinal == 0 || entry_id.is_nil() || *position == 0 {
                    return Err("invalid steering context lineage");
                }
                text
            }
            GroupBody::Assistant { text } => text,
            GroupBody::AssistantOmitted { retention } => {
                if !matches!(
                    retention,
                    Retention::OmittedLimit | Retention::OmittedPolicy
                ) {
                    return Err("invalid assistant omission");
                }
                return Ok(());
            }
            GroupBody::Exchange {
                text,
                text_retention,
                observations,
            } => {
                if !matches!(
                    text_retention,
                    Retention::Retained | Retention::OmittedLimit | Retention::OmittedPolicy
                ) || (*text_retention != Retention::Retained && !text.is_empty())
                {
                    return Err("invalid assistant exchange retention");
                }
                if observations.is_empty() || observations.len() > MAX_GROUP_CALLS {
                    return Err("invalid context exchange size");
                }
                let mut ids = BTreeSet::new();
                for observation in observations {
                    if observation.provider_call_id.is_empty()
                        || observation.provider_call_id.len() > 128
                        || observation.provider_call_id.chars().any(char::is_control)
                        || !ids.insert(&observation.provider_call_id)
                        || observation.runtime_call_id.is_some_and(|id| id.is_nil())
                        || observation.tool.is_empty()
                        || observation.tool.len() > 64
                        || !observation
                            .tool
                            .bytes()
                            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
                        || observation.arguments.len() > 16 * 1024
                        || !serde_json::from_str::<serde_json::Value>(&observation.arguments)
                            .is_ok_and(|value| value.is_object())
                        || observation
                            .result
                            .as_ref()
                            .is_some_and(|body| body.len() > MAX_RESULT_BYTES)
                    {
                        return Err("invalid historical observation");
                    }
                    let has_body = matches!(
                        observation.retention,
                        Retention::Retained | Retention::Redacted
                    );
                    if has_body != observation.result.is_some() {
                        return Err("historical retention disposition does not match body");
                    }
                    if (has_body && observation.state != ObservationState::ResultAvailable)
                        || observation
                            .receipt
                            .as_ref()
                            .is_some_and(|receipt| receipt.audit_id.is_nil())
                        || (observation.state == ObservationState::NotAttempted
                            && (observation.receipt.is_some()
                                || observation.runtime_call_id.is_some()))
                    {
                        return Err("invalid historical outcome association");
                    }
                    if let Some(change) = &observation.change {
                        if observation.tool != "edit_text"
                            || change.file_change_id.is_nil()
                            || change.change_set_id.is_nil()
                            || change.revision == 0
                            || !valid_digest(&change.after_sha256.0)
                            || change
                                .before_sha256
                                .as_ref()
                                .is_some_and(|digest| !valid_digest(&digest.0))
                        {
                            return Err("invalid historical change reference");
                        }
                    }
                }
                text
            }
        };
        if text.len() > MAX_RECORD_BYTES {
            return Err("context record exceeds byte bound");
        }
        Ok(())
    }
}

fn valid_digest(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    fn group() -> ContextGroup {
        ContextGroup {
            version: PROJECTION_VERSION,
            project_id: Uuid::new_v4(),
            session_id: Uuid::new_v4(),
            task_id: Uuid::new_v4(),
            run_id: Uuid::new_v4(),
            ordinal: 1,
            body: GroupBody::Exchange {
                text: String::new(),
                text_retention: Retention::Retained,
                observations: vec![Observation {
                    provider_call_id: "call-1".into(),
                    runtime_call_id: Some(Uuid::new_v4()),
                    tool: "read_text".into(),
                    arguments: "{}".into(),
                    state: ObservationState::ResultAvailable,
                    receipt: None,
                    change: None,
                    retention: Retention::Retained,
                    result: Some("observed".into()),
                }],
            },
        }
    }

    #[test]
    fn completed_observation_requires_matching_retention_and_unique_lineage() {
        let mut value = group();
        assert_eq!(value.validate(), Ok(()));
        let GroupBody::Exchange { observations, .. } = &mut value.body else {
            unreachable!()
        };
        observations[0].retention = Retention::MetadataOnly;
        assert!(value.validate().is_err());
        let GroupBody::Exchange { observations, .. } = &mut value.body else {
            unreachable!()
        };
        observations[0].result = None;
        assert_eq!(value.validate(), Ok(()));
        let GroupBody::Exchange { observations, .. } = &mut value.body else {
            unreachable!()
        };
        observations.push(observations[0].clone());
        assert!(value.validate().is_err());
    }

    #[test]
    fn context_privacy_debug_hides_observation_bodies() {
        let mut value = group();
        let GroupBody::Exchange { observations, .. } = &mut value.body else {
            unreachable!()
        };
        observations[0].result = Some("PRIVATE_DIAGNOSTIC_CANARY".into());
        assert!(!format!("{value:?}").contains("PRIVATE_DIAGNOSTIC_CANARY"));
    }

    #[test]
    fn byte_bounds_count_multibyte_text_without_slicing() {
        let mut value = group();
        value.body = GroupBody::Assistant {
            text: "é".repeat(MAX_RECORD_BYTES / 2),
        };
        assert_eq!(value.validate(), Ok(()));
        let GroupBody::Assistant { text } = &mut value.body else {
            unreachable!()
        };
        text.push('é');
        assert!(value.validate().is_err());
    }
}
