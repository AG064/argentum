/**
 * Self-Evolving Memory Feature
 *
 * Memory system that learns, consolidates, and evolves over time.
 * Merges related memories, extracts patterns, and prunes stale data.
 */

import { type FeatureModule, type FeatureContext, type FeatureMeta, type HealthStatus } from '../core/plugin-loader';

/** Self-evolving memory configuration */
export interface SelfEvolvingConfig {
  enabled: boolean;
  consolidationIntervalMs: number;
  similarityThreshold: number;
  minAccessCount: number;
  maxMemories: number;
  pruneAfterDays: number;
}

/** Evolved memory entry */
export interface EvolvedMemory {
  id: string;
  content: string;
  sourceMemories: string[];
  embeddings: number[];
  importance: number;
  accessCount: number;
  consolidated: boolean;
  createdAt: number;
  updatedAt: number;
}

/** Memory pattern */
export interface MemoryPattern {
  id: string;
  type: 'recurring' | 'temporal' | 'causal' | 'associative';
  description: string;
  memoryIds: string[];
  confidence: number;
  discoveredAt: number;
}

/**
 * Self-Evolving memory — memory consolidation and pattern discovery.
 *
 * Periodically reviews stored memories, merges similar ones,
 * discovers patterns, and prunes stale entries to maintain
 * a relevant and efficient memory store.
 */
class SelfEvolvingMemoryFeature implements FeatureModule {
  readonly meta: FeatureMeta = {
    name: 'self-evolving-memory',
    version: '0.0.5',
    description: 'Memory consolidation, pattern discovery, and evolution',
    dependencies: [],
  };

  private config: SelfEvolvingConfig = {
    enabled: false,
    consolidationIntervalMs: 3600000, // 1 hour
    similarityThreshold: 0.85,
    minAccessCount: 2,
    maxMemories: 50000,
    pruneAfterDays: 90,
  };
  private ctx!: FeatureContext;
  private memories: Map<string, EvolvedMemory> = new Map();
  private patterns: Map<string, MemoryPattern> = new Map();
  private timer: ReturnType<typeof setInterval> | null = null;

  async init(config: Record<string, unknown>, context: FeatureContext): Promise<void> {
    this.ctx = context;
    this.config = { ...this.config, ...(config as Partial<SelfEvolvingConfig>) };
  }

  async start(): Promise<void> {
    this.timer = setInterval(() => {
      void this.runConsolidation().catch((error: unknown) => {
        this.ctx.logger.error('Memory consolidation failed', {
          error: error instanceof Error ? error.message : String(error),
        });
      });
    }, this.config.consolidationIntervalMs);

    this.ctx.logger.info('Self-Evolving Memory active', {
      consolidationInterval: `${this.config.consolidationIntervalMs / 1000}s`,
      similarityThreshold: this.config.similarityThreshold,
    });
  }

  async stop(): Promise<void> {
    if (this.timer) {
      clearInterval(this.timer);
      this.timer = null;
    }
  }

  async healthCheck(): Promise<HealthStatus> {
    return {
      healthy: true,
      details: {
        memories: this.memories.size,
        patterns: this.patterns.size,
        consolidated: Array.from(this.memories.values()).filter(m => m.consolidated).length,
      },
    };
  }

  /** Add a new memory */
  addMemory(content: string, sourceIds: string[] = [], importance = 0.5): EvolvedMemory {
    const id = `evo_${Date.now()}_${Math.random().toString(36).slice(2, 8)}`;
    const memory: EvolvedMemory = {
      id,
      content,
      sourceMemories: sourceIds,
      embeddings: [], // Would be populated by embedding model
      importance: Math.max(0, Math.min(1, importance)),
      accessCount: 0,
      consolidated: false,
      createdAt: Date.now(),
      updatedAt: Date.now(),
    };
    this.memories.set(id, memory);
    return memory;
  }

  /** Access a memory (boosts its importance) */
  accessMemory(id: string): void {
    const memory = this.memories.get(id);
    if (memory) {
      memory.accessCount++;
      memory.importance = Math.min(1, memory.importance + 0.01);
      memory.updatedAt = Date.now();
    }
  }

  /** Run consolidation cycle */
  async runConsolidation(): Promise<void> {
    this.ctx.logger.debug('Starting memory consolidation', {
      memories: this.memories.size,
    });

    let mergedCount = 0;
    const toRemove: string[] = [];

    // Find similar memories to merge
    const allMemories = Array.from(this.memories.values());
    for (let i = 0; i < allMemories.length; i++) {
      for (let j = i + 1; j < allMemories.length; j++) {
        const a = allMemories[i]!;
        const b = allMemories[j]!;

        if (a.consolidated && b.consolidated) continue;

        const similarity = this.computeSimilarity(a.content, b.content);
        if (similarity >= this.config.similarityThreshold) {
          // Merge b into a
          a.content = `[Merged] ${a.content}\n---\n${b.content}`;
          a.sourceMemories = [...new Set([...a.sourceMemories, ...b.sourceMemories, b.id])];
          a.importance = Math.max(a.importance, b.importance);
          a.consolidated = true;
          a.updatedAt = Date.now();
          toRemove.push(b.id);
          mergedCount++;
        }
      }
    }

    // Remove merged memories
    for (const id of toRemove) {
      this.memories.delete(id);
    }

    // Discover patterns
    await this.discoverPatterns();

    // Prune stale memories
    await this.pruneStale();

    this.ctx.logger.info('Consolidation complete', {
      merged: mergedCount,
      pruned: toRemove.length,
      patterns: this.patterns.size,
      remaining: this.memories.size,
    });
  }

  /** Discover patterns in memory */
  private async discoverPatterns(): Promise<void> {
    const memories = Array.from(this.memories.values());

    // Simple frequency-based pattern discovery
    const wordCounts = new Map<string, number>();
    for (const mem of memories) {
      const words = mem.content.toLowerCase().split(/\s+/);
      for (const word of words) {
        if (word.length > 4) {
          wordCounts.set(word, (wordCounts.get(word) ?? 0) + 1);
        }
      }
    }

    // Find recurring themes
    for (const [word, count] of wordCounts) {
      if (count >= 5) { // Appears in at least 5 memories
        const matchingMemories = memories
          .filter(m => m.content.toLowerCase().includes(word))
          .map(m => m.id);

        const patternId = `pattern_${word}`;
        this.patterns.set(patternId, {
          id: patternId,
          type: 'recurring',
          description: `Recurring theme: "${word}" appears in ${count} memories`,
          memoryIds: matchingMemories,
          confidence: Math.min(1, count / memories.length),
          discoveredAt: Date.now(),
        });
      }
    }
  }

  /** Prune stale memories based on age and access count */
  private async pruneStale(): Promise<void> {
    const now = Date.now();
    const maxAge = this.config.pruneAfterDays * 24 * 60 * 60 * 1000;

    for (const [id, memory] of this.memories) {
      const age = now - memory.createdAt;
      if (age > maxAge && memory.accessCount < this.config.minAccessCount && !memory.consolidated) {
        this.memories.delete(id);
      }
    }

    // Enforce max memories by removing lowest importance
    while (this.memories.size > this.config.maxMemories) {
      let lowest: EvolvedMemory | null = null;
      for (const mem of this.memories.values()) {
        if (!lowest || mem.importance < lowest.importance) {
          lowest = mem;
        }
      }
      if (lowest) {
        this.memories.delete(lowest.id);
      } else {
        break;
      }
    }
  }

  /** Get all patterns */
  getPatterns(): MemoryPattern[] {
    return Array.from(this.patterns.values());
  }

  /** Get memories related to a pattern */
  getPatternMemories(patternId: string): EvolvedMemory[] {
    const pattern = this.patterns.get(patternId);
    if (!pattern) return [];
    return pattern.memoryIds
      .map(id => this.memories.get(id))
      .filter((m): m is EvolvedMemory => m !== undefined);
  }

  /** Simple text similarity (Jaccard on words) */
  private computeSimilarity(a: string, b: string): number {
    const wordsA = new Set(a.toLowerCase().split(/\s+/));
    const wordsB = new Set(b.toLowerCase().split(/\s+/));
    const intersection = new Set([...wordsA].filter(w => wordsB.has(w)));
    const union = new Set([...wordsA, ...wordsB]);
    return union.size === 0 ? 0 : intersection.size / union.size;
  }
}

export default new SelfEvolvingMemoryFeature();
