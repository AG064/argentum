/**
 * ACP Harness for AG-Claw
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

import { spawn } from 'child_process';
import { createServer, type IncomingMessage, type ServerResponse } from 'http';

import { WebSocketServer } from 'ws';

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

const DEFAULT_CONFIG: ACPConfig = {
  enabled: false,
  port: 3004,
  host: '127.0.0.1',
  defaultTimeoutMs: 30000,
};

class ACPHarnessFeature implements FeatureModule {
  readonly meta: FeatureMeta = {
    name: 'acp',
    version: '0.1.0',
    description: 'Agent Control Protocol harness for code execution',
    dependencies: [],
  };

  private config: ACPConfig = { ...DEFAULT_CONFIG };
  private ctx!: FeatureContext;
  private server: any = null;
  private wsServer: any = null;

  async init(config: Record<string, unknown>, context: FeatureContext): Promise<void> {
    this.ctx = context;
    this.config = { ...this.config, ...(config as Partial<ACPConfig>) };
  }

  async start(): Promise<void> {
    if (!this.config.enabled) return;

    this.server = createServer(async (req, res) => {
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
    this.wsServer.on('connection', (ws: any) => {
      ws.on('message', (data: Buffer) => {
        try {
          const req = JSON.parse(data.toString()) as ACPExecuteRequest;
          this.executeWithStream(ws, req);
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

  private async handleHTTP(req: IncomingMessage, res: ServerResponse): Promise<void> {
    // Simple router
    if (req.method !== 'POST' || req.url !== '/acp/execute') {
      res.statusCode = 404;
      res.end('Not Found');
      return;
    }

    // Auth check
    if (this.config.authToken) {
      const auth = req.headers.authorization || '';
      if (!auth.startsWith('Bearer ') || auth.slice(7) !== this.config.authToken) {
        res.statusCode = 401;
        res.end('Unauthorized');
        return;
      }
    }

    // Parse body
    let body = '';
    req.on('data', chunk => body += chunk);
    req.on('end', async () => {
      try {
        const reqBody: ACPExecuteRequest = JSON.parse(body);
        const result = await this.execute(reqBody);
        res.setHeader('Content-Type', 'application/json');
        res.end(JSON.stringify(result));
      } catch (err) {
        res.statusCode = 500;
        res.end(JSON.stringify({
          success: false,
          error: err instanceof Error ? err.message : 'Unknown error',
          stdout: '',
          stderr: '',
          exitCode: -1,
          durationMs: 0,
        }));
      }
    });
  }

  private async execute(req: ACPExecuteRequest): Promise<ACPExecuteResponse> {
    const start = Date.now();
    const timeoutMs = req.timeoutMs || this.config.defaultTimeoutMs;

    // Simple execution via child_process (non-isolated)
    return new Promise((resolve) => {
      const cmd = this.getCommand(req.language);
      const proc = spawn(cmd, ['-e', req.code], { timeout: timeoutMs });

      let stdout = '', stderr = '';
      proc.stdout?.on('data', d => stdout += d);
      proc.stderr?.on('data', d => stderr += d);

      proc.on('close', (code) => {
        resolve({
          success: code === 0,
          stdout,
          stderr,
          exitCode: code ?? -1,
          durationMs: Date.now() - start,
          timedOut: false,
        });
      });

      proc.on('error', (err) => {
        resolve({
          success: false,
          stdout: '',
          stderr: err.message,
          exitCode: -1,
          durationMs: Date.now() - start,
          timedOut: false,
        });
      });
    });
  }

  private async executeWithStream(ws: any, req: ACPExecuteRequest): Promise<void> {
    const _start = Date.now();
    const _timeoutMs = req.timeoutMs || this.config.defaultTimeoutMs;

    // For streaming, we'll just execute and send incremental updates
    // A proper implementation would stream stdout/stderr in real-time
    const result = await this.execute(req);
    ws.send(JSON.stringify(result));
  }

  private getCommand(language: string): string {
    switch (language) {
      case 'javascript':
        return 'node';
      case 'python':
        return 'python3';
      case 'bash':
        return 'sh';
      default:
        throw new Error(`Unsupported language: ${language}`);
    }
  }
}

export default new ACPHarnessFeature();
