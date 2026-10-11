//! Synthetic-only wire evidence for the documented MiniMax continuation shape.

use super::*;
use crate::tests::{event_stream_response, spawn_mock_server_sequence, test_provider};

fn scope() -> ContinuationScope {
    ContinuationScope {
        session_id: uuid::Uuid::new_v4(),
        run_id: uuid::Uuid::new_v4(),
    }
}

fn synthetic_call(id: &str) -> ModelToolCall {
    ModelToolCall {
        id: id.into(),
        name: "read_text".into(),
        arguments: r#"{"path":"notes.txt"}"#.into(),
    }
}

fn push_json(
    state: &mut minimax_continuation::Accumulator,
    value: serde_json::Value,
    finish: Option<&str>,
) -> Result<(), ProviderError> {
    let delta: ChatDelta = serde_json::from_value(value)
        .map_err(|_| ProviderError::InvalidResponse("synthetic malformed state".into()))?;
    state.push(Some("synthetic-response"), &delta, finish)
}

fn bound_fixture() -> (
    OpenAiCompatibleProvider,
    ProviderProfile,
    argentum_domain::context_budget::ModelBudget,
    ModelRequest,
) {
    let provider = test_provider("https://api.minimax.io/v1/", None);
    let (mut profile, mut budget, mut request) =
        crate::tests::prepared_fixture("https://api.minimax.io/v1/");
    profile.id = "minimax".into();
    budget.profile_id = profile.id.clone();
    let scope = scope();
    request.continuation_scope = Some(scope);
    let calls = vec![synthetic_call("synthetic-call")];
    let mut assembly = minimax_continuation::Accumulator::new(
        &profile.endpoint,
        &profile.model,
        provider.preparation_binding,
    )
    .unwrap();
    push_json(&mut assembly, serde_json::json!({"content":"visible", "reasoning_details":[
        {"type":"reasoning.text","id":"synthetic-private-id","format":"MiniMax-response-v1","index":0,"text":"SYNTHETIC_PRIVATE_STATE"}
    ]}), Some("tool_calls")).unwrap();
    request.tool_exchanges.push(ModelToolExchange {
        chatgpt_continuation: None,
        assistant_continuation: Some(Box::new(
            assembly
                .finish(&calls)
                .unwrap()
                .bind_run(&profile, scope, uuid::Uuid::new_v4())
                .unwrap(),
        )),
        assistant_content: "visible".into(),
        assistant_reasoning: "SYNTHETIC_PRIVATE_STATE".into(),
        calls,
        results: vec![ModelToolResult {
            images: Default::default(),
            call_id: "synthetic-call".into(),
            content: "bounded result".into(),
        }],
    });
    (provider, profile, budget, request)
}

#[test]
fn continuation_has_no_public_serialization_or_debug_reasoning_projection() {
    let (_, _, _, request) = bound_fixture();
    assert!(!serde_json::to_string(&request)
        .unwrap()
        .contains("SYNTHETIC_PRIVATE_STATE"));
    assert!(
        !format!("{:?}", request.tool_exchanges[0].assistant_continuation)
            .contains("SYNTHETIC_PRIVATE_STATE")
    );
    assert!(
        !format!("{:?}", request.tool_exchanges[0].assistant_continuation)
            .contains("synthetic-private-id")
    );
}

#[test]
fn incremental_details_preserve_prefix_collisions_and_optional_metadata() {
    let mut assembly = minimax_continuation::Accumulator::new(
        "https://api.minimax.io/v1/",
        "test-model",
        uuid::Uuid::new_v4(),
    )
    .unwrap();
    push_json(&mut assembly, serde_json::json!({"reasoning_details":[{"type":"reasoning.text","id":"block","format":"MiniMax-response-v1","index":0,"text":"SYN"}]}), None).unwrap();
    push_json(
        &mut assembly,
        serde_json::json!({"reasoning_details":[{"text":"SYN-more"}]}),
        Some("tool_calls"),
    )
    .unwrap();
    let state = assembly.finish(&[synthetic_call("call")]).unwrap();
    assert_eq!(state.reasoning_details.len(), 1);
    let detail = &state.reasoning_details[0];
    assert_eq!(detail.text.as_deref(), Some("SYNSYN-more"));
    assert_eq!(detail.id.as_deref(), Some("block"));
    assert_eq!(detail.format.as_deref(), Some("MiniMax-response-v1"));
    assert_eq!(detail.index, Some(0));
}

#[test]
fn late_detail_metadata_binds_the_existing_positional_block_without_duplication() {
    let mut assembly = minimax_continuation::Accumulator::new(
        "https://api.minimax.io/v1/",
        "test-model",
        uuid::Uuid::new_v4(),
    )
    .unwrap();
    push_json(
        &mut assembly,
        serde_json::json!({"reasoning_details":[{"text":"first "}]}),
        None,
    )
    .unwrap();
    push_json(&mut assembly, serde_json::json!({"reasoning_details":[{
        "type":"reasoning.text","id":"late-id","format":"MiniMax-response-v1","index":0,"text":"second"
    }]}), Some("tool_calls")).unwrap();
    let state = assembly.finish(&[synthetic_call("call")]).unwrap();
    assert_eq!(state.reasoning_details.len(), 1);
    assert_eq!(
        state.reasoning_details[0].text.as_deref(),
        Some("first second")
    );
    assert_eq!(state.reasoning_details[0].id.as_deref(), Some("late-id"));
}

#[test]
fn native_content_and_split_content_are_preserved_separately_from_display() {
    let mut assembly = minimax_continuation::Accumulator::new(
        "https://api.minimax.io/v1/",
        "test-model",
        uuid::Uuid::new_v4(),
    )
    .unwrap();
    push_json(
        &mut assembly,
        serde_json::json!({"content":"<thi", "reasoning_content":"SYN"}),
        None,
    )
    .unwrap();
    push_json(
        &mut assembly,
        serde_json::json!({"content":"nk>NATIVE</think> visible", "reasoning_content":"SYN-more"}),
        Some("tool_calls"),
    )
    .unwrap();
    let state = assembly.finish(&[synthetic_call("call")]).unwrap();
    assert_eq!(
        state.content.as_deref(),
        Some("<think>NATIVE</think> visible")
    );
    assert_eq!(state.reasoning_content.as_deref(), Some("SYNSYN-more"));
}

#[test]
fn no_reasoning_response_is_valid_without_fabricating_a_reasoning_block() {
    let mut assembly = minimax_continuation::Accumulator::new(
        "https://api.minimax.io/v1/",
        "test-model",
        uuid::Uuid::new_v4(),
    )
    .unwrap();
    push_json(
        &mut assembly,
        serde_json::json!({"role":"assistant","content":""}),
        Some("tool_calls"),
    )
    .unwrap();
    let state = assembly.finish(&[synthetic_call("call")]).unwrap();
    assert_eq!(state.content.as_deref(), Some(""));
    assert!(state.reasoning_details.is_empty());
    assert!(state.reasoning_content.is_none());
}

#[test]
fn repeated_stream_metadata_charges_retained_state_not_sse_frame_overhead() {
    let mut assembly = minimax_continuation::Accumulator::new(
        "https://api.minimax.io/v1/",
        "test-model",
        uuid::Uuid::new_v4(),
    )
    .unwrap();
    for _ in 0..4096 {
        push_json(&mut assembly, serde_json::json!({"reasoning_details":[{
            "type":"reasoning.text","id":"one","format":"MiniMax-response-v1","index":0,"text":"x"
        }]}), None).unwrap();
    }
    push_json(&mut assembly, serde_json::json!({}), Some("tool_calls")).unwrap();
    let state = assembly.finish(&[synthetic_call("call")]).unwrap();
    assert_eq!(
        state.reasoning_details[0].text.as_ref().unwrap().len(),
        4096
    );
    assert!(state.retained_bytes() < 8192);
}

#[test]
fn malformed_oversized_conflicting_and_partial_state_fail_closed() {
    for value in [
        serde_json::json!({"role":"user"}),
        serde_json::json!({"reasoning":"unsupported MiniMax alias"}),
        serde_json::json!({"reasoning_details":[{"type":"reasoning.encrypted","text":"synthetic"}]}),
        serde_json::json!({"reasoning_details":[{"index":32,"text":"synthetic"}]}),
        serde_json::json!({"reasoning_details":[{"id":"x".repeat(257),"text":"synthetic"}]}),
        serde_json::json!({"reasoning_content":"x".repeat(512*1024+1)}),
    ] {
        let mut assembly = minimax_continuation::Accumulator::new(
            "https://api.minimax.io/v1/",
            "test-model",
            uuid::Uuid::new_v4(),
        )
        .unwrap();
        assert!(push_json(&mut assembly, value, None).is_err());
    }
    let mut assembly = minimax_continuation::Accumulator::new(
        "https://api.minimax.io/v1/",
        "test-model",
        uuid::Uuid::new_v4(),
    )
    .unwrap();
    push_json(
        &mut assembly,
        serde_json::json!({"reasoning_details":[{"index":0,"id":"one","text":"synthetic"}]}),
        None,
    )
    .unwrap();
    assert!(push_json(
        &mut assembly,
        serde_json::json!({"reasoning_details":[{"index":0,"id":"different","text":"synthetic"}]}),
        None
    )
    .is_err());
    let mut partial = minimax_continuation::Accumulator::new(
        "https://api.minimax.io/v1/",
        "test-model",
        uuid::Uuid::new_v4(),
    )
    .unwrap();
    push_json(
        &mut partial,
        serde_json::json!({"reasoning_content":"synthetic partial"}),
        None,
    )
    .unwrap();
    assert!(partial.finish(&[synthetic_call("call")]).is_err());
}

#[test]
fn profile_model_scope_compaction_delegate_and_fork_isolation_fail_before_wire_dispatch() {
    let (provider, profile, budget, request) = bound_fixture();
    assert!(provider
        .prepare_request(&profile, &budget, request.clone())
        .is_ok());
    let mut changed = request.clone();
    changed.continuation_scope = None; // tools-disabled compaction / absent owner scope
    changed.tools.clear();
    assert!(provider
        .prepare_request(&profile, &budget, changed)
        .is_err());
    for scope in [
        ContinuationScope {
            session_id: uuid::Uuid::new_v4(),
            run_id: request.continuation_scope.unwrap().run_id,
        },
        ContinuationScope {
            session_id: request.continuation_scope.unwrap().session_id,
            run_id: uuid::Uuid::new_v4(),
        },
    ] {
        let mut changed = request.clone();
        changed.continuation_scope = Some(scope); // fresh fork or child cannot inherit parent state
        assert!(provider
            .prepare_request(&profile, &budget, changed)
            .is_err());
    }
    let mut other_profile = profile.clone();
    other_profile.id = "different-profile".into();
    let mut other_budget = budget.clone();
    other_budget.profile_id = other_profile.id.clone();
    assert!(provider
        .prepare_request(&other_profile, &other_budget, request.clone())
        .is_err());
    let mut other_model = profile.clone();
    other_model.model = "other-model".into();
    let mut other_request = request.clone();
    other_request.model = other_model.model.clone();
    let mut other_budget = budget.clone();
    other_budget.model = other_model.model.clone();
    assert!(provider
        .prepare_request(&other_model, &other_budget, other_request)
        .is_err());
    let other = test_provider("https://api.minimax.io/v1/", None);
    assert!(other
        .prepare_request(&profile, &budget, request.clone())
        .is_err());
    let mut changed = request.clone();
    changed.tool_exchanges[0].calls[0].arguments = r#"{"path":"different.txt"}"#.into();
    assert!(provider
        .prepare_request(&profile, &budget, changed)
        .is_err());
    assert!(build_chat_request_inner(request, false, false, false).is_err());
}

#[test]
fn active_steered_history_keeps_full_chain_but_unbound_durable_history_does_not_replay_reasoning() {
    let (provider, profile, budget, mut request) = bound_fixture();
    let exchange = request.tool_exchanges.pop().unwrap();
    request.history.push(ModelMessage {
        role: ModelMessageRole::Assistant,
        content: String::new(),
        tool_exchange: Some(exchange),
    });
    let prepared = provider
        .prepare_request(&profile, &budget, request.clone())
        .unwrap();
    assert!(
        std::str::from_utf8(&provider.prepared_compatible_body(&prepared).unwrap())
            .unwrap()
            .contains("SYNTHETIC_PRIVATE_STATE")
    );
    request
        .history
        .last_mut()
        .unwrap()
        .tool_exchange
        .as_mut()
        .unwrap()
        .assistant_continuation = None;
    request.continuation_scope = Some(scope());
    let prepared = provider
        .prepare_request(&profile, &budget, request)
        .unwrap();
    let body = provider.prepared_compatible_body(&prepared).unwrap();
    assert!(!std::str::from_utf8(&body)
        .unwrap()
        .contains("SYNTHETIC_PRIVATE_STATE"));
    let body: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert!(body["messages"]
        .as_array()
        .unwrap()
        .iter()
        .all(|message| message.get("tool_calls").is_none()));
    assert!(body.to_string().contains("historical_observation"));
}

#[test]
fn exact_continuation_wire_bytes_participate_in_capacity_admission() {
    let (provider, profile, mut budget, request) = bound_fixture();
    let prepared = provider
        .prepare_request(&profile, &budget, request.clone())
        .unwrap();
    let bytes = provider.prepared_compatible_body(&prepared).unwrap().len() as u64;
    assert_eq!(prepared.accounting().accounted_input, bytes);
    budget.usable_context_tokens =
        bytes + u64::from(budget.requested_output_tokens) + budget.uncertainty_tokens;
    budget.input_limit_tokens = Some(bytes + budget.uncertainty_tokens);
    assert!(
        provider
            .prepare_request(&profile, &budget, request.clone())
            .unwrap()
            .accounting()
            .fits
    );
    budget.input_limit_tokens = Some(bytes + budget.uncertainty_tokens - 1);
    assert!(
        !provider
            .prepare_request(&profile, &budget, request)
            .unwrap()
            .accounting()
            .fits
    );
}

#[tokio::test]
async fn truncated_and_incomplete_streams_never_emit_completed_continuation() {
    for finish in [None, Some("length")] {
        let delta = serde_json::json!({"id":"synthetic-partial","choices":[{"delta":{"reasoning_details":[{"text":"SYNTHETIC_PARTIAL"}],"tool_calls":[{"index":0,"id":"call","type":"function","function":{"name":"read_text","arguments":"{\"path\":\"notes.txt\"}"}}]},"finish_reason":finish}]});
        let response = event_stream_response(&format!("data: {delta}\n\ndata: [DONE]\n\n"));
        let (endpoint, requests, worker) = spawn_mock_server_sequence(vec![response]);
        let mut provider = test_provider("https://api.minimax.io/v1/", None);
        provider.test_completion_endpoint = Some(Url::parse(&endpoint).unwrap());
        let (sender, mut receiver) = mpsc::channel(16);
        assert!(provider
            .stream(crate::tests::empty_model_request(), sender)
            .await
            .is_err());
        while let Ok(event) = receiver.try_recv() {
            assert!(!matches!(
                event,
                ProviderEvent::AssistantContinuation(_)
                    | ProviderEvent::ToolCall(_)
                    | ProviderEvent::Completed
            ));
        }
        requests.recv_timeout(Duration::from_secs(2)).unwrap();
        worker.join().unwrap();
    }
}

#[tokio::test]
async fn completed_state_stable_503_retry_keeps_identical_accounted_body() {
    let transient =
        "HTTP/1.1 503 Service Unavailable\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
            .to_owned();
    let final_response = event_stream_response("data: {\"choices\":[{\"delta\":{\"content\":\"done\"},\"finish_reason\":\"stop\"}]}\n\ndata: [DONE]\n\n");
    let (endpoint, requests, worker) = spawn_mock_server_sequence(vec![transient, final_response]);
    let (mut provider, profile, budget, request) = bound_fixture();
    provider.test_completion_endpoint = Some(Url::parse(&endpoint).unwrap());
    let prepared = provider
        .prepare_request(&profile, &budget, request)
        .unwrap();
    let validation = provider.validate_prepared_request(&prepared).await.unwrap();
    let (sender, mut receiver) = mpsc::channel(16);
    assert!(matches!(
        provider
            .stream_prepared_validated_with_cancellation(
                &prepared,
                &validation,
                sender,
                CancellationToken::new()
            )
            .await,
        Err(ProviderError::TransientHttpStatus { status: 503, .. })
    ));
    assert!(receiver.try_recv().is_err());
    let (sender, _receiver) = mpsc::channel(16);
    provider
        .stream_prepared_validated_with_cancellation(
            &prepared,
            &validation,
            sender,
            CancellationToken::new(),
        )
        .await
        .unwrap();
    let observed = requests.recv_timeout(Duration::from_secs(2)).unwrap();
    worker.join().unwrap();
    let bodies: Vec<_> = observed
        .iter()
        .map(|wire| wire.split_once("\r\n\r\n").unwrap().1)
        .collect();
    assert_eq!(bodies[0], bodies[1]);
    assert_eq!(
        bodies[0].len() as u64,
        prepared.accounting().accounted_input
    );
}

#[tokio::test]
async fn cancellation_after_a_partial_reasoning_frame_discards_continuation() {
    use std::io::{Read, Write};
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let endpoint = format!("http://{}/v1/", listener.local_addr().unwrap());
    let (release, wait) = std::sync::mpsc::channel();
    let worker = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(5)))
            .unwrap();
        let mut request = Vec::new();
        let mut buffer = [0_u8; 4096];
        while !request.windows(4).any(|window| window == b"\r\n\r\n") {
            let count = stream.read(&mut buffer).unwrap();
            assert_ne!(count, 0);
            request.extend_from_slice(&buffer[..count]);
            assert!(request.len() < 16 * 1024);
        }
        stream.write_all(concat!(
            "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nConnection: close\r\n\r\n",
            "data: {\"id\":\"synthetic-partial\",\"choices\":[{\"delta\":{\"reasoning_details\":[{\"text\":\"SYNTHETIC_PARTIAL\"}]},\"finish_reason\":null}]}\n\n"
        ).as_bytes()).unwrap();
        wait.recv_timeout(Duration::from_secs(5)).unwrap();
    });
    let mut provider = test_provider("https://api.minimax.io/v1/", None);
    provider.test_completion_endpoint = Some(Url::parse(&endpoint).unwrap());
    let cancel = CancellationToken::new();
    let cancelling = cancel.clone();
    let (sender, mut receiver) = mpsc::channel(16);
    let stream = tokio::spawn(async move {
        provider
            .stream_with_cancellation(crate::tests::empty_model_request(), sender, cancelling)
            .await
    });
    assert!(matches!(
        tokio::time::timeout(Duration::from_secs(3), receiver.recv())
            .await
            .unwrap(),
        Some(ProviderEvent::ReasoningDelta(_))
    ));
    cancel.cancel();
    assert!(matches!(
        stream.await.unwrap(),
        Err(ProviderError::RequestCancelled)
    ));
    while let Ok(event) = receiver.try_recv() {
        assert!(!matches!(
            event,
            ProviderEvent::AssistantContinuation(_)
                | ProviderEvent::ToolCall(_)
                | ProviderEvent::Completed
        ));
    }
    release.send(()).unwrap();
    worker.join().unwrap();
}

#[tokio::test]
async fn minimax_state_cannot_follow_a_redirect_to_another_origin() {
    let foreign = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    foreign.set_nonblocking(true).unwrap();
    let redirect = format!("HTTP/1.1 307 Temporary Redirect\r\nLocation: http://{}/outside\r\nContent-Length: 0\r\nConnection: close\r\n\r\n", foreign.local_addr().unwrap());
    let (endpoint, requests, worker) = spawn_mock_server_sequence(vec![redirect]);
    let (mut provider, profile, budget, request) = bound_fixture();
    provider.test_completion_endpoint = Some(Url::parse(&endpoint).unwrap());
    let prepared = provider
        .prepare_request(&profile, &budget, request)
        .unwrap();
    let validation = provider.validate_prepared_request(&prepared).await.unwrap();
    let (sender, _receiver) = mpsc::channel(16);
    assert!(matches!(
        provider
            .stream_prepared_validated_with_cancellation(
                &prepared,
                &validation,
                sender,
                CancellationToken::new()
            )
            .await,
        Err(ProviderError::ApiHttpStatus { status: 307, .. })
    ));
    assert_eq!(
        foreign.accept().unwrap_err().kind(),
        std::io::ErrorKind::WouldBlock
    );
    requests.recv_timeout(Duration::from_secs(2)).unwrap();
    worker.join().unwrap();
}

#[tokio::test]
async fn minimax_split_continuation_retains_documented_blocks_on_actual_next_http() {
    let response = event_stream_response(concat!(
        "data: {\"id\":\"synthetic-response-1\",\"choices\":[{\"delta\":{\"role\":\"assistant\",\"content\":\"synthetic visible\",\"reasoning_details\":[{\"type\":\"reasoning.text\",\"id\":\"synthetic-reasoning-1\",\"format\":\"MiniMax-response-v1\",\"index\":0,\"text\":\"SYNTHETIC_FIRST\"}]},\"finish_reason\":null}]}\n\n",
        "data: {\"id\":\"synthetic-response-1\",\"choices\":[{\"delta\":{\"reasoning_details\":[{\"type\":\"reasoning.text\",\"id\":\"synthetic-reasoning-2\",\"format\":\"MiniMax-response-v1\",\"index\":1,\"text\":\"SYNTHETIC_SECOND\"}],\"tool_calls\":[{\"index\":0,\"id\":\"synthetic-call-1\",\"type\":\"function\",\"function\":{\"name\":\"read_text\",\"arguments\":\"{\\\"path\\\":\\\"notes.txt\\\"}\"}}]},\"finish_reason\":\"tool_calls\"}]}\n\n",
        "data: [DONE]\n\n"
    ));
    let final_response = event_stream_response(concat!(
        "data: {\"choices\":[{\"delta\":{\"content\":\"synthetic done\"},\"finish_reason\":\"stop\"}]}\n\n",
        "data: [DONE]\n\n"
    ));
    let (endpoint, requests, worker) = spawn_mock_server_sequence(vec![response, final_response]);
    let mut transport =
        test_provider("https://api.minimax.io/v1/", None).with_hidden_tagged_reasoning(true);
    transport.test_completion_endpoint = Some(Url::parse(&endpoint).unwrap());
    let (mut profile, mut budget, mut request) =
        crate::tests::prepared_fixture("https://api.minimax.io/v1/");
    profile.id = "minimax".into();
    profile.model = "MiniMax-M2.7".into();
    transport.model = profile.model.clone();
    budget.profile_id = profile.id.clone();
    budget.model = profile.model.clone();
    request.model = profile.model.clone();
    let scope = ContinuationScope {
        session_id: uuid::Uuid::new_v4(),
        run_id: uuid::Uuid::new_v4(),
    };
    request.continuation_scope = Some(scope);
    let (sender, mut receiver) = mpsc::channel(16);
    transport
        .stream(request.clone(), sender)
        .await
        .expect("synthetic stream");
    let mut text = String::new();
    let mut reasoning = String::new();
    let mut calls = Vec::new();
    let mut continuation = None;
    while let Some(event) = receiver.recv().await {
        match event {
            ProviderEvent::Delta(delta) => text.push_str(&delta),
            ProviderEvent::ReasoningDelta(delta) => reasoning.push_str(&delta),
            ProviderEvent::ToolCall(call) => calls.push(call),
            ProviderEvent::AssistantContinuation(state) => {
                continuation = Some(Box::new(
                    (*state)
                        .bind_run(&profile, scope, uuid::Uuid::new_v4())
                        .unwrap(),
                ))
            }
            ProviderEvent::Completed => break,
            _ => {}
        }
    }
    request.tool_exchanges.push(ModelToolExchange {
        chatgpt_continuation: None,
        assistant_continuation: continuation,
        assistant_content: text,
        assistant_reasoning: reasoning,
        calls,
        results: vec![ModelToolResult {
            images: Default::default(),
            call_id: "synthetic-call-1".into(),
            content: "synthetic bounded result".into(),
        }],
    });
    let prepared = transport
        .prepare_request(&profile, &budget, request)
        .expect("canonical prepared request");
    let body = transport.prepared_compatible_body(&prepared).unwrap();
    assert_eq!(body.len() as u64, prepared.accounting().accounted_input);
    let validation = transport
        .validate_prepared_request(&prepared)
        .await
        .unwrap();
    let (sender, _receiver) = mpsc::channel(16);
    transport
        .stream_prepared_validated_with_cancellation(
            &prepared,
            &validation,
            sender,
            CancellationToken::new(),
        )
        .await
        .expect("synthetic next HTTP");
    let observed = requests
        .recv_timeout(Duration::from_secs(2))
        .expect("wire capture");
    worker.join().expect("synthetic server");
    assert_eq!(observed.len(), 2);
    let (_, next_body) = observed[1].split_once("\r\n\r\n").expect("HTTP body");
    assert_eq!(next_body.as_bytes(), body.as_ref());
    let next: serde_json::Value = serde_json::from_str(next_body).expect("captured JSON");
    let assistant = next["messages"]
        .as_array()
        .unwrap()
        .iter()
        .find(|message| message["role"] == "assistant")
        .unwrap();
    assert_eq!(next["reasoning_split"], true);
    assert_eq!(assistant["tool_calls"][0]["id"], "synthetic-call-1");
    assert_eq!(assistant["tool_calls"][0]["index"], 0);
    assert_eq!(
        assistant["reasoning_details"],
        serde_json::json!([
            {"type":"reasoning.text","id":"synthetic-reasoning-1","format":"MiniMax-response-v1","index":0,"text":"SYNTHETIC_FIRST"},
            {"type":"reasoning.text","id":"synthetic-reasoning-2","format":"MiniMax-response-v1","index":1,"text":"SYNTHETIC_SECOND"}
        ])
    );
    if let Some(path) = std::env::var_os("ARGENTUM_CONTINUITY_SYNTHETIC_EVIDENCE") {
        let evidence = serde_json::json!({
            "schema":"argentum.minimax.synthetic_continuation_evidence.v1",
            "continuation_preserved":true,
            "transport_note":"Credential-free loopback transport; canonical MiniMax wire builder uses reasoning_split=true. No hosted request.",
            "expected_reasoning_details":[
                {"type":"reasoning.text","id":"synthetic-reasoning-1","format":"MiniMax-response-v1","index":0,"text":"SYNTHETIC_FIRST"},
                {"type":"reasoning.text","id":"synthetic-reasoning-2","format":"MiniMax-response-v1","index":1,"text":"SYNTHETIC_SECOND"}
            ],
            "actual_serialized_next_request":next,
        });
        std::fs::write(path, serde_json::to_vec_pretty(&evidence).unwrap())
            .expect("synthetic-only evidence");
    }
}
