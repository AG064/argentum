"use strict";
/**
 * Argentum Policy Engine
 *
 * Enterprise-grade policy engine with:
 * - Default-deny enforcement
 * - Glob pattern matching for resources
 * - Human-in-the-loop approval workflow
 * - SQLite-backed audit log
 * - Blueprint-driven configuration
 * - Real-time policy hot-reload
 */
var __importDefault = (this && this.__importDefault) || function (mod) {
    return (mod && mod.__esModule) ? mod : { "default": mod };
};
Object.defineProperty(exports, "__esModule", { value: true });
exports.PolicyEngine = void 0;
exports.getPolicyEngine = getPolicyEngine;
exports.resetPolicyEngine = resetPolicyEngine;
const crypto_1 = require("crypto");
const fs_1 = require("fs");
const path_1 = require("path");
const better_sqlite3_1 = __importDefault(require("better-sqlite3"));
const logger_1 = require("../../core/logger");
// ─── Constants ────────────────────────────────────────────────────────────────
const AUDIT_TABLE = `
  CREATE TABLE IF NOT EXISTS security_audit (
    id TEXT PRIMARY KEY,
    timestamp INTEGER NOT NULL,
    iso_time TEXT NOT NULL,
    action TEXT NOT NULL,
    severity TEXT NOT NULL,
    actor TEXT,
    resource TEXT,
    details TEXT NOT NULL,
    decision TEXT,
    policy_id TEXT,
    approval_id TEXT,
    session_id TEXT,
    ip TEXT,
    success INTEGER NOT NULL,
    duration_ms INTEGER
  );
  CREATE INDEX IF NOT EXISTS idx_audit_timestamp ON security_audit(timestamp DESC);
  CREATE INDEX IF NOT EXISTS idx_audit_actor ON security_audit(actor);
  CREATE INDEX IF NOT EXISTS idx_audit_action ON security_audit(action);
`;
const POLICIES_TABLE = `
  CREATE TABLE IF NOT EXISTS security_policies (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    description TEXT,
    effect TEXT NOT NULL,
    resource TEXT NOT NULL,
    action TEXT NOT NULL,
    priority INTEGER NOT NULL DEFAULT 0,
    enabled INTEGER NOT NULL DEFAULT 1,
    conditions TEXT,
    requires_approval INTEGER NOT NULL DEFAULT 0,
    approval_risk TEXT,
    audit_level TEXT DEFAULT 'info',
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
  );
`;
// ─── Glob Matching ───────────────────────────────────────────────────────────
function globMatch(pattern, value) {
    // Normalize paths
    const normPattern = pattern.replace(/\\/g, '/').replace(/\/+/g, '/');
    const normValue = value.replace(/\\/g, '/').replace(/\/+/g, '/');
    // Exact match
    if (normPattern === normValue)
        return true;
    // Wildcard **
    if (normPattern === '**')
        return true;
    // Glob patterns: ** matches anything including /
    if (normPattern.includes('**')) {
        const [prefix, suffix] = normPattern.split('**');
        if (prefix) {
            if (!normValue.startsWith(prefix))
                return false;
        }
        if (suffix) {
            if (!normValue.endsWith(suffix) && !normValue.includes(suffix.slice(1)))
                return false;
        }
        return true;
    }
    // Glob patterns: * matches anything except /
    if (normPattern.includes('*')) {
        const parts = normPattern.split('*');
        let pos = 0;
        for (let i = 0; i < parts.length; i++) {
            const part = parts[i];
            if (i === 0) {
                // First part must match at start
                if (!normValue.startsWith(part))
                    return false;
                pos = part.length;
            }
            else if (i === parts.length - 1) {
                // Last part must match at end
                if (!normValue.endsWith(part))
                    return false;
            }
            else {
                // Middle parts - find next occurrence
                const idx = normValue.indexOf(part, pos);
                if (idx === -1)
                    return false;
                pos = idx + part.length;
            }
        }
        return true;
    }
    return false;
}
function resourceMatches(pattern, resource) {
    // Normalize: ensure both have protocol prefix
    const normPattern = pattern.endsWith('/') ? pattern.slice(0, -1) : pattern;
    const normResource = resource.endsWith('/') ? resource.slice(0, -1) : resource;
    return globMatch(normPattern, normResource);
}
// ─── Condition Evaluation ─────────────────────────────────────────────────────
function evaluateConditions(conditions, context) {
    if (!conditions || conditions.length === 0)
        return true;
    return conditions.every((cond) => {
        const value = context[cond.field];
        switch (cond.operator) {
            case 'equals':
                return value === cond.value;
            case 'not_equals':
                return value !== cond.value;
            case 'contains':
                return typeof value === 'string' && String(value).includes(String(cond.value));
            case 'matches':
                if (typeof value !== 'string')
                    return false;
                try {
                    return new RegExp(String(cond.value)).test(value);
                }
                catch {
                    return false;
                }
            case 'in':
                return Array.isArray(cond.value) && cond.value.includes(value);
            case 'greater_than':
                return typeof value === 'number' && value > Number(cond.value);
            case 'less_than':
                return typeof value === 'number' && value < Number(cond.value);
            default:
                return false;
        }
    });
}
function createApprovalStore() {
    return { requests: new Map() };
}
function approvalToSQL(ar) {
    return {
        id: ar.id,
        agent_id: ar.agentId,
        agent_name: ar.agentName ?? '',
        action_type: ar.action.type,
        action_resource: ar.action.resource,
        action_metadata: JSON.stringify(ar.action.metadata ?? {}),
        risk: ar.risk,
        details_what: ar.details.what,
        details_why: ar.details.why,
        details_consequences: ar.details.consequences,
        details_alternatives: JSON.stringify(ar.details.alternatives ?? []),
        status: ar.status,
        requested_at: ar.requestedAt,
        expires_at: ar.expiresAt,
        responded_at: ar.respondedAt ?? 0,
        responded_by: ar.respondedBy ?? '',
        notes: ar.notes ?? '',
    };
}
function approvalFromSQL(row) {
    return {
        id: row.id,
        agentId: row.agent_id,
        agentName: row.agent_name || undefined,
        action: {
            id: row.action_metadata || (0, crypto_1.randomUUID)(),
            agentId: row.agent_id,
            type: row.action_type || 'read',
            resource: row.action_resource || 'file://unknown',
            metadata: JSON.parse(row.action_metadata || '{}'),
            timestamp: row.requested_at,
        },
        risk: row.risk || 'medium',
        details: {
            what: row.details_what || '',
            why: row.details_why || '',
            consequences: row.details_consequences || '',
            alternatives: JSON.parse(row.details_alternatives || '[]'),
        },
        status: row.status || 'pending',
        requestedAt: row.requested_at,
        expiresAt: row.expires_at,
        respondedAt: row.responded_at,
        respondedBy: row.responded_by || undefined,
        notes: row.notes || undefined,
    };
}
const APPROVALS_TABLE = `
  CREATE TABLE IF NOT EXISTS security_approvals (
    id TEXT PRIMARY KEY,
    agent_id TEXT NOT NULL,
    agent_name TEXT,
    action_type TEXT NOT NULL,
    action_resource TEXT NOT NULL,
    action_metadata TEXT,
    risk TEXT NOT NULL,
    details_what TEXT NOT NULL,
    details_why TEXT NOT NULL,
    details_consequences TEXT NOT NULL,
    details_alternatives TEXT,
    status TEXT NOT NULL DEFAULT 'pending',
    requested_at INTEGER NOT NULL,
    expires_at INTEGER NOT NULL,
    responded_at INTEGER,
    responded_by TEXT,
    notes TEXT
  );
  CREATE INDEX IF NOT EXISTS idx_approvals_status ON security_approvals(status);
  CREATE INDEX IF NOT EXISTS idx_approvals_expires ON security_approvals(expires_at);
`;
// ─── Policy Engine ────────────────────────────────────────────────────────────
class PolicyEngine {
    policies = new Map();
    db = null;
    auditFilePath = null;
    approvalStore;
    logger;
    approvalExpirySeconds = 300; // 5 min default
    blueprint = null;
    constructor(dbPath) {
        this.logger = (0, logger_1.createLogger)().child({ feature: 'policy-engine' });
        this.approvalStore = createApprovalStore();
        if (dbPath) {
            this.initDatabase(dbPath);
        }
    }
    initDatabase(dbPath) {
        const resolved = (0, path_1.resolve)(dbPath);
        const dir = (0, path_1.dirname)(resolved);
        if (!(0, fs_1.existsSync)(dir)) {
            (0, fs_1.mkdirSync)(dir, { recursive: true });
        }
        this.db = new better_sqlite3_1.default(resolved);
        this.db.pragma('journal_mode = WAL');
        this.db.pragma('foreign_keys = ON');
        // Create tables
        this.db.exec(AUDIT_TABLE);
        this.db.exec(POLICIES_TABLE);
        this.db.exec(APPROVALS_TABLE);
        // Load existing policies
        this.loadPoliciesFromDB();
        // Clean up expired approvals
        this.cleanupExpiredApprovals();
        this.logger.info(`Policy engine initialized with database: ${resolved}`);
    }
    loadPoliciesFromDB() {
        if (!this.db)
            return;
        const rows = this.db.prepare('SELECT * FROM security_policies').all();
        for (const row of rows) {
            const policy = {
                id: row.id,
                name: row.name,
                description: row.description || undefined,
                effect: row.effect,
                resource: row.resource,
                action: row.action,
                priority: row.priority,
                enabled: Boolean(row.enabled),
                conditions: row.conditions ? JSON.parse(row.conditions) : undefined,
                requiresApproval: Boolean(row.requires_approval),
                approvalRisk: row.approval_risk || undefined,
                auditLevel: row.audit_level || 'info',
                createdAt: row.created_at,
                updatedAt: row.updated_at,
            };
            this.policies.set(policy.id, policy);
        }
        this.logger.info(`Loaded ${this.policies.size} policies from database`);
    }
    cleanupExpiredApprovals() {
        if (!this.db)
            return;
        const now = Date.now();
        const expired = this.db
            .prepare("UPDATE security_approvals SET status = 'expired' WHERE status = 'pending' AND expires_at < ?")
            .run(now);
        if (expired.changes > 0) {
            this.logger.info(`Cleaned up ${expired.changes} expired approvals`);
        }
    }
    /**
     * Load policies and config from a blueprint YAML file.
     */
    loadBlueprint(blueprint) {
        this.blueprint = blueprint;
        // Load policies from blueprint
        if (blueprint.policies) {
            for (const bp of blueprint.policies) {
                const existing = Array.from(this.policies.values()).find((p) => p.name === bp.name);
                const policy = {
                    id: existing?.id ?? (0, crypto_1.randomUUID)(),
                    name: bp.name,
                    effect: bp.effect,
                    resource: bp.resource,
                    action: bp.action,
                    priority: bp.priority ?? 0,
                    enabled: true,
                    conditions: bp.conditions,
                    requiresApproval: bp.requiresApproval ?? false,
                    approvalRisk: bp.approvalRisk ?? 'medium',
                    auditLevel: bp.effect === 'deny' ? 'warning' : 'info',
                    createdAt: existing?.createdAt ?? Date.now(),
                    updatedAt: Date.now(),
                };
                this.policies.set(policy.id, policy);
                this.savePolicyToDB(policy);
            }
        }
        // Configure approval expiry
        if (blueprint.approval?.autoExpireSeconds) {
            this.approvalExpirySeconds = blueprint.approval.autoExpireSeconds;
        }
        this.logger.info('Blueprint loaded', {
            policiesCount: blueprint.policies?.length ?? 0,
            sandboxEnabled: blueprint.sandbox?.enabled ?? false,
        });
    }
    savePolicyToDB(policy) {
        if (!this.db)
            return;
        const stmt = this.db.prepare(`
      INSERT OR REPLACE INTO security_policies 
      (id, name, description, effect, resource, action, priority, enabled, conditions, requires_approval, approval_risk, audit_level, created_at, updated_at)
      VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
    `);
        stmt.run(policy.id, policy.name, policy.description ?? null, policy.effect, policy.resource, policy.action, policy.priority, policy.enabled ? 1 : 0, policy.conditions ? JSON.stringify(policy.conditions) : null, policy.requiresApproval ? 1 : 0, policy.approvalRisk ?? null, policy.auditLevel ?? 'info', policy.createdAt, policy.updatedAt);
    }
    /**
     * Evaluate an action against all policies.
     * Default-deny: if no policy matches → DENY.
     */
    evaluate(action) {
        // Build context for condition evaluation
        const context = {
            action: action.type,
            resource: action.resource,
            agentId: action.agentId,
            ...action.metadata,
        };
        // Sort policies by priority (highest first)
        const sortedPolicies = Array.from(this.policies.values())
            .filter((p) => p.enabled)
            .sort((a, b) => b.priority - a.priority);
        for (const policy of sortedPolicies) {
            // Check resource match
            if (!resourceMatches(policy.resource, action.resource))
                continue;
            // Check action match
            if (policy.action !== '*' && policy.action !== action.type)
                continue;
            // Check conditions
            if (!evaluateConditions(policy.conditions ?? [], context))
                continue;
            // Policy matched!
            const decision = {
                allowed: policy.effect === 'allow',
                policy,
                reason: `Matched policy: ${policy.name}`,
            };
            // Log decision
            this.logAudit({
                action: policy.effect === 'deny'
                    ? 'policy.deny'
                    : policy.effect === 'approve'
                        ? 'policy.approve'
                        : 'policy.allow',
                severity: policy.effect === 'deny' ? 'warning' : 'info',
                actor: action.agentId,
                resource: action.resource,
                details: { actionType: action.type, matchedPolicy: policy.name },
                decision: policy.effect,
                policyId: policy.id,
                success: policy.effect !== 'deny',
            });
            return decision;
        }
        // Default-deny: no policy matched
        const denyDecision = {
            allowed: false,
            reason: 'Default deny: no matching policy found',
        };
        this.logAudit({
            action: 'policy.deny',
            severity: 'warning',
            actor: action.agentId,
            resource: action.resource,
            details: { actionType: action.type, reason: 'default-deny' },
            decision: 'deny',
            success: false,
        });
        return denyDecision;
    }
    /**
     * Check if action requires human approval.
     */
    requiresApproval(action) {
        // First check if policy requires approval
        const decision = this.evaluate(action);
        if (decision.allowed && decision.policy?.requiresApproval) {
            const risk = decision.policy.approvalRisk ?? 'medium';
            return this.createApprovalRequest(action, risk, {
                what: `Agent ${action.agentId} wants to perform: ${action.type} on ${action.resource}`,
                why: 'Required by security policy',
                consequences: `This action will be ${decision.policy.effect}ed by policy: ${decision.policy.name}`,
                alternatives: [
                    'Modify the policy to allow/deny this action',
                    'Request temporary access via admin',
                ],
            });
        }
        // Check blueprint approval rules for critical actions
        if (this.blueprint?.approval?.criticalActions) {
            for (const pattern of this.blueprint.approval.criticalActions) {
                if (resourceMatches(pattern, action.resource)) {
                    return this.createApprovalRequest(action, 'critical', {
                        what: `Agent ${action.agentId} wants to perform critical action: ${action.type} on ${action.resource}`,
                        why: 'Action matches critical actions pattern in blueprint',
                        consequences: 'This is a high-risk operation that requires human authorization',
                        alternatives: ['Use a safer alternative', 'Escalate to admin'],
                    });
                }
            }
        }
        return null;
    }
    createApprovalRequest(action, risk, details) {
        const now = Date.now();
        const request = {
            id: (0, crypto_1.randomUUID)(),
            agentId: action.agentId,
            action,
            risk,
            details,
            status: 'pending',
            requestedAt: now,
            expiresAt: now + this.approvalExpirySeconds * 1000,
        };
        // Store in memory
        this.approvalStore.requests.set(request.id, request);
        // Persist to DB
        if (this.db) {
            const stmt = this.db.prepare(`
        INSERT INTO security_approvals 
        (id, agent_id, agent_name, action_type, action_resource, action_metadata, risk, details_what, details_why, details_consequences, details_alternatives, status, requested_at, expires_at)
        VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
      `);
            const sqlData = approvalToSQL(request);
            stmt.run(sqlData.id, sqlData.agent_id, sqlData.agent_name ?? null, sqlData.action_type, sqlData.action_resource, sqlData.action_metadata, sqlData.risk, sqlData.details_what, sqlData.details_why, sqlData.details_consequences, sqlData.details_alternatives, sqlData.status, sqlData.requested_at, sqlData.expires_at);
        }
        this.logAudit({
            action: 'approval.request',
            severity: risk === 'critical' ? 'error' : 'warning',
            actor: action.agentId,
            resource: action.resource,
            details: { risk, actionType: action.type },
            approvalId: request.id,
            success: true,
        });
        return request;
    }
    /**
     * Approve a pending request.
     */
    approve(requestId, userId, notes) {
        const request = this.approvalStore.requests.get(requestId);
        if (!request || request.status !== 'pending')
            return false;
        if (Date.now() > request.expiresAt) {
            request.status = 'expired';
            this.updateApprovalInDB(request);
            return false;
        }
        request.status = 'approved';
        request.respondedAt = Date.now();
        request.respondedBy = userId;
        request.notes = notes;
        this.updateApprovalInDB(request);
        this.logAudit({
            action: 'approval.approve',
            severity: 'info',
            actor: userId,
            resource: request.action.resource,
            details: { requestId, risk: request.risk },
            approvalId: requestId,
            success: true,
        });
        return true;
    }
    /**
     * Deny a pending request.
     */
    deny(requestId, userId, notes) {
        const request = this.approvalStore.requests.get(requestId);
        if (!request || request.status !== 'pending')
            return false;
        request.status = 'denied';
        request.respondedAt = Date.now();
        request.respondedBy = userId;
        request.notes = notes;
        this.updateApprovalInDB(request);
        this.logAudit({
            action: 'approval.deny',
            severity: 'warning',
            actor: userId,
            resource: request.action.resource,
            details: { requestId, risk: request.risk, notes },
            approvalId: requestId,
            success: true,
        });
        return true;
    }
    updateApprovalInDB(request) {
        if (!this.db)
            return;
        const stmt = this.db.prepare(`
      UPDATE security_approvals 
      SET status = ?, responded_at = ?, responded_by = ?, notes = ?
      WHERE id = ?
    `);
        stmt.run(request.status, request.respondedAt ?? null, request.respondedBy ?? null, request.notes ?? null, request.id);
    }
    /**
     * Get all pending approval requests.
     */
    getPendingApprovals() {
        this.cleanupExpiredApprovals();
        // Sync from DB to memory
        if (this.db) {
            const rows = this.db
                .prepare("SELECT * FROM security_approvals WHERE status = 'pending' ORDER BY requested_at DESC")
                .all();
            for (const row of rows) {
                const request = approvalFromSQL(row);
                this.approvalStore.requests.set(request.id, request);
            }
        }
        return Array.from(this.approvalStore.requests.values()).filter((r) => r.status === 'pending');
    }
    /**
     * Get approval by ID.
     */
    getApproval(id) {
        return this.approvalStore.requests.get(id);
    }
    // ─── Policy CRUD ──────────────────────────────────────────────────────────
    addPolicy(policy) {
        const now = Date.now();
        const newPolicy = {
            ...policy,
            id: (0, crypto_1.randomUUID)(),
            createdAt: now,
            updatedAt: now,
        };
        this.policies.set(newPolicy.id, newPolicy);
        this.savePolicyToDB(newPolicy);
        return newPolicy;
    }
    updatePolicy(id, updates) {
        const policy = this.policies.get(id);
        if (!policy)
            return false;
        const updated = {
            ...policy,
            ...updates,
            id: policy.id,
            createdAt: policy.createdAt,
            updatedAt: Date.now(),
        };
        this.policies.set(id, updated);
        this.savePolicyToDB(updated);
        return true;
    }
    removePolicy(id) {
        const existed = this.policies.delete(id);
        if (existed && this.db) {
            this.db.prepare('DELETE FROM security_policies WHERE id = ?').run(id);
        }
        return existed;
    }
    getPolicies() {
        return Array.from(this.policies.values()).sort((a, b) => b.priority - a.priority);
    }
    setPolicyEnabled(id, enabled) {
        return this.updatePolicy(id, { enabled });
    }
    // ─── Audit Logging ────────────────────────────────────────────────────────
    logAudit(entry) {
        const fullEntry = {
            ...entry,
            id: (0, crypto_1.randomUUID)(),
            timestamp: Date.now(),
            isoTime: new Date().toISOString(),
        };
        // Write to SQLite
        if (this.db) {
            const stmt = this.db.prepare(`
        INSERT INTO security_audit 
        (id, timestamp, iso_time, action, severity, actor, resource, details, decision, policy_id, approval_id, session_id, ip, success, duration_ms)
        VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
      `);
            stmt.run(fullEntry.id, fullEntry.timestamp, fullEntry.isoTime, fullEntry.action, fullEntry.severity, fullEntry.actor ?? null, fullEntry.resource ?? null, JSON.stringify(fullEntry.details), fullEntry.decision ?? null, fullEntry.policyId ?? null, fullEntry.approvalId ?? null, fullEntry.sessionId ?? null, fullEntry.ip ?? null, fullEntry.success ? 1 : 0, fullEntry.durationMs ?? null);
        }
        // Write to file if configured
        if (this.auditFilePath) {
            try {
                (0, fs_1.appendFileSync)(this.auditFilePath, `${JSON.stringify(fullEntry)}\n`, 'utf-8');
            }
            catch (err) {
                this.logger.error('Failed to write audit to file', { error: String(err) });
            }
        }
        // Emit to logger
        const logMethod = fullEntry.severity === 'error' || fullEntry.severity === 'critical'
            ? this.logger.error.bind(this.logger)
            : fullEntry.severity === 'warning'
                ? this.logger.warn.bind(this.logger)
                : this.logger.info.bind(this.logger);
        logMethod(`Audit: ${fullEntry.action}`, fullEntry.details);
    }
    enableAuditFile(filePath) {
        this.auditFilePath = (0, path_1.resolve)(filePath);
        const dir = (0, path_1.dirname)(this.auditFilePath);
        if (!(0, fs_1.existsSync)(dir)) {
            (0, fs_1.mkdirSync)(dir, { recursive: true });
        }
        this.logger.info(`File-based audit logging enabled: ${this.auditFilePath}`);
    }
    queryAudit(filter) {
        if (!this.db)
            return [];
        let query = 'SELECT * FROM security_audit WHERE 1=1';
        const params = [];
        if (filter?.since) {
            query += ' AND timestamp >= ?';
            params.push(filter.since);
        }
        if (filter?.until) {
            query += ' AND timestamp <= ?';
            params.push(filter.until);
        }
        if (filter?.actor) {
            query += ' AND actor = ?';
            params.push(filter.actor);
        }
        if (filter?.action) {
            query += ' AND action = ?';
            params.push(filter.action);
        }
        query += ' ORDER BY timestamp DESC LIMIT ?';
        params.push(filter?.limit ?? 100);
        const rows = this.db.prepare(query).all(...params);
        return rows.map((row) => ({
            id: row.id,
            timestamp: row.timestamp,
            isoTime: row.iso_time,
            action: row.action,
            severity: row.severity,
            actor: row.actor,
            resource: row.resource,
            details: JSON.parse(row.details),
            decision: row.decision,
            policyId: row.policy_id,
            approvalId: row.approval_id,
            sessionId: row.session_id,
            ip: row.ip,
            success: Boolean(row.success),
            durationMs: row.duration_ms,
        }));
    }
    // ─── Stats ────────────────────────────────────────────────────────────────
    getStats() {
        const now = Date.now();
        const fiveMinFromNow = now + 5 * 60 * 1000;
        const credentialsExpiringSoon = 0;
        return {
            policiesActive: Array.from(this.policies.values()).filter((p) => p.enabled).length,
            policiesTotal: this.policies.size,
            credentialsTotal: 0, // managed by CredentialManager
            credentialsExpiringSoon,
            approvalsPending: this.getPendingApprovals().length,
            approvalsTotal: this.approvalStore.requests.size,
            auditEntriesTotal: this.db
                ? this.db.prepare('SELECT COUNT(*) as c FROM security_audit').get().c
                : 0,
            sandboxExecutionsTotal: 0, // managed by SandboxExecutor
            sandboxBlockedTotal: 0,
            threatsDetected: this.db
                ? this.db
                    .prepare("SELECT COUNT(*) as c FROM security_audit WHERE severity IN ('warning','error','critical') AND timestamp > ?")
                    .get(now - 86400000).c
                : 0,
            uptime: process.uptime(),
        };
    }
    close() {
        if (this.db) {
            this.db.close();
            this.db = null;
        }
    }
}
exports.PolicyEngine = PolicyEngine;
// ─── Singleton ────────────────────────────────────────────────────────────────
let instance = null;
function getPolicyEngine(dbPath) {
    if (!instance) {
        instance = new PolicyEngine(dbPath);
    }
    return instance;
}
function resetPolicyEngine() {
    if (instance) {
        instance.close();
        instance = null;
    }
}
//# sourceMappingURL=index.js.map