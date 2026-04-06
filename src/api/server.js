/**
 * AG-Claw Dashboard - Simple API Server
 *
 * This is a minimal Express server that provides API endpoints
 * for the dashboard. In production, these would be integrated
 * with the main AG-Claw application.
 */

const http = require('http');
const url = require('url');
const fs = require('fs');
const path = require('path');

// Configuration
const PORT = process.env.DASHBOARD_PORT || 3002;
const STATIC_DIR = path.resolve(path.join(__dirname, '../ui/dashboard'));
const ALLOWED_ORIGINS = (process.env.AGCLAW_CORS_ORIGINS || 'http://localhost:3002,http://127.0.0.1:3002').split(',').map(o => o.trim());
const MAX_BODY_SIZE = 1024 * 1024; // 1MB max request body

// Lazily resolved real path for static dir (avoids crash if assets are missing at startup)
let _staticDirReal = null;
let _staticDirResolutionFailed = false;

function getStaticDirReal() {
  if (_staticDirReal) return _staticDirReal;
  if (_staticDirResolutionFailed) return null;
  try {
    _staticDirReal = fs.realpathSync(STATIC_DIR);
    return _staticDirReal;
  } catch {
    _staticDirResolutionFailed = true;
    console.warn(
      `[Dashboard] Static assets directory unavailable: ${STATIC_DIR}. ` +
      'Dashboard file serving will be disabled until assets are present.'
    );
    return null;
  }
}

function isPathUnderStaticDir(p) {
  const staticDirReal = getStaticDirReal();
  if (!p || !staticDirReal) return false;
  const normalized = path.resolve(p);
  const allowedPrefix = staticDirReal.endsWith(path.sep) ? staticDirReal : staticDirReal + path.sep;
  return normalized === staticDirReal || normalized.startsWith(allowedPrefix);
}

// MIME types
const mimeTypes = {
  '.html': 'text/html',
  '.css': 'text/css',
  '.js': 'application/javascript',
  '.json': 'application/json',
  '.png': 'image/png',
  '.jpg': 'image/jpeg',
  '.svg': 'image/svg+xml',
  '.ico': 'image/x-icon',
};

// Mock data generators
const mockData = {
  agents: [
    {
      id: 'coder',
      name: 'Coder',
      status: 'online',
      tasksCompleted: 1247,
      successRate: 99.2,
      memory: '890MB',
      uptime: '99.9%',
    },
    {
      id: 'researcher',
      name: 'Researcher',
      status: 'online',
      tasksCompleted: 843,
      successRate: 98.7,
      memory: '456MB',
      uptime: '99.5%',
    },
    {
      id: 'foreman',
      name: 'Foreman',
      status: 'online',
      tasksCompleted: 432,
      successRate: 99.8,
      memory: '234MB',
      uptime: '99.9%',
    },
    {
      id: 'writer',
      name: 'Writer',
      status: 'idle',
      tasksCompleted: 156,
      successRate: 97.4,
      memory: '178MB',
      uptime: '98.2%',
    },
  ],
  skills: [
    { id: 'github', name: 'GitHub Integration', installed: true, rating: 4.8 },
    { id: 'deep-research-pro', name: 'Deep Research Pro', installed: true, rating: 4.9 },
    { id: 'telegram', name: 'Telegram Bot', installed: true, rating: 4.7 },
    { id: 'weather', name: 'Weather Forecast', installed: false, rating: 4.5 },
  ],
  logs: [],
};

// Generate initial logs
const logLevels = ['DEBUG', 'INFO', 'WARN', 'ERROR'];
const logSources = ['core', 'agent:coder', 'agent:researcher', 'skills:github'];
const logMessages = [
  'Processing request',
  'Task completed',
  'Cache hit',
  'Skill executed',
  'Heartbeat sent',
  'Connection established',
  'Configuration loaded',
];

for (let i = 0; i < 100; i++) {
  mockData.logs.push({
    timestamp: new Date(Date.now() - i * 5000).toISOString(),
    level: logLevels[Math.floor(Math.random() * logLevels.length)],
    source: logSources[Math.floor(Math.random() * logSources.length)],
    message: logMessages[Math.floor(Math.random() * logMessages.length)],
  });
}

// Parse JSON body with size limit
function parseBody(req) {
  return new Promise((resolve, reject) => {
    let body = '';
    let size = 0;
    req.on('data', (chunk) => {
      size += chunk.length;
      if (size > MAX_BODY_SIZE) {
        req.destroy();
        reject(Object.assign(new Error('Request body too large'), { statusCode: 413 }));
        return;
      }
      body += chunk;
    });
    req.on('end', () => {
      try {
        resolve(body ? JSON.parse(body) : {});
      } catch (e) {
        reject(new Error('Invalid JSON'));
      }
    });
    req.on('error', reject);
  });
}

// Get allowed origin for CORS
function getCorsOrigin(reqOrigin) {
  if (ALLOWED_ORIGINS.includes(reqOrigin)) {
    return reqOrigin;
  }
  return '';
}

// Security headers applied to all responses
function setSecurityHeaders(res, reqOrigin) {
  const origin = getCorsOrigin(reqOrigin || '');
  if (origin) {
    res.setHeader('Access-Control-Allow-Origin', origin);
    res.setHeader('Access-Control-Allow-Methods', 'GET, POST, PUT, DELETE, OPTIONS');
    res.setHeader('Access-Control-Allow-Headers', 'Content-Type, Authorization');
  }
  res.setHeader('X-Content-Type-Options', 'nosniff');
  res.setHeader('X-Frame-Options', 'DENY');
  res.setHeader('X-XSS-Protection', '1; mode=block');
  res.setHeader('Referrer-Policy', 'strict-origin-when-cross-origin');
}

// Send JSON response
function sendJson(res, statusCode, data, req) {
  setSecurityHeaders(res, req ? req.headers.origin : undefined);
  res.writeHead(statusCode, {
    'Content-Type': 'application/json',
  });
  res.end(JSON.stringify(data));
}

// Send error response
// Note: 501 (Not Implemented) and 503 (Service Unavailable) pass through because
// they contain actionable client-facing information, unlike 500/502 which may leak internals.
function sendError(res, statusCode, message, req) {
  const safeMessage = statusCode >= 500 && statusCode !== 501 && statusCode !== 503
    ? 'Internal server error'
    : message;
  sendJson(res, statusCode, { error: safeMessage }, req);
}

// Route handlers
const routes = {
  // Health check
  'GET /api/health': (req, res) => {
    sendJson(res, 200, {
      status: 'healthy',
      uptime: process.uptime(),
      timestamp: new Date().toISOString(),
    }, req);
  },

  // System stats
  'GET /api/stats': (req, res) => {
    const cpuUsage = Math.random() * 40 + 10;
    const memoryUsage = Math.random() * 2 + 0.5;

    sendJson(res, 200, {
      uptime: '99.9%',
      activeAgents: 4,
      cpuUsage: cpuUsage.toFixed(1) + '%',
      memoryUsage: memoryUsage.toFixed(1) + ' GB',
      requestsPerMinute: Math.floor(Math.random() * 500) + 100,
    }, req);
  },

  // Get all agents
  'GET /api/agents': (req, res) => {
    sendJson(res, 200, mockData.agents, req);
  },

  // Get single agent
  'GET /api/agents/:id': (req, res) => {
    const agent = mockData.agents.find((a) => a.id === req.params.id);
    if (agent) {
      sendJson(res, 200, agent, req);
    } else {
      sendError(res, 404, 'Agent not found', req);
    }
  },

  // Start agent
  'POST /api/agents/:id/start': (req, res) => {
    const agent = mockData.agents.find((a) => a.id === req.params.id);
    if (agent) {
      agent.status = 'online';
      sendJson(res, 200, { success: true, agent }, req);
    } else {
      sendError(res, 404, 'Agent not found', req);
    }
  },

  // Stop agent
  'POST /api/agents/:id/stop': (req, res) => {
    const agent = mockData.agents.find((a) => a.id === req.params.id);
    if (agent) {
      agent.status = 'offline';
      sendJson(res, 200, { success: true, agent }, req);
    } else {
      sendError(res, 404, 'Agent not found', req);
    }
  },

  // Get all skills
  'GET /api/skills': (req, res) => {
    sendJson(res, 200, mockData.skills, req);
  },

  // Get logs
  'GET /api/logs': (req, res) => {
    const { level, source, limit = 100 } = req.query;
    let logs = [...mockData.logs];

    if (level && level !== 'all') {
      logs = logs.filter((l) => l.level.toLowerCase() === level.toLowerCase());
    }
    if (source) {
      logs = logs.filter((l) => l.source === source);
    }

    logs = logs.slice(-parseInt(limit));

    sendJson(res, 200, {
      logs,
      total: mockData.logs.length,
    }, req);
  },

  // Add log entry
  'POST /api/logs': async (req, res) => {
    const body = await parseBody(req);
    const logEntry = {
      timestamp: new Date().toISOString(),
      level: body.level || 'INFO',
      source: body.source || 'api',
      message: body.message || '',
    };
    mockData.logs.push(logEntry);
    sendJson(res, 201, logEntry, req);
  },

  // Get memories
  'GET /api/memory': (req, res) => {
    const memories = [
      {
        id: 'mem_001',
        type: 'semantic',
        title: 'User Preferences',
        content: 'Language: English, Timezone: Europe/Tallinn',
      },
      {
        id: 'mem_002',
        type: 'episodic',
        title: 'Session Started',
        content: 'Dashboard opened at 10:00',
      },
      {
        id: 'mem_003',
        type: 'procedural',
        title: 'Backup Complete',
        content: '847 files backed up',
      },
    ];
    sendJson(res, 200, { memories, total: memories.length }, req);
  },

  // Get settings
  'GET /api/settings': (req, res) => {
    sendJson(res, 200, {
      theme: 'dark',
      language: 'en',
      autoUpdate: true,
      llmProvider: 'minimax',
      llmModel: 'MiniMax-M2.7',
    }, req);
  },

  // Update settings
  'PUT /api/settings': async (req, res) => {
    const body = await parseBody(req);
    sendJson(res, 200, { success: true, settings: body }, req);
  },
};

// Find matching route
function matchRoute(method, reqPath) {
  // Exact match
  const key = `${method} ${reqPath}`;
  if (routes[key]) return { handler: routes[key], params: {} };

  // Parameter match
  for (const route of Object.keys(routes)) {
    const [routeMethod, routePath] = route.split(' ');
    if (routeMethod !== method) continue;

    const routeParts = routePath.split('/');
    const pathParts = reqPath.split('/');

    if (routeParts.length !== pathParts.length) continue;

    const params = {};
    let match = true;

    for (let i = 0; i < routeParts.length; i++) {
      if (routeParts[i].startsWith(':')) {
        params[routeParts[i].slice(1)] = pathParts[i];
      } else if (routeParts[i] !== pathParts[i]) {
        match = false;
        break;
      }
    }

    if (match) {
      return { handler: routes[route], params };
    }
  }

  return null;
}

// Create HTTP server
const server = http.createServer(async (req, res) => {
  const parsedUrl = url.parse(req.url, true);
  const pathname = parsedUrl.pathname;
  const method = req.method;

  // Apply security headers to every response
  setSecurityHeaders(res, req.headers.origin);

  // CORS preflight
  if (method === 'OPTIONS') {
    res.writeHead(200);
    res.end();
    return;
  }

  // API routes
  if (pathname.startsWith('/api/')) {
    const match = matchRoute(method, pathname);
    if (match) {
      try {
        // Attach query and params to the real req object so handlers
        // can use both Node stream methods (parseBody) and route metadata.
        req.query = parsedUrl.query;
        req.params = match.params;
        await match.handler(req, res);
      } catch (error) {
        const statusCode = error?.statusCode || 500;
        const message = statusCode === 413 ? 'Payload too large' : 'Internal server error';
        sendError(res, statusCode, message, req);
      }
    } else {
      sendError(res, 404, 'API endpoint not found', req);
    }
    return;
  }

  // WebSocket upgrade
  if (pathname === '/ws') {
    res.writeHead(426, { 'Content-Type': 'text/plain' });
    res.end('WebSocket upgrade required');
    return;
  }

  // Static files
  const staticDirReal = getStaticDirReal();
  if (!staticDirReal) {
    res.writeHead(503);
    res.end('Dashboard assets not available');
    return;
  }

  // Decode percent-encoded sequences before checking for traversal,
  // so encoded variants like %2e%2e are caught too.
  let requestedPath;
  try {
    requestedPath = decodeURIComponent(pathname === '/' ? 'index.html' : pathname.replace(/^\/+/, ''));
  } catch {
    res.writeHead(400);
    res.end('Bad Request');
    return;
  }

  // Security: reject directory traversal sequences in user input.
  // This is an explicit check that CodeQL recognizes as a sanitizer
  // for js/path-injection, preventing the user from escaping the
  // static directory via ".." segments.
  if (requestedPath.indexOf('..') !== -1) {
    res.writeHead(403);
    res.end('Forbidden');
    return;
  }

  const safePath = path.resolve(staticDirReal, requestedPath);

  // Security: ensure the resolved path is under the static directory
  if (!isPathUnderStaticDir(safePath)) {
    res.writeHead(403);
    res.end('Forbidden');
    return;
  }

  let realPath;
  try {
    realPath = fs.realpathSync(safePath);
  } catch {
    // File doesn't exist yet - fall through to readFile handler for SPA fallback
  }

  // Security: prevent symlink-based traversal
  if (realPath && !isPathUnderStaticDir(realPath)) {
    res.writeHead(403);
    res.end('Forbidden');
    return;
  }

  // Security: only serve files with allowed extensions
  const ext = path.extname(safePath);
  if (ext && !mimeTypes[ext]) {
    res.writeHead(403);
    res.end('Forbidden');
    return;
  }
  const contentType = mimeTypes[ext] || 'text/html';

  // Use realPath (symlink-resolved) if available, otherwise safePath (for SPA fallback)
  const fileToRead = realPath || safePath;

  fs.readFile(fileToRead, (err, content) => {
    if (err) {
      if (err.code === 'ENOENT') {
        // Serve index.html for SPA routing (constant path, not user-controlled)
        const indexPath = path.join(staticDirReal, 'index.html');
        fs.readFile(indexPath, (err, content) => {
          if (err) {
            res.writeHead(404);
            res.end('Not Found');
          } else {
            res.writeHead(200, { 'Content-Type': 'text/html' });
            res.end(content);
          }
        });
      } else {
        res.writeHead(500);
        res.end('Server Error');
      }
    } else {
      res.writeHead(200, { 'Content-Type': contentType });
      res.end(content);
    }
  });
});

// Start server
server.listen(PORT, () => {
  console.log(`
╔═══════════════════════════════════════════════════════════╗
║                                                           ║
║   AG-Claw Dashboard Server                                ║
║                                                           ║
║   Local:    http://localhost:${PORT}                        ║
║   API:      http://localhost:${PORT}/api                   ║
║   WebSocket: ws://localhost:${PORT}/ws                     ║
║                                                           ║
║   Press Ctrl+C to stop                                    ║
║                                                           ║
╚═══════════════════════════════════════════════════════════╝
  `);
});

// Graceful shutdown
process.on('SIGINT', () => {
  console.log('\nShutting down...');
  server.close(() => {
    console.log('Server stopped');
    process.exit(0);
  });
});
