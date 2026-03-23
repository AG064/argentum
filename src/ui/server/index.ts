/**
 * AG-Claw Dashboard Server
 * 
 * A lightweight secure web server for the AG-Claw dashboard.
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

import { WebSocketServer, WebSocket } from 'ws';
import bcrypt from 'bcryptjs';

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

// Default configuration
const DEFAULT_CONFIG: ServerConfig = {
  port: parseInt(process.env.AGCLAW_DASHBOARD_PORT || '3000', 10),
  host: process.env.AGCLAW_DASHBOARD_HOST || '127.0.0.1',
  staticDir: path.join(__dirname, '..', 'dashboard'),
  auth: {
    username: process.env.AGCLAW_DASHBOARD_USER || 'admin',
    passwordHash: process.env.AGCLAW_DASHBOARD_PASS_HASH || '', // Will be generated if empty
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
 * Load configuration from agclaw.json
 */
function loadConfig(): ServerConfig {
  const config = { ...DEFAULT_CONFIG };
  
  // Try to load from agclaw.json
  const workDir = process.env.AGCLAW_WORKDIR || process.cwd();
  const configPath = path.join(workDir, 'agclaw.json');
  
  if (fs.existsSync(configPath)) {
    try {
      const agclawConfig = JSON.parse(fs.readFileSync(configPath, 'utf8'));
      
      if (agclawConfig.dashboard) {
        config.port = agclawConfig.dashboard.port || config.port;
        config.host = agclawConfig.dashboard.host || config.host;
        
        if (agclawConfig.dashboard.auth) {
          config.auth.username = agclawConfig.dashboard.auth.username || config.auth.username;
          if (agclawConfig.dashboard.auth.password) {
            config.auth.passwordHash = hashPassword(agclawConfig.dashboard.auth.password);
          }
        }
        
        if (agclawConfig.dashboard.rateLimit) {
          config.rateLimit.windowMs = agclawConfig.dashboard.rateLimit.windowMs || config.rateLimit.windowMs;
          config.rateLimit.maxRequests = agclawConfig.dashboard.rateLimit.maxRequests || config.rateLimit.maxRequests;
        }
        
        if (agclawConfig.dashboard.cors) {
          config.cors.allowedOrigins = agclawConfig.dashboard.cors.allowedOrigins || config.cors.allowedOrigins;
        }
      }
    } catch (err) {
      console.warn('[Dashboard Server] Failed to parse agclaw.json:', (err as Error).message);
    }
  }
  
  // Generate password hash if not set
  if (!config.auth.passwordHash) {
    const defaultPassword = process.env.AGCLAW_DASHBOARD_PASS || 'admin';
    config.auth.passwordHash = hashPassword(defaultPassword);
    console.warn('[Dashboard Server] Using default password. Set AGCLAW_DASHBOARD_PASS to change.');
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
  if (!authHeader || !authHeader.startsWith('Basic ')) {
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
    const contentType = mimeType || getMimeType(ext);
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
  return mimeTypes[ext] || 'application/octet-stream';
}

/**
 * Send HTTP response
 */
function sendResponse(res: http.ServerResponse, statusCode: number, contentType: string, data: Buffer | string): void {
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
  const parsedUrl = url.parse(req.url || '/', true);
  const pathname = parsedUrl.pathname || '/';
  const ip = req.socket.remoteAddress || 'unknown';
  
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
    res.writeHead(204, {
      'Access-Control-Allow-Origin': '*',
      'Access-Control-Allow-Methods': 'GET, POST, OPTIONS',
      'Access-Control-Allow-Headers': 'Content-Type, Authorization',
      'Access-Control-Max-Age': '86400',
    });
    res.end();
    return;
  }
  
  // Apply CORS
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
      version: '0.2.0',
      uptime: process.uptime(),
      wsClients: wsClients.size,
    });
    return;
  }
  
  // Auth check for all other requests
  const authHeader = req.headers.authorization;
  const auth = parseBasicAuth(authHeader || '');
  
  if (!auth || auth.username !== config.auth.username || !verifyPassword(auth.password, config.auth.passwordHash)) {
    res.writeHead(401, {
      'WWW-Authenticate': 'Basic realm="AG-Claw Dashboard"',
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
  parsedUrl: url.UrlWithParsedQuery
): void {
  // Auth check for API
  const authHeader = req.headers.authorization;
  const auth = parseBasicAuth(authHeader || '');
  
  if (!auth || auth.username !== config.auth.username || !verifyPassword(auth.password, config.auth.passwordHash)) {
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
    memoryUsage: `${(memUsage.heapUsed / 1024 / 1024).toFixed(2)  } MB`,
    cpuUsage: 0, // Would need os module for accurate reading
    activeAgents: 4,
    wsClients: wsClients.size,
  };
}

// API Handlers (placeholder implementations)
function handleBudgetAPI(req: http.IncomingMessage, res: http.ServerResponse, body: unknown) {
  // In production, this would call the actual budget feature
  sendJSON(res, 200, {
    monthlyCost: 3.47,
    monthlyLimit: 10.0,
    dailyCost: 0.23,
    dailyLimit: 1.0,
    perAgentLimit: 2.0,
    alertThreshold: 0.8,
    blockOnExhausted: true,
    alerts: ['Monthly spending at 34.7% of limit'],
    byAgent: [
      { agent: 'coder', totalCost: 1.82, totalTokens: 124500, requestCount: 47 },
      { agent: 'researcher', totalCost: 0.94, totalTokens: 67800, requestCount: 23 },
    ],
  });
}

function handleOrgChartAPI(req: http.IncomingMessage, res: http.ServerResponse, body: unknown) {
  sendJSON(res, 200, {
    tree: {
      name: 'CEO',
      status: 'active',
      agentType: 'CTO',
      children: [
        { name: 'Alice', role: 'Engineer', status: 'active', agentType: 'coder', children: [] },
        { name: 'Bob', role: 'Researcher', status: 'active', agentType: 'researcher', children: [] },
      ],
    },
    stats: {
      totalAgents: 2,
      activeAgents: 2,
      pausedAgents: 0,
      totalBudget: 10000000,
      totalSpent: 3470000,
    },
    nodes: [
      { id: 'agent-001', name: 'Alice', role: 'Engineer', status: 'active', agentType: 'coder', tasks: [] },
      { id: 'agent-002', name: 'Bob', role: 'Researcher', status: 'active', agentType: 'researcher', tasks: [] },
    ],
  });
}

function handleOrgAPI(req: http.IncomingMessage, res: http.ServerResponse, pathname: string, body: unknown) {
  const action = pathname.replace('/api/org/', '');
  
  switch (action) {
    case 'hire':
      sendJSON(res, 200, { success: true, message: 'Agent hired (demo)' });
      break;
    case 'fire':
      sendJSON(res, 200, { success: true, message: 'Agent terminated (demo)' });
      break;
    case 'pause':
      sendJSON(res, 200, { success: true, message: 'Agent paused (demo)' });
      break;
    case 'resume':
      sendJSON(res, 200, { success: true, message: 'Agent resumed (demo)' });
      break;
    case 'assign':
      sendJSON(res, 200, { success: true, message: 'Task assigned (demo)' });
      break;
    default:
      sendJSON(res, 404, { error: 'Not found' });
  }
}

function handleSelfImprovingAPI(req: http.IncomingMessage, res: http.ServerResponse, body: unknown) {
  sendJSON(res, 200, {
    enabled: true,
    lastRunTime: Date.now() - 3 * 60 * 60 * 1000,
    nextScheduledRun: Date.now() + 21 * 60 * 60 * 1000,
    skillsCreated: 2,
    lessonsLearned: 12,
    config: {
      schedule: 'nightly',
      idleThreshold: 120,
      skillCreationThreshold: 5,
      maxSkillsPerRun: 3,
    },
    phases: {},
    runHistory: [],
    lessons: [],
  });
}

function handleTrajectoryAPI(req: http.IncomingMessage, res: http.ServerResponse, body: unknown) {
  sendJSON(res, 200, {
    sessions: [
      { id: 'session-abc', title: 'Main Session', messageCount: 847, tokens: 1245000, cost: 2.47 },
    ],
    stats: {
      totalSessions: 47,
      totalMessages: 28456,
      totalTokens: 45230000,
      totalCost: 89.34,
      byAgent: {},
    },
    exportHistory: [],
  });
}

function handleSkillsAPI(req: http.IncomingMessage, res: http.ServerResponse, body: unknown) {
  sendJSON(res, 200, {
    installed: [
      { name: 'clawhub', version: '1.2.0', category: 'utility', description: 'Install and manage skills.' },
      { name: 'weather', version: '1.0.0', category: 'utility', description: 'Get weather forecasts.' },
    ],
    marketplace: [
      { name: 'slack-bot', slug: 'slack-bot', version: '1.0.0', category: 'integration', author: 'community', description: 'Send messages to Slack.', stars: 42 },
    ],
  });
}

/**
 * Setup WebSocket server
 */
function setupWebSocket(server: http.Server): void {
  const wss = new WebSocketServer({ server, path: '/ws' });
  
  wss.on('connection', (ws: WebSocket, req) => {
    // Auth check for WebSocket
    const authHeader = req.headers.authorization;
    const auth = parseBasicAuth(authHeader || '');
    
    if (!auth || auth.username !== config.auth.username || !verifyPassword(auth.password, config.auth.passwordHash)) {
      ws.close(4001, 'Unauthorized');
      return;
    }
    
    wsClients.add(ws);
    console.log(`[Dashboard WS] Client connected (${wsClients.size} total)`);
    
    ws.on('close', () => {
      wsClients.delete(ws);
      console.log(`[Dashboard WS] Client disconnected (${wsClients.size} total)`);
    });
    
    ws.on('error', (err) => {
      console.error('[Dashboard WS] Error:', err.message);
      wsClients.delete(ws);
    });
    
    // Send welcome message
    ws.send(JSON.stringify({ type: 'connected', message: 'AG-Claw Dashboard connected' }));
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
        console.error('[Dashboard Server] Try a different port: agclaw dashboard start --port 3001');
      }
      reject(err);
    });
    
    // Start listening
    server.listen(config.port, config.host, () => {
      console.log('');
      console.log('  ╔══════════════════════════════════════════════════════════╗');
      console.log('  ║         AG-Claw Dashboard Server Started                  ║');
      console.log('  ╠══════════════════════════════════════════════════════════╣');
      console.log(`  ║  URL:      http://${config.host}:${config.port}                 ║`);
      console.log(`  ║  Auth:     HTTP Basic Auth (user: ${  config.auth.username  })            ║`);
      console.log(`  ║  WebSocket: ws://${  config.host  }:${  config.port  }/ws               ║`);
      console.log('  ╠══════════════════════════════════════════════════════════╣');
      console.log('  ║  Remote Access:                                        ║');
      console.log(`  ║    SSH:  ssh -L 3000:localhost:${  config.port  } user@host        ║`);
      console.log('  ║    Then open: http://localhost:3000                    ║');
      console.log('  ╚══════════════════════════════════════════════════════════╝');
      console.log('');
      console.log('[Dashboard Server] Press Ctrl+C to stop');
      
      resolve(server);
    });
  });
}

/**
 * Stop the dashboard server
 */
export function stopDashboardServer(server: http.Server): void {
  console.log('[Dashboard Server] Shutting down...');
  
  // Close all WebSocket connections
  wsClients.forEach((ws) => ws.close());
  wsClients.clear();
  
  // Close HTTP server
  server.close(() => {
    console.log('[Dashboard Server] Stopped');
  });
}

// CLI entry point
if (require.main === module) {
  const args = process.argv.slice(2);
  const command = args[0];
  
  if (command === 'start') {
    const portIdx = args.indexOf('--port');
    const port = portIdx !== -1 ? parseInt(args[portIdx + 1] || '3000', 10) : undefined;
    
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
    console.log('AG-Claw Dashboard Server');
    console.log('');
    console.log('Usage:');
    console.log('  agclaw-dashboard start [--port PORT]  Start the dashboard server');
    console.log('  agclaw-dashboard help                 Show this help');
  } else {
    console.log('Unknown command. Use "start" or "help".');
  }
}
