"use strict";
/**
 * Budget Enforcement Feature
 *
 * Tracks token usage and cost per agent with SQLite persistence.
 * Supports monthly limits, per-agent caps, and hard stops.
 */
var __importDefault = (this && this.__importDefault) || function (mod) {
    return (mod && mod.__esModule) ? mod : { "default": mod };
};
Object.defineProperty(exports, "__esModule", { value: true });
const better_sqlite3_1 = __importDefault(require("better-sqlite3"));
const fs_1 = require("fs");
const path_1 = require("path");
// ─── Feature ─────────────────────────────────────────────────────────────────
class BudgetFeature {
    constructor() {
        this.meta = {
            name: 'budget',
            version: '0.1.0',
            description: 'Token usage budget enforcement with per-agent limits',
            dependencies: [],
        };
        this.config = {
            enabled: false,
            dbPath: './data/budget.db',
            monthlyLimit: 1000000,
            perAgentLimits: {},
            alertThreshold: 80,
            hardStop: true,
        };
        this.alertedAgents = new Set();
    }
    async init(config, context) {
        this.ctx = context;
        this.config = { ...this.config, ...config };
        this.initDatabase();
    }
    async start() {
        this.ctx.logger.info('Budget enforcement active', {
            monthlyLimit: this.config.monthlyLimit,
            hardStop: this.config.hardStop,
            alertThreshold: this.config.alertThreshold,
            perAgentLimits: Object.keys(this.config.perAgentLimits).length,
        });
    }
    async stop() {
        this.db?.close();
    }
    async healthCheck() {
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
    recordUsage(agent, tokens, cost) {
        const stmt = this.db.prepare('INSERT INTO budget_log (agent, tokens, cost, timestamp) VALUES (?, ?, ?, ?)');
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
    getUsage(agent) {
        if (agent) {
            return this.getAgentUsage(agent);
        }
        // Aggregate for all agents
        const row = this.db.prepare('SELECT COALESCE(SUM(tokens), 0) as tokens, COALESCE(SUM(cost), 0) as cost FROM budget_log WHERE timestamp >= ?').get(this.getMonthStart());
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
    canProceed(agent) {
        const status = this.getUsage(agent);
        if (this.config.hardStop && !status.canProceed) {
            return false;
        }
        return true;
    }
    /** Get monthly report with all agents */
    getMonthlyReport() {
        const agents = this.db.prepare('SELECT DISTINCT agent FROM budget_log WHERE timestamp >= ?').all(this.getMonthStart());
        const byAgent = agents.map(a => this.getAgentUsage(a.agent));
        const alerts = [];
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
    getLogEntries(from, to) {
        const fromTs = from ?? this.getMonthStart();
        const toTs = to ?? Date.now();
        return this.db.prepare('SELECT * FROM budget_log WHERE timestamp BETWEEN ? AND ? ORDER BY timestamp DESC').all(fromTs, toTs);
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
    getAgentUsage(agent) {
        const row = this.db.prepare('SELECT COALESCE(SUM(tokens), 0) as tokens, COALESCE(SUM(cost), 0) as cost FROM budget_log WHERE agent = ? AND timestamp >= ?').get(agent, this.getMonthStart());
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
    getMonthlyUsage() {
        const row = this.db.prepare('SELECT COALESCE(SUM(tokens), 0) as tokens, COALESCE(SUM(cost), 0) as cost FROM budget_log WHERE timestamp >= ?').get(this.getMonthStart());
        return { totalTokens: row.tokens, totalCost: row.cost };
    }
    getMonthStart() {
        const now = new Date();
        return new Date(now.getFullYear(), now.getMonth(), 1).getTime();
    }
}
exports.default = new BudgetFeature();
