/**
 * Argentum LLM Provider — Fully Configurable
 *
 * All providers are defined in argentum.json. No hardcoded providers.
 * Any OpenAI-compatible API can be added by setting base_url + api_key.
 */
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
    function: {
        name: string;
        arguments: string;
    };
}
export interface ToolDefinition {
    type: 'function';
    function: {
        name: string;
        description: string;
        parameters: Record<string, unknown>;
    };
}
export interface LLMResponse {
    content?: string;
    toolCalls?: Array<{
        id: string;
        name: string;
        arguments: Record<string, unknown>;
    }>;
    usage: {
        prompt: number;
        completion: number;
    };
}
export interface LLMProvider {
    readonly name: string;
    readonly model: string;
    readonly baseUrl: string;
    chat(messages: Message[], tools?: ToolDefinition[]): Promise<LLMResponse>;
}
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
/**
 * Create provider from config. No hardcoded providers.
 *
 * Config format in argentum.json:
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
export declare function createLLMProvider(config: {
    llm?: LLMConfig;
    provider?: string;
    model?: string;
    fallbackModels?: string[];
}): LLMProvider;
//# sourceMappingURL=llm-provider.d.ts.map