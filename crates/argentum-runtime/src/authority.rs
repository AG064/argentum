use std::fmt;
use std::sync::Arc;

use argentum_domain::{AppCommand, PlatformAuthoritySnapshot};
use argentum_platform::PlatformAuthorityBroker;
use argentum_security::{
    execution_policy_digest, execution_policy_is_elevation, host_authority_channel,
    AuthorityGeneration, LocalAuthorityAction, LocalAuthorityTicket, LocalAuthorityVerifier,
    SecurityError, VerifiedLocalAuthority,
};

use crate::{RuntimeError, RuntimeService};

pub struct RuntimeHostAuthority {
    verifier: LocalAuthorityVerifier,
    platform_authority: Option<Arc<PlatformAuthorityBroker>>,
}

impl RuntimeHostAuthority {
    pub fn new(
        verifier: LocalAuthorityVerifier,
        platform_authority: Arc<PlatformAuthorityBroker>,
    ) -> Self {
        Self {
            verifier,
            platform_authority: Some(platform_authority),
        }
    }

    pub(crate) fn sealed_untrusted() -> Result<Self, SecurityError> {
        let (_discarded_issuer, verifier, _discarded_generation) = host_authority_channel()?;
        Ok(Self {
            verifier,
            platform_authority: None,
        })
    }

    fn current_generation(&self) -> Option<AuthorityGeneration> {
        self.platform_authority
            .as_ref()
            .and_then(|broker| broker.current_generation())
    }

    fn consume(
        &self,
        ticket: LocalAuthorityTicket,
        action: &LocalAuthorityAction,
    ) -> Result<VerifiedLocalAuthority, RuntimeError> {
        let generation = self
            .current_generation()
            .ok_or(RuntimeError::LocalAuthorityUnavailable)?;
        Ok(self.verifier.consume(ticket, action, generation)?)
    }

    fn snapshot(&self) -> Option<PlatformAuthoritySnapshot> {
        self.platform_authority
            .as_ref()
            .map(|broker| broker.snapshot())
    }

    async fn retire(
        &self,
        cancellation: &tokio_util::sync::CancellationToken,
    ) -> Result<(), RuntimeError> {
        let Some(platform_authority) = self.platform_authority.as_ref() else {
            return Ok(());
        };
        Ok(platform_authority.retire(cancellation).await?)
    }
}

impl fmt::Debug for RuntimeHostAuthority {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("RuntimeHostAuthority")
            .field("platform_authority", &self.snapshot())
            .finish_non_exhaustive()
    }
}

impl RuntimeService {
    pub fn required_local_authority_action(
        &self,
        command: &AppCommand,
    ) -> Result<Option<LocalAuthorityAction>, RuntimeError> {
        match command {
            AppCommand::SelectExecutionProfile { profile_id }
            | AppCommand::SetOnboardingPermission { profile_id } => {
                let current = self.execution_policy()?;
                let requested = self.with_harness(|harness| {
                    harness.apply_execution_profile(&current, profile_id)
                })?;
                Ok(
                    execution_policy_is_elevation(&current, &requested).then(|| {
                        LocalAuthorityAction::SelectExecutionPolicy {
                            current: execution_policy_digest(&current),
                            requested: execution_policy_digest(&requested),
                        }
                    }),
                )
            }
            AppCommand::SetHarnessCapabilityEnabled {
                capability_id,
                enabled,
            } => {
                let current = self.execution_policy()?;
                let requested = self.with_harness(|harness| {
                    harness.set_capability_enabled(&current, capability_id, *enabled)
                })?;
                Ok(
                    execution_policy_is_elevation(&current, &requested).then(|| {
                        LocalAuthorityAction::EnableCapability {
                            capability_id: capability_id.clone(),
                        }
                    }),
                )
            }
            AppCommand::ApproveTool { approval_id, scope } => {
                Ok(Some(LocalAuthorityAction::ApproveTool {
                    approval_id: *approval_id,
                    scope: scope.clone(),
                }))
            }
            AppCommand::ApproveProjectVerification { runner } => {
                let binding = self
                    .tools
                    .verification_binding(&self.workspace, std::slice::from_ref(runner))?;
                Ok(Some(LocalAuthorityAction::ApproveProjectVerification {
                    runner_id: runner.id(),
                    binding,
                }))
            }
            _ => Ok(None),
        }
    }

    pub fn verify_local_authority_ticket(
        &self,
        ticket: LocalAuthorityTicket,
        action: &LocalAuthorityAction,
    ) -> Result<VerifiedLocalAuthority, RuntimeError> {
        self.host_authority.consume(ticket, action)
    }

    pub fn platform_authority_snapshot(&self) -> Option<PlatformAuthoritySnapshot> {
        self.host_authority.snapshot()
    }

    pub fn publish_platform_authority_snapshot(&self) -> Option<PlatformAuthoritySnapshot> {
        let snapshot = self.platform_authority_snapshot()?;
        self.publish_transient(argentum_domain::AppEvent::PlatformAuthorityChanged(
            snapshot.clone(),
        ));
        Some(snapshot)
    }

    pub(crate) async fn retire_platform_authority(
        &self,
        cancellation: &tokio_util::sync::CancellationToken,
    ) -> Result<(), RuntimeError> {
        let result = self.host_authority.retire(cancellation).await;
        self.publish_platform_authority_snapshot();
        result
    }

    pub(crate) fn current_local_authority_generation(
        &self,
    ) -> Result<AuthorityGeneration, RuntimeError> {
        self.host_authority
            .current_generation()
            .ok_or(RuntimeError::LocalAuthorityUnavailable)
    }
}
