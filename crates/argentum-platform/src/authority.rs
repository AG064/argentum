use std::collections::BTreeSet;
use std::fmt;
use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, RwLock};
use std::time::Instant;

use argentum_domain::{
    is_valid_capability_id, AuthorityProjectionError, PlatformAuthoritySnapshot,
    PlatformAuthorityState, PlatformAuthorityTier, PlatformCapabilityProbeSnapshot,
};
use argentum_lifecycle::{
    EffectDisposer, EffectFailure, LifecycleError, LifecycleKernel, ModuleHandle, ModuleSpec,
    MountFuture,
};
use argentum_security::AuthorityGeneration;
use async_trait::async_trait;
use tokio::sync::Mutex;
use tokio_util::sync::CancellationToken;

#[cfg(target_os = "android")]
use crate::android_authority::AndroidStandardAuthorityBridge;
use crate::PlatformError;

const AUTHORITY_MODULE_ID: &str = "platform.authority.standard";
#[cfg(any(windows, target_os = "android", test, feature = "test-support"))]
const AUTHORITY_EFFECT_LABEL: &str = "platform-authority-lease";
const DEPENDENT_EFFECT_LABEL: &str = "authority-dependent-service";
#[cfg(any(windows, target_os = "android", test, feature = "test-support"))]
const ACTIVE_DETAIL: &str = "Windows current-user authority is active";
#[cfg(any(windows, target_os = "android", test, feature = "test-support"))]
const ANDROID_ACTIVE_DETAIL: &str = "Android app-UID Standard authority is active";
const REVOKED_DETAIL: &str = "Platform authority has been revoked";
const RECOVERY_DETAIL: &str = "Platform authority cleanup is pending";
#[cfg(not(any(windows, target_os = "android")))]
const UNSUPPORTED_DETAIL: &str = "Platform authority is not implemented for this target";

pub type PlatformAuthorityFuture<'a> =
    Pin<Box<dyn Future<Output = Result<(), PlatformError>> + Send + 'a>>;

pub(crate) mod sealed {
    pub trait Sealed {}
}

pub trait PlatformAuthorityBridge: sealed::Sealed + Send + Sync {
    fn platform(&self) -> &'static str;
    fn bridge_id(&self) -> &'static str;
    fn tier(&self) -> PlatformAuthorityTier;
    fn probe(&self) -> Result<PlatformCapabilityProbe, PlatformError>;
    fn release<'a>(
        &'a mut self,
        cancellation: &'a CancellationToken,
    ) -> PlatformAuthorityFuture<'a>;
}

#[cfg(windows)]
#[derive(Debug, Default)]
pub struct WindowsCurrentUserBridge {
    released: bool,
}

#[cfg(windows)]
impl sealed::Sealed for WindowsCurrentUserBridge {}

#[cfg(windows)]
impl PlatformAuthorityBridge for WindowsCurrentUserBridge {
    fn platform(&self) -> &'static str {
        "windows"
    }

    fn bridge_id(&self) -> &'static str {
        "windows.current-user"
    }

    fn tier(&self) -> PlatformAuthorityTier {
        PlatformAuthorityTier::Standard
    }

    fn probe(&self) -> Result<PlatformCapabilityProbe, PlatformError> {
        PlatformCapabilityProbe::new(
            None,
            Some(windows_current_user_sid()?),
            None,
            [
                "filesystem.user",
                "network.user",
                "process.user",
                "windows.job-object",
                "windows.local-runtime",
            ],
        )
        .map_err(|_| PlatformError::InvalidAuthorityProbe)
    }

    fn release<'a>(
        &'a mut self,
        cancellation: &'a CancellationToken,
    ) -> PlatformAuthorityFuture<'a> {
        Box::pin(async move {
            if cancellation.is_cancelled() {
                return Err(PlatformError::AuthorityReleaseCancelled);
            }
            if self.released {
                return Ok(());
            }
            self.released = true;
            Ok(())
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlatformCapabilityProbe {
    snapshot: PlatformCapabilityProbeSnapshot,
}

impl PlatformCapabilityProbe {
    pub fn new<I, S>(
        observed_uid: Option<u32>,
        observed_identity: Option<String>,
        selinux_context: Option<String>,
        capabilities: I,
    ) -> Result<Self, AuthorityProjectionError>
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        let snapshot = PlatformCapabilityProbeSnapshot::new(
            observed_uid,
            observed_identity.as_deref(),
            selinux_context.as_deref(),
            capabilities,
        )?;
        Ok(Self { snapshot })
    }

    pub fn observed_uid(&self) -> Option<u32> {
        self.snapshot.observed_uid()
    }

    pub fn observed_identity(&self) -> Option<&str> {
        self.snapshot.observed_identity()
    }

    pub fn selinux_context(&self) -> Option<&str> {
        self.snapshot.selinux_context()
    }

    pub fn capabilities(&self) -> &[String] {
        self.snapshot.capabilities()
    }

    pub fn snapshot(&self) -> PlatformCapabilityProbeSnapshot {
        self.snapshot.clone()
    }
}

#[derive(Debug, Clone, Copy)]
enum ExpiryPolicy {
    #[cfg(any(windows, target_os = "android", test, feature = "test-support"))]
    ProcessLifetime,
    #[allow(dead_code)]
    Deadline(Instant),
}

pub struct PlatformAuthorityLease {
    _lease_id: [u8; 16],
    _bridge_id: String,
    tier: PlatformAuthorityTier,
    probe: PlatformCapabilityProbe,
    generation: AuthorityGeneration,
    _issued_at: Instant,
    expiry: ExpiryPolicy,
    revocation: CancellationToken,
}

impl PlatformAuthorityLease {
    #[cfg(any(windows, target_os = "android", test, feature = "test-support"))]
    fn new(
        bridge_id: &str,
        tier: PlatformAuthorityTier,
        probe: PlatformCapabilityProbe,
        generation: AuthorityGeneration,
        expiry: ExpiryPolicy,
    ) -> Result<Self, PlatformError> {
        if !is_valid_capability_id(bridge_id) {
            return Err(PlatformError::InvalidAuthorityProbe);
        }
        let mut lease_id = [0_u8; 16];
        getrandom::fill(&mut lease_id)
            .map_err(|_| PlatformError::AuthorityIdentifierUnavailable)?;
        Ok(Self {
            _lease_id: lease_id,
            _bridge_id: bridge_id.into(),
            tier,
            probe,
            generation,
            _issued_at: Instant::now(),
            expiry,
            revocation: CancellationToken::new(),
        })
    }

    pub const fn tier(&self) -> PlatformAuthorityTier {
        self.tier
    }

    pub const fn generation_ordinal(&self) -> u64 {
        self.generation.ordinal()
    }

    pub fn matches_generation(&self, generation: &AuthorityGeneration) -> bool {
        self.generation == *generation
    }

    pub fn probe_snapshot(&self) -> PlatformCapabilityProbeSnapshot {
        self.probe.snapshot()
    }

    pub fn is_current(&self) -> bool {
        if self.revocation.is_cancelled() {
            return false;
        }
        match self.expiry {
            #[cfg(any(windows, target_os = "android", test, feature = "test-support"))]
            ExpiryPolicy::ProcessLifetime => true,
            ExpiryPolicy::Deadline(deadline) => Instant::now() < deadline,
        }
    }

    fn revoke(&self) {
        self.revocation.cancel();
    }
}

impl fmt::Debug for PlatformAuthorityLease {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("PlatformAuthorityLease")
            .field("tier", &self.tier)
            .field("generation", &self.generation.ordinal())
            .field("probe", &self.probe.snapshot())
            .field("current", &self.is_current())
            .finish()
    }
}

#[async_trait]
pub trait AuthorityDependentService: Send + 'static {
    fn service_id(&self) -> &str;

    async fn retire(&mut self, cancellation: &CancellationToken) -> Result<(), PlatformError>;
}

#[cfg(any(windows, target_os = "android", test, feature = "test-support"))]
struct BridgeDisposer {
    bridge: Box<dyn PlatformAuthorityBridge>,
}

#[cfg(any(windows, target_os = "android", test, feature = "test-support"))]
#[async_trait]
impl EffectDisposer for BridgeDisposer {
    async fn dispose(&mut self, cancellation: &CancellationToken) -> Result<(), EffectFailure> {
        self.bridge
            .release(cancellation)
            .await
            .map_err(|_| EffectFailure::new("platform authority release failed"))
    }
}

struct DependentDisposer {
    service: Box<dyn AuthorityDependentService>,
}

#[async_trait]
impl EffectDisposer for DependentDisposer {
    async fn dispose(&mut self, cancellation: &CancellationToken) -> Result<(), EffectFailure> {
        self.service
            .retire(cancellation)
            .await
            .map_err(|_| EffectFailure::new("authority dependent cleanup failed"))
    }
}

struct BrokerLifecycle {
    kernel: LifecycleKernel,
    authority_module: Option<ModuleHandle>,
    dependents: BTreeSet<String>,
}

struct BrokerInner {
    admission: AtomicBool,
    retirement_in_progress: AtomicBool,
    authority_operation: Arc<Mutex<()>>,
    generation: AuthorityGeneration,
    lease: Option<Arc<PlatformAuthorityLease>>,
    lifecycle: Mutex<BrokerLifecycle>,
    snapshot: RwLock<PlatformAuthoritySnapshot>,
}

struct RetirementOwner<'a> {
    in_progress: &'a AtomicBool,
}

impl Drop for RetirementOwner<'_> {
    fn drop(&mut self) {
        self.in_progress.store(false, Ordering::SeqCst);
    }
}

impl BrokerInner {
    fn try_begin_retirement(&self) -> Option<RetirementOwner<'_>> {
        self.retirement_in_progress
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .ok()
            .map(|_| RetirementOwner {
                in_progress: &self.retirement_in_progress,
            })
    }
}

#[derive(Clone)]
pub struct PlatformAuthorityBroker {
    inner: Arc<BrokerInner>,
}

impl PlatformAuthorityBroker {
    pub async fn bootstrap(initial_generation: AuthorityGeneration) -> Result<Self, PlatformError> {
        if initial_generation.ordinal() != 1 {
            return Err(PlatformError::InvalidInitialAuthorityGeneration);
        }

        #[cfg(windows)]
        {
            Self::bootstrap_bridge(
                initial_generation,
                Box::new(WindowsCurrentUserBridge::default()),
            )
            .await
        }

        #[cfg(target_os = "android")]
        {
            Self::bootstrap_bridge(
                initial_generation,
                Box::new(AndroidStandardAuthorityBridge::current_process()?),
            )
            .await
        }

        #[cfg(not(any(windows, target_os = "android")))]
        {
            Self::bootstrap_unavailable(initial_generation)
        }
    }

    #[cfg(not(any(windows, target_os = "android")))]
    fn bootstrap_unavailable(
        initial_generation: AuthorityGeneration,
    ) -> Result<Self, PlatformError> {
        let snapshot = PlatformAuthoritySnapshot::new(
            std::env::consts::OS,
            PlatformAuthorityTier::Standard,
            None,
            PlatformAuthorityState::Unavailable,
            0,
            None,
            UNSUPPORTED_DETAIL,
        )
        .map_err(|_| PlatformError::InvalidAuthorityProbe)?;
        Ok(Self {
            inner: Arc::new(BrokerInner {
                admission: AtomicBool::new(true),
                retirement_in_progress: AtomicBool::new(false),
                authority_operation: Arc::new(Mutex::new(())),
                generation: initial_generation,
                lease: None,
                lifecycle: Mutex::new(BrokerLifecycle {
                    kernel: LifecycleKernel::new(),
                    authority_module: None,
                    dependents: BTreeSet::new(),
                }),
                snapshot: RwLock::new(snapshot),
            }),
        })
    }

    #[cfg(test)]
    async fn bootstrap_with_bridge<B>(
        initial_generation: AuthorityGeneration,
        bridge: B,
    ) -> Result<Self, PlatformError>
    where
        B: PlatformAuthorityBridge + 'static,
    {
        Self::bootstrap_bridge(initial_generation, Box::new(bridge)).await
    }

    #[cfg(feature = "test-support")]
    pub async fn bootstrap_recording_test_bridge(
        initial_generation: AuthorityGeneration,
        events: Arc<std::sync::Mutex<Vec<&'static str>>>,
        release_calls: Arc<std::sync::atomic::AtomicUsize>,
    ) -> Result<Self, PlatformError> {
        Self::bootstrap_bridge(
            initial_generation,
            Box::new(RecordingTestBridge {
                events,
                release_calls,
                released: false,
            }),
        )
        .await
    }

    #[cfg(any(windows, target_os = "android", test, feature = "test-support"))]
    async fn bootstrap_bridge(
        initial_generation: AuthorityGeneration,
        bridge: Box<dyn PlatformAuthorityBridge>,
    ) -> Result<Self, PlatformError> {
        if initial_generation.ordinal() != 1 {
            return Err(PlatformError::InvalidInitialAuthorityGeneration);
        }
        let platform = bridge.platform();
        let bridge_id = bridge.bridge_id();
        let tier = bridge.tier();
        let probe = bridge.probe()?;
        let lease = Arc::new(PlatformAuthorityLease::new(
            bridge_id,
            tier,
            probe.clone(),
            initial_generation.clone(),
            ExpiryPolicy::ProcessLifetime,
        )?);
        let mut kernel = LifecycleKernel::new();
        let binding = kernel.binding();
        let mounted = kernel
            .mount(
                binding,
                ModuleSpec::new(AUTHORITY_MODULE_ID),
                &CancellationToken::new(),
                move |context| -> MountFuture<'_> {
                    Box::pin(async move {
                        context
                            .apply_effect(AUTHORITY_EFFECT_LABEL, move || async move {
                                Ok(BridgeDisposer { bridge })
                            })
                            .await?;
                        Ok(())
                    })
                },
            )
            .await
            .map_err(|_| PlatformError::AuthorityLifecycleFailed)?;
        let snapshot = active_snapshot(
            platform,
            tier,
            initial_generation.ordinal(),
            probe.snapshot(),
        )?;
        Ok(Self {
            inner: Arc::new(BrokerInner {
                admission: AtomicBool::new(true),
                retirement_in_progress: AtomicBool::new(false),
                authority_operation: Arc::new(Mutex::new(())),
                generation: initial_generation,
                lease: Some(lease),
                lifecycle: Mutex::new(BrokerLifecycle {
                    kernel,
                    authority_module: Some(mounted.module().clone()),
                    dependents: BTreeSet::new(),
                }),
                snapshot: RwLock::new(snapshot),
            }),
        })
    }

    pub fn snapshot(&self) -> PlatformAuthoritySnapshot {
        match self.inner.snapshot.read() {
            Ok(snapshot) => snapshot.clone(),
            Err(poisoned) => poisoned.into_inner().clone(),
        }
    }

    #[cfg(any(test, feature = "test-support"))]
    pub async fn lifecycle_snapshot(&self) -> argentum_lifecycle::LifecycleSnapshot {
        self.inner.lifecycle.lock().await.kernel.snapshot()
    }

    pub fn current_generation(&self) -> Option<AuthorityGeneration> {
        self.inner
            .admission
            .load(Ordering::SeqCst)
            .then(|| self.inner.generation.clone())
    }

    pub fn current_lease(&self) -> Option<Arc<PlatformAuthorityLease>> {
        if !self.inner.admission.load(Ordering::SeqCst) {
            return None;
        }
        self.inner
            .lease
            .as_ref()
            .filter(|lease| lease.is_current())
            .cloned()
    }

    pub fn matches_current_authority(
        &self,
        generation: &AuthorityGeneration,
        lease: &Arc<PlatformAuthorityLease>,
    ) -> bool {
        self.inner.admission.load(Ordering::SeqCst)
            && self.inner.generation == *generation
            && lease.is_current()
            && lease.matches_generation(generation)
            && self
                .inner
                .lease
                .as_ref()
                .is_some_and(|current| Arc::ptr_eq(current, lease))
    }

    pub async fn acquire_current_authority(
        &self,
    ) -> Result<
        (
            tokio::sync::OwnedMutexGuard<()>,
            AuthorityGeneration,
            Arc<PlatformAuthorityLease>,
        ),
        PlatformError,
    > {
        if !self.inner.admission.load(Ordering::SeqCst) {
            return Err(PlatformError::AuthorityAdmissionClosed);
        }
        let operation = Arc::clone(&self.inner.authority_operation)
            .lock_owned()
            .await;
        if !self.inner.admission.load(Ordering::SeqCst) {
            return Err(PlatformError::AuthorityAdmissionClosed);
        }
        let generation = self.inner.generation.clone();
        let lease = self
            .inner
            .lease
            .as_ref()
            .cloned()
            .ok_or(PlatformError::AuthorityUnavailable)?;
        if !self.matches_current_authority(&generation, &lease) {
            return Err(PlatformError::AuthorityUnavailable);
        }
        Ok((operation, generation, lease))
    }

    pub async fn mount_dependent<S>(
        &self,
        id: &str,
        service: S,
        cancellation: &CancellationToken,
    ) -> Result<(), PlatformError>
    where
        S: AuthorityDependentService,
    {
        if !is_valid_capability_id(id) || !is_valid_capability_id(service.service_id()) {
            return Err(PlatformError::InvalidAuthorityServiceId);
        }
        if id != service.service_id() {
            return Err(PlatformError::AuthorityDependentIdMismatch);
        }
        let (_operation, generation, lease) = self.acquire_current_authority().await?;
        let mut lifecycle = self.inner.lifecycle.lock().await;
        if !self.matches_current_authority(&generation, &lease) {
            return Err(PlatformError::AuthorityUnavailable);
        }
        if lifecycle.dependents.contains(id) {
            return Err(PlatformError::DuplicateAuthorityDependent);
        }
        let service: Box<dyn AuthorityDependentService> = Box::new(service);
        let binding = lifecycle.kernel.binding();
        lifecycle
            .kernel
            .mount(
                binding,
                ModuleSpec::with_dependencies(id, [AUTHORITY_MODULE_ID]),
                cancellation,
                move |context| -> MountFuture<'_> {
                    Box::pin(async move {
                        context
                            .apply_effect(DEPENDENT_EFFECT_LABEL, move || async move {
                                Ok(DependentDisposer { service })
                            })
                            .await?;
                        Ok(())
                    })
                },
            )
            .await
            .map_err(|_| PlatformError::AuthorityLifecycleFailed)?;
        lifecycle.dependents.insert(id.into());
        Ok(())
    }

    pub async fn retire(&self, cancellation: &CancellationToken) -> Result<(), PlatformError> {
        let Some(_owner) = self.inner.try_begin_retirement() else {
            return Ok(());
        };
        if self.snapshot().state() == PlatformAuthorityState::Revoked {
            return Ok(());
        }
        let _operation = Arc::clone(&self.inner.authority_operation)
            .lock_owned()
            .await;
        let platform = self.snapshot().platform().to_owned();
        let pending_snapshot = self
            .inner
            .lease
            .as_ref()
            .map(|_| recovery_snapshot(&platform, self.inner.generation.ordinal()))
            .transpose()?;
        self.inner.admission.swap(false, Ordering::SeqCst);
        if let Some(snapshot) = pending_snapshot {
            self.replace_snapshot(snapshot);
        }
        if let Some(lease) = &self.inner.lease {
            lease.revoke();
        }

        let mut lifecycle = self.inner.lifecycle.lock().await;
        let Some(authority_module) = lifecycle.authority_module.clone() else {
            return Ok(());
        };
        let binding = lifecycle.kernel.binding();
        match lifecycle
            .kernel
            .retire_tree(binding, &authority_module, cancellation)
            .await
        {
            Ok(_) => {
                lifecycle.authority_module = None;
                lifecycle.dependents.clear();
                self.replace_snapshot(revoked_snapshot(
                    &platform,
                    self.inner.generation.ordinal(),
                )?);
                Ok(())
            }
            Err(error) => {
                self.replace_snapshot(recovery_snapshot(
                    &platform,
                    self.inner.generation.ordinal(),
                )?);
                Err(map_retirement_error(error))
            }
        }
    }

    fn replace_snapshot(&self, replacement: PlatformAuthoritySnapshot) {
        match self.inner.snapshot.write() {
            Ok(mut snapshot) => *snapshot = replacement,
            Err(poisoned) => *poisoned.into_inner() = replacement,
        }
    }
}

#[cfg(feature = "test-support")]
struct RecordingTestBridge {
    events: Arc<std::sync::Mutex<Vec<&'static str>>>,
    release_calls: Arc<std::sync::atomic::AtomicUsize>,
    released: bool,
}

#[cfg(feature = "test-support")]
impl sealed::Sealed for RecordingTestBridge {}

#[cfg(feature = "test-support")]
impl PlatformAuthorityBridge for RecordingTestBridge {
    fn platform(&self) -> &'static str {
        "windows"
    }

    fn bridge_id(&self) -> &'static str {
        "windows.current-user.test-support"
    }

    fn tier(&self) -> PlatformAuthorityTier {
        PlatformAuthorityTier::Standard
    }

    fn probe(&self) -> Result<PlatformCapabilityProbe, PlatformError> {
        PlatformCapabilityProbe::new(
            None,
            Some("S-1-5-21-1000".into()),
            None,
            ["filesystem.user", "process.user", "windows.local-runtime"],
        )
        .map_err(|_| PlatformError::InvalidAuthorityProbe)
    }

    fn release<'a>(
        &'a mut self,
        cancellation: &'a CancellationToken,
    ) -> PlatformAuthorityFuture<'a> {
        Box::pin(async move {
            if cancellation.is_cancelled() {
                return Err(PlatformError::AuthorityReleaseCancelled);
            }
            if self.released {
                return Ok(());
            }
            self.release_calls.fetch_add(1, Ordering::SeqCst);
            self.events
                .lock()
                .map_err(|_| PlatformError::AuthorityReleaseFailed)?
                .push("platform.authority.standard");
            self.released = true;
            Ok(())
        })
    }
}

impl PlatformError {
    pub fn authority_dependent_cleanup(
        service_id: impl Into<String>,
    ) -> Result<Self, PlatformError> {
        let service_id = service_id.into();
        if !is_valid_capability_id(&service_id) {
            return Err(PlatformError::InvalidAuthorityServiceId);
        }
        Ok(PlatformError::AuthorityDependentCleanup { service_id })
    }
}

#[cfg(any(windows, target_os = "android", test, feature = "test-support"))]
fn active_snapshot(
    platform: &str,
    tier: PlatformAuthorityTier,
    generation: u64,
    probe: PlatformCapabilityProbeSnapshot,
) -> Result<PlatformAuthoritySnapshot, PlatformError> {
    let detail = if platform == "android" {
        ANDROID_ACTIVE_DETAIL
    } else {
        ACTIVE_DETAIL
    };
    PlatformAuthoritySnapshot::new(
        platform,
        PlatformAuthorityTier::Standard,
        Some(tier),
        PlatformAuthorityState::Active,
        generation,
        Some(probe),
        detail,
    )
    .map_err(|_| PlatformError::InvalidAuthorityProbe)
}

fn revoked_snapshot(
    platform: &str,
    generation: u64,
) -> Result<PlatformAuthoritySnapshot, PlatformError> {
    PlatformAuthoritySnapshot::new(
        platform,
        PlatformAuthorityTier::Standard,
        None,
        PlatformAuthorityState::Revoked,
        generation,
        None,
        REVOKED_DETAIL,
    )
    .map_err(|_| PlatformError::InvalidAuthorityProbe)
}

fn recovery_snapshot(
    platform: &str,
    generation: u64,
) -> Result<PlatformAuthoritySnapshot, PlatformError> {
    PlatformAuthoritySnapshot::new(
        platform,
        PlatformAuthorityTier::Standard,
        None,
        PlatformAuthorityState::RecoveryPending,
        generation,
        None,
        RECOVERY_DETAIL,
    )
    .map_err(|_| PlatformError::InvalidAuthorityProbe)
}

fn map_retirement_error(error: LifecycleError) -> PlatformError {
    match error {
        LifecycleError::TeardownPending { module_id, .. } if module_id == AUTHORITY_MODULE_ID => {
            PlatformError::AuthorityReleaseFailed
        }
        LifecycleError::TeardownPending { module_id, .. } => {
            PlatformError::authority_dependent_cleanup(module_id)
                .unwrap_or(PlatformError::AuthorityLifecycleFailed)
        }
        LifecycleError::TeardownCancelled { module_id } if module_id == AUTHORITY_MODULE_ID => {
            PlatformError::AuthorityReleaseCancelled
        }
        _ => PlatformError::AuthorityLifecycleFailed,
    }
}

#[cfg(windows)]
fn windows_current_user_sid() -> Result<String, PlatformError> {
    let token = open_current_process_token()?;
    query_token_user_sid(&token)
}

#[cfg(windows)]
fn open_current_process_token() -> Result<std::os::windows::io::OwnedHandle, PlatformError> {
    use std::os::windows::io::{FromRawHandle, OwnedHandle};
    use std::ptr::null_mut;

    use windows_sys::Win32::Security::TOKEN_QUERY;
    use windows_sys::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};

    let mut token = null_mut();
    // SAFETY: GetCurrentProcess returns the documented pseudo-handle, token points to writable
    // HANDLE storage, and the OpenProcessToken result and returned handle are both checked.
    let opened = unsafe { OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) };
    if opened == 0 || token.is_null() {
        return Err(PlatformError::AuthorityIdentityProbeFailed);
    }
    // SAFETY: OpenProcessToken succeeded with a non-null owned token handle, transferring its
    // sole close responsibility to OwnedHandle.
    Ok(unsafe { OwnedHandle::from_raw_handle(token) })
}

#[cfg(windows)]
fn query_token_user_sid(
    token: &std::os::windows::io::OwnedHandle,
) -> Result<String, PlatformError> {
    use std::mem::size_of;
    use std::os::windows::io::AsRawHandle;
    use std::ptr::null_mut;

    use windows_sys::Win32::Foundation::{GetLastError, ERROR_INSUFFICIENT_BUFFER, HANDLE};
    use windows_sys::Win32::Security::{GetTokenInformation, TokenUser, TOKEN_USER};

    let token_handle = token.as_raw_handle() as HANDLE;
    let mut required = 0_u32;
    // SAFETY: token_handle is owned and valid, the null buffer is paired with length zero, and
    // required points to writable length storage for the documented sizing call.
    let sizing =
        unsafe { GetTokenInformation(token_handle, TokenUser, null_mut(), 0, &mut required) };
    // SAFETY: GetLastError is read immediately after the checked sizing call failed.
    let sizing_error = unsafe { GetLastError() };
    if sizing != 0
        || sizing_error != ERROR_INSUFFICIENT_BUFFER
        || required < size_of::<TOKEN_USER>() as u32
    {
        return Err(PlatformError::AuthorityIdentityProbeFailed);
    }

    let required_bytes =
        usize::try_from(required).map_err(|_| PlatformError::AuthorityIdentityProbeFailed)?;
    let word_bytes = size_of::<usize>();
    let word_count = required_bytes
        .checked_add(word_bytes - 1)
        .ok_or(PlatformError::AuthorityIdentityProbeFailed)?
        / word_bytes;
    let mut storage = vec![0_usize; word_count];
    let mut returned = required;
    // SAFETY: storage is aligned for TOKEN_USER and spans at least required bytes, token_handle
    // remains valid, and returned points to writable length storage. The return is checked.
    let queried = unsafe {
        GetTokenInformation(
            token_handle,
            TokenUser,
            storage.as_mut_ptr().cast(),
            required,
            &mut returned,
        )
    };
    if queried == 0 || returned < size_of::<TOKEN_USER>() as u32 || returned > required {
        return Err(PlatformError::AuthorityIdentityProbeFailed);
    }
    // SAFETY: the successful checked query initialized an aligned TOKEN_USER at storage start,
    // and returned confirms that the complete structure is present.
    let token_user = unsafe { &*storage.as_ptr().cast::<TOKEN_USER>() };
    if token_user.User.Sid.is_null() {
        return Err(PlatformError::AuthorityIdentityProbeFailed);
    }
    sid_to_bounded_string(token_user.User.Sid)
}

#[cfg(windows)]
fn sid_to_bounded_string(sid: windows_sys::Win32::Security::PSID) -> Result<String, PlatformError> {
    use std::ptr::null_mut;

    use windows_sys::Win32::Foundation::LocalFree;
    use windows_sys::Win32::Security::Authorization::ConvertSidToStringSidW;

    const MAX_SID_CHARACTERS: usize = 256;

    let mut allocated = null_mut();
    // SAFETY: sid comes from the successful TOKEN_USER query and remains valid for this call,
    // allocated points to writable PWSTR storage, and the return and pointer are checked.
    let converted = unsafe { ConvertSidToStringSidW(sid, &mut allocated) };
    if converted == 0 || allocated.is_null() {
        return Err(PlatformError::AuthorityIdentityProbeFailed);
    }

    let mut units = Vec::new();
    // SAFETY: a successful ConvertSidToStringSidW returns a readable null-terminated allocation.
    // The loop reads through that terminator and enforces the public 256-character bound.
    unsafe {
        for index in 0..=MAX_SID_CHARACTERS {
            let unit = *allocated.add(index);
            if unit == 0 {
                break;
            }
            if index == MAX_SID_CHARACTERS {
                units.clear();
                break;
            }
            units.push(unit);
        }
    }
    let decoded = String::from_utf16(&units)
        .ok()
        .filter(|value| !value.is_empty() && value.len() <= MAX_SID_CHARACTERS);
    // SAFETY: allocated is the unchanged pointer returned by ConvertSidToStringSidW and has not
    // been freed. LocalFree's null return is checked as the documented success value.
    let free_result = unsafe { LocalFree(allocated.cast()) };
    if !free_result.is_null() {
        return Err(PlatformError::AuthorityIdentityProbeFailed);
    }
    decoded.ok_or(PlatformError::AuthorityIdentityProbeFailed)
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
    use std::sync::{Arc, Mutex};
    use std::time::{Duration, Instant};

    use argentum_domain::{PlatformAuthorityState, PlatformAuthorityTier};
    use argentum_security::host_authority_channel;
    use async_trait::async_trait;
    use tokio::sync::Notify;
    use tokio::time::timeout;
    use tokio_util::sync::CancellationToken;

    use super::PlatformCapabilityProbe;
    use super::{
        sealed, AuthorityDependentService, ExpiryPolicy, PlatformAuthorityBridge,
        PlatformAuthorityBroker, PlatformAuthorityFuture, PlatformAuthorityLease, PlatformError,
    };

    fn test_probe() -> PlatformCapabilityProbe {
        PlatformCapabilityProbe::new(
            None,
            Some("S-1-5-21-1000".into()),
            None,
            ["filesystem.user", "process.user", "windows.local-runtime"],
        )
        .expect("test probe")
    }

    struct FakeBridge {
        events: Arc<Mutex<Vec<String>>>,
        release_calls: Arc<AtomicUsize>,
        fail_release_once: Arc<AtomicBool>,
    }

    impl FakeBridge {
        fn new(
            events: Arc<Mutex<Vec<String>>>,
            release_calls: Arc<AtomicUsize>,
            fail_release_once: bool,
        ) -> Self {
            Self {
                events,
                release_calls,
                fail_release_once: Arc::new(AtomicBool::new(fail_release_once)),
            }
        }
    }

    impl sealed::Sealed for FakeBridge {}

    impl PlatformAuthorityBridge for FakeBridge {
        fn platform(&self) -> &'static str {
            "windows"
        }

        fn bridge_id(&self) -> &'static str {
            "windows.current-user.test"
        }

        fn tier(&self) -> PlatformAuthorityTier {
            PlatformAuthorityTier::Standard
        }

        fn probe(&self) -> Result<PlatformCapabilityProbe, PlatformError> {
            Ok(test_probe())
        }

        fn release<'a>(
            &'a mut self,
            cancellation: &'a CancellationToken,
        ) -> PlatformAuthorityFuture<'a> {
            Box::pin(async move {
                if cancellation.is_cancelled() {
                    return Err(PlatformError::AuthorityReleaseCancelled);
                }
                self.release_calls.fetch_add(1, Ordering::SeqCst);
                self.events
                    .lock()
                    .expect("event lock")
                    .push("platform.authority.standard".into());
                if self.fail_release_once.swap(false, Ordering::SeqCst) {
                    return Err(PlatformError::AuthorityReleaseFailed);
                }
                Ok(())
            })
        }
    }

    struct RecordingService {
        id: String,
        events: Arc<Mutex<Vec<String>>>,
        retire_calls: Arc<AtomicUsize>,
        fail_once: AtomicBool,
        lease: Option<Arc<PlatformAuthorityLease>>,
        started: Option<Arc<Notify>>,
        proceed: Option<Arc<Notify>>,
    }

    impl RecordingService {
        fn new(
            id: &str,
            events: Arc<Mutex<Vec<String>>>,
            retire_calls: Arc<AtomicUsize>,
            fail_once: bool,
            lease: Option<Arc<PlatformAuthorityLease>>,
        ) -> Self {
            Self {
                id: id.into(),
                events,
                retire_calls,
                fail_once: AtomicBool::new(fail_once),
                lease,
                started: None,
                proceed: None,
            }
        }

        fn with_gate(mut self, started: Arc<Notify>, proceed: Arc<Notify>) -> Self {
            self.started = Some(started);
            self.proceed = Some(proceed);
            self
        }
    }

    #[async_trait]
    impl AuthorityDependentService for RecordingService {
        fn service_id(&self) -> &str {
            &self.id
        }

        async fn retire(&mut self, _cancellation: &CancellationToken) -> Result<(), PlatformError> {
            self.retire_calls.fetch_add(1, Ordering::SeqCst);
            if let Some(lease) = &self.lease {
                assert!(!lease.is_current());
            }
            self.events
                .lock()
                .expect("event lock")
                .push(self.id.clone());
            if let Some(started) = &self.started {
                started.notify_one();
            }
            if let Some(proceed) = &self.proceed {
                proceed.notified().await;
            }
            if self.fail_once.swap(false, Ordering::SeqCst) {
                return Err(PlatformError::authority_dependent_cleanup(self.id.clone())
                    .expect("valid dependent ID"));
            }
            Ok(())
        }
    }

    struct ReentrantRetirementService {
        broker: PlatformAuthorityBroker,
        outcome: Arc<AtomicUsize>,
        retire_calls: Arc<AtomicUsize>,
    }

    #[async_trait]
    impl AuthorityDependentService for ReentrantRetirementService {
        fn service_id(&self) -> &str {
            "provider.reentrant"
        }

        async fn retire(&mut self, _cancellation: &CancellationToken) -> Result<(), PlatformError> {
            self.retire_calls.fetch_add(1, Ordering::SeqCst);
            let outcome = match timeout(
                Duration::from_millis(100),
                self.broker.retire(&CancellationToken::new()),
            )
            .await
            {
                Ok(Ok(())) => 1,
                Ok(Err(_)) => 2,
                Err(_) => 3,
            };
            self.outcome.store(outcome, Ordering::SeqCst);
            Ok(())
        }
    }

    #[test]
    fn platform_capability_probe_normalizes_and_bounds_public_values() {
        let probe = PlatformCapabilityProbe::new(
            None,
            Some("S-1-5-21-1000".into()),
            None,
            [
                "windows.local-runtime",
                "filesystem.user",
                "filesystem.user",
                "process.user",
            ],
        )
        .expect("bounded Windows probe");
        assert_eq!(
            probe.capabilities(),
            ["filesystem.user", "process.user", "windows.local-runtime",]
        );

        assert!(PlatformCapabilityProbe::new(None, None, None, ["INVALID CAPABILITY"]).is_err());
        assert!(PlatformCapabilityProbe::new(
            None,
            None,
            None,
            (0..65).map(|index| format!("capability.{index}")),
        )
        .is_err());
        assert!(PlatformCapabilityProbe::new(None, None, None, ["a".repeat(65)]).is_err());
        assert!(PlatformCapabilityProbe::new(
            None,
            Some("a".repeat(257)),
            None,
            std::iter::empty::<String>(),
        )
        .is_err());
        assert!(PlatformCapabilityProbe::new(
            None,
            None,
            Some("a".repeat(257)),
            std::iter::empty::<String>(),
        )
        .is_err());
    }

    #[tokio::test]
    async fn platform_lease_matches_the_full_opaque_generation_not_its_ordinal() {
        let (_, _, generation) = host_authority_channel().expect("authority channel");
        let (_, _, independent_generation) =
            host_authority_channel().expect("independent authority channel");
        assert_eq!(generation.ordinal(), independent_generation.ordinal());

        let events = Arc::new(Mutex::new(Vec::new()));
        let release_calls = Arc::new(AtomicUsize::new(0));
        let broker = PlatformAuthorityBroker::bootstrap_with_bridge(
            generation.clone(),
            FakeBridge::new(events, release_calls, false),
        )
        .await
        .expect("broker");
        let lease = broker.current_lease().expect("platform lease");

        assert!(lease.matches_generation(&generation));
        assert!(!lease.matches_generation(&independent_generation));
        assert!(broker.matches_current_authority(&generation, &lease));
        assert!(!broker.matches_current_authority(&independent_generation, &lease));
    }

    #[tokio::test]
    async fn broker_retries_failed_dependents_in_reverse_order_before_authority_release() {
        let (_, _, generation) = host_authority_channel().expect("authority channel");
        let events = Arc::new(Mutex::new(Vec::new()));
        let release_calls = Arc::new(AtomicUsize::new(0));
        let bridge = FakeBridge::new(Arc::clone(&events), Arc::clone(&release_calls), false);
        let broker = PlatformAuthorityBroker::bootstrap_with_bridge(generation, bridge)
            .await
            .expect("broker");
        let lease = broker.current_lease().expect("standard lease");
        let first_calls = Arc::new(AtomicUsize::new(0));
        let second_calls = Arc::new(AtomicUsize::new(0));

        broker
            .mount_dependent(
                "provider.first",
                RecordingService::new(
                    "provider.first",
                    Arc::clone(&events),
                    Arc::clone(&first_calls),
                    true,
                    Some(Arc::clone(&lease)),
                ),
                &CancellationToken::new(),
            )
            .await
            .expect("first dependency");
        broker
            .mount_dependent(
                "provider.second",
                RecordingService::new(
                    "provider.second",
                    Arc::clone(&events),
                    Arc::clone(&second_calls),
                    false,
                    Some(Arc::clone(&lease)),
                ),
                &CancellationToken::new(),
            )
            .await
            .expect("second dependency");

        let first_retirement = broker.retire(&CancellationToken::new()).await;
        assert!(matches!(
            first_retirement,
            Err(PlatformError::AuthorityDependentCleanup { ref service_id })
                if service_id == "provider.first"
        ));
        assert!(!lease.is_current());
        assert_eq!(
            broker.snapshot().state(),
            PlatformAuthorityState::RecoveryPending
        );
        assert_eq!(
            events.lock().expect("event lock").as_slice(),
            ["provider.second", "provider.first"]
        );

        broker
            .retire(&CancellationToken::new())
            .await
            .expect("retirement retry");
        assert_eq!(
            events.lock().expect("event lock").as_slice(),
            [
                "provider.second",
                "provider.first",
                "provider.first",
                "platform.authority.standard",
            ]
        );
        assert_eq!(first_calls.load(Ordering::SeqCst), 2);
        assert_eq!(second_calls.load(Ordering::SeqCst), 1);
        assert_eq!(release_calls.load(Ordering::SeqCst), 1);
        assert_eq!(broker.snapshot().state(), PlatformAuthorityState::Revoked);
        assert!(broker.current_lease().is_none());
    }

    #[tokio::test]
    async fn broker_observes_actual_local_runtime_then_authority_disposer_order() {
        let (_, _, generation) = host_authority_channel().expect("authority channel");
        let events = Arc::new(Mutex::new(Vec::new()));
        let release_calls = Arc::new(AtomicUsize::new(0));
        let bridge = FakeBridge::new(Arc::clone(&events), Arc::clone(&release_calls), false);
        let broker = PlatformAuthorityBroker::bootstrap_with_bridge(generation, bridge)
            .await
            .expect("broker");
        broker
            .mount_dependent(
                "provider.local-runtime",
                RecordingService::new(
                    "provider.local-runtime",
                    Arc::clone(&events),
                    Arc::new(AtomicUsize::new(0)),
                    false,
                    broker.current_lease(),
                ),
                &CancellationToken::new(),
            )
            .await
            .expect("local runtime dependency");

        broker
            .retire(&CancellationToken::new())
            .await
            .expect("authority retirement");

        assert_eq!(
            events.lock().expect("event lock").as_slice(),
            ["provider.local-runtime", "platform.authority.standard"]
        );
        assert_eq!(release_calls.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn broker_retries_authority_release_without_relaunching_a_dependent() {
        let (_, _, generation) = host_authority_channel().expect("authority channel");
        let events = Arc::new(Mutex::new(Vec::new()));
        let release_calls = Arc::new(AtomicUsize::new(0));
        let bridge = FakeBridge::new(Arc::clone(&events), Arc::clone(&release_calls), true);
        let broker = PlatformAuthorityBroker::bootstrap_with_bridge(generation, bridge)
            .await
            .expect("broker");
        let dependent_calls = Arc::new(AtomicUsize::new(0));
        broker
            .mount_dependent(
                "provider.local-runtime",
                RecordingService::new(
                    "provider.local-runtime",
                    Arc::clone(&events),
                    Arc::clone(&dependent_calls),
                    false,
                    broker.current_lease(),
                ),
                &CancellationToken::new(),
            )
            .await
            .expect("provider dependency");

        assert!(matches!(
            broker.retire(&CancellationToken::new()).await,
            Err(PlatformError::AuthorityReleaseFailed)
        ));
        assert_eq!(
            broker.snapshot().state(),
            PlatformAuthorityState::RecoveryPending
        );
        broker
            .retire(&CancellationToken::new())
            .await
            .expect("authority retry");

        assert_eq!(dependent_calls.load(Ordering::SeqCst), 1);
        assert_eq!(release_calls.load(Ordering::SeqCst), 2);
        assert_eq!(
            events.lock().expect("event lock").as_slice(),
            [
                "provider.local-runtime",
                "platform.authority.standard",
                "platform.authority.standard",
            ]
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn concurrent_retirement_closes_admission_and_disposes_each_resource_once() {
        let (_, _, generation) = host_authority_channel().expect("authority channel");
        let events = Arc::new(Mutex::new(Vec::new()));
        let release_calls = Arc::new(AtomicUsize::new(0));
        let bridge = FakeBridge::new(Arc::clone(&events), Arc::clone(&release_calls), false);
        let broker = Arc::new(
            PlatformAuthorityBroker::bootstrap_with_bridge(generation, bridge)
                .await
                .expect("broker"),
        );
        let dependent_calls = Arc::new(AtomicUsize::new(0));
        let started = Arc::new(Notify::new());
        let proceed = Arc::new(Notify::new());
        broker
            .mount_dependent(
                "provider.local-runtime",
                RecordingService::new(
                    "provider.local-runtime",
                    Arc::clone(&events),
                    Arc::clone(&dependent_calls),
                    false,
                    broker.current_lease(),
                )
                .with_gate(Arc::clone(&started), Arc::clone(&proceed)),
                &CancellationToken::new(),
            )
            .await
            .expect("provider dependency");

        let first = {
            let broker = Arc::clone(&broker);
            tokio::spawn(async move { broker.retire(&CancellationToken::new()).await })
        };
        started.notified().await;
        assert!(matches!(
            broker
                .mount_dependent(
                    "provider.late",
                    RecordingService::new(
                        "provider.late",
                        Arc::clone(&events),
                        Arc::new(AtomicUsize::new(0)),
                        false,
                        None,
                    ),
                    &CancellationToken::new(),
                )
                .await,
            Err(PlatformError::AuthorityAdmissionClosed)
        ));
        let second = {
            let broker = Arc::clone(&broker);
            tokio::spawn(async move { broker.retire(&CancellationToken::new()).await })
        };
        proceed.notify_waiters();

        let (first_result, second_result) = timeout(Duration::from_secs(2), async {
            tokio::join!(first, second)
        })
        .await
        .expect("bounded retirement race");
        first_result.expect("first task").expect("first retirement");
        second_result
            .expect("second task")
            .expect("second retirement");
        broker
            .retire(&CancellationToken::new())
            .await
            .expect("idempotent retirement");

        assert_eq!(dependent_calls.load(Ordering::SeqCst), 1);
        assert_eq!(release_calls.load(Ordering::SeqCst), 1);
        assert_eq!(broker.snapshot().state(), PlatformAuthorityState::Revoked);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn reentrant_retirement_returns_without_waiting_for_the_active_owner() {
        let (_, _, generation) = host_authority_channel().expect("authority channel");
        let events = Arc::new(Mutex::new(Vec::new()));
        let release_calls = Arc::new(AtomicUsize::new(0));
        let bridge = FakeBridge::new(Arc::clone(&events), Arc::clone(&release_calls), false);
        let broker = PlatformAuthorityBroker::bootstrap_with_bridge(generation, bridge)
            .await
            .expect("broker");
        let outcome = Arc::new(AtomicUsize::new(0));
        let retire_calls = Arc::new(AtomicUsize::new(0));
        broker
            .mount_dependent(
                "provider.reentrant",
                ReentrantRetirementService {
                    broker: broker.clone(),
                    outcome: Arc::clone(&outcome),
                    retire_calls: Arc::clone(&retire_calls),
                },
                &CancellationToken::new(),
            )
            .await
            .expect("reentrant dependency");

        timeout(
            Duration::from_secs(1),
            broker.retire(&CancellationToken::new()),
        )
        .await
        .expect("bounded outer retirement")
        .expect("outer retirement");

        assert_eq!(outcome.load(Ordering::SeqCst), 1);
        assert_eq!(retire_calls.load(Ordering::SeqCst), 1);
        assert_eq!(release_calls.load(Ordering::SeqCst), 1);
        assert_eq!(broker.snapshot().state(), PlatformAuthorityState::Revoked);
        broker
            .retire(&CancellationToken::new())
            .await
            .expect("later idempotent retirement");
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn slow_cleanup_projects_recovery_pending_before_disposal_finishes() {
        let (_, _, generation) = host_authority_channel().expect("authority channel");
        let events = Arc::new(Mutex::new(Vec::new()));
        let release_calls = Arc::new(AtomicUsize::new(0));
        let bridge = FakeBridge::new(Arc::clone(&events), Arc::clone(&release_calls), false);
        let broker = Arc::new(
            PlatformAuthorityBroker::bootstrap_with_bridge(generation, bridge)
                .await
                .expect("broker"),
        );
        let lease = broker.current_lease().expect("standard lease");
        let dependent_calls = Arc::new(AtomicUsize::new(0));
        let started = Arc::new(Notify::new());
        let proceed = Arc::new(Notify::new());
        broker
            .mount_dependent(
                "provider.slow",
                RecordingService::new(
                    "provider.slow",
                    Arc::clone(&events),
                    Arc::clone(&dependent_calls),
                    false,
                    Some(Arc::clone(&lease)),
                )
                .with_gate(Arc::clone(&started), Arc::clone(&proceed)),
                &CancellationToken::new(),
            )
            .await
            .expect("slow dependency");

        let retirement = {
            let broker = Arc::clone(&broker);
            tokio::spawn(async move { broker.retire(&CancellationToken::new()).await })
        };
        timeout(Duration::from_secs(1), started.notified())
            .await
            .expect("slow cleanup started");
        let lease_is_current = lease.is_current();
        let state_during_cleanup = broker.snapshot().state();
        proceed.notify_one();
        timeout(Duration::from_secs(1), retirement)
            .await
            .expect("bounded slow retirement")
            .expect("retirement task")
            .expect("retirement result");

        assert!(!lease_is_current);
        assert_eq!(
            state_during_cleanup,
            PlatformAuthorityState::RecoveryPending
        );
        assert_eq!(dependent_calls.load(Ordering::SeqCst), 1);
        assert_eq!(release_calls.load(Ordering::SeqCst), 1);
        assert_eq!(broker.snapshot().state(), PlatformAuthorityState::Revoked);
    }

    #[tokio::test]
    async fn broker_rejects_noninitial_generation_and_mismatched_dependent_ids() {
        let (_, _, generation) = host_authority_channel().expect("authority channel");
        let next_generation = generation.successor().expect("next generation");
        let events = Arc::new(Mutex::new(Vec::new()));
        let release_calls = Arc::new(AtomicUsize::new(0));
        let result = PlatformAuthorityBroker::bootstrap_with_bridge(
            next_generation,
            FakeBridge::new(Arc::clone(&events), Arc::clone(&release_calls), false),
        )
        .await;
        assert!(matches!(
            result,
            Err(PlatformError::InvalidInitialAuthorityGeneration)
        ));

        let (_, _, generation) = host_authority_channel().expect("authority channel");
        let broker = PlatformAuthorityBroker::bootstrap_with_bridge(
            generation,
            FakeBridge::new(Arc::clone(&events), Arc::clone(&release_calls), false),
        )
        .await
        .expect("broker");
        assert!(matches!(
            broker
                .mount_dependent(
                    "provider.claimed",
                    RecordingService::new(
                        "provider.actual",
                        events,
                        Arc::new(AtomicUsize::new(0)),
                        false,
                        None,
                    ),
                    &CancellationToken::new(),
                )
                .await,
            Err(PlatformError::AuthorityDependentIdMismatch)
        ));
        assert!(matches!(
            PlatformError::authority_dependent_cleanup("INVALID SERVICE"),
            Err(PlatformError::InvalidAuthorityServiceId)
        ));
    }

    #[tokio::test]
    async fn dependent_mount_rejects_duplicates_and_publishes_only_after_success() {
        let (_, _, generation) = host_authority_channel().expect("authority channel");
        let events = Arc::new(Mutex::new(Vec::new()));
        let release_calls = Arc::new(AtomicUsize::new(0));
        let broker = PlatformAuthorityBroker::bootstrap_with_bridge(
            generation,
            FakeBridge::new(Arc::clone(&events), release_calls, false),
        )
        .await
        .expect("broker");

        broker
            .mount_dependent(
                "provider.existing",
                RecordingService::new(
                    "provider.existing",
                    Arc::clone(&events),
                    Arc::new(AtomicUsize::new(0)),
                    false,
                    broker.current_lease(),
                ),
                &CancellationToken::new(),
            )
            .await
            .expect("first mount");
        assert!(matches!(
            broker
                .mount_dependent(
                    "provider.existing",
                    RecordingService::new(
                        "provider.existing",
                        Arc::clone(&events),
                        Arc::new(AtomicUsize::new(0)),
                        false,
                        broker.current_lease(),
                    ),
                    &CancellationToken::new(),
                )
                .await,
            Err(PlatformError::DuplicateAuthorityDependent)
        ));

        let cancelled = CancellationToken::new();
        cancelled.cancel();
        assert!(broker
            .mount_dependent(
                "provider.retryable",
                RecordingService::new(
                    "provider.retryable",
                    Arc::clone(&events),
                    Arc::new(AtomicUsize::new(0)),
                    false,
                    broker.current_lease(),
                ),
                &cancelled,
            )
            .await
            .is_err());
        broker
            .mount_dependent(
                "provider.retryable",
                RecordingService::new(
                    "provider.retryable",
                    events,
                    Arc::new(AtomicUsize::new(0)),
                    false,
                    broker.current_lease(),
                ),
                &CancellationToken::new(),
            )
            .await
            .expect("retryable mount was not published on failure");
    }

    #[test]
    fn deadline_expiry_revokes_a_future_elevated_lease() {
        let (_, _, generation) = host_authority_channel().expect("authority channel");
        let lease = PlatformAuthorityLease::new(
            "future.elevated.test",
            PlatformAuthorityTier::Root,
            test_probe(),
            generation,
            ExpiryPolicy::Deadline(Instant::now()),
        )
        .expect("deadline lease");
        assert!(!lease.is_current());
    }

    #[cfg(windows)]
    #[tokio::test]
    async fn windows_bootstrap_projects_the_real_current_process_identity() {
        let (_, _, generation) = host_authority_channel().expect("authority channel");
        let broker = PlatformAuthorityBroker::bootstrap(generation)
            .await
            .expect("Windows authority broker");
        let snapshot = broker.snapshot();
        let probe = snapshot.probe().expect("Windows capability probe");

        assert_eq!(snapshot.platform(), "windows");
        assert_eq!(snapshot.state(), PlatformAuthorityState::Active);
        assert_eq!(
            snapshot.active_tier(),
            Some(PlatformAuthorityTier::Standard)
        );
        assert_eq!(snapshot.generation(), 1);
        assert_eq!(probe.observed_uid(), None);
        assert!(probe
            .observed_identity()
            .is_some_and(|identity| identity.starts_with("S-1-")));
        assert_eq!(probe.selinux_context(), None);
        assert_eq!(
            probe.capabilities(),
            [
                "filesystem.user",
                "network.user",
                "process.user",
                "windows.job-object",
                "windows.local-runtime",
            ]
        );
        let lease = broker.current_lease().expect("current Standard lease");
        assert_eq!(lease.tier(), PlatformAuthorityTier::Standard);
        assert_eq!(lease.generation_ordinal(), 1);
    }

    #[cfg(not(any(windows, target_os = "android")))]
    #[tokio::test]
    async fn unsupported_target_keeps_host_generation_without_a_platform_lease() {
        let (_, _, generation) = host_authority_channel().expect("authority channel");
        let broker = PlatformAuthorityBroker::bootstrap(generation.clone())
            .await
            .expect("unavailable broker");

        assert_eq!(
            broker.snapshot().state(),
            PlatformAuthorityState::Unavailable
        );
        assert_eq!(
            broker.snapshot().detail(),
            "Platform authority is not implemented for this target"
        );
        assert_eq!(broker.current_generation(), Some(generation));
        assert!(broker.current_lease().is_none());
        assert!(matches!(
            broker
                .mount_dependent(
                    "provider.local-runtime",
                    RecordingService::new(
                        "provider.local-runtime",
                        Arc::new(Mutex::new(Vec::new())),
                        Arc::new(AtomicUsize::new(0)),
                        false,
                        None,
                    ),
                    &CancellationToken::new(),
                )
                .await,
            Err(PlatformError::AuthorityUnavailable)
        ));
    }

    #[tokio::test]
    async fn public_snapshot_serialization_excludes_opaque_authority_material() {
        let (_, _, generation) = host_authority_channel().expect("authority channel");
        let broker = PlatformAuthorityBroker::bootstrap(generation)
            .await
            .expect("platform broker");
        let serialized = serde_json::to_string(&broker.snapshot()).expect("snapshot JSON");

        for private_name in [
            "lease_id",
            "issuer_id",
            "bridge_id",
            "epoch",
            "cancellation",
            "revocation",
        ] {
            assert!(!serialized.contains(private_name));
        }
        if let Some(lease) = broker.current_lease() {
            let debug = format!("{lease:?}");
            assert!(!debug.contains("lease_id"));
            assert!(!debug.contains("bridge_id"));
            assert!(!debug.contains("epoch"));
            assert!(!debug.contains("cancellation"));
            assert!(!debug.contains("revocation"));
        }
    }
}
