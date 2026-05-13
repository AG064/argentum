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

import {
  createServer,
  type IncomingMessage,
  type Server as HttpServer,
  type ServerResponse,
} from 'http';

import { type RawData, type WebSocket, WebSocketServer } from 'ws';

import {
  type FeatureModule,
  type FeatureContext,
  type FeatureMeta,
  type HealthStatus,
} from '../../core/plugin-loader';
import { SandboxExecutor } from '../../security/sandbox';
import { createWorkspaceBoundary } from '../../security/workspace-boundary';

export interface ACPConfig {
  enabled: boolean;
  port: number;
  host: string;
  authToken?: string;
  defaultTimeoutMs: number;
  workspaceRoot?: string;
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

const DEFAULT_CONFIG: ACPConfig = {
  enabled: false,
  port: 3004,
  host: '127.0.0.1',
  defaultTimeoutMs: 30000,
};

class ACPHarnessFeature implements FeatureModule {
  readonly meta: FeatureMeta = {
    name: 'acp',
    version: '0.0.5',
    description: 'Agent Control Protocol harness for code execution',
    dependencies: [],
  };

  private config: ACPConfig = { ...DEFAULT_CONFIG };
  private ctx!: FeatureContext;
  private server: HttpServer | null = null;
  private wsServer: WebSocketServer | null = null;
  private sandbox = new SandboxExecutor();

  async init(config: Record<string, unknown>, context: FeatureContext): Promise<void> {
    this.ctx = context;
    this.config = { ...this.config, ...(config as Partial<ACPConfig>) };
  }

  async start(): Promise<void> {
    if (!this.config.enabled) return;

    this.server = createServer((req, res) => {
      this.handleHTTP(req, res);
    });

    this.server.listen(this.config.port, this.config.host, () => {
      this.ctx.logger.info('ACP harness started', {
        port: this.config.port,
        host: this.config.host,
      });
    });

    // WebSocket server for streaming
    this.wsServer = new WebSocketServer({ server: this.server, path: '/acp/stream' });
    this.wsServer.on('connection', (ws: WebSocket) => {
      ws.on('message', (data: RawData) => {
        try {
          const req = JSON.parse(data.toString()) as ACPExecuteRequest;
          void this.executeWithStream(ws, req).catch((err: unknown) => {
            ws.send(
              JSON.stringify({
                error: err instanceof Error ? err.message : 'Stream execution failed',
              }),
            );
          });
        } catch (err) {
          ws.send(JSON.stringify({ error: 'Invalid request' }));
        }
      });
    });
  }

  async stop(): Promise<void> {
    this.server?.close();
    this.wsServer?.close();
  }

  async healthCheck(): Promise<HealthStatus> {
    return {
      healthy: this.server !== null,
      details: { port: this.config.port },
    };
  }

  private handleHTTP(req: IncomingMessage, res: ServerResponse): void {
    // Simple router
    if (req.method !== 'POST' || req.url !== '/acp/execute') {
      res.statusCode = 404;
      res.end('Not Found');
      return;
    }

    // Auth check
    if (this.config.authToken) {
      const auth = req.headers.authorization ?? '';
      if (!auth.startsWith('Bearer ') || auth.slice(7) !== this.config.authToken) {
        res.statusCode = 401;
        res.end('Unauthorized');
        return;
      }
    }

    // Parse body
    let body = '';
    req.on('data', (chunk) => (body += chunk));
    req.on('end', () => {
      void this.handleExecuteBody(body, res).catch((err: unknown) => {
        this.writeExecutionError(res, err);
      });
    });
  }

  private async handleExecuteBody(body: string, res: ServerResponse): Promise<void> {
    try {
      const reqBody = JSON.parse(body) as ACPExecuteRequest;
      const result = await this.execute(reqBody);
      res.setHeader('Content-Type', 'application/json');
      res.end(JSON.stringify(result));
    } catch (err) {
      this.writeExecutionError(res, err);
    }
  }

  private writeExecutionError(res: ServerResponse, err: unknown): void {
    res.statusCode = 500;
    const errorMsg = err instanceof Error ? err.message : 'Unknown error';
    const escapedError = errorMsg
      .replace(/&/g, '&amp;')
      .replace(/</g, '&lt;')
      .replace(/>/g, '&gt;')
      .replace(/"/g, '&quot;');
    res.end(
      JSON.stringify({
        success: false,
        error: escapedError,
        stdout: '',
        stderr: '',
        exitCode: -1,
        durationMs: 0,
      }),
    );
  }

  private async execute(req: ACPExecuteRequest): Promise<ACPExecuteResponse> {
    const start = Date.now();
    const timeoutMs = req.timeoutMs ?? this.config.defaultTimeoutMs;
    const workspaceRoot = this.config.workspaceRoot ?? this.ctx.config.security.capabilities.workspaceRoot;
    const workspaceBoundary = createWorkspaceBoundary(workspaceRoot);
    const workingDir = req.workspace ? workspaceBoundary.assertPath(req.workspace) : undefined;

    const result = await this.sandbox.execute(req.code, req.language, {
      timeoutMs,
      workingDir,
    });

    return {
      success: result.success,
      stdout: result.output ?? '',
      stderr: result.error ?? '',
      exitCode: result.exitCode ?? (result.success ? 0 : -1),
      durationMs: Date.now() - start,
      timedOut: result.error?.includes('timed out') ?? false,
    };
  }

  private async executeWithStream(ws: WebSocket, req: ACPExecuteRequest): Promise<void> {
    const _start = Date.now();
    const _timeoutMs = req.timeoutMs ?? this.config.defaultTimeoutMs;

    // For streaming, we'll just execute and send incremental updates
    // A proper implementation would stream stdout/stderr in real-time
    const result = await this.execute(req);
    ws.send(JSON.stringify(result));
  }

  // Note: language mapping and execution is delegated to SandboxExecutor.
}

export default new ACPHarnessFeature();
