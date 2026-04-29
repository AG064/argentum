/**
 * Multimodal Memory Feature
 *
 * Stores and retrieves memories across text, images, audio, and documents.
 * Supports semantic search and temporal queries.
 */

import {
  type FeatureModule,
  type FeatureContext,
  type FeatureMeta,
  type HealthStatus,
} from '../../core/plugin-loader';

/** Multimodal memory configuration */
export interface MultimodalMemoryConfig {
  enabled: boolean;
  maxMemorySize: number;
  embeddingDimension: number;
  retentionDays: number;
}

/** Memory entry types */
export type MemoryType = 'text' | 'image' | 'audio' | 'document' | 'code';

/** Memory entry */
export interface MemoryEntry {
  id: string;
  type: MemoryType;
  content: string;
  metadata: Record<string, unknown>;
  embedding?: number[];
  tags: string[];
  importance: number; // 0-1
  createdAt: number;
  accessedAt: number;
  accessCount: number;
}

/** Search query */
export interface MemorySearchQuery {
  text?: string;
  type?: MemoryType;
  tags?: string[];
  startDate?: number;
  endDate?: number;
  minImportance?: number;
  limit?: number;
}

/** Search result */
export interface MemorySearchResult {
  entry: MemoryEntry;
  score: number;
}

/**
 * Multimodal Memory feature — stores and retrieves memories across modalities.
 *
 * Provides a unified memory system for text, images, audio, and documents
 * with semantic search and importance-based retention.
 */
class MultimodalMemoryFeature implements FeatureModule {
  readonly meta: FeatureMeta = {
    name: 'multimodal-memory',
    version: '0.0.4',
    description: 'Cross-modal memory storage with semantic search',
    dependencies: [],
  };

  private config: MultimodalMemoryConfig = {
    enabled: false,
    maxMemorySize: 100000,
    embeddingDimension: 1536,
    retentionDays: 365,
  };
  private ctx!: FeatureContext;
  private memories: Map<string, MemoryEntry> = new Map();

  async init(config: Record<string, unknown>, context: FeatureContext): Promise<void> {
    this.ctx = context;
    this.config = { ...this.config, ...(config as Partial<MultimodalMemoryConfig>) };
  }

  async start(): Promise<void> {
    this.ctx.logger.info('Multimodal Memory active', {
      maxSize: this.config.maxMemorySize,
      retention: `${this.config.retentionDays}d`,
    });
  }

  async stop(): Promise<void> {
    this.memories.clear();
  }

  async healthCheck(): Promise<HealthStatus> {
    const typeCounts: Record<string, number> = {};
    for (const mem of this.memories.values()) {
      typeCounts[mem.type] = (typeCounts[mem.type] ?? 0) + 1;
    }
    return {
      healthy: true,
      details: { total: this.memories.size, byType: typeCounts },
    };
  }

  /** Store a new memory */
  async store(
    type: MemoryType,
    content: string,
    metadata: Record<string, unknown> = {},
    tags: string[] = [],
    importance = 0.5,
  ): Promise<MemoryEntry> {
    const id = `mem_${Date.now()}_${Math.random().toString(36).slice(2, 8)}`;
    const entry: MemoryEntry = {
      id,
      type,
      content,
      metadata,
      tags,
      importance: Math.max(0, Math.min(1, importance)),
      createdAt: Date.now(),
      accessedAt: Date.now(),
      accessCount: 0,
    };

    // Enforce max size by evicting least important entries
    if (this.memories.size >= this.config.maxMemorySize) {
      this.evictLeastImportant();
    }

    this.memories.set(id, entry);
    this.ctx.logger.debug('Memory stored', { id, type, importance });
    return entry;
  }

  /** Retrieve a memory by ID */
  async retrieve(id: string): Promise<MemoryEntry | null> {
    const entry = this.memories.get(id);
    if (!entry) return null;

    entry.accessedAt = Date.now();
    entry.accessCount++;
    return entry;
  }

  /** Search memories */
  async search(query: MemorySearchQuery): Promise<MemorySearchResult[]> {
    const results: MemorySearchResult[] = [];
    const limit = query.limit ?? 20;

    for (const entry of this.memories.values()) {
      if (query.type && entry.type !== query.type) continue;
      if (query.minImportance && entry.importance < query.minImportance) continue;
      if (query.startDate && entry.createdAt < query.startDate) continue;
      if (query.endDate && entry.createdAt > query.endDate) continue;
      if (query.tags?.length && !query.tags.some((t) => entry.tags.includes(t))) continue;

      let score = entry.importance;
      if (query.text) {
        const textMatch = this.computeTextSimilarity(query.text, entry.content);
        score = score * 0.3 + textMatch * 0.7;
      }

      results.push({ entry, score });
    }

    results.sort((a, b) => b.score - a.score);
    return results.slice(0, limit);
  }

  /** Delete a memory */
  async delete(id: string): Promise<boolean> {
    return this.memories.delete(id);
  }

  /** Evict least important memory */
  private evictLeastImportant(): void {
    let leastImportant: MemoryEntry | null = null;
    for (const entry of this.memories.values()) {
      if (!leastImportant || entry.importance < leastImportant.importance) {
        leastImportant = entry;
      }
    }
    if (leastImportant) {
      this.memories.delete(leastImportant.id);
    }
  }

  /** Simple text similarity (Jaccard on word sets) */
  private computeTextSimilarity(a: string, b: string): number {
    const wordsA = new Set(a.toLowerCase().split(/\s+/));
    const wordsB = new Set(b.toLowerCase().split(/\s+/));
    const intersection = new Set([...wordsA].filter((w) => wordsB.has(w)));
    const union = new Set([...wordsA, ...wordsB]);
    return union.size === 0 ? 0 : intersection.size / union.size;
  }
}

export default new MultimodalMemoryFeature();
