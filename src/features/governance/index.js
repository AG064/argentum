'use strict';
/**
 * Governance Feature
 *
 * Approval gates for sensitive actions. Requires human approval
 * before executing risky operations. Supports approve/reject/rollback.
 */
var __importDefault =
  (this && this.__importDefault) ||
  function (mod) {
    return mod && mod.__esModule ? mod : { default: mod };
  };
Object.defineProperty(exports, '__esModule', { value: true });
const better_sqlite3_1 = __importDefault(require('better-sqlite3'));
const fs_1 = require('fs');
const path_1 = require('path');
const crypto_1 = require('crypto');
// ─── Feature ─────────────────────────────────────────────────────────────────
class GovernanceFeature {
  constructor() {
    this.meta = {
      name: 'governance',
      version: '0.0.4',
      description: 'Approval gates and governance for sensitive actions',
      dependencies: [],
    };
    this.config = {
      enabled: false,
      dbPath: './data/governance.db',
      autoApproveRisk: 'low',
      ticketExpiryMs: 24 * 60 * 60 * 1000, // 24 hours
      requiredApprovers: 1,
      approvers: [],
      notifyOnPending: true,
    };
    this.approvalCallbacks = new Map();
    this.expiryTimer = null;
    this.rowToTicket = (row) => ({
      id: row.id,
      action: {
        type: row.action_type,
        description: row.action_description,
        payload: this.parseJson(row.action_payload),
        risk: row.action_risk,
      },
      requester: row.requester,
      status: row.status,
      approver: row.approver,
      reason: row.reason,
      createdAt: row.created_at,
      resolvedAt: row.resolved_at,
      expiresAt: row.expires_at,
    });
  }
  async init(config, context) {
    this.ctx = context;
    this.config = { ...this.config, ...config };
    this.initDatabase();
  }
  async start() {
    this.expiryTimer = setInterval(() => {
      this.expireOldTickets();
    }, 60000);
    const pending = this.getPendingSync();
    this.ctx.logger.info('Governance active', {
      autoApprove: this.config.autoApproveRisk,
      pendingTickets: pending.length,
      approvers: this.config.approvers.length,
    });
  }
  async stop() {
    if (this.expiryTimer) {
      clearInterval(this.expiryTimer);
      this.expiryTimer = null;
    }
    this.db?.close();
  }
  async healthCheck() {
    const pending = this.getPendingSync();
    const expired = this.getExpiredCount();
    return {
      healthy: expired === 0,
      message: expired > 0 ? `${expired} tickets expired unhandled` : undefined,
      details: {
        pendingTickets: pending.length,
        expiredTickets: expired,
      },
    };
  }
  // ─── Public API ──────────────────────────────────────────────────────────
  /**
   * Request approval for an action.
   * Auto-approves if risk level is below threshold.
   */
  async requireApproval(action, requester) {
    const riskOrder = ['low', 'medium', 'high', 'critical'];
    const autoLevel = riskOrder.indexOf(this.config.autoApproveRisk);
    // Auto-approve low-risk actions
    if (this.config.autoApproveRisk !== 'none') {
      const actionLevel = riskOrder.indexOf(action.risk);
      if (actionLevel <= autoLevel) {
        this.ctx.logger.debug('Auto-approved action', { type: action.type, risk: action.risk });
        return this.createTicket(action, requester, 'approved', 'auto-approved');
      }
    }
    return this.createTicket(action, requester, 'pending');
  }
  /** Approve a pending ticket */
  async approve(ticketId, approver) {
    const ticket = this.getTicketSync(ticketId);
    if (!ticket) throw new Error(`Ticket not found: ${ticketId}`);
    if (ticket.status !== 'pending') throw new Error(`Ticket is not pending: ${ticket.status}`);
    if (this.config.approvers.length > 0 && !this.config.approvers.includes(approver)) {
      throw new Error(`Unauthorized approver: ${approver}`);
    }
    const now = Date.now();
    this.db
      .prepare('UPDATE approval_tickets SET status = ?, approver = ?, resolved_at = ? WHERE id = ?')
      .run('approved', approver, now, ticketId);
    this.ctx.logger.info('Ticket approved', { ticketId, approver, action: ticket.action.type });
    // Notify callback
    const callback = this.approvalCallbacks.get(ticketId);
    if (callback) {
      callback({ ...ticket, status: 'approved', approver, resolvedAt: now });
      this.approvalCallbacks.delete(ticketId);
    }
  }
  /** Reject a pending ticket */
  async reject(ticketId, approver, reason) {
    const ticket = this.getTicketSync(ticketId);
    if (!ticket) throw new Error(`Ticket not found: ${ticketId}`);
    if (ticket.status !== 'pending') throw new Error(`Ticket is not pending: ${ticket.status}`);
    const now = Date.now();
    this.db
      .prepare(
        'UPDATE approval_tickets SET status = ?, approver = ?, reason = ?, resolved_at = ? WHERE id = ?',
      )
      .run('rejected', approver, reason, now, ticketId);
    this.ctx.logger.info('Ticket rejected', { ticketId, approver, reason });
    const callback = this.approvalCallbacks.get(ticketId);
    if (callback) {
      callback({
        ...ticket,
        status: 'rejected',
        approver,
        reason,
        resolvedAt: now,
      });
      this.approvalCallbacks.delete(ticketId);
    }
  }
  /** Get all pending approval tickets */
  async getPendingApprovals() {
    return this.getPendingSync();
  }
  /** Rollback a configuration change (stores rollback data) */
  async rollback(configId) {
    const ticket = this.db
      .prepare("SELECT * FROM approval_tickets WHERE id = ? AND status = 'approved'")
      .get(configId);
    if (!ticket) {
      throw new Error(`No approved ticket found for rollback: ${configId}`);
    }
    const payload = this.parseJson(ticket.action_payload);
    const rollbackData = payload._rollback;
    if (rollbackData) {
      this.ctx.logger.info('Rollback executed', { configId, rollbackData });
      // Emit rollback hook for other features to handle
      await this.ctx.emit('governance:rollback', { configId, rollbackData });
    } else {
      this.ctx.logger.warn('No rollback data available', { configId });
    }
  }
  /** Wait for approval (returns a promise that resolves when approved/rejected) */
  waitForApproval(ticketId, timeoutMs) {
    return new Promise((resolve, reject) => {
      const timer = timeoutMs
        ? setTimeout(() => {
            this.approvalCallbacks.delete(ticketId);
            reject(new Error('Approval timeout'));
          }, timeoutMs)
        : null;
      this.approvalCallbacks.set(ticketId, (ticket) => {
        if (timer) clearTimeout(timer);
        resolve(ticket);
      });
    });
  }
  /** Get a ticket by ID */
  async getTicket(ticketId) {
    return this.getTicketSync(ticketId);
  }
  /** List all tickets with optional status filter */
  async listTickets(status) {
    const rows = status
      ? this.db
          .prepare('SELECT * FROM approval_tickets WHERE status = ? ORDER BY created_at DESC')
          .all(status)
      : this.db.prepare('SELECT * FROM approval_tickets ORDER BY created_at DESC').all();
    return rows.map(this.rowToTicket);
  }
  // ─── Private helpers ─────────────────────────────────────────────────────
  initDatabase() {
    const fullPath = (0, path_1.resolve)(this.config.dbPath);
    if (!(0, fs_1.existsSync)((0, path_1.dirname)(fullPath))) {
      (0, fs_1.mkdirSync)((0, path_1.dirname)(fullPath), { recursive: true });
    }
    this.db = new better_sqlite3_1.default(fullPath);
    this.db.pragma('journal_mode = WAL');
    this.db.pragma('synchronous = NORMAL');
    this.db.exec(`
      CREATE TABLE IF NOT EXISTS approval_tickets (
        id TEXT PRIMARY KEY,
        action_type TEXT NOT NULL,
        action_description TEXT NOT NULL,
        action_payload TEXT DEFAULT '{}',
        action_risk TEXT NOT NULL,
        requester TEXT NOT NULL,
        status TEXT NOT NULL DEFAULT 'pending',
        approver TEXT,
        reason TEXT,
        created_at INTEGER NOT NULL,
        resolved_at INTEGER,
        expires_at INTEGER NOT NULL
      );
      CREATE INDEX IF NOT EXISTS idx_tickets_status ON approval_tickets(status);
      CREATE INDEX IF NOT EXISTS idx_tickets_created ON approval_tickets(created_at DESC);
    `);
  }
  createTicket(action, requester, status, reason) {
    const id = (0, crypto_1.randomUUID)();
    const now = Date.now();
    const expiresAt = now + this.config.ticketExpiryMs;
    const ticket = {
      id,
      action,
      requester,
      status,
      approver: status === 'approved' ? 'system' : null,
      reason: reason ?? null,
      createdAt: now,
      resolvedAt: status === 'approved' ? now : null,
      expiresAt,
    };
    this.db
      .prepare(
        `INSERT INTO approval_tickets
       (id, action_type, action_description, action_payload, action_risk, requester, status, approver, reason, created_at, resolved_at, expires_at)
       VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)`,
      )
      .run(
        id,
        action.type,
        action.description,
        JSON.stringify(action.payload),
        action.risk,
        requester,
        status,
        status === 'approved' ? 'system' : null,
        reason ?? null,
        now,
        status === 'approved' ? now : null,
        expiresAt,
      );
    if (status === 'pending') {
      this.ctx.logger.info('Approval ticket created', {
        id,
        type: action.type,
        risk: action.risk,
        requester,
      });
    }
    return ticket;
  }
  getTicketSync(ticketId) {
    const row = this.db.prepare('SELECT * FROM approval_tickets WHERE id = ?').get(ticketId);
    return row ? this.rowToTicket(row) : null;
  }
  getPendingSync() {
    const rows = this.db
      .prepare("SELECT * FROM approval_tickets WHERE status = 'pending' ORDER BY created_at ASC")
      .all();
    return rows.map(this.rowToTicket);
  }
  getExpiredCount() {
    const now = Date.now();
    const row = this.db
      .prepare(
        "SELECT COUNT(*) as c FROM approval_tickets WHERE status = 'pending' AND expires_at <= ?",
      )
      .get(now);
    return row.c;
  }
  expireOldTickets() {
    const now = Date.now();
    const result = this.db
      .prepare(
        "UPDATE approval_tickets SET status = 'expired', resolved_at = ? WHERE status = 'pending' AND expires_at <= ?",
      )
      .run(now, now);
    if (result.changes > 0) {
      this.ctx.logger.warn('Expired pending tickets', { count: result.changes });
    }
  }
  parseJson(str) {
    try {
      return JSON.parse(str);
    } catch {
      return {};
    }
  }
}
exports.default = new GovernanceFeature();
