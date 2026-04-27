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
import type { Policy, PolicyDecision, AgentAction, ApprovalRequest, AuditEntry, Blueprint, SecurityStats } from '../types';
export declare class PolicyEngine {
    private policies;
    private db;
    private auditFilePath;
    private approvalStore;
    private logger;
    private approvalExpirySeconds;
    private blueprint;
    constructor(dbPath?: string);
    private initDatabase;
    private loadPoliciesFromDB;
    private cleanupExpiredApprovals;
    /**
     * Load policies and config from a blueprint YAML file.
     */
    loadBlueprint(blueprint: Blueprint): void;
    private savePolicyToDB;
    /**
     * Evaluate an action against all policies.
     * Default-deny: if no policy matches → DENY.
     */
    evaluate(action: AgentAction): PolicyDecision;
    /**
     * Check if action requires human approval.
     */
    requiresApproval(action: AgentAction): ApprovalRequest | null;
    private createApprovalRequest;
    /**
     * Approve a pending request.
     */
    approve(requestId: string, userId: string, notes?: string): boolean;
    /**
     * Deny a pending request.
     */
    deny(requestId: string, userId: string, notes?: string): boolean;
    private updateApprovalInDB;
    /**
     * Get all pending approval requests.
     */
    getPendingApprovals(): ApprovalRequest[];
    /**
     * Get approval by ID.
     */
    getApproval(id: string): ApprovalRequest | undefined;
    addPolicy(policy: Omit<Policy, 'id' | 'createdAt' | 'updatedAt'>): Policy;
    updatePolicy(id: string, updates: Partial<Policy>): boolean;
    removePolicy(id: string): boolean;
    getPolicies(): Policy[];
    setPolicyEnabled(id: string, enabled: boolean): boolean;
    logAudit(entry: Omit<AuditEntry, 'id' | 'timestamp' | 'isoTime'>): void;
    enableAuditFile(filePath: string): void;
    queryAudit(filter?: {
        since?: number;
        until?: number;
        actor?: string;
        action?: string;
        limit?: number;
    }): AuditEntry[];
    getStats(): SecurityStats;
    close(): void;
}
export declare function getPolicyEngine(dbPath?: string): PolicyEngine;
export declare function resetPolicyEngine(): void;
//# sourceMappingURL=index.d.ts.map