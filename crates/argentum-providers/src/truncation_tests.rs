//! Synthetic terminal frames, without real providers or credentials.

use super::*;
use crate::tests::{
    empty_model_request, event_stream_response, spawn_mock_server_sequence, test_provider,
};
use serde_json::{json, Value};

async fn truncated_events(
    usage: Option<Value>,
    minimax: bool,
    done: bool,
) -> (ProviderError, Vec<ProviderEvent>) {
    let terminal = json!({"id":"synthetic-truncated", "choices":[{
        "delta":{"content":"partial answer", "tool_calls":[{
            "index":0,"id":"partial","type":"function",
            "function":{"name":"edit_text","arguments":"{\"path\":"}
        }]}, "finish_reason":"length"
    }]});
    let mut body = format!("data: {terminal}\n\n");
    if let Some(usage) = usage {
        body.push_str(&format!(
            "data: {}\n\n",
            json!({"choices":[],"usage":usage})
        ));
    }
    if done {
        body.push_str("data: [DONE]\n\n");
    }
    collect_events(body, minimax).await
}

async fn collect_events(body: String, minimax: bool) -> (ProviderError, Vec<ProviderEvent>) {
    let (endpoint, requests, worker) =
        spawn_mock_server_sequence(vec![event_stream_response(&body)]);
    let mut provider = test_provider(
        if minimax {
            "https://api.minimax.io/v1/"
        } else {
            &endpoint
        },
        None,
    );
    if minimax {
        provider.test_completion_endpoint = Some(Url::parse(&endpoint).unwrap());
    }
    let (sender, mut receiver) = mpsc::channel(16);
    let error = provider
        .stream(empty_model_request(), sender)
        .await
        .unwrap_err();
    let mut events = Vec::new();
    while let Some(event) = receiver.recv().await {
        assert!(!matches!(
            event,
            ProviderEvent::ToolCall(_)
                | ProviderEvent::Completed
                | ProviderEvent::AssistantContinuation(_)
        ));
        events.push(event);
    }
    requests.recv_timeout(Duration::from_secs(2)).unwrap();
    worker.join().unwrap();
    (error, events)
}

#[tokio::test]
async fn truncation_reads_later_terminal_usage_without_emitting_partial_calls() {
    for minimax in [false, true] {
        for done in [false, true] {
            let (error, events) = truncated_events(
                Some(json!({
                    "prompt_tokens":12,"completion_tokens":4096,"total_tokens":4108
                })),
                minimax,
                done,
            )
            .await;
            assert!(matches!(error, ProviderError::ResponseTruncated));
            let observed: Vec<_> = events
                .iter()
                .filter_map(|event| match event {
                    ProviderEvent::UsageObservation(usage) => Some(usage),
                    _ => None,
                })
                .collect();
            assert_eq!(observed.len(), 1, "terminal usage was lost");
            assert_eq!(observed[0].total_tokens, 4108);
            let complete: Vec<_> = events
                .iter()
                .filter_map(|event| match event {
                    ProviderEvent::Usage(usage) => Some(usage),
                    _ => None,
                })
                .collect();
            assert_eq!(complete.len(), 1);
            assert_eq!(complete[0].output_tokens, 4096);
        }
    }
}

#[tokio::test]
async fn truncation_preserves_total_only_usage_without_inventing_components() {
    let (error, events) = truncated_events(Some(json!({"total_tokens":4108})), true, true).await;
    assert!(matches!(error, ProviderError::ResponseTruncated));
    let observed = events
        .iter()
        .find_map(|event| match event {
            ProviderEvent::UsageObservation(usage) => Some(usage),
            _ => None,
        })
        .expect("total-only terminal usage was lost");
    assert_eq!(observed.total_tokens, 4108);
    assert_eq!(observed.input_tokens, None);
    assert_eq!(observed.output_tokens, None);
    assert!(!events
        .iter()
        .any(|event| matches!(event, ProviderEvent::Usage(_))));
}

#[tokio::test]
async fn truncation_without_usage_remains_explicitly_unavailable() {
    let (error, events) = truncated_events(None, true, true).await;
    assert!(matches!(error, ProviderError::ResponseTruncated));
    assert!(!events.iter().any(|event| matches!(
        event,
        ProviderEvent::Usage(_) | ProviderEvent::UsageObservation(_)
    )));
}

#[tokio::test]
async fn truncation_duplicate_usage_is_not_recharged_and_conflicts_are_uncertain() {
    for conflicting in [false, true] {
        let terminal = json!({"choices":[{"delta":{},"finish_reason":"length"}]});
        let first = json!({"choices":[],"usage":{"prompt_tokens":12,
            "completion_tokens":4096,"total_tokens":4108}});
        let last = if conflicting {
            json!({"choices":[],"usage":{"total_tokens":4109}})
        } else {
            first.clone()
        };
        let (error, events) = collect_events(
            format!("data: {terminal}\n\ndata: {first}\n\ndata: {last}\n\ndata: [DONE]\n\n"),
            true,
        )
        .await;
        assert_eq!(
            events
                .iter()
                .filter(|event| matches!(event, ProviderEvent::UsageObservation(_)))
                .count(),
            1
        );
        if conflicting {
            assert!(matches!(error, ProviderError::InvalidResponse(_)));
            assert!(events
                .iter()
                .any(|event| matches!(event, ProviderEvent::UsageUncertain)));
            assert!(!events
                .iter()
                .any(|event| matches!(event, ProviderEvent::Usage(_))));
        } else {
            assert!(matches!(error, ProviderError::ResponseTruncated));
            assert_eq!(
                events
                    .iter()
                    .filter(|event| matches!(event, ProviderEvent::Usage(_)))
                    .count(),
                1
            );
        }
    }
}

#[tokio::test]
async fn truncation_rejects_post_terminal_completion_and_bounds_usage_drain() {
    let terminal = json!({"choices":[{"delta":{},"finish_reason":"length"}]});
    let resumed = json!({"choices":[{"delta":{"content":"not a new answer"},
        "finish_reason":"stop"}]});
    let (error, _) = collect_events(
        format!("data: {terminal}\n\ndata: {resumed}\n\ndata: [DONE]\n\n"),
        false,
    )
    .await;
    assert!(error
        .to_string()
        .contains("completion data after truncation"));
    let mut body = format!("data: {terminal}\n\n");
    for _ in 0..=MAX_TRUNCATED_USAGE_FRAMES {
        body.push_str("data: {\"choices\":[]}\n\n");
    }
    body.push_str("data: [DONE]\n\n");
    let (error, _) = collect_events(body, false).await;
    assert!(error.to_string().contains("terminal usage frames exceeded"));
}
