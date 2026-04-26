"use strict";
/**
 * Budget Enforcement Feature
 *
 * Tracks token usage and cost per agent with SQLite persistence.
 * Supports monthly/daily limits, per-agent caps, and configurable hard stops.
 *
 * Integration points:
 *  - Before LLM call: checkBudget(agentId) → reject if exhausted
 *  - After LLM call: recordCost(agentId, usage, provider, model) → store cost
 *  - Heartbeat: getBudgetStatus() → report if threshold reached
 *  - System prompt: getBudgetSummary() → inject budget status
 */
var __importDefault = (this && this.__importDefault) || function (mod) {
    return (mod && mod.__esModule) ? mod : { "default": mod };
};
Object.defineProperty(exports, "__esModule", { value: true });
exports._MODEL_PRICING_MAP = exports.MODEL_PRICING = void 0;
exports.calculateCost = calculateCost;
exports.validateBudgetConfig = validateBudgetConfig;
const fs_1 = require("fs");
const path_1 = require("path");
const better_sqlite3_1 = __importDefault(require("better-sqlite3"));
// ─── Model Pricing ────────────────────────────────────────────────────────────
var types_1 = require("./types");
Object.defineProperty(exports, "MODEL_PRICING", { enumerable: true, get: function () { return types_1.MODEL_PRICING; } });
exports._MODEL_PRICING_MAP = {
    'minimax/m2': { input: 0.1, output: 0.5 },
    'openai/gpt-4o': { input: 2.5, output: 10 },
    'openai/gpt-4o-mini': { input: 0.15, output: 0.6 },
    'openai/gpt-4-turbo': { input: 10, output: 30 },
    'anthropic/claude-sonnet-4-20250514': { input: 3, output: 15 },
    'anthropic/claude-opus-4-20250514': { input: 15, output: 75 },
    'anthropic/claude-3-5-sonnet-latest': { input: 3, output: 15 },
    'anthropic/claude-3-5-haiku-latest': { input: 0.8, output: 4 },
    'google/gemini-2.5-flash': { input: 0.1, output: 0.4 },
    'google/gemini-2.5-pro': { input: 1.25, output: 5 },
    'nvidia/*': { input: 0, output: 0 },
    'openrouter/free': { input: 0, output: 0 },
    'deepseek-ai/deepseek-v3.2': { input: 0.1, output: 0.5 },
};
/**
 * Calculate cost in USD for a given model and token usage.
 * Falls back to wildcard patterns then to 'unknown' pricing.
 */
function calculateCost(model, usage) {
    const pricing = exports._MODEL_PRICING_MAP[model];
    if (pricing) {
        return ((usage.promptTokens * pricing.input + usage.completionTokens * pricing.output) / 1_000_000);
    }
    for (const [key, val] of Object.entries(exports._MODEL_PRICING_MAP)) {
        if (key.endsWith('/*')) {
            const prefix = key.slice(0, -2);
            if (model.startsWith(prefix)) {
                return (usage.promptTokens * val.input + usage.completionTokens * val.output) / 1_000_000;
            }
        }
    }
    const fallback = exports._MODEL_PRICING_MAP['unknown'] ?? { input: 0.5, output: 2 };
    return ((usage.promptTokens * fallback.input + usage.completionTokens * fallback.output) / 1_000_000);
}
function validateBudgetConfig(input) {
    if (input.globalMonthlyLimit !== undefined &&
        (typeof input.globalMonthlyLimit !== 'number' || input.globalMonthlyLimit < 0)) {
        return {
            valid: false,
            error: 'globalMonthlyLimit must be a non-negative number',
        };
    }
    if (input.globalDailyLimit !== undefined &&
        (typeof input.globalDailyLimit !== 'number' || input.globalDailyLimit < 0)) {
        return {
            valid: false,
            error: 'globalDailyLimit must be a non-negative number',
        };
    }
    if (input.perAgentLimit !== undefined &&
        (typeof input.perAgentLimit !== 'number' || input.perAgentLimit < 0)) {
        return {
            valid: false,
            error: 'perAgentLimit must be a non-negative number',
        };
    }
    if (input.alertThreshold !== undefined &&
        (typeof input.alertThreshold !== 'number' ||
            input.alertThreshold < 0 ||
            input.alertThreshold > 1)) {
        return {
            valid: false,
            error: 'alertThreshold must be between 0 and 1',
        };
    }
    return { valid: true };
}
// ─── Feature ─────────────────────────────────────────────────────────────────
const DEFAULT_CONFIG = {
    enabled: false,
    dbPath: './data/budget.db',
    globalMonthlyLimit: 10,
    globalDailyLimit: 1,
    alertThreshold: 0.8,
    blockOnExhausted: true,
};
class BudgetFeature {
    meta = {
        name: 'budget',
        version: '0.0.2',
        description: 'Cost control and budget management for LLM usage with per-agent and global limits',
        dependencies: [],
    };
    config = { ...DEFAULT_CONFIG };
    db;
    ctx;
    alertedAgents = new Set();
    async init(config, context) {
        this.ctx = context;
        const input = config;
        this.config = {
            ...DEFAULT_CONFIG,
            ...(input.enabled !== undefined ? { enabled: input.enabled } : {}),
            ...(input.globalMonthlyLimit !== undefined
                ? { globalMonthlyLimit: input.globalMonthlyLimit }
                : {}),
            ...(input.globalDailyLimit !== undefined ? { globalDailyLimit: input.globalDailyLimit } : {}),
            ...(input.perAgentLimit !== undefined ? { perAgentLimit: input.perAgentLimit } : {}),
            ...(input.alertThreshold !== undefined ? { alertThreshold: input.alertThreshold } : {}),
            ...(input.blockOnExhausted !== undefined ? { blockOnExhausted: input.blockOnExhausted } : {}),
            ...(input.dbPath ? { dbPath: input.dbPath } : {}),
        };
        this.ensureDb();
    }
    async start() {
        this.log('info', 'Budget enforcement active', {
            globalMonthlyLimit: `$${this.config.globalMonthlyLimit}/month`,
            globalDailyLimit: `$${this.config.globalDailyLimit}/day`,
            perAgentLimit: this.config.perAgentLimit ? `$${this.config.perAgentLimit}/agent` : 'none',
            alertThreshold: `${Math.round(this.config.alertThreshold * 100)}%`,
            blockOnExhausted: this.config.blockOnExhausted,
        });
    }
    async stop() {
        this.db?.close();
    }
    async healthCheck() {
        this.ensureDb();
        try {
            const usage = this.getMonthlyUsage();
            const _daily = this.getDailyUsage();
            const healthy = usage.totalCost < this.config.globalMonthlyLimit &&
                _daily.totalCost < this.config.globalDailyLimit;
            return {
                healthy,
                message: healthy ? 'Budget OK' : 'Budget threshold exceeded',
                details: {
                    monthlyUsage: `$${usage.totalCost.toFixed(4)}`,
                    monthlyLimit: `$${this.config.globalMonthlyLimit}`,
                    dailyUsage: `$${_daily.totalCost.toFixed(4)}`,
                    dailyLimit: `$${this.config.globalDailyLimit}`,
                    percentUsed: Math.round((usage.totalCost / this.config.globalMonthlyLimit) * 100),
                },
            };
        }
        catch {
            return { healthy: false, message: 'Budget health check failed' };
        }
    }
    // ─── Configuration API ─────────────────────────────────────────────────
    updateConfig(input) {
        const validation = validateBudgetConfig(input);
        if (!validation.valid) {
            return { success: false, error: validation.error };
        }
        this.ensureDb();
        if (input.globalMonthlyLimit !== undefined) {
            this.config.globalMonthlyLimit = input.globalMonthlyLimit;
        }
        if (input.globalDailyLimit !== undefined) {
            this.config.globalDailyLimit = input.globalDailyLimit;
        }
        if (input.perAgentLimit !== undefined) {
            this.config.perAgentLimit = input.perAgentLimit;
        }
        if (input.alertThreshold !== undefined) {
            this.config.alertThreshold = input.alertThreshold;
        }
        if (input.blockOnExhausted !== undefined) {
            this.config.blockOnExhausted = input.blockOnExhausted;
        }
        this.db
            .prepare('INSERT OR REPLACE INTO budget_config (key, value) VALUES (?, ?)')
            .run('config', JSON.stringify(this.config));
        this.log('info', 'Budget config updated', {
            config: this.config,
        });
        return { success: true };
    }
    getConfig() {
        this.ensureDb();
        return { ...this.config };
    }
    getConfigDisplay() {
        this.ensureDb();
        return {
            globalMonthlyLimit: {
                value: this.config.globalMonthlyLimit,
                default: DEFAULT_CONFIG.globalMonthlyLimit,
            },
            globalDailyLimit: {
                value: this.config.globalDailyLimit,
                default: DEFAULT_CONFIG.globalDailyLimit,
            },
            perAgentLimit: {
                value: this.config.perAgentLimit ?? DEFAULT_CONFIG.globalMonthlyLimit,
                default: DEFAULT_CONFIG.globalMonthlyLimit,
            },
            alertThreshold: {
                value: this.config.alertThreshold,
                default: DEFAULT_CONFIG.alertThreshold,
            },
            blockOnExhausted: {
                value: this.config.blockOnExhausted,
                default: DEFAULT_CONFIG.blockOnExhausted,
            },
            enabled: {
                value: this.config.enabled,
                default: DEFAULT_CONFIG.enabled,
            },
        };
    }
    // ─── Cost Recording ────────────────────────────────────────────────────
    recordCost(agentId, usage, provider, model) {
        this.ensureDb();
        const cost = calculateCost(model, usage);
        this.db
            .prepare(`INSERT INTO budget_usage (agent_id, provider, model, prompt_tokens, completion_tokens, cost_usd, timestamp)
         VALUES (?, ?, ?, ?, ?, ?, ?)`)
            .run(agentId, provider, model, usage.promptTokens, usage.completionTokens, cost, Date.now());
        this.checkAndAlert(agentId);
        return cost;
    }
    // ─── Budget Checking ───────────────────────────────────────────────────
    checkBudget(agentId) {
        this.ensureDb();
        const _daily = this.getAgentDailyUsage(agentId);
        const monthly = this.getAgentMonthlyUsage(agentId);
        const agentLimit = this.config.perAgentLimit ?? this.config.globalMonthlyLimit;
        const dailyAllowed = _daily.totalCost < this.config.globalDailyLimit;
        const monthlyAllowed = monthly.totalCost < this.config.globalMonthlyLimit;
        const agentAllowed = agentLimit === 0 || monthly.totalCost < agentLimit;
        const monthlyPercent = this.config.globalMonthlyLimit > 0 ? monthly.totalCost / this.config.globalMonthlyLimit : 1;
        const dailyPercent = this.config.globalDailyLimit > 0 ? _daily.totalCost / this.config.globalDailyLimit : 1;
        const maxPercent = Math.max(monthlyPercent, dailyPercent);
        const exhausted = !dailyAllowed || !monthlyAllowed || !agentAllowed;
        const allowed = !this.config.blockOnExhausted || !exhausted;
        let reason;
        if (!monthlyAllowed) {
            reason = `Monthly budget exhausted ($${monthly.totalCost.toFixed(4)}/$${this.config.globalMonthlyLimit})`;
        }
        else if (!dailyAllowed) {
            reason = `Daily budget exhausted ($${_daily.totalCost.toFixed(4)}/$${this.config.globalDailyLimit})`;
        }
        else if (!agentAllowed) {
            reason = `Agent budget exhausted ($${monthly.totalCost.toFixed(4)}/$${agentLimit})`;
        }
        return {
            allowed,
            reason,
            dailyCost: _daily.totalCost,
            monthlyCost: monthly.totalCost,
            dailyLimit: this.config.globalDailyLimit,
            monthlyLimit: this.config.globalMonthlyLimit,
            percentUsed: Math.round(maxPercent * 100),
            threshold: Math.round(this.config.alertThreshold * 100),
            blocked: exhausted && this.config.blockOnExhausted,
        };
    }
    canProceed(agentId) {
        return this.checkBudget(agentId).allowed;
    }
    // ─── Usage Queries ─────────────────────────────────────────────────────
    getUsage(agentId) {
        this.ensureDb();
        const _daily = this.getAgentDailyUsage(agentId);
        const monthly = this.getAgentMonthlyUsage(agentId);
        const agentLimit = this.config.perAgentLimit ?? this.config.globalMonthlyLimit;
        return {
            agent: agentId,
            totalTokens: monthly.totalTokens,
            totalCost: monthly.totalCost,
            limit: agentLimit,
            percentUsed: agentLimit > 0 ? Math.round((monthly.totalCost / agentLimit) * 100) : 0,
            canProceed: monthly.totalCost < agentLimit,
            period: 'monthly',
        };
    }
    getBudgetReport() {
        this.ensureDb();
        const agents = this.db.prepare('SELECT DISTINCT agent_id FROM budget_usage').all();
        const byAgent = agents.map((a) => this.getUsage(a.agent_id));
        const monthly = this.getMonthlyUsage();
        const _daily = this.getDailyUsage();
        const alerts = [];
        for (const status of byAgent) {
            if (status.percentUsed >= this.config.alertThreshold * 100) {
                alerts.push(`${status.agent}: ${status.percentUsed}% of limit ($${status.totalCost.toFixed(4)})`);
            }
        }
        if (this.config.globalMonthlyLimit > 0 &&
            monthly.totalCost / this.config.globalMonthlyLimit >= this.config.alertThreshold) {
            alerts.unshift(`GLOBAL: ${Math.round((monthly.totalCost / this.config.globalMonthlyLimit) * 100)}% of monthly budget used`);
        }
        const currentMonth = new Date().toISOString().slice(0, 7);
        const currentDay = new Date().toISOString().slice(0, 10);
        return {
            period: currentMonth,
            periodDay: currentDay,
            totalTokens: monthly.totalTokens,
            totalCost: monthly.totalCost,
            dailyCost: _daily.totalCost,
            monthlyCost: monthly.totalCost,
            byAgent,
            alerts,
            dailyLimit: this.config.globalDailyLimit,
            monthlyLimit: this.config.globalMonthlyLimit,
        };
    }
    getHistory(days = 30) {
        this.ensureDb();
        const cutoff = Date.now() - days * 24 * 60 * 60 * 1000;
        const rows = this.db
            .prepare(`SELECT
           date(timestamp/1000, 'unixepoch') as date,
           SUM(cost_usd) as total_cost,
           SUM(prompt_tokens + completion_tokens) as total_tokens,
           COUNT(*) as request_count
         FROM budget_usage
         WHERE timestamp >= ?
         GROUP BY date(timestamp/1000, 'unixepoch')
         ORDER BY date DESC`)
            .all(cutoff);
        return rows.map((r) => ({
            date: r.date,
            totalCost: r.total_cost,
            totalTokens: r.total_tokens,
            requestCount: r.request_count,
        }));
    }
    reset() {
        this.ensureDb();
        this.db.prepare('DELETE FROM budget_usage').run();
        this.alertedAgents.clear();
        this.log('info', 'Budget usage reset');
    }
    resetAgent(agentId) {
        this.ensureDb();
        const info = this.db.prepare('DELETE FROM budget_usage WHERE agent_id = ?').run(agentId);
        this.alertedAgents.delete(agentId);
        return { changes: info.changes };
    }
    getBudgetSummary() {
        const report = this.getBudgetReport();
        const lines = [
            '## Budget Status',
            `Monthly: $${report.monthlyCost.toFixed(4)} / $${report.monthlyLimit} (${Math.round((report.monthlyCost / report.monthlyLimit) * 100)}%)`,
            `Daily: $${report.dailyCost.toFixed(4)} / $${report.dailyLimit} (${Math.round((report.dailyCost / report.dailyLimit) * 100)}%)`,
        ];
        if (report.alerts.length > 0) {
            lines.push('Alerts:');
            for (const alert of report.alerts) {
                lines.push(`  - ${alert}`);
            }
        }
        return lines.join('\n');
    }
    // ─── Database ──────────────────────────────────────────────────────────
    ensureDb() {
        if (!this.db) {
            this.initDatabase();
        }
    }
    log(level, msg, meta) {
        if (this.ctx?.logger) {
            this.ctx.logger[level](msg, meta);
        }
    }
    initDatabase() {
        const fullPath = (0, path_1.resolve)(this.config.dbPath);
        if (!(0, fs_1.existsSync)((0, path_1.dirname)(fullPath))) {
            (0, fs_1.mkdirSync)((0, path_1.dirname)(fullPath), { recursive: true });
        }
        this.db = new better_sqlite3_1.default(fullPath);
        this.db.pragma('journal_mode = WAL');
        this.db.pragma('synchronous = NORMAL');
        this.db.exec(`
      CREATE TABLE IF NOT EXISTS budget_usage (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        agent_id TEXT NOT NULL,
        provider TEXT NOT NULL DEFAULT '',
        model TEXT NOT NULL DEFAULT '',
        prompt_tokens INTEGER NOT NULL DEFAULT 0,
        completion_tokens INTEGER NOT NULL DEFAULT 0,
        cost_usd REAL NOT NULL DEFAULT 0,
        timestamp INTEGER NOT NULL
      );

      CREATE INDEX IF NOT EXISTS idx_budget_agent ON budget_usage(agent_id);
      CREATE INDEX IF NOT EXISTS idx_budget_timestamp ON budget_usage(timestamp);
      CREATE INDEX IF NOT EXISTS idx_budget_agent_time ON budget_usage(agent_id, timestamp);

      CREATE TABLE IF NOT EXISTS budget_config (
        key TEXT PRIMARY KEY,
        value TEXT NOT NULL
      );
    `);
        const savedConfig = this.db
            .prepare('SELECT value FROM budget_config WHERE key = ?')
            .get('config');
        if (savedConfig) {
            try {
                const parsed = JSON.parse(savedConfig.value);
                this.config = { ...this.config, ...parsed };
            }
            catch {
                /* ignore parse errors */
            }
        }
    }
    getAgentDailyUsage(agentId) {
        const dayStart = this.getDayStart();
        const row = this.db
            .prepare(`SELECT
           COALESCE(SUM(prompt_tokens + completion_tokens), 0) as tokens,
           COALESCE(SUM(cost_usd), 0) as cost
         FROM budget_usage
         WHERE agent_id = ? AND timestamp >= ?`)
            .get(agentId, dayStart);
        return { totalTokens: row.tokens, totalCost: row.cost };
    }
    getAgentMonthlyUsage(agentId) {
        const monthStart = this.getMonthStart();
        const row = this.db
            .prepare(`SELECT
           COALESCE(SUM(prompt_tokens + completion_tokens), 0) as tokens,
           COALESCE(SUM(cost_usd), 0) as cost
         FROM budget_usage
         WHERE agent_id = ? AND timestamp >= ?`)
            .get(agentId, monthStart);
        return { totalTokens: row.tokens, totalCost: row.cost };
    }
    getDailyUsage() {
        const dayStart = this.getDayStart();
        const row = this.db
            .prepare(`SELECT
           COALESCE(SUM(prompt_tokens + completion_tokens), 0) as tokens,
           COALESCE(SUM(cost_usd), 0) as cost
         FROM budget_usage
         WHERE timestamp >= ?`)
            .get(dayStart);
        return { totalTokens: row.tokens, totalCost: row.cost };
    }
    getMonthlyUsage() {
        const monthStart = this.getMonthStart();
        const row = this.db
            .prepare(`SELECT
           COALESCE(SUM(prompt_tokens + completion_tokens), 0) as tokens,
           COALESCE(SUM(cost_usd), 0) as cost
         FROM budget_usage
         WHERE timestamp >= ?`)
            .get(monthStart);
        return { totalTokens: row.tokens, totalCost: row.cost };
    }
    getDayStart() {
        const now = new Date();
        return new Date(now.getFullYear(), now.getMonth(), now.getDate()).getTime();
    }
    getMonthStart() {
        const now = new Date();
        return new Date(now.getFullYear(), now.getMonth(), 1).getTime();
    }
    checkAndAlert(agentId) {
        const status = this.checkBudget(agentId);
        if (status.percentUsed >= this.config.alertThreshold * 100 &&
            !this.alertedAgents.has(agentId)) {
            this.alertedAgents.add(agentId);
            this.log('warn', `Budget alert: ${agentId} at ${status.percentUsed}% of threshold`, {
                agentId,
                monthlyCost: status.monthlyCost,
                monthlyLimit: status.monthlyLimit,
                dailyCost: status.dailyCost,
                dailyLimit: status.dailyLimit,
            });
        }
    }
}
exports.default = new BudgetFeature();
//# sourceMappingURL=index.js.map