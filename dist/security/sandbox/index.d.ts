/**
 * Argentum Sandbox Executor
 *
 * Secure code execution environment with:
 * - Filesystem access control (whitelist/blacklist)
 * - Network isolation
 * - Memory and CPU limits
 * - Execution timeout
 * - Language-specific sandboxes (JS, Python, Bash)
 * - Full audit trail
 */
import type { SandboxConfig, SandboxResult, SandboxCheckResult, AgentAction } from '../types';
export declare class SandboxExecutor {
    private config;
    private logger;
    private executionCount;
    private blockedCount;
    constructor(config?: Partial<SandboxConfig>);
    /**
     * Update sandbox configuration.
     */
    updateConfig(config: Partial<SandboxConfig>): void;
    getConfig(): SandboxConfig;
    /**
     * Check if a path is allowed for filesystem operations.
     * Returns { allowed: true } or { allowed: false, reason: string }.
     */
    checkPath(path: string): SandboxCheckResult;
    /**
     * Check if network access to a host:port is allowed.
     */
    checkNetwork(host: string, port: number): SandboxCheckResult;
    /**
     * Execute code in a sandboxed environment.
     */
    execute(code: string, lang: string, options?: {
        timeoutMs?: number;
        workingDir?: string;
    }): Promise<SandboxResult>;
    private executeJavaScript;
    private executePython;
    private executeBash;
    /**
     * Check an action against sandbox policies before execution.
     */
    checkAction(action: AgentAction): SandboxCheckResult;
    private logAudit;
    getStats(): {
        enabled: boolean;
        executions: number;
        blocked: number;
    };
}
export declare function getSandboxExecutor(config?: Partial<SandboxConfig>): SandboxExecutor;
export declare function resetSandboxExecutor(): void;
//# sourceMappingURL=index.d.ts.map