export type CapabilityAction = 'file.read' | 'file.write' | 'file.delete' | 'shell.execute' | 'network.request' | 'os.automation' | 'integration.use';
export type CapabilityGrantScope = 'one-action' | 'one-file' | 'one-folder' | 'one-command' | 'one-domain' | 'current-session' | 'always-for-workspace';
export type CapabilityDecisionReason = 'inside-workspace' | 'active-grant' | 'outside-workspace' | 'invalid-path' | 'invalid-resource' | 'workspace-not-configured' | 'no-active-grant';
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
export type CapabilityDecision = {
    allowed: true;
    action: CapabilityAction;
    reason: Extract<CapabilityDecisionReason, 'inside-workspace' | 'active-grant'>;
    resource: string;
    resolvedPath?: string;
    grantId?: string;
} | {
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
export declare function createCapabilityBroker(options: CapabilityBrokerOptions): CapabilityBroker;
//# sourceMappingURL=capability-broker.d.ts.map