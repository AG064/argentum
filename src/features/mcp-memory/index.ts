/**
 * MCP Memory Feature — ClawMem / mem0 integration for AG-Claw
 *
 * Provides a unified memory interface over MCP (Model Context Protocol).
 * Supports multiple backends: ClawMem (local), mem0 (cloud/hybrid), or a custom MCP endpoint.
 *
 * Заметки по реализации:
 * - ClawMem (yoloshii) — локальный MCP-сервер на Bun, гибридный RAG (BM25 + векторный поиск + RRF + cross-encoder reranking)
 * - ClawMem (pigeonflow) — бинарный Rust-проект, 1.4MB, SQLite + centroid routing
 * - DeepExtrema/clawmem — NPM-пакеты (@clawmem/core + @clawmem/openclaw plugin)
 * - mem0 — облачный/гибридный сервис с API-ключом
 *
 * Используем JSON-RPC 2.0 поверх stdio для локальных MCP-серверов,
 * HTTP для облачных провайдеров (mem0).
 */

import { EventEmitter } from 'events';
import { spawn } from 'child_process';
import { Readable } from 'stream';

import {
  type FeatureModule,
  type FeatureContext,
  type FeatureMeta,
  type HealthStatus,
} from '../../core/plugin-loader';
import { Logger } from '../../core/logger';

// ─── Config ────────────────────────────────────────────────────────────────────

/** MCP Memory provider type */
export type MCPProvider = 'clawmem' | 'mem0' | 'custom';

/** Unified MCP memory configuration */
export interface MCPConfig {
  /** Enable the MCP memory feature */
  enabled: boolean;
  /** Memory provider: 'clawmem' | 'mem0' | 'custom' */
  provider: MCPProvider;
  /** For custom provider: MCP server command or HTTP endpoint */
  endpoint?: string;
  /** For mem0: API key */
  apiKey?: string;
  /** For mem0 cloud: base URL */
  baseURL?: string;
  /** For custom: additional env vars passed to the MCP process */
  env?: Record<string, string>;
  /** Namespace for memory isolation (agent/user scoped) */
  namespace?: string;
}

// ─── Search Result ─────────────────────────────────────────────────────────────

/** A single search result with relevance score */
export interface MemorySearchResult {
  key: string;
  score: number;
  content?: string;
  metadata?: Record<string, unknown>;
}

// ─── JSON-RPC Types (subset) ───────────────────────────────────────────────────

interface JSONRPCRequest {
  jsonrpc: '2.0';
  id: number | string;
  method: string;
  params?: Record<string, unknown>;
}

interface JSONRPCResponse {
  jsonrpc: '2.0';
  id: number | string;
  result?: unknown;
  error?: { code: number; message: string; data?: unknown };
}

// ─── MCPMemory ─────────────────────────────────────────────────────────────────

/**
 * Unified MCP-backed memory layer.
 *
 * Wraps a ClawMem MCP server, mem0 API, or any custom MCP-compatible endpoint
 * behind a single clean interface with store / retrieve / search / delete.
 *
 * @example
 * ```typescript
 * const mem = new MCPMemory({
 *   enabled: true,
 *   provider: 'clawmem',
 *   endpoint: 'clawmem', // binary in PATH, or full path
 *   namespace: 'ag-claw',
 * });
 * await mem.start();
 * await mem.store('user_pref_dark', 'User prefers dark mode', 'preference');
 * const results = await mem.search('color theme preference');
 * ```
 */
export class MCPMemory extends EventEmitter {
  private config: Required<MCPConfig>;
  private process?: ReturnType<typeof spawn>;
  private requestId = 0;
  private pendingRequests = new Map<number, {
    resolve: (value: unknown) => void;
    reject: (reason: unknown) => void;
  }>();
  private stdoutBuffer = '';
  private logger: Logger;
  private _ready = false;

  constructor(config: MCPConfig) {
    super();
    this.config = {
      enabled: config.enabled ?? false,
      provider: config.provider ?? 'clawmem',
      endpoint: config.endpoint ?? 'clawmem',
      apiKey: config.apiKey ?? '',
      baseURL: config.baseURL ?? 'https://api.mem0.ai/v1',
      env: config.env ?? {},
      namespace: config.namespace ?? 'default',
    };
    this.logger = new Logger('MCPMemory');
  }

  // ─── Lifecycle ───────────────────────────────────────────────────────────────

  /**
   * Start the MCP memory provider.
   * For local MCP servers (clawmem), spawns the process.
   * For mem0, validates connectivity.
   */
  async start(): Promise<void> {
    if (!this.config.enabled) {
      this.logger.info('MCP Memory is disabled');
      return;
    }

    if (this.config.provider === 'mem0') {
      await this.initMem0();
      return;
    }

    // Local MCP server (clawmem or custom binary)
    await this.spawnMCPServer();
  }

  /** Stop and clean up the MCP server process */
  async stop(): Promise<void> {
    if (this.process) {
      this.process.kill('SIGTERM');
      this.process = undefined;
    }
    this._ready = false;
    this.pendingRequests.clear();
    this.stdoutBuffer = '';
  }

  // ─── Memory Operations ────────────────────────────────────────────────────────

  /**
   * Store a memory entry.
   *
   * @param key  Unique identifier for this memory
   * @param value  Memory content (text)
   * @param tier  Optional memory tier: 'episodic' | 'semantic' | 'procedural' | 'preference'
   */
  async store(key: string, value: string, tier?: string): Promise<void> {
    // Для pigeonflow/clawmem: memory.upsert
    // Для DeepExtrema: memory.add или store
    // Для mem0: POST /memories
    const payload = {
      type: tier ?? 'episodic',
      content: value,
      id: key,
      tags: { namespace: this.config.namespace },
    };

    try {
      if (this.config.provider === 'mem0') {
        await this.mem0Upsert(key, value, tier);
      } else {
        await this.mcpCall('memory.upsert', payload);
      }
    } catch (err) {
      this.logger.error(`Failed to store memory [${key}]`, { error: err });
      throw err;
    }
  }

  /**
   * Retrieve a single memory by key.
   *
   * @param key  Memory identifier
   * @returns The memory content, or null if not found
   */
  async retrieve(key: string): Promise<string | null> {
    try {
      if (this.config.provider === 'mem0') {
        return await this.mem0Retrieve(key);
      }

      // MCP call — key may be stored as 'id' or 'key' field depending on provider
      const result = await this.mcpCall('memory.get', { id: key })
        ?? await this.mcpCall('memory.get', { key });

      if (!result) return null;
      return (result as { content?: string; value?: string })?.content
        ?? (result as { value?: string })?.value
        ?? JSON.stringify(result);
    } catch (err) {
      this.logger.warn(`Failed to retrieve memory [${key}]`, { error: err });
      return null;
    }
  }

  /**
   * Semantic search over stored memories.
   *
   * @param query  Natural-language search query
   * @returns Array of results sorted by relevance score (descending)
   */
  async search(query: string): Promise<Array<{ key: string; score: number }>> {
    try {
      if (this.config.provider === 'mem0') {
        return await this.mem0Search(query);
      }

      const result = await this.mcpCall('memory.search', {
        query,
        namespace: this.config.namespace,
        k: 10,
      });

      if (!result || !Array.isArray(result)) return [];

      // Normalize across different MCP response shapes
      return (result as Array<Record<string, unknown>>).map((item) => ({
        key: String(item.id ?? item.key ?? item.memory_id ?? ''),
        score: typeof item.score === 'number' ? item.score
          : typeof item.relevance === 'number' ? item.relevance
          : 0.5,
        content: item.content as string | undefined,
        metadata: item.metadata as Record<string, unknown> | undefined,
      }));
    } catch (err) {
      this.logger.error('Memory search failed', { query, error: err });
      return [];
    }
  }

  /**
   * Delete a memory entry by key.
   *
   * @param key  Memory identifier
   */
  async delete(key: string): Promise<void> {
    try {
      if (this.config.provider === 'mem0') {
        await this.mem0Delete(key);
      } else {
        await this.mcpCall('memory.delete', { id: key });
      }
    } catch (err) {
      this.logger.warn(`Failed to delete memory [${key}]`, { error: err });
    }
  }

  // ─── Health ──────────────────────────────────────────────────────────────────

  /** Health check — returns status of the MCP memory backend */
  async healthCheck(): Promise<HealthStatus> {
    if (!this.config.enabled) {
      return { healthy: true, message: 'MCP Memory disabled' };
    }

    try {
      if (this.config.provider === 'mem0') {
        // Quick API ping
        const res = await fetch(`${this.config.baseURL}/health`, {
          headers: { Authorization: `Bearer ${this.config.apiKey}` },
          signal: AbortSignal.timeout(3000),
        });
        return res.ok
          ? { healthy: true }
          : { healthy: false, message: `mem0 API ${res.status}` };
      }

      // Try a lightweight MCP call
      await this.mcpCall('memory.stats', {});
      return { healthy: true };
    } catch (err) {
      return { healthy: false, message: String(err) };
    }
  }

  // ─── Private: MCP Server ────────────────────────────────────────────────────

  private async spawnMCPServer(): Promise<void> {
    const cmd = this.config.endpoint;

    return new Promise((resolve, reject) => {
      const env = { ...process.env, ...this.config.env };

      this.process = spawn(cmd, ['serve'], {
        stdio: ['pipe', 'pipe', 'pipe'],
        env,
      });

      if (!this.process.stdout || !this.process.stdin) {
        reject(new Error('Failed to pipe MCP server stdio'));
        return;
      }

      this.process.stdout.on('data', (chunk: Buffer) => {
        this.stdoutBuffer += chunk.toString();
        this.processStdout();
      });

      this.process.stderr.on('data', (chunk: Buffer) => {
        this.logger.debug('[MCP stderr]', chunk.toString().trim());
      });

      this.process.on('error', (err) => {
        this.logger.error('MCP server process error', { error: err });
        this._ready = false;
      });

      this.process.on('exit', (code) => {
        this.logger.info('MCP server exited', { code });
        this._ready = false;
      });

      // Send initialize handshake
      this.sendRaw({
        jsonrpc: '2.0',
        id: ++this.requestId,
        method: 'initialize',
        params: {
          protocolVersion: '2025-06-18',
          capabilities: { tools: {} },
          clientInfo: { name: 'ag-claw', version: '1.0.0' },
        },
      }).catch(reject);

      // Quick ready wait (1.5s max)
      setTimeout(() => {
        this._ready = true;
        this.logger.info('MCP Memory server ready', { provider: this.config.provider });
        resolve();
      }, 1500);
    });
  }

  private processStdout(): void {
    const lines = this.stdoutBuffer.split('\n');
    this.stdoutBuffer = lines.pop() ?? '';

    for (const line of lines) {
      if (!line.trim()) continue;
      try {
        const res: JSONRPCResponse = JSON.parse(line);
        const pending = this.pendingRequests.get(res.id as number);
        if (pending) {
          this.pendingRequests.delete(res.id as number);
          if (res.error) {
            pending.reject(new Error(`[${res.error.code}] ${res.error.message}`));
          } else {
            pending.resolve(res.result);
          }
        }
      } catch {
        // Not a JSON-RPC response line, skip
      }
    }
  }

  private async sendRaw(req: JSONRPCRequest): Promise<void> {
    if (!this.process?.stdin) throw new Error('MCP server not running');
    return new Promise((resolve, reject) => {
      const line = JSON.stringify(req) + '\n';
      const ok = this.process!.stdin.write(line, () => resolve());
      if (!ok) reject(new Error('write failed'));
    });
  }

  private async mcpCall(
    method: string,
    params: Record<string, unknown> = {},
  ): Promise<unknown> {
    if (!this._ready && this.config.provider !== 'mem0') {
      throw new Error('MCP server not ready');
    }

    const id = ++this.requestId;

    return new Promise((resolve, reject) => {
      this.pendingRequests.set(id, { resolve, reject });

      this.sendRaw({ jsonrpc: '2.0', id, method, params }).catch((err) => {
        this.pendingRequests.delete(id);
        reject(err);
      });

      // 10s timeout
      setTimeout(() => {
        if (this.pendingRequests.has(id)) {
          this.pendingRequests.delete(id);
          reject(new Error(`MCP call ${method} timed out`));
        }
      }, 10000);
    });
  }

  // ─── Private: mem0 Cloud ─────────────────────────────────────────────────────

  private async initMem0(): Promise<void> {
    if (!this.config.apiKey) {
      throw new Error('mem0 apiKey is required');
    }
    this._ready = true;
    this.logger.info('mem0 Memory client initialized', { baseURL: this.config.baseURL });
  }

  private async mem0Upsert(key: string, value: string, tier?: string): Promise<void> {
    const res = await fetch(`${this.config.baseURL}/memories`, {
      method: 'POST',
      headers: {
        'Authorization': `Bearer ${this.config.apiKey}`,
        'Content-Type': 'application/json',
      },
      body: JSON.stringify({
        messages: [{ role: 'user', content: value }],
        user_id: this.config.namespace,
        memory_type: tier ?? 'episodic',
        external_id: key,
      }),
      signal: AbortSignal.timeout(10000),
    });

    if (!res.ok) {
      throw new Error(`mem0 upsert failed: ${res.status} ${await res.text()}`);
    }
  }

  private async mem0Retrieve(key: string): Promise<string | null> {
    const res = await fetch(
      `${this.config.baseURL}/memories?user_id=${encodeURIComponent(this.config.namespace)}`,
      {
        headers: { 'Authorization': `Bearer ${this.config.apiKey}` },
        signal: AbortSignal.timeout(10000),
      },
    );

    if (!res.ok) return null;
    const data = await res.json() as { results?: Array<{ content?: string; memory_id?: string }> };
    const match = data.results?.find((m) => m.memory_id === key);
    return match?.content ?? null;
  }

  private async mem0Search(query: string): Promise<Array<{ key: string; score: number }>> {
    const res = await fetch(`${this.config.baseURL}/search`, {
      method: 'POST',
      headers: {
        'Authorization': `Bearer ${this.config.apiKey}`,
        'Content-Type': 'application/json',
      },
      body: JSON.stringify({ query, user_id: this.config.namespace }),
      signal: AbortSignal.timeout(10000),
    });

    if (!res.ok) return [];
    const data = await res.json() as { results?: Array<{ memory_id?: string; score?: number }> };
    return (data.results ?? []).map((r) => ({
      key: r.memory_id ?? '',
      score: r.score ?? 0,
    }));
  }

  private async mem0Delete(key: string): Promise<void> {
    await fetch(`${this.config.baseURL}/memories/${encodeURIComponent(key)}`, {
      method: 'DELETE',
      headers: { 'Authorization': `Bearer ${this.config.apiKey}` },
      signal: AbortSignal.timeout(10000),
    });
  }
}

// ─── Feature Module ────────────────────────────────────────────────────────────

/**
 * MCP Memory Feature for AG-Claw
 *
 * Registers MCPMemory as a first-class feature module with the plugin loader.
 * Configure in config/mcp-memory.json or via environment variables:
 *
 * CLAWMEM_ENABLED=1
 * CLAWMEM_PROVIDER=clawmem        # clawmem | mem0 | custom
 * CLAWMEM_ENDPOINT=clawmem       # binary path or HTTP URL
 * MEM0_API_KEY=sk-...            # required for mem0
 * MEM0_BASE_URL=https://...      # optional for mem0
 *
 * @example config/mcp-memory.json
 * {
 *   "enabled": true,
 *   "provider": "clawmem",
 *   "endpoint": "/usr/local/bin/clawmem",
 *   "namespace": "ag-claw"
 * }
 */
class MCPMemoryFeature implements FeatureModule {
  readonly meta: FeatureMeta = {
    name: 'mcp-memory',
    version: '1.0.0',
    description: 'MCP-backed memory layer (ClawMem, mem0, custom) — store, retrieve, search, delete',
    dependencies: [],
  };

  private config!: Required<MCPConfig>;
  private ctx!: FeatureContext;
  private memory!: MCPMemory;

  constructor() {
    // Defaults — can be overridden via config file or env vars
  }

  async init(config: Record<string, unknown>, context: FeatureContext): Promise<void> {
    this.ctx = context;

    // Merge config file + environment variables
    this.config = {
      enabled: resolveBool(config['enabled'], process.env['CLAWMEM_ENABLED']),
      provider: resolveEnum(
        config['provider'],
        process.env['CLAWMEM_PROVIDER'],
        ['clawmem', 'mem0', 'custom'],
        'clawmem',
      ) as MCPProvider,
      endpoint: resolveString(
        config['endpoint'],
        process.env['CLAWMEM_ENDPOINT'] ?? 'clawmem',
      ),
      apiKey: resolveString(config['apiKey'], process.env['MEM0_API_KEY'] ?? ''),
      baseURL: resolveString(
        config['baseURL'],
        process.env['MEM0_BASE_URL'] ?? 'https://api.mem0.ai/v1',
      ),
      env: (config['env'] as Record<string, string>) ?? {},
      namespace: resolveString(
        config['namespace'],
        process.env['CLAWMEM_NAMESPACE'] ?? 'ag-claw',
      ),
    };

    this.memory = new MCPMemory(this.config);
  }

  async start(): Promise<void> {
    await this.memory.start();
    this.ctx.logger.info('MCP Memory feature started', {
      provider: this.config.provider,
      namespace: this.config.namespace,
    });
  }

  async stop(): Promise<void> {
    await this.memory.stop();
  }

  async healthCheck(): Promise<HealthStatus> {
    return this.memory.healthCheck();
  }
}

// ─── Config Resolution Helpers ─────────────────────────────────────────────────

function resolveBool(value: unknown, env?: string): boolean {
  if (typeof value === 'boolean') return value;
  if (typeof value === 'string') return value === 'true' || value === '1';
  if (typeof env === 'string') return env === 'true' || env === '1';
  return false;
}

function resolveString(value: unknown, fallback: string): string {
  return typeof value === 'string' ? value : fallback;
}

function resolveEnum(
  value: unknown,
  env: string | undefined,
  choices: string[],
  fallback: string,
): string {
  const resolved = typeof value === 'string' ? value : (env ?? fallback);
  return choices.includes(resolved) ? resolved : fallback;
}

// ─── Exports ───────────────────────────────────────────────────────────────────

export { MCPMemoryFeature };
export type {
  MCPConfig,
  MCPProvider,
  MemorySearchResult,
};

export default MCPMemoryFeature;
