/**
 * Dynamic model discovery via /v1/models API.
 * Mirrors the approach used by OpenClaw's plugin system.
 */

export interface DiscoveredModel {
  value: string;      // model id as sent to API
  label: string;      // human-readable name
  ctx: string;        // context window (e.g. "128k", "1M")
  price: string;      // price per 1M tokens, "FREE", or "?"
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

/** Fetch /v1/models from any OpenAI-compatible endpoint. */
async function fetchModelList(
  baseUrl: string,
  apiKey: string,
): Promise<{ id: string; context_window?: number; pricing?: { prompt?: number; completion?: number } }[]> {
  const url = `${baseUrl.replace(/\/$/, '')}/models`;
  const headers: Record<string, string> = {
    'Content-Type': 'application/json',
  };
  if (apiKey) {
    headers['Authorization'] = `Bearer ${apiKey}`;
  }

  const res = await fetch(url, {
    method: 'GET',
    headers,
    signal: AbortSignal.timeout(8000),
  });

  if (!res.ok) {
    throw new Error(`HTTP ${res.status}`);
  }

  const json = await res.json() as {
    object?: string;
    data?: { id: string; context_window?: number; pricing?: { prompt?: number; completion?: number } }[];
    models?: { name: string; details?: { context_length?: number } }[]; // Ollama format
  };

  // Ollama returns { models: [{ name, details }] }
  if (Array.isArray(json.models)) {
    return json.models.map((m) => ({
      id: m.name,
      context_window: m.details?.context_length,
    }));
  }

  return Array.isArray(json.data) ? json.data : [];
}

function formatCtx(n?: number): string {
  if (!n) return '?';
  if (n >= 1_000_000) return `${Math.round(n / 1_000_000)}M`;
  if (n >= 1000) return `${Math.round(n / 1000)}k`;
  return String(n);
}

function formatPrice(prompt?: number, completion?: number): string {
  const input = prompt ?? 0;
  const output = completion ?? 0;
  if (input === 0 && output === 0) return 'FREE';
  const avg = (input + output) / 2;
  if (avg === 0) return '?';
  return `$${avg.toFixed(3)}/M`;
}

/**
 * Discover available models from a provider's live API.
 * Returns empty array on failure — caller falls back to curated list.
 */
export async function discoverModels(
  provider: Provider,
  apiKey: string,
): Promise<DiscoveredModel[]> {
  try {
    const raw = await fetchModelList(provider.base_url, apiKey);

    // Sort: free first, then by context size descending
    const sorted = raw
      .filter((m) => m.id && !m.id.startsWith('.')) // skip hidden Ollama models
      .sort((a, b) => {
        const aFree = a.pricing?.prompt === 0 && a.pricing?.completion === 0;
        const bFree = b.pricing?.prompt === 0 && b.pricing?.completion === 0;
        if (aFree && !bFree) return -1;
        if (!aFree && bFree) return 1;
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
  } catch {
    return [];
  }
}
