// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 AG064

//! The first, free step of compaction: shorten old tool output.
//!
//! When a request is close to the context window, the output of tool calls
//! from earlier turns is cut to its first lines before anything else happens.
//! This needs no model call and changes nothing that is stored: the full
//! output stays in the session's records, and the agent can run the tool
//! again for its current content. The newest exchanges and this run's own
//! tool results are never shortened. A request that does not fit at all goes
//! to the model summary as before.

use argentum_domain::context_budget::RequestAccounting;
use argentum_providers::ModelRequest;
use serde_json::json;

/// Share of the usable context, in percent, at which old tool output starts
/// to be shortened, before the window is full.
pub(super) const TRIM_START_PERCENT: u64 = 85;
/// Earlier exchanges, newest first, whose output is kept whole.
pub(super) const KEEP_WHOLE_EXCHANGES: usize = 2;
/// Output longer than this is shortened.
pub(super) const TRIM_ABOVE_BYTES: usize = 1_024;
/// How much of a shortened output is kept, at most.
pub(super) const KEPT_HEAD_BYTES: usize = 512;

/// True when the request fills at least [`TRIM_START_PERCENT`] of the window.
pub(super) fn near_full(accounting: &RequestAccounting) -> bool {
    accounting.usable_context > 0
        && accounting.accounted_input.saturating_mul(100)
            >= accounting.usable_context.saturating_mul(TRIM_START_PERCENT)
}

/// The request with old tool output shortened, or `None` when nothing in it
/// is old and long enough to shorten.
pub(super) fn trim_old_tool_output(request: &ModelRequest) -> Option<(ModelRequest, usize)> {
    let mut trimmed = request.clone();
    let exchange_positions = trimmed
        .history
        .iter()
        .enumerate()
        .filter(|(_, message)| message.tool_exchange.is_some())
        .map(|(index, _)| index)
        .collect::<Vec<_>>();
    let old = exchange_positions
        .len()
        .saturating_sub(KEEP_WHOLE_EXCHANGES);
    let mut shortened = 0;
    for index in exchange_positions.into_iter().take(old) {
        let Some(exchange) = trimmed.history[index].tool_exchange.as_mut() else {
            continue;
        };
        for result in &mut exchange.results {
            if result.content.len() > TRIM_ABOVE_BYTES {
                result.content = shortened_output(&result.content);
                shortened += 1;
            }
        }
    }
    (shortened > 0).then_some((trimmed, shortened))
}

/// A placeholder that keeps the start of the output and says what happened.
fn shortened_output(content: &str) -> String {
    let mut end = KEPT_HEAD_BYTES.min(content.len());
    while !content.is_char_boundary(end) {
        end -= 1;
    }
    json!({
        "historical_placeholder": true,
        "shortened": true,
        "original_bytes": content.len(),
        "head": &content[..end],
        "notice": "Earlier tool output, shortened to keep the context small. It is not new output, and nothing ran again. Run the tool again if you need its current content."
    })
    .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use argentum_providers::{
        ModelMessage, ModelMessageRole, ModelToolCall, ModelToolExchange, ModelToolResult,
    };

    fn exchange(output: &str) -> ModelMessage {
        ModelMessage {
            role: ModelMessageRole::Assistant,
            content: String::new(),
            tool_exchange: Some(ModelToolExchange {
                chatgpt_continuation: None,
                assistant_continuation: None,
                assistant_content: String::new(),
                assistant_reasoning: String::new(),
                calls: vec![ModelToolCall {
                    id: "call".into(),
                    name: "read_text".into(),
                    arguments: "{}".into(),
                }],
                results: vec![ModelToolResult {
                    images: Default::default(),
                    call_id: "call".into(),
                    content: output.into(),
                }],
            }),
        }
    }

    fn request(history: Vec<ModelMessage>) -> ModelRequest {
        ModelRequest {
            prompt_images: Vec::new(),
            disable_thinking: false,
            require_tool_call: false,
            continuation_scope: None,
            output_limit: 1_000,
            model: "model".into(),
            system: None,
            history,
            prompt: "now".into(),
            tools: Vec::new(),
            tool_exchanges: Vec::new(),
        }
    }

    #[test]
    fn only_old_long_output_is_shortened() {
        let long = "é".repeat(2_000);
        let original = request(vec![
            exchange(&long),
            exchange("short"),
            exchange(&long),
            exchange(&long),
        ]);
        let (trimmed, count) = trim_old_tool_output(&original).expect("something to trim");
        assert_eq!(
            count, 1,
            "the two newest exchanges stay whole, short output too"
        );
        let output = |index: usize| {
            trimmed.history[index]
                .tool_exchange
                .as_ref()
                .unwrap()
                .results[0]
                .content
                .clone()
        };
        let first: serde_json::Value = serde_json::from_str(&output(0)).unwrap();
        assert_eq!(first["shortened"], true);
        assert_eq!(first["original_bytes"], long.len());
        assert!(first["head"].as_str().unwrap().len() <= KEPT_HEAD_BYTES);
        assert_eq!(output(1), "short");
        assert_eq!(output(2), long);
        assert_eq!(output(3), long);
        assert_eq!(trimmed.prompt, original.prompt);
        assert!(trim_old_tool_output(&request(vec![exchange(&long)])).is_none());
    }

    #[test]
    fn trimming_starts_before_the_window_is_full() {
        let mut accounting = RequestAccounting {
            method: "estimated".into(),
            capacity_source: "owner".into(),
            budget_revision: uuid::Uuid::nil(),
            profile_id: "p".into(),
            model: "m".into(),
            provider_endpoint: "https://example.test/".into(),
            provider_kind: argentum_domain::ProviderKind::OpenAiCompatible,
            accounted_input: 0,
            serialized_message_bytes: 0,
            message_components: Default::default(),
            serialized_catalog_bytes: 0,
            serialized_envelope_bytes: 0,
            output_reserve: 0,
            uncertainty_allowance: 0,
            usable_context: 1_000,
            separate_input_limit: None,
            maximum_output: 0,
            bundled_model_context_limit: None,
            profile_catalog_context_limit: None,
            profile_catalog_source: None,
            fits: true,
        };
        accounting.accounted_input = 849;
        assert!(!near_full(&accounting));
        accounting.accounted_input = 850;
        assert!(near_full(&accounting));
        accounting.usable_context = 0;
        assert!(!near_full(&accounting));
    }
}
