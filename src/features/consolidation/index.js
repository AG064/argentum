'use strict';
/**
 * Consolidation Feature
 *
 * OMEGA Memory integration — periodic memory consolidation.
 * Deduplication, decay, merge, and pruning of memory entries.
 */
Object.defineProperty(exports, '__esModule', { value: true });
const semantic_1 = require('../../memory/semantic');
/**
 * Consolidation — periodic memory maintenance.
 *
 * Runs on a schedule to:
 * 1. Deduplicate — remove exact hash matches
 * 2. Decay — reduce weight of old, rarely accessed entries
 * 3. Merge — combine similar entries (semantic similarity > threshold)
 * 4. Prune — remove entries below weight threshold
 */
class ConsolidationFeature {
  constructor() {
    this.meta = {
      name: 'consolidation',
      version: '0.0.4',
      description: 'Periodic memory consolidation: dedup, decay, merge, prune',
      dependencies: [],
    };
    this.config = {
      enabled: false,
      intervalMs: 3600000, // 1 hour
      similarityThreshold: 0.85,
      decayRate: 0.95,
      pruneWeightThreshold: 0.1,
      maxMemories: 50000,
      dryRun: false,
    };
    this.timer = null;
    this.lastRun = 0;
    this.lastResult = null;
    this.runCount = 0;
  }
  async init(config, context) {
    this.ctx = context;
    this.config = { ...this.config, ...config };
  }
  async start() {
    if (this.config.intervalMs > 0) {
      this.timer = setInterval(() => {
        this.run().catch((err) => {
          this.ctx.logger.error('Consolidation run failed', {
            error: err instanceof Error ? err.message : String(err),
          });
        });
      }, this.config.intervalMs);
    }
    this.ctx.logger.info('Consolidation active', {
      interval: `${this.config.intervalMs / 1000}s`,
      similarityThreshold: this.config.similarityThreshold,
      decayRate: this.config.decayRate,
      dryRun: this.config.dryRun,
    });
  }
  async stop() {
    if (this.timer) {
      clearInterval(this.timer);
      this.timer = null;
    }
  }
  async healthCheck() {
    return {
      healthy: true,
      details: {
        lastRun: this.lastRun ? new Date(this.lastRun).toISOString() : 'never',
        runCount: this.runCount,
        lastResult: this.lastResult,
      },
    };
  }
  /** Run full consolidation cycle */
  async run() {
    const startTime = Date.now();
    const memory = (0, semantic_1.getSemanticMemory)();
    const totalBefore = await memory.count();
    this.ctx.logger.info('Starting consolidation', { totalBefore });
    // Step 1: Deduplication
    const deduped = await this.deduplicate();
    // Step 2: Decay
    const decayed = await this.applyDecay();
    // Step 3: Merge similar entries
    const merged = await this.mergeSimilar();
    // Step 4: Prune low-weight entries
    const pruned = await this.prune();
    // Enforce max memories
    const prunedByLimit = await this.enforceMaxLimit();
    const totalAfter = await memory.count();
    const durationMs = Date.now() - startTime;
    const result = {
      deduplicated: deduped,
      decayed,
      merged,
      pruned: pruned + prunedByLimit,
      totalBefore,
      totalAfter,
      durationMs,
    };
    this.lastRun = Date.now();
    this.lastResult = result;
    this.runCount++;
    this.ctx.logger.info('Consolidation complete', {
      deduplicated: result.deduplicated,
      decayed: result.decayed,
      merged: result.merged,
      pruned: result.pruned,
      totalBefore: result.totalBefore,
      totalAfter: result.totalAfter,
      durationMs: result.durationMs,
    });
    return result;
  }
  /** Remove exact content hash duplicates */
  async deduplicate() {
    const memory = (0, semantic_1.getSemanticMemory)();
    const db = memory.getDb();
    // Find duplicates by content hash
    const duplicates = db
      .prepare(
        `
      SELECT content_hash, COUNT(*) as cnt, GROUP_CONCAT(id) as ids
      FROM memories
      WHERE content_hash IS NOT NULL
      GROUP BY content_hash
      HAVING cnt > 1
    `,
      )
      .all();
    let removed = 0;
    for (const dup of duplicates) {
      const ids = dup.ids.split(',');
      // Keep the first (oldest), remove the rest
      const toRemove = ids.slice(1);
      if (!this.config.dryRun) {
        for (const id of toRemove) {
          await memory.delete(id);
        }
      }
      removed += toRemove.length;
    }
    if (removed > 0) {
      this.ctx.logger.debug('Deduplication', { removed, duplicateGroups: duplicates.length });
    }
    return removed;
  }
  /** Apply weight decay to old, rarely accessed memories */
  async applyDecay() {
    const memory = (0, semantic_1.getSemanticMemory)();
    const db = memory.getDb();
    const now = Date.now();
    const weekMs = 7 * 24 * 60 * 60 * 1000;
    // Decay memories not accessed in the last week with low access count
    if (!this.config.dryRun) {
      const result = db
        .prepare(
          `
        UPDATE memories
        SET weight = weight * ?
        WHERE accessed_at < ? AND access_count < 3 AND weight > 0.1
      `,
        )
        .run(this.config.decayRate, now - weekMs);
      return result.changes;
    }
    const count = db
      .prepare(
        `
      SELECT COUNT(*) as c FROM memories
      WHERE accessed_at < ? AND access_count < 3 AND weight > 0.1
    `,
      )
      .get(now - weekMs);
    return count.c;
  }
  /** Merge similar entries using Jaccard similarity */
  async mergeSimilar() {
    const memory = (0, semantic_1.getSemanticMemory)();
    const db = memory.getDb();
    // Get all memories
    const allMemories = db.prepare('SELECT * FROM memories ORDER BY created_at ASC').all();
    let merged = 0;
    const processed = new Set();
    for (let i = 0; i < allMemories.length; i++) {
      const a = allMemories[i];
      if (processed.has(a.id)) continue;
      for (let j = i + 1; j < allMemories.length; j++) {
        const b = allMemories[j];
        if (processed.has(b.id)) continue;
        if (a.type !== b.type) continue; // Only merge same type
        const similarity = this.computeSimilarity(a.content, b.content);
        if (similarity >= this.config.similarityThreshold) {
          if (!this.config.dryRun) {
            // Merge b into a
            const mergedContent = this.mergeContent(a.content, b.content);
            const mergedWeight = Math.max(a.weight, b.weight);
            const mergedAccess = a.access_count + b.access_count;
            db.prepare(
              `
              UPDATE memories
              SET content = ?, weight = ?, access_count = ?, accessed_at = ?
              WHERE id = ?
            `,
            ).run(mergedContent, mergedWeight, mergedAccess, Date.now(), a.id);
            // Delete b
            await memory.delete(b.id);
            // Transfer edges from b to a
            db.prepare('UPDATE edges SET source_id = ? WHERE source_id = ?').run(a.id, b.id);
            db.prepare('UPDATE edges SET target_id = ? WHERE target_id = ?').run(a.id, b.id);
          }
          processed.add(b.id);
          merged++;
        }
      }
    }
    if (merged > 0) {
      this.ctx.logger.debug('Merged similar entries', { merged });
    }
    return merged;
  }
  /** Prune entries below weight threshold */
  async prune() {
    const memory = (0, semantic_1.getSemanticMemory)();
    const db = memory.getDb();
    if (!this.config.dryRun) {
      // First clean up edges pointing to pruned memories
      db.prepare(
        `
        DELETE FROM edges WHERE source_id IN (
          SELECT id FROM memories WHERE weight < ? AND access_count = 0
        ) OR target_id IN (
          SELECT id FROM memories WHERE weight < ? AND access_count = 0
        )
      `,
      ).run(this.config.pruneWeightThreshold, this.config.pruneWeightThreshold);
      const result = db
        .prepare(
          `
        DELETE FROM memories WHERE weight < ? AND access_count = 0
      `,
        )
        .run(this.config.pruneWeightThreshold);
      return result.changes;
    }
    const count = db
      .prepare(
        `
      SELECT COUNT(*) as c FROM memories WHERE weight < ? AND access_count = 0
    `,
      )
      .get(this.config.pruneWeightThreshold);
    return count.c;
  }
  /** Enforce maximum memory limit */
  async enforceMaxLimit() {
    const memory = (0, semantic_1.getSemanticMemory)();
    const db = memory.getDb();
    const count = db.prepare('SELECT COUNT(*) as c FROM memories').get().c;
    const excess = count - this.config.maxMemories;
    if (excess <= 0) return 0;
    if (!this.config.dryRun) {
      // Remove lowest weight, least accessed memories
      const toRemove = db
        .prepare(
          `
        SELECT id FROM memories
        ORDER BY weight ASC, access_count ASC, created_at ASC
        LIMIT ?
      `,
        )
        .all(excess);
      for (const row of toRemove) {
        await memory.delete(row.id);
      }
      this.ctx.logger.debug('Pruned by limit', { removed: toRemove.length, excess });
      return toRemove.length;
    }
    return excess;
  }
  /** Compute Jaccard similarity between two strings */
  computeSimilarity(a, b) {
    const wordsA = new Set(
      a
        .toLowerCase()
        .split(/\s+/)
        .filter((w) => w.length > 2),
    );
    const wordsB = new Set(
      b
        .toLowerCase()
        .split(/\s+/)
        .filter((w) => w.length > 2),
    );
    if (wordsA.size === 0 && wordsB.size === 0) return 1;
    if (wordsA.size === 0 || wordsB.size === 0) return 0;
    const intersection = new Set([...wordsA].filter((w) => wordsB.has(w)));
    const union = new Set([...wordsA, ...wordsB]);
    return intersection.size / union.size;
  }
  /** Merge two pieces of content */
  mergeContent(a, b) {
    // If one is substring of other, keep the longer
    if (a.includes(b)) return a;
    if (b.includes(a)) return b;
    // Otherwise concatenate with separator
    return `${a}\n---\n${b}`;
  }
  /** Manual run trigger */
  async runNow() {
    return this.run();
  }
  /** Get consolidation stats */
  getStats() {
    return {
      lastRun: this.lastRun,
      runCount: this.runCount,
      lastResult: this.lastResult,
    };
  }
}
exports.default = new ConsolidationFeature();
