"use strict";
/**
 * API Gateway Feature
 *
 * REST API for external integrations with authentication, rate limiting,
 * and endpoint management.
 */
var __createBinding = (this && this.__createBinding) || (Object.create ? (function(o, m, k, k2) {
    if (k2 === undefined) k2 = k;
    var desc = Object.getOwnPropertyDescriptor(m, k);
    if (!desc || ("get" in desc ? !m.__esModule : desc.writable || desc.configurable)) {
      desc = { enumerable: true, get: function() { return m[k]; } };
    }
    Object.defineProperty(o, k2, desc);
}) : (function(o, m, k, k2) {
    if (k2 === undefined) k2 = k;
    o[k2] = m[k];
}));
var __setModuleDefault = (this && this.__setModuleDefault) || (Object.create ? (function(o, v) {
    Object.defineProperty(o, "default", { enumerable: true, value: v });
}) : function(o, v) {
    o["default"] = v;
});
var __importStar = (this && this.__importStar) || (function () {
    var ownKeys = function(o) {
        ownKeys = Object.getOwnPropertyNames || function (o) {
            var ar = [];
            for (var k in o) if (Object.prototype.hasOwnProperty.call(o, k)) ar[ar.length] = k;
            return ar;
        };
        return ownKeys(o);
    };
    return function (mod) {
        if (mod && mod.__esModule) return mod;
        var result = {};
        if (mod != null) for (var k = ownKeys(mod), i = 0; i < k.length; i++) if (k[i] !== "default") __createBinding(result, mod, k[i]);
        __setModuleDefault(result, mod);
        return result;
    };
})();
var __importDefault = (this && this.__importDefault) || function (mod) {
    return (mod && mod.__esModule) ? mod : { "default": mod };
};
Object.defineProperty(exports, "__esModule", { value: true });
const crypto_1 = require("crypto");
const express_1 = __importDefault(require("express"));
const express_rate_limit_1 = __importStar(require("express-rate-limit"));
const rate_limiting_1 = __importDefault(require("../rate-limiting"));
/**
 * API Gateway feature — external REST API with authentication and rate limiting.
 *
 * Provides an Express-based HTTP server for external integrations,
 * with configurable endpoints, API key authentication, and rate limiting.
 */
class ApiGatewayFeature {
    meta = {
        name: 'api-gateway',
        version: '0.0.3',
        description: 'REST API for external integrations with auth and rate limiting',
        dependencies: ['rate-limiting'],
    };
    config = {
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
    ctx;
    app;
    server = null;
    endpoints = new Map(); // keyed by method
    apiTokens = new Map();
    active = false;
    callerTokens = new WeakMap();
    async init(config, context) {
        this.ctx = context;
        this.config = { ...this.config, ...config };
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
    async start() {
        this.app = (0, express_1.default)();
        this.app.use(express_1.default.json());
        // Apply global middleware
        this.app.use(this.requestLogger.bind(this));
        this.app.use(this.errorHandler.bind(this));
        // Apply a CodeQL-recognized IP limiter immediately before API key auth so
        // protected routes throttle unauthorized traffic before credential validation.
        // The project limiter still runs after auth for configured endpoint quotas.
        this.app.use(this.createPreAuthRateLimiter(), this.authMiddleware.bind(this), this.rateLimitMiddleware.bind(this));
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
    async stop() {
        if (this.server) {
            await new Promise((resolve) => this.server.close(resolve));
            this.server = null;
        }
        this.active = false;
        this.endpoints.clear();
        this.ctx.logger.info('API Gateway stopped');
    }
    async healthCheck() {
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
    registerEndpoint(path, method, handler, options) {
        if (!this.app) {
            throw new Error('API Gateway not started. Call start() first.');
        }
        const normalizedPath = this.normalizePath(path);
        const endpoint = {
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
    removeEndpoint(path, method) {
        const normalizedPath = this.normalizePath(path);
        let removed = false;
        if (method) {
            const methodEndpoints = this.endpoints.get(method);
            if (methodEndpoints) {
                const filtered = methodEndpoints.filter((e) => e.path !== normalizedPath);
                removed = methodEndpoints.length > filtered.length;
                this.endpoints.set(method, filtered);
            }
        }
        else {
            // Remove for all methods
            for (const [m, endpoints] of this.endpoints) {
                const before = endpoints.length;
                const filtered = endpoints.filter((e) => e.path !== normalizedPath);
                this.endpoints.set(m, filtered);
                if (filtered.length < before)
                    removed = true;
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
    listEndpoints() {
        const all = [];
        for (const [, endpoints] of this.endpoints) {
            all.push(...endpoints);
        }
        return all;
    }
    /** Create an API token */
    createToken(name, expiresInDays, scopes = ['*']) {
        const key = `ak_${(0, crypto_1.randomBytes)(24).toString('hex')}`;
        const salt = this.generateSalt();
        const keyHash = this.hashApiKey(key, salt);
        const token = {
            name,
            key,
            keyHash,
            keySalt: salt,
            keyPreview: this.previewApiKey(key),
            createdAt: Date.now(),
            lastUsed: undefined,
            expiresAt: typeof expiresInDays === 'number'
                ? Date.now() + expiresInDays * 24 * 60 * 60 * 1000
                : undefined,
            scopes: this.normalizeScopes(scopes),
        };
        const stored = {
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
    revokeToken(key) {
        const entry = this.findTokenByKey(key);
        if (!entry)
            return false;
        const [keyHash, token] = entry;
        if (token.revokedAt)
            return false;
        token.revokedAt = Date.now();
        this.apiTokens.set(keyHash, token);
        return true;
    }
    /** List all tokens (without full keys) */
    listTokens() {
        return Array.from(this.apiTokens.values());
    }
    authenticateApiKey(apiKey) {
        const entry = this.findTokenByKey(apiKey);
        if (!entry)
            return null;
        const [keyHash, token] = entry;
        if (token.revokedAt)
            return null;
        if (token.expiresAt && token.expiresAt <= Date.now())
            return null;
        token.lastUsed = Date.now();
        this.apiTokens.set(keyHash, token);
        return token;
    }
    tokenHasScope(token, requiredScope) {
        if (!token)
            return false;
        if (!requiredScope)
            return true;
        return token.scopes.includes('*') || token.scopes.includes(requiredScope);
    }
    /** Normalize path to include config.path prefix */
    normalizePath(path) {
        const base = this.config.path.replace(/\/+$/, '');
        const cleanPath = path.replace(/^\/+/, '');
        return `${base}/${cleanPath}`;
    }
    /** Derive a hash for an API key using scrypt with the provided salt */
    hashApiKey(key, salt) {
        return (0, crypto_1.scryptSync)(key, salt, 64).toString('hex');
    }
    /** Generate a random salt for key hashing */
    generateSalt() {
        return (0, crypto_1.randomBytes)(16).toString('hex');
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
    findTokenByKey(apiKey) {
        let match = null;
        for (const [hash, token] of this.apiTokens) {
            const computed = this.hashApiKey(apiKey, token.keySalt);
            const computedBuf = Buffer.from(computed, 'hex');
            const storedBuf = Buffer.from(hash, 'hex');
            if (computedBuf.length === storedBuf.length && (0, crypto_1.timingSafeEqual)(computedBuf, storedBuf)) {
                match = [hash, token];
            }
        }
        return match;
    }
    previewApiKey(key) {
        return key.length <= 12 ? `${key.slice(0, 4)}...` : `${key.slice(0, 8)}...${key.slice(-4)}`;
    }
    normalizeScopes(scopes) {
        const normalized = scopes
            .filter((scope) => typeof scope === 'string')
            .map((scope) => scope.trim())
            .filter(Boolean);
        return normalized.length > 0 ? [...new Set(normalized)] : ['*'];
    }
    /** Request logger middleware */
    requestLogger(req, res, next) {
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
    errorHandler(err, req, res, next) {
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
    getRateLimitConfig() {
        return this.config.rateLimit ?? { windowMs: 60000, max: 100, byApiKey: true };
    }
    getIpRateLimitKey(req) {
        return (0, express_rate_limit_1.ipKeyGenerator)(req.ip ?? req.socket.remoteAddress ?? '0.0.0.0');
    }
    getRateLimitKey(req, byApiKey) {
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
    createPreAuthRateLimiter() {
        const rl = this.getRateLimitConfig();
        return (0, express_rate_limit_1.default)({
            windowMs: rl.windowMs,
            limit: rl.max,
            standardHeaders: true,
            legacyHeaders: false,
            keyGenerator: (req) => this.getIpRateLimitKey(req),
            skip: (req) => {
                if (req.path.endsWith('/health'))
                    return true;
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
    respondIfRateLimited(req, res) {
        const rl = this.getRateLimitConfig();
        const key = this.getRateLimitKey(req, rl.byApiKey);
        try {
            const result = rate_limiting_1.default.check(key, rl.max, rl.windowMs);
            if (!result.allowed) {
                res.status(429).json({
                    error: 'Too many requests',
                    retryAfter: Math.ceil(rl.windowMs / 1000),
                    limit: rl.max,
                    window: rl.windowMs,
                });
                return true;
            }
        }
        catch (err) {
            this.ctx.logger.warn('Rate limiting check failed, allowing', {
                error: err instanceof Error ? err.message : String(err),
            });
        }
        return false;
    }
    /** Rate limiting middleware */
    async rateLimitMiddleware(req, res, next) {
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
    authMiddleware(req, res, next) {
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
    findEndpoint(method, path) {
        const methodEndpoints = this.endpoints.get(method.toUpperCase());
        if (!methodEndpoints)
            return undefined;
        return methodEndpoints.find((e) => this.matchesEndpointPath(e.path, path));
    }
    matchesEndpointPath(pattern, actualPath) {
        const normalizedPattern = this.normalizeComparablePath(pattern);
        const normalizedActual = this.normalizeComparablePath(actualPath);
        if (normalizedPattern === normalizedActual)
            return true;
        const patternSegments = normalizedPattern.split('/').filter(Boolean);
        const actualSegments = normalizedActual.split('/').filter(Boolean);
        if (patternSegments.length !== actualSegments.length)
            return false;
        return patternSegments.every((segment, index) => {
            if (segment.startsWith(':'))
                return (actualSegments[index]?.length ?? 0) > 0;
            return segment === actualSegments[index];
        });
    }
    normalizeComparablePath(path) {
        if (path === '/')
            return path;
        return path.replace(/\/+$/, '');
    }
    isApiRequest(path) {
        const base = this.config.path.replace(/\/+$/, '') || '/';
        return path === base || path.startsWith(`${base}/`);
    }
    /** Health check handler */
    healthCheckHandler(_req, res) {
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
    registerDefaultEndpoints() {
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
            const requestedScopes = Array.isArray(scopes) ? scopes : ['*'];
            let grantedScopes;
            if (callerScopes.includes('*')) {
                // Caller has wildcard — may grant any scope including '*'
                grantedScopes = requestedScopes;
            }
            else {
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
            const token = this.createToken(name, typeof expiresInDays === 'number' ? expiresInDays : undefined, grantedScopes);
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
exports.default = new ApiGatewayFeature();
//# sourceMappingURL=index.js.map