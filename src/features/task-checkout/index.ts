/**
 * Atomic Task Checkout Feature
 *
 * Prevents multiple agents from working on the same task simultaneously.
 * Uses SQLite atomic UPDATE with row locking for safety.
 */

import { mkdirSync, existsSync } from 'fs';
import { dirname, resolve } from 'path';

import Database from 'better-sqlite3';

import {
  type FeatureModule,
  type FeatureContext,
  type FeatureMeta,
  type HealthStatus,
} from '../../core/plugin-loader';

// ─── Interfaces ──────────────────────────────────────────────────────────────

export interface TaskAssignment {
  taskId: string;
  agentId: string;
  checkedOutAt: number;
  expiresAt: number;
}

export interface TaskCheckoutConfig {
  enabled: boolean;
  dbPath: string;
  leaseDurationMs: number; // How long a checkout lasts before auto-expire
  maxLeasesPerAgent: number;
}

interface TaskRow {
  task_id: string;
  agent_id: string;
  checked_out_at: number;
  expires_at: number;
}

// ─── Feature ─────────────────────────────────────────────────────────────────

class TaskCheckoutFeature implements FeatureModule {
  readonly meta: FeatureMeta = {
    name: 'task-checkout',
    version: '0.0.4',
    description: 'Atomic task checkout to prevent agent conflicts',
    dependencies: [],
  };

  private config: TaskCheckoutConfig = {
    enabled: false,
    dbPath: './data/task-checkout.db',
    leaseDurationMs: 30 * 60 * 1000, // 30 minutes
    maxLeasesPerAgent: 10,
  };
  private db!: Database.Database;
  private ctx!: FeatureContext;
  private cleanupTimer: ReturnType<typeof setInterval> | null = null;

  async init(config: Record<string, unknown>, context: FeatureContext): Promise<void> {
    this.ctx = context;
    this.config = { ...this.config, ...(config as Partial<TaskCheckoutConfig>) };
    this.initDatabase();
  }

  async start(): Promise<void> {
    // Start periodic cleanup of expired checkouts
    this.cleanupTimer = setInterval(() => {
      this.releaseExpired();
    }, 60_000); // Every minute

    const checkedOut = this.getCheckedOutTasksSync();
    this.ctx.logger.info('Task checkout active', {
      leaseDuration: `${this.config.leaseDurationMs / 1000}s`,
      activeCheckouts: checkedOut.length,
    });
  }

  async stop(): Promise<void> {
    if (this.cleanupTimer) {
      clearInterval(this.cleanupTimer);
      this.cleanupTimer = null;
    }
    this.db?.close();
  }

  async healthCheck(): Promise<HealthStatus> {
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
  async checkout(taskId: string, agentId: string): Promise<boolean> {
    // Check agent lease limit
    const agentLeases = this.db
      .prepare('SELECT COUNT(*) as c FROM checkouts WHERE agent_id = ?')
      .get(agentId) as { c: number };

    if (agentLeases.c >= this.config.maxLeasesPerAgent) {
      this.ctx.logger.warn('Agent lease limit reached', { agentId, leases: agentLeases.c });
      return false;
    }

    const now = Date.now();
    const expiresAt = now + this.config.leaseDurationMs;

    // Atomic insert — fails if task already checked out (UNIQUE constraint)
    try {
      const stmt = this.db.prepare(
        `INSERT INTO checkouts (task_id, agent_id, checked_out_at, expires_at)
         VALUES (?, ?, ?, ?)`,
      );
      stmt.run(taskId, agentId, now, expiresAt);

      this.ctx.logger.info('Task checked out', { taskId, agentId, expiresAt });
      return true;
    } catch (err: unknown) {
      // UNIQUE constraint violation — task already checked out
      const e = err as { code?: string; message?: string };
      if (e.code === 'SQLITE_CONSTRAINT_UNIQUE' || e.message?.includes('UNIQUE')) {
        // Check if the existing checkout is expired
        const existing = this.db
          .prepare('SELECT * FROM checkouts WHERE task_id = ?')
          .get(taskId) as TaskRow | undefined;

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
  async release(taskId: string): Promise<void> {
    const result = this.db.prepare('DELETE FROM checkouts WHERE task_id = ?').run(taskId);
    if (result.changes > 0) {
      this.ctx.logger.info('Task released', { taskId });
    }
  }

  /** Release all tasks for an agent */
  async releaseAgent(agentId: string): Promise<number> {
    const result = this.db.prepare('DELETE FROM checkouts WHERE agent_id = ?').run(agentId);
    this.ctx.logger.info('Agent releases all tasks', { agentId, released: result.changes });
    return result.changes;
  }

  /** Get all currently checked out tasks */
  async getCheckedOutTasks(): Promise<TaskAssignment[]> {
    return this.getCheckedOutTasksSync();
  }

  /** Check if a task is available for checkout */
  async isAvailable(taskId: string): Promise<boolean> {
    const row = this.db.prepare('SELECT * FROM checkouts WHERE task_id = ?').get(taskId) as
      | TaskRow
      | undefined;

    if (!row) return true;
    return row.expires_at < Date.now();
  }

  /** Get tasks checked out by a specific agent */
  async getAgentTasks(agentId: string): Promise<TaskAssignment[]> {
    const rows = this.db
      .prepare('SELECT * FROM checkouts WHERE agent_id = ? ORDER BY checked_out_at ASC')
      .all(agentId) as TaskRow[];

    return rows.map((r) => ({
      taskId: r.task_id,
      agentId: r.agent_id,
      checkedOutAt: r.checked_out_at,
      expiresAt: r.expires_at,
    }));
  }

  /** Extend a lease */
  async extendLease(taskId: string): Promise<boolean> {
    const newExpiry = Date.now() + this.config.leaseDurationMs;
    const result = this.db
      .prepare('UPDATE checkouts SET expires_at = ? WHERE task_id = ?')
      .run(newExpiry, taskId);
    return result.changes > 0;
  }

  // ─── Private helpers ─────────────────────────────────────────────────────

  private initDatabase(): void {
    const fullPath = resolve(this.config.dbPath);
    if (!existsSync(dirname(fullPath))) {
      mkdirSync(dirname(fullPath), { recursive: true });
    }

    this.db = new Database(fullPath);
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

  private getCheckedOutTasksSync(): TaskAssignment[] {
    const now = Date.now();
    const rows = this.db
      .prepare('SELECT * FROM checkouts WHERE expires_at > ? ORDER BY checked_out_at ASC')
      .all(now) as TaskRow[];

    return rows.map((r) => ({
      taskId: r.task_id,
      agentId: r.agent_id,
      checkedOutAt: r.checked_out_at,
      expiresAt: r.expires_at,
    }));
  }

  private getExpiredCount(): number {
    const now = Date.now();
    const row = this.db
      .prepare('SELECT COUNT(*) as c FROM checkouts WHERE expires_at <= ?')
      .get(now) as { c: number };
    return row.c;
  }

  private releaseExpired(): number {
    const now = Date.now();
    const result = this.db.prepare('DELETE FROM checkouts WHERE expires_at <= ?').run(now);
    if (result.changes > 0) {
      this.ctx.logger.debug('Released expired checkouts', { count: result.changes });
    }
    return result.changes;
  }
}

export default new TaskCheckoutFeature();
