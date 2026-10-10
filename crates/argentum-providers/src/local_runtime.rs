use std::any::Any;
use std::path::{Component, Path, PathBuf};
use std::sync::{Arc, Mutex as StdMutex};
use std::time::Duration;

use argentum_domain::{ProviderKind, ProviderProfile, Sha256Digest};
#[cfg(windows)]
use argentum_execution::{ensure_owner_only_directory, ensure_owner_only_file};
use argentum_execution::{
    ManagedProcess, ManagedProcessLimits, ModelFileLease, PrivateFile, ProcessError, ProcessSpec,
    VerifiedFileLease,
};
use argentum_lifecycle::{
    EffectDisposer, EffectFailure, LifecycleKernel, LifecycleSnapshot, ModuleSpec, MountFailure,
    MountFuture, MountedModule,
};
use argentum_platform::{PlatformAuthorityBroker, PlatformAuthorityLease};
use argentum_security::{
    AuthorityGeneration, LocalAuthorityAction, SecretValue, SecurityError, VerifiedLocalAuthority,
};
use async_trait::async_trait;
use sha2::{Digest, Sha256};
use tokio::sync::Mutex;
use tokio_util::sync::CancellationToken;

use crate::runtime_bundles::{
    extract_runtime_session_leased, open_runtime_archive_lease, open_runtime_launcher_lease,
};
use crate::{local_runtime_offers, ModelProvider, OpenAiCompatibleProvider, ProviderError};

const LOCAL_RUNTIME_MODULE_ID: &str = "first-party.local-runtime";
const LOCAL_RUNTIME_EFFECT_LABEL: &str = "process:llama-cpp-sidecar";
const LOCAL_RUNTIME_HOST: &str = "127.0.0.1";
const LOCAL_RUNTIME_CONTEXT: u32 = crate::SIDECAR_CONTEXT_TOKENS;
const LOCAL_RUNTIME_GPU_LAYERS: u32 = 99;
const LOCAL_RUNTIME_MAX_PREDICTION: i32 = -1;
const LOCAL_RUNTIME_READY_TIMEOUT: Duration = Duration::from_secs(5 * 60);
const LOCAL_RUNTIME_MIN_MODEL_BYTES: u64 = 1024 * 1024;
const LOCAL_RUNTIME_DEFAULT_MODEL_BYTES: u64 = 64 * 1024 * 1024 * 1024;
const LOCAL_RUNTIME_MAX_MODEL_BYTES: u64 = 256 * 1024 * 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LocalRuntimeLimits {
    pub max_model_bytes: u64,
}

impl Default for LocalRuntimeLimits {
    fn default() -> Self {
        Self {
            max_model_bytes: LOCAL_RUNTIME_DEFAULT_MODEL_BYTES,
        }
    }
}

impl LocalRuntimeLimits {
    fn validate(self) -> Result<Self, ProviderError> {
        if !(LOCAL_RUNTIME_MIN_MODEL_BYTES..=LOCAL_RUNTIME_MAX_MODEL_BYTES)
            .contains(&self.max_model_bytes)
        {
            return Err(ProviderError::LocalRuntimeModelInvalid);
        }
        Ok(self)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocalRuntimeConfig {
    pub bundle_path: PathBuf,
    pub models_dir: PathBuf,
    pub limits: LocalRuntimeLimits,
    pub context_tokens: u32,
}

impl LocalRuntimeConfig {
    pub fn new(
        bundle_path: impl Into<PathBuf>,
        models_dir: impl Into<PathBuf>,
    ) -> Result<Self, ProviderError> {
        let config = Self {
            bundle_path: bundle_path.into(),
            models_dir: models_dir.into(),
            limits: LocalRuntimeLimits::default(),
            context_tokens: LOCAL_RUNTIME_CONTEXT,
        };
        if !config.bundle_path.is_absolute()
            || !config.models_dir.is_absolute()
            || config.bundle_path.as_os_str().to_string_lossy().len() > 2_048
            || config.models_dir.as_os_str().to_string_lossy().len() > 2_048
        {
            return Err(ProviderError::LocalRuntimeBundleInvalid);
        }
        Ok(config)
    }

    pub fn with_max_model_bytes(mut self, max_model_bytes: u64) -> Result<Self, ProviderError> {
        self.limits = LocalRuntimeLimits { max_model_bytes }.validate()?;
        Ok(self)
    }

    pub fn with_context_tokens(mut self, context_tokens: u32) -> Result<Self, ProviderError> {
        if !crate::SIDECAR_CONTEXT_VARIANTS.contains(&context_tokens) {
            return Err(ProviderError::ContextBudgetInvalid(
                "unsupported local context variant",
            ));
        }
        self.context_tokens = context_tokens;
        Ok(self)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocalRuntimeLaunchPlan {
    pub runtime_id: String,
    pub runtime_version: String,
    pub runtime_bundle_sha256: String,
    pub executable_sha256: String,
    pub program: PathBuf,
    pub arguments: Vec<String>,
    pub environment_sha256: Sha256Digest,
    pub model: String,
    pub model_path: PathBuf,
    pub model_sha256: String,
    pub model_size_bytes: u64,
    pub max_model_bytes: u64,
    pub managed_active_process_limit: u32,
    pub managed_cpu_rate_percent: u32,
    pub managed_process_memory_bytes: u64,
    pub managed_job_memory_bytes: u64,
    pub managed_output_hash_limit: usize,
    pub generation: u64,
    pub epoch: String,
    pub host: String,
    pub port: u16,
    pub api_prefix: String,
    pub context_tokens: u32,
    pub gpu_layers: u32,
    pub disabled_features: Vec<String>,
}

impl LocalRuntimeLaunchPlan {
    pub fn digest(&self) -> Sha256Digest {
        let mut hasher = Sha256::new();
        for value in [
            self.runtime_id.as_str(),
            self.runtime_version.as_str(),
            self.runtime_bundle_sha256.as_str(),
            self.executable_sha256.as_str(),
            &self.program.to_string_lossy(),
            self.environment_sha256.0.as_str(),
            self.model.as_str(),
            &self.model_path.to_string_lossy(),
            self.model_sha256.as_str(),
            self.epoch.as_str(),
            self.host.as_str(),
            self.api_prefix.as_str(),
        ] {
            hasher.update(value.as_bytes());
            hasher.update([0]);
        }
        for argument in &self.arguments {
            hasher.update(argument.as_bytes());
            hasher.update([0xff]);
        }
        for feature in &self.disabled_features {
            hasher.update(feature.as_bytes());
            hasher.update([0xfe]);
        }
        hasher.update(self.model_size_bytes.to_le_bytes());
        hasher.update(self.max_model_bytes.to_le_bytes());
        hasher.update(self.managed_active_process_limit.to_le_bytes());
        hasher.update(self.managed_cpu_rate_percent.to_le_bytes());
        hasher.update(self.managed_process_memory_bytes.to_le_bytes());
        hasher.update(self.managed_job_memory_bytes.to_le_bytes());
        hasher.update((self.managed_output_hash_limit as u64).to_le_bytes());
        hasher.update(self.generation.to_le_bytes());
        hasher.update(self.port.to_le_bytes());
        hasher.update(self.context_tokens.to_le_bytes());
        hasher.update(self.gpu_layers.to_le_bytes());
        Sha256Digest(format!("{:x}", hasher.finalize()))
    }

    #[cfg(any(test, feature = "test-support"))]
    fn test_fixture() -> Self {
        Self {
            runtime_id: "llama-cpp".into(),
            runtime_version: "v0.3.0".into(),
            runtime_bundle_sha256: "a".repeat(64),
            executable_sha256: "b".repeat(64),
            program: PathBuf::from(r"C:\runtime\llama-server.exe"),
            arguments: vec!["--offline".into()],
            environment_sha256: Sha256Digest("c".repeat(64)),
            model: "model.gguf".into(),
            model_path: PathBuf::from(r"C:\models\model.gguf"),
            model_sha256: "d".repeat(64),
            model_size_bytes: 4,
            max_model_bytes: LOCAL_RUNTIME_DEFAULT_MODEL_BYTES,
            managed_active_process_limit: 4,
            managed_cpu_rate_percent: 95,
            managed_process_memory_bytes: 4 * 1024 * 1024 * 1024,
            managed_job_memory_bytes: 4 * 1024 * 1024 * 1024,
            managed_output_hash_limit: 4 * 1024 * 1024,
            generation: 1,
            epoch: "epoch".into(),
            host: LOCAL_RUNTIME_HOST.into(),
            port: 43_123,
            api_prefix: "/argentum/generation-1/epoch".into(),
            context_tokens: LOCAL_RUNTIME_CONTEXT,
            gpu_layers: LOCAL_RUNTIME_GPU_LAYERS,
            disabled_features: disabled_runtime_features(),
        }
    }
}

#[cfg(feature = "test-support")]
#[derive(Clone)]
pub struct LocalRuntimeTestControl {
    inner: Arc<LocalRuntimeTestControlInner>,
}

#[cfg(feature = "test-support")]
struct LocalRuntimeTestControlInner {
    requests: std::sync::Mutex<Vec<crate::ModelRequest>>,
    launcher_calls: std::sync::atomic::AtomicUsize,
    listener_calls: std::sync::atomic::AtomicUsize,
    network_calls: std::sync::atomic::AtomicUsize,
    provider_calls: std::sync::atomic::AtomicUsize,
    launches: std::sync::atomic::AtomicUsize,
    stop_attempts: std::sync::atomic::AtomicUsize,
    fail_next_stop: std::sync::atomic::AtomicBool,
    lifecycle_events: Option<Arc<std::sync::Mutex<Vec<&'static str>>>>,
}

#[cfg(feature = "test-support")]
impl LocalRuntimeTestControl {
    pub fn captured_requests(&self) -> Vec<crate::ModelRequest> {
        self.inner.requests.lock().unwrap().clone()
    }
    pub fn launcher_call_count(&self) -> usize {
        self.inner
            .launcher_calls
            .load(std::sync::atomic::Ordering::SeqCst)
    }

    pub fn listener_call_count(&self) -> usize {
        self.inner
            .listener_calls
            .load(std::sync::atomic::Ordering::SeqCst)
    }

    pub fn network_call_count(&self) -> usize {
        self.inner
            .network_calls
            .load(std::sync::atomic::Ordering::SeqCst)
    }

    pub fn provider_call_count(&self) -> usize {
        self.inner
            .provider_calls
            .load(std::sync::atomic::Ordering::SeqCst)
    }

    pub fn launch_count(&self) -> usize {
        self.inner
            .launches
            .load(std::sync::atomic::Ordering::SeqCst)
    }

    pub fn stop_attempt_count(&self) -> usize {
        self.inner
            .stop_attempts
            .load(std::sync::atomic::Ordering::SeqCst)
    }

    pub fn fail_next_stop(&self) {
        self.inner
            .fail_next_stop
            .store(true, std::sync::atomic::Ordering::SeqCst);
    }
}

#[cfg(feature = "test-support")]
struct TestSupportPreparedRuntime {
    listener: std::net::TcpListener,
}

#[cfg(feature = "test-support")]
struct TestSupportLauncher {
    control: LocalRuntimeTestControl,
}

#[cfg(feature = "test-support")]
struct TestSupportProvider {
    control: LocalRuntimeTestControl,
}

#[cfg(feature = "test-support")]
#[async_trait]
impl ModelProvider for TestSupportProvider {
    fn id(&self) -> &'static str {
        "local-runtime-test-support"
    }

    fn status(&self) -> argentum_domain::ProviderStatus {
        argentum_domain::ProviderStatus {
            profile_id: "local-runtime-test-support".into(),
            kind: ProviderKind::LocalLlamaCpp,
            label: "local runtime test support".into(),
            endpoint: "local://test-support/".into(),
            connected: true,
            detail: "ready".into(),
        }
    }

    async fn probe(&self) -> Result<argentum_domain::ProviderStatus, ProviderError> {
        self.control
            .inner
            .provider_calls
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        self.control
            .inner
            .network_calls
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        Ok(self.status())
    }

    async fn stream(
        &self,
        request: crate::ModelRequest,
        sender: tokio::sync::mpsc::Sender<crate::ProviderEvent>,
    ) -> Result<(), ProviderError> {
        {
            let mut requests = self.control.inner.requests.lock().unwrap();
            if requests.len() == 4 {
                requests.remove(0);
            }
            requests.push(request);
        }
        self.control
            .inner
            .provider_calls
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        self.control
            .inner
            .network_calls
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        sender
            .send(crate::ProviderEvent::Completed)
            .await
            .map_err(|_| ProviderError::ConsumerClosed)
    }
}

#[cfg(feature = "test-support")]
struct TestSupportLeaseVerifier {
    authority: LocalRuntimeAuthority,
    pid: u32,
    process_started_at: u64,
    listener_port: u16,
    model_sha256: String,
    active: Arc<std::sync::atomic::AtomicBool>,
}

#[cfg(feature = "test-support")]
#[async_trait]
impl LeaseVerifier for TestSupportLeaseVerifier {
    async fn verify(&self, lease: &SidecarLease) -> Result<(), ProviderError> {
        if !self.authority.is_current()
            || !self
                .authority
                .lease()
                .matches_generation(&lease.authority_generation)
        {
            return Err(ProviderError::RequestCancelled);
        }
        if lease.authority_generation != *self.authority.generation()
            || lease.pid != self.pid
            || lease.process_started_at != self.process_started_at
            || lease.model_sha256 != self.model_sha256
            || self.listener_port == 0
            || !self.active.load(std::sync::atomic::Ordering::SeqCst)
        {
            return Err(ProviderError::LocalRuntimeLeaseStale);
        }
        Ok(())
    }
}

#[cfg(feature = "test-support")]
struct TestSupportRuntimeInstance {
    listener: Option<std::net::TcpListener>,
    active: Arc<std::sync::atomic::AtomicBool>,
    control: LocalRuntimeTestControl,
}

#[cfg(feature = "test-support")]
#[async_trait]
impl RuntimeInstance for TestSupportRuntimeInstance {
    async fn stop(&mut self) -> Result<(), ProviderError> {
        self.control
            .inner
            .stop_attempts
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        if self
            .control
            .inner
            .fail_next_stop
            .swap(false, std::sync::atomic::Ordering::SeqCst)
        {
            return Err(ProviderError::LocalRuntimeLaunchFailed {
                stage: "test-support-teardown",
            });
        }
        self.active
            .store(false, std::sync::atomic::Ordering::SeqCst);
        drop(self.listener.take());
        if let Some(events) = &self.control.inner.lifecycle_events {
            events
                .lock()
                .map_err(|_| ProviderError::LocalRuntimeLifecycleFailed)?
                .push("provider.local-runtime");
        }
        Ok(())
    }
}

#[cfg(feature = "test-support")]
#[async_trait]
impl LocalRuntimeLauncher for TestSupportLauncher {
    async fn prepare(
        &self,
        config: &LocalRuntimeConfig,
        profile: &ProviderProfile,
        generation: u64,
    ) -> Result<PreparedLocalRuntime, ProviderError> {
        self.control
            .inner
            .launcher_calls
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        validate_local_profile(profile)?;
        self.control
            .inner
            .listener_calls
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let listener = std::net::TcpListener::bind((LOCAL_RUNTIME_HOST, 0)).map_err(|_| {
            ProviderError::LocalRuntimeLaunchFailed {
                stage: "test-support-prepare",
            }
        })?;
        let port = listener
            .local_addr()
            .map_err(|_| ProviderError::LocalRuntimeLaunchFailed {
                stage: "test-support-prepare",
            })?
            .port();
        let mut plan = LocalRuntimeLaunchPlan::test_fixture();
        plan.context_tokens = config.context_tokens;
        plan.model = profile.model.clone();
        plan.model_path = PathBuf::from(format!(r"C:\models\{}", profile.model));
        plan.generation = generation;
        plan.epoch = format!("test-support-{generation}");
        plan.port = port;
        plan.api_prefix = format!("/argentum/generation-{generation}/test-support");
        Ok(PreparedLocalRuntime {
            plan,
            resources: Some(Box::new(TestSupportPreparedRuntime { listener })),
        })
    }

    async fn launch(
        &self,
        mut prepared: PreparedLocalRuntime,
        authority: LocalRuntimeAuthority,
        cancellation: &CancellationToken,
    ) -> Result<LaunchedRuntime, ProviderError> {
        self.control
            .inner
            .launcher_calls
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        if cancellation.is_cancelled() {
            return Err(ProviderError::LocalRuntimeCancelled);
        }
        authority.ensure_activation()?;
        let resources = prepared
            .resources
            .take()
            .ok_or(ProviderError::LocalRuntimeApprovalStale)?
            .downcast::<TestSupportPreparedRuntime>()
            .map_err(|_| ProviderError::LocalRuntimeApprovalStale)?;
        let pid = std::process::id();
        let process_started_at = prepared.plan.generation;
        let listener_port = resources
            .listener
            .local_addr()
            .map_err(|_| ProviderError::LocalRuntimeLeaseStale)?
            .port();
        let active = Arc::new(std::sync::atomic::AtomicBool::new(true));
        let provider = Arc::new(TestSupportProvider {
            control: self.control.clone(),
        });
        provider.probe().await?;
        authority.ensure_activation()?;
        let lease = Arc::new(SidecarLease {
            epoch: prepared.plan.epoch,
            generation: prepared.plan.generation,
            authority_generation: authority.generation().clone(),
            model_sha256: prepared.plan.model_sha256.clone(),
            pid,
            process_started_at,
        });
        let verifier = Arc::new(TestSupportLeaseVerifier {
            authority,
            pid,
            process_started_at,
            listener_port,
            model_sha256: prepared.plan.model_sha256,
            active: Arc::clone(&active),
        });
        self.control
            .inner
            .launches
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        Ok(LaunchedRuntime {
            client: Arc::new(LocalRuntimeClient {
                context_tokens: prepared.plan.context_tokens,
                model: prepared.plan.model,
                provider,
                lease,
                verifier,
            }),
            instance: Box::new(TestSupportRuntimeInstance {
                listener: Some(resources.listener),
                active,
                control: self.control.clone(),
            }),
        })
    }
}

#[cfg(feature = "test-support")]
pub fn local_runtime_test_fixture(
    config: LocalRuntimeConfig,
    authority_owner: Arc<PlatformAuthorityBroker>,
) -> (Arc<LocalRuntimeModule>, LocalRuntimeTestControl) {
    local_runtime_test_fixture_with_events(config, authority_owner, None)
}

#[cfg(feature = "test-support")]
pub fn local_runtime_recording_test_fixture(
    config: LocalRuntimeConfig,
    authority_owner: Arc<PlatformAuthorityBroker>,
    lifecycle_events: Arc<std::sync::Mutex<Vec<&'static str>>>,
) -> (Arc<LocalRuntimeModule>, LocalRuntimeTestControl) {
    local_runtime_test_fixture_with_events(config, authority_owner, Some(lifecycle_events))
}

#[cfg(feature = "test-support")]
fn local_runtime_test_fixture_with_events(
    config: LocalRuntimeConfig,
    authority_owner: Arc<PlatformAuthorityBroker>,
    lifecycle_events: Option<Arc<std::sync::Mutex<Vec<&'static str>>>>,
) -> (Arc<LocalRuntimeModule>, LocalRuntimeTestControl) {
    let control = LocalRuntimeTestControl {
        inner: Arc::new(LocalRuntimeTestControlInner {
            requests: std::sync::Mutex::new(Vec::new()),
            launcher_calls: std::sync::atomic::AtomicUsize::new(0),
            listener_calls: std::sync::atomic::AtomicUsize::new(0),
            network_calls: std::sync::atomic::AtomicUsize::new(0),
            provider_calls: std::sync::atomic::AtomicUsize::new(0),
            launches: std::sync::atomic::AtomicUsize::new(0),
            stop_attempts: std::sync::atomic::AtomicUsize::new(0),
            fail_next_stop: std::sync::atomic::AtomicBool::new(false),
            lifecycle_events,
        }),
    };
    let module = Arc::new(LocalRuntimeModule::with_launcher(
        config,
        authority_owner,
        Arc::new(TestSupportLauncher {
            control: control.clone(),
        }),
    ));
    (module, control)
}

pub struct LocalRuntimeLaunchApproval {
    plan_digest: Sha256Digest,
    authority: VerifiedLocalAuthority,
}

impl LocalRuntimeLaunchApproval {
    pub fn bind(
        plan: &LocalRuntimeLaunchPlan,
        authority: VerifiedLocalAuthority,
    ) -> Result<Self, ProviderError> {
        let plan_digest = plan.digest();
        if authority.action()
            != &(LocalAuthorityAction::ActivateLocalRuntime {
                plan: plan_digest.clone(),
            })
        {
            return Err(ProviderError::LocalRuntimeApprovalDenied);
        }
        Ok(Self {
            plan_digest,
            authority,
        })
    }

    pub fn matches(&self, plan: &LocalRuntimeLaunchPlan) -> bool {
        self.plan_digest == plan.digest()
    }

    fn authorize(
        self,
        plan: &LocalRuntimeLaunchPlan,
        generation: &AuthorityGeneration,
    ) -> Result<(), ProviderError> {
        if !self.matches(plan) {
            return Err(ProviderError::LocalRuntimeApprovalStale);
        }
        let action = LocalAuthorityAction::ActivateLocalRuntime {
            plan: plan.digest(),
        };
        self.authority
            .authorize(&action, generation)
            .map_err(|error| match error {
                SecurityError::LocalAuthorityExpired
                | SecurityError::LocalAuthorityGenerationMismatch => {
                    ProviderError::LocalRuntimeApprovalStale
                }
                _ => ProviderError::LocalRuntimeApprovalDenied,
            })
    }
}

impl std::fmt::Debug for LocalRuntimeLaunchApproval {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("LocalRuntimeLaunchApproval")
            .field("plan_digest", &self.plan_digest)
            .field("authority", &self.authority)
            .finish()
    }
}

#[derive(Clone)]
struct LocalRuntimeAuthority {
    owner: Arc<PlatformAuthorityBroker>,
    generation: AuthorityGeneration,
    lease: Arc<PlatformAuthorityLease>,
}

impl LocalRuntimeAuthority {
    fn new(
        owner: Arc<PlatformAuthorityBroker>,
        generation: AuthorityGeneration,
        lease: Arc<PlatformAuthorityLease>,
    ) -> Self {
        Self {
            owner,
            generation,
            lease,
        }
    }

    fn generation(&self) -> &AuthorityGeneration {
        &self.generation
    }

    fn lease(&self) -> &Arc<PlatformAuthorityLease> {
        &self.lease
    }

    fn is_current(&self) -> bool {
        self.owner
            .matches_current_authority(&self.generation, &self.lease)
    }

    fn ensure_activation(&self) -> Result<(), ProviderError> {
        self.is_current()
            .then_some(())
            .ok_or(ProviderError::LocalRuntimeApprovalStale)
    }
}

pub struct SidecarLease {
    epoch: String,
    generation: u64,
    authority_generation: AuthorityGeneration,
    model_sha256: String,
    pid: u32,
    process_started_at: u64,
}

impl SidecarLease {
    fn fingerprint(&self) -> String {
        let mut hash = Sha256::new();
        hash.update(b"argentum.sidecar-lease.tools.v1\0");
        hash.update(self.epoch.as_bytes());
        hash.update(self.generation.to_le_bytes());
        hash.update(self.authority_generation.ordinal().to_le_bytes());
        hash.update(self.model_sha256.as_bytes());
        hash.update(self.pid.to_le_bytes());
        hash.update(self.process_started_at.to_le_bytes());
        format!("{:x}", hash.finalize())
    }
    pub fn epoch(&self) -> &str {
        &self.epoch
    }

    pub const fn generation(&self) -> u64 {
        self.generation
    }

    pub const fn authority_generation_ordinal(&self) -> u64 {
        self.authority_generation.ordinal()
    }

    pub fn model_sha256(&self) -> &str {
        &self.model_sha256
    }

    pub const fn pid(&self) -> u32 {
        self.pid
    }

    pub const fn process_started_at(&self) -> u64 {
        self.process_started_at
    }
}

impl std::fmt::Debug for SidecarLease {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("SidecarLease")
            .field("authority_generation", &self.authority_generation.ordinal())
            .field("runtime_generation", &self.generation)
            .field("model_sha256", &self.model_sha256)
            .field("pid", &self.pid)
            .field("process_started_at", &self.process_started_at)
            .finish()
    }
}

#[async_trait]
trait LeaseVerifier: Send + Sync {
    async fn verify(&self, lease: &SidecarLease) -> Result<(), ProviderError>;
}

pub struct LocalRuntimeClient {
    context_tokens: u32,
    model: String,
    provider: Arc<dyn ModelProvider>,
    lease: Arc<SidecarLease>,
    verifier: Arc<dyn LeaseVerifier>,
}

impl std::fmt::Debug for LocalRuntimeClient {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("LocalRuntimeClient")
            .field("model", &self.model)
            .field("lease", &self.lease)
            .finish_non_exhaustive()
    }
}

impl LocalRuntimeClient {
    pub fn context_tokens(&self) -> u32 {
        self.context_tokens
    }
    pub fn model(&self) -> &str {
        &self.model
    }

    pub fn lease(&self) -> &SidecarLease {
        self.lease.as_ref()
    }

    pub async fn verify_lease(&self) -> Result<(), ProviderError> {
        self.verifier.verify(&self.lease).await
    }

    pub(crate) fn provider(&self) -> Arc<dyn ModelProvider> {
        Arc::new(LeaseBoundProvider {
            provider: Arc::clone(&self.provider),
            lease: Arc::clone(&self.lease),
            verifier: Arc::clone(&self.verifier),
        })
    }
}

struct LeaseBoundProvider {
    provider: Arc<dyn ModelProvider>,
    lease: Arc<SidecarLease>,
    verifier: Arc<dyn LeaseVerifier>,
}

impl std::fmt::Debug for LeaseBoundProvider {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("LeaseBoundProvider")
            .field("provider", &self.provider.id())
            .field("lease", &self.lease)
            .finish()
    }
}

#[async_trait]
impl ModelProvider for LeaseBoundProvider {
    async fn capability_runtime_evidence(&self) -> Result<Option<String>, ProviderError> {
        self.verifier.verify(&self.lease).await?;
        Ok(Some(self.lease.fingerprint()))
    }

    async fn validate_request(
        &self,
        request: &crate::ModelRequest,
    ) -> Result<crate::ProviderRequestValidation, ProviderError> {
        self.verifier.verify(&self.lease).await?;
        let mut result = self.provider.validate_request(request).await?;
        self.verifier.verify(&self.lease).await?;
        result.runtime_fingerprint = Some(self.lease.fingerprint());
        Ok(result)
    }
    fn id(&self) -> &'static str {
        self.provider.id()
    }

    fn status(&self) -> argentum_domain::ProviderStatus {
        self.provider.status()
    }

    async fn probe(&self) -> Result<argentum_domain::ProviderStatus, ProviderError> {
        self.verifier.verify(&self.lease).await?;
        let status = self.provider.probe().await?;
        self.verifier.verify(&self.lease).await?;
        Ok(status)
    }

    async fn list_models(&self) -> Result<Vec<crate::ProviderModel>, ProviderError> {
        self.verifier.verify(&self.lease).await?;
        let models = self.provider.list_models().await?;
        self.verifier.verify(&self.lease).await?;
        Ok(models)
    }

    async fn stream(
        &self,
        request: crate::ModelRequest,
        sender: tokio::sync::mpsc::Sender<crate::ProviderEvent>,
    ) -> Result<(), ProviderError> {
        self.stream_with_cancellation(request, sender, CancellationToken::new())
            .await
    }

    async fn stream_with_cancellation(
        &self,
        request: crate::ModelRequest,
        sender: tokio::sync::mpsc::Sender<crate::ProviderEvent>,
        cancellation: CancellationToken,
    ) -> Result<(), ProviderError> {
        self.verifier.verify(&self.lease).await?;
        let (proxy_sender, mut proxy_receiver) = tokio::sync::mpsc::channel(32);
        let provider_cancellation = cancellation.clone();
        let provider_future =
            self.provider
                .stream_with_cancellation(request, proxy_sender, provider_cancellation);
        tokio::pin!(provider_future);
        let mut lease_checks = tokio::time::interval(Duration::from_millis(25));
        lease_checks.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        lease_checks.tick().await;
        let mut provider_done = false;
        loop {
            tokio::select! {
                _ = cancellation.cancelled() => return Err(ProviderError::RequestCancelled),
                _ = lease_checks.tick() => {
                    self.verifier.verify(&self.lease).await?;
                }
                result = &mut provider_future, if !provider_done => {
                    let result = result;
                    self.verifier.verify(&self.lease).await?;
                    result?;
                    provider_done = true;
                }
                event = proxy_receiver.recv() => {
                    let Some(event) = event else {
                        if provider_done {
                            self.verifier.verify(&self.lease).await?;
                            return Ok(());
                        }
                        return Err(ProviderError::Stream);
                    };
                    let permit = tokio::select! {
                        biased;
                        _ = cancellation.cancelled() => {
                            return Err(ProviderError::RequestCancelled);
                        }
                        permit = sender.reserve() => {
                            permit.map_err(|_| ProviderError::ConsumerClosed)?
                        }
                    };
                    self.verifier.verify(&self.lease).await?;
                    if cancellation.is_cancelled() {
                        return Err(ProviderError::RequestCancelled);
                    }
                    permit.send(event);
                }
            }
            if provider_done && proxy_receiver.is_empty() && proxy_receiver.is_closed() {
                self.verifier.verify(&self.lease).await?;
                return Ok(());
            }
        }
    }
}

pub struct PreparedLocalRuntime {
    plan: LocalRuntimeLaunchPlan,
    resources: Option<Box<dyn Any + Send>>,
}

impl std::fmt::Debug for PreparedLocalRuntime {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("PreparedLocalRuntime")
            .field("plan", &self.plan)
            .field("resources", &"[PRIVATE]")
            .finish()
    }
}

impl PreparedLocalRuntime {
    pub fn plan(&self) -> &LocalRuntimeLaunchPlan {
        &self.plan
    }
}

#[async_trait]
trait RuntimeInstance: Send {
    async fn stop(&mut self) -> Result<(), ProviderError>;
}

struct LaunchedRuntime {
    client: Arc<LocalRuntimeClient>,
    instance: Box<dyn RuntimeInstance>,
}

#[async_trait]
trait LocalRuntimeLauncher: Send + Sync {
    async fn prepare(
        &self,
        config: &LocalRuntimeConfig,
        profile: &ProviderProfile,
        generation: u64,
    ) -> Result<PreparedLocalRuntime, ProviderError>;

    async fn launch(
        &self,
        prepared: PreparedLocalRuntime,
        authority: LocalRuntimeAuthority,
        cancellation: &CancellationToken,
    ) -> Result<LaunchedRuntime, ProviderError>;
}

struct LocalRuntimeEffect {
    instance: Option<Box<dyn RuntimeInstance>>,
}

#[async_trait]
impl EffectDisposer for LocalRuntimeEffect {
    async fn dispose(&mut self, _cancellation: &CancellationToken) -> Result<(), EffectFailure> {
        let Some(instance) = self.instance.as_mut() else {
            return Ok(());
        };
        instance
            .stop()
            .await
            .map_err(|_| EffectFailure::new("local runtime process teardown failed"))?;
        self.instance = None;
        Ok(())
    }
}

struct LocalRuntimeState {
    kernel: LifecycleKernel,
    mounted: Option<MountedModule>,
    active: Option<Arc<LocalRuntimeClient>>,
}

pub struct LocalRuntimeModule {
    config: LocalRuntimeConfig,
    authority_owner: Arc<PlatformAuthorityBroker>,
    launcher: Arc<dyn LocalRuntimeLauncher>,
    state: Mutex<LocalRuntimeState>,
}

impl std::fmt::Debug for LocalRuntimeModule {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("LocalRuntimeModule")
            .field("config", &self.config)
            .finish_non_exhaustive()
    }
}

impl LocalRuntimeModule {
    /// Names only. Selection/activation still validates the held model artifact.
    pub fn installed_model_names(&self) -> Vec<String> {
        let Ok(entries) = std::fs::read_dir(&self.config.models_dir) else {
            return Vec::new();
        };
        let mut names: Vec<_> = entries
            .take(128)
            .filter_map(Result::ok)
            .filter(|e| e.file_type().is_ok_and(|t| t.is_file() && !t.is_symlink()))
            .filter_map(|e| e.file_name().into_string().ok())
            .filter(|n| {
                n.len() <= 256
                    && !n.chars().any(char::is_control)
                    && n.to_ascii_lowercase().ends_with(".gguf")
            })
            .collect();
        names.sort();
        names.dedup();
        names
    }
    /// Last successful activation metadata only. No probe, model load or lease
    /// renewal is performed; the active request path still revalidates its lease.
    pub fn cached_model_identity(&self, model: &str) -> Option<(String, String)> {
        let state = self.state.try_lock().ok()?;
        let client = state.active.as_ref()?;
        (client.model() == model).then(|| {
            (
                client.model().to_owned(),
                client.lease().model_sha256().to_owned(),
            )
        })
    }

    pub fn new(config: LocalRuntimeConfig, authority_owner: Arc<PlatformAuthorityBroker>) -> Self {
        Self::with_launcher(config, authority_owner, Arc::new(LlamaCppSidecarLauncher))
    }

    fn with_launcher(
        config: LocalRuntimeConfig,
        authority_owner: Arc<PlatformAuthorityBroker>,
        launcher: Arc<dyn LocalRuntimeLauncher>,
    ) -> Self {
        Self {
            config,
            authority_owner,
            launcher,
            state: Mutex::new(LocalRuntimeState {
                kernel: LifecycleKernel::new(),
                mounted: None,
                active: None,
            }),
        }
    }

    pub async fn prepare(
        &self,
        profile: &ProviderProfile,
    ) -> Result<PreparedLocalRuntime, ProviderError> {
        self.prepare_with_context(profile, self.config.context_tokens)
            .await
    }

    pub async fn prepare_with_context(
        &self,
        profile: &ProviderProfile,
        context_tokens: u32,
    ) -> Result<PreparedLocalRuntime, ProviderError> {
        let config = self.config.clone().with_context_tokens(context_tokens)?;
        let generation = {
            let state = self.state.lock().await;
            state
                .kernel
                .binding()
                .generation()
                .checked_add(if state.mounted.is_some() { 2 } else { 1 })
                .ok_or(ProviderError::LocalRuntimeLifecycleFailed)?
        };
        self.launcher.prepare(&config, profile, generation).await
    }

    pub async fn activate(
        &self,
        prepared: PreparedLocalRuntime,
        approval: Option<LocalRuntimeLaunchApproval>,
        cancellation: CancellationToken,
    ) -> Result<Arc<LocalRuntimeClient>, ProviderError> {
        let (_authority_operation, authority_generation, platform_authority) = self
            .authority_owner
            .acquire_current_authority()
            .await
            .map_err(|_| ProviderError::LocalRuntimeApprovalStale)?;
        let authority = LocalRuntimeAuthority::new(
            Arc::clone(&self.authority_owner),
            authority_generation,
            platform_authority,
        );
        authority.ensure_activation()?;
        let mut state = self.state.lock().await;
        authority.ensure_activation()?;
        if let Some(active) = state.active.as_ref() {
            if active.model() == prepared.plan.model
                && active.context_tokens() == prepared.plan.context_tokens
            {
                active.verify_lease().await?;
                authority.ensure_activation()?;
                return Ok(Arc::clone(active));
            }
        }
        let approval = approval.ok_or(ProviderError::LocalRuntimeApprovalRequired)?;
        authority.ensure_activation()?;
        approval.authorize(&prepared.plan, authority.generation())?;
        let expected_generation = state
            .kernel
            .binding()
            .generation()
            .checked_add(if state.mounted.is_some() { 2 } else { 1 })
            .ok_or(ProviderError::LocalRuntimeLifecycleFailed)?;
        if prepared.plan.generation != expected_generation {
            return Err(ProviderError::LocalRuntimeApprovalStale);
        }
        if state.mounted.is_some() {
            unmount_runtime(&mut state, &cancellation).await?;
            authority.ensure_activation()?;
        }
        if cancellation.is_cancelled() {
            return Err(ProviderError::LocalRuntimeCancelled);
        }
        authority.ensure_activation()?;

        let binding = state.kernel.binding();
        let launcher = Arc::clone(&self.launcher);
        let client_cell = Arc::new(StdMutex::new(None::<Arc<LocalRuntimeClient>>));
        let publish_cell = Arc::clone(&client_cell);
        let mount_cancellation = cancellation.clone();
        let mount_authority = authority.clone();
        authority.ensure_activation()?;
        let mounted = state
            .kernel
            .mount(
                binding,
                ModuleSpec::new(LOCAL_RUNTIME_MODULE_ID),
                &cancellation,
                move |context| -> MountFuture<'_> {
                    let completion_authority = mount_authority.clone();
                    Box::pin(async move {
                        let launch_authority = mount_authority;
                        context
                            .apply_effect(LOCAL_RUNTIME_EFFECT_LABEL, move || async move {
                                let launched = launcher
                                    .launch(prepared, launch_authority.clone(), &mount_cancellation)
                                    .await
                                    .map_err(|error_value| {
                                        EffectFailure::new(error_value.to_string())
                                    })?;
                                if !launch_authority.is_current() {
                                    return Err(EffectFailure::new(
                                        "local runtime authority became stale",
                                    ));
                                }
                                *publish_cell.lock().map_err(|_| {
                                    EffectFailure::new("local runtime publication failed")
                                })? = Some(Arc::clone(&launched.client));
                                Ok(LocalRuntimeEffect {
                                    instance: Some(launched.instance),
                                })
                            })
                            .await?;
                        if !completion_authority.is_current() {
                            return Err(MountFailure::module(
                                "local runtime authority became stale",
                            ));
                        }
                        Ok(())
                    })
                },
            )
            .await
            .map_err(|_| ProviderError::LocalRuntimeLifecycleFailed)?;
        authority.ensure_activation()?;
        let client = client_cell
            .lock()
            .map_err(|_| ProviderError::LocalRuntimeLifecycleFailed)?
            .take()
            .ok_or(ProviderError::LocalRuntimeLifecycleFailed)?;
        if client.lease.generation() != mounted.module().mount_generation() {
            let binding = state.kernel.binding();
            let _ = state
                .kernel
                .unmount(
                    binding,
                    mounted.module(),
                    mounted.disposer(),
                    &CancellationToken::new(),
                )
                .await;
            return Err(ProviderError::LocalRuntimeLifecycleFailed);
        }
        authority.ensure_activation()?;
        state.active = Some(Arc::clone(&client));
        state.mounted = Some(mounted);
        Ok(client)
    }

    pub async fn client(&self, model: &str) -> Result<Arc<LocalRuntimeClient>, ProviderError> {
        let state = self.state.lock().await;
        let mounted = state
            .mounted
            .as_ref()
            .ok_or(ProviderError::LocalRuntimeLeaseStale)?;
        let snapshot = state
            .kernel
            .inspect_module(mounted.module())
            .map_err(|_| ProviderError::LocalRuntimeLeaseStale)?;
        if snapshot.state() != argentum_lifecycle::ModuleState::Mounted {
            return Err(ProviderError::LocalRuntimeLeaseStale);
        }
        let active = state
            .active
            .as_ref()
            .filter(|active| active.model() == model)
            .ok_or(ProviderError::LocalRuntimeLeaseStale)?;
        Ok(Arc::clone(active))
    }

    pub async fn shutdown(&self, cancellation: CancellationToken) -> Result<(), ProviderError> {
        let mut state = self.state.lock().await;
        if state.mounted.is_none() {
            state.active = None;
            return Ok(());
        }
        unmount_runtime(&mut state, &cancellation).await
    }

    pub async fn lifecycle_snapshot(&self) -> LifecycleSnapshot {
        self.state.lock().await.kernel.snapshot()
    }
}

async fn unmount_runtime(
    state: &mut LocalRuntimeState,
    cancellation: &CancellationToken,
) -> Result<(), ProviderError> {
    let mounted = state
        .mounted
        .as_ref()
        .ok_or(ProviderError::LocalRuntimeLifecycleFailed)?;
    state
        .kernel
        .unmount(
            state.kernel.binding(),
            mounted.module(),
            mounted.disposer(),
            cancellation,
        )
        .await
        .map_err(|_| ProviderError::LocalRuntimeLifecycleFailed)?;
    state.mounted = None;
    state.active = None;
    Ok(())
}

struct LlamaCppSidecarLauncher;

#[derive(Clone)]
struct ArtifactVerificationControl {
    authority: LocalRuntimeAuthority,
    cancellation: CancellationToken,
}

#[derive(Clone, Copy)]
enum ArtifactVerificationStage {
    PreSpawn,
    ModelReadiness,
}

impl ArtifactVerificationControl {
    fn checkpoint(&self) -> Result<(), std::io::Error> {
        if self.cancellation.is_cancelled() || !self.authority.is_current() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::Interrupted,
                "artifact verification authority was withdrawn",
            ));
        }
        Ok(())
    }

    fn classify(
        &self,
        _error_value: std::io::Error,
        stage: ArtifactVerificationStage,
    ) -> ProviderError {
        if self.cancellation.is_cancelled() {
            ProviderError::LocalRuntimeCancelled
        } else if !self.authority.is_current() {
            ProviderError::LocalRuntimeApprovalStale
        } else {
            match stage {
                ArtifactVerificationStage::PreSpawn => ProviderError::LocalRuntimeApprovalStale,
                ArtifactVerificationStage::ModelReadiness => {
                    ProviderError::LocalRuntimeLaunchFailed {
                        stage: "model-recheck",
                    }
                }
            }
        }
    }
}

async fn run_owned_blocking<T, F>(resource: T, operation: F) -> (T, Result<(), ProviderError>)
where
    T: Send + 'static,
    F: FnOnce(&mut T) -> Result<(), ProviderError> + Send + 'static,
{
    let shared = Arc::new(StdMutex::new(Some(resource)));
    let worker_shared = Arc::clone(&shared);
    let join_result = tokio::task::spawn_blocking(move || {
        let mut guard = match worker_shared.lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        };
        let resource = guard
            .as_mut()
            .ok_or(ProviderError::LocalRuntimeLaunchFailed {
                stage: "artifact-verification-worker",
            })?;
        operation(resource)
    })
    .await;
    let operation_result = match join_result {
        Ok(result) => result,
        Err(_) => Err(ProviderError::LocalRuntimeLaunchFailed {
            stage: "artifact-verification-worker",
        }),
    };
    let mut guard = match shared.lock() {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(),
    };
    let resource = guard
        .take()
        .expect("settled blocking verifier retained its owned resource");
    drop(guard);
    (resource, operation_result)
}

#[cfg(test)]
#[derive(Clone, Copy, PartialEq, Eq)]
enum TestArtifactVerificationFailure {
    PreSpawn,
    PostResume,
}

#[cfg(test)]
struct TestPostResumeVerificationHook {
    started_marker: PathBuf,
    observed_tx: std::sync::mpsc::Sender<()>,
}

struct ProductionPreparedRuntime {
    process_spec: ProcessSpec,
    process_limits: ManagedProcessLimits,
    environment_sha256: Sha256Digest,
    reserved_listener: Option<std::net::TcpListener>,
    key_file: Option<PrivateFile>,
    session: Option<tempfile::TempDir>,
    api_key: SecretValue,
    archive_lease: Option<VerifiedFileLease>,
    launcher_lease: Option<VerifiedFileLease>,
    model_lease: Option<ModelFileLease>,
    max_model_bytes: u64,
    endpoint: String,
    #[cfg(test)]
    verification_failure: Option<TestArtifactVerificationFailure>,
    #[cfg(test)]
    fail_session_cleanup: bool,
    #[cfg(test)]
    post_resume_verification_hook: Option<TestPostResumeVerificationHook>,
}

impl ProductionPreparedRuntime {
    fn take_started_resources(
        &mut self,
    ) -> Option<(ModelFileLease, PrivateFile, tempfile::TempDir)> {
        if self.model_lease.is_none() || self.key_file.is_none() || self.session.is_none() {
            return None;
        }
        Some((
            self.model_lease.take()?,
            self.key_file.take()?,
            self.session.take()?,
        ))
    }
}

impl Drop for ProductionPreparedRuntime {
    fn drop(&mut self) {
        drop(self.launcher_lease.take());
        drop(self.archive_lease.take());
        drop(self.key_file.take());
        drop(self.model_lease.take());
        if let Some(session) = self.session.take() {
            let _ = session.close();
        }
    }
}

struct ProductionRuntimeInstance {
    process: Arc<Mutex<Option<ManagedProcess>>>,
    key_file: Option<PrivateFile>,
    session_path: Option<PathBuf>,
    _model_lease: Arc<StdMutex<ModelFileLease>>,
}

#[async_trait]
impl RuntimeInstance for ProductionRuntimeInstance {
    async fn stop(&mut self) -> Result<(), ProviderError> {
        let process = self.process.lock().await.take();
        if let Some(process) = process {
            process
                .terminate_and_wait(Duration::from_secs(2))
                .await
                .map_err(|_| ProviderError::LocalRuntimeLaunchFailed { stage: "teardown" })?;
        }
        drop(self.key_file.take());
        if let Some(path) = self.session_path.take() {
            if let Err(error_value) = remove_runtime_session_path(&path).await {
                self.session_path = Some(path);
                return Err(error_value);
            }
        }
        Ok(())
    }
}

impl Drop for ProductionRuntimeInstance {
    fn drop(&mut self) {
        drop(self.key_file.take());
        if let Some(path) = self.session_path.take() {
            let _ = std::fs::remove_dir_all(path);
        }
    }
}

async fn remove_runtime_session(session: tempfile::TempDir) -> Result<(), ProviderError> {
    remove_runtime_session_path(&session.keep()).await
}

async fn remove_runtime_session_path(path: &Path) -> Result<(), ProviderError> {
    for attempt in 0..20 {
        match tokio::fs::remove_dir_all(path).await {
            Ok(()) => return Ok(()),
            Err(error_value) if error_value.kind() == std::io::ErrorKind::NotFound => return Ok(()),
            Err(_) if attempt < 19 => tokio::time::sleep(Duration::from_millis(50)).await,
            Err(_) => {
                return Err(ProviderError::LocalRuntimeLaunchFailed {
                    stage: "session-cleanup",
                })
            }
        }
    }
    Err(ProviderError::LocalRuntimeLaunchFailed {
        stage: "session-cleanup",
    })
}

fn verify_pre_spawn_artifacts_blocking(
    resources: &mut ProductionPreparedRuntime,
    plan: &LocalRuntimeLaunchPlan,
    control: &ArtifactVerificationControl,
) -> Result<(), ProviderError> {
    #[cfg(test)]
    if resources.verification_failure == Some(TestArtifactVerificationFailure::PreSpawn) {
        return Err(ProviderError::LocalRuntimeApprovalStale);
    }
    let archive = resources
        .archive_lease
        .as_mut()
        .ok_or(ProviderError::LocalRuntimeApprovalStale)?;
    archive
        .verify_unchanged_with(|| control.checkpoint())
        .and_then(|()| archive.verify_identity())
        .map_err(|error_value| {
            control.classify(error_value, ArtifactVerificationStage::PreSpawn)
        })?;
    if archive.sha256() != plan.runtime_bundle_sha256 {
        return Err(ProviderError::LocalRuntimeApprovalStale);
    }

    let launcher = resources
        .launcher_lease
        .as_mut()
        .ok_or(ProviderError::LocalRuntimeApprovalStale)?;
    launcher
        .verify_unchanged_with(|| control.checkpoint())
        .and_then(|()| launcher.verify_identity())
        .map_err(|error_value| {
            control.classify(error_value, ArtifactVerificationStage::PreSpawn)
        })?;
    if launcher.path() != plan.program || launcher.sha256() != plan.executable_sha256 {
        return Err(ProviderError::LocalRuntimeApprovalStale);
    }

    let model = resources
        .model_lease
        .as_mut()
        .ok_or(ProviderError::LocalRuntimeApprovalStale)?;
    model
        .verify_unchanged_with(|| control.checkpoint())
        .and_then(|()| model.verify_identity())
        .map_err(|error_value| {
            control.classify(error_value, ArtifactVerificationStage::PreSpawn)
        })?;
    if model.path() != plan.model_path
        || model.sha256() != plan.model_sha256
        || model.size_bytes() != plan.model_size_bytes
        || resources.max_model_bytes != plan.max_model_bytes
        || model.size_bytes() > plan.max_model_bytes
    {
        return Err(ProviderError::LocalRuntimeApprovalStale);
    }
    Ok(())
}

fn verify_post_resume_artifact_identities(
    resources: &mut ProductionPreparedRuntime,
) -> Result<(), ProviderError> {
    #[cfg(test)]
    if resources.verification_failure == Some(TestArtifactVerificationFailure::PostResume) {
        if let Some(hook) = resources.post_resume_verification_hook.take() {
            let deadline = std::time::Instant::now() + Duration::from_secs(2);
            while !hook.started_marker.is_file() {
                if std::time::Instant::now() >= deadline {
                    return Err(ProviderError::LocalRuntimeLaunchFailed {
                        stage: "post-resume-test-hook",
                    });
                }
                std::thread::sleep(Duration::from_millis(10));
            }
            hook.observed_tx
                .send(())
                .map_err(|_| ProviderError::LocalRuntimeLaunchFailed {
                    stage: "post-resume-test-hook",
                })?;
        }
        return Err(ProviderError::LocalRuntimeApprovalStale);
    }
    resources
        .archive_lease
        .as_ref()
        .ok_or(ProviderError::LocalRuntimeApprovalStale)?
        .verify_identity()
        .map_err(|_| ProviderError::LocalRuntimeApprovalStale)?;
    resources
        .launcher_lease
        .as_ref()
        .ok_or(ProviderError::LocalRuntimeApprovalStale)?
        .verify_identity()
        .map_err(|_| ProviderError::LocalRuntimeApprovalStale)?;
    resources
        .model_lease
        .as_ref()
        .ok_or(ProviderError::LocalRuntimeApprovalStale)?
        .verify_identity()
        .map_err(|_| ProviderError::LocalRuntimeApprovalStale)
}

fn verify_post_readiness_model_blocking(
    resources: &mut ProductionPreparedRuntime,
    control: &ArtifactVerificationControl,
) -> Result<(), ProviderError> {
    resources
        .model_lease
        .as_mut()
        .ok_or(ProviderError::LocalRuntimeApprovalStale)?
        .verify_unchanged_with(|| control.checkpoint())
        .map_err(|error_value| {
            control.classify(error_value, ArtifactVerificationStage::ModelReadiness)
        })
}

async fn cleanup_prepared_resources(
    resources: &mut ProductionPreparedRuntime,
) -> Result<(), ProviderError> {
    drop(resources.reserved_listener.take());
    drop(resources.launcher_lease.take());
    drop(resources.archive_lease.take());
    drop(resources.key_file.take());
    drop(resources.model_lease.take());
    #[cfg(test)]
    if resources.fail_session_cleanup {
        if let Some(session) = resources.session.take() {
            let _ = session.keep();
        }
        return Err(ProviderError::LocalRuntimeLaunchFailed {
            stage: "session-cleanup",
        });
    }
    match resources.session.take() {
        Some(session) => remove_runtime_session(session).await,
        None => Ok(()),
    }
}

async fn cleanup_pre_spawn_failure(
    mut resources: Box<ProductionPreparedRuntime>,
    failure: ProviderError,
) -> ProviderError {
    match cleanup_prepared_resources(&mut resources).await {
        Ok(()) => failure,
        Err(cleanup_error) => cleanup_error,
    }
}

async fn cleanup_failed_runtime_start(
    process: ManagedProcess,
    resources: &mut ProductionPreparedRuntime,
) -> Result<(), ProviderError> {
    let stop_result = process.terminate_and_wait(Duration::from_millis(250)).await;
    let cleanup_result = cleanup_prepared_resources(resources).await;
    cleanup_result?;
    stop_result
        .map(|_| ())
        .map_err(|_| ProviderError::LocalRuntimeLaunchFailed { stage: "teardown" })
}

struct ProductionLeaseVerifier {
    process: Arc<Mutex<Option<ManagedProcess>>>,
    model_lease: Arc<StdMutex<ModelFileLease>>,
    expected_port: u16,
    authority: LocalRuntimeAuthority,
}

#[async_trait]
impl LeaseVerifier for ProductionLeaseVerifier {
    async fn verify(&self, lease: &SidecarLease) -> Result<(), ProviderError> {
        if !self.authority.is_current()
            || !self
                .authority
                .lease()
                .matches_generation(&lease.authority_generation)
        {
            return Err(ProviderError::RequestCancelled);
        }
        if lease.authority_generation != *self.authority.generation() {
            return Err(ProviderError::LocalRuntimeLeaseStale);
        }
        let mut process_guard = self.process.lock().await;
        if !self.authority.is_current()
            || !self
                .authority
                .lease()
                .matches_generation(&lease.authority_generation)
        {
            return Err(ProviderError::RequestCancelled);
        }
        {
            let process = process_guard
                .as_mut()
                .ok_or(ProviderError::LocalRuntimeLeaseStale)?;
            if process.id() != lease.pid
                || process.process_started_at() != lease.process_started_at
                || process
                    .has_exited()
                    .map_err(|_| ProviderError::LocalRuntimeLeaseStale)?
            {
                return Err(ProviderError::LocalRuntimeLeaseStale);
            }
            let listeners = process
                .loopback_listeners()
                .map_err(|_| ProviderError::LocalRuntimeLeaseStale)?;
            if listeners.len() != 1 || listeners[0].port != self.expected_port {
                return Err(ProviderError::LocalRuntimeLeaseStale);
            }
        }
        drop(process_guard);
        self.model_lease
            .lock()
            .map_err(|_| ProviderError::LocalRuntimeLeaseStale)?
            .verify_identity()
            .map_err(|_| ProviderError::LocalRuntimeLeaseStale)
    }
}

#[async_trait]
impl LocalRuntimeLauncher for LlamaCppSidecarLauncher {
    async fn prepare(
        &self,
        config: &LocalRuntimeConfig,
        profile: &ProviderProfile,
        generation: u64,
    ) -> Result<PreparedLocalRuntime, ProviderError> {
        validate_local_profile(profile)?;
        config.clone().with_context_tokens(config.context_tokens)?;
        config.limits.validate()?;
        protect_selected_model_directory(&config.models_dir)?;
        let model_path = resolve_selected_model_with_limit(
            &config.models_dir,
            &profile.model,
            config.limits.max_model_bytes,
        )?;
        protect_selected_model_file(&model_path)?;
        let offer = *local_runtime_offers()
            .first()
            .ok_or(ProviderError::LocalRuntimeBundleInvalid)?;
        let bundle_path = config.bundle_path.clone();
        let session_parent = bundle_path
            .parent()
            .ok_or(ProviderError::LocalRuntimeBundleInvalid)?
            .to_path_buf();
        let max_model_bytes = config.limits.max_model_bytes;
        let prepared_files = tokio::task::spawn_blocking(move || {
            let mut archive_lease = open_runtime_archive_lease(&bundle_path, &offer)?;
            let session =
                extract_runtime_session_leased(&mut archive_lease, &offer, &session_parent)?;
            let launcher_lease = open_runtime_launcher_lease(session.path(), &offer)?;
            let model_lease = ModelFileLease::open(&model_path, max_model_bytes)
                .map_err(|_| ProviderError::LocalRuntimeModelInvalid)?;
            Ok::<_, ProviderError>((session, archive_lease, launcher_lease, model_lease))
        })
        .await
        .map_err(|_| ProviderError::LocalRuntimeLaunchFailed { stage: "prepare" })??;
        let (session, archive_lease, launcher_lease, model_lease) = prepared_files;

        let reserved_listener =
            std::net::TcpListener::bind((LOCAL_RUNTIME_HOST, 0)).map_err(|_| {
                ProviderError::LocalRuntimeLaunchFailed {
                    stage: "reserve-port",
                }
            })?;
        let port = reserved_listener
            .local_addr()
            .map_err(|_| ProviderError::LocalRuntimeLaunchFailed {
                stage: "reserve-port",
            })?
            .port();
        let epoch = random_hex(16)?;
        let api_prefix = format!("/argentum/generation-{generation}/{epoch}");
        let key_value = SecretValue::new(random_hex(32)?);
        let key_path = session.path().join("api-key.txt");
        let key_file =
            PrivateFile::create_new(&key_path, key_value.expose().as_bytes()).map_err(|_| {
                ProviderError::LocalRuntimeLaunchFailed {
                    stage: "private-key",
                }
            })?;
        let program = launcher_lease.path().to_path_buf();
        let arguments = llama_server_arguments_with_context(
            model_lease.path(),
            &key_path,
            port,
            &api_prefix,
            config.context_tokens,
        );
        let process_limits = ManagedProcessLimits::for_local_model(model_lease.size_bytes())
            .map_err(|_| ProviderError::LocalRuntimeModelInvalid)?;
        let process_spec = ProcessSpec::argv(
            program.to_string_lossy().into_owned(),
            arguments.clone(),
            session.path(),
        )
        .and_then(|spec| spec.with_output_limit(1024 * 1024))
        .map_err(|_| ProviderError::LocalRuntimeLaunchFailed { stage: "arguments" })?;
        let environment_sha256 = process_spec.environment_binding();
        let endpoint = format!("http://{LOCAL_RUNTIME_HOST}:{port}{api_prefix}/v1/");
        let plan = LocalRuntimeLaunchPlan {
            runtime_id: offer.id.into(),
            runtime_version: offer.version.into(),
            runtime_bundle_sha256: offer.sha256.into(),
            executable_sha256: offer.executable_sha256.into(),
            program,
            arguments,
            environment_sha256: environment_sha256.clone(),
            model: profile.model.clone(),
            model_path: model_lease.path().to_path_buf(),
            model_sha256: model_lease.sha256().to_owned(),
            model_size_bytes: model_lease.size_bytes(),
            max_model_bytes: config.limits.max_model_bytes,
            managed_active_process_limit: process_limits.active_process_limit(),
            managed_cpu_rate_percent: process_limits.cpu_rate_percent(),
            managed_process_memory_bytes: process_limits.process_memory_bytes(),
            managed_job_memory_bytes: process_limits.job_memory_bytes(),
            managed_output_hash_limit: process_limits.output_hash_limit(),
            generation,
            epoch,
            host: LOCAL_RUNTIME_HOST.into(),
            port,
            api_prefix,
            context_tokens: config.context_tokens,
            gpu_layers: LOCAL_RUNTIME_GPU_LAYERS,
            disabled_features: disabled_runtime_features(),
        };
        Ok(PreparedLocalRuntime {
            plan,
            resources: Some(Box::new(ProductionPreparedRuntime {
                process_spec,
                process_limits,
                environment_sha256,
                reserved_listener: Some(reserved_listener),
                key_file: Some(key_file),
                session: Some(session),
                api_key: key_value,
                archive_lease: Some(archive_lease),
                launcher_lease: Some(launcher_lease),
                model_lease: Some(model_lease),
                max_model_bytes,
                endpoint,
                #[cfg(test)]
                verification_failure: None,
                #[cfg(test)]
                fail_session_cleanup: false,
                #[cfg(test)]
                post_resume_verification_hook: None,
            })),
        })
    }

    async fn launch(
        &self,
        mut prepared: PreparedLocalRuntime,
        authority: LocalRuntimeAuthority,
        cancellation: &CancellationToken,
    ) -> Result<LaunchedRuntime, ProviderError> {
        let resources = prepared
            .resources
            .take()
            .ok_or(ProviderError::LocalRuntimeApprovalStale)?;
        let mut resources = resources
            .downcast::<ProductionPreparedRuntime>()
            .map_err(|_| ProviderError::LocalRuntimeApprovalStale)?;
        if cancellation.is_cancelled() {
            return Err(
                cleanup_pre_spawn_failure(resources, ProviderError::LocalRuntimeCancelled).await,
            );
        }
        if let Err(error_value) = authority.ensure_activation() {
            return Err(cleanup_pre_spawn_failure(resources, error_value).await);
        }
        if resources.process_spec.environment_binding() != resources.environment_sha256
            || resources.environment_sha256 != prepared.plan.environment_sha256
            || resources.process_limits.active_process_limit()
                != prepared.plan.managed_active_process_limit
            || resources.process_limits.cpu_rate_percent() != prepared.plan.managed_cpu_rate_percent
            || resources.process_limits.process_memory_bytes()
                != prepared.plan.managed_process_memory_bytes
            || resources.process_limits.job_memory_bytes() != prepared.plan.managed_job_memory_bytes
            || resources.process_limits.output_hash_limit()
                != prepared.plan.managed_output_hash_limit
        {
            return Err(cleanup_pre_spawn_failure(
                resources,
                ProviderError::LocalRuntimeApprovalStale,
            )
            .await);
        }
        if resources.key_file.is_none()
            || resources.session.is_none()
            || resources.model_lease.is_none()
        {
            return Err(cleanup_pre_spawn_failure(
                resources,
                ProviderError::LocalRuntimeApprovalStale,
            )
            .await);
        }
        drop(resources.reserved_listener.take());
        if let Err(error_value) = authority.ensure_activation() {
            return Err(cleanup_pre_spawn_failure(resources, error_value).await);
        }
        let verification_control = ArtifactVerificationControl {
            authority: authority.clone(),
            cancellation: cancellation.clone(),
        };
        let verification_plan = prepared.plan.clone();
        let worker_control = verification_control.clone();
        let (returned_resources, verification_result) =
            run_owned_blocking(resources, move |resources| {
                verify_pre_spawn_artifacts_blocking(resources, &verification_plan, &worker_control)
            })
            .await;
        resources = returned_resources;
        if let Err(error_value) = verification_result {
            return Err(cleanup_pre_spawn_failure(resources, error_value).await);
        }
        if cancellation.is_cancelled() {
            return Err(
                cleanup_pre_spawn_failure(resources, ProviderError::LocalRuntimeCancelled).await,
            );
        }
        if let Err(error_value) = authority.ensure_activation() {
            return Err(cleanup_pre_spawn_failure(resources, error_value).await);
        }
        let resume_authority = authority.clone();
        let resume_cancellation = cancellation.clone();
        let process_result = ManagedProcess::spawn_checked(
            resources.process_spec.clone(),
            resources.process_limits,
            move || !resume_cancellation.is_cancelled() && resume_authority.is_current(),
        )
        .await;
        let mut process = match process_result {
            Ok(process) => process,
            Err(error_value) => {
                let launch_error = match error_value {
                    ProcessError::PreResumeCheckFailed if cancellation.is_cancelled() => {
                        ProviderError::LocalRuntimeCancelled
                    }
                    ProcessError::PreResumeCheckFailed => ProviderError::LocalRuntimeApprovalStale,
                    _ => ProviderError::LocalRuntimeLaunchFailed { stage: "spawn" },
                };
                return Err(cleanup_pre_spawn_failure(resources, launch_error).await);
            }
        };
        let start_result = async {
            verify_post_resume_artifact_identities(&mut resources)?;
            drop(resources.launcher_lease.take());
            drop(resources.archive_lease.take());
            authority.ensure_activation()?;
            let provider = Arc::new(
                OpenAiCompatibleProvider::new_local_sidecar_with_context(
                    &resources.endpoint,
                    &prepared.plan.model,
                    resources.api_key.clone(),
                    prepared.plan.context_tokens,
                )
                .map_err(|_| ProviderError::LocalRuntimeLaunchFailed { stage: "client" })?,
            );

            let deadline = tokio::time::Instant::now() + LOCAL_RUNTIME_READY_TIMEOUT;
            loop {
                if cancellation.is_cancelled() {
                    return Err(ProviderError::LocalRuntimeCancelled);
                }
                if process
                    .has_exited()
                    .map_err(|_| ProviderError::LocalRuntimeLaunchFailed { stage: "wait" })?
                {
                    return Err(ProviderError::LocalRuntimeLaunchFailed { stage: "startup" });
                }
                let listeners = process
                    .loopback_listeners()
                    .map_err(|_| ProviderError::LocalRuntimeLaunchFailed { stage: "listener" })?;
                if listeners.len() > 1
                    || listeners
                        .first()
                        .is_some_and(|listener| listener.port != prepared.plan.port)
                {
                    return Err(ProviderError::LocalRuntimeLaunchFailed { stage: "listener" });
                }
                if listeners.len() == 1 {
                    let probe = tokio::select! {
                        _ = cancellation.cancelled() => {
                            Err(ProviderError::LocalRuntimeCancelled)
                        }
                        result = provider.list_models() => result.map(|_| ()),
                    };
                    authority.ensure_activation()?;
                    if probe.is_ok() {
                        break;
                    }
                    if matches!(probe, Err(ProviderError::LocalRuntimeCancelled)) {
                        return Err(ProviderError::LocalRuntimeCancelled);
                    }
                }
                if tokio::time::Instant::now() >= deadline {
                    return Err(ProviderError::LocalRuntimeLaunchFailed { stage: "readiness" });
                }
                tokio::select! {
                    _ = cancellation.cancelled() => {
                        return Err(ProviderError::LocalRuntimeCancelled);
                    }
                    _ = tokio::time::sleep(Duration::from_millis(50)) => {}
                }
                authority.ensure_activation()?;
            }
            Ok(provider)
        }
        .await;
        let provider = match start_result {
            Ok(provider) => provider,
            Err(error_value) => {
                cleanup_failed_runtime_start(process, &mut resources).await?;
                return Err(error_value);
            }
        };

        let readiness_control = ArtifactVerificationControl {
            authority: authority.clone(),
            cancellation: cancellation.clone(),
        };
        let worker_control = readiness_control.clone();
        let (returned_resources, model_recheck_result) =
            run_owned_blocking(resources, move |resources| {
                verify_post_readiness_model_blocking(resources, &worker_control)
            })
            .await;
        resources = returned_resources;
        if let Err(error_value) = model_recheck_result {
            cleanup_failed_runtime_start(process, &mut resources).await?;
            return Err(error_value);
        }
        if cancellation.is_cancelled() {
            cleanup_failed_runtime_start(process, &mut resources).await?;
            return Err(ProviderError::LocalRuntimeCancelled);
        }
        if let Err(error_value) = authority.ensure_activation() {
            cleanup_failed_runtime_start(process, &mut resources).await?;
            return Err(error_value);
        }

        let process_id = process.id();
        let process_started_at = process.process_started_at();
        let Some((model_lease, key_file, session)) = resources.take_started_resources() else {
            cleanup_failed_runtime_start(process, &mut resources).await?;
            return Err(ProviderError::LocalRuntimeApprovalStale);
        };
        let process = Arc::new(Mutex::new(Some(process)));
        let model_lease = Arc::new(StdMutex::new(model_lease));
        let session_path = session.keep();
        let lease = Arc::new(SidecarLease {
            epoch: prepared.plan.epoch.clone(),
            generation: prepared.plan.generation,
            authority_generation: authority.generation().clone(),
            model_sha256: prepared.plan.model_sha256.clone(),
            pid: process_id,
            process_started_at,
        });
        let verifier = Arc::new(ProductionLeaseVerifier {
            process: Arc::clone(&process),
            model_lease: Arc::clone(&model_lease),
            expected_port: prepared.plan.port,
            authority,
        });
        let client = Arc::new(LocalRuntimeClient {
            context_tokens: prepared.plan.context_tokens,
            model: prepared.plan.model,
            provider,
            lease,
            verifier,
        });
        Ok(LaunchedRuntime {
            client,
            instance: Box::new(ProductionRuntimeInstance {
                process,
                key_file: Some(key_file),
                session_path: Some(session_path),
                _model_lease: model_lease,
            }),
        })
    }
}

fn validate_local_profile(profile: &ProviderProfile) -> Result<(), ProviderError> {
    if profile.kind != ProviderKind::LocalLlamaCpp || profile.endpoint != "local://llama-cpp/" {
        return Err(ProviderError::InvalidProfile {
            field: "kind",
            reason: "profile must select the built-in llama.cpp runtime",
        });
    }
    Ok(())
}

#[cfg(test)]
fn resolve_selected_model(models_dir: &Path, model: &str) -> Result<PathBuf, ProviderError> {
    resolve_selected_model_with_limit(models_dir, model, LOCAL_RUNTIME_MAX_MODEL_BYTES)
}

fn resolve_selected_model_with_limit(
    models_dir: &Path,
    model: &str,
    max_model_bytes: u64,
) -> Result<PathBuf, ProviderError> {
    LocalRuntimeLimits { max_model_bytes }.validate()?;
    if !models_dir.is_absolute()
        || model.is_empty()
        || model.chars().count() > 256
        || model.chars().any(char::is_control)
        || model.contains(['/', '\\', ':'])
        || !model.to_ascii_lowercase().ends_with(".gguf")
    {
        return Err(ProviderError::LocalRuntimeModelInvalid);
    }
    let mut components = Path::new(model).components();
    if !matches!(components.next(), Some(Component::Normal(_))) || components.next().is_some() {
        return Err(ProviderError::LocalRuntimeModelInvalid);
    }
    let root =
        std::fs::canonicalize(models_dir).map_err(|_| ProviderError::LocalRuntimeModelInvalid)?;
    let selected = root.join(model);
    let metadata = std::fs::symlink_metadata(&selected)
        .map_err(|_| ProviderError::LocalRuntimeModelInvalid)?;
    if metadata.file_type().is_symlink() || !metadata.is_file() || metadata.len() > max_model_bytes
    {
        return Err(ProviderError::LocalRuntimeModelInvalid);
    }
    let selected =
        std::fs::canonicalize(selected).map_err(|_| ProviderError::LocalRuntimeModelInvalid)?;
    if selected.parent() != Some(root.as_path()) {
        return Err(ProviderError::LocalRuntimeModelInvalid);
    }
    Ok(selected)
}

fn protect_selected_model_directory(path: &Path) -> Result<(), ProviderError> {
    #[cfg(windows)]
    ensure_owner_only_directory(path).map_err(|_| ProviderError::LocalRuntimeModelInvalid)?;
    #[cfg(not(windows))]
    let _ = path;
    Ok(())
}

fn protect_selected_model_file(path: &Path) -> Result<(), ProviderError> {
    #[cfg(windows)]
    ensure_owner_only_file(path).map_err(|_| ProviderError::LocalRuntimeModelInvalid)?;
    #[cfg(not(windows))]
    let _ = path;
    Ok(())
}

#[cfg(test)]
fn llama_server_arguments(
    model_path: &Path,
    key_path: &Path,
    port: u16,
    api_prefix: &str,
) -> Vec<String> {
    llama_server_arguments_with_context(
        model_path,
        key_path,
        port,
        api_prefix,
        LOCAL_RUNTIME_CONTEXT,
    )
}

fn llama_server_arguments_with_context(
    model_path: &Path,
    key_path: &Path,
    port: u16,
    api_prefix: &str,
    context_tokens: u32,
) -> Vec<String> {
    vec![
        "--model".into(),
        model_path.to_string_lossy().into_owned(),
        "--host".into(),
        LOCAL_RUNTIME_HOST.into(),
        "--port".into(),
        port.to_string(),
        "--api-prefix".into(),
        api_prefix.into(),
        "--api-key-file".into(),
        key_path.to_string_lossy().into_owned(),
        "--ctx-size".into(),
        context_tokens.to_string(),
        "--n-gpu-layers".into(),
        LOCAL_RUNTIME_GPU_LAYERS.to_string(),
        "--parallel".into(),
        "1".into(),
        "--predict".into(),
        LOCAL_RUNTIME_MAX_PREDICTION.to_string(),
        "--jinja".into(),
        "--no-context-shift".into(),
        "--reasoning".into(),
        "off".into(),
        "--no-webui".into(),
        "--no-slots".into(),
        "--offline".into(),
    ]
}

fn disabled_runtime_features() -> Vec<String> {
    [
        "web-ui",
        "slots-endpoint",
        "remote-downloads",
        "server-side-tool-execution",
        "mcp",
        "agent",
        "router",
        "media",
        "reasoning",
    ]
    .into_iter()
    .map(str::to_owned)
    .collect()
}

fn random_hex(bytes: usize) -> Result<String, ProviderError> {
    let mut value = vec![0_u8; bytes];
    getrandom::fill(&mut value).map_err(|_| ProviderError::LocalRuntimeLaunchFailed {
        stage: "randomness",
    })?;
    let mut encoded = String::with_capacity(bytes * 2);
    for byte in value {
        use std::fmt::Write as _;
        write!(&mut encoded, "{byte:02x}").map_err(|_| {
            ProviderError::LocalRuntimeLaunchFailed {
                stage: "randomness",
            }
        })?;
    }
    Ok(encoded)
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::Path;
    use std::sync::atomic::{AtomicBool, AtomicU16, AtomicU32, AtomicU64, AtomicUsize, Ordering};
    use std::sync::mpsc as std_mpsc;

    use argentum_domain::{PlatformAuthorityState, ProviderKind, ProviderProfile};
    use argentum_platform::{AuthorityDependentService, PlatformAuthorityBroker, PlatformError};
    use argentum_security::{
        host_authority_channel, LocalAuthorityAction, LocalAuthorityIssuer, LocalAuthorityVerifier,
        SecretValue, SecurityError,
    };
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::sync::{mpsc, oneshot, Notify};

    use super::*;

    const TEST_MIN_MODEL_BYTES: u64 = 1024 * 1024;
    const TEST_DEFAULT_MODEL_BYTES: u64 = 64 * 1024 * 1024 * 1024;
    const TEST_MAX_MODEL_BYTES: u64 = 256 * 1024 * 1024 * 1024;

    #[test]
    fn local_runtime_model_limits_default_validate_and_bind_the_plan_digest() {
        let config = LocalRuntimeConfig::new(
            PathBuf::from(r"C:\argentum-test\bundle.zip"),
            PathBuf::from(r"C:\argentum-test\models"),
        )
        .expect("default local runtime config");
        assert_eq!(
            config.limits,
            LocalRuntimeLimits {
                max_model_bytes: TEST_DEFAULT_MODEL_BYTES,
            }
        );
        assert!(config
            .clone()
            .with_max_model_bytes(TEST_MIN_MODEL_BYTES - 1)
            .is_err());
        assert!(config
            .clone()
            .with_max_model_bytes(TEST_MAX_MODEL_BYTES + 1)
            .is_err());
        assert_eq!(
            config
                .clone()
                .with_max_model_bytes(TEST_MIN_MODEL_BYTES)
                .expect("minimum model limit")
                .limits
                .max_model_bytes,
            TEST_MIN_MODEL_BYTES
        );
        assert_eq!(
            config
                .with_max_model_bytes(TEST_MAX_MODEL_BYTES)
                .expect("maximum model limit")
                .limits
                .max_model_bytes,
            TEST_MAX_MODEL_BYTES
        );

        let mut plan = LocalRuntimeLaunchPlan::test_fixture();
        plan.max_model_bytes = TEST_DEFAULT_MODEL_BYTES;
        let default_digest = plan.digest();
        plan.max_model_bytes = TEST_MIN_MODEL_BYTES;
        assert_ne!(plan.digest(), default_digest);
        let mut digest = plan.digest();
        plan.managed_active_process_limit = 3;
        assert_ne!(plan.digest(), digest);
        digest = plan.digest();
        plan.managed_cpu_rate_percent = 94;
        assert_ne!(plan.digest(), digest);
        digest = plan.digest();
        plan.managed_process_memory_bytes += 1;
        assert_ne!(plan.digest(), digest);
        digest = plan.digest();
        plan.managed_job_memory_bytes += 1;
        assert_ne!(plan.digest(), digest);
        digest = plan.digest();
        plan.managed_output_hash_limit += 1;
        assert_ne!(plan.digest(), digest);
    }

    #[tokio::test]
    async fn oversized_selected_model_is_rejected_before_launch_preparation() {
        let directory = tempfile::tempdir().expect("local runtime fixture");
        let models = directory.path().join("models");
        fs::create_dir(&models).expect("models directory");
        let model = models.join("oversized.gguf");
        let file = fs::File::create(&model).expect("oversized model fixture");
        file.set_len(TEST_MIN_MODEL_BYTES + 1)
            .expect("oversized model length");
        drop(file);
        let config = LocalRuntimeConfig::new(directory.path().join("missing.zip"), &models)
            .expect("local runtime config")
            .with_max_model_bytes(TEST_MIN_MODEL_BYTES)
            .expect("minimum model limit");

        let result = LlamaCppSidecarLauncher
            .prepare(&config, &local_profile("oversized.gguf"), 1)
            .await;

        assert!(matches!(
            result,
            Err(ProviderError::LocalRuntimeModelInvalid)
        ));
    }

    #[tokio::test(flavor = "current_thread")]
    async fn full_content_recheck_keeps_executor_responsive_and_honors_cancellation() {
        let directory = tempfile::tempdir().expect("verification directory");
        let path = directory.path().join("large-runtime.zip");
        let bytes = vec![0x41; 128 * 1024 + 1];
        fs::write(&path, &bytes).expect("large verification fixture");
        let lease = VerifiedFileLease::open(&path, bytes.len() as u64).expect("artifact lease");
        let authority = TestRuntimeAuthority::new().await;
        let cancellation = CancellationToken::new();
        let worker_cancellation = cancellation.clone();
        let control = authority.verification_control(worker_cancellation);
        let (started_tx, started_rx) = oneshot::channel();
        let (release_tx, release_rx) = std_mpsc::channel();
        let verification = tokio::spawn(run_owned_blocking(lease, move |lease| {
            let mut started_tx = Some(started_tx);
            lease
                .verify_unchanged_with(|| {
                    if let Some(started_tx) = started_tx.take() {
                        let _ = started_tx.send(());
                        release_rx
                            .recv_timeout(Duration::from_secs(2))
                            .map_err(|_| {
                                std::io::Error::new(
                                    std::io::ErrorKind::TimedOut,
                                    "test release was not signalled",
                                )
                            })?;
                    }
                    control.checkpoint()
                })
                .map_err(|error_value| {
                    control.classify(error_value, ArtifactVerificationStage::PreSpawn)
                })
        }));

        tokio::time::timeout(Duration::from_secs(5), started_rx)
            .await
            .expect("blocking recheck did not reach its bounded synchronization point")
            .expect("blocking recheck start signal");
        tokio::time::timeout(Duration::from_secs(2), tokio::task::yield_now())
            .await
            .expect("executor heartbeat while recheck is blocked");
        cancellation.cancel();
        release_tx.send(()).expect("release blocking recheck");

        let (_lease, result) = verification.await.expect("verification task");
        assert!(matches!(result, Err(ProviderError::LocalRuntimeCancelled)));
    }

    #[tokio::test(flavor = "current_thread")]
    async fn full_content_recheck_honors_authority_retirement_while_active() {
        let directory = tempfile::tempdir().expect("verification directory");
        let path = directory.path().join("large-model.gguf");
        let bytes = vec![0x42; 128 * 1024 + 1];
        fs::write(&path, &bytes).expect("large verification fixture");
        let lease = ModelFileLease::open(&path, bytes.len() as u64).expect("model lease");
        let authority = TestRuntimeAuthority::new().await;
        let control = authority.verification_control(CancellationToken::new());
        let (started_tx, started_rx) = oneshot::channel();
        let (release_tx, release_rx) = std_mpsc::channel();

        let verification = tokio::spawn(run_owned_blocking(lease, move |lease| {
            let mut started_tx = Some(started_tx);
            lease
                .verify_unchanged_with(|| {
                    if let Some(started_tx) = started_tx.take() {
                        let _ = started_tx.send(());
                        release_rx
                            .recv_timeout(Duration::from_secs(2))
                            .map_err(|_| {
                                std::io::Error::new(
                                    std::io::ErrorKind::TimedOut,
                                    "test release was not signalled",
                                )
                            })?;
                    }
                    control.checkpoint()
                })
                .map_err(|error_value| {
                    control.classify(error_value, ArtifactVerificationStage::PreSpawn)
                })
        }));

        started_rx.await.expect("blocking recheck started");
        authority
            .broker
            .retire(&CancellationToken::new())
            .await
            .expect("authority retirement");
        release_tx.send(()).expect("release blocking recheck");

        let (_lease, result) = verification.await.expect("verification task");
        assert!(matches!(
            result,
            Err(ProviderError::LocalRuntimeApprovalStale)
        ));
    }

    #[test]
    fn selected_model_must_be_one_printable_gguf_leaf_beneath_models_dir() {
        let directory = tempfile::tempdir().expect("models directory");
        let models = directory.path().join("models");
        fs::create_dir(&models).expect("create models directory");
        fs::write(models.join("first.gguf"), b"first").expect("first model");
        fs::write(models.join("second.gguf"), b"second").expect("second model");

        let first = resolve_selected_model(&models, "first.gguf").expect("first model path");
        let second = resolve_selected_model(&models, "second.gguf").expect("second model path");
        assert_eq!(
            first.file_name().and_then(|value| value.to_str()),
            Some("first.gguf")
        );
        assert_eq!(
            second.file_name().and_then(|value| value.to_str()),
            Some("second.gguf")
        );
        for rejected in [
            "../first.gguf",
            "sub/first.gguf",
            "sub\\first.gguf",
            "C:\\model.gguf",
            ".",
            "model.bin",
            "bad\nmodel.gguf",
        ] {
            assert!(
                resolve_selected_model(&models, rejected).is_err(),
                "accepted {rejected:?}"
            );
        }
    }

    #[test]
    fn llama_server_arguments_are_fixed_gpu_first_and_private() {
        let arguments = llama_server_arguments(
            Path::new(r"C:\models\selected.gguf"),
            Path::new(r"C:\runtime\api-key.txt"),
            43123,
            "/argentum/generation-7/epoch",
        );
        let joined = arguments.join(" ");
        for required in [
            "--model",
            r"C:\models\selected.gguf",
            "--host",
            "127.0.0.1",
            "--port",
            "43123",
            "--api-prefix",
            "/argentum/generation-7/epoch",
            "--api-key-file",
            r"C:\runtime\api-key.txt",
            "--ctx-size",
            "8192",
            "--n-gpu-layers",
            "99",
            "--predict",
            "-1",
            "--jinja",
            "--no-context-shift",
            "--reasoning",
            "off",
            "--no-webui",
            "--no-slots",
            "--offline",
        ] {
            assert!(
                arguments.iter().any(|value| value == required),
                "missing {required}"
            );
        }
        for forbidden in [
            "--tools", "--agent", "--mcp", "--media", "--router", "0.0.0.0",
        ] {
            assert!(!joined.contains(forbidden), "included {forbidden}");
        }
    }

    #[test]
    fn context_variants_change_only_the_fixed_context_argument_and_plan_digest() {
        let model = Path::new(r"C:\models\fixture.gguf");
        let key = Path::new(r"C:\runtime\key.txt");
        let baseline = llama_server_arguments(model, key, 43123, "/fixture");
        let context_index = baseline.iter().position(|v| v == "--ctx-size").unwrap() + 1;
        for size in crate::SIDECAR_CONTEXT_VARIANTS {
            let mut actual =
                llama_server_arguments_with_context(model, key, 43123, "/fixture", size);
            assert_eq!(actual[context_index], size.to_string());
            actual[context_index] = baseline[context_index].clone();
            assert_eq!(actual, baseline);
            let mut plan = LocalRuntimeLaunchPlan::test_fixture();
            let old = plan.digest();
            plan.context_tokens = size;
            assert_eq!(plan.digest() == old, size == LOCAL_RUNTIME_CONTEXT);
        }
    }

    #[tokio::test]
    async fn context_variants_require_new_approval_for_same_model_and_retire_old_lease() {
        let authority = TestRuntimeAuthority::new().await;
        let launcher = Arc::new(FakeLauncher::new());
        let module = fake_module(Arc::clone(&launcher), Arc::clone(&authority.broker));
        let profile = local_profile("first.gguf");
        let prepared = module.prepare(&profile).await.unwrap();
        let approval = authority.approval(prepared.plan());
        let old = module
            .activate(prepared, Some(approval), CancellationToken::new())
            .await
            .unwrap();
        let prepared = module.prepare_with_context(&profile, 32768).await.unwrap();
        assert!(matches!(
            module
                .activate(prepared, None, CancellationToken::new())
                .await,
            Err(ProviderError::LocalRuntimeApprovalRequired)
        ));
        assert_eq!(launcher.launches.load(Ordering::SeqCst), 1);
        assert!(old.verify_lease().await.is_ok());
        let prepared = module.prepare_with_context(&profile, 32768).await.unwrap();
        let approval = authority.approval(prepared.plan());
        let current = module
            .activate(prepared, Some(approval), CancellationToken::new())
            .await
            .unwrap();
        assert_eq!(current.context_tokens(), 32768);
        assert_eq!(launcher.launches.load(Ordering::SeqCst), 2);
        assert!(old.verify_lease().await.is_err());
        assert!(module.prepare_with_context(&profile, 8193).await.is_err());
        module.shutdown(CancellationToken::new()).await.unwrap();
    }

    #[tokio::test]
    async fn runtime_session_cleanup_removes_private_files_and_extraction() {
        let parent = tempfile::tempdir().expect("session parent");
        let session = tempfile::Builder::new()
            .prefix(".argentum-runtime-test-")
            .tempdir_in(parent.path())
            .expect("runtime session");
        let session_path = session.path().to_path_buf();
        let key = PrivateFile::create_new(session.path().join("api-key.txt"), b"test-key")
            .expect("private key file");
        fs::write(session.path().join("runtime.dll"), b"runtime").expect("runtime file");

        drop(key);
        remove_runtime_session(session)
            .await
            .expect("runtime session cleanup");

        assert!(!session_path.exists());
    }

    #[cfg(windows)]
    #[tokio::test]
    async fn injected_pre_spawn_verification_failure_removes_session_without_starting_child() {
        let directory = tempfile::tempdir().expect("launch failure directory");
        let spawned = directory.path().join("spawned.txt");
        let script = format!(
            "Set-Content -LiteralPath '{}' -Value spawned; Start-Sleep -Seconds 30",
            powershell_literal(&spawned)
        );
        let process_spec = powershell_test_spec(&script, directory.path());
        let (prepared, session_path) = production_failure_fixture(
            directory.path(),
            process_spec,
            Some(TestArtifactVerificationFailure::PreSpawn),
            false,
            None,
        );
        let authority = TestRuntimeAuthority::new().await;

        let result = LlamaCppSidecarLauncher
            .launch(prepared, authority.binding(), &CancellationToken::new())
            .await;

        assert!(matches!(
            result,
            Err(ProviderError::LocalRuntimeApprovalStale)
        ));
        assert!(!spawned.exists(), "pre-spawn failure started the child");
        assert!(!session_path.exists(), "pre-spawn failure retained session");
    }

    #[cfg(windows)]
    #[tokio::test]
    async fn spawn_failure_explicitly_removes_prepared_session() {
        let directory = tempfile::tempdir().expect("spawn failure directory");
        let process_spec = ProcessSpec::argv(
            directory
                .path()
                .join("missing-launcher.exe")
                .display()
                .to_string(),
            Vec::<String>::new(),
            directory.path(),
        )
        .expect("missing launcher process spec");
        let (prepared, session_path) =
            production_failure_fixture(directory.path(), process_spec, None, false, None);
        let authority = TestRuntimeAuthority::new().await;

        let result = LlamaCppSidecarLauncher
            .launch(prepared, authority.binding(), &CancellationToken::new())
            .await;

        assert!(matches!(
            result,
            Err(ProviderError::LocalRuntimeLaunchFailed { stage: "spawn" })
        ));
        assert!(!session_path.exists(), "spawn failure retained session");
    }

    #[cfg(windows)]
    #[tokio::test]
    async fn injected_post_resume_verification_failure_terminates_child_and_removes_session() {
        let directory = tempfile::tempdir().expect("post-resume failure directory");
        let started = directory.path().join("started.txt");
        let survived = directory.path().join("survived.txt");
        let script = format!(
            "Set-Content -LiteralPath '{}' -Value started; Start-Sleep -Milliseconds 750; Set-Content -LiteralPath '{}' -Value survived; Start-Sleep -Seconds 30",
            powershell_literal(&started),
            powershell_literal(&survived)
        );
        let process_spec = powershell_test_spec(&script, directory.path());
        let (prepared, session_path) = production_failure_fixture(
            directory.path(),
            process_spec,
            Some(TestArtifactVerificationFailure::PostResume),
            false,
            None,
        );
        let authority = TestRuntimeAuthority::new().await;

        let result = LlamaCppSidecarLauncher
            .launch(prepared, authority.binding(), &CancellationToken::new())
            .await;

        assert!(matches!(
            result,
            Err(ProviderError::LocalRuntimeApprovalStale)
        ));
        assert!(
            !session_path.exists(),
            "post-resume failure retained session"
        );
        tokio::time::sleep(Duration::from_secs(1)).await;
        assert!(!survived.exists(), "post-resume child survived cleanup");
    }

    #[cfg(windows)]
    #[tokio::test]
    async fn post_resume_failure_cleanup_error_takes_precedence_after_child_start_and_stops_survival(
    ) {
        let directory = tempfile::tempdir().expect("combined cleanup failure directory");
        let started = directory.path().join("started.txt");
        let allow_survival = directory.path().join("allow-survival.txt");
        let survived = directory.path().join("survived.txt");
        let script = format!(
            "Set-Content -LiteralPath '{}' -Value started; while (-not (Test-Path -LiteralPath '{}')) {{ Start-Sleep -Milliseconds 10 }}; Set-Content -LiteralPath '{}' -Value survived; Start-Sleep -Seconds 30",
            powershell_literal(&started),
            powershell_literal(&allow_survival),
            powershell_literal(&survived)
        );
        let process_spec = powershell_test_spec(&script, directory.path());
        let (post_resume_started_tx, post_resume_started_rx) = std_mpsc::channel();
        let (prepared, session_path) = production_failure_fixture(
            directory.path(),
            process_spec,
            Some(TestArtifactVerificationFailure::PostResume),
            true,
            Some(TestPostResumeVerificationHook {
                started_marker: started.clone(),
                observed_tx: post_resume_started_tx,
            }),
        );
        let authority = TestRuntimeAuthority::new().await;

        let result = LlamaCppSidecarLauncher
            .launch(prepared, authority.binding(), &CancellationToken::new())
            .await;

        post_resume_started_rx
            .recv_timeout(Duration::from_secs(2))
            .expect("child reached the started marker before post-resume failure injection");
        assert!(
            started.is_file(),
            "post-resume child never reached the started marker"
        );
        assert!(matches!(
            result,
            Err(ProviderError::LocalRuntimeLaunchFailed {
                stage: "session-cleanup"
            })
        ));
        assert!(
            session_path.is_dir(),
            "session was retained for a reason other than the injected cleanup failure"
        );

        fs::write(&allow_survival, b"continue").expect("allow delayed survival marker");
        let survival_marker = async {
            loop {
                if survived.exists() {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        };
        assert!(
            tokio::time::timeout(Duration::from_millis(500), survival_marker)
                .await
                .is_err(),
            "post-resume child reached its delayed survival marker after launch returned"
        );

        assert!(
            session_path.starts_with(directory.path()),
            "retained session escaped its test fixture root"
        );
        fs::remove_dir_all(&session_path).expect("remove intentionally retained test session");
        assert!(
            !session_path.exists(),
            "intentionally retained test session cleanup did not complete"
        );
    }

    #[cfg(windows)]
    #[tokio::test]
    async fn cleanup_failure_replaces_pre_spawn_error_with_typed_cleanup_error() {
        let directory = tempfile::tempdir().expect("cleanup failure directory");
        let process_spec = powershell_test_spec("Start-Sleep -Seconds 30", directory.path());
        let (prepared, session_path) = production_failure_fixture(
            directory.path(),
            process_spec,
            Some(TestArtifactVerificationFailure::PreSpawn),
            true,
            None,
        );
        let authority = TestRuntimeAuthority::new().await;

        let result = LlamaCppSidecarLauncher
            .launch(prepared, authority.binding(), &CancellationToken::new())
            .await;

        assert!(matches!(
            result,
            Err(ProviderError::LocalRuntimeLaunchFailed {
                stage: "session-cleanup"
            })
        ));
        assert!(session_path.exists(), "cleanup failure was not injected");
        fs::remove_dir_all(&session_path).expect("remove retained injected-failure session");
    }

    #[tokio::test]
    async fn local_runtime_authority_ticket_must_match_the_exact_plan_and_action() {
        let (issuer, verifier, generation) = host_authority_channel().expect("authority channel");
        let broker = Arc::new(
            PlatformAuthorityBroker::bootstrap(generation.clone())
                .await
                .expect("platform authority broker"),
        );
        let launcher = Arc::new(FakeLauncher::new());
        let module = fake_module(Arc::clone(&launcher), broker);
        let prepared = module
            .prepare(&local_profile("first.gguf"))
            .await
            .expect("prepared launch");
        let action = LocalAuthorityAction::ActivateLocalRuntime {
            plan: prepared.plan().digest(),
        };

        let mut other_plan = prepared.plan().clone();
        other_plan.port = other_plan.port.saturating_add(1);
        let other_plan_action = LocalAuthorityAction::ActivateLocalRuntime {
            plan: other_plan.digest(),
        };
        let wrong_plan_ticket = issuer
            .issue(other_plan_action, generation.clone())
            .expect("other plan ticket");
        assert!(matches!(
            verifier.consume(wrong_plan_ticket, &action, generation.clone()),
            Err(SecurityError::LocalAuthorityActionMismatch)
        ));

        let wrong_action_ticket = issuer
            .issue(
                LocalAuthorityAction::EnableCapability {
                    capability_id: "execution.shell".into(),
                },
                generation.clone(),
            )
            .expect("other action ticket");
        assert!(matches!(
            verifier.consume(wrong_action_ticket, &action, generation.clone()),
            Err(SecurityError::LocalAuthorityActionMismatch)
        ));

        let ticket = issuer
            .issue(action.clone(), generation.clone())
            .expect("runtime ticket");
        let proof = verifier
            .consume(ticket, &action, generation)
            .expect("verified runtime authority");
        let approval = LocalRuntimeLaunchApproval::bind(prepared.plan(), proof)
            .expect("bound launch approval");
        assert!(approval.matches(prepared.plan()));
        assert_eq!(launcher.launches.load(Ordering::SeqCst), 0);
        assert!(module.lifecycle_snapshot().await.modules().is_empty());
    }

    #[tokio::test]
    async fn local_runtime_authority_prior_generation_fails_before_approval_or_launch() {
        let (issuer, verifier, generation) = host_authority_channel().expect("authority channel");
        let broker = Arc::new(
            PlatformAuthorityBroker::bootstrap(generation.clone())
                .await
                .expect("platform authority broker"),
        );
        let launcher = Arc::new(FakeLauncher::new());
        let module = fake_module(Arc::clone(&launcher), broker);
        let prepared = module
            .prepare(&local_profile("first.gguf"))
            .await
            .expect("prepared launch");
        let action = LocalAuthorityAction::ActivateLocalRuntime {
            plan: prepared.plan().digest(),
        };
        let ticket = issuer
            .issue(action.clone(), generation.clone())
            .expect("runtime ticket");
        let successor = generation.successor().expect("successor generation");

        assert!(matches!(
            verifier.consume(ticket, &action, successor),
            Err(SecurityError::LocalAuthorityGenerationMismatch)
        ));
        assert_eq!(launcher.launches.load(Ordering::SeqCst), 0);
        assert!(module.lifecycle_snapshot().await.modules().is_empty());
    }

    #[tokio::test]
    async fn local_runtime_authority_expired_ticket_fails_before_approval_or_launch() {
        let (issuer, verifier, generation) = host_authority_channel().expect("authority channel");
        let broker = Arc::new(
            PlatformAuthorityBroker::bootstrap(generation.clone())
                .await
                .expect("platform authority broker"),
        );
        let launcher = Arc::new(FakeLauncher::new());
        let module = fake_module(Arc::clone(&launcher), broker);
        let prepared = module
            .prepare(&local_profile("first.gguf"))
            .await
            .expect("prepared launch");
        let action = LocalAuthorityAction::ActivateLocalRuntime {
            plan: prepared.plan().digest(),
        };
        let ticket = issuer
            .issue(action.clone(), generation.clone())
            .expect("runtime ticket");

        std::thread::sleep(Duration::from_secs(30));

        assert!(matches!(
            verifier.consume(ticket, &action, generation),
            Err(SecurityError::LocalAuthorityExpired)
        ));
        assert_eq!(launcher.launches.load(Ordering::SeqCst), 0);
        assert!(module.lifecycle_snapshot().await.modules().is_empty());
    }

    struct FakePreparedRuntime {
        listener: std::net::TcpListener,
    }

    struct FakeRuntimeIdentity {
        active: AtomicBool,
        pid: AtomicU32,
        process_started_at: AtomicU64,
        listener_port: AtomicU16,
        model_sha256: StdMutex<String>,
    }

    struct FakeLeaseVerifier {
        identity: Arc<FakeRuntimeIdentity>,
        expected_port: u16,
        authority: LocalRuntimeAuthority,
    }

    #[async_trait]
    impl LeaseVerifier for FakeLeaseVerifier {
        async fn verify(&self, lease: &SidecarLease) -> Result<(), ProviderError> {
            if !self.authority.is_current()
                || !self
                    .authority
                    .lease()
                    .matches_generation(&lease.authority_generation)
            {
                return Err(ProviderError::RequestCancelled);
            }
            if lease.authority_generation != *self.authority.generation()
                || lease.pid != self.identity.pid.load(Ordering::SeqCst)
                || lease.process_started_at
                    != self.identity.process_started_at.load(Ordering::SeqCst)
                || self.expected_port != self.identity.listener_port.load(Ordering::SeqCst)
                || lease.model_sha256
                    != *self
                        .identity
                        .model_sha256
                        .lock()
                        .map_err(|_| ProviderError::LocalRuntimeLeaseStale)?
            {
                return Err(ProviderError::LocalRuntimeLeaseStale);
            }
            if self.identity.active.load(Ordering::SeqCst) {
                Ok(())
            } else {
                Err(ProviderError::LocalRuntimeLeaseStale)
            }
        }
    }

    struct FakeRuntimeInstance {
        identity: Arc<FakeRuntimeIdentity>,
        fail_next_stop: Arc<AtomicBool>,
        retirement_order: Arc<StdMutex<Vec<&'static str>>>,
        shutdown: Option<oneshot::Sender<()>>,
        task: Option<tokio::task::JoinHandle<()>>,
    }

    #[async_trait]
    impl RuntimeInstance for FakeRuntimeInstance {
        async fn stop(&mut self) -> Result<(), ProviderError> {
            if self.fail_next_stop.swap(false, Ordering::SeqCst) {
                return Err(ProviderError::LocalRuntimeLaunchFailed {
                    stage: "fake-teardown",
                });
            }
            self.retirement_order
                .lock()
                .expect("retirement order")
                .push("provider.local-runtime");
            self.identity.active.store(false, Ordering::SeqCst);
            if let Some(shutdown) = self.shutdown.take() {
                let _ = shutdown.send(());
            }
            if let Some(task) = self.task.take() {
                task.await
                    .map_err(|_| ProviderError::LocalRuntimeLaunchFailed {
                        stage: "fake-teardown",
                    })?;
            }
            Ok(())
        }
    }

    struct FakeLauncher {
        launches: Arc<AtomicUsize>,
        fail_next_stop: Arc<AtomicBool>,
        retirement_order: Arc<StdMutex<Vec<&'static str>>>,
        latest_identity: Arc<StdMutex<Option<Arc<FakeRuntimeIdentity>>>>,
    }

    impl FakeLauncher {
        fn new() -> Self {
            Self {
                launches: Arc::new(AtomicUsize::new(0)),
                fail_next_stop: Arc::new(AtomicBool::new(false)),
                retirement_order: Arc::new(StdMutex::new(Vec::new())),
                latest_identity: Arc::new(StdMutex::new(None)),
            }
        }
    }

    #[async_trait]
    impl LocalRuntimeLauncher for FakeLauncher {
        async fn prepare(
            &self,
            config: &LocalRuntimeConfig,
            profile: &ProviderProfile,
            generation: u64,
        ) -> Result<PreparedLocalRuntime, ProviderError> {
            validate_local_profile(profile)?;
            let listener = std::net::TcpListener::bind((LOCAL_RUNTIME_HOST, 0)).map_err(|_| {
                ProviderError::LocalRuntimeLaunchFailed {
                    stage: "fake-prepare",
                }
            })?;
            listener.set_nonblocking(true).map_err(|_| {
                ProviderError::LocalRuntimeLaunchFailed {
                    stage: "fake-prepare",
                }
            })?;
            let port = listener
                .local_addr()
                .map_err(|_| ProviderError::LocalRuntimeLaunchFailed {
                    stage: "fake-prepare",
                })?
                .port();
            let mut plan = LocalRuntimeLaunchPlan::test_fixture();
            plan.context_tokens = config.context_tokens;
            plan.model = profile.model.clone();
            plan.model_path = PathBuf::from(format!(r"C:\models\{}", profile.model));
            plan.generation = generation;
            plan.epoch = format!("fake-{generation}");
            plan.port = port;
            plan.api_prefix = format!("/argentum/generation-{generation}/fake");
            Ok(PreparedLocalRuntime {
                plan,
                resources: Some(Box::new(FakePreparedRuntime { listener })),
            })
        }

        async fn launch(
            &self,
            mut prepared: PreparedLocalRuntime,
            authority: LocalRuntimeAuthority,
            cancellation: &CancellationToken,
        ) -> Result<LaunchedRuntime, ProviderError> {
            if cancellation.is_cancelled() {
                return Err(ProviderError::LocalRuntimeCancelled);
            }
            let resources = prepared
                .resources
                .take()
                .ok_or(ProviderError::LocalRuntimeApprovalStale)?
                .downcast::<FakePreparedRuntime>()
                .map_err(|_| ProviderError::LocalRuntimeApprovalStale)?;
            let listener = tokio::net::TcpListener::from_std(resources.listener).map_err(|_| {
                ProviderError::LocalRuntimeLaunchFailed {
                    stage: "fake-launch",
                }
            })?;
            let (shutdown_tx, mut shutdown_rx) = oneshot::channel();
            let task = tokio::spawn(async move {
                loop {
                    let accepted = tokio::select! {
                        _ = &mut shutdown_rx => break,
                        accepted = listener.accept() => accepted,
                    };
                    let Ok((mut stream, _)) = accepted else {
                        break;
                    };
                    tokio::spawn(async move {
                        let mut request = vec![0_u8; 16 * 1024];
                        let read = stream.read(&mut request).await.unwrap_or_default();
                        let request_text = String::from_utf8_lossy(&request[..read]);
                        if request_text.contains("chat/completions") {
                            let headers = "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nCache-Control: no-cache\r\nConnection: close\r\n\r\n";
                            let first = "data: {\"choices\":[{\"delta\":{\"content\":\"first\"},\"finish_reason\":null}]}\n\n";
                            let _ = stream.write_all(headers.as_bytes()).await;
                            let _ = stream.write_all(first.as_bytes()).await;
                            let _ = stream.flush().await;
                            tokio::time::sleep(Duration::from_secs(2)).await;
                            let second = "data: {\"choices\":[{\"delta\":{\"content\":\"late\"},\"finish_reason\":\"stop\"}]}\n\ndata: [DONE]\n\n";
                            let _ = stream.write_all(second.as_bytes()).await;
                            let _ = stream.shutdown().await;
                            return;
                        }
                        let body = r#"{"data":[{"id":"fake.gguf"}]}"#;
                        let response = format!(
                            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                            body.len(),
                            body
                        );
                        let _ = stream.write_all(response.as_bytes()).await;
                        let _ = stream.shutdown().await;
                    });
                }
            });
            let endpoint = format!("http://{LOCAL_RUNTIME_HOST}:{}/v1/", prepared.plan.port);
            let provider = Arc::new(OpenAiCompatibleProvider::new(
                endpoint,
                &prepared.plan.model,
                Some(SecretValue::new("fake-key")),
                ProviderKind::LocalLlamaCpp,
                "llama.cpp",
            )?);
            provider.list_models().await?;
            self.launches.fetch_add(1, Ordering::SeqCst);
            let identity = Arc::new(FakeRuntimeIdentity {
                active: AtomicBool::new(true),
                pid: AtomicU32::new(std::process::id()),
                process_started_at: AtomicU64::new(prepared.plan.generation),
                listener_port: AtomicU16::new(prepared.plan.port),
                model_sha256: StdMutex::new(prepared.plan.model_sha256.clone()),
            });
            *self.latest_identity.lock().expect("latest fake identity") =
                Some(Arc::clone(&identity));
            let client = Arc::new(LocalRuntimeClient {
                context_tokens: prepared.plan.context_tokens,
                model: prepared.plan.model,
                provider,
                lease: Arc::new(SidecarLease {
                    epoch: prepared.plan.epoch,
                    generation: prepared.plan.generation,
                    authority_generation: authority.generation().clone(),
                    model_sha256: prepared.plan.model_sha256,
                    pid: identity.pid.load(Ordering::SeqCst),
                    process_started_at: identity.process_started_at.load(Ordering::SeqCst),
                }),
                verifier: Arc::new(FakeLeaseVerifier {
                    identity: Arc::clone(&identity),
                    expected_port: prepared.plan.port,
                    authority,
                }),
            });
            Ok(LaunchedRuntime {
                client,
                instance: Box::new(FakeRuntimeInstance {
                    identity,
                    fail_next_stop: Arc::clone(&self.fail_next_stop),
                    retirement_order: Arc::clone(&self.retirement_order),
                    shutdown: Some(shutdown_tx),
                    task: Some(task),
                }),
            })
        }
    }

    fn local_profile(model: &str) -> ProviderProfile {
        ProviderProfile {
            id: "llama-cpp".into(),
            label: "llama.cpp".into(),
            kind: ProviderKind::LocalLlamaCpp,
            endpoint: "local://llama-cpp/".into(),
            model: model.into(),
            selected: true,
        }
    }

    #[cfg(windows)]
    fn powershell_literal(path: &Path) -> String {
        path.to_string_lossy().replace('\'', "''")
    }

    #[cfg(windows)]
    fn powershell_test_spec(script: &str, working_directory: &Path) -> ProcessSpec {
        ProcessSpec::argv(
            "powershell.exe",
            [
                "-NoLogo".to_owned(),
                "-NoProfile".to_owned(),
                "-NonInteractive".to_owned(),
                "-Command".to_owned(),
                script.to_owned(),
            ],
            working_directory,
        )
        .expect("PowerShell process spec")
    }

    #[cfg(windows)]
    fn production_failure_fixture(
        root: &Path,
        process_spec: ProcessSpec,
        verification_failure: Option<TestArtifactVerificationFailure>,
        fail_session_cleanup: bool,
        post_resume_verification_hook: Option<TestPostResumeVerificationHook>,
    ) -> (PreparedLocalRuntime, PathBuf) {
        let archive_path = root.join("runtime.zip");
        let model_path = root.join("model.gguf");
        fs::write(&archive_path, b"runtime archive").expect("archive fixture");
        fs::write(&model_path, b"GGUF model").expect("model fixture");
        let session = tempfile::Builder::new()
            .prefix(".argentum-runtime-failure-")
            .tempdir_in(root)
            .expect("runtime failure session");
        let session_path = session.path().to_path_buf();
        let launcher_path = session.path().join("llama-server.exe");
        fs::write(&launcher_path, b"runtime launcher").expect("launcher fixture");
        let key_path = session.path().join("api-key.txt");
        let key_file =
            PrivateFile::create_new(&key_path, b"test-private-key").expect("private key fixture");
        let archive_lease =
            VerifiedFileLease::open(&archive_path, 1024).expect("archive lease fixture");
        let launcher_lease =
            VerifiedFileLease::open(&launcher_path, 1024).expect("launcher lease fixture");
        let model_lease =
            ModelFileLease::open(&model_path, 1024 * 1024).expect("model lease fixture");
        let process_limits =
            ManagedProcessLimits::for_local_model(model_lease.size_bytes()).expect("limits");
        let environment_sha256 = process_spec.environment_binding();
        let plan = LocalRuntimeLaunchPlan {
            runtime_id: "test-runtime".into(),
            runtime_version: "1".into(),
            runtime_bundle_sha256: archive_lease.sha256().to_owned(),
            executable_sha256: launcher_lease.sha256().to_owned(),
            program: launcher_lease.path().to_path_buf(),
            arguments: vec!["test".into()],
            environment_sha256: environment_sha256.clone(),
            model: "model.gguf".into(),
            model_path: model_lease.path().to_path_buf(),
            model_sha256: model_lease.sha256().to_owned(),
            model_size_bytes: model_lease.size_bytes(),
            max_model_bytes: 1024 * 1024,
            managed_active_process_limit: process_limits.active_process_limit(),
            managed_cpu_rate_percent: process_limits.cpu_rate_percent(),
            managed_process_memory_bytes: process_limits.process_memory_bytes(),
            managed_job_memory_bytes: process_limits.job_memory_bytes(),
            managed_output_hash_limit: process_limits.output_hash_limit(),
            generation: 1,
            epoch: "test-epoch".into(),
            host: LOCAL_RUNTIME_HOST.into(),
            port: 43_123,
            api_prefix: "/test".into(),
            context_tokens: LOCAL_RUNTIME_CONTEXT,
            gpu_layers: LOCAL_RUNTIME_GPU_LAYERS,
            disabled_features: disabled_runtime_features(),
        };
        (
            PreparedLocalRuntime {
                plan,
                resources: Some(Box::new(ProductionPreparedRuntime {
                    process_spec,
                    process_limits,
                    environment_sha256,
                    reserved_listener: None,
                    key_file: Some(key_file),
                    session: Some(session),
                    api_key: SecretValue::new("test-private-key"),
                    archive_lease: Some(archive_lease),
                    launcher_lease: Some(launcher_lease),
                    model_lease: Some(model_lease),
                    max_model_bytes: 1024 * 1024,
                    endpoint: "http://127.0.0.1:43123/test/v1/".into(),
                    verification_failure,
                    fail_session_cleanup,
                    post_resume_verification_hook,
                })),
            },
            session_path,
        )
    }

    fn fake_module(
        launcher: Arc<FakeLauncher>,
        broker: Arc<PlatformAuthorityBroker>,
    ) -> LocalRuntimeModule {
        let config = LocalRuntimeConfig::new(
            PathBuf::from(r"C:\argentum-test\bundle.zip"),
            PathBuf::from(r"C:\argentum-test\models"),
        )
        .expect("fake config");
        LocalRuntimeModule::with_launcher(config, broker, launcher)
    }

    struct TestRuntimeAuthority {
        issuer: LocalAuthorityIssuer,
        verifier: LocalAuthorityVerifier,
        broker: Arc<PlatformAuthorityBroker>,
    }

    impl TestRuntimeAuthority {
        async fn new() -> Self {
            let (issuer, verifier, generation) =
                host_authority_channel().expect("authority channel");
            let broker = Arc::new(
                PlatformAuthorityBroker::bootstrap(generation.clone())
                    .await
                    .expect("platform authority broker"),
            );
            Self {
                issuer,
                verifier,
                broker,
            }
        }

        fn approval(&self, plan: &LocalRuntimeLaunchPlan) -> LocalRuntimeLaunchApproval {
            let action = LocalAuthorityAction::ActivateLocalRuntime {
                plan: plan.digest(),
            };
            let generation = self.broker.current_generation().expect("current authority");
            let ticket = self
                .issuer
                .issue(action.clone(), generation.clone())
                .expect("runtime ticket");
            let proof = self
                .verifier
                .consume(ticket, &action, generation)
                .expect("verified runtime authority");
            LocalRuntimeLaunchApproval::bind(plan, proof).expect("bound approval")
        }

        fn binding(&self) -> LocalRuntimeAuthority {
            LocalRuntimeAuthority::new(
                Arc::clone(&self.broker),
                self.broker
                    .current_generation()
                    .expect("current authority generation"),
                self.broker
                    .current_lease()
                    .expect("current authority lease"),
            )
        }

        fn verification_control(
            &self,
            cancellation: CancellationToken,
        ) -> ArtifactVerificationControl {
            ArtifactVerificationControl {
                authority: self.binding(),
                cancellation,
            }
        }
    }

    struct BlockingRetirement {
        started: Arc<Notify>,
        proceed: Arc<Notify>,
    }

    #[async_trait]
    impl AuthorityDependentService for BlockingRetirement {
        fn service_id(&self) -> &str {
            "provider.blocking-retirement"
        }

        async fn retire(&mut self, _cancellation: &CancellationToken) -> Result<(), PlatformError> {
            self.started.notify_one();
            self.proceed.notified().await;
            Ok(())
        }
    }

    struct BurstProvider {
        accepted: Arc<Notify>,
        events: usize,
    }

    #[async_trait]
    impl ModelProvider for BurstProvider {
        fn id(&self) -> &'static str {
            "burst-local-runtime"
        }

        fn status(&self) -> argentum_domain::ProviderStatus {
            argentum_domain::ProviderStatus {
                profile_id: "burst-local-runtime".into(),
                kind: ProviderKind::LocalLlamaCpp,
                label: "burst".into(),
                endpoint: "local://burst/".into(),
                connected: true,
                detail: "ready".into(),
            }
        }

        async fn stream(
            &self,
            _request: crate::ModelRequest,
            sender: mpsc::Sender<crate::ProviderEvent>,
        ) -> Result<(), ProviderError> {
            let accepted = Arc::clone(&self.accepted);
            let events = self.events;
            tokio::spawn(async move {
                for index in 0..events {
                    if sender
                        .send(crate::ProviderEvent::Delta(format!("event-{index}")))
                        .await
                        .is_err()
                    {
                        return;
                    }
                }
                accepted.notify_one();
            });
            Ok(())
        }
    }

    async fn active_fake_client() -> (
        Arc<LocalRuntimeClient>,
        Arc<FakeRuntimeIdentity>,
        Arc<FakeLauncher>,
        TestRuntimeAuthority,
    ) {
        let authority = TestRuntimeAuthority::new().await;
        let launcher = Arc::new(FakeLauncher::new());
        let module = fake_module(Arc::clone(&launcher), Arc::clone(&authority.broker));
        let prepared = module
            .prepare(&local_profile("first.gguf"))
            .await
            .expect("prepared launch");
        let approval = authority.approval(prepared.plan());
        let client = module
            .activate(prepared, Some(approval), CancellationToken::new())
            .await
            .expect("activation");
        let identity = launcher
            .latest_identity
            .lock()
            .expect("latest fake identity")
            .clone()
            .expect("launched fake identity");
        (client, identity, launcher, authority)
    }

    #[tokio::test]
    async fn independent_same_ordinal_authority_cannot_activate_the_bound_module() {
        let authority = TestRuntimeAuthority::new().await;
        let launcher = Arc::new(FakeLauncher::new());
        let module = fake_module(Arc::clone(&launcher), Arc::clone(&authority.broker));
        let prepared = module
            .prepare(&local_profile("first.gguf"))
            .await
            .expect("prepared launch");
        let (foreign_issuer, foreign_verifier, foreign_generation) =
            host_authority_channel().expect("independent authority channel");
        assert_eq!(
            foreign_generation.ordinal(),
            authority
                .broker
                .current_generation()
                .expect("bound generation")
                .ordinal()
        );
        let action = LocalAuthorityAction::ActivateLocalRuntime {
            plan: prepared.plan().digest(),
        };
        let ticket = foreign_issuer
            .issue(action.clone(), foreign_generation.clone())
            .expect("foreign ticket");
        let proof = foreign_verifier
            .consume(ticket, &action, foreign_generation)
            .expect("foreign proof");
        let approval = LocalRuntimeLaunchApproval::bind(prepared.plan(), proof)
            .expect("foreign bound approval");

        assert!(matches!(
            module
                .activate(prepared, Some(approval), CancellationToken::new())
                .await,
            Err(ProviderError::LocalRuntimeApprovalStale)
        ));
        assert_eq!(launcher.launches.load(Ordering::SeqCst), 0);
        assert!(module.lifecycle_snapshot().await.modules().is_empty());
        assert!(module.client("first.gguf").await.is_err());
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn retirement_winning_activation_admission_prevents_mount_launch_and_publication() {
        let authority = TestRuntimeAuthority::new().await;
        let launcher = Arc::new(FakeLauncher::new());
        let module = Arc::new(fake_module(
            Arc::clone(&launcher),
            Arc::clone(&authority.broker),
        ));
        let prepared = module
            .prepare(&local_profile("first.gguf"))
            .await
            .expect("prepared launch");
        let approval = authority.approval(prepared.plan());
        let started = Arc::new(Notify::new());
        let proceed = Arc::new(Notify::new());
        authority
            .broker
            .mount_dependent(
                "provider.blocking-retirement",
                BlockingRetirement {
                    started: Arc::clone(&started),
                    proceed: Arc::clone(&proceed),
                },
                &CancellationToken::new(),
            )
            .await
            .expect("blocking retirement service");

        let retirement = {
            let broker = Arc::clone(&authority.broker);
            tokio::spawn(async move { broker.retire(&CancellationToken::new()).await })
        };
        started.notified().await;
        let activation = {
            let module = Arc::clone(&module);
            tokio::spawn(async move {
                module
                    .activate(prepared, Some(approval), CancellationToken::new())
                    .await
            })
        };
        proceed.notify_one();

        retirement
            .await
            .expect("retirement task")
            .expect("retirement");
        assert!(matches!(
            activation.await.expect("activation task"),
            Err(ProviderError::LocalRuntimeApprovalStale)
        ));
        assert_eq!(launcher.launches.load(Ordering::SeqCst), 0);
        assert!(module.lifecycle_snapshot().await.modules().is_empty());
        assert!(module.client("first.gguf").await.is_err());
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn full_output_channel_cannot_publish_an_event_after_authority_retirement() {
        let authority = TestRuntimeAuthority::new().await;
        let generation = authority
            .broker
            .current_generation()
            .expect("authority generation");
        let platform_lease = authority.broker.current_lease().expect("platform lease");
        let identity = Arc::new(FakeRuntimeIdentity {
            active: AtomicBool::new(true),
            pid: AtomicU32::new(7),
            process_started_at: AtomicU64::new(11),
            listener_port: AtomicU16::new(43_123),
            model_sha256: StdMutex::new("d".repeat(64)),
        });
        let accepted = Arc::new(Notify::new());
        let authority_binding = LocalRuntimeAuthority::new(
            Arc::clone(&authority.broker),
            generation.clone(),
            platform_lease,
        );
        let client = Arc::new(LocalRuntimeClient {
            context_tokens: LOCAL_RUNTIME_CONTEXT,
            model: "first.gguf".into(),
            provider: Arc::new(BurstProvider {
                accepted: Arc::clone(&accepted),
                events: 33,
            }),
            lease: Arc::new(SidecarLease {
                epoch: "backpressure".into(),
                generation: 1,
                authority_generation: generation,
                model_sha256: "d".repeat(64),
                pid: 7,
                process_started_at: 11,
            }),
            verifier: Arc::new(FakeLeaseVerifier {
                identity,
                expected_port: 43_123,
                authority: authority_binding,
            }),
        });
        let provider = client.provider();
        let request = crate::ModelRequest {
            require_tool_call: false,
            continuation_scope: None,
            output_limit: 4096,
            model: "first.gguf".into(),
            system: None,
            history: Vec::new(),
            prompt: "hello".into(),
            tools: Vec::new(),
            tool_exchanges: Vec::new(),
        };
        let (sender, mut receiver) = mpsc::channel(1);
        sender
            .send(crate::ProviderEvent::Delta("occupied".into()))
            .await
            .expect("fill output channel");
        let stream = tokio::spawn(async move {
            provider
                .stream_with_cancellation(request, sender, CancellationToken::new())
                .await
        });

        tokio::time::timeout(Duration::from_secs(2), accepted.notified())
            .await
            .expect("inner forwarding reached full output channel");
        authority
            .broker
            .retire(&CancellationToken::new())
            .await
            .expect("authority retirement");
        assert_eq!(
            receiver.recv().await,
            Some(crate::ProviderEvent::Delta("occupied".into()))
        );
        assert!(matches!(
            stream.await.expect("stream task"),
            Err(ProviderError::RequestCancelled)
        ));
        assert!(
            receiver.try_recv().is_err(),
            "stale event escaped retirement"
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn full_outer_local_model_channel_cannot_publish_after_authority_retirement() {
        let authority = TestRuntimeAuthority::new().await;
        let generation = authority
            .broker
            .current_generation()
            .expect("authority generation");
        let platform_lease = authority.broker.current_lease().expect("platform lease");
        let identity = Arc::new(FakeRuntimeIdentity {
            active: AtomicBool::new(true),
            pid: AtomicU32::new(17),
            process_started_at: AtomicU64::new(19),
            listener_port: AtomicU16::new(43_124),
            model_sha256: StdMutex::new("f".repeat(64)),
        });
        let accepted = Arc::new(Notify::new());
        let client = Arc::new(LocalRuntimeClient {
            context_tokens: LOCAL_RUNTIME_CONTEXT,
            model: "outer.gguf".into(),
            provider: Arc::new(BurstProvider {
                accepted: Arc::clone(&accepted),
                events: 33,
            }),
            lease: Arc::new(SidecarLease {
                epoch: "outer-backpressure".into(),
                generation: 1,
                authority_generation: generation.clone(),
                model_sha256: "f".repeat(64),
                pid: 17,
                process_started_at: 19,
            }),
            verifier: Arc::new(FakeLeaseVerifier {
                identity,
                expected_port: 43_124,
                authority: LocalRuntimeAuthority::new(
                    Arc::clone(&authority.broker),
                    generation,
                    platform_lease,
                ),
            }),
        });
        let launcher = Arc::new(FakeLauncher::new());
        let module = Arc::new(fake_module(launcher, Arc::clone(&authority.broker)));
        {
            let mut state = module.state.lock().await;
            let binding = state.kernel.binding();
            let mounted = state
                .kernel
                .mount(
                    binding,
                    ModuleSpec::new(LOCAL_RUNTIME_MODULE_ID),
                    &CancellationToken::new(),
                    |_| -> MountFuture<'_> { Box::pin(async { Ok(()) }) },
                )
                .await
                .expect("mounted test client");
            state.mounted = Some(mounted);
            state.active = Some(client);
        }
        let provider =
            crate::LocalLlamaCppProvider::with_runtime("outer.gguf", Arc::clone(&module))
                .expect("outer local provider");
        let request = crate::ModelRequest {
            require_tool_call: false,
            continuation_scope: None,
            output_limit: 4096,
            model: "outer.gguf".into(),
            system: None,
            history: Vec::new(),
            prompt: "hello".into(),
            tools: Vec::new(),
            tool_exchanges: Vec::new(),
        };
        let (sender, mut receiver) = mpsc::channel(1);
        sender
            .send(crate::ProviderEvent::Delta("occupied".into()))
            .await
            .expect("fill output channel");
        let stream = tokio::spawn(async move {
            provider
                .stream_with_cancellation(request, sender, CancellationToken::new())
                .await
        });

        tokio::time::timeout(Duration::from_secs(2), accepted.notified())
            .await
            .expect("outer forwarding reached full output channel");
        authority
            .broker
            .retire(&CancellationToken::new())
            .await
            .expect("authority retirement");
        assert_eq!(
            receiver.recv().await,
            Some(crate::ProviderEvent::Delta("occupied".into()))
        );
        assert!(matches!(
            stream.await.expect("stream task"),
            Err(ProviderError::RequestCancelled)
        ));
        assert!(
            receiver.try_recv().is_err(),
            "outer stale event escaped retirement"
        );
    }

    #[tokio::test]
    async fn fake_verifier_rejects_process_identity_mismatch() {
        let (client, identity, _launcher, _authority) = active_fake_client().await;
        identity.pid.fetch_add(1, Ordering::SeqCst);
        assert!(matches!(
            client.verify_lease().await,
            Err(ProviderError::LocalRuntimeLeaseStale)
        ));
    }

    #[tokio::test]
    async fn fake_verifier_rejects_listener_identity_mismatch() {
        let (client, identity, _launcher, _authority) = active_fake_client().await;
        identity.listener_port.fetch_add(1, Ordering::SeqCst);
        assert!(matches!(
            client.verify_lease().await,
            Err(ProviderError::LocalRuntimeLeaseStale)
        ));
    }

    #[tokio::test]
    async fn fake_verifier_rejects_model_identity_mismatch() {
        let (client, identity, _launcher, _authority) = active_fake_client().await;
        *identity.model_sha256.lock().expect("model identity") = "e".repeat(64);
        assert!(matches!(
            client.verify_lease().await,
            Err(ProviderError::LocalRuntimeLeaseStale)
        ));
    }

    #[tokio::test]
    async fn fake_verifier_rejects_independent_full_authority_identity() {
        let (client, _identity, _launcher, _authority) = active_fake_client().await;
        let (_, _, foreign_generation) =
            host_authority_channel().expect("independent authority channel");
        assert_eq!(
            foreign_generation.ordinal(),
            client.lease().authority_generation.ordinal()
        );
        let foreign_lease = SidecarLease {
            epoch: client.lease().epoch.clone(),
            generation: client.lease().generation,
            authority_generation: foreign_generation,
            model_sha256: client.lease().model_sha256.clone(),
            pid: client.lease().pid,
            process_started_at: client.lease().process_started_at,
        };

        assert!(client.verifier.verify(&foreign_lease).await.is_err());
    }

    #[tokio::test]
    async fn missing_or_stale_approval_never_publishes_a_module() {
        let launcher = Arc::new(FakeLauncher::new());
        let authority = TestRuntimeAuthority::new().await;
        let module = fake_module(Arc::clone(&launcher), Arc::clone(&authority.broker));
        let profile = local_profile("first.gguf");

        let prepared = module.prepare(&profile).await.expect("prepared launch");
        assert!(matches!(
            module
                .activate(prepared, None, CancellationToken::new())
                .await,
            Err(ProviderError::LocalRuntimeApprovalRequired)
        ));
        assert!(module.lifecycle_snapshot().await.modules().is_empty());

        let mut prepared = module.prepare(&profile).await.expect("prepared launch");
        let approval = authority.approval(prepared.plan());
        prepared.plan.port = prepared.plan.port.saturating_add(1);
        assert!(matches!(
            module
                .activate(prepared, Some(approval), CancellationToken::new())
                .await,
            Err(ProviderError::LocalRuntimeApprovalStale)
        ));
        assert!(module.lifecycle_snapshot().await.modules().is_empty());
        assert_eq!(launcher.launches.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn lifecycle_reuses_switches_and_tears_down_real_loopback_fakes() {
        let launcher = Arc::new(FakeLauncher::new());
        let authority = TestRuntimeAuthority::new().await;
        let module = fake_module(Arc::clone(&launcher), Arc::clone(&authority.broker));
        let first_profile = local_profile("first.gguf");
        let first_prepared = module
            .prepare(&first_profile)
            .await
            .expect("first prepared");
        let first_approval = authority.approval(first_prepared.plan());
        let first = module
            .activate(
                first_prepared,
                Some(first_approval),
                CancellationToken::new(),
            )
            .await
            .expect("first activation");
        assert_eq!(module.lifecycle_snapshot().await.modules().len(), 1);
        let first_lookup = module.client("first.gguf").await.expect("first lookup");
        assert!(Arc::ptr_eq(&first, &first_lookup));
        first.verify_lease().await.expect("first lease");

        let same_prepared = module.prepare(&first_profile).await.expect("same prepared");
        let same = module
            .activate(same_prepared, None, CancellationToken::new())
            .await
            .expect("same activation");
        assert!(Arc::ptr_eq(&first, &same));
        assert_eq!(launcher.launches.load(Ordering::SeqCst), 1);

        let second_profile = local_profile("second.gguf");
        let second_prepared = module
            .prepare(&second_profile)
            .await
            .expect("second prepared");
        let second_approval = authority.approval(second_prepared.plan());
        let second = module
            .activate(
                second_prepared,
                Some(second_approval),
                CancellationToken::new(),
            )
            .await
            .expect("second activation");
        assert_ne!(first.lease().generation(), second.lease().generation());
        assert!(first.verify_lease().await.is_err());
        assert_eq!(module.lifecycle_snapshot().await.modules().len(), 1);

        module
            .shutdown(CancellationToken::new())
            .await
            .expect("shutdown");
        assert!(module.lifecycle_snapshot().await.modules().is_empty());
        assert!(second.verify_lease().await.is_err());
    }

    #[tokio::test]
    async fn cancellation_publishes_nothing_and_teardown_failure_is_retryable() {
        let launcher = Arc::new(FakeLauncher::new());
        let authority = TestRuntimeAuthority::new().await;
        let module = fake_module(Arc::clone(&launcher), Arc::clone(&authority.broker));
        let profile = local_profile("first.gguf");
        let prepared = module.prepare(&profile).await.expect("prepared launch");
        let approval = authority.approval(prepared.plan());
        let cancelled = CancellationToken::new();
        cancelled.cancel();
        assert!(matches!(
            module.activate(prepared, Some(approval), cancelled).await,
            Err(ProviderError::LocalRuntimeCancelled)
        ));
        assert!(module.lifecycle_snapshot().await.modules().is_empty());

        let prepared = module.prepare(&profile).await.expect("prepared launch");
        let approval = authority.approval(prepared.plan());
        let client = module
            .activate(prepared, Some(approval), CancellationToken::new())
            .await
            .expect("activation");
        launcher.fail_next_stop.store(true, Ordering::SeqCst);
        assert!(module.shutdown(CancellationToken::new()).await.is_err());
        assert_eq!(module.lifecycle_snapshot().await.modules().len(), 1);
        assert!(module.client("first.gguf").await.is_err());
        client
            .verify_lease()
            .await
            .expect("runtime retained for retry");

        module
            .shutdown(CancellationToken::new())
            .await
            .expect("retry shutdown");
        assert!(module.lifecycle_snapshot().await.modules().is_empty());
        assert!(client.verify_lease().await.is_err());
    }

    #[tokio::test]
    async fn local_provider_cancellation_closes_stream_and_emits_nothing_later() {
        let launcher = Arc::new(FakeLauncher::new());
        let authority = TestRuntimeAuthority::new().await;
        let module = Arc::new(fake_module(
            Arc::clone(&launcher),
            Arc::clone(&authority.broker),
        ));
        let profile = local_profile("first.gguf");
        let prepared = module.prepare(&profile).await.expect("prepared launch");
        let approval = authority.approval(prepared.plan());
        module
            .activate(prepared, Some(approval), CancellationToken::new())
            .await
            .expect("activation");
        let provider =
            crate::LocalLlamaCppProvider::with_runtime("first.gguf", Arc::clone(&module))
                .expect("local provider");
        let request = crate::ModelRequest {
            require_tool_call: false,
            continuation_scope: None,
            output_limit: 4096,
            model: "first.gguf".into(),
            system: None,
            history: Vec::new(),
            prompt: "hello".into(),
            tools: Vec::new(),
            tool_exchanges: Vec::new(),
        };
        let cancellation = CancellationToken::new();
        let task_cancellation = cancellation.clone();
        let (sender, mut receiver) = mpsc::channel(8);
        let task = tokio::spawn(async move {
            provider
                .stream_with_cancellation(request, sender, task_cancellation)
                .await
        });
        let first = tokio::time::timeout(Duration::from_secs(2), receiver.recv())
            .await
            .expect("first event timeout")
            .expect("first event");
        assert_eq!(first, crate::ProviderEvent::Delta("first".into()));
        cancellation.cancel();
        let result = tokio::time::timeout(Duration::from_secs(2), task)
            .await
            .expect("cancellation timeout")
            .expect("stream task");
        assert!(matches!(result, Err(ProviderError::RequestCancelled)));
        tokio::time::sleep(Duration::from_millis(2_100)).await;
        assert!(
            receiver.try_recv().is_err(),
            "late provider event escaped cancellation"
        );
        module
            .shutdown(CancellationToken::new())
            .await
            .expect("shutdown");
    }

    #[tokio::test]
    async fn local_runtime_authority_retirement_cancels_stream_before_late_event() {
        let launcher = Arc::new(FakeLauncher::new());
        let authority = TestRuntimeAuthority::new().await;
        let module = Arc::new(fake_module(
            Arc::clone(&launcher),
            Arc::clone(&authority.broker),
        ));
        let registry = crate::ProviderRegistry::default().with_local_runtime(Arc::clone(&module));
        authority
            .broker
            .mount_dependent(
                "provider.local-runtime",
                registry,
                &CancellationToken::new(),
            )
            .await
            .expect("provider authority dependency");

        let profile = local_profile("first.gguf");
        let prepared = module.prepare(&profile).await.expect("prepared launch");
        let approval = authority.approval(prepared.plan());
        module
            .activate(prepared, Some(approval), CancellationToken::new())
            .await
            .expect("activation");

        let provider =
            crate::LocalLlamaCppProvider::with_runtime("first.gguf", Arc::clone(&module))
                .expect("local provider");
        let request = crate::ModelRequest {
            require_tool_call: false,
            continuation_scope: None,
            output_limit: 4096,
            model: "first.gguf".into(),
            system: None,
            history: Vec::new(),
            prompt: "hello".into(),
            tools: Vec::new(),
            tool_exchanges: Vec::new(),
        };
        let (sender, mut receiver) = mpsc::channel(8);
        let stream_task = tokio::spawn(async move {
            provider
                .stream_with_cancellation(request, sender, CancellationToken::new())
                .await
        });
        let first = tokio::time::timeout(Duration::from_secs(2), receiver.recv())
            .await
            .expect("first event timeout")
            .expect("first event");
        assert_eq!(first, crate::ProviderEvent::Delta("first".into()));

        authority
            .broker
            .retire(&CancellationToken::new())
            .await
            .expect("authority retirement");
        let stream_result = tokio::time::timeout(Duration::from_secs(3), stream_task)
            .await
            .expect("stream retirement timeout")
            .expect("stream task");
        assert!(matches!(
            stream_result,
            Err(ProviderError::RequestCancelled)
        ));
        tokio::time::sleep(Duration::from_millis(2_100)).await;
        assert!(
            receiver.try_recv().is_err(),
            "late event escaped authority retirement"
        );
        assert!(module.lifecycle_snapshot().await.modules().is_empty());
        assert_eq!(
            authority.broker.snapshot().state(),
            PlatformAuthorityState::Revoked
        );
        assert_eq!(
            launcher
                .retirement_order
                .lock()
                .expect("retirement order")
                .as_slice(),
            ["provider.local-runtime"]
        );
    }

    #[tokio::test]
    async fn local_runtime_authority_shutdown_failure_is_retryable_without_relaunch() {
        let launcher = Arc::new(FakeLauncher::new());
        let authority = TestRuntimeAuthority::new().await;
        let module = Arc::new(fake_module(
            Arc::clone(&launcher),
            Arc::clone(&authority.broker),
        ));
        let registry = crate::ProviderRegistry::default().with_local_runtime(Arc::clone(&module));
        authority
            .broker
            .mount_dependent(
                "provider.local-runtime",
                registry,
                &CancellationToken::new(),
            )
            .await
            .expect("provider authority dependency");
        let prepared = module
            .prepare(&local_profile("first.gguf"))
            .await
            .expect("prepared launch");
        let approval = authority.approval(prepared.plan());
        module
            .activate(prepared, Some(approval), CancellationToken::new())
            .await
            .expect("activation");
        launcher.fail_next_stop.store(true, Ordering::SeqCst);

        assert!(authority
            .broker
            .retire(&CancellationToken::new())
            .await
            .is_err());
        assert_eq!(
            authority.broker.snapshot().state(),
            PlatformAuthorityState::RecoveryPending
        );
        assert_eq!(module.lifecycle_snapshot().await.modules().len(), 1);
        assert_eq!(launcher.launches.load(Ordering::SeqCst), 1);

        authority
            .broker
            .retire(&CancellationToken::new())
            .await
            .expect("shutdown retry");
        assert!(module.lifecycle_snapshot().await.modules().is_empty());
        assert_eq!(
            authority.broker.snapshot().state(),
            PlatformAuthorityState::Revoked
        );
        assert_eq!(launcher.launches.load(Ordering::SeqCst), 1);
    }
}
