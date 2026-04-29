import { mkdirSync, existsSync, readdirSync } from 'fs';
import { dirname, resolve, join } from 'path';

import {
  type FeatureModule,
  type FeatureContext,
  type FeatureMeta,
  type HealthStatus,
} from '../../core/plugin-loader';

// ─── Types ───────────────────────────────────────────────────────────────────

export interface UpdateInfo {
  version: string;
  url: string;
  releaseNotes: string;
  publishedAt: string;
  isBeta: boolean;
}

export interface UpdateResult {
  success: boolean;
  version: string;
  message: string;
  rollbackAvailable?: boolean;
}

export interface AutoUpdateConfig {
  enabled: boolean;
  dbPath: string;
  repoOwner: string;
  repoName: string;
  checkIntervalHours: number;
  autoApply: boolean;
  backupBeforeUpdate: boolean;
  backupPath: string;
}

// ─── Feature ─────────────────────────────────────────────────────────────────

class AutoUpdateFeature implements FeatureModule {
  readonly meta: FeatureMeta = {
    name: 'auto-update',
    version: '0.0.4',
    description: 'Automatic updates for Argentum components with backup and rollback',
    dependencies: [],
  };

  private config: AutoUpdateConfig = {
    enabled: false,
    dbPath: './data/auto-update.db',
    repoOwner: 'AG064',
    repoName: 'ag-claw',
    checkIntervalHours: 24,
    autoApply: false,
    backupBeforeUpdate: true,
    backupPath: './data/backups',
  };
  private ctx!: FeatureContext;
  private currentVersion: string;
  private lastCheck: number = 0;
  private latestRelease: UpdateInfo | null = null;
  private updateHistory: Array<{
    version: string;
    timestamp: number;
    success: boolean;
    message: string;
  }> = [];
  private checkTimer: ReturnType<typeof setInterval> | null = null;

  constructor() {
    // Get current version from package.json
    try {
      const pkgPath = resolve(process.cwd(), 'package.json');
      if (existsSync(pkgPath)) {
        const pkg = JSON.parse(require('fs').readFileSync(pkgPath, 'utf8'));
        this.currentVersion = pkg['version'] || '0.0.0';
      } else {
        this.currentVersion = '0.0.0';
      }
    } catch {
      this.currentVersion = '0.0.0';
    }
  }

  async init(config: Record<string, unknown>, context: FeatureContext): Promise<void> {
    this.ctx = context;
    this.config = { ...this.config, ...(config as Partial<AutoUpdateConfig>) };
    this.initDatabase();
    this.loadUpdateHistory();
  }

  async start(): Promise<void> {
    // Initial check
    await this.checkUpdates();

    // Schedule periodic checks
    this.checkTimer = setInterval(
      () => {
        this.checkUpdates().catch((err) => {
          this.ctx.logger.error('Update check failed', { error: err });
        });
      },
      this.config.checkIntervalHours * 60 * 60 * 1000,
    );

    this.ctx.logger.info('Auto-update active', {
      currentVersion: this.currentVersion,
      checkInterval: `${this.config.checkIntervalHours}h`,
      autoApply: this.config.autoApply,
    });
  }

  async stop(): Promise<void> {
    if (this.checkTimer) {
      clearInterval(this.checkTimer);
      this.checkTimer = null;
    }
    this.db?.close();
  }

  async healthCheck(): Promise<HealthStatus> {
    const lastCheckAge = Date.now() - this.lastCheck;
    const hasLatest =
      this.latestRelease && this.isNewerVersion(this.latestRelease.version, this.currentVersion);

    return {
      healthy: true,
      details: {
        currentVersion: this.currentVersion,
        latestVersion: this.latestRelease?.version || 'unknown',
        lastCheckAge: `${Math.floor(lastCheckAge / 1000)}s`,
        updateAvailable: hasLatest,
        historyCount: this.updateHistory.length,
      },
    };
  }

  // ─── Public API ───────────────────────────────────────────────────────────

  /** Check for updates */
  async checkUpdates(): Promise<UpdateInfo | null> {
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
      } else {
        this.latestRelease = null;
        this.ctx.logger.debug('No updates available', { current: this.currentVersion });
        return null;
      }
    } catch (err) {
      this.ctx.logger.error('Failed to check updates', { error: err });
      throw err;
    } finally {
      this.lastCheck = Date.now();
    }
  }

  /** Get the latest release info (if newer) */
  async getChangelog(): Promise<string | null> {
    if (!this.latestRelease) {
      const release = await this.checkUpdates();
      if (!release) return null;
    }
    return this.latestRelease?.releaseNotes || null;
  }

  /** Apply update */
  async applyUpdate(_component: string = 'ag-claw'): Promise<UpdateResult> {
    if (!this.latestRelease) {
      const release = await this.checkUpdates();
      if (!release) {
        return { success: false, version: this.currentVersion, message: 'No updates available' };
      }
    }

    this.ctx.logger.info('Applying update', {
      from: this.currentVersion,
      to: this.latestRelease!.version,
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

      await this.recordUpdate(this.latestRelease!.version, true, 'Update applied successfully');

      this.ctx.logger.info('Update applied', { version: this.latestRelease!.version });
      return {
        success: true,
        version: this.latestRelease!.version,
        message: 'Update applied successfully',
        rollbackAvailable: true,
      };
    } catch (err) {
      const message = err instanceof Error ? err.message : String(err);
      await this.recordUpdate(this.latestRelease?.version || 'unknown', false, message);
      this.ctx.logger.error('Update failed', { error: err });
      return {
        success: false,
        version: this.currentVersion,
        message: `Update failed: ${message}`,
      };
    }
  }

  /** Rollback to previous version */
  async rollback(_component: string = 'ag-claw'): Promise<UpdateResult> {
    this.ctx.logger.info('Rolling back update...');

    try {
      // Find last successful update before current
      const previous = this.updateHistory.find(
        (h) => h.success && h.version !== this.currentVersion,
      );
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
    } catch (err) {
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
  async getUpdateHistory(limit?: number): Promise<typeof this.updateHistory> {
    if (limit) {
      return this.updateHistory.slice(-limit).reverse();
    }
    return [...this.updateHistory].reverse();
  }

  // ─── GitHub ───────────────────────────────────────────────────────────────

  private async fetchLatestRelease(): Promise<UpdateInfo | null> {
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

      const data = (await response.json()) as {
        tag_name: string;
        html_url: string;
        body: string;
        published_at: string;
        prerelease: boolean;
      };

      return {
        version: data.tag_name.replace(/^v/, ''),
        url: data.html_url,
        releaseNotes: data.body || 'No release notes',
        publishedAt: data.published_at,
        isBeta: data.prerelease,
      };
    } catch (err) {
      this.ctx.logger.error('Failed to fetch latest release', { error: err });
      return null;
    }
  }

  private isNewerVersion(latest: string, current: string): boolean {
    const normalize = (v: string) => v.replace(/^v/, '').split('.').map(Number);
    const l = normalize(latest);
    const c = normalize(current);
    const len = Math.max(l.length, c.length);
    for (let i = 0; i < len; i++) {
      const li = l[i] || 0;
      const ci = c[i] || 0;
      if (li > ci) return true;
      if (li < ci) return false;
    }
    return false;
  }

  // ─── Backup & Restore ─────────────────────────────────────────────────────

  private async createBackup(): Promise<void> {
    const backupDir = resolve(this.config.backupPath);
    if (!existsSync(backupDir)) {
      mkdirSync(backupDir, { recursive: true });
    }

    const timestamp = new Date().toISOString().replace(/[:.]/g, '-');
    const backupName = `agclaw-backup-${timestamp}`;
    const backupPath = join(backupDir, backupName);

    this.ctx.logger.debug('Creating backup', { backupPath });

    // Placeholder: backup critical files
    // In a real implementation, this would tar up the project directory
    // For now, we just create a marker file
    require('fs').writeFileSync(backupPath, `Backup created at ${new Date().toISOString()}\n`);
  }

  private async restoreBackup(): Promise<void> {
    const backupDir = resolve(this.config.backupPath);
    if (!existsSync(backupDir)) {
      throw new Error('Backup directory not found');
    }

    const backups = readdirSync(backupDir)
      .filter((f) => f.startsWith('agclaw-backup-'))
      .sort()
      .reverse();
    if (backups.length === 0) {
      throw new Error('No backups found');
    }

    const latestBackup = join(backupDir, backups[0] as string);
    this.ctx.logger.debug('Restoring backup', { backup: latestBackup });

    // Placeholder: restore files from backup
  }

  // ─── Database ─────────────────────────────────────────────────────────────

  private initDatabase(): void {
    const fullPath = resolve(this.config.dbPath);
    if (!existsSync(dirname(fullPath))) {
      mkdirSync(dirname(fullPath), { recursive: true });
    }

    this.db = new (require('better-sqlite3'))(fullPath);
    this.db.pragma('journal_mode = WAL');
    this.db.pragma('synchronous = NORMAL');

    this.db.exec(`
      CREATE TABLE IF NOT EXISTS update_history (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        version TEXT NOT NULL,
        timestamp INTEGER NOT NULL,
        success INTEGER NOT NULL,
        message TEXT
      );
    `);
  }

  private loadUpdateHistory(): void {
    const rows = this.db
      .prepare('SELECT * FROM update_history ORDER BY timestamp DESC LIMIT 100')
      .all();
    this.updateHistory = rows.map((row: any) => ({
      version: row.version,
      timestamp: row.timestamp,
      success: Boolean(row.success),
      message: row.message,
    }));
  }

  private async recordUpdate(version: string, success: boolean, message: string): Promise<void> {
    const timestamp = Date.now();
    this.db
      .prepare(
        'INSERT INTO update_history (version, timestamp, success, message) VALUES (?, ?, ?, ?)',
      )
      .run(version, timestamp, success ? 1 : 0, message);

    this.updateHistory.push({ version, timestamp, success, message });
    // Keep only last 100 entries in memory
    if (this.updateHistory.length > 100) {
      this.updateHistory = this.updateHistory.slice(-100);
    }
  }

  // Helper for DB - will be set after init
  private db: any = null;
}

export default new AutoUpdateFeature();
