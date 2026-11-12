use std::time::Duration;

use argentum_providers::{ProviderError, ProviderRetryClass};
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

pub(crate) const MAX_ATTEMPTS: u8 = 3;
pub(crate) const MAX_TOTAL_WAIT: Duration = Duration::from_secs(15);
const INITIAL_DELAY: Duration = Duration::from_millis(500);
const MAX_DELAY: Duration = Duration::from_secs(5);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum WaitOutcome {
    Ready,
    Cancelled,
    Shutdown,
    Deadline,
}

pub(crate) fn class(error: &ProviderError) -> Option<ProviderRetryClass> {
    error.retry_class()
}

pub(crate) fn error_class(error: &ProviderError) -> Option<&'static str> {
    if matches!(error, ProviderError::ApiHttpStatus { .. }) {
        return Some("http_status");
    }
    match error.retry_class()? {
        ProviderRetryClass::PreDispatchConnection => Some("pre_dispatch_connection"),
        ProviderRetryClass::TransientHttpStatus { .. } => Some("transient_http_status"),
    }
}

pub(crate) fn status(error: &ProviderError) -> Option<u16> {
    if let ProviderError::ApiHttpStatus { status, .. } = error {
        return Some(*status);
    }
    match error.retry_class()? {
        ProviderRetryClass::PreDispatchConnection => None,
        ProviderRetryClass::TransientHttpStatus { status, .. } => Some(status),
    }
}

pub(crate) fn planned_delay(
    run_id: Uuid,
    retry_number: u8,
    retry_after_ms: Option<u64>,
    waited: Duration,
) -> Option<Duration> {
    if retry_number == 0 {
        return None;
    }
    let exponent = u32::from(retry_number.saturating_sub(1)).min(4);
    let base_ms = INITIAL_DELAY.as_millis().saturating_mul(1_u128 << exponent);
    let jitter_ms = (run_id.as_u128() ^ u128::from(retry_number)) % 101;
    let fallback_ms = base_ms.saturating_add(jitter_ms).min(MAX_DELAY.as_millis());
    let requested_ms = retry_after_ms.map(u128::from).unwrap_or(0);
    let delay_ms = fallback_ms.max(requested_ms);
    let delay = Duration::from_millis(u64::try_from(delay_ms).ok()?);
    if delay > MAX_DELAY || waited.checked_add(delay)? > MAX_TOTAL_WAIT {
        None
    } else {
        Some(delay)
    }
}

pub(crate) async fn wait(
    delay: Duration,
    cancellation: &CancellationToken,
    shutdown: &CancellationToken,
    deadline: Option<tokio::time::Instant>,
) -> WaitOutcome {
    let sleep = tokio::time::sleep(delay);
    tokio::pin!(sleep);
    tokio::select! {
        biased;
        _ = cancellation.cancelled() => WaitOutcome::Cancelled,
        _ = shutdown.cancelled() => WaitOutcome::Shutdown,
        _ = async {
            if let Some(deadline) = deadline {
                tokio::time::sleep_until(deadline).await;
            } else {
                std::future::pending::<()>().await;
            }
        } => WaitOutcome::Deadline,
        _ = &mut sleep => WaitOutcome::Ready,
    }
}

pub(crate) fn stop_reason(error: &ProviderError) -> &'static str {
    match error {
        ProviderError::PreDispatchConnection | ProviderError::TransientHttpStatus { .. } => {
            "retry_exhausted_or_wait_bound"
        }
        _ => "non_retryable_error",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bounded_backoff_is_deterministic_and_finite() {
        let run = Uuid::from_u128(7);
        let first = planned_delay(run, 1, None, Duration::ZERO).unwrap();
        assert!(first >= INITIAL_DELAY && first <= MAX_DELAY);
        assert!(planned_delay(run, 1, Some(5_001), Duration::ZERO).is_none());
        assert!(planned_delay(run, 1, Some(5_000), Duration::ZERO).is_some());
        assert!(planned_delay(run, 2, None, Duration::from_secs(15)).is_none());
        assert_eq!(Some(first), planned_delay(run, 1, None, Duration::ZERO));
    }

    #[test]
    fn non_retryable_http_status_keeps_safe_terminal_metadata() {
        let error = ProviderError::ApiHttpStatus {
            status: 400,
            safe_detail: String::new(),
        };
        assert_eq!(error_class(&error), Some("http_status"));
        assert_eq!(status(&error), Some(400));
        assert!(class(&error).is_none());
    }

    #[tokio::test]
    async fn cancellation_interrupts_backoff_before_the_deadline() {
        let cancellation = CancellationToken::new();
        cancellation.cancel();
        let outcome = wait(
            Duration::from_secs(5),
            &cancellation,
            &CancellationToken::new(),
            None,
        )
        .await;
        assert_eq!(outcome, WaitOutcome::Cancelled);
    }
}
