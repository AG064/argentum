"use strict";
/**
 * Memory Compression Feature
 *
 * Compresses old memories by deduplicating, merging similar entries,
 * and archiving outdated data to a separate table.
 */
var __importDefault = (this && this.__importDefault) || function (mod) {
    return (mod && mod.__esModule) ? mod : { "default": mod };
};
Object.defineProperty(exports, "__esModule", { value: true });
const fs_1 = require("fs");
const path_1 = require("path");
const better_sqlite3_1 = __importDefault(require("better-sqlite3"));
/**
 * MemoryCompressionFeature — compress and archive old memories.
 *
 * Works with the kv_store table from sqlite-memory feature.
 * Provides deduplication, similarity merging, and archival.
 */
class MemoryCompressionFeature {
    meta = {
        name: 'memory-compression',
        version: '0.0.1',
        description: 'Compress and archive old memory entries',
        dependencies: [],
    };
    config;
    ctx;
    db;
    lastCompressStats = null;
    constructor() {
        this.config = {
            dbPath: './data/sqlite-memory.db',
            archiveAfterDays: 30,
            similarityThreshold: 0.8,
        };
    }
    async init(config, context) {
        this.ctx = context;
        this.config = {
            dbPath: config['dbPath'] ?? this.config['dbPath'],
            archiveAfterDays: config['archiveAfterDays'] ?? this.config['archiveAfterDays'],
            similarityThreshold: config['similarityThreshold'] ?? this.config['similarityThreshold'],
        };
        this.initDatabase();
    }
    async start() {
        this.ctx.logger.info('MemoryCompression active', {
            dbPath: this.config.dbPath,
            archiveAfterDays: this.config.archiveAfterDays,
        });
    }
    async stop() {
        if (this.db) {
            this.db.close();
            this.ctx.logger.info('MemoryCompression stopped');
        }
    }
    async healthCheck() {
        try {
            const stats = this.getStats();
            return {
                healthy: true,
                details: { ...stats },
            };
        }
        catch (err) {
            return {
                healthy: false,
                message: err instanceof Error ? err.message : String(err),
            };
        }
    }
    /** Compress memories in a namespace older than specified days */
    async compress(namespace, olderThanDays) {
        const cutoff = Date.now() - olderThanDays * 24 * 60 * 60 * 1000;
        // Get entries older than cutoff
        const oldEntries = this.db
            .prepare(`
      SELECT rowid, * FROM kv_store
      WHERE namespace = ? AND updated_at < ?
      ORDER BY updated_at ASC
    `)
            .all(namespace, cutoff);
        if (oldEntries.length === 0) {
            return { duplicatesMerged: 0, similarMerged: 0, entriesBefore: 0, entriesAfter: 0 };
        }
        const entriesBefore = oldEntries.length;
        let duplicatesMerged = 0;
        let similarMerged = 0;
        // Group by exact value to deduplicate
        const valueGroups = new Map();
        for (const entry of oldEntries) {
            const group = valueGroups.get(entry.value) ?? [];
            group.push({ rowid: entry.rowid, key: entry.key, updated_at: entry.updated_at });
            valueGroups.set(entry.value, group);
        }
        // For each duplicate group, keep the most recent, mark others for deletion
        const toDelete = new Set();
        for (const [__value, group] of valueGroups.entries()) {
            if (group.length > 1) {
                // Sort by updated_at descending, keep first
                group.sort((a, b) => b.updated_at - a.updated_at);
                for (let i = 1; i < group.length; i++) {
                    toDelete.add(group[i].rowid);
                }
                duplicatesMerged += group.length - 1;
            }
        }
        // Find similar entries among those not already exact duplicates
        const candidates = oldEntries.filter((e) => !toDelete.has(e.rowid));
        const mergedPairs = new Set();
        for (let i = 0; i < candidates.length; i++) {
            if (mergedPairs.has(candidates[i].rowid))
                continue;
            for (let j = i + 1; j < candidates.length; j++) {
                if (mergedPairs.has(candidates[j].rowid))
                    continue;
                const a = candidates[i].value;
                const b = candidates[j].value;
                const similarity = this.jaccardSimilarity(a, b);
                if (similarity >= this.config.similarityThreshold) {
                    // Merge: keep the more recent entry, mark the other for deletion (will be archived)
                    const keep = candidates[i].updated_at >= candidates[j].updated_at
                        ? candidates[i]
                        : candidates[j];
                    const remove = keep === candidates[i] ? candidates[j] : candidates[i];
                    toDelete.add(remove.rowid);
                    mergedPairs.add(remove.rowid);
                    // Update the kept entry's value to include both (concatenate)
                    this.db
                        .prepare(`UPDATE kv_store SET value = ?, updated_at = ? WHERE rowid = ?`)
                        .run(`${keep.value}\n---\n${remove.value}`, Date.now(), keep.rowid);
                    similarMerged++;
                }
            }
        }
        // Move deleted entries to archive
        if (toDelete.size > 0) {
            const archiveRows = oldEntries.filter((e) => toDelete.has(e.rowid));
            const insertArchive = this.db.prepare(`
        INSERT INTO kv_archive (namespace, key, value, archived_at, reason)
        VALUES (@namespace, @key, @value, @archived_at, @reason)
      `);
            const deleteStmt = this.db.prepare('DELETE FROM kv_store WHERE rowid = ?');
            const now = Date.now();
            for (const row of archiveRows) {
                insertArchive.run({
                    namespace: row.namespace,
                    key: row.key,
                    value: row.value,
                    archived_at: now,
                    reason: toDelete.has(row.rowid) && valueGroups.get(row.value).length > 1
                        ? 'duplicate'
                        : 'merged',
                });
                deleteStmt.run(row.rowid);
            }
        }
        this.lastCompressStats = {
            namespace,
            olderThanDays,
            duplicatesMerged,
            similarMerged,
        };
        return {
            duplicatesMerged,
            similarMerged,
            entriesBefore,
            entriesAfter: this.db
                .prepare('SELECT COUNT(*) as c FROM kv_store WHERE namespace = ?')
                .get(namespace).c,
        };
    }
    /** Archive entries older than configured threshold (or all if no threshold) */
    async archive(namespace) {
        const cutoff = Date.now() - this.config.archiveAfterDays * 24 * 60 * 60 * 1000;
        const toArchive = this.db
            .prepare(`
      SELECT rowid, namespace, key, value FROM kv_store
      WHERE namespace = ? AND updated_at < ?
    `)
            .all(namespace, cutoff);
        if (toArchive.length === 0) {
            return 0;
        }
        const insertArchive = this.db.prepare(`
      INSERT INTO kv_archive (namespace, key, value, archived_at, reason)
      VALUES (@namespace, @key, @value, @archived_at, @reason)
    `);
        const deleteStmt = this.db.prepare('DELETE FROM kv_store WHERE rowid = ?');
        const now = Date.now();
        for (const row of toArchive) {
            insertArchive.run({
                namespace: row.namespace,
                key: row.key,
                value: row.value,
                archived_at: now,
                reason: 'age',
            });
            deleteStmt.run(row.rowid);
        }
        return toArchive.length;
    }
    /** Get compression statistics */
    getStats() {
        const totalActive = this.db.prepare('SELECT COUNT(*) as c FROM kv_store').get().c;
        const totalArchived = this.db.prepare('SELECT COUNT(*) as c FROM kv_archive').get().c;
        const namespaces = {};
        // Get distinct namespaces from kv_store
        const nsRows = this.db.prepare('SELECT DISTINCT namespace FROM kv_store').all();
        for (const nsRow of nsRows) {
            const ns = nsRow.namespace;
            const active = this.db.prepare('SELECT COUNT(*) as c FROM kv_store WHERE namespace = ?').get(ns).c;
            const archived = this.db.prepare('SELECT COUNT(*) as c FROM kv_archive WHERE namespace = ?').get(ns).c;
            namespaces[ns] = { active, archived };
        }
        return {
            totalActive,
            totalArchived,
            namespaces,
            lastCompress: this.lastCompressStats ?? undefined,
        };
    }
    /** Initialize database connection and ensure tables */
    initDatabase() {
        const fullPath = (0, path_1.resolve)(this.config.dbPath);
        if (!(0, fs_1.existsSync)((0, path_1.dirname)(fullPath))) {
            (0, fs_1.mkdirSync)((0, path_1.dirname)(fullPath), { recursive: true });
        }
        this.db = new better_sqlite3_1.default(fullPath);
        this.db.pragma('journal_mode = WAL');
        this.db.pragma('synchronous = NORMAL');
        // Ensure kv_archive table exists (kv_store is created by sqlite-memory)
        this.db.exec(`
      CREATE TABLE IF NOT EXISTS kv_archive (
        rowid INTEGER PRIMARY KEY AUTOINCREMENT,
        namespace TEXT NOT NULL,
        key TEXT NOT NULL,
        value TEXT NOT NULL,
        archived_at INTEGER NOT NULL,
        reason TEXT NOT NULL DEFAULT 'age'
      );

      CREATE INDEX IF NOT EXISTS idx_kv_archive_namespace ON kv_archive(namespace);
      CREATE INDEX IF NOT EXISTS idx_kv_archive_archived_at ON kv_archive(archived_at DESC);
    `);
    }
    /** Compute Jaccard similarity between two texts based on word sets */
    jaccardSimilarity(a, b) {
        const wordsA = new Set(a
            .toLowerCase()
            .split(/\s+/)
            .filter((w) => w.length > 2));
        const wordsB = new Set(b
            .toLowerCase()
            .split(/\s+/)
            .filter((w) => w.length > 2));
        if (wordsA.size === 0 && wordsB.size === 0)
            return 1;
        if (wordsA.size === 0 || wordsB.size === 0)
            return 0;
        const intersection = new Set([...wordsA].filter((w) => wordsB.has(w)));
        const union = new Set([...wordsA, ...wordsB]);
        return intersection.size / union.size;
    }
}
exports.default = new MemoryCompressionFeature();
//# sourceMappingURL=index.js.map