//! Synthetic Messages API fixtures. Nothing here contacts a real provider;
//! HTTP tests use a loopback mock server.

use super::*;
use crate::tests::{empty_model_request, event_stream_response, spawn_mock_server_sequence};

fn tool(name: &str) -> ModelToolDefinition {
    ModelToolDefinition {
        name: name.into(),
        description: format!("Synthetic {name}"),
        parameters: serde_json::json!({
            "type": "object",
            "properties": {"path": {"type": "string"}},
            "required": ["path"],
            "additionalProperties": false
        }),
    }
}

fn exchange(id: &str, path: &str, result: &str) -> ModelToolExchange {
    ModelToolExchange {
        chatgpt_continuation: None,
        assistant_continuation: None,
        assistant_content: String::new(),
        assistant_reasoning: String::new(),
        calls: vec![ModelToolCall {
            id: id.into(),
            name: "read_text".into(),
            arguments: serde_json::json!({ "path": path }).to_string(),
        }],
        results: vec![ModelToolResult {
            images: Vec::new(),
            call_id: id.into(),
            content: result.into(),
        }],
    }
}

fn sse(events: &[serde_json::Value]) -> String {
    events
        .iter()
        .map(|event| {
            format!(
                "event: {}\ndata: {}\n\n",
                event["type"].as_str().expect("event type"),
                event
            )
        })
        .collect()
}

fn message_start() -> serde_json::Value {
    serde_json::json!({
        "type": "message_start",
        "message": {
            "id": "msg_synthetic", "type": "message", "role": "assistant",
            "content": [], "model": "claude-sonnet-5-5", "stop_reason": null,
            "usage": {"input_tokens": 120, "output_tokens": 1,
                      "cache_creation_input_tokens": 10, "cache_read_input_tokens": 30}
        }
    })
}

fn text_start(index: usize) -> serde_json::Value {
    serde_json::json!({"type": "content_block_start", "index": index,
                       "content_block": {"type": "text", "text": ""}})
}

fn text_delta(index: usize, text: &str) -> serde_json::Value {
    serde_json::json!({"type": "content_block_delta", "index": index,
                       "delta": {"type": "text_delta", "text": text}})
}

fn tool_start(index: usize, id: &str, name: &str) -> serde_json::Value {
    serde_json::json!({"type": "content_block_start", "index": index,
                       "content_block": {"type": "tool_use", "id": id, "name": name, "input": {}}})
}

fn tool_delta(index: usize, json: &str) -> serde_json::Value {
    serde_json::json!({"type": "content_block_delta", "index": index,
                       "delta": {"type": "input_json_delta", "partial_json": json}})
}

fn block_stop(index: usize) -> serde_json::Value {
    serde_json::json!({"type": "content_block_stop", "index": index})
}

fn message_delta(stop_reason: &str, output_tokens: u64) -> serde_json::Value {
    serde_json::json!({"type": "message_delta",
                       "delta": {"stop_reason": stop_reason, "stop_sequence": null},
                       "usage": {"output_tokens": output_tokens}})
}

fn message_stop() -> serde_json::Value {
    serde_json::json!({"type": "message_stop"})
}

fn text_reply() -> Vec<serde_json::Value> {
    vec![
        message_start(),
        text_start(0),
        serde_json::json!({"type": "ping"}),
        text_delta(0, "Hello"),
        text_delta(0, " there"),
        block_stop(0),
        message_delta("end_turn", 15),
        message_stop(),
    ]
}

fn tool_reply() -> Vec<serde_json::Value> {
    vec![
        message_start(),
        text_start(0),
        text_delta(0, "Reading both."),
        block_stop(0),
        tool_start(1, "toolu_a", "read_text"),
        tool_delta(1, ""),
        tool_delta(1, "{\"path\": \"src/"),
        tool_delta(1, "lib.rs\"}"),
        block_stop(1),
        tool_start(2, "toolu_b", "read_text"),
        tool_delta(2, "{\"path\": \"README.md\"}"),
        block_stop(2),
        message_delta("tool_use", 42),
        message_stop(),
    ]
}

type RunResult = (Vec<StreamOutput>, Result<StreamFinish, StreamFailure>);

fn run(events: &[serde_json::Value]) -> RunResult {
    let mut stream = MessagesStream::default();
    let mut outputs = Vec::new();
    for event in events {
        match stream.push(&event.to_string()) {
            Ok(mut output) => outputs.append(&mut output),
            Err(error) => return (outputs, Err(StreamFailure::Error(error))),
        }
    }
    (outputs, stream.finish())
}

fn expect_error(events: &[serde_json::Value]) -> String {
    match run(events).1 {
        Err(StreamFailure::Error(error)) => error.to_string(),
        Err(StreamFailure::Truncated(_)) => panic!("unexpected truncation"),
        Ok(_) => panic!("stream should fail"),
    }
}

#[test]
fn request_maps_system_history_tools_and_exchanges_to_messages() {
    let mut request = empty_model_request();
    request.model = "claude-sonnet-5-5".into();
    request.system = Some("Follow the workspace policy.".into());
    request.history = vec![
        ModelMessage {
            role: ModelMessageRole::User,
            content: "Earlier question".into(),
            tool_exchange: None,
        },
        ModelMessage {
            role: ModelMessageRole::Assistant,
            content: String::new(),
            tool_exchange: Some(exchange("toolu_old", "old.txt", "old body")),
        },
        ModelMessage {
            role: ModelMessageRole::Assistant,
            content: "Earlier answer".into(),
            tool_exchange: None,
        },
    ];
    request.prompt = "Current task".into();
    request.tools = vec![tool("read_text")];
    let mut current = exchange("toolu_new", "src/lib.rs", "file body");
    current.assistant_content = "Reading it.".into();
    request.tool_exchanges = vec![current];
    request.require_tool_call = true;

    let body = AnthropicProvider::encoded_body_for_test(request);
    assert_eq!(
        body,
        serde_json::json!({
            "model": "claude-sonnet-5-5",
            "max_tokens": 4096,
            "system": "Follow the workspace policy.",
            "messages": [
                {"role": "user", "content": [{"type": "text", "text": "Earlier question"}]},
                {"role": "assistant", "content": [
                    {"type": "tool_use", "id": "toolu_old", "name": "read_text",
                     "input": {"path": "old.txt"}}
                ]},
                {"role": "user", "content": [
                    {"type": "tool_result", "tool_use_id": "toolu_old", "content": "old body"}
                ]},
                {"role": "assistant", "content": [{"type": "text", "text": "Earlier answer"}]},
                {"role": "user", "content": [{"type": "text", "text": "Current task"}]},
                {"role": "assistant", "content": [
                    {"type": "text", "text": "Reading it."},
                    {"type": "tool_use", "id": "toolu_new", "name": "read_text",
                     "input": {"path": "src/lib.rs"}}
                ]},
                {"role": "user", "content": [
                    {"type": "tool_result", "tool_use_id": "toolu_new", "content": "file body"}
                ]}
            ],
            "tools": [{
                "name": "read_text",
                "description": "Synthetic read_text",
                "input_schema": {
                    "type": "object",
                    "properties": {"path": {"type": "string"}},
                    "required": ["path"],
                    "additionalProperties": false
                }
            }],
            "tool_choice": {"type": "any"},
            "stream": true
        })
    );
}

#[test]
fn request_merges_same_role_turns_and_drops_empty_text() {
    let mut request = empty_model_request();
    request.history = vec![
        ModelMessage {
            role: ModelMessageRole::User,
            content: "one".into(),
            tool_exchange: None,
        },
        ModelMessage {
            role: ModelMessageRole::User,
            content: String::new(),
            tool_exchange: None,
        },
    ];
    request.prompt = "two".into();
    let body = AnthropicProvider::encoded_body_for_test(request);
    assert_eq!(
        body["messages"],
        serde_json::json!([{"role": "user", "content": [
            {"type": "text", "text": "one"},
            {"type": "text", "text": "two"}
        ]}])
    );
    assert!(body.get("tools").is_none());
    assert!(body.get("tool_choice").is_none());
    assert!(body.get("system").is_none());
}

#[test]
fn request_rejects_unsafe_or_unbound_input() {
    let mut zero = empty_model_request();
    zero.output_limit = 0;
    assert!(build_messages_request(zero).is_err());

    let mut required = empty_model_request();
    required.require_tool_call = true;
    assert!(build_messages_request(required).is_err());

    let mut mismatched = empty_model_request();
    let mut bad = exchange("toolu_a", "a", "b");
    bad.results[0].call_id = "toolu_other".into();
    mismatched.tool_exchanges = vec![bad];
    assert!(matches!(
        build_messages_request(mismatched),
        Err(ProviderError::InvalidRequest(_))
    ));

    let mut bad_arguments = empty_model_request();
    let mut bad = exchange("toolu_a", "a", "b");
    bad.calls[0].arguments = "[1]".into();
    bad_arguments.tool_exchanges = vec![bad];
    assert!(build_messages_request(bad_arguments).is_err());

    let mut oversized = empty_model_request();
    oversized.tool_exchanges = vec![exchange(
        "toolu_a",
        "a",
        &"x".repeat(MAX_TOOL_RESULT_BYTES + 1),
    )];
    assert!(build_messages_request(oversized).is_err());
}

#[test]
fn text_stream_maps_deltas_model_and_cache_aware_usage() {
    let (outputs, finish) = run(&text_reply());
    assert_eq!(
        outputs,
        vec![
            StreamOutput::ServedModel("claude-sonnet-5-5".into()),
            StreamOutput::Delta("Hello".into()),
            StreamOutput::Delta(" there".into()),
        ]
    );
    let finish = finish.expect("complete stream");
    assert!(finish.calls.is_empty());
    let usage = finish.usage.expect("usage");
    assert_eq!(usage.input_tokens, 160);
    assert_eq!(usage.output_tokens, 15);
    assert_eq!(usage.total_tokens, 175);
    assert_eq!(usage.cached_input_tokens, Some(30));
    let observed = finish.observed.expect("observed usage");
    assert_eq!(observed.total_tokens, 175);
    assert!(observed.validate().is_ok());
}

#[test]
fn tool_stream_assembles_bounded_calls_in_block_order() {
    let (outputs, finish) = run(&tool_reply());
    assert!(outputs.contains(&StreamOutput::Delta("Reading both.".into())));
    let finish = finish.expect("complete stream");
    assert_eq!(
        finish.calls,
        vec![
            ModelToolCall {
                id: "toolu_a".into(),
                name: "read_text".into(),
                arguments: "{\"path\": \"src/lib.rs\"}".into(),
            },
            ModelToolCall {
                id: "toolu_b".into(),
                name: "read_text".into(),
                arguments: "{\"path\": \"README.md\"}".into(),
            },
        ]
    );
}

#[test]
fn tool_input_in_the_start_block_is_used_when_no_delta_follows() {
    let events = vec![
        message_start(),
        serde_json::json!({"type": "content_block_start", "index": 0,
                           "content_block": {"type": "tool_use", "id": "toolu_a",
                                             "name": "list_files", "input": {"path": "."}}}),
        block_stop(0),
        tool_start(1, "toolu_b", "status"),
        block_stop(1),
        message_delta("tool_use", 5),
        message_stop(),
    ];
    let calls = run(&events).1.expect("complete stream").calls;
    assert_eq!(calls[0].arguments, "{\"path\":\".\"}");
    assert_eq!(calls[1].arguments, "{}");
}

#[test]
fn truncated_tool_stream_reports_usage_but_releases_no_call() {
    let mut events = tool_reply();
    let stop = events.len() - 2;
    events[stop] = message_delta("max_tokens", 4096);
    match run(&events).1 {
        Err(StreamFailure::Truncated(truncated)) => {
            assert_eq!(truncated.usage.expect("usage").output_tokens, 4096);
            assert!(truncated.observed.is_some());
        }
        other => panic!("expected truncation, got {other:?}"),
    }
    let mut events = text_reply();
    let stop = events.len() - 2;
    events[stop] = message_delta("model_context_window_exceeded", 20);
    assert!(matches!(run(&events).1, Err(StreamFailure::Truncated(_))));
}

#[test]
fn thinking_blocks_stream_as_reasoning_only() {
    let events = vec![
        message_start(),
        serde_json::json!({"type": "content_block_start", "index": 0,
                           "content_block": {"type": "thinking", "thinking": ""}}),
        serde_json::json!({"type": "content_block_delta", "index": 0,
                           "delta": {"type": "thinking_delta", "thinking": "Plan."}}),
        serde_json::json!({"type": "content_block_delta", "index": 0,
                           "delta": {"type": "signature_delta", "signature": "c2ln"}}),
        block_stop(0),
        serde_json::json!({"type": "content_block_start", "index": 1,
                           "content_block": {"type": "redacted_thinking", "data": "opaque"}}),
        block_stop(1),
        serde_json::json!({"type": "message_delta", "delta": {"stop_reason": "end_turn"}}),
        message_stop(),
    ];
    let (outputs, finish) = run(&events);
    assert_eq!(
        outputs,
        vec![
            StreamOutput::ServedModel("claude-sonnet-5-5".into()),
            StreamOutput::Reasoning("Plan.".into()),
        ]
    );
    let finish = finish.expect("complete stream");
    assert!(finish.calls.is_empty());
    assert!(finish.usage.is_some());
}

#[test]
fn malformed_or_unsafe_streams_fail_closed() {
    let start = message_start();
    let stop_ok =
        serde_json::json!({"type": "message_delta", "delta": {"stop_reason": "tool_use"}});
    let end = message_stop();

    // Content before message_start.
    assert!(expect_error(&[tool_start(0, "toolu_a", "read_text")]).contains("did not start"));

    // Too many tool calls.
    let mut many = vec![start.clone()];
    for index in 0..=MAX_TOOL_CALLS_PER_RESPONSE {
        many.push(tool_start(index, &format!("toolu_{index}"), "read_text"));
    }
    assert!(expect_error(&many).contains("too many tool calls"));

    // Oversized arguments, sent in event-sized pieces.
    let piece = "x".repeat(MAX_STREAM_EVENT_BYTES / 2);
    let mut oversized = vec![start.clone(), tool_start(0, "toolu_a", "read_text")];
    for _ in 0..=(MAX_TOOL_ARGUMENT_BYTES / piece.len()) {
        oversized.push(tool_delta(0, &piece));
    }
    assert!(expect_error(&oversized).contains("safety limit"));

    // Non-object and invalid JSON arguments.
    for arguments in ["[1, 2]", "{\"path\":"] {
        let error = expect_error(&[
            start.clone(),
            tool_start(0, "toolu_a", "read_text"),
            tool_delta(0, arguments),
            block_stop(0),
            stop_ok.clone(),
            end.clone(),
        ]);
        assert!(error.contains("tool-call arguments"), "{error}");
    }

    // Duplicate ids and invalid names.
    assert!(expect_error(&[
        start.clone(),
        tool_start(0, "toolu_a", "read_text"),
        block_stop(0),
        tool_start(1, "toolu_a", "read_text"),
        block_stop(1),
        stop_ok.clone(),
        end.clone(),
    ])
    .contains("identity"));
    assert!(expect_error(&[
        start.clone(),
        tool_start(0, "toolu_a", "read text"),
        block_stop(0),
        stop_ok.clone(),
        end.clone(),
    ])
    .contains("identity"));

    // A delta that does not match its block, and an unknown block.
    assert!(expect_error(&[
        start.clone(),
        tool_start(0, "toolu_a", "read_text"),
        text_delta(0, "hi"),
    ])
    .contains("did not match"));
    assert!(expect_error(&[start.clone(), text_delta(3, "hi")]).contains("unknown content block"));
    assert!(expect_error(&[start.clone(), text_start(0), text_start(0)]).contains("reused"));

    // Server-side tool blocks are not part of the shared contract.
    assert!(expect_error(&[
        start.clone(),
        serde_json::json!({"type": "content_block_start", "index": 0,
                           "content_block": {"type": "server_tool_use", "id": "srvtoolu_a",
                                             "name": "web_search", "input": {}}}),
    ])
    .contains("unsupported content block"));

    // An open block, a missing stop, data after the stop.
    assert!(expect_error(&[
        start.clone(),
        tool_start(0, "toolu_a", "read_text"),
        stop_ok.clone(),
        end.clone()
    ])
    .contains("open"));
    assert!(expect_error(&[start.clone(), stop_ok.clone()]).contains("before the message ended"));
    assert!(
        expect_error(&[start.clone(), stop_ok.clone(), end.clone(), end.clone()])
            .contains("after the message ended")
    );
    assert!(expect_error(&[start.clone(), start.clone()]).contains("twice"));

    // Refusal and stop reasons outside the shared contract.
    assert!(expect_error(&[
        start.clone(),
        serde_json::json!({"type": "message_delta", "delta": {"stop_reason": "refusal"}}),
        end.clone(),
    ])
    .contains("refused"));
    assert!(expect_error(&[
        start.clone(),
        serde_json::json!({"type": "message_delta", "delta": {"stop_reason": "pause_turn"}}),
        end.clone(),
    ])
    .contains("unsupported stop reason"));

    // A provider error event keeps only a bounded type name.
    let error = expect_error(&[
        start.clone(),
        serde_json::json!({"type": "error",
                           "error": {"type": "overloaded_error", "message": "secret detail"}}),
    ]);
    assert!(error.contains("overloaded_error"));
    assert!(!error.contains("secret detail"));

    // Oversized events and decreasing usage.
    let mut stream = MessagesStream::default();
    assert!(stream
        .push(&" ".repeat(MAX_STREAM_EVENT_BYTES + 1))
        .is_err());
    assert!(expect_error(&[
        start.clone(),
        serde_json::json!({"type": "message_delta", "delta": {}, "usage": {"input_tokens": 1}}),
    ])
    .contains("conflicting provider usage"));
}

#[test]
fn visible_text_is_bounded() {
    let chunk = "x".repeat(MAX_STREAM_EVENT_BYTES / 2);
    let mut events = vec![message_start(), text_start(0)];
    for _ in 0..=(MAX_VISIBLE_RESPONSE_BYTES / chunk.len()) {
        events.push(text_delta(0, &chunk));
    }
    assert!(expect_error(&events).contains("safety limit"));
}

fn profile(
    endpoint: &str,
) -> (
    ProviderProfile,
    argentum_domain::context_budget::ModelBudget,
) {
    let profile = ProviderProfile {
        id: "anthropic".into(),
        label: "Anthropic".into(),
        kind: ProviderKind::Anthropic,
        endpoint: endpoint.into(),
        model: "claude-sonnet-5-5".into(),
        selected: true,
    };
    let budget = argentum_domain::context_budget::ModelBudget {
        revision: uuid::Uuid::new_v4(),
        profile_id: profile.id.clone(),
        provider_kind: profile.kind,
        endpoint: profile.endpoint.clone(),
        model: profile.model.clone(),
        usable_context_tokens: 200_000,
        input_limit_tokens: None,
        maximum_output_tokens: 8_192,
        requested_output_tokens: 4_096,
        uncertainty_tokens: 1_024,
    };
    (profile, budget)
}

#[test]
fn prepared_accounting_matches_the_exact_body() {
    let (profile, budget) = profile("https://api.anthropic.com/v1/");
    let mut request = empty_model_request();
    request.model = profile.model.clone();
    request.tools = vec![tool("read_text")];
    request.system = Some("System".into());
    let (body, accounting) = prepare_messages_wire(&profile, &budget, &request).expect("prepared");
    assert_eq!(accounting.accounted_input, body.len() as u64);
    assert!(accounting.serialized_catalog_bytes > 0);
    assert_eq!(
        crate::account_chat_request(&profile, &budget, &request)
            .expect("accounting")
            .accounted_input,
        accounting.accounted_input
    );

    let mut wrong_model = request.clone();
    wrong_model.model = "claude-other".into();
    assert!(prepare_messages_wire(&profile, &budget, &wrong_model).is_err());

    let provider = AnthropicProvider::new(
        "https://api.anthropic.com/v1/",
        "claude-sonnet-5-5",
        SecretValue::new("test-key"),
    )
    .expect("provider");
    let other = AnthropicProvider::new(
        "https://api.anthropic.com/v1/",
        "claude-sonnet-5-5",
        SecretValue::new("test-key"),
    )
    .expect("provider");
    let prepared = provider
        .prepare_request(&profile, &budget, request)
        .expect("prepared request");
    assert!(provider.prepared_body(&prepared).is_ok());
    assert!(matches!(
        other.prepared_body(&prepared),
        Err(ProviderError::PreparedRequestBindingMismatch)
    ));
}

async fn collect(mut receiver: mpsc::Receiver<ProviderEvent>) -> Vec<ProviderEvent> {
    let mut events = Vec::new();
    while let Some(event) = receiver.recv().await {
        events.push(event);
    }
    events
}

fn loopback_provider(endpoint: &str, key: &str) -> AnthropicProvider {
    AnthropicProvider::new(
        format!("{endpoint}/"),
        "claude-sonnet-5-5",
        SecretValue::new(key),
    )
    .expect("provider")
}

#[tokio::test]
async fn loopback_stream_sends_versioned_headers_and_maps_tool_events() {
    let response = event_stream_response(&sse(&tool_reply()));
    let (endpoint, requests, worker) = spawn_mock_server_sequence(vec![response]);
    let (profile, budget) = profile(&format!("{endpoint}/"));
    let provider = loopback_provider(&endpoint, "synthetic-test-key");
    let mut request = empty_model_request();
    request.model = profile.model.clone();
    request.output_limit = budget.requested_output_tokens;
    request.tools = vec![tool("read_text")];
    let prepared = provider
        .prepare_request(&profile, &budget, request)
        .expect("prepared");
    let expected = provider
        .validate_prepared_request(&prepared)
        .await
        .expect("validated");
    let (sender, receiver) = mpsc::channel(64);
    provider
        .stream_prepared_validated_with_cancellation(
            &prepared,
            &expected,
            sender,
            CancellationToken::new(),
        )
        .await
        .expect("stream");
    let events = collect(receiver).await;
    worker.join().expect("mock server");
    let request = requests.recv().expect("requests").remove(0);
    let lowered = request.to_ascii_lowercase();
    assert!(request.starts_with("POST /v1/messages "), "{request}");
    assert!(lowered.contains("x-api-key: synthetic-test-key"));
    assert!(lowered.contains("anthropic-version: 2023-06-01"));
    assert!(!lowered.contains("authorization:"));
    let sent_body = request.split("\r\n\r\n").nth(1).expect("body");
    assert_eq!(
        sent_body.len() as u64,
        prepared.accounting().accounted_input
    );

    let calls = events
        .iter()
        .filter_map(|event| match event {
            ProviderEvent::ToolCall(call) => Some(call.id.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(calls, ["toolu_a", "toolu_b"]);
    assert!(
        matches!(events.first(), Some(ProviderEvent::ServedModel(model)) if model == "claude-sonnet-5-5")
    );
    assert!(events.iter().any(
        |event| matches!(event, ProviderEvent::UsageObservation(usage) if usage.total_tokens == 202)
    ));
    assert!(matches!(events.last(), Some(ProviderEvent::Completed)));
}

#[tokio::test]
async fn loopback_truncation_returns_an_error_without_tool_calls() {
    let mut reply = tool_reply();
    let stop = reply.len() - 2;
    reply[stop] = message_delta("max_tokens", 4096);
    let (endpoint, _requests, worker) =
        spawn_mock_server_sequence(vec![event_stream_response(&sse(&reply))]);
    let provider = loopback_provider(&endpoint, "synthetic-test-key");
    let (sender, receiver) = mpsc::channel(64);
    let result = provider.stream(empty_model_request(), sender).await;
    let events = collect(receiver).await;
    worker.join().expect("mock server");
    assert!(matches!(result, Err(ProviderError::ResponseTruncated)));
    assert!(!events
        .iter()
        .any(|event| matches!(event, ProviderEvent::ToolCall(_) | ProviderEvent::Completed)));
    assert!(events
        .iter()
        .any(|event| matches!(event, ProviderEvent::Usage(_))));
}

#[tokio::test]
async fn loopback_overload_is_retryable_and_hides_the_key() {
    let key = "synthetic-key-must-not-render";
    let body = format!(
        r#"{{"type":"error","error":{{"type":"overloaded_error","message":"busy {key}"}},"request_id":"req_1"}}"#
    );
    let response = format!(
        "HTTP/1.1 529 Overloaded\r\nContent-Type: application/json\r\nRetry-After: 3\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    let (endpoint, _requests, worker) = spawn_mock_server_sequence(vec![response]);
    let provider = loopback_provider(&endpoint, key);
    let (sender, _receiver) = mpsc::channel(8);
    let error = provider
        .stream(empty_model_request(), sender)
        .await
        .expect_err("overloaded");
    worker.join().expect("mock server");
    assert!(matches!(
        error,
        ProviderError::TransientHttpStatus {
            status: 529,
            retry_after_ms: Some(3_000),
            ..
        }
    ));
    let rendered = error.to_string();
    assert!(!rendered.contains(key), "{rendered}");
    assert!(!rendered.contains("busy"), "{rendered}");
    assert!(rendered.contains("code=overloaded_error"), "{rendered}");
    assert!(rendered.contains("request_id=req_1"), "{rendered}");
}

#[tokio::test]
async fn loopback_models_list_uses_the_credential_headers() {
    let body = r#"{"data":[{"type":"model","id":"claude-sonnet-5-5","display_name":"Claude Sonnet 5.5","created_at":"2026-01-01T00:00:00Z"}],"has_more":false,"first_id":"claude-sonnet-5-5","last_id":"claude-sonnet-5-5"}"#;
    let response = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    let (endpoint, requests, worker) = spawn_mock_server_sequence(vec![response]);
    let provider = loopback_provider(&endpoint, "synthetic-test-key");
    let models = provider.list_models().await.expect("models");
    worker.join().expect("mock server");
    assert_eq!(models[0].id, "claude-sonnet-5-5");
    assert_eq!(models[0].label, "Claude Sonnet 5.5");
    let request = requests.recv().expect("requests").remove(0);
    assert!(request.starts_with("GET /v1/models "));
    assert!(request
        .to_ascii_lowercase()
        .contains("anthropic-version: 2023-06-01"));
}
