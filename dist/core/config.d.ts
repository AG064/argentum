/**
 * AG-Claw Configuration Loader
 *
 * Loads and validates configuration from YAML files with environment variable overrides.
 * Supports hot-reloading via chokidar file watcher.
 */
import { z } from 'zod';
/** Model routing scoring weights configuration */
export declare const ModelRoutingWeightsSchema: z.ZodObject<{
    costEfficiency: z.ZodDefault<z.ZodNumber>;
    latency: z.ZodDefault<z.ZodNumber>;
    capabilityMatch: z.ZodDefault<z.ZodNumber>;
    contextLengthFit: z.ZodDefault<z.ZodNumber>;
    toolSupport: z.ZodDefault<z.ZodNumber>;
    recentSuccessRate: z.ZodDefault<z.ZodNumber>;
    tokenEfficiency: z.ZodDefault<z.ZodNumber>;
    specializationMatch: z.ZodDefault<z.ZodNumber>;
    reliability: z.ZodDefault<z.ZodNumber>;
    throughput: z.ZodDefault<z.ZodNumber>;
    customWeight: z.ZodDefault<z.ZodNumber>;
}, z.core.$strip>;
/** Model routing configuration schema */
export declare const ModelRoutingConfigSchema: z.ZodObject<{
    enabled: z.ZodDefault<z.ZodBoolean>;
    cacheScoresMs: z.ZodDefault<z.ZodNumber>;
    weights: z.ZodDefault<z.ZodObject<{
        costEfficiency: z.ZodDefault<z.ZodNumber>;
        latency: z.ZodDefault<z.ZodNumber>;
        capabilityMatch: z.ZodDefault<z.ZodNumber>;
        contextLengthFit: z.ZodDefault<z.ZodNumber>;
        toolSupport: z.ZodDefault<z.ZodNumber>;
        recentSuccessRate: z.ZodDefault<z.ZodNumber>;
        tokenEfficiency: z.ZodDefault<z.ZodNumber>;
        specializationMatch: z.ZodDefault<z.ZodNumber>;
        reliability: z.ZodDefault<z.ZodNumber>;
        throughput: z.ZodDefault<z.ZodNumber>;
        customWeight: z.ZodDefault<z.ZodNumber>;
    }, z.core.$strip>>;
    models: z.ZodOptional<z.ZodArray<z.ZodObject<{
        modelId: z.ZodString;
        costPer1K: z.ZodNumber;
        latency: z.ZodNumber;
        capabilities: z.ZodArray<z.ZodString>;
        contextLength: z.ZodOptional<z.ZodNumber>;
        toolSupport: z.ZodOptional<z.ZodBoolean>;
        reliability: z.ZodOptional<z.ZodNumber>;
        specialization: z.ZodOptional<z.ZodArray<z.ZodString>>;
        throughput: z.ZodOptional<z.ZodNumber>;
    }, z.core.$strip>>>;
}, z.core.$strip>;
/** LLM provider configuration schema */
export declare const LLMProviderConfigSchema: z.ZodObject<{
    base_url: z.ZodString;
    api_key: z.ZodOptional<z.ZodString>;
    api_key_env: z.ZodOptional<z.ZodString>;
    api: z.ZodDefault<z.ZodEnum<{
        openai: "openai";
        anthropic: "anthropic";
    }>>;
    models: z.ZodArray<z.ZodString>;
    headers: z.ZodOptional<z.ZodRecord<z.ZodString, z.ZodString>>;
}, z.core.$strip>;
/** LLM configuration schema */
export declare const LLMConfigSchema: z.ZodDefault<z.ZodObject<{
    providers: z.ZodDefault<z.ZodRecord<z.ZodString, z.ZodObject<{
        base_url: z.ZodString;
        api_key: z.ZodOptional<z.ZodString>;
        api_key_env: z.ZodOptional<z.ZodString>;
        api: z.ZodDefault<z.ZodEnum<{
            openai: "openai";
            anthropic: "anthropic";
        }>>;
        models: z.ZodArray<z.ZodString>;
        headers: z.ZodOptional<z.ZodRecord<z.ZodString, z.ZodString>>;
    }, z.core.$strip>>>;
    default: z.ZodDefault<z.ZodString>;
    fallback: z.ZodOptional<z.ZodArray<z.ZodString>>;
}, z.core.$strip>>;
/** Root configuration schema */
export declare const ConfigSchema: z.ZodObject<{
    server: z.ZodDefault<z.ZodObject<{
        port: z.ZodDefault<z.ZodNumber>;
        host: z.ZodDefault<z.ZodString>;
        cors: z.ZodDefault<z.ZodObject<{
            enabled: z.ZodDefault<z.ZodBoolean>;
            origins: z.ZodDefault<z.ZodArray<z.ZodString>>;
        }, z.core.$strip>>;
        rateLimit: z.ZodDefault<z.ZodObject<{
            enabled: z.ZodDefault<z.ZodBoolean>;
            windowMs: z.ZodDefault<z.ZodNumber>;
            maxRequests: z.ZodDefault<z.ZodNumber>;
        }, z.core.$strip>>;
    }, z.core.$strip>>;
    llm: z.ZodDefault<z.ZodObject<{
        providers: z.ZodDefault<z.ZodRecord<z.ZodString, z.ZodObject<{
            base_url: z.ZodString;
            api_key: z.ZodOptional<z.ZodString>;
            api_key_env: z.ZodOptional<z.ZodString>;
            api: z.ZodDefault<z.ZodEnum<{
                openai: "openai";
                anthropic: "anthropic";
            }>>;
            models: z.ZodArray<z.ZodString>;
            headers: z.ZodOptional<z.ZodRecord<z.ZodString, z.ZodString>>;
        }, z.core.$strip>>>;
        default: z.ZodDefault<z.ZodString>;
        fallback: z.ZodOptional<z.ZodArray<z.ZodString>>;
    }, z.core.$strip>>;
    modelRouting: z.ZodDefault<z.ZodObject<{
        enabled: z.ZodDefault<z.ZodBoolean>;
        cacheScoresMs: z.ZodDefault<z.ZodNumber>;
        weights: z.ZodDefault<z.ZodObject<{
            costEfficiency: z.ZodDefault<z.ZodNumber>;
            latency: z.ZodDefault<z.ZodNumber>;
            capabilityMatch: z.ZodDefault<z.ZodNumber>;
            contextLengthFit: z.ZodDefault<z.ZodNumber>;
            toolSupport: z.ZodDefault<z.ZodNumber>;
            recentSuccessRate: z.ZodDefault<z.ZodNumber>;
            tokenEfficiency: z.ZodDefault<z.ZodNumber>;
            specializationMatch: z.ZodDefault<z.ZodNumber>;
            reliability: z.ZodDefault<z.ZodNumber>;
            throughput: z.ZodDefault<z.ZodNumber>;
            customWeight: z.ZodDefault<z.ZodNumber>;
        }, z.core.$strip>>;
        models: z.ZodOptional<z.ZodArray<z.ZodObject<{
            modelId: z.ZodString;
            costPer1K: z.ZodNumber;
            latency: z.ZodNumber;
            capabilities: z.ZodArray<z.ZodString>;
            contextLength: z.ZodOptional<z.ZodNumber>;
            toolSupport: z.ZodOptional<z.ZodBoolean>;
            reliability: z.ZodOptional<z.ZodNumber>;
            specialization: z.ZodOptional<z.ZodArray<z.ZodString>>;
            throughput: z.ZodOptional<z.ZodNumber>;
        }, z.core.$strip>>>;
    }, z.core.$strip>>;
    features: z.ZodDefault<z.ZodObject<{
        webchat: z.ZodDefault<z.ZodObject<{
            enabled: z.ZodDefault<z.ZodBoolean>;
            port: z.ZodDefault<z.ZodNumber>;
            host: z.ZodDefault<z.ZodString>;
            authToken: z.ZodOptional<z.ZodString>;
            requireAuth: z.ZodDefault<z.ZodBoolean>;
            maxConnections: z.ZodDefault<z.ZodNumber>;
            messageHistory: z.ZodDefault<z.ZodNumber>;
            maxMessageLength: z.ZodDefault<z.ZodNumber>;
            maxPayloadBytes: z.ZodDefault<z.ZodNumber>;
            maxFileSize: z.ZodDefault<z.ZodNumber>;
            rateLimitWindowMs: z.ZodDefault<z.ZodNumber>;
            maxMessagesPerWindow: z.ZodDefault<z.ZodNumber>;
            allowedFileTypes: z.ZodDefault<z.ZodArray<z.ZodString>>;
        }, z.core.$strip>>;
        voice: z.ZodDefault<z.ZodObject<{
            enabled: z.ZodDefault<z.ZodBoolean>;
            provider: z.ZodDefault<z.ZodEnum<{
                elevenlabs: "elevenlabs";
                openai: "openai";
                local: "local";
            }>>;
            voice: z.ZodDefault<z.ZodString>;
            model: z.ZodDefault<z.ZodString>;
            sttProvider: z.ZodDefault<z.ZodEnum<{
                local: "local";
                whisper: "whisper";
                google: "google";
            }>>;
            wakeWord: z.ZodOptional<z.ZodString>;
        }, z.core.$strip>>;
        'knowledge-graph': z.ZodDefault<z.ZodObject<{
            enabled: z.ZodDefault<z.ZodBoolean>;
            backend: z.ZodDefault<z.ZodEnum<{
                sqlite: "sqlite";
                neo4j: "neo4j";
                memory: "memory";
            }>>;
            path: z.ZodDefault<z.ZodString>;
        }, z.core.$strip>>;
        'multimodal-memory': z.ZodDefault<z.ZodObject<{
            enabled: z.ZodDefault<z.ZodBoolean>;
        }, z.core.$strip>>;
        'browser-automation': z.ZodDefault<z.ZodObject<{
            enabled: z.ZodDefault<z.ZodBoolean>;
        }, z.core.$strip>>;
        webhooks: z.ZodDefault<z.ZodObject<{
            enabled: z.ZodDefault<z.ZodBoolean>;
        }, z.core.$strip>>;
        'mesh-workflows': z.ZodDefault<z.ZodObject<{
            enabled: z.ZodDefault<z.ZodBoolean>;
        }, z.core.$strip>>;
        'live-canvas': z.ZodDefault<z.ZodObject<{
            enabled: z.ZodDefault<z.ZodBoolean>;
        }, z.core.$strip>>;
        'container-sandbox': z.ZodDefault<z.ZodObject<{
            enabled: z.ZodDefault<z.ZodBoolean>;
        }, z.core.$strip>>;
        'air-gapped': z.ZodDefault<z.ZodObject<{
            enabled: z.ZodDefault<z.ZodBoolean>;
        }, z.core.$strip>>;
        'morning-briefing': z.ZodDefault<z.ZodObject<{
            enabled: z.ZodDefault<z.ZodBoolean>;
        }, z.core.$strip>>;
        'evening-recap': z.ZodDefault<z.ZodObject<{
            enabled: z.ZodDefault<z.ZodBoolean>;
        }, z.core.$strip>>;
        'smart-recommendations': z.ZodDefault<z.ZodObject<{
            enabled: z.ZodDefault<z.ZodBoolean>;
        }, z.core.$strip>>;
        'group-management': z.ZodDefault<z.ZodObject<{
            enabled: z.ZodDefault<z.ZodBoolean>;
        }, z.core.$strip>>;
        budget: z.ZodDefault<z.ZodObject<{
            enabled: z.ZodDefault<z.ZodBoolean>;
            monthlyLimit: z.ZodDefault<z.ZodNumber>;
            perAgentLimits: z.ZodDefault<z.ZodRecord<z.ZodString, z.ZodNumber>>;
            alertThreshold: z.ZodDefault<z.ZodNumber>;
            hardStop: z.ZodDefault<z.ZodBoolean>;
            dbPath: z.ZodDefault<z.ZodString>;
        }, z.core.$strip>>;
        goals: z.ZodDefault<z.ZodObject<{
            enabled: z.ZodDefault<z.ZodBoolean>;
            dbPath: z.ZodDefault<z.ZodString>;
        }, z.core.$strip>>;
        'task-checkout': z.ZodDefault<z.ZodObject<{
            enabled: z.ZodDefault<z.ZodBoolean>;
            dbPath: z.ZodDefault<z.ZodString>;
            leaseDurationMs: z.ZodDefault<z.ZodNumber>;
            maxLeasesPerAgent: z.ZodDefault<z.ZodNumber>;
        }, z.core.$strip>>;
        'company-templates': z.ZodDefault<z.ZodObject<{
            enabled: z.ZodDefault<z.ZodBoolean>;
            templatesPath: z.ZodDefault<z.ZodString>;
        }, z.core.$strip>>;
        governance: z.ZodDefault<z.ZodObject<{
            enabled: z.ZodDefault<z.ZodBoolean>;
            dbPath: z.ZodDefault<z.ZodString>;
            autoApproveRisk: z.ZodDefault<z.ZodEnum<{
                none: "none";
                low: "low";
                medium: "medium";
            }>>;
            ticketExpiryMs: z.ZodDefault<z.ZodNumber>;
            requiredApprovers: z.ZodDefault<z.ZodNumber>;
            approvers: z.ZodDefault<z.ZodArray<z.ZodString>>;
        }, z.core.$strip>>;
        'multi-agent-coordination': z.ZodDefault<z.ZodObject<{
            enabled: z.ZodDefault<z.ZodBoolean>;
            dbPath: z.ZodDefault<z.ZodString>;
            heartbeatIntervalMs: z.ZodDefault<z.ZodNumber>;
            offlineTimeoutMs: z.ZodDefault<z.ZodNumber>;
            maxAgents: z.ZodDefault<z.ZodNumber>;
        }, z.core.$strip>>;
        'role-based-access': z.ZodDefault<z.ZodObject<{
            enabled: z.ZodDefault<z.ZodBoolean>;
            dbPath: z.ZodDefault<z.ZodString>;
            defaultRole: z.ZodDefault<z.ZodString>;
        }, z.core.$strip>>;
        'shared-knowledge-base': z.ZodDefault<z.ZodObject<{
            enabled: z.ZodDefault<z.ZodBoolean>;
            dbPath: z.ZodDefault<z.ZodString>;
            maxArticles: z.ZodDefault<z.ZodNumber>;
            maxVersionsPerArticle: z.ZodDefault<z.ZodNumber>;
        }, z.core.$strip>>;
        'health-monitoring': z.ZodDefault<z.ZodObject<{
            enabled: z.ZodDefault<z.ZodBoolean>;
            collectionIntervalMs: z.ZodDefault<z.ZodNumber>;
            diskCheckPath: z.ZodDefault<z.ZodString>;
            cpuWarningThreshold: z.ZodDefault<z.ZodNumber>;
            memoryWarningThreshold: z.ZodDefault<z.ZodNumber>;
            diskWarningThreshold: z.ZodDefault<z.ZodNumber>;
        }, z.core.$strip>>;
        'auto-update': z.ZodDefault<z.ZodObject<{
            enabled: z.ZodDefault<z.ZodBoolean>;
            dbPath: z.ZodDefault<z.ZodString>;
            repoOwner: z.ZodDefault<z.ZodString>;
            repoName: z.ZodDefault<z.ZodString>;
            checkIntervalHours: z.ZodDefault<z.ZodNumber>;
            autoApply: z.ZodDefault<z.ZodBoolean>;
            backupBeforeUpdate: z.ZodDefault<z.ZodBoolean>;
            backupPath: z.ZodDefault<z.ZodString>;
        }, z.core.$strip>>;
        'cron-scheduler': z.ZodDefault<z.ZodObject<{
            enabled: z.ZodDefault<z.ZodBoolean>;
            dbPath: z.ZodDefault<z.ZodString>;
            timezone: z.ZodOptional<z.ZodString>;
            maxJobs: z.ZodDefault<z.ZodNumber>;
        }, z.core.$strip>>;
    }, z.core.$catchall<z.ZodObject<{
        enabled: z.ZodDefault<z.ZodBoolean>;
    }, z.core.$loose>>>>;
    memory: z.ZodDefault<z.ZodObject<{
        primary: z.ZodDefault<z.ZodEnum<{
            sqlite: "sqlite";
            supabase: "supabase";
            markdown: "markdown";
        }>>;
        path: z.ZodDefault<z.ZodString>;
        supabaseUrl: z.ZodOptional<z.ZodString>;
        supabaseKey: z.ZodOptional<z.ZodString>;
        selfEvolving: z.ZodDefault<z.ZodBoolean>;
        compressionThreshold: z.ZodDefault<z.ZodNumber>;
    }, z.core.$strip>>;
    security: z.ZodDefault<z.ZodObject<{
        policy: z.ZodDefault<z.ZodString>;
        secrets: z.ZodDefault<z.ZodEnum<{
            file: "file";
            encrypted: "encrypted";
            env: "env";
        }>>;
        auditLog: z.ZodDefault<z.ZodBoolean>;
        allowlistMode: z.ZodDefault<z.ZodEnum<{
            strict: "strict";
            permissive: "permissive";
        }>>;
    }, z.core.$strip>>;
    channels: z.ZodDefault<z.ZodObject<{
        telegram: z.ZodDefault<z.ZodObject<{
            enabled: z.ZodDefault<z.ZodBoolean>;
            token: z.ZodOptional<z.ZodString>;
            allowedUsers: z.ZodDefault<z.ZodArray<z.ZodNumber>>;
            allowedChats: z.ZodDefault<z.ZodArray<z.ZodNumber>>;
            allowAll: z.ZodDefault<z.ZodBoolean>;
        }, z.core.$strip>>;
        webchat: z.ZodDefault<z.ZodObject<{
            enabled: z.ZodDefault<z.ZodBoolean>;
            authToken: z.ZodOptional<z.ZodString>;
        }, z.core.$strip>>;
        mobile: z.ZodDefault<z.ZodObject<{
            enabled: z.ZodDefault<z.ZodBoolean>;
            fcmKey: z.ZodOptional<z.ZodString>;
            httpPort: z.ZodDefault<z.ZodNumber>;
            httpPath: z.ZodDefault<z.ZodString>;
            requireAuth: z.ZodDefault<z.ZodBoolean>;
            authToken: z.ZodOptional<z.ZodString>;
        }, z.core.$strip>>;
    }, z.core.$strip>>;
    logging: z.ZodDefault<z.ZodObject<{
        level: z.ZodDefault<z.ZodEnum<{
            error: "error";
            debug: "debug";
            info: "info";
            warn: "warn";
        }>>;
        format: z.ZodDefault<z.ZodEnum<{
            json: "json";
            pretty: "pretty";
        }>>;
        file: z.ZodOptional<z.ZodString>;
    }, z.core.$strip>>;
}, z.core.$strip>;
export type AGClawConfig = z.infer<typeof ConfigSchema>;
/** Configuration manager with hot-reload support */
export declare class ConfigManager {
    private config;
    private configPath;
    private watcher;
    private listeners;
    constructor(configPath?: string);
    /** Load and validate configuration from YAML file */
    private loadConfig;
    /** Load configuration overrides from environment variables */
    private loadEnvOverrides;
    /** Deep merge two objects */
    private deepMerge;
    /** Get current configuration */
    get(): AGClawConfig;
    /** Get a specific config section */
    getSection<K extends keyof AGClawConfig>(section: K): AGClawConfig[K];
    /** Check if a feature is enabled */
    isFeatureEnabled(feature: keyof AGClawConfig['features']): boolean;
    /** Enable hot-reload watching */
    enableHotReload(): Promise<void>;
    /** Register a listener for config changes */
    onChange(listener: (config: AGClawConfig) => void): () => void;
    /** Stop watching for changes */
    dispose(): void;
}
/** Get or create the global config manager */
export declare function getConfig(configPath?: string): ConfigManager;
//# sourceMappingURL=config.d.ts.map