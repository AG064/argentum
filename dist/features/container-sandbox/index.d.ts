/**
 * AG-Claw Container Sandbox
 *
 * Runs shell commands inside isolated Docker containers.
 * Features:
 *   - Network isolation (--network none)
 *   - Read-only root filesystem
 *   - Limited tmpfs for /tmp
 *   - Memory + CPU limits
 *   - Timeout enforcement (kills container after N ms)
 *   - Returns stdout, stderr, exit code
 */
import { type FeatureModule, type FeatureContext, type FeatureMeta, type HealthStatus } from '../../core/plugin-loader';
export interface SandboxConfig {
    enabled: boolean;
    image: string;
    memoryLimit: string;
    cpuLimit: string;
    timeoutMs: number;
    workspacePath: string;
    networkAccess: boolean;
    readOnlyRoot: boolean;
    tmpfsSize: string;
}
export interface SandboxResult {
    success: boolean;
    stdout: string;
    stderr: string;
    exitCode: number;
    durationMs: number;
    timedOut: boolean;
}
/**
 * Execute a shell command inside an isolated Docker container.
 *
 * @param command - Shell command to run
 * @param options - Sandbox configuration overrides
 */
export declare function runInSandbox(command: string, options?: Partial<SandboxConfig>): Promise<SandboxResult>;
declare class ContainerSandboxFeature implements FeatureModule {
    readonly meta: FeatureMeta;
    private config;
    private ctx;
    init(config: Record<string, unknown>, context: FeatureContext): Promise<void>;
    start(): Promise<void>;
    stop(): Promise<void>;
    healthCheck(): Promise<HealthStatus>;
    /**
     * Execute a command in the sandbox (public API for other features).
     */
    execute(command: string, overrides?: Partial<SandboxConfig>): Promise<SandboxResult>;
}
declare const _default: ContainerSandboxFeature;
export default _default;
//# sourceMappingURL=index.d.ts.map