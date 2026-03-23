/**
 * Semantic Search Feature
 *
 * Vector-based semantic memory search using embeddings.
 * Wraps the SemanticMemory backend with a simplified API.
 */

import {
  type FeatureModule,
  type FeatureContext,
  type FeatureMeta,
  type HealthStatus,
} from '../../core/plugin-loader';
import { getSemanticMemory, type MemoryResult } from '../../memory/semantic';

/** Feature configuration */
export interface SemanticSearchConfig {
  dbPath?: string;
  defaultType?: string;
}

/**
 * SemanticSearchFeature — semantic similarity search over stored memories.
 *
 * Stores text with embeddings and supports both full-text and
 * semantic (vector) search. Ideal for finding related concepts
 * without exact keyword matches.
 */
class SemanticSearchFeature implements FeatureModule {
  readonly meta: FeatureMeta = {
    name: 'semantic-search',
    version: '0.1.0',
    description: 'Semantic vector search with embedding-based similarity',
    dependencies: [],
  };

  private config: Required<SemanticSearchConfig>;
  private ctx!: FeatureContext;
  private memory!: ReturnType<typeof getSemanticMemory>;

  constructor() {
    this.config = {
      dbPath: './data/semantic-memory.db',
      defaultType: 'semantic',
    };
  }

  async init(config: Record<string, unknown>, context: FeatureContext): Promise<void> {
    this.ctx = context;
    this.config = {
      dbPath: (config['dbPath'] as string) ?? this.config['dbPath'],
      defaultType: (config['defaultType'] as string) ?? this.config['defaultType'],
    };

    // Initialize the singleton with custom dbPath if needed
    this.memory = getSemanticMemory(this.config.dbPath);
    // The singleton ensures init() is called
  }

  async start(): Promise<void> {
    this.ctx.logger.info('SemanticSearch active', {
      dbPath: this.config.dbPath,
      defaultType: this.config.defaultType,
    });
  }

  async stop(): Promise<void> {
    this.memory.close();
    this.ctx.logger.info('SemanticSearch stopped');
  }

  async healthCheck(): Promise<HealthStatus> {
    try {
      const count = await this.memory.count();
      return {
        healthy: true,
        details: {
          totalMemories: count,
        },
      };
    } catch (err) {
      return {
        healthy: false,
        message: err instanceof Error ? err.message : String(err),
      };
    }
  }

  /** Index/store a text entry with optional metadata */
  async index(text: string, metadata?: Record<string, unknown>): Promise<string> {
    return this.memory.store(this.config.defaultType, text, metadata);
  }

  /** Semantic + full-text search */
  async search(query: string, limit = 10): Promise<Array<MemoryResult & { similarity?: number }>> {
    return this.memory.search(query, limit);
  }

  /** Delete a memory by its ID */
  async delete(docId: string): Promise<boolean> {
    return this.memory.delete(docId);
  }

  /** Get a memory by ID */
  async get(docId: string): Promise<MemoryResult | null> {
    return this.memory.getById(docId);
  }

  /** Get memories by type */
  async getByType(type: string, limit = 50): Promise<MemoryResult[]> {
    return this.memory.getByType(type, limit);
  }

  /** Consolidate memory (deduplicate, decay) */
  async consolidate(): Promise<void> {
    return this.memory.consolidate();
  }

  /** Save checkpoint for a task */
  async checkpoint(taskId: string, state: unknown): Promise<void> {
    return this.memory.checkpoint(taskId, state);
  }

  /** Resume a checkpointed task */
  async resume(taskId: string): Promise<unknown> {
    return this.memory.resume(taskId);
  }
}

export default new SemanticSearchFeature();
