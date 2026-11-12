//! Local model consumers bind to an interchangeable backend. Artifact
//! preparation and activation authority remain owned by each implementation.
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};

use argentum_domain::{ProviderProfile, ProviderStatus};
use async_trait::async_trait;
use tokio::sync::{mpsc, Mutex, RwLock};
use tokio_util::sync::CancellationToken;

use crate::{
    AndroidLlamaCppProvider, LocalLlamaCppProvider, LocalRuntimeModule, ModelProvider,
    ModelRequest, ProviderError, ProviderEvent, ProviderModel,
};

#[async_trait]
pub trait LocalInferenceBackend: Send + Sync {
    fn id(&self) -> &'static str;
    /// Bind a profile without starting processes, downloading, or loading models.
    fn provider(&self, profile: &ProviderProfile) -> Result<Arc<dyn ModelProvider>, ProviderError>;
    /// Retire all owned resources; failure must retain cleanup ownership for retry.
    async fn shutdown(&self, cancellation: CancellationToken) -> Result<(), ProviderError>;
}

pub struct LlamaCppSidecarBackend(pub Arc<LocalRuntimeModule>);

#[async_trait]
impl LocalInferenceBackend for LlamaCppSidecarBackend {
    fn id(&self) -> &'static str {
        "llama.cpp.sidecar"
    }
    fn provider(&self, profile: &ProviderProfile) -> Result<Arc<dyn ModelProvider>, ProviderError> {
        Ok(Arc::new(LocalLlamaCppProvider::from_profile(
            profile,
            Some(self.0.clone()),
        )?))
    }
    async fn shutdown(&self, cancellation: CancellationToken) -> Result<(), ProviderError> {
        self.0.shutdown(cancellation).await
    }
}

pub struct AndroidLlamaCppBackend {
    pub runtime: Arc<Mutex<argentum_android_inference::AndroidInferenceModule>>,
    pub(crate) sidecar: Option<Arc<LocalRuntimeModule>>,
    pub(crate) foreground_stop: Arc<std::sync::RwLock<Option<AndroidForegroundStop>>>,
}

pub type AndroidForegroundStop = Arc<dyn Fn(u64) -> bool + Send + Sync>;

impl AndroidLlamaCppBackend {
    pub fn new(runtime: Arc<Mutex<argentum_android_inference::AndroidInferenceModule>>) -> Self {
        Self {
            runtime,
            sidecar: None,
            foreground_stop: Arc::default(),
        }
    }
}

#[async_trait]
impl LocalInferenceBackend for AndroidLlamaCppBackend {
    fn id(&self) -> &'static str {
        "llama.cpp.android"
    }
    fn provider(&self, profile: &ProviderProfile) -> Result<Arc<dyn ModelProvider>, ProviderError> {
        Ok(Arc::new(AndroidLlamaCppProvider::from_profile(
            profile,
            self.runtime.clone(),
        )?))
    }
    async fn shutdown(&self, cancellation: CancellationToken) -> Result<(), ProviderError> {
        if let Some(sidecar) = &self.sidecar {
            sidecar.shutdown(cancellation.clone()).await?;
        }
        if cancellation.is_cancelled() {
            return Err(ProviderError::RequestCancelled);
        }
        let stop = self
            .foreground_stop
            .read()
            .map_err(|_| ProviderError::LocalRuntimeLifecycleFailed)?
            .clone();
        let mut runtime = self.runtime.lock().await;
        let generation = runtime.foreground_generation();
        runtime
            .shutdown()
            .await
            .map_err(crate::android_local::map_inference_error)?;
        drop(runtime);
        if generation != 0 && stop.is_some_and(|stop| !stop(generation)) {
            return Err(ProviderError::LocalRuntimeLifecycleFailed);
        }
        Ok(())
    }
}

struct BackendState {
    backend: Option<Arc<dyn LocalInferenceBackend>>,
    generation: u64,
    ready: bool,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct LocalBackendIdentity {
    pub backend_id: Option<&'static str>,
    pub generation: Option<u64>,
}

#[derive(Clone)]
pub(crate) struct LocalBackendSlot(Arc<RwLock<BackendState>>, Arc<AtomicBool>);

#[derive(Debug, Clone, Copy)]
pub struct LocalBackendStatus {
    pub configured: bool,
    pub changing: bool,
    pub recovery_pending: bool,
}

/// Pins an admitted local activation against backend replacement.
#[must_use = "hold this lease through the entire preparation or activation"]
pub struct LocalBackendLease {
    _state: tokio::sync::OwnedRwLockReadGuard<BackendState>,
}

impl Default for LocalBackendSlot {
    fn default() -> Self {
        Self(
            Arc::new(RwLock::new(BackendState {
                backend: None,
                generation: 0,
                ready: true,
            })),
            Arc::new(AtomicBool::new(false)),
        )
    }
}

impl LocalBackendSlot {
    pub fn identity(&self) -> LocalBackendIdentity {
        let Ok(state) = self.0.try_read() else {
            return LocalBackendIdentity::default();
        };
        if !state.ready {
            return LocalBackendIdentity::default();
        }
        LocalBackendIdentity {
            backend_id: state.backend.as_ref().map(|backend| backend.id()),
            generation: state.backend.as_ref().map(|_| state.generation),
        }
    }
    pub fn new(backend: Arc<dyn LocalInferenceBackend>) -> Self {
        Self(
            Arc::new(RwLock::new(BackendState {
                backend: Some(backend),
                generation: 0,
                ready: true,
            })),
            Arc::new(AtomicBool::new(true)),
        )
    }

    pub fn configured(&self) -> bool {
        self.1.load(Ordering::Acquire)
    }

    pub fn status(&self) -> LocalBackendStatus {
        match self.0.try_read() {
            Ok(state) => LocalBackendStatus {
                configured: state.backend.is_some(),
                changing: false,
                recovery_pending: !state.ready,
            },
            Err(_) => LocalBackendStatus {
                configured: self.configured(),
                changing: true,
                recovery_pending: false,
            },
        }
    }

    pub async fn lease(&self, backend_id: &str) -> Result<LocalBackendLease, ProviderError> {
        let state = self.0.clone().read_owned().await;
        if !state.ready || !state.backend.as_ref().is_some_and(|b| b.id() == backend_id) {
            return Err(ProviderError::LocalRuntimeLeaseStale);
        }
        Ok(LocalBackendLease { _state: state })
    }

    pub fn bind(&self, profile: &ProviderProfile) -> Result<Arc<dyn ModelProvider>, ProviderError> {
        let state = self
            .0
            .try_read()
            .map_err(|_| ProviderError::LocalBackendBusy)?;
        if !state.ready {
            return Err(ProviderError::LocalRuntimeLifecycleFailed);
        }
        let Some(backend) = &state.backend else {
            return Ok(Arc::new(LocalLlamaCppProvider::from_profile(
                profile, None,
            )?));
        };
        Ok(Arc::new(BoundLocalProvider {
            slot: self.clone(),
            generation: state.generation,
            provider: backend.provider(profile)?,
            unavailable_status: ProviderStatus {
                profile_id: profile.id.clone(),
                kind: profile.kind,
                label: profile.label.clone(),
                endpoint: profile.endpoint.clone(),
                connected: false,
                detail: "Local backend is changing, disabled, or replaced".into(),
            },
        }))
    }

    pub async fn replace(
        &self,
        replacement: Option<Arc<dyn LocalInferenceBackend>>,
        cancellation: CancellationToken,
    ) -> Result<(), ProviderError> {
        let mut state = tokio::select! {
            biased;
            _ = cancellation.cancelled() => return Err(ProviderError::RequestCancelled),
            state = self.0.write() => state,
        };
        if cancellation.is_cancelled() {
            return Err(ProviderError::RequestCancelled);
        }
        state.generation = state
            .generation
            .checked_add(1)
            .ok_or(ProviderError::LocalRuntimeLifecycleFailed)?;
        state.ready = false;
        if let Some(previous) = &state.backend {
            // Retain the old backend on error so a retry can finish cleanup.
            previous.shutdown(cancellation.clone()).await?;
        }
        state.backend = None;
        self.1.store(false, Ordering::Release);
        state.ready = true;
        if cancellation.is_cancelled() {
            return Err(ProviderError::RequestCancelled);
        }
        self.1.store(replacement.is_some(), Ordering::Release);
        state.backend = replacement;
        Ok(())
    }

    pub async fn shutdown(&self, cancellation: CancellationToken) -> Result<(), ProviderError> {
        if self
            .0
            .try_read()
            .is_ok_and(|state| state.backend.is_none() && state.ready)
        {
            return Ok(());
        }
        self.replace(None, cancellation).await
    }
}

struct BoundLocalProvider {
    slot: LocalBackendSlot,
    generation: u64,
    provider: Arc<dyn ModelProvider>,
    unavailable_status: ProviderStatus,
}

impl BoundLocalProvider {
    fn check(&self, state: &BackendState) -> Result<(), ProviderError> {
        if !state.ready || state.generation != self.generation {
            return Err(ProviderError::LocalRuntimeLeaseStale);
        }
        Ok(())
    }
}

#[async_trait]
impl ModelProvider for BoundLocalProvider {
    fn id(&self) -> &'static str {
        self.provider.id()
    }
    fn status(&self) -> ProviderStatus {
        let Ok(state) = self.slot.0.try_read() else {
            return self.unavailable_status.clone();
        };
        if self.check(&state).is_err() {
            return self.unavailable_status.clone();
        }
        self.provider.status()
    }
    async fn probe(&self) -> Result<ProviderStatus, ProviderError> {
        let state = self.slot.0.read().await;
        self.check(&state)?;
        self.provider.probe().await
    }
    async fn list_models(&self) -> Result<Vec<ProviderModel>, ProviderError> {
        let state = self.slot.0.read().await;
        self.check(&state)?;
        self.provider.list_models().await
    }
    async fn validate_request(
        &self,
        request: &ModelRequest,
    ) -> Result<crate::ProviderRequestValidation, ProviderError> {
        let state = self.slot.0.read().await;
        self.check(&state)?;
        self.provider.validate_request(request).await
    }
    async fn capability_runtime_evidence(&self) -> Result<Option<String>, ProviderError> {
        let state = self.slot.0.read().await;
        self.check(&state)?;
        self.provider.capability_runtime_evidence().await
    }
    async fn stream(
        &self,
        request: ModelRequest,
        sender: mpsc::Sender<ProviderEvent>,
    ) -> Result<(), ProviderError> {
        self.stream_with_cancellation(request, sender, CancellationToken::new())
            .await
    }
    async fn stream_with_cancellation(
        &self,
        request: ModelRequest,
        sender: mpsc::Sender<ProviderEvent>,
        cancellation: CancellationToken,
    ) -> Result<(), ProviderError> {
        let state = tokio::select! {
            biased;
            _ = cancellation.cancelled() => return Err(ProviderError::RequestCancelled),
            state = self.slot.0.read() => state,
        };
        self.check(&state)?;
        self.provider
            .stream_with_cancellation(request, sender, cancellation)
            .await
    }
    async fn stream_validated_with_cancellation(
        &self,
        request: ModelRequest,
        expected: &crate::ProviderRequestValidation,
        sender: mpsc::Sender<ProviderEvent>,
        cancellation: CancellationToken,
    ) -> Result<(), ProviderError> {
        let state = tokio::select! {
            biased;
            _ = cancellation.cancelled() => return Err(ProviderError::RequestCancelled),
            state = self.slot.0.read() => state,
        };
        self.check(&state)?;
        self.provider
            .stream_validated_with_cancellation(request, expected, sender, cancellation)
            .await
    }
}
