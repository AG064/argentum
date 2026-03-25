/**
 * AG-Claw Configuration Loader
 *
 * Loads and validates configuration from YAML files with environment variable overrides.
 * Supports hot-reloading via chokidar file watcher.
 */

import { readFileSync, existsSync } from 'fs';
import { resolve } from 'path';

import { watch } from 'chokidar';
import { parse } from 'yaml';
import { z } from 'zod';

/** Server configuration schema */
const ServerConfigSchema = z.object({
  port: z.number().int().min(1).max(65535).default(3000),
  host: z.string().default('0.0.0.0'),
  cors: z
    .object({
      enabled: z.boolean().default(true),
      origins: z.array(z.string()).default(['*']),
    })
    .default({}),
  rateLimit: z
    .object({
      enabled: z.boolean().default(true),
      windowMs: z.number().int().default(60000),
      maxRequests: z.number().int().default(100),
    })
    .default({}),
});

/** Feature toggle schema */
const FeatureToggleSchema = z.object({
  enabled: z.boolean().default(false),
});

/** Voice feature config */
const VoiceConfigSchema = FeatureToggleSchema.extend({
  provider: z.enum(['elevenlabs', 'openai', 'local']).default('elevenlabs'),
  voice: z.string().default('default'),
  model: z.string().default('eleven_multilingual_v2'),
  sttProvider: z.enum(['whisper', 'google', 'local']).default('whisper'),
  wakeWord: z.string().optional(),
});

/** Webchat feature config */
const WebchatConfigSchema = FeatureToggleSchema.extend({
  port: z.number().int().default(3001),
  maxConnections: z.number().int().default(1000),
  messageHistory: z.number().int().default(100),
});

/** Knowledge Graph config */
const KnowledgeGraphConfigSchema = FeatureToggleSchema.extend({
  backend: z.enum(['sqlite', 'neo4j', 'memory']).default('sqlite'),
  path: z.string().default('./data/knowledge.db'),
});

/** Memory config */
const MemoryConfigSchema = z.object({
  primary: z.enum(['sqlite', 'supabase', 'markdown']).default('sqlite'),
  path: z.string().default('./data/memory.db'),
  supabaseUrl: z.string().optional(),
  supabaseKey: z.string().optional(),
  selfEvolving: z.boolean().default(false),
  compressionThreshold: z.number().int().default(10000),
});

/** Security config */
const SecurityConfigSchema = z.object({
  policy: z.string().default('config/security-policy.yaml'),
  secrets: z.enum(['encrypted', 'env', 'file']).default('encrypted'),
  auditLog: z.boolean().default(true),
  allowlistMode: z.enum(['strict', 'permissive']).default('permissive'),
});

/** Multi-Agent Coordination config */
const MultiAgentCoordinationConfigSchema = FeatureToggleSchema.extend({
  dbPath: z.string().default('./data/multi-agent-coordination.db'),
  heartbeatIntervalMs: z.number().default(30_000),
  offlineTimeoutMs: z.number().default(60_000),
  maxAgents: z.number().default(100),
}).default({});

/** Role-Based Access config */
const RoleBasedAccessConfigSchema = FeatureToggleSchema.extend({
  dbPath: z.string().default('./data/role-based-access.db'),
  defaultRole: z.string().default('viewer'),
}).default({});

/** Shared Knowledge Base config */
const SharedKnowledgeBaseConfigSchema = FeatureToggleSchema.extend({
  dbPath: z.string().default('./data/shared-knowledge-base.db'),
  maxArticles: z.number().default(10_000),
  maxVersionsPerArticle: z.number().default(20),
}).default({});

/** Health Monitoring config */
const HealthMonitoringConfigSchema = FeatureToggleSchema.extend({
  collectionIntervalMs: z.number().default(30_000),
  diskCheckPath: z.string().default('/'),
  cpuWarningThreshold: z.number().default(80),
  memoryWarningThreshold: z.number().default(80),
  diskWarningThreshold: z.number().default(80),
}).default({});

/** Auto-Update config */
const AutoUpdateConfigSchema = FeatureToggleSchema.extend({
  dbPath: z.string().default('./data/auto-update.db'),
  repoOwner: z.string().default('AG064'),
  repoName: z.string().default('ag-claw'),
  checkIntervalHours: z.number().default(24),
  autoApply: z.boolean().default(false),
  backupBeforeUpdate: z.boolean().default(true),
  backupPath: z.string().default('./data/backups'),
}).default({});

/** Cron Scheduler config */
const CronSchedulerConfigSchema = FeatureToggleSchema.extend({
  dbPath: z.string().default('./data/cron-scheduler.db'),
  timezone: z.string().optional(),
  maxJobs: z.number().default(500),
}).default({});

/** Model routing scoring weights configuration */
export const ModelRoutingWeightsSchema = z.object({
  costEfficiency: z.number().min(0).max(3).default(1.2),
  latency: z.number().min(0).max(3).default(1.0),
  capabilityMatch: z.number().min(0).max(3).default(1.5),
  contextLengthFit: z.number().min(0).max(3).default(0.8),
  toolSupport: z.number().min(0).max(3).default(1.3),
  recentSuccessRate: z.number().min(0).max(3).default(1.4),
  tokenEfficiency: z.number().min(0).max(3).default(0.7),
  specializationMatch: z.number().min(0).max(3).default(1.1),
  reliability: z.number().min(0).max(3).default(1.3),
  throughput: z.number().min(0).max(3).default(0.6),
  customWeight: z.number().min(0).max(3).default(1.0),
}).default({});

/** Model routing configuration schema */
export const ModelRoutingConfigSchema = z.object({
  enabled: z.boolean().default(true),
  cacheScoresMs: z.number().int().min(1000).max(600000).default(60000),
  weights: ModelRoutingWeightsSchema.default({}),
  models: z.array(z.object({
    modelId: z.string(),
    costPer1K: z.number(),
    latency: z.number(),
    capabilities: z.array(z.string()),
    contextLength: z.number().optional(),
    toolSupport: z.boolean().optional(),
    reliability: z.number().optional(),
    specialization: z.array(z.string()).optional(),
    throughput: z.number().optional(),
  })).optional(),
}).default({});

/** LLM provider configuration schema */
export const LLMProviderConfigSchema = z.object({
  base_url: z.string().url(),
  api_key: z.string().optional(),
  api_key_env: z.string().optional(),
  api: z.enum(['openai', 'anthropic']).default('openai'),
  models: z.array(z.string()).min(1),
  headers: z.record(z.string()).optional(),
});

/** LLM configuration schema */
export const LLMConfigSchema = z
  .object({
    providers: z.record(LLMProviderConfigSchema).default({}),
    default: z.string().default(''),
    fallback: z.array(z.string()).optional(),
  })
  .default({ providers: {}, default: '' });

/** Root configuration schema */
export const ConfigSchema = z.object({
  server: ServerConfigSchema.default({}),
  llm: LLMConfigSchema.default({}),
  modelRouting: ModelRoutingConfigSchema.default({}),
  features: z
    .object({
      'webchat': WebchatConfigSchema.default({}),
      'voice': VoiceConfigSchema.default({}),
      'knowledge-graph': KnowledgeGraphConfigSchema.default({}),
      'multimodal-memory': FeatureToggleSchema.default({}),
      'browser-automation': FeatureToggleSchema.default({}),
      'webhooks': FeatureToggleSchema.default({}),
      'mesh-workflows': FeatureToggleSchema.default({}),
      'live-canvas': FeatureToggleSchema.default({}),
      'container-sandbox': FeatureToggleSchema.default({}),
      'air-gapped': FeatureToggleSchema.default({}),
      'morning-briefing': FeatureToggleSchema.default({}),
      'evening-recap': FeatureToggleSchema.default({}),
      'smart-recommendations': FeatureToggleSchema.default({}),
      'group-management': FeatureToggleSchema.default({}),
      'budget': FeatureToggleSchema.extend({
        monthlyLimit: z.number().default(1_000_000),
        perAgentLimits: z.record(z.number()).default({}),
        alertThreshold: z.number().default(80),
        hardStop: z.boolean().default(true),
        dbPath: z.string().default('./data/budget.db'),
      }).default({}),
      'goals': FeatureToggleSchema.extend({
        dbPath: z.string().default('./data/goals.db'),
      }).default({}),
      'task-checkout': FeatureToggleSchema.extend({
        dbPath: z.string().default('./data/task-checkout.db'),
        leaseDurationMs: z.number().default(1_800_000),
        maxLeasesPerAgent: z.number().default(10),
      }).default({}),
      'company-templates': FeatureToggleSchema.extend({
        templatesPath: z.string().default('./data/templates'),
      }).default({}),
      'governance': FeatureToggleSchema.extend({
        dbPath: z.string().default('./data/governance.db'),
        autoApproveRisk: z.enum(['none', 'low', 'medium']).default('low'),
        ticketExpiryMs: z.number().default(86_400_000),
        requiredApprovers: z.number().default(1),
        approvers: z.array(z.string()).default([]),
      }).default({}),
      'multi-agent-coordination': MultiAgentCoordinationConfigSchema.default({}),
      'role-based-access': RoleBasedAccessConfigSchema.default({}),
      'shared-knowledge-base': SharedKnowledgeBaseConfigSchema.default({}),
      'health-monitoring': HealthMonitoringConfigSchema.default({}),
      'auto-update': AutoUpdateConfigSchema.default({}),
      'cron-scheduler': CronSchedulerConfigSchema.default({}),
    })
    .default({}),
  memory: MemoryConfigSchema.default({}),
  security: SecurityConfigSchema.default({}),
  channels: z
    .object({
      telegram: z
        .object({
          enabled: z.boolean().default(true),
          token: z.string().optional(),
        })
        .default({}),
      webchat: z
        .object({
          enabled: z.boolean().default(true),
        })
        .default({}),
      mobile: z
        .object({
          enabled: z.boolean().default(false),
          fcmKey: z.string().optional(),
          httpPort: z.number().default(3003),
          httpPath: z.string().default('/mobile'),
          requireAuth: z.boolean().default(false),
          authToken: z.string().optional(),
        })
        .default({}),
    })
    .default({}),
  logging: z
    .object({
      level: z.enum(['debug', 'info', 'warn', 'error']).default('info'),
      format: z.enum(['json', 'pretty']).default('pretty'),
      file: z.string().optional(),
    })
    .default({}),
});

export type AGClawConfig = z.infer<typeof ConfigSchema>;

/** Configuration manager with hot-reload support */
export class ConfigManager {
  private config: AGClawConfig;
  private configPath: string;
  private watcher: ReturnType<typeof watch> | null = null;
  private listeners: Set<(config: AGClawConfig) => void> = new Set();

  constructor(configPath?: string) {
    this.configPath = configPath ?? resolve(process.cwd(), 'config/default.yaml');
    this.config = this.loadConfig();
  }

  /** Load and validate configuration from YAML file */
  private loadConfig(): AGClawConfig {
    let fileConfig: Record<string, unknown> = {};

    if (existsSync(this.configPath)) {
      const raw = readFileSync(this.configPath, 'utf-8');
      fileConfig = parse(raw) ?? {};
    }

    // Environment variable overrides
    const envOverrides = this.loadEnvOverrides();
    const merged = this.deepMerge(fileConfig, envOverrides);

    const result = ConfigSchema.safeParse(merged);
    if (!result.success) {
      console.error('Configuration validation failed:', result.error.format());
      process.exit(1);
    }

    return result.data;
  }

  /** Load configuration overrides from environment variables */
  private loadEnvOverrides(): Record<string, unknown> {
    const overrides: Record<string, unknown> = {};

    if (process.env.AGCLAW_PORT) {
      overrides['server'] = { port: parseInt(process.env.AGCLAW_PORT, 10) };
    }
    if (process.env.AGCLAW_LOG_LEVEL) {
      overrides['logging'] = { level: process.env.AGCLAW_LOG_LEVEL };
    }
    if (process.env.AGCLAW_TELEGRAM_TOKEN) {
      overrides['channels'] = { telegram: { token: process.env.AGCLAW_TELEGRAM_TOKEN } };
    }
    if (process.env.AGCLAW_SUPABASE_URL) {
      overrides['memory'] = { supabaseUrl: process.env.AGCLAW_SUPABASE_URL };
    }
    if (process.env.AGCLAW_SUPABASE_KEY) {
      overrides['memory'] = {
        ...((overrides['memory'] as object) ?? {}),
        supabaseKey: process.env.AGCLAW_SUPABASE_KEY,
      };
    }

    return overrides;
  }

  /** Deep merge two objects */
  private deepMerge(
    target: Record<string, unknown>,
    source: Record<string, unknown>,
  ): Record<string, unknown> {
    const result = { ...target };
    for (const key of Object.keys(source)) {
      if (source[key] instanceof Object && key in target && target[key] instanceof Object) {
        result[key] = this.deepMerge(
          target[key] as Record<string, unknown>,
          source[key] as Record<string, unknown>,
        );
      } else {
        result[key] = source[key];
      }
    }
    return result;
  }

  /** Get current configuration */
  get(): AGClawConfig {
    return this.config;
  }

  /** Get a specific config section */
  getSection<K extends keyof AGClawConfig>(section: K): AGClawConfig[K] {
    return this.config[section];
  }

  /** Check if a feature is enabled */
  isFeatureEnabled(feature: keyof AGClawConfig['features']): boolean {
    return this.config.features[feature]?.enabled ?? false;
  }

  /** Enable hot-reload watching */
  enableHotReload(): void {
    if (this.watcher) return;

    this.watcher = watch(this.configPath, { ignoreInitial: true });
    this.watcher.on('change', () => {
      console.log(`[Config] Reloading ${this.configPath}`);
      this.config = this.loadConfig();
      for (const listener of this.listeners) {
        listener(this.config);
      }
    });
  }

  /** Register a listener for config changes */
  onChange(listener: (config: AGClawConfig) => void): () => void {
    this.listeners.add(listener);
    return () => this.listeners.delete(listener);
  }

  /** Stop watching for changes */
  dispose(): void {
    this.watcher?.close();
    this.watcher = null;
  }
}

// Singleton instance
let instance: ConfigManager | null = null;

/** Get or create the global config manager */
export function getConfig(configPath?: string): ConfigManager {
  if (!instance) {
    instance = new ConfigManager(configPath);
  }
  return instance;
}
