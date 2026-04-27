/**
 * Argentum Allowlists
 *
 * Loads command, path, and host allowlists from security-policy.yaml.
 * Supports glob patterns (docker *, curl https://*.example.com/*),
 * whitelist and blacklist modes, and logs blocked attempts.
 */
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
/**
 * Allowlist manager — loads rules from security-policy.yaml and evaluates
 * commands, paths, and hosts against them.
 *
 * - Blocked lists take priority over allowed lists.
 * - Glob patterns: `docker *` matches any docker subcommand.
 * - Host globs: `*.supabase.co` matches any subdomain.
 */
export declare class AllowlistManager {
    private config;
    private mode;
    private logger;
    private blockedAttempts;
    constructor(mode?: 'strict' | 'permissive');
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
    loadFromFile(filePath: string): void;
    /**
     * Load allowlist config directly (programmatic setup).
     */
    loadConfig(config: Partial<AllowlistConfig>): void;
    /**
     * Check if a command is allowed.
     * Extracts the base command (first word) and checks against lists.
     *
     * @example
     *   isCommandAllowed('ls -la')         // true
     *   isCommandAllowed('docker run x')   // true if 'docker *' or 'docker' in allowed
     *   isCommandAllowed('rm -rf /')       // false (blocked)
     */
    isCommandAllowed(fullCommand: string): AccessDecision;
    /**
     * Check if a filesystem path is allowed.
     *
     * @example
     *   isPathAllowed('/app/data/config.json')  // true
     *   isPathAllowed('/etc/shadow')            // false
     */
    isPathAllowed(path: string): AccessDecision;
    /**
     * Check if a network host is allowed.
     *
     * @example
     *   isHostAllowed('api.openai.com')          // true
     *   isHostAllowed('evil.example.com')         // depends on config
     *   isHostAllowed('myproject.supabase.co')    // true if *.supabase.co is allowed
     */
    isHostAllowed(host: string): AccessDecision;
    /**
     * Get all blocked attempts (for audit/debugging).
     */
    getBlockedAttempts(limit?: number): typeof this.blockedAttempts;
    /**
     * Clear the blocked attempts log.
     */
    clearBlockedLog(): void;
    /**
     * Get current config.
     */
    getConfig(): AllowlistConfig;
    /**
     * Set mode (strict or permissive).
     */
    setMode(mode: 'strict' | 'permissive'): void;
    /**
     * Match a string against a glob pattern.
     * Supports * (any chars) and ? (single char).
     * "docker *" matches "docker run image" and "docker ps"
     */
    private matchGlob;
    /**
     * Match a filesystem path against a pattern.
     * Patterns ending with / are treated as directory prefixes.
     */
    private matchPath;
    /**
     * Match a hostname against a pattern.
     * "*.example.com" matches "sub.example.com" and "deep.sub.example.com"
     */
    private matchHost;
    private logAndReturn;
}
/**
 * Get or create the global allowlist manager.
 */
export declare function getAllowlist(mode?: 'strict' | 'permissive'): AllowlistManager;
/**
 * Reset the singleton (for testing).
 */
export declare function resetAllowlist(): void;
//# sourceMappingURL=allowlists.d.ts.map