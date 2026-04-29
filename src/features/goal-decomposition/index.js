'use strict';
/**
 * Goal Decomposition Feature (SQLite)
 *
 * Stores goals and tasks with parent-child relationships and dependencies.
 */
var __importDefault =
  (this && this.__importDefault) ||
  function (mod) {
    return mod && mod.__esModule ? mod : { default: mod };
  };
Object.defineProperty(exports, '__esModule', { value: true });
const better_sqlite3_1 = __importDefault(require('better-sqlite3'));
const crypto_1 = require('crypto');
const fs_1 = require('fs');
const path_1 = require('path');
const DEFAULT_CONFIG = {
  enabled: false,
  dbPath: './data/goal-decomposition.db',
};
class GoalDecompositionFeature {
  constructor() {
    this.meta = {
      name: 'goal-decomposition',
      version: '0.0.4',
      description: 'Decompose goals into tasks and track dependencies (SQLite)',
      dependencies: [],
    };
    this.config = { ...DEFAULT_CONFIG };
  }
  async init(config, context) {
    this.ctx = context;
    this.config = { ...this.config, ...config };
    this.initDatabase();
  }
  async start() {}
  async stop() {
    this.db?.close();
  }
  async healthCheck() {
    const goals = this.db.prepare('SELECT COUNT(DISTINCT goal_id) as c FROM tasks').get().c;
    const tasks = this.db.prepare('SELECT COUNT(*) as c FROM tasks').get().c;
    return { healthy: true, details: { goals, tasks } };
  }
  // ── API required by task ──────────────────────────────────────────────
  createGoal(title, description) {
    const id = (0, crypto_1.randomUUID)();
    const now = Date.now();
    // A goal is represented as a top-level task with goal_id = id and parent_id = NULL
    this.db
      .prepare(
        `INSERT INTO tasks (id, goal_id, parent_id, title, description, status, priority, created_at, updated_at)
       VALUES (?, ?, NULL, ?, ?, 'pending', 5, ?, ?)`,
      )
      .run(id, id, title, description ?? '', now, now);
    this.ctx.logger?.info('Goal created', { id, title });
    return id;
  }
  addSubTask(goalId, title, description, parentId, priority = 5) {
    const id = (0, crypto_1.randomUUID)();
    const now = Date.now();
    // ensure goal exists
    const goalRow = this.db
      .prepare('SELECT id FROM tasks WHERE id = ? AND goal_id = ?')
      .get(goalId, goalId);
    if (!goalRow) throw new Error('Goal not found');
    this.db
      .prepare(
        `INSERT INTO tasks (id, goal_id, parent_id, title, description, status, priority, created_at, updated_at)
       VALUES (?, ?, ?, ?, ?, 'pending', ?, ?, ?)`,
      )
      .run(id, goalId, parentId ?? null, title, description ?? '', priority, now, now);
    this.ctx.logger?.info('Subtask added', { id, goalId, parentId });
    return id;
  }
  setDependency(taskId, dependsOnId) {
    // prevent circular dependency simple check
    if (taskId === dependsOnId) throw new Error('Task cannot depend on itself');
    this.db
      .prepare('INSERT OR IGNORE INTO dependencies (task_id, depends_on_id) VALUES (?, ?)')
      .run(taskId, dependsOnId);
  }
  updateStatus(taskId, status) {
    if (!['pending', 'in-progress', 'done', 'blocked'].includes(status))
      throw new Error('Invalid status');
    const now = Date.now();
    this.db
      .prepare('UPDATE tasks SET status = ?, updated_at = ? WHERE id = ?')
      .run(status, now, taskId);
  }
  getTaskTree(goalId) {
    const root = this.db
      .prepare('SELECT * FROM tasks WHERE id = ? AND goal_id = ?')
      .get(goalId, goalId);
    if (!root) return null;
    const all = this.db
      .prepare('SELECT * FROM tasks WHERE goal_id = ? ORDER BY priority DESC, created_at ASC')
      .all(goalId);
    const map = {};
    for (const t of all) map[t.id] = { ...t, children: [] };
    for (const t of all) {
      if (t.parent_id && map[t.parent_id]) map[t.parent_id].children.push(map[t.id]);
    }
    return map[goalId] || null;
  }
  getReadyTasks() {
    // Tasks that are not done and have no unfinished dependencies
    const rows = this.db
      .prepare(
        `
      SELECT t.* FROM tasks t
      LEFT JOIN (
        SELECT d.task_id, COUNT(*) as pending_deps FROM dependencies d
        JOIN tasks dep ON dep.id = d.depends_on_id AND dep.status != 'done'
        GROUP BY d.task_id
      ) pd ON pd.task_id = t.id
      WHERE t.status != 'done' AND (pd.pending_deps IS NULL OR pd.pending_deps = 0)
      ORDER BY t.priority DESC, t.created_at ASC
      LIMIT 100
    `,
      )
      .all();
    return rows;
  }
  // ── Private helpers ──────────────────────────────────────────────────
  initDatabase() {
    const full = (0, path_1.resolve)(this.config.dbPath);
    const dir = (0, path_1.dirname)(full);
    if (!(0, fs_1.existsSync)(dir)) (0, fs_1.mkdirSync)(dir, { recursive: true });
    this.db = new better_sqlite3_1.default(full);
    this.db.pragma('journal_mode = WAL');
    this.db.pragma('synchronous = NORMAL');
    this.db.exec(`
      CREATE TABLE IF NOT EXISTS tasks (
        id TEXT PRIMARY KEY,
        goal_id TEXT NOT NULL,
        parent_id TEXT,
        title TEXT NOT NULL,
        description TEXT DEFAULT '',
        status TEXT NOT NULL DEFAULT 'pending',
        priority INTEGER DEFAULT 5,
        created_at INTEGER NOT NULL,
        updated_at INTEGER NOT NULL,
        FOREIGN KEY (parent_id) REFERENCES tasks(id) ON DELETE CASCADE
      );

      CREATE TABLE IF NOT EXISTS dependencies (
        task_id TEXT NOT NULL,
        depends_on_id TEXT NOT NULL,
        PRIMARY KEY (task_id, depends_on_id),
        FOREIGN KEY (task_id) REFERENCES tasks(id) ON DELETE CASCADE,
        FOREIGN KEY (depends_on_id) REFERENCES tasks(id) ON DELETE CASCADE
      );

      CREATE INDEX IF NOT EXISTS idx_tasks_goal ON tasks(goal_id);
      CREATE INDEX IF NOT EXISTS idx_tasks_status ON tasks(status);
      CREATE INDEX IF NOT EXISTS idx_tasks_parent ON tasks(parent_id);
    `);
  }
}
exports.default = new GoalDecompositionFeature();
