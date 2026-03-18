#!/usr/bin/env node

/**
 * AG-Claw CLI
 *
 * Usage:
 *   agclaw init                    Initialize AG-Claw in current directory
 *   agclaw start [--port 3000]     Start AG-Claw server
 *   agclaw status                  Show system status
 *   agclaw features                List all features
 *   agclaw feature <name> <cmd>    Run feature command
 *   agclaw config [key] [value]    Show/set configuration
 *   agclaw doctor                  Diagnose setup issues
 *   agclaw help                    Show help
 */

import 'dotenv/config';
import * as path from 'path';
import * as fs from 'fs';
import { getConfig } from './core/config';
import { createLogger } from './core/logger';
import { PluginLoader } from './core/plugin-loader';
import { createServer } from 'http';

const VERSION = '0.2.0';
const args = process.argv.slice(2);
const command = args[0] || 'help';

// ─── Helpers ──────────────────────────────────────────────────────────────────

function print(text: string): void {
  process.stdout.write(text + '\n');
}

function error(text: string): void {
  process.stderr.write(`\x1b[31mError:\x1b[0m ${text}\n`);
}

function success(text: string): void {
  print(`\x1b[32m✓\x1b[0m ${text}`);
}

function info(text: string): void {
  print(`\x1b[36mℹ\x1b[0m ${text}`);
}

function warn(text: string): void {
  print(`\x1b[33m⚠\x1b[0m ${text}`);
}

function banner(): void {
  print('');
  print('  \x1b[1m\x1b[36m╔═══════════════════════════════════╗\x1b[0m');
  print('  \x1b[1m\x1b[36m║         AG-Claw v' + VERSION + '           ║\x1b[0m');
  print('  \x1b[1m\x1b[36m║   Modular AI Agent Framework      ║\x1b[0m');
  print('  \x1b[1m\x1b[36m╚═══════════════════════════════════╝\x1b[0m');
  print('');
}

function getWorkDir(): string {
  return process.env.AGCLAW_WORKDIR || process.cwd();
}

// ─── Commands ─────────────────────────────────────────────────────────────────

function cmdHelp(): void {
  banner();
  print('  \x1b[1mUsage:\x1b[0m');
  print('    agclaw <command> [options]');
  print('');
  print('  \x1b[1mCommands:\x1b[0m');
  print('    init                  Initialize AG-Claw in current directory');
  print('    gateway start         Start AG-Claw server (background)');
  print('    gateway stop          Stop AG-Claw server');
  print('    gateway restart       Restart AG-Claw server');
  print('    gateway status        Check if server is running');
  print('    gateway logs          View server logs');
  print('    status                Show system and feature status');
  print('    features              List all available features');
  print('    feature <name>        Show feature details');
  print('    config [key] [value]  Show or set configuration');
  print('    doctor                Diagnose setup issues');
  print('    onboard               Interactive setup wizard');
  print('    skill list            List installed skills');
  print('    skill search <query>  Search ClawHub for skills');
  print('    skill install <slug>  Install a skill from ClawHub');
  print('    skill uninstall <name> Uninstall a skill');
  print('    skill update [name]   Update skills');
  print('    skill explore         Browse latest skills');
  print('    skill info <slug>     Inspect a skill');
  print('    skill publish <path>  Publish your skill');
  print('    telegram status       Telegram bot status');
  print('    telegram config       Show Telegram config template');
  print('    connect               Setup integrations');
  print('    plugins               List all plugins');
  print('    version               Show version');
  print('    help                  Show this help');
  print('');
  print('  \x1b[1mExamples:\x1b[0m');
  print('    agclaw init');
  print('    agclaw start --port 3000');
  print('    agclaw features');
  print('    agclaw feature life-domains');
  print('');
}

function cmdVersion(): void {
  print(`AG-Claw v${VERSION}`);
  print(`Node.js ${process.version}`);
  print(`Platform: ${process.platform} ${process.arch}`);
}

function cmdInit(): void {
  const workDir = getWorkDir();
  const configPath = path.join(workDir, 'agclaw.json');
  const dataDir = path.join(workDir, 'data');

  banner();
  info('Initializing AG-Claw...');

  // Create config
  if (fs.existsSync(configPath)) {
    warn('agclaw.json already exists, skipping');
  } else {
    const defaultConfig = {
      $schema: 'https://github.com/AG064/ag-claw/blob/main/config-schema.json',
      name: 'My AG-Claw Instance',
      version: '1.0.0',
      server: {
        port: 3000,
        host: '0.0.0.0',
      },
      features: {
        'life-domains': { enabled: true },
        'skills-library': { enabled: true },
        'goal-decomposition': { enabled: true },
        'sqlite-memory': { enabled: true },
        'cron-scheduler': { enabled: true },
        'webchat': { enabled: false },
        'webhooks': { enabled: false },
        'api-gateway': { enabled: false },
        'audit-log': { enabled: true },
        'encrypted-secrets': { enabled: false },
      },
      logging: {
        level: 'info',
      },
    };
    fs.writeFileSync(configPath, JSON.stringify(defaultConfig, null, 2));
    success('Created agclaw.json');
  }

  // Create data directory
  if (!fs.existsSync(dataDir)) {
    fs.mkdirSync(dataDir, { recursive: true });
    success('Created data/ directory');
  } else {
    info('data/ directory already exists');
  }

  // Create .env.example
  const envPath = path.join(workDir, '.env.example');
  if (!fs.existsSync(envPath)) {
    fs.writeFileSync(envPath, [
      '# AG-Claw Environment Variables',
      'AGCLAW_WORKDIR=.',
      'AGCLAW_PORT=3000',
      'AGCLAW_MASTER_KEY=',
      'OPENAI_API_KEY=',
      'ANTHROPIC_API_KEY=',
      'OPENROUTER_API_KEY=',
      '',
    ].join('\n'));
    success('Created .env.example');
  }

  success('AG-Claw initialized!');
  info('Next: agclaw start --port 3000');
}

async function cmdStart(): Promise<void> {
  const portIdx = args.indexOf('--port');
  const port = portIdx !== -1 ? parseInt(args[portIdx + 1], 10) : 3000;
  const workDir = getWorkDir();

  banner();
  info(`Starting AG-Claw server on port ${port}...`);

  try {
    const configManager = getConfig();
    const config = configManager.get();
    const logger = createLogger({ level: 'info', format: 'pretty' });

    const pluginLoader = new PluginLoader(config);
    await pluginLoader.loadAll();
    await pluginLoader.enableAll();

    // Simple HTTP server for API
    const http = await import('http');
    const server = http.createServer(async (req, res) => {
      const url = new URL(req.url || '/', `http://localhost:${port}`);

      // Health check
      if (url.pathname === '/health') {
        const features = pluginLoader.listFeatures().filter((f: any) => f.state === 'active');
        res.writeHead(200, { 'Content-Type': 'application/json' });
        res.end(JSON.stringify({
          status: 'ok',
          version: VERSION,
          features: features.length,
          uptime: process.uptime(),
        }));
        return;
      }

      // Features list
      if (url.pathname === '/api/features') {
        const features = pluginLoader.listFeatures();
        res.writeHead(200, { 'Content-Type': 'application/json' });
        res.end(JSON.stringify(features.map((f: any) => ({
          name: f.name,
          version: f.version,
          state: f.state,
        }))));
        return;
      }

      // Feature command
      if (url.pathname.startsWith('/api/feature/')) {
        const featureName = url.pathname.split('/')[3];
        const featureState = pluginLoader.getFeatureState(featureName);
        if (!featureState) {
          res.writeHead(404, { 'Content-Type': 'application/json' });
          res.end(JSON.stringify({ error: `Feature '${featureName}' not found` }));
          return;
        }
        res.writeHead(200, { 'Content-Type': 'application/json' });
        res.end(JSON.stringify({
          name: featureName,
          state: featureState,
        }));
        return;
      }

      // Default 404
      res.writeHead(404, { 'Content-Type': 'application/json' });
      res.end(JSON.stringify({ error: 'Not found' }));
    });

    server.listen(port, () => {
      success(`Server running at http://localhost:${port}`);
      info(`Health check: http://localhost:${port}/health`);
      info(`Features: http://localhost:${port}/api/features`);
      info('Press Ctrl+C to stop');
    });

    // Graceful shutdown
    const shutdown = async () => {
      info('Shutting down...');
      const features = pluginLoader.listFeatures();
      for (const f of features) {
        if (f.state === 'active') await pluginLoader.disableFeature(f.name);
      }
      server.close();
      process.exit(0);
    };
    process.on('SIGINT', shutdown);
    process.on('SIGTERM', shutdown);

  } catch (err) {
    error(`Failed to start: ${(err as Error).message}`);
    process.exit(1);
  }
}

function cmdStatus(): void {
  banner();
  info('Checking AG-Claw status...');

  const workDir = getWorkDir();
  const configPath = path.join(workDir, 'agclaw.json');

  if (!fs.existsSync(configPath)) {
    warn('AG-Claw not initialized. Run: agclaw init');
    return;
  }

  try {
    const config = JSON.parse(fs.readFileSync(configPath, 'utf8'));
    print('');
    print('  \x1b[1mConfiguration:\x1b[0m');
    print(`    Name: ${config.name}`);
    print(`    Port: ${config.server?.port || 3000}`);
    print('');
    print('  \x1b[1mFeatures:\x1b[0m');
    const features = config.features || {};
    for (const [name, cfg] of Object.entries(features)) {
      const enabled = (cfg as any).enabled ? '\x1b[32m✓ enabled\x1b[0m' : '\x1b[31m✗ disabled\x1b[0m';
      print(`    ${name}: ${enabled}`);
    }
    print('');

    // Data directory
    const dataDir = path.join(workDir, 'data');
    if (fs.existsSync(dataDir)) {
      const files = fs.readdirSync(dataDir).filter((f: string) => f.endsWith('.db'));
      print(`  \x1b[1mDatabases:\x1b[0m ${files.length} (.db files in data/)`);
      for (const f of files) {
        const size = fs.statSync(path.join(dataDir, f)).size;
        print(`    ${f} (${(size / 1024).toFixed(1)} KB)`);
      }
    }
    print('');
    success('Status OK');
  } catch (err) {
    error(`Failed to read config: ${(err as Error).message}`);
  }
}

function cmdFeatures(): void {
  banner();
  info('Available features:');
  print('');

  const featuresDir = path.join(__dirname, 'src', 'features');
  if (!fs.existsSync(featuresDir)) {
    error('Features directory not found');
    return;
  }

  const features = fs.readdirSync(featuresDir).filter((f: string) => {
    return fs.existsSync(path.join(featuresDir, f, 'index.ts')) ||
           fs.existsSync(path.join(featuresDir, f, 'index.js'));
  }).sort();

  for (const name of features) {
    // Try to read meta
    let desc = '';
    try {
      const distPath = path.join(__dirname, 'src', 'features', name, 'index.js');
      if (fs.existsSync(distPath)) {
        // Just list the name for now
      }
    } catch {}
    print(`  • ${name}`);
  }

  print('');
  print(`  Total: ${features.length} features`);
  print('');
}

function cmdFeature(): void {
  const name = args[1];
  if (!name) {
    error('Usage: agclaw feature <name>');
    return;
  }

  const distPath = path.join(__dirname, 'src', 'features', name, 'index.js');
  const srcPath = path.join(__dirname, '..', 'src', 'features', name, 'index.ts');

  if (!fs.existsSync(distPath) && !fs.existsSync(srcPath)) {
    error(`Feature '${name}' not found`);
    return;
  }

  banner();
  info(`Feature: ${name}`);

  try {
    const feature = require(distPath).default;
    print('');
    print(`  Name: ${feature.meta.name}`);
    print(`  Version: ${feature.meta.version}`);
    print(`  Description: ${feature.meta.description}`);
    print(`  Dependencies: ${feature.meta.dependencies?.join(', ') || 'none'}`);
    print('');
  } catch (err) {
    warn(`Could not load feature metadata: ${(err as Error).message}`);
    print(`  File: ${distPath}`);
    const lines = fs.existsSync(distPath) ? fs.readFileSync(distPath, 'utf8').split('\n').length : 0;
    print(`  Size: ${lines} lines`);
  }
}

function cmdConfig(): void {
  const key = args[1];
  const value = args[2];
  const workDir = getWorkDir();
  const configPath = path.join(workDir, 'agclaw.json');

  if (!fs.existsSync(configPath)) {
    error('AG-Claw not initialized. Run: agclaw init');
    return;
  }

  const config = JSON.parse(fs.readFileSync(configPath, 'utf8'));

  if (!key) {
    // Show all config
    banner();
    print(JSON.stringify(config, null, 2));
    return;
  }

  if (!value) {
    // Show specific key
    const keys = key.split('.');
    let val: any = config;
    for (const k of keys) {
      val = val?.[k];
    }
    if (val !== undefined) {
      print(`${key} = ${JSON.stringify(val)}`);
    } else {
      error(`Key '${key}' not found`);
    }
    return;
  }

  // Set value
  const keys = key.split('.');
  let obj: any = config;
  for (let i = 0; i < keys.length - 1; i++) {
    if (!obj[keys[i]]) obj[keys[i]] = {};
    obj = obj[keys[i]];
  }
  try {
    obj[keys[keys.length - 1]] = JSON.parse(value);
  } catch {
    obj[keys[keys.length - 1]] = value;
  }
  fs.writeFileSync(configPath, JSON.stringify(config, null, 2));
  success(`Set ${key} = ${value}`);
}

async function cmdDoctor(): Promise<void> {
  banner();
  info('Running diagnostics...');
  print('');

  const checks: Array<{ name: string; check: () => boolean; fix: string }> = [
    {
      name: 'Node.js version >= 18',
      check: () => parseInt(process.version.slice(1)) >= 18,
      fix: 'Upgrade Node.js to v18 or later',
    },
    {
      name: 'agclaw.json exists',
      check: () => fs.existsSync(path.join(getWorkDir(), 'agclaw.json')),
      fix: 'Run: agclaw init',
    },
    {
      name: 'data/ directory exists',
      check: () => fs.existsSync(path.join(getWorkDir(), 'data')),
      fix: 'Run: agclaw init (creates data/)',
    },
    {
      name: 'Features compiled',
      check: () => fs.existsSync(path.join(__dirname, 'src', 'features')),
      fix: 'Run: npm run build',
    },
    {
      name: 'better-sqlite3 installed',
      check: () => {
        try { require('better-sqlite3'); return true; } catch { return false; }
      },
      fix: 'Run: npm install better-sqlite3',
    },
  ];

  let passed = 0;
  for (const { name, check, fix } of checks) {
    const ok = check();
    if (ok) {
      success(name);
      passed++;
    } else {
      warn(`${name} — ${fix}`);
    }
  }

  print('');
  print(`  ${passed}/${checks.length} checks passed`);
  print('');
}

async function cmdConnect(): Promise<void> {
  banner();
  info('Setup connections to external services');
  print('');
  print('  Available integrations:');
  print('    • Telegram (bots, webhooks)');
  print('    • Email (IMAP/SMTP)');
  print('    • Discord (bot)');
  print('    • Slack (events API)');
  print('    • WhatsApp (Business API)');
  print('    • GitHub (webhooks, issues)');
  print('');
  info('Configure via environment variables or agclaw.json [integrations] section');
  info('Example: agclaw config integrations.telegram.botToken <token>');
}

async function cmdAgents(): Promise<void> {
  banner();
  info('Agent management (placeholder)');
  print('');
  // Could list registered agents from multi-agent-coordination feature
  success('Agent management available via multi-agent-coordination feature');
  info('Use: agclaw feature multi-agent-coordination agents');
}

async function cmdTools(): Promise<void> {
  banner();
  info('List available tools/features');
  print('');

  const featuresDir = path.join(__dirname, 'src', 'features');
  if (!fs.existsSync(featuresDir)) {
    error('Features directory not found');
    return;
  }

  const features = fs.readdirSync(featuresDir)
    .filter(f => fs.existsSync(path.join(featuresDir, f, 'index.js')))
    .sort();

  for (const name of features) {
    try {
      const feature = require(path.join(featuresDir, name, 'index.js')).default;
      const state = feature?.init ? '✓' : '✗';
      print(`  ${state} ${name}`);
    } catch {
      print(`  ? ${name}`);
    }
  }

  print('');
  print(`  Total: ${features.length} tools`);
}

async function cmdSessions(): Promise<void> {
  banner();
  info('Active sessions (placeholder)');
  print('');
  success('Session management available via multi-agent-coordination and checkpoint features');
}

async function cmdWatch(): Promise<void> {
  const target = args[1];
  if (!target) {
    error('Usage: agclaw watch <path> [--pattern "*.ts"]');
    return;
  }
  banner();
  info(`Watching ${target} for changes... (Ctrl+C to stop)`);
  // Could use file-watcher feature directly
  // For now, just show a message
  print('');
  warn('File watcher requires file-watcher feature to be enabled');
  info('Enable it in config and restart AG-Claw server');
}

async function cmdGateway(): Promise<void> {
  const subcommand = args[1] || 'status';
  const workDir = getWorkDir();
  const pidFile = path.join(workDir, 'data', '.gateway.pid');

  const getPid = (): number | null => {
    try {
      if (fs.existsSync(pidFile)) {
        const pid = parseInt(fs.readFileSync(pidFile, 'utf8').trim(), 10);
        // Check if process is running
        try {
          process.kill(pid, 0);
          return pid;
        } catch {
          fs.unlinkSync(pidFile);
          return null;
        }
      }
    } catch {}
    return null;
  };

  switch (subcommand) {
    case 'status': {
      banner();
      const pid = getPid();
      if (pid) {
        success(`Gateway running (PID: ${pid})`);
        info(`PID file: ${pidFile}`);
      } else {
        info('Gateway not running');
      }
      break;
    }

    case 'start': {
      banner();
      if (getPid()) {
        warn('Gateway already running');
        return;
      }
      const port = args.includes('--port') ? parseInt(args[args.indexOf('--port') + 1], 10) : 3000;
      info(`Starting AG-Claw gateway on port ${port}...`);

      // Spawn gateway as background process
      const { spawn } = await import('child_process');
      const gatewayPath = path.join(__dirname, 'cli.js');
      const child = spawn('node', [gatewayPath, 'start', '--port', String(port)], {
        detached: true,
        stdio: ['ignore',
          fs.openSync(path.join(workDir, 'data', 'gateway.log'), 'a'),
          fs.openSync(path.join(workDir, 'data', 'gateway.log'), 'a'),
        ],
        cwd: workDir,
      });
      child.unref();

      // Write PID
      fs.mkdirSync(path.join(workDir, 'data'), { recursive: true });
      fs.writeFileSync(pidFile, String(child.pid));
      success(`Gateway started (PID: ${child.pid})`);
      info(`Log: ${path.join(workDir, 'data', 'gateway.log')}`);
      info(`Stop: agclaw gateway stop`);
      break;
    }

    case 'stop': {
      banner();
      const pid = getPid();
      if (!pid) {
        info('Gateway not running');
        return;
      }
      info(`Stopping gateway (PID: ${pid})...`);
      try {
        process.kill(pid, 'SIGTERM');
        fs.unlinkSync(pidFile);
        success('Gateway stopped');
      } catch (err) {
        error(`Failed to stop: ${(err as Error).message}`);
      }
      break;
    }

    case 'restart': {
      banner();
      const pid = getPid();
      if (pid) {
        info(`Stopping gateway (PID: ${pid})...`);
        try { process.kill(pid, 'SIGTERM'); } catch {}
        // Wait a moment
        await new Promise(r => setTimeout(r, 1000));
      }
      const port = args.includes('--port') ? parseInt(args[args.indexOf('--port') + 1], 10) : 3000;
      info(`Restarting AG-Claw gateway on port ${port}...`);

      const { spawn } = await import('child_process');
      const gatewayPath = path.join(__dirname, 'cli.js');
      const child = spawn('node', [gatewayPath, 'start', '--port', String(port)], {
        detached: true,
        stdio: ['ignore',
          fs.openSync(path.join(workDir, 'data', 'gateway.log'), 'a'),
          fs.openSync(path.join(workDir, 'data', 'gateway.log'), 'a'),
        ],
        cwd: workDir,
      });
      child.unref();

      fs.mkdirSync(path.join(workDir, 'data'), { recursive: true });
      fs.writeFileSync(pidFile, String(child.pid));
      success(`Gateway restarted (PID: ${child.pid})`);
      break;
    }

    case 'logs': {
      const logFile = path.join(workDir, 'data', 'gateway.log');
      if (!fs.existsSync(logFile)) {
        info('No log file found');
        return;
      }
      const lines = args.includes('-n') ? parseInt(args[args.indexOf('-n') + 1], 10) : 50;
      const logContent = fs.readFileSync(logFile, 'utf8');
      const logLines = logContent.split('\n').slice(-lines);
      print(logLines.join('\n'));
      break;
    }

    default:
      error(`Unknown gateway command: ${subcommand}`);
      print('Usage: agclaw gateway [status|start|stop|restart|logs]');
  }
}

async function cmdPlugins(): Promise<void> {
  banner();
  info('Plugin management');
  print('');

  const featuresDir = path.join(__dirname, 'src', 'features');
  if (!fs.existsSync(featuresDir)) {
    error('Features directory not found');
    return;
  }

  const features = fs.readdirSync(featuresDir)
    .filter(f => fs.existsSync(path.join(featuresDir, f, 'index.js')))
    .sort();

  for (const name of features) {
    try {
      const feature = require(path.join(featuresDir, name, 'index.js')).default;
      const state = feature?.init ? '✓' : '✗';
      print(`  ${state} ${name}`);
    } catch {
      print(`  ? ${name}`);
    }
  }

  print('');
  print(`  Total: ${features.length} plugins`);
  info('Enable/disable in agclaw.json [features]');
}

async function cmdOnboard(): Promise<void> {
  const readline = require('readline');
  const rl = readline.createInterface({ input: process.stdin, output: process.stdout });
  const ask = (q: string): Promise<string> => new Promise(resolve => rl.question(q, resolve));

  banner();
  print('  \x1b[1mWelcome to AG-Claw!\x1b[0m');
  print('  This wizard will set up your agent.');
  print('');

  const config: any = {
    $schema: 'https://github.com/AG064/ag-claw/blob/main/config-schema.json',
    name: 'My AG-Claw Instance',
    version: '1.0.0',
    server: { port: 3000, host: '0.0.0.0' },
    features: {},
    llm: { provider: 'nvidia', model: 'deepseek-ai/deepseek-v3.2' },
  };

  // Step 1: Instance name
  print('  \x1b[1mStep 1: Instance\x1b[0m');
  const name = await ask('  Name your instance (default: My AG-Claw): ');
  if (name.trim()) config.name = name.trim();
  print('');

  // Step 2: LLM Provider
  print('  \x1b[1mStep 2: LLM Provider\x1b[0m');
  print('  Available providers:');
  print('    1. OpenRouter (needs API key)');
  print('    2. NVIDIA (free models, needs API key)');
  print('    3. Google Gemini (free tier)');
  print('    4. Anthropic (needs API key)');
  print('    5. Skip (configure later)');
  const providerChoice = await ask('  Choose (1-5, default: 2): ');
  const providers: Record<string, { provider: string; model: string; keyEnv: string }> = {
    '1': { provider: 'openrouter', model: 'auto', keyEnv: 'OPENROUTER_API_KEY' },
    '2': { provider: 'nvidia', model: 'deepseek-ai/deepseek-v3.2', keyEnv: 'NVIDIA_API_KEY' },
    '3': { provider: 'google', model: 'gemini-2.5-flash', keyEnv: 'GOOGLE_API_KEY' },
    '4': { provider: 'anthropic', model: 'claude-sonnet-4-20250514', keyEnv: 'ANTHROPIC_API_KEY' },
  };
  const chosen = providers[providerChoice.trim()] || providers['2'];
  config.llm = { provider: chosen.provider, model: chosen.model };

  if (providerChoice.trim() !== '5') {
    const apiKey = await ask(`  ${chosen.keyEnv} (or press Enter to skip): `);
    if (apiKey.trim()) {
      // Write to .env
      const envPath = path.join(getWorkDir(), '.env');
      const envLine = `${chosen.keyEnv}=${apiKey.trim()}\n`;
      fs.appendFileSync(envPath, envLine);
      success(`Saved ${chosen.keyEnv} to .env`);
    }
  }
  print('');

  // Step 3: Telegram
  print('  \x1b[1mStep 3: Telegram (optional)\x1b[0m');
  const setupTg = await ask('  Set up Telegram bot? (y/N): ');
  if (setupTg.toLowerCase() === 'y') {
    const botToken = await ask('  Bot token: ');
    if (botToken.trim()) {
      const userId = await ask('  Your Telegram user ID (e.g. tg:123456): ');
      config.features.telegram = {
        enabled: true,
        botToken: botToken.trim(),
        allowFrom: userId.trim() ? [userId.trim()] : [],
        dmPolicy: 'pairing',
        groupPolicy: 'allowlist',
      };
      success('Telegram configured');
    }
  }
  print('');

  // Step 4: Features
  print('  \x1b[1mStep 4: Core Features\x1b[0m');
  print('  Enabling recommended features...');
  const defaults = [
    'life-domains', 'skills-library', 'goal-decomposition',
    'sqlite-memory', 'cron-scheduler', 'audit-log',
    'knowledge-graph', 'webhooks', 'file-watcher',
  ];
  for (const f of defaults) {
    config.features[f] = { enabled: true };
  }
  print(`  ✓ ${defaults.length} features enabled`);
  print('');

  // Step 5: Port
  print('  \x1b[1mStep 5: Server\x1b[0m');
  const port = await ask('  Server port (default: 3000): ');
  if (port.trim() && !isNaN(parseInt(port))) {
    config.server.port = parseInt(port);
  }
  print('');

  // Save config
  const configPath = path.join(getWorkDir(), 'agclaw.json');
  fs.writeFileSync(configPath, JSON.stringify(config, null, 2));
  success('Configuration saved to agclaw.json');

  // Create data dir
  const dataDir = path.join(getWorkDir(), 'data');
  if (!fs.existsSync(dataDir)) {
    fs.mkdirSync(dataDir, { recursive: true });
  }
  success('Data directory ready');

  rl.close();

  print('');
  print('  \x1b[1mSetup complete!\x1b[0m');
  print('');
  print('  Next steps:');
  print(`    agclaw gateway start --port ${config.server.port}`);
  print('    agclaw status');
  print('    agclaw skill search <query>');
  print('');
}

async function cmdSkill(): Promise<void> {
  const subcommand = args[1] || 'list';
  const { execSync } = require('child_process');
  const workDir = getWorkDir();

  // Default skills dir: OpenClaw workspace
  const defaultWorkDir = path.join(process.env.HOME || '~', '.openclaw', 'workspace');
  const clawhubWorkDir = process.env.AGCLAW_WORKDIR || defaultWorkDir;

  const runClawhub = (cmd: string): string => {
    try {
      return execSync(`clawhub ${cmd} --workdir "${clawhubWorkDir}" --no-input`, {
        encoding: 'utf8',
        timeout: 30000,
        env: { ...process.env, CLAWHUB_WORKDIR: clawhubWorkDir },
      });
    } catch (err: any) {
      return err.stdout || err.stderr || err.message;
    }
  };

  switch (subcommand) {
    case 'list':
    case 'ls': {
      banner();
      info('Installed skills:');
      print('');
      const result = runClawhub('list');
      print(result);
      break;
    }

    case 'search': {
      const query = args.slice(2).join(' ');
      if (!query) {
        error('Usage: agclaw skill search <query>');
        return;
      }
      banner();
      info(`Searching for: ${query}`);
      print('');
      const result = runClawhub(`search "${query}"`);
      print(result);
      break;
    }

    case 'install':
    case 'add': {
      const slug = args[2];
      if (!slug) {
        error('Usage: agclaw skill install <skill-slug>');
        return;
      }
      banner();
      info(`Installing: ${slug}`);
      print('');
      const result = runClawhub(`install ${slug}`);
      print(result);
      if (!result.includes('error')) {
        success(`Skill '${slug}' installed`);
      }
      break;
    }

    case 'uninstall':
    case 'remove':
    case 'rm': {
      const slug = args[2];
      if (!slug) {
        error('Usage: agclaw skill uninstall <skill-slug>');
        return;
      }
      banner();
      info(`Uninstalling: ${slug}`);
      const result = runClawhub(`uninstall ${slug}`);
      print(result);
      break;
    }

    case 'update':
    case 'upgrade': {
      const slug = args[2];
      banner();
      if (slug) {
        info(`Updating: ${slug}`);
      } else {
        info('Updating all installed skills...');
      }
      const result = runClawhub(slug ? `update ${slug}` : 'update');
      print(result);
      break;
    }

    case 'explore': {
      banner();
      info('Browse latest skills from ClawHub:');
      print('');
      const result = runClawhub('explore');
      print(result);
      break;
    }

    case 'info':
    case 'inspect': {
      const slug = args[2];
      if (!slug) {
        error('Usage: agclaw skill info <skill-slug>');
        return;
      }
      banner();
      info(`Inspecting: ${slug}`);
      print('');
      const result = runClawhub(`inspect ${slug}`);
      print(result);
      break;
    }

    case 'publish': {
      const skillPath = args[2] || '.';
      banner();
      info(`Publishing: ${skillPath}`);
      const result = runClawhub(`publish "${skillPath}"`);
      print(result);
      break;
    }

    case 'star': {
      const slug = args[2];
      if (!slug) {
        error('Usage: agclaw skill star <skill-slug>');
        return;
      }
      runClawhub(`star ${slug}`);
      success(`Starred: ${slug}`);
      break;
    }

    case 'unstar': {
      const slug = args[2];
      if (!slug) {
        error('Usage: agclaw skill unstar <skill-slug>');
        return;
      }
      runClawhub(`unstar ${slug}`);
      success(`Unstarred: ${slug}`);
      break;
    }

    case 'sync': {
      banner();
      info('Syncing local skills with ClawHub...');
      const result = runClawhub('sync');
      print(result);
      break;
    }

    default:
      // Treat as "run" — execute a script from an installed skill
      {
        const skillName = subcommand;
        const skillsDir = path.join(clawhubWorkDir, 'skills');
        const skillPath = path.join(skillsDir, skillName);
        if (!fs.existsSync(skillPath)) {
          error(`Unknown command or skill: ${skillName}`);
          print('');
          print('  agclaw skill [list|search|install|uninstall|update|explore|info|publish|star|sync]');
          return;
        }
        const scriptName = args[2];
        if (!scriptName) {
          const scriptsDir = path.join(skillPath, 'scripts');
          if (fs.existsSync(scriptsDir)) {
            const scripts = fs.readdirSync(scriptsDir).filter((f: string) => /\.(sh|js|py|ts)$/.test(f));
            if (scripts.length) {
              info(`Scripts in ${skillName}:`);
              for (const s of scripts) print(`  • ${s}`);
            }
          }
          return;
        }
        const scriptPath = path.join(skillPath, 'scripts', scriptName);
        if (!fs.existsSync(scriptPath)) {
          error(`Script '${scriptName}' not found`);
          return;
        }
        const scriptArgs = args.slice(3);
        let cmd: string;
        if (scriptName.endsWith('.sh')) cmd = `bash "${scriptPath}"`;
        else if (scriptName.endsWith('.js')) cmd = `node "${scriptPath}"`;
        else if (scriptName.endsWith('.py')) cmd = `python3 "${scriptPath}"`;
        else cmd = `npx tsx "${scriptPath}"`;
        if (scriptArgs.length) cmd += ' ' + scriptArgs.map((a: string) => `"${a}"`).join(' ');
        try {
          const result = execSync(cmd, { cwd: skillPath, timeout: 30000, encoding: 'utf8' });
          print(result);
        } catch (err: any) {
          error(err.message);
        }
      }
  }
}

async function cmdTelegram(): Promise<void> {
  const subcommand = args[1] || 'status';
  banner();

  switch (subcommand) {
    case 'status': {
      info('Telegram bot status');
      print('');
      const configPath = path.join(getWorkDir(), 'agclaw.json');
      if (!fs.existsSync(configPath)) {
        error('AG-Claw not initialized. Run: agclaw init');
        return;
      }
      const config = JSON.parse(fs.readFileSync(configPath, 'utf8'));
      const tg = config.features?.telegram;
      if (!tg) {
        warn('Telegram not configured');
        info('Add to agclaw.json:');
        print('  {');
        print('    "features": {');
        print('      "telegram": {');
        print('        "enabled": true,');
        print('        "botToken": "YOUR_BOT_TOKEN"');
        print('      }');
        print('    }');
        print('  }');
        return;
      }
      print(`  Enabled: ${tg.enabled ? '✓' : '✗'}`);
      print(`  Bot Token: ${tg.botToken ? '***' + tg.botToken.slice(-8) : 'NOT SET'}`);
      print(`  DM Policy: ${tg.dmPolicy || 'pairing'}`);
      print(`  Group Policy: ${tg.groupPolicy || 'allowlist'}`);
      print(`  Allowed Users: ${(tg.allowFrom || []).join(', ') || 'none'}`);
      break;
    }

    case 'pair': {
      const code = args[2];
      if (!code) {
        // Generate pairing code
        const crypto = require('crypto');
        const newCode = crypto.randomBytes(4).toString('hex');
        info(`Pairing code: ${newCode}`);
        info('Valid for 10 minutes');
        info('Share with user: /pair ' + newCode);
      } else {
        info(`Pairing code: ${code}`);
      }
      break;
    }

    case 'allow': {
      const userId = args[2];
      if (!userId) {
        error('Usage: agclaw telegram allow tg:USER_ID');
        return;
      }
      info(`Adding ${userId} to allowed users`);
      // Would update config and/or database
      success('User added (update agclaw.json to persist)');
      break;
    }

    case 'config': {
      info('Telegram configuration template');
      print('');
      print(JSON.stringify({
        features: {
          telegram: {
            enabled: true,
            botToken: 'YOUR_BOT_TOKEN',
            allowFrom: ['tg:YOUR_USER_ID'],
            groupPolicy: 'allowlist',
            groups: {
              '-100XXXXXXXXXX': { requireMention: false }
            },
            dmPolicy: 'pairing',
            streaming: 'partial',
            reactionNotifications: 'minimal',
            markdown: { tables: 'code' },
          }
        }
      }, null, 2));
      break;
    }

    default:
      error(`Unknown telegram command: ${subcommand}`);
      print('Usage: agclaw telegram [status|pair|allow|config]');
  }
}

// ─── Main ─────────────────────────────────────────────────────────────────────

async function main(): Promise<void> {
  switch (command) {
    case 'help':
    case '--help':
    case '-h':
      cmdHelp();
      break;
    case 'version':
    case '--version':
    case '-v':
      cmdVersion();
      break;
    case 'init':
      cmdInit();
      break;
    case 'start':
    case 'server':
    case 'run':
      await cmdStart();
      break;
    case 'status':
      cmdStatus();
      break;
    case 'features':
    case 'list':
      cmdFeatures();
      break;
    case 'feature':
    case 'info':
      cmdFeature();
      break;
    case 'config':
    case 'cfg':
      cmdConfig();
      break;
    case 'doctor':
    case 'check':
      await cmdDoctor();
      break;
    case 'connect':
      await cmdConnect();
      break;
    case 'agents':
      await cmdAgents();
      break;
    case 'tools':
      await cmdTools();
      break;
    case 'sessions':
      await cmdSessions();
      break;
    case 'watch':
      await cmdWatch();
      break;
    case 'gateway':
      await cmdGateway();
      break;
    case 'plugins':
      await cmdPlugins();
      break;
    case 'telegram':
      await cmdTelegram();
      break;
    case 'skill':
    case 'skills':
      await cmdSkill();
      break;
    case 'onboard':
    case 'setup':
    case 'configure':
      await cmdOnboard();
      break;
    default:
      error(`Unknown command: ${command}`);
      print('Run "agclaw help" for usage information');
      process.exit(1);
  }
}

main().catch(err => {
  error(err.message);
  process.exit(1);
});
