"use strict";
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
Object.defineProperty(exports, "__esModule", { value: true });
const child_process_1 = require("child_process");
const http_1 = require("http");
const ws_1 = require("ws");
const DEFAULT_CONFIG = {
    enabled: false,
    port: 3004,
    host: '127.0.0.1',
    defaultTimeoutMs: 30000,
};
class ACPHarnessFeature {
    meta = {
        name: 'acp',
        version: '0.0.1',
        description: 'Agent Control Protocol harness for code execution',
        dependencies: [],
    };
    config = { ...DEFAULT_CONFIG };
    ctx;
    server = null;
    wsServer = null;
    async init(config, context) {
        this.ctx = context;
        this.config = { ...this.config, ...config };
    }
    async start() {
        if (!this.config.enabled)
            return;
        this.server = (0, http_1.createServer)(async (req, res) => {
            this.handleHTTP(req, res);
        });
        this.server.listen(this.config.port, this.config.host, () => {
            this.ctx.logger.info('ACP harness started', {
                port: this.config.port,
                host: this.config.host,
            });
        });
        // WebSocket server for streaming
        this.wsServer = new ws_1.WebSocketServer({ server: this.server, path: '/acp/stream' });
        this.wsServer.on('connection', (ws) => {
            ws.on('message', (data) => {
                try {
                    const req = JSON.parse(data.toString());
                    this.executeWithStream(ws, req);
                }
                catch (err) {
                    ws.send(JSON.stringify({ error: 'Invalid request' }));
                }
            });
        });
    }
    async stop() {
        this.server?.close();
        this.wsServer?.close();
    }
    async healthCheck() {
        return {
            healthy: this.server !== null,
            details: { port: this.config.port },
        };
    }
    async handleHTTP(req, res) {
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
        req.on('data', (chunk) => (body += chunk));
        req.on('end', async () => {
            try {
                const reqBody = JSON.parse(body);
                const result = await this.execute(reqBody);
                res.setHeader('Content-Type', 'application/json');
                res.end(JSON.stringify(result));
            }
            catch (err) {
                res.statusCode = 500;
                const errorMsg = err instanceof Error ? err.message : 'Unknown error';
                // Escape HTML in error message to prevent XSS if displayed in HTML context
                const escapedError = errorMsg
                    .replace(/&/g, '&amp;')
                    .replace(/</g, '&lt;')
                    .replace(/>/g, '&gt;')
                    .replace(/"/g, '&quot;');
                res.end(JSON.stringify({
                    success: false,
                    error: escapedError,
                    stdout: '',
                    stderr: '',
                    exitCode: -1,
                    durationMs: 0,
                }));
            }
        });
    }
    async execute(req) {
        const start = Date.now();
        const timeoutMs = req.timeoutMs || this.config.defaultTimeoutMs;
        // Simple execution via child_process (non-isolated)
        return new Promise((resolve) => {
            const cmd = this.getCommand(req.language);
            const proc = (0, child_process_1.spawn)(cmd, ['-e', req.code], { timeout: timeoutMs });
            let stdout = '', stderr = '';
            proc.stdout?.on('data', (d) => (stdout += d));
            proc.stderr?.on('data', (d) => (stderr += d));
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
    async executeWithStream(ws, req) {
        const _start = Date.now();
        const _timeoutMs = req.timeoutMs || this.config.defaultTimeoutMs;
        // For streaming, we'll just execute and send incremental updates
        // A proper implementation would stream stdout/stderr in real-time
        const result = await this.execute(req);
        ws.send(JSON.stringify(result));
    }
    getCommand(language) {
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
exports.default = new ACPHarnessFeature();
//# sourceMappingURL=index.js.map