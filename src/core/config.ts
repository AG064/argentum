/**
 * Argentum Configuration Loader
 *
 * Loads and validates configuration from YAML files with environment variable overrides.
 * Supports hot-reloading via chokidar file watcher.
 */

import { readFileSync, existsSync } from 'fs';
import { resolve } from 'path';

import type { FSWatcher } from 'chokidar';
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
    .default({ enabled: true, origins: ['*'] }),
  rateLimit: z
    .object({
      enabled: z.boolean().default(true),
      windowMs: z.number().int().default(60000),
      maxRequests: z.number().int().default(100),
    })
    .default({ enabled: true, windowMs: 60000, maxRequests: 100 }),
});

/** Feature toggle schema */
const FeatureToggleSchema = z.object({
  enabled: z.boolean().default(false),
});
const GenericFeatureConfigSchema = FeatureToggleSchema.passthrough();

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
  host: z.string().default('127.0.0.1'),
  authToken: z.string().min(16).optional(),
  requireAuth: z.boolean().default(true),
  maxConnections: z.number().int().default(1000),
  messageHistory: z.number().int().default(100),
  maxMessageLength: z.number().int().min(1).max(100_000).default(10_000),
  maxPayloadBytes: z.number().int().min(1024).default(1024 * 1024),
  maxFileSize: z.number().int().min(1).default(10 * 1024 * 1024),
  rateLimitWindowMs: z.number().int().min(1000).default(60_000),
  maxMessagesPerWindow: z.number().int().min(1).default(60),
  allowedFileTypes: z
    .array(z.string())
    .default(['image/*', 'text/*', 'application/pdf', 'application/json']),
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
  allowlistMode: z.enum(['strict', 'permissive']).default('strict'),
});

/** Multi-Agent Coordination config */
const MultiAgentCoordinationConfigSchema = FeatureToggleSchema.extend({
  dbPath: z.string().default('./data/multi-agent-coordination.db'),
  heartbeatIntervalMs: z.number().default(30_000),
  offlineTimeoutMs: z.number().default(60_000),
  maxAgents: z.number().default(100),
});

/** Role-Based Access config */
const RoleBasedAccessConfigSchema = FeatureToggleSchema.extend({
  dbPath: z.string().default('./data/role-based-access.db'),
  defaultRole: z.string().default('viewer'),
});

/** Shared Knowledge Base config */
const SharedKnowledgeBaseConfigSchema = FeatureToggleSchema.extend({
  dbPath: z.string().default('./data/shared-knowledge-base.db'),
  maxArticles: z.number().default(10_000),
  maxVersionsPerArticle: z.number().default(20),
});

/** Health Monitoring config */
const HealthMonitoringConfigSchema = FeatureToggleSchema.extend({
  collectionIntervalMs: z.number().default(30_000),
  diskCheckPath: z.string().default('/'),
  cpuWarningThreshold: z.number().default(80),
  memoryWarningThreshold: z.number().default(80),
  diskWarningThreshold: z.number().default(80),
});

/** Auto-Update config */
const AutoUpdateConfigSchema = FeatureToggleSchema.extend({
  dbPath: z.string().default('./data/auto-update.db'),
  repoOwner: z.string().default('AG064'),
  repoName: z.string().default('ag-claw'),
  checkIntervalHours: z.number().default(24),
  autoApply: z.boolean().default(false),
  backupBeforeUpdate: z.boolean().default(true),
  backupPath: z.string().default('./data/backups'),
});

/** Cron Scheduler config */
const CronSchedulerConfigSchema = FeatureToggleSchema.extend({
  dbPath: z.string().default('./data/cron-scheduler.db'),
  timezone: z.string().optional(),
  maxJobs: z.number().default(500),
});

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
});

/** Model routing configuration schema */
export const ModelRoutingConfigSchema = z.object({
  enabled: z.boolean().default(true),
  cacheScoresMs: z.number().int().min(1000).max(600000).default(60000),
  weights: ModelRoutingWeightsSchema.default({
    costEfficiency: 1.2,
    latency: 1.0,
    capabilityMatch: 1.5,
    contextLengthFit: 0.8,
    toolSupport: 1.3,
    recentSuccessRate: 1.4,
    tokenEfficiency: 0.7,
    specializationMatch: 1.1,
    reliability: 1.3,
    throughput: 0.6,
    customWeight: 1.0,
  }),
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
});

/** LLM provider configuration schema */
export const LLMProviderConfigSchema = z.object({
  base_url: z.string().url(),
  api_key: z.string().optional(),
  api_key_env: z.string().optional(),
  api: z.enum(['openai', 'anthropic']).default('openai'),
  models: z.array(z.string()).min(1),
  headers: z.record(z.string(), z.string()).optional(),
});

/** LLM configuration schema */
export const LLMConfigSchema = z
  .object({
    providers: z.record(z.string(), LLMProviderConfigSchema).default({}),
    default: z.string().default(''),
    fallback: z.array(z.string()).optional(),
  })
  .default({ providers: {}, default: '' });

/** Root configuration schema */
export const ConfigSchema = z.object({
  server: ServerConfigSchema.default({
    port: 3000,
    host: '0.0.0.0',
    cors: { enabled: true, origins: ['*'] },
    rateLimit: { enabled: true, windowMs: 60000, maxRequests: 100 },
  }),
  llm: LLMConfigSchema,
  modelRouting: ModelRoutingConfigSchema.default({
    enabled: true,
    cacheScoresMs: 60000,
    weights: {
      costEfficiency: 1.2,
      latency: 1.0,
      capabilityMatch: 1.5,
      contextLengthFit: 0.8,
      toolSupport: 1.3,
      recentSuccessRate: 1.4,
      tokenEfficiency: 0.7,
      specializationMatch: 1.1,
      reliability: 1.3,
      throughput: 0.6,
      customWeight: 1.0,
    },
  }),
  features: z
    .object({
      'webchat': WebchatConfigSchema.default({
        enabled: false,
        port: 3001,
        host: '127.0.0.1',
        requireAuth: true,
        maxConnections: 1000,
        messageHistory: 100,
        maxMessageLength: 10_000,
        maxPayloadBytes: 1024 * 1024,
        maxFileSize: 10 * 1024 * 1024,
        rateLimitWindowMs: 60_000,
        maxMessagesPerWindow: 60,
        allowedFileTypes: ['image/*', 'text/*', 'application/pdf', 'application/json'],
      }),
      'voice': VoiceConfigSchema.default({
        enabled: false,
        provider: 'elevenlabs',
        voice: 'default',
        model: 'eleven_multilingual_v2',
        sttProvider: 'whisper',
      }),
      'knowledge-graph': KnowledgeGraphConfigSchema.default({
        enabled: false,
        backend: 'sqlite',
        path: './data/knowledge.db',
      }),
      'multimodal-memory': FeatureToggleSchema.default({ enabled: false }),
      'browser-automation': FeatureToggleSchema.default({ enabled: false }),
      'webhooks': FeatureToggleSchema.default({ enabled: false }),
      'mesh-workflows': FeatureToggleSchema.default({ enabled: false }),
      'live-canvas': FeatureToggleSchema.default({ enabled: false }),
      'container-sandbox': FeatureToggleSchema.default({ enabled: false }),
      'air-gapped': FeatureToggleSchema.default({ enabled: false }),
      'morning-briefing': FeatureToggleSchema.default({ enabled: false }),
      'evening-recap': FeatureToggleSchema.default({ enabled: false }),
      'smart-recommendations': FeatureToggleSchema.default({ enabled: false }),
      'group-management': FeatureToggleSchema.default({ enabled: false }),
      'budget': FeatureToggleSchema.extend({
        monthlyLimit: z.number().default(1_000_000),
        perAgentLimits: z.record(z.string(), z.number()).default({}),
        alertThreshold: z.number().default(80),
        hardStop: z.boolean().default(true),
        dbPath: z.string().default('./data/budget.db'),
      }).default({
        enabled: false,
        monthlyLimit: 1_000_000,
        perAgentLimits: {},
        alertThreshold: 80,
        hardStop: true,
        dbPath: './data/budget.db',
      }),
      'goals': FeatureToggleSchema.extend({
        dbPath: z.string().default('./data/goals.db'),
      }).default({ enabled: false, dbPath: './data/goals.db' }),
      'task-checkout': FeatureToggleSchema.extend({
        dbPath: z.string().default('./data/task-checkout.db'),
        leaseDurationMs: z.number().default(1_800_000),
        maxLeasesPerAgent: z.number().default(10),
      }).default({
        enabled: false,
        dbPath: './data/task-checkout.db',
        leaseDurationMs: 1_800_000,
        maxLeasesPerAgent: 10,
      }),
      'company-templates': FeatureToggleSchema.extend({
        templatesPath: z.string().default('./data/templates'),
      }).default({ enabled: false, templatesPath: './data/templates' }),
      'governance': FeatureToggleSchema.extend({
        dbPath: z.string().default('./data/governance.db'),
        autoApproveRisk: z.enum(['none', 'low', 'medium']).default('low'),
        ticketExpiryMs: z.number().default(86_400_000),
        requiredApprovers: z.number().default(1),
        approvers: z.array(z.string()).default([]),
      }).default({
        enabled: false,
        dbPath: './data/governance.db',
        autoApproveRisk: 'low',
        ticketExpiryMs: 86_400_000,
        requiredApprovers: 1,
        approvers: [],
      }),
      'multi-agent-coordination': MultiAgentCoordinationConfigSchema.default({
        enabled: false,
        dbPath: './data/multi-agent-coordination.db',
        heartbeatIntervalMs: 30_000,
        offlineTimeoutMs: 60_000,
        maxAgents: 100,
      }),
      'role-based-access': RoleBasedAccessConfigSchema.default({
        enabled: false,
        dbPath: './data/role-based-access.db',
        defaultRole: 'viewer',
      }),
      'shared-knowledge-base': SharedKnowledgeBaseConfigSchema.default({
        enabled: false,
        dbPath: './data/shared-knowledge-base.db',
        maxArticles: 10_000,
        maxVersionsPerArticle: 20,
      }),
      'health-monitoring': HealthMonitoringConfigSchema.default({
        enabled: false,
        collectionIntervalMs: 30_000,
        diskCheckPath: '/',
        cpuWarningThreshold: 80,
        memoryWarningThreshold: 80,
        diskWarningThreshold: 80,
      }),
      'auto-update': AutoUpdateConfigSchema.default({
        enabled: false,
        dbPath: './data/auto-update.db',
        repoOwner: 'AG064',
        repoName: 'ag-claw',
        checkIntervalHours: 24,
        autoApply: false,
        backupBeforeUpdate: true,
        backupPath: './data/backups',
      }),
      'cron-scheduler': CronSchedulerConfigSchema.default({
        enabled: false,
        dbPath: './data/cron-scheduler.db',
        maxJobs: 500,
      }),
    })
    .catchall(GenericFeatureConfigSchema)
    .default({
      'webchat': {
        enabled: false,
        port: 3001,
        host: '127.0.0.1',
        requireAuth: true,
        maxConnections: 1000,
        messageHistory: 100,
        maxMessageLength: 10_000,
        maxPayloadBytes: 1024 * 1024,
        maxFileSize: 10 * 1024 * 1024,
        rateLimitWindowMs: 60_000,
        maxMessagesPerWindow: 60,
        allowedFileTypes: ['image/*', 'text/*', 'application/pdf', 'application/json'],
      },
      'voice': {
        enabled: false,
        provider: 'elevenlabs',
        voice: 'default',
        model: 'eleven_multilingual_v2',
        sttProvider: 'whisper',
      },
      'knowledge-graph': {
        enabled: false,
        backend: 'sqlite',
        path: './data/knowledge.db',
      },
      'multimodal-memory': { enabled: false },
      'browser-automation': { enabled: false },
      'webhooks': { enabled: false },
      'mesh-workflows': { enabled: false },
      'live-canvas': { enabled: false },
      'container-sandbox': { enabled: false },
      'air-gapped': { enabled: false },
      'morning-briefing': { enabled: false },
      'evening-recap': { enabled: false },
      'smart-recommendations': { enabled: false },
      'group-management': { enabled: false },
      'budget': {
        enabled: false,
        monthlyLimit: 1_000_000,
        perAgentLimits: {},
        alertThreshold: 80,
        hardStop: true,
        dbPath: './data/budget.db',
      },
      'goals': { enabled: false, dbPath: './data/goals.db' },
      'task-checkout': {
        enabled: false,
        dbPath: './data/task-checkout.db',
        leaseDurationMs: 1_800_000,
        maxLeasesPerAgent: 10,
      },
      'company-templates': { enabled: false, templatesPath: './data/templates' },
      'governance': {
        enabled: false,
        dbPath: './data/governance.db',
        autoApproveRisk: 'low',
        ticketExpiryMs: 86_400_000,
        requiredApprovers: 1,
        approvers: [],
      },
      'multi-agent-coordination': {
        enabled: false,
        dbPath: './data/multi-agent-coordination.db',
        heartbeatIntervalMs: 30_000,
        offlineTimeoutMs: 60_000,
        maxAgents: 100,
      },
      'role-based-access': {
        enabled: false,
        dbPath: './data/role-based-access.db',
        defaultRole: 'viewer',
      },
      'shared-knowledge-base': {
        enabled: false,
        dbPath: './data/shared-knowledge-base.db',
        maxArticles: 10_000,
        maxVersionsPerArticle: 20,
      },
      'health-monitoring': {
        enabled: false,
        collectionIntervalMs: 30_000,
        diskCheckPath: '/',
        cpuWarningThreshold: 80,
        memoryWarningThreshold: 80,
        diskWarningThreshold: 80,
      },
      'auto-update': {
        enabled: false,
        dbPath: './data/auto-update.db',
        repoOwner: 'AG064',
        repoName: 'ag-claw',
        checkIntervalHours: 24,
        autoApply: false,
        backupBeforeUpdate: true,
        backupPath: './data/backups',
      },
      'cron-scheduler': {
        enabled: false,
        dbPath: './data/cron-scheduler.db',
        maxJobs: 500,
      },
    }),
  memory: MemoryConfigSchema.default({
    primary: 'sqlite',
    path: './data/memory.db',
    selfEvolving: false,
    compressionThreshold: 10000,
  }),
  security: SecurityConfigSchema.default({
    policy: 'config/security-policy.yaml',
    secrets: 'encrypted',
    auditLog: true,
    allowlistMode: 'strict',
  }),
  channels: z
    .object({
      telegram: z
        .object({
          enabled: z.boolean().default(false),
          token: z.string().optional(),
          allowedUsers: z.array(z.number()).default([]),
          allowedChats: z.array(z.number()).default([]),
          allowAll: z.boolean().default(false),
        })
        .default({ enabled: false, allowedUsers: [], allowedChats: [], allowAll: false }),
      webchat: z
        .object({
          enabled: z.boolean().default(false),
          authToken: z.string().min(16).optional(),
        })
        .default({ enabled: false }),
      mobile: z
        .object({
          enabled: z.boolean().default(false),
          fcmKey: z.string().optional(),
          httpPort: z.number().default(3003),
          httpPath: z.string().default('/mobile'),
          requireAuth: z.boolean().default(false),
          authToken: z.string().optional(),
        })
        .default({
          enabled: false,
          httpPort: 3003,
          httpPath: '/mobile',
          requireAuth: false,
        }),
    })
    .default({
      telegram: { enabled: false, allowedUsers: [], allowedChats: [], allowAll: false },
      webchat: { enabled: false },
      mobile: {
        enabled: false,
        httpPort: 3003,
        httpPath: '/mobile',
        requireAuth: false,
      },
    }),
  logging: z
    .object({
      level: z.enum(['debug', 'info', 'warn', 'error']).default('info'),
      format: z.enum(['json', 'pretty']).default('pretty'),
      file: z.string().optional(),
    })
    .default({ level: 'info', format: 'pretty' }),
});

export type ArgentumConfig = z.infer<typeof ConfigSchema>;

/** Configuration manager with hot-reload support */
export class ConfigManager {
  private config: ArgentumConfig;
  private baseConfigPath: string;
  private configPath: string;
  private watcher: FSWatcher | null = null;
  private listeners: Set<(config: ArgentumConfig) => void> = new Set();

  constructor(configPath?: string) {
    this.baseConfigPath = resolve(process.cwd(), 'config/default.yaml');
    const envConfigPath = process.env.ARGENTUM_CONFIG_PATH ?? process.env.AGCLAW_CONFIG_PATH;
    const preferredConfigPath = resolve(process.cwd(), 'argentum.json');
    const legacyConfigPath = resolve(process.cwd(), 'agclaw.json');
    this.configPath =
      configPath ??
      resolve(
        process.cwd(),
        envConfigPath ??
          (existsSync(preferredConfigPath) || !existsSync(legacyConfigPath)
            ? 'argentum.json'
            : 'agclaw.json'),
      );
    this.config = this.loadConfig();
  }

  /** Load and validate configuration from YAML file */
  private loadConfig(): ArgentumConfig {
    const baseConfig = this.loadConfigFile(this.baseConfigPath);
    const fileConfig =
      this.configPath === this.baseConfigPath ? {} : this.loadConfigFile(this.configPath);

    // Environment variable overrides
    const envOverrides = this.loadEnvOverrides();
    const merged = this.deepMerge(this.deepMerge(baseConfig, fileConfig), envOverrides);

    const result = ConfigSchema.safeParse(merged);
    if (!result.success) {
      console.error('Configuration validation failed:', result.error.format());
      process.exit(1);
    }

    return result.data;
  }

  /** Load a single configuration file if it exists */
  private loadConfigFile(filePath: string): Record<string, unknown> {
    if (!existsSync(filePath)) {
      return {};
    }

    const raw = readFileSync(filePath, 'utf-8');
    const parsed = parse(raw);

    if (parsed && typeof parsed === 'object' && !Array.isArray(parsed)) {
      return parsed as Record<string, unknown>;
    }

    return {};
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
    if (process.env.AGCLAW_TELEGRAM_ENABLED) {
      overrides['channels'] = {
        ...((overrides['channels'] as object) ?? {}),
        telegram: {
          ...(((overrides['channels'] as Record<string, object> | undefined)?.['telegram']) ?? {}),
          enabled: process.env.AGCLAW_TELEGRAM_ENABLED === 'true',
        },
      };
    }
    if (process.env.AGCLAW_WEBCHAT_ENABLED) {
      overrides['channels'] = {
        ...((overrides['channels'] as object) ?? {}),
        webchat: { enabled: process.env.AGCLAW_WEBCHAT_ENABLED === 'true' },
      };
      overrides['features'] = {
        ...((overrides['features'] as object) ?? {}),
        webchat: { enabled: process.env.AGCLAW_WEBCHAT_ENABLED === 'true' },
      };
    }
    if (process.env.AGCLAW_WEBCHAT_AUTH_TOKEN) {
      overrides['channels'] = {
        ...((overrides['channels'] as object) ?? {}),
        webchat: {
          ...(((overrides['channels'] as Record<string, object> | undefined)?.['webchat']) ?? {}),
          authToken: process.env.AGCLAW_WEBCHAT_AUTH_TOKEN,
        },
      };
      overrides['features'] = {
        ...((overrides['features'] as object) ?? {}),
        webchat: {
          ...(((overrides['features'] as Record<string, object> | undefined)?.['webchat']) ?? {}),
          authToken: process.env.AGCLAW_WEBCHAT_AUTH_TOKEN,
        },
      };
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
  get(): ArgentumConfig {
    return this.config;
  }

  /** Get a specific config section */
  getSection<K extends keyof ArgentumConfig>(section: K): ArgentumConfig[K] {
    return this.config[section];
  }

  /** Check if a feature is enabled */
  isFeatureEnabled(feature: keyof ArgentumConfig['features']): boolean {
    return this.config.features[feature]?.enabled ?? false;
  }

  /** Enable hot-reload watching */
  enableHotReload(): void {
    if (this.watcher) return;

    void this.startHotReloadWatcher();
  }

  private async startHotReloadWatcher(): Promise<void> {
    try {
      const chokidar = await import('chokidar');

      this.watcher = chokidar.watch(Array.from(new Set([this.baseConfigPath, this.configPath])), {
        ignoreInitial: true,
      });
      this.watcher.on('change', () => {
        console.log(`[Config] Reloading ${this.configPath}`);
        this.config = this.loadConfig();
        for (const listener of this.listeners) {
          listener(this.config);
        }
      });
    } catch (err) {
      console.error('Failed to enable configuration hot reload:', err);
    }
  }

  /** Register a listener for config changes */
  onChange(listener: (config: ArgentumConfig) => void): () => void {
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
