/**
 * Dynamic model discovery via /v1/models API.
 * Mirrors the approach used by OpenClaw's plugin system.
 */
export interface DiscoveredModel {
    value: string;
    label: string;
    ctx: string;
    price: string;
    free?: boolean;
}
export interface Provider {
    value: string;
    label: string;
    hint: string;
    base_url: string;
    api_key_env: string;
    api: 'openai' | 'anthropic';
    headers?: Record<string, string>;
}
/**
 * Discover available models from a provider's live API.
 * Returns empty array on failure — caller falls back to curated list.
 */
export declare function discoverModels(provider: Provider, apiKey: string): Promise<DiscoveredModel[]>;
//# sourceMappingURL=modelDiscovery.d.ts.map