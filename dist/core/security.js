"use strict";
/**
 * AG-Claw Security Middleware
 *
 * Production security middleware providing:
 * - Helmet-style security headers
 * - Request validation with Zod
 * - Rate limiting utilities
 * - Input sanitization
 * - SSRF protection
 * - Audit logging for sensitive operations
 */
var __importDefault = (this && this.__importDefault) || function (mod) {
    return (mod && mod.__esModule) ? mod : { "default": mod };
};
Object.defineProperty(exports, "__esModule", { value: true });
exports.SimpleRateLimiter = exports.SecurityAuditor = exports.ExternalUrlSchema = exports.WebSocketMessageSchema = exports.FileUploadSchema = exports.ChatMessageSchema = exports.UserIdSchema = exports.RoomIdSchema = exports.SECURITY_HEADERS = void 0;
exports.generateNonce = generateNonce;
exports.applySecurityHeaders = applySecurityHeaders;
exports.isReservedHostname = isReservedHostname;
exports.validateOutboundUrl = validateOutboundUrl;
exports.sanitizeHTML = sanitizeHTML;
exports.sanitizeFilename = sanitizeFilename;
exports.escapeHTML = escapeHTML;
exports.containsShellInjection = containsShellInjection;
exports.validateSandboxCommand = validateSandboxCommand;
exports.parseCommandArgs = parseCommandArgs;
exports.withTimeout = withTimeout;
exports.getAuditor = getAuditor;
const crypto_1 = require("crypto");
const sanitize_html_1 = __importDefault(require("sanitize-html"));
const zod_1 = require("zod");
// ─── Security Headers ─────────────────────────────────────────────────────────
/** CSPdirectives that block inline scripts and restrict sources */
exports.SECURITY_HEADERS = {
    'Content-Security-Policy': [
        "default-src 'self'",
        "script-src 'self' 'nonce-{nonce}'", // nonce set per-request
        "style-src 'self' 'unsafe-inline'", // needed for chat UI
        "img-src 'self' data: blob:",
        "connect-src 'self' ws: wss:",
        "font-src 'self'",
        "object-src 'none'",
        "base-uri 'self'",
        "form-action 'self'",
        "frame-ancestors 'none'",
        'upgrade-insecure-requests',
    ].join('; '),
    'X-Content-Type-Options': 'nosniff',
    'X-Frame-Options': 'DENY',
    'X-XSS-Protection': '1; mode=block',
    'Referrer-Policy': 'strict-origin-when-cross-origin',
    'Permissions-Policy': 'camera=(), microphone=(), geolocation=(), payment=()',
    'Strict-Transport-Security': 'max-age=31536000; includeSubDomains',
    'Cross-Origin-Opener-Policy': 'same-origin',
    'Cross-Origin-Resource-Policy': 'same-origin',
};
/**
 * Generate a nonce for CSP
 */
function generateNonce() {
    return (0, crypto_1.randomBytes)(16).toString('base64');
}
/**
 * Apply security headers to an HTTP response
 */
function applySecurityHeaders(headers, nonce) {
    const result = {};
    for (const [key, value] of Object.entries(headers)) {
        if (key === 'Content-Security-Policy' && nonce) {
            result[key] = value.toString().replace('{nonce}', nonce);
        }
        else {
            result[key] = String(value);
        }
    }
    return result;
}
// ─── Input Validation Schemas ─────────────────────────────────────────────────
/** Validate a room ID (alphanumeric, dashes, underscores, 1-64 chars) */
exports.RoomIdSchema = zod_1.z
    .string()
    .min(1)
    .max(64)
    .regex(/^[a-zA-Z0-9_-]+$/, 'Invalid room ID format');
/** Validate a user ID (alphanumeric, dashes, underscores, 1-64 chars) */
exports.UserIdSchema = zod_1.z
    .string()
    .min(1)
    .max(64)
    .regex(/^[a-zA-Z0-9_-]+$/, 'Invalid user ID format');
/** Validate chat message content */
exports.ChatMessageSchema = zod_1.z.object({
    type: zod_1.z.literal('chat'),
    content: zod_1.z
        .string()
        .min(1)
        .max(10000)
        .transform((v) => v.slice(0, 10000)), // hard cap
});
/** Validate file upload payload */
exports.FileUploadSchema = zod_1.z.object({
    type: zod_1.z.literal('file'),
    filename: zod_1.z
        .string()
        .min(1)
        .max(255)
        .regex(/^[a-zA-Z0-9._-]+$/, 'Invalid filename characters')
        .transform((v) => v.slice(0, 255)),
    mimeType: zod_1.z
        .string()
        .min(1)
        .max(100)
        .regex(/^[a-zA-Z0-9\/.-]+$/, 'Invalid MIME type'),
    data: zod_1.z.string().max(15 * 1024 * 1024, 'File too large'), // base64, max 10MB after decode
});
/** Validate WebSocket message */
exports.WebSocketMessageSchema = zod_1.z.discriminatedUnion('type', [
    exports.ChatMessageSchema,
    exports.FileUploadSchema,
    zod_1.z.object({
        type: zod_1.z.literal('typing'),
    }),
]);
/** Validate URL for SSRF prevention */
exports.ExternalUrlSchema = zod_1.z.string().refine((url) => {
    try {
        const parsed = new URL(url);
        const hostname = parsed.hostname.toLowerCase();
        // Block private/internal hostnames
        if (isReservedHostname(hostname))
            return false;
        // Only allow https
        if (parsed.protocol !== 'https:')
            return false;
        // Block suspicious ports
        const port = parsed.port ? parseInt(parsed.port) : 443;
        if (port < 1 || port > 65535)
            return false;
        return true;
    }
    catch {
        return false;
    }
}, { message: 'URL is not a valid external HTTPS URL' });
// ─── SSRF Protection ──────────────────────────────────────────────────────────
const INTERNAL_HOSTNAME_PATTERNS = [
    /^localhost$/i,
    /^::1$/i,
    /^127\.\d+\.\d+\.\d+$/,
    /^10\.\d+\.\d+\.\d+$/,
    /^172\.(1[6-9]|2\d|3[01])\.\d+\.\d+$/,
    /^192\.168\.\d+\.\d+$/,
    /^169\.254\.\d+\.\d+$/,
    /^0\.0\.0\.0$/,
    /^localhost\.localdomain$/i,
    /.*\.local$/i,
    /^(.*:)?::1$/, // IPv6 localhost
    /^(.*:)?fe80:/i, // IPv6 link-local
    /^(.*:)?fc00:/i, // IPv6 unique local
    /^(.*:)?fd00:/i, // IPv6 unique local
];
const _BLOCKED_IP_RANGES = [
    '0.0.0.0/8',
    '10.0.0.0/8',
    '172.16.0.0/12',
    '192.168.0.0/16',
    '169.254.0.0/16',
    '127.0.0.0/8',
    '::1/128',
    'fc00::/7',
    'fe80::/10',
];
/**
 * Check if a hostname is reserved/internal
 */
function isReservedHostname(hostname) {
    if (!hostname)
        return true;
    const h = hostname.toLowerCase();
    // Check exact and pattern matches
    for (const pattern of INTERNAL_HOSTNAME_PATTERNS) {
        if (pattern.test(h))
            return true;
    }
    // Check if it's an IP address
    if (/^\d+\.\d+\.\d+\.\d+$/.test(h)) {
        return isPrivateIP(h);
    }
    // Check IPv6
    if (h.includes(':')) {
        return isPrivateIPv6(h);
    }
    return false;
}
function isPrivateIP(ip) {
    const octets = ip.split('.').map(Number);
    if (octets.length !== 4)
        return false;
    const [a, b] = octets;
    if (a === 127)
        return true;
    if (a === 10)
        return true;
    if (a === 172 && b !== undefined && b >= 16 && b <= 31)
        return true;
    if (a === 192 && b === 168)
        return true;
    if (a === 169 && b === 254)
        return true;
    if (a === 0)
        return true;
    return false;
}
function isPrivateIPv6(ip) {
    const h = ip.toLowerCase();
    if (h === '::1')
        return true;
    if (h.startsWith('fe80:'))
        return true;
    if (h.startsWith('fc00:'))
        return true;
    if (h.startsWith('fd00:'))
        return true;
    if (h === '0:0:0:0:0:0:0:1')
        return true;
    return false;
}
/**
 * Validate a URL for outbound webhook delivery
 */
function validateOutboundUrl(url) {
    try {
        const parsed = new URL(url);
        // Only allow https
        if (parsed.protocol !== 'https:') {
            return { valid: false, reason: 'Only HTTPS URLs are allowed' };
        }
        const hostname = parsed.hostname;
        // Block internal hostnames
        if (isReservedHostname(hostname)) {
            return { valid: false, reason: 'Internal hostnames are not allowed' };
        }
        // Block authentication in URLs
        if (parsed.username || parsed.password) {
            return { valid: false, reason: 'Credentials in URLs are not allowed' };
        }
        return { valid: true };
    }
    catch {
        return { valid: false, reason: 'Invalid URL' };
    }
}
// ─── Content Sanitization ─────────────────────────────────────────────────────
/**
 * Sanitize HTML to prevent XSS while allowing safe formatting.
 * Uses a whitelist approach for tags and attributes.
 */
function sanitizeHTML(html) {
    // Use well-tested library sanitization with an allowlist approach.
    // This avoids multi-character regex pitfalls and ensures comments and
    // dangerous content are fully removed rather than partially rewritten.
    return (0, sanitize_html_1.default)(html, {
        // Allow basic formatting and common safe inline text elements.
        allowedTags: [
            'b',
            'strong',
            'i',
            'em',
            'u',
            's',
            'span',
            'br',
            'p',
            'ul',
            'ol',
            'li',
            'blockquote',
            'code',
            'pre',
            'a',
            'img',
        ],
        allowedAttributes: {
            'a': ['href', 'title', 'target', 'rel'],
            'img': ['src', 'alt', 'title'],
            '*': ['class'],
        },
        // Only allow safe URL schemes; blocks javascript:, vbscript:, etc.
        allowedSchemes: ['http', 'https', 'mailto'],
        // Restrict data: URIs to safe image types only on img tags.
        allowedSchemesByTag: {
            img: ['http', 'https', 'data'],
        },
        // Ensure noreferrer/noopener on links that open in a new tab.
        transformTags: {
            a: (tagName, attribs) => {
                const transformed = { ...attribs };
                if (transformed.target === '_blank') {
                    transformed.rel = transformed.rel
                        ? `${transformed.rel} noopener noreferrer`
                        : 'noopener noreferrer';
                }
                return { tagName, attribs: transformed };
            },
        },
    });
}
/**
 * Sanitize a filename to prevent path traversal
 */
function sanitizeFilename(filename) {
    return filename
        .replace(/\0/g, '') // null bytes
        .replace(/\.\./g, '') // path traversal
        .replace(/[/\\:*?"<>|]/g, '') // reserved filesystem chars
        .replace(/^(\.\.|\\|\/)/, '') // leading traversal
        .slice(0, 255); // max length
}
/**
 * Escape HTML entities in plain text
 */
function escapeHTML(str) {
    return str
        .replace(/&/g, '&amp;')
        .replace(/</g, '&lt;')
        .replace(/>/g, '&gt;')
        .replace(/"/g, '&quot;')
        .replace(/'/g, '&#x27;');
}
// ─── Command Sanitization ─────────────────────────────────────────────────────
/**
 * Characters and patterns that indicate command injection attempts
 */
const DANGEROUS_SHELL_CHARS = /[;&|`$\(\){}\[\]<>\\!#*?"'~\r\n]|\\0|\$\(|``|%0a|%0d/;
/**
 * Characters that are forbidden in sandboxed commands
 */
const BLOCKED_SHELL_CHARS_GLOBAL = /[;&|`$\(\){}<>\\!#*?"'~\r\n]|\\0|%0a|%0d/;
/**
 * Check if a command string contains dangerous shell characters
 */
function containsShellInjection(command) {
    return DANGEROUS_SHELL_CHARS.test(command);
}
/**
 * Validate a command against allowed patterns for sandbox execution.
 * Returns { valid: true } or { valid: false, reason: string }
 */
function validateSandboxCommand(command, allowedCommands) {
    if (!command || command.length === 0) {
        return { valid: false, reason: 'Empty command' };
    }
    if (command.length > 4096) {
        return { valid: false, reason: 'Command too long (max 4096 chars)' };
    }
    // Check for shell injection characters
    if (BLOCKED_SHELL_CHARS_GLOBAL.test(command)) {
        return { valid: false, reason: 'Command contains forbidden shell characters' };
    }
    // Parse into arguments
    const parts = parseCommandArgs(command);
    if (parts.length === 0) {
        return { valid: false, reason: 'No valid command parts found' };
    }
    // Check base command against whitelist
    const base = parts[0];
    if (!allowedCommands.has(base)) {
        return { valid: false, reason: `Command '${base}' is not allowed` };
    }
    return { valid: true };
}
/**
 * Parse command string into safe argument array (quote-aware)
 */
function parseCommandArgs(command) {
    const parts = [];
    const re = /("[^"]*"|'[^']*'|\S+)/g;
    let m;
    // Reset regex state
    re.lastIndex = 0;
    while ((m = re.exec(command)) !== null) {
        let p = m[0];
        // Strip quotes
        if ((p.startsWith('"') && p.endsWith('"')) || (p.startsWith("'") && p.endsWith("'"))) {
            p = p.slice(1, -1);
        }
        parts.push(p);
    }
    return parts;
}
/**
 * Structured audit logger for security events.
 * Logs to a dedicated audit channel (can be connected to SIEM).
 */
class SecurityAuditor {
    entries = [];
    maxEntries = 10000;
    /**
     * Log a security event
     */
    log(action, details, actor, ip) {
        // Never log secrets
        const safeDetails = this.sanitizeDetails(details);
        const entry = {
            timestamp: Date.now(),
            action,
            actor,
            ip,
            details: safeDetails,
        };
        this.entries.push(entry);
        // Cap entries to prevent memory issues
        if (this.entries.length > this.maxEntries) {
            this.entries.splice(0, this.entries.length - this.maxEntries);
        }
        // Also emit to stderr for immediate visibility
        console.error(`[AUDIT] ${action} | actor=${actor ?? 'unknown'} | ip=${ip ?? 'unknown'} | ${JSON.stringify(safeDetails)}`);
    }
    /**
     * Remove sensitive values from details before logging
     */
    sanitizeDetails(details) {
        const sensitiveKeys = [
            'password',
            'passwd',
            'secret',
            'token',
            'api_key',
            'apikey',
            'authorization',
            'credential',
            'private_key',
            'access_key',
            'supabase_key',
            'supabaseKey',
            'botToken',
            'authToken',
        ];
        const result = {};
        for (const [key, value] of Object.entries(details)) {
            const lowerKey = key.toLowerCase();
            if (sensitiveKeys.some((sk) => lowerKey.includes(sk))) {
                result[key] = '[REDACTED]';
            }
            else if (typeof value === 'object' && value !== null) {
                result[key] = this.sanitizeDetails(value);
            }
            else {
                result[key] = value;
            }
        }
        return result;
    }
    /**
     * Get recent audit entries
     */
    getEntries(limit = 100) {
        return this.entries.slice(-limit);
    }
    /**
     * Query audit entries by action type
     */
    queryByAction(action, since) {
        return this.entries.filter((e) => {
            if (e.action !== action)
                return false;
            if (since !== undefined && e.timestamp < since)
                return false;
            return true;
        });
    }
}
exports.SecurityAuditor = SecurityAuditor;
// ─── Timeout wrapper ─────────────────────────────────────────────────────────
/**
 * Wrap a promise with a timeout
 */
function withTimeout(promise, timeoutMs, timeoutError) {
    return Promise.race([
        promise,
        new Promise((_, reject) => setTimeout(() => reject(new Error(timeoutError ?? `Operation timed out after ${timeoutMs}ms`)), timeoutMs)),
    ]);
}
/**
 * Simple in-memory rate limiter for single-instance use.
 * For multi-instance deployments, use the rate-limiting feature with SQLite.
 */
class SimpleRateLimiter {
    entries = new Map();
    windowMs;
    max;
    constructor(windowMs, max) {
        this.windowMs = windowMs;
        this.max = max;
    }
    check(key) {
        const now = Date.now();
        const entry = this.entries.get(key);
        // Clean up expired entries
        if (!entry || entry.resetAt <= now) {
            const resetAt = now + this.windowMs;
            this.entries.set(key, { count: 1, resetAt });
            return { allowed: true, remaining: this.max - 1, resetAt };
        }
        if (entry.count >= this.max) {
            return { allowed: false, remaining: 0, resetAt: entry.resetAt };
        }
        entry.count++;
        return { allowed: true, remaining: this.max - entry.count, resetAt: entry.resetAt };
    }
    reset(key) {
        this.entries.delete(key);
    }
    cleanExpired() {
        const now = Date.now();
        for (const [key, entry] of this.entries) {
            if (entry.resetAt <= now) {
                this.entries.delete(key);
            }
        }
    }
}
exports.SimpleRateLimiter = SimpleRateLimiter;
// Singleton auditor
let auditor = null;
function getAuditor() {
    if (!auditor) {
        auditor = new SecurityAuditor();
    }
    return auditor;
}
//# sourceMappingURL=security.js.map