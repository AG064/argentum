/**
 * SQLite Memory Backend
 *
 * Persistent memory storage using SQLite with WAL mode.
 * Supports keyword search, access tracking, and metadata.
 */

import { randomUUID } from 'crypto';
import { mkdirSync, existsSync } from 'fs';
import { dirname, resolve } from 'path';

import Database from 'better-sqlite3';

/** Memory entry stored in SQLite */
export interface SQLiteMemoryEntry {
  id: string;
  content: string;
  embedding: Buffer | null;
  created_at: number;
  accessed_at: number;
  access_count: number;
  metadata: string;
}

/** Store options */
export interface StoreOptions {
  embedding?: Buffer;
  metadata?: Record<string, unknown>;
}

/** Search options */
export interface SearchOptions {
  limit?: number;
  offset?: number;
}

/**
 * SQLite-backed memory store.
 *
 * Uses WAL journal mode for better concurrency and supports
 * keyword-based full-text search via FTS5.
 */
export class SQLiteMemory {
  private db!: Database.Database;
  private initialized = false;

  constructor(private dbPath: string = './data/memory.db') {}

  /** Initialize the database and create tables */
  init(): void {
    if (this.initialized) return;

    const fullPath = resolve(this.dbPath);
    if (!existsSync(dirname(fullPath))) {
      mkdirSync(dirname(fullPath), { recursive: true });
    }

    this.db = new Database(fullPath);
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
  store(content: string, options: StoreOptions = {}): SQLiteMemoryEntry {
    this.ensureInit();

    const id = randomUUID();
    const now = Date.now();
    const entry: SQLiteMemoryEntry = {
      id,
      content,
      embedding: options.embedding ?? null,
      created_at: now,
      accessed_at: now,
      access_count: 0,
      metadata: JSON.stringify(options.metadata ?? {}),
    };

    this.db.prepare(
      `INSERT INTO memories (id, content, embedding, created_at, accessed_at, access_count, metadata)
       VALUES (@id, @content, @embedding, @created_at, @accessed_at, @access_count, @metadata)`
    ).run(entry);

    return entry;
  }

  /** Search memories by keyword using FTS5 */
  search(query: string, limit = 20): SQLiteMemoryEntry[] {
    this.ensureInit();

    // Sanitize query for FTS5 (remove special characters)
    const sanitized = query.replace(/[^\w\s]/g, ' ').trim();
    if (!sanitized) return [];

    try {
      const rows = this.db.prepare(
        `SELECT m.* FROM memories m
         JOIN memories_fts fts ON m.rowid = fts.rowid
         WHERE memories_fts MATCH ?
         ORDER BY rank
         LIMIT ?`
      ).all(sanitized, limit) as SQLiteMemoryEntry[];

      // Update access stats
      const updateStmt = this.db.prepare(
        'UPDATE memories SET accessed_at = ?, access_count = access_count + 1 WHERE id = ?'
      );
      const now = Date.now();
      for (const row of rows) {
        updateStmt.run(now, row.id);
      }

      return rows;
    } catch {
      // FTS5 syntax error — fall back to LIKE search
      return this.searchByLike(query, limit);
    }
  }

  /** Fallback LIKE-based search */
  private searchByLike(query: string, limit: number): SQLiteMemoryEntry[] {
    const rows = this.db.prepare(
      `SELECT * FROM memories
       WHERE content LIKE ?
       ORDER BY created_at DESC
       LIMIT ?`
    ).all(`%${query}%`, limit) as SQLiteMemoryEntry[];

    const updateStmt = this.db.prepare(
      'UPDATE memories SET accessed_at = ?, access_count = access_count + 1 WHERE id = ?'
    );
    const now = Date.now();
    for (const row of rows) {
      updateStmt.run(now, row.id);
    }

    return rows;
  }

  /** Get recent memories */
  getRecent(limit = 20): SQLiteMemoryEntry[] {
    this.ensureInit();

    return this.db.prepare(
      'SELECT * FROM memories ORDER BY created_at DESC LIMIT ?'
    ).all(limit) as SQLiteMemoryEntry[];
  }

  /** Get a specific memory by ID */
  getById(id: string): SQLiteMemoryEntry | null {
    this.ensureInit();

    const row = this.db.prepare('SELECT * FROM memories WHERE id = ?').get(id) as SQLiteMemoryEntry | undefined;
    if (row) {
      this.db.prepare(
        'UPDATE memories SET accessed_at = ?, access_count = access_count + 1 WHERE id = ?'
      ).run(Date.now(), row.id);
    }
    return row ?? null;
  }

  /** Delete a memory by ID */
  delete(id: string): boolean {
    this.ensureInit();

    const result = this.db.prepare('DELETE FROM memories WHERE id = ?').run(id);
    return result.changes > 0;
  }

  /** Get total count of memories */
  count(): number {
    this.ensureInit();

    const row = this.db.prepare('SELECT COUNT(*) as count FROM memories').get() as { count: number };
    return row.count;
  }

  /** Clear all memories */
  clear(): void {
    this.ensureInit();

    this.db.exec('DELETE FROM memories;');
    // FTS is auto-cleaned by triggers
  }

  /** Close the database connection */
  close(): void {
    if (this.db) {
      this.db.close();
      this.initialized = false;
    }
  }

  /** Ensure database is initialized */
  private ensureInit(): void {
    if (!this.initialized) {
      this.init();
    }
  }
}

// Singleton instance
let instance: SQLiteMemory | null = null;

/** Get or create the global memory instance */
export function getSQLiteMemory(dbPath?: string): SQLiteMemory {
  if (!instance) {
    instance = new SQLiteMemory(dbPath);
    instance.init();
  }
  return instance;
}
