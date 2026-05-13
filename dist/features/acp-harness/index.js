"use strict";
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
Object.defineProperty(exports, "__esModule", { value: true });
const http_1 = require("http");
const ws_1 = require("ws");
const sandbox_1 = require("../../security/sandbox");
const DEFAULT_CONFIG = {
    enabled: false,
    port: 3004,
    host: '127.0.0.1',
    defaultTimeoutMs: 30000,
};
class ACPHarnessFeature {
    meta = {
        name: 'acp',
        version: '0.0.5',
        description: 'Agent Control Protocol harness for code execution',
        dependencies: [],
    };
    config = { ...DEFAULT_CONFIG };
    ctx;
    server = null;
    wsServer = null;
    sandbox = new sandbox_1.SandboxExecutor();
    async init(config, context) {
        this.ctx = context;
        this.config = { ...this.config, ...config };
    }
    async start() {
        if (!this.config.enabled)
            return;
        this.server = (0, http_1.createServer)((req, res) => {
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
                    void this.executeWithStream(ws, req).catch((err) => {
                        ws.send(JSON.stringify({
                            error: err instanceof Error ? err.message : 'Stream execution failed',
                        }));
                    });
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
    handleHTTP(req, res) {
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
            void this.handleExecuteBody(body, res).catch((err) => {
                this.writeExecutionError(res, err);
            });
        });
    }
    async handleExecuteBody(body, res) {
        try {
            const reqBody = JSON.parse(body);
            const result = await this.execute(reqBody);
            res.setHeader('Content-Type', 'application/json');
            res.end(JSON.stringify(result));
        }
        catch (err) {
            this.writeExecutionError(res, err);
        }
    }
    writeExecutionError(res, err) {
        res.statusCode = 500;
        const errorMsg = err instanceof Error ? err.message : 'Unknown error';
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
    async execute(req) {
        const start = Date.now();
        const timeoutMs = req.timeoutMs ?? this.config.defaultTimeoutMs;
        const result = await this.sandbox.execute(req.code, req.language, {
            timeoutMs,
            workingDir: req.workspace,
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
    async executeWithStream(ws, req) {
        const _start = Date.now();
        const _timeoutMs = req.timeoutMs ?? this.config.defaultTimeoutMs;
        // For streaming, we'll just execute and send incremental updates
        // A proper implementation would stream stdout/stderr in real-time
        const result = await this.execute(req);
        ws.send(JSON.stringify(result));
    }
}
exports.default = new ACPHarnessFeature();
//# sourceMappingURL=index.js.map