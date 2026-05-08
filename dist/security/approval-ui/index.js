"use strict";
// @ts-nocheck
/**
 * Argentum Approval UI / Human-in-the-Loop System
 *
 * Provides:
 * - TUI for approving/denying actions in terminal
 * - Dashboard integration (JSON API)
 * - CLI helpers for approve/deny commands
 * - Real-time notification support (Telegram, etc.)
 */
var __createBinding = (this && this.__createBinding) || (Object.create ? (function(o, m, k, k2) {
    if (k2 === undefined) k2 = k;
    var desc = Object.getOwnPropertyDescriptor(m, k);
    if (!desc || ("get" in desc ? !m.__esModule : desc.writable || desc.configurable)) {
      desc = { enumerable: true, get: function() { return m[k]; } };
    }
    Object.defineProperty(o, k2, desc);
}) : (function(o, m, k, k2) {
    if (k2 === undefined) k2 = k;
    o[k2] = m[k];
}));
var __setModuleDefault = (this && this.__setModuleDefault) || (Object.create ? (function(o, v) {
    Object.defineProperty(o, "default", { enumerable: true, value: v });
}) : function(o, v) {
    o["default"] = v;
});
var __importStar = (this && this.__importStar) || (function () {
    var ownKeys = function(o) {
        ownKeys = Object.getOwnPropertyNames || function (o) {
            var ar = [];
            for (var k in o) if (Object.prototype.hasOwnProperty.call(o, k)) ar[ar.length] = k;
            return ar;
        };
        return ownKeys(o);
    };
    return function (mod) {
        if (mod && mod.__esModule) return mod;
        var result = {};
        if (mod != null) for (var k = ownKeys(mod), i = 0; i < k.length; i++) if (k[i] !== "default") __createBinding(result, mod, k[i]);
        __setModuleDefault(result, mod);
        return result;
    };
})();
Object.defineProperty(exports, "__esModule", { value: true });
exports.ApprovalUI = void 0;
exports.printApprovalHelp = printApprovalHelp;
exports.getApprovalUI = getApprovalUI;
const fs_1 = require("fs");
const path_1 = require("path");
const logger_1 = require("../../core/logger");
const policy_engine_1 = require("../policy-engine");
// ─── Constants ────────────────────────────────────────────────────────────────
const RISK_ICONS = {
    low: '🔵',
    medium: '🟡',
    high: '🟠',
    critical: '🔴',
};
const RISK_COLORS = {
    low: '\x1b[34m', // blue
    medium: '\x1b[33m', // yellow
    high: '\x1b[38;5;208m', // orange
    critical: '\x1b[31m', // red
};
const RISK_LABELS = {
    low: 'LOW',
    medium: 'MEDIUM',
    high: 'HIGH',
    critical: 'CRITICAL',
};
// ─── Approval UI ──────────────────────────────────────────────────────────────
class ApprovalUI {
    logger;
    dbPath = null;
    pendingRequests = new Map();
    notifyCallback;
    constructor() {
        this.logger = (0, logger_1.createLogger)().child({ feature: 'approval-ui' });
    }
    /**
     * Initialize with database path for persistence.
     */
    init(dbPath) {
        this.dbPath = (0, path_1.resolve)(dbPath);
        const dir = (0, path_1.dirname)(this.dbPath);
        if (!(0, fs_1.existsSync)(dir)) {
            (0, fs_1.mkdirSync)(dir, { recursive: true });
        }
    }
    /**
     * Set notification callback (e.g., for Telegram alerts).
     */
    onNotify(callback) {
        this.notifyCallback = callback;
    }
    /**
     * Render a TUI prompt for an approval request.
     * Returns a promise that resolves to 'approve' | 'deny' | 'cancel'.
     */
    async prompt(request) {
        const riskColor = RISK_COLORS[request.risk];
        const riskIcon = RISK_ICONS[request.risk];
        console.info('\n');
        console.info(`  ${'═'.repeat(70)}`);
        console.info(`  ${riskIcon} ${riskColor}${RISK_LABELS[request.risk]}\x1b[0m Security Approval Required`);
        console.info(`  ${'═'.repeat(70)}`);
        console.info('');
        console.info(`  \x1b[1mAgent:\x1b[0m ${request.agentId}${request.agentName ? ` (${request.agentName})` : ''}`);
        console.info(`  \x1b[1mRequest ID:\x1b[0m ${request.id}`);
        console.info(`  \x1b[1mTime:\x1b[0m ${new Date(request.requestedAt).toLocaleString()}`);
        console.info(`  \x1b[1mExpires:\x1b[0m ${new Date(request.expiresAt).toLocaleString()}`);
        console.info('');
        console.info(`  ${'─'.repeat(70)}`);
        console.info(`  \x1b[1mWhat:\x1b[0m ${request.details.what}`);
        console.info(`  ${'─'.repeat(70)}`);
        console.info(`  \x1b[1mWhy:\x1b[0m ${request.details.why}`);
        console.info(`  ${'─'.repeat(70)}`);
        console.info(`  \x1b[1mConsequences:\x1b[0m ${request.details.consequences}`);
        if (request.details.alternatives?.length) {
            console.info(`  ${'─'.repeat(70)}`);
            console.info(`  \x1b[1mAlternatives:\x1b[0m`);
            for (const alt of request.details.alternatives) {
                console.info(`    • ${alt}`);
            }
        }
        console.info(`  ${'─'.repeat(70)}`);
        console.info('');
        // Read user input
        const readline = await Promise.resolve().then(() => __importStar(require('readline')));
        const rl = readline.createInterface({
            input: process.stdin,
            output: process.stdout,
        });
        const question = (q) => new Promise((resolve) => rl.question(q, resolve));
        let response = 'cancel';
        while (response === 'cancel') {
            const answer = await question('  \x1b[36m[A]pprove\x1b[0m / \x1b[31m[D]eny\x1b[0m / \x1b[90m[C]ancel\x1b[0m: ');
            switch (answer.trim().toLowerCase()) {
                case 'a':
                case 'approve':
                case 'y':
                case 'yes':
                    response = 'approve';
                    break;
                case 'd':
                case 'deny':
                case 'n':
                case 'no':
                    response = 'deny';
                    break;
                default:
                    response = 'cancel';
            }
        }
        rl.close();
        if (response === 'cancel') {
            console.info('  \x1b[90mCancelled.\x1b[0m');
        }
        console.info('');
        return response;
    }
    /**
     * Notify about a pending approval via configured channels.
     */
    notify(request) {
        // Console notification
        const riskColor = RISK_COLORS[request.risk];
        const riskIcon = RISK_ICONS[request.risk];
        console.info('\n');
        console.info(`  \x1b[1m${riskIcon} ${riskColor}[SECURITY APPROVAL REQUIRED]\x1b[0m ${request.agentId}`);
        console.info(`  \x1b[90mRequest ID: ${request.id} | Risk: ${RISK_LABELS[request.risk]}\x1b[0m`);
        console.info(`  \x1b[90m${request.details.what}\x1b[0m`);
        console.info('');
        // External notification callback (e.g., Telegram)
        if (this.notifyCallback) {
            try {
                this.notifyCallback(request);
            }
            catch (err) {
                this.logger.error('Notification callback failed', { error: String(err) });
            }
        }
    }
    /**
     * Render pending approvals as a formatted list.
     */
    renderPendingList(requests) {
        if (requests.length === 0) {
            return '  \x1b[90mNo pending approvals.\x1b[0m';
        }
        const lines = [];
        lines.push(`\n  \x1b[1mPending Approvals (${requests.length}):\x1b[0m\n`);
        for (const req of requests) {
            const riskColor = RISK_COLORS[req.risk];
            const riskIcon = RISK_ICONS[req.risk];
            const age = formatAge(req.requestedAt);
            const expiresIn = formatExpiry(req.expiresAt);
            lines.push(`  ${riskIcon} \x1b[1m${req.id.slice(0, 8)}\x1b[0m ${riskColor}${RISK_LABELS[req.risk]}\x1b[0m`);
            lines.push(`    Agent: ${req.agentId} | ${age} | Expires: ${expiresIn}`);
            lines.push(`    ${req.details.what.slice(0, 60)}${req.details.what.length > 60 ? '...' : ''}`);
            lines.push('');
        }
        return lines.join('\n');
    }
    /**
     * Render a single approval as a formatted detail.
     */
    renderDetail(request) {
        const riskColor = RISK_COLORS[request.risk];
        const riskIcon = RISK_ICONS[request.risk];
        const lines = [];
        lines.push(`\n  ${'═'.repeat(70)}`);
        lines.push(`  ${riskIcon} Approval Request: ${request.id.slice(0, 8)}`);
        lines.push(`  ${'═'.repeat(70)}`);
        lines.push(`  \x1b[1mStatus:\x1b[0m ${request.status}`);
        lines.push(`  \x1b[1mRisk:\x1b[0m ${riskColor}${RISK_LABELS[request.risk]}\x1b[0m ${riskIcon}`);
        lines.push(`  \x1b[1mAgent:\x1b[0m ${request.agentId}${request.agentName ? ` (${request.agentName})` : ''}`);
        lines.push(`  \x1b[1mRequested:\x1b[0m ${new Date(request.requestedAt).toLocaleString()}`);
        lines.push(`  \x1b[1mExpires:\x1b[0m ${new Date(request.expiresAt).toLocaleString()}`);
        lines.push('');
        lines.push(`  ${'─'.repeat(70)}`);
        lines.push(`  \x1b[1mWhat:\x1b[0m ${request.details.what}`);
        lines.push(`  ${'─'.repeat(70)}`);
        lines.push(`  \x1b[1mWhy:\x1b[0m ${request.details.why}`);
        lines.push(`  ${'─'.repeat(70)}`);
        lines.push(`  \x1b[1mConsequences:\x1b[0m ${request.details.consequences}`);
        if (request.details.alternatives?.length) {
            lines.push(`  ${'─'.repeat(70)}`);
            lines.push(`  \x1b[1mAlternatives:\x1b[0m`);
            for (const alt of request.details.alternatives) {
                lines.push(`    • ${alt}`);
            }
        }
        if (request.respondedBy) {
            lines.push(`  ${'─'.repeat(70)}`);
            lines.push(`  \x1b[1mResponded by:\x1b[0m ${request.respondedBy} at ${request.respondedAt ? new Date(request.respondedAt).toLocaleString() : 'N/A'}`);
            if (request.notes) {
                lines.push(`  \x1b[1mNotes:\x1b[0m ${request.notes}`);
            }
        }
        lines.push(`  ${'═'.repeat(70)}`);
        lines.push('');
        return lines.join('\n');
    }
    /**
     * Handle approval response from any source (TUI, CLI, dashboard, API).
     */
    handleResponse(requestId, decision, userId, notes) {
        try {
            const policyEngine = (0, policy_engine_1.getPolicyEngine)();
            if (decision === 'approve') {
                const ok = policyEngine.approve(requestId, userId, notes);
                if (!ok) {
                    return {
                        success: false,
                        error: 'Failed to approve (request not found, already responded, or expired)',
                    };
                }
            }
            else {
                const ok = policyEngine.deny(requestId, userId, notes);
                if (!ok) {
                    return {
                        success: false,
                        error: 'Failed to deny (request not found, already responded, or expired)',
                    };
                }
            }
            this.logger.info(`Approval ${decision}d by ${userId}`, { requestId });
            return { success: true };
        }
        catch (err) {
            const error = err instanceof Error ? err.message : String(err);
            this.logger.error('Handle response failed', { requestId, error });
            return { success: false, error };
        }
    }
    /**
     * Get pending approvals (from policy engine).
     */
    getPending() {
        try {
            const policyEngine = (0, policy_engine_1.getPolicyEngine)();
            return policyEngine.getPendingApprovals();
        }
        catch {
            return Array.from(this.pendingRequests.values()).filter((r) => r.status === 'pending');
        }
    }
    /**
     * Get a specific approval by ID.
     */
    getApproval(id) {
        try {
            const policyEngine = (0, policy_engine_1.getPolicyEngine)();
            return policyEngine.getApproval(id);
        }
        catch {
            return this.pendingRequests.get(id);
        }
    }
}
exports.ApprovalUI = ApprovalUI;
// ─── Helpers ─────────────────────────────────────────────────────────────────
function formatAge(timestamp) {
    const diff = Date.now() - timestamp;
    const mins = Math.floor(diff / 60000);
    if (mins < 1)
        return 'just now';
    if (mins < 60)
        return `${mins}m ago`;
    const hours = Math.floor(mins / 60);
    if (hours < 24)
        return `${hours}h ago`;
    const days = Math.floor(hours / 24);
    return `${days}d ago`;
}
function formatExpiry(timestamp) {
    const diff = timestamp - Date.now();
    if (diff <= 0)
        return 'expired';
    const mins = Math.floor(diff / 60000);
    if (mins < 1)
        return 'expires soon';
    if (mins < 60)
        return `expires in ${mins}m`;
    const hours = Math.floor(mins / 60);
    return `expires in ${hours}h`;
}
// ─── CLI Helpers ─────────────────────────────────────────────────────────────
function printApprovalHelp() {
    console.info(`
  \x1b[1margentum security approve <request-id> [options]\x1b[0m
    Approve a pending security request.

    Options:
      --notes <text>   Add approval notes
      --agent <id>     Override agent ID

  \x1b[1margentum security deny <request-id> [options]\x1b[0m
    Deny a pending security request.

    Options:
      --notes <text>   Add denial reason/notes
      --agent <id>     Override agent ID

  \x1b[1margentum security approvals [options]\x1b[0m
    List pending approval requests.

    Options:
      --all            Include resolved approvals
      --risk <level>   Filter by risk (low, medium, high, critical)
      --agent <id>     Filter by agent

  \x1b[1margentum security approval <request-id>\x1b[0m
    Show detailed information about a specific approval request.
  `);
}
// ─── Singleton ────────────────────────────────────────────────────────────────
let instance = null;
function getApprovalUI() {
    instance ??= new ApprovalUI();
    return instance;
}
//# sourceMappingURL=index.js.map