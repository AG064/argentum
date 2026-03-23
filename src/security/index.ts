/**
 * AG-Claw Security Module
 *
 * Comprehensive security hardening for AG-Claw.
 * Provides authentication, authorization, rate limiting, and audit logging.
 */

import { createHmac, randomBytes } from 'crypto';
import { EventEmitter } from 'events';

// Types
export interface SecurityConfig {
  authentication: {
    enabled: boolean;
    jwtSecret: string;
    tokenExpiry: number; // seconds
    refreshTokenExpiry: number;
  };
  rateLimiting: {
    enabled: boolean;
    windowMs: number;
    maxRequests: number;
    maxRequestsPerAgent?: number;
  };
  allowedPaths: {
    enabled: boolean;
    paths: string[];
  };
  allowedHosts: {
    enabled: boolean;
    hosts: string[];
  };
  auditLog: {
    enabled: boolean;
    retentionDays: number;
    events: AuditEvent[];
  };
  cors: {
    enabled: boolean;
    origins: string[];
  };
}

export type AuditEvent =
  | 'auth.login'
  | 'auth.logout'
  | 'auth.failed'
  | 'agent.start'
  | 'agent.stop'
  | 'agent.error'
  | 'file.read'
  | 'file.write'
  | 'file.delete'
  | 'exec.run'
  | 'network.request'
  | 'config.change'
  | 'security.violation';

export interface AuditLogEntry {
  timestamp: string;
  event: AuditEvent;
  agentId?: string;
  userId?: string;
  ip?: string;
  details?: Record<string, unknown>;
  success: boolean;
}

// Rate limit storage
interface RateLimitEntry {
  count: number;
  resetAt: number;
}

/**
 * Security Manager
 */
export class SecurityManager extends EventEmitter {
  private config: SecurityConfig;
  private rateLimitStore: Map<string, RateLimitEntry> = new Map();
  private auditLog: AuditLogEntry[] = [];
  private readonly jwtSecret: Buffer;

  constructor(config: SecurityConfig) {
    super();
    this.config = config;
    this.jwtSecret = createHmac('sha256', config.authentication.jwtSecret)
      .update('ag-claw-key')
      .digest();

    // Start rate limit cleanup interval
    if (config.rateLimiting.enabled) {
      setInterval(() => this.cleanupRateLimits(), 60000);
    }
  }

  /**
   * Authenticate a request
   */
  public async authenticate(
    token: string,
  ): Promise<{ valid: boolean; userId?: string; agentId?: string }> {
    if (!this.config.authentication.enabled) {
      return { valid: true };
    }

    try {
      // JWT verification would go here
      // For now, simple token check
      const parts = token.split('.');
      if (parts.length !== 3) {
        this.log('auth.failed', { reason: 'invalid_token_format' });
        return { valid: false };
      }

      // Verify signature
      const [header, payload, signature] = parts as [string, string, string];
      const expectedSig = createHmac('sha256', this.jwtSecret)
        .update(`${header}.${payload}`)
        .digest('base64url');

      if (signature !== expectedSig) {
        this.log('auth.failed', { reason: 'invalid_signature' });
        return { valid: false };
      }

      const data = JSON.parse(Buffer.from(payload, 'base64url').toString());

      // Check expiry
      if (data.exp && data.exp < Date.now() / 1000) {
        this.log('auth.failed', { reason: 'token_expired' });
        return { valid: false };
      }

      this.log('auth.login', { userId: data.sub, agentId: data.agentId });
      return { valid: true, userId: data.sub, agentId: data.agentId };
    } catch (error) {
      this.log('auth.failed', { reason: 'verification_error' });
      return { valid: false };
    }
  }

  /**
   * Generate a JWT token
   */
  public generateToken(userId: string, agentId?: string): string {
    const header = Buffer.from(JSON.stringify({ alg: 'HS256', typ: 'JWT' })).toString('base64url');

    const payload = Buffer.from(
      JSON.stringify({
        sub: userId,
        agentId,
        iat: Math.floor(Date.now() / 1000),
        exp: Math.floor(Date.now() / 1000) + this.config.authentication.tokenExpiry,
      }),
    ).toString('base64url');

    const signature = createHmac('sha256', this.jwtSecret)
      .update(`${header}.${payload}`)
      .digest('base64url');

    return `${header}.${payload}.${signature}`;
  }

  /**
   * Check rate limit
   */
  public async checkRateLimit(
    key: string,
    isAgent = false,
  ): Promise<{ allowed: boolean; remaining: number; resetAt: number }> {
    if (!this.config.rateLimiting.enabled) {
      return { allowed: true, remaining: -1, resetAt: 0 };
    }

    const maxRequests = isAgent
      ? this.config.rateLimiting.maxRequestsPerAgent || this.config.rateLimiting.maxRequests
      : this.config.rateLimiting.maxRequests;

    const now = Date.now();
    const entry = this.rateLimitStore.get(key);

    if (!entry || entry.resetAt < now) {
      const resetAt = now + this.config.rateLimiting.windowMs;
      this.rateLimitStore.set(key, { count: 1, resetAt });
      return { allowed: true, remaining: maxRequests - 1, resetAt };
    }

    if (entry.count >= maxRequests) {
      this.log('security.violation', { type: 'rate_limit', key });
      return { allowed: false, remaining: 0, resetAt: entry.resetAt };
    }

    entry.count++;
    return { allowed: true, remaining: maxRequests - entry.count, resetAt: entry.resetAt };
  }

  /**
   * Validate file path
   */
  public validatePath(filePath: string): boolean {
    if (!this.config.allowedPaths.enabled) {
      return true;
    }

    const normalized = filePath.replace(/\\/g, '/');

    return this.config.allowedPaths.paths.some((allowed) => {
      return normalized.startsWith(allowed);
    });
  }

  /**
   * Validate URL/Host
   */
  public validateHost(url: string): boolean {
    if (!this.config.allowedHosts.enabled) {
      return true;
    }

    try {
      const parsed = new URL(url);
      return this.config.allowedHosts.hosts.includes(parsed.hostname);
    } catch {
      return false;
    }
  }

  /**
   * Log an audit event
   */
  public log(
    event: AuditEvent,
    details: Record<string, unknown> = {},
    context: { agentId?: string; userId?: string; ip?: string } = {},
  ): void {
    if (!this.config.auditLog.enabled) return;
    const allowedEvents = this.config.auditLog.events as string[];
    if (allowedEvents.length > 0 && !allowedEvents.includes(event) && !allowedEvents.includes('*'))
      return;

    const entry: AuditLogEntry = {
      timestamp: new Date().toISOString(),
      event,
      ...context,
      details,
      success: (details['success'] as boolean) !== false,
    };

    this.auditLog.push(entry);

    // Emit event for real-time monitoring
    this.emit('audit', entry);

    // Cleanup old entries
    if (this.auditLog.length > 10000) {
      this.auditLog = this.auditLog.slice(-5000);
    }
  }

  /**
   * Get audit log
   */
  public getAuditLog(filter?: Partial<AuditLogEntry>): AuditLogEntry[] {
    let entries = [...this.auditLog];

    if (filter) {
      if (filter.event) entries = entries.filter((e) => e.event === filter.event);
      if (filter.agentId) entries = entries.filter((e) => e.agentId === filter.agentId);
      if (filter.userId) entries = entries.filter((e) => e.userId === filter.userId);
    }

    return entries.slice(-100); // Last 100 entries
  }

  /**
   * CORS middleware
   */
  public getCorsHeaders(origin: string): Record<string, string> | null {
    if (!this.config.cors.enabled) return null;

    if (this.config.cors.origins.includes('*') || this.config.cors.origins.includes(origin)) {
      return {
        'Access-Control-Allow-Origin': origin,
        'Access-Control-Allow-Methods': 'GET, POST, PUT, DELETE, OPTIONS',
        'Access-Control-Allow-Headers': 'Content-Type, Authorization',
        'Access-Control-Max-Age': '86400',
      };
    }

    return null;
  }

  /**
   * Generate secure random string
   */
  public generateSecureId(length = 32): string {
    return randomBytes(length).toString('base64url');
  }

  /**
   * Cleanup expired rate limits
   */
  private cleanupRateLimits(): void {
    const now = Date.now();
    for (const [key, entry] of this.rateLimitStore.entries()) {
      if (entry.resetAt < now) {
        this.rateLimitStore.delete(key);
      }
    }
  }

  /**
   * Get security stats
   */
  public getStats(): {
    rateLimitKeys: number;
    auditLogSize: number;
    uptime: number;
  } {
    return {
      rateLimitKeys: this.rateLimitStore.size,
      auditLogSize: this.auditLog.length,
      uptime: process.uptime(),
    };
  }
}

/**
 * Create security manager with default config
 */
export function createSecurityManager(config?: Partial<SecurityConfig>): SecurityManager {
  const defaultConfig: SecurityConfig = {
    authentication: {
      enabled: false,
      jwtSecret: process.env.JWT_SECRET || 'change-me-in-production',
      tokenExpiry: 3600,
      refreshTokenExpiry: 86400 * 7,
    },
    rateLimiting: {
      enabled: true,
      windowMs: 60000,
      maxRequests: 100,
      maxRequestsPerAgent: 200,
    },
    allowedPaths: {
      enabled: true,
      paths: [],
    },
    allowedHosts: {
      enabled: false,
      hosts: [],
    },
    auditLog: {
      enabled: true,
      retentionDays: 30,
      events: ['*'] as unknown as AuditEvent[],
    },
    cors: {
      enabled: true,
      origins: ['*'],
    },
  };

  return new SecurityManager({
    ...defaultConfig,
    ...config,
    authentication: { ...defaultConfig.authentication, ...config?.authentication },
    rateLimiting: { ...defaultConfig.rateLimiting, ...config?.rateLimiting },
    allowedPaths: { ...defaultConfig.allowedPaths, ...config?.allowedPaths },
    allowedHosts: { ...defaultConfig.allowedHosts, ...config?.allowedHosts },
    auditLog: { ...defaultConfig.auditLog, ...config?.auditLog },
    cors: { ...defaultConfig.cors, ...config?.cors },
  });
}

export default SecurityManager;
