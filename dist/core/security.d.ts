/**
 * Argentum Security Middleware
 *
 * Production security middleware providing:
 * - Helmet-style security headers
 * - Request validation with Zod
 * - Rate limiting utilities
 * - Input sanitization
 * - SSRF protection
 * - Audit logging for sensitive operations
 */
import { z } from 'zod';
/** CSPdirectives that block inline scripts and restrict sources */
export declare const SECURITY_HEADERS: {
    readonly 'Content-Security-Policy': string;
    readonly 'X-Content-Type-Options': "nosniff";
    readonly 'X-Frame-Options': "DENY";
    readonly 'X-XSS-Protection': "1; mode=block";
    readonly 'Referrer-Policy': "strict-origin-when-cross-origin";
    readonly 'Permissions-Policy': "camera=(), microphone=(), geolocation=(), payment=()";
    readonly 'Strict-Transport-Security': "max-age=31536000; includeSubDomains";
    readonly 'Cross-Origin-Opener-Policy': "same-origin";
    readonly 'Cross-Origin-Resource-Policy': "same-origin";
};
/**
 * Generate a nonce for CSP
 */
export declare function generateNonce(): string;
/**
 * Apply security headers to an HTTP response
 */
export declare function applySecurityHeaders(headers: Record<string, string | number | string[]>, nonce?: string): Record<string, string>;
/** Validate a room ID (alphanumeric, dashes, underscores, 1-64 chars) */
export declare const RoomIdSchema: z.ZodString;
/** Validate a user ID (alphanumeric, dashes, underscores, 1-64 chars) */
export declare const UserIdSchema: z.ZodString;
/** Validate chat message content */
export declare const ChatMessageSchema: z.ZodObject<{
    type: z.ZodLiteral<"chat">;
    content: z.ZodPipe<z.ZodString, z.ZodTransform<string, string>>;
}, z.core.$strip>;
/** Validate file upload payload */
export declare const FileUploadSchema: z.ZodObject<{
    type: z.ZodLiteral<"file">;
    filename: z.ZodPipe<z.ZodString, z.ZodTransform<string, string>>;
    mimeType: z.ZodString;
    data: z.ZodString;
}, z.core.$strip>;
/** Validate WebSocket message */
export declare const WebSocketMessageSchema: z.ZodDiscriminatedUnion<[z.ZodObject<{
    type: z.ZodLiteral<"chat">;
    content: z.ZodPipe<z.ZodString, z.ZodTransform<string, string>>;
}, z.core.$strip>, z.ZodObject<{
    type: z.ZodLiteral<"file">;
    filename: z.ZodPipe<z.ZodString, z.ZodTransform<string, string>>;
    mimeType: z.ZodString;
    data: z.ZodString;
}, z.core.$strip>, z.ZodObject<{
    type: z.ZodLiteral<"typing">;
}, z.core.$strip>], "type">;
/** Validate URL for SSRF prevention */
export declare const ExternalUrlSchema: z.ZodString;
/**
 * Check if a hostname is reserved/internal
 */
export declare function isReservedHostname(hostname: string): boolean;
/**
 * Validate a URL for outbound webhook delivery
 */
export declare function validateOutboundUrl(url: string): {
    valid: boolean;
    reason?: string;
};
/**
 * Sanitize HTML to prevent XSS while allowing safe formatting.
 * Uses a whitelist approach for tags and attributes.
 */
export declare function sanitizeHTML(html: string): string;
/**
 * Sanitize a filename to prevent path traversal
 */
export declare function sanitizeFilename(filename: string): string;
/**
 * Escape HTML entities in plain text
 */
export declare function escapeHTML(str: string): string;
/**
 * Check if a command string contains dangerous shell characters
 */
export declare function containsShellInjection(command: string): boolean;
/**
 * Validate a command against allowed patterns for sandbox execution.
 * Returns { valid: true } or { valid: false, reason: string }
 */
export declare function validateSandboxCommand(command: string, allowedCommands: Set<string>): {
    valid: boolean;
    reason?: string;
};
/**
 * Parse command string into safe argument array (quote-aware)
 */
export declare function parseCommandArgs(command: string): string[];
export type AuditAction = 'auth_failure' | 'auth_success' | 'rate_limited' | 'file_upload' | 'command_execution' | 'config_change' | 'sensitive_access' | 'exec_error';
export interface AuditEntry {
    timestamp: number;
    action: AuditAction;
    actor?: string;
    ip?: string;
    details: Record<string, unknown>;
}
/**
 * Structured audit logger for security events.
 * Logs to a dedicated audit channel (can be connected to SIEM).
 */
export declare class SecurityAuditor {
    private entries;
    private readonly maxEntries;
    /**
     * Log a security event
     */
    log(action: AuditAction, details: Record<string, unknown>, actor?: string, ip?: string): void;
    /**
     * Remove sensitive values from details before logging
     */
    private sanitizeDetails;
    /**
     * Get recent audit entries
     */
    getEntries(limit?: number): AuditEntry[];
    /**
     * Query audit entries by action type
     */
    queryByAction(action: AuditAction, since?: number): AuditEntry[];
}
/**
 * Wrap a promise with a timeout
 */
export declare function withTimeout<T>(promise: Promise<T>, timeoutMs: number, timeoutError?: string): Promise<T>;
export interface RateLimitResult {
    allowed: boolean;
    remaining: number;
    resetAt: number;
}
/**
 * Simple in-memory rate limiter for single-instance use.
 * For multi-instance deployments, use the rate-limiting feature with SQLite.
 */
export declare class SimpleRateLimiter {
    private entries;
    private readonly windowMs;
    private readonly max;
    constructor(windowMs: number, max: number);
    check(key: string): RateLimitResult;
    reset(key: string): void;
    cleanExpired(): void;
}
export declare function getAuditor(): SecurityAuditor;
//# sourceMappingURL=security.d.ts.map