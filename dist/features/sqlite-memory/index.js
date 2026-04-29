"use strict";
/**
 * SQLite Memory Feature
 *
 * Namespace-aware key-value store using SQLite with FTS5 full-text search.
 * Supports multiple namespaces: conversations, facts, preferences, tasks.
 */
var __importDefault = (this && this.__importDefault) || function (mod) {
    return (mod && mod.__esModule) ? mod : { "default": mod };
};
Object.defineProperty(exports, "__esModule", { value: true });
const fs_1 = require("fs");
const path_1 = require("path");
const better_sqlite3_1 = __importDefault(require("better-sqlite3"));
/**
 * SQLiteMemory — persistent namespace key-value store.
 *
 * Provides memory storage with full-text search across values.
 * Supports isolated namespaces for different memory types.
 */
class SQLiteMemoryFeature {
    meta = {
        name: 'sqlite-memory',
        version: '0.0.3',
        description: 'SQLite-backed namespace key-value memory store with full-text search',
        dependencies: [],
    };
    config;
    ctx;
    db;
    defaultNamespaces = ['conversations', 'facts', 'preferences', 'tasks'];
    constructor() {
        this.config = {
            dbPath: './data/sqlite-memory.db',
            namespaces: this.defaultNamespaces,
        };
    }
    async init(config, context) {
        this.ctx = context;
        this.config = {
            dbPath: config['dbPath'] ?? this.config['dbPath'],
            namespaces: config['namespaces'] ?? this.defaultNamespaces,
        };
        this.initDatabase();
    }
    async start() {
        this.ctx.logger.info('SQLiteMemory active', {
            dbPath: this.config.dbPath,
            namespaces: this.config.namespaces,
        });
    }
    async stop() {
        if (this.db) {
            this.db.close();
            this.ctx.logger.info('SQLiteMemory stopped');
        }
    }
    async healthCheck() {
        try {
            const count = this.db.prepare('SELECT COUNT(*) as c FROM kv_store').get()
                .c;
            const namespaceCounts = {};
            for (const ns of this.config.namespaces) {
                const row = this.db
                    .prepare('SELECT COUNT(*) as c FROM kv_store WHERE namespace = ?')
                    .get(ns);
                namespaceCounts[ns] = row.c;
            }
            return {
                healthy: true,
                details: {
                    totalEntries: count,
                    namespaces: namespaceCounts,
                },
            };
        }
        catch (err) {
            return {
                healthy: false,
                message: err instanceof Error ? err.message : String(err),
            };
        }
    }
    /** Store a value under a namespace and key */
    store(namespace, key, value) {
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
    get(namespace, key) {
        const row = this.db
            .prepare('SELECT value FROM kv_store WHERE namespace = ? AND key = ?')
            .get(namespace, key);
        return row?.value ?? null;
    }
    /** Search values by full-text query across all namespaces */
    search(query, limit = 20) {
        if (!query.trim())
            return [];
        try {
            const sanitized = query.replace(/[^\w\s]/g, ' ').trim();
            const rows = this.db
                .prepare(`
        SELECT k.namespace, k.key, k.value, fts.rank
        FROM kv_store k
        JOIN kv_fts fts ON k.rowid = fts.rowid
        WHERE kv_fts MATCH ?
        ORDER BY rank
        LIMIT ?
      `)
                .all(sanitized, limit);
            return rows;
        }
        catch {
            // FTS5 syntax error — fallback to LIKE
            return this.searchByLike(query, limit);
        }
    }
    /** Delete a specific key in a namespace */
    delete(namespace, key) {
        const result = this.db
            .prepare('DELETE FROM kv_store WHERE namespace = ? AND key = ?')
            .run(namespace, key);
        return result.changes > 0;
    }
    /** List all keys in a namespace */
    list(namespace) {
        if (!this.config.namespaces.includes(namespace)) {
            throw new Error(`Invalid namespace: ${namespace}`);
        }
        const rows = this.db
            .prepare('SELECT key FROM kv_store WHERE namespace = ? ORDER BY updated_at DESC')
            .all(namespace);
        return rows.map((r) => r.key);
    }
    /** Get all entries in a namespace */
    getAll(namespace) {
        if (!this.config.namespaces.includes(namespace)) {
            throw new Error(`Invalid namespace: ${namespace}`);
        }
        const rows = this.db
            .prepare('SELECT namespace, key, value, created_at, updated_at FROM kv_store WHERE namespace = ? ORDER BY updated_at DESC')
            .all(namespace);
        return rows;
    }
    /** Clear a namespace (delete all keys) */
    clear(namespace) {
        if (!this.config.namespaces.includes(namespace)) {
            throw new Error(`Invalid namespace: ${namespace}`);
        }
        this.db.prepare('DELETE FROM kv_store WHERE namespace = ?').run(namespace);
    }
    /** Initialize database and create tables */
    initDatabase() {
        const fullPath = (0, path_1.resolve)(this.config.dbPath);
        if (!(0, fs_1.existsSync)((0, path_1.dirname)(fullPath))) {
            (0, fs_1.mkdirSync)((0, path_1.dirname)(fullPath), { recursive: true });
        }
        this.db = new better_sqlite3_1.default(fullPath);
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
    searchByLike(query, limit) {
        const pattern = `%${query}%`;
        const rows = this.db
            .prepare(`SELECT namespace, key, value FROM kv_store
       WHERE value LIKE ?
       ORDER BY updated_at DESC
       LIMIT ?`)
            .all(pattern, limit);
        return rows.map((r) => ({ ...r, rank: 0 }));
    }
}
exports.default = new SQLiteMemoryFeature();
//# sourceMappingURL=index.js.map