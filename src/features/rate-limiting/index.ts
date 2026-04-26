/**
 * Rate Limiting Feature
 *
 * Provides sliding-window rate limiting with configurable limits and keys.
 * Can be used by other features via direct import.
 */

import { mkdirSync } from 'fs';
import path from 'path';

import Database from 'better-sqlite3';

import {
  type FeatureModule,
  type FeatureContext,
  type FeatureMeta,
  type HealthStatus,
} from '../../core/plugin-loader';

/** Rate limiting configuration */
export interface RateLimitConfig {
  windowMs: number;
  max: number;
}

class RateLimitingFeature implements FeatureModule {
  readonly meta: FeatureMeta = {
    name: 'rate-limiting',
    version: '0.0.2',
    description: 'Sliding window rate limiting with configurable limits',
    dependencies: [],
  };

  private db: Database.Database;
  private ctx!: FeatureContext;
  private config: RateLimitConfig = {
    windowMs: 60_000,
    max: 100,
  };

  constructor() {
    const dataDir = path.join(process.cwd(), 'data');
    mkdirSync(dataDir, { recursive: true });
    const dbPath = process.env.AGCLAW_DB_PATH || path.join(dataDir, 'agclaw.db');
    this.db = new Database(dbPath);
  }

  async init(config: Record<string, unknown>, context: FeatureContext): Promise<void> {
    this.ctx = context;
    if (config['windowMs']) this.config['windowMs'] = config['windowMs'] as number;
    if (config['max']) this.config['max'] = config['max'] as number;
    this.initDb();
  }

  private initDb(): void {
    this.db
      .prepare(
        `
      CREATE TABLE IF NOT EXISTS rate_windows (
        key TEXT NOT NULL,
        timestamp INTEGER NOT NULL
      );
    `,
      )
      .run();
    this.db
      .prepare('CREATE INDEX IF NOT EXISTS idx_rate_key_ts ON rate_windows(key, timestamp)')
      .run();
  }

  async start(): Promise<void> {
    this.ctx.logger.info('Rate limiting started', {
      windowMs: this.config.windowMs,
      max: this.config.max,
    });
  }

  async stop(): Promise<void> {
    this.db.close();
  }

  async healthCheck(): Promise<HealthStatus> {
    // Simple check: can we write/read?
    try {
      const testKey = `health_${Date.now()}`;
      this.checkInternal(testKey, 1, 1000);
      return { healthy: true, message: 'Rate limiting OK' };
    } catch (err) {
      return { healthy: false, message: 'Rate limiting check failed' };
    }
  }

  // Sliding window: store timestamps and count how many within window
  check(key: string, limit?: number, windowMs?: number): { allowed: boolean; count: number } {
    return this.checkInternal(key, limit ?? this.config.max, windowMs ?? this.config.windowMs);
  }

  private checkInternal(
    key: string,
    limit: number,
    windowMs: number,
  ): { allowed: boolean; count: number } {
    const now = Date.now();
    const cutoff = now - windowMs;

    // Clean up old entries occasionally (every 1000th check)
    if (Math.random() < 0.001) {
      this.db
        .prepare('DELETE FROM rate_windows WHERE timestamp < ?')
        .run(Date.now() - windowMs * 2);
    }

    const insert = this.db.prepare('INSERT INTO rate_windows (key, timestamp) VALUES (?, ?)');
    insert.run(key, now);

    const row = this.db
      .prepare('SELECT COUNT(*) as c FROM rate_windows WHERE key = ? AND timestamp >= ?')
      .get(key, cutoff) as { c: number };
    const count = row.c;
    return { allowed: count <= limit, count };
  }

  reset(key: string): { changes: number } {
    const info = this.db.prepare('DELETE FROM rate_windows WHERE key = ?').run(key);
    return { changes: info.changes };
  }

  getStats(key: string): number[] {
    const rows = this.db
      .prepare(
        'SELECT timestamp FROM rate_windows WHERE key = ? ORDER BY timestamp DESC LIMIT 1000',
      )
      .all(key) as Array<{ timestamp: number }>;
    return rows.map((r) => r.timestamp);
  }
}

export default new RateLimitingFeature();
