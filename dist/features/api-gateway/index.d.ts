/**
 * API Gateway Feature
 *
 * REST API for external integrations with authentication, rate limiting,
 * and endpoint management.
 */
import { type Handler } from 'express';
import { type FeatureModule, type FeatureContext, type FeatureMeta, type HealthStatus } from '../../core/plugin-loader';
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
declare class ApiGatewayFeature implements FeatureModule {
    readonly meta: FeatureMeta;
    private config;
    private ctx;
    private app;
    private server;
    private endpoints;
    private apiTokens;
    private active;
    init(config: Record<string, unknown>, context: FeatureContext): Promise<void>;
    start(): Promise<void>;
    stop(): Promise<void>;
    healthCheck(): Promise<HealthStatus>;
    /** Register a new endpoint */
    registerEndpoint(path: string, method: ApiEndpoint['method'], handler: Handler, options?: {
        description?: string;
        requiresAuth?: boolean;
        rateLimited?: boolean;
    }): void;
    /** Remove an endpoint */
    removeEndpoint(path: string, method?: ApiEndpoint['method']): boolean;
    /** List all registered endpoints */
    listEndpoints(): ApiEndpoint[];
    /** Create an API token */
    createToken(name: string, _expiresInDays?: number): ApiToken;
    /** Revoke an API token */
    revokeToken(key: string): boolean;
    /** List all tokens (without full keys) */
    listTokens(): ApiToken[];
    /** Normalize path to include config.path prefix */
    private normalizePath;
    /** Request logger middleware */
    private requestLogger;
    /** Error handler middleware */
    private errorHandler;
    /** Rate limiting middleware */
    private rateLimitMiddleware;
    /** Authentication middleware */
    private authMiddleware;
    /** Find registered endpoint for a method/path */
    private findEndpoint;
    /** Health check handler */
    private healthCheckHandler;
    /** Register some default endpoints */
    private registerDefaultEndpoints;
}
declare const _default: ApiGatewayFeature;
export default _default;
//# sourceMappingURL=index.d.ts.map