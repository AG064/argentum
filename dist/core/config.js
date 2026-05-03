"use strict";
/**
 * Argentum Configuration Loader
 *
 * Loads and validates configuration from YAML files with environment variable overrides.
 * Supports hot-reloading via chokidar file watcher.
 */
var __createBinding = (this && this.__createBinding) || (Object.create ? (function(o, m, k, k2) {
    if (k2 === undefined) k2 = k;
    var desc = Object.getOwnPropertyDescriptor(m, k);
    if (!desc || ("get" in desc ? !m.__esModule : desc.writable || desc.configurable)) {
      desc = { enumerable: true, get: function() { return m[k]; } };
    }
    Object.defineProperty(o, k2, desc);
}) : (function(o, m, k, k2) {
    if (k2 === undefined) k2 = k;
    o[k2] = m[k];
}));
var __setModuleDefault = (this && this.__setModuleDefault) || (Object.create ? (function(o, v) {
    Object.defineProperty(o, "default", { enumerable: true, value: v });
}) : function(o, v) {
    o["default"] = v;
});
var __importStar = (this && this.__importStar) || (function () {
    var ownKeys = function(o) {
        ownKeys = Object.getOwnPropertyNames || function (o) {
            var ar = [];
            for (var k in o) if (Object.prototype.hasOwnProperty.call(o, k)) ar[ar.length] = k;
            return ar;
        };
        return ownKeys(o);
    };
    return function (mod) {
        if (mod && mod.__esModule) return mod;
        var result = {};
        if (mod != null) for (var k = ownKeys(mod), i = 0; i < k.length; i++) if (k[i] !== "default") __createBinding(result, mod, k[i]);
        __setModuleDefault(result, mod);
        return result;
    };
})();
Object.defineProperty(exports, "__esModule", { value: true });
exports.ConfigManager = exports.ConfigSchema = exports.LLMConfigSchema = exports.LLMProviderConfigSchema = exports.ModelRoutingConfigSchema = exports.ModelRoutingWeightsSchema = void 0;
exports.getConfig = getConfig;
const fs_1 = require("fs");
const path_1 = require("path");
const yaml_1 = require("yaml");
const zod_1 = require("zod");
/** Server configuration schema */
const ServerConfigSchema = zod_1.z.object({
    port: zod_1.z.number().int().min(1).max(65535).default(3000),
    host: zod_1.z.string().default('0.0.0.0'),
    cors: zod_1.z
        .object({
        enabled: zod_1.z.boolean().default(true),
        origins: zod_1.z.array(zod_1.z.string()).default(['*']),
    })
        .default({ enabled: true, origins: ['*'] }),
    rateLimit: zod_1.z
        .object({
        enabled: zod_1.z.boolean().default(true),
        windowMs: zod_1.z.number().int().default(60000),
        maxRequests: zod_1.z.number().int().default(100),
    })
        .default({ enabled: true, windowMs: 60000, maxRequests: 100 }),
});
/** Feature toggle schema */
const FeatureToggleSchema = zod_1.z.object({
    enabled: zod_1.z.boolean().default(false),
});
const GenericFeatureConfigSchema = FeatureToggleSchema.passthrough();
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
    host: zod_1.z.string().default('127.0.0.1'),
    authToken: zod_1.z.string().min(16).optional(),
    requireAuth: zod_1.z.boolean().default(true),
    maxConnections: zod_1.z.number().int().default(1000),
    messageHistory: zod_1.z.number().int().default(100),
    maxMessageLength: zod_1.z.number().int().min(1).max(100000).default(10000),
    maxPayloadBytes: zod_1.z.number().int().min(1024).default(1024 * 1024),
    maxFileSize: zod_1.z.number().int().min(1).default(10 * 1024 * 1024),
    rateLimitWindowMs: zod_1.z.number().int().min(1000).default(60000),
    maxMessagesPerWindow: zod_1.z.number().int().min(1).default(60),
    allowedFileTypes: zod_1.z
        .array(zod_1.z.string())
        .default(['image/*', 'text/*', 'application/pdf', 'application/json']),
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
/** Capability sandbox defaults */
const CapabilitySecuritySchema = zod_1.z
    .object({
    defaultProfile: zod_1.z
        .enum(['restricted', 'ask-every-time', 'session-grant', 'trusted'])
        .default('restricted'),
    workspaceRoot: zod_1.z.string().default('.'),
    auditPath: zod_1.z.string().default('./data/audit/capabilities.log'),
})
    .default({
    defaultProfile: 'restricted',
    workspaceRoot: '.',
    auditPath: './data/audit/capabilities.log',
});
/** Security config */
const SecurityConfigSchema = zod_1.z.object({
    policy: zod_1.z.string().default('config/security-policy.yaml'),
    secrets: zod_1.z.enum(['encrypted', 'env', 'file']).default('encrypted'),
    auditLog: zod_1.z.boolean().default(true),
    allowlistMode: zod_1.z.enum(['strict', 'permissive']).default('strict'),
    capabilities: CapabilitySecuritySchema,
});
/** Multi-Agent Coordination config */
const MultiAgentCoordinationConfigSchema = FeatureToggleSchema.extend({
    dbPath: zod_1.z.string().default('./data/multi-agent-coordination.db'),
    heartbeatIntervalMs: zod_1.z.number().default(30000),
    offlineTimeoutMs: zod_1.z.number().default(60000),
    maxAgents: zod_1.z.number().default(100),
});
/** Role-Based Access config */
const RoleBasedAccessConfigSchema = FeatureToggleSchema.extend({
    dbPath: zod_1.z.string().default('./data/role-based-access.db'),
    defaultRole: zod_1.z.string().default('viewer'),
});
/** Shared Knowledge Base config */
const SharedKnowledgeBaseConfigSchema = FeatureToggleSchema.extend({
    dbPath: zod_1.z.string().default('./data/shared-knowledge-base.db'),
    maxArticles: zod_1.z.number().default(10000),
    maxVersionsPerArticle: zod_1.z.number().default(20),
});
/** Health Monitoring config */
const HealthMonitoringConfigSchema = FeatureToggleSchema.extend({
    collectionIntervalMs: zod_1.z.number().default(30000),
    diskCheckPath: zod_1.z.string().default('/'),
    cpuWarningThreshold: zod_1.z.number().default(80),
    memoryWarningThreshold: zod_1.z.number().default(80),
    diskWarningThreshold: zod_1.z.number().default(80),
});
/** Auto-Update config */
const AutoUpdateConfigSchema = FeatureToggleSchema.extend({
    dbPath: zod_1.z.string().default('./data/auto-update.db'),
    repoOwner: zod_1.z.string().default('AG064'),
    repoName: zod_1.z.string().default('argentum'),
    checkIntervalHours: zod_1.z.number().default(24),
    autoApply: zod_1.z.boolean().default(false),
    backupBeforeUpdate: zod_1.z.boolean().default(true),
    backupPath: zod_1.z.string().default('./data/backups'),
});
/** Cron Scheduler config */
const CronSchedulerConfigSchema = FeatureToggleSchema.extend({
    dbPath: zod_1.z.string().default('./data/cron-scheduler.db'),
    timezone: zod_1.z.string().optional(),
    maxJobs: zod_1.z.number().default(500),
});
/** Model routing scoring weights configuration */
exports.ModelRoutingWeightsSchema = zod_1.z.object({
    costEfficiency: zod_1.z.number().min(0).max(3).default(1.2),
    latency: zod_1.z.number().min(0).max(3).default(1.0),
    capabilityMatch: zod_1.z.number().min(0).max(3).default(1.5),
    contextLengthFit: zod_1.z.number().min(0).max(3).default(0.8),
    toolSupport: zod_1.z.number().min(0).max(3).default(1.3),
    recentSuccessRate: zod_1.z.number().min(0).max(3).default(1.4),
    tokenEfficiency: zod_1.z.number().min(0).max(3).default(0.7),
    specializationMatch: zod_1.z.number().min(0).max(3).default(1.1),
    reliability: zod_1.z.number().min(0).max(3).default(1.3),
    throughput: zod_1.z.number().min(0).max(3).default(0.6),
    customWeight: zod_1.z.number().min(0).max(3).default(1.0),
});
/** Model routing configuration schema */
exports.ModelRoutingConfigSchema = zod_1.z.object({
    enabled: zod_1.z.boolean().default(true),
    cacheScoresMs: zod_1.z.number().int().min(1000).max(600000).default(60000),
    weights: exports.ModelRoutingWeightsSchema.default({
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
    models: zod_1.z.array(zod_1.z.object({
        modelId: zod_1.z.string(),
        costPer1K: zod_1.z.number(),
        latency: zod_1.z.number(),
        capabilities: zod_1.z.array(zod_1.z.string()),
        contextLength: zod_1.z.number().optional(),
        toolSupport: zod_1.z.boolean().optional(),
        reliability: zod_1.z.number().optional(),
        specialization: zod_1.z.array(zod_1.z.string()).optional(),
        throughput: zod_1.z.number().optional(),
    })).optional(),
});
/** LLM provider configuration schema */
exports.LLMProviderConfigSchema = zod_1.z.object({
    base_url: zod_1.z.string().url(),
    api_key: zod_1.z.string().optional(),
    api_key_env: zod_1.z.string().optional(),
    api: zod_1.z.enum(['openai', 'anthropic']).default('openai'),
    models: zod_1.z.array(zod_1.z.string()).min(1),
    headers: zod_1.z.record(zod_1.z.string(), zod_1.z.string()).optional(),
});
/** LLM configuration schema */
exports.LLMConfigSchema = zod_1.z
    .object({
    providers: zod_1.z.record(zod_1.z.string(), exports.LLMProviderConfigSchema).default({}),
    default: zod_1.z.string().default(''),
    fallback: zod_1.z.array(zod_1.z.string()).optional(),
})
    .default({ providers: {}, default: '' });
/** Root configuration schema */
exports.ConfigSchema = zod_1.z.object({
    server: ServerConfigSchema.default({
        port: 3000,
        host: '0.0.0.0',
        cors: { enabled: true, origins: ['*'] },
        rateLimit: { enabled: true, windowMs: 60000, maxRequests: 100 },
    }),
    llm: exports.LLMConfigSchema,
    modelRouting: exports.ModelRoutingConfigSchema.default({
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
    features: zod_1.z
        .object({
        'webchat': WebchatConfigSchema.default({
            enabled: false,
            port: 3001,
            host: '127.0.0.1',
            requireAuth: true,
            maxConnections: 1000,
            messageHistory: 100,
            maxMessageLength: 10000,
            maxPayloadBytes: 1024 * 1024,
            maxFileSize: 10 * 1024 * 1024,
            rateLimitWindowMs: 60000,
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
            monthlyLimit: zod_1.z.number().default(1000000),
            perAgentLimits: zod_1.z.record(zod_1.z.string(), zod_1.z.number()).default({}),
            alertThreshold: zod_1.z.number().default(80),
            hardStop: zod_1.z.boolean().default(true),
            dbPath: zod_1.z.string().default('./data/budget.db'),
        }).default({
            enabled: false,
            monthlyLimit: 1000000,
            perAgentLimits: {},
            alertThreshold: 80,
            hardStop: true,
            dbPath: './data/budget.db',
        }),
        'goals': FeatureToggleSchema.extend({
            dbPath: zod_1.z.string().default('./data/goals.db'),
        }).default({ enabled: false, dbPath: './data/goals.db' }),
        'task-checkout': FeatureToggleSchema.extend({
            dbPath: zod_1.z.string().default('./data/task-checkout.db'),
            leaseDurationMs: zod_1.z.number().default(1800000),
            maxLeasesPerAgent: zod_1.z.number().default(10),
        }).default({
            enabled: false,
            dbPath: './data/task-checkout.db',
            leaseDurationMs: 1800000,
            maxLeasesPerAgent: 10,
        }),
        'company-templates': FeatureToggleSchema.extend({
            templatesPath: zod_1.z.string().default('./data/templates'),
        }).default({ enabled: false, templatesPath: './data/templates' }),
        'governance': FeatureToggleSchema.extend({
            dbPath: zod_1.z.string().default('./data/governance.db'),
            autoApproveRisk: zod_1.z.enum(['none', 'low', 'medium']).default('low'),
            ticketExpiryMs: zod_1.z.number().default(86400000),
            requiredApprovers: zod_1.z.number().default(1),
            approvers: zod_1.z.array(zod_1.z.string()).default([]),
        }).default({
            enabled: false,
            dbPath: './data/governance.db',
            autoApproveRisk: 'low',
            ticketExpiryMs: 86400000,
            requiredApprovers: 1,
            approvers: [],
        }),
        'multi-agent-coordination': MultiAgentCoordinationConfigSchema.default({
            enabled: false,
            dbPath: './data/multi-agent-coordination.db',
            heartbeatIntervalMs: 30000,
            offlineTimeoutMs: 60000,
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
            maxArticles: 10000,
            maxVersionsPerArticle: 20,
        }),
        'health-monitoring': HealthMonitoringConfigSchema.default({
            enabled: false,
            collectionIntervalMs: 30000,
            diskCheckPath: '/',
            cpuWarningThreshold: 80,
            memoryWarningThreshold: 80,
            diskWarningThreshold: 80,
        }),
        'auto-update': AutoUpdateConfigSchema.default({
            enabled: false,
            dbPath: './data/auto-update.db',
            repoOwner: 'AG064',
            repoName: 'argentum',
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
            maxMessageLength: 10000,
            maxPayloadBytes: 1024 * 1024,
            maxFileSize: 10 * 1024 * 1024,
            rateLimitWindowMs: 60000,
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
            monthlyLimit: 1000000,
            perAgentLimits: {},
            alertThreshold: 80,
            hardStop: true,
            dbPath: './data/budget.db',
        },
        'goals': { enabled: false, dbPath: './data/goals.db' },
        'task-checkout': {
            enabled: false,
            dbPath: './data/task-checkout.db',
            leaseDurationMs: 1800000,
            maxLeasesPerAgent: 10,
        },
        'company-templates': { enabled: false, templatesPath: './data/templates' },
        'governance': {
            enabled: false,
            dbPath: './data/governance.db',
            autoApproveRisk: 'low',
            ticketExpiryMs: 86400000,
            requiredApprovers: 1,
            approvers: [],
        },
        'multi-agent-coordination': {
            enabled: false,
            dbPath: './data/multi-agent-coordination.db',
            heartbeatIntervalMs: 30000,
            offlineTimeoutMs: 60000,
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
            maxArticles: 10000,
            maxVersionsPerArticle: 20,
        },
        'health-monitoring': {
            enabled: false,
            collectionIntervalMs: 30000,
            diskCheckPath: '/',
            cpuWarningThreshold: 80,
            memoryWarningThreshold: 80,
            diskWarningThreshold: 80,
        },
        'auto-update': {
            enabled: false,
            dbPath: './data/auto-update.db',
            repoOwner: 'AG064',
            repoName: 'argentum',
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
        capabilities: {
            defaultProfile: 'restricted',
            workspaceRoot: '.',
            auditPath: './data/audit/capabilities.log',
        },
    }),
    channels: zod_1.z
        .object({
        telegram: zod_1.z
            .object({
            enabled: zod_1.z.boolean().default(false),
            token: zod_1.z.string().optional(),
            allowedUsers: zod_1.z.array(zod_1.z.number()).default([]),
            allowedChats: zod_1.z.array(zod_1.z.number()).default([]),
            allowAll: zod_1.z.boolean().default(false),
        })
            .default({ enabled: false, allowedUsers: [], allowedChats: [], allowAll: false }),
        webchat: zod_1.z
            .object({
            enabled: zod_1.z.boolean().default(false),
            authToken: zod_1.z.string().min(16).optional(),
        })
            .default({ enabled: false }),
        mobile: zod_1.z
            .object({
            enabled: zod_1.z.boolean().default(false),
            fcmKey: zod_1.z.string().optional(),
            httpPort: zod_1.z.number().default(3003),
            httpPath: zod_1.z.string().default('/mobile'),
            requireAuth: zod_1.z.boolean().default(false),
            authToken: zod_1.z.string().optional(),
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
    logging: zod_1.z
        .object({
        level: zod_1.z.enum(['debug', 'info', 'warn', 'error']).default('info'),
        format: zod_1.z.enum(['json', 'pretty']).default('pretty'),
        file: zod_1.z.string().optional(),
    })
        .default({ level: 'info', format: 'pretty' }),
});
/** Configuration manager with hot-reload support */
class ConfigManager {
    constructor(configPath) {
        this.watcher = null;
        this.listeners = new Set();
        this.baseConfigPath = (0, path_1.resolve)(process.cwd(), 'config/default.yaml');
        const envConfigPath = process.env.ARGENTUM_CONFIG_PATH;
        this.configPath = configPath ?? (0, path_1.resolve)(process.cwd(), envConfigPath ?? 'argentum.json');
        this.config = this.loadConfig();
    }
    /** Load and validate configuration from YAML file */
    loadConfig() {
        const baseConfig = this.loadConfigFile(this.baseConfigPath);
        const fileConfig = this.configPath === this.baseConfigPath ? {} : this.loadConfigFile(this.configPath);
        // Environment variable overrides
        const envOverrides = this.loadEnvOverrides();
        const merged = this.deepMerge(this.deepMerge(baseConfig, fileConfig), envOverrides);
        const result = exports.ConfigSchema.safeParse(merged);
        if (!result.success) {
            console.error('Configuration validation failed:', result.error.format());
            process.exit(1);
        }
        return result.data;
    }
    /** Load a single configuration file if it exists */
    loadConfigFile(filePath) {
        if (!(0, fs_1.existsSync)(filePath)) {
            return {};
        }
        const raw = (0, fs_1.readFileSync)(filePath, 'utf-8');
        const parsed = (0, yaml_1.parse)(raw);
        if (parsed && typeof parsed === 'object' && !Array.isArray(parsed)) {
            return parsed;
        }
        return {};
    }
    /** Load configuration overrides from environment variables */
    loadEnvOverrides() {
        const overrides = {};
        if (process.env.ARGENTUM_PORT) {
            overrides['server'] = { port: parseInt(process.env.ARGENTUM_PORT, 10) };
        }
        if (process.env.ARGENTUM_LOG_LEVEL) {
            overrides['logging'] = { level: process.env.ARGENTUM_LOG_LEVEL };
        }
        if (process.env.ARGENTUM_LOG_FORMAT) {
            overrides['logging'] = {
                ...(overrides['logging'] ?? {}),
                format: process.env.ARGENTUM_LOG_FORMAT,
            };
        }
        if (process.env.ARGENTUM_TELEGRAM_TOKEN) {
            overrides['channels'] = { telegram: { token: process.env.ARGENTUM_TELEGRAM_TOKEN } };
        }
        if (process.env.ARGENTUM_TELEGRAM_ENABLED) {
            overrides['channels'] = {
                ...(overrides['channels'] ?? {}),
                telegram: {
                    ...((overrides['channels']?.['telegram']) ?? {}),
                    enabled: process.env.ARGENTUM_TELEGRAM_ENABLED === 'true',
                },
            };
        }
        if (process.env.ARGENTUM_WEBCHAT_ENABLED) {
            overrides['channels'] = {
                ...(overrides['channels'] ?? {}),
                webchat: { enabled: process.env.ARGENTUM_WEBCHAT_ENABLED === 'true' },
            };
            overrides['features'] = {
                ...(overrides['features'] ?? {}),
                webchat: { enabled: process.env.ARGENTUM_WEBCHAT_ENABLED === 'true' },
            };
        }
        if (process.env.ARGENTUM_WEBCHAT_AUTH_TOKEN) {
            overrides['channels'] = {
                ...(overrides['channels'] ?? {}),
                webchat: {
                    ...((overrides['channels']?.['webchat']) ?? {}),
                    authToken: process.env.ARGENTUM_WEBCHAT_AUTH_TOKEN,
                },
            };
            overrides['features'] = {
                ...(overrides['features'] ?? {}),
                webchat: {
                    ...((overrides['features']?.['webchat']) ?? {}),
                    authToken: process.env.ARGENTUM_WEBCHAT_AUTH_TOKEN,
                },
            };
        }
        if (process.env.ARGENTUM_SUPABASE_URL) {
            overrides['memory'] = { supabaseUrl: process.env.ARGENTUM_SUPABASE_URL };
        }
        if (process.env.ARGENTUM_SUPABASE_KEY) {
            overrides['memory'] = {
                ...(overrides['memory'] ?? {}),
                supabaseKey: process.env.ARGENTUM_SUPABASE_KEY,
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
            }
            else {
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
        if (this.watcher)
            return;
        void this.startHotReloadWatcher();
    }
    async startHotReloadWatcher() {
        try {
            const chokidar = await Promise.resolve().then(() => __importStar(require('chokidar')));
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
        }
        catch (err) {
            console.error('Failed to enable configuration hot reload:', err);
        }
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
