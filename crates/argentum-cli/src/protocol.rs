use argentum_domain::{AppCommand, AppEvent};
use serde::{Deserialize, Serialize};
use serde_json::Value;

pub const PROTOCOL_VERSION: u16 = 1;
pub const COMMAND_CANONICALIZATION_VERSION: u16 = argentum_store::COMMAND_IDENTITY_VERSION;

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

    pub fn inspect_command(
        request_id: impl Into<String>,
        command_request_id: impl Into<String>,
    ) -> Self {
        Self {
            protocol_version: PROTOCOL_VERSION,
            request_id: request_id.into(),
            payload: ClientPayload::InspectCommand {
                command_request_id: command_request_id.into(),
            },
        }
    }

    pub fn validate(&self) -> Result<(), ProtocolError> {
        if self.protocol_version != PROTOCOL_VERSION {
            return Err(ProtocolError::UnsupportedVersion {
                received: self.protocol_version,
                supported: PROTOCOL_VERSION,
            });
        }
        if self.request_id.trim().is_empty() {
            return Err(ProtocolError::MissingRequestId);
        }
        if self.request_id.len() > argentum_store::MAX_COMMAND_REQUEST_ID_BYTES {
            return Err(ProtocolError::RequestIdTooLong);
        }
        if self.request_id.trim() != self.request_id
            || self.request_id.chars().any(char::is_control)
        {
            return Err(ProtocolError::MalformedRequestId);
        }
        if let ClientPayload::InspectCommand { command_request_id } = &self.payload {
            if command_request_id.is_empty()
                || command_request_id.len() > argentum_store::MAX_COMMAND_REQUEST_ID_BYTES
                || command_request_id.trim() != command_request_id
                || command_request_id.chars().any(char::is_control)
            {
                return Err(ProtocolError::MalformedInspectedRequestId);
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ClientPayload {
    Command { command: AppCommand },
    InspectCommand { command_request_id: String },
    Ping,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResponseEnvelope {
    pub protocol_version: u16,
    pub sequence: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub request_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub delivery: Option<CommandDelivery>,
    #[serde(flatten)]
    pub payload: ServerPayload,
}

impl ResponseEnvelope {
    pub fn new(sequence: u64, request_id: Option<String>, payload: ServerPayload) -> Self {
        Self {
            protocol_version: PROTOCOL_VERSION,
            sequence,
            request_id,
            delivery: None,
            payload,
        }
    }

    pub fn with_delivery(mut self, delivery: CommandDelivery) -> Self {
        self.delivery = Some(delivery);
        self
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CommandDeliveryDisposition {
    Original,
    InProgress,
    Replayed,
    Interrupted,
    Unknown,
    Conflict,
    ResultUnavailable,
    Restricted,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommandIdentityMetadata {
    pub identity_version: u16,
    pub canonicalization_version: u16,
    pub command_kind: String,
    pub state: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub outcome_kind: Option<String>,
    pub payload_available: bool,
    pub replay_eligible: bool,
    pub accepted_at_unix: i64,
    pub updated_at_unix: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_id: Option<argentum_domain::SessionId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub run_id: Option<argentum_domain::RunId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub task_id: Option<argentum_domain::TaskId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub subject_id: Option<argentum_domain::RunId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider_profile_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub execution_snapshot_sha256: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub definition_revision: Option<argentum_domain::GoalId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub result_session_id: Option<argentum_domain::SessionId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub recovery_revision: Option<argentum_domain::RunId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub recovery_blocker: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommandDelivery {
    pub disposition: CommandDeliveryDisposition,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub command: Option<CommandIdentityMetadata>,
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
        #[serde(default, skip_serializing_if = "Option::is_none")]
        run_id: Option<argentum_domain::RunId>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        tool_call_id: Option<argentum_domain::ToolCallId>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        approval_id: Option<argentum_domain::ApprovalId>,
    },
    Pong,
    CommandIdentityStatus {
        command: CommandIdentityMetadata,
    },
    Event {
        event: Box<AppEvent>,
    },
    Error {
        code: String,
        message: String,
        recoverable: bool,
    },
}

impl ServerPayload {
    pub fn event(event: AppEvent) -> Self {
        Self::Event {
            event: Box::new(event),
        }
    }

    pub fn command_failed(
        code: impl Into<String>,
        message: impl Into<String>,
        recoverable: bool,
    ) -> Self {
        Self::CommandFailed {
            code: code.into(),
            message: message.into(),
            recoverable,
            run_id: None,
            tool_call_id: None,
            approval_id: None,
        }
    }

    pub fn host_command_failed(error: &crate::HostError) -> Self {
        if let crate::HostError::Runtime(runtime_error) = error {
            let code = match runtime_error {
                argentum_runtime::RuntimeError::GoalDeadline => Some("goal_deadline_exceeded"),
                argentum_runtime::RuntimeError::GoalUsageUnresolved
                | argentum_runtime::RuntimeError::ProviderUsageUnresolved { .. } => {
                    Some("goal_usage_unresolved")
                }
                argentum_runtime::RuntimeError::GoalUsageOverrun => Some("goal_usage_overrun"),
                argentum_runtime::RuntimeError::GoalAccountingUnresolved => {
                    Some("goal_accounting_unresolved")
                }
                argentum_runtime::RuntimeError::Store(
                    argentum_store::StoreError::GoalAccounting(code),
                ) => Some(*code),
                argentum_runtime::RuntimeError::ContextBusy => Some("context_busy"),
                argentum_runtime::RuntimeError::Provider(
                    argentum_providers::ProviderError::ContextBudgetInvalid(_),
                ) => Some("context_budget_invalid"),
                argentum_runtime::RuntimeError::Provider(
                    argentum_providers::ProviderError::ContextAccountingUnsupported(_),
                ) => Some("context_accounting_unsupported"),
                argentum_runtime::RuntimeError::ContextCompaction(_)
                | argentum_runtime::RuntimeError::AutomaticCompactionFailed(_) => {
                    Some("context_compaction_failed")
                }
                argentum_runtime::RuntimeError::ContextBudget(_) => Some("context_budget_invalid"),
                argentum_runtime::RuntimeError::ContextTokenCapacity => {
                    Some("context_token_capacity_exceeded")
                }
                argentum_runtime::RuntimeError::ContextCapacity => {
                    Some("context_capacity_exceeded")
                }
                argentum_runtime::RuntimeError::ContextRecording => {
                    Some("context_recording_failed")
                }
                argentum_runtime::RuntimeError::DurableCommandSnapshotChanged => {
                    Some("durable_command_snapshot_changed")
                }
                _ => None,
            };
            if let Some(code) = code {
                return Self::command_failed(code, error.to_string(), false);
            }
        }
        if matches!(
            error,
            crate::HostError::Runtime(argentum_runtime::RuntimeError::Instructions(_))
        ) {
            return Self::command_failed("instruction_loading_failed", error.to_string(), false);
        }
        let classified = match error {
            crate::HostError::Runtime(argentum_runtime::RuntimeError::ApprovalRequired(
                context,
            )) => Some(("approval_required", context)),
            crate::HostError::Runtime(argentum_runtime::RuntimeError::ApprovalDenied(context)) => {
                Some(("approval_denied", context))
            }
            _ => None,
        };
        if let Some((code, context)) = classified {
            Self::CommandFailed {
                code: code.into(),
                message: error.to_string(),
                recoverable: false,
                run_id: Some(context.run_id),
                tool_call_id: context.tool_call_id,
                approval_id: Some(context.approval_id),
            }
        } else {
            Self::command_failed("command_failed", error.to_string(), true)
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
    #[error("request_id must not contain surrounding whitespace or control characters")]
    MalformedRequestId,
    #[error("command_request_id is malformed")]
    MalformedInspectedRequestId,
    #[error("canonical command input exceeds the size limit")]
    CanonicalCommandTooLarge,
}

pub fn parse_request(bytes: &[u8]) -> Result<RequestEnvelope, String> {
    let unique: UniqueJson = serde_json::from_slice(bytes).map_err(|error| {
        if error.to_string().contains("duplicate JSON object member") {
            "duplicate JSON object member".to_owned()
        } else {
            "malformed JSON request".to_owned()
        }
    })?;
    let Value::Object(fields) = &unique.0 else {
        return Err("request must be a JSON object".into());
    };
    let payload_type = fields
        .get("type")
        .and_then(Value::as_str)
        .ok_or_else(|| "request type must be a string".to_owned())?;
    let allowed: &[&str] = match payload_type {
        "command" => &["protocol_version", "request_id", "type", "command"],
        "inspect_command" => &[
            "protocol_version",
            "request_id",
            "type",
            "command_request_id",
        ],
        "ping" => &["protocol_version", "request_id", "type"],
        _ => return Err("request type is unsupported".into()),
    };
    if fields
        .keys()
        .any(|field| !allowed.contains(&field.as_str()))
    {
        return Err("request contains an unknown field".into());
    }
    serde_json::from_value(unique.0)
        .map_err(|_| "request does not match the supported typed shape".to_owned())
}

pub fn canonical_command(command: &AppCommand) -> Result<Vec<u8>, ProtocolError> {
    let value =
        serde_json::to_value(command).map_err(|_| ProtocolError::CanonicalCommandTooLarge)?;
    let mut output = Vec::new();
    write_canonical_value(&value, &mut output);
    if output.len() > argentum_store::MAX_COMMAND_CANONICAL_BYTES {
        return Err(ProtocolError::CanonicalCommandTooLarge);
    }
    Ok(output)
}

fn write_canonical_value(value: &Value, output: &mut Vec<u8>) {
    match value {
        Value::Object(fields) => {
            output.push(b'{');
            let mut keys = fields.keys().collect::<Vec<_>>();
            keys.sort_unstable();
            for (index, key) in keys.into_iter().enumerate() {
                if index > 0 {
                    output.push(b',');
                }
                output.extend_from_slice(
                    serde_json::to_string(key)
                        .expect("JSON object key serialization is infallible")
                        .as_bytes(),
                );
                output.push(b':');
                write_canonical_value(&fields[key], output);
            }
            output.push(b'}');
        }
        Value::Array(values) => {
            output.push(b'[');
            for (index, value) in values.iter().enumerate() {
                if index > 0 {
                    output.push(b',');
                }
                write_canonical_value(value, output);
            }
            output.push(b']');
        }
        _ => output.extend_from_slice(
            serde_json::to_string(value)
                .expect("typed JSON scalar serialization is infallible")
                .as_bytes(),
        ),
    }
}

struct UniqueJson(Value);

impl<'de> Deserialize<'de> for UniqueJson {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        deserializer.deserialize_any(UniqueJsonVisitor).map(Self)
    }
}

struct UniqueJsonVisitor;

impl<'de> serde::de::Visitor<'de> for UniqueJsonVisitor {
    type Value = Value;

    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("a JSON value without duplicate object members")
    }

    fn visit_bool<E>(self, value: bool) -> Result<Self::Value, E> {
        Ok(Value::Bool(value))
    }

    fn visit_i64<E>(self, value: i64) -> Result<Self::Value, E> {
        Ok(Value::Number(value.into()))
    }

    fn visit_u64<E>(self, value: u64) -> Result<Self::Value, E> {
        Ok(Value::Number(value.into()))
    }

    fn visit_f64<E>(self, value: f64) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        serde_json::Number::from_f64(value)
            .map(Value::Number)
            .ok_or_else(|| E::custom("invalid JSON number"))
    }

    fn visit_str<E>(self, value: &str) -> Result<Self::Value, E> {
        Ok(Value::String(value.to_owned()))
    }

    fn visit_string<E>(self, value: String) -> Result<Self::Value, E> {
        Ok(Value::String(value))
    }

    fn visit_none<E>(self) -> Result<Self::Value, E> {
        Ok(Value::Null)
    }

    fn visit_unit<E>(self) -> Result<Self::Value, E> {
        Ok(Value::Null)
    }

    fn visit_some<D>(self, deserializer: D) -> Result<Self::Value, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        UniqueJson::deserialize(deserializer).map(|value| value.0)
    }

    fn visit_seq<A>(self, mut sequence: A) -> Result<Self::Value, A::Error>
    where
        A: serde::de::SeqAccess<'de>,
    {
        let mut values = Vec::new();
        while let Some(value) = sequence.next_element::<UniqueJson>()? {
            values.push(value.0);
        }
        Ok(Value::Array(values))
    }

    fn visit_map<A>(self, mut fields: A) -> Result<Self::Value, A::Error>
    where
        A: serde::de::MapAccess<'de>,
    {
        let mut output = serde_json::Map::new();
        while let Some(key) = fields.next_key::<String>()? {
            if output.contains_key(&key) {
                return Err(serde::de::Error::custom("duplicate JSON object member"));
            }
            let value = fields.next_value::<UniqueJson>()?;
            output.insert(key, value.0);
        }
        Ok(Value::Object(output))
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn v1_command_failure_without_new_correlation_fields_remains_readable() {
        let old = r#"{"protocol_version":1,"sequence":1,"request_id":"old","type":"command_failed","code":"command_failed","message":"failed","recoverable":true}"#;
        let parsed: super::ResponseEnvelope = serde_json::from_str(old).unwrap();
        assert!(matches!(
            parsed.payload,
            super::ServerPayload::CommandFailed {
                run_id: None,
                tool_call_id: None,
                approval_id: None,
                ..
            }
        ));
        assert_eq!(
            serde_json::to_value(parsed).unwrap(),
            serde_json::from_str::<serde_json::Value>(old).unwrap()
        );
    }

    use argentum_domain::{
        AppCommand, AppEvent, PlatformAuthoritySnapshot, PlatformAuthorityState,
        PlatformAuthorityTier, PlatformCapabilityProbeSnapshot, ProviderKind, ProviderProfile,
        SurfaceId, ToolInput, ToolRequest,
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
    fn explicit_skill_command_has_a_stable_versioned_shape() {
        let envelope = RequestEnvelope::command(
            "request-skill-1",
            AppCommand::SubmitTaskWithSkill {
                prompt: "inspect the workspace".into(),
                skill: "rust-fix-and-verify".into(),
            },
        );
        let encoded = serde_json::to_value(&envelope).expect("serialized request");
        assert_eq!(encoded["command"]["kind"], "submit_task_with_skill");
        assert_eq!(encoded["command"]["skill"], "rust-fix-and-verify");
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
                        start_byte: Some(0),
                        end_byte: None,
                        start_line: None,
                        line_count: None,
                        expected_sha256: None,
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
    fn reviewable_change_commands_are_additive_protocol_v1_without_blob_bytes() {
        let run_id = "00000000-0000-0000-0000-0000000000c1"
            .parse()
            .expect("run id");
        let file_change_id = "00000000-0000-0000-0000-0000000000c2"
            .parse()
            .expect("file change id");
        let operation_id = "00000000-0000-0000-0000-0000000000c3"
            .parse()
            .expect("operation id");
        let cases = [
            AppCommand::LoadChangeSet { run_id },
            AppCommand::LoadUnifiedDiff { file_change_id },
            AppCommand::RequestRestore { file_change_id },
            AppCommand::LoadChangeRecovery,
            AppCommand::RetryChangeRecovery { operation_id },
            AppCommand::RequestRecoveryRestore { operation_id },
        ];

        for (index, command) in cases.into_iter().enumerate() {
            let envelope = RequestEnvelope::command(format!("changes-{index}"), command);
            let encoded = serde_json::to_vec(&envelope).expect("request JSON");
            assert!(!encoded.windows(9).any(|window| window == b"blob_bytes"));
            assert_eq!(
                serde_json::from_slice::<RequestEnvelope>(&encoded).expect("parsed request"),
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
        envelope.request_id = " padded ".into();
        assert_eq!(envelope.validate(), Err(ProtocolError::MalformedRequestId));
        envelope.request_id = "line\nbreak".into();
        assert_eq!(envelope.validate(), Err(ProtocolError::MalformedRequestId));
        envelope.request_id = "x".repeat(argentum_store::MAX_COMMAND_REQUEST_ID_BYTES + 1);
        assert_eq!(envelope.validate(), Err(ProtocolError::RequestIdTooLong));
    }

    #[test]
    fn platform_authority_event_is_additive_and_contains_only_public_projection() {
        let snapshot = PlatformAuthoritySnapshot::new(
            "windows",
            PlatformAuthorityTier::Standard,
            Some(PlatformAuthorityTier::Standard),
            PlatformAuthorityState::Active,
            1,
            Some(
                PlatformCapabilityProbeSnapshot::new(
                    None,
                    Some("S-1-5-21-test"),
                    None,
                    ["process.current-user"],
                )
                .expect("probe"),
            ),
            "Windows current-user authority is active",
        )
        .expect("snapshot");
        let response = ResponseEnvelope::new(
            7,
            None,
            ServerPayload::event(AppEvent::PlatformAuthorityChanged(snapshot.clone())),
        );

        let encoded = serde_json::to_value(&response).expect("response JSON");
        let event = &encoded["event"];
        assert_eq!(event["PlatformAuthorityChanged"]["platform"], "windows");
        assert_eq!(event["PlatformAuthorityChanged"]["generation"], 1);
        assert!(event["PlatformAuthorityChanged"].get("ticket").is_none());
        assert!(event["PlatformAuthorityChanged"].get("proof").is_none());
        assert!(event["PlatformAuthorityChanged"].get("issuer_id").is_none());
        assert!(event["PlatformAuthorityChanged"].get("lease_id").is_none());
        assert_eq!(
            serde_json::from_value::<ResponseEnvelope>(encoded).expect("round trip"),
            response
        );
    }

    #[test]
    fn canonical_commands_sort_object_keys_and_preserve_significant_text() {
        let first = parse_request(br#"{"protocol_version":1,"request_id":"a","type":"command","command":{"tool_budget":2,"kind":"set_goal","objective":"same","time_budget_seconds":3,"token_budget":1}}"#)
            .expect("first request");
        let reordered = parse_request(br#"{"type":"command","command":{"kind":"set_goal","objective":"same","token_budget":1,"tool_budget":2,"time_budget_seconds":3},"request_id":"b","protocol_version":1}"#)
            .expect("reordered request");
        let ClientPayload::Command { command: first } = first.payload else {
            panic!("command payload");
        };
        let ClientPayload::Command { command: reordered } = reordered.payload else {
            panic!("command payload");
        };
        assert_eq!(
            canonical_command(&first).unwrap(),
            canonical_command(&reordered).unwrap()
        );

        for changed in ["same ", "same\r\n", "sáme"] {
            let changed = AppCommand::SetGoal {
                objective: changed.into(),
                token_budget: Some(1),
                tool_budget: Some(2),
                time_budget_seconds: Some(3),
            };
            assert_ne!(
                canonical_command(&first).unwrap(),
                canonical_command(&changed).unwrap()
            );
        }
        for changed in [
            AppCommand::SetGoal {
                objective: "same".into(),
                token_budget: Some(4),
                tool_budget: Some(2),
                time_budget_seconds: Some(3),
            },
            AppCommand::SetGoal {
                objective: "same".into(),
                token_budget: Some(1),
                tool_budget: Some(4),
                time_budget_seconds: Some(3),
            },
            AppCommand::SetGoal {
                objective: "same".into(),
                token_budget: Some(1),
                tool_budget: Some(2),
                time_budget_seconds: Some(4),
            },
        ] {
            assert_ne!(
                canonical_command(&first).unwrap(),
                canonical_command(&changed).unwrap()
            );
        }
        assert_ne!(
            canonical_command(&AppCommand::SelectSession {
                session_id: argentum_domain::SessionId::from_u128(1),
            })
            .unwrap(),
            canonical_command(&AppCommand::SelectSession {
                session_id: argentum_domain::SessionId::from_u128(2),
            })
            .unwrap()
        );
    }

    #[test]
    fn syntax_only_optional_defaults_have_one_canonical_form() {
        let absent = parse_request(br#"{"protocol_version":1,"request_id":"a","type":"command","command":{"kind":"set_inspector_surface"}}"#)
            .expect("absent default");
        let explicit = parse_request(br#"{"protocol_version":1,"request_id":"b","type":"command","command":{"kind":"set_inspector_surface","surface":null}}"#)
            .expect("explicit default");
        let ClientPayload::Command { command: absent } = absent.payload else {
            panic!("command payload");
        };
        let ClientPayload::Command { command: explicit } = explicit.payload else {
            panic!("command payload");
        };
        assert_eq!(
            canonical_command(&absent).unwrap(),
            canonical_command(&explicit).unwrap()
        );
    }

    #[test]
    fn parser_rejects_duplicate_members_and_ignored_behavior_fields() {
        let duplicate = br#"{"protocol_version":1,"request_id":"a","type":"command","command":{"kind":"submit_task","prompt":"one","password=CANARY":"first","password=CANARY":"second"}}"#;
        let duplicate_error = parse_request(duplicate).unwrap_err();
        assert!(duplicate_error.contains("duplicate JSON object member"));
        assert!(!duplicate_error.contains("CANARY"));

        let unknown = br#"{"protocol_version":1,"request_id":"a","type":"command","command":{"kind":"submit_task","prompt":"one","api_key_CANARY":true}}"#;
        let unknown_error = parse_request(unknown).unwrap_err();
        assert_eq!(
            unknown_error,
            "request does not match the supported typed shape"
        );
        assert!(!unknown_error.contains("CANARY"));

        let top_level = br#"{"protocol_version":1,"request_id":"a","type":"command","command":{"kind":"new_session"},"client_secret_CANARY":true}"#;
        let top_level_error = parse_request(top_level).unwrap_err();
        assert_eq!(top_level_error, "request contains an unknown field");
        assert!(!top_level_error.contains("CANARY"));
    }

    #[test]
    fn inspection_request_has_a_bounded_typed_shape() {
        let request = RequestEnvelope::inspect_command("inspect-transport", "original-command");
        request.validate().unwrap();
        let encoded = serde_json::to_value(&request).unwrap();
        assert_eq!(encoded["type"], "inspect_command");
        assert_eq!(encoded["command_request_id"], "original-command");
        assert_eq!(
            serde_json::from_value::<RequestEnvelope>(encoded).unwrap(),
            request
        );

        let mut malformed = request;
        malformed.request_id = " padded ".into();
        assert_eq!(malformed.validate(), Err(ProtocolError::MalformedRequestId));
    }
}
