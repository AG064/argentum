/**
 * AG-Claw LLM Provider Interface & Implementations
 *
 * Defines the LLM provider contract and implements OpenRouter API integration.
 */

import { createLogger } from './logger';

const logger = createLogger();

/** Message role types */
export type MessageRole = 'system' | 'user' | 'assistant' | 'tool';

/** Chat message */
export interface Message {
  role: MessageRole;
  content: string;
  name?: string;
  tool_call_id?: string;
  tool_calls?: ToolCall[];
}

/** Tool call from LLM */
export interface ToolCall {
  id: string;
  type: 'function';
  function: {
    name: string;
    arguments: string;
  };
}

/** Tool definition for LLM */
export interface ToolDefinition {
  type: 'function';
  function: {
    name: string;
    description: string;
    parameters: Record<string, unknown>;
  };
}

/** LLM response */
export interface LLMResponse {
  content?: string;
  toolCalls?: Array<{ id: string; name: string; arguments: Record<string, unknown> }>;
  usage: {
    prompt: number;
    completion: number;
  };
}

/** LLM provider interface */
export interface LLMProvider {
  readonly name: string;
  chat(messages: Message[], tools?: ToolDefinition[]): Promise<LLMResponse>;
}

/** OpenRouter provider implementation */
export class OpenRouterProvider implements LLMProvider {
  readonly name = 'openrouter';
  private apiKey: string;
  private model: string;
  private fallbackModels: string[];
  private baseUrl: string;

  constructor(apiKey: string, model: string, fallbackModels: string[] = [], baseUrl?: string) {
    this.apiKey = apiKey;
    this.model = model;
    this.fallbackModels = fallbackModels;
    this.baseUrl = baseUrl ?? 'https://openrouter.ai/api/v1';
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

    throw lastError ?? new Error('All LLM models failed');
  }

  private async callModel(model: string, messages: Message[], tools?: ToolDefinition[]): Promise<LLMResponse> {
    const body: Record<string, unknown> = {
      model,
      messages: messages.map(m => {
        const msg: Record<string, unknown> = { role: m.role, content: m.content };
        if (m.name) msg.name = m.name;
        if (m.tool_call_id) msg.tool_call_id = m.tool_call_id;
        if (m.tool_calls) msg.tool_calls = m.tool_calls;
        return msg;
      }),
      max_tokens: 4096,
      temperature: 0.7,
    };

    if (tools && tools.length > 0) {
      body.tools = tools;
      body.tool_choice = 'auto';
    }

    const response = await fetch(`${this.baseUrl}/chat/completions`, {
      method: 'POST',
      headers: {
        'Authorization': `Bearer ${this.apiKey}`,
        'Content-Type': 'application/json',
        'HTTP-Referer': 'https://github.com/AG064/ag-claw',
        'X-Title': 'AG-Claw',
      },
      body: JSON.stringify(body),
    });

    if (!response.ok) {
      const errorText = await response.text();
      throw new Error(`OpenRouter API error (${response.status}): ${errorText}`);
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
      usage?: {
        prompt_tokens: number;
        completion_tokens: number;
      };
    };

    const choice = data.choices[0];
    if (!choice) {
      throw new Error('No response choices from OpenRouter');
    }

    const result: LLMResponse = {
      usage: {
        prompt: data.usage?.prompt_tokens ?? 0,
        completion: data.usage?.completion_tokens ?? 0,
      },
    };

    if (choice.message.content) {
      result.content = choice.message.content;
    }

    if (choice.message.tool_calls && choice.message.tool_calls.length > 0) {
      result.toolCalls = choice.message.tool_calls.map(tc => {
        let parsedArgs: Record<string, unknown>;
        try {
          parsedArgs = JSON.parse(tc.function.arguments);
        } catch {
          parsedArgs = {};
        }
        return {
          id: tc.id,
          name: tc.function.name,
          arguments: parsedArgs,
        };
      });
    }

    return result;
  }
}

/** Anthropic Claude provider (direct API) */
export class AnthropicProvider implements LLMProvider {
  readonly name = 'anthropic';
  private apiKey: string;
  private model: string;

  constructor(apiKey: string, model: string = 'claude-sonnet-4-20250514') {
    this.apiKey = apiKey;
    this.model = model;
  }

  async chat(messages: Message[], tools?: ToolDefinition[]): Promise<LLMResponse> {
    // Separate system message from conversation
    const systemMsg = messages.find(m => m.role === 'system');
    const conversation = messages.filter(m => m.role !== 'system');

    const body: Record<string, unknown> = {
      model: this.model,
      max_tokens: 4096,
      messages: conversation.map(m => ({
        role: m.role,
        content: m.content,
      })),
    };

    if (systemMsg) {
      body.system = systemMsg.content;
    }

    if (tools && tools.length > 0) {
      body.tools = tools.map(t => ({
        name: t.function.name,
        description: t.function.description,
        input_schema: t.function.parameters,
      }));
    }

    const response = await fetch('https://api.anthropic.com/v1/messages', {
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
    if (textParts.length > 0) {
      result.content = textParts.map(c => c.text).join('\n');
    }

    const toolUses = data.content.filter(c => c.type === 'tool_use');
    if (toolUses.length > 0) {
      result.toolCalls = toolUses.map(tc => ({
        id: tc.id ?? '',
        name: tc.name ?? '',
        arguments: tc.input ?? {},
      }));
    }

    return result;
  }
}

/** Create LLM provider from config */
export function createLLMProvider(config: {
  provider?: string;
  model?: string;
  fallbackModels?: string[];
}): LLMProvider {
  const provider = config.provider ?? 'openrouter';
  const model = config.model ?? 'anthropic/claude-sonnet-4-20250514';
  const fallbackModels = config.fallbackModels ?? [];

  switch (provider) {
    case 'openrouter': {
      const apiKey = process.env.OPENROUTER_API_KEY ?? process.env.AGCLAW_OPENROUTER_KEY;
      if (!apiKey) {
        throw new Error('OPENROUTER_API_KEY or AGCLAW_OPENROUTER_KEY environment variable is required');
      }
      return new OpenRouterProvider(apiKey, model, fallbackModels);
    }
    case 'anthropic': {
      const apiKey = process.env.ANTHROPIC_API_KEY;
      if (!apiKey) {
        throw new Error('ANTHROPIC_API_KEY environment variable is required');
      }
      return new AnthropicProvider(apiKey, model);
    }
    case 'nvidia': {
      const apiKey = process.env.NVIDIA_API_KEY ?? process.env.AGCLAW_NVIDIA_KEY;
      if (!apiKey) {
        throw new Error('NVIDIA_API_KEY or AGCLAW_NVIDIA_KEY environment variable is required');
      }
      return new OpenRouterProvider(apiKey, model ?? 'deepseek-ai/deepseek-v3.2', fallbackModels, 'https://integrate.api.nvidia.com/v1');
    }
    default:
      throw new Error(`Unknown LLM provider: ${provider}`);
  }
}
