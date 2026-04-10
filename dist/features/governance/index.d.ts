/**
 * Governance Feature
 *
 * Approval gates for sensitive actions. Requires human approval
 * before executing risky operations. Supports approve/reject/rollback.
 */
import { type FeatureModule, type FeatureContext, type FeatureMeta, type HealthStatus } from '../../core/plugin-loader';
export interface Action {
    type: string;
    description: string;
    payload: Record<string, unknown>;
    risk: 'low' | 'medium' | 'high' | 'critical';
}
export interface ApprovalTicket {
    id: string;
    action: Action;
    requester: string;
    status: 'pending' | 'approved' | 'rejected' | 'expired';
    approver: string | null;
    reason: string | null;
    createdAt: number;
    resolvedAt: number | null;
    expiresAt: number;
}
export interface GovernanceConfig {
    enabled: boolean;
    dbPath: string;
    autoApproveRisk: 'none' | 'low' | 'medium';
    ticketExpiryMs: number;
    requiredApprovers: number;
    approvers: string[];
    notifyOnPending: boolean;
}
declare class GovernanceFeature implements FeatureModule {
    readonly meta: FeatureMeta;
    private config;
    private db;
    private ctx;
    private approvalCallbacks;
    private expiryTimer;
    init(config: Record<string, unknown>, context: FeatureContext): Promise<void>;
    start(): Promise<void>;
    stop(): Promise<void>;
    healthCheck(): Promise<HealthStatus>;
    /**
     * Request approval for an action.
     * Auto-approves if risk level is below threshold.
     */
    requireApproval(action: Action, requester: string): Promise<ApprovalTicket>;
    /** Approve a pending ticket */
    approve(ticketId: string, approver: string): Promise<void>;
    /** Reject a pending ticket */
    reject(ticketId: string, approver: string, reason: string): Promise<void>;
    /** Get all pending approval tickets */
    getPendingApprovals(): Promise<ApprovalTicket[]>;
    /** Rollback a configuration change (stores rollback data) */
    rollback(configId: string): Promise<void>;
    /** Wait for approval (returns a promise that resolves when approved/rejected) */
    waitForApproval(ticketId: string, timeoutMs?: number): Promise<ApprovalTicket>;
    /** Get a ticket by ID */
    getTicket(ticketId: string): Promise<ApprovalTicket | null>;
    /** List all tickets with optional status filter */
    listTickets(status?: ApprovalTicket['status']): Promise<ApprovalTicket[]>;
    private initDatabase;
    private createTicket;
    private getTicketSync;
    private getPendingSync;
    private getExpiredCount;
    private expireOldTickets;
    private rowToTicket;
    private parseJson;
}
declare const _default: GovernanceFeature;
export default _default;
//# sourceMappingURL=index.d.ts.map