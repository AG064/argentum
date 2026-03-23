/* eslint-disable no-unused-vars, @typescript-eslint/no-unused-vars */
// @ts-nocheck
/**
 * AG-Claw Approval UI / Human-in-the-Loop System
 *
 * Provides:
 * - TUI for approving/denying actions in terminal
 * - Dashboard integration (JSON API)
 * - CLI helpers for approve/deny commands
 * - Real-time notification support (Telegram, etc.)
 */

import { randomUUID } from 'crypto';
import { readFileSync, writeFileSync, existsSync, mkdirSync } from 'fs';
import { resolve, dirname } from 'path';

import { createLogger, type Logger } from '../../core/logger';
import { getPolicyEngine } from '../policy-engine';

import type {
  ApprovalRequest,
  ApprovalRisk,

} from '../types';

// ─── Constants ────────────────────────────────────────────────────────────────

const RISK_ICONS: Record<ApprovalRisk, string> = {
  low: '🔵',
  medium: '🟡',
  high: '🟠',
  critical: '🔴',
};

const RISK_COLORS: Record<ApprovalRisk, string> = {
  low: '\x1b[34m',      // blue
  medium: '\x1b[33m',   // yellow
  high: '\x1b[38;5;208m', // orange
  critical: '\x1b[31m', // red
};

const RISK_LABELS: Record<ApprovalRisk, string> = {
  low: 'LOW',
  medium: 'MEDIUM',
  high: 'HIGH',
  critical: 'CRITICAL',
};

// ─── Approval UI ──────────────────────────────────────────────────────────────

export class ApprovalUI {
  private logger: Logger;
  private dbPath: string | null = null;
  private pendingRequests: Map<string, ApprovalRequest> = new Map();
  private notifyCallback?: (request: ApprovalRequest) => void;

  constructor() {
    this.logger = createLogger().child({ feature: 'approval-ui' });
  }

  /**
   * Initialize with database path for persistence.
   */
  init(dbPath: string): void {
    this.dbPath = resolve(dbPath);
    const dir = dirname(this.dbPath);
    if (!existsSync(dir)) {
      mkdirSync(dir, { recursive: true });
    }
  }

  /**
   * Set notification callback (e.g., for Telegram alerts).
   */
  onNotify(callback: (request: ApprovalRequest) => void): void {
    this.notifyCallback = callback;
  }

  /**
   * Render a TUI prompt for an approval request.
   * Returns a promise that resolves to 'approve' | 'deny' | 'cancel'.
   */
  async prompt(request: ApprovalRequest): Promise<'approve' | 'deny' | 'cancel'> {
    const riskColor = RISK_COLORS[request.risk];
    const riskIcon = RISK_ICONS[request.risk];

    console.log('\n');
    console.log(`  ${'═'.repeat(70)}`);
    console.log(`  ${riskIcon} ${riskColor}${RISK_LABELS[request.risk]}\x1b[0m Security Approval Required`);
    console.log(`  ${'═'.repeat(70)}`);
    console.log('');
    console.log(`  \x1b[1mAgent:\x1b[0m ${request.agentId}${request.agentName ? ` (${request.agentName})` : ''}`);
    console.log(`  \x1b[1mRequest ID:\x1b[0m ${request.id}`);
    console.log(`  \x1b[1mTime:\x1b[0m ${new Date(request.requestedAt).toLocaleString()}`);
    console.log(`  \x1b[1mExpires:\x1b[0m ${new Date(request.expiresAt).toLocaleString()}`);
    console.log('');
    console.log(`  ${'─'.repeat(70)}`);
    console.log(`  \x1b[1mWhat:\x1b[0m ${request.details.what}`);
    console.log(`  ${'─'.repeat(70)}`);
    console.log(`  \x1b[1mWhy:\x1b[0m ${request.details.why}`);
    console.log(`  ${'─'.repeat(70)}`);
    console.log(`  \x1b[1mConsequences:\x1b[0m ${request.details.consequences}`);
    if (request.details.alternatives?.length) {
      console.log(`  ${'─'.repeat(70)}`);
      console.log(`  \x1b[1mAlternatives:\x1b[0m`);
      for (const alt of request.details.alternatives) {
        console.log(`    • ${alt}`);
      }
    }
    console.log(`  ${'─'.repeat(70)}`);
    console.log('');

    // Read user input
    const readline = await import('readline');
    const rl = readline.createInterface({
      input: process.stdin,
      output: process.stdout,
    });

    const question = (q: string): Promise<string> =>
      new Promise((resolve) => rl.question(q, resolve));

    let response: 'approve' | 'deny' | 'cancel' = 'cancel';

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
      console.log('  \x1b[90mCancelled.\x1b[0m');
    }

    console.log('');
    return response;
  }

  /**
   * Notify about a pending approval via configured channels.
   */
  notify(request: ApprovalRequest): void {
    // Console notification
    const riskColor = RISK_COLORS[request.risk];
    const riskIcon = RISK_ICONS[request.risk];

    console.log('\n');
    console.log(`  \x1b[1m${riskIcon} ${riskColor}[SECURITY APPROVAL REQUIRED]\x1b[0m ${request.agentId}`);
    console.log(`  \x1b[90mRequest ID: ${request.id} | Risk: ${RISK_LABELS[request.risk]}\x1b[0m`);
    console.log(`  \x1b[90m${request.details.what}\x1b[0m`);
    console.log('');

    // External notification callback (e.g., Telegram)
    if (this.notifyCallback) {
      try {
        this.notifyCallback(request);
      } catch (err) {
        this.logger.error('Notification callback failed', { error: String(err) });
      }
    }
  }

  /**
   * Render pending approvals as a formatted list.
   */
  renderPendingList(requests: ApprovalRequest[]): string {
    if (requests.length === 0) {
      return '  \x1b[90mNo pending approvals.\x1b[0m';
    }

    const lines: string[] = [];
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
  renderDetail(request: ApprovalRequest): string {
    const riskColor = RISK_COLORS[request.risk];
    const riskIcon = RISK_ICONS[request.risk];

    const lines: string[] = [];
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
  handleResponse(
    requestId: string,
    decision: 'approve' | 'deny',
    userId: string,
    notes?: string,
  ): { success: boolean; error?: string } {
    try {
      const policyEngine = getPolicyEngine();

      if (decision === 'approve') {
        const ok = policyEngine.approve(requestId, userId, notes);
        if (!ok) {
          return { success: false, error: 'Failed to approve (request not found, already responded, or expired)' };
        }
      } else {
        const ok = policyEngine.deny(requestId, userId, notes);
        if (!ok) {
          return { success: false, error: 'Failed to deny (request not found, already responded, or expired)' };
        }
      }

      this.logger.info(`Approval ${decision}d by ${userId}`, { requestId });
      return { success: true };
    } catch (err) {
      const error = err instanceof Error ? err.message : String(err);
      this.logger.error('Handle response failed', { requestId, error });
      return { success: false, error };
    }
  }

  /**
   * Get pending approvals (from policy engine).
   */
  getPending(): ApprovalRequest[] {
    try {
      const policyEngine = getPolicyEngine();
      return policyEngine.getPendingApprovals();
    } catch {
      return Array.from(this.pendingRequests.values()).filter((r) => r.status === 'pending');
    }
  }

  /**
   * Get a specific approval by ID.
   */
  getApproval(id: string): ApprovalRequest | undefined {
    try {
      const policyEngine = getPolicyEngine();
      return policyEngine.getApproval(id);
    } catch {
      return this.pendingRequests.get(id);
    }
  }
}

// ─── Helpers ─────────────────────────────────────────────────────────────────

function formatAge(timestamp: number): string {
  const diff = Date.now() - timestamp;
  const mins = Math.floor(diff / 60000);
  if (mins < 1) return 'just now';
  if (mins < 60) return `${mins}m ago`;
  const hours = Math.floor(mins / 60);
  if (hours < 24) return `${hours}h ago`;
  const days = Math.floor(hours / 24);
  return `${days}d ago`;
}

function formatExpiry(timestamp: number): string {
  const diff = timestamp - Date.now();
  if (diff <= 0) return 'expired';
  const mins = Math.floor(diff / 60000);
  if (mins < 1) return 'expires soon';
  if (mins < 60) return `expires in ${mins}m`;
  const hours = Math.floor(mins / 60);
  return `expires in ${hours}h`;
}

// ─── CLI Helpers ─────────────────────────────────────────────────────────────

export function printApprovalHelp(): void {
  console.log(`
  \x1b[1magclaw security approve <request-id> [options]\x1b[0m
    Approve a pending security request.

    Options:
      --notes <text>   Add approval notes
      --agent <id>     Override agent ID

  \x1b[1magclaw security deny <request-id> [options]\x1b[0m
    Deny a pending security request.

    Options:
      --notes <text>   Add denial reason/notes
      --agent <id>     Override agent ID

  \x1b[1magclaw security approvals [options]\x1b[0m
    List pending approval requests.

    Options:
      --all            Include resolved approvals
      --risk <level>   Filter by risk (low, medium, high, critical)
      --agent <id>     Filter by agent

  \x1b[1magclaw security approval <request-id>\x1b[0m
    Show detailed information about a specific approval request.
  `);
}

// ─── Singleton ────────────────────────────────────────────────────────────────

let instance: ApprovalUI | null = null;

export function getApprovalUI(): ApprovalUI {
  if (!instance) {
    instance = new ApprovalUI();
  }
  return instance;
}
