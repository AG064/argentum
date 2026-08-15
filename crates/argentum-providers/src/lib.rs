use std::collections::BTreeMap;
use std::sync::{Arc, RwLock};
use std::time::Duration;

use argentum_domain::{ProviderKind, ProviderProfile, ProviderStatus};
use argentum_security::SecretValue;
use async_trait::async_trait;
use futures_util::StreamExt;
use reqwest::header::{HeaderMap, HeaderValue, CONTENT_TYPE};
use reqwest_eventsource::{Event, EventSource};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tokio::sync::mpsc;
use url::Url;

#[derive(Debug, Error)]
pub enum ProviderError {
    #[error("provider request failed: {0}")]
    Http(#[from] reqwest::Error),
    #[error("provider endpoint is invalid: {0}")]
    Endpoint(#[from] url::ParseError),
    #[error("provider returned an invalid response: {0}")]
    InvalidResponse(String),
    #[error("provider returned an error: {0}")]
    Api(String),
    #[error("provider returned HTTP {status}")]
    ApiHttpStatus { status: u16 },
    #[error("provider stream could not be created")]
    StreamSetup,
    #[error("provider stream failed")]
    Stream,
    #[error("provider stream consumer closed")]
    ConsumerClosed,
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
}

impl From<reqwest_eventsource::Error> for ProviderError {
    fn from(_error: reqwest_eventsource::Error) -> Self {
        Self::Stream
    }
}

#[derive(Debug, Clone)]
pub struct ModelRequest {
    pub model: String,
    pub system: Option<String>,
    pub history: Vec<ModelMessage>,
    pub prompt: String,
    pub tools: Vec<ModelToolDefinition>,
    pub tool_exchanges: Vec<ModelToolExchange>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModelMessageRole {
    User,
    Assistant,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelMessage {
    pub role: ModelMessageRole,
    pub content: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelToolDefinition {
    pub name: String,
    pub description: String,
    pub parameters: serde_json::Value,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelToolCall {
    pub id: String,
    pub name: String,
    pub arguments: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelToolResult {
    pub call_id: String,
    pub content: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelToolExchange {
    pub assistant_content: String,
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
pub enum ProviderEvent {
    Delta(String),
    ReasoningDelta(String),
    ToolCall(ModelToolCall),
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

    async fn stream(
        &self,
        request: ModelRequest,
        sender: mpsc::Sender<ProviderEvent>,
    ) -> Result<(), ProviderError>;
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
}

const DEFAULT_PROBE_TIMEOUT: Duration = Duration::from_secs(3);
const MAX_PROBE_BODY_BYTES: usize = 256 * 1024;
const MAX_PROBE_MODELS: usize = 4_096;
const MAX_PROBE_MODEL_ID_BYTES: usize = 1_024;
const MAX_STREAM_EVENT_BYTES: usize = 256 * 1024;
const MAX_TOOLS_PER_REQUEST: usize = 16;
const MAX_TOOL_EXCHANGES: usize = 8;
const MAX_TOOL_CALLS_PER_RESPONSE: usize = 8;
const MAX_TOOL_CALL_ID_BYTES: usize = 256;
const MAX_TOOL_NAME_BYTES: usize = 64;
const MAX_TOOL_ARGUMENT_BYTES: usize = 128 * 1024;
const MAX_TOOL_RESULT_BYTES: usize = 64 * 1024;
const MAX_TOOL_SCHEMA_BYTES: usize = 32 * 1024;
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
        Ok(Self {
            client: reqwest::Client::builder().build()?,
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
        })
    }

    fn with_hidden_tagged_reasoning(mut self, hidden: bool) -> Self {
        self.hide_tagged_reasoning = hidden;
        self
    }

    fn supports_minimax_reasoning_protocol(&self) -> bool {
        self.endpoint.scheme() == "https" && self.endpoint.host_str() == Some("api.minimax.io")
    }

    fn initial_reasoning_details_mode(&self) -> ReasoningDetailsMode {
        if self.supports_minimax_reasoning_protocol() {
            ReasoningDetailsMode::Incremental
        } else {
            ReasoningDetailsMode::Unknown
        }
    }

    fn completion_url(&self) -> Result<Url, ProviderError> {
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
            request.model
        };
        let context_window_tokens = profile_id_for_endpoint(&self.endpoint)
            .and_then(|profile_id| canonical_context_window(profile_id, &selected_model));
        let body = ChatCompletionRequest {
            model: selected_model,
            messages: build_messages(
                request.system,
                request.history,
                request.prompt,
                request.tool_exchanges,
                self.supports_minimax_reasoning_protocol(),
            ),
            tools: request
                .tools
                .into_iter()
                .map(ChatToolDefinition::from)
                .collect(),
            stream_options: ChatStreamOptions {
                include_usage: true,
            },
            reasoning_split: self.supports_minimax_reasoning_protocol().then_some(true),
            stream: true,
        };
        let mut headers = HeaderMap::new();
        headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
        let mut request_builder = self
            .client
            .post(self.completion_url()?)
            .headers(headers)
            .json(&body);
        if let Some(api_key) = &self.api_key {
            request_builder = request_builder.bearer_auth(api_key.expose());
        }
        let mut source =
            EventSource::new(request_builder).map_err(|_| ProviderError::StreamSetup)?;
        let mut saw_terminal_chunk = false;
        let mut reasoning_filter = TaggedReasoningFilter::new(self.hide_tagged_reasoning);
        let mut explicit_reasoning =
            ExplicitReasoningAccumulator::new(self.initial_reasoning_details_mode());
        let mut explicit_reasoning_tags = ReasoningTagFilter::default();
        let mut usage = None;
        let mut tool_calls = ToolCallAccumulator::default();
        while let Some(event) = source.next().await {
            match event {
                Ok(Event::Open) => {}
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
                    if chunk.choices.len() > 1 {
                        return Err(ProviderError::InvalidResponse(
                            "provider returned multiple completion choices".into(),
                        ));
                    }
                    if let Some(chunk_usage) = chunk.usage {
                        if usage.is_some() {
                            return Err(ProviderError::InvalidResponse(
                                "provider emitted duplicate usage records".into(),
                            ));
                        }
                        let mut model_usage = ModelUsage::try_from(chunk_usage)?;
                        model_usage.context_window_tokens = context_window_tokens;
                        usage = Some(model_usage);
                    }
                    for choice in chunk.choices {
                        if let Some(finish_reason) = choice.finish_reason.as_deref() {
                            match finish_reason {
                                "stop" | "tool_calls" => saw_terminal_chunk = true,
                                "length" => {
                                    return Err(ProviderError::InvalidResponse(
                                        "provider response was truncated".into(),
                                    ));
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
                            choice.delta.reasoning,
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
                Err(error) => return Err(error.into()),
            }
        }
        if !saw_terminal_chunk {
            return Err(ProviderError::InvalidResponse(
                "provider stream ended without a terminal finish reason".into(),
            ));
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
        for call in tool_calls.finish()? {
            sender
                .send(ProviderEvent::ToolCall(call))
                .await
                .map_err(|_| ProviderError::ConsumerClosed)?;
        }
        if let Some(usage) = usage {
            sender
                .send(ProviderEvent::Usage(usage))
                .await
                .map_err(|_| ProviderError::ConsumerClosed)?;
        }
        sender
            .send(ProviderEvent::Completed)
            .await
            .map_err(|_| ProviderError::ConsumerClosed)
    }
}

#[async_trait]
impl ModelProvider for OpenAiCompatibleProvider {
    fn id(&self) -> &'static str {
        match self.kind {
            ProviderKind::OpenAiCompatible => "openai-compatible",
            ProviderKind::Anthropic => "anthropic-compatible",
            ProviderKind::LocalLmStudio => "lm-studio",
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
        let body = AnthropicRequest {
            model: if request.model.is_empty() {
                self.model.clone()
            } else {
                request.model
            },
            max_tokens: 4096,
            system: request.system,
            messages: build_anthropic_messages(request.history, request.prompt),
        };
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
}

impl std::fmt::Debug for ProviderRegistry {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ProviderRegistry")
            .field("providers", &self.providers.keys().collect::<Vec<_>>())
            .field("credentials", &self.credentials)
            .finish()
    }
}

impl ProviderRegistry {
    pub fn with_credentials(credentials: ProviderCredentials) -> Self {
        Self {
            providers: Arc::default(),
            credentials,
        }
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
        match profile.kind {
            ProviderKind::LocalLmStudio => Ok(Arc::new(LocalLmStudioProvider::new(
                &profile.endpoint,
                &profile.model,
            )?)),
            ProviderKind::OpenAiCompatible => match self.get(&profile.id) {
                Some(provider) => Ok(provider),
                None => {
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
        if profile.kind != ProviderKind::LocalLmStudio {
            if let Some(provider) = self.get(&profile.id) {
                let mut status = sanitize_provider_status(provider.status());
                status.profile_id = profile.id;
                status.kind = profile.kind;
                status.label = profile.label;
                status.endpoint = profile.endpoint;
                return Ok(status);
            }
        }
        let detail = match profile.kind {
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
        Ok(ProviderStatus {
            profile_id: profile.id,
            kind: profile.kind,
            label: profile.label,
            endpoint: profile.endpoint,
            connected: false,
            detail,
        })
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
        status.label = profile.label;
        status.endpoint = profile.endpoint;
        Ok(status)
    }

    pub async fn list_models_for_profile(
        &self,
        profile: &ProviderProfile,
    ) -> Result<Vec<ProviderModel>, ProviderError> {
        let profile = normalize_provider_profile(profile.clone())?;
        let provider = self.provider_for_profile(&profile)?;
        let mut models = sanitize_provider_models(provider.list_models().await?)?;
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
            reason: "select LM Studio, OpenAI compatible, or Anthropic",
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
    if !matches!(endpoint.scheme(), "http" | "https") || endpoint.host_str().is_none() {
        return Err(ProviderError::InvalidProfile {
            field: "endpoint",
            reason: "use an HTTP or HTTPS URL with a host",
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
    Ok(profile)
}

#[derive(Debug, Serialize)]
struct ChatCompletionRequest {
    model: String,
    messages: Vec<ChatMessage>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    tools: Vec<ChatToolDefinition>,
    stream_options: ChatStreamOptions,
    #[serde(skip_serializing_if = "Option::is_none")]
    reasoning_split: Option<bool>,
    stream: bool,
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
    reasoning_details: Option<Vec<ChatReasoningReplay>>,
}

impl ChatMessage {
    fn text(role: &'static str, content: String) -> Self {
        Self {
            role,
            content: Some(content),
            tool_calls: None,
            tool_call_id: None,
            reasoning_details: None,
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

#[derive(Debug, Serialize)]
struct ChatReasoningReplay {
    #[serde(rename = "type")]
    kind: &'static str,
    text: String,
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
    messages.extend(history.into_iter().map(|message| {
        ChatMessage::text(
            match message.role {
                ModelMessageRole::User => "user",
                ModelMessageRole::Assistant => "assistant",
            },
            message.content,
        )
    }));
    messages.push(ChatMessage::text("user", prompt));
    for exchange in tool_exchanges {
        messages.push(ChatMessage {
            role: "assistant",
            content: (!exchange.assistant_content.is_empty()).then_some(exchange.assistant_content),
            tool_calls: Some(
                exchange
                    .calls
                    .into_iter()
                    .map(|call| ChatToolCall {
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
            reasoning_details: (replay_reasoning && !exchange.assistant_reasoning.is_empty()).then(
                || {
                    vec![ChatReasoningReplay {
                        kind: "reasoning.text",
                        text: exchange.assistant_reasoning,
                    }]
                },
            ),
        });
        messages.extend(exchange.results.into_iter().map(|result| ChatMessage {
            role: "tool",
            content: Some(result.content),
            tool_calls: None,
            tool_call_id: Some(result.call_id),
            reasoning_details: None,
        }));
    }
    messages
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
    choices: Vec<ChatStreamChoice>,
    #[serde(default)]
    usage: Option<ChatUsage>,
}

#[derive(Debug, Deserialize)]
struct ChatStreamChoice {
    delta: ChatDelta,
    #[serde(default)]
    finish_reason: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ChatDelta {
    content: Option<String>,
    #[serde(default, alias = "reasoning_content")]
    reasoning: Option<String>,
    #[serde(default)]
    reasoning_details: Vec<ChatReasoningDetail>,
    #[serde(default)]
    tool_calls: Vec<ChatToolCallDelta>,
}

#[derive(Debug, Deserialize)]
struct ChatReasoningDetail {
    #[serde(default)]
    text: Option<String>,
}

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
    if request.tools.len() > MAX_TOOLS_PER_REQUEST {
        return Err(invalid_tool_response("too many tool definitions"));
    }
    let mut tool_names = std::collections::BTreeSet::new();
    for tool in &request.tools {
        if !valid_tool_name(&tool.name) || !tool_names.insert(tool.name.as_str()) {
            return Err(invalid_tool_response("invalid tool definition identity"));
        }
        if !tool.parameters.is_object()
            || serde_json::to_vec(&tool.parameters)
                .map_err(|_| invalid_tool_response("invalid tool definition schema"))?
                .len()
                > MAX_TOOL_SCHEMA_BYTES
        {
            return Err(invalid_tool_response("invalid tool definition schema"));
        }
    }
    if request.tool_exchanges.len() > MAX_TOOL_EXCHANGES {
        return Err(invalid_tool_response("too many tool exchanges"));
    }
    for exchange in &request.tool_exchanges {
        if exchange.calls.is_empty()
            || exchange.calls.len() > MAX_TOOL_CALLS_PER_RESPONSE
            || exchange.results.len() != exchange.calls.len()
            || exchange.assistant_reasoning.len() > MAX_REASONING_RESPONSE_BYTES
        {
            return Err(invalid_tool_response("invalid tool exchange shape"));
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
                    return Err(invalid_tool_response("invalid tool call"));
                }
                Ok(call.id.as_str())
            })
            .collect::<Result<std::collections::BTreeSet<_>, _>>()?;
        if call_ids.len() != exchange.calls.len()
            || exchange.results.iter().any(|result| {
                !call_ids.contains(result.call_id.as_str())
                    || result.content.len() > MAX_TOOL_RESULT_BYTES
            })
        {
            return Err(invalid_tool_response("invalid tool result"));
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
        ("deepseek", "deepseek-v4-flash" | "deepseek-v4-pro") => Some(1_000_000),
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
mod tests {
    use std::io::{Read, Write};
    use std::net::{TcpListener, TcpStream};
    use std::sync::mpsc as std_mpsc;
    use std::thread;

    use super::*;

    struct MockServer {
        endpoint: String,
        request: std_mpsc::Receiver<String>,
        handle: thread::JoinHandle<()>,
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

    fn spawn_mock_server_sequence(
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

    fn event_stream_response(body: &str) -> String {
        format!(
            "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        )
    }

    fn test_provider(endpoint: &str, api_key: Option<SecretValue>) -> OpenAiCompatibleProvider {
        OpenAiCompatibleProvider::new(
            endpoint,
            "local/model",
            api_key,
            ProviderKind::OpenAiCompatible,
            "Test provider",
        )
        .expect("provider")
    }

    fn empty_model_request() -> ModelRequest {
        ModelRequest {
            model: "local/model".into(),
            system: None,
            history: Vec::new(),
            prompt: "test".into(),
            tools: Vec::new(),
            tool_exchanges: Vec::new(),
        }
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
                ProviderEvent::ToolCall(_) | ProviderEvent::Usage(_) => {}
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
        let deepseek = test_provider("https://api.deepseek.com/v1/", None);
        assert!(minimax.supports_minimax_reasoning_protocol());
        assert!(!deepseek.supports_minimax_reasoning_protocol());

        let exchange = ModelToolExchange {
            assistant_content: String::new(),
            assistant_reasoning: "complete reasoning".into(),
            calls: vec![ModelToolCall {
                id: "call_1".into(),
                name: "read_text".into(),
                arguments: r#"{"path":"notes.txt"}"#.into(),
            }],
            results: vec![ModelToolResult {
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
        assert!(minimax_messages[1].reasoning_details.is_some());
        assert!(deepseek_messages[1].reasoning_details.is_none());
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
    fn openai_messages_preserve_system_history_and_current_prompt_order() {
        let messages = build_messages(
            Some("system".into()),
            vec![
                ModelMessage {
                    role: ModelMessageRole::User,
                    content: "earlier user".into(),
                },
                ModelMessage {
                    role: ModelMessageRole::Assistant,
                    content: "earlier assistant".into(),
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
    fn serializes_tool_exchanges_in_openai_message_order() {
        let messages = build_messages(
            Some("system".into()),
            Vec::new(),
            "inspect".into(),
            vec![ModelToolExchange {
                assistant_content: String::new(),
                assistant_reasoning: "same-round reasoning".into(),
                calls: vec![ModelToolCall {
                    id: "call_1".into(),
                    name: "read_text".into(),
                    arguments: r#"{"path":"notes.txt"}"#.into(),
                }],
                results: vec![ModelToolResult {
                    call_id: "call_1".into(),
                    content: "bounded result".into(),
                }],
            }],
            true,
        );

        assert_eq!(messages.len(), 4);
        assert_eq!(messages[2].role, "assistant");
        assert!(messages[2].content.is_none());
        assert_eq!(
            messages[2]
                .reasoning_details
                .as_ref()
                .expect("reasoning replay")[0]
                .text,
            "same-round reasoning"
        );
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
        for model in ["deepseek-v4-flash", "deepseek-v4-pro"] {
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
            ProviderError::ApiHttpStatus { status: 401 }
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
            assert_eq!(status.detail, "Credentials required; profile saved");
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
}
