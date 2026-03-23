'use strict';
/**
 * AG-Claw Configuration Loader
 *
 * Loads and validates configuration from YAML files with environment variable overrides.
 * Supports hot-reloading via chokidar file watcher.
 */
Object.defineProperty(exports, '__esModule', { value: true });
exports.ConfigManager = exports.ConfigSchema = void 0;
exports.getConfig = getConfig;
const fs_1 = require('fs');
const yaml_1 = require('yaml');
const path_1 = require('path');
const zod_1 = require('zod');
const chokidar_1 = require('chokidar');
/** Server configuration schema */
const ServerConfigSchema = zod_1.z.object({
  port: zod_1.z.number().int().min(1).max(65535).default(3000),
  host: zod_1.z.string().default('0.0.0.0'),
  cors: zod_1.z
    .object({
      enabled: zod_1.z.boolean().default(true),
      origins: zod_1.z.array(zod_1.z.string()).default(['*']),
    })
    .default({}),
  rateLimit: zod_1.z
    .object({
      enabled: zod_1.z.boolean().default(true),
      windowMs: zod_1.z.number().int().default(60000),
      maxRequests: zod_1.z.number().int().default(100),
    })
    .default({}),
});
/** Feature toggle schema */
const FeatureToggleSchema = zod_1.z.object({
  enabled: zod_1.z.boolean().default(false),
});
/** Voice feature config */
const VoiceConfigSchema = FeatureToggleSchema.extend({
  provider: zod_1.z.enum(['elevenlabs', 'openai', 'local']).default('elevenlabs'),
  voice: zod_1.z.string().default('default'),
  model: zod_1.z.string().default('eleven_multilingual_v2'),
  sttProvider: zod_1.z.enum(['whisper', 'google', 'local']).default('whisper'),
  wakeWord: zod_1.z.string().optional(),
});
/** Webchat feature config */
const WebchatConfigSchema = FeatureToggleSchema.extend({
  port: zod_1.z.number().int().default(3001),
  maxConnections: zod_1.z.number().int().default(1000),
  messageHistory: zod_1.z.number().int().default(100),
});
/** Knowledge Graph config */
const KnowledgeGraphConfigSchema = FeatureToggleSchema.extend({
  backend: zod_1.z.enum(['sqlite', 'neo4j', 'memory']).default('sqlite'),
  path: zod_1.z.string().default('./data/knowledge.db'),
});
/** Memory config */
const MemoryConfigSchema = zod_1.z.object({
  primary: zod_1.z.enum(['sqlite', 'supabase', 'markdown']).default('sqlite'),
  path: zod_1.z.string().default('./data/memory.db'),
  supabaseUrl: zod_1.z.string().optional(),
  supabaseKey: zod_1.z.string().optional(),
  selfEvolving: zod_1.z.boolean().default(false),
  compressionThreshold: zod_1.z.number().int().default(10000),
});
/** Security config */
const SecurityConfigSchema = zod_1.z.object({
  policy: zod_1.z.string().default('config/security-policy.yaml'),
  secrets: zod_1.z.enum(['encrypted', 'env', 'file']).default('encrypted'),
  auditLog: zod_1.z.boolean().default(true),
  allowlistMode: zod_1.z.enum(['strict', 'permissive']).default('permissive'),
});
/** Root configuration schema */
exports.ConfigSchema = zod_1.z.object({
  server: ServerConfigSchema.default({}),
  features: zod_1.z
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
        monthlyLimit: zod_1.z.number().default(1000000),
        perAgentLimits: zod_1.z.record(zod_1.z.number()).default({}),
        alertThreshold: zod_1.z.number().default(80),
        hardStop: zod_1.z.boolean().default(true),
        dbPath: zod_1.z.string().default('./data/budget.db'),
      }).default({}),
      'goals': FeatureToggleSchema.extend({
        dbPath: zod_1.z.string().default('./data/goals.db'),
      }).default({}),
      'task-checkout': FeatureToggleSchema.extend({
        dbPath: zod_1.z.string().default('./data/task-checkout.db'),
        leaseDurationMs: zod_1.z.number().default(1800000),
        maxLeasesPerAgent: zod_1.z.number().default(10),
      }).default({}),
      'company-templates': FeatureToggleSchema.extend({
        templatesPath: zod_1.z.string().default('./data/templates'),
      }).default({}),
      'governance': FeatureToggleSchema.extend({
        dbPath: zod_1.z.string().default('./data/governance.db'),
        autoApproveRisk: zod_1.z.enum(['none', 'low', 'medium']).default('low'),
        ticketExpiryMs: zod_1.z.number().default(86400000),
        requiredApprovers: zod_1.z.number().default(1),
        approvers: zod_1.z.array(zod_1.z.string()).default([]),
      }).default({}),
    })
    .default({}),
  memory: MemoryConfigSchema.default({}),
  security: SecurityConfigSchema.default({}),
  channels: zod_1.z
    .object({
      telegram: zod_1.z
        .object({
          enabled: zod_1.z.boolean().default(true),
          token: zod_1.z.string().optional(),
        })
        .default({}),
      webchat: zod_1.z
        .object({
          enabled: zod_1.z.boolean().default(true),
        })
        .default({}),
      mobile: zod_1.z
        .object({
          enabled: zod_1.z.boolean().default(false),
          fcmKey: zod_1.z.string().optional(),
          httpPort: zod_1.z.number().default(3003),
          httpPath: zod_1.z.string().default('/mobile'),
          requireAuth: zod_1.z.boolean().default(false),
          authToken: zod_1.z.string().optional(),
        })
        .default({}),
    })
    .default({}),
  logging: zod_1.z
    .object({
      level: zod_1.z.enum(['debug', 'info', 'warn', 'error']).default('info'),
      format: zod_1.z.enum(['json', 'pretty']).default('pretty'),
      file: zod_1.z.string().optional(),
    })
    .default({}),
});
/** Configuration manager with hot-reload support */
class ConfigManager {
  constructor(configPath) {
    this.watcher = null;
    this.listeners = new Set();
    this.configPath = configPath ?? (0, path_1.resolve)(process.cwd(), 'config/default.yaml');
    this.config = this.loadConfig();
  }
  /** Load and validate configuration from YAML file */
  loadConfig() {
    let fileConfig = {};
    if ((0, fs_1.existsSync)(this.configPath)) {
      const raw = (0, fs_1.readFileSync)(this.configPath, 'utf-8');
      fileConfig = (0, yaml_1.parse)(raw) ?? {};
    }
    // Environment variable overrides
    const envOverrides = this.loadEnvOverrides();
    const merged = this.deepMerge(fileConfig, envOverrides);
    const result = exports.ConfigSchema.safeParse(merged);
    if (!result.success) {
      console.error('Configuration validation failed:', result.error.format());
      process.exit(1);
    }
    return result.data;
  }
  /** Load configuration overrides from environment variables */
  loadEnvOverrides() {
    const overrides = {};
    if (process.env.AGCLAW_PORT) {
      overrides.server = { port: parseInt(process.env.AGCLAW_PORT, 10) };
    }
    if (process.env.AGCLAW_LOG_LEVEL) {
      overrides.logging = { level: process.env.AGCLAW_LOG_LEVEL };
    }
    if (process.env.AGCLAW_TELEGRAM_TOKEN) {
      overrides.channels = { telegram: { token: process.env.AGCLAW_TELEGRAM_TOKEN } };
    }
    if (process.env.AGCLAW_SUPABASE_URL) {
      overrides.memory = { supabaseUrl: process.env.AGCLAW_SUPABASE_URL };
    }
    if (process.env.AGCLAW_SUPABASE_KEY) {
      overrides.memory = {
        ...(overrides.memory ?? {}),
        supabaseKey: process.env.AGCLAW_SUPABASE_KEY,
      };
    }
    return overrides;
  }
  /** Deep merge two objects */
  deepMerge(target, source) {
    const result = { ...target };
    for (const key of Object.keys(source)) {
      if (source[key] instanceof Object && key in target && target[key] instanceof Object) {
        result[key] = this.deepMerge(target[key], source[key]);
      } else {
        result[key] = source[key];
      }
    }
    return result;
  }
  /** Get current configuration */
  get() {
    return this.config;
  }
  /** Get a specific config section */
  getSection(section) {
    return this.config[section];
  }
  /** Check if a feature is enabled */
  isFeatureEnabled(feature) {
    return this.config.features[feature]?.enabled ?? false;
  }
  /** Enable hot-reload watching */
  enableHotReload() {
    if (this.watcher) return;
    this.watcher = (0, chokidar_1.watch)(this.configPath, { ignoreInitial: true });
    this.watcher.on('change', () => {
      console.log(`[Config] Reloading ${this.configPath}`);
      this.config = this.loadConfig();
      for (const listener of this.listeners) {
        listener(this.config);
      }
    });
  }
  /** Register a listener for config changes */
  onChange(listener) {
    this.listeners.add(listener);
    return () => this.listeners.delete(listener);
  }
  /** Stop watching for changes */
  dispose() {
    this.watcher?.close();
    this.watcher = null;
  }
}
exports.ConfigManager = ConfigManager;
// Singleton instance
let instance = null;
/** Get or create the global config manager */
function getConfig(configPath) {
  if (!instance) {
    instance = new ConfigManager(configPath);
  }
  return instance;
}
