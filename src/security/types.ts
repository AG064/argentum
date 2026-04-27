/**
 * Argentum Security Types
 *
 * Shared types for the security subsystem:
 * - Policy engine
 * - Credential manager
 * - Sandbox executor
 * - Approval/HITL system
 * - Blueprint configuration
 */

// ─── Agent Actions ─────────────────────────────────────────────────────────────

export type ActionResource =
  | `file://${string}`
  | `http://${string}`
  | `https://${string}`
  | `exec://${string}`
  | `network://${string}`
  | `api://${string}`
  | `env://${string}`;

export type ActionType = 'read' | 'write' | 'delete' | 'exec' | 'network' | 'api' | 'env';

export interface AgentAction {
  id: string;
  agentId: string;
  type: ActionType;
  resource: ActionResource;
  resourcePath?: string; // parsed path for file:// actions
  metadata?: Record<string, unknown>;
  timestamp: number;
}

// ─── Policy Engine ────────────────────────────────────────────────────────────

export type PolicyEffect = 'allow' | 'deny' | 'approve';

export interface PolicyCondition {
  field: string;
  operator: 'equals' | 'not_equals' | 'contains' | 'matches' | 'in' | 'greater_than' | 'less_than';
  value: unknown;
}

export interface Policy {
  id: string;
  name: string;
  description?: string;
  effect: PolicyEffect;
  resource: string; // glob pattern: file://**, http://**, exec://**, etc.
  action: string | '*'; // specific action or wildcard
  priority: number;
  enabled: boolean;
  conditions?: PolicyCondition[];
  requiresApproval: boolean;
  approvalRisk?: 'low' | 'medium' | 'high' | 'critical';
  auditLevel?: 'none' | 'info' | 'warning' | 'error';
  createdAt: number;
  updatedAt: number;
}

export type PolicyDecision =
  | { allowed: true; policy?: Policy; reason: string }
  | { allowed: false; policy?: Policy; reason: string; requiresApproval?: boolean };

// ─── Credential Manager ───────────────────────────────────────────────────────

export type CredentialType = 'api_key' | 'oauth' | 'jwt' | 'password' | 'certificate';

export interface CredentialConfig {
  provider: string;
  name: string;
  type: CredentialType;
  shortLived: boolean;
  ttlSeconds: number; // default 1800 (30 min)
  rotationEnabled: boolean;
  refreshThresholdSeconds?: number; // rotate before expiry, default 60s
}

export interface StoredCredential {
  id: string;
  provider: string;
  name: string;
  type: CredentialType;
  encryptedValue: string; // encrypted with master key
  iv: string;
  salt: string;
  tag: string;
  ttlSeconds: number;
  expiresAt: number; // unix timestamp
  rotatedAt: number;
  createdAt: number;
  lastUsedAt?: number;
  metadata?: Record<string, unknown>;
}

export interface MintedKey {
  key: string;
  expiresAt: number;
  resource: string;
}

// ─── Sandbox ─────────────────────────────────────────────────────────────────

export interface SandboxConfig {
  enabled: boolean;
  allowedPaths: string[]; // whitelist - must be absolute or ~ expanded
  deniedPaths: string[]; // blacklist - takes precedence
  maxMemoryMb?: number;
  maxCpuPercent?: number;
  networkIsolation: boolean;
  allowExec: boolean;
  maxExecutionTimeMs: number; // default 30000
  maxOutputSizeKb?: number; // default 512
  allowedLanguages: string[]; // ['javascript', 'python', 'bash']
}

export interface SandboxResult {
  success: boolean;
  output?: string;
  error?: string;
  exitCode?: number;
  executionTimeMs: number;
  memoryUsedMb?: number;
  language: string;
}

export interface SandboxCheckResult {
  allowed: boolean;
  reason?: string;
}

// ─── Approval / Human-in-the-Loop ───────────────────────────────────────────

export type ApprovalRisk = 'low' | 'medium' | 'high' | 'critical';

export interface ApprovalDetails {
  what: string;
  why: string;
  consequences: string;
  alternatives?: string[];
}

export interface ApprovalRequest {
  id: string;
  agentId: string;
  agentName?: string;
  action: AgentAction;
  risk: ApprovalRisk;
  details: ApprovalDetails;
  status: 'pending' | 'approved' | 'denied' | 'expired' | 'cancelled';
  requestedAt: number;
  expiresAt: number; // auto-expire if not acted upon
  respondedAt?: number;
  respondedBy?: string; // user ID who responded
  notes?: string; // user's notes when responding
}

// ─── Audit Log ────────────────────────────────────────────────────────────────

export type AuditAction =
  | 'policy.evaluate'
  | 'policy.deny'
  | 'policy.approve'
  | 'policy.allow'
  | 'credential.mint'
  | 'credential.rotate'
  | 'credential.use'
  | 'credential.expire'
  | 'sandbox.execute'
  | 'sandbox.block'
  | 'approval.request'
  | 'approval.approve'
  | 'approval.deny'
  | 'approval.expire'
  | 'exec.run'
  | 'exec.deny'
  | 'file.read'
  | 'file.write'
  | 'file.delete'
  | 'network.request'
  | 'network.block'
  | 'config.change'
  | 'security.violation'
  | 'session.start'
  | 'session.end';

export type AuditSeverity = 'debug' | 'info' | 'warning' | 'error' | 'critical';

export interface AuditEntry {
  id: string;
  timestamp: number; // unix ms
  isoTime: string; // ISO 8601
  action: AuditAction;
  severity: AuditSeverity;
  actor?: string; // agentId or userId
  resource?: string;
  details: Record<string, unknown>;
  decision?: 'allow' | 'deny' | 'approve' | 'block';
  policyId?: string;
  approvalId?: string;
  sessionId?: string;
  ip?: string;
  success: boolean;
  durationMs?: number;
}

// ─── Blueprint ────────────────────────────────────────────────────────────────

export interface BlueprintSandbox {
  enabled: boolean;
  allowedPaths: string[];
  deniedPaths: string[];
  maxMemory?: number;
  maxCpu?: number;
  networkIsolation: boolean;
  allowExec: boolean;
  maxExecutionTimeMs?: number;
}

export interface BlueprintPolicy {
  name: string;
  resource: string;
  action: string;
  effect: PolicyEffect;
  conditions?: PolicyCondition[];
  requiresApproval?: boolean;
  approvalRisk?: ApprovalRisk;
  priority?: number;
}

export interface BlueprintCredentials {
  autoRotate: boolean;
  ttlSeconds?: number;
  providers?: Record<string, CredentialConfig>;
}

export interface BlueprintApproval {
  criticalActions: string[]; // resource patterns that require approval
  autoExpireSeconds?: number; // default 300 (5 min)
  notifyChannels?: string[]; // ['telegram', 'email']
}

export interface Blueprint {
  version: string;
  sandbox?: BlueprintSandbox;
  policies?: BlueprintPolicy[];
  credentials?: BlueprintCredentials;
  approval?: BlueprintApproval;
  rateLimits?: Record<string, { windowMs: number; maxRequests: number }>;
  allowedHosts?: string[];
  allowedPaths?: string[];
}

// ─── Security Stats ───────────────────────────────────────────────────────────

export interface SecurityStats {
  policiesActive: number;
  policiesTotal: number;
  credentialsTotal: number;
  credentialsExpiringSoon: number; // expiring in next 5 min
  approvalsPending: number;
  approvalsTotal: number;
  auditEntriesTotal: number;
  sandboxExecutionsTotal: number;
  sandboxBlockedTotal: number;
  threatsDetected: number;
  uptime: number;
}

// ─── CLI Options ─────────────────────────────────────────────────────────────

export interface SecurityCLIOptions {
  format?: 'table' | 'json' | 'yaml';
  since?: number; // timestamp
  until?: number;
  actor?: string;
  action?: string;
  limit?: number;
  includeDetails?: boolean;
}
