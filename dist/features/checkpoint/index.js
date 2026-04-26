"use strict";
/**
 * Checkpoint Feature
 *
 * OMEGA Memory integration — save and restore task state between sessions.
 * Integrates with mesh-workflows for persistent workflow state.
 */
Object.defineProperty(exports, "__esModule", { value: true });
const semantic_1 = require("../../memory/semantic");
/**
 * Checkpoint — persistent task state management.
 *
 * Save and resume task state across sessions. Integrates with
 * mesh-workflows to persist workflow execution state.
 */
class CheckpointFeature {
    meta = {
        name: 'checkpoint',
        version: '0.0.1',
        description: 'Task checkpoint and resume across sessions',
        dependencies: [],
    };
    config = {
        enabled: false,
        maxCheckpoints: 100,
        autoCheckpoint: true,
        checkpointIntervalMs: 300000, // 5 minutes
    };
    ctx;
    activeTasks = new Map();
    timer = null;
    checkpointCount = 0;
    resumeCount = 0;
    async init(config, context) {
        this.ctx = context;
        this.config = { ...this.config, ...config };
        // Register hooks for auto-checkpointing
        context.registerHook('task:start', this.handleTaskStart.bind(this));
        context.registerHook('task:update', this.handleTaskUpdate.bind(this));
        context.registerHook('task:complete', this.handleTaskComplete.bind(this));
    }
    async start() {
        if (this.config.autoCheckpoint && this.config.checkpointIntervalMs > 0) {
            this.timer = setInterval(() => {
                this.autoCheckpointAll().catch((err) => {
                    this.ctx.logger.error('Auto-checkpoint failed', {
                        error: err instanceof Error ? err.message : String(err),
                    });
                });
            }, this.config.checkpointIntervalMs);
        }
        // Load active checkpoints on start
        await this.loadActiveCheckpoints();
        this.ctx.logger.info('Checkpoint active', {
            autoCheckpoint: this.config.autoCheckpoint,
            interval: `${this.config.checkpointIntervalMs / 1000}s`,
        });
    }
    async stop() {
        // Save all active checkpoints before stopping
        if (this.config.autoCheckpoint) {
            await this.autoCheckpointAll();
        }
        if (this.timer) {
            clearInterval(this.timer);
            this.timer = null;
        }
    }
    async healthCheck() {
        const memory = (0, semantic_1.getSemanticMemory)();
        const db = memory.getDb();
        const count = db.prepare('SELECT COUNT(*) as c FROM checkpoints').get().c;
        return {
            healthy: count < this.config.maxCheckpoints,
            details: {
                totalCheckpoints: count,
                activeTasks: this.activeTasks.size,
                checkpointCount: this.checkpointCount,
                resumeCount: this.resumeCount,
            },
        };
    }
    /** Save a checkpoint */
    async checkpoint(taskId, state, context = {}) {
        const memory = (0, semantic_1.getSemanticMemory)();
        await memory.checkpoint(taskId, {
            state,
            context,
            savedAt: new Date().toISOString(),
        });
        this.activeTasks.set(taskId, {
            lastCheckpoint: Date.now(),
            state,
        });
        this.checkpointCount++;
        this.ctx.logger.debug('Checkpoint saved', {
            taskId,
            stateSize: JSON.stringify(state).length,
        });
        // Also store as semantic memory for searchability
        await memory.store('checkpoint', `Checkpoint for task ${taskId}: ${JSON.stringify(state).slice(0, 200)}`, {
            taskId,
            checkpointTime: new Date().toISOString(),
        });
    }
    /** Resume a checkpointed task */
    async resume(taskId) {
        const memory = (0, semantic_1.getSemanticMemory)();
        const data = await memory.resume(taskId);
        if (!data) {
            this.ctx.logger.debug('No checkpoint found', { taskId });
            return null;
        }
        const parsed = data;
        this.activeTasks.set(taskId, {
            lastCheckpoint: Date.now(),
            state: parsed.state,
        });
        this.resumeCount++;
        this.ctx.logger.info('Checkpoint resumed', { taskId });
        return parsed;
    }
    /** List all checkpoints */
    async listCheckpoints() {
        const memory = (0, semantic_1.getSemanticMemory)();
        const db = memory.getDb();
        const rows = db.prepare('SELECT * FROM checkpoints ORDER BY updated_at DESC').all();
        return rows.map((row) => {
            const stateStr = row.state;
            return {
                taskId: row.task_id,
                contextPreview: this.getContextPreview(row.context),
                createdAt: new Date(row.created_at).toISOString(),
                updatedAt: new Date(row.updated_at).toISOString(),
                stateSize: stateStr.length,
            };
        });
    }
    /** Delete a checkpoint */
    async deleteCheckpoint(taskId) {
        const memory = (0, semantic_1.getSemanticMemory)();
        const db = memory.getDb();
        const result = db.prepare('DELETE FROM checkpoints WHERE task_id = ?').run(taskId);
        this.activeTasks.delete(taskId);
        return result.changes > 0;
    }
    /** Get a specific checkpoint */
    async getCheckpoint(taskId) {
        const memory = (0, semantic_1.getSemanticMemory)();
        const db = memory.getDb();
        const row = db.prepare('SELECT * FROM checkpoints WHERE task_id = ?').get(taskId);
        if (!row)
            return null;
        return {
            taskId: row.task_id,
            state: JSON.parse(row.state),
            context: this.parseJson(row.context),
            createdAt: row.created_at,
            updatedAt: row.updated_at,
        };
    }
    /** Handle task start hook */
    async handleTaskStart(data) {
        const taskData = data;
        if (!taskData.taskId)
            return;
        this.activeTasks.set(taskData.taskId, {
            lastCheckpoint: Date.now(),
            state: taskData.state ?? {},
        });
        this.ctx.logger.debug('Task started, tracking', { taskId: taskData.taskId });
    }
    /** Handle task update hook */
    async handleTaskUpdate(data) {
        const taskData = data;
        if (!taskData.taskId)
            return;
        const existing = this.activeTasks.get(taskData.taskId);
        if (existing) {
            existing.state = taskData.state ?? existing.state;
        }
    }
    /** Handle task complete hook */
    async handleTaskComplete(data) {
        const taskData = data;
        if (!taskData.taskId)
            return;
        // Save final checkpoint
        const task = this.activeTasks.get(taskData.taskId);
        if (task) {
            await this.checkpoint(taskData.taskId, {
                ...task.state,
                completed: true,
                completedAt: new Date().toISOString(),
            });
        }
        this.activeTasks.delete(taskData.taskId);
    }
    /** Auto-checkpoint all active tasks */
    async autoCheckpointAll() {
        const now = Date.now();
        for (const [taskId, task] of this.activeTasks) {
            if (now - task.lastCheckpoint >= this.config.checkpointIntervalMs) {
                try {
                    await this.checkpoint(taskId, task.state);
                }
                catch (err) {
                    this.ctx.logger.error('Auto-checkpoint failed', {
                        taskId,
                        error: err instanceof Error ? err.message : String(err),
                    });
                }
            }
        }
    }
    /** Load active checkpoints on startup */
    async loadActiveCheckpoints() {
        const checkpoints = await this.listCheckpoints();
        this.ctx.logger.info('Loaded checkpoints', { count: checkpoints.length });
    }
    /** Extract preview from context JSON */
    getContextPreview(contextStr) {
        try {
            const ctx = JSON.parse(contextStr);
            const keys = Object.keys(ctx).slice(0, 3);
            return keys.map((k) => `${k}: ${JSON.stringify(ctx[k]).slice(0, 30)}`).join(', ');
        }
        catch {
            return contextStr.slice(0, 100);
        }
    }
    /** Safe JSON parse */
    parseJson(str) {
        try {
            return JSON.parse(str);
        }
        catch {
            return {};
        }
    }
}
exports.default = new CheckpointFeature();
//# sourceMappingURL=index.js.map