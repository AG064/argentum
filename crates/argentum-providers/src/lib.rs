use std::collections::BTreeMap;
use std::sync::{Arc, RwLock};
use std::time::Duration;

use argentum_domain::{ProviderKind, ProviderProfile, ProviderStatus};
use argentum_platform::{AuthorityDependentService, PlatformError};
use argentum_security::SecretValue;
mod chatgpt;
pub mod chatgpt_auth;
mod chatgpt_identity;
mod chatgpt_vault;
use async_trait::async_trait;
use bytes::Bytes;
pub use chatgpt::ChatGptAssistantState;
use futures_util::StreamExt;
use reqwest::header::{HeaderMap, HeaderValue, CONTENT_TYPE, RETRY_AFTER};
use reqwest_eventsource::{Event, EventSource};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tokio::sync::mpsc;
use tokio::sync::Mutex as AsyncMutex;
use tokio_util::sync::CancellationToken;
use url::Url;

mod android_local;
mod capabilities;
pub use capabilities::{
    CapabilitySupport, ProviderCapabilities, ProviderCapabilityIdentity,
    ProviderCapabilityResolution,
};
mod local_backend;
pub use local_backend::{
    AndroidForegroundStop, AndroidLlamaCppBackend, LlamaCppSidecarBackend, LocalBackendLease,
    LocalBackendStatus, LocalInferenceBackend,
};
mod local_models;
mod minimax_continuation;
pub use minimax_continuation::{ContinuationScope, MinimaxAssistantState};
mod sidecar_protocol;
pub use sidecar_protocol::{
    parse_sidecar_context, sidecar_context_for_limit, SIDECAR_CONTEXT_TOKENS,
    SIDECAR_CONTEXT_VARIANTS,
};
mod local_runtime;
mod runtime_bundles;

pub use android_local::AndroidLlamaCppProvider;
mod tool_contract;
mod usage_accounting;
pub use local_models::{
    android_local_model_offers, download_local_model, local_model_offers, LocalLlamaCppProvider,
    LocalModelOffer,
};
#[cfg(feature = "test-support")]
pub use local_runtime::{
    local_runtime_recording_test_fixture, local_runtime_test_fixture, LocalRuntimeTestControl,
};
pub use local_runtime::{
    LocalRuntimeClient, LocalRuntimeConfig, LocalRuntimeLaunchApproval, LocalRuntimeLaunchPlan,
    LocalRuntimeModule, PreparedLocalRuntime, SidecarLease,
};
pub use runtime_bundles::{
    install_local_runtime, local_runtime_bundle_path, local_runtime_offers,
    runtime_bundle_is_valid, LocalRuntimeOffer,
};
pub use tool_contract::ModelToolContract;

#[derive(Debug, Error)]
pub enum ProviderError {
    #[error("context_budget_invalid: {0}")]
    ContextBudgetInvalid(&'static str),
    #[error("context_accounting_unsupported: {0}")]
    ContextAccountingUnsupported(&'static str),
    #[error("provider request failed: {0}")]
    Http(#[from] reqwest::Error),
    #[error("provider endpoint is invalid: {0}")]
    Endpoint(#[from] url::ParseError),
    #[error("provider returned an invalid response: {0}")]
    InvalidResponse(String),
    #[error("response truncated: output limit or context window reached; partial tool calls were not executed")]
    ResponseTruncated,
    #[error("local model request is invalid: {0}")]
    InvalidRequest(&'static str),
    #[error("provider returned an error: {0}")]
    Api(String),
    #[error("provider returned HTTP {status}{safe_detail}")]
    ApiHttpStatus { status: u16, safe_detail: String },
    #[error("provider connection failed before dispatch")]
    PreDispatchConnection,
    #[error("provider returned retryable HTTP {status}{safe_detail}")]
    TransientHttpStatus {
        status: u16,
        retry_after_ms: Option<u64>,
        safe_detail: String,
    },
    #[error("provider stream could not be created")]
    StreamSetup,
    #[error("provider stream failed")]
    Stream,
    #[error("provider stream consumer closed")]
    ConsumerClosed,
    #[error("provider request was cancelled")]
    RequestCancelled,
    #[error("prepared provider request does not match the selected provider binding")]
    PreparedRequestBindingMismatch,
    #[error("provider connectivity probe is not supported for {provider}")]
    ProbeUnsupported { provider: &'static str },
    #[error("provider probe returned HTTP {status}")]
    ProbeHttpStatus { status: u16 },
    #[error("provider probe timed out after {timeout_ms} ms")]
    ProbeTimeout { timeout_ms: u64 },
    #[error("provider probe response exceeded {limit_bytes} bytes")]
    ProbeResponseTooLarge { limit_bytes: usize },
    #[error("provider probe endpoint is unsafe: {reason}")]
    UnsafeProbeEndpoint { reason: &'static str },
    #[error("provider is not configured: {provider_id}")]
    ProviderNotConfigured { provider_id: String },
    #[error("provider profile {field} is invalid: {reason}")]
    InvalidProfile {
        field: &'static str,
        reason: &'static str,
    },
    #[error("provider credentials are required for {provider_id}")]
    CredentialsRequired { provider_id: String },
    #[error("hosted provider endpoint is not approved for {provider_id}")]
    UnsafeHostedEndpoint { provider_id: String },
    #[error("provider kind is not usable for {provider_id}")]
    UnsupportedProviderKind { provider_id: String },
    #[error("local llama.cpp runtime is unavailable: {reason}")]
    LocalRuntimeUnavailable { reason: &'static str },
    #[error("local llama.cpp model could not be loaded")]
    LocalModelLoadFailed,
    #[error("local llama.cpp does not support {capability}")]
    LocalCapabilityUnsupported { capability: &'static str },
    #[error("local model download failed")]
    LocalModelDownloadFailed,
    #[error("local model download exceeded its {timeout_ms}ms deadline")]
    LocalModelDownloadDeadline { timeout_ms: u64 },
    #[error("local model download exceeded its size limit of {limit_bytes} bytes")]
    LocalModelDownloadTooLarge { limit_bytes: u64 },
    #[error("local model download failed its integrity check")]
    LocalModelIntegrityMismatch,
    #[error("local model download destination is invalid")]
    LocalModelDownloadDestinationInvalid,
    #[error("local runtime download failed")]
    LocalRuntimeDownloadFailed,
    #[error("local runtime download exceeded its {timeout_ms}ms deadline")]
    LocalRuntimeDownloadDeadline { timeout_ms: u64 },
    #[error("local runtime download exceeded its size limit of {limit_bytes} bytes")]
    LocalRuntimeDownloadTooLarge { limit_bytes: u64 },
    #[error("local runtime bundle failed its integrity check")]
    LocalRuntimeIntegrityMismatch,
    #[error("local runtime bundle or destination is invalid")]
    LocalRuntimeBundleInvalid,
    #[error("local runtime model selection is invalid")]
    LocalRuntimeModelInvalid,
    #[error("local runtime launch requires exact local-user approval")]
    LocalRuntimeApprovalRequired,
    #[error("local runtime launch approval was denied")]
    LocalRuntimeApprovalDenied,
    #[error("local runtime launch approval is stale")]
    LocalRuntimeApprovalStale,
    #[error("local runtime launch was cancelled")]
    LocalRuntimeCancelled,
    #[error("local runtime launch failed during {stage}")]
    LocalRuntimeLaunchFailed { stage: &'static str },
    #[error("local runtime lifecycle operation failed")]
    LocalRuntimeLifecycleFailed,
    #[error("local runtime client lease is stale")]
    LocalRuntimeLeaseStale,
    #[error("local inference backend is being reconfigured")]
    LocalBackendBusy,
    #[error("coding_capability_unavailable: {reason}")]
    CodingCapabilityUnavailable { reason: &'static str },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProviderRetryClass {
    PreDispatchConnection,
    TransientHttpStatus {
        status: u16,
        retry_after_ms: Option<u64>,
    },
}

impl ProviderError {
    pub fn retry_class(&self) -> Option<ProviderRetryClass> {
        match self {
            Self::PreDispatchConnection => Some(ProviderRetryClass::PreDispatchConnection),
            Self::TransientHttpStatus {
                status,
                retry_after_ms,
                ..
            } => Some(ProviderRetryClass::TransientHttpStatus {
                status: *status,
                retry_after_ms: *retry_after_ms,
            }),
            _ => None,
        }
    }
}

impl From<reqwest_eventsource::Error> for ProviderError {
    fn from(_error: reqwest_eventsource::Error) -> Self {
        Self::Stream
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct ModelRequest {
    #[serde(skip)]
    pub continuation_scope: Option<ContinuationScope>,
    /// A response constraint only. It enables no tool or execution authority.
    #[serde(skip_serializing_if = "tool_requirement_absent")]
    pub require_tool_call: bool,
    pub output_limit: u32,
    pub model: String,
    pub system: Option<String>,
    pub history: Vec<ModelMessage>,
    pub prompt: String,
    pub tools: Vec<ModelToolDefinition>,
    pub tool_exchanges: Vec<ModelToolExchange>,
}

fn tool_requirement_absent(value: &bool) -> bool {
    !value
}

/// One logical request prepared under an exact provider and budget binding.
///
/// Construction is controlled by the provider registry. This value contains
/// no credential, permission, reservation, or authority to dispatch.
pub struct PreparedProviderRequest {
    request: ModelRequest,
    accounting: argentum_domain::context_budget::RequestAccounting,
    transport: PreparedTransport,
}

enum PreparedTransport {
    Raw,
    Compatible(PreparedCompatibleRequest),
    Responses(PreparedCompatibleRequest),
}

struct PreparedCompatibleRequest {
    provider_binding: uuid::Uuid,
    profile_id: String,
    endpoint: String,
    model: String,
    output_limit: u32,
    budget_revision: uuid::Uuid,
    body: Bytes,
}

impl std::fmt::Debug for PreparedProviderRequest {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let (transport, body_bytes) = match &self.transport {
            PreparedTransport::Raw => ("raw", None),
            PreparedTransport::Compatible(prepared) => {
                ("compatible_http", Some(prepared.body.len()))
            }
            PreparedTransport::Responses(prepared) => ("responses_http", Some(prepared.body.len())),
        };
        formatter
            .debug_struct("PreparedProviderRequest")
            .field("profile_id", &self.accounting.profile_id)
            .field("model", &self.accounting.model)
            .field("budget_revision", &self.accounting.budget_revision)
            .field("transport", &transport)
            .field("body_bytes", &body_bytes)
            .finish_non_exhaustive()
    }
}

impl PreparedProviderRequest {
    fn raw(
        request: ModelRequest,
        accounting: argentum_domain::context_budget::RequestAccounting,
    ) -> Self {
        Self {
            request,
            accounting,
            transport: PreparedTransport::Raw,
        }
    }

    pub fn request(&self) -> &ModelRequest {
        &self.request
    }

    pub fn accounting(&self) -> &argentum_domain::context_budget::RequestAccounting {
        &self.accounting
    }

    pub fn with_profile_catalog_context(
        mut self,
        limit: Option<u64>,
        source: Option<argentum_domain::ProviderCatalogSource>,
    ) -> Self {
        self.accounting.profile_catalog_context_limit = limit;
        self.accounting.profile_catalog_source = source;
        self
    }

    #[cfg(feature = "test-support")]
    pub fn encoded_body_for_test(&self) -> Option<&[u8]> {
        match &self.transport {
            PreparedTransport::Raw => None,
            PreparedTransport::Compatible(prepared) => Some(&prepared.body),
            PreparedTransport::Responses(prepared) => Some(&prepared.body),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum ModelMessageRole {
    User,
    Assistant,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ModelMessage {
    pub role: ModelMessageRole,
    pub content: String,
    /// A complete historical protocol group. Never executable or authoritative.
    pub tool_exchange: Option<ModelToolExchange>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ModelToolDefinition {
    pub name: String,
    pub description: String,
    pub parameters: serde_json::Value,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ModelToolCall {
    pub id: String,
    pub name: String,
    pub arguments: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ModelToolResult {
    /// Transient broker-validated image data, never reconstructed from history.
    #[serde(skip)]
    pub images: Vec<ModelImage>,
    pub call_id: String,
    pub content: String,
}

#[derive(Clone, PartialEq, Eq)]
pub struct ModelImage {
    data_url: String,
    pub sha256: String,
    pub origin: String,
}
impl std::fmt::Debug for ModelImage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ModelImage")
            .field("bytes", &self.data_url.len())
            .field("sha256", &self.sha256)
            .finish_non_exhaustive()
    }
}
impl ModelImage {
    pub fn from_png(
        bytes: &[u8],
        expected_sha256: &str,
        origin: String,
    ) -> Result<Self, ProviderError> {
        if bytes.len() > 128 * 1024
            || bytes.len() < 24
            || !bytes.starts_with(b"\x89PNG\r\n\x1a\n")
            || &bytes[12..16] != b"IHDR"
        {
            return Err(ProviderError::InvalidRequest("image must be a bounded PNG"));
        }
        let width = u32::from_be_bytes(bytes[16..20].try_into().expect("checked PNG header"));
        let height = u32::from_be_bytes(bytes[20..24].try_into().expect("checked PNG header"));
        if width == 0 || height == 0 || width > 1200 || height > 800 {
            return Err(ProviderError::InvalidRequest(
                "PNG viewport exceeds admitted dimensions",
            ));
        }
        use sha2::Digest;
        let hash = format!("{:x}", sha2::Sha256::digest(bytes));
        if hash != expected_sha256 {
            return Err(ProviderError::InvalidRequest(
                "image artifact identity changed",
            ));
        }
        let parsed = Url::parse(&origin)?;
        if !matches!(parsed.scheme(), "http" | "https")
            || !parsed.username().is_empty()
            || parsed.password().is_some()
            || parsed.origin().ascii_serialization() != origin
        {
            return Err(ProviderError::InvalidRequest("image origin is invalid"));
        }
        use base64::Engine;
        Ok(Self {
            data_url: format!(
                "data:image/png;base64,{}",
                base64::engine::general_purpose::STANDARD.encode(bytes)
            ),
            sha256: hash,
            origin,
        })
    }
    pub fn data_url(&self) -> &str {
        &self.data_url
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ModelToolExchange {
    #[serde(skip)]
    pub chatgpt_continuation: Option<Box<ChatGptAssistantState>>,
    #[serde(skip)]
    pub assistant_continuation: Option<Box<MinimaxAssistantState>>,
    pub assistant_content: String,
    #[serde(skip)]
    pub assistant_reasoning: String,
    pub calls: Vec<ModelToolCall>,
    pub results: Vec<ModelToolResult>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelUsage {
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub total_tokens: u64,
    pub cached_input_tokens: Option<u64>,
    pub reasoning_tokens: Option<u64>,
    pub context_window_tokens: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderModel {
    pub id: String,
    pub label: String,
    pub context_window_tokens: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderRequestValidation {
    pub exact_input_tokens: Option<u64>,
    pub context_tokens: Option<u64>,
    pub output_limit: u32,
    pub representation_fingerprint: Option<String>,
    pub runtime_fingerprint: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProviderPreset {
    ChatGpt,
    OpenAiCompatible,
    MiniMax,
    OpenAi,
    DeepSeek,
    LlamaCpp,
    #[doc(hidden)]
    LmStudio,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProviderPresetInfo {
    pub id: &'static str,
    pub label: &'static str,
    pub default_endpoint: Option<&'static str>,
    pub default_model: &'static str,
    pub credential_profile_id: Option<&'static str>,
}

const PROVIDER_PRESET_INFOS: [ProviderPresetInfo; 6] = [
    ProviderPresetInfo {
        id: "openai-compatible",
        label: "OpenAI compatible",
        default_endpoint: None,
        default_model: "model",
        credential_profile_id: None,
    },
    ProviderPresetInfo {
        id: "minimax",
        label: "MiniMax",
        default_endpoint: Some("https://api.minimax.io/v1/"),
        default_model: "MiniMax-M2.7",
        credential_profile_id: Some("minimax"),
    },
    ProviderPresetInfo {
        id: "openai",
        label: "OpenAI API",
        default_endpoint: Some("https://api.openai.com/v1/"),
        default_model: "gpt-5.6",
        credential_profile_id: Some("openai"),
    },
    ProviderPresetInfo {
        id: "deepseek",
        label: "DeepSeek",
        default_endpoint: Some("https://api.deepseek.com/"),
        default_model: "deepseek-flash",
        credential_profile_id: Some("deepseek"),
    },
    ProviderPresetInfo {
        id: "llama-cpp",
        label: "llama.cpp",
        default_endpoint: Some("local://llama-cpp/"),
        default_model: "Qwen3.5-0.8B-Q8_0.gguf",
        credential_profile_id: None,
    },
    ProviderPresetInfo {
        id: "chatgpt",
        label: "ChatGPT plan",
        default_endpoint: Some("https://api.openai.com/v1/"),
        default_model: "gpt-6-luna",
        credential_profile_id: None,
    },
];

const LM_STUDIO_COMPATIBILITY_INFO: ProviderPresetInfo = ProviderPresetInfo {
    id: "lm-studio",
    label: "LM Studio",
    default_endpoint: Some("http://127.0.0.1:1234/v1/"),
    default_model: "local-model",
    credential_profile_id: None,
};

pub fn provider_presets() -> &'static [ProviderPresetInfo] {
    &PROVIDER_PRESET_INFOS
}

pub fn provider_preset(id: &str) -> Option<ProviderPresetInfo> {
    ProviderPreset::parse(id).map(ProviderPreset::info)
}

impl ProviderPreset {
    pub fn parse(id: &str) -> Option<Self> {
        match id.trim() {
            "chatgpt" => Some(Self::ChatGpt),
            "openai-compatible" => Some(Self::OpenAiCompatible),
            "minimax" => Some(Self::MiniMax),
            "openai" => Some(Self::OpenAi),
            "deepseek" => Some(Self::DeepSeek),
            "llama-cpp" => Some(Self::LlamaCpp),
            "lm-studio" => Some(Self::LmStudio),
            _ => None,
        }
    }

    pub fn info(self) -> ProviderPresetInfo {
        match self {
            Self::ChatGpt => PROVIDER_PRESET_INFOS[5],
            Self::OpenAiCompatible => PROVIDER_PRESET_INFOS[0],
            Self::MiniMax => PROVIDER_PRESET_INFOS[1],
            Self::OpenAi => PROVIDER_PRESET_INFOS[2],
            Self::DeepSeek => PROVIDER_PRESET_INFOS[3],
            Self::LlamaCpp => PROVIDER_PRESET_INFOS[4],
            Self::LmStudio => LM_STUDIO_COMPATIBILITY_INFO,
        }
    }

    fn is_hosted(self) -> bool {
        matches!(self, Self::MiniMax | Self::OpenAi | Self::DeepSeek)
    }

    pub fn profile(
        self,
        endpoint: Option<&str>,
        model: Option<&str>,
    ) -> Result<ProviderProfile, ProviderError> {
        let info = self.info();
        let endpoint =
            endpoint
                .or(info.default_endpoint)
                .ok_or_else(|| ProviderError::InvalidProfile {
                    field: "endpoint",
                    reason: "generic OpenAI-compatible preset requires an explicit endpoint",
                })?;
        normalize_provider_profile(ProviderProfile {
            id: info.id.into(),
            label: info.label.into(),
            kind: if matches!(self, Self::LmStudio) {
                ProviderKind::LocalLmStudio
            } else if matches!(self, Self::LlamaCpp) {
                ProviderKind::LocalLlamaCpp
            } else {
                ProviderKind::OpenAiCompatible
            },
            endpoint: endpoint.into(),
            model: model.unwrap_or(info.default_model).into(),
            selected: false,
        })
    }

    pub fn build_provider(
        self,
        profile: &ProviderProfile,
        api_key: Option<SecretValue>,
    ) -> Result<Arc<dyn ModelProvider>, ProviderError> {
        let profile = normalize_provider_profile(profile.clone())?;
        let info = self.info();
        if profile.id != info.id {
            return Err(ProviderError::InvalidProfile {
                field: "id",
                reason: "profile identity does not match the provider preset",
            });
        }
        let expected_kind = if matches!(self, Self::LmStudio) {
            ProviderKind::LocalLmStudio
        } else if matches!(self, Self::LlamaCpp) {
            ProviderKind::LocalLlamaCpp
        } else {
            ProviderKind::OpenAiCompatible
        };
        if profile.kind != expected_kind {
            return Err(ProviderError::InvalidProfile {
                field: "kind",
                reason: "profile kind does not match the provider preset",
            });
        }
        if self.is_hosted() {
            if api_key.is_none() {
                return Err(ProviderError::CredentialsRequired {
                    provider_id: profile.id,
                });
            }
            validate_hosted_credential_origin(&profile)?;
        }

        match self {
            Self::ChatGpt => Err(ProviderError::CredentialsRequired {
                provider_id: "chatgpt".into(),
            }),
            Self::LlamaCpp => Ok(Arc::new(LocalLlamaCppProvider::unconfigured(profile.model))),
            Self::LmStudio => Ok(Arc::new(LocalLmStudioProvider::new(
                &profile.endpoint,
                &profile.model,
            )?)),
            Self::OpenAiCompatible | Self::MiniMax | Self::OpenAi | Self::DeepSeek => Ok(Arc::new(
                OpenAiCompatibleProvider::new(
                    &profile.endpoint,
                    &profile.model,
                    api_key,
                    ProviderKind::OpenAiCompatible,
                    info.label,
                )?
                .with_hidden_tagged_reasoning(matches!(self, Self::MiniMax | Self::DeepSeek)),
            )),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProviderEvent {
    ChatGptContinuation(Box<ChatGptAssistantState>),
    UsageObservation(argentum_domain::goal_accounting::ObservedUsage),
    UsageUncertain,
    Delta(String),
    ReasoningDelta(String),
    ToolCall(ModelToolCall),
    AssistantContinuation(Box<MinimaxAssistantState>),
    Usage(ModelUsage),
    Completed,
}

#[async_trait]
pub trait ModelProvider: Send + Sync {
    fn id(&self) -> &'static str;

    fn status(&self) -> ProviderStatus;

    async fn probe(&self) -> Result<ProviderStatus, ProviderError> {
        Err(ProviderError::ProbeUnsupported {
            provider: self.id(),
        })
    }

    async fn list_models(&self) -> Result<Vec<ProviderModel>, ProviderError> {
        Err(ProviderError::ProbeUnsupported {
            provider: self.id(),
        })
    }

    /// Validate an assembled request and any required already-active runtime
    /// state without beginning inference. A contained sidecar may use its
    /// authenticated local metadata/token-count IPC, never a hosted inference call.
    async fn validate_request(
        &self,
        request: &ModelRequest,
    ) -> Result<ProviderRequestValidation, ProviderError> {
        validate_model_tool_payload(request)?;
        Ok(ProviderRequestValidation {
            exact_input_tokens: None,
            context_tokens: None,
            output_limit: request.output_limit,
            representation_fingerprint: None,
            runtime_fingerprint: None,
        })
    }

    /// Prepare immutable request structure and exact accounting without
    /// granting permission to dispatch it. Compatible HTTP implementations
    /// override this to retain their single encoded application body.
    fn prepare_request(
        &self,
        profile: &ProviderProfile,
        budget: &argentum_domain::context_budget::ModelBudget,
        request: ModelRequest,
    ) -> Result<PreparedProviderRequest, ProviderError> {
        let accounting = account_chat_request(profile, budget, &request)?;
        Ok(PreparedProviderRequest::raw(request, accounting))
    }

    /// Revalidate provider-specific state for an already structurally checked
    /// request. The default preserves existing raw-provider behavior.
    async fn validate_prepared_request(
        &self,
        prepared: &PreparedProviderRequest,
    ) -> Result<ProviderRequestValidation, ProviderError> {
        self.validate_request(prepared.request()).await
    }

    /// Identity of already-active local runtime evidence, when such evidence
    /// is required by this adapter. Hosted adapters return no local identity.
    async fn capability_runtime_evidence(&self) -> Result<Option<String>, ProviderError> {
        Ok(None)
    }

    async fn stream(
        &self,
        request: ModelRequest,
        sender: mpsc::Sender<ProviderEvent>,
    ) -> Result<(), ProviderError>;

    async fn stream_with_cancellation(
        &self,
        request: ModelRequest,
        sender: mpsc::Sender<ProviderEvent>,
        cancellation: CancellationToken,
    ) -> Result<(), ProviderError> {
        tokio::select! {
            _ = cancellation.cancelled() => Err(ProviderError::RequestCancelled),
            result = self.stream(request, sender) => result,
        }
    }

    /// Dispatch a request only if its provider-specific final validation still
    /// matches. Hosted adapters use their ordinary cancellation path because
    /// they have no local loaded-runtime snapshot to pin.
    async fn stream_validated_with_cancellation(
        &self,
        request: ModelRequest,
        _expected: &ProviderRequestValidation,
        sender: mpsc::Sender<ProviderEvent>,
        cancellation: CancellationToken,
    ) -> Result<(), ProviderError> {
        self.stream_with_cancellation(request, sender, cancellation)
            .await
    }

    /// Dispatch an already prepared request. Dynamic admission remains owned
    /// by the caller and is required for every physical attempt.
    async fn stream_prepared_validated_with_cancellation(
        &self,
        prepared: &PreparedProviderRequest,
        expected: &ProviderRequestValidation,
        sender: mpsc::Sender<ProviderEvent>,
        cancellation: CancellationToken,
    ) -> Result<(), ProviderError> {
        self.stream_validated_with_cancellation(
            prepared.request().clone(),
            expected,
            sender,
            cancellation,
        )
        .await
    }

    #[cfg(feature = "test-support")]
    fn build_prepared_transport_request_for_test(
        &self,
        _prepared: &PreparedProviderRequest,
    ) -> Result<(Vec<u8>, Option<String>), ProviderError> {
        Err(ProviderError::PreparedRequestBindingMismatch)
    }

    #[cfg(feature = "test-support")]
    fn build_legacy_transport_request_for_test(
        &self,
        _request: ModelRequest,
    ) -> Result<(Vec<u8>, Option<String>), ProviderError> {
        Err(ProviderError::PreparedRequestBindingMismatch)
    }
}

#[derive(Clone)]
pub struct OpenAiCompatibleProvider {
    client: reqwest::Client,
    probe_client: reqwest::Client,
    endpoint: Url,
    model: String,
    api_key: Option<SecretValue>,
    kind: ProviderKind,
    label: &'static str,
    probe_timeout: Duration,
    hide_tagged_reasoning: bool,
    sidecar_protocol: bool,
    sidecar_context_tokens: u32,
    preparation_binding: uuid::Uuid,
    #[cfg(any(test, feature = "test-support"))]
    test_completion_endpoint: Option<Url>,
}

const DEFAULT_PROBE_TIMEOUT: Duration = Duration::from_secs(3);
const LOCAL_SIDECAR_CONNECT_TIMEOUT: Duration = Duration::from_secs(2);
const MAX_PROBE_BODY_BYTES: usize = 256 * 1024;
const MAX_PROBE_MODELS: usize = 4_096;
const MAX_PROBE_MODEL_ID_BYTES: usize = 1_024;
const MAX_STREAM_EVENT_BYTES: usize = 256 * 1024;
const MAX_TRUNCATED_USAGE_FRAMES: usize = 8;
const MAX_PROVIDER_ERROR_BODY_BYTES: usize = 16 * 1024;
const MAX_PROVIDER_ERROR_CODE_BYTES: usize = 64;
const MAX_PROVIDER_ERROR_MESSAGE_BYTES: usize = 512;
const MAX_PROVIDER_ERROR_REQUEST_ID_BYTES: usize = 128;
const MAX_TOOL_EXCHANGES: usize = argentum_domain::working_context::MAX_RUN_GROUPS as usize - 1;
const MAX_TOOL_CALLS_PER_RESPONSE: usize = 8;
const MAX_TOOL_CALL_ID_BYTES: usize = 256;
const MAX_TOOL_NAME_BYTES: usize = 64;
const MAX_TOOL_ARGUMENT_BYTES: usize = 128 * 1024;
const MAX_TOOL_RESULT_BYTES: usize = 64 * 1024;
const MAX_VISIBLE_RESPONSE_BYTES: usize = 512 * 1024;
const MAX_REASONING_RESPONSE_BYTES: usize = 512 * 1024;

impl std::fmt::Debug for OpenAiCompatibleProvider {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("OpenAiCompatibleProvider")
            .field("endpoint", &public_endpoint(&self.endpoint))
            .field("model", &self.model)
            .field("kind", &self.kind)
            .finish_non_exhaustive()
    }
}

impl OpenAiCompatibleProvider {
    /// Test-only transport seam. The logical origin, wire protocol and body
    /// binding remain canonical MiniMax; only credential-free HTTP is local.
    #[cfg(feature = "test-support")]
    pub fn minimax_loopback_for_test(endpoint: &str, model: &str) -> Result<Self, ProviderError> {
        let endpoint = Url::parse(endpoint)?;
        if endpoint.scheme() != "http"
            || endpoint.host_str() != Some("127.0.0.1")
            || !endpoint.username().is_empty()
            || endpoint.password().is_some()
            || endpoint.query().is_some()
            || endpoint.fragment().is_some()
        {
            return Err(ProviderError::InvalidRequest(
                "synthetic transport must be literal HTTP loopback",
            ));
        }
        let mut provider = Self::new(
            "https://api.minimax.io/v1/",
            model,
            None,
            ProviderKind::OpenAiCompatible,
            "MiniMax",
        )?
        .with_hidden_tagged_reasoning(true);
        provider.client = reqwest::Client::builder()
            .no_proxy()
            .redirect(reqwest::redirect::Policy::none())
            .build()?;
        provider.test_completion_endpoint = Some(endpoint);
        Ok(provider)
    }
    pub fn new(
        endpoint: impl AsRef<str>,
        model: impl Into<String>,
        api_key: Option<SecretValue>,
        kind: ProviderKind,
        label: &'static str,
    ) -> Result<Self, ProviderError> {
        let mut endpoint = Url::parse(endpoint.as_ref())?;
        if !endpoint.path().ends_with('/') {
            endpoint.set_path(&format!("{}/", endpoint.path()));
        }
        let mut client = reqwest::Client::builder();
        if is_minimax_continuation_origin(&endpoint) {
            // Returned private provider state is bound to this origin. A
            // redirect is not permission to disclose it at a different one.
            client = client.redirect(reqwest::redirect::Policy::none());
        }
        Ok(Self {
            client: client.build()?,
            probe_client: reqwest::Client::builder()
                .redirect(reqwest::redirect::Policy::none())
                .connect_timeout(DEFAULT_PROBE_TIMEOUT)
                .build()?,
            endpoint,
            model: model.into(),
            api_key,
            kind,
            label,
            probe_timeout: DEFAULT_PROBE_TIMEOUT,
            hide_tagged_reasoning: false,
            sidecar_protocol: false,
            sidecar_context_tokens: SIDECAR_CONTEXT_TOKENS,
            preparation_binding: uuid::Uuid::new_v4(),
            #[cfg(any(test, feature = "test-support"))]
            test_completion_endpoint: None,
        })
    }

    #[cfg(test)]
    pub(crate) fn new_local_sidecar(
        endpoint: impl AsRef<str>,
        model: impl Into<String>,
        api_key: SecretValue,
    ) -> Result<Self, ProviderError> {
        Self::new_local_sidecar_with_context(endpoint, model, api_key, SIDECAR_CONTEXT_TOKENS)
    }

    pub(crate) fn new_local_sidecar_with_context(
        endpoint: impl AsRef<str>,
        model: impl Into<String>,
        api_key: SecretValue,
        context_tokens: u32,
    ) -> Result<Self, ProviderError> {
        if !SIDECAR_CONTEXT_VARIANTS.contains(&context_tokens) {
            return Err(ProviderError::ContextBudgetInvalid(
                "unsupported local context variant",
            ));
        }
        let mut provider = Self::new(
            endpoint,
            model,
            Some(api_key),
            ProviderKind::LocalLlamaCpp,
            "llama.cpp",
        )?;
        provider.client = reqwest::Client::builder()
            .no_proxy()
            .redirect(reqwest::redirect::Policy::none())
            .connect_timeout(LOCAL_SIDECAR_CONNECT_TIMEOUT)
            .pool_max_idle_per_host(0)
            .http1_only()
            .build()?;
        provider.probe_client = reqwest::Client::builder()
            .no_proxy()
            .redirect(reqwest::redirect::Policy::none())
            .connect_timeout(LOCAL_SIDECAR_CONNECT_TIMEOUT)
            .pool_max_idle_per_host(0)
            .http1_only()
            .build()?;
        provider.sidecar_protocol = true;
        provider.sidecar_context_tokens = context_tokens;
        Ok(provider)
    }

    fn with_hidden_tagged_reasoning(mut self, hidden: bool) -> Self {
        self.hide_tagged_reasoning = hidden;
        self
    }

    fn supports_minimax_reasoning_protocol(&self) -> bool {
        is_minimax_continuation_origin(&self.endpoint)
    }

    fn supports_deepseek_protocol(&self) -> bool {
        self.endpoint.scheme() == "https"
            && self.endpoint.host_str() == Some("api.deepseek.com")
            && self.endpoint.port_or_known_default() == Some(443)
    }

    fn prepare_compatible_request(
        &self,
        profile: &ProviderProfile,
        budget: &argentum_domain::context_budget::ModelBudget,
        request: ModelRequest,
    ) -> Result<PreparedProviderRequest, ProviderError> {
        if self.sidecar_protocol {
            return Ok(PreparedProviderRequest::raw(
                request.clone(),
                sidecar_protocol::account_request(profile, budget, &request)?,
            ));
        }
        let mut profile_endpoint = Url::parse(&profile.endpoint)?;
        if !profile_endpoint.path().ends_with('/') {
            profile_endpoint.set_path(&format!("{}/", profile_endpoint.path()));
        }
        if profile_endpoint != self.endpoint
            || profile.model != self.model
            || request.model != profile.model
        {
            return Err(ProviderError::PreparedRequestBindingMismatch);
        }
        for exchange in request.tool_exchanges.iter().chain(
            request
                .history
                .iter()
                .filter_map(|message| message.tool_exchange.as_ref()),
        ) {
            if let Some(state) = &exchange.assistant_continuation {
                state.validate_provider(self.preparation_binding)?;
            }
        }
        let (body, accounting) = prepare_compatible_wire(profile, budget, &request)?;
        Ok(PreparedProviderRequest {
            transport: PreparedTransport::Compatible(PreparedCompatibleRequest {
                provider_binding: self.preparation_binding,
                profile_id: profile.id.clone(),
                endpoint: self.endpoint.to_string(),
                model: request.model.clone(),
                output_limit: request.output_limit,
                budget_revision: budget.revision,
                body,
            }),
            request,
            accounting,
        })
    }

    fn prepared_compatible_body(
        &self,
        prepared: &PreparedProviderRequest,
    ) -> Result<Bytes, ProviderError> {
        let PreparedTransport::Compatible(compatible) = &prepared.transport else {
            return Err(ProviderError::PreparedRequestBindingMismatch);
        };
        if compatible.provider_binding != self.preparation_binding
            || compatible.endpoint != self.endpoint.as_str()
            || compatible.model != prepared.request.model
            || compatible.output_limit != prepared.request.output_limit
            || compatible.profile_id != prepared.accounting.profile_id
            || compatible.budget_revision != prepared.accounting.budget_revision
            || compatible.body.len() as u64 != prepared.accounting.accounted_input
        {
            return Err(ProviderError::PreparedRequestBindingMismatch);
        }
        Ok(compatible.body.clone())
    }

    /// PERF04 measurement seam for the accepted pre-reuse transport path.
    ///
    /// This is available only when explicit test support is enabled. It builds
    /// the same reqwest request as dispatch did before prepared-body reuse and
    /// returns only the synthetic encoded body and content type.
    #[cfg(feature = "test-support")]
    fn build_legacy_transport_request(
        &self,
        request: ModelRequest,
    ) -> Result<(Vec<u8>, Option<String>), ProviderError> {
        validate_model_tool_payload(&request)?;
        let selected_model = if request.model.is_empty() {
            self.model.clone()
        } else {
            request.model.clone()
        };
        let body = build_chat_request_inner(
            ModelRequest {
                model: selected_model,
                ..request
            },
            self.supports_minimax_reasoning_protocol(),
            self.supports_deepseek_protocol(),
            self.sidecar_protocol,
        )?;
        let mut headers = HeaderMap::new();
        headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
        let built = self
            .client
            .post(self.completion_url()?)
            .headers(headers)
            .json(&body)
            .build()?;
        let content_type = built
            .headers()
            .get(CONTENT_TYPE)
            .and_then(|value| value.to_str().ok())
            .map(str::to_owned);
        let body = built
            .body()
            .and_then(reqwest::Body::as_bytes)
            .ok_or(ProviderError::StreamSetup)?
            .to_vec();
        Ok((body, content_type))
    }

    fn initial_reasoning_details_mode(&self) -> ReasoningDetailsMode {
        if self.supports_minimax_reasoning_protocol() {
            ReasoningDetailsMode::Incremental
        } else {
            ReasoningDetailsMode::Unknown
        }
    }

    fn completion_url(&self) -> Result<Url, ProviderError> {
        #[cfg(any(test, feature = "test-support"))]
        if let Some(endpoint) = &self.test_completion_endpoint {
            return Ok(endpoint.join("chat/completions")?);
        }
        Ok(self.endpoint.join("chat/completions")?)
    }

    fn models_url(&self) -> Result<Url, ProviderError> {
        let endpoint = self.endpoint.join("models")?;
        if !matches!(endpoint.scheme(), "http" | "https") {
            return Err(ProviderError::UnsafeProbeEndpoint {
                reason: "only HTTP and HTTPS endpoints are supported",
            });
        }
        if endpoint.host_str().is_none() {
            return Err(ProviderError::UnsafeProbeEndpoint {
                reason: "the endpoint must include a host",
            });
        }
        if !endpoint.username().is_empty() || endpoint.password().is_some() {
            return Err(ProviderError::UnsafeProbeEndpoint {
                reason: "embedded URL credentials are not allowed",
            });
        }
        Ok(endpoint)
    }

    fn map_probe_request_error(&self, error: reqwest::Error) -> ProviderError {
        if error.is_timeout() {
            return ProviderError::ProbeTimeout {
                timeout_ms: duration_millis(self.probe_timeout),
            };
        }
        ProviderError::Http(error.without_url())
    }

    async fn read_probe_body(&self, response: reqwest::Response) -> Result<Vec<u8>, ProviderError> {
        if response
            .content_length()
            .is_some_and(|length| length > MAX_PROBE_BODY_BYTES as u64)
        {
            return Err(ProviderError::ProbeResponseTooLarge {
                limit_bytes: MAX_PROBE_BODY_BYTES,
            });
        }

        let mut body = Vec::new();
        let mut stream = response.bytes_stream();
        while let Some(chunk) = stream.next().await {
            let chunk = chunk.map_err(|error| self.map_probe_request_error(error))?;
            if body.len().saturating_add(chunk.len()) > MAX_PROBE_BODY_BYTES {
                return Err(ProviderError::ProbeResponseTooLarge {
                    limit_bytes: MAX_PROBE_BODY_BYTES,
                });
            }
            body.extend_from_slice(&chunk);
        }
        Ok(body)
    }

    async fn probe_openai(&self) -> Result<ProviderStatus, ProviderError> {
        let mut request = self
            .probe_client
            .get(self.models_url()?)
            .timeout(self.probe_timeout);
        if let Some(api_key) = &self.api_key {
            request = request.bearer_auth(api_key.expose());
        }

        let response = request
            .send()
            .await
            .map_err(|error| self.map_probe_request_error(error))?;
        let status = response.status();
        if !status.is_success() {
            return Err(ProviderError::ProbeHttpStatus {
                status: status.as_u16(),
            });
        }

        let body = self.read_probe_body(response).await?;
        let _ = parse_models_response(&body)?;

        let mut status = self.status();
        status.connected = true;
        status.detail = reachable_detail(&self.model);
        Ok(status)
    }

    async fn list_openai_models(&self) -> Result<Vec<ProviderModel>, ProviderError> {
        let mut request = self
            .probe_client
            .get(self.models_url()?)
            .timeout(self.probe_timeout);
        if let Some(api_key) = &self.api_key {
            request = request.bearer_auth(api_key.expose());
        }
        let response = request
            .send()
            .await
            .map_err(|error| self.map_probe_request_error(error))?;
        let status = response.status();
        if !status.is_success() {
            return Err(ProviderError::ProbeHttpStatus {
                status: status.as_u16(),
            });
        }
        parse_models_response(&self.read_probe_body(response).await?)
    }

    async fn stream_openai(
        &self,
        request: ModelRequest,
        sender: mpsc::Sender<ProviderEvent>,
    ) -> Result<(), ProviderError> {
        validate_model_tool_payload(&request)?;
        let selected_model = if request.model.is_empty() {
            self.model.clone()
        } else {
            request.model.clone()
        };
        let context_window_tokens = profile_id_for_endpoint(&self.endpoint)
            .and_then(|profile_id| canonical_context_window(profile_id, &selected_model));
        let body = build_chat_request_inner(
            ModelRequest {
                model: selected_model,
                ..request
            },
            self.supports_minimax_reasoning_protocol(),
            self.supports_deepseek_protocol(),
            self.sidecar_protocol,
        )?;
        let body = Bytes::from(
            serde_json::to_vec(&body)
                .map_err(|_| ProviderError::InvalidRequest("wire encoding failed"))?,
        );
        self.stream_openai_body(body, context_window_tokens, sender)
            .await
    }

    async fn stream_openai_body(
        &self,
        body: Bytes,
        context_window_tokens: Option<u64>,
        sender: mpsc::Sender<ProviderEvent>,
    ) -> Result<(), ProviderError> {
        let mut headers = HeaderMap::new();
        headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
        let mut request_builder = self
            .client
            .post(self.completion_url()?)
            .headers(headers)
            .body(body);
        if let Some(api_key) = &self.api_key {
            request_builder = request_builder.bearer_auth(api_key.expose());
        }
        let mut source =
            EventSource::new(request_builder).map_err(|_| ProviderError::StreamSetup)?;
        // reqwest-eventsource otherwise reconnects indefinitely. Retry ownership
        // belongs to the runtime, where accounting and cancellation are visible.
        source.set_retry_policy(Box::new(reqwest_eventsource::retry::Never));
        let mut response_opened = false;
        let mut saw_terminal_chunk = false;
        let mut response_truncated = false;
        let mut truncated_usage_frames = 0_usize;
        let mut reasoning_filter = TaggedReasoningFilter::new(
            self.hide_tagged_reasoning || self.supports_minimax_reasoning_protocol(),
        );
        let mut explicit_reasoning =
            ExplicitReasoningAccumulator::new(self.initial_reasoning_details_mode());
        let mut explicit_reasoning_tags = ReasoningTagFilter::default();
        let mut usage = usage_accounting::StreamUsage::default();
        let mut tool_calls = ToolCallAccumulator::default();
        let mut continuation = if self.supports_minimax_reasoning_protocol() {
            Some(minimax_continuation::Accumulator::new(
                self.endpoint.as_str(),
                &self.model,
                self.preparation_binding,
            )?)
        } else {
            None
        };
        while let Some(event) = source.next().await {
            match event {
                Ok(Event::Open) => {
                    response_opened = true;
                }
                Ok(Event::Message(message)) => {
                    if message.data.trim() == "[DONE]" {
                        break;
                    }
                    if message.data.len() > MAX_STREAM_EVENT_BYTES {
                        return Err(ProviderError::InvalidResponse(
                            "provider stream event exceeded the safety limit".into(),
                        ));
                    }
                    let chunk = parse_chat_completion_chunk(&message.data)?;
                    if response_truncated {
                        truncated_usage_frames += 1;
                        if truncated_usage_frames > MAX_TRUNCATED_USAGE_FRAMES {
                            return Err(ProviderError::InvalidResponse(
                                "terminal usage frames exceeded the safety limit".into(),
                            ));
                        }
                        if !chunk.choices.is_empty() {
                            return Err(ProviderError::InvalidResponse(
                                "provider returned completion data after truncation".into(),
                            ));
                        }
                    }
                    if continuation.is_some()
                        && chunk
                            .model
                            .as_deref()
                            .is_some_and(|model| model != self.model)
                    {
                        return Err(ProviderError::InvalidResponse(
                            "MiniMax response model binding changed".into(),
                        ));
                    }
                    if chunk.choices.len() > 1 {
                        return Err(ProviderError::InvalidResponse(
                            "provider returned multiple completion choices".into(),
                        ));
                    }
                    if let Some(chunk_usage) = chunk.usage {
                        let final_report = chunk.choices.is_empty()
                            || chunk
                                .choices
                                .iter()
                                .any(|choice| choice.finish_reason.is_some());
                        match usage.accept(chunk_usage, final_report) {
                            Ok(Some(observed)) => sender
                                .send(ProviderEvent::UsageObservation(observed))
                                .await
                                .map_err(|_| ProviderError::ConsumerClosed)?,
                            Ok(None) => {}
                            Err(error) => {
                                sender
                                    .send(ProviderEvent::UsageUncertain)
                                    .await
                                    .map_err(|_| ProviderError::ConsumerClosed)?;
                                return Err(error);
                            }
                        }
                    }
                    for choice in chunk.choices {
                        if let Some(state) = &mut continuation {
                            state.push(
                                chunk.id.as_deref(),
                                &choice.delta,
                                choice.finish_reason.as_deref(),
                            )?;
                        }
                        if let Some(finish_reason) = choice.finish_reason.as_deref() {
                            match finish_reason {
                                "stop" | "tool_calls" => saw_terminal_chunk = true,
                                "length" => {
                                    saw_terminal_chunk = true;
                                    response_truncated = true;
                                }
                                "content_filter" => {
                                    return Err(ProviderError::InvalidResponse(
                                        "provider response was filtered".into(),
                                    ));
                                }
                                _ => {
                                    return Err(ProviderError::InvalidResponse(
                                        "provider returned an unsupported finish reason".into(),
                                    ));
                                }
                            }
                        }
                        let reasoning = merge_explicit_reasoning(
                            choice.delta.reasoning_content.or(choice.delta.reasoning),
                            choice.delta.reasoning_details,
                        )?;
                        if let Some(reasoning) = reasoning {
                            let delta = match reasoning {
                                ReasoningFragment::Delta(delta) => {
                                    explicit_reasoning.push_delta(&delta)?
                                }
                                ReasoningFragment::Details(value) => {
                                    explicit_reasoning.push_details(&value)?
                                }
                            };
                            let delta = explicit_reasoning_tags.push(&delta);
                            if !delta.is_empty() {
                                sender
                                    .send(ProviderEvent::ReasoningDelta(delta))
                                    .await
                                    .map_err(|_| ProviderError::ConsumerClosed)?;
                            }
                        }
                        if let Some(content) = choice.delta.content {
                            let split = reasoning_filter.push(&content)?;
                            if !split.reasoning.is_empty() {
                                sender
                                    .send(ProviderEvent::ReasoningDelta(split.reasoning))
                                    .await
                                    .map_err(|_| ProviderError::ConsumerClosed)?;
                            }
                            if !split.visible.is_empty() {
                                sender
                                    .send(ProviderEvent::Delta(split.visible))
                                    .await
                                    .map_err(|_| ProviderError::ConsumerClosed)?;
                            }
                        }
                        tool_calls.push(choice.delta.tool_calls)?;
                    }
                }
                Err(reqwest_eventsource::Error::StreamEnded) if saw_terminal_chunk => break,
                Err(reqwest_eventsource::Error::InvalidStatusCode(status, response)) => {
                    let retry_after_ms = parse_retry_after(response.headers().get(RETRY_AFTER));
                    let safe_detail =
                        read_safe_provider_error_detail(response, self.api_key.as_ref()).await;
                    if matches!(status.as_u16(), 429 | 503)
                        || (status.as_u16() == 529 && self.supports_minimax_reasoning_protocol())
                    {
                        return Err(ProviderError::TransientHttpStatus {
                            status: status.as_u16(),
                            retry_after_ms,
                            safe_detail,
                        });
                    }
                    return Err(ProviderError::ApiHttpStatus {
                        status: status.as_u16(),
                        safe_detail,
                    });
                }
                Err(reqwest_eventsource::Error::Transport(_)) if !response_opened => {
                    return Err(ProviderError::PreDispatchConnection);
                }
                Err(error) => return Err(error.into()),
            }
        }
        if !saw_terminal_chunk {
            return Err(ProviderError::InvalidResponse(
                "provider stream ended without a terminal finish reason".into(),
            ));
        }
        if response_truncated {
            // A terminal length frame can precede the provider's usage-only
            // frame. Retain trustworthy accounting, but never complete partial
            // calls, publish reusable continuation state, or report success.
            if let Some(usage) = usage.complete_components(context_window_tokens) {
                sender
                    .send(ProviderEvent::Usage(usage))
                    .await
                    .map_err(|_| ProviderError::ConsumerClosed)?;
            }
            return Err(ProviderError::ResponseTruncated);
        }
        let trailing = reasoning_filter.finish()?;
        let explicit_trailing = explicit_reasoning_tags.finish()?;
        if !explicit_trailing.is_empty() {
            sender
                .send(ProviderEvent::ReasoningDelta(explicit_trailing))
                .await
                .map_err(|_| ProviderError::ConsumerClosed)?;
        }
        if !trailing.reasoning.is_empty() {
            sender
                .send(ProviderEvent::ReasoningDelta(trailing.reasoning))
                .await
                .map_err(|_| ProviderError::ConsumerClosed)?;
        }
        if !trailing.visible.is_empty() {
            sender
                .send(ProviderEvent::Delta(trailing.visible))
                .await
                .map_err(|_| ProviderError::ConsumerClosed)?;
        }
        let calls = tool_calls.finish()?;
        let completed_continuation = continuation.map(|state| state.finish(&calls)).transpose()?;
        for call in calls {
            sender
                .send(ProviderEvent::ToolCall(call))
                .await
                .map_err(|_| ProviderError::ConsumerClosed)?;
        }
        if let Some(usage) = usage.complete_components(context_window_tokens) {
            sender
                .send(ProviderEvent::Usage(usage))
                .await
                .map_err(|_| ProviderError::ConsumerClosed)?;
        }
        if let Some(state) = completed_continuation {
            sender
                .send(ProviderEvent::AssistantContinuation(Box::new(state)))
                .await
                .map_err(|_| ProviderError::ConsumerClosed)?;
        }
        sender
            .send(ProviderEvent::Completed)
            .await
            .map_err(|_| ProviderError::ConsumerClosed)
    }
}

fn is_minimax_continuation_origin(endpoint: &Url) -> bool {
    endpoint.scheme() == "https"
        && endpoint.host_str() == Some("api.minimax.io")
        && endpoint.port_or_known_default() == Some(443)
        && endpoint.path().trim_end_matches('/') == "/v1"
}

#[async_trait]
impl ModelProvider for OpenAiCompatibleProvider {
    fn prepare_request(
        &self,
        profile: &ProviderProfile,
        budget: &argentum_domain::context_budget::ModelBudget,
        request: ModelRequest,
    ) -> Result<PreparedProviderRequest, ProviderError> {
        self.prepare_compatible_request(profile, budget, request)
    }

    async fn validate_request(
        &self,
        request: &ModelRequest,
    ) -> Result<ProviderRequestValidation, ProviderError> {
        if self.sidecar_protocol {
            return self.validate_sidecar_request(request).await;
        }
        validate_model_tool_payload(request)?;
        Ok(ProviderRequestValidation {
            exact_input_tokens: None,
            context_tokens: None,
            output_limit: request.output_limit,
            representation_fingerprint: None,
            runtime_fingerprint: None,
        })
    }

    async fn validate_prepared_request(
        &self,
        prepared: &PreparedProviderRequest,
    ) -> Result<ProviderRequestValidation, ProviderError> {
        if self.sidecar_protocol {
            return self.validate_request(prepared.request()).await;
        }
        let _ = self.prepared_compatible_body(prepared)?;
        Ok(ProviderRequestValidation {
            exact_input_tokens: None,
            context_tokens: None,
            output_limit: prepared.request.output_limit,
            representation_fingerprint: None,
            runtime_fingerprint: None,
        })
    }
    fn id(&self) -> &'static str {
        match self.kind {
            ProviderKind::OpenAiCompatible => "openai-compatible",
            ProviderKind::Anthropic => "anthropic-compatible",
            ProviderKind::LocalLmStudio => "lm-studio",
            ProviderKind::LocalLlamaCpp => "llama-cpp",
            ProviderKind::Unknown => "unknown-compatible",
        }
    }

    fn status(&self) -> ProviderStatus {
        ProviderStatus {
            profile_id: self.id().into(),
            kind: self.kind,
            label: self.label.into(),
            endpoint: public_endpoint(&self.endpoint),
            connected: false,
            detail: configured_model_detail(&self.model),
        }
    }

    async fn probe(&self) -> Result<ProviderStatus, ProviderError> {
        self.probe_openai().await
    }

    async fn list_models(&self) -> Result<Vec<ProviderModel>, ProviderError> {
        self.list_openai_models().await
    }

    async fn stream(
        &self,
        request: ModelRequest,
        sender: mpsc::Sender<ProviderEvent>,
    ) -> Result<(), ProviderError> {
        self.stream_openai(request, sender).await
    }

    async fn stream_prepared_validated_with_cancellation(
        &self,
        prepared: &PreparedProviderRequest,
        _expected: &ProviderRequestValidation,
        sender: mpsc::Sender<ProviderEvent>,
        cancellation: CancellationToken,
    ) -> Result<(), ProviderError> {
        if self.sidecar_protocol {
            return self
                .stream_validated_with_cancellation(
                    prepared.request().clone(),
                    _expected,
                    sender,
                    cancellation,
                )
                .await;
        }
        let body = self.prepared_compatible_body(prepared)?;
        let context_window_tokens = profile_id_for_endpoint(&self.endpoint)
            .and_then(|profile_id| canonical_context_window(profile_id, &prepared.request.model));
        tokio::select! {
            _ = cancellation.cancelled() => Err(ProviderError::RequestCancelled),
            result = self.stream_openai_body(body, context_window_tokens, sender) => result,
        }
    }

    #[cfg(feature = "test-support")]
    fn build_prepared_transport_request_for_test(
        &self,
        prepared: &PreparedProviderRequest,
    ) -> Result<(Vec<u8>, Option<String>), ProviderError> {
        let body = self.prepared_compatible_body(prepared)?;
        let mut headers = HeaderMap::new();
        headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
        let built = self
            .client
            .post(self.completion_url()?)
            .headers(headers)
            .body(body)
            .build()?;
        let content_type = built
            .headers()
            .get(CONTENT_TYPE)
            .and_then(|value| value.to_str().ok())
            .map(str::to_owned);
        let body = built
            .body()
            .and_then(reqwest::Body::as_bytes)
            .ok_or(ProviderError::StreamSetup)?
            .to_vec();
        Ok((body, content_type))
    }

    #[cfg(feature = "test-support")]
    fn build_legacy_transport_request_for_test(
        &self,
        request: ModelRequest,
    ) -> Result<(Vec<u8>, Option<String>), ProviderError> {
        self.build_legacy_transport_request(request)
    }
}

pub struct LocalLmStudioProvider {
    inner: OpenAiCompatibleProvider,
}

impl LocalLmStudioProvider {
    pub fn new(endpoint: impl AsRef<str>, model: impl Into<String>) -> Result<Self, ProviderError> {
        Ok(Self {
            inner: OpenAiCompatibleProvider::new(
                endpoint,
                model,
                None,
                ProviderKind::LocalLmStudio,
                "LM Studio",
            )?,
        })
    }

    fn native_models_url(&self) -> Result<Url, ProviderError> {
        let mut endpoint = self.inner.endpoint.clone();
        endpoint.set_path("/api/v1/models");
        endpoint.set_query(None);
        endpoint.set_fragment(None);
        if !matches!(endpoint.scheme(), "http" | "https") || endpoint.host_str().is_none() {
            return Err(ProviderError::UnsafeProbeEndpoint {
                reason: "the LM Studio endpoint must use HTTP or HTTPS with a host",
            });
        }
        Ok(endpoint)
    }

    async fn list_native_models(&self) -> Result<Vec<ProviderModel>, ProviderError> {
        let response = match self
            .inner
            .probe_client
            .get(self.native_models_url()?)
            .timeout(self.inner.probe_timeout)
            .send()
            .await
        {
            Ok(response) if response.status().is_success() => response,
            Ok(_) | Err(_) => return self.inner.list_openai_models().await,
        };
        let body = self.inner.read_probe_body(response).await?;
        match parse_lm_studio_models_response(&body) {
            Ok(models) => Ok(models),
            Err(ProviderError::InvalidResponse(_)) => self.inner.list_openai_models().await,
            Err(error) => Err(error),
        }
    }
}

#[async_trait]
impl ModelProvider for LocalLmStudioProvider {
    fn prepare_request(
        &self,
        profile: &ProviderProfile,
        budget: &argentum_domain::context_budget::ModelBudget,
        request: ModelRequest,
    ) -> Result<PreparedProviderRequest, ProviderError> {
        self.inner.prepare_request(profile, budget, request)
    }

    async fn validate_prepared_request(
        &self,
        prepared: &PreparedProviderRequest,
    ) -> Result<ProviderRequestValidation, ProviderError> {
        self.inner.validate_prepared_request(prepared).await
    }

    fn id(&self) -> &'static str {
        self.inner.id()
    }

    fn status(&self) -> ProviderStatus {
        self.inner.status()
    }

    async fn probe(&self) -> Result<ProviderStatus, ProviderError> {
        self.inner.probe().await
    }

    async fn list_models(&self) -> Result<Vec<ProviderModel>, ProviderError> {
        self.list_native_models().await
    }

    async fn stream(
        &self,
        request: ModelRequest,
        sender: mpsc::Sender<ProviderEvent>,
    ) -> Result<(), ProviderError> {
        self.inner.stream(request, sender).await
    }

    async fn stream_prepared_validated_with_cancellation(
        &self,
        prepared: &PreparedProviderRequest,
        expected: &ProviderRequestValidation,
        sender: mpsc::Sender<ProviderEvent>,
        cancellation: CancellationToken,
    ) -> Result<(), ProviderError> {
        self.inner
            .stream_prepared_validated_with_cancellation(prepared, expected, sender, cancellation)
            .await
    }

    #[cfg(feature = "test-support")]
    fn build_prepared_transport_request_for_test(
        &self,
        prepared: &PreparedProviderRequest,
    ) -> Result<(Vec<u8>, Option<String>), ProviderError> {
        self.inner
            .build_prepared_transport_request_for_test(prepared)
    }

    #[cfg(feature = "test-support")]
    fn build_legacy_transport_request_for_test(
        &self,
        request: ModelRequest,
    ) -> Result<(Vec<u8>, Option<String>), ProviderError> {
        self.inner.build_legacy_transport_request(request)
    }
}

pub struct AnthropicProvider {
    client: reqwest::Client,
    endpoint: Url,
    model: String,
    api_key: SecretValue,
}

impl AnthropicProvider {
    pub fn new(
        endpoint: impl AsRef<str>,
        model: impl Into<String>,
        api_key: SecretValue,
    ) -> Result<Self, ProviderError> {
        Ok(Self {
            client: reqwest::Client::builder().build()?,
            endpoint: Url::parse(endpoint.as_ref())?,
            model: model.into(),
            api_key,
        })
    }
}

#[async_trait]
impl ModelProvider for AnthropicProvider {
    fn id(&self) -> &'static str {
        "anthropic"
    }

    fn status(&self) -> ProviderStatus {
        ProviderStatus {
            profile_id: self.id().into(),
            kind: ProviderKind::Anthropic,
            label: "Anthropic".into(),
            endpoint: public_endpoint(&self.endpoint),
            connected: false,
            detail: configured_model_detail(&self.model),
        }
    }

    async fn probe(&self) -> Result<ProviderStatus, ProviderError> {
        Err(ProviderError::ProbeUnsupported {
            provider: self.id(),
        })
    }

    async fn stream(
        &self,
        request: ModelRequest,
        sender: mpsc::Sender<ProviderEvent>,
    ) -> Result<(), ProviderError> {
        let model = if request.model.is_empty() {
            self.model.clone()
        } else {
            request.model.clone()
        };
        let body = build_anthropic_request(ModelRequest { model, ..request })?;
        let response = self
            .client
            .post(self.endpoint.clone())
            .header("x-api-key", self.api_key.expose())
            .header("anthropic-version", "2023-06-01")
            .json(&body)
            .send()
            .await?;
        let status = response.status();
        if !status.is_success() {
            return Err(ProviderError::ApiHttpStatus {
                status: status.as_u16(),
                safe_detail: String::new(),
            });
        }
        let payload: serde_json::Value = response.json().await?;
        let parsed: AnthropicResponse = serde_json::from_value(payload)
            .map_err(|error| ProviderError::InvalidResponse(error.to_string()))?;
        let text = parsed
            .content
            .into_iter()
            .find_map(|block| (block.kind == "text").then_some(block.text))
            .ok_or_else(|| {
                ProviderError::InvalidResponse("response did not contain text".into())
            })?;
        sender
            .send(ProviderEvent::Delta(text))
            .await
            .map_err(|_| ProviderError::ConsumerClosed)?;
        sender
            .send(ProviderEvent::Completed)
            .await
            .map_err(|_| ProviderError::ConsumerClosed)
    }
}

#[derive(Clone, Default)]
pub struct ProviderRegistry {
    providers: Arc<BTreeMap<String, Arc<dyn ModelProvider>>>,
    credentials: ProviderCredentials,
    chatgpt_auth: Option<chatgpt_auth::ChatGptAuth>,
    local_runtime: Option<Arc<LocalRuntimeModule>>,
    android_inference: Option<Arc<AsyncMutex<argentum_android_inference::AndroidInferenceModule>>>,
    local_backend: local_backend::LocalBackendSlot,
    backend_changes: Arc<AsyncMutex<()>>,
    android_foreground_stop: Arc<RwLock<Option<AndroidForegroundStop>>>,
}

impl std::fmt::Debug for ProviderRegistry {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ProviderRegistry")
            .field("providers", &self.providers.keys().collect::<Vec<_>>())
            .field("credentials", &self.credentials)
            .field("local_runtime_configured", &self.local_backend.configured())
            .finish()
    }
}

impl ProviderRegistry {
    fn annotate_capability_status(
        &self,
        profile: &ProviderProfile,
        mut status: ProviderStatus,
    ) -> ProviderStatus {
        if let Ok(resolution) = self.resolve_capabilities(profile) {
            let coding = if resolution.supports_coding() {
                "supported by the configured adapter; runtime validation is required at dispatch"
            } else {
                resolution.detail
            };
            if !status.detail.is_empty() {
                status.detail.push_str("; ");
            }
            status.detail.push_str("Coding workflow: ");
            status.detail.push_str(coding);
        }
        status
    }

    /// Resolve configured adapter capabilities without contacting a provider or
    /// starting/loading a local backend. Dispatch performs the runtime checks.
    pub fn resolve_capabilities(
        &self,
        profile: &ProviderProfile,
    ) -> Result<ProviderCapabilityResolution, ProviderError> {
        let profile = normalize_provider_profile(profile.clone())?;
        Ok(capabilities::resolve_provider_capabilities(
            &profile,
            self.local_backend.identity(),
        ))
    }

    pub fn cached_local_model_identity(
        &self,
        profile: &ProviderProfile,
    ) -> Option<(String, String)> {
        if profile.kind != ProviderKind::LocalLlamaCpp
            || self.local_backend.identity().backend_id != Some("llama.cpp.sidecar")
        {
            return None;
        }
        self.local_runtime
            .as_ref()?
            .cached_model_identity(&profile.model)
    }

    pub fn account_request(
        &self,
        profile: &ProviderProfile,
        budget: &argentum_domain::context_budget::ModelBudget,
        request: &ModelRequest,
    ) -> Result<argentum_domain::context_budget::RequestAccounting, ProviderError> {
        let resolution = self.resolve_capabilities(profile)?;
        if resolution.capabilities.context_accounting != CapabilitySupport::Supported {
            return Err(ProviderError::ContextAccountingUnsupported(
                resolution.detail,
            ));
        }
        if resolution.identity.local_backend_id == Some("llama.cpp.sidecar") {
            return sidecar_protocol::account_request(profile, budget, request);
        }
        if profile.id == "chatgpt" {
            return chatgpt::ChatGptProvider::account_request(profile, budget, request);
        }
        account_chat_request(profile, budget, request)
    }

    pub fn prepare_request(
        &self,
        provider: &dyn ModelProvider,
        profile: &ProviderProfile,
        budget: &argentum_domain::context_budget::ModelBudget,
        request: ModelRequest,
    ) -> Result<PreparedProviderRequest, ProviderError> {
        let profile = normalize_provider_profile(profile.clone())?;
        let resolution = self.resolve_capabilities(&profile)?;
        if resolution.capabilities.context_accounting != CapabilitySupport::Supported {
            return Err(ProviderError::ContextAccountingUnsupported(
                resolution.detail,
            ));
        }
        if resolution.identity.local_backend_id == Some("llama.cpp.sidecar") {
            let accounting = sidecar_protocol::account_request(&profile, budget, &request)?;
            return Ok(PreparedProviderRequest::raw(request, accounting));
        }
        provider.prepare_request(&profile, budget, request)
    }
    pub fn with_credentials(credentials: ProviderCredentials) -> Self {
        Self {
            providers: Arc::default(),
            credentials,
            chatgpt_auth: None,
            local_runtime: None,
            android_inference: None,
            local_backend: local_backend::LocalBackendSlot::default(),
            backend_changes: Arc::default(),
            android_foreground_stop: Arc::default(),
        }
    }

    pub fn with_local_runtime(mut self, runtime: Arc<LocalRuntimeModule>) -> Self {
        self.local_runtime = Some(runtime);
        self.configure_builtin_backend();
        self
    }

    pub fn with_chatgpt_auth(
        mut self,
        store: Arc<dyn argentum_platform::SecretStore>,
    ) -> Result<Self, ProviderError> {
        self.chatgpt_auth = Some(chatgpt_auth::ChatGptAuth::new(store).map_err(|_| {
            ProviderError::CredentialsRequired {
                provider_id: "chatgpt".into(),
            }
        })?);
        Ok(self)
    }

    pub fn with_android_inference(
        mut self,
        runtime: Arc<AsyncMutex<argentum_android_inference::AndroidInferenceModule>>,
    ) -> Self {
        self.android_inference = Some(runtime);
        self.configure_builtin_backend();
        self
    }

    pub fn local_runtime_configured(&self) -> bool {
        self.local_backend.configured()
    }

    pub fn local_backend_status(&self) -> LocalBackendStatus {
        self.local_backend.status()
    }

    pub fn set_android_foreground_stop(
        &self,
        stop: AndroidForegroundStop,
    ) -> Result<(), ProviderError> {
        *self
            .android_foreground_stop
            .write()
            .map_err(|_| ProviderError::LocalRuntimeLifecycleFailed)? = Some(stop);
        Ok(())
    }

    pub fn builtin_local_backend_id(&self) -> Option<&'static str> {
        if self.android_inference.is_some() {
            Some("llama.cpp.android")
        } else if self.local_runtime.is_some() {
            Some("llama.cpp.sidecar")
        } else {
            None
        }
    }

    pub async fn local_backend_lease(&self, id: &str) -> Result<LocalBackendLease, ProviderError> {
        self.local_backend.lease(id).await
    }

    pub async fn set_builtin_backend_enabled(&self, enabled: bool) -> Result<(), ProviderError> {
        let _change = self.backend_changes.lock().await;
        if let Some(runtime) = &self.android_inference {
            let status = self.local_backend.status();
            let worker_started = runtime.lock().await.is_running();
            if status.configured == enabled
                && !status.recovery_pending
                && !status.changing
                && (!enabled || worker_started)
            {
                return Ok(());
            }
            if !enabled {
                return self
                    .local_backend
                    .replace(None, CancellationToken::new())
                    .await;
            }
            #[cfg(not(target_os = "android"))]
            {
                let _ = runtime;
                return Err(ProviderError::LocalCapabilityUnsupported {
                    capability: "Android worker restart on this target",
                });
            }
            #[cfg(target_os = "android")]
            {
                self.local_backend
                    .shutdown(CancellationToken::new())
                    .await?;
                runtime
                    .lock()
                    .await
                    .restart()
                    .map_err(android_local::map_inference_error)?;
                let replacement = Arc::new(AndroidLlamaCppBackend {
                    runtime: runtime.clone(),
                    sidecar: self.local_runtime.clone(),
                    foreground_stop: self.android_foreground_stop.clone(),
                });
                if let Err(error) = self
                    .local_backend
                    .replace(Some(replacement), CancellationToken::new())
                    .await
                {
                    runtime
                        .lock()
                        .await
                        .shutdown()
                        .await
                        .map_err(android_local::map_inference_error)?;
                    return Err(error);
                }
                return Ok(());
            }
        }
        let runtime = self
            .local_runtime
            .as_ref()
            .ok_or(ProviderError::ProviderNotConfigured {
                provider_id: "local-backend".into(),
            })?;
        let status = self.local_backend.status();
        if status.configured == enabled && !status.recovery_pending && !status.changing {
            return Ok(());
        }
        let replacement: Option<Arc<dyn LocalInferenceBackend>> = if enabled {
            Some(Arc::new(LlamaCppSidecarBackend(runtime.clone())))
        } else {
            None
        };
        self.local_backend
            .replace(replacement, CancellationToken::new())
            .await
    }

    fn configure_builtin_backend(&mut self) {
        if let Some(runtime) = &self.android_inference {
            self.local_backend =
                local_backend::LocalBackendSlot::new(Arc::new(AndroidLlamaCppBackend {
                    runtime: runtime.clone(),
                    sidecar: self.local_runtime.clone(),
                    foreground_stop: self.android_foreground_stop.clone(),
                }));
        } else if let Some(runtime) = &self.local_runtime {
            self.local_backend = local_backend::LocalBackendSlot::new(Arc::new(
                LlamaCppSidecarBackend(runtime.clone()),
            ));
        }
    }

    /// Configure a trusted backend before sharing the registry with a host.
    pub fn with_local_backend(mut self, backend: Arc<dyn LocalInferenceBackend>) -> Self {
        self.local_runtime = None;
        self.android_inference = None;
        self.local_backend = local_backend::LocalBackendSlot::new(backend);
        self
    }

    /// Drain active calls, retire the old backend and publish its replacement.
    /// All registry clones share this slot. This does not install or activate artifacts.
    /// Supply a dormant replacement; its caller retains ownership until accepted.
    /// Drive this future to completion so shutdown can finish or retain recovery state.
    pub async fn replace_local_backend(
        &self,
        backend: Option<Arc<dyn LocalInferenceBackend>>,
        cancellation: CancellationToken,
    ) -> Result<(), ProviderError> {
        let _change = tokio::select! {
            biased;
            _ = cancellation.cancelled() => return Err(ProviderError::RequestCancelled),
            guard = self.backend_changes.lock() => guard,
        };
        self.local_backend.replace(backend, cancellation).await
    }

    pub async fn shutdown(&self, cancellation: CancellationToken) -> Result<(), ProviderError> {
        let _change = self.backend_changes.lock().await;
        self.local_backend.shutdown(cancellation).await
    }

    pub fn set_credential(
        &self,
        profile_id: impl AsRef<str>,
        api_key: SecretValue,
    ) -> Result<(), ProviderError> {
        self.credentials.insert(profile_id, api_key)
    }

    pub fn clear_credential(&self, profile_id: &str) -> bool {
        self.credentials.remove(profile_id)
    }

    pub fn credential_configured(&self, profile_id: &str) -> bool {
        if profile_id == "chatgpt" {
            return self
                .chatgpt_auth
                .as_ref()
                .is_some_and(|auth| auth.configured());
        }
        self.credentials.contains_profile(profile_id)
    }

    pub fn register<P>(&mut self, provider: P)
    where
        P: ModelProvider + 'static,
    {
        let mut providers = (*self.providers).clone();
        providers.insert(provider.id().to_owned(), Arc::new(provider));
        self.providers = Arc::new(providers);
    }

    pub fn statuses(&self) -> Vec<ProviderStatus> {
        self.providers
            .values()
            .map(|provider| sanitize_provider_status(provider.status()))
            .collect()
    }

    pub fn get(&self, id: &str) -> Option<Arc<dyn ModelProvider>> {
        self.providers.get(id).cloned()
    }

    pub fn status(&self, id: &str) -> Option<ProviderStatus> {
        self.providers
            .get(id)
            .map(|provider| sanitize_provider_status(provider.status()))
    }

    pub async fn probe(&self, id: &str) -> Result<ProviderStatus, ProviderError> {
        let provider =
            self.providers
                .get(id)
                .ok_or_else(|| ProviderError::ProviderNotConfigured {
                    provider_id: display_provider_id(id),
                })?;
        provider.probe().await.map(sanitize_provider_status)
    }

    pub fn provider_for_profile(
        &self,
        profile: &ProviderProfile,
    ) -> Result<Arc<dyn ModelProvider>, ProviderError> {
        let profile = normalize_provider_profile(profile.clone())?;
        if profile.id == "chatgpt" {
            return Ok(Arc::new(chatgpt::ChatGptProvider::new(
                &profile,
                self.chatgpt_auth
                    .clone()
                    .ok_or_else(|| ProviderError::CredentialsRequired {
                        provider_id: "chatgpt".into(),
                    })?,
            )?));
        }
        match profile.kind {
            ProviderKind::LocalLlamaCpp => match self.get(&profile.id) {
                Some(provider) => Ok(provider),
                None => self.local_backend.bind(&profile),
            },
            ProviderKind::LocalLmStudio => {
                if profile.id == "lm-studio" {
                    ProviderPreset::LmStudio.build_provider(&profile, None)
                } else {
                    Ok(Arc::new(LocalLmStudioProvider::new(
                        &profile.endpoint,
                        &profile.model,
                    )?))
                }
            }
            ProviderKind::OpenAiCompatible => match self.get(&profile.id) {
                Some(provider) => Ok(provider),
                None => {
                    if let Some(preset) = ProviderPreset::parse(&profile.id) {
                        return preset
                            .build_provider(&profile, self.credentials.for_profile(&profile.id));
                    }
                    let api_key = self.credentials.for_profile(&profile.id);
                    if is_canonical_hosted_profile_id(&profile.id) && api_key.is_none() {
                        return Err(ProviderError::CredentialsRequired {
                            provider_id: profile.id,
                        });
                    }
                    if api_key.is_some() && is_canonical_hosted_profile_id(&profile.id) {
                        validate_hosted_credential_origin(&profile)?;
                    }
                    let hide_tagged_reasoning =
                        matches!(profile.id.as_str(), "minimax" | "deepseek");
                    Ok(Arc::new(
                        OpenAiCompatibleProvider::new(
                            &profile.endpoint,
                            &profile.model,
                            api_key,
                            ProviderKind::OpenAiCompatible,
                            "OpenAI compatible",
                        )?
                        .with_hidden_tagged_reasoning(hide_tagged_reasoning),
                    ))
                }
            },
            ProviderKind::Anthropic => {
                self.get(&profile.id)
                    .ok_or_else(|| ProviderError::CredentialsRequired {
                        provider_id: profile.id,
                    })
            }
            ProviderKind::Unknown => {
                self.get(&profile.id)
                    .ok_or_else(|| ProviderError::UnsupportedProviderKind {
                        provider_id: profile.id,
                    })
            }
        }
    }

    pub fn status_for_profile(
        &self,
        profile: &ProviderProfile,
    ) -> Result<ProviderStatus, ProviderError> {
        let profile = normalize_provider_profile(profile.clone())?;
        if !matches!(
            profile.kind,
            ProviderKind::LocalLmStudio | ProviderKind::LocalLlamaCpp
        ) {
            if let Some(provider) = self.get(&profile.id) {
                let mut status = sanitize_provider_status(provider.status());
                status.profile_id = profile.id.clone();
                status.kind = profile.kind;
                status.label = profile.label.clone();
                status.endpoint = profile.endpoint.clone();
                return Ok(self.annotate_capability_status(&profile, status));
            }
        }
        if profile.kind == ProviderKind::LocalLlamaCpp {
            let provider = self.provider_for_profile(&profile)?;
            let mut status = sanitize_provider_status(provider.status());
            status.profile_id = profile.id.clone();
            status.kind = profile.kind;
            status.label = profile.label.clone();
            status.endpoint = profile.endpoint.clone();
            return Ok(self.annotate_capability_status(&profile, status));
        }
        let detail = match profile.kind {
            ProviderKind::OpenAiCompatible
                if profile.id == "chatgpt" && !self.credential_configured("chatgpt") =>
            {
                "ChatGPT browser sign-in required; no API key".into()
            }
            ProviderKind::Anthropic => "Credentials required; profile saved".into(),
            ProviderKind::OpenAiCompatible
                if is_canonical_hosted_profile_id(&profile.id)
                    && !self.credentials.contains_profile(&profile.id) =>
            {
                "Credentials required; profile saved".into()
            }
            ProviderKind::Unknown => "Provider type unavailable".into(),
            _ => configured_model_detail(&profile.model),
        };
        Ok(self.annotate_capability_status(
            &profile,
            ProviderStatus {
                profile_id: profile.id.clone(),
                kind: profile.kind,
                label: profile.label.clone(),
                endpoint: profile.endpoint.clone(),
                connected: false,
                detail,
            },
        ))
    }

    pub async fn probe_profile(
        &self,
        profile: &ProviderProfile,
    ) -> Result<ProviderStatus, ProviderError> {
        let profile = normalize_provider_profile(profile.clone())?;
        let provider = self.provider_for_profile(&profile)?;
        let mut status = sanitize_provider_status(provider.probe().await?);
        status.kind = profile.kind;
        status.profile_id = profile.id.clone();
        status.label = profile.label.clone();
        status.endpoint = profile.endpoint.clone();
        Ok(self.annotate_capability_status(&profile, status))
    }

    pub async fn list_models_for_profile(
        &self,
        profile: &ProviderProfile,
    ) -> Result<Vec<ProviderModel>, ProviderError> {
        let profile = normalize_provider_profile(profile.clone())?;
        let provider = self.provider_for_profile(&profile)?;
        let raw = provider.list_models().await?;
        let mut models = if profile.id == "chatgpt" {
            if raw.len() > MAX_PROBE_MODELS {
                return Err(ProviderError::InvalidResponse(
                    "ChatGPT catalogue exceeded the safety limit".into(),
                ));
            }
            let mut seen = std::collections::BTreeSet::new();
            let mut ordered = Vec::new();
            for model in raw {
                for model in sanitize_provider_models(vec![model])? {
                    if seen.insert(model.id.clone()) {
                        ordered.push(model);
                    }
                }
            }
            ordered
        } else {
            sanitize_provider_models(raw)?
        };
        if profile.id == "openai" {
            models.retain(|model| is_supported_openai_chat_model(&model.id));
            if models.is_empty() {
                return Err(ProviderError::InvalidResponse(
                    "OpenAI catalog contained no supported chat models".into(),
                ));
            }
        }
        for model in &mut models {
            model.context_window_tokens =
                canonical_context_window(&profile.id, &model.id).or(model.context_window_tokens);
        }
        Ok(models)
    }

    pub fn first(&self) -> Option<Arc<dyn ModelProvider>> {
        self.providers.values().next().cloned()
    }
}

#[async_trait]
impl AuthorityDependentService for ProviderRegistry {
    fn service_id(&self) -> &str {
        "provider.local-runtime"
    }

    async fn retire(&mut self, cancellation: &CancellationToken) -> Result<(), PlatformError> {
        self.shutdown(cancellation.clone()).await.map_err(|_| {
            PlatformError::AuthorityDependentCleanup {
                service_id: self.service_id().into(),
            }
        })
    }
}

#[derive(Clone, Default)]
pub struct ProviderCredentials {
    api_keys: Arc<RwLock<BTreeMap<String, SecretValue>>>,
}

impl std::fmt::Debug for ProviderCredentials {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ProviderCredentials")
            .field(
                "profile_ids",
                &self
                    .api_keys
                    .read()
                    .map(|keys| keys.keys().cloned().collect::<Vec<_>>())
                    .unwrap_or_default(),
            )
            .finish()
    }
}

impl ProviderCredentials {
    pub fn from_profile_api_keys(
        values: impl IntoIterator<Item = (String, SecretValue)>,
    ) -> Result<Self, ProviderError> {
        let credentials = Self::default();
        for (profile_id, api_key) in values {
            credentials.insert(profile_id, api_key)?;
        }
        Ok(credentials)
    }

    pub fn insert(
        &self,
        profile_id: impl AsRef<str>,
        api_key: SecretValue,
    ) -> Result<(), ProviderError> {
        let profile_id = normalize_profile_id(profile_id.as_ref())?;
        if api_key.expose().trim().is_empty() {
            return Err(ProviderError::CredentialsRequired {
                provider_id: profile_id,
            });
        }
        self.api_keys
            .write()
            .map_err(|_| ProviderError::InvalidResponse("credential state is unavailable".into()))?
            .insert(profile_id, api_key);
        Ok(())
    }

    pub fn contains_profile(&self, profile_id: &str) -> bool {
        self.api_keys
            .read()
            .map(|keys| keys.contains_key(profile_id))
            .unwrap_or(false)
    }

    fn for_profile(&self, profile_id: &str) -> Option<SecretValue> {
        self.api_keys
            .read()
            .ok()
            .and_then(|keys| keys.get(profile_id).cloned())
    }

    pub fn remove(&self, profile_id: &str) -> bool {
        self.api_keys
            .write()
            .ok()
            .and_then(|mut keys| keys.remove(profile_id))
            .is_some()
    }
}

fn is_canonical_hosted_profile_id(profile_id: &str) -> bool {
    matches!(profile_id, "openai" | "minimax" | "deepseek")
}

fn validate_hosted_credential_origin(profile: &ProviderProfile) -> Result<(), ProviderError> {
    let endpoint = Url::parse(&profile.endpoint).map_err(|_| ProviderError::InvalidProfile {
        field: "endpoint",
        reason: "use a valid HTTP or HTTPS URL",
    })?;
    let expected_host = match profile.id.as_str() {
        "openai" => "api.openai.com",
        "minimax" => "api.minimax.io",
        "deepseek" => "api.deepseek.com",
        _ => return Ok(()),
    };
    if endpoint.scheme() != "https"
        || endpoint.host_str() != Some(expected_host)
        || endpoint.port().is_some()
    {
        return Err(ProviderError::UnsafeHostedEndpoint {
            provider_id: profile.id.clone(),
        });
    }
    Ok(())
}

fn parse_retry_after(value: Option<&HeaderValue>) -> Option<u64> {
    let value = value?.to_str().ok()?.trim();
    if value.is_empty() || value.len() > 10 {
        return None;
    }
    value
        .parse::<u64>()
        .ok()
        .and_then(|seconds| seconds.checked_mul(1_000))
}

async fn read_safe_provider_error_detail(
    response: reqwest::Response,
    api_key: Option<&SecretValue>,
) -> String {
    if response
        .content_length()
        .is_some_and(|length| length > MAX_PROVIDER_ERROR_BODY_BYTES as u64)
    {
        return String::new();
    }
    let mut body = Vec::new();
    let mut stream = response.bytes_stream();
    while let Some(chunk) = stream.next().await {
        let Ok(chunk) = chunk else {
            return String::new();
        };
        if body.len().saturating_add(chunk.len()) > MAX_PROVIDER_ERROR_BODY_BYTES {
            return String::new();
        }
        body.extend_from_slice(&chunk);
    }
    safe_provider_error_detail(&body, api_key)
}

fn safe_provider_error_detail(body: &[u8], api_key: Option<&SecretValue>) -> String {
    let Ok(value) = serde_json::from_slice::<serde_json::Value>(body) else {
        return String::new();
    };
    let Some(root) = value.as_object() else {
        return String::new();
    };
    let nested = root
        .get("error")
        .and_then(serde_json::Value::as_object)
        .or_else(|| root.get("base_resp").and_then(serde_json::Value::as_object));

    let code = first_provider_error_value(nested, root, &["code", "status_code", "error_code"])
        .and_then(|value| bounded_provider_identifier(value, MAX_PROVIDER_ERROR_CODE_BYTES));
    let message =
        first_provider_error_value(nested, root, &["message", "msg", "status_msg", "error_msg"])
            .or_else(|| root.get("error").filter(|value| value.is_string()))
            .and_then(serde_json::Value::as_str)
            .and_then(|value| {
                bounded_provider_message(value, api_key, MAX_PROVIDER_ERROR_MESSAGE_BYTES)
            });
    let request_id =
        first_provider_error_value(nested, root, &["request_id", "trace_id", "requestId"])
            .and_then(|value| {
                bounded_provider_identifier(value, MAX_PROVIDER_ERROR_REQUEST_ID_BYTES)
            });

    let mut fields = Vec::new();
    if let Some(code) = code {
        fields.push(format!("code={code}"));
    }
    if let Some(message) = message {
        fields.push(format!("message={message}"));
    }
    if let Some(request_id) = request_id {
        fields.push(format!("request_id={request_id}"));
    }
    if fields.is_empty() {
        String::new()
    } else {
        format!("; provider detail: {}", fields.join(", "))
    }
}

fn first_provider_error_value<'a>(
    nested: Option<&'a serde_json::Map<String, serde_json::Value>>,
    root: &'a serde_json::Map<String, serde_json::Value>,
    keys: &[&str],
) -> Option<&'a serde_json::Value> {
    keys.iter()
        .find_map(|key| nested.and_then(|object| object.get(*key)))
        .or_else(|| keys.iter().find_map(|key| root.get(*key)))
}

fn bounded_provider_identifier(value: &serde_json::Value, limit: usize) -> Option<String> {
    let value = match value {
        serde_json::Value::String(value) => value.trim().to_owned(),
        serde_json::Value::Number(value) => value.to_string(),
        _ => return None,
    };
    if value.is_empty()
        || value.len() > limit
        || !value.chars().all(|character| {
            character.is_ascii_alphanumeric() || matches!(character, '-' | '_' | '.' | ':')
        })
    {
        return None;
    }
    Some(value)
}

fn bounded_provider_message(
    value: &str,
    api_key: Option<&SecretValue>,
    limit: usize,
) -> Option<String> {
    let redacted = api_key
        .filter(|secret| !secret.expose().is_empty())
        .map_or_else(
            || value.to_owned(),
            |secret| value.replace(secret.expose(), "[REDACTED]"),
        );
    let mut normalized = String::new();
    let mut pending_space = false;
    for character in redacted.chars() {
        if character.is_control() || character.is_whitespace() {
            pending_space = !normalized.is_empty();
            continue;
        }
        if pending_space {
            normalized.push(' ');
            pending_space = false;
        }
        normalized.push(character);
    }
    let normalized = normalized.trim();
    if normalized.is_empty() {
        return None;
    }
    if normalized.len() <= limit {
        return Some(normalized.to_owned());
    }
    let mut end = limit.saturating_sub(3).min(normalized.len());
    while !normalized.is_char_boundary(end) {
        end = end.saturating_sub(1);
    }
    Some(format!("{}...", &normalized[..end]))
}

fn normalize_profile_id(profile_id: &str) -> Result<String, ProviderError> {
    let profile_id = profile_id.trim().to_owned();
    if profile_id.is_empty()
        || profile_id.len() > 64
        || !profile_id.chars().all(|character| {
            character.is_ascii_alphanumeric() || matches!(character, '-' | '_' | '.')
        })
    {
        return Err(ProviderError::InvalidProfile {
            field: "id",
            reason: "use 1 to 64 ASCII letters, digits, dots, hyphens, or underscores",
        });
    }
    Ok(profile_id)
}

pub fn normalize_provider_profile(
    mut profile: ProviderProfile,
) -> Result<ProviderProfile, ProviderError> {
    profile.id = normalize_profile_id(&profile.id)?;

    profile.label = profile.label.trim().to_owned();
    if profile.label.is_empty()
        || profile.label.chars().count() > 80
        || profile.label.chars().any(char::is_control)
    {
        return Err(ProviderError::InvalidProfile {
            field: "label",
            reason: "use 1 to 80 printable characters",
        });
    }

    profile.model = profile.model.trim().to_owned();
    if profile.model.is_empty()
        || profile.model.chars().count() > 256
        || profile.model.chars().any(char::is_control)
    {
        return Err(ProviderError::InvalidProfile {
            field: "model",
            reason: "use 1 to 256 printable characters",
        });
    }

    if profile.kind == ProviderKind::Unknown {
        return Err(ProviderError::InvalidProfile {
            field: "kind",
            reason: "select llama.cpp, OpenAI compatible, or Anthropic",
        });
    }

    if profile.endpoint.len() > 2_048 {
        return Err(ProviderError::InvalidProfile {
            field: "endpoint",
            reason: "must not exceed 2048 bytes",
        });
    }
    let mut endpoint =
        Url::parse(profile.endpoint.trim()).map_err(|_| ProviderError::InvalidProfile {
            field: "endpoint",
            reason: "use a valid HTTP or HTTPS URL",
        })?;
    let valid_local_endpoint = profile.kind == ProviderKind::LocalLlamaCpp
        && endpoint.scheme() == "local"
        && endpoint.host_str() == Some("llama-cpp")
        && endpoint.path() == "/";
    let valid_http_endpoint = profile.kind != ProviderKind::LocalLlamaCpp
        && matches!(endpoint.scheme(), "http" | "https")
        && endpoint.host_str().is_some();
    if !valid_local_endpoint && !valid_http_endpoint {
        return Err(ProviderError::InvalidProfile {
            field: "endpoint",
            reason: "use the local llama.cpp URI or an HTTP or HTTPS URL with a host",
        });
    }
    if !endpoint.username().is_empty() || endpoint.password().is_some() {
        return Err(ProviderError::InvalidProfile {
            field: "endpoint",
            reason: "URL credentials are not allowed",
        });
    }
    if endpoint.query().is_some() || endpoint.fragment().is_some() {
        return Err(ProviderError::InvalidProfile {
            field: "endpoint",
            reason: "query strings and fragments are not allowed",
        });
    }
    if !endpoint.path().ends_with('/') {
        endpoint.set_path(&format!("{}/", endpoint.path()));
    }
    profile.endpoint = endpoint.to_string();
    if profile.id == "chatgpt" {
        chatgpt::valid_profile(&profile)?;
    }
    Ok(profile)
}

#[derive(Debug, Serialize)]
struct ChatCompletionRequest {
    max_tokens: i64,
    model: String,
    messages: Vec<ChatMessage>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    tools: Vec<ChatToolDefinition>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_choice: Option<&'static str>,
    stream_options: ChatStreamOptions,
    #[serde(skip_serializing_if = "Option::is_none")]
    reasoning_split: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    thinking: Option<DeepSeekThinking>,
    stream: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    chat_template_kwargs: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    parallel_tool_calls: Option<bool>,
}

#[derive(Debug, Serialize)]
struct DeepSeekThinking {
    #[serde(rename = "type")]
    kind: &'static str,
}

fn build_chat_request_inner(
    request: ModelRequest,
    minimax: bool,
    deepseek: bool,
    sidecar: bool,
) -> Result<ChatCompletionRequest, ProviderError> {
    validate_model_tool_payload(&request)?;
    let mut continuation_ids = std::collections::BTreeSet::new();
    for exchange in request.tool_exchanges.iter().chain(
        request
            .history
            .iter()
            .filter_map(|message| message.tool_exchange.as_ref()),
    ) {
        if exchange.chatgpt_continuation.is_some() {
            return Err(ProviderError::PreparedRequestBindingMismatch);
        }
        if let Some(state) = &exchange.assistant_continuation {
            if !minimax
                || !continuation_ids.insert(state.validate_scope(
                    request.continuation_scope,
                    &request.model,
                    &exchange.calls,
                )?)
            {
                return Err(ProviderError::PreparedRequestBindingMismatch);
            }
        }
    }
    if (!sidecar && request.output_limit == 0) || request.model.is_empty() {
        return Err(ProviderError::InvalidRequest(
            "model and positive output limit are required",
        ));
    }
    if request.require_tool_call && (sidecar || request.tools.is_empty()) {
        return Err(ProviderError::InvalidRequest(
            "required tool response needs a compatible hosted tool catalog",
        ));
    }
    Ok(ChatCompletionRequest {
        tool_choice: request.require_tool_call.then_some("required"),
        max_tokens: if sidecar && request.output_limit == 0 {
            -1
        } else {
            i64::from(request.output_limit)
        },
        model: request.model,
        messages: build_messages(
            request.system,
            request.history,
            request.prompt,
            request.tool_exchanges,
            minimax,
        ),
        tools: request
            .tools
            .into_iter()
            .map(ChatToolDefinition::from)
            .collect(),
        stream_options: ChatStreamOptions {
            include_usage: true,
        },
        reasoning_split: minimax.then_some(true),
        // Argentum's durable conversation projection does not retain every
        // prior DeepSeek reasoning_content field. DeepSeek requires all of
        // those fields to be replayed when thinking and tools are combined,
        // so use its documented non-thinking tool mode instead of sending an
        // incomplete history that the provider rejects.
        thinking: deepseek.then_some(DeepSeekThinking { kind: "disabled" }),
        stream: true,
        chat_template_kwargs: sidecar.then(|| serde_json::json!({"enable_thinking":false})),
        parallel_tool_calls: sidecar.then_some(false),
    })
}

/// Uses exactly the chat-completions wire builder used by dispatch. This route
/// accepts text and complete tool groups only; no opaque/media item is free.
pub fn account_chat_request(
    profile: &ProviderProfile,
    budget: &argentum_domain::context_budget::ModelBudget,
    request: &ModelRequest,
) -> Result<argentum_domain::context_budget::RequestAccounting, ProviderError> {
    if matches!(
        profile.kind,
        ProviderKind::OpenAiCompatible | ProviderKind::LocalLmStudio
    ) {
        return prepare_compatible_wire(profile, budget, request).map(|(_, accounting)| accounting);
    }
    budget
        .validate(profile)
        .map_err(ProviderError::ContextBudgetInvalid)?;
    let bundled_limit = bundled_context_window(profile);
    if bundled_limit.is_some_and(|limit| budget.usable_context_tokens > limit) {
        return Err(ProviderError::ContextBudgetInvalid(
            "owner context declaration exceeds bundled model-bound context metadata",
        ));
    }
    if profile.kind == ProviderKind::LocalLlamaCpp {
        if request.model != profile.model || request.output_limit != budget.requested_output_tokens
        {
            return Err(ProviderError::ContextBudgetInvalid(
                "model or output control binding mismatch",
            ));
        }
        if profile.endpoint != "local://llama-cpp/" || profile.model != "Qwen3.5-0.8B-Q4_0.gguf" {
            return Err(ProviderError::ContextAccountingUnsupported(
                "this contained model/configuration has no implemented representation accounting contract",
            ));
        }
        if budget.usable_context_tokens
            > u64::from(argentum_android_inference::ANDROID_CONTEXT_TOKENS)
            || budget.maximum_output_tokens
                > argentum_android_inference::ANDROID_MAX_GENERATED_TOKENS as u32
        {
            return Err(ProviderError::ContextBudgetInvalid(
                "owner declaration exceeds the contained engine context or output control",
            ));
        }
        let native = android_local::build_native_request(request)?;
        let message_bytes = native
            .message_bytes
            .checked_sub(native.catalog_bytes)
            .ok_or(ProviderError::InvalidRequest(
                "native representation accounting mismatch",
            ))?;
        let estimated_message_tokens = native
            .estimated_input_tokens
            .saturating_sub(native.estimated_catalog_tokens);
        let mut accounting = argentum_domain::context_budget::RequestAccounting::from_wire_bytes(
            budget,
            estimated_message_tokens,
            native.estimated_catalog_tokens,
            0,
        )
        .map_err(ProviderError::InvalidRequest)?;
        accounting.serialized_message_bytes = message_bytes;
        accounting.serialized_catalog_bytes = native.catalog_bytes;
        accounting.serialized_envelope_bytes = native.envelope_bytes;
        accounting.method =
            "estimated_utf8_final_native_messages_v1_with_owner_uncertainty_not_tokenizer_upper_bound"
                .into();
        accounting.capacity_source =
            "contained_engine_4096_and_trusted_owner_exact_profile_declaration".into();
        accounting.bundled_model_context_limit = Some(u64::from(
            argentum_android_inference::ANDROID_CONTEXT_TOKENS,
        ));
        accounting.message_components.insert(
            "estimated_final_native_role_tokens_excluding_catalog".into(),
            estimated_message_tokens,
        );
        accounting.message_components.insert(
            "estimated_embedded_structured_tool_catalog_tokens".into(),
            native.estimated_catalog_tokens,
        );
        return Ok(accounting);
    }
    if profile.kind == ProviderKind::Anthropic {
        if request.model != profile.model || request.output_limit != budget.requested_output_tokens
        {
            return Err(ProviderError::ContextBudgetInvalid(
                "model or output control binding mismatch",
            ));
        }
        let body = build_anthropic_request(request.clone())?;
        let messages = serde_json::to_vec(&body.messages)
            .map_err(|_| ProviderError::InvalidRequest("message accounting failed"))?
            .len() as u64;
        let system = if let Some(system) = &body.system {
            serde_json::to_vec(system)
                .map_err(|_| ProviderError::InvalidRequest("system accounting failed"))?
                .len() as u64
        } else {
            0
        };
        let total = serde_json::to_vec(&body)
            .map_err(|_| ProviderError::InvalidRequest("wire accounting failed"))?
            .len() as u64;
        let mut accounting = argentum_domain::context_budget::RequestAccounting::from_wire_bytes(
            budget,
            messages + system,
            0,
            total
                .checked_sub(messages + system)
                .ok_or(ProviderError::InvalidRequest("accounting overflow"))?,
        )
        .map_err(ProviderError::InvalidRequest)?;
        accounting
            .message_components
            .insert("host_guidance_and_fresh_instructions".into(), system);
        accounting
            .message_components
            .insert("historical_projection_and_current_task".into(), messages);
        accounting.bundled_model_context_limit = bundled_limit;
        return Ok(accounting);
    }
    Err(ProviderError::ContextAccountingUnsupported(
        "this route has no model-bound request accounting/output control contract",
    ))
}

fn prepare_compatible_wire(
    profile: &ProviderProfile,
    budget: &argentum_domain::context_budget::ModelBudget,
    request: &ModelRequest,
) -> Result<(Bytes, argentum_domain::context_budget::RequestAccounting), ProviderError> {
    if request
        .tool_exchanges
        .iter()
        .chain(
            request
                .history
                .iter()
                .filter_map(|message| message.tool_exchange.as_ref()),
        )
        .any(|exchange| {
            exchange
                .results
                .iter()
                .any(|result| !result.images.is_empty())
        })
    {
        return Err(ProviderError::InvalidRequest(
            "this route has no qualified image input representation",
        ));
    }
    budget
        .validate(profile)
        .map_err(ProviderError::ContextBudgetInvalid)?;
    let bundled_limit = bundled_context_window(profile);
    if bundled_limit.is_some_and(|limit| budget.usable_context_tokens > limit) {
        return Err(ProviderError::ContextBudgetInvalid(
            "owner context declaration exceeds bundled model-bound context metadata",
        ));
    }
    if !matches!(
        profile.kind,
        ProviderKind::OpenAiCompatible | ProviderKind::LocalLmStudio
    ) {
        return Err(ProviderError::ContextAccountingUnsupported(
            "this route has no model-bound request accounting/output control contract",
        ));
    }
    if request.model != profile.model || request.output_limit != budget.requested_output_tokens {
        return Err(ProviderError::ContextBudgetInvalid(
            "model or output control binding mismatch",
        ));
    }
    let endpoint = Url::parse(&profile.endpoint)?;
    let minimax = is_minimax_continuation_origin(&endpoint);
    for exchange in request.tool_exchanges.iter().chain(
        request
            .history
            .iter()
            .filter_map(|message| message.tool_exchange.as_ref()),
    ) {
        if let Some(state) = &exchange.assistant_continuation {
            if !minimax {
                return Err(ProviderError::PreparedRequestBindingMismatch);
            }
            state.validate(profile, request.continuation_scope, &exchange.calls)?;
        }
    }
    if minimax
        && request
            .tool_exchanges
            .iter()
            .any(|exchange| exchange.assistant_continuation.is_none())
    {
        return Err(ProviderError::InvalidRequest(
            "completed MiniMax assistant continuation is unavailable",
        ));
    }
    let deepseek = endpoint.scheme() == "https"
        && endpoint.host_str() == Some("api.deepseek.com")
        && endpoint.port_or_known_default() == Some(443);
    let body = build_chat_request_inner(request.clone(), minimax, deepseek, false)?;
    let messages = serde_json::to_vec(&body.messages)
        .map_err(|_| ProviderError::InvalidRequest("message accounting failed"))?
        .len() as u64;
    let catalog = if body.tools.is_empty() {
        0
    } else {
        serde_json::to_vec(&body.tools)
            .map_err(|_| ProviderError::InvalidRequest("catalog accounting failed"))?
            .len() as u64
    };
    let encoded = serde_json::to_vec(&body)
        .map_err(|_| ProviderError::InvalidRequest("wire encoding failed"))?;
    let total = encoded.len() as u64;
    let envelope = total
        .checked_sub(messages)
        .and_then(|n| n.checked_sub(catalog))
        .ok_or(ProviderError::InvalidRequest("wire accounting overflow"))?;
    let mut accounting = argentum_domain::context_budget::RequestAccounting::from_wire_bytes(
        budget, messages, catalog, envelope,
    )
    .map_err(ProviderError::InvalidRequest)?;
    let system_end = usize::from(request.system.is_some());
    let history_end = system_end
        + request
            .history
            .iter()
            .map(|message| {
                message.tool_exchange.as_ref().map_or(1, |exchange| {
                    if minimax && exchange.assistant_continuation.is_none() {
                        1
                    } else {
                        1 + exchange.results.len()
                    }
                })
            })
            .sum::<usize>();
    let task_end = history_end + 1;
    if task_end > body.messages.len() {
        return Err(ProviderError::InvalidRequest(
            "message component accounting mismatch",
        ));
    }
    let mut content_bytes = 0_u64;
    for (name, range) in [
        ("host_guidance_and_fresh_instructions", 0..system_end),
        ("historical_projection", system_end..history_end),
        ("current_task", history_end..task_end),
        ("current_complete_exchanges", task_end..body.messages.len()),
    ] {
        let mut bytes = 0_u64;
        for message in &body.messages[range] {
            bytes = bytes
                .checked_add(
                    serde_json::to_vec(message)
                        .map_err(|_| ProviderError::InvalidRequest("component accounting failed"))?
                        .len() as u64,
                )
                .ok_or(ProviderError::InvalidRequest(
                    "component accounting overflow",
                ))?;
        }
        content_bytes = content_bytes
            .checked_add(bytes)
            .ok_or(ProviderError::InvalidRequest(
                "component accounting overflow",
            ))?;
        accounting.message_components.insert(name.into(), bytes);
    }
    accounting.message_components.insert(
        "message_array_overhead".into(),
        messages
            .checked_sub(content_bytes)
            .ok_or(ProviderError::InvalidRequest(
                "component accounting overflow",
            ))?,
    );
    accounting.bundled_model_context_limit = bundled_limit;
    if accounting.accounted_input != total {
        return Err(ProviderError::InvalidRequest(
            "prepared body accounting mismatch",
        ));
    }
    Ok((Bytes::from(encoded), accounting))
}

/// Existing bundled hosted metadata is only an additional ceiling. It does not
/// establish usable input/output capacity, and is never a local runtime size.
pub fn bundled_context_window(profile: &ProviderProfile) -> Option<u64> {
    if profile.kind != ProviderKind::OpenAiCompatible {
        return None;
    }
    let endpoint = Url::parse(&profile.endpoint).ok()?;
    if endpoint.scheme() != "https" || endpoint.port_or_known_default() != Some(443) {
        return None;
    }
    canonical_context_window(profile_id_for_endpoint(&endpoint)?, &profile.model)
}

#[derive(Debug, Serialize)]
struct ChatStreamOptions {
    include_usage: bool,
}

#[derive(Debug, Serialize)]
struct ChatMessage {
    role: &'static str,
    content: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_calls: Option<Vec<ChatToolCall>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_call_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    reasoning_details: Option<Vec<minimax_continuation::ReasoningDetail>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    reasoning_content: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    name: Option<String>,
}

impl ChatMessage {
    fn text(role: &'static str, content: String) -> Self {
        Self {
            role,
            content: Some(content),
            tool_calls: None,
            tool_call_id: None,
            reasoning_details: None,
            reasoning_content: None,
            name: None,
        }
    }
}

#[derive(Debug, Serialize)]
struct ChatToolDefinition {
    #[serde(rename = "type")]
    kind: &'static str,
    function: ChatFunctionDefinition,
}

#[derive(Debug, Serialize)]
struct ChatFunctionDefinition {
    name: String,
    description: String,
    parameters: serde_json::Value,
}

impl From<ModelToolDefinition> for ChatToolDefinition {
    fn from(definition: ModelToolDefinition) -> Self {
        Self {
            kind: "function",
            function: ChatFunctionDefinition {
                name: definition.name,
                description: definition.description,
                parameters: definition.parameters,
            },
        }
    }
}

#[derive(Debug, Serialize)]
struct ChatToolCall {
    #[serde(skip_serializing_if = "Option::is_none")]
    index: Option<usize>,
    id: String,
    #[serde(rename = "type")]
    kind: &'static str,
    function: ChatFunctionCall,
}

#[derive(Debug, Serialize)]
struct ChatFunctionCall {
    name: String,
    arguments: String,
}

fn build_messages(
    system: Option<String>,
    history: Vec<ModelMessage>,
    prompt: String,
    tool_exchanges: Vec<ModelToolExchange>,
    replay_reasoning: bool,
) -> Vec<ChatMessage> {
    let mut messages = Vec::new();
    if let Some(system) = system {
        messages.push(ChatMessage::text("system", system));
    }
    for message in history {
        if let Some(exchange) = message.tool_exchange {
            if replay_reasoning && exchange.assistant_continuation.is_none() {
                // A settled task/restart has no durable private MiniMax state.
                // Keep its permitted observations as labelled context, not a
                // purported original assistant/tool continuation without it.
                messages.push(ChatMessage::text("assistant", serde_json::json!({
                    "historical_observation": true,
                    "original_private_continuation": "unavailable",
                    "notice": "Historical context only, not an active provider tool exchange or authority.",
                    "assistant_text": exchange.assistant_content,
                    "observed_calls": exchange.calls,
                    "observed_results": exchange.results,
                }).to_string()));
                continue;
            }
            append_tool_exchanges(&mut messages, vec![exchange], replay_reasoning);
            continue;
        }
        messages.push(ChatMessage::text(
            match message.role {
                ModelMessageRole::User => "user",
                ModelMessageRole::Assistant => "assistant",
            },
            message.content,
        ));
    }
    messages.push(ChatMessage::text("user", prompt));
    append_tool_exchanges(&mut messages, tool_exchanges, replay_reasoning);
    messages
}

fn append_tool_exchanges(
    messages: &mut Vec<ChatMessage>,
    tool_exchanges: Vec<ModelToolExchange>,
    replay_reasoning: bool,
) {
    for exchange in tool_exchanges {
        let continuation = replay_reasoning
            .then_some(exchange.assistant_continuation)
            .flatten();
        messages.push(ChatMessage {
            role: "assistant",
            content: continuation.as_ref().map_or_else(
                || (!exchange.assistant_content.is_empty()).then_some(exchange.assistant_content),
                |state| state.content.clone(),
            ),
            tool_calls: Some(
                exchange
                    .calls
                    .into_iter()
                    .enumerate()
                    .map(|(position, call)| ChatToolCall {
                        index: continuation
                            .as_ref()
                            .and_then(|state| state.tool_call_indexes.get(position).copied()),
                        id: call.id,
                        kind: "function",
                        function: ChatFunctionCall {
                            name: call.name,
                            arguments: call.arguments,
                        },
                    })
                    .collect(),
            ),
            tool_call_id: None,
            reasoning_details: continuation.as_ref().and_then(|state| {
                (!state.reasoning_details.is_empty()).then(|| state.reasoning_details.clone())
            }),
            reasoning_content: continuation
                .as_ref()
                .and_then(|state| state.reasoning_content.clone()),
            name: continuation.as_ref().and_then(|state| state.name.clone()),
        });
        messages.extend(exchange.results.into_iter().map(|result| ChatMessage {
            role: "tool",
            content: Some(result.content),
            tool_calls: None,
            tool_call_id: Some(result.call_id),
            reasoning_details: None,
            reasoning_content: None,
            name: None,
        }));
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TaggedReasoningState {
    Visible,
    Hidden,
}

#[derive(Debug)]
struct TaggedReasoningFilter {
    state: TaggedReasoningState,
    pending: String,
    visible_bytes: usize,
    reasoning_bytes: usize,
}

impl TaggedReasoningFilter {
    const OPEN: &'static str = "<think>";
    const CLOSE: &'static str = "</think>";

    fn new(_enabled: bool) -> Self {
        Self {
            state: TaggedReasoningState::Visible,
            pending: String::new(),
            visible_bytes: 0,
            reasoning_bytes: 0,
        }
    }

    fn push(&mut self, text: &str) -> Result<ReasoningSplit, ProviderError> {
        self.pending.push_str(text);
        let mut split = ReasoningSplit::default();
        loop {
            let marker = match self.state {
                TaggedReasoningState::Visible => Self::OPEN,
                TaggedReasoningState::Hidden => Self::CLOSE,
            };
            if let Some(index) = self.pending.find(marker) {
                let segment = self.pending[..index].to_owned();
                self.append_segment(&mut split, &segment)?;
                self.pending.drain(..index + marker.len());
                self.state = match self.state {
                    TaggedReasoningState::Visible => TaggedReasoningState::Hidden,
                    TaggedReasoningState::Hidden => TaggedReasoningState::Visible,
                };
                continue;
            }

            let retained = partial_marker_suffix_len(&self.pending, marker);
            let consumed = self.pending.len().saturating_sub(retained);
            let segment = self.pending[..consumed].to_owned();
            self.append_segment(&mut split, &segment)?;
            self.pending.drain(..consumed);
            break;
        }
        Ok(split)
    }

    fn finish(mut self) -> Result<ReasoningSplit, ProviderError> {
        if self.state == TaggedReasoningState::Hidden {
            return Err(ProviderError::InvalidResponse(
                "provider reasoning block was not closed".into(),
            ));
        }
        let trailing = std::mem::take(&mut self.pending);
        let mut split = ReasoningSplit::default();
        self.append_segment(&mut split, &trailing)?;
        Ok(split)
    }

    fn append_segment(
        &mut self,
        split: &mut ReasoningSplit,
        segment: &str,
    ) -> Result<(), ProviderError> {
        let (target, total, limit, reason) = match self.state {
            TaggedReasoningState::Visible => (
                &mut split.visible,
                &mut self.visible_bytes,
                MAX_VISIBLE_RESPONSE_BYTES,
                "provider visible response exceeded the safety limit",
            ),
            TaggedReasoningState::Hidden => (
                &mut split.reasoning,
                &mut self.reasoning_bytes,
                MAX_REASONING_RESPONSE_BYTES,
                "provider reasoning response exceeded the safety limit",
            ),
        };
        if total.saturating_add(segment.len()) > limit {
            return Err(ProviderError::InvalidResponse(reason.into()));
        }
        *total += segment.len();
        target.push_str(segment);
        Ok(())
    }
}

#[derive(Debug, Default, PartialEq, Eq)]
struct ReasoningSplit {
    visible: String,
    reasoning: String,
}

#[derive(Debug, Default)]
struct ExplicitReasoningAccumulator {
    total_bytes: usize,
    details_value: String,
    details_mode: ReasoningDetailsMode,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
enum ReasoningDetailsMode {
    #[default]
    Unknown,
    Cumulative,
    Incremental,
}

#[derive(Debug, Default)]
struct ReasoningTagFilter {
    pending: String,
}

impl ReasoningTagFilter {
    fn push(&mut self, text: &str) -> String {
        self.pending.push_str(text);
        let mut output = String::new();
        loop {
            let next_marker = [TaggedReasoningFilter::OPEN, TaggedReasoningFilter::CLOSE]
                .into_iter()
                .filter_map(|marker| self.pending.find(marker).map(|index| (index, marker)))
                .min_by_key(|(index, _)| *index);
            if let Some((index, marker)) = next_marker {
                output.push_str(&self.pending[..index]);
                self.pending.drain(..index + marker.len());
                continue;
            }
            let retained = [TaggedReasoningFilter::OPEN, TaggedReasoningFilter::CLOSE]
                .into_iter()
                .map(|marker| partial_marker_suffix_len(&self.pending, marker))
                .max()
                .unwrap_or(0);
            let consumed = self.pending.len().saturating_sub(retained);
            output.push_str(&self.pending[..consumed]);
            self.pending.drain(..consumed);
            break;
        }
        output
    }

    fn finish(self) -> Result<String, ProviderError> {
        if self.pending.is_empty() {
            Ok(String::new())
        } else {
            Err(ProviderError::InvalidResponse(
                "provider reasoning field ended with a fragmented raw tag".into(),
            ))
        }
    }
}

impl ExplicitReasoningAccumulator {
    fn new(details_mode: ReasoningDetailsMode) -> Self {
        Self {
            details_mode,
            ..Self::default()
        }
    }

    fn push_delta(&mut self, delta: &str) -> Result<String, ProviderError> {
        if self.total_bytes.saturating_add(delta.len()) > MAX_REASONING_RESPONSE_BYTES {
            return Err(ProviderError::InvalidResponse(
                "provider reasoning response exceeded the safety limit".into(),
            ));
        }
        self.total_bytes += delta.len();
        Ok(delta.to_owned())
    }

    fn push_details(&mut self, value: &str) -> Result<String, ProviderError> {
        match self.details_mode {
            ReasoningDetailsMode::Unknown if self.details_value.is_empty() => {
                self.details_value = value.to_owned();
                self.push_delta(value)
            }
            ReasoningDetailsMode::Unknown => {
                let delta = if value.starts_with(&self.details_value) {
                    self.details_mode = ReasoningDetailsMode::Cumulative;
                    value[self.details_value.len()..].to_owned()
                } else {
                    self.details_mode = ReasoningDetailsMode::Incremental;
                    value.to_owned()
                };
                self.details_value = value.to_owned();
                self.push_delta(&delta)
            }
            ReasoningDetailsMode::Cumulative if value.starts_with(&self.details_value) => {
                let delta = value[self.details_value.len()..].to_owned();
                self.details_value = value.to_owned();
                self.push_delta(&delta)
            }
            ReasoningDetailsMode::Cumulative => {
                self.details_mode = ReasoningDetailsMode::Incremental;
                self.details_value = value.to_owned();
                self.push_delta(value)
            }
            ReasoningDetailsMode::Incremental => {
                self.details_value = value.to_owned();
                self.push_delta(value)
            }
        }
    }
}

fn merge_explicit_reasoning(
    reasoning: Option<String>,
    details: Vec<ChatReasoningDetail>,
) -> Result<Option<ReasoningFragment>, ProviderError> {
    let detail_text = details
        .into_iter()
        .filter_map(|detail| detail.text)
        .collect::<String>();
    let selected = match (reasoning, detail_text.is_empty()) {
        (Some(reasoning), _) if !reasoning.is_empty() => Some(ReasoningFragment::Delta(reasoning)),
        (_, false) => Some(ReasoningFragment::Details(detail_text)),
        _ => None,
    };
    Ok(selected)
}

enum ReasoningFragment {
    Delta(String),
    Details(String),
}

fn partial_marker_suffix_len(text: &str, marker: &str) -> usize {
    (1..marker.len())
        .rev()
        .find(|length| text.ends_with(&marker[..*length]))
        .unwrap_or(0)
}

#[derive(Debug, Deserialize)]
struct ChatCompletionChunk {
    #[serde(default)]
    id: Option<String>,
    #[serde(default)]
    model: Option<String>,
    choices: Vec<ChatStreamChoice>,
    #[serde(default)]
    usage: Option<serde_json::Value>,
}

#[derive(Debug, Deserialize)]
struct ChatStreamChoice {
    delta: ChatDelta,
    #[serde(default)]
    finish_reason: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ChatDelta {
    #[serde(default)]
    role: Option<String>,
    #[serde(default)]
    name: Option<String>,
    content: Option<String>,
    #[serde(default)]
    reasoning: Option<String>,
    #[serde(default)]
    reasoning_content: Option<String>,
    #[serde(default)]
    reasoning_details: Vec<ChatReasoningDetail>,
    #[serde(default)]
    tool_calls: Vec<ChatToolCallDelta>,
}

type ChatReasoningDetail = minimax_continuation::ReasoningDetail;

#[derive(Debug, Deserialize)]
struct ChatUsage {
    prompt_tokens: u64,
    completion_tokens: u64,
    total_tokens: u64,
    #[serde(default)]
    prompt_tokens_details: Option<PromptTokenDetails>,
    #[serde(default)]
    completion_tokens_details: Option<CompletionTokenDetails>,
}

#[derive(Debug, Deserialize)]
struct PromptTokenDetails {
    #[serde(default)]
    cached_tokens: Option<u64>,
}

#[derive(Debug, Deserialize)]
struct CompletionTokenDetails {
    #[serde(default)]
    reasoning_tokens: Option<u64>,
}

impl TryFrom<ChatUsage> for ModelUsage {
    type Error = ProviderError;

    fn try_from(usage: ChatUsage) -> Result<Self, Self::Error> {
        if usage.prompt_tokens.saturating_add(usage.completion_tokens) != usage.total_tokens {
            return Err(ProviderError::InvalidResponse(
                "provider usage totals were inconsistent".into(),
            ));
        }
        let cached_input_tokens = usage
            .prompt_tokens_details
            .and_then(|details| details.cached_tokens);
        let reasoning_tokens = usage
            .completion_tokens_details
            .and_then(|details| details.reasoning_tokens);
        if cached_input_tokens.is_some_and(|tokens| tokens > usage.prompt_tokens)
            || reasoning_tokens.is_some_and(|tokens| tokens > usage.completion_tokens)
        {
            return Err(ProviderError::InvalidResponse(
                "provider usage details were inconsistent".into(),
            ));
        }
        Ok(Self {
            input_tokens: usage.prompt_tokens,
            output_tokens: usage.completion_tokens,
            total_tokens: usage.total_tokens,
            cached_input_tokens,
            reasoning_tokens,
            context_window_tokens: None,
        })
    }
}

#[derive(Debug, Deserialize)]
struct ChatToolCallDelta {
    #[serde(default)]
    index: usize,
    id: Option<String>,
    #[serde(rename = "type")]
    kind: Option<String>,
    function: Option<ChatFunctionCallDelta>,
}

#[derive(Debug, Deserialize)]
struct ChatFunctionCallDelta {
    name: Option<String>,
    arguments: Option<String>,
}

#[derive(Debug, Default)]
struct PartialToolCall {
    id: String,
    name: String,
    arguments: String,
}

#[derive(Debug, Default)]
struct ToolCallAccumulator {
    calls: BTreeMap<usize, PartialToolCall>,
}

impl ToolCallAccumulator {
    fn push(&mut self, fragments: Vec<ChatToolCallDelta>) -> Result<(), ProviderError> {
        for fragment in fragments {
            if fragment.index >= MAX_TOOL_CALLS_PER_RESPONSE {
                return Err(invalid_tool_response("too many tool calls"));
            }
            if fragment
                .kind
                .as_deref()
                .is_some_and(|kind| kind != "function")
            {
                return Err(invalid_tool_response("unsupported tool-call type"));
            }
            let is_new = !self.calls.contains_key(&fragment.index);
            if is_new && self.calls.len() >= MAX_TOOL_CALLS_PER_RESPONSE {
                return Err(invalid_tool_response("too many tool calls"));
            }
            let call = self.calls.entry(fragment.index).or_default();
            if let Some(id) = fragment.id {
                append_bounded(
                    &mut call.id,
                    &id,
                    MAX_TOOL_CALL_ID_BYTES,
                    "tool-call identifier exceeded the safety limit",
                )?;
            }
            if let Some(function) = fragment.function {
                if let Some(name) = function.name {
                    append_bounded(
                        &mut call.name,
                        &name,
                        MAX_TOOL_NAME_BYTES,
                        "tool name exceeded the safety limit",
                    )?;
                }
                if let Some(arguments) = function.arguments {
                    append_bounded(
                        &mut call.arguments,
                        &arguments,
                        MAX_TOOL_ARGUMENT_BYTES,
                        "tool-call arguments exceeded the safety limit",
                    )?;
                }
            }
        }
        Ok(())
    }

    fn finish(self) -> Result<Vec<ModelToolCall>, ProviderError> {
        let mut seen_ids = std::collections::BTreeSet::new();
        self.calls
            .into_values()
            .map(|call| {
                if !valid_tool_call_id(&call.id)
                    || !valid_tool_name(&call.name)
                    || !seen_ids.insert(call.id.clone())
                {
                    return Err(invalid_tool_response("invalid tool-call identity"));
                }
                let arguments: serde_json::Value =
                    serde_json::from_str(&call.arguments).map_err(|_| {
                        invalid_tool_response("tool-call arguments were not valid JSON")
                    })?;
                if !arguments.is_object() {
                    return Err(invalid_tool_response(
                        "tool-call arguments must be a JSON object",
                    ));
                }
                Ok(ModelToolCall {
                    id: call.id,
                    name: call.name,
                    arguments: call.arguments,
                })
            })
            .collect()
    }
}

fn append_bounded(
    target: &mut String,
    fragment: &str,
    limit: usize,
    reason: &'static str,
) -> Result<(), ProviderError> {
    if target.len().saturating_add(fragment.len()) > limit {
        return Err(invalid_tool_response(reason));
    }
    target.push_str(fragment);
    Ok(())
}

fn invalid_tool_response(reason: &'static str) -> ProviderError {
    ProviderError::InvalidResponse(reason.into())
}

fn invalid_tool_request(reason: &'static str) -> ProviderError {
    ProviderError::InvalidRequest(reason)
}

fn valid_tool_call_id(id: &str) -> bool {
    !id.is_empty() && id.len() <= MAX_TOOL_CALL_ID_BYTES && !id.chars().any(char::is_control)
}

fn valid_tool_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= MAX_TOOL_NAME_BYTES
        && name
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || matches!(character, '_' | '-'))
}

fn validate_model_tool_payload(request: &ModelRequest) -> Result<(), ProviderError> {
    ModelToolContract::validate_catalog(&request.tools)?;
    if request.tool_exchanges.len() > MAX_TOOL_EXCHANGES {
        return Err(invalid_tool_request("too many tool exchanges"));
    }
    if request.history.len() > 128 {
        return Err(invalid_tool_request("too many historical messages"));
    }
    for message in &request.history {
        if message.tool_exchange.is_some()
            && (message.role != ModelMessageRole::Assistant || !message.content.is_empty())
        {
            return Err(invalid_tool_request("invalid historical exchange envelope"));
        }
    }
    for exchange in request
        .history
        .iter()
        .filter_map(|message| message.tool_exchange.as_ref())
        .chain(request.tool_exchanges.iter())
    {
        if exchange.calls.is_empty()
            || exchange.calls.len() > MAX_TOOL_CALLS_PER_RESPONSE
            || exchange.results.len() != exchange.calls.len()
            || exchange.assistant_reasoning.len() > MAX_REASONING_RESPONSE_BYTES
        {
            return Err(invalid_tool_request("invalid tool exchange shape"));
        }
        let call_ids = exchange
            .calls
            .iter()
            .map(|call| {
                if !valid_tool_call_id(&call.id)
                    || !valid_tool_name(&call.name)
                    || call.arguments.len() > MAX_TOOL_ARGUMENT_BYTES
                    || !serde_json::from_str::<serde_json::Value>(&call.arguments)
                        .is_ok_and(|arguments| arguments.is_object())
                {
                    return Err(invalid_tool_request("invalid tool call"));
                }
                Ok(call.id.as_str())
            })
            .collect::<Result<std::collections::BTreeSet<_>, _>>()?;
        if call_ids.len() != exchange.calls.len()
            || exchange
                .results
                .iter()
                .zip(&exchange.calls)
                .any(|(result, call)| {
                    result.call_id != call.id || result.content.len() > MAX_TOOL_RESULT_BYTES
                })
        {
            return Err(invalid_tool_request("invalid tool result"));
        }
    }
    Ok(())
}

fn parse_chat_completion_chunk(data: &str) -> Result<ChatCompletionChunk, ProviderError> {
    serde_json::from_str(data).map_err(|error| ProviderError::InvalidResponse(error.to_string()))
}

#[derive(Debug, Deserialize)]
struct ModelsResponse {
    data: Vec<ModelDescriptor>,
}

#[derive(Debug, Deserialize)]
struct ModelDescriptor {
    id: String,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum LmStudioModelsResponse {
    Envelope { models: Vec<LmStudioModel> },
    Direct(Vec<LmStudioModel>),
}

#[derive(Debug, Deserialize)]
struct LmStudioModel {
    #[serde(rename = "type")]
    kind: String,
    key: String,
    display_name: String,
    #[serde(default)]
    loaded_instances: Vec<LmStudioLoadedInstance>,
    #[serde(default)]
    max_context_length: Option<u64>,
}

#[derive(Debug, Deserialize)]
struct LmStudioLoadedInstance {
    id: String,
    #[serde(default)]
    config: Option<LmStudioInstanceConfig>,
}

#[derive(Debug, Deserialize)]
struct LmStudioInstanceConfig {
    #[serde(default)]
    context_length: Option<u64>,
}

fn parse_lm_studio_models_response(body: &[u8]) -> Result<Vec<ProviderModel>, ProviderError> {
    let response: LmStudioModelsResponse = serde_json::from_slice(body)
        .map_err(|error| ProviderError::InvalidResponse(error.to_string()))?;
    let models = match response {
        LmStudioModelsResponse::Envelope { models } | LmStudioModelsResponse::Direct(models) => {
            models
        }
    };
    if models.len() > MAX_PROBE_MODELS {
        return Err(ProviderError::InvalidResponse(format!(
            "models response exceeded {MAX_PROBE_MODELS} entries"
        )));
    }
    let mut result = Vec::new();
    for model in models.into_iter().filter(|model| model.kind == "llm") {
        if result
            .len()
            .saturating_add(model.loaded_instances.len())
            .saturating_add(1)
            > MAX_PROBE_MODELS
        {
            return Err(ProviderError::InvalidResponse(format!(
                "models response exceeded {MAX_PROBE_MODELS} entries"
            )));
        }
        for instance in model.loaded_instances {
            result.push(ProviderModel {
                id: instance.id,
                label: model.display_name.clone(),
                context_window_tokens: instance
                    .config
                    .and_then(|config| config.context_length)
                    .filter(|tokens| *tokens > 0),
            });
        }
        result.push(ProviderModel {
            id: model.key,
            label: model.display_name,
            context_window_tokens: model.max_context_length.filter(|tokens| *tokens > 0),
        });
    }
    sanitize_provider_models(result)
}

fn parse_models_response(body: &[u8]) -> Result<Vec<ProviderModel>, ProviderError> {
    let response: ModelsResponse = serde_json::from_slice(body)
        .map_err(|error| ProviderError::InvalidResponse(error.to_string()))?;
    if response.data.len() > MAX_PROBE_MODELS {
        return Err(ProviderError::InvalidResponse(format!(
            "models response exceeded {MAX_PROBE_MODELS} entries"
        )));
    }
    if response
        .data
        .iter()
        .any(|model| model.id.trim().is_empty() || model.id.len() > MAX_PROBE_MODEL_ID_BYTES)
    {
        return Err(ProviderError::InvalidResponse(
            "models response contained an invalid model identifier".into(),
        ));
    }
    sanitize_provider_models(
        response
            .data
            .into_iter()
            .map(|model| ProviderModel {
                label: model.id.clone(),
                id: model.id,
                context_window_tokens: None,
            })
            .collect(),
    )
}

fn sanitize_provider_models(
    models: Vec<ProviderModel>,
) -> Result<Vec<ProviderModel>, ProviderError> {
    if models.len() > MAX_PROBE_MODELS {
        return Err(ProviderError::InvalidResponse(format!(
            "models response exceeded {MAX_PROBE_MODELS} entries"
        )));
    }
    let mut normalized = BTreeMap::new();
    for model in models {
        let id = model.id.trim();
        if id.is_empty() || id.len() > MAX_PROBE_MODEL_ID_BYTES || id.chars().any(char::is_control)
        {
            return Err(ProviderError::InvalidResponse(
                "models response contained an invalid model identifier".into(),
            ));
        }
        let label = model.label.trim();
        let label = if label.is_empty() { id } else { label };
        if label.len() > MAX_PROBE_MODEL_ID_BYTES || label.chars().any(char::is_control) {
            return Err(ProviderError::InvalidResponse(
                "models response contained an invalid model label".into(),
            ));
        }
        normalized.entry(id.to_owned()).or_insert(ProviderModel {
            id: id.to_owned(),
            label: label.to_owned(),
            context_window_tokens: model.context_window_tokens,
        });
    }
    Ok(normalized.into_values().collect())
}

fn canonical_context_window(profile_id: &str, model_id: &str) -> Option<u64> {
    match (profile_id, model_id) {
        (
            "minimax",
            "MiniMax-M2.7"
            | "MiniMax-M2.7-highspeed"
            | "MiniMax-M2.5"
            | "MiniMax-M2.5-highspeed"
            | "MiniMax-M2.1"
            | "MiniMax-M2.1-highspeed"
            | "MiniMax-M2",
        ) => Some(204_800),
        ("minimax", "MiniMax-M3") => Some(1_000_000),
        (
            "deepseek",
            "deepseek-flash"
            | "deepseek-v4-pro"
            | "deepseek-v4-flash"
            | "deepseek-v4-flash-vision-exp",
        ) => Some(1_000_000),
        ("openai", "gpt-5.6" | "gpt-5.6-sol" | "gpt-5.6-terra" | "gpt-5.6-luna") => Some(1_050_000),
        _ => None,
    }
}

fn is_supported_openai_chat_model(model_id: &str) -> bool {
    matches!(
        model_id,
        "gpt-5.6" | "gpt-5.6-sol" | "gpt-5.6-terra" | "gpt-5.6-luna"
    )
}

fn profile_id_for_endpoint(endpoint: &Url) -> Option<&'static str> {
    match endpoint.host_str()? {
        "api.openai.com" => Some("openai"),
        "api.minimax.io" => Some("minimax"),
        "api.deepseek.com" => Some("deepseek"),
        _ => None,
    }
}

fn duration_millis(duration: Duration) -> u64 {
    duration.as_millis().min(u64::MAX as u128) as u64
}

fn public_endpoint(endpoint: &Url) -> String {
    let mut endpoint = endpoint.clone();
    let _ = endpoint.set_username("");
    let _ = endpoint.set_password(None);
    endpoint.set_query(None);
    endpoint.set_fragment(None);
    endpoint.to_string()
}

fn sanitize_provider_status(mut status: ProviderStatus) -> ProviderStatus {
    status.endpoint = match Url::parse(&status.endpoint) {
        Ok(endpoint) => public_endpoint(&endpoint),
        Err(_) => status
            .endpoint
            .split(['?', '#'])
            .next()
            .unwrap_or_default()
            .chars()
            .filter(|character| !character.is_control())
            .take(2_048)
            .collect(),
    };
    status
}

fn display_model(model: &str) -> Option<String> {
    let model = model
        .trim()
        .chars()
        .filter(|character| !character.is_control())
        .take(128)
        .collect::<String>();
    (!model.is_empty()).then_some(model)
}

fn display_provider_id(provider_id: &str) -> String {
    let provider_id = provider_id
        .trim()
        .chars()
        .filter(|character| {
            character.is_ascii_alphanumeric() || matches!(character, '-' | '_' | '.')
        })
        .take(64)
        .collect::<String>();
    if provider_id.is_empty() {
        "requested-provider".into()
    } else {
        provider_id
    }
}

fn configured_model_detail(model: &str) -> String {
    display_model(model)
        .map(|model| format!("Model: {model}"))
        .unwrap_or_else(|| "No model configured".into())
}

fn reachable_detail(model: &str) -> String {
    display_model(model)
        .map(|model| format!("Reachable; configured model: {model}"))
        .unwrap_or_else(|| "Reachable; no model configured".into())
}

#[derive(Debug, Serialize)]
struct AnthropicRequest {
    model: String,
    max_tokens: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    system: Option<String>,
    messages: Vec<AnthropicMessage>,
}

fn build_anthropic_request(request: ModelRequest) -> Result<AnthropicRequest, ProviderError> {
    if request.output_limit == 0
        || request.model.is_empty()
        || !request.tools.is_empty()
        || !request.tool_exchanges.is_empty()
        || request
            .history
            .iter()
            .any(|message| message.tool_exchange.is_some())
    {
        return Err(ProviderError::InvalidRequest("Anthropic text route requires a model, positive output limit and no tools or exchanges"));
    }
    Ok(AnthropicRequest {
        model: request.model,
        max_tokens: request.output_limit,
        system: request.system,
        messages: build_anthropic_messages(request.history, request.prompt),
    })
}

#[derive(Debug, Serialize)]
struct AnthropicMessage {
    role: String,
    content: String,
}

#[derive(Debug, Deserialize)]
struct AnthropicResponse {
    content: Vec<AnthropicContent>,
}

#[derive(Debug, Deserialize)]
struct AnthropicContent {
    #[serde(rename = "type")]
    kind: String,
    text: String,
}

fn build_anthropic_messages(history: Vec<ModelMessage>, prompt: String) -> Vec<AnthropicMessage> {
    let mut messages = history
        .into_iter()
        .map(|message| AnthropicMessage {
            role: match message.role {
                ModelMessageRole::User => "user".into(),
                ModelMessageRole::Assistant => "assistant".into(),
            },
            content: message.content,
        })
        .collect::<Vec<_>>();
    messages.push(AnthropicMessage {
        role: "user".into(),
        content: prompt,
    });
    messages
}

#[cfg(test)]
mod continuation_contract_tests;

#[cfg(test)]
mod truncation_tests;

#[cfg(test)]
mod tests {
    use std::io::{Read, Write};
    use std::net::{TcpListener, TcpStream};
    use std::process::{Command, Stdio};
    use std::sync::mpsc as std_mpsc;
    use std::thread;
    use std::time::Instant;

    use super::*;

    struct MockServer {
        endpoint: String,
        request: std_mpsc::Receiver<String>,
        handle: thread::JoinHandle<()>,
    }

    #[tokio::test]
    async fn sidecar_tools_and_uncapped_output_use_identical_count_and_generation_bodies() {
        let reply = |body: serde_json::Value| {
            let body = body.to_string();
            format!("HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",body.len())
        };
        let props = serde_json::json!({"default_generation_settings":{"n_ctx":8192},"chat_template":"synthetic tool template","chat_template_caps":{"supports_tools":true,"supports_tool_calls":true}});
        let answer = serde_json::to_string(&vec!["long-json-value"; 2500]).unwrap();
        let frame =
            serde_json::json!({"choices":[{"delta":{"content":answer},"finish_reason":"stop"}]});
        let stream=format!("HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nConnection: close\r\n\r\ndata: {frame}\n\ndata: [DONE]\n\n");
        let (endpoint, requests, worker) = spawn_mock_server_sequence(vec![
            reply(props),
            reply(serde_json::json!({"input_tokens":64})),
            stream,
        ]);
        let provider = OpenAiCompatibleProvider::new_local_sidecar(
            &endpoint,
            "fixture.gguf",
            SecretValue::new("synthetic-auth"),
        )
        .unwrap();
        let mut request = tool_catalog_request(1);
        request.model = "fixture.gguf".into();
        request.output_limit = 0;
        let validation = provider.validate_request(&request).await.unwrap();
        assert_eq!(validation.exact_input_tokens, Some(64));
        assert_eq!(validation.context_tokens, Some(8192));
        assert_eq!(validation.output_limit, 0);
        let (tx, mut rx) = mpsc::channel(8);
        provider.stream(request, tx).await.unwrap();
        let mut output = String::new();
        while let Some(event) = rx.recv().await {
            if let ProviderEvent::Delta(s) = event {
                output.push_str(&s);
            }
        }
        assert_eq!(output, answer);
        let requests = requests.recv_timeout(Duration::from_secs(2)).unwrap();
        worker.join().unwrap();
        assert!(requests[0].starts_with("GET /props "));
        assert!(requests[1].starts_with("POST /v1/chat/completions/input_tokens "));
        let body = |request: &str| {
            serde_json::from_str::<serde_json::Value>(request.split_once("\r\n\r\n").unwrap().1)
                .unwrap()
        };
        assert_eq!(body(&requests[1]), body(&requests[2]));
        assert_eq!(body(&requests[2])["max_tokens"], -1);
        assert_eq!(body(&requests[2])["tools"].as_array().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn context_variants_validate_exact_loaded_size_and_reject_mismatch() {
        let reply = |body: serde_json::Value| {
            let body = body.to_string();
            format!("HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",body.len())
        };
        for size in SIDECAR_CONTEXT_VARIANTS {
            for matches in [true, false] {
                let actual = if matches { size } else { size / 2 };
                let mut replies = vec![reply(
                    serde_json::json!({"default_generation_settings":{"n_ctx":actual},"chat_template":"fixture tool template","chat_template_caps":{"supports_tools":true,"supports_tool_calls":true}}),
                )];
                if matches {
                    replies.push(reply(serde_json::json!({"input_tokens":size-100})));
                }
                let (endpoint, requests, worker) = spawn_mock_server_sequence(replies);
                let provider = OpenAiCompatibleProvider::new_local_sidecar_with_context(
                    endpoint,
                    "fixture.gguf",
                    SecretValue::new("synthetic-auth"),
                    size,
                )
                .unwrap();
                let mut request = tool_catalog_request(1);
                request.model = "fixture.gguf".into();
                request.output_limit = 0;
                let result = provider.validate_request(&request).await;
                if matches {
                    let v = result.unwrap();
                    assert_eq!(v.context_tokens, Some(u64::from(size)));
                    assert_eq!(v.exact_input_tokens, Some(u64::from(size - 100)));
                } else {
                    assert!(matches!(
                        result,
                        Err(ProviderError::LocalRuntimeUnavailable { .. })
                    ));
                }
                let observed = requests.recv_timeout(Duration::from_secs(2)).unwrap();
                assert_eq!(observed.len(), if matches { 2 } else { 1 });
                worker.join().unwrap();
            }
        }
    }

    #[tokio::test]
    async fn sidecar_rejects_a_loaded_template_that_drops_tools() {
        let body=serde_json::json!({"default_generation_settings":{"n_ctx":8192},"chat_template":"text only","chat_template_caps":{"supports_tools":false,"supports_tool_calls":false}}).to_string();
        let server = spawn_mock_server(
            format!(
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            ),
            Duration::ZERO,
        );
        let provider = OpenAiCompatibleProvider::new_local_sidecar(
            &server.endpoint,
            "fixture.gguf",
            SecretValue::new("synthetic-auth"),
        )
        .unwrap();
        let mut request = tool_catalog_request(1);
        request.model = "fixture.gguf".into();
        assert!(matches!(
            provider.validate_request(&request).await,
            Err(ProviderError::LocalCapabilityUnsupported { .. })
        ));
        assert!(server.finish().starts_with("GET /props "));
    }

    #[tokio::test]
    async fn sidecar_truncated_tools_are_not_emitted_as_executable_calls() {
        let frame = serde_json::json!({"choices":[{"delta":{"content":"partial visible answer","tool_calls":[{"index":0,"id":"partial","type":"function","function":{"name":"read_text","arguments":"{\"path\":"}}]},"finish_reason":"length"}]});
        let server=spawn_mock_server(format!("HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nConnection: close\r\n\r\ndata: {frame}\n\ndata: [DONE]\n\n"),Duration::ZERO);
        let provider = OpenAiCompatibleProvider::new_local_sidecar(
            &server.endpoint,
            "fixture.gguf",
            SecretValue::new("synthetic-auth"),
        )
        .unwrap();
        let mut request = tool_catalog_request(1);
        request.model = "fixture.gguf".into();
        request.output_limit = 0;
        let (tx, mut rx) = mpsc::channel(8);
        assert!(matches!(
            provider.stream(request, tx).await,
            Err(ProviderError::ResponseTruncated)
        ));
        let mut visible = false;
        while let Some(event) = rx.recv().await {
            assert!(!matches!(
                event,
                ProviderEvent::ToolCall(_) | ProviderEvent::Completed
            ));
            visible |= matches!(event, ProviderEvent::Delta(_));
        }
        assert!(visible);
        server.finish();
    }

    const PROXY_ENVIRONMENT_NAMES: [&str; 6] = [
        "HTTP_PROXY",
        "http_proxy",
        "HTTPS_PROXY",
        "https_proxy",
        "ALL_PROXY",
        "all_proxy",
    ];
    const PROXY_CHILD_CASE: &str = "ARGENTUM_PROVIDER_PROXY_CHILD_CASE";
    const PROXY_CHILD_DIRECT_ENDPOINT: &str = "ARGENTUM_PROVIDER_PROXY_CHILD_DIRECT_ENDPOINT";
    const PROXY_CHILD_PROXY_ENDPOINT: &str = "ARGENTUM_PROVIDER_PROXY_CHILD_PROXY_ENDPOINT";

    struct ObservedServer {
        endpoint: String,
        request: std_mpsc::Receiver<Option<String>>,
        stop: std_mpsc::Sender<()>,
        handle: thread::JoinHandle<()>,
    }

    impl ObservedServer {
        fn finish(self) -> Option<String> {
            let _ = self.stop.send(());
            let request = self
                .request
                .recv_timeout(Duration::from_secs(2))
                .expect("observed server result");
            self.handle.join().expect("observed server");
            request
        }
    }

    fn spawn_observed_server(response: String) -> ObservedServer {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind observed server");
        listener
            .set_nonblocking(true)
            .expect("set observed server nonblocking");
        let address = listener.local_addr().expect("observed address");
        let (request_sender, request) = std_mpsc::channel();
        let (stop, stop_receiver) = std_mpsc::channel();
        let handle = thread::spawn(move || {
            let deadline = Instant::now() + Duration::from_secs(5);
            let observed = loop {
                if stop_receiver.try_recv().is_ok() {
                    break None;
                }
                match listener.accept() {
                    Ok((mut stream, _)) => {
                        stream
                            .set_nonblocking(false)
                            .expect("set observed stream blocking");
                        let request =
                            String::from_utf8_lossy(&read_mock_request(&mut stream)).into_owned();
                        let _ = stream.write_all(response.as_bytes());
                        break Some(request);
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        if Instant::now() >= deadline {
                            break None;
                        }
                        thread::sleep(Duration::from_millis(5));
                    }
                    Err(error) => panic!("accept observed request: {error}"),
                }
            };
            request_sender
                .send(observed)
                .expect("record observed request");
        });

        ObservedServer {
            endpoint: format!("http://{address}/v1"),
            request,
            stop,
            handle,
        }
    }

    impl MockServer {
        fn finish(self) -> String {
            let request = self
                .request
                .recv_timeout(Duration::from_secs(1))
                .expect("mock request");
            self.handle.join().expect("mock server");
            request
        }
    }

    fn spawn_mock_server(response: String, delay: Duration) -> MockServer {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind mock server");
        let address = listener.local_addr().expect("mock address");
        let (request_sender, request) = std_mpsc::channel();
        let handle = thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("accept mock request");
            let request_bytes = read_mock_request(&mut stream);
            request_sender
                .send(String::from_utf8_lossy(&request_bytes).into_owned())
                .expect("record mock request");
            thread::sleep(delay);
            let _ = stream.write_all(response.as_bytes());
        });

        MockServer {
            endpoint: format!("http://{address}/v1"),
            request,
            handle,
        }
    }

    fn read_mock_request(stream: &mut TcpStream) -> Vec<u8> {
        stream
            .set_read_timeout(Some(Duration::from_secs(1)))
            .expect("set mock read timeout");
        let mut request_bytes = Vec::new();
        let mut buffer = [0_u8; 2_048];
        loop {
            let count = stream.read(&mut buffer).expect("read mock request");
            if count == 0 {
                break;
            }
            request_bytes.extend_from_slice(&buffer[..count]);
            if let Some(header_end) = request_bytes
                .windows(4)
                .position(|window| window == b"\r\n\r\n")
                .map(|index| index + 4)
            {
                let headers = String::from_utf8_lossy(&request_bytes[..header_end]);
                let content_length = headers.lines().find_map(|line| {
                    let (name, value) = line.split_once(':')?;
                    name.eq_ignore_ascii_case("content-length")
                        .then(|| value.trim().parse::<usize>().ok())
                        .flatten()
                });
                let request_complete = match content_length {
                    Some(length) => request_bytes.len() >= header_end + length,
                    None => true,
                };
                if request_complete {
                    break;
                }
            }
        }
        request_bytes
    }

    pub(super) fn spawn_mock_server_sequence(
        responses: Vec<String>,
    ) -> (
        String,
        std_mpsc::Receiver<Vec<String>>,
        thread::JoinHandle<()>,
    ) {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind mock server");
        let address = listener.local_addr().expect("mock address");
        let (sender, receiver) = std_mpsc::channel();
        let handle = thread::spawn(move || {
            let mut requests = Vec::new();
            for response in responses {
                let (mut stream, _) = listener.accept().expect("accept mock request");
                requests
                    .push(String::from_utf8_lossy(&read_mock_request(&mut stream)).into_owned());
                stream
                    .write_all(response.as_bytes())
                    .expect("write mock response");
            }
            sender.send(requests).expect("record mock requests");
        });
        (format!("http://{address}/v1"), receiver, handle)
    }

    fn json_response(status: &str, body: &str) -> String {
        format!(
            "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        )
    }

    pub(super) fn event_stream_response(body: &str) -> String {
        format!(
            "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        )
    }

    pub(super) fn test_provider(
        endpoint: &str,
        api_key: Option<SecretValue>,
    ) -> OpenAiCompatibleProvider {
        OpenAiCompatibleProvider::new(
            endpoint,
            "local/model",
            api_key,
            ProviderKind::OpenAiCompatible,
            "Test provider",
        )
        .expect("provider")
    }

    pub(super) fn empty_model_request() -> ModelRequest {
        ModelRequest {
            require_tool_call: false,
            continuation_scope: None,
            output_limit: 4096,
            model: "local/model".into(),
            system: None,
            history: Vec::new(),
            prompt: "test".into(),
            tools: Vec::new(),
            tool_exchanges: Vec::new(),
        }
    }

    pub(super) fn prepared_fixture(
        endpoint: &str,
    ) -> (
        ProviderProfile,
        argentum_domain::context_budget::ModelBudget,
        ModelRequest,
    ) {
        let profile = ProviderProfile {
            id: "perf04-compatible".into(),
            label: "PERF04 compatible".into(),
            kind: ProviderKind::OpenAiCompatible,
            endpoint: endpoint.into(),
            model: "local/model".into(),
            selected: true,
        };
        let budget = argentum_domain::context_budget::ModelBudget {
            revision: uuid::Uuid::new_v4(),
            profile_id: profile.id.clone(),
            provider_kind: profile.kind,
            endpoint: profile.endpoint.clone(),
            model: profile.model.clone(),
            usable_context_tokens: 204_800,
            input_limit_tokens: Some(196_608),
            maximum_output_tokens: 2_048,
            requested_output_tokens: 1_024,
            uncertainty_tokens: 1_024,
        };
        let mut request = empty_model_request();
        request.output_limit = budget.requested_output_tokens;
        request.system = Some("synthetic CRLF\r\nUnicode: Tere maailm".into());
        request.history.push(ModelMessage {
            role: ModelMessageRole::User,
            content: "quoted \"history\"".into(),
            tool_exchange: None,
        });
        request.tools.push(ModelToolDefinition {
            name: "read_text".into(),
            description: "Read one synthetic path".into(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {"path": {"type": "string"}},
                "required": ["path"],
                "additionalProperties": false
            }),
        });
        (profile, budget, request)
    }

    #[tokio::test]
    async fn prepared_compatible_request_reuses_the_exact_admitted_body() {
        let response = event_stream_response(concat!(
            "data: {\"choices\":[{\"delta\":{\"content\":\"ok\"},\"finish_reason\":\"stop\"}]}\n\n",
            "data: [DONE]\n\n"
        ));
        let (endpoint, requests, worker) =
            spawn_mock_server_sequence(vec![response.clone(), response]);
        let provider = test_provider(&endpoint, None);
        let (profile, budget, request) = prepared_fixture(&endpoint);
        let expected = serde_json::to_vec(
            &build_chat_request_inner(request.clone(), false, false, false)
                .expect("legacy adapter body"),
        )
        .expect("legacy wire encoding");
        let prepared = provider
            .prepare_request(&profile, &budget, request)
            .expect("prepared request");
        let PreparedTransport::Compatible(compatible) = &prepared.transport else {
            panic!("compatible body was not retained");
        };
        assert_eq!(compatible.body.as_ref(), expected);
        assert_eq!(
            prepared.accounting().accounted_input,
            compatible.body.len() as u64
        );
        assert!(!format!("{prepared:?}").contains("Tere maailm"));
        let validation = provider
            .validate_prepared_request(&prepared)
            .await
            .expect("prepared validation");
        for _ in 0..2 {
            let (sender, mut receiver) = mpsc::channel(4);
            provider
                .stream_prepared_validated_with_cancellation(
                    &prepared,
                    &validation,
                    sender,
                    CancellationToken::new(),
                )
                .await
                .expect("prepared dispatch");
            assert!(matches!(
                receiver.recv().await,
                Some(ProviderEvent::Delta(_))
            ));
            assert_eq!(receiver.recv().await, Some(ProviderEvent::Completed));
        }
        let observed = requests
            .recv_timeout(Duration::from_secs(1))
            .expect("two observed requests");
        worker.join().expect("server worker");
        assert_eq!(observed.len(), 2);
        for wire in observed {
            let (headers, body) = wire.split_once("\r\n\r\n").expect("HTTP request");
            assert!(headers
                .to_ascii_lowercase()
                .contains("content-type: application/json"));
            assert_eq!(body.as_bytes(), expected);
        }

        let other = test_provider("http://127.0.0.1:1/v1/", None);
        assert!(matches!(
            other.prepared_compatible_body(&prepared),
            Err(ProviderError::PreparedRequestBindingMismatch)
        ));
    }

    fn tool_catalog_request(count: usize) -> ModelRequest {
        let mut request = empty_model_request();
        request.tools = (0..count)
            .map(|index| ModelToolDefinition {
                name: format!("tool_{index}"),
                description: "Bounded test tool".into(),
                parameters: serde_json::json!({"type":"object","properties":{}}),
            })
            .collect();
        request
    }

    #[tokio::test]
    async fn tool_contract_boundary_reaches_production_http_adapter() {
        let request = tool_catalog_request(ModelToolContract::MAX_DEFINITIONS);
        ModelToolContract::validate_catalog(&request.tools).unwrap();
        let server = spawn_observed_server(event_stream_response(concat!(
            "data: {\"choices\":[{\"delta\":{\"content\":\"ok\"},\"finish_reason\":\"stop\"}]}\n\n",
            "data: [DONE]\n\n"
        )));
        let provider = test_provider(&server.endpoint, None);
        let (sender, mut receiver) = mpsc::channel(4);
        provider.stream(request, sender).await.unwrap();
        assert!(matches!(
            receiver.recv().await,
            Some(ProviderEvent::Delta(_))
        ));
        assert_eq!(receiver.recv().await, Some(ProviderEvent::Completed));
        let wire = server.finish().expect("boundary request reached HTTP");
        let (_, body) = wire.split_once("\r\n\r\n").unwrap();
        let body: serde_json::Value = serde_json::from_str(body).unwrap();
        assert_eq!(
            body["tools"].as_array().unwrap().len(),
            ModelToolContract::MAX_DEFINITIONS
        );
    }

    #[tokio::test]
    async fn tool_contract_rejects_invalid_local_requests_before_http() {
        let mut malformed = tool_catalog_request(1);
        malformed.tools[0].parameters = serde_json::json!(["not-a-schema-canary"]);
        let mut oversized = tool_catalog_request(1);
        oversized.tools[0].parameters =
            serde_json::json!({"description":"x".repeat(ModelToolContract::MAX_SCHEMA_BYTES)});
        let mut duplicate = tool_catalog_request(2);
        duplicate.tools[1].name = duplicate.tools[0].name.clone();
        let mut invalid_exchange = tool_catalog_request(1);
        invalid_exchange.tool_exchanges.push(ModelToolExchange {
            chatgpt_continuation: None,
            assistant_continuation: None,
            assistant_content: String::new(),
            assistant_reasoning: String::new(),
            calls: Vec::new(),
            results: Vec::new(),
        });
        for request in [
            tool_catalog_request(ModelToolContract::MAX_DEFINITIONS + 1),
            malformed,
            oversized,
            duplicate,
            invalid_exchange,
        ] {
            let server = spawn_observed_server(event_stream_response("data: [DONE]\n\n"));
            let provider = test_provider(&server.endpoint, None);
            let (sender, _receiver) = mpsc::channel(4);
            let error = provider.stream(request, sender).await.unwrap_err();
            assert!(matches!(error, ProviderError::InvalidRequest(_)), "{error}");
            assert!(error
                .to_string()
                .starts_with("local model request is invalid:"));
            assert!(!error.to_string().contains("canary"));
            assert!(server.finish().is_none(), "invalid request reached HTTP");
        }
    }

    #[test]
    fn tool_contract_preserves_the_exact_schema_byte_boundary() {
        let mut request = tool_catalog_request(1);
        let base = serde_json::json!({"description":""});
        let overhead = serde_json::to_vec(&base).unwrap().len();
        request.tools[0].parameters = serde_json::json!({"description":"x".repeat(ModelToolContract::MAX_SCHEMA_BYTES - overhead)});
        assert_eq!(
            serde_json::to_vec(&request.tools[0].parameters)
                .unwrap()
                .len(),
            ModelToolContract::MAX_SCHEMA_BYTES
        );
        ModelToolContract::validate_catalog(&request.tools).unwrap();
        request.tools[0].parameters = serde_json::json!({"description":"x".repeat(ModelToolContract::MAX_SCHEMA_BYTES - overhead + 1)});
        assert!(matches!(
            ModelToolContract::validate_catalog(&request.tools),
            Err(ProviderError::InvalidRequest(_))
        ));
    }

    #[tokio::test]
    async fn tool_contract_pre_cancelled_boundary_request_makes_no_http_call() {
        let server = spawn_observed_server(event_stream_response("data: [DONE]\n\n"));
        let provider = test_provider(&server.endpoint, None);
        let cancellation = CancellationToken::new();
        cancellation.cancel();
        let (sender, _receiver) = mpsc::channel(4);
        let result = provider
            .stream_with_cancellation(
                tool_catalog_request(ModelToolContract::MAX_DEFINITIONS),
                sender,
                cancellation,
            )
            .await;
        assert!(matches!(result, Err(ProviderError::RequestCancelled)));
        assert!(server.finish().is_none());
    }

    #[test]
    fn local_sidecar_client_ignores_process_proxy_for_models() {
        if let Ok(proxy_spelling) = std::env::var(PROXY_CHILD_CASE) {
            run_proxy_child(&proxy_spelling);
            return;
        }

        let body = r#"{"data":[{"id":"local/model"}]}"#;
        for proxy_spelling in PROXY_ENVIRONMENT_NAMES {
            let direct = spawn_observed_server(json_response("200 OK", body));
            let proxy = spawn_observed_server(json_response("200 OK", body));

            run_proxy_child_process(proxy_spelling, &direct.endpoint, &proxy.endpoint);

            assert!(
                direct.finish().is_some(),
                "direct listener received no request for {proxy_spelling}"
            );
            assert!(
                proxy.finish().is_none(),
                "proxy listener received a request for {proxy_spelling}"
            );
        }
    }

    fn run_proxy_child(proxy_spelling: &str) {
        assert!(
            PROXY_ENVIRONMENT_NAMES.contains(&proxy_spelling),
            "unexpected child proxy spelling"
        );
        assert_eq!(
            std::env::var_os("NO_PROXY").as_deref(),
            Some(std::ffi::OsStr::new("")),
            "child did not explicitly clear NO_PROXY"
        );
        assert_eq!(
            std::env::var_os("no_proxy").as_deref(),
            Some(std::ffi::OsStr::new("")),
            "child did not explicitly clear no_proxy"
        );
        let direct_endpoint =
            std::env::var(PROXY_CHILD_DIRECT_ENDPOINT).expect("child direct endpoint");
        let provider = OpenAiCompatibleProvider::new_local_sidecar(
            &direct_endpoint,
            "local/model",
            SecretValue::new("proxy-test-secret"),
        )
        .expect("local sidecar provider");
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("child runtime");
        let models = runtime
            .block_on(provider.list_models())
            .expect("sidecar models");

        assert_eq!(models.len(), 1);
    }

    fn run_proxy_child_process(proxy_spelling: &str, direct_endpoint: &str, proxy_endpoint: &str) {
        let executable = std::env::current_exe().expect("test executable");
        let system_root = std::env::var_os("SystemRoot").expect("SystemRoot");
        let path = std::env::var_os("PATH").expect("PATH");
        let mut child = Command::new(executable)
            .arg("--exact")
            .arg("tests::local_sidecar_client_ignores_process_proxy_for_models")
            .arg("--nocapture")
            .env_clear()
            .env("SystemRoot", system_root)
            .env("PATH", path)
            .env(PROXY_CHILD_CASE, proxy_spelling)
            .env(PROXY_CHILD_DIRECT_ENDPOINT, direct_endpoint)
            .env(PROXY_CHILD_PROXY_ENDPOINT, proxy_endpoint)
            .env(proxy_spelling, proxy_endpoint)
            .env("NO_PROXY", "")
            .env("no_proxy", "")
            .stdin(Stdio::null())
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit())
            .spawn()
            .expect("spawn proxy child");
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            if let Some(status) = child.try_wait().expect("poll proxy child") {
                assert!(status.success(), "proxy child failed for {proxy_spelling}");
                return;
            }
            if Instant::now() >= deadline {
                child.kill().expect("stop timed-out proxy child");
                let _ = child.wait();
                panic!("proxy child timed out for {proxy_spelling}");
            }
            thread::sleep(Duration::from_millis(10));
        }
    }

    #[tokio::test]
    async fn local_sidecar_client_refuses_generation_redirect_without_forwarding_key() {
        let redirected = spawn_observed_server(event_stream_response(concat!(
            "data: {\"choices\":[{\"delta\":{\"content\":\"redirected\"},\"finish_reason\":null}]}\n\n",
            "data: {\"choices\":[{\"delta\":{},\"finish_reason\":\"stop\"}]}\n\n",
            "data: [DONE]\n\n"
        )));
        let redirect_response = format!(
            "HTTP/1.1 302 Found\r\nLocation: {}/redirected\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
            redirected.endpoint
        );
        let source = spawn_observed_server(redirect_response);
        let provider = OpenAiCompatibleProvider::new_local_sidecar(
            &source.endpoint,
            "local/model",
            SecretValue::new("redirect-test-secret"),
        )
        .expect("local sidecar provider");
        let (sender, _receiver) = mpsc::channel(4);

        let result = provider.stream(empty_model_request(), sender).await;

        assert!(result.is_err(), "sidecar generation followed a redirect");
        assert!(
            source.finish().is_some(),
            "source listener received no request"
        );
        assert!(
            redirected.finish().is_none(),
            "redirect target received a request"
        );
    }

    #[test]
    fn rejects_malformed_stream_chunks() {
        assert!(matches!(
            parse_chat_completion_chunk("not-json"),
            Err(ProviderError::InvalidResponse(_))
        ));
    }

    #[test]
    fn parses_stream_delta_content() {
        let chunk = parse_chat_completion_chunk(r#"{"choices":[{"delta":{"content":"hello"}}]}"#)
            .expect("stream chunk");
        assert_eq!(chunk.choices[0].delta.content.as_deref(), Some("hello"));
    }

    #[test]
    fn tagged_reasoning_filter_handles_split_markers_without_leaking_reasoning() {
        let mut filter = TaggedReasoningFilter::new(true);
        let chunks = ["Before <thi", "nk>private reasoning", "</thi", "nk>After"];
        let mut split = ReasoningSplit::default();
        for chunk in chunks {
            let next = filter.push(chunk).expect("reasoning chunk");
            split.visible.push_str(&next.visible);
            split.reasoning.push_str(&next.reasoning);
        }
        let trailing = filter.finish().expect("closed reasoning");
        split.visible.push_str(&trailing.visible);
        split.reasoning.push_str(&trailing.reasoning);

        assert_eq!(split.visible, "Before After");
        assert_eq!(split.reasoning, "private reasoning");
        assert!(!split.visible.contains("private"));
    }

    #[test]
    fn disabled_tagged_reasoning_filter_preserves_content_exactly() {
        let mut filter = TaggedReasoningFilter::new(false);
        let content = "<think>shown by provider policy</think>answer";
        let split = filter.push(content).expect("split tagged content");
        let trailing = filter.finish().expect("closed filter");
        assert_eq!(split.visible + &trailing.visible, "answer");
        assert_eq!(
            split.reasoning + &trailing.reasoning,
            "shown by provider policy"
        );
    }

    #[test]
    fn tagged_reasoning_filter_fails_closed_on_unclosed_hidden_content() {
        let mut filter = TaggedReasoningFilter::new(true);
        assert_eq!(
            filter
                .push("answer<think>sensitive reasoning")
                .expect("split"),
            ReasoningSplit {
                visible: "answer".into(),
                reasoning: "sensitive reasoning".into(),
            }
        );
        let error = filter.finish().expect_err("unclosed reasoning");
        assert!(!error.to_string().contains("sensitive"));
    }

    #[tokio::test]
    async fn accepts_terminal_finish_reason_when_stream_closes_without_done_marker() {
        let body = concat!(
            "data: {\"choices\":[{\"delta\":{\"content\":\"hello\"},\"finish_reason\":null}]}\n\n",
            "data: {\"choices\":[{\"delta\":{\"content\":null},\"finish_reason\":\"stop\"}]}\n\n"
        );
        let server = spawn_mock_server(event_stream_response(body), Duration::ZERO);
        let provider = test_provider(&server.endpoint, None);
        let (sender, mut receiver) = mpsc::channel(4);

        provider
            .stream(empty_model_request(), sender)
            .await
            .expect("terminal stream");

        assert!(
            matches!(receiver.recv().await, Some(ProviderEvent::Delta(text)) if text == "hello")
        );
        assert!(matches!(
            receiver.recv().await,
            Some(ProviderEvent::Completed)
        ));
        let request = server.finish();
        assert!(request.starts_with("POST /v1/chat/completions HTTP/1.1\r\n"));
    }

    #[tokio::test]
    async fn separates_streamed_reasoning_visible_text_and_usage() {
        let body = concat!(
            "data: {\"choices\":[{\"delta\":{\"reasoning_content\":\"step \"},\"finish_reason\":null}]}\n\n",
            "data: {\"choices\":[{\"delta\":{\"reasoning_content\":\"one\",\"content\":\"answer\"},\"finish_reason\":\"stop\"}]}\n\n",
            "data: {\"choices\":[],\"usage\":{\"prompt_tokens\":4,\"completion_tokens\":3,\"total_tokens\":7,\"prompt_tokens_details\":{\"cached_tokens\":2},\"completion_tokens_details\":{\"reasoning_tokens\":2}}}\n\n",
            "data: [DONE]\n\n"
        );
        let server = spawn_mock_server(event_stream_response(body), Duration::ZERO);
        let provider = test_provider(&server.endpoint, None);
        let (sender, mut receiver) = mpsc::channel(8);

        provider
            .stream(empty_model_request(), sender)
            .await
            .expect("reasoning stream");

        assert_eq!(
            receiver.recv().await,
            Some(ProviderEvent::ReasoningDelta("step ".into()))
        );
        assert_eq!(
            receiver.recv().await,
            Some(ProviderEvent::ReasoningDelta("one".into()))
        );
        assert_eq!(
            receiver.recv().await,
            Some(ProviderEvent::Delta("answer".into()))
        );
        assert_eq!(
            receiver.recv().await,
            Some(ProviderEvent::UsageObservation(
                argentum_domain::goal_accounting::ObservedUsage {
                    input_tokens: Some(4),
                    output_tokens: Some(3),
                    total_tokens: 7,
                    cached_input_tokens: Some(2),
                    reasoning_tokens: Some(2),
                }
            ))
        );
        assert_eq!(
            receiver.recv().await,
            Some(ProviderEvent::Usage(ModelUsage {
                input_tokens: 4,
                output_tokens: 3,
                total_tokens: 7,
                cached_input_tokens: Some(2),
                reasoning_tokens: Some(2),
                context_window_tokens: None,
            }))
        );
        assert_eq!(receiver.recv().await, Some(ProviderEvent::Completed));
        let request = server.finish();
        assert!(request.contains(r#""stream_options":{"include_usage":true}"#));
        assert!(!request.contains("reasoning_split"));
    }

    #[tokio::test]
    async fn strips_fragmented_tags_from_explicit_reasoning_fields() {
        let body = concat!(
            "data: {\"choices\":[{\"delta\":{\"reasoning_content\":\"<thi\"},\"finish_reason\":null}]}\n\n",
            "data: {\"choices\":[{\"delta\":{\"reasoning_content\":\"nk>private</thi\"},\"finish_reason\":null}]}\n\n",
            "data: {\"choices\":[{\"delta\":{\"reasoning_content\":\"nk>\",\"content\":\"answer\"},\"finish_reason\":\"stop\"}]}\n\n",
            "data: [DONE]\n\n"
        );
        let server = spawn_mock_server(event_stream_response(body), Duration::ZERO);
        let provider = test_provider(&server.endpoint, None);
        let (sender, mut receiver) = mpsc::channel(8);

        provider
            .stream(empty_model_request(), sender)
            .await
            .expect("tagged explicit reasoning stream");

        assert_eq!(
            receiver.recv().await,
            Some(ProviderEvent::ReasoningDelta("private".into()))
        );
        assert_eq!(
            receiver.recv().await,
            Some(ProviderEvent::Delta("answer".into()))
        );
        assert_eq!(receiver.recv().await, Some(ProviderEvent::Completed));
        let _ = server.finish();
    }

    #[tokio::test]
    async fn converts_cumulative_reasoning_details_to_deltas() {
        let body = concat!(
            "data: {\"choices\":[{\"delta\":{\"reasoning_details\":[{\"type\":\"reasoning.text\",\"text\":\"first\"}]},\"finish_reason\":null}]}\n\n",
            "data: {\"choices\":[{\"delta\":{\"reasoning_details\":[{\"type\":\"reasoning.text\",\"text\":\"first second\"}],\"content\":\"done\"},\"finish_reason\":\"stop\"}]}\n\n"
        );
        let server = spawn_mock_server(event_stream_response(body), Duration::ZERO);
        let provider = test_provider(&server.endpoint, None);
        let (sender, mut receiver) = mpsc::channel(8);

        provider
            .stream(empty_model_request(), sender)
            .await
            .expect("cumulative reasoning stream");

        assert_eq!(
            receiver.recv().await,
            Some(ProviderEvent::ReasoningDelta("first".into()))
        );
        assert_eq!(
            receiver.recv().await,
            Some(ProviderEvent::ReasoningDelta(" second".into()))
        );
        assert_eq!(
            receiver.recv().await,
            Some(ProviderEvent::Delta("done".into()))
        );
        assert_eq!(receiver.recv().await, Some(ProviderEvent::Completed));
        let _ = server.finish();
    }

    #[tokio::test]
    async fn accepts_incremental_reasoning_details_without_truncation() {
        let body = concat!(
            "data: {\"choices\":[{\"delta\":{\"reasoning_details\":[{\"type\":\"reasoning.text\",\"text\":\"The user\"}]},\"finish_reason\":null}]}\n\n",
            "data: {\"choices\":[{\"delta\":{\"reasoning_details\":[{\"type\":\"reasoning.text\",\"text\":\" is asking\"}]},\"finish_reason\":null}]}\n\n",
            "data: {\"choices\":[{\"delta\":{\"reasoning_details\":[{\"type\":\"reasoning.text\",\"text\":\" a question\"}],\"content\":\"done\"},\"finish_reason\":\"stop\"}]}\n\n"
        );
        let server = spawn_mock_server(event_stream_response(body), Duration::ZERO);
        let provider = test_provider(&server.endpoint, None);
        let (sender, mut receiver) = mpsc::channel(8);

        provider
            .stream(empty_model_request(), sender)
            .await
            .expect("incremental reasoning stream");

        for expected in ["The user", " is asking", " a question"] {
            assert_eq!(
                receiver.recv().await,
                Some(ProviderEvent::ReasoningDelta(expected.into()))
            );
        }
        assert_eq!(
            receiver.recv().await,
            Some(ProviderEvent::Delta("done".into()))
        );
        assert_eq!(receiver.recv().await, Some(ProviderEvent::Completed));
        let _ = server.finish();
    }

    #[tokio::test]
    async fn reasoning_details_can_switch_from_cumulative_to_incremental() {
        let body = concat!(
            "data: {\"choices\":[{\"delta\":{\"reasoning_details\":[{\"text\":\"first\"}]},\"finish_reason\":null}]}\n\n",
            "data: {\"choices\":[{\"delta\":{\"reasoning_details\":[{\"text\":\"first second\"}]},\"finish_reason\":null}]}\n\n",
            "data: {\"choices\":[{\"delta\":{\"reasoning_details\":[{\"text\":\" third\"}]},\"finish_reason\":null}]}\n\n",
            "data: {\"choices\":[{\"delta\":{\"reasoning_details\":[{\"text\":\"first\"}],\"content\":\"done\"},\"finish_reason\":\"stop\"}]}\n\n"
        );
        let server = spawn_mock_server(event_stream_response(body), Duration::ZERO);
        let provider = test_provider(&server.endpoint, None);
        let (sender, mut receiver) = mpsc::channel(8);

        provider
            .stream(empty_model_request(), sender)
            .await
            .expect("mixed reasoning stream");

        for expected in ["first", " second", " third", "first"] {
            assert_eq!(
                receiver.recv().await,
                Some(ProviderEvent::ReasoningDelta(expected.into()))
            );
        }
        assert_eq!(
            receiver.recv().await,
            Some(ProviderEvent::Delta("done".into()))
        );
        assert_eq!(receiver.recv().await, Some(ProviderEvent::Completed));
        let _ = server.finish();
    }

    #[test]
    fn canonical_minimax_reasoning_details_preserve_incremental_prefix_collisions() {
        let provider = test_provider("https://api.minimax.io/v1/", None);
        let mut reasoning =
            ExplicitReasoningAccumulator::new(provider.initial_reasoning_details_mode());

        assert_eq!(reasoning.push_details(" ").expect("first detail"), " ");
        assert_eq!(
            reasoning.push_details(" step").expect("second detail"),
            " step"
        );
        assert_eq!(reasoning.details_mode, ReasoningDetailsMode::Incremental);
    }

    #[test]
    fn generic_reasoning_details_retain_cumulative_snapshot_support() {
        let provider = test_provider("https://compatible.example.test/v1/", None);
        let mut reasoning =
            ExplicitReasoningAccumulator::new(provider.initial_reasoning_details_mode());

        assert_eq!(
            reasoning.push_details("first").expect("first snapshot"),
            "first"
        );
        assert_eq!(
            reasoning
                .push_details("first second")
                .expect("second snapshot"),
            " second"
        );
        assert_eq!(reasoning.details_mode, ReasoningDetailsMode::Cumulative);
    }

    #[tokio::test]
    async fn reasoning_content_takes_precedence_over_duplicate_details() {
        let body = "data: {\"choices\":[{\"delta\":{\"reasoning_content\":\"direct\",\"reasoning_details\":[{\"text\":\"duplicate\"}],\"content\":\"done\"},\"finish_reason\":\"stop\"}]}\n\n";
        let server = spawn_mock_server(event_stream_response(body), Duration::ZERO);
        let provider = test_provider(&server.endpoint, None);
        let (sender, mut receiver) = mpsc::channel(8);

        provider
            .stream(empty_model_request(), sender)
            .await
            .expect("explicit reasoning stream");

        assert_eq!(
            receiver.recv().await,
            Some(ProviderEvent::ReasoningDelta("direct".into()))
        );
        assert_eq!(
            receiver.recv().await,
            Some(ProviderEvent::Delta("done".into()))
        );
        assert_eq!(receiver.recv().await, Some(ProviderEvent::Completed));
        let _ = server.finish();
    }

    #[tokio::test]
    async fn tagged_reasoning_fallback_never_leaks_tags_into_visible_text() {
        let body = concat!(
            "data: {\"choices\":[{\"delta\":{\"content\":\"before <thi\"},\"finish_reason\":null}]}\n\n",
            "data: {\"choices\":[{\"delta\":{\"content\":\"nk>reason</thi\"},\"finish_reason\":null}]}\n\n",
            "data: {\"choices\":[{\"delta\":{\"content\":\"nk> after\"},\"finish_reason\":\"stop\"}]}\n\n"
        );
        let server = spawn_mock_server(event_stream_response(body), Duration::ZERO);
        let provider = test_provider(&server.endpoint, None);
        let (sender, mut receiver) = mpsc::channel(8);

        provider
            .stream(empty_model_request(), sender)
            .await
            .expect("tagged reasoning stream");

        let mut visible = String::new();
        let mut reasoning = String::new();
        while let Some(event) = receiver.recv().await {
            match event {
                ProviderEvent::Delta(delta) => visible.push_str(&delta),
                ProviderEvent::ReasoningDelta(delta) => reasoning.push_str(&delta),
                ProviderEvent::Completed => break,
                ProviderEvent::ToolCall(_)
                | ProviderEvent::AssistantContinuation(_)
                | ProviderEvent::ChatGptContinuation(_)
                | ProviderEvent::Usage(_)
                | ProviderEvent::UsageObservation(_)
                | ProviderEvent::UsageUncertain => {}
            }
        }
        assert_eq!(visible, "before  after");
        assert_eq!(reasoning, "reason");
        assert!(!visible.contains("think"));
        let _ = server.finish();
    }

    #[test]
    fn minimax_request_options_and_reasoning_replay_are_provider_specific() {
        let minimax = test_provider("https://api.minimax.io/v1/", None);
        let deepseek = test_provider("https://api.deepseek.com/", None);
        assert!(minimax.supports_minimax_reasoning_protocol());
        assert!(!deepseek.supports_minimax_reasoning_protocol());
        assert!(!minimax.supports_deepseek_protocol());
        assert!(deepseek.supports_deepseek_protocol());

        let exchange = ModelToolExchange {
            chatgpt_continuation: None,
            assistant_continuation: None,
            assistant_content: String::new(),
            assistant_reasoning: "complete reasoning".into(),
            calls: vec![ModelToolCall {
                id: "call_1".into(),
                name: "read_text".into(),
                arguments: r#"{"path":"notes.txt"}"#.into(),
            }],
            results: vec![ModelToolResult {
                images: Default::default(),
                call_id: "call_1".into(),
                content: "result".into(),
            }],
        };
        let minimax_messages = build_messages(
            None,
            Vec::new(),
            "prompt".into(),
            vec![exchange.clone()],
            true,
        );
        let deepseek_messages =
            build_messages(None, Vec::new(), "prompt".into(), vec![exchange], false);
        // Display text cannot manufacture a provider continuation object.
        assert!(minimax_messages[1].reasoning_details.is_none());
        assert!(deepseek_messages[1].reasoning_details.is_none());

        let body = serde_json::to_value(
            build_chat_request_inner(tool_catalog_request(1), false, true, false)
                .expect("DeepSeek request"),
        )
        .expect("request JSON");
        assert_eq!(body["thinking"]["type"], "disabled");
        assert!(body.get("reasoning_split").is_none());
        assert!(body["tools"].is_array());
    }

    #[test]
    fn required_tool_choice_is_explicit_catalog_bound_and_non_thinking_deepseek() {
        let ordinary = serde_json::to_value(
            build_chat_request_inner(tool_catalog_request(1), false, true, false).unwrap(),
        )
        .unwrap();
        assert!(ordinary.get("tool_choice").is_none());
        let mut request = tool_catalog_request(1);
        request.require_tool_call = true;
        let required = serde_json::to_value(
            build_chat_request_inner(request.clone(), false, true, false).unwrap(),
        )
        .unwrap();
        assert_eq!(required["tool_choice"], "required");
        assert_eq!(required["thinking"]["type"], "disabled");
        assert_eq!(required["tools"], ordinary["tools"]);
        assert_eq!(required["max_tokens"], ordinary["max_tokens"]);
        assert!(
            serde_json::to_vec(&required).unwrap().len()
                > serde_json::to_vec(&ordinary).unwrap().len()
        );
        request.tools.clear();
        assert!(matches!(
            build_chat_request_inner(request, false, true, false),
            Err(ProviderError::InvalidRequest(_))
        ));
        let mut sidecar = tool_catalog_request(1);
        sidecar.require_tool_call = true;
        assert!(matches!(
            build_chat_request_inner(sidecar, false, false, true),
            Err(ProviderError::InvalidRequest(_))
        ));
    }

    #[test]
    fn reasoning_and_usage_fail_closed_when_inconsistent_or_oversized() {
        let mut reasoning = ExplicitReasoningAccumulator::default();
        assert!(reasoning
            .push_delta(&"r".repeat(MAX_REASONING_RESPONSE_BYTES + 1))
            .is_err());
        let mut details = ExplicitReasoningAccumulator::default();
        details
            .push_details(&"r".repeat(MAX_REASONING_RESPONSE_BYTES))
            .expect("reasoning at the limit");
        assert!(details.push_details("overflow").is_err());
        let usage = ChatUsage {
            prompt_tokens: 3,
            completion_tokens: 4,
            total_tokens: 8,
            prompt_tokens_details: None,
            completion_tokens_details: None,
        };
        assert!(ModelUsage::try_from(usage).is_err());

        let mut tags = ReasoningTagFilter::default();
        assert_eq!(tags.push("<thi"), "");
        assert_eq!(tags.push("nk>reason</thi"), "reason");
        assert_eq!(tags.push("nk>"), "");
        assert_eq!(tags.finish().expect("complete tags"), "");
    }

    #[tokio::test]
    async fn rejects_truncated_and_filtered_finish_reasons() {
        for (finish_reason, expected) in [("length", "truncated"), ("content_filter", "filtered")] {
            let body = format!(
                "data: {{\"choices\":[{{\"delta\":{{\"content\":\"partial\"}},\"finish_reason\":\"{finish_reason}\"}}]}}\n\n"
            );
            let server = spawn_mock_server(event_stream_response(&body), Duration::ZERO);
            let provider = test_provider(&server.endpoint, None);
            let (sender, _receiver) = mpsc::channel(4);

            let error = provider
                .stream(empty_model_request(), sender)
                .await
                .expect_err("unsafe finish reason");
            assert!(error.to_string().contains(expected));
            let _ = server.finish();
        }
    }

    #[tokio::test]
    async fn hidden_reasoning_must_close_before_stream_completion() {
        let body = concat!(
            "data: {\"choices\":[{\"delta\":{\"content\":\"<think>private\"},\"finish_reason\":null}]}\n\n",
            "data: {\"choices\":[{\"delta\":{\"content\":null},\"finish_reason\":\"stop\"}]}\n\n"
        );
        let server = spawn_mock_server(event_stream_response(body), Duration::ZERO);
        let provider = test_provider(&server.endpoint, None).with_hidden_tagged_reasoning(true);
        let (sender, mut receiver) = mpsc::channel(4);

        let error = provider
            .stream(empty_model_request(), sender)
            .await
            .expect_err("unclosed reasoning");
        assert!(error.to_string().contains("was not closed"));
        assert!(!error.to_string().contains("private"));
        assert!(matches!(
            receiver.try_recv(),
            Ok(ProviderEvent::ReasoningDelta(reasoning)) if reasoning == "private"
        ));
        assert!(receiver.try_recv().is_err());
        let _ = server.finish();
    }

    #[tokio::test]
    async fn streams_a_fragmented_openai_tool_call_before_completion() {
        let body = concat!(
            "data: {\"choices\":[{\"delta\":{\"tool_calls\":[{\"index\":0,\"id\":\"call_\",\"type\":\"function\",\"function\":{\"name\":\"read_\",\"arguments\":\"{\\\"pa\"}}]},\"finish_reason\":null}]}\n\n",
            "data: {\"choices\":[{\"delta\":{\"tool_calls\":[{\"index\":0,\"id\":\"1\",\"function\":{\"name\":\"text\",\"arguments\":\"th\\\":\\\"notes.txt\\\"}\"}}]},\"finish_reason\":\"tool_calls\"}]}\n\n"
        );
        let server = spawn_mock_server(event_stream_response(body), Duration::ZERO);
        let provider = test_provider(&server.endpoint, None);
        let (sender, mut receiver) = mpsc::channel(4);

        provider
            .stream(empty_model_request(), sender)
            .await
            .expect("tool-call stream");
        assert_eq!(
            receiver.recv().await,
            Some(ProviderEvent::ToolCall(ModelToolCall {
                id: "call_1".into(),
                name: "read_text".into(),
                arguments: r#"{"path":"notes.txt"}"#.into(),
            }))
        );
        assert_eq!(receiver.recv().await, Some(ProviderEvent::Completed));
        let _ = server.finish();
    }

    #[test]
    fn historical_exchanges_preserve_pairing_without_replaying_reasoning() {
        let mut request = empty_model_request();
        request.history = vec![
            ModelMessage {
                role: ModelMessageRole::User,
                content: "old task".into(),
                tool_exchange: None,
            },
            ModelMessage {
                role: ModelMessageRole::Assistant,
                content: String::new(),
                tool_exchange: Some(ModelToolExchange {
                    chatgpt_continuation: None,
                    assistant_continuation: None,
                    assistant_content: "historical observation".into(),
                    assistant_reasoning: "PRIVATE_OLD_REASONING".into(),
                    calls: vec![ModelToolCall {
                        id: "old-call".into(),
                        name: "read_text".into(),
                        arguments: "{\"path\":\"fixture.txt\"}".into(),
                    }],
                    results: vec![ModelToolResult {
                        images: Default::default(),
                        call_id: "old-call".into(),
                        content: "original result".into(),
                    }],
                }),
            },
            ModelMessage {
                role: ModelMessageRole::Assistant,
                content: "old final".into(),
                tool_exchange: None,
            },
        ];
        validate_model_tool_payload(&request).unwrap();
        let messages = serde_json::to_value(build_messages(
            Some("fresh system".into()),
            request.history.clone(),
            "current task".into(),
            Vec::new(),
            false,
        ))
        .unwrap();
        assert_eq!(
            messages
                .as_array()
                .unwrap()
                .iter()
                .map(|message| message["role"].as_str().unwrap())
                .collect::<Vec<_>>(),
            ["system", "user", "assistant", "tool", "assistant", "user"]
        );
        assert_eq!(messages[2]["tool_calls"][0]["id"], "old-call");
        assert_eq!(messages[3]["tool_call_id"], "old-call");
        assert_eq!(messages[3]["content"], "original result");
        assert!(!messages.to_string().contains("PRIVATE_OLD_REASONING"));
        let minimax = serde_json::to_value(build_messages(
            Some("fresh system".into()),
            request.history.clone(),
            "current task".into(),
            Vec::new(),
            true,
        ))
        .unwrap();
        assert_eq!(
            minimax
                .as_array()
                .unwrap()
                .iter()
                .map(|message| message["role"].as_str().unwrap())
                .collect::<Vec<_>>(),
            ["system", "user", "assistant", "assistant", "user"]
        );
        let observation: serde_json::Value =
            serde_json::from_str(minimax[2]["content"].as_str().unwrap()).unwrap();
        assert_eq!(observation["historical_observation"], true);
        assert_eq!(observation["original_private_continuation"], "unavailable");
        assert_eq!(observation["observed_calls"][0]["id"], "old-call");
        assert_eq!(observation["observed_results"][0]["call_id"], "old-call");
        assert_eq!(
            observation["observed_results"][0]["content"],
            "original result"
        );
        assert!(minimax
            .as_array()
            .unwrap()
            .iter()
            .all(|message| message.get("tool_calls").is_none()
                && message.get("tool_call_id").is_none()));
        assert!(!minimax.to_string().contains("PRIVATE_OLD_REASONING"));
        request.history[1].tool_exchange.as_mut().unwrap().results[0].call_id = "orphan".into();
        assert!(validate_model_tool_payload(&request).is_err());
    }

    #[test]
    fn openai_messages_preserve_system_history_and_current_prompt_order() {
        let messages = build_messages(
            Some("system".into()),
            vec![
                ModelMessage {
                    role: ModelMessageRole::User,
                    content: "earlier user".into(),
                    tool_exchange: None,
                },
                ModelMessage {
                    role: ModelMessageRole::Assistant,
                    content: "earlier assistant".into(),
                    tool_exchange: None,
                },
            ],
            "current user".into(),
            Vec::new(),
            false,
        );

        assert_eq!(messages.len(), 4);
        assert_eq!(messages[0].role, "system");
        assert_eq!(messages[0].content.as_deref(), Some("system"));
        assert_eq!(messages[1].role, "user");
        assert_eq!(messages[1].content.as_deref(), Some("earlier user"));
        assert_eq!(messages[2].role, "assistant");
        assert_eq!(messages[2].content.as_deref(), Some("earlier assistant"));
        assert_eq!(messages[3].role, "user");
        assert_eq!(messages[3].content.as_deref(), Some("current user"));
    }

    #[test]
    fn accumulates_fragmented_openai_tool_calls_without_exposing_arguments_in_errors() {
        let chunks = [
            r#"{"choices":[{"delta":{"tool_calls":[{"index":0,"id":"call_","type":"function","function":{"name":"read_","arguments":"{\"pa"}}]}}]}"#,
            r#"{"choices":[{"delta":{"tool_calls":[{"index":0,"id":"1","function":{"name":"text","arguments":"th\":\"notes.txt\"}"}}]}}]}"#,
        ];
        let mut calls = ToolCallAccumulator::default();
        for chunk in chunks {
            let chunk = parse_chat_completion_chunk(chunk).expect("tool chunk");
            for choice in chunk.choices {
                calls.push(choice.delta.tool_calls).expect("tool fragment");
            }
        }

        assert_eq!(
            calls.finish().expect("completed tool call"),
            vec![ModelToolCall {
                id: "call_1".into(),
                name: "read_text".into(),
                arguments: r#"{"path":"notes.txt"}"#.into(),
            }]
        );

        let mut oversized = ToolCallAccumulator::default();
        let error = oversized
            .push(vec![ChatToolCallDelta {
                index: 0,
                id: Some("call_2".into()),
                kind: Some("function".into()),
                function: Some(ChatFunctionCallDelta {
                    name: Some("write_text".into()),
                    arguments: Some("sensitive".repeat(MAX_TOOL_ARGUMENT_BYTES)),
                }),
            }])
            .expect_err("oversized arguments");
        assert!(!error.to_string().contains("sensitive"));
    }

    #[test]
    fn fragmented_openai_edit_arguments_decode_json_once_and_preserve_source_escapes() {
        let arguments = serde_json::json!({
            "path": "source.rs",
            "expected_sha256": "a".repeat(64),
            "old_text": "let literal = \\\"\\\\n\\\";\nprintln!(\\\"näide 中\\\");",
            "new_text": "let literal = \\\"\\\\r\\\\n\\\";\nprintln!(\\\"changed 中\\\");",
            "newline_mode": "preserve",
        })
        .to_string();
        assert!(arguments.contains("\\\\n"));
        assert!(arguments.contains("\\nprintln"));

        let mut fragments = Vec::new();
        let mut start = 0;
        for (index, _) in arguments.char_indices().skip(1) {
            if index - start >= 3 {
                fragments.push(arguments[start..index].to_owned());
                start = index;
            }
        }
        fragments.push(arguments[start..].to_owned());

        let mut calls = ToolCallAccumulator::default();
        for (index, fragment) in fragments.into_iter().enumerate() {
            let chunk = serde_json::json!({
                "choices": [{
                    "delta": {
                        "tool_calls": [{
                            "index": 0,
                            "id": (index == 0).then_some("call-preserve"),
                            "type": (index == 0).then_some("function"),
                            "function": {
                                "name": (index == 0).then_some("edit_text"),
                                "arguments": fragment,
                            }
                        }]
                    }
                }]
            });
            let chunk = parse_chat_completion_chunk(&chunk.to_string()).expect("tool chunk");
            for choice in chunk.choices {
                calls.push(choice.delta.tool_calls).expect("tool fragment");
            }
        }
        let call = calls.finish().expect("completed call").remove(0);
        assert_eq!(call.arguments, arguments);
        let decoded: serde_json::Value = serde_json::from_str(&call.arguments).unwrap();
        assert_eq!(
            decoded["old_text"],
            "let literal = \\\"\\\\n\\\";\nprintln!(\\\"näide 中\\\");"
        );
        assert_eq!(
            decoded["new_text"],
            "let literal = \\\"\\\\r\\\\n\\\";\nprintln!(\\\"changed 中\\\");"
        );
        assert_eq!(decoded["newline_mode"], "preserve");
    }

    #[test]
    fn serializes_tool_exchanges_in_openai_message_order() {
        let messages = build_messages(
            Some("system".into()),
            Vec::new(),
            "inspect".into(),
            vec![ModelToolExchange {
                chatgpt_continuation: None,
                assistant_continuation: None,
                assistant_content: String::new(),
                assistant_reasoning: "same-round reasoning".into(),
                calls: vec![ModelToolCall {
                    id: "call_1".into(),
                    name: "read_text".into(),
                    arguments: r#"{"path":"notes.txt"}"#.into(),
                }],
                results: vec![ModelToolResult {
                    images: Default::default(),
                    call_id: "call_1".into(),
                    content: "bounded result".into(),
                }],
            }],
            true,
        );

        assert_eq!(messages.len(), 4);
        assert_eq!(messages[2].role, "assistant");
        assert!(messages[2].content.is_none());
        assert!(messages[2].reasoning_details.is_none());
        assert_eq!(
            messages[2].tool_calls.as_ref().expect("tool calls")[0]
                .function
                .name,
            "read_text"
        );
        assert_eq!(messages[3].role, "tool");
        assert_eq!(messages[3].tool_call_id.as_deref(), Some("call_1"));
        assert_eq!(messages[3].content.as_deref(), Some("bounded result"));
    }

    #[test]
    fn joins_models_url_from_normalized_base_path() {
        let provider = test_provider("http://127.0.0.1:1234/v1?ignored=value#fragment", None);

        assert_eq!(
            provider.models_url().expect("models URL").as_str(),
            "http://127.0.0.1:1234/v1/models"
        );
    }

    #[test]
    fn public_status_redacts_url_credentials_and_query() {
        let provider = test_provider(
            "https://user:password@example.test/v1?api_key=sensitive#fragment",
            None,
        );

        let status = provider.status();
        assert_eq!(status.endpoint, "https://example.test/v1/");
        assert!(!status.endpoint.contains("password"));
        assert!(!status.endpoint.contains("sensitive"));
        assert!(matches!(
            provider.models_url(),
            Err(ProviderError::UnsafeProbeEndpoint { .. })
        ));
    }

    #[tokio::test]
    async fn probe_accepts_successful_bounded_models_response() {
        let server = spawn_mock_server(
            json_response(
                "200 OK",
                r#"{"object":"list","data":[{"id":"local/model"}]}"#,
            ),
            Duration::ZERO,
        );
        let provider = test_provider(&server.endpoint, None);

        let status = provider.probe().await.expect("successful probe");
        assert!(status.connected);
        assert_eq!(status.detail, "Reachable; configured model: local/model");
        let request = server.finish();
        assert!(request.starts_with("GET /v1/models HTTP/1.1\r\n"));
    }

    #[tokio::test]
    async fn model_catalog_is_bounded_sorted_and_deduplicated() {
        let server = spawn_mock_server(
            json_response(
                "200 OK",
                r#"{"data":[{"id":"model-z"},{"id":" model-a "},{"id":"model-z"}]}"#,
            ),
            Duration::ZERO,
        );
        let provider = test_provider(&server.endpoint, None);

        let models = provider.list_models().await.expect("model catalog");

        assert_eq!(
            models,
            vec![
                ProviderModel {
                    id: "model-a".into(),
                    label: "model-a".into(),
                    context_window_tokens: None,
                },
                ProviderModel {
                    id: "model-z".into(),
                    label: "model-z".into(),
                    context_window_tokens: None,
                },
            ]
        );
        let request = server.finish();
        assert!(request.starts_with("GET /v1/models HTTP/1.1\r\n"));
    }

    #[test]
    fn lm_studio_native_catalog_filters_llms_and_preserves_exact_context() {
        let body = br#"{
            "models": [
                {
                    "type": "llm",
                    "key": "publisher/model",
                    "display_name": "Model",
                    "max_context_length": 131072,
                    "loaded_instances": [
                        {"id":"publisher/model:loaded","config":{"context_length":32768}},
                        {"id":"publisher/model","config":{"context_length":65536}}
                    ]
                },
                {
                    "type": "embedding",
                    "key": "publisher/embed",
                    "display_name": "Embed",
                    "max_context_length": 4096,
                    "loaded_instances": []
                }
            ]
        }"#;

        let models = parse_lm_studio_models_response(body).expect("native catalog");

        assert_eq!(
            models,
            vec![
                ProviderModel {
                    id: "publisher/model".into(),
                    label: "Model".into(),
                    context_window_tokens: Some(65_536),
                },
                ProviderModel {
                    id: "publisher/model:loaded".into(),
                    label: "Model".into(),
                    context_window_tokens: Some(32_768),
                },
            ]
        );
    }

    #[test]
    fn lm_studio_native_models_url_uses_the_same_origin() {
        let provider =
            LocalLmStudioProvider::new("http://127.0.0.1:1234/custom/v1/", "local-model")
                .expect("LM Studio provider");
        assert_eq!(
            provider.native_models_url().expect("native URL").as_str(),
            "http://127.0.0.1:1234/api/v1/models"
        );
    }

    #[tokio::test]
    async fn lm_studio_lists_models_from_the_native_endpoint() {
        let server = spawn_mock_server(
            json_response(
                "200 OK",
                r#"{"models":[{"type":"llm","key":"local/model","display_name":"Local model","max_context_length":131072,"loaded_instances":[{"id":"local/model:loaded","config":{"context_length":32768}}]}]}"#,
            ),
            Duration::ZERO,
        );
        let provider = LocalLmStudioProvider::new(&server.endpoint, "local/model")
            .expect("LM Studio provider");

        let models = provider.list_models().await.expect("native models");

        assert_eq!(models.len(), 2);
        assert_eq!(models[0].id, "local/model");
        assert_eq!(models[0].context_window_tokens, Some(131_072));
        assert_eq!(models[1].id, "local/model:loaded");
        assert_eq!(models[1].context_window_tokens, Some(32_768));
        let request = server.finish();
        assert!(request.starts_with("GET /api/v1/models HTTP/1.1\r\n"));
    }

    #[tokio::test]
    async fn lm_studio_falls_back_to_openai_catalog_when_native_is_unavailable() {
        let (endpoint, requests, handle) = spawn_mock_server_sequence(vec![
            json_response("404 Not Found", r#"{"error":"unsupported"}"#),
            json_response("200 OK", r#"{"data":[{"id":"fallback-model"}]}"#),
        ]);
        let provider =
            LocalLmStudioProvider::new(&endpoint, "fallback-model").expect("LM Studio provider");

        let models = provider.list_models().await.expect("fallback models");

        assert_eq!(models[0].id, "fallback-model");
        let requests = requests
            .recv_timeout(Duration::from_secs(1))
            .expect("requests");
        handle.join().expect("server");
        assert!(requests[0].starts_with("GET /api/v1/models HTTP/1.1\r\n"));
        assert!(requests[1].starts_with("GET /v1/models HTTP/1.1\r\n"));
    }

    #[test]
    fn canonical_context_windows_are_exact_and_unknown_models_remain_unknown() {
        for model in [
            "MiniMax-M2.7",
            "MiniMax-M2.7-highspeed",
            "MiniMax-M2.5",
            "MiniMax-M2.5-highspeed",
            "MiniMax-M2.1",
            "MiniMax-M2.1-highspeed",
            "MiniMax-M2",
        ] {
            assert_eq!(canonical_context_window("minimax", model), Some(204_800));
        }
        assert_eq!(
            canonical_context_window("minimax", "MiniMax-M3"),
            Some(1_000_000)
        );
        for model in [
            "deepseek-flash",
            "deepseek-v4-pro",
            "deepseek-v4-flash",
            "deepseek-v4-flash-vision-exp",
        ] {
            assert_eq!(canonical_context_window("deepseek", model), Some(1_000_000));
        }
        for model in ["gpt-5.6", "gpt-5.6-sol", "gpt-5.6-terra", "gpt-5.6-luna"] {
            assert_eq!(canonical_context_window("openai", model), Some(1_050_000));
        }
        assert_eq!(canonical_context_window("minimax", "future-model"), None);
        assert_eq!(canonical_context_window("custom", "MiniMax-M3"), None);
        assert!(is_supported_openai_chat_model("gpt-5.6-sol"));
        for unsupported in [
            "text-embedding-3-large",
            "omni-moderation-latest",
            "gpt-image-1",
            "whisper-1",
        ] {
            assert!(!is_supported_openai_chat_model(unsupported));
        }
    }

    #[tokio::test]
    async fn probe_rejects_non_success_without_exposing_secret() {
        let secret = "sk-test-sensitive-value";
        let server = spawn_mock_server(
            json_response(
                "401 Unauthorized",
                &format!(r#"{{"error":"reflected {secret}"}}"#),
            ),
            Duration::ZERO,
        );
        let provider = test_provider(&server.endpoint, Some(SecretValue::new(secret)));

        let error = provider.probe().await.expect_err("failed probe");
        assert!(matches!(
            &error,
            ProviderError::ProbeHttpStatus { status: 401 }
        ));
        assert!(!error.to_string().contains(secret));
        assert!(!format!("{error:?}").contains(secret));
        assert!(!format!("{provider:?}").contains(secret));

        let request = server.finish().to_ascii_lowercase();
        assert!(request.contains(&format!("authorization: bearer {secret}")));
    }

    #[tokio::test]
    async fn probe_rejects_invalid_models_shape() {
        let server = spawn_mock_server(
            json_response("200 OK", r#"{"object":"list","data":[{"id":42}]}"#),
            Duration::ZERO,
        );
        let provider = test_provider(&server.endpoint, None);

        assert!(matches!(
            provider.probe().await,
            Err(ProviderError::InvalidResponse(_))
        ));
        let _ = server.finish();
    }

    #[tokio::test]
    async fn compatible_http_503_is_classified_without_body_exposure_or_hidden_retry() {
        let server = spawn_observed_server(
            "HTTP/1.1 503 Service Unavailable\r\nContent-Type: text/event-stream\r\nRetry-After: 2\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".into(),
        );
        let provider = test_provider(&server.endpoint, None);
        let (sender, _receiver) = mpsc::channel(2);

        let error = provider
            .stream(empty_model_request(), sender)
            .await
            .expect_err("transient HTTP rejection");
        assert_eq!(
            error.retry_class(),
            Some(ProviderRetryClass::TransientHttpStatus {
                status: 503,
                retry_after_ms: Some(2_000),
            })
        );
        assert!(!error.to_string().contains("Service Unavailable"));
        let request = server.finish().expect("one physical request");
        assert!(request.starts_with("POST "));
    }

    #[tokio::test]
    async fn compatible_minimax_http_529_has_bounded_retry_class_without_hidden_retry() {
        for minimax in [true, false] {
            let response_body = serde_json::json!({
                "error": {
                    "type": "overloaded_error",
                    "message": "The server cluster is currently under high load. (2064)",
                    "request_id": "synthetic-overload-529",
                    "private_context": "must not be exposed"
                }
            })
            .to_string();
            let server = spawn_observed_server(format!(
                "HTTP/1.1 529 Overloaded\r\nContent-Type: application/json\r\nRetry-After: 2\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                response_body.len(), response_body
            ));
            let mut provider = test_provider(&server.endpoint, None);
            if minimax {
                provider.endpoint = Url::parse("https://api.minimax.io/v1/").unwrap();
                provider.test_completion_endpoint = Some(Url::parse(&server.endpoint).unwrap());
            }
            let (sender, mut receiver) = mpsc::channel(2);
            let error = provider
                .stream(empty_model_request(), sender)
                .await
                .expect_err("synthetic overload");
            let request = server.finish().expect("one observable physical request");
            assert!(request.starts_with("POST "));
            assert!(
                receiver.try_recv().is_err(),
                "no response or usage invented"
            );
            assert!(!error.to_string().contains("private_context"));
            assert!(!format!("{error:?}").contains("must not be exposed"));
            if minimax {
                assert_eq!(
                    error.retry_class(),
                    Some(ProviderRetryClass::TransientHttpStatus {
                        status: 529,
                        retry_after_ms: Some(2_000),
                    })
                );
            } else {
                assert!(matches!(
                    error,
                    ProviderError::ApiHttpStatus { status: 529, .. }
                ));
                assert!(error.retry_class().is_none());
            }
        }
    }

    #[tokio::test]
    async fn compatible_http_429_retains_only_bounded_sanitized_structured_detail() {
        let secret = "sk-test-sensitive-provider-key";
        let response_body = serde_json::json!({
            "error": {
                "code": 2062,
                "message": format!("Weekly quota exhausted for {secret}\nretry after reset"),
                "request_id": "req-safe-42",
                "private_context": "must not be retained"
            },
            "untrusted_extra": "must not be retained"
        })
        .to_string();
        let server = spawn_observed_server(format!(
            "HTTP/1.1 429 Too Many Requests\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            response_body.len(),
            response_body
        ));
        let provider = test_provider(&server.endpoint, Some(SecretValue::new(secret)));
        let (sender, _receiver) = mpsc::channel(2);

        let error = provider
            .stream(empty_model_request(), sender)
            .await
            .expect_err("rate-limited request");
        let ProviderError::TransientHttpStatus {
            status,
            retry_after_ms,
            safe_detail,
        } = &error
        else {
            panic!("unexpected error: {error}");
        };
        assert_eq!(*status, 429);
        assert_eq!(*retry_after_ms, None);
        assert_eq!(
            safe_detail,
            "; provider detail: code=2062, message=Weekly quota exhausted for [REDACTED] retry after reset, request_id=req-safe-42"
        );
        assert!(!error.to_string().contains(secret));
        assert!(!format!("{error:?}").contains(secret));
        assert!(!error.to_string().contains("private_context"));
        assert!(!error.to_string().contains("untrusted_extra"));
        assert_eq!(
            error.retry_class(),
            Some(ProviderRetryClass::TransientHttpStatus {
                status: 429,
                retry_after_ms: None,
            })
        );
        assert!(server
            .finish()
            .expect("one physical request")
            .starts_with("POST "));
    }

    #[tokio::test]
    async fn compatible_http_error_omits_oversized_or_unstructured_body() {
        for body in [
            "x".repeat(MAX_PROVIDER_ERROR_BODY_BYTES + 1),
            "plain provider failure".into(),
        ] {
            let server = spawn_observed_server(format!(
                "HTTP/1.1 429 Too Many Requests\r\nContent-Type: text/plain\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                body.len(),
                body
            ));
            let provider = test_provider(&server.endpoint, None);
            let (sender, _receiver) = mpsc::channel(2);
            let error = provider
                .stream(empty_model_request(), sender)
                .await
                .expect_err("rejected request");
            let ProviderError::TransientHttpStatus { safe_detail, .. } = &error else {
                panic!("unexpected error: {error}");
            };
            assert!(safe_detail.is_empty());
            assert_eq!(error.to_string(), "provider returned retryable HTTP 429");
            let _ = server.finish();
        }
    }

    #[test]
    fn retry_after_parser_is_bounded_and_numeric_only() {
        assert_eq!(
            parse_retry_after(Some(&HeaderValue::from_static("15"))),
            Some(15_000)
        );
        assert_eq!(
            parse_retry_after(Some(&HeaderValue::from_static("9999999999"))),
            Some(9_999_999_999_000)
        );
        assert_eq!(
            parse_retry_after(Some(&HeaderValue::from_static(
                "Wed, 21 Oct 2015 07:28:00 GMT",
            ))),
            None
        );
        assert_eq!(parse_retry_after(None), None);
    }

    #[tokio::test]
    async fn probe_times_out_with_typed_error() {
        let server = spawn_mock_server(
            json_response("200 OK", r#"{"data":[]}"#),
            Duration::from_millis(150),
        );
        let mut provider = test_provider(&server.endpoint, None);
        provider.probe_timeout = Duration::from_millis(30);

        assert!(matches!(
            provider.probe().await,
            Err(ProviderError::ProbeTimeout { timeout_ms: 30 })
        ));
        let _ = server.finish();
    }

    #[tokio::test]
    async fn probe_rejects_declared_oversized_body_before_reading_it() {
        let server = spawn_mock_server(
            format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                MAX_PROBE_BODY_BYTES + 1
            ),
            Duration::ZERO,
        );
        let provider = test_provider(&server.endpoint, None);

        assert!(matches!(
            provider.probe().await,
            Err(ProviderError::ProbeResponseTooLarge {
                limit_bytes: MAX_PROBE_BODY_BYTES
            })
        ));
        let _ = server.finish();
    }

    #[tokio::test]
    async fn anthropic_probe_is_explicitly_unsupported() {
        let provider = AnthropicProvider::new(
            "https://api.anthropic.com/v1/messages",
            "claude-test",
            SecretValue::new("test-key"),
        )
        .expect("anthropic provider");

        assert!(matches!(
            provider.probe().await,
            Err(ProviderError::ProbeUnsupported {
                provider: "anthropic"
            })
        ));
    }

    #[tokio::test]
    async fn anthropic_http_errors_do_not_expose_response_bodies() {
        let secret = "response-secret-that-must-not-render";
        let server = spawn_mock_server(
            json_response("401 Unauthorized", &format!(r#"{{"error":"{secret}"}}"#)),
            Duration::ZERO,
        );
        let provider = AnthropicProvider::new(
            format!("{}/messages", server.endpoint),
            "claude-test",
            SecretValue::new("request-secret"),
        )
        .expect("anthropic provider");
        let (sender, _receiver) = mpsc::channel(2);

        let error = provider
            .stream(empty_model_request(), sender)
            .await
            .expect_err("HTTP failure");
        let rendered = error.to_string();
        assert!(matches!(
            error,
            ProviderError::ApiHttpStatus { status: 401, .. }
        ));
        assert!(!rendered.contains(secret));
        assert!(!rendered.contains("request-secret"));
        let _ = server.finish();
    }

    #[test]
    fn profile_validation_rejects_secret_endpoint_parts() {
        for endpoint in [
            "https://user:password@example.test/v1",
            "https://example.test/v1?api_key=secret",
            "https://example.test/v1#secret",
        ] {
            let profile = ProviderProfile {
                id: "safe-id".into(),
                label: "Safe label".into(),
                kind: ProviderKind::OpenAiCompatible,
                endpoint: endpoint.into(),
                model: "test-model".into(),
                selected: false,
            };
            let error = normalize_provider_profile(profile).expect_err("unsafe endpoint");
            let rendered = error.to_string();
            assert!(matches!(
                error,
                ProviderError::InvalidProfile {
                    field: "endpoint",
                    ..
                }
            ));
            assert!(!rendered.contains("password"));
            assert!(!rendered.contains("secret"));
        }
    }

    #[test]
    fn profile_validation_normalizes_a_safe_endpoint() {
        let profile = normalize_provider_profile(ProviderProfile {
            id: " local-secondary ".into(),
            label: " Secondary local ".into(),
            kind: ProviderKind::LocalLmStudio,
            endpoint: "http://127.0.0.1:5678/v1".into(),
            model: " secondary-model ".into(),
            selected: true,
        })
        .expect("valid profile");

        assert_eq!(profile.id, "local-secondary");
        assert_eq!(profile.label, "Secondary local");
        assert_eq!(profile.endpoint, "http://127.0.0.1:5678/v1/");
        assert_eq!(profile.model, "secondary-model");
        assert!(profile.selected);
    }

    #[test]
    fn anthropic_profile_status_does_not_claim_usability_without_credentials() {
        let profile = ProviderProfile {
            id: "anthropic-work".into(),
            label: "Anthropic work".into(),
            kind: ProviderKind::Anthropic,
            endpoint: "https://api.anthropic.com/v1/messages".into(),
            model: "claude-test".into(),
            selected: false,
        };
        let registry = ProviderRegistry::default();

        let status = registry
            .status_for_profile(&profile)
            .expect("profile status");
        assert!(!status.connected);
        assert!(status.detail.contains("Credentials required"));
        assert!(matches!(
            registry.provider_for_profile(&profile),
            Err(ProviderError::CredentialsRequired { .. })
        ));
    }

    fn openai_compatible_profile(id: &str, endpoint: &str) -> ProviderProfile {
        ProviderProfile {
            id: id.into(),
            label: format!("{id} profile"),
            kind: ProviderKind::OpenAiCompatible,
            endpoint: endpoint.into(),
            model: "test-model".into(),
            selected: false,
        }
    }

    #[test]
    fn profile_credentials_route_only_to_exact_ids_and_approved_origins() {
        let openai_secret = "openai-test-credential";
        let deepseek_secret = "deepseek-test-credential";
        let credentials = ProviderCredentials::from_profile_api_keys([
            ("openai".into(), SecretValue::new(openai_secret)),
            ("deepseek".into(), SecretValue::new(deepseek_secret)),
        ])
        .expect("credentials");
        assert_eq!(
            credentials
                .for_profile("openai")
                .expect("OpenAI credential")
                .expose(),
            openai_secret
        );
        assert_eq!(
            credentials
                .for_profile("deepseek")
                .expect("DeepSeek credential")
                .expose(),
            deepseek_secret
        );
        assert!(credentials.for_profile("custom").is_none());

        let registry = ProviderRegistry::with_credentials(credentials);
        for (id, endpoint) in [
            ("openai", "https://api.openai.com/v1/"),
            ("deepseek", "https://api.deepseek.com/v1/"),
        ] {
            registry
                .provider_for_profile(&openai_compatible_profile(id, endpoint))
                .expect("approved hosted origin");
        }
    }

    #[test]
    fn registry_routes_canonical_profiles_through_their_preset_factory() {
        let credentials = ProviderCredentials::from_profile_api_keys([
            ("openai".into(), SecretValue::new("openai-secret")),
            ("minimax".into(), SecretValue::new("minimax-secret")),
            ("deepseek".into(), SecretValue::new("deepseek-secret")),
        ])
        .expect("credentials");
        let registry = ProviderRegistry::with_credentials(credentials);

        for (preset, endpoint, expected_label) in [
            (
                ProviderPreset::OpenAi,
                "https://api.openai.com/v1/",
                "OpenAI API",
            ),
            (
                ProviderPreset::MiniMax,
                "https://api.minimax.io/v1/",
                "MiniMax",
            ),
            (
                ProviderPreset::DeepSeek,
                "https://api.deepseek.com/v1/",
                "DeepSeek",
            ),
        ] {
            let profile = preset
                .profile(Some(endpoint), None)
                .expect("preset profile");
            let provider = registry
                .provider_for_profile(&profile)
                .expect("preset provider");
            assert_eq!(provider.status().label, expected_label);
        }
    }

    #[test]
    fn hosted_credentials_reject_http_and_unapproved_origins_before_network_use() {
        let credentials = ProviderCredentials::from_profile_api_keys([(
            "openai".into(),
            SecretValue::new("credential-must-not-leave-approved-origin"),
        )])
        .expect("credentials");
        let registry = ProviderRegistry::with_credentials(credentials);

        for endpoint in [
            "http://api.openai.com/v1/",
            "https://api.openai.com.evil.example/v1/",
            "https://example.test/v1/",
            "https://api.openai.com:8443/v1/",
        ] {
            assert!(matches!(
                registry.provider_for_profile(&openai_compatible_profile("openai", endpoint)),
                Err(ProviderError::UnsafeHostedEndpoint { provider_id })
                    if provider_id == "openai"
            ));
        }
    }

    #[tokio::test]
    async fn hosted_label_and_endpoint_do_not_receive_another_profiles_credential() {
        let secret = "openai-isolated-credential";
        let server = spawn_mock_server(json_response("200 OK", r#"{"data":[]}"#), Duration::ZERO);
        let credentials = ProviderCredentials::from_profile_api_keys([(
            "openai".into(),
            SecretValue::new(secret),
        )])
        .expect("credentials");
        let registry = ProviderRegistry::with_credentials(credentials);
        let mut profile = openai_compatible_profile("custom", &server.endpoint);
        profile.label = "OpenAI".into();

        registry
            .probe_profile(&profile)
            .await
            .expect("generic compatible probe");

        let request = server.finish().to_ascii_lowercase();
        assert!(!request.contains("authorization:"));
        assert!(!request.contains(secret));
    }

    #[test]
    fn canonical_hosted_profiles_require_their_own_credential_before_network_use() {
        let credentials = ProviderCredentials::from_profile_api_keys([(
            "openai".into(),
            SecretValue::new("openai-only-credential"),
        )])
        .expect("credentials");
        let registry = ProviderRegistry::with_credentials(credentials);

        for profile_id in ["minimax", "deepseek"] {
            let profile = openai_compatible_profile(profile_id, "https://example.test/v1/");
            assert!(matches!(
                registry.provider_for_profile(&profile),
                Err(ProviderError::CredentialsRequired { provider_id })
                    if provider_id == profile_id
            ));
            let status = registry.status_for_profile(&profile).expect("status");
            assert!(!status.connected);
            assert!(status
                .detail
                .starts_with("Credentials required; profile saved"));
            assert!(status.detail.contains("Coding workflow: supported"));
        }
    }

    #[test]
    fn credential_debug_and_status_output_never_include_values() {
        let secret = "credential-that-must-not-render";
        let credentials = ProviderCredentials::from_profile_api_keys([(
            "minimax".into(),
            SecretValue::new(secret),
        )])
        .expect("credentials");
        let registry = ProviderRegistry::with_credentials(credentials.clone());
        let profile = openai_compatible_profile("minimax", "https://example.test/v1/");
        let status = registry.status_for_profile(&profile).expect("status");
        let rendered = format!(
            "{:?}\n{:?}\n{}",
            credentials,
            registry,
            serde_json::to_string(&status).expect("status JSON")
        );

        assert!(rendered.contains("minimax"));
        assert!(!rendered.contains(secret));
    }

    #[test]
    fn provider_presets_expose_the_required_ids_and_canonical_metadata() {
        let ids = provider_presets()
            .iter()
            .map(|preset| preset.id)
            .collect::<Vec<_>>();
        assert_eq!(
            ids,
            vec![
                "openai-compatible",
                "minimax",
                "openai",
                "deepseek",
                "llama-cpp",
                "chatgpt"
            ]
        );

        let minimax = provider_preset("minimax").expect("MiniMax preset");
        assert_eq!(minimax.label, "MiniMax");
        assert_eq!(minimax.default_endpoint, Some("https://api.minimax.io/v1/"));
        assert_eq!(minimax.credential_profile_id, Some("minimax"));

        let openai = provider_preset("openai").expect("OpenAI preset");
        assert_eq!(openai.label, "OpenAI API");
        assert_eq!(openai.default_endpoint, Some("https://api.openai.com/v1/"));
        assert_eq!(openai.credential_profile_id, Some("openai"));

        let deepseek = provider_preset("deepseek").expect("DeepSeek preset");
        assert_eq!(deepseek.label, "DeepSeek");
        assert_eq!(deepseek.default_endpoint, Some("https://api.deepseek.com/"));
        assert_eq!(deepseek.default_model, "deepseek-flash");
        assert_eq!(deepseek.credential_profile_id, Some("deepseek"));

        let lm_studio = provider_preset("lm-studio").expect("LM Studio preset");
        assert_eq!(lm_studio.label, "LM Studio");
        assert_eq!(
            lm_studio.default_endpoint,
            Some("http://127.0.0.1:1234/v1/")
        );
        assert_eq!(lm_studio.credential_profile_id, None);
    }

    #[test]
    fn provider_presets_build_normalized_profiles_without_changing_identity() {
        assert!(ProviderPreset::OpenAiCompatible
            .profile(None, None)
            .is_err());

        let generic = ProviderPreset::OpenAiCompatible
            .profile(Some("https://example.test/v1"), Some(" custom-model "))
            .expect("generic profile");
        assert_eq!(generic.id, "openai-compatible");
        assert_eq!(generic.label, "OpenAI compatible");
        assert_eq!(generic.endpoint, "https://example.test/v1/");
        assert_eq!(generic.model, "custom-model");
        assert_eq!(generic.kind, ProviderKind::OpenAiCompatible);

        for preset in [
            ProviderPreset::MiniMax,
            ProviderPreset::OpenAi,
            ProviderPreset::DeepSeek,
            ProviderPreset::LlamaCpp,
            ProviderPreset::ChatGpt,
        ] {
            let profile = preset.profile(None, None).expect("canonical profile");
            assert_eq!(profile.id, preset.info().id);
            assert_eq!(profile.label, preset.info().label);
            assert_eq!(profile.endpoint, preset.info().default_endpoint.unwrap());
            assert_eq!(profile.model, preset.info().default_model);
        }
    }

    #[test]
    fn provider_preset_factory_preserves_provider_specific_behavior() {
        let profile = ProviderPreset::MiniMax
            .profile(None, None)
            .expect("MiniMax profile");
        let provider = ProviderPreset::MiniMax
            .build_provider(&profile, Some(SecretValue::new("test-secret")))
            .expect("MiniMax provider");
        let status = provider.status();
        assert_eq!(status.profile_id, "openai-compatible");
        assert_eq!(status.label, "MiniMax");
        assert_eq!(status.endpoint, "https://api.minimax.io/v1/");

        let local_profile = ProviderPreset::LlamaCpp
            .profile(None, None)
            .expect("llama.cpp profile");
        let local_provider = ProviderPreset::LlamaCpp
            .build_provider(&local_profile, None)
            .expect("llama.cpp provider");
        assert_eq!(local_provider.id(), "llama-cpp");
        assert_eq!(local_provider.status().label, "llama.cpp");
    }

    #[test]
    fn active_local_preset_is_llama_cpp_and_excludes_lm_studio() {
        let ids = provider_presets()
            .iter()
            .map(|preset| preset.id)
            .collect::<Vec<_>>();
        assert_eq!(
            ids,
            vec![
                "openai-compatible",
                "minimax",
                "openai",
                "deepseek",
                "llama-cpp",
                "chatgpt"
            ]
        );
        assert!(provider_presets()
            .iter()
            .all(|preset| preset.id != "lm-studio"));

        let local = provider_preset("llama-cpp").expect("llama.cpp preset");
        assert_eq!(local.label, "llama.cpp");
        assert_eq!(local.default_endpoint, Some("local://llama-cpp/"));
        assert_eq!(local.default_model, "Qwen3.5-0.8B-Q8_0.gguf");
        assert_eq!(local.credential_profile_id, None);
    }

    #[test]
    fn llama_cpp_profile_is_local_and_does_not_create_an_http_provider() {
        let profile = ProviderPreset::LlamaCpp
            .profile(None, None)
            .expect("llama.cpp profile");
        assert_eq!(profile.id, "llama-cpp");
        assert_eq!(profile.kind, ProviderKind::LocalLlamaCpp);
        assert_eq!(profile.endpoint, "local://llama-cpp/");

        let provider = LocalLlamaCppProvider::unconfigured(&profile.model);
        let status = provider.status();
        assert_eq!(status.profile_id, "llama-cpp");
        assert_eq!(status.kind, ProviderKind::LocalLlamaCpp);
        assert_eq!(status.endpoint, "local://llama-cpp/");
        assert!(!status.connected);
        assert!(!format!("{:?}", provider).contains("127.0.0.1"));
    }

    #[tokio::test]
    async fn unconfigured_llama_cpp_catalog_is_explicitly_unavailable() {
        let provider = LocalLlamaCppProvider::unconfigured("Qwen3.5-0.8B-Q8_0.gguf");
        assert!(matches!(
            provider.list_models().await,
            Err(ProviderError::ProviderNotConfigured { provider_id })
                if provider_id == "llama-cpp"
        ));
    }
}
