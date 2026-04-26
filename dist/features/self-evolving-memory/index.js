"use strict";
/**
 * Self-Evolving Memory Feature
 *
 * Automatically improves memory by analyzing patterns, deduplicating,
 * consolidating similar entries, and promoting important memories.
 */
var __importDefault = (this && this.__importDefault) || function (mod) {
    return (mod && mod.__esModule) ? mod : { "default": mod };
};
Object.defineProperty(exports, "__esModule", { value: true });
const fs_1 = require("fs");
const path_1 = require("path");
const better_sqlite3_1 = __importDefault(require("better-sqlite3"));
/**
 * SelfEvolvingMemoryFeature — automatic memory optimization.
 *
 * Connects to a SemanticMemory database and provides:
 * - analyze(): collect statistics and pattern insights
 * - deduplicate(): remove exact content duplicates
 * - consolidate(): full cycle (dedup + decay + merge similar + prune)
 * - promote(pattern): boost weight of memories matching a text pattern
 *
 * Can run automatically on a schedule.
 */
class SelfEvolvingMemoryFeature {
    meta = {
        name: 'self-evolving-memory',
        version: '0.0.1',
        description: 'Automatic memory optimization: dedup, merge, decay, promote',
        dependencies: [],
    };
    config;
    ctx;
    db;
    timer = null;
    lastAnalysis = null;
    lastConsolidation = null;
    constructor() {
        this.config = {
            dbPath: './data/semantic-memory.db',
            autoEvolve: true,
            evolveIntervalMs: 3600000, // 1 hour
            similarityThreshold: 0.85,
            decayRate: 0.95,
            weightBoost: 1.5,
        };
    }
    async init(config, context) {
        this.ctx = context;
        this.config = {
            dbPath: config['dbPath'] ?? this.config['dbPath'],
            autoEvolve: config['autoEvolve'] ?? this.config['autoEvolve'],
            evolveIntervalMs: config['evolveIntervalMs'] ?? this.config['evolveIntervalMs'],
            similarityThreshold: config['similarityThreshold'] ?? this.config['similarityThreshold'],
            decayRate: config['decayRate'] ?? this.config['decayRate'],
            weightBoost: config['weightBoost'] ?? this.config['weightBoost'],
        };
        this.initDatabase();
    }
    async start() {
        if (this.config.autoEvolve) {
            this.timer = setInterval(() => {
                this.consolidate().catch((err) => {
                    this.ctx.logger.error('Auto-consolidation failed', {
                        error: err instanceof Error ? err.message : String(err),
                    });
                });
            }, this.config.evolveIntervalMs);
        }
        this.ctx.logger.info('SelfEvolvingMemory active', {
            autoEvolve: this.config.autoEvolve,
            intervalMs: this.config.evolveIntervalMs,
        });
    }
    async stop() {
        if (this.timer) {
            clearInterval(this.timer);
            this.timer = null;
        }
        this.db?.close();
        this.ctx.logger.info('SelfEvolvingMemory stopped');
    }
    async healthCheck() {
        try {
            const count = this.db.prepare('SELECT COUNT(*) as c FROM memories').get()
                .c;
            return {
                healthy: true,
                details: {
                    totalMemories: count,
                    lastAnalysis: this.lastAnalysis,
                    lastConsolidation: this.lastConsolidation,
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
    /** Analyze memory store for patterns and statistics */
    async analyze() {
        const total = this.db.prepare('SELECT COUNT(*) as c FROM memories').get().c;
        const byType = {};
        const typeRows = this.db
            .prepare('SELECT type, COUNT(*) as c FROM memories GROUP BY type')
            .all();
        for (const row of typeRows) {
            byType[row.type] = row.c;
        }
        const avgWeight = this.db.prepare('SELECT AVG(weight) as avg FROM memories').get()?.avg ??
            0;
        const zeroWeight = this.db.prepare('SELECT COUNT(*) as c FROM memories WHERE weight < 0.1').get().c;
        // Top words (simple word frequency from content)
        const rows = this.db.prepare('SELECT content FROM memories').all();
        const wordCounts = new Map();
        for (const row of rows) {
            const words = row.content.toLowerCase().split(/\s+/);
            for (const w of words) {
                if (w.length > 3) {
                    wordCounts.set(w, (wordCounts.get(w) ?? 0) + 1);
                }
            }
        }
        const topWords = Array.from(wordCounts.entries())
            .sort((a, b) => b[1] - a[1])
            .slice(0, 10)
            .map(([word, frequency]) => ({ word, frequency }));
        // Duplicate groups by content_hash
        const dupGroups = this.db
            .prepare(`
      SELECT content_hash, COUNT(*) as cnt FROM memories
      WHERE content_hash IS NOT NULL
      GROUP BY content_hash HAVING cnt > 1
    `)
            .all().length;
        this.lastAnalysis = {
            totalMemories: total,
            byType,
            averageWeight: avgWeight,
            zeroWeightCount: zeroWeight,
            topWords,
            duplicateGroups: dupGroups,
        };
        return this.lastAnalysis;
    }
    /** Deduplicate exact content hashes */
    async deduplicate() {
        // Find duplicates (keep oldest by created_at)
        const duplicates = this.db
            .prepare(`
      SELECT MIN(rowid) as keep_id, GROUP_CONCAT(rowid) as ids
      FROM memories
      WHERE content_hash IS NOT NULL
      GROUP BY content_hash
      HAVING COUNT(*) > 1
    `)
            .all();
        let removed = 0;
        const transaction = this.db.transaction(() => {
            for (const dup of duplicates) {
                const ids = dup.ids
                    .split(',')
                    .map(Number)
                    .filter((id) => id !== dup.keep_id);
                for (const id of ids) {
                    this.db.prepare('DELETE FROM memories WHERE rowid = ?').run(id);
                    // Also delete edges? Not needed if FK cascade is set? Edges table has FK with ON DELETE CASCADE.
                    removed++;
                }
            }
        });
        transaction();
        this.ctx.logger.info('Deduplication complete', { removed });
        return removed;
    }
    /** Full consolidation cycle */
    async consolidate() {
        this.ctx.logger.info('Starting consolidation cycle');
        const startTime = Date.now();
        // 1. Deduplicate
        const dedup = await this.deduplicate();
        // 2. Decay weights for old, rarely accessed memories
        const now = Date.now();
        const weekMs = 7 * 24 * 60 * 60 * 1000;
        const decayResult = this.db
            .prepare(`
      UPDATE memories
      SET weight = weight * ?
      WHERE accessed_at < ? AND access_count < 3 AND weight > 0.1
    `)
            .run(this.config.decayRate, now - weekMs);
        const decayed = decayResult.changes;
        // 3. Merge similar entries (Jaccard similarity)
        const merged = await this.mergeSimilar();
        // 4. Prune entries with very low weight and no access
        const pruneResult = this.db
            .prepare(`
      DELETE FROM memories
      WHERE weight < 0.1 AND access_count = 0
    `)
            .run();
        const pruned = pruneResult.changes;
        const durationMs = Date.now() - startTime;
        this.lastConsolidation = { dedup, merged, decayed, pruned };
        this.ctx.logger.info('Consolidation complete', {
            dedup,
            merged,
            decayed,
            pruned,
            durationMs,
        });
        return this.lastConsolidation;
    }
    /** Promote memories matching a text pattern by boosting weight */
    async promote(pattern) {
        // Find memories where content contains the pattern (simple LIKE)
        const likePattern = `%${pattern}%`;
        const toBoost = this.db
            .prepare(`
      SELECT rowid FROM memories WHERE content LIKE ?
    `)
            .all(likePattern);
        if (toBoost.length === 0) {
            return 0;
        }
        const transaction = this.db.transaction(() => {
            for (const row of toBoost) {
                this.db
                    .prepare(`
          UPDATE memories
          SET weight = MIN(weight * ?, 1.0), accessed_at = ?
          WHERE rowid = ?
        `)
                    .run(this.config.weightBoost, Date.now(), row.rowid);
            }
        });
        transaction();
        this.ctx.logger.info('Promotion applied', { pattern, count: toBoost.length });
        return toBoost.length;
    }
    /** Initialize database connection */
    initDatabase() {
        const fullPath = (0, path_1.resolve)(this.config.dbPath);
        if (!(0, fs_1.existsSync)((0, path_1.dirname)(fullPath))) {
            (0, fs_1.mkdirSync)((0, path_1.dirname)(fullPath), { recursive: true });
        }
        this.db = new better_sqlite3_1.default(fullPath);
        this.db.pragma('journal_mode = WAL');
        this.db.pragma('synchronous = NORMAL');
        // Ensure the memories table exists (from semantic memory)
        // We'll not create; assume it exists. But we can ensure indexes needed for this feature.
        this.db.exec(`
      CREATE INDEX IF NOT EXISTS idx_mem_content_hash ON memories(content_hash);
      CREATE INDEX IF NOT EXISTS idx_mem_weight ON memories(weight);
    `);
    }
    /** Merge similar memory entries using Jaccard similarity */
    async mergeSimilar() {
        // Get all memories (sample if too many)
        const all = this.db
            .prepare('SELECT rowid, content, weight, access_count FROM memories')
            .all();
        if (all.length < 2)
            return 0;
        let merged = 0;
        const toDelete = new Set();
        for (let i = 0; i < all.length; i++) {
            if (toDelete.has(all[i].rowid))
                continue;
            for (let j = i + 1; j < all.length; j++) {
                if (toDelete.has(all[j].rowid))
                    continue;
                const a = all[i].content;
                const b = all[j].content;
                const similarity = this.jaccardSimilarity(a, b);
                if (similarity >= this.config.similarityThreshold) {
                    // Keep the one with higher weight or more accesses
                    const keep = all[i].weight >= all[j].weight ? all[i] : all[j];
                    const remove = keep === all[i] ? all[j] : all[i];
                    toDelete.add(remove.rowid);
                    // Merge: update kept entry's content to combine both
                    const mergedContent = `${keep.content}\n---\n${remove.content}`;
                    this.db
                        .prepare('UPDATE memories SET content = ?, weight = ?, access_count = ? WHERE rowid = ?')
                        .run(mergedContent, keep.weight, keep.access_count + remove.access_count, keep.rowid);
                    merged++;
                }
            }
        }
        // Delete merged entries
        if (toDelete.size > 0) {
            const delStmt = this.db.prepare('DELETE FROM memories WHERE rowid = ?');
            for (const id of toDelete) {
                delStmt.run(id);
            }
        }
        return merged;
    }
    /** Jaccard similarity on word sets */
    jaccardSimilarity(a, b) {
        const wordsA = new Set(a
            .toLowerCase()
            .split(/\s+/)
            .filter((w) => w.length > 3));
        const wordsB = new Set(b
            .toLowerCase()
            .split(/\s+/)
            .filter((w) => w.length > 3));
        if (wordsA.size === 0 && wordsB.size === 0)
            return 1;
        if (wordsA.size === 0 || wordsB.size === 0)
            return 0;
        const intersection = new Set([...wordsA].filter((w) => wordsB.has(w)));
        const union = new Set([...wordsA, ...wordsB]);
        return intersection.size / union.size;
    }
}
exports.default = new SelfEvolvingMemoryFeature();
//# sourceMappingURL=index.js.map