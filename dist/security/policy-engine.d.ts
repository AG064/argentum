/**
 * Argentum Policy Engine
 *
 * YAML-based security policies loaded from config/security-policy.yaml.
 * Evaluates actions against rules by role/agent, enforces rate limits
 * per user/session, and logs all decisions to an audit trail.
 */
export interface PolicyCondition {
    field: string;
    operator: 'equals' | 'not_equals' | 'contains' | 'matches' | 'greater_than' | 'less_than' | 'in';
    value: unknown;
}
export interface PolicyRule {
    id: string;
    name: string;
    description?: string;
    action: 'allow' | 'deny' | 'audit' | 'rate_limit';
    priority: number;
    enabled: boolean;
    auditLevel?: 'none' | 'info' | 'warning' | 'error';
    conditions: PolicyCondition[];
}
export interface RateLimitConfig {
    windowMs: number;
    maxRequests: number;
    keyField: string;
}
export interface PolicyEvaluation {
    allowed: boolean;
    action: string;
    matchedRule?: PolicyRule;
    reason: string;
    shouldAudit: boolean;
}
export interface PolicyContext {
    action: string;
    resource: string;
    user?: string;
    agent?: string;
    role?: string;
    channel?: string;
    feature?: string;
    sessionId?: string;
    [key: string]: unknown;
}
export interface AuditEntry {
    timestamp: number;
    isoTime: string;
    context: PolicyContext;
    evaluation: PolicyEvaluation;
}
export declare class PolicyEngine {
    private rules;
    private rateLimits;
    private rateLimitState;
    private auditLog;
    private logger;
    private auditFilePath;
    private maxAuditLogSize;
    constructor();
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
    loadFromFile(filePath: string): void;
    /**
     * Add a policy rule programmatically.
     */
    addRule(rule: PolicyRule): void;
    /**
     * Remove a rule by ID.
     */
    removeRule(id: string): boolean;
    /**
     * Evaluate an action context against all loaded policies.
     *
     * Evaluation order:
     * 1. Rate limits (if user/session specified)
     * 2. Rules sorted by priority (highest first)
     * 3. First matching rule wins
     * 4. Default: allow
     */
    evaluate(context: PolicyContext): PolicyEvaluation;
    /**
     * Quick helper: is this action allowed?
     */
    isAllowed(context: PolicyContext): boolean;
    /**
     * Get the audit log.
     */
    getAuditLog(limit?: number): AuditEntry[];
    /**
     * Clear the in-memory audit log.
     */
    clearAuditLog(): void;
    /**
     * Enable file-based audit logging.
     */
    enableAuditFile(filePath: string): void;
    /**
     * Get all loaded rules.
     */
    getRules(): PolicyRule[];
    /**
     * Enable or disable a rule.
     */
    setRuleEnabled(id: string, enabled: boolean): boolean;
    /**
     * Get rate limit status for a key.
     */
    getRateLimitStatus(key: string): {
        count: number;
        remaining: number;
        resetIn: number;
    } | null;
    private evaluateConditions;
    private checkRateLimits;
    private recordAudit;
}
/**
 * Get or create the global policy engine.
 */
export declare function getPolicyEngine(): PolicyEngine;
/**
 * Reset the singleton (for testing).
 */
export declare function resetPolicyEngine(): void;
//# sourceMappingURL=policy-engine.d.ts.map