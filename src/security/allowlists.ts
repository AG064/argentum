/**
 * AG-Claw Allowlists
 *
 * Loads command, path, and host allowlists from security-policy.yaml.
 * Supports glob patterns (docker *, curl https://*.example.com/*),
 * whitelist and blacklist modes, and logs blocked attempts.
 */

import { readFileSync, existsSync } from 'fs';
import { resolve } from 'path';

import { parse } from 'yaml';

import { createLogger, type Logger } from '../core/logger';

export interface AllowlistConfig {
  allowedCommands: string[];
  blockedCommands: string[];
  allowedPaths: string[];
  blockedPaths: string[];
  allowedHosts: string[];
  blockedHosts: string[];
}

export interface AccessDecision {
  allowed: boolean;
  reason: string;
  matchedRule?: string;
}

const DEFAULT_CONFIG: AllowlistConfig = {
  allowedCommands: [],
  blockedCommands: [],
  allowedPaths: [],
  blockedPaths: [],
  allowedHosts: [],
  blockedHosts: [],
};

/**
 * Allowlist manager — loads rules from security-policy.yaml and evaluates
 * commands, paths, and hosts against them.
 *
 * - Blocked lists take priority over allowed lists.
 * - Glob patterns: `docker *` matches any docker subcommand.
 * - Host globs: `*.supabase.co` matches any subdomain.
 */
export class AllowlistManager {
  private config: AllowlistConfig = { ...DEFAULT_CONFIG };
  private mode: 'strict' | 'permissive';
  private logger: Logger;
  private blockedAttempts: Array<{
    timestamp: number;
    type: 'command' | 'path' | 'host';
    value: string;
    reason: string;
  }> = [];

  constructor(mode: 'strict' | 'permissive' = 'permissive') {
    this.mode = mode;
    this.logger = createLogger().child({ feature: 'allowlists' });
  }

  /**
   * Load allowlist rules from a YAML policy file.
   * Expected format (same as security-policy.yaml):
   *   allowedCommands: [...]
   *   blockedCommands: [...]
   *   allowedPaths: [...]
   *   blockedPaths: [...]
   *   allowedHosts: [...]
   *   blockedHosts: [...]
   */
  loadFromFile(filePath: string): void {
    const fullPath = resolve(filePath);
    if (!existsSync(fullPath)) {
      this.logger.warn(`Allowlist file not found: ${fullPath}, using defaults`);
      return;
    }

    try {
      const raw = readFileSync(fullPath, 'utf-8');
      const parsed = parse(raw) as Partial<AllowlistConfig>;

      this.config = {
        allowedCommands: parsed.allowedCommands ?? [],
        blockedCommands: parsed.blockedCommands ?? [],
        allowedPaths: parsed.allowedPaths ?? [],
        blockedPaths: parsed.blockedPaths ?? [],
        allowedHosts: parsed.allowedHosts ?? [],
        blockedHosts: parsed.blockedHosts ?? [],
      };

      this.logger.info('Allowlist loaded', {
        allowedCommands: this.config.allowedCommands.length,
        blockedCommands: this.config.blockedCommands.length,
        allowedPaths: this.config.allowedPaths.length,
        blockedPaths: this.config.blockedPaths.length,
        allowedHosts: this.config.allowedHosts.length,
        blockedHosts: this.config.blockedHosts.length,
      });
    } catch (err) {
      this.logger.error(`Failed to load allowlist: ${fullPath}`, {
        error: err instanceof Error ? err.message : String(err),
      });
    }
  }

  /**
   * Load allowlist config directly (programmatic setup).
   */
  loadConfig(config: Partial<AllowlistConfig>): void {
    this.config = {
      allowedCommands: config.allowedCommands ?? this.config.allowedCommands,
      blockedCommands: config.blockedCommands ?? this.config.blockedCommands,
      allowedPaths: config.allowedPaths ?? this.config.allowedPaths,
      blockedPaths: config.blockedPaths ?? this.config.blockedPaths,
      allowedHosts: config.allowedHosts ?? this.config.allowedHosts,
      blockedHosts: config.blockedHosts ?? this.config.blockedHosts,
    };
  }

  /**
   * Check if a command is allowed.
   * Extracts the base command (first word) and checks against lists.
   *
   * @example
   *   isCommandAllowed('ls -la')         // true
   *   isCommandAllowed('docker run x')   // true if 'docker *' or 'docker' in allowed
   *   isCommandAllowed('rm -rf /')       // false (blocked)
   */
  isCommandAllowed(fullCommand: string): AccessDecision {
    const trimmed = fullCommand.trim();
    if (!trimmed) {
      return this.logAndReturn(false, 'command', trimmed, 'Empty command');
    }

    // Extract base command and full command for pattern matching
    const baseCmd = trimmed.split(/\s+/)[0] ?? '';

    // 1. Check blocked list first (always wins)
    for (const pattern of this.config.blockedCommands) {
      if (this.matchGlob(trimmed, pattern) || this.matchGlob(baseCmd, pattern)) {
        return this.logAndReturn(false, 'command', trimmed, `Blocked by rule: ${pattern}`, pattern);
      }
    }

    // 2. Check allowed list
    for (const pattern of this.config.allowedCommands) {
      if (this.matchGlob(trimmed, pattern) || this.matchGlob(baseCmd, pattern)) {
        return { allowed: true, reason: `Allowed by rule: ${pattern}`, matchedRule: pattern };
      }
    }

    // 3. Default depends on mode
    if (this.mode === 'strict') {
      return this.logAndReturn(false, 'command', trimmed, 'Not in allowed list (strict mode)');
    }
    return { allowed: true, reason: 'Not explicitly blocked (permissive mode)' };
  }

  /**
   * Check if a filesystem path is allowed.
   *
   * @example
   *   isPathAllowed('/app/data/config.json')  // true
   *   isPathAllowed('/etc/shadow')            // false
   */
  isPathAllowed(path: string): AccessDecision {
    const trimmed = path.trim();
    if (!trimmed) {
      return this.logAndReturn(false, 'path', trimmed, 'Empty path');
    }

    // 1. Check blocked paths first
    for (const pattern of this.config.blockedPaths) {
      if (this.matchPath(trimmed, pattern)) {
        return this.logAndReturn(false, 'path', trimmed, `Blocked path: ${pattern}`, pattern);
      }
    }

    // 2. Check allowed paths
    for (const pattern of this.config.allowedPaths) {
      if (this.matchPath(trimmed, pattern)) {
        return { allowed: true, reason: `Allowed path: ${pattern}`, matchedRule: pattern };
      }
    }

    // 3. Default
    if (this.mode === 'strict') {
      return this.logAndReturn(false, 'path', trimmed, 'Not in allowed paths (strict mode)');
    }
    return { allowed: true, reason: 'Not explicitly blocked (permissive mode)' };
  }

  /**
   * Check if a network host is allowed.
   *
   * @example
   *   isHostAllowed('api.openai.com')          // true
   *   isHostAllowed('evil.example.com')         // depends on config
   *   isHostAllowed('myproject.supabase.co')    // true if *.supabase.co is allowed
   */
  isHostAllowed(host: string): AccessDecision {
    const trimmed = host.trim().toLowerCase();
    if (!trimmed) {
      return this.logAndReturn(false, 'host', trimmed, 'Empty host');
    }

    // Strip port if present
    const hostOnly = trimmed.includes(':') ? trimmed.split(':')[0] ?? trimmed : trimmed;

    // 1. Check blocked hosts first
    for (const pattern of this.config.blockedHosts) {
      if (this.matchHost(hostOnly, pattern.toLowerCase())) {
        return this.logAndReturn(false, 'host', hostOnly, `Blocked host: ${pattern}`, pattern);
      }
    }

    // 2. Check allowed hosts
    for (const pattern of this.config.allowedHosts) {
      if (this.matchHost(hostOnly, pattern.toLowerCase())) {
        return { allowed: true, reason: `Allowed host: ${pattern}`, matchedRule: pattern };
      }
    }

    // 3. Default
    if (this.mode === 'strict') {
      return this.logAndReturn(false, 'host', hostOnly, 'Not in allowed hosts (strict mode)');
    }
    return { allowed: true, reason: 'Not explicitly blocked (permissive mode)' };
  }

  /**
   * Get all blocked attempts (for audit/debugging).
   */
  getBlockedAttempts(limit = 100): typeof this.blockedAttempts {
    return this.blockedAttempts.slice(-limit);
  }

  /**
   * Clear the blocked attempts log.
   */
  clearBlockedLog(): void {
    this.blockedAttempts = [];
  }

  /**
   * Get current config.
   */
  getConfig(): AllowlistConfig {
    return { ...this.config };
  }

  /**
   * Set mode (strict or permissive).
   */
  setMode(mode: 'strict' | 'permissive'): void {
    this.mode = mode;
    this.logger.info(`Allowlist mode set to: ${mode}`);
  }

  // ─── Internal helpers ───────────────────────────────────────

  /**
   * Match a string against a glob pattern.
   * Supports * (any chars) and ? (single char).
   * "docker *" matches "docker run image" and "docker ps"
   */
  private matchGlob(value: string, pattern: string): boolean {
    if (pattern === '*') return true;

    // Convert glob to regex
    const escaped = pattern
      .replace(/[.+^${}()|[\]\\]/g, '\\$&')
      .replace(/\*/g, '.*')
      .replace(/\?/g, '.');

    try {
      return new RegExp(`^${escaped}$`, 'i').test(value);
    } catch {
      return value === pattern;
    }
  }

  /**
   * Match a filesystem path against a pattern.
   * Patterns ending with / are treated as directory prefixes.
   */
  private matchPath(path: string, pattern: string): boolean {
    // Normalize paths
    const normalizedPath = path.replace(/\\/g, '/');
    const normalizedPattern = pattern.replace(/\\/g, '/');

    // Directory prefix: "/app/" matches "/app/anything"
    if (normalizedPattern.endsWith('/')) {
      return normalizedPath.startsWith(normalizedPattern) ||
        normalizedPath === normalizedPattern.slice(0, -1);
    }

    // Glob pattern
    if (normalizedPattern.includes('*')) {
      return this.matchGlob(normalizedPath, normalizedPattern);
    }

    // Exact or prefix match
    return normalizedPath === normalizedPattern ||
      normalizedPath.startsWith(`${normalizedPattern  }/`);
  }

  /**
   * Match a hostname against a pattern.
   * "*.example.com" matches "sub.example.com" and "deep.sub.example.com"
   */
  private matchHost(host: string, pattern: string): boolean {
    if (pattern.startsWith('*.')) {
      const domain = pattern.slice(2);
      return host === domain || host.endsWith(`.${  domain}`);
    }
    return host === pattern;
  }

  private logAndReturn(
    allowed: boolean,
    type: 'command' | 'path' | 'host',
    value: string,
    reason: string,
    rule?: string,
  ): AccessDecision {
    if (!allowed) {
      const entry = {
        timestamp: Date.now(),
        type,
        value,
        reason,
      };
      this.blockedAttempts.push(entry);

      // Cap the log at 1000 entries
      if (this.blockedAttempts.length > 1000) {
        this.blockedAttempts = this.blockedAttempts.slice(-500);
      }

      this.logger.warn(`Blocked ${type}: ${value}`, { reason, rule });
    }

    return { allowed, reason, matchedRule: rule };
  }
}

// ─── Singleton ────────────────────────────────────────────────

let instance: AllowlistManager | null = null;

/**
 * Get or create the global allowlist manager.
 */
export function getAllowlist(mode?: 'strict' | 'permissive'): AllowlistManager {
  if (!instance) {
    instance = new AllowlistManager(mode);
  }
  return instance;
}

/**
 * Reset the singleton (for testing).
 */
export function resetAllowlist(): void {
  instance = null;
}
