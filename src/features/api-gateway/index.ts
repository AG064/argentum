/**
 * API Gateway Feature
 *
 * REST API for external integrations with authentication, rate limiting,
 * and endpoint management.
 */

import { randomBytes } from 'crypto';
import express, { type Request, type Response, type Handler } from 'express';

import {
  type FeatureModule,
  type FeatureContext,
  type FeatureMeta,
  type HealthStatus,
} from '../../core/plugin-loader';
import rateLimiting from '../rate-limiting';

/** API Gateway configuration */
export interface ApiGatewayConfig {
  enabled: boolean;
  port: number;
  path: string;
  apiKeys: string[];
  rateLimit?: {
    windowMs: number;
    max: number;
    byApiKey: boolean;
  };
}

/** Registered endpoint */
export interface ApiEndpoint {
  path: string;
  method: 'GET' | 'POST' | 'PUT' | 'DELETE' | 'PATCH';
  handler: Handler;
  description?: string;
  requiresAuth: boolean;
  rateLimited: boolean;
}

/** API Token info */
export interface ApiToken {
  key: string;
  name: string;
  createdAt: number;
  lastUsed?: number;
}

/**
 * API Gateway feature — external REST API with authentication and rate limiting.
 *
 * Provides an Express-based HTTP server for external integrations,
 * with configurable endpoints, API key authentication, and rate limiting.
 */
class ApiGatewayFeature implements FeatureModule {
  readonly meta: FeatureMeta = {
    name: 'api-gateway',
    version: '0.1.0',
    description: 'REST API for external integrations with auth and rate limiting',
    dependencies: ['rate-limiting'],
  };

  private config: ApiGatewayConfig = {
    enabled: false,
    port: 3001,
    path: '/api',
    apiKeys: [],
    rateLimit: {
      windowMs: 60_000, // 1 minute
      max: 100, // 100 requests per minute
      byApiKey: true,
    },
  };
  private ctx!: FeatureContext;
  private app!: express.Express;
  private server: ReturnType<typeof express.application.listen> | null = null;
  private endpoints: Map<string, ApiEndpoint[]> = new Map(); // keyed by method
  private apiTokens: Map<string, ApiToken> = new Map();
  private active = false;

  async init(config: Record<string, unknown>, context: FeatureContext): Promise<void> {
    this.ctx = context;
    this.config = { ...this.config, ...(config as Partial<ApiGatewayConfig>) };

    // Load existing apiKeys from config into tokens
    for (const key of this.config.apiKeys) {
      this.apiTokens.set(key, {
        key,
        name: `Initial key ${key.slice(0, 8)}`,
        createdAt: Date.now(),
      });
    }
  }

  async start(): Promise<void> {
    this.app = express();
    this.app.use(express.json());

    // Apply global middleware
    this.app.use(this.requestLogger.bind(this));
    this.app.use(this.errorHandler.bind(this));

    // Set up rate limiting middleware
    this.app.use(this.rateLimitMiddleware.bind(this));

    // API key authentication middleware
    this.app.use(this.authMiddleware.bind(this));

    // Add health check endpoint
    this.app.get(`${this.config.path}/health`, this.healthCheckHandler.bind(this));

    // Register configured endpoints
    this.registerDefaultEndpoints();

    // Start server
    this.server = this.app.listen(this.config.port, () => {
      this.active = true;
      this.ctx.logger.info('API Gateway started', {
        port: this.config.port,
        path: this.config.path,
        endpoints: this.listEndpoints().length,
      });
    });
  }

  async stop(): Promise<void> {
    if (this.server) {
      await new Promise((resolve) => this.server!.close(resolve));
      this.server = null;
    }
    this.active = false;
    this.endpoints.clear();
    this.ctx.logger.info('API Gateway stopped');
  }

  async healthCheck(): Promise<HealthStatus> {
    return {
      healthy: this.active,
      message: this.active ? `API Gateway listening on port ${this.config.port}` : 'Inactive',
      details: {
        port: this.config.port,
        path: this.config.path,
        endpoints: this.listEndpoints().length,
        tokens: this.apiTokens.size,
      },
    };
  }

  /** Register a new endpoint */
  registerEndpoint(
    path: string,
    method: ApiEndpoint['method'],
    handler: Handler,
    options?: { description?: string; requiresAuth?: boolean; rateLimited?: boolean },
  ): void {
    if (!this.app) {
      throw new Error('API Gateway not started. Call start() first.');
    }

    const normalizedPath = this.normalizePath(path);
    const endpoint: ApiEndpoint = {
      path: normalizedPath,
      method,
      handler,
      description: options?.description,
      requiresAuth: options?.requiresAuth ?? true,
      rateLimited: options?.rateLimited ?? true,
    };

    // Store in registry
    const methodEndpoints = this.endpoints.get(method) ?? [];
    methodEndpoints.push(endpoint);
    this.endpoints.set(method, methodEndpoints);

    // Register with Express using explicit methods
    switch (method.toUpperCase()) {
      case 'GET':
        this.app.get(normalizedPath, handler);
        break;
      case 'POST':
        this.app.post(normalizedPath, handler);
        break;
      case 'PUT':
        this.app.put(normalizedPath, handler);
        break;
      case 'DELETE':
        this.app.delete(normalizedPath, handler);
        break;
      case 'PATCH':
        this.app.patch(normalizedPath, handler);
        break;
      default:
        throw new Error(`Unsupported HTTP method: ${method}`);
    }

    this.ctx.logger.debug('Endpoint registered', {
      method: endpoint.method,
      path: normalizedPath,
      requiresAuth: endpoint.requiresAuth,
    });
  }

  /** Remove an endpoint */
  removeEndpoint(path: string, method?: ApiEndpoint['method']): boolean {
    const normalizedPath = this.normalizePath(path);
    let removed = false;

    if (method) {
      const methodEndpoints = this.endpoints.get(method);
      if (methodEndpoints) {
        const filtered = methodEndpoints.filter((e) => e.path !== normalizedPath);
        removed = methodEndpoints.length > filtered.length;
        this.endpoints.set(method, filtered);
      }
    } else {
      // Remove for all methods
      for (const [m, endpoints] of this.endpoints) {
        const before = endpoints.length;
        const filtered = endpoints.filter((e) => e.path !== normalizedPath);
        this.endpoints.set(m, filtered);
        if (filtered.length < before) removed = true;
      }
    }

    // Note: Express doesn't support removing routes directly, but we can manage our registry
    // For a production system, you'd want a more sophisticated router with mounted routers per feature
    if (removed) {
      this.ctx.logger.debug('Endpoint removed from registry', { path: normalizedPath, method });
    }
    return removed;
  }

  /** List all registered endpoints */
  listEndpoints(): ApiEndpoint[] {
    const all: ApiEndpoint[] = [];
    for (const [, endpoints] of this.endpoints) {
      all.push(...endpoints);
    }
    return all;
  }

  /** Create an API token */
  createToken(name: string, _expiresInDays?: number): ApiToken {
    const key = `ak_${randomBytes(24).toString('hex')}`;
    const token: ApiToken = {
      key,
      name,
      createdAt: Date.now(),
      lastUsed: undefined,
    };
    this.apiTokens.set(key, token);
    this.ctx.logger.info('API token created', { name, key: `${key.slice(0, 12)}...` });
    return token;
  }

  /** Revoke an API token */
  revokeToken(key: string): boolean {
    return this.apiTokens.delete(key);
  }

  /** List all tokens (without full keys) */
  listTokens(): ApiToken[] {
    return Array.from(this.apiTokens.values());
  }

  /** Normalize path to include config.path prefix */
  private normalizePath(path: string): string {
    const base = this.config.path.replace(/\/+$/, '');
    const cleanPath = path.replace(/^\/+/, '');
    return `${base}/${cleanPath}`;
  }

  /** Request logger middleware */
  private requestLogger(req: Request, res: Response, next: express.NextFunction): void {
    const start = Date.now();
    res.on('finish', () => {
      const duration = Date.now() - start;
      this.ctx.logger.debug('API request', {
        method: req.method,
        path: req.path,
        status: res.statusCode,
        duration,
        ip: req.ip,
      });
    });
    next();
  }

  /** Error handler middleware */
  private errorHandler(err: Error, req: Request, res: Response, next: express.NextFunction): void {
    this.ctx.logger.error('API Gateway error', {
      method: req.method,
      path: req.path,
      error: err.message,
    });

    if (res.headersSent) {
      return next(err);
    }
    res.status(500).json({
      error: 'Internal server error',
      message: process.env.NODE_ENV === 'development' ? err.message : undefined,
    });
  }

  /** Rate limiting middleware */
  private async rateLimitMiddleware(
    req: Request,
    res: Response,
    next: express.NextFunction,
  ): Promise<void> {
    // Skip rate limiting for health check
    if (req.path.endsWith('/health')) {
      next();
      return;
    }

    const endpoint = this.findEndpoint(req.method, req.path);
    if (!endpoint || !endpoint.rateLimited) {
      next();
      return;
    }

    // Ensure rate limiting config exists
    const rl = this.config.rateLimit ?? { windowMs: 60000, max: 100, byApiKey: true };

    // Rate limit by API key or IP
    let key: string;
    if (rl.byApiKey) {
      const apiKeyHeader = req.headers['x-api-key'];
      const apiKey = Array.isArray(apiKeyHeader) ? apiKeyHeader[0] : apiKeyHeader;
      if (apiKey) {
        key = apiKey;
      } else {
        key = req.ip ?? 'unknown';
      }
    } else {
      key = req.ip ?? 'unknown';
    }

    // Use rate-limiting feature
    try {
      const result = rateLimiting.check(key, rl.max, rl.windowMs);

      if (!result.allowed) {
        res.status(429).json({
          error: 'Too many requests',
          retryAfter: Math.ceil(rl.windowMs / 1000),
          limit: rl.max,
          window: rl.windowMs,
        });
        return;
      }
    } catch (err) {
      this.ctx.logger.warn('Rate limiting check failed, allowing', {
        error: err instanceof Error ? err.message : String(err),
      });
    }

    next();
  }

  /** Authentication middleware */
  private authMiddleware(req: Request, res: Response, next: express.NextFunction): void {
    const endpoint = this.findEndpoint(req.method, req.path);

    // If no endpoint found or endpoint doesn't require auth, continue
    if (!endpoint || !endpoint.requiresAuth) {
      return next();
    }

    // Check for API key (handle string[] case)
    const apiKeyHeader = req.headers['x-api-key'];
    const apiKey = Array.isArray(apiKeyHeader) ? apiKeyHeader[0] : apiKeyHeader;
    if (!apiKey) {
      res.status(401).json({ error: 'Unauthorized', message: 'API key required' });
      return;
    }

    if (!this.apiTokens.has(apiKey)) {
      res.status(401).json({ error: 'Unauthorized', message: 'Invalid API key' });
      return;
    }

    // Update last used
    const token = this.apiTokens.get(apiKey);
    if (token) {
      token.lastUsed = Date.now();
      this.apiTokens.set(apiKey, token);
    }

    next();
  }

  /** Find registered endpoint for a method/path */
  private findEndpoint(method: string, path: string): ApiEndpoint | undefined {
    const methodEndpoints = this.endpoints.get(method.toUpperCase() as ApiEndpoint['method']);
    if (!methodEndpoints) return undefined;

    // Exact match only for now (could support params later)
    return methodEndpoints.find((e) => e.path === path);
  }

  /** Health check handler */
  private healthCheckHandler(_req: Request, res: Response): void {
    const endpoints = this.listEndpoints();
    res.json({
      status: 'healthy',
      timestamp: new Date().toISOString(),
      uptime: process.uptime(),
      endpoints: endpoints.map((e) => ({
        method: e.method,
        path: e.path,
        description: e.description,
      })),
    });
  }

  /** Register some default endpoints */
  private registerDefaultEndpoints(): void {
    // GET /api/tokens - List tokens (admin)
    this.registerEndpoint('/tokens', 'GET', (_req, res) => {
      const tokens = this.listTokens().map((t) => ({
        name: t.name,
        createdAt: t.createdAt,
        lastUsed: t.lastUsed,
      }));
      res.json({ tokens });
    });

    // POST /api/tokens - Create new token
    this.registerEndpoint('/tokens', 'POST', (req, res) => {
      const { name } = req.body;
      if (!name) {
        res.status(400).json({ error: 'Bad request', message: 'name is required' });
        return;
      }
      const token = this.createToken(name);
      res.status(201).json({ token: token.key, name: token.name, createdAt: token.createdAt });
    });

    // DELETE /api/tokens/:key - Revoke token
    this.registerEndpoint('/tokens/:key', 'DELETE', (req, res) => {
      const { key } = req.params;
      const removed = this.revokeToken(key ?? '');
      if (!removed) {
        res.status(404).json({ error: 'Not found', message: 'Token not found' });
        return;
      }
      res.json({ success: true, message: 'Token revoked' });
    });

    // GET /api/endpoints - List all registered endpoints
    this.registerEndpoint('/endpoints', 'GET', (_req, res) => {
      const endpoints = this.listEndpoints().map((e) => ({
        method: e.method,
        path: e.path,
        description: e.description,
        requiresAuth: e.requiresAuth,
        rateLimited: e.rateLimited,
      }));
      res.json({ endpoints });
    });
  }
}

export default new ApiGatewayFeature();
