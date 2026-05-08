"use strict";
/**
 * Goal Hierarchy Feature
 *
 * Tree-structured goal management with task linking and progress tracking.
 * Supports parent-child relationships and metrics.
 */
var __importDefault = (this && this.__importDefault) || function (mod) {
    return (mod && mod.__esModule) ? mod : { "default": mod };
};
Object.defineProperty(exports, "__esModule", { value: true });
const crypto_1 = require("crypto");
const fs_1 = require("fs");
const path_1 = require("path");
const better_sqlite3_1 = __importDefault(require("better-sqlite3"));
// ─── Feature ─────────────────────────────────────────────────────────────────
class GoalsFeature {
    meta = {
        name: 'goals',
        version: '0.0.5',
        description: 'Hierarchical goal management with task linking',
        dependencies: [],
    };
    config = {
        enabled: false,
        dbPath: './data/goals.db',
    };
    db;
    ctx;
    async init(config, context) {
        this.ctx = context;
        this.config = { ...this.config, ...config };
        this.initDatabase();
    }
    async start() {
        const goalCount = this.db.prepare('SELECT COUNT(*) as c FROM goals').get().c;
        this.ctx.logger.info('Goals feature active', { goals: goalCount });
    }
    async stop() {
        this.db?.close();
    }
    async healthCheck() {
        const goals = this.db.prepare('SELECT COUNT(*) as c FROM goals').get();
        const active = this.db
            .prepare("SELECT COUNT(*) as c FROM goals WHERE status = 'active'")
            .get();
        return {
            healthy: true,
            details: { totalGoals: goals.c, activeGoals: active.c },
        };
    }
    // ─── Public API ──────────────────────────────────────────────────────────
    /** Create a new goal */
    async createGoal(title, description, parentId) {
        if (parentId) {
            const parent = this.db.prepare('SELECT id FROM goals WHERE id = ?').get(parentId);
            if (!parent)
                throw new Error(`Parent goal not found: ${parentId}`);
        }
        const id = (0, crypto_1.randomUUID)();
        const now = Date.now();
        const goal = {
            id,
            title,
            description,
            parentId: parentId ?? null,
            status: 'active',
            metrics: {},
            createdAt: now,
            updatedAt: now,
        };
        this.db
            .prepare(`INSERT INTO goals (id, title, description, parent_id, status, metrics, created_at, updated_at)
       VALUES (?, ?, ?, ?, ?, ?, ?, ?)`)
            .run(id, title, description, parentId ?? null, 'active', '{}', now, now);
        this.ctx.logger.info('Goal created', { id, title, parentId });
        return goal;
    }
    /** Link a task to a goal */
    async linkTaskToGoal(taskId, goalId) {
        const goal = this.db.prepare('SELECT id FROM goals WHERE id = ?').get(goalId);
        if (!goal)
            throw new Error(`Goal not found: ${goalId}`);
        const now = Date.now();
        this.db
            .prepare(`INSERT OR REPLACE INTO task_goals (task_id, goal_id, linked_at) VALUES (?, ?, ?)`)
            .run(taskId, goalId, now);
        this.ctx.logger.debug('Task linked to goal', { taskId, goalId });
    }
    /** Get full goal tree */
    async getGoalTree() {
        const rows = this.db.prepare('SELECT * FROM goals ORDER BY created_at ASC').all();
        const taskCounts = this.getTaskCounts();
        const goalMap = new Map();
        const roots = [];
        // Build nodes
        for (const row of rows) {
            const node = {
                id: row.id,
                title: row.title,
                description: row.description,
                parentId: row.parent_id,
                status: row.status,
                metrics: this.parseJson(row.metrics),
                createdAt: row.created_at,
                updatedAt: row.updated_at,
                children: [],
                taskCount: taskCounts.get(row.id)?.total ?? 0,
                completedTasks: taskCounts.get(row.id)?.completed ?? 0,
            };
            goalMap.set(row.id, node);
        }
        // Build tree
        for (const [, node] of goalMap) {
            if (node.parentId && goalMap.has(node.parentId)) {
                goalMap.get(node.parentId).children.push(node);
            }
            else {
                roots.push(node);
            }
        }
        return roots;
    }
    /** Get progress report for a goal */
    async getGoalProgress(goalId) {
        const row = this.db.prepare('SELECT * FROM goals WHERE id = ?').get(goalId);
        if (!row)
            throw new Error(`Goal not found: ${goalId}`);
        const taskCounts = this.db
            .prepare(`SELECT
        COUNT(*) as total,
        SUM(CASE WHEN t.status = 'completed' THEN 1 ELSE 0 END) as completed
       FROM task_goals tg
       LEFT JOIN tasks t ON t.id = tg.task_id
       WHERE tg.goal_id = ?`)
            .get(goalId);
        const subGoalCount = this.db.prepare('SELECT COUNT(*) as c FROM goals WHERE parent_id = ?').get(goalId).c;
        const total = taskCounts.total ?? 0;
        const completed = taskCounts.completed ?? 0;
        const percent = total > 0 ? Math.round((completed / total) * 100) : 0;
        return {
            goalId,
            title: row.title,
            status: row.status,
            totalTasks: total,
            completedTasks: completed,
            subGoals: subGoalCount,
            percentComplete: percent,
            estimatedCompletion: null,
            metrics: this.parseJson(row.metrics),
        };
    }
    /** Update goal status */
    async updateGoalStatus(goalId, status) {
        const result = this.db
            .prepare('UPDATE goals SET status = ?, updated_at = ? WHERE id = ?')
            .run(status, Date.now(), goalId);
        if (result.changes === 0)
            throw new Error(`Goal not found: ${goalId}`);
        this.ctx.logger.info('Goal status updated', { goalId, status });
    }
    /** Update goal metrics */
    async updateGoalMetrics(goalId, metrics) {
        const existing = this.db.prepare('SELECT metrics FROM goals WHERE id = ?').get(goalId);
        if (!existing)
            throw new Error(`Goal not found: ${goalId}`);
        const merged = { ...this.parseJson(existing.metrics), ...metrics };
        this.db
            .prepare('UPDATE goals SET metrics = ?, updated_at = ? WHERE id = ?')
            .run(JSON.stringify(merged), Date.now(), goalId);
    }
    /** Get a single goal by ID */
    async getGoal(goalId) {
        const row = this.db.prepare('SELECT * FROM goals WHERE id = ?').get(goalId);
        if (!row)
            return null;
        return {
            id: row.id,
            title: row.title,
            description: row.description,
            parentId: row.parent_id,
            status: row.status,
            metrics: this.parseJson(row.metrics),
            createdAt: row.created_at,
            updatedAt: row.updated_at,
        };
    }
    /** List all goals */
    async listGoals(status) {
        const rows = status
            ? this.db
                .prepare('SELECT * FROM goals WHERE status = ? ORDER BY created_at DESC')
                .all(status)
            : this.db.prepare('SELECT * FROM goals ORDER BY created_at DESC').all();
        return rows.map((r) => ({
            id: r.id,
            title: r.title,
            description: r.description,
            parentId: r.parent_id,
            status: r.status,
            metrics: this.parseJson(r.metrics),
            createdAt: r.created_at,
            updatedAt: r.updated_at,
        }));
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
        this.db.pragma('foreign_keys = ON');
        this.db.exec(`
      CREATE TABLE IF NOT EXISTS goals (
        id TEXT PRIMARY KEY,
        title TEXT NOT NULL,
        description TEXT DEFAULT '',
        parent_id TEXT,
        status TEXT NOT NULL DEFAULT 'active',
        metrics TEXT DEFAULT '{}',
        created_at INTEGER NOT NULL,
        updated_at INTEGER NOT NULL,
        FOREIGN KEY (parent_id) REFERENCES goals(id) ON DELETE SET NULL
      );

      CREATE INDEX IF NOT EXISTS idx_goals_parent ON goals(parent_id);
      CREATE INDEX IF NOT EXISTS idx_goals_status ON goals(status);

      CREATE TABLE IF NOT EXISTS task_goals (
        task_id TEXT NOT NULL,
        goal_id TEXT NOT NULL,
        linked_at INTEGER NOT NULL,
        PRIMARY KEY (task_id, goal_id),
        FOREIGN KEY (goal_id) REFERENCES goals(id) ON DELETE CASCADE
      );

      CREATE INDEX IF NOT EXISTS idx_task_goals_goal ON task_goals(goal_id);

      -- Placeholder tasks table for progress tracking
      CREATE TABLE IF NOT EXISTS tasks (
        id TEXT PRIMARY KEY,
        title TEXT NOT NULL,
        status TEXT NOT NULL DEFAULT 'pending',
        created_at INTEGER NOT NULL
      );
    `);
    }
    getTaskCounts() {
        const rows = this.db
            .prepare(`SELECT tg.goal_id,
        COUNT(*) as total,
        SUM(CASE WHEN t.status = 'completed' THEN 1 ELSE 0 END) as completed
       FROM task_goals tg
       LEFT JOIN tasks t ON t.id = tg.task_id
       GROUP BY tg.goal_id`)
            .all();
        const map = new Map();
        for (const row of rows) {
            map.set(row.goal_id, { total: row.total, completed: row.completed ?? 0 });
        }
        return map;
    }
    parseJson(str) {
        try {
            return JSON.parse(str);
        }
        catch {
            return {};
        }
    }
}
exports.default = new GoalsFeature();
//# sourceMappingURL=index.js.map