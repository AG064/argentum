"use strict";
/**
 * Semantic Memory Module
 *
 * OMEGA Memory integration — semantic search using embeddings.
 * Uses SQLite with FTS5 for full-text search and stores embeddings
 * for semantic similarity matching.
 */
var __importDefault = (this && this.__importDefault) || function (mod) {
    return (mod && mod.__esModule) ? mod : { "default": mod };
};
Object.defineProperty(exports, "__esModule", { value: true });
exports.SemanticMemory = void 0;
exports.getSemanticMemory = getSemanticMemory;
const crypto_1 = require("crypto");
const fs_1 = require("fs");
const path_1 = require("path");
const better_sqlite3_1 = __importDefault(require("better-sqlite3"));
/**
 * Semantic Memory — stores memories with embeddings for semantic search.
 *
 * Provides full-text search via FTS5 and semantic similarity via
 * vector embeddings (bge-small ONNX model, 384-dim).
 */
class SemanticMemory {
    dbPath;
    db;
    initialized = false;
    constructor(dbPath = './data/semantic-memory.db') {
        this.dbPath = dbPath;
    }
    /** Initialize database with schema */
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
      -- Core memories table
      CREATE TABLE IF NOT EXISTS memories (
        id TEXT PRIMARY KEY,
        type TEXT NOT NULL DEFAULT 'general',
        content TEXT NOT NULL,
        content_hash TEXT,
        embedding BLOB,
        created_at INTEGER NOT NULL,
        accessed_at INTEGER NOT NULL,
        access_count INTEGER DEFAULT 0,
        weight REAL DEFAULT 1.0,
        metadata TEXT DEFAULT '{}'
      );

      CREATE INDEX IF NOT EXISTS idx_mem_type ON memories(type);
      CREATE INDEX IF NOT EXISTS idx_mem_created ON memories(created_at DESC);
      CREATE INDEX IF NOT EXISTS idx_mem_accessed ON memories(accessed_at DESC);
      CREATE INDEX IF NOT EXISTS idx_mem_weight ON memories(weight DESC);
      CREATE INDEX IF NOT EXISTS idx_mem_hash ON memories(content_hash);

      -- Edges table for graph connections
      CREATE TABLE IF NOT EXISTS edges (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        source_id TEXT NOT NULL,
        target_id TEXT NOT NULL,
        relation_type TEXT NOT NULL DEFAULT 'related',
        weight REAL DEFAULT 1.0,
        created_at INTEGER NOT NULL,
        FOREIGN KEY (source_id) REFERENCES memories(id) ON DELETE CASCADE,
        FOREIGN KEY (target_id) REFERENCES memories(id) ON DELETE CASCADE
      );

      CREATE INDEX IF NOT EXISTS idx_edges_source ON edges(source_id);
      CREATE INDEX IF NOT EXISTS idx_edges_target ON edges(target_id);
      CREATE INDEX IF NOT EXISTS idx_edges_relation ON edges(relation_type);

      -- FTS5 full-text search
      CREATE VIRTUAL TABLE IF NOT EXISTS memories_fts USING fts5(
        content,
        type,
        content='memories',
        content_rowid='rowid'
      );

      -- FTS sync triggers
      CREATE TRIGGER IF NOT EXISTS memories_ai AFTER INSERT ON memories BEGIN
        INSERT INTO memories_fts(rowid, content, type)
        VALUES (new.rowid, new.content, new.type);
      END;

      CREATE TRIGGER IF NOT EXISTS memories_ad AFTER DELETE ON memories BEGIN
        INSERT INTO memories_fts(memories_fts, rowid, content, type)
        VALUES ('delete', old.rowid, old.content, old.type);
      END;

      CREATE TRIGGER IF NOT EXISTS memories_au AFTER UPDATE ON memories BEGIN
        INSERT INTO memories_fts(memories_fts, rowid, content, type)
        VALUES ('delete', old.rowid, old.content, old.type);
        INSERT INTO memories_fts(rowid, content, type)
        VALUES (new.rowid, new.content, new.type);
      END;

      -- Checkpoints table
      CREATE TABLE IF NOT EXISTS checkpoints (
        task_id TEXT PRIMARY KEY,
        state TEXT NOT NULL,
        context TEXT DEFAULT '{}',
        created_at INTEGER NOT NULL,
        updated_at INTEGER NOT NULL
      );
    `);
        this.initialized = true;
    }
    /** Store a memory entry */
    async store(type, content, metadata) {
        this.ensureInit();
        const id = (0, crypto_1.randomUUID)();
        const now = Date.now();
        const contentHash = this.hashContent(content);
        // Check for duplicates
        const existing = this.db.prepare('SELECT id FROM memories WHERE content_hash = ? LIMIT 1').get(contentHash);
        if (existing) {
            // Update access count on existing
            this.db.prepare('UPDATE memories SET accessed_at = ?, access_count = access_count + 1 WHERE id = ?').run(now, existing.id);
            return existing.id;
        }
        // Generate embedding (stub — TODO: ONNX bge-small model)
        const embedding = await this.generateEmbedding(content);
        this.db.prepare(`INSERT INTO memories (id, type, content, content_hash, embedding, created_at, accessed_at, access_count, weight, metadata)
       VALUES (@id, @type, @content, @content_hash, @embedding, @created_at, @accessed_at, 0, 1.0, @metadata)`).run({
            id,
            type,
            content,
            content_hash: contentHash,
            embedding: embedding ? Buffer.from(embedding.buffer) : null,
            created_at: now,
            accessed_at: now,
            metadata: JSON.stringify(metadata ?? {}),
        });
        return id;
    }
    /** Search memories by keyword (FTS5) and semantic similarity */
    async search(query, limit = 10) {
        this.ensureInit();
        // Full-text search
        const sanitized = query.replace(/[^\w\s]/g, ' ').trim();
        let results = [];
        if (sanitized) {
            try {
                const rows = this.db.prepare(`SELECT m.*, rank FROM memories m
           JOIN memories_fts fts ON m.rowid = fts.rowid
           WHERE memories_fts MATCH ?
           ORDER BY rank
           LIMIT ?`).all(sanitized, limit);
                results = rows.map(r => this.rowToResult(r));
            }
            catch {
                // FTS syntax error — fallback to LIKE
                results = this.searchByLike(query, limit);
            }
        }
        // Semantic similarity boost (if we have embeddings)
        if (results.length > 0) {
            const queryEmbedding = await this.generateEmbedding(query);
            if (queryEmbedding) {
                for (const result of results) {
                    if (result.embedding) {
                        const memEmbedding = new Float32Array(result.embedding.buffer);
                        result.similarity = this.cosineSimilarity(queryEmbedding, memEmbedding);
                    }
                }
                results.sort((a, b) => (b.similarity ?? 0) - (a.similarity ?? 0));
            }
        }
        // Update access stats
        const updateStmt = this.db.prepare('UPDATE memories SET accessed_at = ?, access_count = access_count + 1 WHERE id = ?');
        const now = Date.now();
        for (const r of results) {
            updateStmt.run(now, r.id);
        }
        return results;
    }
    /** Get recent memories from last N hours */
    async getRecent(hours) {
        this.ensureInit();
        const cutoff = Date.now() - (hours * 60 * 60 * 1000);
        const rows = this.db.prepare('SELECT * FROM memories WHERE created_at >= ? ORDER BY created_at DESC').all(cutoff);
        return rows.map(r => this.rowToResult(r));
    }
    /** Consolidate memories — deduplicate and decay weights */
    async consolidate() {
        this.ensureInit();
        // Decay weights for old, rarely accessed memories
        const now = Date.now();
        const dayMs = 86400000;
        this.db.prepare(`
      UPDATE memories SET weight = weight * 0.95
      WHERE accessed_at < ? AND access_count < 3
    `).run(now - (7 * dayMs));
        // Remove entries with very low weight
        this.db.prepare('DELETE FROM memories WHERE weight < 0.1 AND access_count = 0').run();
        // Deduplicate by content hash
        this.db.prepare(`
      DELETE FROM memories WHERE id NOT IN (
        SELECT MIN(id) FROM memories GROUP BY content_hash
      ) AND content_hash IS NOT NULL
    `).run();
    }
    /** Save checkpoint state for a task */
    async checkpoint(taskId, state) {
        this.ensureInit();
        const now = Date.now();
        this.db.prepare(`
      INSERT INTO checkpoints (task_id, state, created_at, updated_at)
      VALUES (@task_id, @state, @created_at, @updated_at)
      ON CONFLICT(task_id) DO UPDATE SET
        state = @state,
        updated_at = @updated_at
    `).run({
            task_id: taskId,
            state: JSON.stringify(state),
            created_at: now,
            updated_at: now,
        });
    }
    /** Resume a checkpointed task */
    async resume(taskId) {
        this.ensureInit();
        const row = this.db.prepare('SELECT state FROM checkpoints WHERE task_id = ?').get(taskId);
        return row ? JSON.parse(row.state) : null;
    }
    /** Get a memory by ID */
    async getById(id) {
        this.ensureInit();
        const row = this.db.prepare('SELECT * FROM memories WHERE id = ?').get(id);
        if (row) {
            this.db.prepare('UPDATE memories SET accessed_at = ?, access_count = access_count + 1 WHERE id = ?').run(Date.now(), id);
            return this.rowToResult(row);
        }
        return null;
    }
    /** Delete a memory */
    async delete(id) {
        this.ensureInit();
        const result = this.db.prepare('DELETE FROM memories WHERE id = ?').run(id);
        return result.changes > 0;
    }
    /** Get total memory count */
    async count() {
        this.ensureInit();
        const row = this.db.prepare('SELECT COUNT(*) as count FROM memories').get();
        return row.count;
    }
    /** Get memories by type */
    async getByType(type, limit = 50) {
        this.ensureInit();
        const rows = this.db.prepare('SELECT * FROM memories WHERE type = ? ORDER BY created_at DESC LIMIT ?').all(type, limit);
        return rows.map(r => this.rowToResult(r));
    }
    /** Close database */
    close() {
        if (this.db) {
            this.db.close();
            this.initialized = false;
        }
    }
    /** Get raw DB instance (for graph module) */
    getDb() {
        this.ensureInit();
        return this.db;
    }
    /** Ensure initialized */
    ensureInit() {
        if (!this.initialized) {
            this.init();
        }
    }
    /** Fallback LIKE search */
    searchByLike(query, limit) {
        const rows = this.db.prepare(`SELECT * FROM memories WHERE content LIKE ? ORDER BY created_at DESC LIMIT ?`).all(`%${query}%`, limit);
        return rows.map(r => this.rowToResult(r));
    }
    /** Generate embedding for text — stub for ONNX bge-small model */
    async generateEmbedding(text) {
        // TODO: Load ONNX model (bge-small-en-v1.5, 384-dim) via onnxruntime-node
        // const session = await InferenceSession.create('./models/bge-small.onnx');
        // const tokens = tokenize(text);
        // const result = await session.run({ input_ids: tokens });
        // return result.last_hidden_state as Float32Array;
        // Stub: simple hash-based pseudo-embedding for testing
        // Real implementation will use the ONNX model
        const dim = 32; // Reduced for stub
        const embedding = new Float32Array(dim);
        const hash = (0, crypto_1.createHash)('sha256').update(text).digest();
        for (let i = 0; i < dim; i++) {
            embedding[i] = (hash[i % hash.length] / 255.0) * 2 - 1;
        }
        // Normalize
        const norm = Math.sqrt(embedding.reduce((s, v) => s + v * v, 0));
        if (norm > 0) {
            for (let i = 0; i < dim; i++) {
                embedding[i] = embedding[i] / norm;
            }
        }
        return embedding;
    }
    /** Cosine similarity between two vectors */
    cosineSimilarity(a, b) {
        const len = Math.min(a.length, b.length);
        let dot = 0, normA = 0, normB = 0;
        for (let i = 0; i < len; i++) {
            dot += a[i] * b[i];
            normA += a[i] * a[i];
            normB += b[i] * b[i];
        }
        const denom = Math.sqrt(normA) * Math.sqrt(normB);
        return denom > 0 ? dot / denom : 0;
    }
    /** SHA256 hash of content */
    hashContent(content) {
        return (0, crypto_1.createHash)('sha256').update(content.trim().toLowerCase()).digest('hex');
    }
    /** Convert DB row to MemoryResult */
    rowToResult(row) {
        return {
            id: row['id'],
            type: row['type'],
            content: row['content'],
            embedding: row['embedding'],
            created_at: row['created_at'],
            accessed_at: row['accessed_at'],
            access_count: row['access_count'],
            metadata: this.parseJson(row['metadata']),
        };
    }
    /** Safe JSON parse */
    parseJson(str) {
        try {
            return JSON.parse(str);
        }
        catch {
            return {};
        }
    }
}
exports.SemanticMemory = SemanticMemory;
// Singleton
let instance = null;
function getSemanticMemory(dbPath) {
    if (!instance) {
        instance = new SemanticMemory(dbPath);
        instance.init();
    }
    return instance;
}
//# sourceMappingURL=semantic.js.map