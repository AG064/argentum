//! Local authority material is intentionally opaque and one use.
//!
//! ```compile_fail
//! use argentum_security::{host_authority_channel, LocalAuthorityAction};
//!
//! let (issuer, _, generation) = host_authority_channel().expect("authority channel");
//! let ticket = issuer
//!     .issue(
//!         LocalAuthorityAction::EnableCapability {
//!             capability_id: "execution.shell".into(),
//!         },
//!         generation,
//!     )
//!     .expect("ticket");
//! let _copy = ticket.clone();
//! ```
//!
//! ```compile_fail
//! use argentum_security::{host_authority_channel, LocalAuthorityAction};
//!
//! let (issuer, verifier, generation) = host_authority_channel().expect("authority channel");
//! let action = LocalAuthorityAction::EnableCapability {
//!     capability_id: "execution.shell".into(),
//! };
//! let ticket = issuer.issue(action.clone(), generation.clone()).expect("ticket");
//! let verified = verifier.consume(ticket, &action, generation).expect("verified");
//! let _copy = verified.clone();
//! ```
//!
//! ```compile_fail
//! use argentum_security::{host_authority_channel, LocalAuthorityAction};
//!
//! let (issuer, _, generation) = host_authority_channel().expect("authority channel");
//! let ticket = issuer
//!     .issue(
//!         LocalAuthorityAction::EnableCapability {
//!             capability_id: "execution.shell".into(),
//!         },
//!         generation,
//!     )
//!     .expect("ticket");
//! let _json = serde_json::to_string(&ticket).expect("authority JSON");
//! ```
//!
//! ```compile_fail
//! use argentum_security::{host_authority_channel, LocalAuthorityAction};
//!
//! let (issuer, verifier, generation) = host_authority_channel().expect("authority channel");
//! let action = LocalAuthorityAction::EnableCapability {
//!     capability_id: "execution.shell".into(),
//! };
//! let ticket = issuer.issue(action.clone(), generation.clone()).expect("ticket");
//! let verified = verifier.consume(ticket, &action, generation).expect("verified");
//! let _json = serde_json::to_string(&verified).expect("authority JSON");
//! ```

use std::fmt;
use std::sync::Arc;
use std::time::{Duration, Instant};

use argentum_domain::{
    is_valid_capability_id, ApprovalId, ApprovalMode, ApprovalScope, HarnessExecutionPolicy,
    PlatformAuthorityTier, SandboxScope, Sha256Digest,
};
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::SecurityError;

const PRODUCTION_TICKET_LIFETIME: Duration = Duration::from_secs(30);
const MAX_TICKET_LIFETIME: Duration = Duration::from_secs(60);

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LocalAuthorityAction {
    SelectExecutionPolicy {
        current: Sha256Digest,
        requested: Sha256Digest,
    },
    EnableCapability {
        capability_id: String,
    },
    ApproveTool {
        approval_id: ApprovalId,
        scope: ApprovalScope,
    },
    ApproveProjectVerification {
        runner_id: String,
        binding: Sha256Digest,
    },
    ActivateLocalRuntime {
        plan: Sha256Digest,
    },
    ActivatePlatformTier {
        tier: PlatformAuthorityTier,
        probe: Sha256Digest,
    },
    ResolveGoalAccounting {
        binding: Sha256Digest,
    },
}

impl LocalAuthorityAction {
    fn kind(&self) -> &'static str {
        match self {
            Self::SelectExecutionPolicy { .. } => "select_execution_policy",
            Self::EnableCapability { .. } => "enable_capability",
            Self::ApproveTool { .. } => "approve_tool",
            Self::ApproveProjectVerification { .. } => "approve_project_verification",
            Self::ActivateLocalRuntime { .. } => "activate_local_runtime",
            Self::ActivatePlatformTier { .. } => "activate_platform_tier",
            Self::ResolveGoalAccounting { .. } => "resolve_goal_accounting",
        }
    }

    fn validate(&self) -> Result<(), SecurityError> {
        match self {
            Self::SelectExecutionPolicy { current, requested } => {
                validate_digest(current)?;
                validate_digest(requested)
            }
            Self::EnableCapability { capability_id } => {
                if is_valid_capability_id(capability_id) {
                    Ok(())
                } else {
                    Err(SecurityError::LocalAuthorityInvalidCapability)
                }
            }
            Self::ApproveTool { approval_id, .. } => {
                if approval_id.is_nil() {
                    Err(SecurityError::LocalAuthorityInvalidApprovalId)
                } else {
                    Ok(())
                }
            }
            Self::ApproveProjectVerification { runner_id, binding } => {
                if runner_id.is_empty() || runner_id.len() > 128 {
                    return Err(SecurityError::LocalAuthorityInvalidApprovalId);
                }
                validate_digest(binding)
            }
            Self::ActivateLocalRuntime { plan } => validate_digest(plan),
            Self::ActivatePlatformTier { probe, .. } => validate_digest(probe),
            Self::ResolveGoalAccounting { binding } => validate_digest(binding),
        }
    }
}

#[derive(Clone, PartialEq, Eq)]
pub struct AuthorityGeneration {
    epoch: Uuid,
    ordinal: u64,
}

impl AuthorityGeneration {
    pub const fn ordinal(&self) -> u64 {
        self.ordinal
    }

    pub fn successor(&self) -> Result<Self, SecurityError> {
        let ordinal = self
            .ordinal
            .checked_add(1)
            .ok_or(SecurityError::AuthorityGenerationExhausted)?;
        Ok(Self {
            epoch: self.epoch,
            ordinal,
        })
    }
}

impl fmt::Debug for AuthorityGeneration {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("AuthorityGeneration")
            .field("ordinal", &self.ordinal)
            .finish()
    }
}

trait AuthorityClock: Send + Sync {
    fn now(&self) -> Duration;
}

struct SystemAuthorityClock {
    origin: Instant,
}

impl SystemAuthorityClock {
    fn new() -> Self {
        Self {
            origin: Instant::now(),
        }
    }
}

impl AuthorityClock for SystemAuthorityClock {
    fn now(&self) -> Duration {
        self.origin.elapsed()
    }
}

pub struct LocalAuthorityIssuer {
    issuer_id: Uuid,
    epoch: Uuid,
    lifetime: Duration,
    clock: Arc<dyn AuthorityClock>,
}

impl LocalAuthorityIssuer {
    pub fn issue(
        &self,
        action: LocalAuthorityAction,
        generation: AuthorityGeneration,
    ) -> Result<LocalAuthorityTicket, SecurityError> {
        if generation.epoch != self.epoch {
            return Err(SecurityError::LocalAuthorityForeignGeneration);
        }
        action.validate()?;

        let issued_at = self.clock.now();
        let expires_at = issued_at
            .checked_add(self.lifetime)
            .ok_or(SecurityError::LocalAuthorityTimeOverflow)?;
        Ok(LocalAuthorityTicket {
            issuer_id: self.issuer_id,
            ticket_id: Uuid::new_v4(),
            action,
            generation,
            issued_at,
            expires_at,
        })
    }
}

pub struct LocalAuthorityVerifier {
    issuer_id: Uuid,
    clock: Arc<dyn AuthorityClock>,
}

impl LocalAuthorityVerifier {
    pub fn consume(
        &self,
        ticket: LocalAuthorityTicket,
        action: &LocalAuthorityAction,
        generation: AuthorityGeneration,
    ) -> Result<VerifiedLocalAuthority, SecurityError> {
        if ticket.issuer_id != self.issuer_id {
            return Err(SecurityError::LocalAuthorityIssuerMismatch);
        }
        if ticket.generation != generation {
            return Err(SecurityError::LocalAuthorityGenerationMismatch);
        }
        if &ticket.action != action {
            return Err(SecurityError::LocalAuthorityActionMismatch);
        }
        if self.clock.now() >= ticket.expires_at {
            return Err(SecurityError::LocalAuthorityExpired);
        }

        let _ticket_identity = (ticket.ticket_id, ticket.issued_at);
        Ok(VerifiedLocalAuthority {
            action: ticket.action,
            generation: ticket.generation,
            expires_at: ticket.expires_at,
            clock: Arc::clone(&self.clock),
        })
    }
}

pub struct LocalAuthorityTicket {
    issuer_id: Uuid,
    ticket_id: Uuid,
    action: LocalAuthorityAction,
    generation: AuthorityGeneration,
    issued_at: Duration,
    expires_at: Duration,
}

impl fmt::Debug for LocalAuthorityTicket {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("LocalAuthorityTicket")
            .field("action_kind", &self.action.kind())
            .field("generation", &self.generation.ordinal())
            .finish()
    }
}

pub struct VerifiedLocalAuthority {
    action: LocalAuthorityAction,
    generation: AuthorityGeneration,
    expires_at: Duration,
    clock: Arc<dyn AuthorityClock>,
}

impl VerifiedLocalAuthority {
    pub fn action(&self) -> &LocalAuthorityAction {
        &self.action
    }

    pub fn authorize(
        self,
        action: &LocalAuthorityAction,
        generation: &AuthorityGeneration,
    ) -> Result<(), SecurityError> {
        if &self.action != action {
            return Err(SecurityError::LocalAuthorityActionMismatch);
        }
        if &self.generation != generation {
            return Err(SecurityError::LocalAuthorityGenerationMismatch);
        }
        if self.clock.now() >= self.expires_at {
            return Err(SecurityError::LocalAuthorityExpired);
        }
        Ok(())
    }
}

impl fmt::Debug for VerifiedLocalAuthority {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("VerifiedLocalAuthority")
            .field("action_kind", &self.action.kind())
            .field("generation", &self.generation.ordinal())
            .finish()
    }
}

pub fn host_authority_channel() -> Result<
    (
        LocalAuthorityIssuer,
        LocalAuthorityVerifier,
        AuthorityGeneration,
    ),
    SecurityError,
> {
    authority_channel_with_clock(
        Arc::new(SystemAuthorityClock::new()),
        PRODUCTION_TICKET_LIFETIME,
        1,
    )
}

fn authority_channel_with_clock(
    clock: Arc<dyn AuthorityClock>,
    lifetime: Duration,
    initial_ordinal: u64,
) -> Result<
    (
        LocalAuthorityIssuer,
        LocalAuthorityVerifier,
        AuthorityGeneration,
    ),
    SecurityError,
> {
    let epoch = Uuid::new_v4();
    let issuer_id = Uuid::new_v4();
    let lifetime = lifetime.min(MAX_TICKET_LIFETIME);
    Ok((
        LocalAuthorityIssuer {
            issuer_id,
            epoch,
            lifetime,
            clock: Arc::clone(&clock),
        },
        LocalAuthorityVerifier { issuer_id, clock },
        AuthorityGeneration {
            epoch,
            ordinal: initial_ordinal,
        },
    ))
}

pub fn execution_policy_digest(policy: &HarnessExecutionPolicy) -> Sha256Digest {
    let mut hasher = Sha256::new();
    hasher.update(b"argentum.execution-policy.v1\0");
    hash_text(&mut hasher, &policy.profile_id);
    hasher.update([sandbox_order(policy.sandbox_scope)]);
    hasher.update([approval_order(policy.approval_mode)]);
    hasher.update(
        u64::try_from(policy.capability_enabled.len())
            .expect("policy capability count fits u64")
            .to_be_bytes(),
    );
    for (capability_id, enabled) in &policy.capability_enabled {
        hash_text(&mut hasher, capability_id);
        hasher.update([u8::from(*enabled)]);
    }
    Sha256Digest(format!("{:x}", hasher.finalize()))
}

pub fn execution_policy_is_elevation(
    current: &HarnessExecutionPolicy,
    requested: &HarnessExecutionPolicy,
) -> bool {
    sandbox_order(requested.sandbox_scope) > sandbox_order(current.sandbox_scope)
        || approval_order(requested.approval_mode) > approval_order(current.approval_mode)
        || requested.capability_enabled.iter().any(|(id, enabled)| {
            *enabled && !current.capability_enabled.get(id).copied().unwrap_or(false)
        })
}

pub fn authorize_profile_transition(
    current: &HarnessExecutionPolicy,
    requested: &HarnessExecutionPolicy,
    generation: &AuthorityGeneration,
    authority: Option<VerifiedLocalAuthority>,
) -> Result<(), SecurityError> {
    if execution_policy_is_elevation(current, requested) {
        let authority = authority.ok_or(SecurityError::ProfileElevationDenied)?;
        let action = LocalAuthorityAction::SelectExecutionPolicy {
            current: execution_policy_digest(current),
            requested: execution_policy_digest(requested),
        };
        authority.authorize(&action, generation)
    } else if authority.is_some() {
        Err(SecurityError::LocalAuthorityActionMismatch)
    } else {
        Ok(())
    }
}

fn validate_digest(digest: &Sha256Digest) -> Result<(), SecurityError> {
    if digest.0.len() == 64
        && digest
            .0
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        Ok(())
    } else {
        Err(SecurityError::LocalAuthorityInvalidDigest)
    }
}

fn hash_text(hasher: &mut Sha256, value: &str) {
    hasher.update(
        u64::try_from(value.len())
            .expect("text length fits u64")
            .to_be_bytes(),
    );
    hasher.update(value.as_bytes());
}

const fn sandbox_order(scope: SandboxScope) -> u8 {
    match scope {
        SandboxScope::ReadOnly => 0,
        SandboxScope::WorkspaceWrite => 1,
        SandboxScope::Unrestricted => 2,
    }
}

const fn approval_order(mode: ApprovalMode) -> u8 {
    match mode {
        ApprovalMode::Always => 0,
        ApprovalMode::OnRisk => 1,
        ApprovalMode::OnFailure => 2,
        ApprovalMode::Never => 3,
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::sync::Arc;
    use std::time::Duration;

    use argentum_domain::{ApprovalMode, HarnessExecutionPolicy, SandboxScope};

    use super::*;

    #[derive(Default)]
    struct TestAuthorityClock {
        millis: AtomicU64,
    }

    impl TestAuthorityClock {
        fn advance(&self, duration: Duration) {
            self.millis.fetch_add(
                u64::try_from(duration.as_millis()).expect("bounded test duration"),
                Ordering::SeqCst,
            );
        }
    }

    impl AuthorityClock for TestAuthorityClock {
        fn now(&self) -> Duration {
            Duration::from_millis(self.millis.load(Ordering::SeqCst))
        }
    }

    fn policy(
        profile_id: &str,
        sandbox_scope: SandboxScope,
        approval_mode: ApprovalMode,
        capabilities: &[(&str, bool)],
    ) -> HarnessExecutionPolicy {
        HarnessExecutionPolicy {
            profile_id: profile_id.to_owned(),
            source_profile_id: Some(profile_id.to_owned()),
            sandbox_scope,
            approval_mode,
            capability_enabled: capabilities
                .iter()
                .map(|(id, enabled)| ((*id).to_owned(), *enabled))
                .collect::<BTreeMap<_, _>>(),
        }
    }

    #[test]
    fn ticket_consumption_and_authorization_require_the_exact_action() {
        let (issuer, verifier, generation) = host_authority_channel().expect("authority channel");
        let action = LocalAuthorityAction::EnableCapability {
            capability_id: "execution.shell".into(),
        };
        let ticket = issuer
            .issue(action.clone(), generation.clone())
            .expect("ticket");
        let verified = verifier
            .consume(ticket, &action, generation.clone())
            .expect("exact authority");
        assert_eq!(verified.action(), &action);
        assert_eq!(
            format!("{verified:?}"),
            "VerifiedLocalAuthority { action_kind: \"enable_capability\", generation: 1 }"
        );
        verified
            .authorize(&action, &generation)
            .expect("one consuming authorization");

        let wrong = LocalAuthorityAction::EnableCapability {
            capability_id: "git.write".into(),
        };
        let ticket = issuer
            .issue(action.clone(), generation.clone())
            .expect("ticket");
        assert!(matches!(
            verifier.consume(ticket, &wrong, generation),
            Err(SecurityError::LocalAuthorityActionMismatch)
        ));
    }

    #[test]
    fn ticket_rejects_foreign_issuers_and_generations() {
        let (issuer, verifier, generation) =
            host_authority_channel().expect("first authority channel");
        let (foreign_issuer, foreign_verifier, foreign_generation) =
            host_authority_channel().expect("second authority channel");
        let action = LocalAuthorityAction::EnableCapability {
            capability_id: "execution.shell".into(),
        };

        assert!(matches!(
            issuer.issue(action.clone(), foreign_generation),
            Err(SecurityError::LocalAuthorityForeignGeneration)
        ));

        let ticket = issuer
            .issue(action.clone(), generation.clone())
            .expect("ticket");
        assert!(matches!(
            foreign_verifier.consume(ticket, &action, generation.clone()),
            Err(SecurityError::LocalAuthorityIssuerMismatch)
        ));

        let successor = generation.successor().expect("next generation");
        let ticket = issuer
            .issue(action.clone(), generation.clone())
            .expect("ticket");
        assert!(matches!(
            verifier.consume(ticket, &action, successor),
            Err(SecurityError::LocalAuthorityGenerationMismatch)
        ));

        let foreign_ticket = foreign_issuer.issue(
            action.clone(),
            host_authority_channel().expect("third channel").2,
        );
        assert!(matches!(
            foreign_ticket,
            Err(SecurityError::LocalAuthorityForeignGeneration)
        ));
    }

    #[test]
    fn verified_authority_rechecks_expiry_immediately_before_mutation() {
        let clock = Arc::new(TestAuthorityClock::default());
        let (issuer, verifier, generation) =
            authority_channel_with_clock(clock.clone(), Duration::from_secs(30), 1)
                .expect("test authority channel");
        let action = LocalAuthorityAction::ActivateLocalRuntime {
            plan: Sha256Digest("a".repeat(64)),
        };
        let ticket = issuer
            .issue(action.clone(), generation.clone())
            .expect("ticket");

        clock.advance(Duration::from_millis(29_999));
        let verified = verifier
            .consume(ticket, &action, generation.clone())
            .expect("valid just before expiry");
        clock.advance(Duration::from_millis(1));

        assert!(matches!(
            verified.authorize(&action, &generation),
            Err(SecurityError::LocalAuthorityExpired)
        ));
    }

    #[test]
    fn ticket_lifetime_is_capped_at_sixty_seconds() {
        let clock = Arc::new(TestAuthorityClock::default());
        let (issuer, verifier, generation) =
            authority_channel_with_clock(clock.clone(), Duration::from_secs(300), 1)
                .expect("test authority channel");
        let action = LocalAuthorityAction::EnableCapability {
            capability_id: "execution.shell".into(),
        };
        let ticket = issuer
            .issue(action.clone(), generation.clone())
            .expect("ticket");
        clock.advance(Duration::from_secs(60));

        assert!(matches!(
            verifier.consume(ticket, &action, generation),
            Err(SecurityError::LocalAuthorityExpired)
        ));
    }

    #[test]
    fn issuing_validates_action_identifiers_and_digests() {
        let (issuer, _, generation) = host_authority_channel().expect("authority channel");

        assert!(matches!(
            issuer.issue(
                LocalAuthorityAction::EnableCapability {
                    capability_id: "Execution_Shell".into(),
                },
                generation.clone(),
            ),
            Err(SecurityError::LocalAuthorityInvalidCapability)
        ));
        assert!(matches!(
            issuer.issue(
                LocalAuthorityAction::ActivateLocalRuntime {
                    plan: Sha256Digest("A".repeat(64)),
                },
                generation.clone(),
            ),
            Err(SecurityError::LocalAuthorityInvalidDigest)
        ));
        assert!(matches!(
            issuer.issue(
                LocalAuthorityAction::ApproveTool {
                    approval_id: ApprovalId::nil(),
                    scope: ApprovalScope::Once,
                },
                generation,
            ),
            Err(SecurityError::LocalAuthorityInvalidApprovalId)
        ));
    }

    #[test]
    fn generation_debug_omits_epoch_and_successor_preserves_binding() {
        let (_, _, generation) = host_authority_channel().expect("authority channel");
        assert_eq!(generation.ordinal(), 1);
        assert_eq!(
            format!("{generation:?}"),
            "AuthorityGeneration { ordinal: 1 }"
        );
        assert_eq!(generation.successor().expect("successor").ordinal(), 2);
    }

    #[test]
    fn generation_successor_rejects_ordinal_exhaustion() {
        let (_, _, generation) = host_authority_channel().expect("authority channel");
        let exhausted = AuthorityGeneration {
            epoch: generation.epoch,
            ordinal: u64::MAX,
        };

        assert!(matches!(
            exhausted.successor(),
            Err(SecurityError::AuthorityGenerationExhausted)
        ));
    }

    #[test]
    fn final_authorization_rechecks_action_and_generation_bindings() {
        let (issuer, verifier, generation) = host_authority_channel().expect("authority channel");
        let action = LocalAuthorityAction::EnableCapability {
            capability_id: "execution.shell".into(),
        };
        let wrong_action = LocalAuthorityAction::EnableCapability {
            capability_id: "git.write".into(),
        };
        let ticket = issuer
            .issue(action.clone(), generation.clone())
            .expect("ticket");
        let verified = verifier
            .consume(ticket, &action, generation.clone())
            .expect("verified authority");
        assert!(matches!(
            verified.authorize(&wrong_action, &generation),
            Err(SecurityError::LocalAuthorityActionMismatch)
        ));

        let ticket = issuer
            .issue(action.clone(), generation.clone())
            .expect("ticket");
        let verified = verifier
            .consume(ticket, &action, generation.clone())
            .expect("verified authority");
        let successor = generation.successor().expect("successor");
        assert!(matches!(
            verified.authorize(&action, &successor),
            Err(SecurityError::LocalAuthorityGenerationMismatch)
        ));
    }

    #[test]
    fn execution_policy_digest_has_a_stable_manual_field_order() {
        let policy = policy(
            "guided",
            SandboxScope::WorkspaceWrite,
            ApprovalMode::Always,
            &[("execution.shell", false), ("tool.read-text", true)],
        );

        assert_eq!(
            execution_policy_digest(&policy),
            Sha256Digest("a940cc7c0e7903cd17be24523c1fc67fc6447532b7a3de75dff1e9a8549b64a3".into())
        );
    }

    #[test]
    fn execution_policy_elevation_compares_scope_prompts_and_capabilities() {
        let full_access = policy(
            "full-access",
            SandboxScope::Unrestricted,
            ApprovalMode::Never,
            &[("execution.shell", true), ("tool.read-text", true)],
        );
        let guided = policy(
            "guided",
            SandboxScope::WorkspaceWrite,
            ApprovalMode::Always,
            &[("execution.shell", false), ("tool.read-text", true)],
        );
        let guided_with_shell_enabled = policy(
            "custom",
            SandboxScope::WorkspaceWrite,
            ApprovalMode::Always,
            &[("execution.shell", true), ("tool.read-text", true)],
        );
        let guided_with_write_disabled = policy(
            "custom",
            SandboxScope::WorkspaceWrite,
            ApprovalMode::Always,
            &[("execution.shell", false), ("tool.read-text", false)],
        );

        assert!(!execution_policy_is_elevation(&full_access, &guided));
        assert!(execution_policy_is_elevation(&guided, &full_access));
        assert!(execution_policy_is_elevation(
            &guided,
            &guided_with_shell_enabled
        ));
        assert!(!execution_policy_is_elevation(
            &guided,
            &guided_with_write_disabled
        ));
    }

    #[test]
    fn profile_elevation_consumes_an_exact_policy_transition_proof() {
        let current = policy(
            "guided",
            SandboxScope::WorkspaceWrite,
            ApprovalMode::Always,
            &[("execution.shell", false)],
        );
        let requested = policy(
            "full-access",
            SandboxScope::Unrestricted,
            ApprovalMode::Never,
            &[("execution.shell", true)],
        );
        let (issuer, verifier, generation) = host_authority_channel().expect("authority channel");
        let action = LocalAuthorityAction::SelectExecutionPolicy {
            current: execution_policy_digest(&current),
            requested: execution_policy_digest(&requested),
        };
        let ticket = issuer
            .issue(action.clone(), generation.clone())
            .expect("ticket");
        let authority = verifier
            .consume(ticket, &action, generation.clone())
            .expect("transition proof");

        authorize_profile_transition(&current, &requested, &generation, Some(authority))
            .expect("authorized elevation");
        assert!(matches!(
            authorize_profile_transition(&current, &requested, &generation, None),
            Err(SecurityError::ProfileElevationDenied)
        ));
    }

    #[test]
    fn profile_reduction_rejects_an_unexpected_authority_proof() {
        let current = policy(
            "full-access",
            SandboxScope::Unrestricted,
            ApprovalMode::Never,
            &[("execution.shell", true)],
        );
        let requested = policy(
            "guided",
            SandboxScope::WorkspaceWrite,
            ApprovalMode::Always,
            &[("execution.shell", false)],
        );
        let (issuer, verifier, generation) = host_authority_channel().expect("authority channel");
        let action = LocalAuthorityAction::SelectExecutionPolicy {
            current: execution_policy_digest(&current),
            requested: execution_policy_digest(&requested),
        };
        let ticket = issuer
            .issue(action.clone(), generation.clone())
            .expect("ticket");
        let authority = verifier
            .consume(ticket, &action, generation.clone())
            .expect("transition proof");

        authorize_profile_transition(&current, &requested, &generation, None)
            .expect("reduction without proof");
        assert!(matches!(
            authorize_profile_transition(&current, &requested, &generation, Some(authority)),
            Err(SecurityError::LocalAuthorityActionMismatch)
        ));
    }
}
