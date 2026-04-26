/**
 * Goal Decomposition Feature (SQLite)
 *
 * Stores goals and tasks with parent-child relationships and dependencies.
 */

import { randomUUID } from 'crypto';
import { mkdirSync, existsSync } from 'fs';
import { dirname, resolve } from 'path';

import Database from 'better-sqlite3';

import type {
  FeatureModule,
  FeatureMeta,
  FeatureContext,
  HealthStatus,
} from '../../core/plugin-loader';

export type TaskStatus = 'pending' | 'in-progress' | 'done' | 'blocked';

export interface TaskRow {
  id: string;
  goal_id: string;
  parent_id: string | null;
  title: string;
  description: string;
  status: TaskStatus;
  priority: number;
  created_at: number;
  updated_at: number;
}

export interface GoalDecompositionConfig {
  enabled: boolean;
  dbPath: string;
}

const DEFAULT_CONFIG: GoalDecompositionConfig = {
  enabled: false,
  dbPath: './data/goal-decomposition.db',
};

class GoalDecompositionFeature implements FeatureModule {
  readonly meta: FeatureMeta = {
    name: 'goal-decomposition',
    version: '0.0.2',
    description: 'Decompose goals into tasks and track dependencies (SQLite)',
    dependencies: [],
  };

  private config: GoalDecompositionConfig = { ...DEFAULT_CONFIG };
  private ctx!: FeatureContext;
  private db!: Database.Database;

  async init(config: Record<string, unknown>, context: FeatureContext): Promise<void> {
    this.ctx = context;
    this.config = { ...this.config, ...(config as Partial<GoalDecompositionConfig>) };
    this.initDatabase();
  }

  async start(): Promise<void> {
    /* nothing */
  }
  async stop(): Promise<void> {
    this.db?.close();
  }

  async healthCheck(): Promise<HealthStatus> {
    const goals = (
      this.db.prepare('SELECT COUNT(DISTINCT goal_id) as c FROM tasks').get() as { c: number }
    ).c;
    const tasks = (this.db.prepare('SELECT COUNT(*) as c FROM tasks').get() as { c: number }).c;
    return { healthy: true, details: { goals, tasks } };
  }

  // ── API required by task ──────────────────────────────────────────────

  createGoal(title: string, description?: string): string {
    const id = randomUUID();
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

  addSubTask(
    goalId: string,
    title: string,
    description?: string,
    parentId?: string,
    priority = 5,
  ): string {
    const id = randomUUID();
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

  setDependency(taskId: string, dependsOnId: string): void {
    // prevent circular dependency simple check
    if (taskId === dependsOnId) throw new Error('Task cannot depend on itself');
    this.db
      .prepare('INSERT OR IGNORE INTO dependencies (task_id, depends_on_id) VALUES (?, ?)')
      .run(taskId, dependsOnId);
  }

  updateStatus(taskId: string, status: TaskStatus): void {
    if (!['pending', 'in-progress', 'done', 'blocked'].includes(status))
      throw new Error('Invalid status');
    const now = Date.now();
    this.db
      .prepare('UPDATE tasks SET status = ?, updated_at = ? WHERE id = ?')
      .run(status, now, taskId);
  }

  getTaskTree(goalId: string): TaskRow | null {
    const root = this.db
      .prepare('SELECT * FROM tasks WHERE id = ? AND goal_id = ?')
      .get(goalId, goalId) as TaskRow | undefined;
    if (!root) return null;

    const all = this.db
      .prepare('SELECT * FROM tasks WHERE goal_id = ? ORDER BY priority DESC, created_at ASC')
      .all(goalId) as TaskRow[];
    const map: Record<string, TaskRow & { children?: TaskRow[] }> = {};
    for (const t of all) map[t.id] = { ...t, children: [] } as any;
    for (const t of all) {
      if (t.parent_id && map[t.parent_id] && map[t.id])
        map[t.parent_id]!.children!.push(map[t.id] as TaskRow & { children?: TaskRow[] });
    }

    return map[goalId] || null;
  }

  getReadyTasks(): TaskRow[] {
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
      .all() as TaskRow[];
    return rows;
  }

  // ── Private helpers ──────────────────────────────────────────────────

  private initDatabase(): void {
    const full = resolve(this.config.dbPath);
    const dir = dirname(full);
    if (!existsSync(dir)) mkdirSync(dir, { recursive: true });

    this.db = new Database(full);
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

export default new GoalDecompositionFeature();
