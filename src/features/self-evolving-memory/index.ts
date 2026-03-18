/**
 * Self-Evolving Memory Feature
 *
 * Automatically improves memory by analyzing patterns, deduplicating,
 * consolidating similar entries, and promoting important memories.
 */

import Database from 'better-sqlite3';
import { mkdirSync, existsSync } from 'fs';
import { dirname, resolve } from 'path';
import { FeatureModule, FeatureContext, FeatureMeta, HealthStatus } from '../../core/plugin-loader';

/** Feature configuration */
export interface SelfEvolvingConfig {
  dbPath?: string;
  autoEvolve?: boolean;
  evolveIntervalMs?: number;
  similarityThreshold?: number;
  decayRate?: number;
  weightBoost?: number;
};

/** Analysis result */
export interface AnalysisResult {
  totalMemories: number;
  byType: Record<string, number>;
  averageWeight: number;
  zeroWeightCount: number;
  topWords: Array<{ word: string; frequency: number }>;
  duplicateGroups: number;
};

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
class SelfEvolvingMemoryFeature implements FeatureModule {
  readonly meta: FeatureMeta = {
    name: 'self-evolving-memory',
    version: '0.1.0',
    description: 'Automatic memory optimization: dedup, merge, decay, promote',
    dependencies: [],
  };

  private config: Required<SelfEvolvingConfig>;
  private ctx!: FeatureContext;
  private db!: Database.Database;
  private timer: ReturnType<typeof setInterval> | null = null;
  private lastAnalysis: AnalysisResult | null = null;
  private lastConsolidation: { dedup: number; merged: number; decayed: number; pruned: number } | null = null;

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

  async init(config: Record<string, unknown>, context: FeatureContext): Promise<void> {
    this.ctx = context;
    this.config = {
      dbPath: (config.dbPath as string) ?? this.config.dbPath,
      autoEvolve: (config.autoEvolve as boolean) ?? this.config.autoEvolve,
      evolveIntervalMs: (config.evolveIntervalMs as number) ?? this.config.evolveIntervalMs,
      similarityThreshold: (config.similarityThreshold as number) ?? this.config.similarityThreshold,
      decayRate: (config.decayRate as number) ?? this.config.decayRate,
      weightBoost: (config.weightBoost as number) ?? this.config.weightBoost,
    };

    this.initDatabase();
  }

  async start(): Promise<void> {
    if (this.config.autoEvolve) {
      this.timer = setInterval(() => {
        this.consolidate().catch((err: Error) => {
          this.ctx.logger.error('Auto-consolidation failed', { error: err instanceof Error ? err.message : String(err) });
        });
      }, this.config.evolveIntervalMs);
    }

    this.ctx.logger.info('SelfEvolvingMemory active', {
      autoEvolve: this.config.autoEvolve,
      intervalMs: this.config.evolveIntervalMs,
    });
  }

  async stop(): Promise<void> {
    if (this.timer) {
      clearInterval(this.timer);
      this.timer = null;
    }
    this.db?.close();
    this.ctx.logger.info('SelfEvolvingMemory stopped');
  }

  async healthCheck(): Promise<HealthStatus> {
    try {
      const count = (this.db.prepare('SELECT COUNT(*) as c FROM memories').get() as { c: number }).c;
      return {
        healthy: true,
        details: {
          totalMemories: count,
          lastAnalysis: this.lastAnalysis,
          lastConsolidation: this.lastConsolidation,
        },
      };
    } catch (err) {
      return {
        healthy: false,
        message: err instanceof Error ? err.message : String(err),
      };
    }
  }

  /** Analyze memory store for patterns and statistics */
  async analyze(): Promise<AnalysisResult> {
    const total = (this.db.prepare('SELECT COUNT(*) as c FROM memories').get() as { c: number }).c;

    const byType: Record<string, number> = {};
    const typeRows = this.db.prepare('SELECT type, COUNT(*) as c FROM memories GROUP BY type').all() as Array<{ type: string; c: number }>;
    for (const row of typeRows) {
      byType[row.type] = row.c;
    }

    const avgWeight = (this.db.prepare('SELECT AVG(weight) as avg FROM memories').get() as { avg: number })?.avg ?? 0;
    const zeroWeight = (this.db.prepare('SELECT COUNT(*) as c FROM memories WHERE weight < 0.1').get() as { c: number }).c;

    // Top words (simple word frequency from content)
    const rows = this.db.prepare('SELECT content FROM memories').all() as Array<{ content: string }>;
    const wordCounts = new Map<string, number>();
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
    const dupGroups = (this.db.prepare(`
      SELECT content_hash, COUNT(*) as cnt FROM memories
      WHERE content_hash IS NOT NULL
      GROUP BY content_hash HAVING cnt > 1
    `).all() as Array<{ content_hash: string; cnt: number }>).length;

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
  async deduplicate(): Promise<number> {
    // Find duplicates (keep oldest by created_at)
    const duplicates = this.db.prepare(`
      SELECT MIN(rowid) as keep_id, GROUP_CONCAT(rowid) as ids
      FROM memories
      WHERE content_hash IS NOT NULL
      GROUP BY content_hash
      HAVING COUNT(*) > 1
    `).all() as Array<{ keep_id: number; ids: string }>;

    let removed = 0;
    const transaction = this.db.transaction(() => {
      for (const dup of duplicates) {
        const ids = dup.ids.split(',').map(Number).filter(id => id !== dup.keep_id);
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
  async consolidate(): Promise<{ dedup: number; merged: number; decayed: number; pruned: number }> {
    this.ctx.logger.info('Starting consolidation cycle');
    const startTime = Date.now();

    // 1. Deduplicate
    const dedup = await this.deduplicate();

    // 2. Decay weights for old, rarely accessed memories
    const now = Date.now();
    const weekMs = 7 * 24 * 60 * 60 * 1000;
    const decayResult = this.db.prepare(`
      UPDATE memories
      SET weight = weight * ?
      WHERE accessed_at < ? AND access_count < 3 AND weight > 0.1
    `).run(this.config.decayRate, now - weekMs);
    const decayed = decayResult.changes;

    // 3. Merge similar entries (Jaccard similarity)
    const merged = await this.mergeSimilar();

    // 4. Prune entries with very low weight and no access
    const pruneResult = this.db.prepare(`
      DELETE FROM memories
      WHERE weight < 0.1 AND access_count = 0
    `).run();
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
  async promote(pattern: string): Promise<number> {
    // Find memories where content contains the pattern (simple LIKE)
    const likePattern = `%${pattern}%`;
    const toBoost = this.db.prepare(`
      SELECT rowid FROM memories WHERE content LIKE ?
    `).all(likePattern) as Array<{ rowid: number }>;

    if (toBoost.length === 0) {
      return 0;
    }

    const transaction = this.db.transaction(() => {
      for (const row of toBoost) {
        this.db.prepare(`
          UPDATE memories
          SET weight = MIN(weight * ?, 1.0), accessed_at = ?
          WHERE rowid = ?
        `).run(this.config.weightBoost, Date.now(), row.rowid);
      }
    });
    transaction();

    this.ctx.logger.info('Promotion applied', { pattern, count: toBoost.length });
    return toBoost.length;
  }

  /** Initialize database connection */
  private initDatabase(): void {
    const fullPath = resolve(this.config.dbPath);
    if (!existsSync(dirname(fullPath))) {
      mkdirSync(dirname(fullPath), { recursive: true });
    }

    this.db = new Database(fullPath);
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
  private async mergeSimilar(): Promise<number> {
    // Get all memories (sample if too many)
    const all = this.db.prepare('SELECT rowid, content, weight, access_count FROM memories').all() as Array<{
      rowid: number;
      content: string;
      weight: number;
      access_count: number;
    }>;

    if (all.length < 2) return 0;

    let merged = 0;
    const toDelete = new Set<number>();

    for (let i = 0; i < all.length; i++) {
      if (toDelete.has(all[i]!.rowid)) continue;
      for (let j = i + 1; j < all.length; j++) {
        if (toDelete.has(all[j]!.rowid)) continue;

        const a = all[i]!.content;
        const b = all[j]!.content;
        const similarity = this.jaccardSimilarity(a, b);
        if (similarity >= this.config.similarityThreshold) {
          // Keep the one with higher weight or more accesses
          const keep = all[i]!.weight >= all[j]!.weight ? all[i]! : all[j]!;
          const remove = keep === all[i]! ? all[j]! : all[i]!;
          toDelete.add(remove.rowid);
          // Merge: update kept entry's content to combine both
          const mergedContent = `${keep.content}\n---\n${remove.content}`;
          this.db.prepare('UPDATE memories SET content = ?, weight = ?, access_count = ? WHERE rowid = ?').run(
            mergedContent,
            keep.weight,
            keep.access_count + remove.access_count,
            keep.rowid
          );
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
  private jaccardSimilarity(a: string, b: string): number {
    const wordsA = new Set(a.toLowerCase().split(/\s+/).filter(w => w.length > 3));
    const wordsB = new Set(b.toLowerCase().split(/\s+/).filter(w => w.length > 3));
    if (wordsA.size === 0 && wordsB.size === 0) return 1;
    if (wordsA.size === 0 || wordsB.size === 0) return 0;
    const intersection = new Set([...wordsA].filter(w => wordsB.has(w)));
    const union = new Set([...wordsA, ...wordsB]);
    return intersection.size / union.size;
  }
}

export default new SelfEvolvingMemoryFeature();