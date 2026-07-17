// SPDX-License-Identifier: MIT
import { existsSync, mkdirSync, readFileSync } from 'fs';
import { dirname, resolve } from 'path';

import Database from 'better-sqlite3';

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

interface PackageMetadata {
  version?: unknown;
}

interface UpdateHistoryRow {
  version: string;
  timestamp: number;
  success: number;
  message: string;
}

// ─── Feature ─────────────────────────────────────────────────────────────────

class AutoUpdateFeature implements FeatureModule {
  readonly meta: FeatureMeta = {
    name: 'auto-update',
    version: '0.0.9',
    description: 'Optional GitHub release checks and update history for Argentum',
    dependencies: [],
  };

  private config: AutoUpdateConfig = {
    enabled: false,
    dbPath: './data/auto-update.db',
    repoOwner: 'AG064',
    repoName: 'argentum',
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
        const pkg = JSON.parse(readFileSync(pkgPath, 'utf8')) as PackageMetadata;
        this.currentVersion = typeof pkg.version === 'string' ? pkg.version : '0.0.9';
      } else {
        this.currentVersion = '0.0.9';
      }
    } catch {
      this.currentVersion = '0.0.9';
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
        latestVersion: this.latestRelease?.version ?? 'unknown',
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
          this.ctx.logger.warn(
            'Automatic install requested, but signed in-place updates are not enabled in this build',
            { releaseUrl: release.url },
          );
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
    return this.latestRelease?.releaseNotes ?? null;
  }

  /** Apply update */
  async applyUpdate(_component: string = 'argentum'): Promise<UpdateResult> {
    if (!this.latestRelease) {
      const release = await this.checkUpdates();
      if (!release) {
        return { success: false, version: this.currentVersion, message: 'No updates available' };
      }
    }

    const message =
      'Signed in-place installation is not enabled in this build. Open the release URL and use the platform installer; Argentum will not report an update as installed until signature verification and atomic replacement are implemented.';
    this.ctx.logger.warn('Update installation unavailable', {
      from: this.currentVersion,
      to: this.latestRelease!.version,
      releaseUrl: this.latestRelease!.url,
    });
    return { success: false, version: this.currentVersion, message, rollbackAvailable: false };
  }

  /** Rollback to previous version */
  async rollback(_component: string = 'argentum'): Promise<UpdateResult> {
    const message =
      'Rollback is unavailable because this build does not perform signed in-place installations.';
    this.ctx.logger.warn('Update rollback unavailable');
    return { success: false, version: this.currentVersion, message, rollbackAvailable: false };
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
          'User-Agent': `argentum-updater/${this.currentVersion}`,
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
        releaseNotes: data.body ?? 'No release notes',
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
      const li = l[i] ?? 0;
      const ci = c[i] ?? 0;
      if (li > ci) return true;
      if (li < ci) return false;
    }
    return false;
  }

  // ─── Database ─────────────────────────────────────────────────────────────

  private initDatabase(): void {
    const fullPath = resolve(this.config.dbPath);
    if (!existsSync(dirname(fullPath))) {
      mkdirSync(dirname(fullPath), { recursive: true });
    }

    this.db = new Database(fullPath);
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

  private loadUpdateHistory(): void {
    const rows = this.database
      .prepare<
        [],
        UpdateHistoryRow
      >('SELECT * FROM update_history ORDER BY timestamp DESC LIMIT 100')
      .all();
    this.updateHistory = rows.map((row) => ({
      version: row.version,
      timestamp: row.timestamp,
      success: Boolean(row.success),
      message: row.message,
    }));
  }

  private async recordUpdate(version: string, success: boolean, message: string): Promise<void> {
    const timestamp = Date.now();
    this.database
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
  private db: Database.Database | null = null;

  private get database(): Database.Database {
    if (!this.db) {
      throw new Error('Auto-update database is not initialized');
    }
    return this.db;
  }
}

export default new AutoUpdateFeature();
