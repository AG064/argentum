/**
 * Budget Enforcement Feature
 *
 * Tracks token usage and cost per agent with SQLite persistence.
 * Supports monthly limits, per-agent caps, and hard stops.
 */

import { mkdirSync, existsSync } from 'fs';
import { dirname, resolve } from 'path';

import Database from 'better-sqlite3';

import { type FeatureModule, type FeatureContext, type FeatureMeta, type HealthStatus } from '../../core/plugin-loader';

// ─── Interfaces ──────────────────────────────────────────────────────────────

export interface BudgetConfig {
  enabled: boolean;
  dbPath: string;
  monthlyLimit: number;
  perAgentLimits: Record<string, number>;
  alertThreshold: number;
  hardStop: boolean;
  alertCallback?: (agent: string, usage: BudgetStatus) => void;
}

export interface BudgetStatus {
  agent: string;
  totalTokens: number;
  totalCost: number;
  limit: number;
  percentUsed: number;
  canProceed: boolean;
}

export interface BudgetReport {
  period: string;
  totalTokens: number;
  totalCost: number;
  byAgent: BudgetStatus[];
  alerts: string[];
}

interface BudgetLogRow {
  id: number;
  agent: string;
  tokens: number;
  cost: number;
  timestamp: number;
}

// ─── Feature ─────────────────────────────────────────────────────────────────

class BudgetFeature implements FeatureModule {
  readonly meta: FeatureMeta = {
    name: 'budget',
    version: '0.1.0',
    description: 'Token usage budget enforcement with per-agent limits',
    dependencies: [],
  };

  private config: BudgetConfig = {
    enabled: false,
    dbPath: './data/budget.db',
    monthlyLimit: 1_000_000,
    perAgentLimits: {},
    alertThreshold: 80,
    hardStop: true,
  };
  private db!: Database.Database;
  private ctx!: FeatureContext;
  private alertedAgents: Set<string> = new Set();

  async init(config: Record<string, unknown>, context: FeatureContext): Promise<void> {
    this.ctx = context;
    this.config = { ...this.config, ...(config as Partial<BudgetConfig>) };
    this.initDatabase();
  }

  async start(): Promise<void> {
    this.ctx.logger.info('Budget enforcement active', {
      monthlyLimit: this.config.monthlyLimit,
      hardStop: this.config.hardStop,
      alertThreshold: this.config.alertThreshold,
      perAgentLimits: Object.keys(this.config.perAgentLimits).length,
    });
  }

  async stop(): Promise<void> {
    this.db?.close();
  }

  async healthCheck(): Promise<HealthStatus> {
    const usage = this.getMonthlyUsage();
    return {
      healthy: usage.totalCost < this.config.monthlyLimit,
      details: {
        monthlyUsage: usage.totalCost,
        monthlyLimit: this.config.monthlyLimit,
        percentUsed: Math.round((usage.totalCost / this.config.monthlyLimit) * 100),
      },
    };
  }

  // ─── Public API ──────────────────────────────────────────────────────────

  /** Record token usage for an agent */
  recordUsage(agent: string, tokens: number, cost: number): void {
    const stmt = this.db.prepare(
      'INSERT INTO budget_log (agent, tokens, cost, timestamp) VALUES (?, ?, ?, ?)'
    );
    stmt.run(agent, tokens, cost, Date.now());

    // Check if alert threshold crossed
    const status = this.getUsage(agent);
    if (status.percentUsed >= this.config.alertThreshold && !this.alertedAgents.has(agent)) {
      this.alertedAgents.add(agent);
      this.ctx.logger.warn(`Budget alert: ${agent} at ${status.percentUsed}% of limit`, {
        agent,
        used: status.totalCost,
        limit: status.limit,
      });
      this.config.alertCallback?.(agent, status);
    }
  }

  /** Get usage status for an agent (or all agents if omitted) */
  getUsage(agent?: string): BudgetStatus {
    if (agent) {
      return this.getAgentUsage(agent);
    }
    // Aggregate for all agents
    const row = this.db.prepare(
      'SELECT COALESCE(SUM(tokens), 0) as tokens, COALESCE(SUM(cost), 0) as cost FROM budget_log WHERE timestamp >= ?'
    ).get(this.getMonthStart()) as { tokens: number; cost: number };

    const limit = this.config.monthlyLimit;
    return {
      agent: '*',
      totalTokens: row.tokens,
      totalCost: row.cost,
      limit,
      percentUsed: limit > 0 ? Math.round((row.cost / limit) * 100) : 0,
      canProceed: !this.config.hardStop || row.cost < limit,
    };
  }

  /** Check if an agent can proceed based on budget */
  canProceed(agent: string): boolean {
    const status = this.getUsage(agent);
    if (this.config.hardStop && !status.canProceed) {
      return false;
    }
    return true;
  }

  /** Get monthly report with all agents */
  getMonthlyReport(): BudgetReport {
    const agents = this.db.prepare(
      'SELECT DISTINCT agent FROM budget_log WHERE timestamp >= ?'
    ).all(this.getMonthStart()) as Array<{ agent: string }>;

    const byAgent = agents.map(a => this.getAgentUsage(a.agent));
    const alerts: string[] = [];

    for (const status of byAgent) {
      if (status.percentUsed >= this.config.alertThreshold) {
        alerts.push(`${status.agent}: ${status.percentUsed}% of limit used`);
      }
    }

    const total = this.getMonthlyUsage();

    return {
      period: new Date().toISOString().slice(0, 7),
      totalTokens: total.totalTokens,
      totalCost: total.totalCost,
      byAgent,
      alerts,
    };
  }

  /** Get budget log rows for a date range */
  getLogEntries(from?: number, to?: number): BudgetLogRow[] {
    const fromTs = from ?? this.getMonthStart();
    const toTs = to ?? Date.now();
    return this.db.prepare(
      'SELECT * FROM budget_log WHERE timestamp BETWEEN ? AND ? ORDER BY timestamp DESC'
    ).all(fromTs, toTs) as BudgetLogRow[];
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
      CREATE TABLE IF NOT EXISTS budget_log (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        agent TEXT NOT NULL,
        tokens INTEGER NOT NULL,
        cost REAL NOT NULL,
        timestamp INTEGER NOT NULL
      );
      CREATE INDEX IF NOT EXISTS idx_budget_agent ON budget_log(agent);
      CREATE INDEX IF NOT EXISTS idx_budget_timestamp ON budget_log(timestamp);
    `);
  }

  private getAgentUsage(agent: string): BudgetStatus {
    const row = this.db.prepare(
      'SELECT COALESCE(SUM(tokens), 0) as tokens, COALESCE(SUM(cost), 0) as cost FROM budget_log WHERE agent = ? AND timestamp >= ?'
    ).get(agent, this.getMonthStart()) as { tokens: number; cost: number };

    const limit = this.config.perAgentLimits[agent] ?? this.config.monthlyLimit;

    return {
      agent,
      totalTokens: row.tokens,
      totalCost: row.cost,
      limit,
      percentUsed: limit > 0 ? Math.round((row.cost / limit) * 100) : 0,
      canProceed: !this.config.hardStop || row.cost < limit,
    };
  }

  private getMonthlyUsage(): { totalTokens: number; totalCost: number } {
    const row = this.db.prepare(
      'SELECT COALESCE(SUM(tokens), 0) as tokens, COALESCE(SUM(cost), 0) as cost FROM budget_log WHERE timestamp >= ?'
    ).get(this.getMonthStart()) as { tokens: number; cost: number };
    return { totalTokens: row.tokens, totalCost: row.cost };
  }

  private getMonthStart(): number {
    const now = new Date();
    return new Date(now.getFullYear(), now.getMonth(), 1).getTime();
  }
}

export default new BudgetFeature();
