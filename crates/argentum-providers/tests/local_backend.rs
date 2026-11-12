use argentum_domain::{ProviderProfile, ProviderStatus};
use argentum_providers::{
    LocalInferenceBackend, ModelProvider, ModelRequest, ProviderError, ProviderEvent,
    ProviderModel, ProviderRegistry,
};
use async_trait::async_trait;
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};
use tokio::sync::{mpsc, Notify};
use tokio_util::sync::CancellationToken;

struct Backend {
    name: &'static str,
    shutdowns: Arc<AtomicUsize>,
    fail_first: bool,
    release: Option<Arc<Notify>>,
}

struct Provider {
    name: &'static str,
    release: Option<Arc<Notify>>,
    shutdowns: Option<Arc<AtomicUsize>>,
}

#[async_trait]
impl ModelProvider for Provider {
    fn id(&self) -> &'static str {
        self.name
    }
    fn status(&self) -> ProviderStatus {
        if let Some(shutdowns) = &self.shutdowns {
            assert_eq!(
                shutdowns.load(Ordering::SeqCst),
                0,
                "retired provider status was invoked"
            );
        }
        ProviderStatus {
            profile_id: "llama-cpp".into(),
            kind: argentum_domain::ProviderKind::LocalLlamaCpp,
            label: self.name.into(),
            endpoint: "local://llama-cpp/".into(),
            connected: true,
            detail: self.name.into(),
        }
    }
    async fn list_models(&self) -> Result<Vec<ProviderModel>, ProviderError> {
        Ok(Vec::new())
    }
    async fn stream(
        &self,
        _: ModelRequest,
        sender: mpsc::Sender<ProviderEvent>,
    ) -> Result<(), ProviderError> {
        sender
            .send(ProviderEvent::Delta(self.name.into()))
            .await
            .map_err(|_| ProviderError::ConsumerClosed)?;
        if let Some(release) = &self.release {
            release.notified().await;
        }
        sender
            .send(ProviderEvent::Completed)
            .await
            .map_err(|_| ProviderError::ConsumerClosed)
    }
}

#[async_trait]
impl LocalInferenceBackend for Backend {
    fn id(&self) -> &'static str {
        self.name
    }
    fn provider(&self, _: &ProviderProfile) -> Result<Arc<dyn ModelProvider>, ProviderError> {
        Ok(Arc::new(Provider {
            name: self.name,
            release: self.release.clone(),
            shutdowns: Some(self.shutdowns.clone()),
        }))
    }
    async fn shutdown(&self, _: CancellationToken) -> Result<(), ProviderError> {
        let prior = self.shutdowns.fetch_add(1, Ordering::SeqCst);
        if self.fail_first && prior == 0 {
            return Err(ProviderError::LocalRuntimeLifecycleFailed);
        }
        Ok(())
    }
}

fn backend(
    name: &'static str,
    calls: Arc<AtomicUsize>,
    fail_first: bool,
) -> Arc<dyn LocalInferenceBackend> {
    Arc::new(Backend {
        name,
        shutdowns: calls,
        fail_first,
        release: None,
    })
}

struct CancellingBackend;

#[async_trait]
impl LocalInferenceBackend for CancellingBackend {
    fn id(&self) -> &'static str {
        "cancel-during-cleanup"
    }
    fn provider(&self, _: &ProviderProfile) -> Result<Arc<dyn ModelProvider>, ProviderError> {
        Ok(Arc::new(Provider {
            name: "old",
            release: None,
            shutdowns: None,
        }))
    }
    async fn shutdown(&self, cancellation: CancellationToken) -> Result<(), ProviderError> {
        cancellation.cancel();
        Ok(())
    }
}

#[tokio::test]
async fn cancellation_during_completed_cleanup_does_not_publish_replacement() {
    let registry = ProviderRegistry::default().with_local_backend(Arc::new(CancellingBackend));
    let result = registry
        .replace_local_backend(
            Some(backend("new", Arc::default(), false)),
            CancellationToken::new(),
        )
        .await;
    assert!(matches!(result, Err(ProviderError::RequestCancelled)));
    assert!(!registry.local_runtime_configured());
}

#[tokio::test]
async fn registry_replaces_backend_and_revokes_existing_provider_handles() {
    let calls = Arc::new(AtomicUsize::new(0));
    let registry =
        ProviderRegistry::default().with_local_backend(backend("first", calls.clone(), false));
    let clone = registry.clone();
    let profile = ProviderProfile::default_llama_cpp();
    let old = registry.provider_for_profile(&profile).unwrap();
    assert_eq!(old.status().detail, "first");
    registry
        .replace_local_backend(
            Some(backend("second", Arc::default(), false)),
            CancellationToken::new(),
        )
        .await
        .unwrap();
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    assert!(old.list_models().await.is_err());
    assert!(!old.status().connected);
    assert_eq!(
        clone
            .provider_for_profile(&profile)
            .unwrap()
            .status()
            .detail,
        "second"
    );
    registry
        .replace_local_backend(None, CancellationToken::new())
        .await
        .unwrap();
    assert!(!clone.local_runtime_configured());
    assert!(clone
        .provider_for_profile(&profile)
        .unwrap()
        .list_models()
        .await
        .is_err());
}

#[tokio::test]
async fn failed_cleanup_prevents_replacement_and_can_be_retried() {
    let calls = Arc::new(AtomicUsize::new(0));
    let registry =
        ProviderRegistry::default().with_local_backend(backend("old", calls.clone(), true));
    let profile = ProviderProfile::default_llama_cpp();
    let old = registry.provider_for_profile(&profile).unwrap();
    assert!(registry
        .replace_local_backend(
            Some(backend("new", Arc::default(), false)),
            CancellationToken::new()
        )
        .await
        .is_err());
    assert!(old.list_models().await.is_err());
    assert!(registry.provider_for_profile(&profile).is_err());
    registry
        .replace_local_backend(
            Some(backend("new", Arc::default(), false)),
            CancellationToken::new(),
        )
        .await
        .unwrap();
    assert_eq!(calls.load(Ordering::SeqCst), 2);
    assert_eq!(
        registry
            .provider_for_profile(&profile)
            .unwrap()
            .status()
            .detail,
        "new"
    );
}

#[tokio::test]
async fn active_stream_drains_before_replacement_and_cancelled_wait_keeps_old_backend() {
    let calls = Arc::new(AtomicUsize::new(0));
    let release = Arc::new(Notify::new());
    let registry = ProviderRegistry::default().with_local_backend(Arc::new(Backend {
        name: "old",
        shutdowns: calls.clone(),
        fail_first: false,
        release: Some(release.clone()),
    }));
    let provider = registry
        .provider_for_profile(&ProviderProfile::default_llama_cpp())
        .unwrap();
    let (tx, mut rx) = mpsc::channel(4);
    let stream = tokio::spawn(async move {
        provider
            .stream(
                ModelRequest {
                    require_tool_call: false,
                    continuation_scope: None,
                    output_limit: 4096,
                    model: "test".into(),
                    system: None,
                    history: vec![],
                    prompt: "test".into(),
                    tools: vec![],
                    tool_exchanges: vec![],
                },
                tx,
            )
            .await
    });
    assert!(matches!(rx.recv().await, Some(ProviderEvent::Delta(_))));
    let cancelled = CancellationToken::new();
    cancelled.cancel();
    assert!(registry
        .replace_local_backend(None, cancelled)
        .await
        .is_err());
    assert_eq!(calls.load(Ordering::SeqCst), 0);
    let mut replacing = Box::pin(registry.replace_local_backend(None, CancellationToken::new()));
    assert!(
        tokio::time::timeout(std::time::Duration::from_millis(20), &mut replacing)
            .await
            .is_err()
    );
    assert_eq!(calls.load(Ordering::SeqCst), 0);
    assert!(registry.local_runtime_configured());
    assert!(registry.local_backend_status().changing);
    release.notify_one();
    stream.await.unwrap().unwrap();
    replacing.await.unwrap();
    assert_eq!(calls.load(Ordering::SeqCst), 1);
}
