use std::future::Future;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use argentum_domain::{ProviderKind, ProviderProfile, ProviderStatus};
#[cfg(windows)]
use argentum_execution::{ensure_owner_only_directory, ensure_owner_only_file};
use async_trait::async_trait;
use futures_util::StreamExt;
use sha2::{Digest, Sha256};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;
use url::Url;

use crate::{
    LocalRuntimeModule, ModelProvider, ModelRequest, ProviderError, ProviderEvent, ProviderModel,
};

const LOCAL_ENDPOINT: &str = "local://llama-cpp/";
const LOCAL_PROVIDER_ID: &str = "llama-cpp";
const LOCAL_PROVIDER_LABEL: &str = "llama.cpp";
const MODEL_DOWNLOAD_TOTAL_TIMEOUT: Duration = Duration::from_secs(2 * 60 * 60);
const ARTIFACT_CONNECT_TIMEOUT: Duration = Duration::from_secs(30);

#[derive(Clone)]
struct DownloadPolicy {
    total_timeout: Duration,
    connect_timeout: Duration,
    #[cfg(test)]
    hooks: DownloadTestHooks,
}

impl DownloadPolicy {
    fn model() -> Self {
        Self {
            total_timeout: MODEL_DOWNLOAD_TOTAL_TIMEOUT,
            connect_timeout: ARTIFACT_CONNECT_TIMEOUT,
            #[cfg(test)]
            hooks: DownloadTestHooks::default(),
        }
    }
}

#[derive(Clone, Copy)]
struct DownloadDeadline {
    expires_at: tokio::time::Instant,
    timeout_ms: u64,
}

impl DownloadDeadline {
    fn new(total_timeout: Duration) -> Self {
        Self {
            expires_at: tokio::time::Instant::now() + total_timeout,
            timeout_ms: total_timeout.as_millis().try_into().unwrap_or(u64::MAX),
        }
    }

    fn check(self) -> Result<(), ProviderError> {
        if tokio::time::Instant::now() >= self.expires_at {
            return Err(ProviderError::LocalModelDownloadDeadline {
                timeout_ms: self.timeout_ms,
            });
        }
        Ok(())
    }

    async fn wait<T>(self, future: impl Future<Output = T>) -> Result<T, ProviderError> {
        tokio::time::timeout_at(self.expires_at, future)
            .await
            .map_err(|_| ProviderError::LocalModelDownloadDeadline {
                timeout_ms: self.timeout_ms,
            })
    }
}

#[cfg(test)]
#[derive(Clone, Default)]
struct DownloadTestHooks {
    publication_gate: Option<PublicationGate>,
}

#[cfg(test)]
#[derive(Clone)]
struct PublicationGate {
    reached: std::sync::Arc<tokio::sync::Barrier>,
    release: std::sync::Arc<tokio::sync::Barrier>,
}

#[cfg(test)]
impl PublicationGate {
    fn new() -> Self {
        Self {
            reached: std::sync::Arc::new(tokio::sync::Barrier::new(2)),
            release: std::sync::Arc::new(tokio::sync::Barrier::new(2)),
        }
    }

    async fn wait_until_reached(&self) {
        self.reached.wait().await;
    }

    async fn release(&self) {
        self.release.wait().await;
    }

    async fn wait_for_release(&self) {
        self.reached.wait().await;
        self.release.wait().await;
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LocalModelOffer {
    pub id: &'static str,
    pub label: &'static str,
    pub filename: &'static str,
    pub download_url: &'static str,
    pub source_revision: &'static str,
    pub artifact_revision: &'static str,
    pub size_bytes: u64,
    pub sha256: &'static str,
    pub license: &'static str,
    pub size_limit_bytes: u64,
    pub context_window_tokens: Option<u64>,
}

const LOCAL_MODEL_OFFERS: [LocalModelOffer; 1] = [LocalModelOffer {
    id: "Qwen3.5-0.8B-Q8_0.gguf",
    label: "Qwen3.5 0.8B Q8_0",
    filename: "Qwen3.5-0.8B-Q8_0.gguf",
    download_url: "https://huggingface.co/ggml-org/Qwen3.5-0.8B-GGUF/resolve/8fea620810c4afa23dd6443f999a48574c1611a3/Qwen3.5-0.8B-Q8_0.gguf",
    source_revision: "2fc06364715b967f1860aea9cf38778875588b17",
    artifact_revision: "8fea620810c4afa23dd6443f999a48574c1611a3",
    size_bytes: 833_592_096,
    sha256: "37ae482d336108d23516fa35e8e0c4126688d81018b87178a18d752a1357814f",
    license: "Apache-2.0",
    size_limit_bytes: 2 * 1024 * 1024 * 1024,
    context_window_tokens: Some(262_144),
}];

const ANDROID_LOCAL_MODEL_OFFERS: [LocalModelOffer; 1] = [LocalModelOffer {
    id: "Qwen3.5-0.8B-Q4_0.gguf",
    label: "Qwen3.5 0.8B Q4_0",
    filename: "Qwen3.5-0.8B-Q4_0.gguf",
    download_url: "https://huggingface.co/ggml-org/Qwen3.5-0.8B-GGUF/resolve/8fea620810c4afa23dd6443f999a48574c1611a3/Qwen3.5-0.8B-Q4_0.gguf",
    source_revision: "2fc06364715b967f1860aea9cf38778875588b17",
    artifact_revision: "8fea620810c4afa23dd6443f999a48574c1611a3",
    size_bytes: 563_036_064,
    sha256: "57d1997790d1744fba5b40a7317df71ea5e2acee28c47e78f0cce39c0703f8cf",
    license: "Apache-2.0",
    size_limit_bytes: 1024 * 1024 * 1024,
    context_window_tokens: Some(4_096),
}];

pub fn local_model_offers() -> &'static [LocalModelOffer] {
    &LOCAL_MODEL_OFFERS
}

pub fn android_local_model_offers() -> &'static [LocalModelOffer] {
    &ANDROID_LOCAL_MODEL_OFFERS
}

pub async fn download_local_model(
    offer: LocalModelOffer,
    destination_dir: impl AsRef<Path>,
) -> Result<PathBuf, ProviderError> {
    if !is_trusted_offer(&offer) {
        return Err(ProviderError::LocalModelDownloadDestinationInvalid);
    }
    let target = validate_download_destination(&offer, destination_dir.as_ref())?;
    let url = Url::parse(offer.download_url)
        .map_err(|_| ProviderError::LocalModelDownloadDestinationInvalid)?;
    if !is_allowed_download_url(&url) {
        return Err(ProviderError::LocalModelDownloadDestinationInvalid);
    }
    Box::pin(download_model_with_policy(
        offer,
        target,
        url,
        DownloadPolicy::model(),
        is_allowed_download_url,
    ))
    .await
}

async fn download_model_with_policy(
    offer: LocalModelOffer,
    target: PathBuf,
    url: Url,
    policy: DownloadPolicy,
    is_allowed_url: fn(&Url) -> bool,
) -> Result<PathBuf, ProviderError> {
    let deadline = DownloadDeadline::new(policy.total_timeout);
    download_model_inner(
        offer,
        target,
        url,
        policy.connect_timeout,
        is_allowed_url,
        deadline,
        #[cfg(test)]
        policy.hooks,
    )
    .await
}

async fn download_model_inner(
    offer: LocalModelOffer,
    target: PathBuf,
    url: Url,
    connect_timeout: Duration,
    is_allowed_url: fn(&Url) -> bool,
    deadline: DownloadDeadline,
    #[cfg(test)] hooks: DownloadTestHooks,
) -> Result<PathBuf, ProviderError> {
    deadline.check()?;
    if let Ok(metadata) = std::fs::symlink_metadata(&target) {
        if metadata.file_type().is_symlink() || !metadata.is_file() {
            return Err(ProviderError::LocalModelDownloadDestinationInvalid);
        }
        set_private_model_permissions(&target)?;
        if metadata.len() > offer.size_limit_bytes {
            return Err(ProviderError::LocalModelDownloadTooLarge {
                limit_bytes: offer.size_limit_bytes,
            });
        }
        if metadata.len() == offer.size_bytes
            && deadline.wait(file_sha256(&target)).await?? == offer.sha256
        {
            set_private_model_permissions(&target)?;
            return Ok(target);
        }
        return Err(ProviderError::LocalModelIntegrityMismatch);
    }
    if !is_allowed_url(&url) {
        return Err(ProviderError::LocalModelDownloadDestinationInvalid);
    }
    deadline.check()?;
    let client = reqwest::Client::builder()
        .connect_timeout(connect_timeout)
        .redirect(reqwest::redirect::Policy::custom(move |attempt| {
            if attempt.previous().len() >= 3 {
                attempt.error("too many redirects")
            } else if is_allowed_url(attempt.url()) {
                attempt.follow()
            } else {
                attempt.error("model download redirect is not allowed")
            }
        }))
        .build()
        .map_err(|_| ProviderError::LocalModelDownloadFailed)?;
    let response = deadline
        .wait(client.get(url).send())
        .await?
        .map_err(|_| ProviderError::LocalModelDownloadFailed)?;
    if !response.status().is_success() || !is_allowed_url(response.url()) {
        return Err(ProviderError::LocalModelDownloadFailed);
    }
    if response
        .content_length()
        .is_some_and(|length| length > offer.size_limit_bytes)
    {
        return Err(ProviderError::LocalModelDownloadTooLarge {
            limit_bytes: offer.size_limit_bytes,
        });
    }
    if response
        .content_length()
        .is_some_and(|length| length != offer.size_bytes)
    {
        return Err(ProviderError::LocalModelIntegrityMismatch);
    }

    let temporary = target.with_file_name(format!(
        ".{}.{}.{}.part",
        offer.filename,
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or_default()
    ));
    let temporary_guard = TemporaryModelFile::new(temporary.clone());
    let mut file = deadline
        .wait(
            tokio::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&temporary),
        )
        .await?
        .map_err(|_| ProviderError::LocalModelDownloadFailed)?;
    set_private_model_permissions(&temporary)?;
    let mut stream = response.bytes_stream();
    let mut hasher = Sha256::new();
    let mut total_bytes = 0_u64;
    while let Some(chunk) = deadline.wait(stream.next()).await? {
        let chunk = chunk.map_err(|_| ProviderError::LocalModelDownloadFailed)?;
        total_bytes = total_bytes.checked_add(chunk.len() as u64).ok_or(
            ProviderError::LocalModelDownloadTooLarge {
                limit_bytes: offer.size_limit_bytes,
            },
        )?;
        if total_bytes > offer.size_limit_bytes {
            return Err(ProviderError::LocalModelDownloadTooLarge {
                limit_bytes: offer.size_limit_bytes,
            });
        }
        hasher.update(&chunk);
        deadline
            .wait(file.write_all(&chunk))
            .await?
            .map_err(|_| ProviderError::LocalModelDownloadFailed)?;
    }
    deadline
        .wait(file.sync_all())
        .await?
        .map_err(|_| ProviderError::LocalModelDownloadFailed)?;
    drop(file);

    let digest = format!("{:x}", hasher.finalize());
    if total_bytes != offer.size_bytes || digest != offer.sha256 {
        return Err(ProviderError::LocalModelIntegrityMismatch);
    }
    deadline.check()?;
    #[cfg(test)]
    if let Some(gate) = hooks.publication_gate {
        deadline.wait(gate.wait_for_release()).await?;
    }
    publish_model_without_clobber(temporary.clone(), target.clone(), deadline).await?;
    drop(temporary_guard);
    Ok(target)
}

async fn publish_model_without_clobber(
    temporary: PathBuf,
    target: PathBuf,
    deadline: DownloadDeadline,
) -> Result<(), ProviderError> {
    let expires_at = deadline.expires_at.into_std();
    let timeout_ms = deadline.timeout_ms;
    tokio::task::spawn_blocking(move || {
        if std::time::Instant::now() >= expires_at {
            return Err(ProviderError::LocalModelDownloadDeadline { timeout_ms });
        }
        #[cfg(target_os = "android")]
        {
            return match rustix::fs::renameat_with(
                rustix::fs::CWD,
                &temporary,
                rustix::fs::CWD,
                &target,
                rustix::fs::RenameFlags::NOREPLACE,
            ) {
                // The temporary file is already private and rename preserves its mode.
                // Avoid reporting failure after the target has been published atomically.
                Ok(()) => Ok(()),
                Err(rustix::io::Errno::EXIST) => {
                    Err(ProviderError::LocalModelDownloadDestinationInvalid)
                }
                Err(_) => Err(ProviderError::LocalModelDownloadFailed),
            };
        }
        #[cfg(not(target_os = "android"))]
        match std::fs::hard_link(&temporary, &target) {
            Ok(()) => set_private_model_permissions(&target),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                Err(ProviderError::LocalModelDownloadDestinationInvalid)
            }
            Err(_) => Err(ProviderError::LocalModelDownloadFailed),
        }
    })
    .await
    .map_err(|_| ProviderError::LocalModelDownloadFailed)?
}

fn validate_download_destination(
    offer: &LocalModelOffer,
    directory: &Path,
) -> Result<PathBuf, ProviderError> {
    if !is_trusted_offer(offer)
        || !directory.is_absolute()
        || directory.as_os_str().to_string_lossy().len() > 2_048
        || directory.as_os_str().to_string_lossy().contains('\0')
    {
        return Err(ProviderError::LocalModelDownloadDestinationInvalid);
    }
    match std::fs::symlink_metadata(directory) {
        Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_dir() => {
            return Err(ProviderError::LocalModelDownloadDestinationInvalid);
        }
        Ok(_) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            std::fs::create_dir_all(directory)
                .map_err(|_| ProviderError::LocalModelDownloadDestinationInvalid)?;
        }
        Err(_) => return Err(ProviderError::LocalModelDownloadDestinationInvalid),
    }
    if path_contains_symlink(directory)? {
        return Err(ProviderError::LocalModelDownloadDestinationInvalid);
    }
    #[cfg(windows)]
    ensure_owner_only_directory(directory)
        .map_err(|_| ProviderError::LocalModelDownloadDestinationInvalid)?;
    let target = directory.join(offer.filename);
    validate_model_path_shape(&target)
        .map_err(|_| ProviderError::LocalModelDownloadDestinationInvalid)?;
    Ok(target)
}

fn is_trusted_offer(offer: &LocalModelOffer) -> bool {
    LOCAL_MODEL_OFFERS
        .iter()
        .chain(ANDROID_LOCAL_MODEL_OFFERS.iter())
        .any(|candidate| candidate == offer)
}

fn is_allowed_download_url(url: &Url) -> bool {
    if url.scheme() != "https" {
        return false;
    }
    url.host_str().is_some_and(|host| {
        host == "huggingface.co"
            || host.ends_with(".huggingface.co")
            || host == "hf.co"
            || host.ends_with(".hf.co")
    })
}

async fn file_sha256(path: &Path) -> Result<String, ProviderError> {
    let mut file = tokio::fs::File::open(path)
        .await
        .map_err(|_| ProviderError::LocalModelDownloadFailed)?;
    let mut hasher = Sha256::new();
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let read = file
            .read(&mut buffer)
            .await
            .map_err(|_| ProviderError::LocalModelDownloadFailed)?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

fn path_contains_symlink(path: &Path) -> Result<bool, ProviderError> {
    #[cfg(target_os = "android")]
    {
        let canonical = std::fs::canonicalize(path)
            .map_err(|_| ProviderError::LocalModelDownloadDestinationInvalid)?;
        return Ok(canonical != path && !android_private_data_aliases_match(path, &canonical));
    }

    #[cfg(not(target_os = "android"))]
    {
        let mut current = PathBuf::new();
        for component in path.components() {
            current.push(component);
            match std::fs::symlink_metadata(&current) {
                Ok(metadata) if metadata.file_type().is_symlink() => return Ok(true),
                Ok(_) => {}
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => break,
                Err(_) => return Err(ProviderError::LocalModelDownloadDestinationInvalid),
            }
        }
        Ok(false)
    }
}

#[cfg(any(target_os = "android", test))]
fn android_private_data_aliases_match(path: &Path, canonical: &Path) -> bool {
    let user_root = Path::new("/data/user/0");
    let legacy_root = Path::new("/data/data");
    let Ok(declared_suffix) = path.strip_prefix(user_root) else {
        return false;
    };
    let Ok(canonical_suffix) = canonical.strip_prefix(legacy_root) else {
        return false;
    };
    let components = declared_suffix.components().collect::<Vec<_>>();
    components.len() >= 3
        && components
            .iter()
            .all(|component| matches!(component, std::path::Component::Normal(_)))
        && matches!(
            components.get(1),
            Some(std::path::Component::Normal(name)) if *name == std::ffi::OsStr::new("files")
        )
        && declared_suffix == canonical_suffix
}

fn set_private_model_permissions(path: &Path) -> Result<(), ProviderError> {
    #[cfg(windows)]
    ensure_owner_only_file(path).map_err(|_| ProviderError::LocalModelDownloadFailed)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))
            .map_err(|_| ProviderError::LocalModelDownloadFailed)?;
    }
    #[cfg(not(unix))]
    let _ = path;
    Ok(())
}

struct TemporaryModelFile {
    path: PathBuf,
}

impl TemporaryModelFile {
    fn new(path: PathBuf) -> Self {
        Self { path }
    }
}

impl Drop for TemporaryModelFile {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

#[derive(Clone)]
pub struct LocalLlamaCppProvider {
    model: String,
    runtime: Option<Arc<LocalRuntimeModule>>,
}

impl std::fmt::Debug for LocalLlamaCppProvider {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("LocalLlamaCppProvider")
            .field("model", &self.model)
            .field("runtime_configured", &self.runtime.is_some())
            .finish()
    }
}

impl LocalLlamaCppProvider {
    pub fn unconfigured(model: impl Into<String>) -> Self {
        Self::new(model, None).expect("a missing local model path is valid configuration")
    }

    pub fn new(
        model: impl Into<String>,
        runtime: Option<Arc<LocalRuntimeModule>>,
    ) -> Result<Self, ProviderError> {
        let model = model.into();
        if model.trim().is_empty()
            || model.chars().count() > 256
            || model.chars().any(char::is_control)
        {
            return Err(ProviderError::InvalidProfile {
                field: "model",
                reason: "use 1 to 256 printable characters",
            });
        }
        Ok(Self { model, runtime })
    }

    pub fn from_profile(
        profile: &ProviderProfile,
        runtime: Option<Arc<LocalRuntimeModule>>,
    ) -> Result<Self, ProviderError> {
        if profile.kind != ProviderKind::LocalLlamaCpp {
            return Err(ProviderError::InvalidProfile {
                field: "kind",
                reason: "profile kind does not match llama.cpp",
            });
        }
        Self::new(profile.model.clone(), runtime)
    }

    pub fn with_runtime(
        model: impl Into<String>,
        runtime: Arc<LocalRuntimeModule>,
    ) -> Result<Self, ProviderError> {
        Self::new(model, Some(runtime))
    }

    fn require_runtime(&self) -> Result<&Arc<LocalRuntimeModule>, ProviderError> {
        self.runtime
            .as_ref()
            .ok_or_else(|| ProviderError::ProviderNotConfigured {
                provider_id: LOCAL_PROVIDER_ID.into(),
            })
    }

    fn status_detail(&self) -> String {
        match self.runtime {
            None => "Local llama.cpp runtime is not configured".into(),
            Some(_) => "Local runtime configured; explicit activation required".into(),
        }
    }
}

#[async_trait]
impl ModelProvider for LocalLlamaCppProvider {
    async fn capability_runtime_evidence(&self) -> Result<Option<String>, ProviderError> {
        self.require_runtime()?
            .client(&self.model)
            .await?
            .provider()
            .capability_runtime_evidence()
            .await
    }

    async fn validate_request(
        &self,
        request: &ModelRequest,
    ) -> Result<crate::ProviderRequestValidation, ProviderError> {
        if !request.model.is_empty() && request.model != self.model {
            return Err(ProviderError::LocalRuntimeLeaseStale);
        }
        self.require_runtime()?
            .client(&self.model)
            .await?
            .provider()
            .validate_request(request)
            .await
    }

    async fn stream_validated_with_cancellation(
        &self,
        request: ModelRequest,
        expected: &crate::ProviderRequestValidation,
        sender: mpsc::Sender<ProviderEvent>,
        cancellation: CancellationToken,
    ) -> Result<(), ProviderError> {
        let current = tokio::select! { biased;
            _ = cancellation.cancelled() => return Err(ProviderError::RequestCancelled),
            result = self.validate_request(&request) => result?,
        };
        if &current != expected {
            return Err(ProviderError::LocalRuntimeLeaseStale);
        }
        self.stream_with_cancellation(request, sender, cancellation)
            .await
    }
    fn id(&self) -> &'static str {
        LOCAL_PROVIDER_ID
    }

    fn status(&self) -> ProviderStatus {
        ProviderStatus {
            profile_id: LOCAL_PROVIDER_ID.into(),
            kind: ProviderKind::LocalLlamaCpp,
            label: LOCAL_PROVIDER_LABEL.into(),
            endpoint: LOCAL_ENDPOINT.into(),
            connected: false,
            detail: self.status_detail(),
        }
    }

    async fn probe(&self) -> Result<ProviderStatus, ProviderError> {
        let client = self.require_runtime()?.client(&self.model).await?;
        client.verify_lease().await?;
        client.provider().probe().await?;
        let mut status = self.status();
        status.connected = true;
        status.detail = "Local GGUF model served by contained llama.cpp".into();
        Ok(status)
    }

    async fn list_models(&self) -> Result<Vec<ProviderModel>, ProviderError> {
        let client = self.require_runtime()?.client(&self.model).await?;
        client.verify_lease().await?;
        client.provider().list_models().await
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
        crate::validate_model_tool_payload(&request)?;
        if !request.tools.is_empty()
            || !request.tool_exchanges.is_empty()
            || request.history.iter().any(|m| m.tool_exchange.is_some())
        {
            tokio::select! { biased;
                _ = cancellation.cancelled() => return Err(ProviderError::RequestCancelled),
                result = self.validate_request(&request) => { result?; },
            }
        }
        if !request.model.is_empty() && request.model != self.model {
            return Err(ProviderError::LocalRuntimeLeaseStale);
        }
        let client = self.require_runtime()?.client(&self.model).await?;
        client.verify_lease().await?;
        let (proxy_sender, mut proxy_receiver) = mpsc::channel(32);
        let provider = client.provider();
        let provider_cancellation = cancellation.clone();
        let provider_future =
            provider.stream_with_cancellation(request, proxy_sender, provider_cancellation);
        tokio::pin!(provider_future);
        let mut provider_done = false;
        loop {
            tokio::select! {
                _ = cancellation.cancelled() => return Err(ProviderError::RequestCancelled),
                result = &mut provider_future, if !provider_done => {
                    result?;
                    provider_done = true;
                }
                event = proxy_receiver.recv() => {
                    let Some(event) = event else {
                        if provider_done {
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
                    client.verify_lease().await?;
                    if cancellation.is_cancelled() {
                        return Err(ProviderError::RequestCancelled);
                    }
                    permit.send(event);
                }
            }
            if provider_done && proxy_receiver.is_empty() && proxy_receiver.is_closed() {
                return Ok(());
            }
        }
    }
}

fn validate_model_path_shape(path: &Path) -> Result<(), ProviderError> {
    if !path.is_absolute() {
        return Err(ProviderError::InvalidProfile {
            field: "model_path",
            reason: "the local model path must be absolute",
        });
    }
    if path.as_os_str().to_string_lossy().len() > 2_048
        || path.as_os_str().to_string_lossy().contains('\0')
    {
        return Err(ProviderError::InvalidProfile {
            field: "model_path",
            reason: "the local model path is too long",
        });
    }
    let is_gguf = path
        .extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| extension.eq_ignore_ascii_case("gguf"));
    if !is_gguf {
        return Err(ProviderError::InvalidProfile {
            field: "model_path",
            reason: "the local model path must end in .gguf",
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::io::{Read, Write};
    use std::net::{TcpListener, TcpStream};
    use std::sync::mpsc;
    use std::thread;
    use std::time::Duration;

    use sha2::{Digest, Sha256};
    use tempfile::TempDir;

    use super::*;

    struct DownloadServer {
        endpoint: String,
        first_chunk: Option<tokio::sync::oneshot::Receiver<()>>,
        handle: thread::JoinHandle<()>,
    }

    impl DownloadServer {
        async fn wait_for_first_chunk(&mut self) {
            self.first_chunk
                .take()
                .expect("first fixture chunk receiver")
                .await
                .expect("first fixture chunk");
        }

        fn finish(self) {
            self.handle.join().expect("download fixture");
        }
    }

    struct PausedChunkedServer {
        endpoint: String,
        first_chunk: tokio::sync::oneshot::Receiver<()>,
        continue_download: mpsc::Sender<()>,
        handle: thread::JoinHandle<()>,
    }

    impl PausedChunkedServer {
        async fn wait_for_first_chunk(&mut self) {
            (&mut self.first_chunk)
                .await
                .expect("first chunk fixture signal");
        }

        fn continue_download(&self) {
            self.continue_download
                .send(())
                .expect("continue oversized fixture");
        }

        fn finish(self) {
            self.handle.join().expect("chunked download fixture");
        }
    }

    fn read_request_headers(stream: &mut TcpStream) {
        stream
            .set_read_timeout(Some(Duration::from_secs(1)))
            .expect("fixture read timeout");
        let mut request = Vec::new();
        let mut buffer = [0_u8; 1_024];
        while !request.windows(4).any(|window| window == b"\r\n\r\n") {
            let read = stream.read(&mut buffer).expect("fixture request");
            if read == 0 {
                break;
            }
            request.extend_from_slice(&buffer[..read]);
        }
    }

    fn spawn_download_server(chunks: Vec<Vec<u8>>, pause: Duration) -> DownloadServer {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind download fixture");
        let address = listener.local_addr().expect("download fixture address");
        let (first_sender, first_chunk) = tokio::sync::oneshot::channel();
        let handle = thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("accept download fixture");
            read_request_headers(&mut stream);
            let total = chunks.iter().map(|chunk| chunk.len()).sum::<usize>();
            write!(
                stream,
                "HTTP/1.1 200 OK\r\nContent-Length: {total}\r\nConnection: close\r\n\r\n"
            )
            .expect("download response headers");
            let mut first_sender = Some(first_sender);
            for (index, chunk) in chunks.iter().enumerate() {
                stream.write_all(chunk).expect("download response chunk");
                stream.flush().expect("flush download chunk");
                if index == 0 {
                    first_sender
                        .take()
                        .expect("first fixture chunk sender")
                        .send(())
                        .expect("record first fixture chunk");
                }
                if index + 1 != chunks.len() {
                    thread::sleep(pause);
                }
            }
        });
        DownloadServer {
            endpoint: format!("http://{address}/fixture.gguf"),
            first_chunk: Some(first_chunk),
            handle,
        }
    }

    fn spawn_paused_chunked_server() -> PausedChunkedServer {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind chunked fixture");
        let address = listener.local_addr().expect("chunked fixture address");
        let (first_sender, first_chunk) = tokio::sync::oneshot::channel();
        let (continue_download, continue_receiver) = mpsc::channel();
        let handle = thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("accept chunked fixture");
            read_request_headers(&mut stream);
            stream
                .write_all(
                    b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\nConnection: close\r\n\r\n1\r\na\r\n",
                )
                .expect("first chunked response");
            stream.flush().expect("flush first chunked response");
            first_sender
                .send(())
                .expect("record first chunked response");
            continue_receiver
                .recv_timeout(Duration::from_secs(2))
                .expect("continue chunked response");
            stream
                .write_all(b"2\r\nbc\r\n0\r\n\r\n")
                .expect("oversized chunked response");
        });
        PausedChunkedServer {
            endpoint: format!("http://{address}/fixture.gguf"),
            first_chunk,
            continue_download,
            handle,
        }
    }

    fn spawn_raw_response_server(response: String) -> DownloadServer {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind response fixture");
        let address = listener.local_addr().expect("response fixture address");
        let (first_sender, first_chunk) = tokio::sync::oneshot::channel();
        let handle = thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("accept response fixture");
            read_request_headers(&mut stream);
            stream
                .write_all(response.as_bytes())
                .expect("response fixture");
            first_sender.send(()).expect("record response fixture");
        });
        DownloadServer {
            endpoint: format!("http://{address}/fixture.gguf"),
            first_chunk: Some(first_chunk),
            handle,
        }
    }

    struct NoRequestServer {
        endpoint: String,
        accepted: mpsc::Receiver<bool>,
        handle: thread::JoinHandle<()>,
    }

    impl NoRequestServer {
        fn finish(self) -> bool {
            let accepted = self
                .accepted
                .recv_timeout(Duration::from_secs(1))
                .expect("no-request fixture result");
            self.handle.join().expect("no-request fixture");
            accepted
        }
    }

    fn spawn_no_request_server() -> NoRequestServer {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind no-request fixture");
        listener
            .set_nonblocking(true)
            .expect("set no-request fixture nonblocking");
        let address = listener.local_addr().expect("no-request fixture address");
        let (sender, accepted) = mpsc::channel();
        let handle = thread::spawn(move || {
            let deadline = std::time::Instant::now() + Duration::from_millis(400);
            let accepted = loop {
                match listener.accept() {
                    Ok(_) => break true,
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        if std::time::Instant::now() >= deadline {
                            break false;
                        }
                        thread::sleep(Duration::from_millis(5));
                    }
                    Err(error) => panic!("accept no-request fixture: {error}"),
                }
            };
            sender.send(accepted).expect("record no-request fixture");
        });
        NoRequestServer {
            endpoint: format!("http://{address}/fixture.gguf"),
            accepted,
            handle,
        }
    }

    fn fixture_offer(bytes: &[u8]) -> LocalModelOffer {
        LocalModelOffer {
            id: "fixture.gguf",
            label: "fixture",
            filename: "fixture.gguf",
            download_url: "http://127.0.0.1/fixture.gguf",
            source_revision: "fixture",
            artifact_revision: "fixture",
            size_bytes: bytes.len() as u64,
            sha256: Box::leak(format!("{:x}", Sha256::digest(bytes)).into_boxed_str()),
            license: "fixture",
            size_limit_bytes: bytes.len() as u64,
            context_window_tokens: None,
        }
    }

    fn fixture_policy() -> DownloadPolicy {
        DownloadPolicy {
            total_timeout: Duration::from_millis(250),
            connect_timeout: Duration::from_secs(1),
            hooks: DownloadTestHooks::default(),
        }
    }

    fn fixture_download_url(url: &Url) -> bool {
        url.scheme() == "http" && url.host_str() == Some("127.0.0.1")
    }

    fn assert_no_part_file(directory: &TempDir) {
        assert!(
            std::fs::read_dir(directory.path())
                .expect("fixture directory entries")
                .all(|entry| !entry
                    .expect("fixture directory entry")
                    .file_name()
                    .to_string_lossy()
                    .ends_with(".part")),
            "download left a temporary part file"
        );
    }

    async fn wait_for_part_contents(directory: &TempDir, expected: &[u8]) {
        tokio::time::timeout(Duration::from_secs(1), async {
            loop {
                let found = std::fs::read_dir(directory.path())
                    .expect("fixture directory entries")
                    .filter_map(Result::ok)
                    .map(|entry| entry.path())
                    .find(|path| {
                        path.file_name()
                            .is_some_and(|name| name.to_string_lossy().ends_with(".part"))
                    });
                if let Some(path) = found {
                    if std::fs::read(&path).ok().as_deref() == Some(expected) {
                        #[cfg(windows)]
                        {
                            let evidence = argentum_execution::owner_only_acl_evidence(&path)
                                .expect("owner-only model part ACL evidence");
                            assert!(evidence.protected);
                            assert_eq!(evidence.inherited_ace_count, 0);
                            assert_eq!(evidence.non_owner_allow_ace_count, 0);
                            assert!(evidence.owner_full_control);
                        }
                        return;
                    }
                }
                tokio::task::yield_now().await;
            }
        })
        .await
        .expect("part file was created and written before overflow");
    }

    async fn fixture_download(
        offer: LocalModelOffer,
        directory: &TempDir,
        endpoint: &str,
    ) -> Result<PathBuf, ProviderError> {
        let target = directory.path().join(offer.filename);
        download_model_with_policy(
            offer,
            target,
            Url::parse(endpoint).expect("fixture download URL"),
            fixture_policy(),
            fixture_download_url,
        )
        .await
    }

    #[test]
    fn qwen_offer_is_bounded_and_integrity_pinned() {
        let offer = local_model_offers()
            .first()
            .expect("starter local model offer");
        assert_eq!(offer.id, "Qwen3.5-0.8B-Q8_0.gguf");
        assert_eq!(offer.filename, "Qwen3.5-0.8B-Q8_0.gguf");
        assert_eq!(offer.size_bytes, 833_592_096);
        assert_eq!(
            offer.sha256,
            "37ae482d336108d23516fa35e8e0c4126688d81018b87178a18d752a1357814f"
        );
        assert_eq!(
            offer.source_revision,
            "2fc06364715b967f1860aea9cf38778875588b17"
        );
        assert_eq!(
            offer.artifact_revision,
            "8fea620810c4afa23dd6443f999a48574c1611a3"
        );
        assert_eq!(offer.license, "Apache-2.0");
        assert_eq!(offer.context_window_tokens, Some(262_144));
        assert!(offer.download_url.starts_with("https://huggingface.co/"));
        assert!(offer
            .download_url
            .contains("/resolve/8fea620810c4afa23dd6443f999a48574c1611a3/"));
        assert!(!offer.download_url.contains("/resolve/main/"));
        assert!(offer.size_limit_bytes <= 2 * 1024 * 1024 * 1024);
    }

    #[test]
    fn android_qwen_offer_is_compact_bounded_and_integrity_pinned() {
        let offer = android_local_model_offers()
            .first()
            .expect("Android starter local model offer");
        assert_eq!(offer.id, "Qwen3.5-0.8B-Q4_0.gguf");
        assert_eq!(offer.filename, "Qwen3.5-0.8B-Q4_0.gguf");
        assert_eq!(offer.size_bytes, 563_036_064);
        assert_eq!(
            offer.sha256,
            "57d1997790d1744fba5b40a7317df71ea5e2acee28c47e78f0cce39c0703f8cf"
        );
        assert_eq!(
            offer.source_revision,
            "2fc06364715b967f1860aea9cf38778875588b17"
        );
        assert_eq!(
            offer.artifact_revision,
            "8fea620810c4afa23dd6443f999a48574c1611a3"
        );
        assert_eq!(offer.license, "Apache-2.0");
        assert_eq!(offer.context_window_tokens, Some(4_096));
        assert!(offer.download_url.starts_with("https://huggingface.co/"));
        assert!(offer
            .download_url
            .contains("/resolve/8fea620810c4afa23dd6443f999a48574c1611a3/"));
        assert!(!offer.download_url.contains("/resolve/main/"));
        assert_eq!(offer.size_limit_bytes, 1024 * 1024 * 1024);
        assert!(is_trusted_offer(offer));
    }

    #[test]
    fn android_private_data_alias_requires_the_same_app_relative_path() {
        assert!(android_private_data_aliases_match(
            Path::new("/data/user/0/com.argentum/files/models"),
            Path::new("/data/data/com.argentum/files/models"),
        ));
        assert!(!android_private_data_aliases_match(
            Path::new("/data/user/0/com.argentum/files/models"),
            Path::new("/data/data/com.other/files/models"),
        ));
        assert!(!android_private_data_aliases_match(
            Path::new("/data/user/0/com.argentum/files/models"),
            Path::new("/data/local/tmp/models"),
        ));
        assert!(!android_private_data_aliases_match(
            Path::new("/data/user/0/com.argentum/cache/models"),
            Path::new("/data/data/com.argentum/cache/models"),
        ));
        assert!(!android_private_data_aliases_match(
            Path::new("/data/data/com.argentum/files/models"),
            Path::new("/data/user/0/com.argentum/files/models"),
        ));
    }

    #[test]
    fn public_model_download_future_fits_the_windows_stack_budget() {
        let future = download_local_model(
            *local_model_offers()
                .first()
                .expect("starter local model offer"),
            PathBuf::from(r"C:\\Argentum\\models"),
        );
        let future_bytes = std::mem::size_of_val(&future);

        assert!(
            future_bytes <= 16 * 1024,
            "public model-download future is {future_bytes} bytes, above the 16 KiB stack budget"
        );
        drop(future);
    }

    #[test]
    fn local_model_path_requires_an_absolute_gguf_file_shape() {
        assert!(matches!(
            validate_model_path_shape(Path::new("model.gguf")),
            Err(ProviderError::InvalidProfile {
                field: "model_path",
                ..
            })
        ));
        assert!(matches!(
            validate_model_path_shape(Path::new("C:\\models\\model.bin")),
            Err(ProviderError::InvalidProfile {
                field: "model_path",
                ..
            })
        ));
    }

    #[test]
    fn local_download_target_uses_only_the_pinned_offer_filename() {
        let offer = local_model_offers().first().expect("starter offer");
        let directory = Path::new(r"C:\Argentum\models");
        assert_eq!(
            validate_download_destination(offer, directory).expect("download target"),
            directory.join(offer.filename)
        );

        assert!(validate_download_destination(offer, Path::new("models")).is_err());
    }

    #[test]
    fn local_download_rejects_untrusted_offer_metadata() {
        let offer = LocalModelOffer {
            id: "untrusted",
            label: "untrusted",
            filename: "untrusted.gguf",
            download_url: "https://example.test/untrusted.gguf",
            source_revision: "untrusted",
            artifact_revision: "untrusted",
            size_bytes: 1,
            sha256: "0000000000000000000000000000000000000000000000000000000000000000",
            license: "Unknown",
            size_limit_bytes: 1,
            context_window_tokens: None,
        };
        assert!(!is_trusted_offer(&offer));
    }

    #[tokio::test]
    async fn local_download_never_replaces_an_existing_invalid_target() {
        let directory = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("..")
            .join("target")
            .join(format!(
                "local-model-test-{}-{}",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .expect("test clock")
                    .as_nanos()
            ));
        std::fs::create_dir_all(&directory).expect("model directory");
        let offer = *local_model_offers().first().expect("starter offer");
        let target = directory.join(offer.filename);
        std::fs::write(&target, b"not a verified model").expect("invalid model");

        let result = download_local_model(offer, &directory).await;

        assert!(matches!(
            result,
            Err(ProviderError::LocalModelIntegrityMismatch)
        ));
        assert_eq!(
            std::fs::read(&target).expect("existing model bytes"),
            b"not a verified model"
        );
        std::fs::remove_dir_all(&directory).expect("remove model test directory");
    }

    #[test]
    fn local_model_download_urls_require_https_and_approved_hosts() {
        assert!(is_allowed_download_url(
            &Url::parse("https://huggingface.co/model.gguf").expect("approved URL")
        ));
        assert!(is_allowed_download_url(
            &Url::parse("https://cdn-lfs.huggingface.co/model.gguf").expect("approved CDN URL")
        ));
        assert!(!is_allowed_download_url(
            &Url::parse("http://huggingface.co/model.gguf").expect("HTTP URL")
        ));
        assert!(!is_allowed_download_url(
            &Url::parse("https://huggingface.co.attacker.test/model.gguf").expect("unapproved URL")
        ));
    }

    #[tokio::test]
    async fn download_deadline_model_stall_returns_typed_error_and_cleans_part_file() {
        let bytes = b"ok";
        let offer = fixture_offer(bytes);
        let directory = tempfile::tempdir().expect("fixture directory");
        let server = spawn_download_server(
            vec![bytes[..1].to_vec(), bytes[1..].to_vec()],
            Duration::from_millis(400),
        );

        let result = tokio::time::timeout(
            Duration::from_millis(500),
            fixture_download(offer, &directory, &server.endpoint),
        )
        .await;

        assert!(matches!(
            result,
            Ok(Err(ProviderError::LocalModelDownloadDeadline {
                timeout_ms: 250
            }))
        ));
        assert_no_part_file(&directory);
        server.finish();
    }

    #[tokio::test]
    async fn download_deadline_model_slow_drip_cannot_outlive_total_deadline() {
        let bytes = b"abc";
        let offer = fixture_offer(bytes);
        let directory = tempfile::tempdir().expect("fixture directory");
        let server = spawn_download_server(
            vec![b"a".to_vec(), b"b".to_vec(), b"c".to_vec()],
            Duration::from_millis(140),
        );

        let result = tokio::time::timeout(
            Duration::from_millis(500),
            fixture_download(offer, &directory, &server.endpoint),
        )
        .await;

        assert!(matches!(
            result,
            Ok(Err(ProviderError::LocalModelDownloadDeadline {
                timeout_ms: 250
            }))
        ));
        assert_no_part_file(&directory);
        server.finish();
    }

    #[tokio::test]
    async fn download_model_bad_redirect_cleans_part_file() {
        let offer = fixture_offer(b"ok");
        let directory = tempfile::tempdir().expect("fixture directory");
        let server = spawn_raw_response_server(
            "HTTP/1.1 302 Found\r\nLocation: http://example.invalid/fixture.gguf\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".into(),
        );

        let result = fixture_download(offer, &directory, &server.endpoint).await;

        assert!(matches!(
            result,
            Err(ProviderError::LocalModelDownloadFailed)
        ));
        assert_no_part_file(&directory);
        server.finish();
    }

    #[tokio::test]
    async fn download_model_wrong_size_cleans_part_file() {
        let offer = fixture_offer(b"ab");
        let directory = tempfile::tempdir().expect("fixture directory");
        let target = directory.path().join(offer.filename);
        let mut server = spawn_paused_chunked_server();
        let endpoint = server.endpoint.clone();
        let download = tokio::spawn(async move {
            download_model_with_policy(
                offer,
                target,
                Url::parse(&endpoint).expect("fixture download URL"),
                fixture_policy(),
                fixture_download_url,
            )
            .await
        });

        server.wait_for_first_chunk().await;
        wait_for_part_contents(&directory, b"a").await;
        server.continue_download();
        let result = download.await.expect("oversized download task");

        assert!(matches!(
            result,
            Err(ProviderError::LocalModelDownloadTooLarge { limit_bytes: 2 })
        ));
        assert_no_part_file(&directory);
        server.finish();
    }

    #[tokio::test]
    async fn download_model_wrong_hash_cleans_part_file() {
        let offer = fixture_offer(b"ok");
        let directory = tempfile::tempdir().expect("fixture directory");
        let server = spawn_download_server(vec![b"no".to_vec()], Duration::ZERO);

        let result = fixture_download(offer, &directory, &server.endpoint).await;

        assert!(matches!(
            result,
            Err(ProviderError::LocalModelIntegrityMismatch)
        ));
        assert_no_part_file(&directory);
        server.finish();
    }

    #[tokio::test]
    async fn download_model_target_created_during_download_is_preserved_and_cleans_part_file() {
        let bytes = b"ok";
        let offer = fixture_offer(bytes);
        let directory = tempfile::tempdir().expect("fixture directory");
        let target = directory.path().join(offer.filename);
        let mut server = spawn_download_server(
            vec![bytes[..1].to_vec(), bytes[1..].to_vec()],
            Duration::from_millis(50),
        );
        let endpoint = server.endpoint.clone();
        let download = tokio::spawn(async move {
            download_model_with_policy(
                offer,
                target,
                Url::parse(&endpoint).expect("fixture download URL"),
                fixture_policy(),
                fixture_download_url,
            )
            .await
        });

        server.wait_for_first_chunk().await;
        let target = directory.path().join(offer.filename);
        std::fs::write(&target, b"concurrent target").expect("concurrent target bytes");

        let result = download.await.expect("download task");

        assert!(matches!(
            result,
            Err(ProviderError::LocalModelDownloadDestinationInvalid)
        ));
        assert_eq!(
            std::fs::read(&target).expect("concurrent target bytes"),
            b"concurrent target"
        );
        assert_no_part_file(&directory);
        server.finish();
    }

    #[tokio::test]
    async fn download_model_valid_existing_target_skips_network() {
        let bytes = b"ok";
        let offer = fixture_offer(bytes);
        let directory = tempfile::tempdir().expect("fixture directory");
        let target = directory.path().join(offer.filename);
        std::fs::write(&target, bytes).expect("verified target bytes");
        let server = spawn_no_request_server();

        let result = fixture_download(offer, &directory, &server.endpoint).await;

        assert_eq!(result.expect("valid existing target"), target);
        #[cfg(windows)]
        {
            let evidence = argentum_execution::owner_only_acl_evidence(&target)
                .expect("owner-only final model ACL evidence");
            assert!(evidence.protected);
            assert_eq!(evidence.inherited_ace_count, 0);
            assert_eq!(evidence.non_owner_allow_ace_count, 0);
            assert!(evidence.owner_full_control);
        }
        assert!(
            !server.finish(),
            "valid existing target made a network request"
        );
    }

    #[tokio::test]
    async fn download_model_atomic_publication_preserves_target_created_in_final_gap() {
        let bytes = b"ok";
        let offer = fixture_offer(bytes);
        let directory = tempfile::tempdir().expect("fixture directory");
        let target = directory.path().join(offer.filename);
        let gate = PublicationGate::new();
        // This control tests the atomic publication gap, not a 250ms transfer.
        // Use the actual production envelope and a bounded barrier watchdog.
        let mut policy = DownloadPolicy::model();
        policy.hooks.publication_gate = Some(gate.clone());
        let server = spawn_download_server(vec![bytes.to_vec()], Duration::ZERO);
        let endpoint = server.endpoint.clone();
        let download = tokio::spawn({
            let target = target.clone();
            async move {
                download_model_with_policy(
                    offer,
                    target,
                    Url::parse(&endpoint).expect("fixture download URL"),
                    policy,
                    fixture_download_url,
                )
                .await
            }
        });

        tokio::time::timeout(Duration::from_secs(30), gate.wait_until_reached())
            .await
            .expect("download reached the pre-publication barrier");
        std::fs::write(&target, b"created in final gap").expect("concurrent target bytes");
        gate.release().await;

        let result = download.await.expect("download task");

        assert!(matches!(
            result,
            Err(ProviderError::LocalModelDownloadDestinationInvalid)
        ));
        assert_eq!(
            std::fs::read(&target).expect("concurrent target bytes"),
            b"created in final gap"
        );
        assert_no_part_file(&directory);
        server.finish();
    }

    #[tokio::test]
    async fn download_model_deadline_before_publication_never_publishes_after_return() {
        let bytes = b"ok";
        let offer = fixture_offer(bytes);
        let directory = tempfile::tempdir().expect("fixture directory");
        let target = directory.path().join(offer.filename);
        let gate = PublicationGate::new();
        // Let real loopback I/O and owner-ACL setup reach the intended boundary
        // before driving the actual deadline with the test-only Tokio clock.
        let mut policy = DownloadPolicy::model();
        let total_timeout = policy.total_timeout;
        let timeout_ms = total_timeout.as_millis() as u64;
        policy.hooks.publication_gate = Some(gate.clone());
        let server = spawn_download_server(vec![bytes.to_vec()], Duration::ZERO);
        let endpoint = server.endpoint.clone();
        let download = tokio::spawn({
            let target = target.clone();
            async move {
                download_model_with_policy(
                    offer,
                    target,
                    Url::parse(&endpoint).expect("fixture download URL"),
                    policy,
                    fixture_download_url,
                )
                .await
            }
        });

        tokio::time::timeout(Duration::from_secs(30), gate.wait_until_reached())
            .await
            .expect("download reached the pre-publication barrier");
        tokio::time::pause();
        tokio::time::sleep(total_timeout).await;
        let result = tokio::time::timeout(Duration::from_secs(1), download)
            .await
            .expect("the expired download settled")
            .expect("deadline download task");

        assert!(matches!(
            result,
            Err(ProviderError::LocalModelDownloadDeadline { timeout_ms: observed })
                if observed == timeout_ms
        ));
        assert!(!target.exists(), "deadline returned after publication");
        // Keep the post-return observation in wall time, as before, so a late
        // blocking effect cannot hide behind the paused test clock.
        tokio::time::resume();
        tokio::time::sleep(Duration::from_millis(50)).await;
        assert!(
            !target.exists(),
            "publication continued after deadline result"
        );
        assert_no_part_file(&directory);
        server.finish();
    }
}
