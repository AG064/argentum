// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 AG064

//! Measure how fast the running local model generates, on this computer.
//!
//! The owner starts a measurement with a button. It sends one fixed prompt to
//! the local model that is already running and approved, on 127.0.0.1, and
//! times the answer. Nothing leaves the machine. The result is only shown, and
//! the owner decides whether to share it (see the marketplace's speed report).

use std::sync::Arc;
use std::time::{Duration, Instant};

use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use crate::{ModelProvider, ModelRequest, ProviderError, ProviderEvent};

/// The fixed prompt, the same for every measurement so results compare.
pub const SPEED_PROMPT: &str =
    "Write a short paragraph about the history of the printing press. Use plain sentences.";
/// Output tokens asked for.
pub const SPEED_OUTPUT_TOKENS: u32 = 128;
const SPEED_TIMEOUT: Duration = Duration::from_secs(10 * 60);

/// One measurement.
#[derive(Debug, Clone, PartialEq)]
pub struct LocalSpeedMeasurement {
    pub model: String,
    pub context_tokens: u32,
    pub prompt_tokens: u64,
    pub output_tokens: u64,
    /// From the first generated text to the end of the answer.
    pub generation_seconds: f64,
    /// Output tokens per second of generation.
    pub tokens_per_second: f64,
}

/// Send the fixed prompt through `provider` and time the answer.
pub async fn measure_speed(
    provider: Arc<dyn ModelProvider>,
    model: &str,
    context_tokens: u32,
    cancellation: CancellationToken,
) -> Result<LocalSpeedMeasurement, ProviderError> {
    let request = ModelRequest {
        disable_thinking: true,
        require_tool_call: false,
        continuation_scope: None,
        output_limit: SPEED_OUTPUT_TOKENS,
        model: model.to_owned(),
        system: None,
        history: Vec::new(),
        prompt: SPEED_PROMPT.into(),
        tools: Vec::new(),
        tool_exchanges: Vec::new(),
    };
    let (sender, mut receiver) = mpsc::channel(64);
    let stream = tokio::spawn({
        let cancellation = cancellation.clone();
        async move {
            provider
                .stream_with_cancellation(request, sender, cancellation)
                .await
        }
    });
    let started = Instant::now();
    let mut first_output = None;
    let mut finished = None;
    let mut usage = None;
    let collect = async {
        while let Some(event) = receiver.recv().await {
            match event {
                ProviderEvent::Delta(_) | ProviderEvent::ReasoningDelta(_) => {
                    first_output.get_or_insert_with(Instant::now);
                }
                ProviderEvent::Usage(reported) => usage = Some(reported),
                ProviderEvent::Completed => finished = Some(Instant::now()),
                _ => {}
            }
        }
    };
    if tokio::time::timeout(SPEED_TIMEOUT, collect).await.is_err() {
        cancellation.cancel();
        return Err(ProviderError::InvalidResponse(
            "the speed measurement took longer than ten minutes".into(),
        ));
    }
    stream
        .await
        .map_err(|_| ProviderError::InvalidResponse("the speed measurement stopped".into()))??;
    let usage = usage
        .filter(|usage| usage.output_tokens > 0)
        .ok_or_else(|| {
            ProviderError::InvalidResponse("the local model did not report its token counts".into())
        })?;
    let end = finished.unwrap_or_else(Instant::now);
    let begin = first_output.filter(|first| *first < end).unwrap_or(started);
    let seconds = end.duration_since(begin).as_secs_f64().max(0.001);
    Ok(LocalSpeedMeasurement {
        model: model.to_owned(),
        context_tokens,
        prompt_tokens: usage.input_tokens,
        output_tokens: usage.output_tokens,
        generation_seconds: seconds,
        tokens_per_second: usage.output_tokens as f64 / seconds,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ModelUsage;
    use argentum_domain::{ProviderKind, ProviderStatus};
    use async_trait::async_trait;

    #[derive(Debug)]
    struct Timed {
        report_usage: bool,
    }

    #[async_trait]
    impl ModelProvider for Timed {
        fn id(&self) -> &'static str {
            "timed"
        }

        fn status(&self) -> ProviderStatus {
            ProviderStatus {
                profile_id: "timed".into(),
                kind: ProviderKind::OpenAiCompatible,
                label: "timed".into(),
                endpoint: String::new(),
                connected: true,
                detail: String::new(),
            }
        }

        async fn stream(
            &self,
            request: ModelRequest,
            sender: mpsc::Sender<ProviderEvent>,
        ) -> Result<(), ProviderError> {
            assert_eq!(request.prompt, SPEED_PROMPT);
            assert_eq!(request.output_limit, SPEED_OUTPUT_TOKENS);
            assert!(request.disable_thinking && request.tools.is_empty());
            let _ = sender.send(ProviderEvent::Delta("The".into())).await;
            tokio::time::sleep(Duration::from_millis(200)).await;
            let _ = sender.send(ProviderEvent::Delta(" press".into())).await;
            if self.report_usage {
                let _ = sender
                    .send(ProviderEvent::Usage(ModelUsage {
                        input_tokens: 20,
                        output_tokens: 40,
                        total_tokens: 60,
                        cached_input_tokens: None,
                        reasoning_tokens: None,
                        context_window_tokens: None,
                    }))
                    .await;
            }
            let _ = sender.send(ProviderEvent::Completed).await;
            Ok(())
        }
    }

    #[tokio::test]
    async fn speed_is_output_tokens_over_generation_time() {
        let measured = measure_speed(
            Arc::new(Timed { report_usage: true }),
            "Tiny-Q4_K_M.gguf",
            8192,
            CancellationToken::new(),
        )
        .await
        .expect("measured");
        assert_eq!(measured.output_tokens, 40);
        assert_eq!(measured.prompt_tokens, 20);
        assert!(measured.generation_seconds >= 0.19, "{measured:?}");
        assert!(
            measured.tokens_per_second > 50.0 && measured.tokens_per_second < 210.0,
            "{measured:?}"
        );
    }

    #[tokio::test]
    async fn a_model_without_token_counts_is_not_guessed() {
        let error = measure_speed(
            Arc::new(Timed {
                report_usage: false,
            }),
            "m.gguf",
            8192,
            CancellationToken::new(),
        )
        .await
        .expect_err("no usage");
        assert!(
            error.to_string().contains("token counts")
                || matches!(error, ProviderError::InvalidResponse(_))
        );
    }
}
