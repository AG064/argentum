/**
 * Argentum Security Module
 *
 * Re-exports all security components for easy access.
 */
export { getPolicyEngine, resetPolicyEngine, type PolicyEngine } from './policy-engine/index';
export { getCredentialManager, resetCredentialManager, type CredentialManager, } from './credential-manager/index';
export { getSandboxExecutor, resetSandboxExecutor, type SandboxExecutor } from './sandbox/index';
export { getApprovalUI, type ApprovalUI } from './approval-ui/index';
export { getBlueprintLoader, type BlueprintLoader } from './blueprint/index';
export { createCapabilityBroker, type CapabilityAction, type CapabilityAuditEntry, type CapabilityBroker, type CapabilityDecision, type CapabilityGrant, type CapabilityGrantInput, type CapabilityGrantScope, type CapabilityRequest, } from './capability-broker';
export type { Policy, PolicyDecision, AgentAction, ApprovalRequest, ApprovalRisk, SandboxConfig, SandboxResult, SandboxCheckResult, CredentialConfig, StoredCredential, MintedKey, AuditEntry, AuditAction, AuditSeverity, Blueprint, SecurityStats, SecurityCLIOptions, PolicyCondition, PolicyEffect, } from './types';
//# sourceMappingURL=index.d.ts.map