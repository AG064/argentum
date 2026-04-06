/**
 * mem0 Memory Feature
 *
 * Adaptive memory layer powered by Mem0 — learns user preferences over time.
 * Provides semantic memory storage and retrieval with +26% accuracy vs baseline.
 */

import { MemoryClient, type Message } from 'mem0ai';
import {
  type FeatureModule,
  type FeatureContext,
  type FeatureMeta,
  type HealthStatus,
} from '../../core/plugin-loader';
import { featureLogger } from '../../core/logger';

export interface Mem0Config {
  enabled: boolean;
  apiKey?: string;
  model?: string;
  temperature?: number;
  maxTokens?: number;
  userId?: string;
}

class Mem0MemoryFeature implements FeatureModule {
  readonly meta: FeatureMeta = {
    name: 'mem0-memory',
    version: '0.1.0',
    description: 'Adaptive memory layer powered by Mem0',
    dependencies: [],
  };

  private client: MemoryClient | null = null;
  private config: Mem0Config = { enabled: false };
  private ctx!: FeatureContext;
  private logger = featureLogger('mem0-memory');

  async init(config: Record<string, unknown>, _context: FeatureContext): Promise<void> {
    this.config = {
      enabled: (config['enabled'] as boolean) ?? false,
      apiKey: config['apiKey'] as string | undefined,
      model: config['model'] as string | undefined,
      temperature: config['temperature'] as number | undefined,
      maxTokens: config['maxTokens'] as number | undefined,
      userId: config['userId'] as string | undefined,
    };

    if (!this.config.enabled) return;

    try {
      this.client = new MemoryClient({
        apiKey: this.config.apiKey || process.env.MEM0_API_KEY || '',
      });
      this.logger.info('[Mem0] Initialized', { userId: this.config.userId });
    } catch (err) {
      this.logger.error('[Mem0] Failed to initialize', { error: String(err) });
    }
  }

  async start(): Promise<void> {
    this.logger.info('Mem0MemoryFeature started', {
      userId: this.config.userId,
    });
  }

  async stop(): Promise<void> {
    this.logger.info('Mem0MemoryFeature stopped');
    this.client = null;
  }

  async healthCheck(): Promise<HealthStatus> {
    return {
      healthy: this.config.enabled && !!this.client,
      message: this.client ? 'Mem0 connected' : 'Mem0 disabled or not configured',
    };
  }

  async store(query: string, userId?: string): Promise<void> {
    if (!this.client) return;
    const messages: Message[] = [{ role: 'user', content: query }];
    await this.client.add(messages, { user_id: userId || this.config.userId });
  }

  async search(query: string, limit: number = 5): Promise<string[]> {
    if (!this.client) return [];
    const results = await this.client.search(query, {
      user_id: this.config.userId,
      limit,
    });
    return results
      .map((r) => r.memory || r.data?.memory)
      .filter((m): m is string => m !== undefined);
  }

  async getAll(userId?: string): Promise<string[]> {
    if (!this.client) return [];
    const results = await this.client.getAll({ user_id: userId || this.config.userId });
    return results
      .map((r) => r.memory || r.data?.memory)
      .filter((m): m is string => m !== undefined);
  }
}

export default new Mem0MemoryFeature();
