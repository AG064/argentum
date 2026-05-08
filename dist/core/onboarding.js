"use strict";
Object.defineProperty(exports, "__esModule", { value: true });
exports.PROVIDER_PRESETS = void 0;
exports.generateWebchatAuthToken = generateWebchatAuthToken;
exports.createOnboardingProfile = createOnboardingProfile;
exports.writeOnboardingProfile = writeOnboardingProfile;
const crypto_1 = require("crypto");
const fs_1 = require("fs");
const path_1 = require("path");
const yaml_1 = require("yaml");
const config_1 = require("./config");
exports.PROVIDER_PRESETS = {
    minimax: {
        name: 'minimax',
        label: 'MiniMax',
        base_url: 'https://api.minimax.io/v1',
        api_key_env: 'MINIMAX_API_KEY',
        api: 'openai',
        defaultModel: 'MiniMax-M2.7',
    },
    groq: {
        name: 'groq',
        label: 'Groq',
        base_url: 'https://api.groq.com/openai/v1',
        api_key_env: 'GROQ_API_KEY',
        api: 'openai',
        defaultModel: 'meta-llama/llama-4-scout-17b-16e-instruct',
    },
    ollama: {
        name: 'ollama',
        label: 'Ollama',
        base_url: 'http://127.0.0.1:11434/v1',
        api_key_env: 'OLLAMA_API_KEY',
        api: 'openai',
        defaultModel: 'llama3.1',
    },
    nvidia: {
        name: 'nvidia',
        label: 'NVIDIA',
        base_url: 'https://integrate.api.nvidia.com/v1',
        api_key_env: 'NVIDIA_API_KEY',
        api: 'openai',
        defaultModel: 'deepseek-ai/deepseek-v3.2',
    },
    openrouter: {
        name: 'openrouter',
        label: 'OpenRouter',
        base_url: 'https://openrouter.ai/api/v1',
        api_key_env: 'OPENROUTER_API_KEY',
        api: 'openai',
        defaultModel: 'google/gemma-3-27b-it',
        headers: {
            'HTTP-Referer': 'https://github.com/AG064/argentum',
            'X-Title': 'Argentum',
        },
    },
    google: {
        name: 'google',
        label: 'Google Gemini',
        base_url: 'https://generativelanguage.googleapis.com/v1beta/openai/',
        api_key_env: 'GOOGLE_API_KEY',
        api: 'openai',
        defaultModel: 'gemini-2.5-flash',
    },
    anthropic: {
        name: 'anthropic',
        label: 'Anthropic Claude',
        base_url: 'https://api.anthropic.com',
        api_key_env: 'ANTHROPIC_API_KEY',
        api: 'anthropic',
        defaultModel: 'claude-sonnet-4-20250514',
    },
    openai: {
        name: 'openai',
        label: 'OpenAI',
        base_url: 'https://api.openai.com/v1',
        api_key_env: 'OPENAI_API_KEY',
        api: 'openai',
        defaultModel: 'gpt-4o-mini',
    },
};
const FEATURE_CATEGORIES = {
    core: ['sqlite-memory', 'cron-scheduler', 'audit-log'],
    comm: ['webchat'],
    memory: ['knowledge-graph', 'semantic-search', 'markdown-memory', 'multimodal-memory'],
    productivity: ['goals', 'life-domains', 'task-checkout', 'goal-decomposition'],
    automation: ['container-sandbox', 'webhooks', 'file-watcher'],
    monitoring: ['health-monitoring', 'budget', 'email-integration'],
    skills: ['skills-library', 'skill-loader', 'skill-evolution'],
};
function generateWebchatAuthToken() {
    return (0, crypto_1.randomBytes)(32).toString('hex');
}
function nonEmptyTrimmed(value, fallback) {
    const trimmed = value?.trim();
    return trimmed && trimmed.length > 0 ? trimmed : fallback;
}
function createOnboardingProfile(options = {}) {
    const warnings = [];
    const env = {};
    const provider = resolveProvider(options);
    const model = nonEmptyTrimmed(options.model, provider.defaultModel);
    const port = normalizePort(options.port ?? 3000);
    if (options.apiKey?.trim()) {
        env[provider.api_key_env] = options.apiKey.trim();
    }
    const features = {};
    for (const featureName of selectedFeatures(options.featureCategories ?? [])) {
        features[featureName] = { enabled: true };
    }
    const webchatSelected = selectedFeatures(options.featureCategories ?? []).includes('webchat');
    if (webchatSelected) {
        const token = options.webchatAuthToken?.trim();
        if (token) {
            features['webchat'] = {
                enabled: true,
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
            };
            env.ARGENTUM_WEBCHAT_AUTH_TOKEN = token;
        }
        else {
            features['webchat'] = { enabled: false };
            warnings.push('Webchat was selected but no auth token was provided, so it stayed disabled.');
        }
    }
    const telegram = normalizeTelegram(options.telegram);
    if (telegram.enabled && telegram.token) {
        env.ARGENTUM_TELEGRAM_TOKEN = telegram.token;
    }
    else if (options.telegram?.token?.trim()) {
        warnings.push('Telegram token was provided without allowed users/chats or allowAll, so Telegram stayed disabled.');
    }
    const config = {
        $schema: 'https://github.com/AG064/argentum/blob/main/config-schema.json',
        name: nonEmptyTrimmed(options.name, 'My Argentum Instance'),
        version: '0.0.5',
        server: {
            port,
            host: '127.0.0.1',
            cors: {
                enabled: true,
                origins: [`http://127.0.0.1:${port}`, `http://localhost:${port}`],
            },
            rateLimit: {
                enabled: true,
                windowMs: 60000,
                maxRequests: 100,
            },
        },
        llm: {
            providers: {
                [provider.name]: {
                    base_url: provider.base_url,
                    api_key_env: provider.api_key_env,
                    api: provider.api,
                    models: [model],
                    ...(provider.headers ? { headers: provider.headers } : {}),
                },
            },
            default: provider.name,
            fallback: [],
        },
        features,
        memory: {
            primary: 'sqlite',
            path: './data/memory.db',
            selfEvolving: false,
            compressionThreshold: 10000,
        },
        security: {
            policy: 'config/security-policy.yaml',
            secrets: 'env',
            auditLog: true,
            allowlistMode: 'strict',
            capabilities: {
                defaultProfile: 'restricted',
                workspaceRoot: '.',
                auditPath: './data/audit/capabilities.log',
            },
        },
        channels: {
            telegram: {
                enabled: telegram.enabled,
                allowedUsers: telegram.allowedUsers,
                allowedChats: telegram.allowedChats,
                allowAll: telegram.allowAll,
            },
            webchat: {
                enabled: Boolean(features['webchat']?.['enabled']),
            },
            mobile: {
                enabled: false,
                httpPort: 3003,
                httpPath: '/mobile',
                requireAuth: false,
            },
        },
        logging: {
            level: 'info',
            format: 'pretty',
        },
    };
    config_1.ConfigSchema.parse(config);
    return { config, env, warnings };
}
function writeOnboardingProfile(workDir, profile, options = {}) {
    const configDir = (0, path_1.join)(workDir, 'config');
    const dataDir = (0, path_1.join)(workDir, 'data');
    const configPath = (0, path_1.join)(configDir, 'default.yaml');
    const envPath = (0, path_1.join)(workDir, '.env');
    const envExamplePath = (0, path_1.join)(workDir, '.env.example');
    (0, fs_1.mkdirSync)(configDir, { recursive: true });
    (0, fs_1.mkdirSync)(dataDir, { recursive: true });
    if ((0, fs_1.existsSync)(configPath) && !options.overwrite) {
        throw new Error('config/default.yaml already exists');
    }
    (0, fs_1.writeFileSync)(configPath, (0, yaml_1.stringify)(profile.config, { lineWidth: 120 }));
    mergeDotenv(envPath, profile.env);
    if (!(0, fs_1.existsSync)(envExamplePath) || options.overwrite) {
        (0, fs_1.writeFileSync)(envExamplePath, createEnvExample(profile), 'utf8');
    }
    return { configPath, envPath, envExamplePath, dataDir };
}
function resolveProvider(options) {
    if (options.provider === 'custom') {
        return {
            name: nonEmptyTrimmed(options.customProvider?.name, 'custom'),
            label: nonEmptyTrimmed(options.customProvider?.label, 'Custom'),
            base_url: nonEmptyTrimmed(options.customProvider?.base_url, 'https://example.invalid/v1'),
            api_key_env: nonEmptyTrimmed(options.customProvider?.api_key_env, 'MY_API_KEY'),
            api: options.customProvider?.api ?? 'openai',
            defaultModel: nonEmptyTrimmed(options.customProvider?.defaultModel, nonEmptyTrimmed(options.model, 'custom-model')),
            ...(options.customProvider?.headers ? { headers: options.customProvider.headers } : {}),
        };
    }
    return exports.PROVIDER_PRESETS[options.provider ?? 'nvidia'];
}
function normalizePort(port) {
    if (!Number.isInteger(port) || port < 1 || port > 65535) {
        return 3000;
    }
    return port;
}
function selectedFeatures(categories) {
    const features = new Set();
    for (const category of categories) {
        for (const feature of FEATURE_CATEGORIES[category] ?? []) {
            features.add(feature);
        }
    }
    return [...features];
}
function normalizeTelegram(options) {
    const token = options?.token?.trim();
    const allowedUsers = options?.allowedUsers?.filter(Number.isFinite) ?? [];
    const allowedChats = options?.allowedChats?.filter(Number.isFinite) ?? [];
    const allowAll = options?.allowAll === true;
    const enabled = Boolean(token && (allowAll || allowedUsers.length > 0 || allowedChats.length > 0));
    return {
        enabled,
        ...(token ? { token } : {}),
        allowedUsers,
        allowedChats,
        allowAll,
    };
}
function mergeDotenv(envPath, env) {
    const entries = Object.entries(env).filter(([, value]) => value.length > 0);
    if (entries.length === 0 && !(0, fs_1.existsSync)(envPath)) {
        (0, fs_1.writeFileSync)(envPath, '', 'utf8');
        return;
    }
    const existing = (0, fs_1.existsSync)(envPath) ? (0, fs_1.readFileSync)(envPath, 'utf8') : '';
    const lines = existing ? [existing.replace(/\s*$/, '')] : [];
    for (const [key, value] of entries) {
        if (new RegExp(`^${escapeRegExp(key)}=`, 'm').test(existing)) {
            continue;
        }
        lines.push(`${key}=${formatDotenvValue(value)}`);
    }
    (0, fs_1.writeFileSync)(envPath, `${lines.filter(Boolean).join('\n')}\n`, 'utf8');
}
function createEnvExample(profile) {
    const config = profile.config;
    const providerName = config.llm?.default ?? 'nvidia';
    const apiKeyEnv = config.llm?.providers?.[providerName]?.api_key_env ?? 'NVIDIA_API_KEY';
    return [
        '# Argentum Environment Variables',
        'ARGENTUM_WORKDIR=.',
        `ARGENTUM_PORT=${config.server?.port ?? 3000}`,
        'ARGENTUM_MASTER_KEY=',
        `${apiKeyEnv}=`,
        'ARGENTUM_WEBCHAT_AUTH_TOKEN=',
        'ARGENTUM_TELEGRAM_TOKEN=',
        '',
    ].join('\n');
}
function formatDotenvValue(value) {
    if (/[\s"'#=]/.test(value)) {
        return JSON.stringify(value);
    }
    return value;
}
function escapeRegExp(value) {
    return value.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
}
