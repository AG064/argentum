/**
 * Trajectory Export Feature
 *
 * Records conversation trajectories for RL fine-tuning data preparation.
 * Supports JSONL export with optional gzip compression.
 *
 * Stores entries in SQLite alongside the sessions database for easy joining.
 */

import { createReadStream, createWriteStream, existsSync, mkdirSync } from 'fs';
import { dirname, resolve, join } from 'path';
import { createInterface } from 'readline';
import { pipeline } from 'stream/promises';
import { createGzip } from 'zlib';

import Database from 'better-sqlite3';


import {
  type TrajectoryEntry,
  type TrajectoryMessage,
  type TrajectoryMetadata,
  type TrajectoryExportOptions,
  type TrajectoryStats,
  type TrajectoryConfig,
} from './types';

import {
  type FeatureModule,
  type FeatureContext,
  type FeatureMeta,
  type HealthStatus,
} from '../../core/plugin-loader';

// ─── Feature ─────────────────────────────────────────────────────────────────

class TrajectoryExportFeature implements FeatureModule {
  readonly meta: FeatureMeta = {
    name: 'trajectory-export',
    version: '0.1.0',
    description: 'Export conversation trajectories as JSONL for RL fine-tuning',
    dependencies: [],
  };

  private config: TrajectoryConfig = {
    enabled: false,
    dbPath: '',
    compressExports: false,
    defaultFormat: 'jsonl',
  };
  private ctx!: FeatureContext;
  private db!: Database.Database;
  private dbPath!: string;

  constructor() {
    // Auto-init for CLI usage (singleton bypasses plugin loader)
    const workDir = process.env.AGCLAW_WORKDIR || process.cwd();
    this.dbPath = resolve(join(workDir, 'data', 'trajectory.db'));
    this.config.dbPath = this.dbPath;
    this.initDatabase();
  }

  async init(config: Record<string, unknown>, context: FeatureContext): Promise<void> {
    this.ctx = context;
    this.config = {
      enabled: true,
      dbPath: resolve((config['dbPath'] as string) ?? './data/trajectory.db'),
      compressExports: (config['compressExports'] as boolean) ?? false,
      defaultFormat: (config['defaultFormat'] as 'jsonl' | 'json') ?? 'jsonl',
    };
    this.dbPath = this.config.dbPath;
    this.initDatabase();
  }

  async start(): Promise<void> {
    this.ctx.logger.info('Trajectory export feature started', {
      dbPath: this.dbPath,
      compressExports: this.config.compressExports,
    });
  }

  async stop(): Promise<void> {
    this.db?.close();
  }

  async healthCheck(): Promise<HealthStatus> {
    try {
      const row = this.db
        .prepare('SELECT COUNT(*) as c FROM trajectories')
        .get() as { c: number };
      return { healthy: true, message: `${row.c} trajectory entries` };
    } catch {
      return { healthy: false, message: 'Trajectory database unavailable' };
    }
  }

  // ─── Public API ──────────────────────────────────────────────────────────

  /**
   * Record a trajectory entry after a conversation completes.
   */
  record(entry: TrajectoryEntry): void {
    const stmt = this.db.prepare(`
      INSERT INTO trajectories (timestamp, session_id, messages, metadata)
      VALUES (?, ?, ?, ?)
    `);
    stmt.run(
      entry.timestamp,
      entry.sessionId,
      JSON.stringify(entry.messages),
      JSON.stringify(entry.metadata),
    );
  }

  /**
   * Record a raw conversation as a trajectory.
   */
  recordConversation(
    sessionId: string,
    messages: TrajectoryMessage[],
    metadata: TrajectoryMetadata,
  ): void {
    this.record({
      timestamp: new Date().toISOString(),
      sessionId,
      messages,
      metadata,
    });
  }

  /**
   * Export trajectories matching the given options.
   * Returns the path to the exported file.
   */
  async export(options: TrajectoryExportOptions = {}): Promise<string> {
    const entries = this.queryEntries(options);
    const format = options.format ?? this.config.defaultFormat;
    const gzip = options.gzip ?? this.config.compressExports;

    const timestamp = new Date().toISOString().replace(/[:.]/g, '-').slice(0, 19);
    const ext = gzip ? 'jsonl.gz' : 'jsonl';
    const exportDir = resolve(dirname(this.dbPath));
    const outPath = resolve(exportDir, `trajectory-export-${timestamp}.${ext}`);

    if (!existsSync(exportDir)) {
      mkdirSync(exportDir, { recursive: true });
    }

    if (format === 'jsonl') {
      const content = entries.map((e) => JSON.stringify(e)).join('\n');
      if (gzip) {
        const { writeFileSync } = await import('fs');
        const zlib = await import('zlib');
        const gz = zlib.createGzip();
        const buf = Buffer.from(content, 'utf8');
        const result = await new Promise<Buffer>((resolve, reject) => {
          const chunks: Buffer[] = [];
          gz.on('data', (chunk: Buffer) => chunks.push(chunk));
          gz.on('end', () => resolve(Buffer.concat(chunks)));
          gz.on('error', reject);
          gz.end(buf);
        });
        writeFileSync(outPath, result);
      } else {
        const { writeFileSync } = await import('fs');
        writeFileSync(outPath, content, 'utf8');
      }
    } else {
      const { writeFileSync } = await import('fs');
      writeFileSync(outPath, JSON.stringify(entries, null, 2), 'utf8');
    }

    this.ctx.logger.info('Trajectory export complete', { path: outPath, count: entries.length });
    return outPath;
  }

  /**
   * Export a single session as JSONL string.
   */
  exportSession(sessionId: string): TrajectoryEntry[] {
    return this.queryEntries({ sessionId });
  }

  /**
   * Get aggregate statistics across all trajectories.
   */
  getStats(options: TrajectoryExportOptions = {}): TrajectoryStats {
    const entries = this.queryEntries(options);

    const stats: TrajectoryStats = {
      totalSessions: new Set(entries.map((e) => e.sessionId)).size,
      totalMessages: entries.reduce((sum, e) => sum + e.messages.length, 0),
      totalTokens: entries.reduce((sum, e) => sum + e.metadata.tokens, 0),
      totalCost: entries.reduce((sum, e) => sum + e.metadata.cost, 0),
      byAgent: {},
      byTag: {},
    };

    for (const entry of entries) {
      const agentId = entry.metadata.agentId ?? 'unknown';
      if (!stats.byAgent[agentId]) {
        stats.byAgent[agentId] = { sessions: 0, messages: 0, tokens: 0, cost: 0 };
      }
      stats.byAgent[agentId].sessions++;
      stats.byAgent[agentId].messages += entry.messages.length;
      stats.byAgent[agentId].tokens += entry.metadata.tokens;
      stats.byAgent[agentId].cost += entry.metadata.cost;

      for (const tag of entry.metadata.tags) {
        if (!stats.byTag[tag]) {
          stats.byTag[tag] = { sessions: 0, messages: 0 };
        }
        stats.byTag[tag].sessions++;
        stats.byTag[tag].messages += entry.messages.length;
      }
    }

    return stats;
  }

  // ─── Private helpers ─────────────────────────────────────────────────────

  private initDatabase(): void {
    const fullPath = this.dbPath;
    if (!existsSync(dirname(fullPath))) {
      mkdirSync(dirname(fullPath), { recursive: true });
    }

    this.db = new Database(fullPath);
    this.db.pragma('journal_mode = WAL');

    this.db.exec(`
      CREATE TABLE IF NOT EXISTS trajectories (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        timestamp TEXT NOT NULL,
        session_id TEXT NOT NULL,
        messages TEXT NOT NULL,
        metadata TEXT NOT NULL
      );

      CREATE INDEX IF NOT EXISTS idx_trajectories_session ON trajectories(session_id);
      CREATE INDEX IF NOT EXISTS idx_trajectories_timestamp ON trajectories(timestamp);
      CREATE INDEX IF NOT EXISTS idx_trajectories_agent ON trajectories(metadata);
    `);
  }

  private queryEntries(options: TrajectoryExportOptions): TrajectoryEntry[] {
    const conditions: string[] = [];
    const params: unknown[] = [];

    if (options.sessionId) {
      conditions.push('session_id = ?');
      params.push(options.sessionId);
    }

    if (options.since) {
      conditions.push('timestamp >= ?');
      params.push(options.since.toISOString());
    }

    if (options.until) {
      conditions.push('timestamp <= ?');
      params.push(options.until.toISOString());
    }

    const where = conditions.length > 0 ? `WHERE ${conditions.join(' AND ')}` : '';
    const rows = this.db
      .prepare(`SELECT * FROM trajectories ${where} ORDER BY timestamp ASC`)
      .all(...params) as Array<{
      id: number;
      timestamp: string;
      session_id: string;
      messages: string;
      metadata: string;
    }>;

    return rows
      .map((row) => {
        let entry: TrajectoryEntry;
        try {
          const messages = JSON.parse(row.messages) as TrajectoryMessage[];
          const metadata = JSON.parse(row.metadata) as TrajectoryMetadata;
          entry = {
            timestamp: row.timestamp,
            sessionId: row.session_id,
            messages,
            metadata,
          };
        } catch {
          return null;
        }

        // Filter by agentId if specified
        if (options.agentId && entry.metadata.agentId !== options.agentId) {
          return null;
        }

        // Filter by tags if specified
        if (options.tags && options.tags.length > 0) {
          const hasTag = options.tags.some((t) => entry.metadata.tags.includes(t));
          if (!hasTag) return null;
        }

        return entry;
      })
      .filter((e): e is TrajectoryEntry => e !== null);
  }
}

export default new TrajectoryExportFeature();
