use std::collections::VecDeque;
use std::sync::{
    atomic::{AtomicBool, AtomicU64, Ordering},
    Arc, Mutex,
};

use argentum_domain::{now, DiagnosticRecord, DiagnosticSnapshot, RunId};
use tracing::{Event, Subscriber};
use tracing_subscriber::{layer::Context, prelude::*, Layer};

const CAPACITY: usize = 256;
const SNAPSHOT_LIMIT: usize = 128;

#[derive(Default)]
struct State {
    records: VecDeque<DiagnosticRecord>,
    recorded: u64,
    evicted: u64,
}

#[derive(Default)]
struct Inner {
    enabled: AtomicBool,
    generation: AtomicU64,
    dropped: AtomicU64,
    degraded: AtomicBool,
    state: Mutex<State>,
}

#[derive(Clone, Default)]
pub(super) struct DiagnosticCapture(Arc<Inner>);

impl DiagnosticCapture {
    pub fn enabled(&self) -> bool {
        self.0.enabled.load(Ordering::Acquire)
    }

    pub fn enable(&self) {
        let mut state = self
            .0
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        *state = State::default();
        self.0.state.clear_poison();
        self.0.generation.fetch_add(1, Ordering::AcqRel);
        self.0.dropped.store(0, Ordering::Release);
        self.0.degraded.store(false, Ordering::Release);
        self.0.enabled.store(true, Ordering::Release);
    }

    pub fn disable(&self) {
        self.0.enabled.store(false, Ordering::Release);
        self.0.generation.fetch_add(1, Ordering::AcqRel);
        let mut state = self
            .0
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        *state = State::default();
    }

    pub fn record(
        &self,
        code: &'static str,
        component: &str,
        line: Option<u32>,
        level: &str,
        run_id: Option<RunId>,
    ) {
        if !self.enabled() {
            return;
        }
        let generation = self.0.generation.load(Ordering::Acquire);
        if code.len() > 96
            || !matches!(level, "TRACE" | "DEBUG" | "INFO" | "WARN" | "ERROR")
            || component.len() > 96
            || !component
                .bytes()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, b'_' | b':' | b'.' | b'-'))
        {
            return;
        }
        let Ok(mut state) = self.0.state.try_lock() else {
            self.0.dropped.fetch_add(1, Ordering::Relaxed);
            self.0.degraded.store(true, Ordering::Release);
            return;
        };
        if !self.enabled() || generation != self.0.generation.load(Ordering::Acquire) {
            return;
        }
        state.recorded = state.recorded.saturating_add(1);
        let sequence = state.recorded;
        if state.records.len() == CAPACITY {
            state.records.pop_front();
            state.evicted = state.evicted.saturating_add(1);
        }
        state.records.push_back(DiagnosticRecord {
            sequence,
            occurred_at: now(),
            code: code.into(),
            component: component.into(),
            line,
            level: level.into(),
            run_id,
        });
    }

    pub fn snapshot(&self) -> Option<DiagnosticSnapshot> {
        if !self.enabled() {
            return None;
        }
        let state = self
            .0
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if !self.enabled() {
            return None;
        }
        Some(DiagnosticSnapshot {
            records: state
                .records
                .iter()
                .skip(state.records.len().saturating_sub(SNAPSHOT_LIMIT))
                .cloned()
                .collect(),
            recorded: state.recorded,
            evicted: state.evicted,
            dropped: self.0.dropped.load(Ordering::Acquire),
            degraded: self.0.degraded.load(Ordering::Acquire) || self.0.state.is_poisoned(),
        })
    }

    pub fn dispatcher(&self) -> tracing::Dispatch {
        tracing::Dispatch::new(tracing_subscriber::registry().with(MetadataLayer(self.clone())))
    }
}

struct MetadataLayer(DiagnosticCapture);

impl<S: Subscriber> Layer<S> for MetadataLayer {
    fn on_event(&self, event: &Event<'_>, _: Context<'_, S>) {
        let metadata = event.metadata();
        if metadata.target().starts_with("argentum") {
            // Deliberately never visit fields, messages, errors, or debug values.
            self.0.record(
                "trace",
                metadata.target(),
                metadata.line(),
                metadata.level().as_str(),
                None,
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn contention_is_nonblocking_and_reported() {
        let capture = DiagnosticCapture::default();
        capture.enable();
        let guard = capture.0.state.lock().unwrap();
        capture.record("start", "argentum.runtime", None, "INFO", None);
        drop(guard);
        let snapshot = capture.snapshot().unwrap();
        assert_eq!(snapshot.dropped, 1);
        assert!(snapshot.degraded);
        assert!(snapshot.records.is_empty());
    }

    #[tokio::test]
    async fn runtime_module_owns_capture_and_remove_clears_it() {
        use argentum_domain::{AppCommand, ModelDiagnosticPhase, ModuleAction};
        let (_dir, runtime) = super::super::tests::composed_runtime().await;
        assert!(runtime.harness_snapshot().unwrap().diagnostics.is_none());
        runtime
            .dispatch(AppCommand::SetHarnessModule {
                module_id: "first-party.diagnostics".into(),
                action: ModuleAction::Enable,
            })
            .await
            .unwrap();
        runtime.record_model_phase(ModelDiagnosticPhase::Preparing);
        let snapshot = runtime.harness_snapshot().unwrap().diagnostics.unwrap();
        assert!(snapshot.recorded > 0);
        runtime
            .dispatch(AppCommand::SetHarnessModule {
                module_id: "first-party.diagnostics".into(),
                action: ModuleAction::Remove,
            })
            .await
            .unwrap();
        runtime.record_model_phase(ModelDiagnosticPhase::Preparing);
        assert!(runtime.harness_snapshot().unwrap().diagnostics.is_none());
        runtime.shutdown().await.unwrap();
    }

    #[tokio::test]
    async fn restart_restores_enablement_but_not_capture_records() {
        use super::super::modules::{RuntimeModules, DIAGNOSTICS};
        use argentum_domain::ModuleAction;
        use argentum_store::Store;
        let store = Store::open_in_memory().unwrap();
        let project = uuid::Uuid::new_v4();
        let mut first = RuntimeModules::restore(&store, project).await.unwrap();
        first
            .change(&store, project, DIAGNOSTICS, ModuleAction::Enable)
            .await
            .unwrap();
        first
            .diagnostics
            .record("start", "argentum.runtime", None, "INFO", None);
        first
            .host
            .shutdown(&tokio_util::sync::CancellationToken::new())
            .await
            .unwrap();
        assert!(first.diagnostics.snapshot().is_none());
        let mut second = RuntimeModules::restore(&store, project).await.unwrap();
        assert!(second.diagnostics.snapshot().unwrap().records.is_empty());
        second
            .host
            .shutdown(&tokio_util::sync::CancellationToken::new())
            .await
            .unwrap();
    }

    #[test]
    fn capture_is_optional_bounded_and_cleared_on_disable() {
        let capture = DiagnosticCapture::default();
        capture.record("start", "argentum.runtime", None, "INFO", None);
        assert!(capture.snapshot().is_none());
        capture.enable();
        for _ in 0..300 {
            capture.record("start", "argentum.runtime", None, "INFO", None);
        }
        let snapshot = capture.snapshot().unwrap();
        assert_eq!(snapshot.recorded, 300);
        assert_eq!(snapshot.evicted, 44);
        assert_eq!(snapshot.records.len(), 128);
        assert_eq!(snapshot.records[0].sequence, 173);
        capture.disable();
        assert!(capture.snapshot().is_none());
        capture.enable();
        assert!(capture.snapshot().unwrap().records.is_empty());
    }

    #[test]
    fn tracing_never_copies_sensitive_fields_or_messages_and_keeps_scopes_separate() {
        let first = DiagnosticCapture::default();
        let second = DiagnosticCapture::default();
        first.enable();
        second.enable();
        tracing::dispatcher::with_default(&first.dispatcher(), || {
            tracing::error!(
                credential = "sensitive-canary-credential",
                "sensitive-canary-prompt"
            );
            tracing::info!(target: "external_library", "excluded");
        });
        let snapshot = first.snapshot().unwrap();
        assert_eq!(snapshot.records.len(), 1);
        let json = serde_json::to_string(&snapshot).unwrap();
        assert!(!json.contains("sensitive-canary"));
        assert!(second.snapshot().unwrap().records.is_empty());
        first.disable();
        tracing::dispatcher::with_default(&first.dispatcher(), || tracing::warn!("after disable"));
        assert!(first.snapshot().is_none());
    }
}
