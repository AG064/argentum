'use strict';
/**
 * Argentum LLM Provider Interface & Implementations
 *
 * Defines the LLM provider contract and implements OpenRouter API integration.
 */
Object.defineProperty(exports, '__esModule', { value: true });
exports.AnthropicProvider = exports.OpenRouterProvider = void 0;
exports.createLLMProvider = createLLMProvider;
const logger_1 = require('./logger');
const logger = (0, logger_1.createLogger)();
/** OpenRouter provider implementation */
class OpenRouterProvider {
  constructor(apiKey, model, fallbackModels = []) {
    this.name = 'openrouter';
    this.baseUrl = 'https://openrouter.ai/api/v1';
    this.apiKey = apiKey;
    this.model = model;
    this.fallbackModels = fallbackModels;
  }
  async chat(messages, tools) {
    const modelsToTry = [this.model, ...this.fallbackModels];
    let lastError = null;
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
  async callModel(model, messages, tools) {
    const body = {
      model,
      messages: messages.map((m) => {
        const msg = { role: m.role, content: m.content };
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
        'X-Title': 'Argentum',
      },
      body: JSON.stringify(body),
    });
    if (!response.ok) {
      const errorText = await response.text();
      throw new Error(`OpenRouter API error (${response.status}): ${errorText}`);
    }
    const data = await response.json();
    const choice = data.choices[0];
    if (!choice) {
      throw new Error('No response choices from OpenRouter');
    }
    const result = {
      usage: {
        prompt: data.usage?.prompt_tokens ?? 0,
        completion: data.usage?.completion_tokens ?? 0,
      },
    };
    if (choice.message.content) {
      result.content = choice.message.content;
    }
    if (choice.message.tool_calls && choice.message.tool_calls.length > 0) {
      result.toolCalls = choice.message.tool_calls.map((tc) => {
        let parsedArgs;
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
exports.OpenRouterProvider = OpenRouterProvider;
/** Anthropic Claude provider (direct API) */
class AnthropicProvider {
  constructor(apiKey, model = 'claude-sonnet-4-20250514') {
    this.name = 'anthropic';
    this.apiKey = apiKey;
    this.model = model;
  }
  async chat(messages, tools) {
    // Separate system message from conversation
    const systemMsg = messages.find((m) => m.role === 'system');
    const conversation = messages.filter((m) => m.role !== 'system');
    const body = {
      model: this.model,
      max_tokens: 4096,
      messages: conversation.map((m) => ({
        role: m.role,
        content: m.content,
      })),
    };
    if (systemMsg) {
      body.system = systemMsg.content;
    }
    if (tools && tools.length > 0) {
      body.tools = tools.map((t) => ({
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
    const data = await response.json();
    const result = {
      usage: {
        prompt: data.usage?.input_tokens ?? 0,
        completion: data.usage?.output_tokens ?? 0,
      },
    };
    const textParts = data.content.filter((c) => c.type === 'text');
    if (textParts.length > 0) {
      result.content = textParts.map((c) => c.text).join('\n');
    }
    const toolUses = data.content.filter((c) => c.type === 'tool_use');
    if (toolUses.length > 0) {
      result.toolCalls = toolUses.map((tc) => ({
        id: tc.id ?? '',
        name: tc.name ?? '',
        arguments: tc.input ?? {},
      }));
    }
    return result;
  }
}
exports.AnthropicProvider = AnthropicProvider;
/** Create LLM provider from config */
function createLLMProvider(config) {
  const provider = config.provider ?? 'openrouter';
  const model = config.model ?? 'anthropic/claude-sonnet-4-20250514';
  const fallbackModels = config.fallbackModels ?? [];
  switch (provider) {
    case 'openrouter': {
      const apiKey = process.env.OPENROUTER_API_KEY ?? process.env.AGCLAW_OPENROUTER_KEY;
      if (!apiKey) {
        throw new Error(
          'OPENROUTER_API_KEY or AGCLAW_OPENROUTER_KEY environment variable is required',
        );
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
    default:
      throw new Error(`Unknown LLM provider: ${provider}`);
  }
}
