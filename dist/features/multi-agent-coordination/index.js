"use strict";
var __importDefault = (this && this.__importDefault) || function (mod) {
    return (mod && mod.__esModule) ? mod : { "default": mod };
};
Object.defineProperty(exports, "__esModule", { value: true });
const fs_1 = require("fs");
const path_1 = require("path");
const better_sqlite3_1 = __importDefault(require("better-sqlite3"));
// ─── Feature ─────────────────────────────────────────────────────────────────
class MultiAgentCoordinationFeature {
    meta = {
        name: 'multi-agent-coordination',
        version: '0.0.1',
        description: 'Coordination between multiple agents with task assignment and broadcasting',
        dependencies: [],
    };
    config = {
        enabled: false,
        dbPath: './data/multi-agent-coordination.db',
        heartbeatIntervalMs: 30_000,
        offlineTimeoutMs: 60_000,
        maxAgents: 100,
    };
    db;
    ctx;
    agents = new Map();
    heartbeatTimer = null;
    async init(config, context) {
        this.ctx = context;
        this.config = { ...this.config, ...config };
        this.initDatabase();
        this.loadAgentsFromDb();
    }
    async start() {
        // Start heartbeat checker to mark offline agents
        this.heartbeatTimer = setInterval(() => {
            this.checkOfflineAgents();
        }, this.config.heartbeatIntervalMs);
        this.ctx.logger.info('Multi-agent coordination active', {
            registeredAgents: this.agents.size,
            offlineTimeout: `${this.config.offlineTimeoutMs / 1000}s`,
        });
    }
    async stop() {
        if (this.heartbeatTimer) {
            clearInterval(this.heartbeatTimer);
            this.heartbeatTimer = null;
        }
        // Update all agents to offline before shutdown
        this.updateAgentStatusesToOffline();
        this.db?.close();
    }
    async healthCheck() {
        const totalAgents = this.agents.size;
        const idleAgents = Array.from(this.agents.values()).filter((a) => a.status === 'idle').length;
        const busyAgents = Array.from(this.agents.values()).filter((a) => a.status === 'busy').length;
        const offlineAgents = Array.from(this.agents.values()).filter((a) => a.status === 'offline').length;
        const pendingTasks = this.db
            .prepare("SELECT COUNT(*) as c FROM tasks WHERE status = 'pending'")
            .get();
        const pendingCount = pendingTasks.c;
        return {
            healthy: true,
            details: {
                totalAgents,
                idleAgents,
                busyAgents,
                offlineAgents,
                pendingTasks: pendingCount,
            },
        };
    }
    // ─── Agent Management ─────────────────────────────────────────────────────
    /** Register a new agent or update existing one */
    async registerAgent(id, name, capabilities) {
        const now = Date.now();
        if (this.agents.size >= this.config.maxAgents) {
            throw new Error(`Max agent limit reached: ${this.config.maxAgents}`);
        }
        const existing = this.agents.get(id);
        if (existing) {
            // Update existing agent
            existing.name = name;
            existing.capabilities = capabilities;
            existing.lastSeen = now;
            existing.metadata = existing.metadata; // preserve
            this.upsertAgentToDb(existing);
            this.ctx.logger.debug('Agent updated', { agentId: id, name, capabilities });
        }
        else {
            // Create new agent
            const agent = {
                id,
                name,
                capabilities,
                status: 'idle',
                currentTaskId: null,
                lastSeen: now,
                metadata: {},
            };
            this.agents.set(id, agent);
            this.insertAgentToDb(agent);
            this.ctx.logger.info('Agent registered', { agentId: id, name, capabilities });
        }
    }
    /** Unregister an agent */
    async unregisterAgent(id) {
        const agent = this.agents.get(id);
        if (!agent)
            return false;
        this.agents.delete(id);
        this.db.prepare('DELETE FROM agents WHERE id = ?').run(id);
        this.ctx.logger.info('Agent unregistered', { agentId: id });
        return true;
    }
    /** Get agent info */
    async getAgent(id) {
        return this.agents.get(id) ?? null;
    }
    /** Get all agents */
    getAvailableAgents() {
        return Array.from(this.agents.values());
    }
    /** Get agents by capability */
    getAgentsByCapability(capability) {
        return Array.from(this.agents.values()).filter((a) => a.capabilities.includes(capability));
    }
    /** Update agent's last seen timestamp */
    async heartbeat(agentId) {
        const agent = this.agents.get(agentId);
        if (!agent)
            return false;
        agent.lastSeen = Date.now();
        if (agent.status === 'offline') {
            agent.status = 'idle';
            this.updateAgentInDb(agent);
            this.ctx.logger.info('Agent came back online', { agentId });
        }
        return true;
    }
    /** Check agents that haven't sent heartbeat and mark them offline */
    checkOfflineAgents() {
        const now = Date.now();
        const timeout = this.config.offlineTimeoutMs;
        for (const agent of this.agents.values()) {
            if (agent.status !== 'offline' && now - agent.lastSeen > timeout) {
                agent.status = 'offline';
                this.updateAgentInDb(agent);
                this.ctx.logger.warn('Agent marked offline (no heartbeat)', { agentId: agent.id });
            }
        }
    }
    updateAgentStatusesToOffline() {
        for (const agent of this.agents.values()) {
            if (agent.status !== 'offline') {
                agent.status = 'offline';
                this.updateAgentInDb(agent);
            }
        }
    }
    // ─── Task Management ──────────────────────────────────────────────────────
    /** Assign a task to an agent */
    async assignTask(taskId, agentId, payload = {}) {
        const agent = this.agents.get(agentId);
        if (!agent) {
            throw new Error(`Agent not found: ${agentId}`);
        }
        if (agent.status !== 'idle') {
            this.ctx.logger.warn('Agent not idle, cannot assign task', { agentId, status: agent.status });
            return false;
        }
        // Create task record
        const now = Date.now();
        const task = {
            taskId,
            assignedAgentId: agentId,
            status: 'assigned',
            createdAt: now,
            assignedAt: now,
            completedAt: null,
            payload,
            result: null,
        };
        // Insert task
        this.insertTaskToDb(task);
        // Update agent
        agent.status = 'busy';
        agent.currentTaskId = taskId;
        this.updateAgentInDb(agent);
        this.ctx.logger.info('Task assigned', { taskId, agentId });
        return true;
    }
    /** Complete a task */
    async completeTask(taskId, result = {}) {
        const task = this.db.prepare('SELECT * FROM tasks WHERE task_id = ?').get(taskId);
        if (!task)
            return false;
        const now = Date.now();
        this.db
            .prepare("UPDATE tasks SET status = 'completed', completed_at = ?, result = ? WHERE task_id = ?")
            .run(now, JSON.stringify(result), taskId);
        // Free up the agent
        const agent = this.agents.get(task.assigned_agent_id);
        if (agent) {
            agent.status = 'idle';
            agent.currentTaskId = null;
            this.updateAgentInDb(agent);
        }
        this.ctx.logger.info('Task completed', { taskId, agentId: task.assigned_agent_id });
        return true;
    }
    /** Fail a task */
    async failTask(taskId) {
        const task = this.db.prepare('SELECT * FROM tasks WHERE task_id = ?').get(taskId);
        if (!task)
            return false;
        const now = Date.now();
        this.db
            .prepare("UPDATE tasks SET status = 'failed', completed_at = ? WHERE task_id = ?")
            .run(now, taskId);
        // Free up the agent
        const agent = this.agents.get(task.assigned_agent_id);
        if (agent) {
            agent.status = 'idle';
            agent.currentTaskId = null;
            this.updateAgentInDb(agent);
        }
        this.ctx.logger.info('Task failed', { taskId, agentId: task.assigned_agent_id });
        return true;
    }
    /** Get task by ID */
    async getTask(taskId) {
        const row = this.db.prepare('SELECT * FROM tasks WHERE task_id = ?').get(taskId);
        if (!row)
            return null;
        return this.mapTaskRow(row);
    }
    /** List tasks by status */
    async listTasks(status) {
        let stmt = this.db.prepare('SELECT * FROM tasks');
        if (status) {
            stmt = this.db.prepare('SELECT * FROM tasks WHERE status = ?');
        }
        const rows = status ? stmt.all(status) : stmt.all();
        return rows.map(this.mapTaskRow);
    }
    mapTaskRow(row) {
        return {
            taskId: row.task_id,
            assignedAgentId: row.assigned_agent_id,
            status: row.status,
            createdAt: row.created_at,
            assignedAt: row.assigned_at,
            completedAt: row.completed_at,
            payload: JSON.parse(row.payload || '{}'),
            result: row.result ? JSON.parse(row.result) : null,
        };
    }
    // ─── Broadcasting ─────────────────────────────────────────────────────────
    /** Broadcast a message to all agents (or filtered by capability) */
    async broadcast(message, capability, excludeAgentId) {
        const targets = capability ? this.getAgentsByCapability(capability) : this.getAvailableAgents();
        const filtered = targets.filter((a) => a.id !== excludeAgentId && a.status !== 'offline');
        const sent = filtered.length;
        this.ctx.logger.info('Broadcasting message', {
            message: message.substring(0, 100),
            targetCount: sent,
            capability: capability || 'all',
            excludeAgent: excludeAgentId,
        });
        // Emit hook for each target agent (can be picked up by communication layer)
        for (const agent of filtered) {
            await this.ctx.emit('agent.broadcast', {
                agentId: agent.id,
                message,
                from: 'multi-agent-coordination',
            });
        }
        return sent;
    }
    // ─── Database ─────────────────────────────────────────────────────────────
    initDatabase() {
        const fullPath = (0, path_1.resolve)(this.config.dbPath);
        if (!(0, fs_1.existsSync)((0, path_1.dirname)(fullPath))) {
            (0, fs_1.mkdirSync)((0, path_1.dirname)(fullPath), { recursive: true });
        }
        this.db = new better_sqlite3_1.default(fullPath);
        this.db.pragma('journal_mode = WAL');
        this.db.pragma('synchronous = NORMAL');
        this.db.exec(`
      CREATE TABLE IF NOT EXISTS agents (
        id TEXT PRIMARY KEY,
        name TEXT NOT NULL,
        capabilities TEXT NOT NULL, -- JSON array
        status TEXT NOT NULL,
        current_task_id TEXT,
        last_seen INTEGER NOT NULL,
        metadata TEXT -- JSON object
      );

      CREATE TABLE IF NOT EXISTS tasks (
        task_id TEXT PRIMARY KEY,
        assigned_agent_id TEXT,
        status TEXT NOT NULL,
        created_at INTEGER NOT NULL,
        assigned_at INTEGER,
        completed_at INTEGER,
        payload TEXT DEFAULT '{}',
        result TEXT,
        FOREIGN KEY (assigned_agent_id) REFERENCES agents(id)
      );

      CREATE INDEX IF NOT EXISTS idx_tasks_agent ON tasks(assigned_agent_id);
      CREATE INDEX IF NOT EXISTS idx_tasks_status ON tasks(status);
      CREATE INDEX IF NOT EXISTS idx_tasks_created ON tasks(created_at);
    `);
    }
    insertAgentToDb(agent) {
        this.db
            .prepare(`INSERT OR REPLACE INTO agents (id, name, capabilities, status, current_task_id, last_seen, metadata)
       VALUES (?, ?, ?, ?, ?, ?, ?)`)
            .run(agent.id, agent.name, JSON.stringify(agent.capabilities), agent.status, agent.currentTaskId, agent.lastSeen, JSON.stringify(agent.metadata));
    }
    upsertAgentToDb(agent) {
        this.insertAgentToDb(agent);
    }
    updateAgentInDb(agent) {
        this.db
            .prepare(`UPDATE agents SET status = ?, current_task_id = ?, last_seen = ? WHERE id = ?`)
            .run(agent.status, agent.currentTaskId, agent.lastSeen, agent.id);
    }
    loadAgentsFromDb() {
        const rows = this.db.prepare('SELECT * FROM agents').all();
        for (const row of rows) {
            const agent = {
                id: row.id,
                name: row.name,
                capabilities: JSON.parse(row.capabilities),
                status: row.status,
                currentTaskId: row.current_task_id,
                lastSeen: row.last_seen,
                metadata: row.metadata ? JSON.parse(row.metadata) : {},
            };
            this.agents.set(agent.id, agent);
        }
    }
    insertTaskToDb(task) {
        this.db
            .prepare(`INSERT INTO tasks (task_id, assigned_agent_id, status, created_at, assigned_at, payload)
       VALUES (?, ?, ?, ?, ?, ?)`)
            .run(task.taskId, task.assignedAgentId, task.status, task.createdAt, task.assignedAt, JSON.stringify(task.payload));
    }
}
exports.default = new MultiAgentCoordinationFeature();
//# sourceMappingURL=index.js.map