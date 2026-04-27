/**
 * Argentum Approval UI / Human-in-the-Loop System
 *
 * Provides:
 * - TUI for approving/denying actions in terminal
 * - Dashboard integration (JSON API)
 * - CLI helpers for approve/deny commands
 * - Real-time notification support (Telegram, etc.)
 */
import type { ApprovalRequest } from '../types';
export declare class ApprovalUI {
    private logger;
    private dbPath;
    private pendingRequests;
    private notifyCallback?;
    constructor();
    /**
     * Initialize with database path for persistence.
     */
    init(dbPath: string): void;
    /**
     * Set notification callback (e.g., for Telegram alerts).
     */
    onNotify(callback: (request: ApprovalRequest) => void): void;
    /**
     * Render a TUI prompt for an approval request.
     * Returns a promise that resolves to 'approve' | 'deny' | 'cancel'.
     */
    prompt(request: ApprovalRequest): Promise<'approve' | 'deny' | 'cancel'>;
    /**
     * Notify about a pending approval via configured channels.
     */
    notify(request: ApprovalRequest): void;
    /**
     * Render pending approvals as a formatted list.
     */
    renderPendingList(requests: ApprovalRequest[]): string;
    /**
     * Render a single approval as a formatted detail.
     */
    renderDetail(request: ApprovalRequest): string;
    /**
     * Handle approval response from any source (TUI, CLI, dashboard, API).
     */
    handleResponse(requestId: string, decision: 'approve' | 'deny', userId: string, notes?: string): {
        success: boolean;
        error?: string;
    };
    /**
     * Get pending approvals (from policy engine).
     */
    getPending(): ApprovalRequest[];
    /**
     * Get a specific approval by ID.
     */
    getApproval(id: string): ApprovalRequest | undefined;
}
export declare function printApprovalHelp(): void;
export declare function getApprovalUI(): ApprovalUI;
//# sourceMappingURL=index.d.ts.map