use argentum_domain::{AppCommand, AppEvent};
use serde::{Deserialize, Serialize};

pub const PROTOCOL_VERSION: u16 = 1;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RequestEnvelope {
    pub protocol_version: u16,
    pub request_id: String,
    #[serde(flatten)]
    pub payload: ClientPayload,
}

impl RequestEnvelope {
    pub fn command(request_id: impl Into<String>, command: AppCommand) -> Self {
        Self {
            protocol_version: PROTOCOL_VERSION,
            request_id: request_id.into(),
            payload: ClientPayload::Command { command },
        }
    }

    pub fn validate(&self) -> Result<(), ProtocolError> {
        if self.protocol_version != PROTOCOL_VERSION {
            return Err(ProtocolError::UnsupportedVersion {
                received: self.protocol_version,
                supported: PROTOCOL_VERSION,
            });
        }
        let request_id = self.request_id.trim();
        if request_id.is_empty() {
            return Err(ProtocolError::MissingRequestId);
        }
        if request_id.len() > 128 {
            return Err(ProtocolError::RequestIdTooLong);
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ClientPayload {
    Command { command: AppCommand },
    Ping,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResponseEnvelope {
    pub protocol_version: u16,
    pub sequence: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub request_id: Option<String>,
    #[serde(flatten)]
    pub payload: ServerPayload,
}

impl ResponseEnvelope {
    pub fn new(sequence: u64, request_id: Option<String>, payload: ServerPayload) -> Self {
        Self {
            protocol_version: PROTOCOL_VERSION,
            sequence,
            request_id,
            payload,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ServerPayload {
    Ready {
        workspace: String,
    },
    CommandAccepted,
    CommandCompleted,
    CommandFailed {
        code: String,
        message: String,
        recoverable: bool,
    },
    Pong,
    Event {
        event: AppEvent,
    },
    Error {
        code: String,
        message: String,
        recoverable: bool,
    },
}

impl ServerPayload {
    pub fn command_failed(
        code: impl Into<String>,
        message: impl Into<String>,
        recoverable: bool,
    ) -> Self {
        Self::CommandFailed {
            code: code.into(),
            message: message.into(),
            recoverable,
        }
    }

    pub fn error(code: impl Into<String>, message: impl Into<String>, recoverable: bool) -> Self {
        Self::Error {
            code: code.into(),
            message: message.into(),
            recoverable,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ProtocolError {
    #[error("unsupported protocol version {received}; this build supports version {supported}")]
    UnsupportedVersion { received: u16, supported: u16 },
    #[error("request_id must not be empty")]
    MissingRequestId,
    #[error("request_id must not exceed 128 bytes")]
    RequestIdTooLong,
}

#[cfg(test)]
mod tests {
    use argentum_domain::{
        AppCommand, ProviderKind, ProviderProfile, SurfaceId, ToolInput, ToolRequest,
    };

    use super::*;

    #[test]
    fn command_envelope_has_a_stable_versioned_shape() {
        let envelope = RequestEnvelope::command(
            "request-1",
            AppCommand::SubmitTask {
                prompt: "inspect the workspace".into(),
            },
        );

        let encoded = serde_json::to_value(&envelope).expect("serialized request");
        assert_eq!(encoded["protocol_version"], PROTOCOL_VERSION);
        assert_eq!(encoded["request_id"], "request-1");
        assert_eq!(encoded["type"], "command");
        assert_eq!(encoded["command"]["kind"], "submit_task");
        assert_eq!(
            serde_json::from_value::<RequestEnvelope>(encoded).expect("parsed request"),
            envelope
        );
    }

    #[test]
    fn tool_request_envelope_has_a_stable_typed_shape() {
        let envelope = RequestEnvelope::command(
            "tool-read-1",
            AppCommand::RequestTool {
                request: ToolRequest {
                    call_id: "00000000-0000-0000-0000-000000000001"
                        .parse()
                        .expect("call id"),
                    run_id: "00000000-0000-0000-0000-000000000002"
                        .parse()
                        .expect("run id"),
                    input: ToolInput::ReadText {
                        path: "README.md".into(),
                    },
                },
            },
        );

        let encoded = serde_json::to_value(&envelope).expect("serialized request");
        assert_eq!(encoded["protocol_version"], PROTOCOL_VERSION);
        assert_eq!(encoded["request_id"], "tool-read-1");
        assert_eq!(encoded["type"], "command");
        assert_eq!(encoded["command"]["kind"], "request_tool");
        assert_eq!(encoded["command"]["request"]["input"]["kind"], "read_text");
        assert_eq!(encoded["command"]["request"]["input"]["path"], "README.md");
        assert_eq!(
            serde_json::from_value::<RequestEnvelope>(encoded).expect("parsed request"),
            envelope
        );
    }

    #[test]
    fn provider_probe_is_an_additive_protocol_v1_command() {
        let envelope = RequestEnvelope::command(
            "provider-probe-1",
            AppCommand::ProbeProvider {
                provider_id: "lm-studio".into(),
            },
        );

        let encoded = serde_json::to_value(&envelope).expect("serialized request");
        assert_eq!(encoded["protocol_version"], PROTOCOL_VERSION);
        assert_eq!(encoded["command"]["kind"], "probe_provider");
        assert_eq!(encoded["command"]["provider_id"], "lm-studio");
        assert_eq!(
            serde_json::from_value::<RequestEnvelope>(encoded).expect("parsed request"),
            envelope
        );
    }

    #[test]
    fn provider_profile_commands_have_stable_additive_protocol_shapes() {
        let profile = ProviderProfile {
            id: "local-secondary".into(),
            label: "Secondary local".into(),
            kind: ProviderKind::LocalLmStudio,
            endpoint: "http://127.0.0.1:5678/v1/".into(),
            model: "secondary-model".into(),
            selected: false,
        };
        let cases = [
            (
                AppCommand::ListProviderProfiles,
                "list_provider_profiles",
                None,
            ),
            (
                AppCommand::SaveProviderProfile {
                    profile: profile.clone(),
                },
                "save_provider_profile",
                Some("local-secondary"),
            ),
            (
                AppCommand::SelectProviderProfile {
                    provider_id: profile.id.clone(),
                },
                "select_provider_profile",
                Some("local-secondary"),
            ),
        ];

        for (index, (command, expected_kind, expected_id)) in cases.into_iter().enumerate() {
            let envelope = RequestEnvelope::command(format!("provider-{index}"), command);
            let encoded = serde_json::to_value(&envelope).expect("serialized request");
            assert_eq!(encoded["protocol_version"], PROTOCOL_VERSION);
            assert_eq!(encoded["command"]["kind"], expected_kind);
            if let Some(expected_id) = expected_id {
                let encoded_id = encoded["command"]
                    .get("provider_id")
                    .or_else(|| encoded["command"]["profile"].get("id"))
                    .expect("provider ID");
                assert_eq!(encoded_id, expected_id);
            }
            serde_json::from_value::<RequestEnvelope>(encoded).expect("parsed request");
        }
    }

    #[test]
    fn provider_model_commands_have_stable_additive_protocol_v1_shapes() {
        let cases = [
            AppCommand::ListProviderModels {
                provider_id: "deepseek".into(),
            },
            AppCommand::SelectProviderModel {
                provider_id: "deepseek".into(),
                model: "deepseek-chat".into(),
            },
        ];

        for (index, command) in cases.into_iter().enumerate() {
            let envelope = RequestEnvelope::command(format!("provider-model-{index}"), command);
            let encoded = serde_json::to_value(&envelope).expect("serialized request");
            assert_eq!(encoded["protocol_version"], PROTOCOL_VERSION);
            assert_eq!(encoded["command"]["provider_id"], "deepseek");
            if index == 0 {
                assert_eq!(encoded["command"]["kind"], "list_provider_models");
                assert!(encoded["command"].get("model").is_none());
            } else {
                assert_eq!(encoded["command"]["kind"], "select_provider_model");
                assert_eq!(encoded["command"]["model"], "deepseek-chat");
            }
            assert_eq!(
                serde_json::from_value::<RequestEnvelope>(encoded).expect("parsed request"),
                envelope
            );
        }
    }

    #[test]
    fn session_selection_is_an_additive_protocol_v1_command() {
        let session_id = "00000000-0000-0000-0000-000000000091"
            .parse()
            .expect("session id");
        let envelope =
            RequestEnvelope::command("session-select-1", AppCommand::SelectSession { session_id });

        let encoded = serde_json::to_value(&envelope).expect("serialized request");

        assert_eq!(encoded["protocol_version"], PROTOCOL_VERSION);
        assert_eq!(encoded["command"]["kind"], "select_session");
        assert_eq!(encoded["command"]["session_id"], session_id.to_string());
    }

    #[test]
    fn harness_commands_have_stable_additive_protocol_v1_shapes() {
        let cases = [
            (AppCommand::ListHarnessState, "list_harness_state"),
            (
                AppCommand::SelectHarnessProfile {
                    profile_id: "review".into(),
                },
                "select_harness_profile",
            ),
            (
                AppCommand::SetSurfaceVisibility {
                    surface: SurfaceId::Activity,
                    visible: true,
                },
                "set_surface_visibility",
            ),
            (
                AppCommand::SelectExecutionProfile {
                    profile_id: "read-only".into(),
                },
                "select_execution_profile",
            ),
            (
                AppCommand::SetHarnessCapabilityEnabled {
                    capability_id: "tool.write-text".into(),
                    enabled: false,
                },
                "set_harness_capability_enabled",
            ),
            (
                AppCommand::LoadTrajectory { session_id: None },
                "load_trajectory",
            ),
        ];

        for (index, (command, expected_kind)) in cases.into_iter().enumerate() {
            let envelope = RequestEnvelope::command(format!("harness-{index}"), command);
            let encoded = serde_json::to_value(&envelope).expect("serialized request");
            assert_eq!(encoded["protocol_version"], PROTOCOL_VERSION);
            assert_eq!(encoded["command"]["kind"], expected_kind);
            assert_eq!(
                serde_json::from_value::<RequestEnvelope>(encoded).expect("parsed request"),
                envelope
            );
        }
    }

    #[test]
    fn rejects_unknown_protocol_versions_and_unusable_request_ids() {
        let mut envelope = RequestEnvelope::command("request-1", AppCommand::NewSession);
        envelope.protocol_version = PROTOCOL_VERSION + 1;
        assert!(matches!(
            envelope.validate(),
            Err(ProtocolError::UnsupportedVersion { .. })
        ));

        envelope.protocol_version = PROTOCOL_VERSION;
        envelope.request_id = "  ".into();
        assert_eq!(envelope.validate(), Err(ProtocolError::MissingRequestId));
    }
}
