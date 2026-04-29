import { randomBytes } from 'crypto';
import { existsSync, mkdirSync, readFileSync, writeFileSync } from 'fs';
import { join } from 'path';

import { stringify } from 'yaml';

import { ConfigSchema } from './config';

export type ProviderName =
  | 'minimax'
  | 'groq'
  | 'ollama'
  | 'nvidia'
  | 'openrouter'
  | 'google'
  | 'anthropic'
  | 'openai'
  | 'custom';

export interface ProviderPreset {
  name: string;
  label: string;
  base_url: string;
  api_key_env: string;
  api: 'openai' | 'anthropic';
  defaultModel: string;
  headers?: Record<string, string>;
}

export interface OnboardingTelegramOptions {
  token?: string;
  allowedUsers?: number[];
  allowedChats?: number[];
  allowAll?: boolean;
}

export interface OnboardingOptions {
  name?: string;
  provider?: ProviderName;
  customProvider?: Partial<ProviderPreset>;
  model?: string;
  apiKey?: string;
  port?: number;
  featureCategories?: string[];
  webchatAuthToken?: string;
  telegram?: OnboardingTelegramOptions;
}

export interface OnboardingProfile {
  config: Record<string, unknown>;
  env: Record<string, string>;
  warnings: string[];
}

export interface WrittenOnboardingProfile {
  configPath: string;
  envPath: string;
  envExamplePath: string;
  dataDir: string;
}

export const PROVIDER_PRESETS: Record<Exclude<ProviderName, 'custom'>, ProviderPreset> = {
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

const FEATURE_CATEGORIES: Record<string, string[]> = {
  core: ['sqlite-memory', 'cron-scheduler', 'audit-log'],
  comm: ['webchat'],
  memory: ['knowledge-graph', 'semantic-search', 'markdown-memory', 'multimodal-memory'],
  productivity: ['goals', 'life-domains', 'task-checkout', 'goal-decomposition'],
  automation: ['container-sandbox', 'webhooks', 'file-watcher'],
  monitoring: ['health-monitoring', 'budget', 'email-integration'],
  skills: ['skills-library', 'skill-loader', 'skill-evolution'],
};

export function generateWebchatAuthToken(): string {
  return randomBytes(32).toString('hex');
}

export function createOnboardingProfile(options: OnboardingOptions = {}): OnboardingProfile {
  const warnings: string[] = [];
  const env: Record<string, string> = {};
  const provider = resolveProvider(options);
  const model = options.model?.trim() || provider.defaultModel;
  const port = normalizePort(options.port ?? 3000);

  if (options.apiKey?.trim()) {
    env[provider.api_key_env] = options.apiKey.trim();
  }

  const features: Record<string, Record<string, unknown>> = {};
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
        maxMessageLength: 10_000,
        maxPayloadBytes: 1024 * 1024,
        maxFileSize: 10 * 1024 * 1024,
        rateLimitWindowMs: 60_000,
        maxMessagesPerWindow: 60,
        allowedFileTypes: ['image/*', 'text/*', 'application/pdf', 'application/json'],
      };
      env.ARGENTUM_WEBCHAT_AUTH_TOKEN = token;
    } else {
      features['webchat'] = { enabled: false };
      warnings.push('Webchat was selected but no auth token was provided, so it stayed disabled.');
    }
  }

  const telegram = normalizeTelegram(options.telegram);
  if (telegram.enabled && telegram.token) {
    env.ARGENTUM_TELEGRAM_TOKEN = telegram.token;
  } else if (options.telegram?.token?.trim()) {
    warnings.push(
      'Telegram token was provided without allowed users/chats or allowAll, so Telegram stayed disabled.',
    );
  }

  const config: Record<string, unknown> = {
    $schema: 'https://github.com/AG064/argentum/blob/main/config-schema.json',
    name: options.name?.trim() || 'My Argentum Instance',
    version: '0.0.4',
    server: {
      port,
      host: '127.0.0.1',
      cors: {
        enabled: true,
        origins: [`http://127.0.0.1:${port}`, `http://localhost:${port}`],
      },
      rateLimit: {
        enabled: true,
        windowMs: 60_000,
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
      compressionThreshold: 10_000,
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

  ConfigSchema.parse(config);
  return { config, env, warnings };
}

export function writeOnboardingProfile(
  workDir: string,
  profile: OnboardingProfile,
  options: { overwrite?: boolean } = {},
): WrittenOnboardingProfile {
  const configDir = join(workDir, 'config');
  const dataDir = join(workDir, 'data');
  const configPath = join(configDir, 'default.yaml');
  const envPath = join(workDir, '.env');
  const envExamplePath = join(workDir, '.env.example');

  mkdirSync(configDir, { recursive: true });
  mkdirSync(dataDir, { recursive: true });

  if (existsSync(configPath) && !options.overwrite) {
    throw new Error('config/default.yaml already exists');
  }

  writeFileSync(configPath, stringify(profile.config, { lineWidth: 120 }));
  mergeDotenv(envPath, profile.env);

  if (!existsSync(envExamplePath) || options.overwrite) {
    writeFileSync(envExamplePath, createEnvExample(profile), 'utf8');
  }

  return { configPath, envPath, envExamplePath, dataDir };
}

function resolveProvider(options: OnboardingOptions): ProviderPreset {
  if (options.provider === 'custom') {
    return {
      name: options.customProvider?.name?.trim() || 'custom',
      label: options.customProvider?.label?.trim() || 'Custom',
      base_url: options.customProvider?.base_url?.trim() || 'https://example.invalid/v1',
      api_key_env: options.customProvider?.api_key_env?.trim() || 'MY_API_KEY',
      api: options.customProvider?.api ?? 'openai',
      defaultModel: options.customProvider?.defaultModel?.trim() || options.model?.trim() || 'custom-model',
      ...(options.customProvider?.headers ? { headers: options.customProvider.headers } : {}),
    };
  }

  return PROVIDER_PRESETS[options.provider ?? 'nvidia'];
}

function normalizePort(port: number): number {
  if (!Number.isInteger(port) || port < 1 || port > 65_535) {
    return 3000;
  }
  return port;
}

function selectedFeatures(categories: string[]): string[] {
  const features = new Set<string>();
  for (const category of categories) {
    for (const feature of FEATURE_CATEGORIES[category] ?? []) {
      features.add(feature);
    }
  }
  return [...features];
}

function normalizeTelegram(options?: OnboardingTelegramOptions): {
  enabled: boolean;
  token?: string;
  allowedUsers: number[];
  allowedChats: number[];
  allowAll: boolean;
} {
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

function mergeDotenv(envPath: string, env: Record<string, string>): void {
  const entries = Object.entries(env).filter(([, value]) => value.length > 0);
  if (entries.length === 0 && !existsSync(envPath)) {
    writeFileSync(envPath, '', 'utf8');
    return;
  }

  const existing = existsSync(envPath) ? readFileSync(envPath, 'utf8') : '';
  const lines = existing ? [existing.replace(/\s*$/, '')] : [];

  for (const [key, value] of entries) {
    if (new RegExp(`^${escapeRegExp(key)}=`, 'm').test(existing)) {
      continue;
    }
    lines.push(`${key}=${formatDotenvValue(value)}`);
  }

  writeFileSync(envPath, `${lines.filter(Boolean).join('\n')}\n`, 'utf8');
}

function createEnvExample(profile: OnboardingProfile): string {
  const config = profile.config as {
    server?: { port?: number };
    llm?: { default?: string; providers?: Record<string, { api_key_env?: string }> };
  };
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

function formatDotenvValue(value: string): string {
  if (/[\s"'#=]/.test(value)) {
    return JSON.stringify(value);
  }
  return value;
}

function escapeRegExp(value: string): string {
  return value.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
}
