pub mod onboarding;
pub mod protocol;
pub mod server;
pub mod tui;

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use argentum_android_inference::{
    ActivationApproval, AndroidInferenceModule, BackendReport, InferenceConfig, InferenceError,
    PreparedModel, VerifiedModelDescriptor,
};
use argentum_domain::ModelDiagnosticPhase as ModelPhase;
use argentum_domain::{
    is_supported_hosted_credential_provider_id, AppCommand, AppEvent, Capability, Goal,
    HarnessExecutionPolicy, HarnessSnapshot, OnboardingSnapshot, PlatformAuthoritySnapshot,
    ProviderKind, ProviderProfile, ProviderStatus, Sha256Digest, ToolDescriptor, WorkspaceSnapshot,
};
use argentum_platform::ValidatedAndroidAppPaths;
use argentum_platform::{
    default_secret_store, provider_credential_key, AppPaths, PlatformAuthorityBroker,
    PlatformError, SecretStore, UnavailableSecretStore, SECRET_SERVICE,
};
use argentum_providers::{
    android_local_model_offers, download_local_model, local_runtime_bundle_path,
    local_runtime_offers, LocalRuntimeClient, LocalRuntimeConfig, LocalRuntimeLaunchApproval,
    LocalRuntimeLaunchPlan, LocalRuntimeModule, PreparedLocalRuntime, ProviderCredentials,
    ProviderError, ProviderRegistry,
};
use argentum_runtime::{RuntimeError, RuntimeHostAuthority, RuntimeService};
use argentum_security::{
    host_authority_channel, ApprovalPolicy, AuthorityGeneration, CapabilityBroker,
    LocalAuthorityAction, LocalAuthorityIssuer, SecretValue, SecurityError, VerifiedLocalAuthority,
};
use argentum_store::{Store, StoreError, StoreHostOwner};
use argentum_tools::ToolRegistry;
use argentum_workspaces::WorkspaceManager;
use thiserror::Error;
use tokio::sync::{broadcast, Mutex as AsyncMutex};
use tracing::warn;

pub const DEFAULT_LLAMA_CPP_ENDPOINT: &str = argentum_domain::DEFAULT_LLAMA_CPP_ENDPOINT;
pub const DEFAULT_LOCAL_MODEL_FILENAME: &str = argentum_domain::DEFAULT_LLAMA_CPP_MODEL;
pub const DEFAULT_MODEL: &str = argentum_domain::DEFAULT_MODEL;
pub const DEFAULT_PROVIDER_ID: &str = argentum_domain::DEFAULT_PROVIDER_ID;

/// Parse owner input only; the runtime validates and persists the declaration.
pub fn parse_model_budget_limits(value: &str) -> Result<[u64; 5], String> {
    let limits = value
        .split(',')
        .map(|v| v.trim().parse::<u64>())
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| "limits require five unsigned integers".to_owned())?;
    limits.try_into().map_err(|_| {
        "limits require TOTAL,INPUT,MAX_OUTPUT,REQUESTED_OUTPUT,UNCERTAINTY".to_owned()
    })
}

#[derive(Clone)]
pub struct HostConfig {
    pub workspace: PathBuf,
    pub database: Option<PathBuf>,
    pub provider_endpoint: String,
    pub model: String,
    pub local_runtime: Option<LocalRuntimeConfig>,
    pub android_inference: Option<InferenceConfig>,
    provider_credentials: ProviderCredentials,
    secret_store: Arc<dyn SecretStore>,
}

impl std::fmt::Debug for HostConfig {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("HostConfig")
            .field("workspace", &self.workspace)
            .field("database", &self.database)
            .field("provider_endpoint", &self.provider_endpoint)
            .field("model", &self.model)
            .field("local_runtime", &self.local_runtime)
            .field(
                "android_inference_configured",
                &self.android_inference.is_some(),
            )
            .field("provider_credentials", &self.provider_credentials)
            .finish()
    }
}

impl HostConfig {
    /// Local text chat uses existing runtime/model paths without loading any
    /// hosted credentials from the environment or the secure store.
    pub fn discover_local_only(workspace: impl Into<PathBuf>) -> Result<Self, HostError> {
        let paths = AppPaths::discover()?;
        let offer = local_runtime_offers()
            .first()
            .ok_or(ProviderError::LocalRuntimeBundleInvalid)?;
        let mut config = Self::in_memory(workspace);
        config.database = Some(paths.database);
        config.local_runtime = Some(LocalRuntimeConfig::new(
            local_runtime_bundle_path(&paths.data_dir.join("runtimes"), offer)?,
            paths.data_dir.join("models"),
        )?);
        Ok(config)
    }

    pub fn discover(workspace: impl Into<PathBuf>) -> Result<Self, HostError> {
        let paths = AppPaths::discover()?;
        let secret_store = default_secret_store(SECRET_SERVICE);
        let mut provider_credentials = provider_credentials_from_environment()?;
        load_secure_provider_credentials(secret_store.as_ref(), &mut provider_credentials);
        let runtime_offer = local_runtime_offers()
            .first()
            .ok_or(ProviderError::LocalRuntimeBundleInvalid)?;
        let runtime_bundle =
            local_runtime_bundle_path(&paths.data_dir.join("runtimes"), runtime_offer)?;
        Ok(Self {
            workspace: workspace.into(),
            database: Some(paths.database),
            provider_endpoint: DEFAULT_LLAMA_CPP_ENDPOINT.into(),
            model: DEFAULT_MODEL.into(),
            local_runtime: Some(LocalRuntimeConfig::new(
                runtime_bundle,
                paths.data_dir.join("models"),
            )?),
            android_inference: None,
            provider_credentials,
            secret_store,
        })
    }

    #[cfg(test)]
    fn discover_with_app_paths(
        workspace: impl Into<PathBuf>,
        paths: AppPaths,
        secret_store: Arc<dyn SecretStore>,
    ) -> Result<Self, HostError> {
        let paths = paths.validate_android_private_layout()?;
        let mut provider_credentials = provider_credentials_from_environment()?;
        load_secure_provider_credentials(secret_store.as_ref(), &mut provider_credentials);
        let android_offer = android_local_model_offers()
            .first()
            .ok_or(ProviderError::LocalRuntimeBundleInvalid)?;
        Ok(Self {
            workspace: workspace.into(),
            database: Some(paths.database),
            provider_endpoint: DEFAULT_LLAMA_CPP_ENDPOINT.into(),
            model: android_offer.id.into(),
            local_runtime: None,
            android_inference: Some(InferenceConfig::new(
                paths.data_dir.join("models"),
                Default::default(),
            )?),
            provider_credentials,
            secret_store,
        })
    }

    /// Builds the Android host only from paths validated as one private layout.
    ///
    /// An independent workspace argument is not accepted.
    ///
    /// ```compile_fail
    /// use std::path::PathBuf;
    /// use std::sync::Arc;
    /// use argentum_cli::{HostConfig, HostError};
    /// use argentum_platform::{SecretStore, ValidatedAndroidAppPaths};
    ///
    /// fn invalid_constructor(
    ///     token: ValidatedAndroidAppPaths,
    ///     secret_store: Arc<dyn SecretStore>,
    /// ) -> Result<HostConfig, HostError> {
    ///     HostConfig::discover_with_validated_app_paths(
    ///         PathBuf::from("/independent/workspace"),
    ///         token,
    ///         secret_store,
    ///     )
    /// }
    /// ```
    pub fn discover_with_validated_app_paths(
        validated: ValidatedAndroidAppPaths,
        secret_store: Arc<dyn SecretStore>,
    ) -> Result<Self, HostError> {
        let (workspace, paths) = validated.into_parts();
        Self::from_validated_android_parts(workspace, paths, secret_store)
    }

    fn from_validated_android_parts(
        workspace: PathBuf,
        paths: AppPaths,
        secret_store: Arc<dyn SecretStore>,
    ) -> Result<Self, HostError> {
        let mut provider_credentials = provider_credentials_from_environment()?;
        load_secure_provider_credentials(secret_store.as_ref(), &mut provider_credentials);
        let android_offer = android_local_model_offers()
            .first()
            .ok_or(ProviderError::LocalRuntimeBundleInvalid)?;
        Ok(Self {
            workspace,
            database: Some(paths.database),
            provider_endpoint: DEFAULT_LLAMA_CPP_ENDPOINT.into(),
            model: android_offer.id.into(),
            local_runtime: None,
            android_inference: Some(InferenceConfig::new(
                paths.data_dir.join("models"),
                Default::default(),
            )?),
            provider_credentials,
            secret_store,
        })
    }

    pub fn in_memory(workspace: impl Into<PathBuf>) -> Self {
        Self {
            workspace: workspace.into(),
            database: None,
            provider_endpoint: DEFAULT_LLAMA_CPP_ENDPOINT.into(),
            model: DEFAULT_MODEL.into(),
            local_runtime: None,
            android_inference: None,
            provider_credentials: ProviderCredentials::default(),
            secret_store: Arc::new(UnavailableSecretStore),
        }
    }

    pub fn with_provider_credentials(mut self, credentials: ProviderCredentials) -> Self {
        self.provider_credentials = credentials;
        self
    }

    pub fn with_local_runtime(mut self, config: LocalRuntimeConfig) -> Self {
        self.local_runtime = Some(config);
        self
    }

    pub fn with_android_inference(mut self, config: InferenceConfig) -> Self {
        self.android_inference = Some(config);
        self
    }

    pub fn with_secret_store(mut self, secret_store: Arc<dyn SecretStore>) -> Self {
        load_secure_provider_credentials(secret_store.as_ref(), &mut self.provider_credentials);
        self.secret_store = secret_store;
        self
    }

    /// Attach only the runtime's vault boundary, without enumerating unrelated
    /// provider keys during extension-only local configuration.
    #[cfg(feature = "extensions")]
    pub fn with_extension_secret_store(mut self, secret_store: Arc<dyn SecretStore>) -> Self {
        self.secret_store = secret_store;
        self
    }

    pub fn with_provider_credential(
        self,
        profile_id: impl AsRef<str>,
        api_key: SecretValue,
    ) -> Result<Self, HostError> {
        self.provider_credentials.insert(profile_id, api_key)?;
        Ok(self)
    }

    pub fn with_provider_api_keys(
        mut self,
        values: impl IntoIterator<Item = (String, SecretValue)>,
    ) -> Result<Self, HostError> {
        self.provider_credentials = ProviderCredentials::from_profile_api_keys(values)?;
        Ok(self)
    }
}

fn provider_credentials_from_environment() -> Result<ProviderCredentials, HostError> {
    provider_credentials_from_lookup(|variable| std::env::var(variable))
}

fn load_secure_provider_credentials(
    store: &dyn SecretStore,
    credentials: &mut ProviderCredentials,
) {
    for (profile_id, _) in argentum_providers::provider_credential_environment() {
        let Ok(key) = provider_credential_key(profile_id) else {
            continue;
        };
        match store.get(&key) {
            Ok(Some(secret)) if !credentials.contains_profile(profile_id) => {
                if let Err(error_value) = credentials.insert(profile_id, secret) {
                    warn!(profile_id, error = %error_value, "secure provider credential was rejected");
                }
            }
            Ok(Some(_)) | Ok(None) => {}
            Err(_) => {
                warn!(profile_id, "secure provider credential is unavailable");
            }
        }
    }
}

fn provider_credentials_from_lookup(
    mut lookup: impl FnMut(&str) -> Result<String, std::env::VarError>,
) -> Result<ProviderCredentials, HostError> {
    let credentials = ProviderCredentials::default();
    for (profile_id, variable) in argentum_providers::provider_credential_environment() {
        match lookup(variable) {
            Ok(value) if !value.trim().is_empty() => {
                credentials.insert(profile_id, SecretValue::new(value))?;
            }
            Ok(_) | Err(std::env::VarError::NotPresent) => {}
            Err(std::env::VarError::NotUnicode(_)) => {
                warn!(
                    environment_variable = variable,
                    "provider credential is not valid Unicode"
                );
            }
        }
    }
    Ok(credentials)
}

#[derive(Debug, Error)]
pub enum HostError {
    #[error(transparent)]
    ChatGptAuth(#[from] argentum_providers::chatgpt_auth::AuthError),
    #[error(transparent)]
    Platform(#[from] PlatformError),
    #[error(transparent)]
    Store(#[from] StoreError),
    #[error(transparent)]
    Security(#[from] SecurityError),
    #[error(transparent)]
    Provider(#[from] ProviderError),
    #[error(transparent)]
    AndroidInference(#[from] InferenceError),
    #[error(transparent)]
    Runtime(#[from] RuntimeError),
    #[error("secure credentials are supported only for catalogued hosted provider profiles")]
    ProviderCredentialUnsupported,
    #[error("provider credential storage is unavailable")]
    ProviderCredentialStorageUnavailable,
    #[error("provider credential transaction is unavailable")]
    ProviderCredentialTransactionUnavailable,
}

/// Owns the runtime lifecycle and is the only public shutdown authority.
///
/// The owner cannot be cloned:
///
/// ```compile_fail
/// use argentum_cli::CommandHost;
///
/// fn require_clone<T: Clone>() {}
///
/// fn command_host_must_not_clone() {
///     require_clone::<CommandHost>();
/// }
/// ```
///
/// A shared host borrow cannot request shutdown:
///
/// ```compile_fail
/// fn request_shutdown(host: &argentum_cli::CommandHost) {
///     let _ = host.request_shutdown();
/// }
/// ```
///
/// An untrusted client has neither shutdown entry point:
///
/// ```compile_fail
/// fn shutdown(mut client: argentum_cli::InProcessClient) {
///     let _ = client.shutdown();
/// }
/// ```
///
/// ```compile_fail
/// fn request_shutdown(mut client: argentum_cli::InProcessClient) {
///     let _ = client.request_shutdown();
/// }
/// ```
///
/// A trusted local command handle has neither shutdown entry point:
///
/// ```compile_fail
/// fn shutdown(mut local: argentum_cli::LocalCommandHandle) {
///     let _ = local.shutdown();
/// }
/// ```
///
/// ```compile_fail
/// fn request_shutdown(mut local: argentum_cli::LocalCommandHandle) {
///     let _ = local.request_shutdown();
/// }
/// ```
pub struct CommandHost {
    runtime: RuntimeService,
    store_owner: Option<StoreHostOwner>,
    approval_responder: Option<argentum_runtime::InteractiveApprovalResponder>,
    secret_store: Arc<dyn SecretStore>,
    credential_transaction: Arc<Mutex<()>>,
    local_runtime: Option<Arc<LocalRuntimeModule>>,
    android_inference: Option<Arc<AsyncMutex<AndroidInferenceModule>>>,
    local_authority: Option<Arc<LocalAuthorityOwner>>,
    provider_authority_mounted: bool,
}

impl std::fmt::Debug for CommandHost {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("CommandHost")
            .field("local_runtime_configured", &self.local_runtime.is_some())
            .field(
                "android_inference_configured",
                &self.android_inference.is_some(),
            )
            .field(
                "provider_authority_mounted",
                &self.provider_authority_mounted,
            )
            .finish_non_exhaustive()
    }
}

#[derive(Clone)]
pub struct CommandHostObserver {
    runtime: RuntimeService,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HostLifecycleSnapshot {
    generation: u64,
    module_ids: Vec<String>,
}

impl HostLifecycleSnapshot {
    pub const fn generation(&self) -> u64 {
        self.generation
    }

    pub fn module_ids(&self) -> &[String] {
        &self.module_ids
    }
}

impl CommandHostObserver {
    pub fn publish_ui_state(&self) -> Result<(), HostError> {
        publish_runtime_ui_state(&self.runtime)
    }

    pub async fn harness_lifecycle_snapshot(&self) -> Option<HostLifecycleSnapshot> {
        self.runtime
            .harness_lifecycle_snapshot()
            .await
            .map(|snapshot| HostLifecycleSnapshot {
                generation: snapshot.generation(),
                module_ids: snapshot
                    .modules()
                    .iter()
                    .map(|module| module.id().to_owned())
                    .collect(),
            })
    }
}

struct LocalAuthorityOwner {
    issuer: LocalAuthorityIssuer,
    platform_authority: Arc<PlatformAuthorityBroker>,
}

pub struct AndroidInferenceApproval {
    inner: ActivationApproval,
    authority: VerifiedLocalAuthority,
    plan_digest: Sha256Digest,
}

#[derive(Clone)]
pub struct AndroidInferenceControl {
    runtime: Arc<AsyncMutex<AndroidInferenceModule>>,
    local: LocalCommandHandle,
}

impl std::fmt::Debug for AndroidInferenceControl {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("AndroidInferenceControl")
            .finish_non_exhaustive()
    }
}

impl AndroidInferenceControl {
    pub async fn begin_operation(&self) -> Result<AndroidInferenceOperation, HostError> {
        let lease = self
            .local
            .runtime
            .local_backend_lease("llama.cpp.android")
            .await?;
        let foreground_generation = self.foreground_generation().await;
        Ok(AndroidInferenceOperation {
            control: self.clone(),
            _lease: lease,
            foreground_generation,
        })
    }
    pub async fn lifecycle_generation(&self) -> u64 {
        self.runtime.lock().await.lifecycle_generation().ordinal()
    }

    pub async fn foreground_generation(&self) -> u64 {
        self.runtime.lock().await.foreground_generation()
    }

    pub async fn install_and_activate_default(
        &self,
        cancellation: tokio_util::sync::CancellationToken,
    ) -> Result<BackendReport, HostError> {
        self.begin_operation()
            .await?
            .install_and_activate_default(cancellation)
            .await
    }

    async fn install_and_activate_admitted(
        &self,
        cancellation: tokio_util::sync::CancellationToken,
    ) -> Result<BackendReport, HostError> {
        self.local
            .runtime
            .record_model_phase(ModelPhase::ArtifactPreparing);
        let offer = *android_local_model_offers()
            .first()
            .ok_or(ProviderError::LocalRuntimeBundleInvalid)?;
        let model_directory = self.runtime.lock().await.model_directory().to_path_buf();
        tokio::fs::create_dir_all(&model_directory)
            .await
            .map_err(|_| InferenceError::InvalidModelDirectory)?;
        tokio::select! {
            biased;
            _ = cancellation.cancelled() => return Err(ProviderError::RequestCancelled.into()),
            result = download_local_model(offer, &model_directory) => {
                result?;
            }
        }
        if cancellation.is_cancelled() {
            return Err(ProviderError::RequestCancelled.into());
        }
        let descriptor = VerifiedModelDescriptor::new(
            offer.label,
            offer.filename,
            offer.size_bytes,
            offer.sha256,
        )?;
        self.local
            .runtime
            .record_model_phase(ModelPhase::ArtifactReady);
        self.local.runtime.record_model_phase(ModelPhase::Preparing);
        let prepared = self.runtime.lock().await.prepare(descriptor).await?;
        self.local.runtime.record_model_phase(ModelPhase::Prepared);
        if cancellation.is_cancelled() {
            return Err(ProviderError::RequestCancelled.into());
        }
        let approval = self.local.approve_android_inference(&prepared)?;
        self.activate_admitted(prepared, approval).await
    }

    pub async fn activate(
        &self,
        prepared: PreparedModel,
        approval: AndroidInferenceApproval,
    ) -> Result<BackendReport, HostError> {
        let _backend = self
            .local
            .runtime
            .local_backend_lease("llama.cpp.android")
            .await?;
        self.activate_admitted(prepared, approval).await
    }

    async fn activate_admitted(
        &self,
        prepared: PreparedModel,
        approval: AndroidInferenceApproval,
    ) -> Result<BackendReport, HostError> {
        let authority = self
            .local
            .local_authority
            .as_ref()
            .ok_or(RuntimeError::LocalAuthorityUnavailable)?;
        let generation = authority
            .platform_authority
            .current_generation()
            .ok_or(RuntimeError::LocalAuthorityUnavailable)?;
        let mut approval = approval.authorize(&prepared, &generation)?;
        self.local.runtime.record_model_phase(ModelPhase::Loading);
        Ok(self
            .runtime
            .lock()
            .await
            .activate(prepared, &mut approval)
            .await?)
    }

    pub async fn unload(&self) -> Result<(), HostError> {
        let _backend = self
            .local
            .runtime
            .local_backend_lease("llama.cpp.android")
            .await?;
        self.runtime.lock().await.unload().await?;
        Ok(())
    }

    pub async fn active_model(&self) -> Option<VerifiedModelDescriptor> {
        self.runtime.lock().await.active_model().cloned()
    }

    pub async fn backend_report(&self) -> Option<BackendReport> {
        self.runtime.lock().await.backend_report().cloned()
    }
}

/// Keeps model work and its foreground generation attached to one live worker.
pub struct AndroidInferenceOperation {
    control: AndroidInferenceControl,
    _lease: argentum_providers::LocalBackendLease,
    foreground_generation: u64,
}

impl AndroidInferenceOperation {
    pub fn foreground_generation(&self) -> u64 {
        self.foreground_generation
    }

    pub async fn install_and_activate_default(
        &self,
        cancellation: tokio_util::sync::CancellationToken,
    ) -> Result<BackendReport, HostError> {
        let result = self
            .control
            .install_and_activate_admitted(cancellation)
            .await;
        self.control
            .local
            .runtime
            .record_model_phase(if result.is_ok() {
                ModelPhase::Ready
            } else {
                ModelPhase::Failed
            });
        result
    }

    pub async fn unload(&self) -> Result<(), HostError> {
        self.control
            .local
            .runtime
            .record_model_phase(ModelPhase::Unloading);
        let result = self.control.runtime.lock().await.unload().await;
        self.control
            .local
            .runtime
            .record_model_phase(if result.is_ok() {
                ModelPhase::Unloaded
            } else {
                ModelPhase::Failed
            });
        result?;
        Ok(())
    }
}

impl std::fmt::Debug for AndroidInferenceApproval {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("AndroidInferenceApproval")
            .finish_non_exhaustive()
    }
}

impl AndroidInferenceApproval {
    fn bind(
        prepared: &PreparedModel,
        authority: VerifiedLocalAuthority,
    ) -> Result<Self, HostError> {
        let plan_digest = android_inference_plan_digest(prepared);
        if authority.action()
            != &(LocalAuthorityAction::ActivateLocalRuntime {
                plan: plan_digest.clone(),
            })
        {
            return Err(ProviderError::LocalRuntimeApprovalDenied.into());
        }
        Ok(Self {
            inner: ActivationApproval::bind(prepared),
            authority,
            plan_digest,
        })
    }

    fn authorize(
        self,
        prepared: &PreparedModel,
        generation: &AuthorityGeneration,
    ) -> Result<ActivationApproval, HostError> {
        let plan_digest = android_inference_plan_digest(prepared);
        if self.plan_digest != plan_digest {
            return Err(ProviderError::LocalRuntimeApprovalStale.into());
        }
        let action = LocalAuthorityAction::ActivateLocalRuntime { plan: plan_digest };
        self.authority
            .authorize(&action, generation)
            .map_err(|error| match error {
                SecurityError::LocalAuthorityExpired
                | SecurityError::LocalAuthorityGenerationMismatch => {
                    HostError::Provider(ProviderError::LocalRuntimeApprovalStale)
                }
                _ => HostError::Provider(ProviderError::LocalRuntimeApprovalDenied),
            })?;
        Ok(self.inner)
    }
}

fn android_inference_plan_digest(prepared: &PreparedModel) -> Sha256Digest {
    Sha256Digest(prepared.plan_digest_hex())
}

fn build_android_inference(
    config: Option<InferenceConfig>,
    authority: &Arc<PlatformAuthorityBroker>,
    defer_start: bool,
) -> Result<Option<Arc<AsyncMutex<AndroidInferenceModule>>>, HostError> {
    let Some(config) = config else {
        return Ok(None);
    };
    let generation = authority
        .current_generation()
        .ok_or(RuntimeError::LocalAuthorityUnavailable)?;
    #[cfg(target_os = "android")]
    {
        Ok(Some(Arc::new(AsyncMutex::new(if defer_start {
            AndroidInferenceModule::stopped(config, generation)?
        } else {
            AndroidInferenceModule::new(config, generation)?
        }))))
    }
    #[cfg(not(target_os = "android"))]
    {
        let _ = (config, generation, defer_start);
        Err(ProviderError::LocalCapabilityUnsupported {
            capability: "Android local inference on this target",
        }
        .into())
    }
}

impl CommandHost {
    pub fn historical_change_page(
        &self,
        run: argentum_domain::RunId,
        limit: usize,
        after: Option<argentum_domain::ChangePageCursor>,
    ) -> Result<argentum_domain::ChangePage, HostError> {
        Ok(self.runtime.historical_change_page(run, limit, after)?)
    }

    pub fn inspect_existing_change_page(
        config: HostConfig,
        run: argentum_domain::RunId,
        limit: usize,
        after: Option<argentum_domain::ChangePageCursor>,
    ) -> Result<argentum_domain::ChangePage, HostError> {
        // Historical metadata needs neither hosted credentials nor authentication.
        let database = config.database.ok_or(StoreError::InvalidRecord(
            "metadata inspection requires an existing persistent store".into(),
        ))?;
        let store = Store::open_existing_read_only(database)?;
        let policy = ApprovalPolicy::default().allow_without_approval(Capability::ReadFiles);
        let workspace = WorkspaceManager::new(CapabilityBroker::new(&config.workspace, policy)?);
        let tools = ToolRegistry::with_builtins(workspace.clone());
        let providers = ProviderRegistry::with_credentials(ProviderCredentials::default());
        Ok(RuntimeService::inspect_existing_change_page(
            store, providers, tools, workspace, run, limit, after,
        )?)
    }
    // Existing inspection entry point is defined above.

    fn existing_inspection_parts(
        config: HostConfig,
    ) -> Result<(Store, ProviderRegistry, ToolRegistry, WorkspaceManager), HostError> {
        let database = config.database.ok_or(StoreError::InvalidRecord(
            "metadata inspection requires an existing persistent store".into(),
        ))?;
        let store = Store::open_existing_read_only(database)?;
        let policy = ApprovalPolicy::default().allow_without_approval(Capability::ReadFiles);
        let broker = CapabilityBroker::new(&config.workspace, policy)?;
        let workspace = WorkspaceManager::new(broker);
        let tools = ToolRegistry::with_builtins(workspace.clone());
        let providers = ProviderRegistry::with_credentials(config.provider_credentials)
            .with_chatgpt_auth(Arc::clone(&config.secret_store))?;
        Ok((store, providers, tools, workspace))
    }

    /// Metadata-only fallback for a database whose writable lifetime guard is
    /// held by another process. It does not migrate, reconcile, or acquire host
    /// authority.
    pub fn inspect_existing_instructions(
        config: HostConfig,
        directory: &str,
    ) -> Result<argentum_runtime::instructions::InstructionPreview, HostError> {
        let (store, providers, tools, workspace) = Self::existing_inspection_parts(config)?;
        Ok(RuntimeService::inspect_existing_instructions(
            store, providers, tools, workspace, directory,
        )?)
    }

    /// Metadata-only context fallback for a live writable owner.
    pub fn inspect_existing_context(
        config: HostConfig,
    ) -> Result<argentum_runtime::working_context::ContextPreview, HostError> {
        let (store, providers, tools, workspace) = Self::existing_inspection_parts(config)?;
        Ok(RuntimeService::inspect_existing_context(
            store, providers, tools, workspace,
        )?)
    }

    pub fn start(config: HostConfig) -> Result<Self, HostError> {
        let secret_store = config.secret_store.clone();
        let (store, store_owner) = match config.database {
            Some(path) => Store::open_for_host(path)?,
            None => Store::open_in_memory_for_host()?,
        };
        let policy = ApprovalPolicy::default().allow_without_approval(Capability::ReadFiles);
        let broker = CapabilityBroker::new(&config.workspace, policy)?;
        let workspace = WorkspaceManager::new(broker);
        let tools = ToolRegistry::with_builtins(workspace.clone());
        let (local_authority, runtime_authority) = bootstrap_host_authority_blocking()?;
        let mut providers = ProviderRegistry::with_credentials(config.provider_credentials)
            .with_chatgpt_auth(Arc::clone(&secret_store))?;
        let local_runtime = config
            .local_runtime
            .map(|runtime_config| {
                LocalRuntimeModule::new(
                    runtime_config,
                    Arc::clone(&local_authority.platform_authority),
                )
            })
            .map(Arc::new);
        let android_inference = build_android_inference(
            config.android_inference,
            &local_authority.platform_authority,
            false,
        )?;
        if let Some(runtime) = local_runtime.as_ref() {
            providers = providers.with_local_runtime(Arc::clone(runtime));
        }
        if let Some(runtime) = android_inference.as_ref() {
            providers = providers.with_android_inference(Arc::clone(runtime));
        }
        let default_provider = ProviderProfile {
            id: DEFAULT_PROVIDER_ID.into(),
            label: "llama.cpp".into(),
            kind: ProviderKind::LocalLlamaCpp,
            endpoint: DEFAULT_LLAMA_CPP_ENDPOINT.into(),
            model: config.model,
            selected: true,
        };
        Self::from_runtime_with_local_runtime_and_authority(
            RuntimeService::new_with_default_provider_profile_and_host_authority(
                store,
                providers,
                tools,
                workspace,
                default_provider,
                runtime_authority,
            )?,
            secret_store,
            local_runtime,
            android_inference,
            Some(local_authority),
            false,
            Some(store_owner),
        )
    }

    pub async fn start_composed(config: HostConfig) -> Result<Self, HostError> {
        Self::start_composed_with_local_runtime_factory(config, |runtime_config, authority| {
            Arc::new(LocalRuntimeModule::new(runtime_config, authority))
        })
        .await
    }

    async fn start_composed_with_local_runtime_factory<F>(
        config: HostConfig,
        local_runtime_factory: F,
    ) -> Result<Self, HostError>
    where
        F: FnOnce(LocalRuntimeConfig, Arc<PlatformAuthorityBroker>) -> Arc<LocalRuntimeModule>,
    {
        let (local_authority, runtime_authority) = bootstrap_host_authority().await?;
        Self::start_composed_with_authority_and_local_runtime_factory(
            config,
            local_authority,
            runtime_authority,
            local_runtime_factory,
        )
        .await
    }

    #[cfg(test)]
    async fn start_composed_with_recording_factories<F>(
        config: HostConfig,
        lifecycle_events: Arc<std::sync::Mutex<Vec<&'static str>>>,
        authority_release_calls: Arc<std::sync::atomic::AtomicUsize>,
        local_runtime_factory: F,
    ) -> Result<Self, HostError>
    where
        F: FnOnce(LocalRuntimeConfig, Arc<PlatformAuthorityBroker>) -> Arc<LocalRuntimeModule>,
    {
        let (issuer, verifier, generation) = host_authority_channel()?;
        let platform_authority = Arc::new(
            PlatformAuthorityBroker::bootstrap_recording_test_bridge(
                generation,
                lifecycle_events,
                authority_release_calls,
            )
            .await?,
        );
        let local_authority = Arc::new(LocalAuthorityOwner {
            issuer,
            platform_authority: Arc::clone(&platform_authority),
        });
        let runtime_authority = RuntimeHostAuthority::new(verifier, platform_authority);
        Self::start_composed_with_authority_and_local_runtime_factory(
            config,
            local_authority,
            runtime_authority,
            local_runtime_factory,
        )
        .await
    }

    async fn start_composed_with_authority_and_local_runtime_factory<F>(
        config: HostConfig,
        local_authority: Arc<LocalAuthorityOwner>,
        runtime_authority: RuntimeHostAuthority,
        local_runtime_factory: F,
    ) -> Result<Self, HostError>
    where
        F: FnOnce(LocalRuntimeConfig, Arc<PlatformAuthorityBroker>) -> Arc<LocalRuntimeModule>,
    {
        let secret_store = config.secret_store.clone();
        let (store, store_owner) = match config.database {
            Some(path) => Store::open_for_host(path)?,
            None => Store::open_in_memory_for_host()?,
        };
        let policy = ApprovalPolicy::default().allow_without_approval(Capability::ReadFiles);
        let broker = CapabilityBroker::new(&config.workspace, policy)?;
        let workspace = WorkspaceManager::new(broker);
        let tools = ToolRegistry::with_builtins(workspace.clone());
        let mut providers = ProviderRegistry::with_credentials(config.provider_credentials)
            .with_chatgpt_auth(Arc::clone(&secret_store))?;
        let local_runtime = config.local_runtime.map(|runtime_config| {
            local_runtime_factory(
                runtime_config,
                Arc::clone(&local_authority.platform_authority),
            )
        });
        let android_inference = build_android_inference(
            config.android_inference,
            &local_authority.platform_authority,
            true,
        )?;
        if let Some(runtime) = local_runtime.as_ref() {
            providers = providers.with_local_runtime(Arc::clone(runtime));
        }
        if let Some(runtime) = android_inference.as_ref() {
            providers = providers.with_android_inference(Arc::clone(runtime));
        }
        let default_provider = ProviderProfile {
            id: DEFAULT_PROVIDER_ID.into(),
            label: "llama.cpp".into(),
            kind: ProviderKind::LocalLlamaCpp,
            endpoint: DEFAULT_LLAMA_CPP_ENDPOINT.into(),
            model: config.model,
            selected: true,
        };
        let provider_authority = providers
            .local_runtime_configured()
            .then(|| providers.clone());
        let runtime =
            RuntimeService::new_composed_with_default_provider_profile_and_host_authority(
                store,
                providers,
                tools,
                workspace,
                default_provider,
                runtime_authority,
            )
            .await?;
        let provider_authority_mounted = if let Some(provider_registry) = provider_authority {
            if let Err(error) = local_authority
                .platform_authority
                .mount_dependent(
                    "provider.local-runtime",
                    provider_registry,
                    &tokio_util::sync::CancellationToken::new(),
                )
                .await
            {
                let _ = runtime.shutdown().await;
                return Err(error.into());
            }
            runtime.mark_providers_authority_managed();
            true
        } else {
            false
        };
        Self::from_runtime_with_local_runtime_and_authority(
            runtime,
            secret_store,
            local_runtime,
            android_inference,
            Some(local_authority),
            provider_authority_mounted,
            Some(store_owner),
        )
    }

    pub async fn shutdown(&mut self) -> Result<(), HostError> {
        self.runtime.shutdown().await?;
        self.approval_responder.take();
        if let Some(owner) = self.store_owner.as_mut() {
            owner.retire()?;
        }
        Ok(())
    }

    /// Select the fail-closed approval strategy for a frontend without an owner
    /// responder. Open stdin and terminal detection cannot change this decision.
    pub fn disconnect_approval_responder(&mut self) {
        self.approval_responder.take();
    }

    pub fn request_shutdown(&mut self) -> Result<(), HostError> {
        self.runtime.request_shutdown()?;
        Ok(())
    }

    #[cfg(test)]
    pub(crate) fn from_runtime(runtime: RuntimeService) -> Self {
        Self::from_runtime_with_secret_store(runtime, default_secret_store(SECRET_SERVICE))
    }

    #[cfg(test)]
    pub(crate) fn from_runtime_with_secret_store(
        runtime: RuntimeService,
        secret_store: Arc<dyn SecretStore>,
    ) -> Self {
        Self::from_runtime_with_local_runtime(runtime, secret_store, None)
    }

    #[cfg(test)]
    fn from_runtime_with_local_runtime(
        runtime: RuntimeService,
        secret_store: Arc<dyn SecretStore>,
        local_runtime: Option<Arc<LocalRuntimeModule>>,
    ) -> Self {
        Self::from_runtime_with_local_runtime_and_authority(
            runtime,
            secret_store,
            local_runtime,
            None,
            None,
            false,
            None,
        )
        .expect("test command host")
    }

    fn from_runtime_with_local_runtime_and_authority(
        runtime: RuntimeService,
        secret_store: Arc<dyn SecretStore>,
        local_runtime: Option<Arc<LocalRuntimeModule>>,
        android_inference: Option<Arc<AsyncMutex<AndroidInferenceModule>>>,
        local_authority: Option<Arc<LocalAuthorityOwner>>,
        provider_authority_mounted: bool,
        store_owner: Option<StoreHostOwner>,
    ) -> Result<Self, HostError> {
        let approval_responder = local_authority
            .as_ref()
            .map(|_| runtime.connect_interactive_approval_responder())
            .transpose()?;
        #[cfg(feature = "extensions")]
        runtime.attach_extension_secret_store(Arc::clone(&secret_store))?;
        Ok(Self {
            runtime,
            store_owner,
            approval_responder,
            secret_store,
            credential_transaction: Arc::new(Mutex::new(())),
            local_runtime,
            android_inference,
            local_authority,
            provider_authority_mounted,
        })
    }

    pub fn local_runtime_configured(&self) -> bool {
        self.runtime.local_backend_configured()
    }

    pub async fn local_runtime_active(&self, model: &str) -> bool {
        if let Some(runtime) = self.android_inference.as_ref() {
            return runtime
                .lock()
                .await
                .active_model()
                .is_some_and(|active| active.file_name() == model);
        }
        let Some(runtime) = self.local_runtime.as_ref() else {
            return false;
        };
        match (
            runtime.client(model).await,
            self.runtime.configured_local_context_tokens(model),
        ) {
            (Ok(client), Ok(context)) => client.context_tokens() == context,
            _ => false,
        }
    }

    pub async fn prepare_local_runtime(
        &self,
        model: impl Into<String>,
    ) -> Result<PreparedLocalRuntime, HostError> {
        let _backend = self
            .runtime
            .local_backend_lease("llama.cpp.sidecar")
            .await?;
        let model = model.into();
        let context_tokens = self.runtime.configured_local_context_tokens(&model)?;
        let runtime = self
            .local_runtime
            .as_ref()
            .ok_or(ProviderError::ProviderNotConfigured {
                provider_id: DEFAULT_PROVIDER_ID.into(),
            })?;
        self.runtime.record_model_phase(ModelPhase::Preparing);
        let result = runtime
            .prepare_with_context(
                &ProviderProfile {
                    id: DEFAULT_PROVIDER_ID.into(),
                    label: "llama.cpp".into(),
                    kind: ProviderKind::LocalLlamaCpp,
                    endpoint: DEFAULT_LLAMA_CPP_ENDPOINT.into(),
                    model,
                    selected: true,
                },
                context_tokens,
            )
            .await;
        self.runtime.record_model_phase(if result.is_ok() {
            ModelPhase::Prepared
        } else {
            ModelPhase::Failed
        });
        Ok(result?)
    }

    pub async fn activate_local_runtime(
        &self,
        prepared: PreparedLocalRuntime,
        approval: LocalRuntimeLaunchApproval,
    ) -> Result<Arc<LocalRuntimeClient>, HostError> {
        if prepared.plan().context_tokens
            != self
                .runtime
                .configured_local_context_tokens(&prepared.plan().model)?
        {
            return Err(ProviderError::LocalRuntimeApprovalStale.into());
        }
        let _backend = self
            .runtime
            .local_backend_lease("llama.cpp.sidecar")
            .await?;
        let runtime = self
            .local_runtime
            .as_ref()
            .ok_or(ProviderError::ProviderNotConfigured {
                provider_id: DEFAULT_PROVIDER_ID.into(),
            })?;
        self.runtime.record_model_phase(ModelPhase::Loading);
        let result = runtime
            .activate(
                prepared,
                Some(approval),
                tokio_util::sync::CancellationToken::new(),
            )
            .await;
        self.runtime.record_model_phase(if result.is_ok() {
            ModelPhase::Ready
        } else {
            ModelPhase::Failed
        });
        Ok(result?)
    }

    pub fn android_inference_configured(&self) -> bool {
        self.android_inference.is_some()
    }

    pub fn set_android_foreground_stop(
        &self,
        stop: argentum_providers::AndroidForegroundStop,
    ) -> Result<(), HostError> {
        Ok(self.runtime.set_android_foreground_stop(stop)?)
    }

    pub fn android_inference_control(&self) -> Option<AndroidInferenceControl> {
        Some(AndroidInferenceControl {
            runtime: Arc::clone(self.android_inference.as_ref()?),
            local: self.local_handle(),
        })
    }

    pub async fn prepare_android_inference(
        &self,
        descriptor: VerifiedModelDescriptor,
    ) -> Result<PreparedModel, HostError> {
        let _backend = self
            .runtime
            .local_backend_lease("llama.cpp.android")
            .await?;
        let runtime =
            self.android_inference
                .as_ref()
                .ok_or(ProviderError::ProviderNotConfigured {
                    provider_id: DEFAULT_PROVIDER_ID.into(),
                })?;
        Ok(runtime.lock().await.prepare(descriptor).await?)
    }

    pub async fn activate_android_inference(
        &self,
        prepared: PreparedModel,
        approval: AndroidInferenceApproval,
    ) -> Result<BackendReport, HostError> {
        let _backend = self
            .runtime
            .local_backend_lease("llama.cpp.android")
            .await?;
        let authority = self
            .local_authority
            .as_ref()
            .ok_or(RuntimeError::LocalAuthorityUnavailable)?;
        let generation = authority
            .platform_authority
            .current_generation()
            .ok_or(RuntimeError::LocalAuthorityUnavailable)?;
        let mut approval = approval.authorize(&prepared, &generation)?;
        let runtime =
            self.android_inference
                .as_ref()
                .ok_or(ProviderError::ProviderNotConfigured {
                    provider_id: DEFAULT_PROVIDER_ID.into(),
                })?;
        Ok(runtime
            .lock()
            .await
            .activate(prepared, &mut approval)
            .await?)
    }

    pub async fn unload_android_inference(&self) -> Result<(), HostError> {
        let _backend = self
            .runtime
            .local_backend_lease("llama.cpp.android")
            .await?;
        let runtime =
            self.android_inference
                .as_ref()
                .ok_or(ProviderError::ProviderNotConfigured {
                    provider_id: DEFAULT_PROVIDER_ID.into(),
                })?;
        runtime.lock().await.unload().await?;
        Ok(())
    }

    pub async fn android_inference_backend_report(&self) -> Option<BackendReport> {
        let runtime = self.android_inference.as_ref()?;
        runtime.lock().await.backend_report().cloned()
    }

    pub fn client(&self) -> InProcessClient {
        InProcessClient {
            runtime: self.runtime.clone(),
        }
    }

    pub fn local_handle(&self) -> LocalCommandHandle {
        LocalCommandHandle {
            runtime: self.runtime.clone(),
            local_runtime: self.local_runtime.clone(),
            local_authority: self.local_authority.clone(),
            secret_store: Arc::clone(&self.secret_store),
            credential_transaction: Arc::clone(&self.credential_transaction),
        }
    }

    pub fn subscribe(&self) -> broadcast::Receiver<AppEvent> {
        self.runtime.subscribe()
    }

    pub fn provider_statuses(&self) -> Vec<ProviderStatus> {
        self.runtime.provider_statuses()
    }

    pub fn provider_profiles(&self) -> Result<Vec<ProviderProfile>, HostError> {
        Ok(self.runtime.provider_profiles()?)
    }

    pub fn provider_capabilities(
        &self,
        profile: &ProviderProfile,
    ) -> Result<argentum_providers::ProviderCapabilityResolution, HostError> {
        Ok(self.runtime.provider_capabilities(profile)?)
    }

    pub fn selected_provider_coding_issue(&self) -> Result<Option<String>, HostError> {
        if self
            .runtime
            .provider_profiles()?
            .iter()
            .any(|p| p.selected && p.id == "chatgpt")
            && !self.provider_credential_configured("chatgpt")
        {
            return Ok(Some("ChatGPT browser sign-in is missing; use 'argentum provider login chatgpt'. No request sent.".into()));
        }
        Ok(self.runtime.selected_coding_readiness().err().map(|e|
            format!("{e}. Open /context or use 'argentum provider budget PROFILE_ID --limits TOTAL,INPUT,MAX_OUTPUT,REQUESTED_OUTPUT,UNCERTAINTY' with the same workspace and database. No request sent.")))
    }

    pub fn declare_model_budget(
        &self,
        profile_id: &str,
        limits: [u64; 5],
    ) -> Result<argentum_domain::context_budget::ModelBudget, HostError> {
        Ok(self.runtime.declare_model_budget(profile_id, limits)?)
    }

    pub fn configure_local_context(
        &self,
        profile_id: &str,
        context_tokens: u32,
    ) -> Result<argentum_domain::context_budget::ModelBudget, HostError> {
        Ok(self
            .runtime
            .configure_local_context(profile_id, context_tokens)?)
    }

    pub fn configured_local_context_tokens(&self, model: &str) -> Result<u32, HostError> {
        Ok(self.runtime.configured_local_context_tokens(model)?)
    }

    pub fn selected_model_budget(
        &self,
    ) -> Result<Option<argentum_domain::context_budget::ModelBudget>, HostError> {
        Ok(self.runtime.selected_model_budget()?)
    }

    pub async fn compact_context(
        &self,
        deactivate: bool,
    ) -> Result<argentum_domain::context_summary::CompactionResult, HostError> {
        Ok(self
            .runtime
            .compact_context(
                self.runtime.session_id(),
                argentum_domain::RunId::new_v4(),
                deactivate,
            )
            .await?)
    }

    pub fn publish_initial_state(&self) -> Result<(), HostError> {
        publish_runtime_ui_state(&self.runtime)
    }

    pub fn harness_snapshot(&self) -> Result<HarnessSnapshot, HostError> {
        Ok(self.runtime.harness_snapshot()?)
    }

    pub fn instruction_preview(
        &self,
        directory: &str,
    ) -> Result<argentum_runtime::instructions::InstructionPreview, HostError> {
        Ok(self.runtime.instruction_preview(directory)?)
    }

    pub fn context_preview(
        &self,
    ) -> Result<argentum_runtime::working_context::ContextPreview, HostError> {
        Ok(self.runtime.context_preview(self.runtime.session_id())?)
    }

    pub fn workspace_snapshot(&self) -> Result<WorkspaceSnapshot, HostError> {
        Ok(self.runtime.workspace_snapshot()?)
    }

    pub fn list_sessions(
        &self,
        query: &str,
        filter: argentum_domain::SessionArchiveFilter,
        limit: u16,
        cursor: Option<&str>,
    ) -> Result<argentum_domain::SessionListSnapshot, HostError> {
        Ok(self.runtime.list_sessions(query, filter, limit, cursor)?)
    }

    pub fn inspect_session(
        &self,
        session_id: argentum_domain::SessionId,
    ) -> Result<argentum_domain::SessionListRow, HostError> {
        Ok(self.runtime.inspect_session(session_id)?)
    }

    pub fn session_fork_preview(
        &self,
        source_session_id: argentum_domain::SessionId,
        title: &str,
    ) -> Result<argentum_domain::SessionForkPreview, HostError> {
        Ok(self
            .runtime
            .session_fork_preview(source_session_id, title)?)
    }

    pub fn fork_session(
        &self,
        source_session_id: argentum_domain::SessionId,
        title: &str,
        expected_last_message_id: Option<argentum_domain::MessageId>,
    ) -> Result<argentum_domain::SessionForkSnapshot, HostError> {
        Ok(self
            .runtime
            .fork_session(source_session_id, title, expected_last_message_id)?)
    }

    pub fn final_run_result(
        &self,
        run_id: argentum_domain::RunId,
    ) -> Result<Option<argentum_domain::final_result::FinalRunResult>, HostError> {
        Ok(self.runtime.final_run_result(run_id)?)
    }

    pub fn onboarding_snapshot(&self) -> Result<OnboardingSnapshot, HostError> {
        Ok(self.runtime.onboarding_snapshot()?)
    }

    pub fn platform_authority_snapshot(&self) -> Option<PlatformAuthoritySnapshot> {
        self.runtime.platform_authority_snapshot()
    }

    pub fn goal(&self) -> Result<Option<Goal>, HostError> {
        Ok(self.runtime.goal()?)
    }
    pub fn inspect_goal_accounting(
        &self,
    ) -> Result<argentum_domain::goal_accounting::GoalInspection, HostError> {
        Ok(self.runtime.inspect_goal_accounting()?)
    }

    pub fn provider_credential_configured(&self, profile_id: &str) -> bool {
        self.runtime.provider_credential_configured(profile_id)
    }

    pub fn chatgpt_auth(
        &self,
    ) -> Result<
        argentum_providers::chatgpt_auth::ChatGptAuth,
        argentum_providers::chatgpt_auth::AuthError,
    > {
        argentum_providers::chatgpt_auth::ChatGptAuth::new(Arc::clone(&self.secret_store))
    }

    pub fn set_provider_credential(
        &self,
        profile_id: &str,
        credential: SecretValue,
    ) -> Result<(), HostError> {
        set_hosted_provider_credential(
            &self.runtime,
            self.secret_store.as_ref(),
            &self.credential_transaction,
            profile_id,
            credential,
        )
    }

    pub fn clear_provider_credential(&self, profile_id: &str) -> Result<(), HostError> {
        clear_hosted_provider_credential(
            &self.runtime,
            self.secret_store.as_ref(),
            &self.credential_transaction,
            profile_id,
        )
    }

    pub fn workspace_root(&self) -> &Path {
        self.runtime.workspace_root()
    }

    pub fn skill_catalog(&self) -> Result<argentum_workspaces::SkillCatalog, HostError> {
        Ok(self.runtime.skill_catalog()?)
    }

    pub fn inspect_skill(
        &self,
        name: &str,
    ) -> Result<argentum_workspaces::SkillInspection, HostError> {
        Ok(self.runtime.inspect_skill(name)?)
    }

    pub fn select_skill(
        &self,
        name: &str,
    ) -> Result<argentum_workspaces::SelectedSkill, HostError> {
        Ok(self.runtime.select_skill(name)?)
    }

    pub(crate) fn project_id(&self) -> argentum_domain::ProjectId {
        self.runtime.project_id()
    }

    pub(crate) fn durable_command_match(
        &self,
        request_id: &str,
        canonicalization_version: u16,
        canonical_command: &[u8],
    ) -> Result<argentum_store::CommandIdentityMatch, HostError> {
        Ok(self.runtime.durable_command_match(
            request_id,
            canonicalization_version,
            canonical_command,
        )?)
    }

    pub(crate) fn durable_command_identity(
        &self,
        request_id: &str,
    ) -> Result<Option<argentum_store::StoredCommandIdentity>, HostError> {
        Ok(self.runtime.durable_command_identity(request_id)?)
    }

    pub(crate) fn resolve_durable_command_scope(
        &self,
        command: &AppCommand,
    ) -> Result<argentum_store::CommandIdentityScope, HostError> {
        Ok(self.runtime.resolve_durable_command_scope(command)?)
    }

    pub(crate) fn claim_durable_command(
        &self,
        input: &argentum_store::CommandClaimInput,
    ) -> Result<argentum_store::CommandClaim, HostError> {
        Ok(self.runtime.claim_durable_command(input)?)
    }

    pub(crate) fn claim_durable_new_session(
        &self,
        input: &argentum_store::CommandClaimInput,
    ) -> Result<argentum_store::NewSessionCommandClaim, HostError> {
        Ok(self.runtime.claim_durable_new_session(input)?)
    }

    pub(crate) fn claim_durable_fork_session(
        &self,
        input: &argentum_store::CommandClaimInput,
        title: &str,
        expected_last_message_id: Option<argentum_domain::MessageId>,
    ) -> Result<argentum_store::ForkSessionCommandClaim, HostError> {
        Ok(self
            .runtime
            .claim_durable_fork_session(input, title, expected_last_message_id)?)
    }

    pub fn tool_descriptors(&self) -> Vec<ToolDescriptor> {
        self.runtime.tool_descriptors()
    }

    pub fn execution_policy(&self) -> Result<HarnessExecutionPolicy, HostError> {
        Ok(self.runtime.execution_policy()?)
    }

    pub fn current_verification_revision(&self) -> Result<Sha256Digest, HostError> {
        Ok(self.runtime.current_verification_revision()?)
    }

    pub fn project_verification_review(
        &self,
        runner: &argentum_domain::VerificationRunnerId,
    ) -> Result<argentum_domain::VerificationRunnerReview, HostError> {
        Ok(self.runtime.project_verification_review(runner)?)
    }

    pub fn verification_evidence_is_current(
        &self,
        evidence: &argentum_domain::VerificationEvidence,
    ) -> Result<bool, HostError> {
        Ok(self.runtime.verification_evidence_is_current(evidence)?)
    }

    pub async fn harness_lifecycle_snapshot(&self) -> Option<HostLifecycleSnapshot> {
        self.lifecycle_observer().harness_lifecycle_snapshot().await
    }

    pub fn lifecycle_observer(&self) -> CommandHostObserver {
        CommandHostObserver {
            runtime: self.runtime.clone(),
        }
    }
}

async fn bootstrap_host_authority(
) -> Result<(Arc<LocalAuthorityOwner>, RuntimeHostAuthority), HostError> {
    let (issuer, verifier, generation) = host_authority_channel()?;
    let platform_authority = Arc::new(PlatformAuthorityBroker::bootstrap(generation).await?);
    let owner = Arc::new(LocalAuthorityOwner {
        issuer,
        platform_authority: Arc::clone(&platform_authority),
    });
    Ok((
        owner,
        RuntimeHostAuthority::new(verifier, platform_authority),
    ))
}

fn bootstrap_host_authority_blocking(
) -> Result<(Arc<LocalAuthorityOwner>, RuntimeHostAuthority), HostError> {
    let (issuer, verifier, generation) = host_authority_channel()?;
    let platform_authority = std::thread::spawn(move || {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|_| PlatformError::AuthorityLifecycleFailed)?
            .block_on(PlatformAuthorityBroker::bootstrap(generation))
    })
    .join()
    .map_err(|_| PlatformError::AuthorityLifecycleFailed)??;
    let platform_authority = Arc::new(platform_authority);
    let owner = Arc::new(LocalAuthorityOwner {
        issuer,
        platform_authority: Arc::clone(&platform_authority),
    });
    Ok((
        owner,
        RuntimeHostAuthority::new(verifier, platform_authority),
    ))
}

fn validate_hosted_credential_profile(profile_id: &str) -> Result<(), HostError> {
    if is_supported_hosted_credential_provider_id(profile_id) {
        Ok(())
    } else {
        Err(HostError::ProviderCredentialUnsupported)
    }
}

fn publish_runtime_ui_state(runtime: &RuntimeService) -> Result<(), HostError> {
    runtime.publish_platform_authority_snapshot();
    runtime.publish_harness_snapshot()?;
    runtime.publish_layout()?;
    runtime.publish_workspace_state()?;
    runtime.publish_provider_profiles()?;
    runtime.publish_provider_statuses();
    runtime.publish_provider_credential_statuses();
    runtime.publish_onboarding_snapshot()?;
    Ok(())
}

fn set_hosted_provider_credential(
    runtime: &RuntimeService,
    secret_store: &dyn SecretStore,
    credential_transaction: &Mutex<()>,
    profile_id: &str,
    credential: SecretValue,
) -> Result<(), HostError> {
    let _transaction = credential_transaction
        .lock()
        .map_err(|_| HostError::ProviderCredentialTransactionUnavailable)?;
    validate_hosted_credential_profile(profile_id)?;
    ProviderCredentials::default().insert(profile_id, credential.clone())?;
    let key = provider_credential_key(profile_id)?;
    let previous = secret_store_result(secret_store.get(&key))?;
    if secret_store.set(&key, credential.clone()).is_err() {
        let _ = restore_hosted_provider_credential(secret_store, &key, previous);
        return Err(HostError::ProviderCredentialStorageUnavailable);
    }
    if let Err(error) = runtime.set_provider_credential(profile_id, credential) {
        if restore_hosted_provider_credential(secret_store, &key, previous).is_err() {
            return Err(HostError::ProviderCredentialStorageUnavailable);
        }
        return Err(error.into());
    }
    refresh_provider_credential_projection(runtime, profile_id, "update");
    Ok(())
}

fn restore_hosted_provider_credential(
    secret_store: &dyn SecretStore,
    key: &str,
    previous: Option<SecretValue>,
) -> Result<(), HostError> {
    match previous {
        Some(previous) => secret_store_result(secret_store.set(key, previous)),
        None => secret_store_result(secret_store.delete(key)),
    }
}

fn clear_hosted_provider_credential(
    runtime: &RuntimeService,
    secret_store: &dyn SecretStore,
    credential_transaction: &Mutex<()>,
    profile_id: &str,
) -> Result<(), HostError> {
    let _transaction = credential_transaction
        .lock()
        .map_err(|_| HostError::ProviderCredentialTransactionUnavailable)?;
    validate_hosted_credential_profile(profile_id)?;
    let key = provider_credential_key(profile_id)?;
    let previous = secret_store_result(secret_store.get(&key))?;
    if secret_store.delete(&key).is_err() {
        let _ = restore_hosted_provider_credential(secret_store, &key, previous);
        return Err(HostError::ProviderCredentialStorageUnavailable);
    }
    runtime.clear_provider_credential(profile_id);
    refresh_provider_credential_projection(runtime, profile_id, "removal");
    Ok(())
}

fn secret_store_result<T>(result: Result<T, PlatformError>) -> Result<T, HostError> {
    result.map_err(|_| HostError::ProviderCredentialStorageUnavailable)
}

fn refresh_provider_credential_projection(
    runtime: &RuntimeService,
    profile_id: &str,
    operation: &str,
) {
    runtime.publish_provider_credential_status(profile_id);
    if let Err(error_value) = runtime.publish_harness_snapshot() {
        warn!(error = %error_value, operation, "unable to refresh harness state after credential change");
    }
}

#[derive(Clone, Debug)]
pub struct InProcessClient {
    runtime: RuntimeService,
}

#[derive(Clone)]
pub struct LocalCommandHandle {
    runtime: RuntimeService,
    local_runtime: Option<Arc<LocalRuntimeModule>>,
    local_authority: Option<Arc<LocalAuthorityOwner>>,
    secret_store: Arc<dyn SecretStore>,
    credential_transaction: Arc<Mutex<()>>,
}

#[derive(Debug, Clone)]
pub struct LocalAccessReview {
    pub review: argentum_runtime::AccessReview,
    generation: argentum_security::AuthorityGeneration,
}

impl std::fmt::Debug for LocalCommandHandle {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("LocalCommandHandle")
            .field("authority_available", &self.local_authority.is_some())
            .finish()
    }
}

impl LocalCommandHandle {
    #[cfg(feature = "extensions")]
    pub async fn set_extension_secret(
        &self,
        namespace: &str,
        secret: SecretValue,
    ) -> Result<(), HostError> {
        Ok(self.runtime.set_extension_secret(namespace, secret).await?)
    }
    #[cfg(feature = "extensions")]
    pub fn extension_records(
        &self,
    ) -> Result<Vec<argentum_domain::extensions::ExtensionRecord>, HostError> {
        Ok(self.runtime.extension_records()?)
    }
    #[cfg(feature = "extensions")]
    pub fn extension_readiness(&self) -> Result<Vec<serde_json::Value>, HostError> {
        Ok(self.runtime.extension_readiness()?)
    }
    #[cfg(feature = "extensions")]
    pub fn extension_receipts(
        &self,
        run: Option<argentum_domain::RunId>,
    ) -> Result<serde_json::Value, HostError> {
        Ok(self.runtime.extension_receipts(run)?)
    }

    #[cfg(feature = "extensions")]
    pub async fn configure_extension(
        &self,
        definition: argentum_domain::extensions::ExtensionDefinition,
    ) -> Result<argentum_domain::extensions::ExtensionRecord, HostError> {
        Ok(self.runtime.configure_extension(definition).await?)
    }

    #[cfg(feature = "extensions")]
    pub async fn activate_extension(
        &self,
        namespace: &str,
    ) -> Result<argentum_domain::extensions::ExtensionRecord, HostError> {
        let authority = self
            .local_authority
            .as_ref()
            .ok_or(RuntimeError::LocalAuthorityUnavailable)?;
        let action = self.runtime.extension_activation_action(namespace)?;
        let generation = authority
            .platform_authority
            .current_generation()
            .ok_or(RuntimeError::LocalAuthorityUnavailable)?;
        let ticket = authority.issuer.issue(action, generation)?;
        Ok(self.runtime.activate_extension(namespace, ticket).await?)
    }

    #[cfg(feature = "extensions")]
    pub async fn disable_extension(&self, namespace: &str, remove: bool) -> Result<(), HostError> {
        Ok(self.runtime.disable_extension(namespace, remove).await?)
    }
    pub fn chatgpt_auth(
        &self,
    ) -> Result<
        argentum_providers::chatgpt_auth::ChatGptAuth,
        argentum_providers::chatgpt_auth::AuthError,
    > {
        argentum_providers::chatgpt_auth::ChatGptAuth::new(Arc::clone(&self.secret_store))
    }
    pub fn steer_active_run(
        &self,
        submission: argentum_runtime::SteeringSubmission,
    ) -> Result<argentum_runtime::SteeringReceipt, HostError> {
        Ok(self.runtime.steer_active_run(submission)?)
    }

    pub fn model_choices(&self, profile: &ProviderProfile) -> Result<Vec<String>, HostError> {
        let mut names = vec![profile.model.clone()];
        if let Some(models) = self.runtime.cached_profile_model_catalog(&profile.id)? {
            names.extend(models.into_iter().take(128).map(|m| m.id));
        }
        if profile.kind == ProviderKind::LocalLlamaCpp {
            if let Some(module) = &self.local_runtime {
                names.extend(module.installed_model_names());
            }
        }
        let mut seen = std::collections::BTreeSet::new();
        names.retain(|n| seen.insert(n.clone()));
        names.truncate(128);
        Ok(names)
    }
    pub fn configure_local_context(
        &self,
        id: &str,
        size: u32,
    ) -> Result<argentum_domain::context_budget::ModelBudget, HostError> {
        Ok(self.runtime.configure_local_context(id, size)?)
    }
    pub fn selected_model_budget(
        &self,
    ) -> Result<Option<argentum_domain::context_budget::ModelBudget>, HostError> {
        Ok(self.runtime.selected_model_budget()?)
    }
    pub fn profile_model_budget(
        &self,
        p: &ProviderProfile,
    ) -> Result<Option<argentum_domain::context_budget::ModelBudget>, HostError> {
        Ok(self.runtime.profile_model_budget(p)?)
    }
    pub fn seed_selected_model_budget(&self) -> Result<bool, HostError> {
        if self.runtime.selected_model_budget()?.is_some() {
            return Ok(true);
        }
        let Some(profile) = self.provider_profiles()?.into_iter().find(|p| p.selected) else {
            return Ok(false);
        };
        if profile.kind == ProviderKind::LocalLlamaCpp {
            self.runtime
                .configure_local_context(&profile.id, argentum_providers::SIDECAR_CONTEXT_TOKENS)?;
            return Ok(true);
        }
        // Hosted declarations require explicit owner input. Catalog capacity is
        // a ceiling, not an input/output/uncertainty policy.
        Ok(false)
    }
    pub async fn local_model_ready(&self, model: &str) -> bool {
        let Some(module) = &self.local_runtime else {
            return false;
        };
        match (
            module.client(model).await,
            self.runtime.configured_local_context_tokens(model),
        ) {
            (Ok(c), Ok(n)) => c.context_tokens() == n && c.verify_lease().await.is_ok(),
            _ => false,
        }
    }
    pub async fn prepare_local_model(
        &self,
        model: String,
    ) -> Result<PreparedLocalRuntime, HostError> {
        let _lease = self
            .runtime
            .local_backend_lease("llama.cpp.sidecar")
            .await?;
        let module = self
            .local_runtime
            .as_ref()
            .ok_or(ProviderError::LocalRuntimeUnavailable {
                reason: "local runtime is not configured",
            })?;
        let size = self.runtime.configured_local_context_tokens(&model)?;
        Ok(module
            .prepare_with_context(
                &ProviderProfile {
                    id: DEFAULT_PROVIDER_ID.into(),
                    label: "llama.cpp".into(),
                    kind: ProviderKind::LocalLlamaCpp,
                    endpoint: DEFAULT_LLAMA_CPP_ENDPOINT.into(),
                    model,
                    selected: true,
                },
                size,
            )
            .await?)
    }
    pub async fn activate_local_model(
        &self,
        prepared: PreparedLocalRuntime,
        cancellation: tokio_util::sync::CancellationToken,
    ) -> Result<(), HostError> {
        if cancellation.is_cancelled() {
            return Err(ProviderError::LocalRuntimeCancelled.into());
        }
        if !self
            .provider_profiles()?
            .iter()
            .any(|p| p.id == DEFAULT_PROVIDER_ID && p.model == prepared.plan().model)
        {
            return Err(ProviderError::LocalRuntimeApprovalStale.into());
        }
        if prepared.plan().context_tokens
            != self
                .runtime
                .configured_local_context_tokens(&prepared.plan().model)?
        {
            return Err(ProviderError::LocalRuntimeApprovalStale.into());
        }
        let _lease = self
            .runtime
            .local_backend_lease("llama.cpp.sidecar")
            .await?;
        let module = self
            .local_runtime
            .as_ref()
            .ok_or(ProviderError::LocalRuntimeUnavailable {
                reason: "local runtime is not configured",
            })?;
        let approval = self.approve_local_runtime(prepared.plan())?;
        module
            .activate(prepared, Some(approval), cancellation)
            .await?;
        Ok(())
    }
    pub async fn unload_local_model(&self) -> Result<(), HostError> {
        if let Some(module) = &self.local_runtime {
            module
                .shutdown(tokio_util::sync::CancellationToken::new())
                .await?;
        }
        Ok(())
    }
    pub async fn refresh_model_choices(
        &self,
        id: String,
        cancel: tokio_util::sync::CancellationToken,
    ) -> Result<(), HostError> {
        Ok(self
            .runtime
            .list_provider_models_with_cancellation(id, cancel)
            .await?)
    }
    pub fn select_skill(
        &self,
        name: &str,
    ) -> Result<argentum_workspaces::SelectedSkill, HostError> {
        Ok(self.runtime.select_skill(name)?)
    }
    pub fn review_page(
        &self,
        project: argentum_domain::ProjectId,
        session: argentum_domain::SessionId,
        request: argentum_runtime::review::ReviewRequest,
    ) -> Result<argentum_runtime::review::ReviewPage, HostError> {
        Ok(self.runtime.review_page(project, session, request)?)
    }
    pub fn historical_change_page(
        &self,
        run: argentum_domain::RunId,
        limit: usize,
        after: Option<argentum_domain::ChangePageCursor>,
    ) -> Result<argentum_domain::ChangePage, HostError> {
        Ok(self.runtime.historical_change_page(run, limit, after)?)
    }
    pub fn review_policy_stamp(&self) -> Result<String, HostError> {
        Ok(self.runtime.review_policy_stamp()?)
    }
    pub fn review_body_allowed(&self, path: &str) -> Result<bool, HostError> {
        Ok(self.runtime.review_body_allowed(path)?)
    }
    pub fn local_text_input_capacity(&self) -> Result<usize, HostError> {
        Ok(self.runtime.local_text_input_capacity()?)
    }
    pub fn verification_summary(&self) -> Result<String, HostError> {
        Ok(self.runtime.verification_summary()?)
    }
    pub fn runtime_facts(&self, local_text: bool) -> Result<serde_json::Value, HostError> {
        Ok(self.runtime.runtime_facts(local_text)?)
    }
    pub fn harness_snapshot(&self) -> Result<HarnessSnapshot, HostError> {
        Ok(self.runtime.harness_snapshot()?)
    }
    pub fn workspace_snapshot(&self) -> Result<WorkspaceSnapshot, HostError> {
        Ok(self.runtime.workspace_snapshot()?)
    }
    pub fn session_fork_preview(
        &self,
        source_session_id: argentum_domain::SessionId,
        title: &str,
    ) -> Result<argentum_domain::SessionForkPreview, HostError> {
        Ok(self
            .runtime
            .session_fork_preview(source_session_id, title)?)
    }
    pub fn fork_session(
        &self,
        source_session_id: argentum_domain::SessionId,
        title: &str,
        expected_last_message_id: Option<argentum_domain::MessageId>,
    ) -> Result<argentum_domain::SessionForkSnapshot, HostError> {
        Ok(self
            .runtime
            .fork_session(source_session_id, title, expected_last_message_id)?)
    }
    pub fn provider_profiles(&self) -> Result<Vec<ProviderProfile>, HostError> {
        Ok(self.runtime.provider_profiles()?)
    }
    pub fn tool_descriptors(&self) -> Vec<ToolDescriptor> {
        self.runtime.tool_descriptors()
    }
    pub fn skill_catalog(&self) -> Result<argentum_workspaces::SkillCatalog, HostError> {
        Ok(self.runtime.skill_catalog()?)
    }
    pub fn access_review(&self, profile_id: &str) -> Result<LocalAccessReview, HostError> {
        let owner = self
            .local_authority
            .as_ref()
            .ok_or(RuntimeError::LocalAuthorityUnavailable)?;
        let generation = owner
            .platform_authority
            .current_generation()
            .ok_or(RuntimeError::LocalAuthorityUnavailable)?;
        Ok(LocalAccessReview {
            review: self.runtime.access_review(profile_id)?,
            generation,
        })
    }
    pub async fn apply_access_review(&self, reviewed: LocalAccessReview) -> Result<(), HostError> {
        let owner = self
            .local_authority
            .as_ref()
            .ok_or(RuntimeError::LocalAuthorityUnavailable)?;
        if owner.platform_authority.current_generation().as_ref() != Some(&reviewed.generation) {
            return Err(RuntimeError::DurableCommandSnapshotChanged.into());
        }
        let ticket = if reviewed.review.increase {
            Some(owner.issuer.issue(
                LocalAuthorityAction::SelectExecutionPolicy {
                    current: argentum_security::execution_policy_digest(&reviewed.review.current),
                    requested: argentum_security::execution_policy_digest(
                        &reviewed.review.proposed,
                    ),
                },
                reviewed.generation,
            )?)
        } else {
            None
        };
        Ok(self
            .runtime
            .apply_access_review(reviewed.review, ticket)
            .await?)
    }

    pub fn history_page(
        &self,
        session: argentum_domain::SessionId,
        cursor: Option<argentum_domain::MessageId>,
        newer: bool,
    ) -> Result<argentum_store::HistoryPage, HostError> {
        Ok(self.runtime.history_page(session, cursor, newer)?)
    }

    /// An explicitly selected local text conversation, never a coding task.
    pub async fn stream_local_text(
        &self,
        prompt: String,
        history: Vec<argentum_providers::ModelMessage>,
        sender: tokio::sync::mpsc::Sender<argentum_providers::ProviderEvent>,
        cancellation: tokio_util::sync::CancellationToken,
    ) -> Result<(), HostError> {
        Ok(self
            .runtime
            .stream_local_text(prompt, history, sender, cancellation)
            .await?)
    }

    pub fn selected_provider_coding_issue(&self) -> Result<Option<String>, HostError> {
        if self
            .runtime
            .provider_profiles()?
            .iter()
            .any(|p| p.selected && p.id == "chatgpt")
            && !self.provider_credential_configured("chatgpt")
        {
            return Ok(Some("ChatGPT browser sign-in is missing; use 'argentum provider login chatgpt'. No request sent.".into()));
        }
        Ok(self.runtime.selected_coding_readiness().err().map(|e|
            format!("{e}. Open /context or use 'argentum provider budget PROFILE_ID --limits TOTAL,INPUT,MAX_OUTPUT,REQUESTED_OUTPUT,UNCERTAINTY' with the same workspace and database. No request sent.")))
    }

    pub fn set_retained_session_work(
        &self,
        session_id: argentum_domain::SessionId,
        retained: usize,
    ) -> Result<(), HostError> {
        Ok(self
            .runtime
            .set_retained_session_work(session_id, retained)?)
    }

    pub fn goal_accounting_snapshot(&self) -> Result<Option<Goal>, HostError> {
        Ok(self.runtime.goal()?)
    }
    pub fn configure_goal_acceptance(
        &self,
        goal_id: argentum_domain::GoalId,
        runners: &[argentum_domain::VerificationRunnerId],
        owner_attestation_required: bool,
    ) -> Result<argentum_domain::Goal, HostError> {
        Ok(self
            .runtime
            .configure_goal_acceptance(goal_id, runners, owner_attestation_required)?)
    }
    pub fn attest_goal_acceptance(
        &self,
        goal_id: argentum_domain::GoalId,
        summary: &str,
    ) -> Result<argentum_domain::Goal, HostError> {
        Ok(self.runtime.attest_goal_acceptance(goal_id, summary)?)
    }
    pub fn inspect_goal_supervision(
        &self,
        goal_id: argentum_domain::GoalId,
    ) -> Result<argentum_domain::goal_supervision::GoalSupervisionInspection, HostError> {
        Ok(self.runtime.inspect_goal_supervision(goal_id)?)
    }
    pub async fn supervise_goal(
        &self,
        request: argentum_domain::goal_supervision::SupervisionRequest,
    ) -> Result<argentum_domain::goal_supervision::GoalInvocationStatus, HostError> {
        Ok(self.runtime.supervise_goal(request).await?)
    }
    pub async fn deliver_goal(
        &self,
        request: argentum_domain::goal_supervision::SupervisionRequest,
    ) -> Result<argentum_domain::goal_supervision::GoalInvocationStatus, HostError> {
        Ok(self.runtime.deliver_goal(request).await?)
    }
    pub fn cancel_goal_invocation(
        &self,
        invocation_id: argentum_domain::GoalId,
    ) -> Result<(), HostError> {
        Ok(self.runtime.cancel_goal_invocation(invocation_id)?)
    }
    pub fn configure_goal_limits(
        &self,
        goal_id: argentum_domain::GoalId,
        limits: [u64; 9],
    ) -> Result<argentum_domain::Goal, HostError> {
        Ok(self.runtime.configure_goal_limits(goal_id, limits)?)
    }
    pub fn remove_goal_aggregate_limits(
        &self,
        goal_id: argentum_domain::GoalId,
    ) -> Result<argentum_domain::Goal, HostError> {
        Ok(self.runtime.remove_goal_aggregate_limits(goal_id)?)
    }
    pub async fn compact_goal_context(
        &self,
        session: argentum_domain::SessionId,
        operation: argentum_domain::RunId,
        goal_id: argentum_domain::GoalId,
    ) -> Result<argentum_domain::context_summary::CompactionResult, HostError> {
        let goal = self
            .runtime
            .goal()?
            .ok_or(argentum_runtime::RuntimeError::GoalMissing)?;
        let account =
            goal.accounting
                .as_ref()
                .ok_or(argentum_store::StoreError::GoalAccounting(
                    "legacy_usage_history_unavailable",
                ))?;
        if goal.id != goal_id || goal.session_id != session {
            return Err(argentum_runtime::RuntimeError::SubmissionTargetChanged.into());
        }
        Ok(self
            .runtime
            .compact_context_scoped(
                session,
                operation,
                false,
                tokio_util::sync::CancellationToken::new(),
                Some(argentum_domain::goal_accounting::GoalBinding {
                    goal_id,
                    revision: account.revision,
                }),
            )
            .await?)
    }
    pub async fn submit_goal_task(
        &self,
        prompt: String,
        goal_id: argentum_domain::GoalId,
    ) -> Result<(), HostError> {
        let goal = self
            .runtime
            .goal()?
            .ok_or(argentum_runtime::RuntimeError::GoalMissing)?;
        let account =
            goal.accounting
                .as_ref()
                .ok_or(argentum_store::StoreError::GoalAccounting(
                    "legacy_usage_history_unavailable",
                ))?;
        if goal.id != goal_id {
            return Err(argentum_runtime::RuntimeError::SubmissionTargetChanged.into());
        }
        let submission = argentum_runtime::TaskSubmission {
            id: argentum_domain::TaskId::new_v4(),
            run_id: argentum_domain::RunId::new_v4(),
            prompt,
            skill: None,
            answer_contract: None,
            target: argentum_runtime::SubmissionTarget {
                project_id: goal.project_id,
                session_id: goal.session_id,
                workspace: self.runtime.workspace_root().to_path_buf(),
            },
        };
        let result = self
            .runtime
            .submit_goal_bound_task(
                submission,
                argentum_domain::goal_accounting::GoalBinding {
                    goal_id,
                    revision: account.revision,
                },
            )
            .await;
        result.result?;
        Ok(())
    }
    pub async fn submit_goal_bound_task(
        &self,
        submission: argentum_runtime::TaskSubmission,
        binding: argentum_domain::goal_accounting::GoalBinding,
    ) -> argentum_runtime::TaskSubmissionResult {
        self.runtime
            .submit_goal_bound_task(submission, binding)
            .await
    }
    pub async fn submit_goal_entry(
        &self,
        submission: argentum_runtime::TaskSubmission,
        goal_id: argentum_domain::GoalId,
    ) -> argentum_runtime::TaskSubmissionResult {
        let binding = (|| -> Result<_, argentum_runtime::RuntimeError> {
            let goal = self
                .runtime
                .goal()?
                .ok_or(argentum_runtime::RuntimeError::GoalMissing)?;
            if goal.id != goal_id
                || goal.project_id != submission.target.project_id
                || goal.session_id != submission.target.session_id
            {
                return Err(argentum_runtime::RuntimeError::SubmissionTargetChanged);
            }
            let account =
                goal.accounting
                    .as_ref()
                    .ok_or(argentum_store::StoreError::GoalAccounting(
                        "legacy_usage_history_unavailable",
                    ))?;
            Ok(argentum_domain::goal_accounting::GoalBinding {
                goal_id,
                revision: account.revision,
            })
        })();
        match binding {
            Ok(binding) => {
                self.runtime
                    .submit_goal_bound_task(submission, binding)
                    .await
            }
            Err(error) => argentum_runtime::TaskSubmissionResult {
                id: submission.id,
                run_id: submission.run_id,
                admitted: false,
                terminal_lifecycle: None,
                result: Err(error),
            },
        }
    }
    pub async fn compact_context_with_cancellation(
        &self,
        session: argentum_domain::SessionId,
        operation: argentum_domain::RunId,
        deactivate: bool,
        cancellation: tokio_util::sync::CancellationToken,
    ) -> Result<argentum_domain::context_summary::CompactionResult, HostError> {
        Ok(self
            .runtime
            .compact_context_with_cancellation(session, operation, deactivate, cancellation)
            .await?)
    }
    pub fn declare_model_budget(
        &self,
        profile_id: &str,
        limits: [u64; 5],
    ) -> Result<argentum_domain::context_budget::ModelBudget, HostError> {
        Ok(self.runtime.declare_model_budget(profile_id, limits)?)
    }
    pub async fn compact_context(
        &self,
        session: argentum_domain::SessionId,
        operation: argentum_domain::RunId,
        deactivate: bool,
    ) -> Result<argentum_domain::context_summary::CompactionResult, HostError> {
        Ok(self
            .runtime
            .compact_context(session, operation, deactivate)
            .await?)
    }
    pub fn context_preview(
        &self,
        session_id: argentum_domain::SessionId,
    ) -> Result<argentum_runtime::working_context::ContextPreview, HostError> {
        Ok(self.runtime.context_preview(session_id)?)
    }
    pub async fn submit_bound_task(
        &self,
        submission: argentum_runtime::TaskSubmission,
    ) -> argentum_runtime::TaskSubmissionResult {
        self.runtime.submit_bound_task(submission).await
    }

    pub fn provider_credential_configured(&self, profile_id: &str) -> bool {
        self.runtime.provider_credential_configured(profile_id)
    }

    pub fn set_provider_credential(
        &self,
        profile_id: &str,
        credential: SecretValue,
    ) -> Result<(), HostError> {
        set_hosted_provider_credential(
            &self.runtime,
            self.secret_store.as_ref(),
            &self.credential_transaction,
            profile_id,
            credential,
        )
    }

    pub fn clear_provider_credential(&self, profile_id: &str) -> Result<(), HostError> {
        clear_hosted_provider_credential(
            &self.runtime,
            self.secret_store.as_ref(),
            &self.credential_transaction,
            profile_id,
        )
    }

    pub fn approve_local_runtime(
        &self,
        plan: &LocalRuntimeLaunchPlan,
    ) -> Result<LocalRuntimeLaunchApproval, HostError> {
        let authority = self
            .local_authority
            .as_ref()
            .ok_or(RuntimeError::LocalAuthorityUnavailable)?;
        let action = LocalAuthorityAction::ActivateLocalRuntime {
            plan: plan.digest(),
        };
        let generation = authority
            .platform_authority
            .current_generation()
            .ok_or(RuntimeError::LocalAuthorityUnavailable)?;
        let ticket = authority.issuer.issue(action.clone(), generation)?;
        let proof = self
            .runtime
            .verify_local_authority_ticket(ticket, &action)?;
        Ok(LocalRuntimeLaunchApproval::bind(plan, proof)?)
    }

    pub fn approve_android_inference(
        &self,
        prepared: &PreparedModel,
    ) -> Result<AndroidInferenceApproval, HostError> {
        let authority = self
            .local_authority
            .as_ref()
            .ok_or(RuntimeError::LocalAuthorityUnavailable)?;
        let action = LocalAuthorityAction::ActivateLocalRuntime {
            plan: android_inference_plan_digest(prepared),
        };
        let generation = authority
            .platform_authority
            .current_generation()
            .ok_or(RuntimeError::LocalAuthorityUnavailable)?;
        let ticket = authority.issuer.issue(action.clone(), generation)?;
        let proof = self
            .runtime
            .verify_local_authority_ticket(ticket, &action)?;
        AndroidInferenceApproval::bind(prepared, proof)
    }

    pub async fn dispatch(&self, command: AppCommand) -> Result<(), HostError> {
        let Some(action) = self.runtime.required_local_authority_action(&command)? else {
            self.runtime.dispatch(command).await?;
            return Ok(());
        };
        let authority = self
            .local_authority
            .as_ref()
            .ok_or(RuntimeError::LocalAuthorityUnavailable)?;
        let generation = authority
            .platform_authority
            .current_generation()
            .ok_or(RuntimeError::LocalAuthorityUnavailable)?;
        let ticket = authority.issuer.issue(action, generation)?;
        self.runtime
            .dispatch_with_local_authority(command, ticket)
            .await?;
        Ok(())
    }

    /// Trusted local owner API, deliberately absent from AppCommand and JSONL.
    pub fn resolve_accounting_recovery(
        &self,
        resolution: &argentum_domain::goal_accounting::AccountingResolution,
    ) -> Result<argentum_domain::goal_accounting::RequestCharge, HostError> {
        let authority = self
            .local_authority
            .as_ref()
            .ok_or(RuntimeError::LocalAuthorityUnavailable)?;
        let action = self.runtime.accounting_resolution_action(resolution)?;
        let generation = authority
            .platform_authority
            .current_generation()
            .ok_or(RuntimeError::LocalAuthorityUnavailable)?;
        let ticket = authority.issuer.issue(action, generation)?;
        Ok(self
            .runtime
            .resolve_accounting_recovery(resolution, ticket)?)
    }

    pub fn subscribe(&self) -> broadcast::Receiver<AppEvent> {
        self.runtime.subscribe()
    }
}

impl InProcessClient {
    pub fn steer_active_run(
        &self,
        submission: argentum_runtime::SteeringSubmission,
    ) -> Result<argentum_runtime::SteeringReceipt, HostError> {
        Ok(self.runtime.steer_active_run(submission)?)
    }

    pub(crate) fn durable_command_identity(
        &self,
        request_id: &str,
    ) -> Result<Option<argentum_store::StoredCommandIdentity>, HostError> {
        Ok(self.runtime.durable_command_identity(request_id)?)
    }

    pub(crate) fn validate_durable_command_scope(
        &self,
        command: &AppCommand,
        scope: &argentum_store::CommandIdentityScope,
    ) -> Result<(), HostError> {
        Ok(self
            .runtime
            .validate_durable_command_scope(command, scope)?)
    }

    pub async fn compact_context_with_cancellation(
        &self,
        session: argentum_domain::SessionId,
        operation: argentum_domain::RunId,
        deactivate: bool,
        cancellation: tokio_util::sync::CancellationToken,
    ) -> Result<argentum_domain::context_summary::CompactionResult, HostError> {
        Ok(self
            .runtime
            .compact_context_with_cancellation(session, operation, deactivate, cancellation)
            .await?)
    }
    pub async fn compact_context(
        &self,
        session: argentum_domain::SessionId,
        operation: argentum_domain::RunId,
        deactivate: bool,
    ) -> Result<argentum_domain::context_summary::CompactionResult, HostError> {
        Ok(self
            .runtime
            .compact_context(session, operation, deactivate)
            .await?)
    }
    pub fn context_preview(
        &self,
        session_id: argentum_domain::SessionId,
    ) -> Result<argentum_runtime::working_context::ContextPreview, HostError> {
        Ok(self.runtime.context_preview(session_id)?)
    }
    pub async fn submit_bound_task(
        &self,
        submission: argentum_runtime::TaskSubmission,
    ) -> argentum_runtime::TaskSubmissionResult {
        self.runtime.submit_bound_task(submission).await
    }

    pub(crate) async fn submit_durable_bound_task(
        &self,
        submission: argentum_runtime::TaskSubmission,
        scope: &argentum_store::CommandIdentityScope,
    ) -> argentum_runtime::TaskSubmissionResult {
        self.runtime
            .submit_durable_bound_task(submission, scope)
            .await
    }
    pub async fn dispatch(&self, command: AppCommand) -> Result<(), HostError> {
        self.runtime.dispatch(command).await?;
        Ok(())
    }

    pub(crate) async fn dispatch_durable_command(
        &self,
        command: AppCommand,
        scope: argentum_store::CommandIdentityScope,
    ) -> Result<(), HostError> {
        if matches!(
            &command,
            AppCommand::SetGoal { .. }
                | AppCommand::PauseGoal
                | AppCommand::ResumeGoal
                | AppCommand::ClearGoal
        ) {
            self.runtime
                .dispatch_durable_goal_command(command, &scope)?;
        } else if let AppCommand::ProbeProvider { provider_id } = command {
            self.runtime
                .probe_provider_with_durable_scope(provider_id, Some(&scope))
                .await?;
        } else {
            self.runtime.dispatch(command).await?;
        }
        Ok(())
    }

    pub(crate) fn mark_durable_command_executing(
        &self,
        request_id: &str,
        canonicalization_version: u16,
        canonical_command: &[u8],
    ) -> Result<argentum_store::StoredCommandIdentity, HostError> {
        Ok(self.runtime.mark_durable_command_executing(
            request_id,
            canonicalization_version,
            canonical_command,
        )?)
    }

    pub(crate) fn settle_durable_command(
        &self,
        request_id: &str,
        canonicalization_version: u16,
        canonical_command: &[u8],
        outcome_kind: argentum_store::CommandOutcomeKind,
        result_payload: Option<&str>,
    ) -> Result<argentum_store::StoredCommandIdentity, HostError> {
        Ok(self.runtime.settle_durable_command(
            request_id,
            canonicalization_version,
            canonical_command,
            outcome_kind,
            result_payload,
        )?)
    }

    pub fn subscribe(&self) -> broadcast::Receiver<AppEvent> {
        self.runtime.subscribe()
    }
}

#[cfg(test)]
mod tests {
    use std::collections::{BTreeMap, VecDeque};
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
    use std::sync::{mpsc, Arc, Mutex};
    use std::time::Duration;

    use argentum_domain::{
        AppCommand, AppEvent, ApprovalScope, Capability, SurfaceId, ToolInput, ToolRequest,
        ToolResultState,
    };
    use argentum_providers::{
        local_runtime_recording_test_fixture, local_runtime_test_fixture, LocalRuntimeTestControl,
    };
    use tokio_util::sync::CancellationToken;

    use super::*;

    #[tokio::test]
    async fn chatgpt_budget_readback_and_missing_signin_are_independent_of_deepseek() {
        let temp = tempfile::tempdir().unwrap();
        let database = temp.path().join("test.db");
        let mut config = HostConfig::in_memory(temp.path());
        config.database = Some(database.clone());
        let mut host = CommandHost::start(config).unwrap();
        let client = host.client();
        client
            .dispatch(AppCommand::AddProviderPreset {
                preset_id: "chatgpt".into(),
            })
            .await
            .unwrap();
        client
            .dispatch(AppCommand::SelectProviderProfile {
                provider_id: "chatgpt".into(),
            })
            .await
            .unwrap();
        let budget = host
            .declare_model_budget("chatgpt", [1050000, 920976, 128000, 0, 1024])
            .unwrap();
        assert_eq!(budget.requested_output_tokens, 0);
        assert!(host
            .selected_provider_coding_issue()
            .unwrap()
            .unwrap()
            .contains("browser sign-in"));
        assert!(host
            .set_provider_credential("chatgpt", SecretValue::new("SYNTHETIC_API_KEY"))
            .is_err());
        host.shutdown().await.unwrap();
        let mut config = HostConfig::in_memory(temp.path());
        config.database = Some(database);
        let mut reopened = CommandHost::start(config).unwrap();
        assert_eq!(reopened.selected_model_budget().unwrap().unwrap(), budget);
        assert!(!reopened.provider_credential_configured("chatgpt"));
        assert!(reopened
            .provider_profiles()
            .unwrap()
            .iter()
            .any(|p| p.id == "chatgpt" && p.model == "gpt-6-luna" && p.selected));
        reopened.shutdown().await.unwrap();
    }

    #[test]
    fn validated_android_constructor_accepts_only_token_and_secret_store() {
        let constructor: fn(
            ValidatedAndroidAppPaths,
            Arc<dyn SecretStore>,
        ) -> Result<HostConfig, HostError> = HostConfig::discover_with_validated_app_paths;

        let _ = constructor;
    }

    #[test]
    fn validated_android_constructor_configures_only_android_local_inference() {
        let data = tempfile::tempdir().expect("data root");
        let workspace = data.path().join("workspace");
        std::fs::create_dir(&workspace).expect("workspace");
        let paths = AppPaths::from_data_dir(data.path());

        let config = HostConfig::from_validated_android_parts(
            workspace.clone(),
            paths.clone(),
            Arc::new(UnavailableSecretStore),
        )
        .expect("Android config");

        assert_eq!(config.workspace, workspace);
        assert_eq!(config.database, Some(paths.database));
        assert!(config.local_runtime.is_none());
        assert!(config.android_inference.is_some());
        assert_eq!(config.model, "Qwen3.5-0.8B-Q4_0.gguf");
    }

    #[test]
    fn discovered_local_runtime_uses_the_pinned_versioned_bundle_path() {
        let workspace = tempfile::tempdir().expect("workspace");
        let config = HostConfig::discover(workspace.path()).expect("host config");
        let runtime = config.local_runtime.expect("local runtime config");
        assert!(runtime.bundle_path.ends_with(
            Path::new("llama-cpp")
                .join("v0.3.0")
                .join("vulkan-x64")
                .join("bundle.zip")
        ));
        assert!(runtime.models_dir.ends_with("models"));
    }

    #[test]
    fn discover_with_app_paths_uses_android_storage_and_inference_config() {
        let data = tempfile::tempdir().expect("data root");
        let workspace = tempfile::tempdir().expect("workspace");
        let paths = AppPaths::from_data_dir(data.path());
        let secret_store: Arc<dyn SecretStore> = Arc::new(UnavailableSecretStore);

        let config = HostConfig::discover_with_app_paths(
            workspace.path(),
            paths.clone(),
            Arc::clone(&secret_store),
        )
        .expect("Android host config");

        assert_eq!(config.workspace, workspace.path());
        assert_eq!(
            config.database,
            Some(
                std::fs::canonicalize(data.path())
                    .expect("canonical data root")
                    .join("argentum.db")
            )
        );
        assert!(config.local_runtime.is_none());
        assert!(config.android_inference.is_some());
        assert_eq!(config.model, "Qwen3.5-0.8B-Q4_0.gguf");
        assert!(Arc::ptr_eq(&config.secret_store, &secret_store));
    }

    #[test]
    fn discover_with_app_paths_rejects_forged_android_layout_before_outside_access() {
        let data = tempfile::tempdir().expect("data root");
        let workspace = tempfile::tempdir().expect("workspace");
        let outside = tempfile::tempdir().expect("outside root");
        let mut paths = AppPaths::from_data_dir(data.path());
        paths.database = outside.path().join("redirected").join("argentum.db");
        let secret_store: Arc<dyn SecretStore> = Arc::new(UnavailableSecretStore);

        assert!(
            HostConfig::discover_with_app_paths(workspace.path(), paths, secret_store,).is_err()
        );
        assert!(!outside.path().join("redirected").exists());
    }

    #[derive(Default)]
    struct MemorySecretStore {
        values: Mutex<BTreeMap<String, String>>,
    }

    impl MemorySecretStore {
        fn value(&self, key: &str) -> Option<String> {
            self.values
                .lock()
                .expect("secret store lock")
                .get(key)
                .cloned()
        }
    }

    impl SecretStore for MemorySecretStore {
        fn get(&self, key: &str) -> Result<Option<SecretValue>, PlatformError> {
            Ok(self.value(key).map(SecretValue::new))
        }

        fn set(&self, key: &str, value: SecretValue) -> Result<(), PlatformError> {
            self.values
                .lock()
                .expect("secret store lock")
                .insert(key.to_owned(), value.expose().to_owned());
            Ok(())
        }

        fn delete(&self, key: &str) -> Result<(), PlatformError> {
            self.values.lock().expect("secret store lock").remove(key);
            Ok(())
        }
    }

    struct BlockingSetSecretStore {
        values: Mutex<BTreeMap<String, String>>,
        block_first_set: AtomicBool,
        set_entered: mpsc::SyncSender<()>,
        release_set: Mutex<mpsc::Receiver<()>>,
        delete_entered: mpsc::SyncSender<()>,
    }

    impl BlockingSetSecretStore {
        fn new() -> (
            Self,
            mpsc::Receiver<()>,
            mpsc::SyncSender<()>,
            mpsc::Receiver<()>,
        ) {
            let (set_entered, set_entered_rx) = mpsc::sync_channel(1);
            let (release_set, release_set_rx) = mpsc::sync_channel(1);
            let (delete_entered, delete_entered_rx) = mpsc::sync_channel(1);
            (
                Self {
                    values: Mutex::new(BTreeMap::new()),
                    block_first_set: AtomicBool::new(true),
                    set_entered,
                    release_set: Mutex::new(release_set_rx),
                    delete_entered,
                },
                set_entered_rx,
                release_set,
                delete_entered_rx,
            )
        }

        fn value(&self, key: &str) -> Option<String> {
            self.values
                .lock()
                .expect("secret store lock")
                .get(key)
                .cloned()
        }
    }

    impl SecretStore for BlockingSetSecretStore {
        fn get(&self, key: &str) -> Result<Option<SecretValue>, PlatformError> {
            Ok(self.value(key).map(SecretValue::new))
        }

        fn set(&self, key: &str, value: SecretValue) -> Result<(), PlatformError> {
            self.values
                .lock()
                .expect("secret store lock")
                .insert(key.to_owned(), value.expose().to_owned());
            if self.block_first_set.swap(false, Ordering::SeqCst) {
                self.set_entered.send(()).expect("first set entered");
                self.release_set
                    .lock()
                    .expect("set release lock")
                    .recv()
                    .expect("release first set");
            }
            Ok(())
        }

        fn delete(&self, key: &str) -> Result<(), PlatformError> {
            self.values.lock().expect("secret store lock").remove(key);
            self.delete_entered.send(()).expect("delete entered");
            Ok(())
        }
    }

    struct PartiallyFailingSecretStore {
        values: Mutex<BTreeMap<String, String>>,
        fail_next_set: AtomicBool,
    }

    impl PartiallyFailingSecretStore {
        fn with_value(key: &str, value: &str) -> Self {
            Self {
                values: Mutex::new(BTreeMap::from([(key.to_owned(), value.to_owned())])),
                fail_next_set: AtomicBool::new(false),
            }
        }

        fn value(&self, key: &str) -> Option<String> {
            self.values
                .lock()
                .expect("secret store lock")
                .get(key)
                .cloned()
        }

        fn fail_next_set(&self) {
            self.fail_next_set.store(true, Ordering::SeqCst);
        }
    }

    impl SecretStore for PartiallyFailingSecretStore {
        fn get(&self, key: &str) -> Result<Option<SecretValue>, PlatformError> {
            Ok(self.value(key).map(SecretValue::new))
        }

        fn set(&self, key: &str, value: SecretValue) -> Result<(), PlatformError> {
            self.values
                .lock()
                .expect("secret store lock")
                .insert(key.to_owned(), value.expose().to_owned());
            if self.fail_next_set.swap(false, Ordering::SeqCst) {
                return Err(PlatformError::SecureStorage(
                    "fixture replacement failure".into(),
                ));
            }
            Ok(())
        }

        fn delete(&self, key: &str) -> Result<(), PlatformError> {
            self.values.lock().expect("secret store lock").remove(key);
            Ok(())
        }
    }

    #[derive(Clone, Copy, PartialEq, Eq)]
    enum SecretStoreOperation {
        Get,
        Set,
        Delete,
    }

    struct LeakySecretStore {
        values: Mutex<BTreeMap<String, String>>,
        failures: Mutex<VecDeque<SecretStoreOperation>>,
        error: String,
        set_calls: AtomicUsize,
    }

    impl LeakySecretStore {
        fn new(secret: &str, failures: impl IntoIterator<Item = SecretStoreOperation>) -> Self {
            Self {
                values: Mutex::new(BTreeMap::new()),
                failures: Mutex::new(failures.into_iter().collect()),
                error: format!("secure storage failure includes {secret}"),
                set_calls: AtomicUsize::new(0),
            }
        }

        fn with_value(
            key: &str,
            value: &str,
            secret: &str,
            failures: impl IntoIterator<Item = SecretStoreOperation>,
        ) -> Self {
            Self {
                values: Mutex::new(BTreeMap::from([(key.to_owned(), value.to_owned())])),
                failures: Mutex::new(failures.into_iter().collect()),
                error: format!("secure storage failure includes {secret}"),
                set_calls: AtomicUsize::new(0),
            }
        }

        fn fails(&self, operation: SecretStoreOperation) -> bool {
            let mut failures = self.failures.lock().expect("failure plan lock");
            if failures.front().copied() == Some(operation) {
                failures.pop_front();
                true
            } else {
                false
            }
        }

        fn error(&self) -> PlatformError {
            PlatformError::SecureStorage(self.error.clone())
        }

        fn value(&self, key: &str) -> Option<String> {
            self.values
                .lock()
                .expect("secret store lock")
                .get(key)
                .cloned()
        }

        fn set_calls(&self) -> usize {
            self.set_calls.load(Ordering::SeqCst)
        }
    }

    impl SecretStore for LeakySecretStore {
        fn get(&self, key: &str) -> Result<Option<SecretValue>, PlatformError> {
            if self.fails(SecretStoreOperation::Get) {
                return Err(self.error());
            }
            Ok(self
                .values
                .lock()
                .expect("secret store lock")
                .get(key)
                .cloned()
                .map(SecretValue::new))
        }

        fn set(&self, key: &str, value: SecretValue) -> Result<(), PlatformError> {
            self.set_calls.fetch_add(1, Ordering::SeqCst);
            self.values
                .lock()
                .expect("secret store lock")
                .insert(key.to_owned(), value.expose().to_owned());
            if self.fails(SecretStoreOperation::Set) {
                return Err(self.error());
            }
            Ok(())
        }

        fn delete(&self, key: &str) -> Result<(), PlatformError> {
            self.values.lock().expect("secret store lock").remove(key);
            if self.fails(SecretStoreOperation::Delete) {
                return Err(self.error());
            }
            Ok(())
        }
    }

    fn host_config_with_secret_store(
        workspace: &Path,
        secret_store: Arc<dyn SecretStore>,
    ) -> HostConfig {
        let mut config = HostConfig::in_memory(workspace);
        config.secret_store = secret_store;
        config
    }

    fn assert_credential_storage_failure_is_redacted(
        error: HostError,
        events: Vec<AppEvent>,
        secret: &str,
    ) {
        let rendered = format!("{error} {error:?} {events:?}");
        assert_eq!(
            error.to_string(),
            "provider credential storage is unavailable"
        );
        assert!(!rendered.contains(secret));
        assert!(events.is_empty());
    }

    async fn composed_host_with_fake_local_runtime(
        workspace: &Path,
    ) -> (CommandHost, LocalRuntimeTestControl) {
        let config = HostConfig::in_memory(workspace).with_local_runtime(
            LocalRuntimeConfig::new(
                PathBuf::from(r"C:\argentum-test\bundle.zip"),
                PathBuf::from(r"C:\argentum-test\models"),
            )
            .expect("local runtime config"),
        );
        let control = Arc::new(Mutex::new(None));
        let capture = Arc::clone(&control);
        let host = CommandHost::start_composed_with_local_runtime_factory(
            config,
            move |runtime_config, authority| {
                let (module, fixture) = local_runtime_test_fixture(runtime_config, authority);
                *capture.lock().expect("test runtime fixture") = Some(fixture);
                module
            },
        )
        .await
        .expect("composed fake-runtime host");
        let fixture = control
            .lock()
            .expect("test runtime fixture")
            .take()
            .expect("captured test runtime fixture");
        (host, fixture)
    }

    #[tokio::test]
    async fn context_variants_persist_without_launch_and_preserve_explicit_output_limits() {
        let directory = tempfile::tempdir().unwrap();
        let workspace = directory.path().join("workspace");
        std::fs::create_dir(&workspace).unwrap();
        let mut config = HostConfig::in_memory(&workspace);
        config.database = Some(directory.path().join("state.db"));
        let mut host = CommandHost::start_composed(config.clone()).await.unwrap();
        let profiles = host.provider_profiles().unwrap();
        let policy = host.execution_policy().unwrap();
        let profile = profiles.iter().find(|p| p.id == "llama-cpp").unwrap();
        for size in argentum_providers::SIDECAR_CONTEXT_VARIANTS {
            let b = host.configure_local_context("llama-cpp", size).unwrap();
            assert_eq!(b.usable_context_tokens, u64::from(size));
            assert_eq!(b.requested_output_tokens, 0);
            assert_eq!(b.maximum_output_tokens, size - 1);
            assert_eq!(
                host.configured_local_context_tokens(&profile.model)
                    .unwrap(),
                size
            );
            assert!(!host.local_runtime_active(&profile.model).await);
        }
        assert_eq!(host.provider_profiles().unwrap(), profiles);
        assert_eq!(host.execution_policy().unwrap(), policy);
        host.shutdown().await.unwrap();
        drop(host);
        let mut host = CommandHost::start_composed(config).await.unwrap();
        assert_eq!(
            host.configured_local_context_tokens(&profile.model)
                .unwrap(),
            262144
        );
        host.declare_model_budget("llama-cpp", [8192, 8192, 4096, 512, 512])
            .unwrap();
        let b = host.configure_local_context("llama-cpp", 16384).unwrap();
        assert_eq!(
            (
                b.requested_output_tokens,
                b.maximum_output_tokens,
                b.uncertainty_tokens
            ),
            (512, 4096, 512)
        );
        host.declare_model_budget("llama-cpp", [65536, 65536, 20000, 18000, 128])
            .unwrap();
        let before = host.selected_model_budget().unwrap();
        assert!(host.configure_local_context("llama-cpp", 8192).is_err());
        assert_eq!(host.selected_model_budget().unwrap(), before);
        assert!(host.configure_local_context("llama-cpp", 12288).is_err());
        assert!(host.configure_local_context("minimax", 32768).is_err());
        host.shutdown().await.unwrap();
    }

    #[tokio::test]
    async fn reliability_settings_launch_rejects_changed_model_and_uses_the_owned_module() {
        let root = tempfile::tempdir().unwrap();
        let (mut host, control) = composed_host_with_fake_local_runtime(root.path()).await;
        let local = host.local_handle();
        let model = local
            .provider_profiles()
            .unwrap()
            .into_iter()
            .find(|p| p.id == "llama-cpp")
            .unwrap()
            .model;
        let prepared = local.prepare_local_model(model.clone()).await.unwrap();
        local
            .dispatch(AppCommand::SelectProviderModel {
                provider_id: "llama-cpp".into(),
                model: "other.gguf".into(),
            })
            .await
            .unwrap();
        let calls = control.launcher_call_count();
        assert!(matches!(
            local
                .activate_local_model(prepared, CancellationToken::new())
                .await,
            Err(HostError::Provider(
                ProviderError::LocalRuntimeApprovalStale
            ))
        ));
        assert_eq!(control.launcher_call_count(), calls);
        local
            .dispatch(AppCommand::SelectProviderModel {
                provider_id: "llama-cpp".into(),
                model: model.clone(),
            })
            .await
            .unwrap();
        let prepared = local.prepare_local_model(model.clone()).await.unwrap();
        local
            .activate_local_model(prepared, CancellationToken::new())
            .await
            .unwrap();
        assert!(local.local_model_ready(&model).await);
        local.unload_local_model().await.unwrap();
        assert!(!local.local_model_ready(&model).await);
        // Changing a selected profile may probe it; that is not inference.
        assert!(control.captured_requests().is_empty());
        assert_eq!(control.launch_count(), 1);
        host.shutdown().await.unwrap();
    }

    #[tokio::test]
    async fn context_variants_host_plan_matches_budget_and_rejects_changed_setting() {
        let directory = tempfile::tempdir().unwrap();
        let (mut host, control) = composed_host_with_fake_local_runtime(directory.path()).await;
        let model = host
            .provider_profiles()
            .unwrap()
            .into_iter()
            .find(|p| p.id == "llama-cpp")
            .unwrap()
            .model;
        let small_text = host.local_handle().local_text_input_capacity().unwrap();
        host.configure_local_context("llama-cpp", 32768).unwrap();
        assert!(host.local_handle().local_text_input_capacity().unwrap() > small_text);
        assert_eq!(control.launcher_call_count(), 0);
        let prepared = host.prepare_local_runtime(model.clone()).await.unwrap();
        assert_eq!(prepared.plan().context_tokens, 32768);
        let approval = host
            .local_handle()
            .approve_local_runtime(prepared.plan())
            .unwrap();
        host.configure_local_context("llama-cpp", 65536).unwrap();
        assert!(matches!(
            host.activate_local_runtime(prepared, approval).await,
            Err(HostError::Provider(
                ProviderError::LocalRuntimeApprovalStale
            ))
        ));
        assert_eq!(control.provider_call_count(), 0);
        host.shutdown().await.unwrap();
    }

    #[tokio::test]
    async fn f20c_local_prepared_request_carries_current_facts_without_tools_or_secrets() {
        let directory = tempfile::tempdir().unwrap();
        let (mut host, control) = composed_host_with_fake_local_runtime(directory.path()).await;
        let prepared = host.prepare_local_runtime(DEFAULT_MODEL).await.unwrap();
        let approval = host
            .local_handle()
            .approve_local_runtime(prepared.plan())
            .unwrap();
        host.activate_local_runtime(prepared, approval)
            .await
            .unwrap();
        let local = host.local_handle();
        let (sender, mut receiver) = tokio::sync::mpsc::channel(4);
        local
            .stream_local_text(
                "Describe your host".into(),
                Vec::new(),
                sender,
                tokio_util::sync::CancellationToken::new(),
            )
            .await
            .unwrap();
        while receiver.try_recv().is_ok() {}
        let requests = control.captured_requests();
        assert_eq!(requests.len(), 1);
        let request = &requests[0];
        let system = request.system.as_ref().unwrap();
        let facts: serde_json::Value =
            serde_json::from_str(system.lines().nth(1).unwrap()).unwrap();
        assert_eq!(facts["application"], "Argentum");
        assert_eq!(facts["mode"], "local text-only");
        assert_eq!(facts["provider"]["configured_model"], DEFAULT_MODEL);
        assert_eq!(facts["loaded_model"]["model"], DEFAULT_MODEL);
        assert_eq!(facts["access"]["profile"], "guided");
        assert!(request.tools.is_empty() && request.tool_exchanges.is_empty());
        assert!(system.len() + request.prompt.len() <= 4096);
        assert!(!system.contains("api-key"));
        assert!(!system.contains("authority_epoch"));
        local
            .apply_access_review(local.access_review("observe").unwrap())
            .await
            .unwrap();
        let (sender, _receiver) = tokio::sync::mpsc::channel(4);
        local
            .stream_local_text(
                "What changed?".into(),
                Vec::new(),
                sender,
                tokio_util::sync::CancellationToken::new(),
            )
            .await
            .unwrap();
        let requests = control.captured_requests();
        let facts: serde_json::Value =
            serde_json::from_str(requests[1].system.as_ref().unwrap().lines().nth(1).unwrap())
                .unwrap();
        assert_eq!(facts["access"]["profile"], "observe");
        let before = control.provider_call_count();
        let (sender, _receiver) = tokio::sync::mpsc::channel(4);
        let too_large = "x".repeat(local.local_text_input_capacity().unwrap() + 1);
        assert!(local
            .stream_local_text(
                too_large,
                Vec::new(),
                sender,
                tokio_util::sync::CancellationToken::new()
            )
            .await
            .is_err());
        assert_eq!(control.provider_call_count(), before);
        host.shutdown().await.unwrap();
    }

    async fn composed_host_with_recorded_local_runtime(
        workspace: &Path,
    ) -> (
        CommandHost,
        LocalRuntimeTestControl,
        Arc<Mutex<Vec<&'static str>>>,
        Arc<AtomicUsize>,
    ) {
        let config = HostConfig::in_memory(workspace).with_local_runtime(
            LocalRuntimeConfig::new(
                PathBuf::from(r"C:\argentum-test\bundle.zip"),
                PathBuf::from(r"C:\argentum-test\models"),
            )
            .expect("local runtime config"),
        );
        let events = Arc::new(Mutex::new(Vec::new()));
        let release_calls = Arc::new(AtomicUsize::new(0));
        let control = Arc::new(Mutex::new(None));
        let capture = Arc::clone(&control);
        let runtime_events = Arc::clone(&events);
        let host = CommandHost::start_composed_with_recording_factories(
            config,
            Arc::clone(&events),
            Arc::clone(&release_calls),
            move |runtime_config, authority| {
                let (module, fixture) =
                    local_runtime_recording_test_fixture(runtime_config, authority, runtime_events);
                *capture.lock().expect("test runtime fixture") = Some(fixture);
                module
            },
        )
        .await
        .expect("recorded composed fake-runtime host");
        let fixture = control
            .lock()
            .expect("test runtime fixture")
            .take()
            .expect("captured test runtime fixture");
        (host, fixture, events, release_calls)
    }

    #[tokio::test]
    async fn backend_toggle_gates_activation_without_launching_on_enable() {
        let workspace = tempfile::tempdir().unwrap();
        let (mut host, control) = composed_host_with_fake_local_runtime(workspace.path()).await;
        assert!(host.local_runtime_configured());
        host.client()
            .dispatch(AppCommand::SetLocalBackendEnabled { enabled: false })
            .await
            .unwrap();
        assert!(!host.local_runtime_configured());
        assert!(host.prepare_local_runtime("test.gguf").await.is_err());
        assert_eq!(control.launch_count(), 0);
        host.client()
            .dispatch(AppCommand::SetLocalBackendEnabled { enabled: true })
            .await
            .unwrap();
        assert!(host.local_runtime_configured());
        assert_eq!(control.launch_count(), 0);
        host.shutdown().await.unwrap();
    }

    #[tokio::test]
    async fn composed_command_host_owns_and_shuts_down_the_runtime_lifecycle() {
        let workspace = tempfile::tempdir().expect("workspace");
        let mut host = CommandHost::start_composed(HostConfig::in_memory(workspace.path()))
            .await
            .expect("composed host");

        let mounted = host
            .harness_lifecycle_snapshot()
            .await
            .expect("mounted lifecycle");
        assert_eq!(mounted.generation(), 1);
        assert_eq!(mounted.module_ids()[0], "first-party.review-diff");

        host.shutdown().await.expect("host shutdown");
        assert!(host
            .harness_lifecycle_snapshot()
            .await
            .expect("shutdown lifecycle")
            .module_ids()
            .is_empty());
    }

    #[tokio::test]
    async fn active_composed_host_retires_real_provider_before_platform_bridge_release() {
        let workspace = tempfile::tempdir().expect("workspace");
        let (mut host, control, events, release_calls) =
            composed_host_with_recorded_local_runtime(workspace.path()).await;
        let prepared = host
            .prepare_local_runtime("first.gguf")
            .await
            .expect("prepared fake runtime");
        let approval = host
            .local_handle()
            .approve_local_runtime(prepared.plan())
            .expect("host approval");
        host.activate_local_runtime(prepared, approval)
            .await
            .expect("host activation");

        assert!(host.provider_authority_mounted);
        assert!(host.local_runtime_active("first.gguf").await);
        assert_eq!(control.launch_count(), 1);
        assert_eq!(control.stop_attempt_count(), 0);
        assert_eq!(release_calls.load(Ordering::SeqCst), 0);
        assert_eq!(
            host.local_runtime
                .as_ref()
                .expect("local runtime")
                .lifecycle_snapshot()
                .await
                .modules()
                .len(),
            1
        );
        assert_eq!(
            host.local_authority
                .as_ref()
                .expect("local authority")
                .platform_authority
                .lifecycle_snapshot()
                .await
                .modules()
                .len(),
            2
        );
        assert!(!host
            .harness_lifecycle_snapshot()
            .await
            .expect("active harness")
            .module_ids()
            .is_empty());
        assert_eq!(
            host.local_authority
                .as_ref()
                .expect("local authority")
                .platform_authority
                .snapshot()
                .state(),
            argentum_domain::PlatformAuthorityState::Active
        );

        host.shutdown().await.expect("host shutdown");

        assert_eq!(control.launch_count(), 1);
        assert_eq!(control.stop_attempt_count(), 1);
        assert_eq!(release_calls.load(Ordering::SeqCst), 1);
        assert_eq!(
            events.lock().expect("lifecycle event lock").as_slice(),
            ["provider.local-runtime", "platform.authority.standard"]
        );
        assert!(host
            .local_runtime
            .as_ref()
            .expect("local runtime")
            .lifecycle_snapshot()
            .await
            .modules()
            .is_empty());
        assert!(host
            .local_authority
            .as_ref()
            .expect("local authority")
            .platform_authority
            .lifecycle_snapshot()
            .await
            .modules()
            .is_empty());
        assert_eq!(
            host.local_authority
                .as_ref()
                .expect("local authority")
                .platform_authority
                .snapshot()
                .state(),
            argentum_domain::PlatformAuthorityState::Revoked
        );
        assert!(host
            .harness_lifecycle_snapshot()
            .await
            .expect("shutdown harness")
            .module_ids()
            .is_empty());
    }

    #[tokio::test]
    async fn actual_composed_host_local_runtime_rejects_independent_same_ordinal_authority() {
        let workspace = tempfile::tempdir().expect("workspace");
        let (host, control) = composed_host_with_fake_local_runtime(workspace.path()).await;
        let prepared = host
            .prepare_local_runtime("first.gguf")
            .await
            .expect("prepared fake runtime");
        let (foreign_issuer, foreign_verifier, foreign_generation) =
            host_authority_channel().expect("independent authority channel");
        assert_eq!(
            foreign_generation.ordinal(),
            host.local_authority
                .as_ref()
                .expect("host authority")
                .platform_authority
                .current_generation()
                .expect("host generation")
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
        let approval =
            LocalRuntimeLaunchApproval::bind(prepared.plan(), proof).expect("foreign approval");

        assert!(matches!(
            host.local_runtime
                .as_ref()
                .expect("host local runtime")
                .activate(prepared, Some(approval), CancellationToken::new())
                .await,
            Err(ProviderError::LocalRuntimeApprovalStale)
        ));
        assert_eq!(control.launch_count(), 0);
        assert!(host
            .local_runtime
            .as_ref()
            .expect("host local runtime")
            .lifecycle_snapshot()
            .await
            .modules()
            .is_empty());
    }

    #[tokio::test]
    async fn same_command_host_retries_failed_local_runtime_retirement_without_relaunch() {
        let workspace = tempfile::tempdir().expect("workspace");
        let (mut host, control) = composed_host_with_fake_local_runtime(workspace.path()).await;
        let prepared = host
            .prepare_local_runtime("first.gguf")
            .await
            .expect("prepared fake runtime");
        let approval = host
            .local_handle()
            .approve_local_runtime(prepared.plan())
            .expect("host approval");
        host.activate_local_runtime(prepared, approval)
            .await
            .expect("host activation");
        assert_eq!(control.launch_count(), 1);
        control.fail_next_stop();

        assert!(host.shutdown().await.is_err());
        assert_eq!(control.launch_count(), 1);
        assert_eq!(control.stop_attempt_count(), 1);
        assert_eq!(
            host.local_authority
                .as_ref()
                .expect("host authority")
                .platform_authority
                .snapshot()
                .state(),
            argentum_domain::PlatformAuthorityState::RecoveryPending
        );

        host.shutdown().await.expect("same-owner shutdown retry");
        assert_eq!(control.launch_count(), 1);
        assert_eq!(control.stop_attempt_count(), 2);
        assert!(host
            .harness_lifecycle_snapshot()
            .await
            .expect("shutdown harness")
            .module_ids()
            .is_empty());
    }

    #[tokio::test]
    async fn in_process_client_dispatches_without_a_transport_boundary() {
        let workspace = tempfile::tempdir().expect("workspace");
        let host = CommandHost::start(HostConfig::in_memory(workspace.path())).expect("host");
        let client = host.client();
        let mut events = client.subscribe();

        client
            .dispatch(AppCommand::NewSession)
            .await
            .expect("new session command");

        assert!(matches!(
            events.recv().await.expect("session event"),
            AppEvent::SessionCreated(_)
        ));
    }

    #[tokio::test]
    async fn host_keeps_write_approval_enabled() {
        let workspace = tempfile::tempdir().expect("workspace");
        let host = CommandHost::start(HostConfig::in_memory(workspace.path())).expect("host");

        assert_eq!(
            host.workspace_root(),
            std::fs::canonicalize(workspace.path())
                .expect("canonical workspace")
                .as_path()
        );
        let registry = ToolRegistry::with_builtins(WorkspaceManager::new(
            CapabilityBroker::new(workspace.path(), ApprovalPolicy::default()).expect("broker"),
        ));
        let host_descriptors = host.tool_descriptors();
        let builtin_descriptors = registry.descriptors();
        assert!(builtin_descriptors
            .iter()
            .all(|descriptor| host_descriptors.contains(descriptor)));
        assert!(host_descriptors.iter().all(|descriptor| {
            builtin_descriptors.contains(descriptor) || descriptor.id == "delegate_readonly"
        }));
        let write = host
            .tool_descriptors()
            .into_iter()
            .find(|tool| tool.id == "write_text")
            .expect("write tool");
        assert!(write.requires_approval);
        assert!(write.capabilities.contains(&Capability::WriteFiles));
    }

    #[tokio::test]
    async fn in_process_client_executes_read_tools_through_app_commands() {
        let workspace = tempfile::tempdir().expect("workspace");
        std::fs::write(workspace.path().join("note.txt"), "workspace note").expect("fixture");
        let host = CommandHost::start(HostConfig::in_memory(workspace.path())).expect("host");
        let client = host.client();
        let mut events = client.subscribe();
        let call_id = "00000000-0000-0000-0000-000000000011"
            .parse()
            .expect("call id");
        let run_id = "00000000-0000-0000-0000-000000000012"
            .parse()
            .expect("run id");

        client
            .dispatch(AppCommand::RequestTool {
                request: ToolRequest {
                    call_id,
                    run_id,
                    input: ToolInput::ReadText {
                        path: "note.txt".into(),
                        start_byte: Some(0),
                        end_byte: None,
                        start_line: None,
                        line_count: None,
                        expected_sha256: None,
                    },
                },
            })
            .await
            .expect("read tool command");

        let emitted = std::iter::from_fn(|| events.try_recv().ok()).collect::<Vec<_>>();
        assert!(emitted.iter().any(|event| matches!(
            event,
            AppEvent::ToolStarted(trace)
                if trace.id == call_id && trace.tool_id == "read_text"
        )));
        assert!(emitted.iter().any(|event| matches!(
            event,
            AppEvent::ToolFinished(trace)
                if trace.id == call_id && trace.result == ToolResultState::Succeeded
        )));
        assert!(!emitted
            .iter()
            .any(|event| matches!(event, AppEvent::ApprovalRequested(_))));
    }

    #[tokio::test]
    async fn local_command_handle_preserves_write_approval() {
        let workspace = tempfile::tempdir().expect("workspace");
        let host = CommandHost::start(HostConfig::in_memory(workspace.path())).expect("host");
        let client = host.client();
        let local = host.local_handle();
        let mut events = client.subscribe();
        let call_id = "00000000-0000-0000-0000-000000000021"
            .parse()
            .expect("call id");
        let run_id = "00000000-0000-0000-0000-000000000022"
            .parse()
            .expect("run id");

        client
            .dispatch(AppCommand::RequestTool {
                request: ToolRequest {
                    call_id,
                    run_id,
                    input: ToolInput::WriteText {
                        path: "approved.txt".into(),
                        content: "approved content".into(),
                    },
                },
            })
            .await
            .expect("write tool command");

        let approval = match events.recv().await.expect("approval event") {
            AppEvent::ApprovalRequested(approval) => approval,
            event => panic!("unexpected event: {event:?}"),
        };
        assert_eq!(approval.run_id, run_id);
        assert_eq!(approval.tool_id, "write_text");
        assert_eq!(approval.capabilities, vec![Capability::WriteFiles]);
        assert!(!workspace.path().join("approved.txt").exists());

        local
            .dispatch(AppCommand::ApproveTool {
                approval_id: approval.id,
                scope: ApprovalScope::Once,
            })
            .await
            .expect("approval command");

        let emitted = std::iter::from_fn(|| events.try_recv().ok()).collect::<Vec<_>>();
        assert!(emitted.iter().any(|event| matches!(
            event,
            AppEvent::ApprovalResolved {
                approval_id,
                approved: true,
            } if *approval_id == approval.id
        )));
        assert!(emitted.iter().any(|event| matches!(
            event,
            AppEvent::ToolFinished(trace)
                if trace.id == call_id && trace.result == ToolResultState::Succeeded
        )));
        assert_eq!(
            std::fs::read_to_string(workspace.path().join("approved.txt")).expect("approved write"),
            "approved content"
        );
    }

    #[tokio::test]
    async fn in_process_tool_commands_cannot_escape_the_workspace() {
        let workspace = tempfile::tempdir().expect("workspace");
        let outside = tempfile::NamedTempFile::new().expect("outside file");
        std::fs::write(outside.path(), "private").expect("outside fixture");
        let host = CommandHost::start(HostConfig::in_memory(workspace.path())).expect("host");
        let client = host.client();
        let mut events = client.subscribe();
        let call_id = "00000000-0000-0000-0000-000000000031"
            .parse()
            .expect("call id");

        client
            .dispatch(AppCommand::RequestTool {
                request: ToolRequest {
                    call_id,
                    run_id: "00000000-0000-0000-0000-000000000032"
                        .parse()
                        .expect("run id"),
                    input: ToolInput::ReadText {
                        path: outside.path().display().to_string(),
                        start_byte: Some(0),
                        end_byte: None,
                        start_line: None,
                        line_count: None,
                        expected_sha256: None,
                    },
                },
            })
            .await
            .expect("bounded read command");

        let emitted = std::iter::from_fn(|| events.try_recv().ok()).collect::<Vec<_>>();
        assert!(emitted.iter().any(|event| matches!(
            event,
            AppEvent::ToolFinished(trace)
                if trace.id == call_id && trace.result == ToolResultState::Failed
        )));
        assert!(emitted.iter().any(|event| matches!(
            event,
            AppEvent::RunError { message, .. }
                if message == "Tool execution failed inside the active security boundary."
        )));
    }

    #[tokio::test]
    async fn command_host_restores_and_selects_durable_sessions() {
        let directory = tempfile::tempdir().expect("directory");
        let workspace = directory.path().join("workspace");
        std::fs::create_dir(&workspace).expect("workspace");
        let database = directory.path().join("argentum.db");
        let mut config = HostConfig::in_memory(&workspace);
        config.database = Some(database.clone());
        let host = CommandHost::start(config.clone()).expect("host");
        let first = host.workspace_snapshot().expect("first snapshot");
        host.client()
            .dispatch(AppCommand::NewSession)
            .await
            .expect("new session");
        let expanded = host.workspace_snapshot().expect("expanded snapshot");
        assert_eq!(expanded.sessions.len(), 2);
        drop(host);

        let reopened = CommandHost::start(config).expect("reopened host");
        let restored = reopened.workspace_snapshot().expect("restored snapshot");
        assert_eq!(restored.project.id, first.project.id);
        assert_eq!(restored.sessions, expanded.sessions);
        reopened
            .client()
            .dispatch(AppCommand::SelectSession {
                session_id: first.active_session_id.expect("first session"),
            })
            .await
            .expect("select session");

        assert_eq!(
            reopened
                .workspace_snapshot()
                .expect("selected snapshot")
                .active_session_id,
            first.active_session_id
        );
    }

    #[test]
    fn supported_environment_credentials_map_to_canonical_profile_ids_safely() {
        let openai_secret = "openai-environment-fixture";
        let minimax_secret = "minimax-environment-fixture";
        let deepseek_secret = "deepseek-environment-fixture";
        let credentials = provider_credentials_from_lookup(|variable| match variable {
            "OPENAI_API_KEY" => Ok(openai_secret.into()),
            "MINIMAX_API_KEY" => Ok(minimax_secret.into()),
            "DEEPSEEK_API_KEY" => Ok(deepseek_secret.into()),
            _ => Err(std::env::VarError::NotPresent),
        })
        .expect("environment credentials");

        for profile_id in ["openai", "minimax", "deepseek"] {
            assert!(credentials.contains_profile(profile_id));
        }
        let rendered = format!("{credentials:?}");
        assert!(!rendered.contains(openai_secret));
        assert!(!rendered.contains(minimax_secret));
        assert!(!rendered.contains(deepseek_secret));
    }

    #[test]
    fn single_credential_builder_merges_with_existing_credentials() {
        let config = HostConfig::in_memory(".")
            .with_provider_credential("openai", SecretValue::new("openai-fixture"))
            .expect("OpenAI credential")
            .with_provider_credential("minimax", SecretValue::new("minimax-fixture"))
            .expect("MiniMax credential");

        assert!(config.provider_credentials.contains_profile("openai"));
        assert!(config.provider_credentials.contains_profile("minimax"));
        let rendered = format!("{config:?}");
        assert!(!rendered.contains("openai-fixture"));
        assert!(!rendered.contains("minimax-fixture"));
    }

    #[tokio::test]
    async fn command_host_round_trips_hosted_credentials_through_secure_store() {
        let workspace = tempfile::tempdir().expect("workspace");
        let store = Arc::new(MemorySecretStore::default());
        let config = HostConfig::in_memory(workspace.path()).with_secret_store(store.clone());
        let secret = "minimax-secure-store-fixture";
        let host = CommandHost::start(config).expect("host");

        host.set_provider_credential("minimax", SecretValue::new(secret))
            .expect("store credential");
        assert!(host.provider_credential_configured("minimax"));
        assert_eq!(store.value("provider/minimax").as_deref(), Some(secret));
        assert!(!format!("{:?}", host.provider_statuses()).contains(secret));

        let reopened = CommandHost::start(
            HostConfig::in_memory(workspace.path()).with_secret_store(store.clone()),
        )
        .expect("reopened host");
        assert!(reopened.provider_credential_configured("minimax"));

        reopened
            .clear_provider_credential("minimax")
            .expect("clear credential");
        assert!(!reopened.provider_credential_configured("minimax"));
        assert_eq!(store.value("provider/minimax"), None);
    }

    #[tokio::test]
    async fn local_command_handle_round_trips_supported_credentials_only() {
        let workspace = tempfile::tempdir().expect("workspace");
        let store = Arc::new(MemorySecretStore::default());
        let host = CommandHost::start(
            HostConfig::in_memory(workspace.path()).with_secret_store(store.clone()),
        )
        .expect("host");
        let local = host.local_handle();
        let mut events = local.subscribe();
        let first = "deepseek-local-first-fixture";
        let second = "deepseek-local-second-fixture";

        local
            .set_provider_credential("deepseek", SecretValue::new(first))
            .expect("save first credential");
        assert!(local.provider_credential_configured("deepseek"));
        assert_eq!(store.value("provider/deepseek").as_deref(), Some(first));

        local
            .set_provider_credential("deepseek", SecretValue::new(second))
            .expect("replace credential");
        assert!(local.provider_credential_configured("deepseek"));
        assert_eq!(store.value("provider/deepseek").as_deref(), Some(second));

        local
            .clear_provider_credential("deepseek")
            .expect("clear credential");
        assert!(!local.provider_credential_configured("deepseek"));
        assert_eq!(store.value("provider/deepseek"), None);
        let custom_error = local
            .set_provider_credential("custom", SecretValue::new("blocked-fixture"))
            .expect_err("reject custom credential profile");

        let published = std::iter::from_fn(|| events.try_recv().ok()).collect::<Vec<_>>();
        let credential_statuses = published
            .iter()
            .filter_map(|event| match event {
                AppEvent::ProviderCredentialStatus {
                    provider_id,
                    configured,
                } => Some((provider_id.as_str(), *configured)),
                _ => None,
            })
            .collect::<Vec<_>>();
        let rendered = format!(
            "{custom_error}\n{custom_error:?}\n{local:?}\n{}",
            serde_json::to_string(&published).expect("serialize published state")
        );
        assert!(!rendered.contains(first));
        assert!(!rendered.contains(second));
        assert!(!rendered.contains("blocked-fixture"));
        assert_eq!(
            credential_statuses,
            vec![("deepseek", true), ("deepseek", true), ("deepseek", false)]
        );
        assert!(!published.iter().any(|event| matches!(
            event,
            AppEvent::ProviderCredentialStatus { provider_id, .. } if provider_id == "custom"
        )));
    }

    #[test]
    fn cloned_local_handles_serialize_hosted_credential_transactions() {
        let workspace = tempfile::tempdir().expect("workspace");
        let (store, set_entered, release_set, delete_entered) = BlockingSetSecretStore::new();
        let store = Arc::new(store);
        let host = CommandHost::start(
            HostConfig::in_memory(workspace.path()).with_secret_store(store.clone()),
        )
        .expect("host");
        let setter = host.local_handle();
        let clearer = setter.clone();
        let (clear_started, clear_started_rx) = mpsc::sync_channel(1);

        let set_thread = std::thread::spawn(move || {
            setter.set_provider_credential(
                "deepseek",
                SecretValue::new("deepseek-concurrent-fixture"),
            )
        });
        set_entered.recv().expect("persisted set entered");
        let clear_thread = std::thread::spawn(move || {
            clear_started.send(()).expect("clear thread started");
            clearer.clear_provider_credential("deepseek")
        });
        clear_started_rx.recv().expect("clear thread started");

        let interleaved = delete_entered.recv_timeout(Duration::from_secs(1)).is_ok();
        release_set.send(()).expect("release persisted set");
        set_thread
            .join()
            .expect("set thread")
            .expect("set credential");
        clear_thread
            .join()
            .expect("clear thread")
            .expect("clear credential");

        assert!(!interleaved, "clear entered storage before set committed");
        assert_eq!(store.value("provider/deepseek"), None);
        assert!(!host.provider_credential_configured("deepseek"));
    }

    #[test]
    fn poisoned_credential_transaction_fails_closed_without_mutation() {
        let workspace = tempfile::tempdir().expect("workspace");
        let store = Arc::new(MemorySecretStore::default());
        let host = CommandHost::start(
            HostConfig::in_memory(workspace.path()).with_secret_store(store.clone()),
        )
        .expect("host");
        let local = host.local_handle();
        let transaction = Arc::clone(&local.credential_transaction);

        assert!(std::thread::spawn(move || {
            let _guard = transaction.lock().expect("transaction lock");
            panic!("poison credential transaction fixture");
        })
        .join()
        .is_err());

        assert!(matches!(
            local.set_provider_credential(
                "deepseek",
                SecretValue::new("deepseek-poisoned-transaction-fixture"),
            ),
            Err(HostError::ProviderCredentialTransactionUnavailable)
        ));
        assert_eq!(store.value("provider/deepseek"), None);
        assert!(!host.provider_credential_configured("deepseek"));
    }

    #[tokio::test]
    async fn failed_credential_replacement_restores_the_previous_secret() {
        let workspace = tempfile::tempdir().expect("workspace");
        let store = Arc::new(PartiallyFailingSecretStore::with_value(
            "provider/deepseek",
            "deepseek-previous-fixture",
        ));
        let host = CommandHost::start(
            HostConfig::in_memory(workspace.path()).with_secret_store(store.clone()),
        )
        .expect("host");
        let local = host.local_handle();
        store.fail_next_set();

        assert!(local
            .set_provider_credential("deepseek", SecretValue::new("deepseek-replacement-fixture"))
            .is_err());
        assert!(local.provider_credential_configured("deepseek"));
        assert_eq!(
            store.value("provider/deepseek").as_deref(),
            Some("deepseek-previous-fixture")
        );
    }

    #[test]
    fn failed_replacement_keeps_an_injected_runtime_credential_when_storage_was_empty() {
        let workspace = tempfile::tempdir().expect("workspace");
        let previous = "deepseek-injected-previous-fixture";
        let attempted = "deepseek-empty-store-replacement-fixture";
        let store = Arc::new(LeakySecretStore::new(
            attempted,
            [SecretStoreOperation::Set],
        ));
        let mut config = HostConfig::in_memory(workspace.path())
            .with_provider_credential("deepseek", SecretValue::new(previous))
            .expect("prior credential");
        config.secret_store = store.clone();
        let host = CommandHost::start(config).expect("host");
        let local = host.local_handle();
        let mut events = local.subscribe();

        let error = local
            .set_provider_credential("deepseek", SecretValue::new(attempted))
            .expect_err("storage write failure");
        let emitted = std::iter::from_fn(|| events.try_recv().ok()).collect::<Vec<_>>();
        let rendered = format!("{error} {error:?} {emitted:?}");

        assert!(local.provider_credential_configured("deepseek"));
        assert_eq!(store.value("provider/deepseek"), None);
        assert!(emitted.is_empty());
        assert!(!rendered.contains(previous));
        assert!(!rendered.contains(attempted));
    }

    #[test]
    fn failed_delete_keeps_an_injected_runtime_credential_when_storage_was_empty() {
        let workspace = tempfile::tempdir().expect("workspace");
        let previous = "deepseek-injected-delete-fixture";
        let store = Arc::new(LeakySecretStore::new(
            previous,
            [SecretStoreOperation::Delete],
        ));
        let mut config = HostConfig::in_memory(workspace.path())
            .with_provider_credential("deepseek", SecretValue::new(previous))
            .expect("prior credential");
        config.secret_store = store.clone();
        let host = CommandHost::start(config).expect("host");
        let local = host.local_handle();
        let mut events = local.subscribe();

        let error = local
            .clear_provider_credential("deepseek")
            .expect_err("storage delete failure");
        let emitted = std::iter::from_fn(|| events.try_recv().ok()).collect::<Vec<_>>();

        assert!(local.provider_credential_configured("deepseek"));
        assert_eq!(store.value("provider/deepseek"), None);
        assert!(emitted.is_empty());
        assert_credential_storage_failure_is_redacted(error, emitted, previous);
    }

    #[test]
    fn empty_replacement_does_not_reach_secure_storage_or_mutate_the_runtime() {
        let workspace = tempfile::tempdir().expect("workspace");
        let previous = "deepseek-injected-validation-fixture";
        let store = Arc::new(LeakySecretStore::new(previous, []));
        let mut config = HostConfig::in_memory(workspace.path())
            .with_provider_credential("deepseek", SecretValue::new(previous))
            .expect("prior credential");
        config.secret_store = store.clone();
        let host = CommandHost::start(config).expect("host");
        let local = host.local_handle();
        let mut events = local.subscribe();

        assert!(local
            .set_provider_credential("deepseek", SecretValue::new("   "))
            .is_err());

        assert_eq!(store.set_calls(), 0);
        assert_eq!(store.value("provider/deepseek"), None);
        assert!(local.provider_credential_configured("deepseek"));
        assert!(std::iter::from_fn(|| events.try_recv().ok())
            .collect::<Vec<_>>()
            .is_empty());
    }

    #[test]
    fn credential_store_get_errors_are_redacted_from_local_mutations() {
        let workspace = tempfile::tempdir().expect("workspace");
        let secret = "leaky-get-credential-fixture";
        let store = Arc::new(LeakySecretStore::new(secret, [SecretStoreOperation::Get]));
        let host = CommandHost::start(host_config_with_secret_store(workspace.path(), store))
            .expect("host");
        let local = host.local_handle();
        let mut events = local.subscribe();

        let error = local
            .set_provider_credential("deepseek", SecretValue::new(secret))
            .expect_err("storage read failure");

        assert_credential_storage_failure_is_redacted(
            error,
            std::iter::from_fn(|| events.try_recv().ok()).collect(),
            secret,
        );
    }

    #[test]
    fn credential_store_set_errors_are_redacted_from_local_mutations() {
        let workspace = tempfile::tempdir().expect("workspace");
        let secret = "leaky-set-credential-fixture";
        let store = Arc::new(LeakySecretStore::new(secret, [SecretStoreOperation::Set]));
        let host = CommandHost::start(host_config_with_secret_store(workspace.path(), store))
            .expect("host");
        let local = host.local_handle();
        let mut events = local.subscribe();

        let error = local
            .set_provider_credential("deepseek", SecretValue::new(secret))
            .expect_err("storage write failure");

        assert_credential_storage_failure_is_redacted(
            error,
            std::iter::from_fn(|| events.try_recv().ok()).collect(),
            secret,
        );
    }

    #[test]
    fn credential_store_delete_errors_are_redacted_from_local_mutations() {
        let workspace = tempfile::tempdir().expect("workspace");
        let secret = "leaky-delete-credential-fixture";
        let store = Arc::new(LeakySecretStore::new(
            secret,
            [SecretStoreOperation::Delete],
        ));
        let host = CommandHost::start(host_config_with_secret_store(workspace.path(), store))
            .expect("host");
        let local = host.local_handle();
        let mut events = local.subscribe();

        let error = local
            .clear_provider_credential("deepseek")
            .expect_err("storage delete failure");

        assert_credential_storage_failure_is_redacted(
            error,
            std::iter::from_fn(|| events.try_recv().ok()).collect(),
            secret,
        );
    }

    #[test]
    fn credential_store_rollback_errors_are_redacted_from_local_mutations() {
        let workspace = tempfile::tempdir().expect("workspace");
        let secret = "leaky-rollback-credential-fixture";
        let store = Arc::new(LeakySecretStore::with_value(
            "provider/deepseek",
            "deepseek-previous-fixture",
            secret,
            [SecretStoreOperation::Set, SecretStoreOperation::Set],
        ));
        let mut config = HostConfig::in_memory(workspace.path())
            .with_provider_credential("deepseek", SecretValue::new("deepseek-previous-fixture"))
            .expect("prior credential");
        config.secret_store = store;
        let host = CommandHost::start(config).expect("host");
        let local = host.local_handle();
        let mut events = local.subscribe();

        let error = local
            .set_provider_credential("deepseek", SecretValue::new(secret))
            .expect_err("storage rollback failure");

        assert_credential_storage_failure_is_redacted(
            error,
            std::iter::from_fn(|| events.try_recv().ok()).collect(),
            secret,
        );
    }

    #[tokio::test]
    async fn command_host_keeps_injected_credentials_out_of_profiles_events_and_sqlite() {
        let directory = tempfile::tempdir().expect("directory");
        let workspace = directory.path().join("workspace");
        std::fs::create_dir(&workspace).expect("workspace");
        let database = directory.path().join("argentum.db");
        let secret = "minimax-host-only-credential";
        let mut config = HostConfig::in_memory(&workspace)
            .with_provider_api_keys([("minimax".into(), SecretValue::new(secret))])
            .expect("host credentials");
        config.database = Some(database.clone());
        assert!(!format!("{config:?}").contains(secret));

        let host = CommandHost::start(config).expect("host");
        let client = host.client();
        let mut events = client.subscribe();
        client
            .dispatch(AppCommand::SaveProviderProfile {
                profile: ProviderProfile {
                    id: "minimax".into(),
                    label: "MiniMax".into(),
                    kind: ProviderKind::OpenAiCompatible,
                    endpoint: "https://api.minimax.example/v1/".into(),
                    model: "minimax-test".into(),
                    selected: false,
                },
            })
            .await
            .expect("save profile");
        host.publish_initial_state().expect("initial state");

        let profiles = host.provider_profiles().expect("profiles");
        let statuses = host.provider_statuses();
        let emitted = std::iter::from_fn(|| events.try_recv().ok()).collect::<Vec<_>>();
        let rendered = format!(
            "{}\n{}\n{}",
            serde_json::to_string(&profiles).expect("profiles JSON"),
            serde_json::to_string(&statuses).expect("statuses JSON"),
            serde_json::to_string(&emitted).expect("events JSON")
        );
        assert!(rendered.contains("minimax"));
        assert!(!rendered.contains(secret));
        drop(host);

        for path in [
            database.clone(),
            database.with_extension("db-wal"),
            database.with_extension("db-shm"),
        ] {
            if let Ok(bytes) = std::fs::read(path) {
                assert!(!bytes
                    .windows(secret.len())
                    .any(|window| window == secret.as_bytes()));
            }
        }
    }

    #[tokio::test]
    async fn initial_state_publishes_layout_before_an_active_trajectory_snapshot() {
        let workspace = tempfile::tempdir().expect("workspace");
        let host = CommandHost::start(HostConfig::in_memory(workspace.path())).expect("host");
        let client = host.client();
        client
            .dispatch(AppCommand::SetSurfaceVisibility {
                surface: SurfaceId::Trajectory,
                visible: true,
            })
            .await
            .expect("enable trajectory");
        client
            .dispatch(AppCommand::SetInspectorSurface {
                surface: Some(SurfaceId::Trajectory),
            })
            .await
            .expect("present trajectory");

        let mut events = host.subscribe();
        host.publish_initial_state().expect("initial state");
        let emitted = std::iter::from_fn(|| events.try_recv().ok()).collect::<Vec<_>>();
        let layout_position = emitted
            .iter()
            .position(|event| matches!(event, AppEvent::LayoutChanged(_)))
            .expect("layout event");
        let trajectory_position = emitted
            .iter()
            .position(|event| matches!(event, AppEvent::TrajectorySnapshotLoaded(_)))
            .expect("trajectory event");

        assert!(layout_position < trajectory_position);
    }
}
