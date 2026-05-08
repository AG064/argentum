"use strict";
var __importDefault = (this && this.__importDefault) || function (mod) {
    return (mod && mod.__esModule) ? mod : { "default": mod };
};
Object.defineProperty(exports, "__esModule", { value: true });
const fs_1 = require("fs");
const path_1 = require("path");
const better_sqlite3_1 = __importDefault(require("better-sqlite3"));
const node_cron_1 = __importDefault(require("node-cron"));
// ─── Feature ─────────────────────────────────────────────────────────────────
class CronSchedulerFeature {
    meta = {
        name: 'cron-scheduler',
        version: '0.0.5',
        description: 'Cron-like job scheduler with persistent storage and custom handlers',
        dependencies: [],
    };
    config = {
        enabled: false,
        dbPath: './data/cron-scheduler.db',
        maxJobs: 500,
        maxConcurrentRuns: 5,
    };
    ctx;
    db;
    jobs = new Map();
    handlers = new Map();
    cronJobs = new Map();
    schedulerStartTime = 0;
    async init(config, context) {
        this.ctx = context;
        this.config = { ...this.config, ...config };
        this.initDatabase();
        this.loadJobsFromDb();
    }
    async start() {
        this.schedulerStartTime = Date.now();
        // Schedule all enabled jobs
        for (const job of this.jobs.values()) {
            if (job.enabled) {
                this.scheduleJob(job);
            }
        }
        this.ctx.logger.info('Cron scheduler active', {
            scheduledJobs: this.jobs.size,
            enabledJobs: Array.from(this.jobs.values()).filter((j) => j.enabled).length,
            timezone: this.config.timezone ?? 'local',
        });
    }
    async stop() {
        // Stop all cron jobs
        for (const scheduledTask of this.cronJobs.values()) {
            await Promise.resolve(scheduledTask.stop());
        }
        this.cronJobs.clear();
        this.db?.close();
    }
    async healthCheck() {
        const totalJobs = this.jobs.size;
        const enabledJobs = Array.from(this.jobs.values()).filter((j) => j.enabled).length;
        const registeredHandlers = this.handlers.size;
        const recentlyFailed = this.getRecentlyFailed(15 * 60 * 1000); // last 15 min
        return {
            healthy: true,
            details: {
                totalJobs,
                enabledJobs,
                registeredHandlers,
                recentlyFailed,
                uptime: Date.now() - this.schedulerStartTime,
            },
        };
    }
    // ─── Handler Registration ─────────────────────────────────────────────────
    /** Register a handler function that can be called by jobs */
    registerHandler(handlerId, handler) {
        if (this.handlers.has(handlerId)) {
            this.ctx.logger.warn('Handler overwritten', { handlerId });
        }
        this.handlers.set(handlerId, handler);
        this.ctx.logger.debug('Handler registered', { handlerId });
    }
    /** Unregister a handler */
    unregisterHandler(handlerId) {
        const result = this.handlers.delete(handlerId);
        if (result) {
            this.ctx.logger.debug('Handler unregistered', { handlerId });
        }
        return result;
    }
    // ─── Job Management ───────────────────────────────────────────────────────
    /** Add a new cron job */
    async addJob(name, cronExpr, handlerId, enabled = true) {
        if (this.jobs.size >= this.config.maxJobs) {
            throw new Error(`Max job limit reached: ${this.config.maxJobs}`);
        }
        // Validate cron expression
        if (!node_cron_1.default.validate(cronExpr)) {
            throw new Error(`Invalid cron expression: ${cronExpr}`);
        }
        // Check if handler exists
        if (!this.handlers.has(handlerId)) {
            throw new Error(`Handler not found: ${handlerId}`);
        }
        const id = `job:${Date.now()}:${Math.random().toString(36).substr(2, 9)}`;
        const now = Date.now();
        const job = {
            id,
            name,
            cronExpr,
            handlerId,
            enabled,
            lastRun: null,
            lastError: null,
            createdAt: now,
        };
        // Save to DB
        this.db
            .prepare(`INSERT INTO jobs (id, name, cron_expr, handler_id, enabled, created_at)
       VALUES (?, ?, ?, ?, ?, ?)`)
            .run(id, name, cronExpr, handlerId, enabled ? 1 : 0, now);
        this.jobs.set(id, job);
        // Schedule if enabled
        if (enabled) {
            this.scheduleJob(job);
        }
        this.ctx.logger.info('Job added', { jobId: id, name, cronExpr, handlerId, enabled });
        return job;
    }
    /** Remove a job */
    async removeJob(jobId) {
        const job = this.jobs.get(jobId);
        if (!job)
            return false;
        // Stop cron if running
        const cron = this.cronJobs.get(jobId);
        if (cron) {
            await Promise.resolve(cron.stop());
            this.cronJobs.delete(jobId);
        }
        // Remove from DB
        this.db.prepare('DELETE FROM jobs WHERE id = ?').run(jobId);
        this.jobs.delete(jobId);
        this.ctx.logger.info('Job removed', { jobId, name: job.name });
        return true;
    }
    /** List all jobs */
    listJobs() {
        return Array.from(this.jobs.values());
    }
    /** Get job by ID */
    async getJob(jobId) {
        return this.jobs.get(jobId) ?? null;
    }
    /** Enable a job */
    async enableJob(jobId) {
        const job = this.jobs.get(jobId);
        if (!job)
            return false;
        job.enabled = true;
        this.db.prepare('UPDATE jobs SET enabled = 1 WHERE id = ?').run(jobId);
        this.scheduleJob(job);
        this.ctx.logger.debug('Job enabled', { jobId });
        return true;
    }
    /** Disable a job */
    async disableJob(jobId) {
        const job = this.jobs.get(jobId);
        if (!job)
            return false;
        job.enabled = false;
        this.db.prepare('UPDATE jobs SET enabled = 0 WHERE id = ?').run(jobId);
        // Stop cron if running
        const cron = this.cronJobs.get(jobId);
        if (cron) {
            await Promise.resolve(cron.stop());
            this.cronJobs.delete(jobId);
        }
        this.ctx.logger.debug('Job disabled', { jobId });
        return true;
    }
    /** Run a job immediately (bypassing schedule) */
    async runJob(jobId) {
        const job = this.jobs.get(jobId);
        if (!job) {
            this.ctx.logger.warn('Job not found for immediate run', { jobId });
            return false;
        }
        const handler = this.handlers.get(job.handlerId);
        if (!handler) {
            this.ctx.logger.error('Handler not found for job', { jobId, handlerId: job.handlerId });
            return false;
        }
        this.ctx.logger.info('Running job manually', { jobId, name: job.name });
        await this.executeHandler(job, handler);
        return true;
    }
    /** Trigger a job run by name */
    async triggerJobByName(name) {
        let triggered = 0;
        for (const job of this.jobs.values()) {
            if (job.name === name && job.enabled) {
                await this.runJob(job.id);
                triggered++;
            }
        }
        return triggered;
    }
    // ─── Scheduling ───────────────────────────────────────────────────────────
    scheduleJob(job) {
        if (this.cronJobs.has(job.id)) {
            const previousTask = this.cronJobs.get(job.id);
            if (previousTask) {
                void Promise.resolve(previousTask.stop()).catch((err) => {
                    this.ctx.logger.warn('Failed to stop previous cron task before rescheduling', {
                        jobId: job.id,
                        error: err instanceof Error ? err.message : String(err),
                    });
                });
            }
        }
        const cronJob = node_cron_1.default.schedule(job.cronExpr, async () => {
            const handler = this.handlers.get(job.handlerId);
            if (!handler) {
                this.ctx.logger.error('Handler not found during cron execution', {
                    jobId: job.id,
                    handlerId: job.handlerId,
                });
                return;
            }
            await this.executeHandler(job, handler);
        }, {
            timezone: this.config.timezone,
        });
        this.cronJobs.set(job.id, cronJob);
    }
    async executeHandler(job, handler) {
        const startTime = Date.now();
        let success = false;
        let error = null;
        // --- Atomic checkout: try to claim the job in running_tasks. If another process claimed it, skip.
        try {
            const insert = this.db.prepare('INSERT INTO running_tasks (job_id, started_at) VALUES (?, ?)');
            insert.run(job.id, Date.now());
        }
        catch (e) {
            // UNIQUE constraint failed -> another worker is running it
            this.ctx.logger.info('Job checkout failed, already running elsewhere', { jobId: job.id });
            return;
        }
        // --- Concurrency check: ensure we don't exceed maxConcurrentRuns
        const runningCount = this.db.prepare('SELECT COUNT(*) as c FROM running_tasks').get()?.c ?? 0;
        if (runningCount > this.config.maxConcurrentRuns) {
            this.ctx.logger.warn('Concurrency limit reached, skipping job', {
                jobId: job.id,
                runningCount,
            });
            // release claim
            this.db.prepare('DELETE FROM running_tasks WHERE job_id = ?').run(job.id);
            return;
        }
        // --- Budget enforcement hook (optional)
        try {
            const budgetService = this.getBudgetService();
            if (budgetService) {
                const ok = await budgetService.checkJobBudget(job.id);
                if (!ok) {
                    this.ctx.logger.warn('Job skipped due to budget enforcement', { jobId: job.id });
                    this.db.prepare('DELETE FROM running_tasks WHERE job_id = ?').run(job.id);
                    return;
                }
            }
        }
        catch (e) {
            this.ctx.logger.error('Budget check failed, proceeding cautiously', { err: e });
        }
        try {
            // Run handler
            await handler(job.id);
            success = true;
        }
        catch (err) {
            error = err instanceof Error ? err.message : String(err);
            this.ctx.logger.error('Job handler failed', { jobId: job.id, error });
        }
        finally {
            const endTime = Date.now();
            // Update job last run
            job.lastRun = endTime;
            if (!success) {
                job.lastError = error ?? 'Unknown error';
            }
            this.db
                .prepare('UPDATE jobs SET last_run = ?, last_error = ? WHERE id = ?')
                .run(endTime, error, job.id);
            // Record run in history
            this.db
                .prepare(`INSERT INTO job_runs (job_id, started_at, completed_at, success, error)
         VALUES (?, ?, ?, ?, ?)`)
                .run(job.id, startTime, endTime, success ? 1 : 0, error);
            // persist any session state optionally provided by handler via running_tasks.session_state
            try {
                const row = this.db
                    .prepare('SELECT session_state FROM running_tasks WHERE job_id = ?')
                    .get(job.id);
                if (row?.session_state) {
                    // store as last_error field for visibility (placeholder) or a dedicated sessions table
                    this.db
                        .prepare('UPDATE jobs SET last_error = ? WHERE id = ?')
                        .run(`session:${row.session_state}`, job.id);
                }
            }
            catch (e) {
                this.ctx.logger.warn('Unable to persist cron session state', {
                    jobId: job.id,
                    error: e instanceof Error ? e.message : String(e),
                });
            }
            // release claim
            this.db.prepare('DELETE FROM running_tasks WHERE job_id = ?').run(job.id);
        }
    }
    // ─── Queries ───────────────────────────────────────────────────────────────
    getRecentlyFailed(withinMs) {
        const cutoff = Date.now() - withinMs;
        const count = this.db
            .prepare('SELECT COUNT(*) as c FROM job_runs WHERE started_at >= ? AND success = 0')
            .get(cutoff).c;
        return count;
    }
    async getJobRuns(jobId, limit = 50) {
        const rows = this.db
            .prepare('SELECT * FROM job_runs WHERE job_id = ? ORDER BY started_at DESC LIMIT ?')
            .all(jobId, limit);
        return rows.map((row) => ({
            jobId: row.job_id,
            startedAt: row.started_at,
            completedAt: row.completed_at,
            success: Boolean(row.success),
            error: row.error,
        }));
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
      CREATE TABLE IF NOT EXISTS jobs (
        id TEXT PRIMARY KEY,
        name TEXT NOT NULL,
        cron_expr TEXT NOT NULL,
        handler_id TEXT NOT NULL,
        enabled INTEGER NOT NULL DEFAULT 1,
        last_run INTEGER,
        last_error TEXT,
        created_at INTEGER NOT NULL
      );

      CREATE TABLE IF NOT EXISTS job_runs (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        job_id TEXT NOT NULL,
        started_at INTEGER NOT NULL,
        completed_at INTEGER,
        success INTEGER NOT NULL DEFAULT 0,
        error TEXT,
        FOREIGN KEY (job_id) REFERENCES jobs(id) ON DELETE CASCADE
      );

      -- running_tasks is used to perform atomic checkouts and limit concurrency
      CREATE TABLE IF NOT EXISTS running_tasks (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        job_id TEXT NOT NULL UNIQUE,
        started_at INTEGER NOT NULL,
        session_state TEXT,
        FOREIGN KEY (job_id) REFERENCES jobs(id) ON DELETE CASCADE
      );

      CREATE INDEX IF NOT EXISTS idx_running_tasks_started ON running_tasks(started_at);

      CREATE INDEX IF NOT EXISTS idx_jobs_enabled ON jobs(enabled);
      CREATE INDEX IF NOT EXISTS idx_job_runs_job ON job_runs(job_id);
      CREATE INDEX IF NOT EXISTS idx_job_runs_started ON job_runs(started_at);
    `);
    }
    loadJobsFromDb() {
        const rows = this.db.prepare('SELECT * FROM jobs').all();
        for (const row of rows) {
            const job = {
                id: row.id,
                name: row.name,
                cronExpr: row.cron_expr,
                handlerId: row.handler_id,
                enabled: Boolean(row.enabled),
                lastRun: row.last_run,
                lastError: row.last_error,
                createdAt: row.created_at,
            };
            this.jobs.set(job.id, job);
        }
    }
    getBudgetService() {
        const contextWithServices = this.ctx;
        const candidate = contextWithServices.services?.budget;
        if (candidate &&
            typeof candidate === 'object' &&
            'checkJobBudget' in candidate &&
            typeof candidate.checkJobBudget === 'function') {
            return candidate;
        }
        return undefined;
    }
}
exports.default = new CronSchedulerFeature();
//# sourceMappingURL=index.js.map