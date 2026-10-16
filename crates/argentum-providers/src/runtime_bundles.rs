use std::fs::{File, OpenOptions};
use std::future::Future;
use std::io::Read;
use std::path::{Component, Path, PathBuf};
use std::time::Duration;

use argentum_execution::VerifiedFileLease;
#[cfg(windows)]
use argentum_execution::{ensure_owner_only_directory, ensure_owner_only_file};
use futures_util::StreamExt;
use sha2::{Digest, Sha256};
use tokio::io::AsyncWriteExt;
use url::Url;
use zip::ZipArchive;

use crate::ProviderError;

const MAX_RUNTIME_ARCHIVE_BYTES: u64 = 64 * 1024 * 1024;
const MAX_RUNTIME_ENTRIES: usize = 64;
const MAX_RUNTIME_FILE_BYTES: u64 = 64 * 1024 * 1024;
const MAX_RUNTIME_EXPANDED_BYTES: u64 = 128 * 1024 * 1024;
const RUNTIME_DOWNLOAD_TOTAL_TIMEOUT: Duration = Duration::from_secs(20 * 60);
const ARTIFACT_CONNECT_TIMEOUT: Duration = Duration::from_secs(30);

#[derive(Clone)]
struct DownloadPolicy {
    total_timeout: Duration,
    connect_timeout: Duration,
    #[cfg(test)]
    hooks: DownloadTestHooks,
}

impl DownloadPolicy {
    fn runtime() -> Self {
        Self {
            total_timeout: RUNTIME_DOWNLOAD_TOTAL_TIMEOUT,
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
            return Err(ProviderError::LocalRuntimeDownloadDeadline {
                timeout_ms: self.timeout_ms,
            });
        }
        Ok(())
    }

    async fn wait<T>(self, future: impl Future<Output = T>) -> Result<T, ProviderError> {
        tokio::time::timeout_at(self.expires_at, future)
            .await
            .map_err(|_| ProviderError::LocalRuntimeDownloadDeadline {
                timeout_ms: self.timeout_ms,
            })
    }
}

#[cfg(test)]
#[derive(Clone, Default)]
struct DownloadTestHooks {
    publication_gate: Option<PublicationGate>,
    existing_inspection_gate: Option<InspectionGate>,
    final_inspection_gate: Option<InspectionGate>,
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

#[cfg(test)]
#[derive(Clone)]
struct InspectionGate {
    entered: std::sync::Arc<std::sync::Mutex<Option<tokio::sync::oneshot::Sender<()>>>>,
    release: std::sync::Arc<std::sync::Mutex<std::sync::mpsc::Receiver<()>>>,
}

#[cfg(test)]
struct InspectionGateControl {
    entered: tokio::sync::oneshot::Receiver<()>,
    release: std::sync::mpsc::Sender<()>,
}

#[cfg(test)]
impl InspectionGate {
    fn new() -> (Self, InspectionGateControl) {
        let (entered_sender, entered) = tokio::sync::oneshot::channel();
        let (release, release_receiver) = std::sync::mpsc::channel();
        (
            Self {
                entered: std::sync::Arc::new(std::sync::Mutex::new(Some(entered_sender))),
                release: std::sync::Arc::new(std::sync::Mutex::new(release_receiver)),
            },
            InspectionGateControl { entered, release },
        )
    }

    fn wait_until_released_or_deadline(&self, deadline: std::time::Instant) -> bool {
        if let Some(entered) = self.entered.lock().expect("inspection gate lock").take() {
            let _ = entered.send(());
        }
        let remaining = deadline.saturating_duration_since(std::time::Instant::now());
        !remaining.is_zero()
            && self
                .release
                .lock()
                .expect("inspection gate lock")
                .recv_timeout(remaining)
                .is_ok()
            && std::time::Instant::now() < deadline
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LocalRuntimeOffer {
    pub id: &'static str,
    pub version: &'static str,
    pub backend: &'static str,
    pub platform: &'static str,
    pub arch: &'static str,
    pub install_dir: &'static str,
    pub filename: &'static str,
    pub download_url: &'static str,
    pub size_bytes: u64,
    pub sha256: &'static str,
    pub executable: &'static str,
    pub executable_size_bytes: u64,
    pub executable_sha256: &'static str,
    pub license: &'static str,
}

const LOCAL_RUNTIME_OFFERS: [LocalRuntimeOffer; 1] = [LocalRuntimeOffer {
    id: "llama-cpp-v0.3.0-b10621-vulkan-x64",
    version: "v0.3.0",
    backend: "vulkan",
    platform: "windows",
    arch: "x86_64",
    install_dir: "vulkan-x64",
    filename: "llama-b10621-bin-win-vulkan-x64.zip",
    download_url:
        "https://github.com/ggml-org/llama.cpp/releases/download/b10621/llama-b10621-bin-win-vulkan-x64.zip",
    size_bytes: 34_403_304,
    sha256: "2672d85bf87c8280d94dee01eb6a86280046878f70a07d786a93637fa9081163",
    executable: "llama-server.exe",
    executable_size_bytes: 9_216,
    executable_sha256:
        "0f706d509ce7937504d527f49cc1e68a0e45ee60cfc4153b3ff16f76a0f4e3ac",
    license: "MIT",
}];

pub fn local_runtime_offers() -> &'static [LocalRuntimeOffer] {
    &LOCAL_RUNTIME_OFFERS
}

pub fn local_runtime_bundle_path(
    root: &Path,
    offer: &LocalRuntimeOffer,
) -> Result<PathBuf, ProviderError> {
    if !runtime_offer_is_trusted(offer)
        || !root.is_absolute()
        || root.as_os_str().to_string_lossy().len() > 2_048
        || root.as_os_str().to_string_lossy().contains('\0')
    {
        return Err(ProviderError::LocalRuntimeBundleInvalid);
    }
    Ok(root
        .join("llama-cpp")
        .join(offer.version)
        .join(offer.install_dir)
        .join("bundle.zip"))
}

pub fn runtime_bundle_is_valid(path: &Path, offer: &LocalRuntimeOffer) -> bool {
    runtime_offer_is_trusted(offer) && inspect_runtime_archive(path, offer).is_ok()
}

pub async fn install_local_runtime(
    offer: LocalRuntimeOffer,
    root: impl AsRef<Path>,
) -> Result<PathBuf, ProviderError> {
    if !runtime_offer_is_trusted(&offer) {
        return Err(ProviderError::LocalRuntimeBundleInvalid);
    }
    prepare_private_directory(root.as_ref())?;
    let target = local_runtime_bundle_path(root.as_ref(), &offer)?;
    let parent = target
        .parent()
        .ok_or(ProviderError::LocalRuntimeBundleInvalid)?;
    prepare_private_directory(parent)?;

    let url =
        Url::parse(offer.download_url).map_err(|_| ProviderError::LocalRuntimeBundleInvalid)?;
    if !is_allowed_runtime_download_url(&url) {
        return Err(ProviderError::LocalRuntimeBundleInvalid);
    }
    download_runtime_bundle_with_policy(
        offer,
        target,
        url,
        DownloadPolicy::runtime(),
        is_allowed_runtime_download_url,
    )
    .await
}

async fn download_runtime_bundle_with_policy(
    offer: LocalRuntimeOffer,
    target: PathBuf,
    url: Url,
    policy: DownloadPolicy,
    is_allowed_url: fn(&Url) -> bool,
) -> Result<PathBuf, ProviderError> {
    let deadline = DownloadDeadline::new(policy.total_timeout);
    download_runtime_bundle_inner(
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

async fn download_runtime_bundle_inner(
    offer: LocalRuntimeOffer,
    target: PathBuf,
    url: Url,
    connect_timeout: Duration,
    is_allowed_url: fn(&Url) -> bool,
    deadline: DownloadDeadline,
    #[cfg(test)] hooks: DownloadTestHooks,
) -> Result<PathBuf, ProviderError> {
    deadline.check()?;
    if std::fs::symlink_metadata(&target).is_ok() {
        match inspect_runtime_archive_before_deadline(
            target.clone(),
            offer,
            deadline,
            #[cfg(test)]
            hooks.existing_inspection_gate.clone(),
        )
        .await
        {
            Ok(()) => {
                protect_runtime_file(&target)?;
                return Ok(target);
            }
            Err(ProviderError::LocalRuntimeDownloadDeadline { timeout_ms }) => {
                return Err(ProviderError::LocalRuntimeDownloadDeadline { timeout_ms });
            }
            Err(_) => return Err(ProviderError::LocalRuntimeIntegrityMismatch),
        }
    }

    if !is_allowed_url(&url) {
        return Err(ProviderError::LocalRuntimeBundleInvalid);
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
                attempt.error("runtime download redirect is not allowed")
            }
        }))
        .build()
        .map_err(|_| ProviderError::LocalRuntimeDownloadFailed)?;
    let response = deadline
        .wait(client.get(url).send())
        .await?
        .map_err(|_| ProviderError::LocalRuntimeDownloadFailed)?;
    if !response.status().is_success() || !is_allowed_url(response.url()) {
        return Err(ProviderError::LocalRuntimeDownloadFailed);
    }
    if response
        .content_length()
        .is_some_and(|length| length != offer.size_bytes)
    {
        return Err(ProviderError::LocalRuntimeIntegrityMismatch);
    }

    let temporary = target.with_file_name(format!(
        ".bundle.{}.{}.part",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or_default()
    ));
    let guard = TemporaryPath::new(temporary.clone());
    let mut file = deadline
        .wait(
            tokio::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&temporary),
        )
        .await?
        .map_err(|_| ProviderError::LocalRuntimeDownloadFailed)?;
    protect_runtime_file(&temporary)?;
    let mut stream = response.bytes_stream();
    let mut hasher = Sha256::new();
    let mut total_bytes = 0_u64;
    while let Some(chunk) = deadline.wait(stream.next()).await? {
        let chunk = chunk.map_err(|_| ProviderError::LocalRuntimeDownloadFailed)?;
        total_bytes = total_bytes.checked_add(chunk.len() as u64).ok_or(
            ProviderError::LocalRuntimeDownloadTooLarge {
                limit_bytes: MAX_RUNTIME_ARCHIVE_BYTES,
            },
        )?;
        if total_bytes > MAX_RUNTIME_ARCHIVE_BYTES || total_bytes > offer.size_bytes {
            return Err(ProviderError::LocalRuntimeDownloadTooLarge {
                limit_bytes: offer.size_bytes,
            });
        }
        hasher.update(&chunk);
        deadline
            .wait(file.write_all(&chunk))
            .await?
            .map_err(|_| ProviderError::LocalRuntimeDownloadFailed)?;
    }
    deadline
        .wait(file.sync_all())
        .await?
        .map_err(|_| ProviderError::LocalRuntimeDownloadFailed)?;
    drop(file);

    if total_bytes != offer.size_bytes || format!("{:x}", hasher.finalize()) != offer.sha256 {
        return Err(ProviderError::LocalRuntimeIntegrityMismatch);
    }
    inspect_runtime_archive_before_deadline(
        temporary.clone(),
        offer,
        deadline,
        #[cfg(test)]
        hooks.final_inspection_gate.clone(),
    )
    .await?;
    #[cfg(test)]
    if let Some(gate) = hooks.publication_gate {
        deadline.wait(gate.wait_for_release()).await?;
    }
    publish_runtime_without_clobber(temporary.clone(), target.clone(), deadline).await?;
    drop(guard);
    Ok(target)
}

async fn publish_runtime_without_clobber(
    temporary: PathBuf,
    target: PathBuf,
    deadline: DownloadDeadline,
) -> Result<(), ProviderError> {
    let expires_at = deadline.expires_at.into_std();
    let timeout_ms = deadline.timeout_ms;
    tokio::task::spawn_blocking(move || {
        if std::time::Instant::now() >= expires_at {
            return Err(ProviderError::LocalRuntimeDownloadDeadline { timeout_ms });
        }
        match std::fs::hard_link(&temporary, &target) {
            Ok(()) => protect_runtime_file(&target),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                Err(ProviderError::LocalRuntimeBundleInvalid)
            }
            Err(_) => Err(ProviderError::LocalRuntimeDownloadFailed),
        }
    })
    .await
    .map_err(|_| ProviderError::LocalRuntimeDownloadFailed)?
}

async fn inspect_runtime_archive_before_deadline(
    path: PathBuf,
    offer: LocalRuntimeOffer,
    deadline: DownloadDeadline,
    #[cfg(test)] inspection_gate: Option<InspectionGate>,
) -> Result<(), ProviderError> {
    deadline.check()?;
    let inspection_deadline = deadline.expires_at.into_std();
    let timeout_ms = deadline.timeout_ms;
    deadline
        .wait(tokio::task::spawn_blocking(move || {
            #[cfg(test)]
            if inspection_gate
                .is_some_and(|gate| !gate.wait_until_released_or_deadline(inspection_deadline))
            {
                return Err(ProviderError::LocalRuntimeDownloadDeadline { timeout_ms });
            }
            inspect_runtime_archive_bounded(&path, &offer, Some((inspection_deadline, timeout_ms)))
        }))
        .await?
        .map_err(|_| ProviderError::LocalRuntimeBundleInvalid)?
}

pub(crate) fn inspect_runtime_archive(
    path: &Path,
    offer: &LocalRuntimeOffer,
) -> Result<(), ProviderError> {
    inspect_runtime_archive_bounded(path, offer, None)
}

pub(crate) fn open_runtime_archive_lease(
    path: &Path,
    offer: &LocalRuntimeOffer,
) -> Result<VerifiedFileLease, ProviderError> {
    if !runtime_offer_is_trusted(offer) {
        return Err(ProviderError::LocalRuntimeBundleInvalid);
    }
    protect_runtime_file(path)?;
    let mut lease = VerifiedFileLease::open(path, MAX_RUNTIME_ARCHIVE_BYTES)
        .map_err(|_| ProviderError::LocalRuntimeBundleInvalid)?;
    inspect_runtime_archive_lease(&mut lease, offer)?;
    Ok(lease)
}

fn inspect_runtime_archive_lease(
    lease: &mut VerifiedFileLease,
    offer: &LocalRuntimeOffer,
) -> Result<(), ProviderError> {
    if lease.size_bytes() != offer.size_bytes || lease.sha256() != offer.sha256 {
        return Err(ProviderError::LocalRuntimeIntegrityMismatch);
    }
    lease
        .verify_unchanged()
        .map_err(|_| ProviderError::LocalRuntimeIntegrityMismatch)?;
    inspect_runtime_archive(lease.path(), offer)?;
    lease
        .verify_unchanged()
        .map_err(|_| ProviderError::LocalRuntimeIntegrityMismatch)
}

fn inspect_runtime_archive_bounded(
    path: &Path,
    offer: &LocalRuntimeOffer,
    deadline: Option<(std::time::Instant, u64)>,
) -> Result<(), ProviderError> {
    check_archive_deadline(deadline)?;
    let metadata =
        std::fs::symlink_metadata(path).map_err(|_| ProviderError::LocalRuntimeBundleInvalid)?;
    check_archive_deadline(deadline)?;
    if metadata.file_type().is_symlink()
        || !metadata.is_file()
        || metadata.len() != offer.size_bytes
        || metadata.len() > MAX_RUNTIME_ARCHIVE_BYTES
        || file_sha256_with_deadline(path, deadline)? != offer.sha256
    {
        return Err(ProviderError::LocalRuntimeIntegrityMismatch);
    }

    check_archive_deadline(deadline)?;
    let file = File::open(path).map_err(|_| ProviderError::LocalRuntimeBundleInvalid)?;
    let mut archive =
        ZipArchive::new(file).map_err(|_| ProviderError::LocalRuntimeBundleInvalid)?;
    check_archive_deadline(deadline)?;
    if archive.is_empty() || archive.len() > MAX_RUNTIME_ENTRIES {
        return Err(ProviderError::LocalRuntimeBundleInvalid);
    }
    let mut expanded = 0_u64;
    let mut launcher_count = 0_usize;
    for index in 0..archive.len() {
        check_archive_deadline(deadline)?;
        let mut entry = archive
            .by_index(index)
            .map_err(|_| ProviderError::LocalRuntimeBundleInvalid)?;
        check_archive_deadline(deadline)?;
        let relative = validated_entry_path(&entry)?;
        expanded = expanded
            .checked_add(entry.size())
            .ok_or(ProviderError::LocalRuntimeBundleInvalid)?;
        if expanded > MAX_RUNTIME_EXPANDED_BYTES {
            return Err(ProviderError::LocalRuntimeBundleInvalid);
        }
        if relative == Path::new(offer.executable) {
            launcher_count += 1;
            if !entry.is_file() || entry.size() != offer.executable_size_bytes {
                return Err(ProviderError::LocalRuntimeIntegrityMismatch);
            }
            let mut hasher = Sha256::new();
            let mut copied = 0_u64;
            let mut buffer = [0_u8; 64 * 1024];
            loop {
                check_archive_deadline(deadline)?;
                let read = entry
                    .read(&mut buffer)
                    .map_err(|_| ProviderError::LocalRuntimeBundleInvalid)?;
                check_archive_deadline(deadline)?;
                if read == 0 {
                    break;
                }
                copied = copied
                    .checked_add(read as u64)
                    .ok_or(ProviderError::LocalRuntimeBundleInvalid)?;
                if copied > offer.executable_size_bytes {
                    return Err(ProviderError::LocalRuntimeIntegrityMismatch);
                }
                hasher.update(&buffer[..read]);
            }
            if copied != offer.executable_size_bytes
                || format!("{:x}", hasher.finalize()) != offer.executable_sha256
            {
                return Err(ProviderError::LocalRuntimeIntegrityMismatch);
            }
        }
    }
    if launcher_count != 1 {
        return Err(ProviderError::LocalRuntimeBundleInvalid);
    }
    Ok(())
}

fn check_archive_deadline(
    deadline: Option<(std::time::Instant, u64)>,
) -> Result<(), ProviderError> {
    if let Some((expires_at, timeout_ms)) = deadline {
        if std::time::Instant::now() >= expires_at {
            return Err(ProviderError::LocalRuntimeDownloadDeadline { timeout_ms });
        }
    }
    Ok(())
}

#[cfg(test)]
pub(crate) fn extract_runtime_session(
    path: &Path,
    offer: &LocalRuntimeOffer,
    parent: &Path,
) -> Result<tempfile::TempDir, ProviderError> {
    inspect_runtime_archive(path, offer)?;
    extract_runtime_session_inner(path, offer, parent)
}

pub(crate) fn extract_runtime_session_leased(
    lease: &mut VerifiedFileLease,
    offer: &LocalRuntimeOffer,
    parent: &Path,
) -> Result<tempfile::TempDir, ProviderError> {
    inspect_runtime_archive_lease(lease, offer)?;
    let session = extract_runtime_session_inner(lease.path(), offer, parent)?;
    lease
        .verify_unchanged()
        .map_err(|_| ProviderError::LocalRuntimeIntegrityMismatch)?;
    Ok(session)
}

fn extract_runtime_session_inner(
    path: &Path,
    offer: &LocalRuntimeOffer,
    parent: &Path,
) -> Result<tempfile::TempDir, ProviderError> {
    prepare_private_directory(parent)?;
    let session = tempfile::Builder::new()
        .prefix(".argentum-runtime-")
        .tempdir_in(parent)
        .map_err(|_| ProviderError::LocalRuntimeBundleInvalid)?;
    protect_runtime_directory(session.path())?;
    let file = File::open(path).map_err(|_| ProviderError::LocalRuntimeBundleInvalid)?;
    let mut archive =
        ZipArchive::new(file).map_err(|_| ProviderError::LocalRuntimeBundleInvalid)?;
    let mut expanded = 0_u64;
    for index in 0..archive.len() {
        let mut entry = archive
            .by_index(index)
            .map_err(|_| ProviderError::LocalRuntimeBundleInvalid)?;
        let relative = validated_entry_path(&entry)?;
        expanded = expanded
            .checked_add(entry.size())
            .ok_or(ProviderError::LocalRuntimeBundleInvalid)?;
        if expanded > MAX_RUNTIME_EXPANDED_BYTES {
            return Err(ProviderError::LocalRuntimeBundleInvalid);
        }
        let target = session.path().join(relative);
        if entry.is_dir() {
            prepare_private_directory(&target)?;
            continue;
        }
        let target_parent = target
            .parent()
            .ok_or(ProviderError::LocalRuntimeBundleInvalid)?;
        prepare_private_directory(target_parent)?;
        let mut output = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&target)
            .map_err(|_| ProviderError::LocalRuntimeBundleInvalid)?;
        protect_runtime_file(&target)?;
        let copied = std::io::copy(&mut entry, &mut output)
            .map_err(|_| ProviderError::LocalRuntimeBundleInvalid)?;
        if copied != entry.size() || copied > MAX_RUNTIME_FILE_BYTES {
            return Err(ProviderError::LocalRuntimeBundleInvalid);
        }
        output
            .sync_all()
            .map_err(|_| ProviderError::LocalRuntimeBundleInvalid)?;
    }
    let launcher = session.path().join(offer.executable);
    let metadata = std::fs::symlink_metadata(&launcher)
        .map_err(|_| ProviderError::LocalRuntimeBundleInvalid)?;
    if metadata.file_type().is_symlink()
        || !metadata.is_file()
        || metadata.len() != offer.executable_size_bytes
        || file_sha256(&launcher)? != offer.executable_sha256
    {
        return Err(ProviderError::LocalRuntimeIntegrityMismatch);
    }
    Ok(session)
}

pub(crate) fn open_runtime_launcher_lease(
    session: &Path,
    offer: &LocalRuntimeOffer,
) -> Result<VerifiedFileLease, ProviderError> {
    let launcher = session.join(offer.executable);
    protect_runtime_file(&launcher)?;
    let lease = VerifiedFileLease::open(&launcher, MAX_RUNTIME_FILE_BYTES)
        .map_err(|_| ProviderError::LocalRuntimeBundleInvalid)?;
    if lease.size_bytes() != offer.executable_size_bytes
        || lease.sha256() != offer.executable_sha256
    {
        return Err(ProviderError::LocalRuntimeIntegrityMismatch);
    }
    Ok(lease)
}

fn runtime_offer_is_trusted(offer: &LocalRuntimeOffer) -> bool {
    LOCAL_RUNTIME_OFFERS
        .iter()
        .any(|candidate| candidate == offer)
}

fn validated_entry_path<R: Read>(
    entry: &zip::read::ZipFile<'_, R>,
) -> Result<PathBuf, ProviderError> {
    let name = entry
        .name()
        .map_err(|_| ProviderError::LocalRuntimeBundleInvalid)?;
    // zip 9 normalizes roots, drive prefixes and parent segments. Validate
    // the original component contract before asking it for a relative path.
    let drive_prefix =
        name.as_bytes().get(1) == Some(&b':') && name.as_bytes()[0].is_ascii_alphabetic();
    if entry.encrypted()
        || entry.is_symlink()
        || (!entry.is_file() && !entry.is_dir())
        || entry.size() > MAX_RUNTIME_FILE_BYTES
        || name.contains('\\')
        || name.contains('\0')
        || Path::new(name.as_ref())
            .components()
            .any(|component| !matches!(component, Component::Normal(_)))
        || drive_prefix
    {
        return Err(ProviderError::LocalRuntimeBundleInvalid);
    }
    let relative = entry
        .enclosed_name()
        .ok_or(ProviderError::LocalRuntimeBundleInvalid)?;
    if relative.as_os_str().is_empty()
        || relative
            .components()
            .any(|component| !matches!(component, Component::Normal(_)))
    {
        return Err(ProviderError::LocalRuntimeBundleInvalid);
    }
    Ok(relative.to_path_buf())
}

fn prepare_private_directory(path: &Path) -> Result<(), ProviderError> {
    if !path.is_absolute()
        || path.as_os_str().to_string_lossy().len() > 2_048
        || path.as_os_str().to_string_lossy().contains('\0')
    {
        return Err(ProviderError::LocalRuntimeBundleInvalid);
    }
    let mut current = PathBuf::new();
    let mut created_component = false;
    for component in path.components() {
        current.push(component);
        if !matches!(component, Component::Normal(_)) {
            continue;
        }
        match std::fs::symlink_metadata(&current) {
            Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_dir() => {
                return Err(ProviderError::LocalRuntimeBundleInvalid);
            }
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                protect_runtime_directory(&current)?;
                created_component = true;
            }
            Err(_) => return Err(ProviderError::LocalRuntimeBundleInvalid),
        }
        if created_component {
            protect_runtime_directory(&current)?;
        }
    }
    protect_runtime_directory(path)?;
    Ok(())
}

fn protect_runtime_directory(path: &Path) -> Result<(), ProviderError> {
    #[cfg(windows)]
    ensure_owner_only_directory(path).map_err(|_| ProviderError::LocalRuntimeBundleInvalid)?;
    #[cfg(not(windows))]
    if !path.exists() {
        std::fs::create_dir(path).map_err(|_| ProviderError::LocalRuntimeBundleInvalid)?;
    }
    Ok(())
}

fn protect_runtime_file(path: &Path) -> Result<(), ProviderError> {
    #[cfg(windows)]
    ensure_owner_only_file(path).map_err(|_| ProviderError::LocalRuntimeBundleInvalid)?;
    #[cfg(not(windows))]
    let _ = path;
    Ok(())
}

fn file_sha256(path: &Path) -> Result<String, ProviderError> {
    file_sha256_with_deadline(path, None)
}

fn file_sha256_with_deadline(
    path: &Path,
    deadline: Option<(std::time::Instant, u64)>,
) -> Result<String, ProviderError> {
    check_archive_deadline(deadline)?;
    let mut file = File::open(path).map_err(|_| ProviderError::LocalRuntimeBundleInvalid)?;
    let mut hasher = Sha256::new();
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        check_archive_deadline(deadline)?;
        let read = file
            .read(&mut buffer)
            .map_err(|_| ProviderError::LocalRuntimeBundleInvalid)?;
        check_archive_deadline(deadline)?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

fn is_allowed_runtime_download_url(url: &Url) -> bool {
    url.scheme() == "https"
        && matches!(
            url.host_str(),
            Some("github.com")
                | Some("release-assets.githubusercontent.com")
                | Some("objects.githubusercontent.com")
        )
}

struct TemporaryPath {
    path: PathBuf,
}

impl TemporaryPath {
    fn new(path: PathBuf) -> Self {
        Self { path }
    }
}

impl Drop for TemporaryPath {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

#[cfg(test)]
mod tests {
    use std::io::{Cursor, Read, Write};
    use std::net::{TcpListener, TcpStream};
    use std::path::Path;
    use std::sync::mpsc;
    use std::thread;
    use std::time::Duration;

    use sha2::{Digest, Sha256};
    use tempfile::TempDir;
    use zip::write::SimpleFileOptions;
    use zip::ZipWriter;

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
            endpoint: format!("http://{address}/fixture.zip"),
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
            endpoint: format!("http://{address}/fixture.zip"),
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
            endpoint: format!("http://{address}/fixture.zip"),
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
            endpoint: format!("http://{address}/fixture.zip"),
            accepted,
            handle,
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

    async fn fixture_download(
        offer: LocalRuntimeOffer,
        directory: &TempDir,
        endpoint: &str,
    ) -> Result<PathBuf, ProviderError> {
        let target = directory.path().join("bundle.zip");
        download_runtime_bundle_with_policy(
            offer,
            target,
            Url::parse(endpoint).expect("fixture download URL"),
            fixture_policy(),
            fixture_download_url,
        )
        .await
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
                                .expect("owner-only runtime part ACL evidence");
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

    fn digest(bytes: &[u8]) -> &'static str {
        Box::leak(format!("{:x}", Sha256::digest(bytes)).into_boxed_str())
    }

    fn write_archive(
        directory: &TempDir,
        name: &str,
        build: impl FnOnce(&mut ZipWriter<Cursor<Vec<u8>>>),
    ) -> (std::path::PathBuf, Vec<u8>) {
        let mut writer = ZipWriter::new(Cursor::new(Vec::new()));
        build(&mut writer);
        let bytes = writer.finish().expect("finish ZIP").into_inner();
        let path = directory.path().join(name);
        std::fs::write(&path, &bytes).expect("write ZIP fixture");
        (path, bytes)
    }

    fn fixture_offer(archive: &[u8], executable: &[u8]) -> LocalRuntimeOffer {
        LocalRuntimeOffer {
            id: "test-vulkan-x64",
            version: "test",
            backend: "vulkan",
            platform: "windows",
            arch: "x86_64",
            install_dir: "vulkan-x64",
            filename: "fixture.zip",
            download_url: "https://github.com/example/fixture.zip",
            size_bytes: archive.len() as u64,
            sha256: digest(archive),
            executable: "llama-server.exe",
            executable_size_bytes: executable.len() as u64,
            executable_sha256: digest(executable),
            license: "MIT",
        }
    }

    fn valid_fixture_archive(directory: &TempDir) -> (LocalRuntimeOffer, Vec<u8>) {
        let executable = b"verified server";
        let (_, archive) = write_archive(directory, "fixture.zip", |writer| {
            writer
                .start_file("llama-server.exe", SimpleFileOptions::default())
                .expect("server entry");
            writer.write_all(executable).expect("server bytes");
        });
        (fixture_offer(&archive, executable), archive)
    }

    #[test]
    fn official_runtime_offer_is_exact_and_trusted() {
        let offer = local_runtime_offers().first().expect("runtime offer");

        assert!(runtime_offer_is_trusted(offer));
        assert_eq!(offer.version, "v0.3.0");
        assert_eq!(offer.backend, "vulkan");
        assert_eq!(offer.platform, "windows");
        assert_eq!(offer.arch, "x86_64");
        assert_eq!(offer.size_bytes, 34_403_304);
        assert_eq!(
            offer.sha256,
            "2672d85bf87c8280d94dee01eb6a86280046878f70a07d786a93637fa9081163"
        );
        assert_eq!(offer.executable, "llama-server.exe");
        assert_eq!(offer.executable_size_bytes, 9_216);
        assert_eq!(
            offer.executable_sha256,
            "0f706d509ce7937504d527f49cc1e68a0e45ee60cfc4153b3ff16f76a0f4e3ac"
        );
        assert_eq!(offer.license, "MIT");
    }

    #[test]
    fn valid_archive_extracts_only_inside_owned_session() {
        let directory = tempfile::tempdir().expect("fixture directory");
        let executable = b"verified server";
        let (archive_path, archive) = write_archive(&directory, "valid.zip", |writer| {
            let options = SimpleFileOptions::default();
            writer
                .start_file("llama-server.exe", options)
                .expect("server entry");
            writer.write_all(executable).expect("server bytes");
            writer
                .start_file("ggml-vulkan.dll", options)
                .expect("DLL entry");
            writer.write_all(b"backend").expect("DLL bytes");
        });
        let offer = fixture_offer(&archive, executable);

        inspect_runtime_archive(&archive_path, &offer).expect("valid archive");
        let session = extract_runtime_session(&archive_path, &offer, directory.path())
            .expect("runtime session");

        assert_eq!(
            std::fs::read(session.path().join("llama-server.exe")).expect("server file"),
            executable
        );
        assert!(session.path().starts_with(directory.path()));
        assert!(!directory.path().join("escape.txt").exists());
    }

    #[cfg(windows)]
    #[test]
    fn archive_and_launcher_leases_block_replacement_before_activation() {
        let directory = tempfile::tempdir().expect("runtime lease fixture");
        let (offer, archive) = valid_fixture_archive(&directory);
        let archive_path = directory.path().join("fixture.zip");
        protect_runtime_file(&archive_path).expect("protect archive");
        let mut archive_lease = VerifiedFileLease::open(&archive_path, MAX_RUNTIME_ARCHIVE_BYTES)
            .expect("archive lease");
        let session = extract_runtime_session_leased(&mut archive_lease, &offer, directory.path())
            .expect("leased runtime extraction");
        let launcher_path = session.path().join(offer.executable);
        let launcher_lease =
            open_runtime_launcher_lease(session.path(), &offer).expect("launcher lease");

        assert_eq!(archive_lease.sha256(), digest(&archive));
        assert!(OpenOptions::new().write(true).open(&archive_path).is_err());
        assert!(std::fs::remove_file(&archive_path).is_err());
        assert!(OpenOptions::new().write(true).open(&launcher_path).is_err());
        assert!(std::fs::remove_file(&launcher_path).is_err());
        archive_lease
            .verify_unchanged()
            .expect("archive unchanged before activation");
        launcher_lease
            .verify_identity()
            .expect("launcher identity before activation");

        for path in [
            session.path(),
            archive_path.as_path(),
            launcher_path.as_path(),
        ] {
            let evidence = argentum_execution::owner_only_acl_evidence(path)
                .expect("owner-only runtime ACL evidence");
            assert!(evidence.protected);
            assert_eq!(evidence.inherited_ace_count, 0);
            assert_eq!(evidence.non_owner_allow_ace_count, 0);
            assert!(evidence.owner_full_control);
        }

        drop(launcher_lease);
        drop(archive_lease);
        OpenOptions::new()
            .write(true)
            .open(&launcher_path)
            .expect("launcher replacement allowed after lease");
        OpenOptions::new()
            .write(true)
            .open(&archive_path)
            .expect("archive replacement allowed after lease");
    }

    #[test]
    fn archive_rejects_traversal_links_and_wrong_launcher() {
        let directory = tempfile::tempdir().expect("fixture directory");
        let options = SimpleFileOptions::default();

        let (traversal_path, traversal) = write_archive(&directory, "traversal.zip", |writer| {
            writer
                .start_file("../escape.txt", options)
                .expect("traversal entry");
            writer.write_all(b"escape").expect("traversal bytes");
            writer
                .start_file("llama-server.exe", options)
                .expect("server entry");
            writer.write_all(b"server").expect("server bytes");
        });
        let traversal_offer = fixture_offer(&traversal, b"server");
        assert!(inspect_runtime_archive(&traversal_path, &traversal_offer).is_err());

        let (link_path, link) = write_archive(&directory, "link.zip", |writer| {
            writer
                .add_symlink("llama-server.exe", "target.exe", options)
                .expect("symlink entry");
        });
        let link_offer = fixture_offer(&link, b"target.exe");
        assert!(inspect_runtime_archive(&link_path, &link_offer).is_err());

        let (wrong_path, wrong) = write_archive(&directory, "wrong.zip", |writer| {
            writer
                .start_file("llama-server.exe", options)
                .expect("server entry");
            writer.write_all(b"wrong").expect("wrong bytes");
        });
        let wrong_offer = fixture_offer(&wrong, b"expected");
        assert!(inspect_runtime_archive(&wrong_path, &wrong_offer).is_err());
    }

    #[test]
    fn archive_supported_compression_and_unicode_survive_extraction() {
        let directory = tempfile::tempdir().expect("fixture directory");
        let executable = b"verified server";
        for method in [
            zip::CompressionMethod::Stored,
            zip::CompressionMethod::Deflated,
        ] {
            let (path, bytes) = write_archive(&directory, "formats.zip", |writer| {
                let options = SimpleFileOptions::default().compression_method(method);
                writer
                    .start_file("llama-server.exe", options)
                    .expect("launcher");
                writer.write_all(executable).expect("launcher bytes");
                writer
                    .start_file("data/café-日本語.txt", options)
                    .expect("Unicode path");
                writer
                    .write_all("café 日本語".as_bytes())
                    .expect("Unicode bytes");
            });
            let offer = fixture_offer(&bytes, executable);
            let session =
                extract_runtime_session(&path, &offer, directory.path()).expect("extract");
            assert_eq!(
                std::fs::read(session.path().join(offer.executable)).expect("launcher"),
                executable
            );
            assert_eq!(
                std::fs::read_to_string(session.path().join("data/café-日本語.txt"))
                    .expect("Unicode file"),
                "café 日本語"
            );
            assert_eq!(std::fs::read(&path).expect("unchanged archive"), bytes);
        }
    }

    #[test]
    fn archive_rejects_unsafe_names_and_entry_count_before_extraction() {
        let directory = tempfile::tempdir().expect("fixture directory");
        for name in [
            "../escape.txt",
            "data/../escape.txt",
            "./escape.txt",
            "/escape.txt",
            "C:/escape.txt",
            "C:escape.txt",
            "data\\escape.txt",
        ] {
            // Newer writers normalize names. Inject exact raw header bytes so
            // these controls exercise the consumer, not writer sanitization.
            let placeholder = "x".repeat(name.len());
            let (path, mut bytes) = write_archive(&directory, "unsafe.zip", |writer| {
                writer
                    .start_file(&placeholder, SimpleFileOptions::default())
                    .expect("unsafe fixture");
                writer.write_all(b"server").expect("fixture bytes");
                writer
                    .start_file("llama-server.exe", SimpleFileOptions::default())
                    .expect("launcher");
                writer.write_all(b"server").expect("launcher bytes");
            });
            let positions: Vec<_> = bytes
                .windows(placeholder.len())
                .enumerate()
                .filter_map(|(offset, part)| (part == placeholder.as_bytes()).then_some(offset))
                .collect();
            assert_eq!(positions.len(), 2, "local and central names");
            for offset in positions {
                bytes[offset..offset + name.len()].copy_from_slice(name.as_bytes());
            }
            std::fs::write(&path, &bytes).expect("literal unsafe names");
            let mut fixture = ZipArchive::new(Cursor::new(&bytes)).expect("raw archive fixture");
            assert_eq!(
                fixture
                    .by_index(0)
                    .expect("raw entry")
                    .name()
                    .expect("name"),
                name
            );
            let offer = fixture_offer(&bytes, b"server");
            assert!(
                extract_runtime_session(&path, &offer, directory.path()).is_err(),
                "accepted {name}"
            );
        }
        let (path, bytes) = write_archive(&directory, "too-many.zip", |writer| {
            for index in 0..=MAX_RUNTIME_ENTRIES {
                writer
                    .start_file(format!("entry-{index}"), SimpleFileOptions::default())
                    .expect("entry");
            }
        });
        assert!(extract_runtime_session(
            &path,
            &fixture_offer(&bytes, b"server"),
            directory.path()
        )
        .is_err());
        assert!(!directory.path().join("escape.txt").exists());
        assert!(!std::fs::read_dir(directory.path())
            .expect("fixture entries")
            .any(|entry| entry
                .expect("entry")
                .file_name()
                .to_string_lossy()
                .starts_with(".argentum-runtime-")));
    }

    #[test]
    fn archive_rejects_crc_malformed_and_oversized_metadata() {
        let directory = tempfile::tempdir().expect("fixture directory");
        let executable = b"verified server";
        let (path, mut bytes) = write_archive(&directory, "crc.zip", |writer| {
            writer
                .start_file(
                    "llama-server.exe",
                    SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored),
                )
                .expect("launcher");
            writer.write_all(executable).expect("launcher bytes");
        });
        let start = bytes
            .windows(executable.len())
            .position(|part| part == executable)
            .expect("stored payload");
        bytes[start] = b'V';
        std::fs::write(&path, &bytes).expect("corrupt fixture");
        // Both host digests match the changed data; only the stored CRC is stale.
        assert!(
            inspect_runtime_archive(&path, &fixture_offer(&bytes, b"Verified server")).is_err()
        );

        let malformed = b"not a ZIP archive";
        std::fs::write(&path, malformed).expect("malformed fixture");
        assert!(inspect_runtime_archive(&path, &fixture_offer(malformed, executable)).is_err());

        let (path, mut bytes) = write_archive(&directory, "size.zip", |writer| {
            writer
                .start_file("llama-server.exe", SimpleFileOptions::default())
                .expect("launcher");
            writer.write_all(executable).expect("launcher bytes");
        });
        let central = bytes
            .windows(4)
            .position(|part| part == b"PK\x01\x02")
            .expect("central header");
        let oversized = u32::try_from(MAX_RUNTIME_FILE_BYTES + 1)
            .expect("fixture size")
            .to_le_bytes();
        bytes[central + 24..central + 28].copy_from_slice(&oversized);
        std::fs::write(&path, &bytes).expect("oversized metadata");
        assert!(inspect_runtime_archive(&path, &fixture_offer(&bytes, executable)).is_err());

        let (path, mut bytes) = write_archive(&directory, "expanded.zip", |writer| {
            for name in ["llama-server.exe", "first.dll", "second.dll"] {
                writer
                    .start_file(name, SimpleFileOptions::default())
                    .expect("entry");
                writer.write_all(executable).expect("entry bytes");
            }
        });
        let headers: Vec<_> = bytes
            .windows(4)
            .enumerate()
            .filter_map(|(offset, part)| (part == b"PK\x01\x02").then_some(offset))
            .collect();
        assert_eq!(headers.len(), 3);
        for offset in &headers[1..] {
            bytes[offset + 24..offset + 28].copy_from_slice(
                &u32::try_from(MAX_RUNTIME_FILE_BYTES)
                    .expect("file limit")
                    .to_le_bytes(),
            );
        }
        // Individual declared sizes fit, but their sum plus the launcher does not.
        let mut fixture = ZipArchive::new(Cursor::new(&bytes)).expect("size fixture");
        for index in 1..3 {
            assert_eq!(
                fixture.by_index(index).expect("size entry").size(),
                MAX_RUNTIME_FILE_BYTES
            );
        }
        std::fs::write(&path, &bytes).expect("expanded metadata");
        assert!(inspect_runtime_archive(&path, &fixture_offer(&bytes, executable)).is_err());
    }

    #[test]
    fn bundle_path_is_versioned_and_never_uses_offer_input_as_a_path() {
        let root = Path::new(r"C:\Argentum\runtimes");
        let offer = local_runtime_offers().first().expect("runtime offer");

        assert_eq!(
            local_runtime_bundle_path(root, offer).expect("bundle path"),
            root.join("llama-cpp")
                .join("v0.3.0")
                .join("vulkan-x64")
                .join("bundle.zip")
        );
    }

    #[tokio::test]
    async fn download_deadline_runtime_stall_returns_typed_error_and_cleans_part_file() {
        let bytes = b"ok";
        let offer = fixture_offer(bytes, b"server");
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
            Ok(Err(ProviderError::LocalRuntimeDownloadDeadline {
                timeout_ms: 250
            }))
        ));
        assert_no_part_file(&directory);
        server.finish();
    }

    #[tokio::test]
    async fn download_deadline_runtime_slow_drip_cannot_outlive_total_deadline() {
        let bytes = b"abc";
        let offer = fixture_offer(bytes, b"server");
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
            Ok(Err(ProviderError::LocalRuntimeDownloadDeadline {
                timeout_ms: 250
            }))
        ));
        assert_no_part_file(&directory);
        server.finish();
    }

    #[tokio::test]
    async fn download_runtime_bad_redirect_cleans_part_file() {
        let offer = fixture_offer(b"ok", b"server");
        let directory = tempfile::tempdir().expect("fixture directory");
        let server = spawn_raw_response_server(
            "HTTP/1.1 302 Found\r\nLocation: http://example.invalid/fixture.zip\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".into(),
        );

        let result = fixture_download(offer, &directory, &server.endpoint).await;

        assert!(matches!(
            result,
            Err(ProviderError::LocalRuntimeDownloadFailed)
        ));
        assert_no_part_file(&directory);
        server.finish();
    }

    #[tokio::test]
    async fn download_runtime_wrong_size_cleans_part_file() {
        let offer = fixture_offer(b"ab", b"server");
        let directory = tempfile::tempdir().expect("fixture directory");
        let target = directory.path().join("bundle.zip");
        let mut server = spawn_paused_chunked_server();
        let endpoint = server.endpoint.clone();
        let download = tokio::spawn(async move {
            download_runtime_bundle_with_policy(
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
            Err(ProviderError::LocalRuntimeDownloadTooLarge { limit_bytes: 2 })
        ));
        assert_no_part_file(&directory);
        server.finish();
    }

    #[tokio::test]
    async fn download_runtime_wrong_hash_cleans_part_file() {
        let offer = fixture_offer(b"ok", b"server");
        let directory = tempfile::tempdir().expect("fixture directory");
        let server = spawn_download_server(vec![b"no".to_vec()], Duration::ZERO);

        let result = fixture_download(offer, &directory, &server.endpoint).await;

        assert!(matches!(
            result,
            Err(ProviderError::LocalRuntimeIntegrityMismatch)
        ));
        assert_no_part_file(&directory);
        server.finish();
    }

    #[tokio::test]
    async fn download_runtime_target_created_during_download_is_preserved_and_cleans_part_file() {
        let bytes = b"ok";
        let offer = fixture_offer(bytes, b"server");
        let directory = tempfile::tempdir().expect("fixture directory");
        let target = directory.path().join("bundle.zip");
        let mut server = spawn_download_server(
            vec![bytes[..1].to_vec(), bytes[1..].to_vec()],
            Duration::from_millis(50),
        );
        let endpoint = server.endpoint.clone();
        let download = tokio::spawn(async move {
            download_runtime_bundle_with_policy(
                offer,
                target,
                Url::parse(&endpoint).expect("fixture download URL"),
                fixture_policy(),
                fixture_download_url,
            )
            .await
        });

        server.wait_for_first_chunk().await;
        let target = directory.path().join("bundle.zip");
        std::fs::write(&target, b"concurrent target").expect("concurrent target bytes");

        let result = download.await.expect("download task");

        assert!(matches!(
            result,
            Err(ProviderError::LocalRuntimeBundleInvalid)
        ));
        assert_eq!(
            std::fs::read(&target).expect("concurrent target bytes"),
            b"concurrent target"
        );
        assert_no_part_file(&directory);
        server.finish();
    }

    #[tokio::test]
    async fn download_runtime_valid_existing_target_skips_network() {
        let directory = tempfile::tempdir().expect("fixture directory");
        let executable = b"verified server";
        let (_, archive) = write_archive(&directory, "fixture.zip", |writer| {
            writer
                .start_file("llama-server.exe", SimpleFileOptions::default())
                .expect("server entry");
            writer.write_all(executable).expect("server bytes");
        });
        let offer = fixture_offer(&archive, executable);
        let target = directory.path().join("bundle.zip");
        std::fs::write(&target, &archive).expect("verified target bytes");
        let server = spawn_no_request_server();

        let result = fixture_download(offer, &directory, &server.endpoint).await;

        assert_eq!(result.expect("valid existing target"), target);
        #[cfg(windows)]
        {
            let evidence = argentum_execution::owner_only_acl_evidence(&target)
                .expect("owner-only final archive ACL evidence");
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
    async fn download_runtime_atomic_publication_preserves_target_created_in_final_gap() {
        let directory = tempfile::tempdir().expect("fixture directory");
        let (offer, archive) = valid_fixture_archive(&directory);
        let target = directory.path().join("bundle.zip");
        let gate = PublicationGate::new();
        let mut policy = fixture_policy();
        policy.hooks.publication_gate = Some(gate.clone());
        let server = spawn_download_server(vec![archive], Duration::ZERO);
        let endpoint = server.endpoint.clone();
        let download = tokio::spawn({
            let target = target.clone();
            async move {
                download_runtime_bundle_with_policy(
                    offer,
                    target,
                    Url::parse(&endpoint).expect("fixture download URL"),
                    policy,
                    fixture_download_url,
                )
                .await
            }
        });

        tokio::time::timeout(Duration::from_secs(1), gate.wait_until_reached())
            .await
            .expect("download reached the pre-publication barrier");
        std::fs::write(&target, b"created in final gap").expect("concurrent target bytes");
        gate.release().await;

        let result = download.await.expect("download task");

        assert!(matches!(
            result,
            Err(ProviderError::LocalRuntimeBundleInvalid)
        ));
        assert_eq!(
            std::fs::read(&target).expect("concurrent target bytes"),
            b"created in final gap"
        );
        assert_no_part_file(&directory);
        server.finish();
    }

    #[tokio::test]
    async fn download_runtime_deadline_before_publication_never_publishes_after_return() {
        let directory = tempfile::tempdir().expect("fixture directory");
        let (offer, archive) = valid_fixture_archive(&directory);
        let target = directory.path().join("bundle.zip");
        let gate = PublicationGate::new();
        let mut policy = fixture_policy();
        policy.total_timeout = Duration::from_millis(100);
        policy.hooks.publication_gate = Some(gate.clone());
        let server = spawn_download_server(vec![archive], Duration::ZERO);
        let endpoint = server.endpoint.clone();
        let download = tokio::spawn({
            let target = target.clone();
            async move {
                download_runtime_bundle_with_policy(
                    offer,
                    target,
                    Url::parse(&endpoint).expect("fixture download URL"),
                    policy,
                    fixture_download_url,
                )
                .await
            }
        });

        tokio::time::timeout(Duration::from_secs(1), gate.wait_until_reached())
            .await
            .expect("download reached the pre-publication barrier");
        let result = download.await.expect("deadline download task");

        assert!(matches!(
            result,
            Err(ProviderError::LocalRuntimeDownloadDeadline { timeout_ms: 100 })
        ));
        assert!(!target.exists(), "deadline returned after publication");
        tokio::time::sleep(Duration::from_millis(50)).await;
        assert!(
            !target.exists(),
            "publication continued after deadline result"
        );
        assert_no_part_file(&directory);
        server.finish();
    }

    #[tokio::test(flavor = "current_thread")]
    async fn runtime_existing_target_inspection_obeys_deadline_without_blocking_executor() {
        let directory = tempfile::tempdir().expect("fixture directory");
        let (offer, archive) = valid_fixture_archive(&directory);
        let target = directory.path().join("bundle.zip");
        std::fs::write(&target, &archive).expect("existing verified archive");
        let (gate, inspection) = InspectionGate::new();
        let InspectionGateControl {
            entered,
            release: _release,
        } = inspection;
        let mut policy = fixture_policy();
        policy.hooks.existing_inspection_gate = Some(gate);
        let server = spawn_no_request_server();
        let endpoint = server.endpoint.clone();
        let download = tokio::spawn({
            let target = target.clone();
            async move {
                download_runtime_bundle_with_policy(
                    offer,
                    target,
                    Url::parse(&endpoint).expect("fixture download URL"),
                    policy,
                    fixture_download_url,
                )
                .await
            }
        });

        tokio::time::timeout(Duration::from_secs(1), entered)
            .await
            .expect("existing inspection entered the worker")
            .expect("existing inspection entry signal");
        let result = download.await.expect("existing inspection task");

        assert!(matches!(
            result,
            Err(ProviderError::LocalRuntimeDownloadDeadline { timeout_ms: 250 })
        ));
        assert_eq!(
            std::fs::read(&target).expect("existing verified archive"),
            archive
        );
        assert_no_part_file(&directory);
        assert!(!server.finish(), "existing target made a network request");
    }

    #[tokio::test(flavor = "current_thread")]
    async fn runtime_final_inspection_obeys_deadline_without_later_publication() {
        let directory = tempfile::tempdir().expect("fixture directory");
        let (offer, archive) = valid_fixture_archive(&directory);
        let target = directory.path().join("bundle.zip");
        let (gate, inspection) = InspectionGate::new();
        let InspectionGateControl {
            entered,
            release: _release,
        } = inspection;
        let mut policy = fixture_policy();
        policy.hooks.final_inspection_gate = Some(gate);
        let server = spawn_download_server(vec![archive], Duration::ZERO);
        let endpoint = server.endpoint.clone();
        let download = tokio::spawn({
            let target = target.clone();
            async move {
                download_runtime_bundle_with_policy(
                    offer,
                    target,
                    Url::parse(&endpoint).expect("fixture download URL"),
                    policy,
                    fixture_download_url,
                )
                .await
            }
        });

        tokio::time::timeout(Duration::from_secs(1), entered)
            .await
            .expect("final inspection entered the worker")
            .expect("final inspection entry signal");
        let (progress_sender, progress) = tokio::sync::oneshot::channel();
        tokio::spawn(async move {
            tokio::task::yield_now().await;
            let _ = progress_sender.send(());
        });
        tokio::time::timeout(Duration::from_millis(100), progress)
            .await
            .expect("Tokio executor remained responsive")
            .expect("Tokio progress signal");
        let result = download.await.expect("final inspection task");

        assert!(matches!(
            result,
            Err(ProviderError::LocalRuntimeDownloadDeadline { timeout_ms: 250 })
        ));
        assert!(!target.exists(), "deadline returned after publication");
        tokio::time::sleep(Duration::from_millis(50)).await;
        assert!(
            !target.exists(),
            "publication continued after deadline result"
        );
        assert_no_part_file(&directory);
        server.finish();
    }
}
