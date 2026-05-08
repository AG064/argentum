/**
 * ACP Harness for Argentum
 *
 * HTTP/WebSocket endpoint for running code in sandbox.
 * Allows remote agents to execute tasks securely.
 *
 * Config:
 *   acp:
 *     enabled: true
 *     port: 3004
 *     host: 127.0.0.1
 *     authToken: "secret"  # optional
 *
 * Endpoints:
 *   POST /acp/execute  - run code (JSON body)
 *   WS  /acp/stream   - streaming output
 */
import { type FeatureModule, type FeatureContext, type FeatureMeta, type HealthStatus } from '../../core/plugin-loader';
export interface ACPConfig {
    enabled: boolean;
    port: number;
    host: string;
    authToken?: string;
    defaultTimeoutMs: number;
}
export interface ACPExecuteRequest {
    code: string;
    language: 'javascript' | 'python' | 'bash';
    timeoutMs?: number;
    workspace?: string;
}
export interface ACPExecuteResponse {
    success: boolean;
    stdout: string;
    stderr: string;
    exitCode: number;
    durationMs: number;
    timedOut?: boolean;
    error?: string;
}
declare class ACPHarnessFeature implements FeatureModule {
    readonly meta: FeatureMeta;
    private config;
    private ctx;
    private server;
    private wsServer;
    init(config: Record<string, unknown>, context: FeatureContext): Promise<void>;
    start(): Promise<void>;
    stop(): Promise<void>;
    healthCheck(): Promise<HealthStatus>;
    private handleHTTP;
    private handleExecuteBody;
    private writeExecutionError;
    private execute;
    private executeWithStream;
    private getCommand;
}
declare const _default: ACPHarnessFeature;
export default _default;
//# sourceMappingURL=index.d.ts.map