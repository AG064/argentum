'use strict';
/**
 * Atomic Task Checkout Feature
 *
 * Prevents multiple agents from working on the same task simultaneously.
 * Uses SQLite atomic UPDATE with row locking for safety.
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
// ─── Feature ─────────────────────────────────────────────────────────────────
class TaskCheckoutFeature {
  constructor() {
    this.meta = {
      name: 'task-checkout',
      version: '0.0.3',
      description: 'Atomic task checkout to prevent agent conflicts',
      dependencies: [],
    };
    this.config = {
      enabled: false,
      dbPath: './data/task-checkout.db',
      leaseDurationMs: 30 * 60 * 1000, // 30 minutes
      maxLeasesPerAgent: 10,
    };
    this.cleanupTimer = null;
  }
  async init(config, context) {
    this.ctx = context;
    this.config = { ...this.config, ...config };
    this.initDatabase();
  }
  async start() {
    // Start periodic cleanup of expired checkouts
    this.cleanupTimer = setInterval(() => {
      this.releaseExpired();
    }, 60000); // Every minute
    const checkedOut = this.getCheckedOutTasksSync();
    this.ctx.logger.info('Task checkout active', {
      leaseDuration: `${this.config.leaseDurationMs / 1000}s`,
      activeCheckouts: checkedOut.length,
    });
  }
  async stop() {
    if (this.cleanupTimer) {
      clearInterval(this.cleanupTimer);
      this.cleanupTimer = null;
    }
    this.db?.close();
  }
  async healthCheck() {
    const active = this.getCheckedOutTasksSync();
    const expired = this.getExpiredCount();
    return {
      healthy: expired === 0,
      message: expired > 0 ? `${expired} expired checkouts found` : undefined,
      details: {
        activeCheckouts: active.length,
        expiredCheckouts: expired,
      },
    };
  }
  // ─── Public API ──────────────────────────────────────────────────────────
  /**
   * Atomically checkout a task for an agent.
   * Returns true if checkout succeeded, false if task is already taken.
   */
  async checkout(taskId, agentId) {
    // Check agent lease limit
    const agentLeases = this.db
      .prepare('SELECT COUNT(*) as c FROM checkouts WHERE agent_id = ?')
      .get(agentId);
    if (agentLeases.c >= this.config.maxLeasesPerAgent) {
      this.ctx.logger.warn('Agent lease limit reached', { agentId, leases: agentLeases.c });
      return false;
    }
    const now = Date.now();
    const expiresAt = now + this.config.leaseDurationMs;
    // Atomic insert — fails if task already checked out (UNIQUE constraint)
    try {
      const stmt = this.db
        .prepare(`INSERT INTO checkouts (task_id, agent_id, checked_out_at, expires_at)
         VALUES (?, ?, ?, ?)`);
      stmt.run(taskId, agentId, now, expiresAt);
      this.ctx.logger.info('Task checked out', { taskId, agentId, expiresAt });
      return true;
    } catch (err) {
      // UNIQUE constraint violation — task already checked out
      const e = err;
      if (e.code === 'SQLITE_CONSTRAINT_UNIQUE' || e.message?.includes('UNIQUE')) {
        // Check if the existing checkout is expired
        const existing = this.db.prepare('SELECT * FROM checkouts WHERE task_id = ?').get(taskId);
        if (existing && existing.expires_at < now) {
          // Expired — steal the lease
          this.db
            .prepare(
              'UPDATE checkouts SET agent_id = ?, checked_out_at = ?, expires_at = ? WHERE task_id = ?',
            )
            .run(agentId, now, expiresAt, taskId);
          this.ctx.logger.info('Task lease stolen (expired)', {
            taskId,
            previousAgent: existing.agent_id,
            newAgent: agentId,
          });
          return true;
        }
        this.ctx.logger.debug('Task already checked out', { taskId, agent: existing?.agent_id });
        return false;
      }
      throw err;
    }
  }
  /** Release a task checkout */
  async release(taskId) {
    const result = this.db.prepare('DELETE FROM checkouts WHERE task_id = ?').run(taskId);
    if (result.changes > 0) {
      this.ctx.logger.info('Task released', { taskId });
    }
  }
  /** Release all tasks for an agent */
  async releaseAgent(agentId) {
    const result = this.db.prepare('DELETE FROM checkouts WHERE agent_id = ?').run(agentId);
    this.ctx.logger.info('Agent releases all tasks', { agentId, released: result.changes });
    return result.changes;
  }
  /** Get all currently checked out tasks */
  async getCheckedOutTasks() {
    return this.getCheckedOutTasksSync();
  }
  /** Check if a task is available for checkout */
  async isAvailable(taskId) {
    const row = this.db.prepare('SELECT * FROM checkouts WHERE task_id = ?').get(taskId);
    if (!row) return true;
    return row.expires_at < Date.now();
  }
  /** Get tasks checked out by a specific agent */
  async getAgentTasks(agentId) {
    const rows = this.db
      .prepare('SELECT * FROM checkouts WHERE agent_id = ? ORDER BY checked_out_at ASC')
      .all(agentId);
    return rows.map((r) => ({
      taskId: r.task_id,
      agentId: r.agent_id,
      checkedOutAt: r.checked_out_at,
      expiresAt: r.expires_at,
    }));
  }
  /** Extend a lease */
  async extendLease(taskId) {
    const newExpiry = Date.now() + this.config.leaseDurationMs;
    const result = this.db
      .prepare('UPDATE checkouts SET expires_at = ? WHERE task_id = ?')
      .run(newExpiry, taskId);
    return result.changes > 0;
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
      CREATE TABLE IF NOT EXISTS checkouts (
        task_id TEXT PRIMARY KEY,
        agent_id TEXT NOT NULL,
        checked_out_at INTEGER NOT NULL,
        expires_at INTEGER NOT NULL
      );
      CREATE INDEX IF NOT EXISTS idx_checkouts_agent ON checkouts(agent_id);
      CREATE INDEX IF NOT EXISTS idx_checkouts_expires ON checkouts(expires_at);
    `);
  }
  getCheckedOutTasksSync() {
    const now = Date.now();
    const rows = this.db
      .prepare('SELECT * FROM checkouts WHERE expires_at > ? ORDER BY checked_out_at ASC')
      .all(now);
    return rows.map((r) => ({
      taskId: r.task_id,
      agentId: r.agent_id,
      checkedOutAt: r.checked_out_at,
      expiresAt: r.expires_at,
    }));
  }
  getExpiredCount() {
    const now = Date.now();
    const row = this.db
      .prepare('SELECT COUNT(*) as c FROM checkouts WHERE expires_at <= ?')
      .get(now);
    return row.c;
  }
  releaseExpired() {
    const now = Date.now();
    const result = this.db.prepare('DELETE FROM checkouts WHERE expires_at <= ?').run(now);
    if (result.changes > 0) {
      this.ctx.logger.debug('Released expired checkouts', { count: result.changes });
    }
    return result.changes;
  }
}
exports.default = new TaskCheckoutFeature();
