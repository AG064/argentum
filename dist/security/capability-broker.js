"use strict";
Object.defineProperty(exports, "__esModule", { value: true });
exports.createCapabilityBroker = createCapabilityBroker;
const crypto_1 = require("crypto");
const fs_1 = require("fs");
const path_1 = require("path");
const workspace_boundary_1 = require("./workspace-boundary");
const FILE_ACTIONS = new Set(['file.read', 'file.write', 'file.delete']);
const SENSITIVE_KEY_PATTERN = /((?:api[_-]?key|authorization|bearer|password|secret|token)\s*[:=]\s*)([^\s"'&]+)/gi;
function createCapabilityBroker(options) {
    return new DefaultCapabilityBroker(options);
}
class DefaultCapabilityBroker {
    workspaceRoot;
    workspaceBoundary;
    now;
    auditSink;
    auditPath;
    grants = new Map();
    auditEntries = [];
    expiredGrantAuditIds = new Set();
    constructor(options) {
        this.workspaceBoundary = (0, workspace_boundary_1.createWorkspaceBoundary)(options.workspaceRoot);
        this.workspaceRoot = this.workspaceBoundary.workspaceRoot;
        this.now = options.now ?? Date.now;
        this.auditSink = options.auditSink;
        this.auditPath = options.auditPath
            ? resolveAuditPath(options.auditPath, this.workspaceRoot)
            : undefined;
    }
    authorize(request) {
        const normalized = this.normalizeRequest(request);
        if (!normalized) {
            const decision = {
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
                const decision = {
                    allowed: true,
                    action: normalized.action,
                    reason: 'inside-workspace',
                    resource: normalized.resource,
                    resolvedPath: normalized.resolvedPath,
                };
                this.recordAudit(request, decision);
                return decision;
            }
            if (normalized.workspaceReason === 'invalid-resource' ||
                normalized.workspaceReason === 'workspace-not-configured') {
                const decision = {
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
            const decision = {
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
        const decision = {
            allowed: false,
            action: normalized.action,
            reason: FILE_ACTIONS.has(normalized.action) ? 'outside-workspace' : 'no-active-grant',
            resource: normalized.resource,
            resolvedPath: normalized.resolvedPath,
        };
        this.recordAudit(request, decision);
        return decision;
    }
    grant(input) {
        const createdAt = this.now();
        const grant = {
            id: (0, crypto_1.randomUUID)(),
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
    revokeGrant(grantId, revokedBy) {
        const grant = this.grants.get(grantId);
        if (!grant || grant.revokedAt !== undefined)
            return false;
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
    getGrants(includeRevoked = false) {
        return Array.from(this.grants.values())
            .filter((grant) => includeRevoked || grant.revokedAt === undefined)
            .map((grant) => ({ ...grant }));
    }
    getAuditEntries(limit = 100) {
        return this.auditEntries.slice(-limit).map((entry) => ({ ...entry }));
    }
    normalizeRequest(request) {
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
    normalizeGrantResource(action, resource) {
        if (action === '*' || resource === '*')
            return resource;
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
    findActiveGrant(action, resource) {
        for (const grant of this.grants.values()) {
            if (grant.revokedAt !== undefined)
                continue;
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
    auditExpiredGrant(grant) {
        if (this.expiredGrantAuditIds.has(grant.id))
            return;
        this.expiredGrantAuditIds.add(grant.id);
        this.recordLifecycleAudit('capability.expire', 'expire', 'grant-expired', grant.resource, {
            grantId: grant.id,
            action: grant.action,
            scope: grant.scope,
            expiresAt: grant.expiresAt,
        });
    }
    recordAudit(originalRequest, decision) {
        const entry = {
            id: (0, crypto_1.randomUUID)(),
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
            }),
        };
        this.pushAudit(entry);
    }
    recordLifecycleAudit(action, decision, reason, resource, details) {
        this.pushAudit({
            id: (0, crypto_1.randomUUID)(),
            timestamp: this.now(),
            isoTime: new Date(this.now()).toISOString(),
            action,
            decision,
            reason,
            resource: redactText(resource),
            grantId: typeof details['grantId'] === 'string' ? details['grantId'] : undefined,
            success: true,
            details: redactValue(details),
        });
    }
    pushAudit(entry) {
        this.auditEntries.push(entry);
        if (this.auditPath) {
            (0, fs_1.mkdirSync)((0, path_1.dirname)(this.auditPath), { recursive: true });
            (0, fs_1.appendFileSync)(this.auditPath, `${JSON.stringify(entry)}\n`, 'utf8');
        }
        this.auditSink?.(entry);
    }
}
function stripProtocol(resource, protocol) {
    return resource.startsWith(protocol) ? resource.slice(protocol.length) : resource;
}
function resourceMatches(pattern, resource) {
    if (pattern === '*' || pattern === resource)
        return true;
    if (pattern.endsWith('://*')) {
        return resource.startsWith(pattern.slice(0, -1));
    }
    if (pattern.endsWith('/**')) {
        return resource.startsWith(pattern.slice(0, -3));
    }
    return false;
}
function redactValue(value) {
    if (typeof value === 'string')
        return redactText(value);
    if (Array.isArray(value))
        return value.map(redactValue);
    if (!value || typeof value !== 'object')
        return value;
    const redacted = {};
    for (const [key, item] of Object.entries(value)) {
        redacted[key] = isSensitiveKey(key) ? '[REDACTED]' : redactValue(item);
    }
    return redacted;
}
function redactText(value) {
    return value
        .replace(/(authorization\s*[:=]\s*bearer\s+)[^"'\s&]+/gi, '$1[REDACTED]')
        .replace(/(bearer\s+)[^"'\s&]+/gi, '$1[REDACTED]')
        .replace(SENSITIVE_KEY_PATTERN, '$1[REDACTED]');
}
function isSensitiveKey(key) {
    const normalized = key.toLowerCase().replace(/[-_]/g, '');
    return (normalized.includes('token') ||
        normalized.includes('secret') ||
        normalized.includes('password') ||
        normalized.includes('apikey') ||
        normalized.includes('authorization') ||
        normalized.includes('credential'));
}
function resolveAuditPath(auditPath, workspaceRoot) {
    if ((0, path_1.isAbsolute)(auditPath))
        return auditPath;
    return (0, path_1.resolve)(workspaceRoot || process.cwd(), auditPath);
}
//# sourceMappingURL=capability-broker.js.map