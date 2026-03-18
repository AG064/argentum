"use strict";
/**
 * AG-Claw Policy Engine
 *
 * YAML-based security policies loaded from config/security-policy.yaml.
 * Evaluates actions against rules by role/agent, enforces rate limits
 * per user/session, and logs all decisions to an audit trail.
 */
Object.defineProperty(exports, "__esModule", { value: true });
exports.PolicyEngine = void 0;
exports.getPolicyEngine = getPolicyEngine;
exports.resetPolicyEngine = resetPolicyEngine;
const fs_1 = require("fs");
const path_1 = require("path");
const yaml_1 = require("yaml");
const logger_1 = require("../core/logger");
// ─── Engine ───────────────────────────────────────────────────
class PolicyEngine {
    constructor() {
        this.rules = new Map();
        this.rateLimits = new Map();
        this.rateLimitState = new Map();
        this.auditLog = [];
        this.auditFilePath = null;
        this.maxAuditLogSize = 10000;
        this.logger = (0, logger_1.createLogger)().child({ feature: 'policy-engine' });
    }
    /**
     * Load policies from the YAML config file (config/security-policy.yaml).
     *
     * Expected structure:
     *   rules:
     *     - id: rule_admin
     *       name: Admin Access
     *       action: allow
     *       priority: 100
     *       enabled: true
     *       conditions:
     *         - field: role
     *           operator: equals
     *           value: admin
     *   rateLimits:
     *     api_calls:
     *       windowMs: 60000
     *       maxRequests: 60
     *       keyField: user
     */
    loadFromFile(filePath) {
        const fullPath = (0, path_1.resolve)(filePath);
        if (!(0, fs_1.existsSync)(fullPath)) {
            this.logger.warn(`Policy file not found: ${fullPath}`);
            return;
        }
        try {
            const raw = (0, fs_1.readFileSync)(fullPath, 'utf-8');
            const config = (0, yaml_1.parse)(raw);
            // Load rules
            if (config.rules && Array.isArray(config.rules)) {
                for (const rule of config.rules) {
                    this.addRule(rule);
                }
                this.logger.info(`Loaded ${config.rules.length} policy rules`);
            }
            // Load rate limits
            if (config.rateLimits) {
                for (const [name, limit] of Object.entries(config.rateLimits)) {
                    this.rateLimits.set(name, limit);
                }
                this.logger.info(`Loaded ${Object.keys(config.rateLimits).length} rate limit configs`);
            }
        }
        catch (err) {
            this.logger.error(`Failed to load policy file: ${fullPath}`, {
                error: err instanceof Error ? err.message : String(err),
            });
        }
    }
    /**
     * Add a policy rule programmatically.
     */
    addRule(rule) {
        this.rules.set(rule.id, rule);
    }
    /**
     * Remove a rule by ID.
     */
    removeRule(id) {
        return this.rules.delete(id);
    }
    /**
     * Evaluate an action context against all loaded policies.
     *
     * Evaluation order:
     * 1. Rate limits (if user/session specified)
     * 2. Rules sorted by priority (highest first)
     * 3. First matching rule wins
     * 4. Default: allow
     */
    evaluate(context) {
        // 1. Check rate limits
        if (context.user || context.sessionId) {
            const rateLimitResult = this.checkRateLimits(context);
            if (rateLimitResult) {
                this.recordAudit(context, rateLimitResult);
                return rateLimitResult;
            }
        }
        // 2. Sort rules by priority (highest first)
        const sortedRules = Array.from(this.rules.values())
            .filter(r => r.enabled)
            .sort((a, b) => b.priority - a.priority);
        // 3. Find first matching rule
        for (const rule of sortedRules) {
            if (this.evaluateConditions(rule.conditions, context)) {
                const evaluation = {
                    allowed: rule.action === 'allow' || rule.action === 'audit',
                    action: rule.action,
                    matchedRule: rule,
                    reason: `Matched rule: ${rule.name}`,
                    shouldAudit: (rule.auditLevel ?? 'none') !== 'none',
                };
                this.recordAudit(context, evaluation);
                return evaluation;
            }
        }
        // 4. Default: allow
        const defaultEval = {
            allowed: true,
            action: 'allow',
            reason: 'No matching rule, default allow',
            shouldAudit: false,
        };
        return defaultEval;
    }
    /**
     * Quick helper: is this action allowed?
     */
    isAllowed(context) {
        return this.evaluate(context).allowed;
    }
    /**
     * Get the audit log.
     */
    getAuditLog(limit = 100) {
        return this.auditLog.slice(-limit);
    }
    /**
     * Clear the in-memory audit log.
     */
    clearAuditLog() {
        this.auditLog = [];
    }
    /**
     * Enable file-based audit logging.
     */
    enableAuditFile(filePath) {
        this.auditFilePath = (0, path_1.resolve)(filePath);
        const dir = (0, path_1.dirname)(this.auditFilePath);
        if (!(0, fs_1.existsSync)(dir)) {
            (0, fs_1.mkdirSync)(dir, { recursive: true });
        }
        this.logger.info(`Audit file logging enabled: ${this.auditFilePath}`);
    }
    /**
     * Get all loaded rules.
     */
    getRules() {
        return Array.from(this.rules.values()).sort((a, b) => b.priority - a.priority);
    }
    /**
     * Enable or disable a rule.
     */
    setRuleEnabled(id, enabled) {
        const rule = this.rules.get(id);
        if (!rule)
            return false;
        rule.enabled = enabled;
        return true;
    }
    /**
     * Get rate limit status for a key.
     */
    getRateLimitStatus(key) {
        const state = this.rateLimitState.get(key);
        if (!state)
            return null;
        // Find applicable limit (check all, use the most restrictive)
        let minRemaining = Infinity;
        let minResetIn = 0;
        for (const [, config] of this.rateLimits) {
            const elapsed = Date.now() - state.windowStart;
            if (elapsed > config.windowMs) {
                return { count: 0, remaining: config.maxRequests, resetIn: 0 };
            }
            const remaining = Math.max(0, config.maxRequests - state.count);
            const resetIn = config.windowMs - elapsed;
            if (remaining < minRemaining) {
                minRemaining = remaining;
                minResetIn = resetIn;
            }
        }
        return { count: state.count, remaining: minRemaining, resetIn: minResetIn };
    }
    // ─── Internal ─────────────────────────────────────────────
    evaluateConditions(conditions, context) {
        if (!conditions || conditions.length === 0)
            return true;
        return conditions.every(cond => {
            const value = context[cond.field];
            switch (cond.operator) {
                case 'equals':
                    return value === cond.value;
                case 'not_equals':
                    return value !== cond.value;
                case 'contains':
                    return typeof value === 'string' && value.includes(String(cond.value));
                case 'matches':
                    if (typeof value !== 'string')
                        return false;
                    try {
                        return new RegExp(String(cond.value)).test(value);
                    }
                    catch {
                        return false;
                    }
                case 'greater_than':
                    return typeof value === 'number' && value > Number(cond.value);
                case 'less_than':
                    return typeof value === 'number' && value < Number(cond.value);
                case 'in':
                    return Array.isArray(cond.value) && cond.value.includes(value);
                default:
                    return false;
            }
        });
    }
    checkRateLimits(context) {
        const now = Date.now();
        for (const [name, config] of this.rateLimits) {
            const keyValue = context[config.keyField] ?? 'anonymous';
            const stateKey = `${name}:${keyValue}`;
            let state = this.rateLimitState.get(stateKey);
            if (!state || now - state.windowStart > config.windowMs) {
                // New window
                state = { count: 1, windowStart: now };
                this.rateLimitState.set(stateKey, state);
                continue;
            }
            state.count++;
            if (state.count > config.maxRequests) {
                const resetIn = config.windowMs - (now - state.windowStart);
                return {
                    allowed: false,
                    action: 'rate_limit',
                    reason: `Rate limit exceeded for ${name}: ${state.count}/${config.maxRequests} (resets in ${Math.ceil(resetIn / 1000)}s)`,
                    shouldAudit: true,
                };
            }
        }
        return null;
    }
    recordAudit(context, evaluation) {
        if (!evaluation.shouldAudit)
            return;
        const entry = {
            timestamp: Date.now(),
            isoTime: new Date().toISOString(),
            context: { ...context },
            evaluation: { ...evaluation },
        };
        // In-memory log
        this.auditLog.push(entry);
        if (this.auditLog.length > this.maxAuditLogSize) {
            this.auditLog = this.auditLog.slice(-Math.floor(this.maxAuditLogSize / 2));
        }
        // File-based audit log
        if (this.auditFilePath) {
            try {
                const line = JSON.stringify(entry) + '\n';
                (0, fs_1.appendFileSync)(this.auditFilePath, line, 'utf-8');
            }
            catch (err) {
                this.logger.error('Failed to write audit log', {
                    error: err instanceof Error ? err.message : String(err),
                });
            }
        }
        // Log based on audit level
        const level = evaluation.matchedRule?.auditLevel ?? 'info';
        const logMsg = `Policy ${evaluation.action}: ${context.action} by ${context.user ?? 'unknown'}`;
        const logData = {
            rule: evaluation.matchedRule?.id,
            reason: evaluation.reason,
            resource: context.resource,
        };
        switch (level) {
            case 'error':
                this.logger.error(logMsg, logData);
                break;
            case 'warning':
                this.logger.warn(logMsg, logData);
                break;
            default:
                this.logger.info(logMsg, logData);
        }
    }
}
exports.PolicyEngine = PolicyEngine;
// ─── Singleton ────────────────────────────────────────────────
let instance = null;
/**
 * Get or create the global policy engine.
 */
function getPolicyEngine() {
    if (!instance) {
        instance = new PolicyEngine();
    }
    return instance;
}
/**
 * Reset the singleton (for testing).
 */
function resetPolicyEngine() {
    instance = null;
}
