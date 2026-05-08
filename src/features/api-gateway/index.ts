/**
 * API Gateway Feature
 *
 * REST API for external integrations with authentication, rate limiting,
 * and endpoint management.
 */

import { randomBytes, scryptSync, timingSafeEqual } from 'crypto';

import express, { type Request, type Response, type Handler } from 'express';
import expressRateLimit, { ipKeyGenerator } from 'express-rate-limit';

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
  requiredScope?: string;
}

/** API Token info */
export interface ApiToken {
  name: string;
  keyHash: string;
  keySalt: string;
  keyPreview: string;
  createdAt: number;
  lastUsed?: number;
  expiresAt?: number;
  scopes: string[];
  revokedAt?: number;
}

export interface CreatedApiToken extends ApiToken {
  key: string;
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
    version: '0.0.5',
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
  private readonly callerTokens = new WeakMap<object, ApiToken>();

  async init(config: Record<string, unknown>, context: FeatureContext): Promise<void> {
    this.ctx = context;
    this.config = { ...this.config, ...(config as Partial<ApiGatewayConfig>) };

    // Load existing apiKeys from config into tokens
    this.apiTokens.clear();
    for (const key of this.config.apiKeys) {
      const salt = this.generateSalt();
      const keyHash = this.hashApiKey(key, salt);
      this.apiTokens.set(keyHash, {
        name: `Initial key ${key.slice(0, 8)}`,
        keyHash,
        keySalt: salt,
        keyPreview: this.previewApiKey(key),
        createdAt: Date.now(),
        scopes: ['*'],
      });
    }
  }

  async start(): Promise<void> {
    this.app = express();
    this.app.use(express.json());

    // Apply global middleware
    this.app.use(this.requestLogger.bind(this));
    this.app.use(this.errorHandler.bind(this));

    // Apply a CodeQL-recognized IP limiter immediately before API key auth so
    // protected routes throttle unauthorized traffic before credential validation.
    // The project limiter still runs after auth for configured endpoint quotas.
    this.app.use(
      this.createPreAuthRateLimiter(),
      this.authMiddleware.bind(this),
      this.rateLimitMiddleware.bind(this),
    );

    // Add health check endpoint
    this.registerEndpoint('/health', 'GET', this.healthCheckHandler.bind(this), {
      description: 'Gateway health check',
      requiresAuth: false,
      rateLimited: false,
    });

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
    options?: {
      description?: string;
      requiresAuth?: boolean;
      rateLimited?: boolean;
      requiredScope?: string;
    },
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
      requiredScope: options?.requiredScope,
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
  createToken(name: string, expiresInDays?: number, scopes: string[] = ['*']): CreatedApiToken {
    const key = `ak_${randomBytes(24).toString('hex')}`;
    const salt = this.generateSalt();
    const keyHash = this.hashApiKey(key, salt);
    const token: CreatedApiToken = {
      name,
      key,
      keyHash,
      keySalt: salt,
      keyPreview: this.previewApiKey(key),
      createdAt: Date.now(),
      lastUsed: undefined,
      expiresAt:
        typeof expiresInDays === 'number'
          ? Date.now() + expiresInDays * 24 * 60 * 60 * 1000
          : undefined,
      scopes: this.normalizeScopes(scopes),
    };
    const stored: ApiToken = {
      name: token.name,
      keyHash: token.keyHash,
      keySalt: token.keySalt,
      keyPreview: token.keyPreview,
      createdAt: token.createdAt,
      lastUsed: token.lastUsed,
      expiresAt: token.expiresAt,
      scopes: token.scopes,
    };
    this.apiTokens.set(keyHash, stored);
    this.ctx.logger.info('API token created', { name, keyPreview: token.keyPreview });
    return token;
  }

  /** Revoke an API token */
  revokeToken(key: string): boolean {
    const entry = this.findTokenByKey(key);
    if (!entry) return false;
    const [keyHash, token] = entry;
    if (token.revokedAt) return false;
    token.revokedAt = Date.now();
    this.apiTokens.set(keyHash, token);
    return true;
  }

  /** List all tokens (without full keys) */
  listTokens(): ApiToken[] {
    return Array.from(this.apiTokens.values());
  }

  authenticateApiKey(apiKey: string): ApiToken | null {
    const entry = this.findTokenByKey(apiKey);
    if (!entry) return null;
    const [keyHash, token] = entry;
    if (token.revokedAt) return null;
    if (token.expiresAt && token.expiresAt <= Date.now()) return null;

    token.lastUsed = Date.now();
    this.apiTokens.set(keyHash, token);
    return token;
  }

  tokenHasScope(token: ApiToken | null | undefined, requiredScope?: string): boolean {
    if (!token) return false;
    if (!requiredScope) return true;
    return token.scopes.includes('*') || token.scopes.includes(requiredScope);
  }

  /** Normalize path to include config.path prefix */
  private normalizePath(path: string): string {
    const base = this.config.path.replace(/\/+$/, '');
    const cleanPath = path.replace(/^\/+/, '');
    return `${base}/${cleanPath}`;
  }

  /** Derive a hash for an API key using scrypt with the provided salt */
  private hashApiKey(key: string, salt: string): string {
    return scryptSync(key, salt, 64).toString('hex');
  }

  /** Generate a random salt for key hashing */
  private generateSalt(): string {
    return randomBytes(16).toString('hex');
  }

  /**
   * Find a stored token entry by matching the raw API key.
   * Uses scrypt to re-derive the hash with the stored salt, then constant-time
   * comparison to prevent timing attacks.
   *
   * Intentionally scans the full token set to reduce timing differences between
   * matching and non-matching keys.
   *
   * O(n) over stored tokens — acceptable for typical deployment sizes (< 1 000 tokens).
   */
  private findTokenByKey(apiKey: string): [string, ApiToken] | null {
    let match: [string, ApiToken] | null = null;

    for (const [hash, token] of this.apiTokens) {
      const computed = this.hashApiKey(apiKey, token.keySalt);
      const computedBuf = Buffer.from(computed, 'hex');
      const storedBuf = Buffer.from(hash, 'hex');
      if (computedBuf.length === storedBuf.length && timingSafeEqual(computedBuf, storedBuf)) {
        match = [hash, token];
      }
    }

    return match;
  }

  private previewApiKey(key: string): string {
    return key.length <= 12 ? `${key.slice(0, 4)}...` : `${key.slice(0, 8)}...${key.slice(-4)}`;
  }

  private normalizeScopes(scopes: string[]): string[] {
    const normalized = scopes
      .filter((scope): scope is string => typeof scope === 'string')
      .map((scope) => scope.trim())
      .filter(Boolean);
    return normalized.length > 0 ? [...new Set(normalized)] : ['*'];
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

  private getRateLimitConfig(): NonNullable<ApiGatewayConfig['rateLimit']> {
    return this.config.rateLimit ?? { windowMs: 60000, max: 100, byApiKey: true };
  }

  private getIpRateLimitKey(req: Request): string {
    return ipKeyGenerator(req.ip ?? req.socket.remoteAddress ?? '0.0.0.0');
  }

  private getRateLimitKey(req: Request, byApiKey: boolean): string {
    if (!byApiKey) {
      return this.getIpRateLimitKey(req);
    }

    const apiKeyHeader = req.headers['x-api-key'];
    const apiKey = Array.isArray(apiKeyHeader) ? apiKeyHeader[0] : apiKeyHeader;
    if (!apiKey) {
      return this.getIpRateLimitKey(req);
    }

    return this.findTokenByKey(apiKey)?.[0] ?? this.getIpRateLimitKey(req);
  }

  private createPreAuthRateLimiter(): Handler {
    const rl = this.getRateLimitConfig();

    return expressRateLimit({
      windowMs: rl.windowMs,
      limit: rl.max,
      standardHeaders: true,
      legacyHeaders: false,
      keyGenerator: (req) => this.getIpRateLimitKey(req),
      skip: (req) => {
        if (req.path.endsWith('/health')) return true;
        const endpoint = this.findEndpoint(req.method, req.path);
        return !endpoint?.rateLimited || !endpoint.requiresAuth;
      },
      handler: (_req, res) => {
        res.status(429).json({
          error: 'Too many requests',
          retryAfter: Math.ceil(rl.windowMs / 1000),
          limit: rl.max,
          window: rl.windowMs,
        });
      },
    });
  }

  private respondIfRateLimited(req: Request, res: Response): boolean {
    const rl = this.getRateLimitConfig();
    const key = this.getRateLimitKey(req, rl.byApiKey);

    try {
      const result = rateLimiting.check(key, rl.max, rl.windowMs);

      if (!result.allowed) {
        res.status(429).json({
          error: 'Too many requests',
          retryAfter: Math.ceil(rl.windowMs / 1000),
          limit: rl.max,
          window: rl.windowMs,
        });
        return true;
      }
    } catch (err) {
      this.ctx.logger.warn('Rate limiting check failed, allowing', {
        error: err instanceof Error ? err.message : String(err),
      });
    }

    return false;
  }

  /** Rate limiting middleware */
  private rateLimitMiddleware(
    req: Request,
    res: Response,
    next: express.NextFunction,
  ): void {
    // Skip rate limiting for health check
    if (req.path.endsWith('/health')) {
      next();
      return;
    }

    const endpoint = this.findEndpoint(req.method, req.path);
    if (!endpoint?.rateLimited) {
      next();
      return;
    }

    if (this.respondIfRateLimited(req, res)) {
      return;
    }

    next();
  }

  /** Authentication middleware */
  private authMiddleware(req: Request, res: Response, next: express.NextFunction): void {
    const endpoint = this.findEndpoint(req.method, req.path);

    if (!endpoint) {
      if (this.isApiRequest(req.path)) {
        res.status(404).json({ error: 'Not found', message: 'API endpoint not registered' });
        return;
      }
      return next();
    }

    if (!endpoint.requiresAuth) {
      return next();
    }

    // Check for API key (handle string[] case)
    const apiKeyHeader = req.headers['x-api-key'];
    const apiKey = Array.isArray(apiKeyHeader) ? apiKeyHeader[0] : apiKeyHeader;
    if (!apiKey) {
      res.status(401).json({ error: 'Unauthorized', message: 'API key required' });
      return;
    }

    const token = this.authenticateApiKey(apiKey);
    if (!token) {
      res.status(401).json({ error: 'Unauthorized', message: 'Invalid API key' });
      return;
    }

    if (!this.tokenHasScope(token, endpoint.requiredScope)) {
      res.status(403).json({ error: 'Forbidden', message: 'API key scope is not sufficient' });
      return;
    }

    // Attach caller token to request for downstream handlers (e.g., scope enforcement)
    this.callerTokens.set(req, token);

    next();
  }

  /** Find registered endpoint for a method/path */
  private findEndpoint(method: string, path: string): ApiEndpoint | undefined {
    const methodEndpoints = this.endpoints.get(method.toUpperCase() as ApiEndpoint['method']);
    if (!methodEndpoints) return undefined;

    return methodEndpoints.find((e) => this.matchesEndpointPath(e.path, path));
  }

  private matchesEndpointPath(pattern: string, actualPath: string): boolean {
    const normalizedPattern = this.normalizeComparablePath(pattern);
    const normalizedActual = this.normalizeComparablePath(actualPath);
    if (normalizedPattern === normalizedActual) return true;

    const patternSegments = normalizedPattern.split('/').filter(Boolean);
    const actualSegments = normalizedActual.split('/').filter(Boolean);
    if (patternSegments.length !== actualSegments.length) return false;

    return patternSegments.every((segment, index) => {
      if (segment.startsWith(':')) return (actualSegments[index]?.length ?? 0) > 0;
      return segment === actualSegments[index];
    });
  }

  private normalizeComparablePath(path: string): string {
    if (path === '/') return path;
    return path.replace(/\/+$/, '');
  }

  private isApiRequest(path: string): boolean {
    const base = this.config.path.replace(/\/+$/, '') || '/';
    return path === base || path.startsWith(`${base}/`);
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
        keyPreview: t.keyPreview,
        createdAt: t.createdAt,
        lastUsed: t.lastUsed,
        expiresAt: t.expiresAt,
        scopes: t.scopes,
        revokedAt: t.revokedAt,
      }));
      res.json({ tokens });
    }, {
      requiredScope: 'tokens:read',
    });

    // POST /api/tokens - Create new token
    this.registerEndpoint('/tokens', 'POST', (req, res) => {
      const { name, expiresInDays, scopes } = req.body;
      if (!name) {
        res.status(400).json({ error: 'Bad request', message: 'name is required' });
        return;
      }

      // Restrict created token scopes to a subset of the caller's own scopes
      // to prevent privilege escalation.
      const callerToken = this.callerTokens.get(req);
      if (!callerToken) {
        // authMiddleware must have populated this; if it didn't, refuse to proceed.
        res.status(403).json({ error: 'Forbidden', message: 'Caller identity could not be determined' });
        return;
      }
      const callerScopes = callerToken.scopes;
      const requestedScopes: string[] = Array.isArray(scopes) ? scopes : ['*'];
      let grantedScopes: string[];
      if (callerScopes.includes('*')) {
        // Caller has wildcard — may grant any scope including '*'
        grantedScopes = requestedScopes;
      } else {
        // Caller may only grant scopes they already hold (no '*' escalation)
        grantedScopes = requestedScopes.filter((s) => s !== '*' && callerScopes.includes(s));
        if (grantedScopes.length === 0) {
          res.status(403).json({
            error: 'Forbidden',
            message: 'Requested scopes exceed caller privileges',
          });
          return;
        }
      }

      const token = this.createToken(
        name,
        typeof expiresInDays === 'number' ? expiresInDays : undefined,
        grantedScopes,
      );
      res.status(201).json({
        token: token.key,
        keyPreview: token.keyPreview,
        name: token.name,
        createdAt: token.createdAt,
        expiresAt: token.expiresAt,
        scopes: token.scopes,
      });
    }, {
      requiredScope: 'tokens:create',
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
    }, {
      requiredScope: 'tokens:revoke',
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
    }, {
      requiredScope: 'endpoints:read',
    });
  }
}

export default new ApiGatewayFeature();
