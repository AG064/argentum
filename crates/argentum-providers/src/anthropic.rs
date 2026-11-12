//! Native adapter for the Anthropic Messages API.
//!
//! The adapter maps the shared request and tool contract onto Messages API
//! content blocks and maps the streamed reply back onto provider events. It
//! applies the same structural checks as the OpenAI-compatible route: a
//! validated tool payload, bounded stream events, bounded visible and reasoning
//! text, bounded tool calls with JSON object arguments, no partial tool calls
//! after truncation, and no retries inside the adapter. Execution authority is
//! never decided here; tool calls are only reported to the runtime, which keeps
//! policy and approval checks.

use super::*;
use argentum_domain::goal_accounting::ObservedUsage;

/// The Messages API version this adapter speaks.
pub(crate) const ANTHROPIC_VERSION: &str = "2023-06-01";
const ANTHROPIC_VERSION_HEADER: &str = "anthropic-version";
const ANTHROPIC_KEY_HEADER: &str = "x-api-key";
/// More content blocks than this in one reply is treated as a broken stream.
const MAX_CONTENT_BLOCKS: usize = 64;
const MAX_SERVED_MODEL_BYTES: usize = 128;
const MAX_STREAM_ERROR_TYPE_BYTES: usize = 64;

#[derive(Debug, Serialize)]
pub(crate) struct MessagesRequest {
    model: String,
    max_tokens: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    system: Option<String>,
    messages: Vec<WireMessage>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    tools: Vec<WireTool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_choice: Option<WireToolChoice>,
    stream: bool,
}

#[derive(Debug, Serialize)]
struct WireMessage {
    role: &'static str,
    content: Vec<WireBlock>,
}

#[derive(Debug, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum WireBlock {
    Text {
        text: String,
    },
    ToolUse {
        id: String,
        name: String,
        input: serde_json::Value,
    },
    ToolResult {
        tool_use_id: String,
        content: String,
    },
}

#[derive(Debug, Serialize)]
struct WireTool {
    name: String,
    description: String,
    input_schema: serde_json::Value,
}

#[derive(Debug, Serialize)]
struct WireToolChoice {
    #[serde(rename = "type")]
    kind: &'static str,
}

/// Appends blocks to the conversation, merging consecutive turns from the same
/// role. The Messages API expects alternating roles and rejects empty text.
fn push_blocks(messages: &mut Vec<WireMessage>, role: &'static str, blocks: Vec<WireBlock>) {
    if blocks.is_empty() {
        return;
    }
    match messages.last_mut() {
        Some(last) if last.role == role => last.content.extend(blocks),
        _ => messages.push(WireMessage {
            role,
            content: blocks,
        }),
    }
}

fn text_blocks(text: String) -> Vec<WireBlock> {
    if text.is_empty() {
        Vec::new()
    } else {
        vec![WireBlock::Text { text }]
    }
}

fn push_exchange(
    messages: &mut Vec<WireMessage>,
    exchange: ModelToolExchange,
) -> Result<(), ProviderError> {
    let mut assistant = text_blocks(exchange.assistant_content);
    for call in exchange.calls {
        let input = serde_json::from_str::<serde_json::Value>(&call.arguments)
            .ok()
            .filter(serde_json::Value::is_object)
            .ok_or(ProviderError::InvalidRequest("invalid tool call"))?;
        assistant.push(WireBlock::ToolUse {
            id: call.id,
            name: call.name,
            input,
        });
    }
    push_blocks(messages, "assistant", assistant);
    // Tool results must lead the following user turn.
    let results = exchange
        .results
        .into_iter()
        .map(|result| WireBlock::ToolResult {
            tool_use_id: result.call_id,
            content: result.content,
        })
        .collect();
    push_blocks(messages, "user", results);
    Ok(())
}

pub(crate) fn build_messages_request(
    request: ModelRequest,
) -> Result<MessagesRequest, ProviderError> {
    validate_model_tool_payload(&request)?;
    for exchange in request.tool_exchanges.iter().chain(
        request
            .history
            .iter()
            .filter_map(|message| message.tool_exchange.as_ref()),
    ) {
        // Private continuation state belongs to the route that produced it.
        if exchange.chatgpt_continuation.is_some() || exchange.assistant_continuation.is_some() {
            return Err(ProviderError::PreparedRequestBindingMismatch);
        }
        if exchange
            .results
            .iter()
            .any(|result| !result.images.is_empty())
        {
            return Err(ProviderError::InvalidRequest(
                "this route has no qualified image input representation",
            ));
        }
    }
    if request.output_limit == 0 || request.model.is_empty() {
        return Err(ProviderError::InvalidRequest(
            "model and positive output limit are required",
        ));
    }
    if request.require_tool_call && request.tools.is_empty() {
        return Err(ProviderError::InvalidRequest(
            "required tool response needs a compatible hosted tool catalog",
        ));
    }
    let mut messages = Vec::new();
    for message in request.history {
        if let Some(exchange) = message.tool_exchange {
            push_exchange(&mut messages, exchange)?;
            continue;
        }
        let role = match message.role {
            ModelMessageRole::User => "user",
            ModelMessageRole::Assistant => "assistant",
        };
        push_blocks(&mut messages, role, text_blocks(message.content));
    }
    push_blocks(&mut messages, "user", text_blocks(request.prompt));
    for exchange in request.tool_exchanges {
        push_exchange(&mut messages, exchange)?;
    }
    if messages.is_empty() {
        return Err(ProviderError::InvalidRequest(
            "the request contains no message content",
        ));
    }
    Ok(MessagesRequest {
        model: request.model,
        max_tokens: request.output_limit,
        system: request.system.filter(|system| !system.is_empty()),
        messages,
        tools: request
            .tools
            .into_iter()
            .map(|tool| WireTool {
                name: tool.name,
                description: tool.description,
                input_schema: tool.parameters,
            })
            .collect(),
        tool_choice: request
            .require_tool_call
            .then_some(WireToolChoice { kind: "any" }),
        stream: true,
    })
}

/// Encodes the exact body that dispatch sends and accounts for it.
pub(crate) fn prepare_messages_wire(
    profile: &ProviderProfile,
    budget: &argentum_domain::context_budget::ModelBudget,
    request: &ModelRequest,
) -> Result<(Bytes, argentum_domain::context_budget::RequestAccounting), ProviderError> {
    if profile.kind != ProviderKind::Anthropic {
        return Err(ProviderError::ContextAccountingUnsupported(
            "this route has no model-bound request accounting/output control contract",
        ));
    }
    budget
        .validate(profile)
        .map_err(ProviderError::ContextBudgetInvalid)?;
    if request.model != profile.model || request.output_limit != budget.requested_output_tokens {
        return Err(ProviderError::ContextBudgetInvalid(
            "model or output control binding mismatch",
        ));
    }
    let body = build_messages_request(request.clone())?;
    let messages = encoded_len(&body.messages)?;
    let system = match &body.system {
        Some(system) => encoded_len(system)?,
        None => 0,
    };
    let catalog = if body.tools.is_empty() {
        0
    } else {
        encoded_len(&body.tools)?
    };
    let encoded = serde_json::to_vec(&body)
        .map_err(|_| ProviderError::InvalidRequest("wire encoding failed"))?;
    let total = encoded.len() as u64;
    let envelope = total
        .checked_sub(messages)
        .and_then(|n| n.checked_sub(system))
        .and_then(|n| n.checked_sub(catalog))
        .ok_or(ProviderError::InvalidRequest("wire accounting overflow"))?;
    let mut accounting = argentum_domain::context_budget::RequestAccounting::from_wire_bytes(
        budget,
        messages + system,
        catalog,
        envelope,
    )
    .map_err(ProviderError::InvalidRequest)?;
    accounting
        .message_components
        .insert("host_guidance_and_fresh_instructions".into(), system);
    accounting
        .message_components
        .insert("historical_projection_and_current_task".into(), messages);
    if accounting.accounted_input != total {
        return Err(ProviderError::InvalidRequest(
            "prepared body accounting mismatch",
        ));
    }
    Ok((Bytes::from(encoded), accounting))
}

fn encoded_len<T: Serialize>(value: &T) -> Result<u64, ProviderError> {
    serde_json::to_vec(value)
        .map(|bytes| bytes.len() as u64)
        .map_err(|_| ProviderError::InvalidRequest("wire accounting failed"))
}

#[derive(Debug, Deserialize)]
struct StreamEvent {
    #[serde(rename = "type")]
    kind: String,
    #[serde(default)]
    index: Option<usize>,
    #[serde(default)]
    message: Option<StreamMessage>,
    #[serde(default)]
    content_block: Option<serde_json::Value>,
    #[serde(default)]
    delta: Option<serde_json::Value>,
    #[serde(default)]
    usage: Option<StreamUsageReport>,
    #[serde(default)]
    error: Option<serde_json::Value>,
}

#[derive(Debug, Deserialize)]
struct StreamMessage {
    #[serde(default)]
    model: Option<String>,
    #[serde(default)]
    usage: Option<StreamUsageReport>,
}

#[derive(Debug, Default, Deserialize)]
struct StreamUsageReport {
    #[serde(default)]
    input_tokens: Option<u64>,
    #[serde(default)]
    output_tokens: Option<u64>,
    #[serde(default)]
    cache_creation_input_tokens: Option<u64>,
    #[serde(default)]
    cache_read_input_tokens: Option<u64>,
}

#[derive(Debug)]
enum OpenBlock {
    Text,
    Thinking,
    /// Content the adapter does not surface, such as redacted reasoning.
    Hidden,
    ToolUse {
        id: String,
        name: String,
        arguments: String,
        initial: String,
    },
}

#[derive(Debug)]
struct ContentBlock {
    block: OpenBlock,
    closed: bool,
}

/// One piece of streamed output for the consumer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum StreamOutput {
    ServedModel(String),
    Delta(String),
    Reasoning(String),
}

/// The result of a complete stream.
#[derive(Debug)]
pub(crate) struct StreamFinish {
    pub(crate) calls: Vec<ModelToolCall>,
    pub(crate) observed: Option<ObservedUsage>,
    pub(crate) usage: Option<ModelUsage>,
}

/// Why a complete stream could not be used.
#[derive(Debug)]
pub(crate) enum StreamFailure {
    /// The reply hit its output limit or the context window. Usage stays
    /// reportable, but no tool call is released.
    Truncated(Box<TruncatedUsage>),
    Error(ProviderError),
}

/// Usage reported by a truncated reply.
#[derive(Debug)]
pub(crate) struct TruncatedUsage {
    pub(crate) observed: Option<ObservedUsage>,
    pub(crate) usage: Option<ModelUsage>,
}

impl From<ProviderError> for StreamFailure {
    fn from(error: ProviderError) -> Self {
        Self::Error(error)
    }
}

/// Decodes Messages API stream events. It is independent of HTTP so recorded
/// and synthetic fixtures can drive it directly.
#[derive(Debug, Default)]
pub(crate) struct MessagesStream {
    started: bool,
    stopped: bool,
    blocks: BTreeMap<usize, ContentBlock>,
    tool_blocks: usize,
    visible_bytes: usize,
    reasoning_bytes: usize,
    stop_reason: Option<String>,
    input_tokens: Option<u64>,
    cache_creation_tokens: Option<u64>,
    cache_read_tokens: Option<u64>,
    output_tokens: Option<u64>,
}

fn invalid(reason: &'static str) -> ProviderError {
    ProviderError::InvalidResponse(reason.into())
}

fn bounded_display(value: &str, limit: usize) -> Option<String> {
    (!value.is_empty() && value.len() <= limit && !value.chars().any(char::is_control))
        .then(|| value.to_owned())
}

impl MessagesStream {
    pub(crate) fn stopped(&self) -> bool {
        self.stopped
    }

    pub(crate) fn push(&mut self, data: &str) -> Result<Vec<StreamOutput>, ProviderError> {
        if data.len() > MAX_STREAM_EVENT_BYTES {
            return Err(invalid("provider stream event exceeded the safety limit"));
        }
        let event: StreamEvent = serde_json::from_str(data)
            .map_err(|_| invalid("provider stream event was malformed"))?;
        if self.stopped {
            return Err(invalid("provider returned data after the message ended"));
        }
        let mut output = Vec::new();
        match event.kind.as_str() {
            "ping" => {}
            "error" => return Err(stream_error(event.error.as_ref())),
            "message_start" => {
                if self.started {
                    return Err(invalid("provider started the message twice"));
                }
                self.started = true;
                let message = event
                    .message
                    .ok_or_else(|| invalid("message start had no message"))?;
                if let Some(model) = message
                    .model
                    .as_deref()
                    .and_then(|model| bounded_display(model, MAX_SERVED_MODEL_BYTES))
                {
                    output.push(StreamOutput::ServedModel(model));
                }
                if let Some(usage) = message.usage {
                    self.accept_usage(usage)?;
                }
            }
            _ if !self.started => {
                return Err(invalid("provider stream did not start with a message"));
            }
            "content_block_start" => {
                let index = event
                    .index
                    .ok_or_else(|| invalid("content block had no index"))?;
                let block = event
                    .content_block
                    .ok_or_else(|| invalid("content block start had no block"))?;
                self.start_block(index, &block, &mut output)?;
            }
            "content_block_delta" => {
                let index = event
                    .index
                    .ok_or_else(|| invalid("content block had no index"))?;
                let delta = event
                    .delta
                    .ok_or_else(|| invalid("content block delta had no delta"))?;
                self.apply_delta(index, &delta, &mut output)?;
            }
            "content_block_stop" => {
                let index = event
                    .index
                    .ok_or_else(|| invalid("content block had no index"))?;
                let block = self
                    .blocks
                    .get_mut(&index)
                    .filter(|block| !block.closed)
                    .ok_or_else(|| invalid("provider closed an unknown content block"))?;
                block.closed = true;
            }
            "message_delta" => {
                if let Some(stop_reason) = event
                    .delta
                    .as_ref()
                    .and_then(|delta| delta.get("stop_reason"))
                    .and_then(serde_json::Value::as_str)
                {
                    if self.stop_reason.is_some() {
                        return Err(invalid("provider reported two stop reasons"));
                    }
                    self.stop_reason = Some(
                        bounded_display(stop_reason, MAX_STREAM_ERROR_TYPE_BYTES)
                            .ok_or_else(|| invalid("provider stop reason was invalid"))?,
                    );
                }
                if let Some(usage) = event.usage {
                    self.accept_usage(usage)?;
                }
            }
            "message_stop" => self.stopped = true,
            // The API may add event types. Unknown ones carry no content the
            // adapter would act on.
            _ => {}
        }
        Ok(output)
    }

    fn accept_usage(&mut self, usage: StreamUsageReport) -> Result<(), ProviderError> {
        // Reported counts are cumulative; a count that goes down is not trusted.
        for (slot, value) in [
            (&mut self.input_tokens, usage.input_tokens),
            (
                &mut self.cache_creation_tokens,
                usage.cache_creation_input_tokens,
            ),
            (&mut self.cache_read_tokens, usage.cache_read_input_tokens),
            (&mut self.output_tokens, usage.output_tokens),
        ] {
            if let Some(value) = value {
                if slot.is_some_and(|previous| value < previous) {
                    return Err(invalid("conflicting provider usage"));
                }
                *slot = Some(value);
            }
        }
        Ok(())
    }

    fn start_block(
        &mut self,
        index: usize,
        block: &serde_json::Value,
        output: &mut Vec<StreamOutput>,
    ) -> Result<(), ProviderError> {
        if self.blocks.contains_key(&index) {
            return Err(invalid("provider reused a content block index"));
        }
        if self.blocks.len() >= MAX_CONTENT_BLOCKS {
            return Err(invalid("provider returned too many content blocks"));
        }
        let kind = block
            .get("type")
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default();
        let block = match kind {
            "text" => {
                let text = block
                    .get("text")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or_default();
                self.push_visible(text, output)?;
                OpenBlock::Text
            }
            "thinking" => {
                let thinking = block
                    .get("thinking")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or_default();
                self.push_reasoning(thinking, output)?;
                OpenBlock::Thinking
            }
            "redacted_thinking" => OpenBlock::Hidden,
            "tool_use" => {
                if self.tool_blocks >= MAX_TOOL_CALLS_PER_RESPONSE {
                    return Err(invalid_tool_response("too many tool calls"));
                }
                self.tool_blocks += 1;
                let mut id = String::new();
                let mut name = String::new();
                append_bounded(
                    &mut id,
                    block
                        .get("id")
                        .and_then(serde_json::Value::as_str)
                        .unwrap_or_default(),
                    MAX_TOOL_CALL_ID_BYTES,
                    "tool-call identifier exceeded the safety limit",
                )?;
                append_bounded(
                    &mut name,
                    block
                        .get("name")
                        .and_then(serde_json::Value::as_str)
                        .unwrap_or_default(),
                    MAX_TOOL_NAME_BYTES,
                    "tool name exceeded the safety limit",
                )?;
                // A complete input can arrive in the start block instead of
                // as deltas. An empty object is the usual placeholder.
                let initial = match block.get("input") {
                    None => String::new(),
                    Some(serde_json::Value::Object(map)) if map.is_empty() => String::new(),
                    Some(input) => {
                        let mut initial = String::new();
                        append_bounded(
                            &mut initial,
                            &input.to_string(),
                            MAX_TOOL_ARGUMENT_BYTES,
                            "tool-call arguments exceeded the safety limit",
                        )?;
                        initial
                    }
                };
                OpenBlock::ToolUse {
                    id,
                    name,
                    arguments: String::new(),
                    initial,
                }
            }
            _ => {
                return Err(invalid(
                    "provider returned an unsupported content block type",
                ))
            }
        };
        self.blocks.insert(
            index,
            ContentBlock {
                block,
                closed: false,
            },
        );
        Ok(())
    }

    fn apply_delta(
        &mut self,
        index: usize,
        delta: &serde_json::Value,
        output: &mut Vec<StreamOutput>,
    ) -> Result<(), ProviderError> {
        let kind = delta
            .get("type")
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default();
        let field = |name: &str| {
            delta
                .get(name)
                .and_then(serde_json::Value::as_str)
                .ok_or_else(|| invalid("content block delta was malformed"))
        };
        let block = self
            .blocks
            .get_mut(&index)
            .filter(|block| !block.closed)
            .ok_or_else(|| invalid("provider updated an unknown content block"))?;
        match (&mut block.block, kind) {
            (OpenBlock::Text, "text_delta") => {
                let text = field("text")?.to_owned();
                self.push_visible(&text, output)
            }
            // Citations add metadata to text already streamed.
            (OpenBlock::Text, "citations_delta") => Ok(()),
            (OpenBlock::Thinking, "thinking_delta") => {
                let thinking = field("thinking")?.to_owned();
                self.push_reasoning(&thinking, output)
            }
            (OpenBlock::Thinking, "signature_delta") => Ok(()),
            (OpenBlock::ToolUse { arguments, .. }, "input_json_delta") => append_bounded(
                arguments,
                field("partial_json")?,
                MAX_TOOL_ARGUMENT_BYTES,
                "tool-call arguments exceeded the safety limit",
            ),
            (OpenBlock::Hidden, _) => Ok(()),
            _ => Err(invalid("content block delta did not match its block")),
        }
    }

    fn push_visible(
        &mut self,
        text: &str,
        output: &mut Vec<StreamOutput>,
    ) -> Result<(), ProviderError> {
        if text.is_empty() {
            return Ok(());
        }
        self.visible_bytes = self.visible_bytes.saturating_add(text.len());
        if self.visible_bytes > MAX_VISIBLE_RESPONSE_BYTES {
            return Err(invalid("provider response exceeded the safety limit"));
        }
        output.push(StreamOutput::Delta(text.to_owned()));
        Ok(())
    }

    fn push_reasoning(
        &mut self,
        text: &str,
        output: &mut Vec<StreamOutput>,
    ) -> Result<(), ProviderError> {
        if text.is_empty() {
            return Ok(());
        }
        self.reasoning_bytes = self.reasoning_bytes.saturating_add(text.len());
        if self.reasoning_bytes > MAX_REASONING_RESPONSE_BYTES {
            return Err(invalid("provider reasoning exceeded the safety limit"));
        }
        output.push(StreamOutput::Reasoning(text.to_owned()));
        Ok(())
    }

    fn usage(&self) -> Result<(Option<ObservedUsage>, Option<ModelUsage>), ProviderError> {
        let (Some(base_input), Some(output)) = (self.input_tokens, self.output_tokens) else {
            return Ok((None, None));
        };
        // input_tokens excludes cache reads and writes; all three are input.
        let cached = self.cache_read_tokens.unwrap_or(0);
        let input = base_input
            .checked_add(self.cache_creation_tokens.unwrap_or(0))
            .and_then(|n| n.checked_add(cached))
            .ok_or_else(|| invalid("provider usage overflowed"))?;
        let total = input
            .checked_add(output)
            .ok_or_else(|| invalid("provider usage overflowed"))?;
        let observed = ObservedUsage {
            input_tokens: Some(input),
            output_tokens: Some(output),
            total_tokens: total,
            cached_input_tokens: self.cache_read_tokens,
            reasoning_tokens: None,
        };
        observed.validate().map_err(invalid_tool_response)?;
        let usage = ModelUsage {
            input_tokens: input,
            output_tokens: output,
            total_tokens: total,
            cached_input_tokens: self.cache_read_tokens,
            reasoning_tokens: None,
            context_window_tokens: None,
        };
        Ok((Some(observed), Some(usage)))
    }

    pub(crate) fn finish(self) -> Result<StreamFinish, StreamFailure> {
        if !self.started || !self.stopped {
            return Err(invalid("provider stream ended before the message ended").into());
        }
        let stop_reason = self
            .stop_reason
            .as_deref()
            .ok_or_else(|| invalid("provider stream ended without a stop reason"))?;
        let (observed, usage) = self.usage()?;
        match stop_reason {
            "end_turn" | "stop_sequence" | "tool_use" => {}
            "max_tokens" | "model_context_window_exceeded" => {
                return Err(StreamFailure::Truncated(Box::new(TruncatedUsage {
                    observed,
                    usage,
                })));
            }
            "refusal" => return Err(invalid("provider response was refused").into()),
            _ => return Err(invalid("provider returned an unsupported stop reason").into()),
        }
        if self.blocks.values().any(|block| !block.closed) {
            return Err(invalid("provider left a content block open").into());
        }
        let mut seen_ids = std::collections::BTreeSet::new();
        let mut calls = Vec::new();
        for block in self.blocks.into_values() {
            let OpenBlock::ToolUse {
                id,
                name,
                arguments,
                initial,
            } = block.block
            else {
                continue;
            };
            if !valid_tool_call_id(&id) || !valid_tool_name(&name) || !seen_ids.insert(id.clone()) {
                return Err(invalid_tool_response("invalid tool-call identity").into());
            }
            let arguments = if !arguments.is_empty() {
                arguments
            } else if !initial.is_empty() {
                initial
            } else {
                "{}".to_owned()
            };
            let parsed: serde_json::Value = serde_json::from_str(&arguments)
                .map_err(|_| invalid_tool_response("tool-call arguments were not valid JSON"))?;
            if !parsed.is_object() {
                return Err(
                    invalid_tool_response("tool-call arguments must be a JSON object").into(),
                );
            }
            calls.push(ModelToolCall {
                id,
                name,
                arguments,
            });
        }
        Ok(StreamFinish {
            calls,
            observed,
            usage,
        })
    }
}

fn stream_error(error: Option<&serde_json::Value>) -> ProviderError {
    let kind = error
        .and_then(|error| error.get("type"))
        .and_then(serde_json::Value::as_str)
        .filter(|kind| {
            !kind.is_empty()
                && kind.len() <= MAX_STREAM_ERROR_TYPE_BYTES
                && kind
                    .chars()
                    .all(|character| character.is_ascii_alphanumeric() || character == '_')
        })
        .unwrap_or("unknown_error");
    ProviderError::Api(format!("provider stream reported {kind}"))
}

pub struct AnthropicProvider {
    client: reqwest::Client,
    probe_client: reqwest::Client,
    endpoint: Url,
    model: String,
    api_key: SecretValue,
    probe_timeout: Duration,
    preparation_binding: uuid::Uuid,
}

impl std::fmt::Debug for AnthropicProvider {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("AnthropicProvider")
            .field("endpoint", &public_endpoint(&self.endpoint))
            .field("model", &self.model)
            .finish_non_exhaustive()
    }
}

impl AnthropicProvider {
    /// `endpoint` is the API base, such as `https://api.anthropic.com/v1/`.
    pub fn new(
        endpoint: impl AsRef<str>,
        model: impl Into<String>,
        api_key: SecretValue,
    ) -> Result<Self, ProviderError> {
        let mut endpoint = Url::parse(endpoint.as_ref())?;
        if !matches!(endpoint.scheme(), "http" | "https") || endpoint.host_str().is_none() {
            return Err(ProviderError::InvalidProfile {
                field: "endpoint",
                reason: "use an HTTP or HTTPS URL with a host",
            });
        }
        if !endpoint.path().ends_with('/') {
            endpoint.set_path(&format!("{}/", endpoint.path()));
        }
        // The key travels in a custom header that redirects would forward, so
        // redirects are never followed.
        Ok(Self {
            client: reqwest::Client::builder()
                .connect_timeout(MODEL_CONNECT_TIMEOUT)
                .redirect(reqwest::redirect::Policy::none())
                .build()?,
            probe_client: reqwest::Client::builder()
                .connect_timeout(DEFAULT_PROBE_TIMEOUT)
                .redirect(reqwest::redirect::Policy::none())
                .build()?,
            endpoint,
            model: model.into(),
            api_key,
            probe_timeout: DEFAULT_PROBE_TIMEOUT,
            preparation_binding: uuid::Uuid::new_v4(),
        })
    }

    fn messages_url(&self) -> Result<Url, ProviderError> {
        Ok(self.endpoint.join("messages")?)
    }

    fn headers(&self) -> Result<HeaderMap, ProviderError> {
        let mut headers = HeaderMap::new();
        let mut key = HeaderValue::from_str(self.api_key.expose()).map_err(|_| {
            ProviderError::CredentialsRequired {
                provider_id: "anthropic".into(),
            }
        })?;
        key.set_sensitive(true);
        headers.insert(ANTHROPIC_KEY_HEADER, key);
        headers.insert(
            ANTHROPIC_VERSION_HEADER,
            HeaderValue::from_static(ANTHROPIC_VERSION),
        );
        Ok(headers)
    }

    fn prepared_body(&self, prepared: &PreparedProviderRequest) -> Result<Bytes, ProviderError> {
        let PreparedTransport::Messages(messages) = &prepared.transport else {
            return Err(ProviderError::PreparedRequestBindingMismatch);
        };
        if messages.provider_binding != self.preparation_binding
            || messages.endpoint != self.endpoint.as_str()
            || messages.model != prepared.request.model
            || messages.output_limit != prepared.request.output_limit
            || messages.profile_id != prepared.accounting.profile_id
            || messages.budget_revision != prepared.accounting.budget_revision
            || messages.body.len() as u64 != prepared.accounting.accounted_input
        {
            return Err(ProviderError::PreparedRequestBindingMismatch);
        }
        Ok(messages.body.clone())
    }

    async fn fetch_models(&self) -> Result<Vec<ProviderModel>, ProviderError> {
        let response = self
            .probe_client
            .get(self.endpoint.join("models")?)
            .headers(self.headers()?)
            .timeout(self.probe_timeout)
            .send()
            .await
            .map_err(|error| self.map_probe_error(error))?;
        let status = response.status();
        if !status.is_success() {
            return Err(ProviderError::ProbeHttpStatus {
                status: status.as_u16(),
            });
        }
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
            let chunk = chunk.map_err(|error| self.map_probe_error(error))?;
            if body.len().saturating_add(chunk.len()) > MAX_PROBE_BODY_BYTES {
                return Err(ProviderError::ProbeResponseTooLarge {
                    limit_bytes: MAX_PROBE_BODY_BYTES,
                });
            }
            body.extend_from_slice(&chunk);
        }
        parse_models_response(&body)
    }

    fn map_probe_error(&self, error: reqwest::Error) -> ProviderError {
        if error.is_timeout() {
            return ProviderError::ProbeTimeout {
                timeout_ms: duration_millis(self.probe_timeout),
            };
        }
        ProviderError::Http(error.without_url())
    }

    async fn stream_body(
        &self,
        body: Bytes,
        sender: mpsc::Sender<ProviderEvent>,
    ) -> Result<(), ProviderError> {
        let mut headers = self.headers()?;
        headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
        let request = self
            .client
            .post(self.messages_url()?)
            .headers(headers)
            .body(body);
        let mut source = EventSource::new(request).map_err(|_| ProviderError::StreamSetup)?;
        // Retry ownership belongs to the runtime, where accounting and
        // cancellation are visible.
        source.set_retry_policy(Box::new(reqwest_eventsource::retry::Never));
        let mut response_opened = false;
        let mut state = MessagesStream::default();
        while let Some(event) = source.next().await {
            match event {
                Ok(Event::Open) => response_opened = true,
                Ok(Event::Message(message)) => {
                    for output in state.push(&message.data)? {
                        let event = match output {
                            StreamOutput::ServedModel(model) => ProviderEvent::ServedModel(model),
                            StreamOutput::Delta(text) => ProviderEvent::Delta(text),
                            StreamOutput::Reasoning(text) => ProviderEvent::ReasoningDelta(text),
                        };
                        sender
                            .send(event)
                            .await
                            .map_err(|_| ProviderError::ConsumerClosed)?;
                    }
                    if state.stopped() {
                        break;
                    }
                }
                Err(reqwest_eventsource::Error::StreamEnded) => break,
                Err(reqwest_eventsource::Error::InvalidStatusCode(status, response)) => {
                    let retry_after_ms = parse_retry_after(response.headers().get(RETRY_AFTER));
                    let safe_detail = read_error_identifiers(response).await;
                    // 529 is Anthropic's overloaded status.
                    if matches!(status.as_u16(), 429 | 503 | 529) {
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
        source.close();
        let finish = match state.finish() {
            Ok(finish) => finish,
            Err(StreamFailure::Truncated(truncated)) => {
                let TruncatedUsage { observed, usage } = *truncated;
                send_usage(&sender, observed, usage).await?;
                return Err(ProviderError::ResponseTruncated);
            }
            Err(StreamFailure::Error(error)) => return Err(error),
        };
        if let Some(observed) = finish.observed {
            sender
                .send(ProviderEvent::UsageObservation(observed))
                .await
                .map_err(|_| ProviderError::ConsumerClosed)?;
        } else {
            sender
                .send(ProviderEvent::UsageUncertain)
                .await
                .map_err(|_| ProviderError::ConsumerClosed)?;
        }
        for call in finish.calls {
            sender
                .send(ProviderEvent::ToolCall(call))
                .await
                .map_err(|_| ProviderError::ConsumerClosed)?;
        }
        if let Some(usage) = finish.usage {
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

    #[cfg(test)]
    pub(crate) fn encoded_body_for_test(request: ModelRequest) -> serde_json::Value {
        serde_json::to_value(build_messages_request(request).expect("messages request"))
            .expect("request JSON")
    }
}

/// Keeps only the error type and request ID from an error body. Free-text
/// messages are dropped, since they can echo request content.
async fn read_error_identifiers(response: reqwest::Response) -> String {
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
    error_identifiers(&body)
}

fn error_identifiers(body: &[u8]) -> String {
    let Ok(value) = serde_json::from_slice::<serde_json::Value>(body) else {
        return String::new();
    };
    let mut fields = Vec::new();
    if let Some(kind) = value
        .get("error")
        .and_then(|error| error.get("type"))
        .and_then(|kind| bounded_provider_identifier(kind, MAX_PROVIDER_ERROR_CODE_BYTES))
    {
        fields.push(format!("code={kind}"));
    }
    if let Some(request_id) = value
        .get("request_id")
        .and_then(|id| bounded_provider_identifier(id, MAX_PROVIDER_ERROR_REQUEST_ID_BYTES))
    {
        fields.push(format!("request_id={request_id}"));
    }
    if fields.is_empty() {
        String::new()
    } else {
        format!("; provider detail: {}", fields.join(", "))
    }
}

async fn send_usage(
    sender: &mpsc::Sender<ProviderEvent>,
    observed: Option<ObservedUsage>,
    usage: Option<ModelUsage>,
) -> Result<(), ProviderError> {
    if let Some(observed) = observed {
        sender
            .send(ProviderEvent::UsageObservation(observed))
            .await
            .map_err(|_| ProviderError::ConsumerClosed)?;
    }
    if let Some(usage) = usage {
        sender
            .send(ProviderEvent::Usage(usage))
            .await
            .map_err(|_| ProviderError::ConsumerClosed)?;
    }
    Ok(())
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
        let _ = self.fetch_models().await?;
        let mut status = self.status();
        status.connected = true;
        status.detail = reachable_detail(&self.model);
        Ok(status)
    }

    async fn list_models(&self) -> Result<Vec<ProviderModel>, ProviderError> {
        self.fetch_models().await
    }

    fn prepare_request(
        &self,
        profile: &ProviderProfile,
        budget: &argentum_domain::context_budget::ModelBudget,
        request: ModelRequest,
    ) -> Result<PreparedProviderRequest, ProviderError> {
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
        let (body, accounting) = prepare_messages_wire(profile, budget, &request)?;
        Ok(PreparedProviderRequest {
            transport: PreparedTransport::Messages(PreparedCompatibleRequest {
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

    async fn validate_prepared_request(
        &self,
        prepared: &PreparedProviderRequest,
    ) -> Result<ProviderRequestValidation, ProviderError> {
        let _ = self.prepared_body(prepared)?;
        Ok(ProviderRequestValidation {
            exact_input_tokens: None,
            context_tokens: None,
            output_limit: prepared.request.output_limit,
            representation_fingerprint: None,
            runtime_fingerprint: None,
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
        let body = build_messages_request(ModelRequest { model, ..request })?;
        let body = Bytes::from(
            serde_json::to_vec(&body)
                .map_err(|_| ProviderError::InvalidRequest("wire encoding failed"))?,
        );
        self.stream_body(body, sender).await
    }

    async fn stream_prepared_validated_with_cancellation(
        &self,
        prepared: &PreparedProviderRequest,
        _expected: &ProviderRequestValidation,
        sender: mpsc::Sender<ProviderEvent>,
        cancellation: CancellationToken,
    ) -> Result<(), ProviderError> {
        let body = self.prepared_body(prepared)?;
        tokio::select! {
            _ = cancellation.cancelled() => Err(ProviderError::RequestCancelled),
            result = self.stream_body(body, sender) => result,
        }
    }
}

#[cfg(test)]
mod tests;
