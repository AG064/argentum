"use strict";
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
var __importDefault = (this && this.__importDefault) || function (mod) {
    return (mod && mod.__esModule) ? mod : { "default": mod };
};
Object.defineProperty(exports, "__esModule", { value: true });
const fs_1 = require("fs");
const path_1 = require("path");
const better_sqlite3_1 = __importDefault(require("better-sqlite3"));
// ─── Feature ─────────────────────────────────────────────────────────────────
class OrgChartFeature {
    meta = {
        name: 'org-chart',
        version: '0.0.3',
        description: 'Organizational chart with Argentum as CEO and subagents as team members',
        dependencies: [],
    };
    config = {
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
    ctx;
    db;
    constructor() {
        // Auto-init for CLI usage (singleton bypasses plugin loader)
        const workDir = process.env.AGCLAW_WORKDIR || process.cwd();
        this.config.dbPath = (0, path_1.resolve)((0, path_1.join)(workDir, 'data', 'org-chart.db'));
        this.initDatabase();
        this.ensureCEO();
    }
    async init(config, context) {
        this.ctx = context;
        this.config = {
            enabled: true,
            dbPath: (0, path_1.resolve)(config['dbPath'] ?? './data/org-chart.db'),
            ceoId: config['ceoId'] ?? 'ag-claw-ceo',
            defaultBudget: {
                monthlyLimit: config['monthlyLimit'] ?? 1_000_000,
                perAgentLimits: config['perAgentLimits'] ?? {},
                alertThreshold: config['alertThreshold'] ?? 80,
                hardStop: config['hardStop'] ?? true,
                spent: 0,
                periodStart: Date.now(),
            },
        };
        this.initDatabase();
        this.ensureCEO();
    }
    async start() {
        const stats = this.getStats();
        this.ctx.logger.info('Org chart feature started', {
            agents: stats.activeAgents,
            ceoId: this.config.ceoId,
        });
    }
    async stop() {
        this.db?.close();
    }
    async healthCheck() {
        try {
            const row = this.db
                .prepare('SELECT COUNT(*) as c FROM org_nodes WHERE status = ?')
                .get('active');
            return {
                healthy: true,
                message: `${row.c} active agents`,
                details: { ceoId: this.config.ceoId },
            };
        }
        catch {
            return { healthy: false, message: 'Org chart database unavailable' };
        }
    }
    // ─── Public API ──────────────────────────────────────────────────────────
    /**
     * Get all org nodes as a flat list.
     */
    getAllNodes() {
        const rows = this.db.prepare('SELECT * FROM org_nodes ORDER BY hired_at ASC').all();
        return rows.map((r) => ({
            id: r.id,
            name: r.name,
            role: r.role,
            parentId: r.parent_id ?? undefined,
            agentType: r.agent_type,
            config: JSON.parse(r.config),
            budget: r.budget ? JSON.parse(r.budget) : undefined,
            status: r.status,
            hiredAt: r.hired_at,
            notes: r.notes ?? undefined,
        }));
    }
    /**
     * Get org nodes in tree format (for display).
     */
    getTree() {
        return this.getAllNodes();
    }
    /**
     * Get active agents only.
     */
    getActiveAgents() {
        return this.getAllNodes().filter((n) => n.status === 'active');
    }
    /**
     * Get a single node by ID.
     */
    getNode(id) {
        const row = this.db.prepare('SELECT * FROM org_nodes WHERE id = ?').get(id);
        if (!row)
            return null;
        return {
            id: row.id,
            name: row.name,
            role: row.role,
            parentId: row.parent_id ?? undefined,
            agentType: row.agent_type,
            config: JSON.parse(row.config),
            budget: row.budget ? JSON.parse(row.budget) : undefined,
            status: row.status,
            hiredAt: row.hired_at,
            notes: row.notes ?? undefined,
        };
    }
    /**
     * Hire a new agent/subagent.
     */
    hire(node) {
        const now = Date.now();
        const budget = node.budget ?? { ...this.config.defaultBudget, periodStart: now };
        const stmt = this.db.prepare(`
      INSERT INTO org_nodes (id, name, role, parent_id, agent_type, config, budget, status, hired_at, notes)
      VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
    `);
        stmt.run(node.id, node.name, node.role, node.parentId ?? null, node.agentType, JSON.stringify(node.config), JSON.stringify(budget), 'active', now, node.notes ?? null);
        this.ctx.logger.info('Agent hired', { id: node.id, role: node.role, name: node.name });
        return { ...node, status: 'active', hiredAt: now };
    }
    /**
     * Terminate an agent.
     */
    fire(id) {
        const node = this.getNode(id);
        if (!node)
            return false;
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
    pause(id) {
        const node = this.getNode(id);
        if (!node || id === this.config.ceoId)
            return false;
        this.db.prepare("UPDATE org_nodes SET status = 'paused' WHERE id = ?").run(id);
        return true;
    }
    /**
     * Resume a paused agent.
     */
    resume(id) {
        const node = this.getNode(id);
        if (!node || node.status === 'terminated')
            return false;
        this.db.prepare("UPDATE org_nodes SET status = 'active' WHERE id = ?").run(id);
        return true;
    }
    /**
     * Assign a task to an agent.
     */
    assignTask(task) {
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
    getAgentTasks(agentId) {
        const rows = this.db
            .prepare('SELECT * FROM task_assignments WHERE agent_id = ? ORDER BY priority DESC, assigned_at ASC')
            .all(agentId);
        return rows.map((r) => ({
            taskId: r.task_id,
            agentId: r.agent_id,
            assignedAt: r.assigned_at,
            priority: r.priority,
            status: r.status,
            description: r.description,
        }));
    }
    /**
     * Update task status.
     */
    updateTaskStatus(taskId, status) {
        const result = this.db
            .prepare('UPDATE task_assignments SET status = ? WHERE task_id = ?')
            .run(status, taskId);
        return result.changes > 0;
    }
    /**
     * Generate system prompt section describing the team.
     */
    getTeamContext() {
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
    getStats() {
        const nodes = this.getAllNodes();
        const stats = {
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
    printTree() {
        const nodes = this.getAllNodes();
        if (nodes.length === 0) {
            return '  (no org chart data)';
        }
        // Build parent -> children map
        const children = {};
        let root = null;
        for (const node of nodes) {
            if (node.id === this.config.ceoId) {
                root = node;
            }
            else {
                const parent = node.parentId ?? this.config.ceoId;
                if (!children[parent])
                    children[parent] = [];
                children[parent].push(node);
            }
        }
        if (!root) {
            // No CEO node found, show flat list
            return nodes.map((n) => `  • ${n.name} (${n.role})`).join('\n');
        }
        const lines = [];
        const statusIcon = (s) => s === 'active'
            ? '\x1b[32m●\x1b[0m'
            : s === 'paused'
                ? '\x1b[33m◌\x1b[0m'
                : '\x1b[31m○\x1b[0m';
        const budgetBar = (b) => {
            if (!b)
                return '';
            const pct = Math.min(100, Math.round((b.spent / b.monthlyLimit) * 100));
            const filled = Math.round(pct / 10);
            const bar = '█'.repeat(filled) + '░'.repeat(10 - filled);
            const color = pct >= 90 ? '\x1b[31m' : pct >= 75 ? '\x1b[33m' : '\x1b[32m';
            return ` ${color}${bar}\x1b[0m ${pct}%`;
        };
        const printNode = (node, prefix, isLast) => {
            const connector = isLast ? '└── ' : '├── ';
            const newPrefix = prefix + (isLast ? '    ' : '│   ');
            lines.push(`${prefix}${connector}${statusIcon(node.status)} \x1b[1m${node.name}\x1b[0m \x1b[90m(${node.role})\x1b[0m \x1b[36m${node.agentType}\x1b[0m${budgetBar(node.budget)}`);
            const nodeChildren = children[node.id] ?? [];
            for (let i = 0; i < nodeChildren.length; i++) {
                printNode(nodeChildren[i], newPrefix, i === nodeChildren.length - 1);
            }
        };
        lines.push(`${statusIcon(root.status)} \x1b[1m${root.name}\x1b[0m \x1b[90m(${root.role})\x1b[0m \x1b[36m${root.agentType}\x1b[0m${budgetBar(root.budget)}`);
        const rootChildren = children[root.id] ?? [];
        for (let i = 0; i < rootChildren.length; i++) {
            printNode(rootChildren[i], '', i === rootChildren.length - 1);
        }
        return lines.join('\n');
    }
    /**
     * Generate default config for a new hire based on role.
     */
    getDefaultConfig(role) {
        const defaults = {
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
        return (defaults[role] ?? {
            role,
            agentType: 'custom',
            config: {
                description: `Custom ${role} role`,
                specialties: [],
                maxConcurrentTasks: 1,
            },
        });
    }
    // ─── Private helpers ─────────────────────────────────────────────────────
    initDatabase() {
        const fullPath = this.config.dbPath;
        if (!(0, fs_1.existsSync)((0, path_1.dirname)(fullPath))) {
            (0, fs_1.mkdirSync)((0, path_1.dirname)(fullPath), { recursive: true });
        }
        this.db = new better_sqlite3_1.default(fullPath);
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
    ensureCEO() {
        const existing = this.getNode(this.config.ceoId);
        if (!existing) {
            this.ctx.logger.info('Creating CEO node for Argentum');
            this.db
                .prepare(`
        INSERT OR IGNORE INTO org_nodes (id, name, role, agent_type, config, status, hired_at)
        VALUES (?, ?, ?, ?, ?, ?, ?)
      `)
                .run(this.config.ceoId, 'Argentum', 'CEO', 'ag-claw', JSON.stringify({ description: 'Argentum CEO - top level orchestrator' }), 'active', Date.now());
        }
    }
}
exports.default = new OrgChartFeature();
//# sourceMappingURL=index.js.map