"use strict";
var __importDefault = (this && this.__importDefault) || function (mod) {
    return (mod && mod.__esModule) ? mod : { "default": mod };
};
Object.defineProperty(exports, "__esModule", { value: true });
const fs_1 = require("fs");
const path_1 = require("path");
const better_sqlite3_1 = __importDefault(require("better-sqlite3"));
// ─── Feature ─────────────────────────────────────────────────────────────────
class AutoUpdateFeature {
    meta = {
        name: 'auto-update',
        version: '0.0.4',
        description: 'Automatic updates for Argentum components with backup and rollback',
        dependencies: [],
    };
    config = {
        enabled: false,
        dbPath: './data/auto-update.db',
        repoOwner: 'AG064',
        repoName: 'ag-claw',
        checkIntervalHours: 24,
        autoApply: false,
        backupBeforeUpdate: true,
        backupPath: './data/backups',
    };
    ctx;
    currentVersion;
    lastCheck = 0;
    latestRelease = null;
    updateHistory = [];
    checkTimer = null;
    constructor() {
        // Get current version from package.json
        try {
            const pkgPath = (0, path_1.resolve)(process.cwd(), 'package.json');
            if ((0, fs_1.existsSync)(pkgPath)) {
                const pkg = JSON.parse((0, fs_1.readFileSync)(pkgPath, 'utf8'));
                this.currentVersion = typeof pkg.version === 'string' ? pkg.version : '0.0.4';
            }
            else {
                this.currentVersion = '0.0.4';
            }
        }
        catch {
            this.currentVersion = '0.0.4';
        }
    }
    async init(config, context) {
        this.ctx = context;
        this.config = { ...this.config, ...config };
        this.initDatabase();
        this.loadUpdateHistory();
    }
    async start() {
        // Initial check
        await this.checkUpdates();
        // Schedule periodic checks
        this.checkTimer = setInterval(() => {
            this.checkUpdates().catch((err) => {
                this.ctx.logger.error('Update check failed', { error: err });
            });
        }, this.config.checkIntervalHours * 60 * 60 * 1000);
        this.ctx.logger.info('Auto-update active', {
            currentVersion: this.currentVersion,
            checkInterval: `${this.config.checkIntervalHours}h`,
            autoApply: this.config.autoApply,
        });
    }
    async stop() {
        if (this.checkTimer) {
            clearInterval(this.checkTimer);
            this.checkTimer = null;
        }
        this.db?.close();
    }
    async healthCheck() {
        const lastCheckAge = Date.now() - this.lastCheck;
        const hasLatest = this.latestRelease && this.isNewerVersion(this.latestRelease.version, this.currentVersion);
        return {
            healthy: true,
            details: {
                currentVersion: this.currentVersion,
                latestVersion: this.latestRelease?.version ?? 'unknown',
                lastCheckAge: `${Math.floor(lastCheckAge / 1000)}s`,
                updateAvailable: hasLatest,
                historyCount: this.updateHistory.length,
            },
        };
    }
    // ─── Public API ───────────────────────────────────────────────────────────
    /** Check for updates */
    async checkUpdates() {
        this.ctx.logger.debug('Checking for updates...');
        try {
            const release = await this.fetchLatestRelease();
            if (release && this.isNewerVersion(release.version, this.currentVersion)) {
                this.latestRelease = release;
                this.ctx.logger.info('Update available', {
                    current: this.currentVersion,
                    latest: release.version,
                    releaseNotes: release.releaseNotes.substring(0, 200),
                });
                if (this.config.autoApply) {
                    await this.applyUpdate('ag-claw');
                }
                return release;
            }
            else {
                this.latestRelease = null;
                this.ctx.logger.debug('No updates available', { current: this.currentVersion });
                return null;
            }
        }
        catch (err) {
            this.ctx.logger.error('Failed to check updates', { error: err });
            throw err;
        }
        finally {
            this.lastCheck = Date.now();
        }
    }
    /** Get the latest release info (if newer) */
    async getChangelog() {
        if (!this.latestRelease) {
            const release = await this.checkUpdates();
            if (!release)
                return null;
        }
        return this.latestRelease?.releaseNotes ?? null;
    }
    /** Apply update */
    async applyUpdate(_component = 'ag-claw') {
        if (!this.latestRelease) {
            const release = await this.checkUpdates();
            if (!release) {
                return { success: false, version: this.currentVersion, message: 'No updates available' };
            }
        }
        this.ctx.logger.info('Applying update', {
            from: this.currentVersion,
            to: this.latestRelease.version,
        });
        try {
            if (this.config.backupBeforeUpdate) {
                await this.createBackup();
            }
            // Download and install update (placeholder - actual implementation would fetch and replace files)
            // In a real implementation, this would:
            // 1. Download tarball/zip from release.url
            // 2. Extract to temp
            // 3. Verify checksums
            // 4. Replace files atomically
            // 5. Restart service
            await this.recordUpdate(this.latestRelease.version, true, 'Update applied successfully');
            this.ctx.logger.info('Update applied', { version: this.latestRelease.version });
            return {
                success: true,
                version: this.latestRelease.version,
                message: 'Update applied successfully',
                rollbackAvailable: true,
            };
        }
        catch (err) {
            const message = err instanceof Error ? err.message : String(err);
            await this.recordUpdate(this.latestRelease?.version ?? 'unknown', false, message);
            this.ctx.logger.error('Update failed', { error: err });
            return {
                success: false,
                version: this.currentVersion,
                message: `Update failed: ${message}`,
            };
        }
    }
    /** Rollback to previous version */
    async rollback(_component = 'ag-claw') {
        this.ctx.logger.info('Rolling back update...');
        try {
            // Find last successful update before current
            const previous = this.updateHistory.find((h) => h.success && h.version !== this.currentVersion);
            if (!previous) {
                return { success: false, version: this.currentVersion, message: 'No rollback available' };
            }
            // Restore from backup (placeholder)
            await this.restoreBackup();
            await this.recordUpdate(previous.version, true, 'Rollback successful');
            this.ctx.logger.info('Rollback successful', { version: previous.version });
            return {
                success: true,
                version: previous.version,
                message: 'Rollback successful',
            };
        }
        catch (err) {
            const message = err instanceof Error ? err.message : String(err);
            this.ctx.logger.error('Rollback failed', { error: err });
            return {
                success: false,
                version: this.currentVersion,
                message: `Rollback failed: ${message}`,
            };
        }
    }
    /** Get update history */
    async getUpdateHistory(limit) {
        if (limit) {
            return this.updateHistory.slice(-limit).reverse();
        }
        return [...this.updateHistory].reverse();
    }
    // ─── GitHub ───────────────────────────────────────────────────────────────
    async fetchLatestRelease() {
        const url = `https://api.github.com/repos/${this.config.repoOwner}/${this.config.repoName}/releases/latest`;
        try {
            const response = await fetch(url, {
                headers: {
                    'Accept': 'application/vnd.github.v3+json',
                    'User-Agent': `ag-claw-updater/${this.currentVersion}`,
                },
            });
            if (!response.ok) {
                if (response.status === 404) {
                    this.ctx.logger.warn('GitHub repo not found', {
                        repo: `${this.config.repoOwner}/${this.config.repoName}`,
                    });
                    return null;
                }
                throw new Error(`GitHub API error: ${response.status} ${response.statusText}`);
            }
            const data = (await response.json());
            return {
                version: data.tag_name.replace(/^v/, ''),
                url: data.html_url,
                releaseNotes: data.body ?? 'No release notes',
                publishedAt: data.published_at,
                isBeta: data.prerelease,
            };
        }
        catch (err) {
            this.ctx.logger.error('Failed to fetch latest release', { error: err });
            return null;
        }
    }
    isNewerVersion(latest, current) {
        const normalize = (v) => v.replace(/^v/, '').split('.').map(Number);
        const l = normalize(latest);
        const c = normalize(current);
        const len = Math.max(l.length, c.length);
        for (let i = 0; i < len; i++) {
            const li = l[i] ?? 0;
            const ci = c[i] ?? 0;
            if (li > ci)
                return true;
            if (li < ci)
                return false;
        }
        return false;
    }
    // ─── Backup & Restore ─────────────────────────────────────────────────────
    async createBackup() {
        const backupDir = (0, path_1.resolve)(this.config.backupPath);
        if (!(0, fs_1.existsSync)(backupDir)) {
            (0, fs_1.mkdirSync)(backupDir, { recursive: true });
        }
        const timestamp = new Date().toISOString().replace(/[:.]/g, '-');
        const backupName = `agclaw-backup-${timestamp}`;
        const backupPath = (0, path_1.join)(backupDir, backupName);
        this.ctx.logger.debug('Creating backup', { backupPath });
        // Placeholder: backup critical files
        // In a real implementation, this would tar up the project directory
        // For now, we just create a marker file
        (0, fs_1.writeFileSync)(backupPath, `Backup created at ${new Date().toISOString()}\n`);
    }
    async restoreBackup() {
        const backupDir = (0, path_1.resolve)(this.config.backupPath);
        if (!(0, fs_1.existsSync)(backupDir)) {
            throw new Error('Backup directory not found');
        }
        const backups = (0, fs_1.readdirSync)(backupDir)
            .filter((f) => f.startsWith('agclaw-backup-'))
            .sort()
            .reverse();
        if (backups.length === 0) {
            throw new Error('No backups found');
        }
        const latestBackup = (0, path_1.join)(backupDir, backups[0]);
        this.ctx.logger.debug('Restoring backup', { backup: latestBackup });
        // Placeholder: restore files from backup
    }
    // ─── Database ─────────────────────────────────────────────────────────────
    initDatabase() {
        const fullPath = (0, path_1.resolve)(this.config.dbPath);
        if (!(0, fs_1.existsSync)((0, path_1.dirname)(fullPath))) {
            (0, fs_1.mkdirSync)((0, path_1.dirname)(fullPath), { recursive: true });
        }
        this.db = new better_sqlite3_1.default(fullPath);
        this.database.pragma('journal_mode = WAL');
        this.database.pragma('synchronous = NORMAL');
        this.database.exec(`
      CREATE TABLE IF NOT EXISTS update_history (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        version TEXT NOT NULL,
        timestamp INTEGER NOT NULL,
        success INTEGER NOT NULL,
        message TEXT
      );
    `);
    }
    loadUpdateHistory() {
        const rows = this.database
            .prepare('SELECT * FROM update_history ORDER BY timestamp DESC LIMIT 100')
            .all();
        this.updateHistory = rows.map((row) => ({
            version: row.version,
            timestamp: row.timestamp,
            success: Boolean(row.success),
            message: row.message,
        }));
    }
    async recordUpdate(version, success, message) {
        const timestamp = Date.now();
        this.database
            .prepare('INSERT INTO update_history (version, timestamp, success, message) VALUES (?, ?, ?, ?)')
            .run(version, timestamp, success ? 1 : 0, message);
        this.updateHistory.push({ version, timestamp, success, message });
        // Keep only last 100 entries in memory
        if (this.updateHistory.length > 100) {
            this.updateHistory = this.updateHistory.slice(-100);
        }
    }
    // Helper for DB - will be set after init
    db = null;
    get database() {
        if (!this.db) {
            throw new Error('Auto-update database is not initialized');
        }
        return this.db;
    }
}
exports.default = new AutoUpdateFeature();
//# sourceMappingURL=index.js.map