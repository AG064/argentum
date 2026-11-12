use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};
use thiserror::Error;

const MAX_PLATFORM_BYTES: usize = 32;
const MAX_CAPABILITIES: usize = 64;
const MAX_CAPABILITY_BYTES: usize = 64;
const MAX_IDENTITY_BYTES: usize = 256;
const MAX_DETAIL_BYTES: usize = 512;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PlatformAuthorityTier {
    #[default]
    Standard,
    ShizukuShell,
    Root,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PlatformAuthorityState {
    #[default]
    Unavailable,
    Active,
    Revoked,
    RecoveryPending,
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum AuthorityProjectionError {
    #[error("platform identifier is invalid")]
    InvalidPlatform,
    #[error("capability count exceeds the projection bound")]
    TooManyCapabilities,
    #[error("capability identifier is invalid")]
    InvalidCapability,
    #[error("observed identity is invalid")]
    InvalidIdentity,
    #[error("SELinux context is invalid")]
    InvalidSelinuxContext,
    #[error("authority detail is invalid")]
    InvalidDetail,
    #[error("authority state requires a nonzero generation")]
    GenerationRequired,
    #[error("active tier is not valid for this authority state")]
    UnexpectedActiveTier,
    #[error("active authority state requires an active tier")]
    ActiveTierRequired,
    #[error("active authority state requires a capability probe")]
    ActiveProbeRequired,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "PlatformCapabilityProbeSnapshotWire")]
pub struct PlatformCapabilityProbeSnapshot {
    observed_uid: Option<u32>,
    observed_identity: Option<String>,
    selinux_context: Option<String>,
    capabilities: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct PlatformCapabilityProbeSnapshotWire {
    observed_uid: Option<u32>,
    observed_identity: Option<String>,
    selinux_context: Option<String>,
    capabilities: Vec<String>,
}

impl PlatformCapabilityProbeSnapshot {
    pub fn new<I, S>(
        observed_uid: Option<u32>,
        observed_identity: Option<&str>,
        selinux_context: Option<&str>,
        capabilities: I,
    ) -> Result<Self, AuthorityProjectionError>
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        let observed_identity = observed_identity.map(str::to_owned);
        if observed_identity
            .as_deref()
            .is_some_and(|value| !is_printable_ascii(value, MAX_IDENTITY_BYTES))
        {
            return Err(AuthorityProjectionError::InvalidIdentity);
        }

        let selinux_context = selinux_context.map(str::to_owned);
        if selinux_context
            .as_deref()
            .is_some_and(|value| !is_printable_ascii(value, MAX_IDENTITY_BYTES))
        {
            return Err(AuthorityProjectionError::InvalidSelinuxContext);
        }

        let mut unique = BTreeSet::new();
        for capability in capabilities {
            let capability = capability.into();
            if !is_valid_capability_id(&capability) {
                return Err(AuthorityProjectionError::InvalidCapability);
            }
            unique.insert(capability);
            if unique.len() > MAX_CAPABILITIES {
                return Err(AuthorityProjectionError::TooManyCapabilities);
            }
        }

        Ok(Self {
            observed_uid,
            observed_identity,
            selinux_context,
            capabilities: unique.into_iter().collect(),
        })
    }

    pub const fn observed_uid(&self) -> Option<u32> {
        self.observed_uid
    }

    pub fn observed_identity(&self) -> Option<&str> {
        self.observed_identity.as_deref()
    }

    pub fn selinux_context(&self) -> Option<&str> {
        self.selinux_context.as_deref()
    }

    pub fn capabilities(&self) -> &[String] {
        &self.capabilities
    }
}

impl TryFrom<PlatformCapabilityProbeSnapshotWire> for PlatformCapabilityProbeSnapshot {
    type Error = AuthorityProjectionError;

    fn try_from(wire: PlatformCapabilityProbeSnapshotWire) -> Result<Self, Self::Error> {
        Self::new(
            wire.observed_uid,
            wire.observed_identity.as_deref(),
            wire.selinux_context.as_deref(),
            wire.capabilities,
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "PlatformAuthoritySnapshotWire")]
pub struct PlatformAuthoritySnapshot {
    platform: String,
    requested_tier: PlatformAuthorityTier,
    active_tier: Option<PlatformAuthorityTier>,
    state: PlatformAuthorityState,
    generation: u64,
    probe: Option<PlatformCapabilityProbeSnapshot>,
    detail: String,
}

#[derive(Debug, Deserialize)]
struct PlatformAuthoritySnapshotWire {
    platform: String,
    requested_tier: PlatformAuthorityTier,
    active_tier: Option<PlatformAuthorityTier>,
    state: PlatformAuthorityState,
    generation: u64,
    probe: Option<PlatformCapabilityProbeSnapshot>,
    detail: String,
}

impl PlatformAuthoritySnapshot {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        platform: &str,
        requested_tier: PlatformAuthorityTier,
        active_tier: Option<PlatformAuthorityTier>,
        state: PlatformAuthorityState,
        generation: u64,
        probe: Option<PlatformCapabilityProbeSnapshot>,
        detail: &str,
    ) -> Result<Self, AuthorityProjectionError> {
        if !is_platform_id(platform) {
            return Err(AuthorityProjectionError::InvalidPlatform);
        }
        if !is_printable_ascii(detail, MAX_DETAIL_BYTES) {
            return Err(AuthorityProjectionError::InvalidDetail);
        }
        if state != PlatformAuthorityState::Unavailable && generation == 0 {
            return Err(AuthorityProjectionError::GenerationRequired);
        }
        if state != PlatformAuthorityState::Active && active_tier.is_some() {
            return Err(AuthorityProjectionError::UnexpectedActiveTier);
        }
        if state == PlatformAuthorityState::Active && active_tier.is_none() {
            return Err(AuthorityProjectionError::ActiveTierRequired);
        }
        if state == PlatformAuthorityState::Active && probe.is_none() {
            return Err(AuthorityProjectionError::ActiveProbeRequired);
        }

        Ok(Self {
            platform: platform.to_owned(),
            requested_tier,
            active_tier,
            state,
            generation,
            probe,
            detail: detail.to_owned(),
        })
    }

    pub fn platform(&self) -> &str {
        &self.platform
    }

    pub const fn requested_tier(&self) -> PlatformAuthorityTier {
        self.requested_tier
    }

    pub const fn active_tier(&self) -> Option<PlatformAuthorityTier> {
        self.active_tier
    }

    pub const fn state(&self) -> PlatformAuthorityState {
        self.state
    }

    pub const fn generation(&self) -> u64 {
        self.generation
    }

    pub fn probe(&self) -> Option<&PlatformCapabilityProbeSnapshot> {
        self.probe.as_ref()
    }

    pub fn detail(&self) -> &str {
        &self.detail
    }
}

impl TryFrom<PlatformAuthoritySnapshotWire> for PlatformAuthoritySnapshot {
    type Error = AuthorityProjectionError;

    fn try_from(wire: PlatformAuthoritySnapshotWire) -> Result<Self, Self::Error> {
        Self::new(
            &wire.platform,
            wire.requested_tier,
            wire.active_tier,
            wire.state,
            wire.generation,
            wire.probe,
            &wire.detail,
        )
    }
}

fn is_platform_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= MAX_PLATFORM_BYTES
        && value.bytes().all(is_lowercase_identifier_byte)
}

/// Returns whether a capability identifier satisfies the shared authority grammar.
pub fn is_valid_capability_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= MAX_CAPABILITY_BYTES
        && value.bytes().all(is_lowercase_identifier_byte)
}

fn is_lowercase_identifier_byte(byte: u8) -> bool {
    byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'.' | b'-')
}

fn is_printable_ascii(value: &str, max_bytes: usize) -> bool {
    value.len() <= max_bytes && value.bytes().all(|byte| (b' '..=b'~').contains(&byte))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn authority_snapshot_round_trips_with_bounded_probe() {
        let probe = PlatformCapabilityProbeSnapshot::new(
            Some(2_000),
            Some("uid:2000"),
            Some("u:r:shell:s0"),
            ["process.shell", "package.query"],
        )
        .expect("bounded probe");
        let snapshot = PlatformAuthoritySnapshot::new(
            "android",
            PlatformAuthorityTier::ShizukuShell,
            Some(PlatformAuthorityTier::ShizukuShell),
            PlatformAuthorityState::Active,
            7,
            Some(probe),
            "Shizuku shell service verified",
        )
        .expect("bounded authority snapshot");
        let value = serde_json::to_value(&snapshot).expect("authority JSON");
        assert_eq!(value["requested_tier"], "shizuku_shell");
        assert_eq!(value["probe"]["observed_uid"], 2_000);
        assert_eq!(
            serde_json::from_value::<PlatformAuthoritySnapshot>(value)
                .expect("authority round trip"),
            snapshot
        );
    }

    #[test]
    fn authority_snapshot_deserialization_enforces_projection_bounds() {
        let capabilities = (0..65)
            .map(|index| format!("capability.{index}"))
            .collect::<Vec<_>>();
        let fixtures = [
            json!({
                "platform": "android",
                "requested_tier": "shizuku_shell",
                "active_tier": "shizuku_shell",
                "state": "active",
                "generation": 1,
                "probe": {
                    "observed_uid": 2000,
                    "observed_identity": "uid:2000",
                    "selinux_context": "u:r:shell:s0",
                    "capabilities": capabilities,
                },
                "detail": "verified",
            }),
            json!({
                "observed_uid": 2000,
                "observed_identity": "a".repeat(257),
                "selinux_context": "u:r:shell:s0",
                "capabilities": [],
            }),
            json!({
                "platform": "Android",
                "requested_tier": "standard",
                "active_tier": null,
                "state": "unavailable",
                "generation": 0,
                "probe": null,
                "detail": "not available",
            }),
            json!({
                "observed_uid": null,
                "observed_identity": null,
                "selinux_context": null,
                "capabilities": ["process_shell"],
            }),
            json!({
                "platform": "android",
                "requested_tier": "shizuku_shell",
                "active_tier": "shizuku_shell",
                "state": "active",
                "generation": 1,
                "probe": null,
                "detail": "missing probe",
            }),
            json!({
                "platform": "android",
                "requested_tier": "shizuku_shell",
                "active_tier": "shizuku_shell",
                "state": "revoked",
                "generation": 1,
                "probe": null,
                "detail": "revoked",
            }),
        ];

        assert!(serde_json::from_value::<PlatformAuthoritySnapshot>(fixtures[0].clone()).is_err());
        assert!(
            serde_json::from_value::<PlatformCapabilityProbeSnapshot>(fixtures[1].clone()).is_err()
        );
        assert!(serde_json::from_value::<PlatformAuthoritySnapshot>(fixtures[2].clone()).is_err());
        assert!(
            serde_json::from_value::<PlatformCapabilityProbeSnapshot>(fixtures[3].clone()).is_err()
        );
        assert!(serde_json::from_value::<PlatformAuthoritySnapshot>(fixtures[4].clone()).is_err());
        assert!(serde_json::from_value::<PlatformAuthoritySnapshot>(fixtures[5].clone()).is_err());
    }

    #[test]
    fn duplicate_capabilities_deserialize_to_sorted_unique_values() {
        let from_json = serde_json::from_value::<PlatformCapabilityProbeSnapshot>(json!({
            "observed_uid": 2000,
            "observed_identity": "uid:2000",
            "selinux_context": "u:r:shell:s0",
            "capabilities": ["process.shell", "package.query", "process.shell"],
        }))
        .expect("normalized probe");
        let from_constructor = PlatformCapabilityProbeSnapshot::new(
            Some(2_000),
            Some("uid:2000"),
            Some("u:r:shell:s0"),
            ["process.shell", "package.query", "process.shell"],
        )
        .expect("checked probe");

        assert_eq!(from_json, from_constructor);
        assert_eq!(
            from_json.capabilities(),
            &["package.query".to_owned(), "process.shell".to_owned()]
        );
    }

    #[test]
    fn non_unavailable_states_require_a_nonzero_generation() {
        for state in [
            PlatformAuthorityState::Active,
            PlatformAuthorityState::Revoked,
            PlatformAuthorityState::RecoveryPending,
        ] {
            let result = PlatformAuthoritySnapshot::new(
                "android",
                PlatformAuthorityTier::Standard,
                if state == PlatformAuthorityState::Active {
                    Some(PlatformAuthorityTier::Standard)
                } else {
                    None
                },
                state,
                0,
                if state == PlatformAuthorityState::Active {
                    Some(
                        PlatformCapabilityProbeSnapshot::new(
                            None,
                            None::<&str>,
                            None::<&str>,
                            std::iter::empty::<&str>(),
                        )
                        .expect("empty bounded probe"),
                    )
                } else {
                    None
                },
                "state",
            );
            assert!(matches!(
                result,
                Err(AuthorityProjectionError::GenerationRequired)
            ));
        }
    }
}
