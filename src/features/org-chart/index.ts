/**
 * Org Chart Feature
 *
 * Manages organizational hierarchy where Argentum is CEO and subagents are team members.
 * Provides hire/fire/assign operations and system prompt injection.
 *
 * Argentum as CEO injects org context:
 *   ## Your Team
 *   - CTO (coder agent): Handles all code tasks
 *   - Researcher (researcher agent): Handles research
 *   - Analyst: Handles data analysis
 *
 *   You can delegate tasks to them using subagent spawning.
 */

import { existsSync, mkdirSync } from 'fs';
import { resolve, dirname, join } from 'path';

import Database from 'better-sqlite3';

import {
  type OrgNode,
  type OrgChartConfig,
  type TaskAssignment,
  type OrgStats,
  type OrgRole,
  type AgentType,
  type OrgStatus,
  type BudgetConfig,
} from './types';

import {
  type FeatureModule,
  type FeatureContext,
  type FeatureMeta,
  type HealthStatus,
} from '../../core/plugin-loader';

// ─── Feature ─────────────────────────────────────────────────────────────────

class OrgChartFeature implements FeatureModule {
  readonly meta: FeatureMeta = {
    name: 'org-chart',
    version: '0.0.2',
    description: 'Organizational chart with Argentum as CEO and subagents as team members',
    dependencies: [],
  };

  private config: OrgChartConfig = {
    enabled: false,
    dbPath: '',
    ceoId: 'ag-claw-ceo',
    defaultBudget: {
      monthlyLimit: 1_000_000,
      perAgentLimits: {},
      alertThreshold: 80,
      hardStop: true,
      spent: 0,
      periodStart: Date.now(),
    },
  };
  private ctx!: FeatureContext;
  private db!: Database.Database;

  constructor() {
    // Auto-init for CLI usage (singleton bypasses plugin loader)
    const workDir = process.env.AGCLAW_WORKDIR || process.cwd();
    this.config.dbPath = resolve(join(workDir, 'data', 'org-chart.db'));
    this.initDatabase();
    this.ensureCEO();
  }

  async init(config: Record<string, unknown>, context: FeatureContext): Promise<void> {
    this.ctx = context;
    this.config = {
      enabled: true,
      dbPath: resolve((config['dbPath'] as string) ?? './data/org-chart.db'),
      ceoId: (config['ceoId'] as string) ?? 'ag-claw-ceo',
      defaultBudget: {
        monthlyLimit: (config['monthlyLimit'] as number) ?? 1_000_000,
        perAgentLimits: (config['perAgentLimits'] as Record<string, number>) ?? {},
        alertThreshold: (config['alertThreshold'] as number) ?? 80,
        hardStop: (config['hardStop'] as boolean) ?? true,
        spent: 0,
        periodStart: Date.now(),
      },
    };
    this.initDatabase();
    this.ensureCEO();
  }

  async start(): Promise<void> {
    const stats = this.getStats();
    this.ctx.logger.info('Org chart feature started', {
      agents: stats.activeAgents,
      ceoId: this.config.ceoId,
    });
  }

  async stop(): Promise<void> {
    this.db?.close();
  }

  async healthCheck(): Promise<HealthStatus> {
    try {
      const row = this.db
        .prepare('SELECT COUNT(*) as c FROM org_nodes WHERE status = ?')
        .get('active') as { c: number };
      return {
        healthy: true,
        message: `${row.c} active agents`,
        details: { ceoId: this.config.ceoId },
      };
    } catch {
      return { healthy: false, message: 'Org chart database unavailable' };
    }
  }

  // ─── Public API ──────────────────────────────────────────────────────────

  /**
   * Get all org nodes as a flat list.
   */
  getAllNodes(): OrgNode[] {
    const rows = this.db.prepare('SELECT * FROM org_nodes ORDER BY hired_at ASC').all() as Array<{
      id: string;
      name: string;
      role: string;
      parent_id: string | null;
      agent_type: string;
      config: string;
      budget: string | null;
      status: string;
      hired_at: number;
      notes: string | null;
    }>;

    return rows.map((r) => ({
      id: r.id,
      name: r.name,
      role: r.role,
      parentId: r.parent_id ?? undefined,
      agentType: r.agent_type as AgentType,
      config: JSON.parse(r.config),
      budget: r.budget ? JSON.parse(r.budget) : undefined,
      status: r.status as OrgStatus,
      hiredAt: r.hired_at,
      notes: r.notes ?? undefined,
    }));
  }

  /**
   * Get org nodes in tree format (for display).
   */
  getTree(): OrgNode[] {
    return this.getAllNodes();
  }

  /**
   * Get active agents only.
   */
  getActiveAgents(): OrgNode[] {
    return this.getAllNodes().filter((n) => n.status === 'active');
  }

  /**
   * Get a single node by ID.
   */
  getNode(id: string): OrgNode | null {
    const row = this.db.prepare('SELECT * FROM org_nodes WHERE id = ?').get(id) as
      | {
          id: string;
          name: string;
          role: string;
          parent_id: string | null;
          agent_type: string;
          config: string;
          budget: string | null;
          status: string;
          hired_at: number;
          notes: string | null;
        }
      | undefined;

    if (!row) return null;

    return {
      id: row.id,
      name: row.name,
      role: row.role,
      parentId: row.parent_id ?? undefined,
      agentType: row.agent_type as AgentType,
      config: JSON.parse(row.config),
      budget: row.budget ? JSON.parse(row.budget) : undefined,
      status: row.status as OrgStatus,
      hiredAt: row.hired_at,
      notes: row.notes ?? undefined,
    };
  }

  /**
   * Hire a new agent/subagent.
   */
  hire(node: Omit<OrgNode, 'hiredAt' | 'status'>): OrgNode {
    const now = Date.now();
    const budget = node.budget ?? { ...this.config.defaultBudget, periodStart: now };

    const stmt = this.db.prepare(`
      INSERT INTO org_nodes (id, name, role, parent_id, agent_type, config, budget, status, hired_at, notes)
      VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
    `);
    stmt.run(
      node.id,
      node.name,
      node.role,
      node.parentId ?? null,
      node.agentType,
      JSON.stringify(node.config),
      JSON.stringify(budget),
      'active',
      now,
      node.notes ?? null,
    );

    this.ctx.logger.info('Agent hired', { id: node.id, role: node.role, name: node.name });
    return { ...node, status: 'active', hiredAt: now };
  }

  /**
   * Terminate an agent.
   */
  fire(id: string): boolean {
    const node = this.getNode(id);
    if (!node) return false;
    if (id === this.config.ceoId) {
      this.ctx.logger.warn('Cannot fire the CEO', { id });
      return false;
    }

    this.db.prepare("UPDATE org_nodes SET status = 'terminated' WHERE id = ?").run(id);
    this.ctx.logger.info('Agent terminated', { id, name: node.name });
    return true;
  }

  /**
   * Pause an agent (can be resumed).
   */
  pause(id: string): boolean {
    const node = this.getNode(id);
    if (!node || id === this.config.ceoId) return false;
    this.db.prepare("UPDATE org_nodes SET status = 'paused' WHERE id = ?").run(id);
    return true;
  }

  /**
   * Resume a paused agent.
   */
  resume(id: string): boolean {
    const node = this.getNode(id);
    if (!node || node.status === 'terminated') return false;
    this.db.prepare("UPDATE org_nodes SET status = 'active' WHERE id = ?").run(id);
    return true;
  }

  /**
   * Assign a task to an agent.
   */
  assignTask(task: Omit<TaskAssignment, 'assignedAt'>): TaskAssignment {
    const now = Date.now();
    const stmt = this.db.prepare(`
      INSERT INTO task_assignments (task_id, agent_id, assigned_at, priority, status, description)
      VALUES (?, ?, ?, ?, ?, ?)
    `);
    stmt.run(task.taskId, task.agentId, now, task.priority, task.status, task.description);
    this.ctx.logger.info('Task assigned', { taskId: task.taskId, agentId: task.agentId });
    return { ...task, assignedAt: now };
  }

  /**
   * Get tasks assigned to an agent.
   */
  getAgentTasks(agentId: string): TaskAssignment[] {
    const rows = this.db
      .prepare(
        'SELECT * FROM task_assignments WHERE agent_id = ? ORDER BY priority DESC, assigned_at ASC',
      )
      .all(agentId) as Array<{
      task_id: string;
      agent_id: string;
      assigned_at: number;
      priority: number;
      status: string;
      description: string;
    }>;

    return rows.map((r) => ({
      taskId: r.task_id,
      agentId: r.agent_id,
      assignedAt: r.assigned_at,
      priority: r.priority,
      status: r.status as TaskAssignment['status'],
      description: r.description,
    }));
  }

  /**
   * Update task status.
   */
  updateTaskStatus(taskId: string, status: TaskAssignment['status']): boolean {
    const result = this.db
      .prepare('UPDATE task_assignments SET status = ? WHERE task_id = ?')
      .run(status, taskId);
    return result.changes > 0;
  }

  /**
   * Generate system prompt section describing the team.
   */
  getTeamContext(): string {
    const active = this.getActiveAgents().filter((n) => n.id !== this.config.ceoId);

    if (active.length === 0) {
      return '## Your Team\nYou have no team members yet. You handle all tasks directly.';
    }

    const lines = ['## Your Team'];
    for (const node of active) {
      const parent = node.parentId ? this.getNode(node.parentId) : null;
      const parentLine = parent ? ` (reports to ${parent.role} ${parent.name})` : '';
      lines.push(`- ${node.role} (${node.agentType}): ${node.name}${parentLine}`);
    }
    lines.push('\nYou can delegate tasks to team members using subagent spawning.');

    return lines.join('\n');
  }

  /**
   * Get organization statistics.
   */
  getStats(): OrgStats {
    const nodes = this.getAllNodes();
    const stats: OrgStats = {
      totalAgents: nodes.length,
      activeAgents: nodes.filter((n) => n.status === 'active').length,
      pausedAgents: nodes.filter((n) => n.status === 'paused').length,
      byRole: {},
      byType: {},
      totalBudget: 0,
      totalSpent: 0,
    };

    for (const node of nodes) {
      stats.byRole[node.role] = (stats.byRole[node.role] ?? 0) + 1;
      stats.byType[node.agentType] = (stats.byType[node.agentType] ?? 0) + 1;
      if (node.budget) {
        stats.totalBudget += node.budget.monthlyLimit;
        stats.totalSpent += node.budget.spent;
      }
    }

    return stats;
  }

  /**
   * Pretty-print the org chart as an ASCII tree.
   */
  printTree(): string {
    const nodes = this.getAllNodes();
    if (nodes.length === 0) {
      return '  (no org chart data)';
    }

    // Build parent -> children map
    const children: Record<string, OrgNode[]> = {};
    let root: OrgNode | null = null;

    for (const node of nodes) {
      if (node.id === this.config.ceoId) {
        root = node;
      } else {
        const parent = node.parentId ?? this.config.ceoId;
        if (!children[parent]) children[parent] = [];
        children[parent].push(node);
      }
    }

    if (!root) {
      // No CEO node found, show flat list
      return nodes.map((n) => `  • ${n.name} (${n.role})`).join('\n');
    }

    const lines: string[] = [];
    const statusIcon = (s: OrgStatus) =>
      s === 'active'
        ? '\x1b[32m●\x1b[0m'
        : s === 'paused'
          ? '\x1b[33m◌\x1b[0m'
          : '\x1b[31m○\x1b[0m';
    const budgetBar = (b: BudgetConfig | undefined): string => {
      if (!b) return '';
      const pct = Math.min(100, Math.round((b.spent / b.monthlyLimit) * 100));
      const filled = Math.round(pct / 10);
      const bar = '█'.repeat(filled) + '░'.repeat(10 - filled);
      const color = pct >= 90 ? '\x1b[31m' : pct >= 75 ? '\x1b[33m' : '\x1b[32m';
      return ` ${color}${bar}\x1b[0m ${pct}%`;
    };

    const printNode = (node: OrgNode, prefix: string, isLast: boolean): void => {
      const connector = isLast ? '└── ' : '├── ';
      const newPrefix = prefix + (isLast ? '    ' : '│   ');

      lines.push(
        `${prefix}${connector}${statusIcon(node.status)} \x1b[1m${node.name}\x1b[0m \x1b[90m(${node.role})\x1b[0m \x1b[36m${node.agentType}\x1b[0m${budgetBar(node.budget)}`,
      );

      const nodeChildren = children[node.id] ?? [];
      for (let i = 0; i < nodeChildren.length; i++) {
        printNode(nodeChildren[i]!, newPrefix, i === nodeChildren.length - 1);
      }
    };

    lines.push(
      `${statusIcon(root.status)} \x1b[1m${root.name}\x1b[0m \x1b[90m(${root.role})\x1b[0m \x1b[36m${root.agentType}\x1b[0m${budgetBar(root.budget)}`,
    );

    const rootChildren = children[root.id] ?? [];
    for (let i = 0; i < rootChildren.length; i++) {
      printNode(rootChildren[i]!, '', i === rootChildren.length - 1);
    }

    return lines.join('\n');
  }

  /**
   * Generate default config for a new hire based on role.
   */
  getDefaultConfig(role: OrgRole): Partial<OrgNode> {
    const defaults: Record<string, Partial<OrgNode>> = {
      CTO: {
        role: 'CTO',
        agentType: 'coder',
        config: {
          description: 'Chief Technology Officer - handles all code tasks',
          specialties: ['architecture', 'code-review', 'refactoring'],
          maxConcurrentTasks: 3,
        },
        budget: {
          monthlyLimit: 200_000,
          perAgentLimits: {},
          alertThreshold: 80,
          hardStop: true,
          spent: 0,
          periodStart: Date.now(),
        },
      },
      Engineer: {
        role: 'Engineer',
        agentType: 'coder',
        config: {
          description: 'Software Engineer - implements features and fixes bugs',
          specialties: ['frontend', 'backend', 'testing'],
          maxConcurrentTasks: 2,
        },
        budget: {
          monthlyLimit: 100_000,
          perAgentLimits: {},
          alertThreshold: 80,
          hardStop: true,
          spent: 0,
          periodStart: Date.now(),
        },
      },
      Researcher: {
        role: 'Researcher',
        agentType: 'researcher',
        config: {
          description: 'Research specialist - investigates and analyzes',
          specialties: ['analysis', 'research', 'summarization'],
          maxConcurrentTasks: 2,
        },
        budget: {
          monthlyLimit: 80_000,
          perAgentLimits: {},
          alertThreshold: 80,
          hardStop: true,
          spent: 0,
          periodStart: Date.now(),
        },
      },
      Analyst: {
        role: 'Analyst',
        agentType: 'analyst',
        config: {
          description: 'Data analyst - processes and interprets data',
          specialties: ['data-analysis', 'reporting', 'visualization'],
          maxConcurrentTasks: 2,
        },
        budget: {
          monthlyLimit: 80_000,
          perAgentLimits: {},
          alertThreshold: 80,
          hardStop: true,
          spent: 0,
          periodStart: Date.now(),
        },
      },
      Foreman: {
        role: 'Foreman',
        agentType: 'foreman',
        config: {
          description: 'Project foreman - coordinates complex multi-step tasks',
          specialties: ['coordination', 'task-decomposition', 'delegation'],
          maxConcurrentTasks: 5,
        },
        budget: {
          monthlyLimit: 150_000,
          perAgentLimits: {},
          alertThreshold: 80,
          hardStop: true,
          spent: 0,
          periodStart: Date.now(),
        },
      },
    };

    return (
      defaults[role] ?? {
        role,
        agentType: 'custom',
        config: {
          description: `Custom ${role} role`,
          specialties: [],
          maxConcurrentTasks: 1,
        },
      }
    );
  }

  // ─── Private helpers ─────────────────────────────────────────────────────

  private initDatabase(): void {
    const fullPath = this.config.dbPath;
    if (!existsSync(dirname(fullPath))) {
      mkdirSync(dirname(fullPath), { recursive: true });
    }

    this.db = new Database(fullPath);
    this.db.pragma('journal_mode = WAL');

    this.db.exec(`
      CREATE TABLE IF NOT EXISTS org_nodes (
        id TEXT PRIMARY KEY,
        name TEXT NOT NULL,
        role TEXT NOT NULL,
        parent_id TEXT,
        agent_type TEXT NOT NULL,
        config TEXT NOT NULL DEFAULT '{}',
        budget TEXT,
        status TEXT NOT NULL DEFAULT 'active',
        hired_at INTEGER NOT NULL,
        notes TEXT,
        FOREIGN KEY (parent_id) REFERENCES org_nodes(id)
      );

      CREATE TABLE IF NOT EXISTS task_assignments (
        task_id TEXT PRIMARY KEY,
        agent_id TEXT NOT NULL,
        assigned_at INTEGER NOT NULL,
        priority INTEGER NOT NULL DEFAULT 0,
        status TEXT NOT NULL DEFAULT 'pending',
        description TEXT NOT NULL DEFAULT '',
        FOREIGN KEY (agent_id) REFERENCES org_nodes(id)
      );

      CREATE INDEX IF NOT EXISTS idx_org_nodes_status ON org_nodes(status);
      CREATE INDEX IF NOT EXISTS idx_org_nodes_parent ON org_nodes(parent_id);
      CREATE INDEX IF NOT EXISTS idx_tasks_agent ON task_assignments(agent_id);
      CREATE INDEX IF NOT EXISTS idx_tasks_status ON task_assignments(status);
    `);
  }

  private ensureCEO(): void {
    const existing = this.getNode(this.config.ceoId);
    if (!existing) {
      this.ctx.logger.info('Creating CEO node for Argentum');
      this.db
        .prepare(
          `
        INSERT OR IGNORE INTO org_nodes (id, name, role, agent_type, config, status, hired_at)
        VALUES (?, ?, ?, ?, ?, ?, ?)
      `,
        )
        .run(
          this.config.ceoId,
          'Argentum',
          'CEO',
          'ag-claw',
          JSON.stringify({ description: 'Argentum CEO - top level orchestrator' }),
          'active',
          Date.now(),
        );
    }
  }
}

export default new OrgChartFeature();
