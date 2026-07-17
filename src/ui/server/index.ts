// SPDX-License-Identifier: MIT
// Copyright (c) 2024-2026 AG064
/**
 * Argentum Dashboard Server
 *
 * A lightweight secure web server for the Argentum dashboard.
 * Features:
 * - HTTP Basic Auth
 * - Static file serving
 * - WebSocket for real-time updates
 * - Rate limiting
 * - CORS configuration
 */

import * as crypto from 'crypto';
import * as fs from 'fs';
import * as http from 'http';
import * as https from 'https';
import * as path from 'path';
import * as url from 'url';

import bcrypt from 'bcryptjs';
import { WebSocketServer, WebSocket } from 'ws';

// Types
interface AuthConfig {
  username: string;
  passwordHash: string;
}

interface ServerConfig {
  port: number;
  host: string;
  staticDir: string;
  auth: AuthConfig;
  rateLimit: {
    windowMs: number;
    maxRequests: number;
  };
  cors: {
    enabled: boolean;
    allowedOrigins: string[];
  };
}

interface RateLimitEntry {
  count: number;
  resetAt: number;
}

// Rate limit storage
const rateLimits = new Map<string, RateLimitEntry>();

// Connected WebSocket clients
const wsClients = new Set<WebSocket>();

function envOrDefault(name: string, fallback: string): string {
  const value = process.env[name];
  return value?.trim() ? value : fallback;
}

function firstEnvOrDefault(names: string[], fallback: string): string {
  for (const name of names) {
    const value = process.env[name];
    if (value?.trim()) return value;
  }
  return fallback;
}

// Default configuration
const DEFAULT_CONFIG: ServerConfig = {
  port: parseInt(envOrDefault('ARGENTUM_DASHBOARD_PORT', '3000'), 10),
  host: envOrDefault('ARGENTUM_DASHBOARD_HOST', '127.0.0.1'),
  staticDir: path.join(__dirname, '..', 'dashboard'),
  auth: {
    username: envOrDefault('ARGENTUM_DASHBOARD_USER', 'admin'),
    passwordHash: process.env.ARGENTUM_DASHBOARD_PASS_HASH ?? '', // Will be generated if empty
  },
  rateLimit: {
    windowMs: 60 * 1000, // 1 minute
    maxRequests: 100,
  },
  cors: {
    enabled: true,
    allowedOrigins: ['http://localhost:3000', 'http://127.0.0.1:3000'],
  },
};

/**
 * Load configuration from argentum.json
 */
function loadConfig(): ServerConfig {
  const config = { ...DEFAULT_CONFIG };

  // Try the new Argentum config first, then the legacy config file name.
  const workDir = firstEnvOrDefault(['ARGENTUM_WORKDIR', 'ARGENTUM_WORKDIR'], process.cwd());
  const preferredConfigPath = path.join(workDir, 'argentum.json');
  const legacyConfigPath = path.join(workDir, 'agclaw.json');
  const configPath = fs.existsSync(preferredConfigPath) ? preferredConfigPath : legacyConfigPath;

  if (fs.existsSync(configPath)) {
    try {
      const argentumConfig = JSON.parse(fs.readFileSync(configPath, 'utf8'));

      if (argentumConfig.dashboard) {
        config.port = argentumConfig.dashboard.port ?? config.port;
        config.host = argentumConfig.dashboard.host ?? config.host;

        if (argentumConfig.dashboard.auth) {
          config.auth.username = argentumConfig.dashboard.auth.username ?? config.auth.username;
          if (argentumConfig.dashboard.auth.password) {
            config.auth.passwordHash = hashPassword(argentumConfig.dashboard.auth.password);
          }
        }

        if (argentumConfig.dashboard.rateLimit) {
          config.rateLimit.windowMs =
            argentumConfig.dashboard.rateLimit.windowMs ?? config.rateLimit.windowMs;
          config.rateLimit.maxRequests =
            argentumConfig.dashboard.rateLimit.maxRequests ?? config.rateLimit.maxRequests;
        }

        if (argentumConfig.dashboard.cors) {
          config.cors.allowedOrigins =
            argentumConfig.dashboard.cors.allowedOrigins ?? config.cors.allowedOrigins;
        }
      }
    } catch (err) {
      console.warn('[Dashboard Server] Failed to parse Argentum config:', (err as Error).message);
    }
  }

  // Generate password hash if not set
  if (!config.auth.passwordHash) {
    const envPass = process.env.ARGENTUM_DASHBOARD_PASS;
    if (!envPass) {
      // Try to load a previously persisted hash
      const workDir = firstEnvOrDefault(['ARGENTUM_WORKDIR', 'ARGENTUM_WORKDIR'], process.cwd());
      const preferredHashFile = path.join(workDir, '.argentum-dashboard-pass-hash');
      const legacyHashFile = path.join(workDir, '.agclaw-dashboard-pass-hash');
      const hashFile = fs.existsSync(preferredHashFile) ? preferredHashFile : legacyHashFile;
      let loaded = false;

      try {
        if (fs.existsSync(hashFile)) {
          const savedHash = fs.readFileSync(hashFile, 'utf8').trim();
          if (savedHash) {
            config.auth.passwordHash = savedHash;
            loaded = true;
          }
        }
      } catch {
        // Ignore read errors, will generate a new password
      }

      if (!loaded) {
        // Generate a random password and persist the hash so it survives restarts
        const randomPass = crypto.randomBytes(16).toString('hex');
        config.auth.passwordHash = hashPassword(randomPass);

        try {
          fs.writeFileSync(hashFile, config.auth.passwordHash, { mode: 0o600 });
          try {
            fs.chmodSync(hashFile, 0o600);
          } catch {
            /* chmod may fail on some platforms */
          }
        } catch {
          console.warn('[Dashboard Server] Could not persist password hash to disk.');
        }

        console.warn(`[Dashboard Server] Generated dashboard password written to: ${hashFile}`);
        console.warn(
          '[Dashboard Server] Set ARGENTUM_DASHBOARD_PASS or ARGENTUM_DASHBOARD_PASS_HASH to use your own.',
        );
      }
    } else {
      config.auth.passwordHash = hashPassword(envPass);
    }
  }

  return config;
}

/**
 * Simple password hashing using bcryptjs (production-grade)
 */
function hashPassword(password: string): string {
  return bcrypt.hashSync(password, 10);
}

/**
 * Verify password against hash
 */
function verifyPassword(password: string, hash: string): boolean {
  return bcrypt.compareSync(password, hash);
}

/**
 * Parse Basic Auth header
 */
function parseBasicAuth(authHeader: string): { username: string; password: string } | null {
  if (!authHeader?.startsWith('Basic ')) {
    return null;
  }

  try {
    const base64Credentials = authHeader.slice(6);
    const credentials = Buffer.from(base64Credentials, 'base64').toString('utf8');
    const [username, password] = credentials.split(':');

    if (!username || !password) {
      return null;
    }

    return { username, password };
  } catch {
    return null;
  }
}

/**
 * Check rate limit for IP
 */
function checkRateLimit(ip: string): boolean {
  const now = Date.now();
  const entry = rateLimits.get(ip);

  if (!entry || now > entry.resetAt) {
    rateLimits.set(ip, { count: 1, resetAt: now + config.rateLimit.windowMs });
    return true;
  }

  if (entry.count >= config.rateLimit.maxRequests) {
    return false;
  }

  entry.count++;
  return true;
}

/**
 * Serve static files
 */
function serveStatic(filePath: string, res: http.ServerResponse, mimeType?: string): void {
  fs.readFile(filePath, (err, data) => {
    if (err) {
      if (err.code === 'ENOENT') {
        // Try index.html for SPA routing
        const indexPath = path.join(config.staticDir, 'index.html');
        fs.readFile(indexPath, (err2, indexData) => {
          if (err2) {
            sendError(res, 404, 'Not Found');
          } else {
            sendResponse(res, 200, 'text/html', indexData);
          }
        });
      } else {
        sendError(res, 500, 'Server Error');
      }
      return;
    }

    const ext = path.extname(filePath).toLowerCase();
    const contentType = mimeType ?? getMimeType(ext);
    sendResponse(res, 200, contentType, data);
  });
}

/**
 * Get MIME type from extension
 */
function getMimeType(ext: string): string {
  const mimeTypes: Record<string, string> = {
    '.html': 'text/html',
    '.css': 'text/css',
    '.js': 'application/javascript',
    '.json': 'application/json',
    '.png': 'image/png',
    '.jpg': 'image/jpeg',
    '.gif': 'image/gif',
    '.svg': 'image/svg+xml',
    '.ico': 'image/x-icon',
    '.woff': 'font/woff',
    '.woff2': 'font/woff2',
    '.ttf': 'font/ttf',
    '.eot': 'application/vnd.ms-fontobject',
    '.txt': 'text/plain',
  };
  return mimeTypes[ext] ?? 'application/octet-stream';
}

/**
 * Send HTTP response
 */
function sendResponse(
  res: http.ServerResponse,
  statusCode: number,
  contentType: string,
  data: Buffer | string,
): void {
  res.writeHead(statusCode, {
    'Content-Type': contentType,
    'Cache-Control': 'no-cache',
    'X-Content-Type-Options': 'nosniff',
    'X-Frame-Options': 'DENY',
  });
  res.end(data);
}

/**
 * Send error response
 */
function sendError(res: http.ServerResponse, statusCode: number, message: string): void {
  res.writeHead(statusCode, { 'Content-Type': 'text/plain' });
  res.end(message);
}

/**
 * Send JSON response
 */
function sendJSON(res: http.ServerResponse, statusCode: number, data: unknown): void {
  res.writeHead(statusCode, { 'Content-Type': 'application/json' });
  res.end(JSON.stringify(data));
}

/**
 * Handle incoming HTTP request
 */
function handleRequest(req: http.IncomingMessage, res: http.ServerResponse): void {
  const parsedUrl = url.parse(req.url ?? '/', true);
  const pathname = parsedUrl.pathname ?? '/';
  const ip = req.socket.remoteAddress ?? 'unknown';

  // Check rate limit
  if (!checkRateLimit(ip)) {
    res.writeHead(429, {
      'Content-Type': 'text/plain',
      'Retry-After': '60',
    });
    res.end('Too Many Requests');
    return;
  }

  // CORS preflight
  if (req.method === 'OPTIONS' && config.cors.enabled) {
    const origin = req.headers.origin;
    const allowedOrigin =
      origin && config.cors.allowedOrigins.includes(origin) ? origin : undefined;
    res.writeHead(204, {
      'Access-Control-Allow-Origin': allowedOrigin ?? '',
      'Access-Control-Allow-Methods': 'GET, POST, OPTIONS',
      'Access-Control-Allow-Headers': 'Content-Type, Authorization',
      'Access-Control-Max-Age': '86400',
    });
    res.end();
    return;
  }

  // Apply CORS

  // allowedOrigins is a strict whitelist from config (not user-controlled), so this is safe
  if (config.cors.enabled) {
    const origin = req.headers.origin;
    if (origin && config.cors.allowedOrigins.includes(origin)) {
      res.setHeader('Access-Control-Allow-Origin', origin);
    }
  }

  // WebSocket upgrade
  if (pathname === '/ws') {
    // WebSocket handling is done separately
    return;
  }

  // API endpoints
  if (pathname.startsWith('/api/')) {
    handleAPIRequest(req, res, pathname, parsedUrl);
    return;
  }

  // Health check (no auth required)
  if (pathname === '/health') {
    sendJSON(res, 200, {
      status: 'ok',
      version: '0.0.9',
      uptime: process.uptime(),
      wsClients: wsClients.size,
    });
    return;
  }

  // Auth check for all other requests
  const authHeader = req.headers.authorization;
  const auth = parseBasicAuth(authHeader ?? '');

  if (
    auth?.username !== config.auth.username ||
    !verifyPassword(auth.password, config.auth.passwordHash)
  ) {
    res.writeHead(401, {
      'WWW-Authenticate': 'Basic realm="Argentum Dashboard"',
      'Content-Type': 'text/html',
    });
    res.end(`
      <!DOCTYPE html>
      <html>
        <head><title>401 Unauthorized</title></head>
        <body>
          <h1>401 Unauthorized</h1>
          <p>Invalid credentials. <a href="/">Try again</a>.</p>
        </body>
      </html>
    `);
    return;
  }

  // Serve static files
  let filePath: string;

  if (pathname === '/' || pathname === '/index.html') {
    filePath = path.join(config.staticDir, 'index.html');
  } else {
    // Path traversal is prevented by normalization + startsWith check below
    filePath = path.join(config.staticDir, pathname);
  }

  // Security: prevent directory traversal
  const normalizedPath = path.normalize(filePath);
  if (!normalizedPath.startsWith(config.staticDir)) {
    sendError(res, 403, 'Forbidden');
    return;
  }

  serveStatic(normalizedPath, res);
}

/**
 * Handle API requests
 */
function handleAPIRequest(
  req: http.IncomingMessage,
  res: http.ServerResponse,
  pathname: string,
  parsedUrl: url.UrlWithParsedQuery,
): void {
  // Auth check for API
  const authHeader = req.headers.authorization;
  const auth = parseBasicAuth(authHeader ?? '');

  if (
    auth?.username !== config.auth.username ||
    !verifyPassword(auth.password, config.auth.passwordHash)
  ) {
    sendJSON(res, 401, { error: 'Unauthorized' });
    return;
  }

  // Parse body for POST requests
  const chunks: Buffer[] = [];
  req.on('data', (chunk: Buffer) => chunks.push(chunk));
  req.on('end', () => {
    let body: unknown = {};

    if (chunks.length > 0) {
      try {
        body = JSON.parse(Buffer.concat(chunks).toString());
      } catch {
        // Ignore invalid JSON
      }
    }

    // Route API requests
    if (pathname === '/api/budget') {
      handleBudgetAPI(req, res, body);
    } else if (pathname === '/api/org/chart') {
      handleOrgChartAPI(req, res, body);
    } else if (pathname.startsWith('/api/org/')) {
      handleOrgAPI(req, res, pathname, body);
    } else if (pathname === '/api/self-improving') {
      handleSelfImprovingAPI(req, res, body);
    } else if (pathname === '/api/trajectory') {
      handleTrajectoryAPI(req, res, body);
    } else if (pathname === '/api/skills') {
      handleSkillsAPI(req, res, body);
    } else if (pathname.startsWith('/api/skills/')) {
      handleSkillsAPI(req, res, body);
    } else if (pathname === '/api/system/stats') {
      sendJSON(res, 200, getSystemStats());
    } else {
      sendJSON(res, 404, { error: 'Not found' });
    }
  });
}

/**
 * Get system stats
 */
function getSystemStats() {
  const memUsage = process.memoryUsage();
  return {
    uptime: process.uptime(),
    memoryUsage: `${(memUsage.heapUsed / 1024 / 1024).toFixed(2)} MB`,
    wsClients: wsClients.size,
  };
}

function sendUnavailable(res: http.ServerResponse, capability: string): void {
  sendJSON(res, 501, {
    error: `${capability} is not connected to a live feature module. Enable and configure that module before exposing this endpoint.`,
    available: false,
  });
}

function handleBudgetAPI(req: http.IncomingMessage, res: http.ServerResponse, body: unknown) {
  sendUnavailable(res, 'Budget dashboard');
}

function handleOrgChartAPI(req: http.IncomingMessage, res: http.ServerResponse, body: unknown) {
  sendUnavailable(res, 'Organization dashboard');
}

function handleOrgAPI(
  req: http.IncomingMessage,
  res: http.ServerResponse,
  pathname: string,
  body: unknown,
) {
  sendUnavailable(res, 'Organization actions');
}

function handleSelfImprovingAPI(
  req: http.IncomingMessage,
  res: http.ServerResponse,
  body: unknown,
) {
  sendUnavailable(res, 'Self-improving dashboard');
}

function handleTrajectoryAPI(req: http.IncomingMessage, res: http.ServerResponse, body: unknown) {
  sendUnavailable(res, 'Trajectory dashboard');
}

function handleSkillsAPI(req: http.IncomingMessage, res: http.ServerResponse, body: unknown) {
  sendUnavailable(res, 'Skills dashboard');
}

/**
 * Setup WebSocket server
 */
function setupWebSocket(server: http.Server): void {
  // ws:// is correct here - server is plain HTTP, not HTTPS, so wss:// is not applicable
  const wss = new WebSocketServer({ server, path: '/ws' });

  wss.on('connection', (ws: WebSocket, req) => {
    // Auth check for WebSocket
    const authHeader = req.headers.authorization;
    const auth = parseBasicAuth(authHeader ?? '');

    if (
      auth?.username !== config.auth.username ||
      !verifyPassword(auth.password, config.auth.passwordHash)
    ) {
      ws.close(4001, 'Unauthorized');
      return;
    }

    wsClients.add(ws);
    console.info(`[Dashboard WS] Client connected (${wsClients.size} total)`);

    ws.on('close', () => {
      wsClients.delete(ws);
      console.info(`[Dashboard WS] Client disconnected (${wsClients.size} total)`);
    });

    ws.on('error', (err) => {
      console.error('[Dashboard WS] Error:', err.message);
      wsClients.delete(ws);
    });

    // Send welcome message

    ws.send(JSON.stringify({ type: 'connected', message: 'Argentum Dashboard connected' }));
  });

  // Broadcast to all clients
  globalBroadcast = (data: unknown) => {
    const message = JSON.stringify(data);
    wsClients.forEach((ws) => {
      if (ws.readyState === WebSocket.OPEN) {
        ws.send(message);
      }
    });
  };
}

let globalBroadcast: (data: unknown) => void = () => {};

/**
 * Broadcast event to all WebSocket clients
 */
export function broadcast(type: string, data: unknown): void {
  globalBroadcast({ type, data, timestamp: Date.now() });
}

// Global config
let config: ServerConfig;

/**
 * Start the dashboard server
 */
export async function startDashboardServer(options?: Partial<ServerConfig>): Promise<http.Server> {
  config = loadConfig();

  // Override with provided options
  if (options) {
    config = { ...config, ...options };
  }

  // Ensure static directory exists
  if (!fs.existsSync(config.staticDir)) {
    console.error(`[Dashboard Server] Static directory not found: ${config.staticDir}`);
    console.error('[Dashboard Server] Run: npm run build  (to build the dashboard)');
    throw new Error(`Static directory not found: ${config.staticDir}`);
  }

  return new Promise((resolve, reject) => {
    const server = http.createServer(handleRequest);

    // Setup WebSocket
    setupWebSocket(server);

    // Handle server errors
    server.on('error', (err) => {
      if ((err as NodeJS.ErrnoException).code === 'EADDRINUSE') {
        console.error(`[Dashboard Server] Port ${config.port} is already in use.`);
        console.error(
          '[Dashboard Server] Try a different port: argentum dashboard start --port 3001',
        );
      }
      reject(err);
    });

    // Start listening
    server.listen(config.port, config.host, () => {
      console.info('');
      console.info('  ╔══════════════════════════════════════════════════════════╗');
      console.info('  ║         Argentum Dashboard Server Started                  ║');
      console.info('  ╠══════════════════════════════════════════════════════════╣');
      console.info(`  ║  URL:      http://${config.host}:${config.port}                 ║`);
      console.info(`  ║  Auth:     HTTP Basic Auth (user: ${config.auth.username})            ║`);

      console.info(`  ║  WebSocket: ws://${config.host}:${config.port}/ws     ║`);
      console.info('  ╠══════════════════════════════════════════════════════════╣');
      console.info('  ║  Remote Access:                                        ║');
      console.info(`  ║    SSH:  ssh -L 3000:localhost:${config.port} user@host        ║`);
      console.info('  ║    Then open: http://localhost:3000                    ║');
      console.info('  ╚══════════════════════════════════════════════════════════╝');
      console.info('');
      console.info('[Dashboard Server] Press Ctrl+C to stop');

      resolve(server);
    });
  });
}

/**
 * Stop the dashboard server
 */
export function stopDashboardServer(server: http.Server): void {
  console.info('[Dashboard Server] Shutting down...');

  // Close all WebSocket connections
  wsClients.forEach((ws) => ws.close());
  wsClients.clear();

  // Close HTTP server
  server.close(() => {
    console.info('[Dashboard Server] Stopped');
  });
}

// CLI entry point
if (require.main === module) {
  const args = process.argv.slice(2);
  const command = args[0];

  if (command === 'start') {
    const portIdx = args.indexOf('--port');
    const port = portIdx !== -1 ? parseInt(args[portIdx + 1] ?? '3000', 10) : undefined;

    startDashboardServer(port ? { port } : undefined)
      .then((server) => {
        process.on('SIGINT', () => {
          stopDashboardServer(server);
          process.exit(0);
        });
      })
      .catch((err) => {
        console.error('[Dashboard Server] Failed to start:', err.message);
        process.exit(1);
      });
  } else if (command === 'help') {
    console.info('Argentum Dashboard Server');
    console.info('');
    console.info('Usage:');
    console.info('  argentum-dashboard start [--port PORT]  Start the dashboard server');
    console.info('  argentum-dashboard help                 Show this help');
  } else {
    console.info('Unknown command. Use "start" or "help".');
  }
}
