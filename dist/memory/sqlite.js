"use strict";
/**
 * SQLite Memory Backend
 *
 * Persistent memory storage using SQLite with WAL mode.
 * Supports keyword search, access tracking, and metadata.
 */
var __importDefault = (this && this.__importDefault) || function (mod) {
    return (mod && mod.__esModule) ? mod : { "default": mod };
};
Object.defineProperty(exports, "__esModule", { value: true });
exports.SQLiteMemory = void 0;
exports.getSQLiteMemory = getSQLiteMemory;
const crypto_1 = require("crypto");
const fs_1 = require("fs");
const path_1 = require("path");
const better_sqlite3_1 = __importDefault(require("better-sqlite3"));
/**
 * SQLite-backed memory store.
 *
 * Uses WAL journal mode for better concurrency and supports
 * keyword-based full-text search via FTS5.
 */
class SQLiteMemory {
    dbPath;
    db;
    initialized = false;
    constructor(dbPath = './data/memory.db') {
        this.dbPath = dbPath;
    }
    /** Initialize the database and create tables */
    init() {
        if (this.initialized)
            return;
        const fullPath = (0, path_1.resolve)(this.dbPath);
        if (!(0, fs_1.existsSync)((0, path_1.dirname)(fullPath))) {
            (0, fs_1.mkdirSync)((0, path_1.dirname)(fullPath), { recursive: true });
        }
        this.db = new better_sqlite3_1.default(fullPath);
        this.db.pragma('journal_mode = WAL');
        this.db.pragma('synchronous = NORMAL');
        this.db.pragma('foreign_keys = ON');
        this.db.exec(`
      CREATE TABLE IF NOT EXISTS memories (
        id TEXT PRIMARY KEY,
        content TEXT NOT NULL,
        embedding BLOB,
        created_at INTEGER NOT NULL,
        accessed_at INTEGER NOT NULL,
        access_count INTEGER DEFAULT 0,
        metadata TEXT DEFAULT '{}'
      );

      CREATE INDEX IF NOT EXISTS idx_memories_created ON memories(created_at DESC);
      CREATE INDEX IF NOT EXISTS idx_memories_accessed ON memories(accessed_at DESC);
      CREATE INDEX IF NOT EXISTS idx_memories_access_count ON memories(access_count DESC);

      -- FTS5 full-text search table
      CREATE VIRTUAL TABLE IF NOT EXISTS memories_fts USING fts5(
        content,
        content='memories',
        content_rowid='rowid'
      );

      -- Triggers to keep FTS in sync
      CREATE TRIGGER IF NOT EXISTS memories_ai AFTER INSERT ON memories BEGIN
        INSERT INTO memories_fts(rowid, content)
        VALUES (new.rowid, new.content);
      END;

      CREATE TRIGGER IF NOT EXISTS memories_ad AFTER DELETE ON memories BEGIN
        INSERT INTO memories_fts(memories_fts, rowid, content)
        VALUES ('delete', old.rowid, old.content);
      END;

      CREATE TRIGGER IF NOT EXISTS memories_au AFTER UPDATE ON memories BEGIN
        INSERT INTO memories_fts(memories_fts, rowid, content)
        VALUES ('delete', old.rowid, old.content);
        INSERT INTO memories_fts(rowid, content)
        VALUES (new.rowid, new.content);
      END;
    `);
        this.initialized = true;
    }
    /** Store a memory entry */
    store(content, options = {}) {
        this.ensureInit();
        const id = (0, crypto_1.randomUUID)();
        const now = Date.now();
        const entry = {
            id,
            content,
            embedding: options.embedding ?? null,
            created_at: now,
            accessed_at: now,
            access_count: 0,
            metadata: JSON.stringify(options.metadata ?? {}),
        };
        this.db.prepare(`INSERT INTO memories (id, content, embedding, created_at, accessed_at, access_count, metadata)
       VALUES (@id, @content, @embedding, @created_at, @accessed_at, @access_count, @metadata)`).run(entry);
        return entry;
    }
    /** Search memories by keyword using FTS5 */
    search(query, limit = 20) {
        this.ensureInit();
        // Sanitize query for FTS5 (remove special characters)
        const sanitized = query.replace(/[^\w\s]/g, ' ').trim();
        if (!sanitized)
            return [];
        try {
            const rows = this.db.prepare(`SELECT m.* FROM memories m
         JOIN memories_fts fts ON m.rowid = fts.rowid
         WHERE memories_fts MATCH ?
         ORDER BY rank
         LIMIT ?`).all(sanitized, limit);
            // Update access stats
            const updateStmt = this.db.prepare('UPDATE memories SET accessed_at = ?, access_count = access_count + 1 WHERE id = ?');
            const now = Date.now();
            for (const row of rows) {
                updateStmt.run(now, row.id);
            }
            return rows;
        }
        catch {
            // FTS5 syntax error — fall back to LIKE search
            return this.searchByLike(query, limit);
        }
    }
    /** Fallback LIKE-based search */
    searchByLike(query, limit) {
        const rows = this.db.prepare(`SELECT * FROM memories
       WHERE content LIKE ?
       ORDER BY created_at DESC
       LIMIT ?`).all(`%${query}%`, limit);
        const updateStmt = this.db.prepare('UPDATE memories SET accessed_at = ?, access_count = access_count + 1 WHERE id = ?');
        const now = Date.now();
        for (const row of rows) {
            updateStmt.run(now, row.id);
        }
        return rows;
    }
    /** Get recent memories */
    getRecent(limit = 20) {
        this.ensureInit();
        return this.db.prepare('SELECT * FROM memories ORDER BY created_at DESC LIMIT ?').all(limit);
    }
    /** Get a specific memory by ID */
    getById(id) {
        this.ensureInit();
        const row = this.db.prepare('SELECT * FROM memories WHERE id = ?').get(id);
        if (row) {
            this.db.prepare('UPDATE memories SET accessed_at = ?, access_count = access_count + 1 WHERE id = ?').run(Date.now(), row.id);
        }
        return row ?? null;
    }
    /** Delete a memory by ID */
    delete(id) {
        this.ensureInit();
        const result = this.db.prepare('DELETE FROM memories WHERE id = ?').run(id);
        return result.changes > 0;
    }
    /** Get total count of memories */
    count() {
        this.ensureInit();
        const row = this.db.prepare('SELECT COUNT(*) as count FROM memories').get();
        return row.count;
    }
    /** Clear all memories */
    clear() {
        this.ensureInit();
        this.db.exec('DELETE FROM memories;');
        // FTS is auto-cleaned by triggers
    }
    /** Close the database connection */
    close() {
        if (this.db) {
            this.db.close();
            this.initialized = false;
        }
    }
    /** Ensure database is initialized */
    ensureInit() {
        if (!this.initialized) {
            this.init();
        }
    }
}
exports.SQLiteMemory = SQLiteMemory;
// Singleton instance
let instance = null;
/** Get or create the global memory instance */
function getSQLiteMemory(dbPath) {
    if (!instance) {
        instance = new SQLiteMemory(dbPath);
        instance.init();
    }
    return instance;
}
//# sourceMappingURL=sqlite.js.map