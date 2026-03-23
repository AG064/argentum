/**
 * AG-Claw LLM Provider — Fully Configurable
 *
 * All providers are defined in agclaw.json. No hardcoded providers.
 * Any OpenAI-compatible API can be added by setting base_url + api_key.
 */

import { createLogger } from './logger';

const logger = createLogger();

export type MessageRole = 'system' | 'user' | 'assistant' | 'tool';

export interface Message {
  role: MessageRole;
  content: string;
  name?: string;
  tool_call_id?: string;
  tool_calls?: ToolCall[];
}

export interface ToolCall {
  id: string;
  type: 'function';
  function: { name: string; arguments: string };
}

export interface ToolDefinition {
  type: 'function';
  function: { name: string; description: string; parameters: Record<string, unknown> };
}

export interface LLMResponse {
  content?: string;
  toolCalls?: Array<{ id: string; name: string; arguments: Record<string, unknown> }>;
  usage: { prompt: number; completion: number };
}

export interface LLMProvider {
  readonly name: string;
  readonly model: string;
  readonly baseUrl: string;
  chat(messages: Message[], tools?: ToolDefinition[]): Promise<LLMResponse>;
}

// ─── Provider Config (from agclaw.json) ─────────────────────────────────────

export interface ProviderConfig {
  /** Base URL for the API (OpenAI-compatible) */
  base_url: string;
  /** API key, or env var name if api_key_env is set */
  api_key?: string;
  /** Environment variable containing the API key (alternative to api_key) */
  api_key_env?: string;
  /** API style — "openai" (default) or "anthropic" */
  api?: 'openai' | 'anthropic';
  /** Models available from this provider */
  models: string[];
  /** Optional extra headers (e.g. HTTP-Referer for OpenRouter) */
  headers?: Record<string, string>;
}

export interface LLMConfig {
  /** Provider configurations */
  providers: Record<string, ProviderConfig>;
  /** Default provider name */
  default: string;
  /** Fallback chain — list of "provider/model" pairs */
  fallback?: string[];
}

// ─── OpenAI-Compatible Provider ──────────────────────────────────────────────

class OpenAICompatibleProvider implements LLMProvider {
  readonly name: string;
  readonly model: string;
  readonly baseUrl: string;
  private apiKey: string;
  private fallbackModels: string[];
  private extraHeaders: Record<string, string>;

  constructor(config: {
    name: string;
    baseUrl: string;
    apiKey: string;
    model: string;
    fallbackModels?: string[];
    extraHeaders?: Record<string, string>;
  }) {
    this.name = config.name;
    this.baseUrl = config.baseUrl;
    this.apiKey = config.apiKey;
    this.model = config.model;
    this.fallbackModels = config.fallbackModels ?? [];
    this.extraHeaders = config.extraHeaders ?? {};
  }

  async chat(messages: Message[], tools?: ToolDefinition[]): Promise<LLMResponse> {
    const modelsToTry = [this.model, ...this.fallbackModels];
    let lastError: Error | null = null;

    for (const model of modelsToTry) {
      try {
        return await this.callModel(model, messages, tools);
      } catch (err) {
        lastError = err instanceof Error ? err : new Error(String(err));
        logger.warn(`Model ${model} failed, trying next`, { error: lastError.message });
      }
    }

    throw lastError ?? new Error('All models failed');
  }

  private async callModel(model: string, messages: Message[], tools?: ToolDefinition[]): Promise<LLMResponse> {
    const body: Record<string, unknown> = {
      model,
      messages: messages.map(m => {
        const msg: Record<string, unknown> = { role: m.role, content: m.content };
        if (m.name) msg['name'] = m['name'];
        if (m.tool_call_id) msg['tool_call_id'] = m['tool_call_id'];
        if (m.tool_calls) msg['tool_calls'] = m['tool_calls'];
        return msg;
      }),
      max_tokens: 4096,
      temperature: 0.7,
    };

    if (tools && tools.length > 0) {
      body['tools'] = tools;
      body['tool_choice'] = 'auto';
    }

    const headers: Record<string, string> = {
      'Authorization': `Bearer ${this.apiKey}`,
      'Content-Type': 'application/json',
      ...this.extraHeaders,
    };

    const response = await fetch(`${this.baseUrl}/chat/completions`, {
      method: 'POST',
      headers,
      body: JSON.stringify(body),
    });

    if (!response.ok) {
      const errorText = await response.text();
      throw new Error(`API error (${response.status}): ${errorText}`);
    }

    const data = await response.json() as {
      choices: Array<{
        message: {
          content: string | null;
          tool_calls?: Array<{
            id: string;
            type: 'function';
            function: { name: string; arguments: string };
          }>;
        };
      }>;
      usage?: { prompt_tokens: number; completion_tokens: number };
    };

    const choice = data.choices[0];
    if (!choice) throw new Error('No response choices');

    const result: LLMResponse = {
      usage: {
        prompt: data.usage?.prompt_tokens ?? 0,
        completion: data.usage?.completion_tokens ?? 0,
      },
    };

    if (choice.message.content) result.content = choice.message.content;

    if (choice.message.tool_calls?.length) {
      result.toolCalls = choice.message.tool_calls.map(tc => {
        let args: Record<string, unknown> = {};
        try { args = JSON.parse(tc.function.arguments); } catch { /* ignore */ }
        return { id: tc.id, name: tc.function.name, arguments: args };
      });
    }

    return result;
  }
}

// ─── Anthropic Provider ──────────────────────────────────────────────────────

class AnthropicProvider implements LLMProvider {
  readonly name: string;
  readonly model: string;
  readonly baseUrl = 'https://api.anthropic.com';
  private apiKey: string;

  constructor(config: { name: string; apiKey: string; model: string }) {
    this.name = config.name;
    this.apiKey = config.apiKey;
    this.model = config.model;
  }

  async chat(messages: Message[], tools?: ToolDefinition[]): Promise<LLMResponse> {
    const systemMsg = messages.find(m => m.role === 'system');
    const conversation = messages.filter(m => m.role !== 'system');

    const body: Record<string, unknown> = {
      model: this.model,
      max_tokens: 4096,
      messages: conversation.map(m => ({ role: m.role, content: m.content })),
    };

    if (systemMsg) body['system'] = systemMsg.content;

    if (tools?.length) {
      body['tools'] = tools.map(t => ({
        name: t.function.name,
        description: t.function.description,
        input_schema: t.function.parameters,
      }));
    }

    const response = await fetch(`${this.baseUrl}/v1/messages`, {
      method: 'POST',
      headers: {
        'x-api-key': this.apiKey,
        'Content-Type': 'application/json',
        'anthropic-version': '2023-06-01',
      },
      body: JSON.stringify(body),
    });

    if (!response.ok) {
      const errorText = await response.text();
      throw new Error(`Anthropic API error (${response.status}): ${errorText}`);
    }

    const data = await response.json() as {
      content: Array<{ type: string; text?: string; name?: string; input?: Record<string, unknown>; id?: string }>;
      usage?: { input_tokens: number; output_tokens: number };
    };

    const result: LLMResponse = {
      usage: {
        prompt: data.usage?.input_tokens ?? 0,
        completion: data.usage?.output_tokens ?? 0,
      },
    };

    const textParts = data.content.filter(c => c.type === 'text');
    if (textParts.length) result.content = textParts.map(c => c.text).join('\n');

    const toolUses = data.content.filter(c => c.type === 'tool_use');
    if (toolUses.length) {
      result.toolCalls = toolUses.map(tc => ({
        id: tc.id ?? '',
        name: tc.name ?? '',
        arguments: tc.input ?? {},
      }));
    }

    return result;
  }
}

// ─── Config-Driven Factory ───────────────────────────────────────────────────

/**
 * Create provider from config. No hardcoded providers.
 *
 * Config format in agclaw.json:
 * {
 *   "llm": {
 *     "providers": {
 *       "nvidia": {
 *         "base_url": "https://integrate.api.nvidia.com/v1",
 *         "api_key_env": "NVIDIA_API_KEY",
 *         "api": "openai",
 *         "models": ["deepseek-ai/deepseek-v3.2"]
 *       },
 *       "anthropic": {
 *         "base_url": "https://api.anthropic.com",
 *         "api_key_env": "ANTHROPIC_API_KEY",
 *         "api": "anthropic",
 *         "models": ["claude-sonnet-4-20250514"]
 *       }
 *     },
 *     "default": "nvidia",
 *     "fallback": ["nvidia/deepseek-ai/deepseek-v3.2", "openrouter/auto"]
 *   }
 * }
 */
export function createLLMProvider(config: {
  llm?: LLMConfig;
  provider?: string;
  model?: string;
  fallbackModels?: string[];
}): LLMProvider {
  const llmConfig = config.llm;

  // If llm config is provided, use config-driven approach
  if (llmConfig?.providers) {
    const providerName = config.provider ?? llmConfig.default ?? Object.keys(llmConfig.providers)[0];
    const providerConfig = llmConfig.providers[providerName];

    if (!providerConfig) {
      throw new Error(`Provider '${providerName}' not found in config. Available: ${Object.keys(llmConfig.providers).join(', ')}`);
    }

    const model = config.model ?? providerConfig.models?.[0] ?? 'default';

    // Resolve API key
    let apiKey: string | undefined;
    if (providerConfig.api_key) {
      apiKey = providerConfig.api_key;
    } else if (providerConfig.api_key_env) {
      apiKey = process.env[providerConfig.api_key_env];
    }
    if (!apiKey) {
      const envHint = providerConfig.api_key_env ? ` Set ${providerConfig.api_key_env} or api_key in agclaw.json` : ' Set api_key or api_key_env in agclaw.json';
      throw new Error(`No API key for provider '${providerName}'.${envHint}`);
    }

    // Collect fallback models from fallback chain config
    const fallbackModels: string[] = [];
    if (config.fallbackModels?.length) {
      fallbackModels.push(...config.fallbackModels);
    } else if (llmConfig.fallback) {
      for (const entry of llmConfig.fallback) {
        const [prov, mod] = entry.split('/');
        if (prov === providerName && mod && mod !== model) {
          fallbackModels.push(mod);
        }
      }
    }

    const api = providerConfig.api ?? 'openai';

    if (api === 'anthropic') {
      return new AnthropicProvider({
        name: providerName,
        apiKey,
        model,
      });
    }

    return new OpenAICompatibleProvider({
      name: providerName,
      baseUrl: providerConfig.base_url,
      apiKey,
      model,
      fallbackModels,
      extraHeaders: providerConfig.headers,
    });
  }

  // Fallback: legacy direct params (for backwards compat)
  const apiKey = process.env['OPENROUTER_API_KEY'];
  if (!apiKey) {
    throw new Error('No LLM config found. Run: agclaw onboard');
  }
  return new OpenAICompatibleProvider({
    name: 'legacy',
    baseUrl: 'https://openrouter.ai/api/v1',
    apiKey,
    model: config.model ?? 'auto',
    fallbackModels: config.fallbackModels,
  });
}
