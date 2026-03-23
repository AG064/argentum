/**
 * SQLite Memory Feature
 *
 * Namespace-aware key-value store using SQLite with FTS5 full-text search.
 * Supports multiple namespaces: conversations, facts, preferences, tasks.
 */

import Database from 'better-sqlite3';
import { mkdirSync, existsSync } from 'fs';
import { dirname, resolve } from 'path';
import { FeatureModule, FeatureContext, FeatureMeta, HealthStatus } from '../../core/plugin-loader';

/** Memory entry */
export interface MemoryEntry {
  namespace: string;
  key: string;
  value: string;
  created_at: number;
  updated_at: number;
}

/** Feature configuration */
export interface SQLiteMemoryConfig {
  dbPath?: string;
  namespaces?: string[];
}

/**
 * SQLiteMemory — persistent namespace key-value store.
 *
 * Provides memory storage with full-text search across values.
 * Supports isolated namespaces for different memory types.
 */
class SQLiteMemoryFeature implements FeatureModule {
  readonly meta: FeatureMeta = {
    name: 'sqlite-memory',
    version: '0.1.0',
    description: 'SQLite-backed namespace key-value memory store with full-text search',
    dependencies: [],
  };

  private config: Required<SQLiteMemoryConfig>;
  private ctx!: FeatureContext;
  private db!: Database.Database;
  private defaultNamespaces = ['conversations', 'facts', 'preferences', 'tasks'];

  constructor() {
    this.config = {
      dbPath: './data/sqlite-memory.db',
      namespaces: this.defaultNamespaces,
    };
  }

  async init(config: Record<string, unknown>, context: FeatureContext): Promise<void> {
    this.ctx = context;
    this.config = {
      dbPath: (config['dbPath'] as string) ?? this.config['dbPath'],
      namespaces: (config['namespaces'] as string[]) ?? this.defaultNamespaces,
    };

    this.initDatabase();
  }

  async start(): Promise<void> {
    this.ctx.logger.info('SQLiteMemory active', {
      dbPath: this.config.dbPath,
      namespaces: this.config.namespaces,
    });
  }

  async stop(): Promise<void> {
    if (this.db) {
      this.db.close();
      this.ctx.logger.info('SQLiteMemory stopped');
    }
  }

  async healthCheck(): Promise<HealthStatus> {
    try {
      const count = (this.db.prepare('SELECT COUNT(*) as c FROM kv_store').get() as { c: number }).c;
      const namespaceCounts: Record<string, number> = {};
      for (const ns of this.config.namespaces) {
        const row = this.db.prepare('SELECT COUNT(*) as c FROM kv_store WHERE namespace = ?').get(ns) as { c: number };
        namespaceCounts[ns] = row.c;
      }
      return {
        healthy: true,
        details: {
          totalEntries: count,
          namespaces: namespaceCounts,
        },
      };
    } catch (err) {
      return {
        healthy: false,
        message: err instanceof Error ? err.message : String(err),
      };
    }
  }

  /** Store a value under a namespace and key */
  store(namespace: string, key: string, value: string): void {
    if (!this.config.namespaces.includes(namespace)) {
      throw new Error(`Invalid namespace: ${namespace}. Allowed: ${this.config.namespaces.join(', ')}`);
    }

    const now = Date.now();
    const stmt = this.db.prepare(`
      INSERT INTO kv_store (namespace, key, value, created_at, updated_at)
      VALUES (@namespace, @key, @value, @now, @now)
      ON CONFLICT(namespace, key) DO UPDATE SET
        value = @value,
        updated_at = @now
    `);
    stmt.run({ namespace, key, value, now });
  }

  /** Get a value by namespace and key */
  get(namespace: string, key: string): string | null {
    const row = this.db.prepare(
      'SELECT value FROM kv_store WHERE namespace = ? AND key = ?'
    ).get(namespace, key) as { value: string } | undefined;

    return row?.value ?? null;
  }

  /** Search values by full-text query across all namespaces */
  search(query: string, limit = 20): Array<{ namespace: string; key: string; value: string; rank: number }> {
    if (!query.trim()) return [];

    try {
      const sanitized = query.replace(/[^\w\s]/g, ' ').trim();
      const rows = this.db.prepare(`
        SELECT k.namespace, k.key, k.value, fts.rank
        FROM kv_store k
        JOIN kv_fts fts ON k.rowid = fts.rowid
        WHERE kv_fts MATCH ?
        ORDER BY rank
        LIMIT ?
      `).all(sanitized, limit) as Array<{ namespace: string; key: string; value: string; rank: number }>;

      return rows;
    } catch {
      // FTS5 syntax error — fallback to LIKE
      return this.searchByLike(query, limit);
    }
  }

  /** Delete a specific key in a namespace */
  delete(namespace: string, key: string): boolean {
    const result = this.db.prepare(
      'DELETE FROM kv_store WHERE namespace = ? AND key = ?'
    ).run(namespace, key);
    return result.changes > 0;
  }

  /** List all keys in a namespace */
  list(namespace: string): string[] {
    if (!this.config.namespaces.includes(namespace)) {
      throw new Error(`Invalid namespace: ${namespace}`);
    }

    const rows = this.db.prepare(
      'SELECT key FROM kv_store WHERE namespace = ? ORDER BY updated_at DESC'
    ).all(namespace) as Array<{ key: string }>;

    return rows.map(r => r.key);
  }

  /** Get all entries in a namespace */
  getAll(namespace: string): MemoryEntry[] {
    if (!this.config.namespaces.includes(namespace)) {
      throw new Error(`Invalid namespace: ${namespace}`);
    }

    const rows = this.db.prepare(
      'SELECT namespace, key, value, created_at, updated_at FROM kv_store WHERE namespace = ? ORDER BY updated_at DESC'
    ).all(namespace) as MemoryEntry[];

    return rows;
  }

  /** Clear a namespace (delete all keys) */
  clear(namespace: string): void {
    if (!this.config.namespaces.includes(namespace)) {
      throw new Error(`Invalid namespace: ${namespace}`);
    }
    this.db.prepare('DELETE FROM kv_store WHERE namespace = ?').run(namespace);
  }

  /** Initialize database and create tables */
  private initDatabase(): void {
    const fullPath = resolve(this.config.dbPath);
    if (!existsSync(dirname(fullPath))) {
      mkdirSync(dirname(fullPath), { recursive: true });
    }

    this.db = new Database(fullPath);
    this.db.pragma('journal_mode = WAL');
    this.db.pragma('synchronous = NORMAL');
    this.db.pragma('foreign_keys = ON');

    this.db.exec(`
      CREATE TABLE IF NOT EXISTS kv_store (
        namespace TEXT NOT NULL,
        key TEXT NOT NULL,
        value TEXT NOT NULL,
        created_at INTEGER NOT NULL,
        updated_at INTEGER NOT NULL,
        PRIMARY KEY (namespace, key)
      );

      CREATE INDEX IF NOT EXISTS idx_kv_namespace ON kv_store(namespace);
      CREATE INDEX IF NOT EXISTS idx_kv_updated ON kv_store(updated_at DESC);

      CREATE VIRTUAL TABLE IF NOT EXISTS kv_fts USING fts5(
        value,
        content='kv_store',
        content_rowid='rowid'
      );

      CREATE TRIGGER IF NOT EXISTS kv_ai AFTER INSERT ON kv_store BEGIN
        INSERT INTO kv_fts(rowid, value) VALUES (new.rowid, new.value);
      END;

      CREATE TRIGGER IF NOT EXISTS kv_ad AFTER DELETE ON kv_store BEGIN
        INSERT INTO kv_fts(kv_fts, rowid, value) VALUES ('delete', old.rowid, old.value);
      END;

      CREATE TRIGGER IF NOT EXISTS kv_au AFTER UPDATE ON kv_store BEGIN
        INSERT INTO kv_fts(kv_fts, rowid, value) VALUES ('delete', old.rowid, old.value);
        INSERT INTO kv_fts(rowid, value) VALUES (new.rowid, new.value);
      END;
    `);
  }

  /** Fallback LIKE search */
  private searchByLike(query: string, limit: number): Array<{ namespace: string; key: string; value: string; rank: number }> {
    const pattern = `%${query}%`;
    const rows = this.db.prepare(
      `SELECT namespace, key, value FROM kv_store
       WHERE value LIKE ?
       ORDER BY updated_at DESC
       LIMIT ?`
    ).all(pattern, limit) as Array<{ namespace: string; key: string; value: string }>;

    return rows.map(r => ({ ...r, rank: 0 }));
  }
}

export default new SQLiteMemoryFeature();
