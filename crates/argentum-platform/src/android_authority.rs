#[cfg(target_os = "android")]
use std::fs::File;
#[cfg(target_os = "android")]
use std::io::Read;

use argentum_domain::PlatformAuthorityTier;
use tokio_util::sync::CancellationToken;

use crate::authority::{
    sealed, PlatformAuthorityBridge, PlatformAuthorityFuture, PlatformCapabilityProbe,
};
use crate::PlatformError;

const ROOT_UID: u32 = 0;
const SHELL_UID: u32 = 2_000;
const MAX_SELINUX_CONTEXT_BYTES: u64 = 256;

#[derive(Debug)]
pub struct AndroidStandardAuthorityBridge {
    observed_uid: u32,
    selinux_context: Option<String>,
    released: bool,
}

impl AndroidStandardAuthorityBridge {
    #[cfg(target_os = "android")]
    pub(crate) fn current_process() -> Result<Self, PlatformError> {
        // SAFETY: getuid takes no arguments and returns the real UID of the current process.
        let observed_uid = unsafe { getuid() };
        Self::from_observation(observed_uid, read_current_selinux_context())
    }

    fn from_observation(
        observed_uid: u32,
        selinux_context: Option<String>,
    ) -> Result<Self, PlatformError> {
        if matches!(observed_uid, ROOT_UID | SHELL_UID) {
            return Err(PlatformError::AuthorityIdentityProbeFailed);
        }
        Ok(Self {
            observed_uid,
            selinux_context,
            released: false,
        })
    }
}

impl sealed::Sealed for AndroidStandardAuthorityBridge {}

impl PlatformAuthorityBridge for AndroidStandardAuthorityBridge {
    fn platform(&self) -> &'static str {
        "android"
    }

    fn bridge_id(&self) -> &'static str {
        "android.app-uid"
    }

    fn tier(&self) -> PlatformAuthorityTier {
        PlatformAuthorityTier::Standard
    }

    fn probe(&self) -> Result<PlatformCapabilityProbe, PlatformError> {
        PlatformCapabilityProbe::new(
            Some(self.observed_uid),
            Some(format!("uid:{}", self.observed_uid)),
            self.selinux_context.clone(),
            [
                "android.app-sandbox",
                "filesystem.app-private",
                "network.user",
                "process.app",
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
            self.released = true;
            Ok(())
        })
    }
}

#[cfg(target_os = "android")]
unsafe extern "C" {
    fn getuid() -> u32;
}

#[cfg(target_os = "android")]
fn read_current_selinux_context() -> Option<String> {
    let file = File::open("/proc/self/attr/current").ok()?;
    let mut bytes = Vec::new();
    file.take(MAX_SELINUX_CONTEXT_BYTES + 1)
        .read_to_end(&mut bytes)
        .ok()?;
    normalize_selinux_context(bytes)
}

fn normalize_selinux_context(mut bytes: Vec<u8>) -> Option<String> {
    if bytes.len() > MAX_SELINUX_CONTEXT_BYTES as usize {
        return None;
    }
    while matches!(bytes.last(), Some(b'\0' | b'\n' | b'\r')) {
        bytes.pop();
    }
    let context = String::from_utf8(bytes).ok()?;
    (!context.is_empty()
        && context
            .bytes()
            .all(|byte| byte.is_ascii_graphic() || byte == b' '))
    .then_some(context)
}

#[cfg(test)]
mod tests {
    use argentum_domain::PlatformAuthorityTier;

    use super::{normalize_selinux_context, AndroidStandardAuthorityBridge};
    use crate::authority::PlatformAuthorityBridge;

    #[test]
    fn standard_bridge_projects_only_the_observed_app_uid() {
        let bridge = AndroidStandardAuthorityBridge::from_observation(
            10_123,
            Some("u:r:untrusted_app:s0:c1,c2".into()),
        )
        .expect("Android Standard bridge");
        let probe = bridge.probe().expect("Android Standard probe");

        assert_eq!(bridge.platform(), "android");
        assert_eq!(bridge.bridge_id(), "android.app-uid");
        assert_eq!(bridge.tier(), PlatformAuthorityTier::Standard);
        assert_eq!(probe.observed_uid(), Some(10_123));
        assert_eq!(probe.observed_identity(), Some("uid:10123"));
        assert_eq!(probe.selinux_context(), Some("u:r:untrusted_app:s0:c1,c2"));
        assert_eq!(
            probe.capabilities(),
            [
                "android.app-sandbox",
                "filesystem.app-private",
                "network.user",
                "process.app",
            ]
        );
    }

    #[test]
    fn standard_bridge_rejects_shell_and_root_observations() {
        assert!(AndroidStandardAuthorityBridge::from_observation(0, None).is_err());
        assert!(AndroidStandardAuthorityBridge::from_observation(2_000, None).is_err());
    }

    #[test]
    fn selinux_context_normalization_is_bounded_and_printable() {
        assert_eq!(
            normalize_selinux_context(b"u:r:untrusted_app:s0\n".to_vec()).as_deref(),
            Some("u:r:untrusted_app:s0")
        );
        assert!(normalize_selinux_context(vec![b'a'; 257]).is_none());
        assert!(normalize_selinux_context(b"u:r:bad\x01:s0".to_vec()).is_none());
    }
}
