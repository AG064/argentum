import { randomUUID } from 'crypto';
import { appendFileSync, mkdirSync } from 'fs';
import { dirname, isAbsolute, resolve } from 'path';

import { createWorkspaceBoundary, type WorkspaceBoundary } from './workspace-boundary';

export type CapabilityAction =
  | 'file.read'
  | 'file.write'
  | 'file.delete'
  | 'shell.execute'
  | 'network.request'
  | 'os.automation'
  | 'integration.use';

export type CapabilityGrantScope =
  | 'one-action'
  | 'one-file'
  | 'one-folder'
  | 'one-command'
  | 'one-domain'
  | 'current-session'
  | 'always-for-workspace';

export type CapabilityDecisionReason =
  | 'inside-workspace'
  | 'active-grant'
  | 'outside-workspace'
  | 'invalid-path'
  | 'invalid-resource'
  | 'workspace-not-configured'
  | 'no-active-grant';

export interface CapabilityRequest {
  action: CapabilityAction;
  resource: string;
  requester?: string;
  reason?: string;
  metadata?: Record<string, unknown>;
}

export interface CapabilityGrantInput {
  action: CapabilityAction | '*';
  resource: string;
  scope: CapabilityGrantScope;
  grantedBy: string;
  reason: string;
  expiresAt?: number;
  expiresInMs?: number;
}

export interface CapabilityGrant {
  id: string;
  action: CapabilityAction | '*';
  resource: string;
  scope: CapabilityGrantScope;
  grantedBy: string;
  reason: string;
  createdAt: number;
  expiresAt?: number;
  revokedAt?: number;
  revokedBy?: string;
}

export type CapabilityAuditDecision = 'allow' | 'deny' | 'grant' | 'revoke' | 'expire';

export interface CapabilityAuditEntry {
  id: string;
  timestamp: number;
  isoTime: string;
  action: CapabilityAction | 'capability.grant' | 'capability.revoke' | 'capability.expire';
  decision: CapabilityAuditDecision;
  reason: string;
  requester?: string;
  resource: string;
  grantId?: string;
  success: boolean;
  details: Record<string, unknown>;
}

export type CapabilityDecision =
  | {
      allowed: true;
      action: CapabilityAction;
      reason: Extract<CapabilityDecisionReason, 'inside-workspace' | 'active-grant'>;
      resource: string;
      resolvedPath?: string;
      grantId?: string;
    }
  | {
      allowed: false;
      action: CapabilityAction;
      reason: Exclude<CapabilityDecisionReason, 'inside-workspace' | 'active-grant'>;
      resource: string;
      resolvedPath?: string;
    };

export interface CapabilityBrokerOptions {
  workspaceRoot: string;
  auditPath?: string;
  now?: () => number;
  auditSink?: (entry: CapabilityAuditEntry) => void;
}

export interface CapabilityBroker {
  readonly workspaceRoot: string;
  authorize(request: CapabilityRequest): CapabilityDecision;
  grant(input: CapabilityGrantInput): CapabilityGrant;
  revokeGrant(grantId: string, revokedBy: string): boolean;
  getGrants(includeRevoked?: boolean): CapabilityGrant[];
  getAuditEntries(limit?: number): CapabilityAuditEntry[];
}

interface NormalizedRequest {
  action: CapabilityAction;
  resource: string;
  resolvedPath?: string;
  workspaceAllowed?: boolean;
  workspaceReason?: CapabilityDecisionReason;
}

const FILE_ACTIONS = new Set<CapabilityAction>(['file.read', 'file.write', 'file.delete']);
const SENSITIVE_KEY_PATTERN = /((?:api[_-]?key|authorization|bearer|password|secret|token)\s*[:=]\s*)([^\s"'&]+)/gi;
const MAX_AUDIT_ENTRIES = 10_000;

export function createCapabilityBroker(options: CapabilityBrokerOptions): CapabilityBroker {
  return new DefaultCapabilityBroker(options);
}

class DefaultCapabilityBroker implements CapabilityBroker {
  readonly workspaceRoot: string;

  private readonly workspaceBoundary: WorkspaceBoundary;
  private readonly now: () => number;
  private readonly auditSink?: (entry: CapabilityAuditEntry) => void;
  private readonly auditPath?: string;
  private readonly grants = new Map<string, CapabilityGrant>();
  private readonly auditEntries: CapabilityAuditEntry[] = [];
  private readonly expiredGrantAuditIds = new Set<string>();

  constructor(options: CapabilityBrokerOptions) {
    this.workspaceBoundary = createWorkspaceBoundary(options.workspaceRoot);
    this.workspaceRoot = this.workspaceBoundary.workspaceRoot;
    this.now = options.now ?? Date.now;
    this.auditSink = options.auditSink;
    this.auditPath = options.auditPath
      ? resolveAuditPath(options.auditPath, this.workspaceRoot)
      : undefined;
  }

  authorize(request: CapabilityRequest): CapabilityDecision {
    const normalized = this.normalizeRequest(request);
    if (!normalized) {
      const decision: CapabilityDecision = {
        allowed: false,
        action: request.action,
        reason: 'invalid-resource',
        resource: redactText(request.resource),
      };
      this.recordAudit(request, decision);
      return decision;
    }

    if (FILE_ACTIONS.has(normalized.action)) {
      if (normalized.workspaceAllowed) {
        const decision: CapabilityDecision = {
          allowed: true,
          action: normalized.action,
          reason: 'inside-workspace',
          resource: normalized.resource,
          resolvedPath: normalized.resolvedPath,
        };
        this.recordAudit(request, decision);
        return decision;
      }

      if (
        normalized.workspaceReason === 'invalid-resource' ||
        normalized.workspaceReason === 'workspace-not-configured'
      ) {
        const decision: CapabilityDecision = {
          allowed: false,
          action: normalized.action,
          reason: normalized.workspaceReason,
          resource: normalized.resource,
          resolvedPath: normalized.resolvedPath,
        };
        this.recordAudit(request, decision);
        return decision;
      }
    }

    const grant = this.findActiveGrant(normalized.action, normalized.resource);
    if (grant) {
      const decision: CapabilityDecision = {
        allowed: true,
        action: normalized.action,
        reason: 'active-grant',
        resource: normalized.resource,
        resolvedPath: normalized.resolvedPath,
        grantId: grant.id,
      };
      this.recordAudit(request, decision);
      return decision;
    }

    const decision: CapabilityDecision = {
      allowed: false,
      action: normalized.action,
      reason: FILE_ACTIONS.has(normalized.action) ? 'outside-workspace' : 'no-active-grant',
      resource: normalized.resource,
      resolvedPath: normalized.resolvedPath,
    };
    this.recordAudit(request, decision);
    return decision;
  }

  grant(input: CapabilityGrantInput): CapabilityGrant {
    const createdAt = this.now();
    const grant: CapabilityGrant = {
      id: randomUUID(),
      action: input.action,
      resource: this.normalizeGrantResource(input.action, input.resource),
      scope: input.scope,
      grantedBy: input.grantedBy,
      reason: input.reason,
      createdAt,
      expiresAt: input.expiresAt ?? (input.expiresInMs === undefined ? undefined : createdAt + input.expiresInMs),
    };

    this.grants.set(grant.id, grant);
    this.recordLifecycleAudit('capability.grant', 'grant', grant.reason, grant.resource, {
      grantId: grant.id,
      action: grant.action,
      scope: grant.scope,
      grantedBy: grant.grantedBy,
      expiresAt: grant.expiresAt,
    });
    return { ...grant };
  }

  revokeGrant(grantId: string, revokedBy: string): boolean {
    const grant = this.grants.get(grantId);
    if (!grant || grant.revokedAt !== undefined) return false;

    grant.revokedAt = this.now();
    grant.revokedBy = revokedBy;
    this.recordLifecycleAudit('capability.revoke', 'revoke', 'grant-revoked', grant.resource, {
      grantId,
      action: grant.action,
      scope: grant.scope,
      revokedBy,
    });
    return true;
  }

  getGrants(includeRevoked = false): CapabilityGrant[] {
    return Array.from(this.grants.values())
      .filter((grant) => includeRevoked || grant.revokedAt === undefined)
      .map((grant) => ({ ...grant }));
  }

  getAuditEntries(limit = 100): CapabilityAuditEntry[] {
    return this.auditEntries.slice(-limit).map((entry) => ({ ...entry }));
  }

  private normalizeRequest(request: CapabilityRequest): NormalizedRequest | null {
    if (!request.resource || request.resource.includes('\0')) {
      return null;
    }

    if (FILE_ACTIONS.has(request.action)) {
      const path = stripProtocol(request.resource, 'file://');
      const decision = this.workspaceBoundary.checkPath(path);
      return {
        action: request.action,
        resource: decision.resolvedPath ? `file://${decision.resolvedPath}` : `file://${path}`,
        resolvedPath: decision.resolvedPath || undefined,
        workspaceAllowed: decision.allowed,
        workspaceReason: decision.reason,
      };
    }

    if (request.action === 'shell.execute') {
      const command = stripProtocol(request.resource, 'exec://');
      return {
        action: request.action,
        resource: `exec://${command}`,
      };
    }

    return {
      action: request.action,
      resource: request.resource,
    };
  }

  private normalizeGrantResource(action: CapabilityAction | '*', resource: string): string {
    if (action === '*' || resource === '*') return resource;

    if (FILE_ACTIONS.has(action)) {
      if (resource.endsWith('/**') || resource.endsWith('\\**')) {
        const base = resource.slice(0, -3);
        const decision = this.workspaceBoundary.checkPath(stripProtocol(base, 'file://'));
        return decision.resolvedPath ? `file://${decision.resolvedPath}/**` : `file://${base}/**`;
      }

      const decision = this.workspaceBoundary.checkPath(stripProtocol(resource, 'file://'));
      return decision.resolvedPath ? `file://${decision.resolvedPath}` : `file://${resource}`;
    }

    if (action === 'shell.execute') {
      return resource === 'exec://*' ? resource : `exec://${stripProtocol(resource, 'exec://')}`;
    }

    return resource;
  }

  private findActiveGrant(action: CapabilityAction, resource: string): CapabilityGrant | null {
    for (const grant of this.grants.values()) {
      if (grant.revokedAt !== undefined) continue;

      if (grant.expiresAt !== undefined && this.now() > grant.expiresAt) {
        this.auditExpiredGrant(grant);
        continue;
      }

      if ((grant.action === '*' || grant.action === action) && resourceMatches(grant.resource, resource)) {
        return grant;
      }
    }

    return null;
  }

  private auditExpiredGrant(grant: CapabilityGrant): void {
    if (this.expiredGrantAuditIds.has(grant.id)) return;

    this.expiredGrantAuditIds.add(grant.id);
    this.recordLifecycleAudit('capability.expire', 'expire', 'grant-expired', grant.resource, {
      grantId: grant.id,
      action: grant.action,
      scope: grant.scope,
      expiresAt: grant.expiresAt,
    });
  }

  private recordAudit(originalRequest: CapabilityRequest, decision: CapabilityDecision): void {
    const entry: CapabilityAuditEntry = {
      id: randomUUID(),
      timestamp: this.now(),
      isoTime: new Date(this.now()).toISOString(),
      action: decision.action,
      decision: decision.allowed ? 'allow' : 'deny',
      reason: decision.reason,
      requester: originalRequest.requester,
      resource: redactText(decision.resource),
      grantId: decision.allowed ? decision.grantId : undefined,
      success: decision.allowed,
      details: redactValue({
        reason: originalRequest.reason,
        metadata: originalRequest.metadata,
        resolvedPath: decision.resolvedPath,
      }) as Record<string, unknown>,
    };
    this.pushAudit(entry);
  }

  private recordLifecycleAudit(
    action: CapabilityAuditEntry['action'],
    decision: CapabilityAuditDecision,
    reason: string,
    resource: string,
    details: Record<string, unknown>,
  ): void {
    this.pushAudit({
      id: randomUUID(),
      timestamp: this.now(),
      isoTime: new Date(this.now()).toISOString(),
      action,
      decision,
      reason,
      resource: redactText(resource),
      grantId: typeof details['grantId'] === 'string' ? details['grantId'] : undefined,
      success: true,
      details: redactValue(details) as Record<string, unknown>,
    });
  }

  private pushAudit(entry: CapabilityAuditEntry): void {
    this.auditEntries.push(entry);
    if (this.auditEntries.length > MAX_AUDIT_ENTRIES) {
      this.auditEntries.splice(0, this.auditEntries.length - MAX_AUDIT_ENTRIES);
    }
    if (this.auditPath) {
      try {
        mkdirSync(dirname(this.auditPath), { recursive: true });
        appendFileSync(this.auditPath, `${JSON.stringify(entry)}\n`, 'utf8');
      } catch {
        // Audit persistence failures must not interrupt capability enforcement.
      }
    }
    this.auditSink?.(entry);
  }
}

function stripProtocol(resource: string, protocol: 'file://' | 'exec://'): string {
  return resource.startsWith(protocol) ? resource.slice(protocol.length) : resource;
}

function resourceMatches(pattern: string, resource: string): boolean {
  if (pattern === '*' || pattern === resource) return true;
  if (pattern.endsWith('://*')) {
    return resource.startsWith(pattern.slice(0, -1));
  }
  if (pattern.endsWith('/**')) {
    return resource.startsWith(pattern.slice(0, -3));
  }
  return false;
}

function redactValue(value: unknown): unknown {
  if (typeof value === 'string') return redactText(value);
  if (Array.isArray(value)) return value.map(redactValue);
  if (!value || typeof value !== 'object') return value;

  const redacted: Record<string, unknown> = {};
  for (const [key, item] of Object.entries(value)) {
    redacted[key] = isSensitiveKey(key) ? '[REDACTED]' : redactValue(item);
  }
  return redacted;
}

function redactText(value: string): string {
  return value
    .replace(/(authorization\s*[:=]\s*bearer\s+)[^"'\s&]+/gi, '$1[REDACTED]')
    .replace(/(bearer\s+)[^"'\s&]+/gi, '$1[REDACTED]')
    .replace(SENSITIVE_KEY_PATTERN, '$1[REDACTED]');
}

function isSensitiveKey(key: string): boolean {
  const normalized = key.toLowerCase().replace(/[-_]/g, '');
  return (
    normalized.includes('token') ||
    normalized.includes('secret') ||
    normalized.includes('password') ||
    normalized.includes('apikey') ||
    normalized.includes('authorization') ||
    normalized.includes('credential')
  );
}

function resolveAuditPath(auditPath: string, workspaceRoot: string): string {
  if (isAbsolute(auditPath)) return auditPath;
  return resolve(workspaceRoot || process.cwd(), auditPath);
}
