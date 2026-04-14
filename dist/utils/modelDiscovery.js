"use strict";
/**
 * Dynamic model discovery via /v1/models API.
 * Mirrors the approach used by OpenClaw's plugin system.
 */
Object.defineProperty(exports, "__esModule", { value: true });
exports.discoverModels = discoverModels;
/** Fetch /v1/models from any OpenAI-compatible endpoint. */
async function fetchModelList(baseUrl, apiKey) {
    const url = `${baseUrl.replace(/\/$/, '')}/models`;
    const headers = {
        'Content-Type': 'application/json',
    };
    if (apiKey) {
        headers.Authorization = `Bearer ${apiKey}`;
    }
    const res = await fetch(url, {
        method: 'GET',
        headers,
        signal: AbortSignal.timeout(8000),
    });
    if (!res.ok) {
        throw new Error(`HTTP ${res.status}`);
    }
    const json = await res.json();
    // Ollama returns { models: [{ name, details }] }
    if (Array.isArray(json.models)) {
        return json.models.map((m) => ({
            id: m.name,
            context_window: m.details?.context_length,
        }));
    }
    return Array.isArray(json.data) ? json.data : [];
}
function formatCtx(n) {
    if (!n)
        return '?';
    if (n >= 1000000)
        return `${Math.round(n / 1000000)}M`;
    if (n >= 1000)
        return `${Math.round(n / 1000)}k`;
    return String(n);
}
function formatPrice(prompt, completion) {
    const input = prompt ?? 0;
    const output = completion ?? 0;
    if (input === 0 && output === 0)
        return 'FREE';
    const avg = (input + output) / 2;
    if (avg === 0)
        return '?';
    return `$${avg.toFixed(3)}/M`;
}
/**
 * Discover available models from a provider's live API.
 * Returns empty array on failure — caller falls back to curated list.
 */
async function discoverModels(provider, apiKey) {
    try {
        const raw = await fetchModelList(provider.base_url, apiKey);
        // Sort: free first, then by context size descending
        const sorted = raw
            .filter((m) => m.id && !m.id.startsWith('.')) // skip hidden Ollama models
            .sort((a, b) => {
            const aFree = a.pricing?.prompt === 0 && a.pricing?.completion === 0;
            const bFree = b.pricing?.prompt === 0 && b.pricing?.completion === 0;
            if (aFree && !bFree)
                return -1;
            if (!aFree && bFree)
                return 1;
            return (b.context_window ?? 0) - (a.context_window ?? 0);
        })
            .slice(0, 50); // cap at 50 to keep select usable
        return sorted.map((m) => ({
            value: m.id,
            label: m.id,
            ctx: formatCtx(m.context_window),
            price: formatPrice(m.pricing?.prompt, m.pricing?.completion),
            free: m.pricing?.prompt === 0 && m.pricing?.completion === 0,
        }));
    }
    catch {
        return [];
    }
}
