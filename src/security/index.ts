/**
 * AG-Claw Security Module
 * Comprehensive security hardening for production deployments
 */

import { createHash, randomBytes, timingSafeEqual } from 'crypto';

// Types
export interface SecurityConfig {
  enableRateLimiting: boolean;
  enableIPAllowlist: boolean;
  enableAuditLog: boolean;
  enableRequestTimeout: boolean;
  enableCSP: boolean;
  maxRequestSize: number;
  rateLimitWindow: number;
  rateLimitMax: number;
  trustedProxies: string[];
  allowedOrigins: string[];
}

export interface AuditEntry {
  timestamp: Date;
  level: 'INFO' | 'WARN' | 'ERROR' | 'CRITICAL';
  action: string;
  userId?: string;
  ip?: string;
  userAgent?: string;
  resource?: string;
  details?: Record<string, unknown>;
  success: boolean;
}

export interface RateLimitEntry {
  count: number;
  resetAt: number;
}

// Default configuration
const DEFAULT_CONFIG: SecurityConfig = {
  enableRateLimiting: true,
  enableIPAllowlist: false,
  enableAuditLog: true,
  enableRequestTimeout: true,
  enableCSP: true,
  maxRequestSize: 10 * 1024 * 1024, // 10MB
  rateLimitWindow: 60 * 1000, // 1 minute
  rateLimitMax: 100,
  trustedProxies: ['127.0.0.1', '::1'],
  allowedOrigins: ['http://localhost:3000'],
};

// Rate limit store (in-memory, use Redis for production)
const rateLimitStore = new Map<string, RateLimitEntry>();

// Audit log store
const auditLog: AuditEntry[] = [];
const MAX_AUDIT_LOG_SIZE = 10000;

/**
 * Security Manager Class
 */
export class SecurityManager {
  private config: SecurityConfig;
  private ipAllowlist: Set<string> = new Set();
  private blockedIPs: Set<string> = new Set();
  private requestTimeouts: Map<string, NodeJS.Timeout> = new Map();

  constructor(config: Partial<SecurityConfig> = {}) {
    this.config = { ...DEFAULT_CONFIG, ...config };
  }

  /**
   * Update configuration
   */
  updateConfig(config: Partial<SecurityConfig>): void {
    this.config = { ...this.config, ...config };
  }

  /**
   * Add IP to allowlist
   */
  addToAllowlist(ip: string): void {
    this.ipAllowlist.add(ip);
  }

  /**
   * Remove IP from allowlist
   */
  removeFromAllowlist(ip: string): void {
    this.ipAllowlist.delete(ip);
  }

  /**
   * Block IP temporarily
   */
  blockIP(ip: string, durationMs: number = 5 * 60 * 1000): void {
    this.blockedIPs.add(ip);
    setTimeout(() => {
      this.blockedIPs.delete(ip);
      this.audit('IP_UNBLOCKED', { ip, duration: durationMs }, true);
    }, durationMs);
  }

  /**
   * Check if IP is blocked
   */
  isIPBlocked(ip: string): boolean {
    return this.blockedIPs.has(ip);
  }

  /**
   * Rate limiting check
   */
  checkRateLimit(identifier: string): { allowed: boolean; remaining: number; resetAt: number } {
    if (!this.config.enableRateLimiting) {
      return { allowed: true, remaining: -1, resetAt: -1 };
    }

    const now = Date.now();
    const entry = rateLimitStore.get(identifier);

    if (!entry || now > entry.resetAt) {
      rateLimitStore.set(identifier, {
        count: 1,
        resetAt: now + this.config.rateLimitWindow,
      });
      return { allowed: true, remaining: this.config.rateLimitMax - 1, resetAt: now + this.config.rateLimitWindow };
    }

    if (entry.count >= this.config.rateLimitMax) {
      this.audit('RATE_LIMIT_EXCEEDED', { identifier, count: entry.count }, false);
      return { allowed: false, remaining: 0, resetAt: entry.resetAt };
    }

    entry.count++;
    return { allowed: true, remaining: this.config.rateLimitMax - entry.count, resetAt: entry.resetAt };
  }

  /**
   * Clean expired rate limit entries
   */
  cleanExpiredRateLimits(): void {
    const now = Date.now();
    for (const [key, entry] of rateLimitStore.entries()) {
      if (now > entry.resetAt) {
        rateLimitStore.delete(key);
      }
    }
  }

  /**
   * Validate request size
   */
  validateRequestSize(size: number): boolean {
    return size <= this.config.maxRequestSize;
  }

  /**
   * Generate Content Security Policy header
   */
  getCSPHeader(): string {
    if (!this.config.enableCSP) return '';

    const directives = [
      "default-src 'self'",
      "script-src 'self' 'unsafe-inline' 'unsafe-eval'",
      "style-src 'self' 'unsafe-inline'",
      "img-src 'self' data: https:",
      "font-src 'self'",
      "connect-src 'self' https: wss:",
      "frame-ancestors 'none'",
      "form-action 'self'",
      "base-uri 'self'",
      "object-src 'none'",
      "upgrade-insecure-requests",
    ];

    return directives.join('; ');
  }

  /**
   * Generate security headers
   */
  getSecurityHeaders(): Record<string, string> {
    return {
      'Content-Security-Policy': this.getCSPHeader(),
      'X-Content-Type-Options': 'nosniff',
      'X-Frame-Options': 'DENY',
      'X-XSS-Protection': '1; mode=block',
      'Strict-Transport-Security': 'max-age=31536000; includeSubDomains',
      'Referrer-Policy': 'strict-origin-when-cross-origin',
      'Permissions-Policy': 'geolocation=(), microphone=(), camera=()',
    };
  }

  /**
   * Audit log entry
   */
  audit(
    action: string,
    details: Record<string, unknown> = {},
    success: boolean = true,
    userId?: string,
    ip?: string,
    userAgent?: string,
    resource?: string
  ): void {
    if (!this.config.enableAuditLog) return;

    const entry: AuditEntry = {
      timestamp: new Date(),
      level: success ? 'INFO' : 'WARN',
      action,
      userId,
      ip,
      userAgent,
      resource,
      details,
      success,
    };

    auditLog.push(entry);

    // Keep log size bounded
    if (auditLog.length > MAX_AUDIT_LOG_SIZE) {
      auditLog.shift();
    }

    // Log critical errors to stderr
    if (action.includes('CRITICAL') || action.includes('SECURITY')) {
      console.error('[SECURITY AUDIT]', JSON.stringify(entry));
    }
  }

  /**
   * Get audit log entries with filtering
   */
  getAuditLog(options: {
    startDate?: Date;
    endDate?: Date;
    level?: AuditEntry['level'];
    action?: string;
    userId?: string;
    limit?: number;
  } = {}): AuditEntry[] {
    let entries = [...auditLog];

    if (options.startDate) {
      entries = entries.filter(e => e.timestamp >= options.startDate!);
    }
    if (options.endDate) {
      entries = entries.filter(e => e.timestamp <= options.endDate!);
    }
    if (options.level) {
      entries = entries.filter(e => e.level === options.level);
    }
    if (options.action) {
      entries = entries.filter(e => e.action.includes(options.action!));
    }
    if (options.userId) {
      entries = entries.filter(e => e.userId === options.userId);
    }

    if (options.limit) {
      entries = entries.slice(-options.limit);
    }

    return entries;
  }

  /**
   * Detect suspicious activity
   */
  detectSuspiciousActivity(ip: string): {
    suspicious: boolean;
    reasons: string[];
  } {
    const reasons: string[] = [];

    // Check rate limit
    const rateLimit = rateLimitStore.get(ip);
    if (rateLimit && rateLimit.count > this.config.rateLimitMax * 0.8) {
      reasons.push('High rate limit usage');
    }

    // Check for blocked IP
    if (this.blockedIPs.has(ip)) {
      reasons.push('IP is blocked');
    }

    // Check audit log for failed attempts
    const recentFailures = auditLog.filter(
      e => e.ip === ip && !e.success && 
      Date.now() - e.timestamp.getTime() < 5 * 60 * 1000
    );

    if (recentFailures.length > 5) {
      reasons.push('Multiple failed attempts');
    }

    return {
      suspicious: reasons.length > 0,
      reasons,
    };
  }

  /**
   * Hash sensitive data
   */
  hash(data: string, algorithm: 'sha256' | 'sha512' = 'sha256'): string {
    return createHash(algorithm).update(data).digest('hex');
  }

  /**
   * Generate secure random token
   */
  generateToken(length: number = 32): string {
    return randomBytes(length).toString('hex');
  }

  /**
   * Safe string comparison (timing attack prevention)
   */
  safeCompare(a: string, b: string): boolean {
    if (a.length !== b.length) {
      return false;
    }
    return timingSafeEqual(Buffer.from(a), Buffer.from(b));
  }

  /**
   * Validate URL (prevent SSRF)
   */
  validateURL(url: string, allowedProtocols: string[] = ['http:', 'https:']): boolean {
    try {
      const parsed = new URL(url);
      if (!allowedProtocols.includes(parsed.protocol)) {
        return false;
      }
      // Block localhost/private IPs in production
      const hostname = parsed.hostname;
      if (
        hostname === 'localhost' ||
        hostname === '127.0.0.1' ||
        hostname === '::1' ||
        hostname === '0.0.0.0' ||
        /^10\./.test(hostname) ||
        /^172\.(1[6-9]|2[0-9]|3[0-1])\./.test(hostname) ||
        /^192\.168\./.test(hostname) ||
        hostname.endsWith('.local')
      ) {
        // Allow in development, block in production
        if (process.env.NODE_ENV === 'production') {
          this.audit('SSRF_ATTEMPT', { url, hostname }, false);
          return false;
        }
      }
      return true;
    } catch {
      return false;
    }
  }

  /**
   * Sanitize filename (prevent path traversal)
   */
  sanitizeFilename(filename: string): string {
    // Remove path components, keep only basename
    const basename = filename.split('/').pop()?.split('\\').pop() || '';
    // Remove null bytes and control characters
    return basename.replace(/[\x00-\x1f\x7f]/g, '').substring(0, 255);
  }

  /**
   * Get security statistics
   */
  getStats(): {
    rateLimitEntries: number;
    auditLogSize: number;
    blockedIPs: number;
    allowedIPs: number;
  } {
    return {
      rateLimitEntries: rateLimitStore.size,
      auditLogSize: auditLog.length,
      blockedIPs: this.blockedIPs.size,
      allowedIPs: this.ipAllowlist.size,
    };
  }
}

// Singleton instance
export const security = new SecurityManager();

// Export default
export default security;
