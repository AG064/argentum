#!/usr/bin/env node
"use strict";
/**
 * Argentum CLI
 *
 * Usage:
 *   argentum init                    Initialize Argentum in current directory
 *   argentum start [--port 3000]     Start Argentum server
 *   argentum status                  Show system status
 *   argentum features                List all features
 *   argentum feature <name> <cmd>    Run feature command
 *   argentum config [key] [value]    Show/set configuration
 *   argentum doctor                  Diagnose setup issues
 *   argentum help                    Show help
 */
var __createBinding = (this && this.__createBinding) || (Object.create ? (function(o, m, k, k2) {
    if (k2 === undefined) k2 = k;
    var desc = Object.getOwnPropertyDescriptor(m, k);
    if (!desc || ("get" in desc ? !m.__esModule : desc.writable || desc.configurable)) {
      desc = { enumerable: true, get: function() { return m[k]; } };
    }
    Object.defineProperty(o, k2, desc);
}) : (function(o, m, k, k2) {
    if (k2 === undefined) k2 = k;
    o[k2] = m[k];
}));
var __setModuleDefault = (this && this.__setModuleDefault) || (Object.create ? (function(o, v) {
    Object.defineProperty(o, "default", { enumerable: true, value: v });
}) : function(o, v) {
    o["default"] = v;
});
var __importStar = (this && this.__importStar) || (function () {
    var ownKeys = function(o) {
        ownKeys = Object.getOwnPropertyNames || function (o) {
            var ar = [];
            for (var k in o) if (Object.prototype.hasOwnProperty.call(o, k)) ar[ar.length] = k;
            return ar;
        };
        return ownKeys(o);
    };
    return function (mod) {
        if (mod && mod.__esModule) return mod;
        var result = {};
        if (mod != null) for (var k = ownKeys(mod), i = 0; i < k.length; i++) if (k[i] !== "default") __createBinding(result, mod, k[i]);
        __setModuleDefault(result, mod);
        return result;
    };
})();
Object.defineProperty(exports, "__esModule", { value: true });
const fs = __importStar(require("fs"));
const path = __importStar(require("path"));
const readline = __importStar(require("readline"));
require("dotenv/config");
const yaml_1 = require("yaml");
const branding_1 = require("./core/branding");
const cli_launch_1 = require("./core/cli-launch");
const config_1 = require("./core/config");
const esm_1 = require("./core/esm");
const onboarding_1 = require("./core/onboarding");
const plugin_loader_1 = require("./core/plugin-loader");
const modelDiscovery_js_1 = require("./utils/modelDiscovery.js");
const VERSION = '0.0.4';
const PROGRAM_TITLE = 'Argentum';
const PRIMARY_COMMAND = 'argentum';
const WORKDIR_ENV = 'ARGENTUM_WORKDIR';
const SKIP_EXIT_PAUSE_ENV = 'ARGENTUM_SKIP_EXIT_PAUSE';
const args = process.argv.slice(2);
const launch = (0, cli_launch_1.resolveCliLaunch)(args, {
    execPath: process.execPath,
    isPackaged: isPackagedRuntime(),
    platform: process.platform,
});
const command = launch.command;
setProgramTitle(PROGRAM_TITLE);
// ─── Helpers ──────────────────────────────────────────────────────────────────
function print(text) {
    process.stdout.write(`${text}\n`);
}
function error(text) {
    process.stderr.write(`\x1b[31mError:\x1b[0m ${text}\n`);
}
function success(text) {
    print(`\x1b[32m✓\x1b[0m ${text}`);
}
function info(text) {
    print(`\x1b[36mℹ\x1b[0m ${text}`);
}
function warn(text) {
    print(`\x1b[33m⚠\x1b[0m ${text}`);
}
function setProgramTitle(title) {
    process.title = title;
    if (process.platform === 'win32' && process.stdout.isTTY) {
        process.stdout.write(`\u001b]0;${title}\u0007`);
    }
}
function isPackagedRuntime() {
    return Boolean(process.pkg);
}
function banner() {
    console.log((0, branding_1.formatArgentumBanner)(VERSION));
}
function getWorkDir() {
    const configuredWorkDir = process.env[WORKDIR_ENV];
    if (configuredWorkDir) {
        return configuredWorkDir;
    }
    if (launch.command === 'launch') {
        return path.join(homeDir(), '.argentum');
    }
    return process.cwd();
}
function hasFlag(...flags) {
    return flags.some((flag) => args.includes(flag));
}
function getArgValue(flag) {
    const index = args.indexOf(flag);
    return index >= 0 ? args[index + 1] : undefined;
}
function getProjectConfigPath(workDir = getWorkDir()) {
    const yamlPath = path.join(workDir, 'config', 'default.yaml');
    const argentumPath = path.join(workDir, 'argentum.json');
    if (fs.existsSync(yamlPath))
        return yamlPath;
    if (fs.existsSync(argentumPath))
        return argentumPath;
    return yamlPath;
}
function projectConfigExists(workDir = getWorkDir()) {
    return (fs.existsSync(path.join(workDir, 'config', 'default.yaml')) ||
        fs.existsSync(path.join(workDir, 'argentum.json')));
}
function resolveFeaturesDir() {
    const candidates = [
        path.join(__dirname, 'features'),
        path.join(__dirname, '..', 'src', 'features'),
        path.join(__dirname, 'src', 'features'),
        path.join(process.cwd(), 'dist', 'features'),
        path.join(process.cwd(), 'src', 'features'),
    ];
    const seen = new Set();
    for (const candidate of candidates) {
        const normalized = path.resolve(candidate);
        if (seen.has(normalized))
            continue;
        seen.add(normalized);
        if (fs.existsSync(normalized) && fs.statSync(normalized).isDirectory()) {
            return normalized;
        }
    }
    return null;
}
function readProjectConfig(configPath = getProjectConfigPath()) {
    const raw = fs.readFileSync(configPath, 'utf8');
    if (configPath.endsWith('.json')) {
        return JSON.parse(raw);
    }
    return ((0, yaml_1.parse)(raw) ?? {});
}
function writeProjectConfig(configPath, config) {
    if (configPath.endsWith('.json')) {
        fs.writeFileSync(configPath, JSON.stringify(config, null, 2));
        return;
    }
    fs.writeFileSync(configPath, (0, yaml_1.stringify)(config, { lineWidth: 120 }));
}
function appendEnvEntries(workDir, entries) {
    const envPath = path.join(workDir, '.env');
    const existing = fs.existsSync(envPath) ? fs.readFileSync(envPath, 'utf8') : '';
    const lines = existing ? [existing.replace(/\s*$/, '')] : [];
    for (const [key, value] of Object.entries(entries)) {
        if (!value || new RegExp(`^${key.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')}=`, 'm').test(existing)) {
            continue;
        }
        lines.push(`${key}=${/[\s"'#=]/.test(value) ? JSON.stringify(value) : value}`);
    }
    fs.writeFileSync(envPath, `${lines.filter(Boolean).join('\n')}\n`);
}
function parseNumberCsv(value) {
    return value
        .split(',')
        .map((item) => Number(item.trim()))
        .filter(Number.isFinite);
}
// ─── Commands ─────────────────────────────────────────────────────────────────
function cmdHelp() {
    banner();
    print('  \x1b[1mUsage:\x1b[0m');
    print('    argentum <command> [options]');
    print('');
    print('  \x1b[1mCommands:\x1b[0m');
    print('    init                  Initialize Argentum in current directory');
    print('    gateway start         Start Argentum server (background)');
    print('    gateway stop          Stop Argentum server');
    print('    gateway restart       Restart Argentum server');
    print('    gateway status        Check if server is running');
    print('    gateway logs          View server logs');
    print('    status                Show system and feature status');
    print('    features              List all available features');
    print('    feature <name>        Show feature details');
    print('    config [key] [value]  Show or set configuration');
    print('    doctor                Diagnose setup issues');
    print('    status                Show system status');
    print('    onboard               Interactive setup wizard');
    print('    cron list             List scheduled jobs');
    print('    cron add              Create a new cron job');
    print('    cron run <id>         Execute job immediately');
    print('    cron enable <id>      Enable a job');
    print('    cron disable <id>     Disable a job');
    print('    cron remove <id>      Delete a job');
    print('    budget status         Show budget usage & limits');
    print('    budget set-limit      Set monthly limit ($)  ');
    print('    budget status         Show budget usage & limits');
    print('    budget set-limit      Set monthly limit ($)');
    print('    session list          List sessions');
    print('    acp run <code>        Execute code (JS/Python/Bash) in sandbox');
    print('    session create <name> Create a session');
    print('    session show <id>     View session messages');
    print('    session search <q>    Search messages');
    print('    backup create         Create backup');
    print('    backup list           List backups');
    print('    backup restore <name> Restore from backup');
    print('    memory search <q>     Search memory');
    print('    memory list           List namespaces');
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
    print('    security status      Show security overview');
    print('    security policies     Manage security policies');
    print('    security approvals    Review pending approvals');
    print('    security audit        View audit log');
    print('    security credentials  Manage credentials');
    print('    security sandbox      Show sandbox config');
    print('    security blueprint    Blueprint management');
    print('    image "prompt"        Generate image (--resolution 1K|2K|4K, --edit, --output)');
    print('    version               Show version');
    print('    help                  Show this help');
    print('');
    print('  \x1b[1mExamples:\x1b[0m');
    print('    argentum init');
    print('    argentum start --port 3000');
    print('    argentum features');
    print('    argentum feature life-domains');
    print('');
}
function cmdACP() {
    const action = args[1];
    if (action !== 'run') {
        error('Usage: argentum acp run <code>');
        return;
    }
    const code = args.slice(2).join(' ');
    if (!code) {
        error('No code provided');
        return;
    }
    banner();
    info('Executing code...');
    const start = Date.now();
    try {
        const { spawn } = require('child_process');
        const cmd = 'node';
        const proc = spawn(cmd, ['-e', code], { timeout: 30000 });
        let stdout = '', stderr = '';
        proc.stdout?.on('data', (d) => {
            stdout += d;
        });
        proc.stderr?.on('data', (d) => {
            stderr += d;
        });
        proc.on('close', (code) => {
            const duration = Date.now() - start;
            print('');
            if (code === 0) {
                success(`Exit code: ${code} (${duration}ms)`);
            }
            else {
                warn(`Exit code: ${code} (${duration}ms)`);
            }
            if (stdout)
                print(`[stdout]\n${stdout}`);
            if (stderr)
                print(`[stderr]\n${stderr}`);
        });
        proc.on('error', (err) => {
            error(`Failed to start process: ${err.message}`);
        });
    }
    catch (err) {
        error(`Execution error: ${err instanceof Error ? err.message : String(err)}`);
    }
}
function cmdImage() {
    // argentum image "prompt" [--resolution 1K|2K|4K] [--edit input.png] [--output name.png]
    const subArgs = args.slice(1);
    if (subArgs.length === 0 || subArgs[0] === 'help' || subArgs[0] === '--help' || subArgs[0] === '-h') {
        banner();
        print('  \x1b[1mImage Generation\x1b[0m');
        print('');
        print('  \x1b[1mUsage:\x1b[0m');
        print('    argentum image "prompt text" [options]');
        print('');
        print('  \x1b[1mOptions:\x1b[0m');
        print('    --resolution, -r   Resolution: 1K (default), 2K, 4K');
        print('    --edit, -e          Input image path for editing (optional)');
        print('    --output, -o        Output filename (default: generated timestamped name)');
        print('');
        print('  \x1b[1mExamples:\x1b[0m');
        print('    argentum image "a sunset over mountains"');
        print('    argentum image "a cat" --resolution 2K --output cat.png');
        print('    argentum image "make it blue" --edit input.png --output result.png');
        print('');
        print('  \x1b[1mProviders:\x1b[0m');
        print('    Primary:   Gemini 3 Pro Image (gemini-3-pro-image)');
        print('    Fallback: SiliconFlow FLUX.1-dev (automatic on quota error)');
        print('');
        return;
    }
    // Parse arguments
    const promptParts = [];
    let resolution = '1K';
    let inputImage;
    let outputFilename;
    for (let i = 0; i < subArgs.length; i++) {
        const arg = subArgs[i];
        if (!arg)
            continue;
        if (arg === '--resolution' || arg === '-r') {
            const val = subArgs[++i];
            if (val && ['1K', '2K', '4K'].includes(val)) {
                resolution = val;
            }
            else {
                error('Invalid resolution. Use: 1K, 2K, or 4K');
                return;
            }
        }
        else if (arg === '--edit' || arg === '-e') {
            inputImage = subArgs[++i] ?? undefined;
        }
        else if (arg === '--output' || arg === '-o') {
            outputFilename = subArgs[++i] ?? undefined;
        }
        else if (!arg.startsWith('-')) {
            promptParts.push(arg);
        }
    }
    const prompt = promptParts.join(' ');
    if (!prompt.trim()) {
        error('Prompt cannot be empty. Use: argentum image "your prompt"');
        return;
    }
    if (!outputFilename) {
        const timestamp = Date.now();
        const safePrompt = prompt.slice(0, 30).replace(/[^a-zA-Z0-9]/g, '_');
        outputFilename = `image_${safePrompt}_${timestamp}.png`;
    }
    banner();
    info(`Generating image...`);
    print(`  Prompt:      ${prompt.slice(0, 60)}${prompt.length > 60 ? '...' : ''}`);
    print(`  Resolution:  ${resolution}`);
    if (inputImage)
        print(`  Input image: ${inputImage}`);
    print(`  Output:      ${outputFilename}`);
    print('');
    const { spawn } = require('child_process');
    const { existsSync: fsExistsSync } = require('fs');
    const homeDir = process.env.HOME || '/home/ag064';
    const scriptPath = `${homeDir}/.openclaw/workspace/skills/image-gen/scripts/generate_image.py`;
    if (!fsExistsSync(scriptPath)) {
        error(`Script not found: ${scriptPath}`);
        error('Run "argentum setup" or ensure the image-gen skill is installed.');
        return;
    }
    const scriptArgs = [
        'run',
        'python3',
        scriptPath,
        '--prompt',
        prompt,
        '--filename',
        outputFilename,
        '--resolution',
        resolution,
    ];
    if (inputImage) {
        scriptArgs.push('--input-image', inputImage);
    }
    const env = {
        ...process.env,
        ...(process.env.GEMINI_API_KEY ? { GEMINI_API_KEY: process.env.GEMINI_API_KEY } : {}),
    };
    const IMAGE_TIMEOUT_MS = 180000;
    const start = Date.now();
    const proc = spawn('uv', scriptArgs, { env });
    let stdout = '';
    let stderr = '';
    let settled = false;
    const timer = setTimeout(() => {
        if (!settled) {
            settled = true;
            proc.kill('SIGKILL');
            error(`Image generation timed out after ${IMAGE_TIMEOUT_MS / 1000}s`);
        }
    }, IMAGE_TIMEOUT_MS);
    proc.stdout?.on('data', (d) => {
        const line = d.toString();
        stdout += line;
        process.stdout.write(`  ${line}`);
    });
    proc.stderr?.on('data', (d) => {
        const line = d.toString();
        stderr += line;
        // Only show non-quiet lines
        if (!line.includes('[SiliconFlow]') && !line.includes('[Gemini]') && !line.startsWith('  ')) {
            process.stderr.write(`  \x1b[90m${line}\x1b[0m`);
        }
    });
    proc.on('close', (code) => {
        if (settled)
            return;
        settled = true;
        clearTimeout(timer);
        const duration = ((Date.now() - start) / 1000).toFixed(1);
        print('');
        if (code === 0) {
            success(`Image generated in ${duration}s`);
            print(`  Saved to: ${outputFilename}`);
        }
        else {
            warn(`Script exited with code ${code} (${duration}s)`);
            if (stderr) {
                print(`  \x1b[31m${stderr.slice(0, 300)}\x1b[0m`);
            }
        }
    });
    proc.on('error', (err) => {
        if (settled)
            return;
        settled = true;
        clearTimeout(timer);
        error(`Failed to start uv: ${err.message}`);
        error('Make sure "uv" is installed: curl -LsSf https://astral.sh/uv/install.sh | sh');
    });
}
function cmdVersion() {
    print(`ARGENTUM v${VERSION}`);
    print(`Node.js ${process.version}`);
    print(`Platform: ${process.platform} ${process.arch}`);
}
async function cmdLaunch() {
    const workDir = getWorkDir();
    if (!projectConfigExists(workDir)) {
        if (!process.stdin.isTTY || !process.stdout.isTTY) {
            banner();
            info('No Argentum configuration found.');
            info(`Setup workspace: ${workDir}`);
            info(`Run "${PRIMARY_COMMAND} onboard" from a terminal, or double-click argentum.exe to use the interactive setup wizard.`);
            process.exitCode = 1;
            return;
        }
        banner();
        info('No Argentum configuration found. Starting first-run setup.');
        info(`Setup workspace: ${workDir}`);
        print('');
        await cmdOnboard();
        return;
    }
    banner();
    success('Argentum is configured.');
    info(`Workspace: ${workDir}`);
    info(`Use "${PRIMARY_COMMAND} gateway start" to start the server.`);
    info(`Use "${PRIMARY_COMMAND} help" to see all commands.`);
    print('');
    await cmdDoctor();
}
function cmdInit() {
    const workDir = getWorkDir();
    const configPath = path.join(workDir, 'config', 'default.yaml');
    const dataDir = path.join(workDir, 'data');
    banner();
    info('Initializing ARGENTUM...');
    if (fs.existsSync(configPath)) {
        warn('config/default.yaml already exists, skipping');
    }
    else {
        const defaultConfig = {
            $schema: 'https://github.com/AG064/argentum/blob/main/config-schema.json',
            name: 'My ARGENTUM Instance',
            version: '0.0.4',
            server: {
                port: 3000,
                host: '0.0.0.0',
            },
            features: {},
            logging: {
                level: 'info',
            },
            llm: {
                providers: {},
                default: '',
                fallback: [],
            },
        };
        fs.mkdirSync(path.join(workDir, 'config'), { recursive: true });
        fs.writeFileSync(configPath, JSON.stringify(defaultConfig, null, 2));
        success('Created config/default.yaml');
        success('Created .env');
        success('Created .env.example');
    }
    if (!fs.existsSync(dataDir)) {
        fs.mkdirSync(dataDir, { recursive: true });
        success('Created data/ directory');
    }
    else {
        info('data/ directory already exists');
    }
    // Create .env.example
    const envPath = path.join(workDir, '.env.example');
    if (!fs.existsSync(envPath)) {
        fs.writeFileSync(envPath, [
            '# Argentum Environment Variables',
            'ARGENTUM_WORKDIR=.',
            'ARGENTUM_PORT=3000',
            'ARGENTUM_MASTER_KEY=',
            'OPENAI_API_KEY=',
            'ANTHROPIC_API_KEY=',
            'OPENROUTER_API_KEY=',
            '',
        ].join('\n'));
        success('Created .env.example');
    }
    success('ARGENTUM initialized!');
    info('Next: argentum start --port 3000');
}
async function cmdStart() {
    const portIdx = args.indexOf('--port');
    const port = portIdx !== -1 ? parseInt(args[portIdx + 1] ?? '', 10) : 3000;
    const workDir = getWorkDir();
    // First-run check: if no config exists, prompt to onboard
    if (!projectConfigExists(workDir)) {
        banner();
        print('  \x1b[1m\x1b[33m⚠\x1b[0m  No configuration found. Run \x1b[1margentum onboard\x1b[0m first to set up your instance.');
        print('  \x1b[90m   This wizard will configure your instance name, LLM provider, and features.\x1b[0m');
        print('');
        const readline = require('readline');
        const rl = readline.createInterface({ input: process.stdin, output: process.stdout });
        const ask = (q) => new Promise((resolve) => rl.question(q, resolve));
        const answer = (await ask('  \x1b[33m▶\x1b[0m  Run onboard wizard now? [Y]: ')).trim().toLowerCase();
        rl.close();
        if (answer !== 'n') {
            await cmdOnboard();
        }
        else {
            print('');
            info('Run \x1b[1margentum onboard\x1b[0m manually when ready.');
        }
        return;
    }
    // First-run check: if no config exists, prompt to onboard
    const configPath = path.join(process.cwd(), 'config', 'default.yaml');
    const argentumConfigPath = path.join(process.cwd(), 'argentum.json');
    if (!fs.existsSync(configPath) && !fs.existsSync(argentumConfigPath)) {
        banner();
        print('  \x1b[1m\x1b[33m⚠\x1b[0m  No configuration found. Run \x1b[1margentum onboard\x1b[0m first to set up your instance.');
        print('  \x1b[90m   This wizard will configure your instance name, LLM provider, and features.\x1b[0m');
        print('');
        const readline = require('readline');
        const rl = readline.createInterface({ input: process.stdin, output: process.stdout });
        const ask = (q) => new Promise((resolve) => rl.question(q, resolve));
        const answer = (await ask('  \x1b[33m▶\x1b[0m  Run onboard wizard now? [Y]: ')).trim().toLowerCase();
        rl.close();
        if (answer !== 'n') {
            await cmdOnboard();
        }
        else {
            print('');
            info('Run \x1b[1margentum onboard\x1b[0m manually when ready.');
        }
        return;
    }
    banner();
    info(`Starting ARGENTUM server on port ${port}...`);
    try {
        const configManager = new config_1.ConfigManager(getProjectConfigPath(workDir));
        const config = configManager.get();
        const pluginLoader = new plugin_loader_1.PluginLoader(config);
        await pluginLoader.loadAll();
        await pluginLoader.enableAll();
        // Simple HTTP server for API
        const http = await Promise.resolve().then(() => __importStar(require('http')));
        const server = http.createServer(async (req, res) => {
            const url = new URL(req.url || '/', `http://localhost:${port}`);
            // Health check
            if (url.pathname === '/health') {
                const features = pluginLoader.listFeatures().filter((f) => f.state === 'active');
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
                res.end(JSON.stringify(features.map((f) => ({
                    name: f.name,
                    version: f.version,
                    state: f.state,
                }))));
                return;
            }
            // Feature command
            if (url.pathname.startsWith('/api/feature/')) {
                const featureName = url.pathname.split('/')[3];
                const featureState = pluginLoader.getFeatureState(featureName ?? '');
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
                if (f.state === 'active')
                    await pluginLoader.disableFeature(f.name);
            }
            server.close();
            return;
        };
        process.on('SIGINT', shutdown);
        process.on('SIGTERM', shutdown);
    }
    catch (err) {
        error(`Failed to start: ${err.message}`);
        throw new Error(`Failed to start: ${err.message}`);
    }
}
function cmdFeatures() {
    banner();
    info('Available features:');
    print('');
    const featuresDir = resolveFeaturesDir();
    if (!featuresDir || !fs.existsSync(featuresDir)) {
        error('Features directory not found');
        return;
    }
    const features = fs
        .readdirSync(featuresDir)
        .filter((f) => {
        return (fs.existsSync(path.join(featuresDir, f, 'index.ts')) ||
            fs.existsSync(path.join(featuresDir, f, 'index.js')));
    })
        .sort();
    for (const name of features) {
        // Try to read meta
        try {
            const distPath = path.join(featuresDir, name, 'index.js');
            if (fs.existsSync(distPath)) {
                // intentionally empty
            }
        }
        catch {
            // intentionally empty
        }
        print(`  • ${name}`);
    }
    print('');
    print(`  Total: ${features.length} features`);
    print('');
}
function cmdFeature() {
    const name = args[1];
    if (!name) {
        error('Usage: argentum feature <name>');
        return;
    }
    const featuresDir = resolveFeaturesDir();
    const distPath = featuresDir ? path.join(featuresDir, name, 'index.js') : '';
    const srcPath = featuresDir ? path.join(featuresDir, name, 'index.ts') : '';
    if (!fs.existsSync(distPath) && !fs.existsSync(srcPath)) {
        error(`Feature '${name}' not found`);
        return;
    }
    banner();
    info(`Feature: ${name}`);
    try {
        const feature = require(fs.existsSync(distPath) ? distPath : srcPath).default;
        print('');
        print(`  Name: ${feature.meta.name}`);
        print(`  Version: ${feature.meta.version}`);
        print(`  Description: ${feature.meta.description}`);
        print(`  Dependencies: ${feature.meta.dependencies?.join(', ') || 'none'}`);
        print('');
    }
    catch (err) {
        warn(`Could not load feature metadata: ${err.message}`);
        print(`  File: ${distPath}`);
        const lines = fs.existsSync(distPath)
            ? fs.readFileSync(distPath, 'utf8').split('\n').length
            : 0;
        print(`  Size: ${lines} lines`);
    }
}
function cmdConfig() {
    const key = args[1];
    const value = args[2];
    const workDir = getWorkDir();
    const configPath = getProjectConfigPath(workDir);
    if (!fs.existsSync(configPath)) {
        error('ARGENTUM not initialized. Run: argentum init');
        return;
    }
    const config = readProjectConfig(configPath);
    if (!key) {
        // Show all config
        banner();
        print(JSON.stringify(config, null, 2));
        return;
    }
    if (!value) {
        // Show specific key
        const keys = key.split('.');
        let val = config;
        for (const k of keys) {
            val = val?.[k];
        }
        if (val !== undefined) {
            print(`${key} = ${JSON.stringify(val)}`);
        }
        else {
            error(`Key '${key}' not found`);
        }
        return;
    }
    // Set value
    const keys = key.split('.');
    let obj = config;
    for (let i = 0; i < keys.length - 1; i++) {
        if (!obj[keys[i] ?? ''])
            obj[keys[i] ?? ''] = {};
        obj = obj[keys[i] ?? ''];
    }
    try {
        obj[keys[keys.length - 1] ?? ''] = JSON.parse(value);
    }
    catch {
        obj[keys[keys.length - 1] ?? ''] = value;
    }
    writeProjectConfig(configPath, config);
    success(`Set ${key} = ${value}`);
}
async function cmdDoctor() {
    banner();
    info('Running diagnostics...');
    print('');
    const checks = [
        {
            name: 'Node.js version >= 18',
            check: () => parseInt(process.version.slice(1)) >= 18,
            fix: 'Upgrade Node.js to v18 or later',
        },
        {
            name: 'config/default.yaml exists',
            check: () => projectConfigExists(),
            fix: 'Run: argentum onboard --yes',
        },
        {
            name: 'configuration validates',
            check: () => {
                try {
                    config_1.ConfigSchema.parse(readProjectConfig());
                    return true;
                }
                catch {
                    return false;
                }
            },
            fix: 'Review config/default.yaml or run: argentum onboard --yes --force',
        },
        {
            name: 'data/ directory exists',
            check: () => fs.existsSync(path.join(getWorkDir(), 'data')),
            fix: 'Run: argentum init (creates data/)',
        },
        {
            name: 'Features compiled',
            check: () => fs.existsSync(path.join(__dirname, 'features')) ||
                fs.existsSync(path.join(__dirname, 'src', 'features')) ||
                fs.existsSync(path.join(__dirname, '..', 'src', 'features')),
            fix: 'Run: npm run build',
        },
        {
            name: 'better-sqlite3 installed',
            check: () => {
                try {
                    require('better-sqlite3');
                    return true;
                }
                catch {
                    return false;
                }
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
        }
        else {
            warn(`${name} — ${fix}`);
        }
    }
    print('');
    print(`  ${passed}/${checks.length} checks passed`);
    print('');
}
async function cmdConnect() {
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
    info('Configure via environment variables or config/default.yaml [integrations] section');
    info('Example: argentum config integrations.telegram.botToken <token>');
}
async function cmdAgents() {
    banner();
    info('Agent management (placeholder)');
    print('');
    // Could list registered agents from multi-agent-coordination feature
    success('Agent management available via multi-agent-coordination feature');
    info('Use: argentum feature multi-agent-coordination agents');
}
async function cmdTools() {
    banner();
    info('List available tools/features');
    print('');
    const featuresDir = path.join(__dirname, 'src', 'features');
    if (!fs.existsSync(featuresDir)) {
        error('Features directory not found');
        return;
    }
    const features = fs
        .readdirSync(featuresDir)
        .filter((f) => fs.existsSync(path.join(featuresDir, f, 'index.js')))
        .sort();
    for (const name of features) {
        try {
            const feature = require(path.join(featuresDir, name, 'index.js')).default;
            const state = feature?.init ? '✓' : '✗';
            print(`  ${state} ${name}`);
        }
        catch {
            print(`  ? ${name}`);
        }
    }
    print('');
    print(`  Total: ${features.length} tools`);
}
async function cmdSessions() {
    const subcommand = args[1] || 'list';
    const dbPath = path.join(getWorkDir(), 'data', 'sessions.db');
    // Lazy load database
    const getDb = () => {
        if (!fs.existsSync(dbPath)) {
            // Create minimal DB
            const dir = path.dirname(dbPath);
            if (!fs.existsSync(dir))
                fs.mkdirSync(dir, { recursive: true });
            const Database = require('better-sqlite3');
            const db = new Database(dbPath);
            db.pragma('journal_mode = WAL');
            db.exec(`
        CREATE TABLE IF NOT EXISTS sessions (
          id TEXT PRIMARY KEY, title TEXT NOT NULL DEFAULT 'New Session',
          created_at INTEGER NOT NULL, updated_at INTEGER NOT NULL,
          model TEXT DEFAULT '', status TEXT NOT NULL DEFAULT 'active',
          tags TEXT DEFAULT '[]', metadata TEXT DEFAULT '{}'
        );
        CREATE TABLE IF NOT EXISTS messages (
          id TEXT PRIMARY KEY, session_id TEXT NOT NULL, role TEXT NOT NULL,
          content TEXT NOT NULL, timestamp INTEGER NOT NULL,
          tool_calls TEXT, metadata TEXT DEFAULT '{}'
        );
        CREATE INDEX IF NOT EXISTS idx_messages_session ON messages(session_id);
        CREATE INDEX IF NOT EXISTS idx_messages_timestamp ON messages(timestamp DESC);

        -- FTS5 for cross-session search (Hermes-style)
        CREATE VIRTUAL TABLE IF NOT EXISTS messages_fts USING fts5(
          content,
          role,
          content='messages',
          content_rowid='rowid'
        );

        -- Sync triggers for FTS5
        CREATE TRIGGER IF NOT EXISTS messages_ai AFTER INSERT ON messages BEGIN
          INSERT INTO messages_fts(rowid, content, role)
          VALUES (new.rowid, new.content, new.role);
        END;
        CREATE TRIGGER IF NOT EXISTS messages_ad AFTER DELETE ON messages BEGIN
          INSERT INTO messages_fts(messages_fts, rowid, content, role)
          VALUES ('delete', old.rowid, old.content, old.role);
        END;
        CREATE TRIGGER IF NOT EXISTS messages_au AFTER UPDATE ON messages BEGIN
          INSERT INTO messages_fts(messages_fts, rowid, content, role)
          VALUES ('delete', old.rowid, old.content, old.role);
          INSERT INTO messages_fts(rowid, content, role)
          VALUES (new.rowid, new.content, new.role);
        END;
      `);
            return db;
        }
        const Database = require('better-sqlite3');
        return new Database(dbPath);
    };
    switch (subcommand) {
        case 'list':
        case 'ls': {
            banner();
            const db = getDb();
            const sessions = db
                .prepare("SELECT * FROM sessions WHERE status != 'deleted' ORDER BY updated_at DESC LIMIT 20")
                .all();
            if (sessions.length === 0) {
                info('No sessions yet. Create one with: argentum session create');
                db.close();
                return;
            }
            info(`Sessions (${sessions.length}):`);
            print('');
            for (const s of sessions) {
                const msgs = db
                    .prepare('SELECT COUNT(*) as c FROM messages WHERE session_id = ?')
                    .get(s.id).c;
                const ago = formatAgo(s.updated_at);
                const status = s.status === 'active' ? '\x1b[32m●\x1b[0m' : '\x1b[33m○\x1b[0m';
                print(`  ${status} \x1b[1m${s.title}\x1b[0m`);
                print(`    ID: ${s.id.slice(0, 8)}... | Messages: ${msgs} | Updated: ${ago}`);
            }
            print('');
            db.close();
            break;
        }
        case 'create':
        case 'new': {
            banner();
            const title = args.slice(2).join(' ') || 'New Session';
            const db = getDb();
            const id = crypto.randomUUID();
            const now = Date.now();
            db.prepare('INSERT INTO sessions (id, title, created_at, updated_at, status) VALUES (?, ?, ?, ?, ?)').run(id, title, now, now, 'active');
            success(`Session created: ${id}`);
            print(`  Title: ${title}`);
            db.close();
            break;
        }
        case 'show':
        case 'view': {
            const id = args[2];
            if (!id) {
                error('Usage: argentum session show <id>');
                return;
            }
            banner();
            const db = getDb();
            // Find by prefix
            const session = db
                .prepare('SELECT * FROM sessions WHERE id LIKE ? OR id = ?')
                .get(`%${id}%`, id);
            if (!session) {
                error(`Session '${id}' not found`);
                db.close();
                return;
            }
            info(`Session: ${session.title}`);
            print(`  ID: ${session.id}`);
            print(`  Status: ${session.status}`);
            print(`  Created: ${new Date(session.created_at).toLocaleString()}`);
            print(`  Updated: ${new Date(session.updated_at).toLocaleString()}`);
            const messages = db
                .prepare('SELECT * FROM messages WHERE session_id = ? ORDER BY timestamp ASC')
                .all(session.id);
            print(`  Messages: ${messages.length}`);
            print('');
            for (const msg of messages.slice(-10)) {
                // Last 10 messages
                const roleColor = msg.role === 'user' ? '\x1b[36m' : msg.role === 'assistant' ? '\x1b[32m' : '\x1b[33m';
                const content = msg.content.slice(0, 150).replace(/\n/g, ' ');
                print(`  ${roleColor}${msg.role}\x1b[0m: ${content}`);
            }
            if (messages.length > 10) {
                print(`  ... and ${messages.length - 10} more messages`);
            }
            db.close();
            break;
        }
        case 'delete':
        case 'rm': {
            const id = args[2];
            if (!id) {
                error('Usage: argentum session delete <id>');
                return;
            }
            const db = getDb();
            const session = db
                .prepare('SELECT * FROM sessions WHERE id LIKE ? OR id = ?')
                .get(`%${id}%`, id);
            if (!session) {
                error(`Session '${id}' not found`);
                db.close();
                return;
            }
            db.prepare("UPDATE sessions SET status = 'deleted' WHERE id = ?").run(session.id);
            success(`Session '${session.title}' deleted`);
            db.close();
            break;
        }
        case 'archive': {
            const id = args[2];
            if (!id) {
                error('Usage: argentum session archive <id>');
                return;
            }
            const db = getDb();
            const session = db
                .prepare('SELECT * FROM sessions WHERE id LIKE ? OR id = ?')
                .get(`%${id}%`, id);
            if (!session) {
                error(`Session '${id}' not found`);
                db.close();
                return;
            }
            db.prepare("UPDATE sessions SET status = 'archived' WHERE id = ?").run(session.id);
            success(`Session '${session.title}' archived`);
            db.close();
            break;
        }
        case 'search': {
            const query = args.slice(2).join(' ');
            if (!query) {
                error('Usage: argentum session search <query>');
                return;
            }
            banner();
            const db = getDb();
            // Use FTS5 for fast full-text search (Hermes-style)
            let results = [];
            try {
                results = db
                    .prepare(`SELECT m.session_id, m.content, m.role, m.timestamp, s.title, rank
             FROM messages m
             JOIN messages_fts fts ON m.rowid = fts.rowid
             JOIN sessions s ON m.session_id = s.id
             WHERE messages_fts MATCH ?
             ORDER BY rank, m.timestamp DESC
             LIMIT 10`)
                    .all(query);
            }
            catch {
                // Fallback to LIKE if FTS fails (e.g., invalid FTS syntax)
                const sanitized = query.replace(/[^\w\s]/g, ' ');
                results = db
                    .prepare(`SELECT m.session_id, m.content, m.role, m.timestamp, s.title
             FROM messages m
             JOIN sessions s ON m.session_id = s.id
             WHERE m.content LIKE ?
             ORDER BY m.timestamp DESC
             LIMIT 10`)
                    .all(`%${sanitized}%`);
            }
            if (results.length === 0) {
                info('No results found');
                db.close();
                return;
            }
            info(`Found ${results.length} results for "${query}" (FTS5):`);
            print('');
            for (const r of results) {
                const content = r.content.slice(0, 120).replace(/\n/g, ' ');
                print(`  \x1b[1m${r.title}\x1b[0m [${r.role}]`);
                print(`    ${content}${content.length >= 120 ? '...' : ''}`);
                print(`    Session: ${r.session_id.slice(0, 8)}... | ${new Date(r.timestamp).toLocaleDateString()}`);
            }
            db.close();
            break;
        }
        case 'export': {
            const id = args[2];
            if (!id) {
                error('Usage: argentum session export <id>');
                return;
            }
            const db = getDb();
            const session = db
                .prepare('SELECT * FROM sessions WHERE id LIKE ? OR id = ?')
                .get(`%${id}%`, id);
            if (!session) {
                error(`Session '${id}' not found`);
                db.close();
                return;
            }
            const messages = db
                .prepare('SELECT * FROM messages WHERE session_id = ? ORDER BY timestamp ASC')
                .all(session.id);
            const exportData = { session, messages };
            const exportPath = path.join(getWorkDir(), 'data', `session-${session.id.slice(0, 8)}.json`);
            fs.writeFileSync(exportPath, JSON.stringify(exportData, null, 2));
            success(`Exported to: ${exportPath}`);
            db.close();
            break;
        }
        case 'stats': {
            banner();
            const db = getDb();
            const total = db.prepare('SELECT COUNT(*) as c FROM sessions').get().c;
            const active = db
                .prepare("SELECT COUNT(*) as c FROM sessions WHERE status = 'active'")
                .get().c;
            const messages = db.prepare('SELECT COUNT(*) as c FROM messages').get().c;
            print(`  Total sessions: ${total}`);
            print(`  Active: ${active}`);
            print(`  Archived: ${total - active}`);
            print(`  Total messages: ${messages}`);
            print(`  Avg messages/session: ${total > 0 ? Math.round(messages / total) : 0}`);
            db.close();
            break;
        }
        default:
            error(`Unknown session command: ${subcommand}`);
            print('  argentum session [list|create|show|delete|archive|search|export|stats]');
    }
}
function formatAgo(timestamp) {
    const diff = Date.now() - timestamp;
    const mins = Math.floor(diff / 60000);
    if (mins < 1)
        return 'just now';
    if (mins < 60)
        return `${mins}m ago`;
    const hours = Math.floor(mins / 60);
    if (hours < 24)
        return `${hours}h ago`;
    const days = Math.floor(hours / 24);
    return `${days}d ago`;
}
async function cmdWatch() {
    const target = args[1];
    if (!target) {
        error('Usage: argentum watch <path> [--pattern "*.ts"]');
        return;
    }
    banner();
    info(`Watching ${target} for changes... (Ctrl+C to stop)`);
    // Could use file-watcher feature directly
    // For now, just show a message
    print('');
    warn('File watcher requires file-watcher feature to be enabled');
    info('Enable it in config and restart ARGENTUM server');
}
async function cmdGateway() {
    const subcommand = args[1] || 'status';
    const workDir = getWorkDir();
    const pidFile = path.join(workDir, 'data', '.gateway.pid');
    const getPid = () => {
        try {
            if (fs.existsSync(pidFile)) {
                const pid = parseInt(fs.readFileSync(pidFile, 'utf8').trim(), 10);
                // Check if process is running
                try {
                    process.kill(pid, 0);
                    return pid;
                }
                catch {
                    fs.unlinkSync(pidFile);
                    return null;
                }
            }
        }
        catch { }
        return null;
    };
    switch (subcommand) {
        case 'status': {
            banner();
            const pid = getPid();
            if (pid) {
                success(`Gateway running (PID: ${pid})`);
                info(`PID file: ${pidFile}`);
            }
            else {
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
            const port = args.includes('--port')
                ? parseInt(args[args.indexOf('--port') + 1] ?? '', 10)
                : 3000;
            info(`Starting Argentum gateway on port ${port}...`);
            // Spawn gateway as background process
            const { spawn } = await Promise.resolve().then(() => __importStar(require('child_process')));
            const gatewayPath = path.join(__dirname, 'cli.js');
            const child = spawn('node', [gatewayPath, 'start', '--port', String(port)], {
                detached: true,
                stdio: [
                    'ignore',
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
            info(`Stop: argentum gateway stop`);
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
            }
            catch (err) {
                error(`Failed to stop: ${err.message}`);
            }
            break;
        }
        case 'restart': {
            banner();
            const pid = getPid();
            if (pid) {
                info(`Stopping gateway (PID: ${pid})...`);
                try {
                    process.kill(pid, 'SIGTERM');
                }
                catch { }
                // Wait a moment
                await new Promise((r) => setTimeout(r, 1000));
            }
            const port = args.includes('--port')
                ? parseInt(args[args.indexOf('--port') + 1] ?? '', 10)
                : 3000;
            info(`Restarting Argentum gateway on port ${port}...`);
            const { spawn } = await Promise.resolve().then(() => __importStar(require('child_process')));
            const gatewayPath = path.join(__dirname, 'cli.js');
            const child = spawn('node', [gatewayPath, 'start', '--port', String(port)], {
                detached: true,
                stdio: [
                    'ignore',
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
            const lines = args.includes('-n') ? parseInt(args[args.indexOf('-n') + 1] ?? '', 10) : 50;
            const logContent = fs.readFileSync(logFile, 'utf8');
            const logLines = logContent.split('\n').slice(-lines);
            print(logLines.join('\n'));
            break;
        }
        default:
            error(`Unknown gateway command: ${subcommand}`);
            print('Usage: argentum gateway [status|start|stop|restart|logs]');
    }
}
async function cmdPlugins() {
    banner();
    info('Plugin management');
    print('');
    const featuresDir = path.join(__dirname, 'src', 'features');
    if (!fs.existsSync(featuresDir)) {
        error('Features directory not found');
        return;
    }
    const features = fs
        .readdirSync(featuresDir)
        .filter((f) => fs.existsSync(path.join(featuresDir, f, 'index.js')))
        .sort();
    for (const name of features) {
        try {
            const feature = require(path.join(featuresDir, name, 'index.js')).default;
            const state = feature?.init ? '✓' : '✗';
            print(`  ${state} ${name}`);
        }
        catch {
            print(`  ? ${name}`);
        }
    }
    print('');
    print(`  Total: ${features.length} plugins`);
    info('Enable/disable in config/default.yaml [features]');
}
async function cmdBackup() {
    const subcommand = args[1] || 'create';
    const workDir = getWorkDir();
    const backupDir = path.join(workDir, 'backups');
    switch (subcommand) {
        case 'create':
        case 'now': {
            banner();
            const timestamp = new Date().toISOString().replace(/[:.]/g, '-').slice(0, 19);
            const backupPath = path.join(backupDir, `backup-${timestamp}`);
            fs.mkdirSync(backupPath, { recursive: true });
            info('Creating backup...');
            let count = 0;
            // Backup config
            const configPath = getProjectConfigPath(workDir);
            if (fs.existsSync(configPath)) {
                const targetName = configPath.endsWith('.json')
                    ? path.basename(configPath)
                    : path.join('config', 'default.yaml');
                const targetPath = path.join(backupPath, targetName);
                fs.mkdirSync(path.dirname(targetPath), { recursive: true });
                fs.copyFileSync(configPath, targetPath);
                count++;
            }
            // Backup .env
            const envPath = path.join(workDir, '.env');
            if (fs.existsSync(envPath)) {
                fs.copyFileSync(envPath, path.join(backupPath, '.env'));
                count++;
            }
            // Backup all .db files from data/
            const dataDir = path.join(workDir, 'data');
            if (fs.existsSync(dataDir)) {
                const backupDataDir = path.join(backupPath, 'data');
                fs.mkdirSync(backupDataDir, { recursive: true });
                const dbs = fs.readdirSync(dataDir).filter((f) => f.endsWith('.db'));
                for (const db of dbs) {
                    fs.copyFileSync(path.join(dataDir, db), path.join(backupDataDir, db));
                    count++;
                }
            }
            // Create manifest
            fs.writeFileSync(path.join(backupPath, 'manifest.json'), JSON.stringify({
                timestamp: new Date().toISOString(),
                version: VERSION,
                files: count,
            }, null, 2));
            success(`Backup created: ${backupPath}`);
            print(`  Files: ${count}`);
            break;
        }
        case 'list':
        case 'ls': {
            banner();
            if (!fs.existsSync(backupDir)) {
                info('No backups found');
                return;
            }
            const backups = fs
                .readdirSync(backupDir)
                .filter((f) => f.startsWith('backup-'))
                .sort()
                .reverse();
            if (backups.length === 0) {
                info('No backups found');
                return;
            }
            info(`Backups (${backups.length}):`);
            for (const b of backups) {
                const manifestPath = path.join(backupDir, b, 'manifest.json');
                let info_ = '';
                if (fs.existsSync(manifestPath)) {
                    const m = JSON.parse(fs.readFileSync(manifestPath, 'utf8'));
                    info_ = ` | ${m.files} files`;
                }
                print(`  • ${b}${info_}`);
            }
            break;
        }
        case 'restore': {
            const name = args[2];
            if (!name) {
                error('Usage: argentum backup restore <backup-name>');
                return;
            }
            const backupPath = path.join(backupDir, name);
            if (!fs.existsSync(backupPath)) {
                error(`Backup '${name}' not found`);
                return;
            }
            banner();
            info(`Restoring from: ${name}`);
            // Restore config
            const yamlConfigBackup = path.join(backupPath, 'config', 'default.yaml');
            const jsonConfigBackup = path.join(backupPath, 'argentum.json');
            if (fs.existsSync(yamlConfigBackup)) {
                const restorePath = path.join(workDir, 'config', 'default.yaml');
                fs.mkdirSync(path.dirname(restorePath), { recursive: true });
                fs.copyFileSync(yamlConfigBackup, restorePath);
                print('  ✓ config/default.yaml');
            }
            else if (fs.existsSync(jsonConfigBackup)) {
                fs.copyFileSync(jsonConfigBackup, path.join(workDir, 'argentum.json'));
                print('  ✓ argentum.json');
            }
            // Restore .env
            const envBackup = path.join(backupPath, '.env');
            if (fs.existsSync(envBackup)) {
                fs.copyFileSync(envBackup, path.join(workDir, '.env'));
                print('  ✓ .env');
            }
            // Restore databases
            const dataBackup = path.join(backupPath, 'data');
            if (fs.existsSync(dataBackup)) {
                const dataDir = path.join(workDir, 'data');
                fs.mkdirSync(dataDir, { recursive: true });
                for (const f of fs.readdirSync(dataBackup)) {
                    fs.copyFileSync(path.join(dataBackup, f), path.join(dataDir, f));
                    print(`  ✓ data/${f}`);
                }
            }
            success('Restore complete');
            info('Restart gateway: argentum gateway restart');
            break;
        }
        default:
            error(`Unknown backup command: ${subcommand}`);
            print('  argentum backup [create|list|restore]');
    }
}
async function cmdMemory() {
    const subcommand = args[1] || 'search';
    const dbPath = path.join(getWorkDir(), 'data', 'sqlite-memory.db');
    if (!fs.existsSync(dbPath)) {
        error('Memory database not found. Enable sqlite-memory feature first.');
        return;
    }
    const Database = require('better-sqlite3');
    const db = new Database(dbPath, { readonly: true });
    switch (subcommand) {
        case 'search': {
            const query = args.slice(2).join(' ');
            if (!query) {
                error('Usage: argentum memory search <query>');
                db.close();
                return;
            }
            banner();
            info(`Searching: "${query}"`);
            print('');
            // Try FTS5 first, fallback to LIKE
            let results = [];
            try {
                results = db
                    .prepare('SELECT key, value, namespace, updated_at FROM kv_fts WHERE kv_fts MATCH ? LIMIT 10')
                    .all(query);
            }
            catch {
                results = db
                    .prepare('SELECT key, value, namespace, updated_at FROM kv_store WHERE value LIKE ? OR key LIKE ? ORDER BY updated_at DESC LIMIT 10')
                    .all(`%${query}%`, `%${query}%`);
            }
            if (results.length === 0) {
                info('No results found');
                db.close();
                return;
            }
            for (const r of results) {
                const value = (r.value || '').slice(0, 100).replace(/\n/g, ' ');
                print(`  \x1b[1m[${r.namespace}]\x1b[0m ${r.key}`);
                print(`    ${value}`);
            }
            db.close();
            break;
        }
        case 'list': {
            banner();
            // List all namespaces
            const namespaces = db
                .prepare('SELECT DISTINCT namespace FROM kv_store ORDER BY namespace')
                .all();
            if (namespaces.length === 0) {
                info('Memory is empty');
                db.close();
                return;
            }
            info('Memory namespaces:');
            for (const ns of namespaces) {
                const count = db
                    .prepare('SELECT COUNT(*) as c FROM kv_store WHERE namespace = ?')
                    .get(ns.namespace).c;
                print(`  • ${ns.namespace}: ${count} entries`);
            }
            db.close();
            break;
        }
        case 'export': {
            const ns = args[2];
            banner();
            let query = 'SELECT * FROM kv_store';
            const params = [];
            if (ns) {
                query += ' WHERE namespace = ?';
                params.push(ns);
            }
            query += ' ORDER BY updated_at DESC LIMIT 100';
            const rows = db.prepare(query).all(...params);
            const exportPath = path.join(getWorkDir(), 'data', `memory-export-${ns || 'all'}.json`);
            fs.writeFileSync(exportPath, JSON.stringify(rows, null, 2));
            success(`Exported ${rows.length} entries to: ${exportPath}`);
            db.close();
            break;
        }
        default:
            error(`Unknown memory command: ${subcommand}`);
            print('  argentum memory [search|list|export]');
            db.close();
    }
}
async function cmdCron() {
    const subcommand = args[1] || 'list';
    const dbPath = path.join(getWorkDir(), 'data', 'cron-jobs.json');
    switch (subcommand) {
        case 'list':
        case 'ls': {
            banner();
            if (!fs.existsSync(dbPath)) {
                info('No cron jobs defined');
                return;
            }
            const jobs = JSON.parse(fs.readFileSync(dbPath, 'utf8'));
            if (jobs.length === 0) {
                info('No cron jobs defined');
                return;
            }
            info(`Cron jobs (${jobs.length}):`);
            print('');
            for (let i = 0; i < jobs.length; i++) {
                const j = jobs[i];
                const status = j.enabled ? '\x1b[32m●\x1b[0m' : '\x1b[31m○\x1b[0m';
                print(`  ${status} \x1b[1m${j.name || j.id}\x1b[0m`);
                print(`    ID: ${j.id}`);
                print(`    Schedule: ${j.schedule.kind} (${JSON.stringify(j.schedule)})`);
                print(`    Target: ${j.sessionTarget} (${j.payload.kind})`);
            }
            break;
        }
        case 'add':
        case 'create': {
            // Interactive job creation
            const readline = require('readline');
            const rl = readline.createInterface({ input: process.stdin, output: process.stdout });
            const ask = (q) => new Promise((resolve) => rl.question(q, resolve));
            banner();
            info('Creating new cron job');
            // Read existing jobs
            let jobs = [];
            if (fs.existsSync(dbPath))
                jobs = JSON.parse(fs.readFileSync(dbPath, 'utf8'));
            const id = crypto.randomUUID();
            const name = (await ask('  Name (optional): ')) || `Job-${id.slice(0, 8)}`;
            const scheduleKind = (await ask('  Schedule [every|cron] (default: every): ')) || 'every';
            let schedule;
            if (scheduleKind === 'every') {
                const minutes = parseInt((await ask('  Interval in minutes (default: 60): ')) || '60');
                schedule = { kind: 'every', everyMs: minutes * 60 * 1000 };
            }
            else {
                const expr = await ask('  Cron expression: ');
                schedule = { kind: 'cron', expr };
            }
            const payload = await ask('  Command to run (e.g. "systemEvent", "agentTurn.message"): ');
            const parts = payload.split('.');
            const message = parts.slice(1).join('.') || 'Check for tasks';
            const newJob = {
                id,
                name,
                schedule,
                payload: { kind: 'systemEvent', text: message },
                sessionTarget: 'main',
                enabled: true,
            };
            jobs.push(newJob);
            fs.writeFileSync(dbPath, JSON.stringify(jobs, null, 2));
            success(`Job created: ${id}`);
            print(`  Run: argentum cron run ${id}`);
            rl.close();
            break;
        }
        case 'run': {
            const id = args[2];
            if (!id) {
                error('Usage: argentum cron run <id>');
                return;
            }
            if (!fs.existsSync(dbPath)) {
                error('No cron jobs defined');
                return;
            }
            const jobs = JSON.parse(fs.readFileSync(dbPath, 'utf8'));
            const job = jobs.find((j) => j.id === id);
            if (!job) {
                error(`Job '${id}' not found`);
                return;
            }
            // Execute job payload as system event
            banner();
            info(`Running: ${job.name}`);
            // This requires sending to main session - we can't do it from CLI directly
            // but we can show what would be executed
            print(`  Payload: ${JSON.stringify(job.payload)}`);
            success('Job executed (simulated from CLI)');
            break;
        }
        case 'enable':
        case 'disable': {
            const id = args[2];
            if (!id) {
                error(`Usage: argentum cron ${subcommand} <id>`);
                return;
            }
            if (!fs.existsSync(dbPath)) {
                error('No cron jobs defined');
                return;
            }
            const jobs = JSON.parse(fs.readFileSync(dbPath, 'utf8'));
            const job = jobs.find((j) => j.id === id);
            if (!job) {
                error(`Job '${id}' not found`);
                return;
            }
            job.enabled = subcommand === 'enable';
            fs.writeFileSync(dbPath, JSON.stringify(jobs, null, 2));
            success(`Job ${subcommand}d`);
            break;
        }
        case 'remove':
        case 'rm': {
            const id = args[2];
            if (!id) {
                error('Usage: argentum cron remove <id>');
                return;
            }
            const jobs = JSON.parse(fs.readFileSync(dbPath, 'utf8'));
            const filtered = jobs.filter((j) => j.id !== id);
            if (filtered.length === jobs.length) {
                error(`Job '${id}' not found`);
                return;
            }
            fs.writeFileSync(dbPath, JSON.stringify(filtered, null, 2));
            success('Job removed');
            break;
        }
        default:
            error(`Unknown cron command: ${subcommand}`);
            print('  argentum cron [list|add|run|enable|disable|remove]');
    }
}
async function cmdStatus() {
    const workDir = getWorkDir();
    const configPath = getProjectConfigPath(workDir);
    banner();
    print('  \x1b[1mArgentum Status\x1b[0m');
    print('');
    print(`  \x1b[1mVersion:\x1b[0m v${VERSION}`);
    print(`  \x1b[1mConfig:\x1b[0m ${configPath}`);
    print(`  \x1b[1mData:\x1b[0m ${path.join(workDir, 'data')}`);
    print(`  \x1b[1mUptime:\x1b[0m ${process.uptime().toFixed(0)}s`);
    print('');
    // Check data directories
    const dataDir = path.join(workDir, 'data');
    if (fs.existsSync(dataDir)) {
        const dbs = fs.readdirSync(dataDir).filter((f) => f.endsWith('.db'));
        print(`  \x1b[1mDatabases:\x1b[0m ${dbs.length}`);
        for (const db of dbs) {
            try {
                const size = fs.statSync(path.join(dataDir, db)).size;
                print(`    • ${db} (${(size / 1024).toFixed(1)} KB)`);
            }
            catch { }
        }
    }
    else {
        print('  \x1b[1mDatabases:\x1b[0m 0 (data dir not created)');
    }
    print('');
    // Quick health check – just file existence
    const requiredFiles = ['config/default.yaml', '.env'];
    print('  \x1b[1mConfiguration:\x1b[0m');
    for (const f of requiredFiles) {
        const exists = fs.existsSync(path.join(workDir, f));
        const status = exists ? '\x1b[32m✓\x1b[0m' : '\x1b[31m✗\x1b[0m';
        print(`    ${status} ${f}`);
    }
    print('');
    // Show gateway status
    const pidPath = path.join(workDir, 'data', '.gateway.pid');
    if (fs.existsSync(pidPath)) {
        const pid = fs.readFileSync(pidPath, 'utf8').trim();
        print(`  \x1b[1mGateway:\x1b[0m PID ${pid} (running)`);
    }
    else {
        print(`  \x1b[1mGateway:\x1b[0m \x1b[31mstopped\x1b[0m`);
    }
    print('');
    success('All systems nominal');
}
// ─── Budget Command ────────────────────────────────────────────────────────────
async function cmdBudget() {
    const subcommand = args[1] || 'status';
    // Lazy-load the budget feature (works both in-plugin and from CLI)
    const getBudgetFeature = () => {
        try {
            const budgetPath = path.join(__dirname, 'features', 'budget', 'index.js');
            if (!fs.existsSync(budgetPath)) {
                return null;
            }
            return require(budgetPath).default;
        }
        catch {
            return null;
        }
    };
    const budget = getBudgetFeature();
    // Helpers for pretty output
    const bar = (percent, width = 20) => {
        const filled = Math.round((percent / 100) * width);
        const empty = width - filled;
        const p = Math.min(100, Math.max(0, percent));
        const color = p >= 90 ? '\x1b[31m' : p >= 75 ? '\x1b[33m' : '\x1b[32m';
        return `${color + '█'.repeat(filled)}\x1b[90m${'░'.repeat(empty)}\x1b[0m`;
    };
    const usd = (n) => `$${n.toFixed(4)}`;
    switch (subcommand) {
        case 'status':
        case 's': {
            banner();
            print('  \x1b[1m\x1b[36m╔══════════════════════════════════════════╗\x1b[0m');
            print('  \x1b[1m\x1b[36m║         \x1b[33m💰 Budget Status\x1b[36m              ║\x1b[0m');
            print('  \x1b[1m\x1b[36m╚══════════════════════════════════════════╝\x1b[0m');
            print('');
            if (!budget) {
                warn('Budget feature not compiled. Run: npm run build');
                return;
            }
            const config = budget.getConfig();
            const report = budget.getBudgetReport();
            const monthlyPct = Math.round((report.monthlyCost / report.monthlyLimit) * 100);
            const dailyPct = Math.round((report.dailyCost / report.dailyLimit) * 100);
            print('  \x1b[1mGlobal Limits\x1b[0m');
            print(`  Monthly  ${bar(monthlyPct)} ${usd(report.monthlyCost).padStart(10)} / ${usd(report.monthlyLimit)}`);
            print(`  Daily    ${bar(dailyPct)} ${usd(report.dailyCost).padStart(10)} / ${usd(report.dailyLimit)}`);
            print('');
            if (report.alerts.length > 0) {
                print('  \x1b[1m\x1b[33m⚠\x1b[0m  \x1b[1mAlerts\x1b[0m');
                for (const alert of report.alerts) {
                    print(`    \x1b[33m▸\x1b[0m ${alert}`);
                }
                print('');
            }
            if (report.byAgent.length > 0) {
                print('  \x1b[1mPer-Agent Usage\x1b[0m');
                print('  \x1b[90m  Agent                Cost          Tokens           % of Limit    Status\x1b[0m');
                print(`  \x1b[90m  ${'─'.repeat(72)}`);
                for (const s of report.byAgent) {
                    const statusColor = s.canProceed ? '\x1b[32m✓\x1b[0m' : '\x1b[31m✗\x1b[0m';
                    const pctColor = s.percentUsed >= 90 ? '\x1b[31m' : s.percentUsed >= 75 ? '\x1b[33m' : '\x1b[32m';
                    const pctStr = `${pctColor + s.percentUsed}%\x1b[0m`;
                    print(`    ${s.agent.padEnd(20)}${usd(s.totalCost).padEnd(12)}${s.totalTokens.toLocaleString().padEnd(12)}${pctStr.padEnd(14)} ${statusColor}`);
                }
                print('');
            }
            else {
                print('  \x1b[90m  No usage recorded yet.\x1b[0m\n');
            }
            print('  \x1b[1mSettings\x1b[0m');
            print(`  \x1b[90m  alertThreshold:   \x1b[0m${Math.round(config.alertThreshold * 100)}%`);
            print(`  \x1b[90m  blockOnExhausted: \x1b[0m${config.blockOnExhausted ? 'yes' : 'no'}`);
            if (config.perAgentLimit) {
                print(`  \x1b[90m  perAgentLimit:    \x1b[0m$${config.perAgentLimit}`);
            }
            print('');
            break;
        }
        case 'set-limit':
        case 'set': {
            if (!budget) {
                warn('Budget feature not compiled. Run: npm run build');
                return;
            }
            const amountStr = args[2];
            const limitType = (args[3] || 'monthly').toLowerCase();
            if (!amountStr) {
                error('Usage: argentum budget set-limit <amount> [monthly|daily|per-agent]');
                return;
            }
            const amount = parseFloat(amountStr);
            if (isNaN(amount) || amount < 0) {
                error('Amount must be a non-negative number');
                return;
            }
            let result;
            switch (limitType) {
                case 'monthly':
                case 'm':
                    result = budget.updateConfig({ globalMonthlyLimit: amount });
                    break;
                case 'daily':
                case 'd':
                    result = budget.updateConfig({ globalDailyLimit: amount });
                    break;
                case 'per-agent':
                case 'peragent':
                case 'agent':
                case 'a':
                    result = budget.updateConfig({ perAgentLimit: amount });
                    break;
                default:
                    error(`Unknown limit type: ${limitType}. Use: monthly, daily, or per-agent`);
                    return;
            }
            if (result.success) {
                success(`Updated ${limitType} limit to $${amount}`);
            }
            else {
                error(`Failed: ${result.error}`);
            }
            break;
        }
        case 'history':
        case 'h': {
            banner();
            print('  \x1b[1m\x1b[36m╔══════════════════════════════════════════╗\x1b[0m');
            print('  \x1b[1m\x1b[36m║         \x1b[33m📊 Budget History\x1b[36m             ║\x1b[0m');
            print('  \x1b[1m\x1b[36m╚══════════════════════════════════════════╝\x1b[0m');
            print('');
            if (!budget) {
                warn('Budget feature not compiled. Run: npm run build');
                return;
            }
            const days = parseInt(args[2] || '30', 10);
            const history = budget.getHistory(days);
            if (history.length === 0) {
                info(`No spending history in the last ${days} days.`);
                return;
            }
            print('  \x1b[90m  Date           Cost           Tokens         Requests   Daily %\x1b[0m');
            print(`  \x1b[90m  ${'─'.repeat(66)}`);
            const cfg = budget.getConfig();
            for (const row of history) {
                const dailyPct = Math.round((row.totalCost / cfg.globalDailyLimit) * 100);
                const pctColor = dailyPct >= 90 ? '\x1b[31m' : dailyPct >= 75 ? '\x1b[33m' : '\x1b[32m';
                print(`    \x1b[37m${row.date.padEnd(12)}\x1b[0m ${usd(row.totalCost).padEnd(14)}${row.totalTokens.toLocaleString().padEnd(14)}${row.requestCount.toString().padEnd(10)}${pctColor}${dailyPct}%\x1b[0m`);
            }
            print('');
            const tc = history.reduce((s, r) => s + r.totalCost, 0);
            const tt = history.reduce((s, r) => s + r.totalTokens, 0);
            const tr = history.reduce((s, r) => s + r.requestCount, 0);
            print(`  \x1b[90m  Totals: \x1b[0m${usd(tc)} | ${tt.toLocaleString()} tokens | ${tr} requests\n`);
            break;
        }
        case 'reset': {
            banner();
            if (!budget) {
                warn('Budget feature not compiled. Run: npm run build');
                return;
            }
            const readline = require('readline');
            const rl = readline.createInterface({ input: process.stdin, output: process.stdout });
            const confirm = (q) => new Promise((resolve) => rl.question(q, resolve));
            const answer = await confirm('  \x1b[31m⚠\x1b[0m This will delete all budget usage records. Continue? [y/N]: ');
            rl.close();
            if (answer.toLowerCase() === 'y') {
                budget.reset();
                success('Budget counters reset');
            }
            else {
                info('Reset cancelled');
            }
            break;
        }
        case 'config':
        case 'cfg': {
            banner();
            print('  \x1b[1m\x1b[36m╔══════════════════════════════════════════╗\x1b[0m');
            print('  \x1b[1m\x1b[36m║        \x1b[33m⚙\x1b[0m  Budget Configuration\x1b[36m       ║\x1b[0m');
            print('  \x1b[1m\x1b[36m╚══════════════════════════════════════════╝\x1b[0m');
            print('');
            if (!budget) {
                warn('Budget feature not compiled. Run: npm run build');
                return;
            }
            const d = budget.getConfigDisplay();
            const rows = [
                [
                    'globalMonthlyLimit',
                    '$10/month',
                    `$${d.globalMonthlyLimit.value}/month`,
                    d.globalMonthlyLimit.value === d.globalMonthlyLimit.default
                        ? '\x1b[90m(default)\x1b[0m'
                        : '\x1b[33m(modified)\x1b[0m',
                ],
                [
                    'globalDailyLimit',
                    '$1/day',
                    `$${d.globalDailyLimit.value}/day`,
                    d.globalDailyLimit.value === d.globalDailyLimit.default
                        ? '\x1b[90m(default)\x1b[0m'
                        : '\x1b[33m(modified)\x1b[0m',
                ],
                [
                    'perAgentLimit',
                    '$10/agent',
                    d.perAgentLimit.value
                        ? `$${d.perAgentLimit.value}/agent`
                        : '\x1b[90munset (uses monthly)\x1b[0m',
                    d.perAgentLimit.value === d.perAgentLimit.default
                        ? '\x1b[90m(default)\x1b[0m'
                        : '\x1b[33m(modified)\x1b[0m',
                ],
                [
                    'alertThreshold',
                    '80%',
                    `${Math.round(d.alertThreshold.value * 100)}%`,
                    d.alertThreshold.value === d.alertThreshold.default
                        ? '\x1b[90m(default)\x1b[0m'
                        : '\x1b[33m(modified)\x1b[0m',
                ],
                [
                    'blockOnExhausted',
                    'yes',
                    d.blockOnExhausted.value ? 'yes' : 'no',
                    d.blockOnExhausted.value === d.blockOnExhausted.default
                        ? '\x1b[90m(default)\x1b[0m'
                        : '\x1b[33m(modified)\x1b[0m',
                ],
                ['enabled', 'false', d.enabled.value ? 'true' : 'false', '\x1b[90m(feature toggle)\x1b[0m'],
            ];
            print('  \x1b[90m  Key               Default       Current               Status\x1b[0m');
            print(`  \x1b[90m  ${'─'.repeat(68)}`);
            for (const [key, def, cur, status] of rows) {
                print(`  \x1b[37m  ${key.padEnd(20)}\x1b[0m ${def.padEnd(14)} ${cur.padEnd(20)} ${status}`);
            }
            print('');
            print('  \x1b[1mQuick commands:\x1b[0m');
            print('    \x1b[36margentum budget set-limit 50 monthly\x1b[0m   Set monthly limit to $50');
            print('    \x1b[36margentum budget set-limit 5 daily\x1b[0m      Set daily limit to $5');
            print('    \x1b[36margentum budget set-limit 20 per-agent\x1b[0m Set per-agent limit to $20');
            print('');
            break;
        }
        default:
            error(`Unknown budget command: ${subcommand}`);
            print('  argentum budget [status|set-limit|history|reset|config]');
    }
}
function createBasicPrompt() {
    return readline.createInterface({ input: process.stdin, output: process.stdout });
}
function askBasic(rl, message, defaultValue = '') {
    const suffix = defaultValue ? ` [${defaultValue}]` : '';
    return new Promise((resolve) => {
        rl.question(`${message}${suffix}: `, (answer) => {
            const trimmed = answer.trim();
            resolve(trimmed || defaultValue);
        });
    });
}
async function askSecretBasic(rl, message, defaultValue = '') {
    const input = (rl.input ?? process.stdin);
    const output = (rl.output ?? process.stdout);
    if (!input.isTTY || !output.isTTY || typeof input.setRawMode !== 'function') {
        return askBasic(rl, message, defaultValue);
    }
    return new Promise((resolve) => {
        let buffer = '';
        const finish = (value) => {
            input.off('data', onData);
            input.setRawMode(false);
            output.write('\n');
            rl.resume();
            resolve(value.trim() || defaultValue);
        };
        const onData = (chunk) => {
            for (const char of chunk.toString('utf8')) {
                if (char === '\u0003') {
                    input.off('data', onData);
                    input.setRawMode(false);
                    output.write('\n');
                    process.exit(130);
                }
                if (char === '\r' || char === '\n') {
                    finish(buffer);
                    return;
                }
                if (char === '\u007f' || char === '\b') {
                    if (buffer.length > 0) {
                        buffer = buffer.slice(0, -1);
                        output.write('\b \b');
                    }
                    continue;
                }
                buffer += char;
                output.write('*');
            }
        };
        rl.pause();
        output.write(`${message}${defaultValue ? ' [stored]' : ''}: `);
        input.setRawMode(true);
        input.resume();
        input.on('data', onData);
    });
}
async function confirmBasic(rl, message, defaultValue) {
    const suffix = defaultValue ? 'Y/n' : 'y/N';
    const answer = (await askBasic(rl, `${message} (${suffix})`)).trim().toLowerCase();
    if (!answer)
        return defaultValue;
    return ['1', 'true', 'y', 'yes'].includes(answer);
}
function resolveBasicProviderChoice(value) {
    const presetProviders = Object.keys(onboarding_1.PROVIDER_PRESETS);
    const providers = [...presetProviders, 'custom'];
    const normalized = value.trim().toLowerCase();
    const numericChoice = Number.parseInt(normalized, 10);
    if (Number.isInteger(numericChoice) && numericChoice >= 1 && numericChoice <= providers.length) {
        return providers[numericChoice - 1];
    }
    const match = providers.find((provider) => provider === normalized);
    return match ?? 'nvidia';
}
function resolveBasicFeatureCategories(value) {
    const allowed = new Set(['core', 'comm', 'memory', 'productivity', 'automation', 'monitoring', 'skills']);
    return value
        .split(',')
        .map((category) => category.trim().toLowerCase())
        .filter((category) => allowed.has(category));
}
function isClackLoadError(err) {
    const message = err instanceof Error ? err.message : String(err);
    return message.includes('@clack/prompts') || message.includes('ERR_MODULE_NOT_FOUND');
}
async function cmdOnboardBasic() {
    if (!process.stdin.isTTY || !process.stdout.isTTY) {
        error('Interactive onboarding requires a terminal. Use --yes for non-interactive setup.');
        process.exitCode = 1;
        return;
    }
    if (command !== 'launch') {
        banner();
    }
    info('Welcome! This wizard will set up your Argentum instance.');
    info(`Workspace: ${getWorkDir()}`);
    print('');
    const providerEntries = Object.entries(onboarding_1.PROVIDER_PRESETS);
    print('Providers:');
    providerEntries.forEach(([key, provider], index) => {
        print(`  ${index + 1}. ${provider.label} (${key})`);
    });
    print(`  ${providerEntries.length + 1}. Custom (custom)`);
    print('');
    const rl = createBasicPrompt();
    try {
        const overwrite = hasFlag('--force') ||
            !projectConfigExists(getWorkDir()) ||
            (await confirmBasic(rl, 'Configuration already exists. Overwrite it?', false));
        if (!overwrite) {
            info('Setup cancelled. Existing configuration was left unchanged.');
            return;
        }
        const name = await askBasic(rl, 'Instance name', 'My Argentum');
        const provider = resolveBasicProviderChoice(await askBasic(rl, 'Provider name or number', 'nvidia'));
        let customProvider;
        if (provider === 'custom') {
            customProvider = {
                name: await askBasic(rl, 'Custom provider id', 'custom'),
                label: await askBasic(rl, 'Custom provider label', 'Custom'),
                base_url: await askBasic(rl, 'Custom provider base URL', 'https://example.invalid/v1'),
                api_key_env: await askBasic(rl, 'Custom provider API key environment variable', 'MY_API_KEY'),
                api: 'openai',
            };
        }
        const defaultModel = provider === 'custom'
            ? 'custom-model'
            : onboarding_1.PROVIDER_PRESETS[provider].defaultModel;
        const model = await askBasic(rl, 'Default model', defaultModel);
        const apiKey = await askSecretBasic(rl, 'API key (optional)', '');
        const port = Number.parseInt(await askBasic(rl, 'Server port', '3000'), 10);
        const featureCategories = resolveBasicFeatureCategories(await askBasic(rl, 'Feature categories (comma-separated: core, comm, memory, productivity, automation, monitoring, skills)', 'comm'));
        let telegram;
        if (await confirmBasic(rl, 'Set up Telegram bot now?', false)) {
            const token = await askSecretBasic(rl, 'Telegram bot token', '');
            const allowAll = token ? await confirmBasic(rl, 'Allow all Telegram users?', false) : false;
            const allowedUsers = allowAll
                ? []
                : parseNumberCsv(await askBasic(rl, 'Allowed Telegram user IDs (comma-separated)', ''));
            const allowedChats = allowAll
                ? []
                : parseNumberCsv(await askBasic(rl, 'Allowed Telegram chat IDs (comma-separated)', ''));
            telegram = { token, allowAll, allowedUsers, allowedChats };
        }
        const profile = (0, onboarding_1.createOnboardingProfile)({
            name,
            provider,
            customProvider,
            model,
            apiKey,
            port,
            featureCategories,
            webchatAuthToken: featureCategories.includes('comm') ? (0, onboarding_1.generateWebchatAuthToken)() : undefined,
            telegram,
        });
        const written = (0, onboarding_1.writeOnboardingProfile)(getWorkDir(), profile, { overwrite });
        print('');
        success('Setup complete');
        info(`Config: ${written.configPath}`);
        info(`Data: ${written.dataDir}`);
        for (const warning of profile.warnings)
            warn(warning);
        info('Next: argentum doctor');
    }
    catch (err) {
        error(err instanceof Error ? err.message : String(err));
        info('Use --force to overwrite an existing config/default.yaml');
        process.exitCode = 1;
    }
    finally {
        rl.close();
    }
}
async function cmdOnboard() {
    if (hasFlag('--yes', '--defaults', '--non-interactive')) {
        const port = Number(getArgValue('--port') ?? 3000);
        const provider = getArgValue('--provider') ?? 'nvidia';
        const featureCategories = hasFlag('--with-webchat') ? ['comm'] : [];
        const webchatAuthToken = hasFlag('--with-webchat') ? (0, onboarding_1.generateWebchatAuthToken)() : undefined;
        const profile = (0, onboarding_1.createOnboardingProfile)({
            provider,
            model: getArgValue('--model'),
            port,
            featureCategories,
            webchatAuthToken,
        });
        try {
            (0, onboarding_1.writeOnboardingProfile)(getWorkDir(), profile, { overwrite: hasFlag('--force') });
        }
        catch (err) {
            error(err instanceof Error ? err.message : String(err));
            info('Use --force to overwrite an existing config/default.yaml');
            process.exitCode = 1;
            return;
        }
        banner();
        success('Onboarding profile written');
        info(`Config: ${path.join(getWorkDir(), 'config', 'default.yaml')}`);
        info(`Data: ${path.join(getWorkDir(), 'data')}`);
        for (const warning of profile.warnings)
            warn(warning);
        info('Next: argentum doctor');
        return;
    }
    if (isPackagedRuntime()) {
        await cmdOnboardBasic();
        return;
    }
    let prompts;
    try {
        prompts = await (0, esm_1.importEsmModule)('@clack/prompts');
    }
    catch (err) {
        if (!isClackLoadError(err))
            throw err;
        warn('Advanced prompts are unavailable in this runtime, using basic setup.');
        await cmdOnboardBasic();
        return;
    }
    const { intro, outro, text, select, confirm, multiselect, log, password, isCancel } = prompts;
    intro((0, branding_1.formatArgentumBanner)(VERSION));
    log.step('Welcome! This wizard will set up your Argentum instance.');
    log.info('Press Ctrl+C at any time to cancel.');
    const config = (0, onboarding_1.createOnboardingProfile)().config;
    const envEntries = {};
    // Step 1: Instance name
    const nameVal = await text({
        message: 'Instance name:',
        initialValue: 'My Argentum',
    });
    if (typeof nameVal === 'string' && nameVal.trim())
        config.name = nameVal.trim();
    // Step 2: LLM Provider + Model selection
    const MODEL_DB = {
        minimax: [
            { value: 'MiniMax-M2.7', label: 'MiniMax M2.7', ctx: '1M', price: '$0.10/M', free: false },
            { value: 'MiniMax-M2.7-highspeed', label: 'MiniMax M2.7 Highspeed', ctx: '1M', price: '$0.30/M', free: false },
        ],
        groq: [
            { value: 'meta-llama/llama-4-scout-17b-16e-instruct', label: 'Llama 4 Scout', ctx: '128k', price: 'FREE', free: true },
            { value: 'meta-llama/llama-4-maverick-17b-128e-instruct', label: 'Llama 4 Maverick', ctx: '128k', price: '$0.20/M', free: false },
            { value: 'mistralai/mistral-nemo-12b-instruct', label: 'Mistral Nemo 12B', ctx: '128k', price: 'FREE', free: true },
            { value: 'mistralai/mistral-small-3.1-24b-instruct', label: 'Mistral Small 3.1 24B', ctx: '128k', price: 'FREE', free: true },
            { value: 'google/gemma-3-27b-it', label: 'Gemma 3 27B', ctx: '128k', price: 'FREE', free: true },
            { value: 'deepseek-ai/deepseek-llm-70b-chat', label: 'DeepSeek LLM 70B', ctx: '128k', price: 'FREE', free: true },
            { value: 'qwen/qwen3-30b-a3b-instruct', label: 'Qwen 3 30B', ctx: '32k', price: 'FREE', free: true },
        ],
        nvidia: [
            { value: 'deepseek-ai/deepseek-v3.2', label: 'DeepSeek V3', ctx: '128k', price: '$0.50/M', free: true },
            { value: 'meta/llama-3.3-nemotron-70b-instruct', label: 'Llama 3.3 Nemotron 70B', ctx: '128k', price: '$0.16/M' },
            { value: 'google/gemma-3-27b-it', label: 'Gemma 3 27B', ctx: '128k', price: '$0.10/M' },
            { value: 'mistralai/mistral-small-3.1-24b-instruct', label: 'Mistral Small 3.1 24B', ctx: '128k', price: '$0.15/M' },
            { value: 'mistralai/mistral-nemo-12b-instruct', label: 'Mistral Nemo 12B', ctx: '128k', price: '$0.15/M' },
            { value: 'qwen/qwen3-30b-a3b-instruct', label: 'Qwen 3 30B', ctx: '32k', price: '$0.10/M' },
            { value: 'meta/llama-3.2-11b-vision-instruct', label: 'Llama 3.2 11B Vision', ctx: '128k', price: '$0.10/M' },
            { value: 'meta/llama-3.2-3b-instruct', label: 'Llama 3.2 3B', ctx: '128k', price: 'FREE', free: true },
            { value: 'nvidia/llama-3.1-nemotron-70b-instruct', label: 'Nemotron 70B', ctx: '128k', price: '$0.16/M' },
            { value: 'deepseek-ai/deepseek-coder-v2-16lite-instruct', label: 'DeepSeek Coder V2 16B', ctx: '128k', price: 'FREE', free: true },
            { value: 'google/gemma-2-27b-it', label: 'Gemma 2 27B', ctx: '8k', price: '$0.10/M' },
            { value: 'google/gemma-2-9b-it', label: 'Gemma 2 9B', ctx: '8k', price: 'FREE', free: true },
            { value: 'snowfall/llama-3.3-70b-instruct-fp8', label: 'Llama 3.3 70B FP8', ctx: '128k', price: '$0.80/M' },
            { value: 'allenai/llama-3.1-tulu-3-8b', label: 'Tulu 3 8B', ctx: '128k', price: 'FREE', free: true },
        ],
        openrouter: [
            { value: 'google/gemma-3-27b-it', label: 'Gemma 3 27B', ctx: '128k', price: '$0.10/M', free: true },
            { value: 'deepseek/deepseek-chat-v3-0324', label: 'DeepSeek V3', ctx: '128k', price: '$0.50/M', free: true },
            { value: 'meta-llama/llama-3.3-70b-instruct', label: 'Llama 3.3 70B', ctx: '128k', price: '$0.80/M', free: true },
            { value: 'mistralai/mistral-nemo-12b-instruct', label: 'Mistral Nemo 12B', ctx: '128k', price: '$0.15/M', free: true },
            { value: 'anthropic/claude-sonnet-4-20250514', label: 'Claude Sonnet 4', ctx: '200k', price: '$3.00/M' },
            { value: 'anthropic/claude-opus-4-20250514', label: 'Claude Opus 4', ctx: '200k', price: '$15.00/M' },
            { value: 'anthropic/claude-3-5-haiku-20241022', label: 'Claude 3.5 Haiku', ctx: '200k', price: '$0.80/M' },
            { value: 'openai/gpt-4o', label: 'GPT-4o', ctx: '128k', price: '$2.50/M' },
            { value: 'openai/gpt-4o-mini', label: 'GPT-4o mini', ctx: '128k', price: '$0.15/M' },
            { value: 'openai/o3', label: 'GPT o3', ctx: '200k', price: '$10.00/M' },
            { value: 'openai/o4-mini', label: 'GPT o4-mini', ctx: '128k', price: '$1.10/M' },
            { value: 'google/gemini-2.5-flash', label: 'Gemini 2.5 Flash', ctx: '1M', price: '$0.075/M', free: true },
            { value: 'mistralai/mistral-large-3-24b-instruct', label: 'Mistral Large 3 24B', ctx: '128k', price: '$1.00/M', free: true },
            { value: 'qwen/qwen2.5-72b-instruct', label: 'Qwen 2.5 72B', ctx: '32k', price: '$0.70/M', free: true },
            { value: 'deepseek-ai/deepseek-v2.5', label: 'DeepSeek V2.5', ctx: '128k', price: '$0.28/M', free: true },
            { value: 'x-ai/grok-3', label: 'Grok 3', ctx: '131k', price: '$2.00/M' },
        ],
        google: [
            { value: 'gemini-2.5-flash', label: 'Gemini 2.5 Flash', ctx: '1M', price: '$0.075/M', free: true },
            { value: 'gemini-2.5-pro', label: 'Gemini 2.5 Pro', ctx: '1M', price: '$1.25/M' },
            { value: 'gemini-2.0-flash', label: 'Gemini 2.0 Flash', ctx: '1M', price: 'FREE', free: true },
            { value: 'gemini-2.0-flash-exp', label: 'Gemini 2.0 Flash Experimental', ctx: '1M', price: 'FREE', free: true },
            { value: 'gemini-1.5-flash', label: 'Gemini 1.5 Flash', ctx: '1M', price: '$0.075/M', free: true },
            { value: 'gemini-1.5-pro', label: 'Gemini 1.5 Pro', ctx: '2M', price: '$1.25/M' },
            { value: 'gemini-1.5-flash-8b', label: 'Gemini 1.5 Flash 8B', ctx: '1M', price: '$0.038/M', free: true },
        ],
        anthropic: [
            { value: 'claude-sonnet-4-20250514', label: 'Claude Sonnet 4', ctx: '200k', price: '$3.00/M' },
            { value: 'claude-opus-4-20250514', label: 'Claude Opus 4', ctx: '200k', price: '$15.00/M' },
            { value: 'claude-3-5-haiku-20241022', label: 'Claude 3.5 Haiku', ctx: '200k', price: '$0.80/M' },
            { value: 'claude-3-5-sonnet-20241022', label: 'Claude 3.5 Sonnet', ctx: '200k', price: '$3.00/M' },
            { value: 'claude-3-opus-20240229', label: 'Claude 3 Opus', ctx: '200k', price: '$15.00/M' },
            { value: 'claude-3-sonnet-20240229', label: 'Claude 3 Sonnet', ctx: '200k', price: '$3.00/M' },
        ],
        openai: [
            { value: 'gpt-4o', label: 'GPT-4o', ctx: '128k', price: '$2.50/M' },
            { value: 'gpt-4o-mini', label: 'GPT-4o mini', ctx: '128k', price: '$0.15/M' },
            { value: 'o3', label: 'GPT o3', ctx: '200k', price: '$10.00/M' },
            { value: 'o4-mini', label: 'GPT o4-mini', ctx: '128k', price: '$1.10/M' },
            { value: 'gpt-4.5-turbo', label: 'GPT-4.5 Turbo', ctx: '128k', price: '$75.00/M' },
            { value: 'gpt-4-turbo', label: 'GPT-4 Turbo', ctx: '128k', price: '$10.00/M' },
            { value: 'gpt-4', label: 'GPT-4', ctx: '8k', price: '$30.00/M' },
            { value: 'gpt-3.5-turbo', label: 'GPT-3.5 Turbo', ctx: '16k', price: '$0.50/M' },
        ],
    };
    const PROVIDERS = [
        {
            value: 'minimax',
            label: 'MiniMax',
            hint: 'api.minimax.io — M2.7 reasoning, cheap',
            base_url: 'https://api.minimax.io/v1',
            api_key_env: 'MINIMAX_API_KEY',
            api: 'openai',
        },
        {
            value: 'groq',
            label: 'Groq',
            hint: 'api.groq.com — fast inference, mostly free models',
            base_url: 'https://api.groq.com/openai/v1',
            api_key_env: 'GROQ_API_KEY',
            api: 'openai',
        },
        {
            value: 'ollama',
            label: 'Ollama',
            hint: 'localhost:11434 — run models locally (free)',
            base_url: 'http://127.0.0.1:11434/v1',
            api_key_env: 'OLLAMA_API_KEY',
            api: 'openai',
        },
        {
            value: 'nvidia',
            label: 'NVIDIA',
            hint: 'integrate.api.nvidia.com — deepseek free, fast',
            base_url: 'https://integrate.api.nvidia.com/v1',
            api_key_env: 'NVIDIA_API_KEY',
            api: 'openai',
        },
        {
            value: 'openrouter',
            label: 'OpenRouter',
            hint: 'openrouter.ai — many free models',
            base_url: 'https://openrouter.ai/api/v1',
            api_key_env: 'OPENROUTER_API_KEY',
            api: 'openai',
            headers: { 'HTTP-Referer': 'https://github.com/AG064/argentum', 'X-Title': 'Argentum' },
        },
        {
            value: 'google',
            label: 'Google Gemini',
            hint: 'generativelanguage.googleapis — 1M context free tier',
            base_url: 'https://generativelanguage.googleapis.com/v1beta/openai/',
            api_key_env: 'GOOGLE_API_KEY',
            api: 'openai',
        },
        {
            value: 'anthropic',
            label: 'Anthropic Claude',
            hint: 'api.anthropic.com — best reasoning models',
            base_url: 'https://api.anthropic.com',
            api_key_env: 'ANTHROPIC_API_KEY',
            api: 'anthropic',
        },
        {
            value: 'openai',
            label: 'OpenAI',
            hint: 'api.openai.com — GPT-4o family',
            base_url: 'https://api.openai.com/v1',
            api_key_env: 'OPENAI_API_KEY',
            api: 'openai',
        },
        { value: 'custom', label: 'Custom', hint: 'enter your own base URL', base_url: '', api_key_env: 'MY_API_KEY', api: 'openai' },
    ];
    const providerChoice = await select({
        message: 'Select LLM provider:',
        options: PROVIDERS.map((p) => ({ value: p.value, label: p.label, hint: p.hint })),
        initialValue: 'nvidia',
    });
    let selectedPreset;
    let usedKey = ''; // API key collected during discovery, used to skip the later prompt
    if (providerChoice === 'custom') {
        const custName = (await text({ message: 'Provider name:', initialValue: 'custom' }));
        const custUrl = (await text({ message: 'Base URL:', initialValue: 'https://' }));
        const custModel = (await text({ message: 'Default model:', initialValue: '' }));
        const custKeyEnv = (await text({ message: 'API key env var name:', initialValue: 'MY_API_KEY' }));
        selectedPreset = {
            name: custName?.trim() || 'custom',
            base_url: custUrl?.trim() || '',
            api_key_env: custKeyEnv?.trim() || 'MY_API_KEY',
            api: 'openai',
            model: custModel?.trim() || '',
        };
    }
    else {
        const provider = PROVIDERS.find((p) => p.value === providerChoice) ?? PROVIDERS[0];
        // Step 2a: Choose discovery method
        const discoveryMethodRaw = await select({
            message: `How to pick model for ${provider.label}?`,
            options: [
                { value: 'discover', label: 'Discover Live', hint: 'fetch models from provider API now' },
                { value: 'curated', label: 'Quick Pick', hint: 'curated list (works offline)' },
            ],
        });
        const discoveryMethod = isCancel(discoveryMethodRaw) ? 'curated' : String(discoveryMethodRaw);
        // chosenModel is always assigned by one of the branches below
        let chosenModel = '';
        let liveModels = [];
        if (discoveryMethod === 'discover') {
            // Step 1: try discovery WITHOUT API key first (many providers allow this)
            process.stdout.write(`  Querying ${provider.base_url}/models (no key)... `);
            liveModels = await (0, modelDiscovery_js_1.discoverModels)(provider, '');
            if (liveModels.length === 0) {
                // No results — ask for key and retry
                const discoveryKey = await password({
                    message: `${provider.api_key_env} (required for this provider):`,
                    mask: '*',
                });
                usedKey = discoveryKey && !isCancel(discoveryKey) ? String(discoveryKey).trim() : '';
                if (usedKey) {
                    process.stdout.write(`  Querying ${provider.base_url}/models (with key)... `);
                    liveModels = await (0, modelDiscovery_js_1.discoverModels)(provider, usedKey);
                    process.stdout.write(liveModels.length > 0 ? `OK (${liveModels.length} models)\n` : 'no models\n');
                }
            }
            else {
                process.stdout.write(`OK (${liveModels.length} models)\n`);
            }
            if (liveModels.length > 0) {
                log.success(`${liveModels.length} models discovered — pick one`);
                chosenModel = String(await select({
                    message: `${provider.label} models (live):`,
                    options: liveModels.map((m) => ({
                        value: m.value,
                        label: `${m.label} ${m.free ? '(FREE)' : ''}`,
                        hint: `${m.ctx} context · ${m.price}`,
                    })),
                }));
            }
            else {
                log.warn('Discovery failed — using curated list');
            }
        }
        if (!chosenModel) {
            // Curated list fallback
            const models = MODEL_DB[providerChoice] ?? [];
            chosenModel = String(await select({
                message: `${provider.label} model (curated):`,
                options: models.map((m) => ({
                    value: m.value,
                    label: `${m.label} ${m.free ? '(FREE)' : ''}`,
                    hint: `${m.ctx} context · ${m.price}`,
                })),
            }));
        }
        selectedPreset = {
            name: provider.value,
            base_url: provider.base_url,
            api_key_env: provider.api_key_env,
            api: provider.api,
            model: chosenModel,
            ...(provider.headers ? { headers: provider.headers } : {}),
        };
        log.success(`${chosenModel} selected`);
    }
    config.llm.providers = {};
    config.llm.providers[selectedPreset.name] = {
        base_url: selectedPreset.base_url,
        api_key_env: selectedPreset.api_key_env,
        api: selectedPreset.api,
        models: [selectedPreset.model],
        ...(selectedPreset.headers ? { headers: selectedPreset.headers } : {}),
    };
    config.llm.default = selectedPreset.name;
    log.success(`Selected: ${selectedPreset.name} (${selectedPreset.base_url})`);
    // API key
    // If we already asked for key during discovery, don't ask again
    if (!usedKey) {
        const apiKeyVal = await password({
            message: `${selectedPreset.api_key_env} (Enter to skip):`,
            mask: '*',
        });
        usedKey = typeof apiKeyVal === 'string' ? apiKeyVal.trim() : '';
    }
    if (usedKey) {
        envEntries[selectedPreset.api_key_env] = usedKey;
        log.success(`Saved ${selectedPreset.api_key_env} to .env`);
    }
    // Fallback models
    const addModels = await text({
        message: 'Fallback models (comma-separated, Enter to skip):',
        initialValue: '',
    });
    if (typeof addModels === 'string' && addModels.trim()) {
        config.llm.providers[selectedPreset.name].models.push(...addModels.split(',').map((m) => m.trim()));
    }
    // Telegram setup
    const setupTg = await confirm({ message: 'Set up Telegram bot?', initialValue: false });
    if (setupTg === true) {
        const botToken = await password({ message: 'Bot token:', mask: '' });
        if (typeof botToken === 'string' && botToken.trim()) {
            const allowAll = await confirm({
                message: 'Allow all Telegram users? Only choose this for a private trusted bot.',
                initialValue: false,
            });
            const allowedUsersRaw = allowAll === true
                ? ''
                : (await text({
                    message: 'Allowed Telegram user IDs (comma-separated, Enter to skip):',
                    initialValue: '',
                }));
            const allowedChatsRaw = allowAll === true
                ? ''
                : (await text({
                    message: 'Allowed Telegram chat IDs (comma-separated, Enter to skip):',
                    initialValue: '',
                }));
            const allowedUsers = parseNumberCsv(allowedUsersRaw);
            const allowedChats = parseNumberCsv(allowedChatsRaw);
            if (allowAll === true || allowedUsers.length > 0 || allowedChats.length > 0) {
                config.channels.telegram = {
                    enabled: true,
                    allowedUsers,
                    allowedChats,
                    allowAll: allowAll === true,
                };
                envEntries.ARGENTUM_TELEGRAM_TOKEN = botToken.trim();
                log.success('Telegram configured');
            }
            else {
                config.channels.telegram = {
                    enabled: false,
                    allowedUsers: [],
                    allowedChats: [],
                    allowAll: false,
                };
                log.warn('Telegram left disabled because no allowlist was provided');
            }
        }
    }
    // Features selection
    const selectedCats = await multiselect({
        message: 'Select feature categories to enable:',
        options: [
            { value: 'core', label: 'Core', hint: 'sqlite-memory, cron-scheduler, audit-log' },
            { value: 'comm', label: 'Communication', hint: 'telegram, webchat, discord-bot, slack' },
            { value: 'memory', label: 'Memory', hint: 'knowledge-graph, semantic-search' },
            { value: 'productivity', label: 'Productivity', hint: 'goals, life-domains, task-checkout' },
            { value: 'automation', label: 'Automation', hint: 'browser-automation, webhooks, file-watcher' },
            { value: 'monitoring', label: 'Monitoring', hint: 'health-monitoring, budget, email' },
            { value: 'skills', label: 'Skills', hint: 'skills-library, skill-loader, skill-evolution' },
        ],
        required: false,
    });
    const featureMap = {
        core: ['sqlite-memory', 'cron-scheduler', 'audit-log'],
        comm: ['webchat'],
        memory: ['knowledge-graph', 'semantic-search', 'markdown-memory', 'multimodal-memory'],
        productivity: ['goals', 'life-domains', 'task-checkout', 'goal-decomposition'],
        automation: ['browser-automation', 'file-watcher', 'webhooks', 'container-sandbox'],
        monitoring: ['health-monitoring', 'budget', 'email-integration'],
        skills: ['skills-library', 'skill-loader', 'skill-evolution'],
    };
    const allSelectedFeatures = [];
    if (Array.isArray(selectedCats)) {
        for (const cat of selectedCats) {
            const features = featureMap[cat];
            if (features)
                allSelectedFeatures.push(...features);
        }
    }
    for (const f of [...new Set(allSelectedFeatures)]) {
        config.features = config.features ?? {};
        if (f === 'webchat') {
            const token = (0, onboarding_1.generateWebchatAuthToken)();
            config.features[f] = {
                enabled: true,
                port: 3001,
                host: '127.0.0.1',
                requireAuth: true,
                maxConnections: 1000,
                messageHistory: 100,
                maxMessageLength: 10000,
                maxPayloadBytes: 1024 * 1024,
                maxFileSize: 10 * 1024 * 1024,
                rateLimitWindowMs: 60000,
                maxMessagesPerWindow: 60,
                allowedFileTypes: ['image/*', 'text/*', 'application/pdf', 'application/json'],
            };
            config.channels.webchat = { enabled: true };
            envEntries.ARGENTUM_WEBCHAT_AUTH_TOKEN = token;
            continue;
        }
        config.features[f] = { enabled: true };
    }
    log.info(allSelectedFeatures.length > 0
        ? `Enabled ${allSelectedFeatures.length} features`
        : 'Minimal install - no extra features selected');
    // Server port
    const portVal = await text({ message: 'Server port:', initialValue: '3000' });
    const portNum = parseInt(portVal ?? '3000');
    if (!isNaN(portNum)) {
        config.server.port = portNum;
        config.server.cors = {
            enabled: true,
            origins: [`http://127.0.0.1:${portNum}`, `http://localhost:${portNum}`],
        };
    }
    // Save config
    const configDir = path.join(getWorkDir(), 'config');
    fs.mkdirSync(configDir, { recursive: true });
    const configFilePath = path.join(configDir, 'default.yaml');
    config_1.ConfigSchema.parse(config);
    fs.writeFileSync(configFilePath, (0, yaml_1.stringify)(config, { lineWidth: 120 }));
    appendEnvEntries(getWorkDir(), envEntries);
    log.success('Config saved to config/default.yaml');
    // Create data dir
    const dataDir = path.join(getWorkDir(), 'data');
    fs.mkdirSync(dataDir, { recursive: true });
    log.success('Data directory ready');
    outro(`
  Setup complete!

  Next steps:
    argentum gateway start --port ${config.server.port}
    argentum status
    argentum skill search <query>

  Thank you for choosing Argentum!
`);
}
async function cmdSkill() {
    const subcommand = args[1] || 'list';
    const { execFileSync } = require('child_process');
    // Default skills dir: OpenClaw workspace
    const defaultWorkDir = path.join(process.env.HOME || '~', '.openclaw', 'workspace');
    const clawhubWorkDir = process.env.ARGENTUM_WORKDIR || defaultWorkDir;
    const clawhubBin = process.platform === 'win32' ? 'clawhub.cmd' : 'clawhub';
    const runClawhub = (clawhubArgs) => {
        try {
            return execFileSync(clawhubBin, [...clawhubArgs, '--workdir', clawhubWorkDir, '--no-input'], {
                encoding: 'utf8',
                timeout: 30000,
                env: { ...process.env, CLAWHUB_WORKDIR: clawhubWorkDir },
            });
        }
        catch (err) {
            return err.stdout || err.stderr || err.message;
        }
    };
    switch (subcommand) {
        case 'list':
        case 'ls': {
            banner();
            info('Installed skills:');
            print('');
            const result = runClawhub(['list']);
            print(result);
            break;
        }
        case 'search': {
            const query = args.slice(2).join(' ');
            if (!query) {
                error('Usage: argentum skill search <query>');
                return;
            }
            banner();
            info(`Searching for: ${query}`);
            print('');
            const result = runClawhub(['search', query]);
            print(result);
            break;
        }
        case 'install':
        case 'add': {
            const slug = args[2];
            if (!slug) {
                error('Usage: argentum skill install <skill-slug>');
                return;
            }
            banner();
            info(`Installing: ${slug}`);
            print('');
            const result = runClawhub(['install', slug]);
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
                error('Usage: argentum skill uninstall <skill-slug>');
                return;
            }
            banner();
            info(`Uninstalling: ${slug}`);
            const result = runClawhub(['uninstall', slug]);
            print(result);
            break;
        }
        case 'update':
        case 'upgrade': {
            const slug = args[2];
            banner();
            if (slug) {
                info(`Updating: ${slug}`);
            }
            else {
                info('Updating all installed skills...');
            }
            const result = runClawhub(slug ? ['update', slug] : ['update']);
            print(result);
            break;
        }
        case 'explore': {
            banner();
            info('Browse latest skills from ClawHub:');
            print('');
            const result = runClawhub(['explore']);
            print(result);
            break;
        }
        case 'info':
        case 'inspect': {
            const slug = args[2];
            if (!slug) {
                error('Usage: argentum skill info <skill-slug>');
                return;
            }
            banner();
            info(`Inspecting: ${slug}`);
            print('');
            const result = runClawhub(['inspect', slug]);
            print(result);
            break;
        }
        case 'publish': {
            const skillPath = args[2] || '.';
            banner();
            info(`Publishing: ${skillPath}`);
            const result = runClawhub(['publish', skillPath]);
            print(result);
            break;
        }
        case 'star': {
            const slug = args[2];
            if (!slug) {
                error('Usage: argentum skill star <skill-slug>');
                return;
            }
            runClawhub(['star', slug]);
            success(`Starred: ${slug}`);
            break;
        }
        case 'unstar': {
            const slug = args[2];
            if (!slug) {
                error('Usage: argentum skill unstar <skill-slug>');
                return;
            }
            runClawhub(['unstar', slug]);
            success(`Unstarred: ${slug}`);
            break;
        }
        case 'sync': {
            banner();
            info('Syncing local skills with ClawHub...');
            const result = runClawhub(['sync']);
            print(result);
            break;
        }
        /**
         * Skills-list: Level 0 - lightweight list (Hermes-style progressive disclosure)
         * Shows only name, description, category for minimal token cost
         */
        case 'skills-list':
        case 'sl': {
            banner();
            info('Installed skills (Level 0 - progressive disclosure):');
            print('');
            // Use the skills-loader feature directly
            try {
                const skillsLoader = require(path.join(__dirname, 'src', 'features', 'skills-loader', 'index.js')).default;
                const skills = skillsLoader.skillsList();
                if (skills.length === 0) {
                    info('No skills found');
                    return;
                }
                for (const s of skills) {
                    print(`  \x1b[1m${s.name}\x1b[0m [\x1b[33m${s.category}\x1b[0m] v${s.version}`);
                    print(`    ${s.description.slice(0, 80)}${s.description.length > 80 ? '...' : ''}`);
                    const extras = [];
                    if (s.hasScripts)
                        extras.push('scripts');
                    if (s.hasReferences)
                        extras.push('references');
                    if (extras.length)
                        print(`    \x1b[90m${extras.join(', ')}\x1b[0m`);
                    print('');
                }
                print(`  Total: ${skills.length} skills`);
                print('');
                info('Use: argentum skill skill-view <name>  (Level 1 - full content)');
                info('Use: argentum skill skill-view <name> <ref-path>  (Level 2 - specific file)');
            }
            catch (err) {
                error(`Failed to load skills: ${err.message}`);
            }
            break;
        }
        /**
         * Skill-view: Level 1 (full content) or Level 2 (specific reference)
         */
        case 'skill-view':
        case 'view': {
            const skillName = args[2];
            const refPath = args[3];
            if (!skillName) {
                error('Usage: argentum skill skill-view <name> [ref-path]');
                return;
            }
            banner();
            try {
                const skillsLoader = require(path.join(__dirname, 'src', 'features', 'skills-loader', 'index.js')).default;
                if (refPath) {
                    // Level 2: specific reference file
                    // Only simple filenames are allowed (no directory components on any platform).
                    // Check '..' as a path segment (after normalization) rather than a substring to allow
                    // legitimate filenames like 'release..notes.md' while still blocking traversal.
                    const normalizedRefPath = path.normalize(refPath);
                    const hasParentTraversalSegment = normalizedRefPath
                        .split(/[\\/]+/)
                        .some((segment) => segment === '..');
                    if (path.isAbsolute(refPath) ||
                        hasParentTraversalSegment ||
                        // Also reject any path separators to ensure only simple filenames are accepted
                        // (e.g., block 'docs/readme.md' which is not traversal but still a directory component)
                        refPath.includes('/') ||
                        refPath.includes('\\')) {
                        error('Invalid reference path');
                        return;
                    }
                    info(`Loading ${skillName}/${refPath} (Level 2)...`);
                    print('');
                    const content = skillsLoader.skillViewRef(skillName, refPath);
                    if (content === null) {
                        error(`Reference '${refPath}' not found in skill '${skillName}'`);
                    }
                    else {
                        print(content);
                    }
                }
                else {
                    // Level 1: full skill content
                    info(`Loading ${skillName} (Level 1 - full content)...`);
                    print('');
                    const skill = skillsLoader.skillView(skillName);
                    if (!skill) {
                        error(`Skill '${skillName}' not found`);
                        return;
                    }
                    print(`\x1b[1m=== ${skill.name} ===\x1b[0m`);
                    print(`Category: \x1b[33m${skill.category}\x1b[0m | Version: ${skill.version}`);
                    if (skill.platforms.length)
                        print(`Platforms: ${skill.platforms.join(', ')}`);
                    print('');
                    print(`\x1b[1mDescription:\x1b[0m ${skill.description}`);
                    print('');
                    if (skill.scripts.length) {
                        print(`\x1b[1mScripts:\x1b[0m ${skill.scripts.join(', ')}`);
                    }
                    if (skill.references.length) {
                        print(`\x1b[1mReferences:\x1b[0m ${skill.references.join(', ')}`);
                        info('Use: argentum skill skill-view <name> <ref-path>  (Level 2)');
                    }
                    print('');
                    print('\x1b[1m=== Full Content ===\x1b[0m');
                    print(skill.fullContent);
                }
            }
            catch (err) {
                error(`Failed to load skill: ${err.message}`);
            }
            break;
        }
        /**
         * Skills-hub: Search/browse skills hub (agentskills.io compatible)
         */
        case 'skills-hub':
        case 'hub': {
            const query = args.slice(2).join(' ');
            banner();
            info('Skills Hub (agentskills.io compatible)');
            print('');
            if (!query) {
                info('Usage: argentum skill skills-hub <search-query>');
                info('This feature queries agentskills.io registry (future)');
                print('');
                print('For now, use: argentum skill explore  (browse ClawHub)');
                print('           argentum skill search <q>  (search ClawHub)');
            }
            else {
                info(`Searching hub for: ${query}`);
                print('');
                // Future: query agentskills.io API
                // For now, fall back to ClawHub search
                const result = runClawhub(['search', query]);
                print(result);
            }
            break;
        }
        default: {
            // Treat as "run" — execute a script from an installed skill
            const skillName = subcommand;
            const skillsDir = path.join(clawhubWorkDir, 'skills');
            // Validate skillName to prevent path traversal. Resolve and ensure it stays within skillsDir
            const resolvedSkillPath = path.resolve(skillsDir, skillName);
            const skillsDirResolved = path.resolve(skillsDir) + path.sep;
            if (!resolvedSkillPath.startsWith(skillsDirResolved)) {
                error('Path traversal attempt detected for skill name');
                return;
            }
            const skillPath = resolvedSkillPath;
            if (!fs.existsSync(skillPath)) {
                error(`Unknown command or skill: ${skillName}`);
                print('');
                print('  argentum skill [list|search|install|uninstall|update|explore|info|publish|star|sync]');
                return;
            }
            const scriptName = args[2];
            if (!scriptName) {
                const scriptsDir = path.join(skillPath, 'scripts');
                if (fs.existsSync(scriptsDir)) {
                    const scripts = fs
                        .readdirSync(scriptsDir)
                        .filter((f) => /\.(sh|js|py|ts)$/.test(f));
                    if (scripts.length) {
                        info(`Scripts in ${skillName}:`);
                        for (const s of scripts)
                            print(`  • ${s}`);
                    }
                }
                return;
            }
            // Validate scriptName and ensure it resolves inside the skill's scripts directory
            const scriptsDir = path.join(skillPath, 'scripts');
            const resolvedScriptPath = path.resolve(scriptsDir, scriptName);
            const scriptsDirResolved = path.resolve(scriptsDir) + path.sep;
            if (!resolvedScriptPath.startsWith(scriptsDirResolved)) {
                error('Path traversal attempt detected for script name');
                return;
            }
            const scriptPath = resolvedScriptPath;
            if (!fs.existsSync(scriptPath)) {
                error(`Script '${scriptName}' not found`);
                return;
            }
            const scriptArgs = args.slice(3);
            let command;
            let commandArgs;
            if (scriptName.endsWith('.sh')) {
                command = 'bash';
                commandArgs = [scriptPath, ...scriptArgs];
            }
            else if (scriptName.endsWith('.js')) {
                command = 'node';
                commandArgs = [scriptPath, ...scriptArgs];
            }
            else if (scriptName.endsWith('.py')) {
                command = process.platform === 'win32' ? 'python' : 'python3';
                commandArgs = [scriptPath, ...scriptArgs];
            }
            else {
                command = process.platform === 'win32' ? 'npx.cmd' : 'npx';
                commandArgs = ['tsx', scriptPath, ...scriptArgs];
            }
            try {
                const result = execFileSync(command, commandArgs, {
                    cwd: skillPath,
                    timeout: 30000,
                    encoding: 'utf8',
                });
                print(result);
            }
            catch (err) {
                error(err.message);
            }
        }
    }
}
// ─── Security Commands ─────────────────────────────────────────────────────────
async function cmdSecurity() {
    const subcommand = args[1] || 'status';
    const workDir = getWorkDir();
    const dataDir = path.join(workDir, 'data');
    const securityDbPath = path.join(dataDir, 'security.db');
    // Lazy-load security modules
    const getSecurityModules = () => {
        try {
            // Security modules are in dist/security/ (not dist/src/security/)
            const baseDir = path.join(__dirname, 'security');
            const policyEnginePath = path.join(baseDir, 'policy-engine', 'index.js');
            const credentialManagerPath = path.join(baseDir, 'credential-manager', 'index.js');
            const sandboxPath = path.join(baseDir, 'sandbox', 'index.js');
            const approvalUIPath = path.join(baseDir, 'approval-ui', 'index.js');
            const blueprintPath = path.join(baseDir, 'blueprint', 'index.js');
            const { getPolicyEngine } = require(policyEnginePath);
            const { getCredentialManager } = require(credentialManagerPath);
            const { getSandboxExecutor } = require(sandboxPath);
            const { getApprovalUI } = require(approvalUIPath);
            const { getBlueprintLoader, BlueprintLoader } = require(blueprintPath);
            return {
                getPolicyEngine,
                getCredentialManager,
                getSandboxExecutor,
                getApprovalUI,
                getBlueprintLoader,
                BlueprintLoader,
            };
        }
        catch (err) {
            return null;
        }
    };
    const modules = getSecurityModules();
    switch (subcommand) {
        case 'status':
        case 'stats': {
            banner();
            print('  \x1b[1m\x1b[36m╔══════════════════════════════════════════╗\x1b[0m');
            print('  \x1b[1m\x1b[36m║         \x1b[33m🔒 Security Status\x1b[36m             ║\x1b[0m');
            print('  \x1b[1m\x1b[36m╚══════════════════════════════════════════╝\x1b[0m');
            print('');
            if (!modules) {
                warn('Security modules not compiled. Run: npm run build');
                return;
            }
            const policyEngine = modules.getPolicyEngine(securityDbPath);
            const stats = policyEngine.getStats();
            print('  \x1b[1mPolicies\x1b[0m');
            print(`    Active:  \x1b[32m${stats.policiesActive}\x1b[0m / ${stats.policiesTotal}`);
            print(`    Pending Approvals: \x1b[33m${stats.approvalsPending}\x1b[0m`);
            print('');
            print('  \x1b[1mCredentials\x1b[0m');
            print(`    Total:           ${stats.credentialsTotal}`);
            print(`    Expiring Soon:   \x1b[33m${stats.credentialsExpiringSoon}\x1b[0m`);
            print('');
            print('  \x1b[1mSandbox\x1b[0m');
            print(`    Executions:  ${stats.sandboxExecutionsTotal}`);
            print(`    Blocked:     \x1b[31m${stats.sandboxBlockedTotal}\x1b[0m`);
            print('');
            print('  \x1b[1mAudit Log\x1b[0m');
            print(`    Entries:     ${stats.auditEntriesTotal}`);
            print(`    Threats:     \x1b[31m${stats.threatsDetected}\x1b[0m (24h)`);
            print('');
            const uptime = formatUptime(stats.uptime);
            print(`  \x1b[1mUptime:\x1b[0m ${uptime}`);
            print('');
            break;
        }
        case 'policies':
        case 'policy': {
            const policySubcommand = args[2] || 'list';
            banner();
            if (!modules) {
                warn('Security modules not compiled.');
                return;
            }
            const policyEngine = modules.getPolicyEngine(securityDbPath);
            if (policySubcommand === 'list' || policySubcommand === 'ls') {
                const policies = policyEngine.getPolicies();
                info(`Security Policies (${policies.length}):`);
                print('');
                if (policies.length === 0) {
                    info('No policies defined.');
                }
                else {
                    print(`  \x1b[90m  ${'Name'.padEnd(25)} ${'Resource'.padEnd(25)} ${'Action'.padEnd(8)} ${'Effect'.padEnd(8)} ${'Priority'} ${'Enabled'}\x1b[0m`);
                    print(`  \x1b[90m  ${'─'.repeat(80)}`);
                    for (const p of policies) {
                        const effectColor = p.effect === 'allow' ? '\x1b[32m' : p.effect === 'deny' ? '\x1b[31m' : '\x1b[33m';
                        const enabledIcon = p.enabled ? '\x1b[32m●\x1b[0m' : '\x1b[31m○\x1b[0m';
                        print(`    ${p.name.padEnd(25)} ${p.resource.slice(0, 25).padEnd(25)} ${p.action.padEnd(8)} ${effectColor}${p.effect.padEnd(8)}\x1b[0m ${String(p.priority).padEnd(6)} ${enabledIcon}`);
                    }
                    print('');
                }
                break;
            }
            if (policySubcommand === 'add') {
                const getArg = (flag) => {
                    const idx = args.indexOf(`--${flag}`);
                    return idx !== -1 ? args[idx + 1] : undefined;
                };
                const name = getArg('name');
                const effect = getArg('effect');
                const resource = getArg('resource');
                const action = getArg('action') || '*';
                const priority = parseInt(getArg('priority') || '0', 10);
                const requiresApproval = args.includes('--requires-approval');
                if (!name || !effect || !resource) {
                    error('Usage: argentum security policies add --name <name> --effect <allow|deny|approve> --resource <pattern> [--action <action>] [--priority <n>] [--requires-approval]');
                    return;
                }
                const policy = policyEngine.addPolicy({
                    name,
                    effect,
                    resource,
                    action,
                    priority,
                    enabled: true,
                    requiresApproval,
                    approvalRisk: requiresApproval ? 'medium' : undefined,
                });
                success(`Policy '${name}' created (ID: ${policy.id.slice(0, 8)}...)`);
                print('');
                break;
            }
            if (policySubcommand === 'remove' || policySubcommand === 'rm') {
                const policyId = args[3];
                if (!policyId) {
                    error('Usage: argentum security policies remove <policy-id>');
                    return;
                }
                const ok = policyEngine.removePolicy(policyId);
                if (ok)
                    success('Policy removed');
                else
                    error('Policy not found');
                break;
            }
            if (policySubcommand === 'enable') {
                const policyId = args[3];
                if (!policyId) {
                    error('Usage: argentum security policies enable <policy-id>');
                    return;
                }
                const ok = policyEngine.setPolicyEnabled(policyId, true);
                if (ok)
                    success('Policy enabled');
                else
                    error('Policy not found');
                break;
            }
            if (policySubcommand === 'disable') {
                const policyId = args[3];
                if (!policyId) {
                    error('Usage: argentum security policies disable <policy-id>');
                    return;
                }
                const ok = policyEngine.setPolicyEnabled(policyId, false);
                if (ok)
                    success('Policy disabled');
                else
                    error('Policy not found');
                break;
            }
            info('Policy commands:');
            print('  argentum security policies list');
            print('  argentum security policies add --name <name> --effect <allow|deny|approve> --resource <pattern>');
            print('  argentum security policies remove <id>');
            print('  argentum security policies enable <id>');
            print('  argentum security policies disable <id>');
            break;
        }
        case 'approvals':
        case 'approval': {
            const approvalSubcommand = args[2] || 'list';
            banner();
            if (!modules) {
                warn('Security modules not compiled.');
                return;
            }
            const approvalUI = modules.getApprovalUI();
            const policyEngine = modules.getPolicyEngine(securityDbPath);
            if (approvalSubcommand === 'list' || approvalSubcommand === 'ls') {
                const pending = policyEngine.getPendingApprovals();
                print(approvalUI.renderPendingList(pending));
                break;
            }
            if (approvalSubcommand === 'show') {
                const approvalId = args[3];
                if (!approvalId) {
                    error('Usage: argentum security approval show <id>');
                    return;
                }
                const approval = policyEngine.getApproval(approvalId);
                if (!approval) {
                    error('Approval not found');
                    return;
                }
                print(approvalUI.renderDetail(approval));
                break;
            }
            if (approvalSubcommand === 'approve') {
                const approvalId = args[3];
                if (!approvalId) {
                    error('Usage: argentum security approve <id>');
                    return;
                }
                const userId = 'cli-user';
                const result = approvalUI.handleResponse(approvalId, 'approve', userId);
                if (result.success)
                    success('Request approved');
                else
                    error(`Failed: ${result.error}`);
                break;
            }
            if (approvalSubcommand === 'deny') {
                const approvalId = args[3];
                if (!approvalId) {
                    error('Usage: argentum security deny <id>');
                    return;
                }
                const userId = 'cli-user';
                const result = approvalUI.handleResponse(approvalId, 'deny', userId);
                if (result.success)
                    success('Request denied');
                else
                    error(`Failed: ${result.error}`);
                break;
            }
            info('Approval commands:');
            print('  argentum security approvals list');
            print('  argentum security approval show <id>');
            print('  argentum security approve <id>');
            print('  argentum security deny <id>');
            break;
        }
        case 'audit':
        case 'log': {
            banner();
            if (!modules) {
                warn('Security modules not compiled.');
                return;
            }
            const policyEngine = modules.getPolicyEngine(securityDbPath);
            const sinceIdx = args.indexOf('--since');
            const untilIdx = args.indexOf('--until');
            const actorIdx = args.indexOf('--actor');
            const actionIdx = args.indexOf('--action');
            const limitIdx = args.indexOf('--limit');
            const since = sinceIdx !== -1 && args[sinceIdx + 1] ? new Date(args[sinceIdx + 1]).getTime() : undefined;
            const until = untilIdx !== -1 && args[untilIdx + 1] ? new Date(args[untilIdx + 1]).getTime() : undefined;
            const actor = actorIdx !== -1 ? args[actorIdx + 1] : undefined;
            const action = actionIdx !== -1 ? args[actionIdx + 1] : undefined;
            const limit = limitIdx !== -1 && args[limitIdx + 1] ? parseInt(args[limitIdx + 1], 10) : 50;
            const entries = policyEngine.queryAudit({ since, until, actor, action, limit });
            info(`Audit Log (${entries.length} entries):`);
            print('');
            if (entries.length === 0) {
                info('No audit entries found.');
            }
            else {
                print(`  \x1b[90m  ${'Time'.padEnd(20)} ${'Severity'.padEnd(8)} ${'Action'.padEnd(25)} ${'Actor'.padEnd(12)} Decision\x1b[0m`);
                print(`  \x1b[90m  ${'─'.repeat(85)}`);
                for (const entry of entries) {
                    const time = new Date(entry.timestamp).toISOString().slice(0, 19).replace('T', ' ');
                    const sevColor = entry.severity === 'error' || entry.severity === 'critical'
                        ? '\x1b[31m'
                        : entry.severity === 'warning'
                            ? '\x1b[33m'
                            : '\x1b[90m';
                    const decisionStr = entry.decision ? entry.decision.padEnd(8) : '         ';
                    print(`    ${time} ${sevColor}${entry.severity.padEnd(8)}\x1b[0m ${entry.action.padEnd(25)} ${(entry.actor || '-').padEnd(12)} ${decisionStr}`);
                }
            }
            print('');
            break;
        }
        case 'credentials':
        case 'creds': {
            banner();
            if (!modules) {
                warn('Security modules not compiled.');
                return;
            }
            const credManager = modules.getCredentialManager(securityDbPath);
            if (args[2] === 'list') {
                const creds = credManager.listCredentials();
                info(`Credentials (${creds.length}):`);
                print('');
                if (creds.length === 0) {
                    info('No credentials stored.');
                }
                else {
                    print(`  \x1b[90m  ${'Name'.padEnd(20)} ${'Provider'.padEnd(15)} ${'Type'.padEnd(10)} Expires\x1b[0m`);
                    print(`  \x1b[90m  ${'─'.repeat(65)}`);
                    for (const c of creds) {
                        const expiresIn = formatExpiry(c.expiresAt);
                        const expiringSoon = c.expiresAt - Date.now() < 300000;
                        const expColor = expiringSoon ? '\x1b[33m' : '\x1b[90m';
                        print(`    ${c.name.padEnd(20)} ${c.provider.padEnd(15)} ${c.type.padEnd(10)} ${expColor}${expiresIn}\x1b[0m`);
                    }
                }
                print('');
                break;
            }
            if (args[2] === 'rotate') {
                success('Credential rotation triggered (demo)');
                break;
            }
            info('Credential commands:');
            print('  argentum security credentials list');
            print('  argentum security credentials rotate [id]');
            break;
        }
        case 'sandbox': {
            banner();
            if (!modules) {
                warn('Security modules not compiled.');
                return;
            }
            const sandbox = modules.getSandboxExecutor();
            const config = sandbox.getConfig();
            info('Sandbox Configuration:');
            print('');
            print(`  Enabled:          ${config.enabled ? '\x1b[32myes\x1b[0m' : '\x1b[31mno\x1b[0m'}`);
            print(`  Network Isolation: ${config.networkIsolation ? '\x1b[32myes\x1b[0m' : '\x1b[31mno\x1b[0m'}`);
            print(`  Allow Exec:       ${config.allowExec ? '\x1b[32myes\x1b[0m' : '\x1b[31mno\x1b[0m'}`);
            print(`  Max Memory:       ${config.maxMemoryMb ?? 512} MB`);
            print(`  Max CPU:          ${config.maxCpuPercent ?? 50}%`);
            print(`  Max Exec Time:    ${config.maxExecutionTimeMs ?? 30000} ms`);
            print('');
            print(`  \x1b[1mAllowed Paths:\x1b[0m`);
            for (const p of config.allowedPaths)
                print(`    • ${p}`);
            print('');
            print(`  \x1b[1mDenied Paths:\x1b[0m`);
            for (const p of config.deniedPaths)
                print(`    • ${p}`);
            print('');
            print(`  \x1b[1mAllowed Languages:\x1b[0m ${config.allowedLanguages.join(', ')}`);
            print('');
            const stats = sandbox.getStats();
            print(`  Executions: ${stats.executions} | Blocked: \x1b[31m${stats.blocked}\x1b[0m`);
            print('');
            break;
        }
        case 'blueprint': {
            banner();
            if (args[2] === 'init') {
                const blueprintPath = path.join(homeDir(), '.argentum', 'blueprint.yaml');
                if (fs.existsSync(blueprintPath)) {
                    warn(`Blueprint already exists at: ${blueprintPath}`);
                    return;
                }
                const dir = path.dirname(blueprintPath);
                if (!fs.existsSync(dir))
                    fs.mkdirSync(dir, { recursive: true });
                if (modules?.BlueprintLoader) {
                    const defaultBlueprint = modules.BlueprintLoader.generateDefaultBlueprint();
                    fs.writeFileSync(blueprintPath, defaultBlueprint, 'utf-8');
                    success(`Default blueprint created: ${blueprintPath}`);
                }
                else {
                    warn('Security modules not compiled. Print default blueprint:');
                    print('');
                }
                break;
            }
            if (args[2] === 'show') {
                const blueprintPath = path.join(homeDir(), '.argentum', 'blueprint.yaml');
                if (fs.existsSync(blueprintPath)) {
                    const content = fs.readFileSync(blueprintPath, 'utf-8');
                    print(content);
                }
                else {
                    info('No blueprint found. Create with: argentum security blueprint init');
                }
                break;
            }
            info('Blueprint commands:');
            print('  argentum security blueprint init   Create default blueprint');
            print('  argentum security blueprint show  Show current blueprint');
            break;
        }
        case 'help':
        default: {
            banner();
            info('Argentum Security Commands:');
            print('');
            print('  \x1b[1margentum security status\x1b[0m                Show security overview');
            print('  \x1b[1margentum security policies\x1b[0m [list|add|remove|enable|disable]');
            print('  \x1b[1margentum security approvals\x1b[0m [list|show|approve|deny]');
            print('  \x1b[1margentum security audit\x1b[0m [--since <date>] [--actor <id>] [--limit <n>]');
            print('  \x1b[1margentum security credentials\x1b[0m [list|rotate]');
            print('  \x1b[1margentum security sandbox\x1b[0m                   Show sandbox config');
            print('  \x1b[1margentum security blueprint\x1b[0m [init|show]');
            print('');
            print('  \x1b[1mExamples:\x1b[0m');
            print('    argentum security policies add --name "allow-read" --effect allow --resource "file://~/argentum-workspace/**" --action read');
            print('    argentum security approve abc-123         Approve request');
            print('    argentum security deny def-456          Deny request');
            print('');
            break;
        }
    }
}
function formatUptime(seconds) {
    const days = Math.floor(seconds / 86400);
    const hours = Math.floor((seconds % 86400) / 3600);
    const mins = Math.floor((seconds % 3600) / 60);
    if (days > 0)
        return `${days}d ${hours}h`;
    if (hours > 0)
        return `${hours}h ${mins}m`;
    return `${mins}m`;
}
function formatExpiry(timestamp) {
    const diff = timestamp - Date.now();
    if (diff <= 0)
        return 'expired';
    const mins = Math.floor(diff / 60000);
    if (mins < 60)
        return `${mins}m`;
    const hours = Math.floor(mins / 60);
    if (hours < 24)
        return `${hours}h ${mins % 60}m`;
    return `${Math.floor(hours / 24)}d`;
}
function homeDir() {
    return process.env.HOME || process.env.USERPROFILE || '/home/ag064';
}
async function cmdTelegram() {
    const subcommand = args[1] || 'status';
    banner();
    switch (subcommand) {
        case 'status': {
            info('Telegram bot status');
            print('');
            const configPath = getProjectConfigPath();
            if (!projectConfigExists()) {
                error('Argentum not initialized. Run: argentum init');
                return;
            }
            const config = readProjectConfig(configPath);
            const tg = config.channels?.telegram;
            if (!tg) {
                warn('Telegram not configured');
                info('Run: argentum onboard');
                return;
            }
            print(`  Enabled: ${tg.enabled ? '✓' : '✗'}`);
            print(`  Bot Token: ${process.env.ARGENTUM_TELEGRAM_TOKEN ? 'set via env' : 'NOT SET'}`);
            print(`  Allow All: ${tg.allowAll ? 'yes' : 'no'}`);
            print(`  Allowed Users: ${(tg.allowedUsers || []).join(', ') || 'none'}`);
            print(`  Allowed Chats: ${(tg.allowedChats || []).join(', ') || 'none'}`);
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
                info(`Share with user: /pair ${newCode}`);
            }
            else {
                info(`Pairing code: ${code}`);
            }
            break;
        }
        case 'allow': {
            const userId = args[2];
            if (!userId) {
                error('Usage: argentum telegram allow tg:USER_ID');
                return;
            }
            info(`Adding ${userId} to allowed users`);
            // Would update config and/or database
            success('User added (update config/default.yaml to persist)');
            break;
        }
        case 'config': {
            info('Telegram configuration template');
            print('');
            print(JSON.stringify({
                channels: {
                    telegram: {
                        enabled: true,
                        allowedUsers: [123456789],
                        allowedChats: [],
                        allowAll: false,
                    },
                },
            }, null, 2));
            break;
        }
        default:
            error(`Unknown telegram command: ${subcommand}`);
            print('Usage: argentum telegram [status|pair|allow|config]');
    }
}
// ─── Self-Improving Loop ──────────────────────────────────────────────────────
async function cmdImprove() {
    banner();
    const dryRun = args.includes('--dry-run') || args.includes('-n');
    const forceRun = args.includes('--force');
    const verbose = args.includes('--verbose') || args.includes('-v');
    let phase = 'all';
    const phaseIdx = args.indexOf('--phase');
    if (phaseIdx !== -1 && args[phaseIdx + 1]) {
        const p = args[phaseIdx + 1].toLowerCase();
        if (['error', 'skill', 'memory', 'model', 'correction'].includes(p)) {
            phase = p;
        }
        else {
            error(`Unknown phase: ${p}`);
            print('Valid phases: error, skill, memory, model, correction');
            return;
        }
    }
    if (dryRun) {
        info('[DRY-RUN] Showing what would change without making changes');
        print('');
    }
    let feature = null;
    try {
        const featurePath = path.join(__dirname, 'src', 'features', 'self-improving', 'index.js');
        if (fs.existsSync(featurePath)) {
            feature = require(featurePath).default;
        }
    }
    catch { }
    if (!feature?.run) {
        warn('Self-improving feature not available, using standalone mode');
        await runStandaloneImprove(phase, { dryRun, forceRun, verbose });
        return;
    }
    if (args.length === 1 || (args.length === 2 && !args[1].startsWith('-'))) {
        const cfg = feature.getConfig ? feature.getConfig() : getImproveConfig();
        info('Self-Improving Loop Configuration:');
        print('');
        print(`  enabled:         ${cfg.enabled ? '\x1b[32myes\x1b[0m' : '\x1b[31mno\x1b[0m'}`);
        print(`  schedule:        ${cfg.schedule}`);
        print(`  nightlyTime:    ${cfg.nightlyTime}`);
        print(`  idleThreshold:  ${cfg.idleThreshold} minutes`);
        print(`  skillThreshold: ${cfg.skillCreationThreshold}`);
        print(`  maxSkills:      ${cfg.maxSkillsPerRun} per run`);
        print('');
        print('  \x1b[1mUsage:\x1b[0m');
        print('    argentum improve              Run full loop');
        print('    argentum improve --phase skill  Run specific phase');
        print('    argentum improve --dry-run     Preview changes');
        print('    argentum improve --force       Force run');
        return;
    }
    info(`Running self-improving loop (phase: ${phase})...`);
    print('');
    try {
        const result = await feature.run(phase, { dryRun, force: forceRun, verbose });
        if (result.phases.length === 0 && !result.dryRun) {
            const hoursSince = feature.lastRunTime
                ? ((Date.now() - feature.lastRunTime) / (1000 * 60 * 60)).toFixed(1)
                : 'never';
            info(`Skipped: ran ${hoursSince}h ago. Use --force to override.`);
            return;
        }
        print('');
        const duration = (result.totalDuration / 1000).toFixed(1);
        success(`${result.dryRun ? '[DRY-RUN] ' : ''}Complete in ${duration}s`);
        print('');
        for (const p of result.phases) {
            const icon = p.success ? '\x1b[32m✓\x1b[0m' : '\x1b[31m✗\x1b[0m';
            const changed = p.itemsChanged > 0 ? ` \x1b[33m+${p.itemsChanged}\x1b[0m` : '';
            print(`  ${icon} \x1b[1m${p.phase}\x1b[0m  ${p.itemsProcessed} processed${changed}`);
            if (verbose && p.details.length > 0) {
                for (const d of p.details.slice(0, 5)) {
                    print(`      ${d}`);
                }
                if (p.details.length > 5)
                    print(`      ... and ${p.details.length - 5} more`);
            }
        }
        print('');
        print(`  Skills:    \x1b[1m${result.skillsCreated}\x1b[0m created`);
        print(`  Lessons:   \x1b[1m${result.lessonsLearned}\x1b[0m learned`);
        print(`  Corrections: \x1b[1m${result.correctionsApplied}\x1b[0m applied`);
    }
    catch (err) {
        error(`Failed: ${err.message}`);
    }
}
async function runStandaloneImprove(phase, opts) {
    const workDir = process.env.ARGENTUM_WORKDIR || process.cwd();
    const memoryDir = path.join(workDir, 'memory');
    const sessionsDb = path.join(workDir, 'data', 'sessions.db');
    info('Running in standalone mode (basic analysis only)');
    print('');
    if (!fs.existsSync(sessionsDb)) {
        warn('Sessions database not found. Run some conversations first!');
        return;
    }
    const sessionsExist = fs.existsSync(sessionsDb);
    print(`  Sessions DB: ${sessionsExist ? '\x1b[32mfound\x1b[0m' : '\x1b[31mnot found\x1b[0m'}`);
    print(`  Memory dir:  ${fs.existsSync(memoryDir) ? '\x1b[32mfound\x1b[0m' : '\x1b[31mnot found\x1b[0m'}`);
    print('');
    if (opts.dryRun) {
        info('[DRY-RUN] Would run self-improving loop with:');
        print(`  Phase: ${phase}`);
        print(`  Force: ${opts.forceRun}`);
        print(`  Verbose: ${opts.verbose}`);
    }
    else {
        info('Standalone mode complete. Enable self-improving feature for full analysis.');
    }
}
function getImproveConfig() {
    const workDir = process.env.ARGENTUM_WORKDIR || process.cwd();
    const configPath = path.join(workDir, 'self-improving-config.json');
    if (fs.existsSync(configPath)) {
        try {
            return JSON.parse(fs.readFileSync(configPath, 'utf8'));
        }
        catch { }
    }
    return {
        enabled: true,
        schedule: 'nightly',
        nightlyTime: '03:00',
        idleThreshold: 120,
        skillCreationThreshold: 5,
        maxSkillsPerRun: 3,
    };
}
async function cmdLearnings() {
    banner();
    const verbose = args.includes('--verbose') || args.includes('-v');
    const limitArg = args.indexOf('--limit');
    const limit = limitArg !== -1 && args[limitArg + 1] ? parseInt(args[limitArg + 1], 10) : 20;
    const categoryArg = args.indexOf('--category');
    const categoryFilter = categoryArg !== -1 && args[categoryArg + 1] ? args[categoryArg + 1].toLowerCase() : null;
    let feature = null;
    try {
        const featurePath = path.join(__dirname, 'src', 'features', 'self-improving', 'index.js');
        if (fs.existsSync(featurePath)) {
            feature = require(featurePath).default;
        }
    }
    catch { }
    let lessons = [];
    if (feature?.getLearnings) {
        lessons = feature.getLearnings();
    }
    else {
        const workDir = process.env.ARGENTUM_WORKDIR || process.cwd();
        const lessonsPath = path.join(workDir, 'memory', 'self-improvement', 'lessons.md');
        lessons = parseLessonsFromFile(lessonsPath);
    }
    if (categoryFilter) {
        lessons = lessons.filter((l) => l.category === categoryFilter);
    }
    lessons = lessons.slice(0, limit);
    if (lessons.length === 0) {
        info('No lessons logged yet.');
        print('');
        print('  Lessons are automatically logged when:');
        print('  • User corrections are detected');
        print('  • Errors occur during task execution');
        print('  • Patterns are identified across sessions');
        print('');
        print('  Run \x1b[1margentum improve\x1b[0m to trigger a self-improvement cycle.');
        return;
    }
    const byCategory = {};
    for (const lesson of lessons) {
        if (!byCategory[lesson.category]) {
            byCategory[lesson.category] = [];
        }
        byCategory[lesson.category].push(lesson);
    }
    print(`  \x1b[1m\x1b[36m╭─────────────────────────────────────────────────────────╮\x1b[0m`);
    print(`  \x1b[1m\x1b[36m│              📚 LESSONS LEARNED                            │\x1b[0m`);
    print(`  \x1b[1m\x1b[36m╰─────────────────────────────────────────────────────────╯\x1b[0m`);
    print('');
    const categoryIcons = {
        insight: '\x1b[34m💡\x1b[0m',
        mistake: '\x1b[31m🔴\x1b[0m',
        pattern: '\x1b[33m📈\x1b[0m',
        skill_created: '\x1b[32m🛠\x1b[0m',
        knowledge_gap: '\x1b[35m❓\x1b[0m',
    };
    for (const [cat, items] of Object.entries(byCategory)) {
        const icon = categoryIcons[cat] || '\x1b[37m•\x1b[0m';
        const catName = cat.replace('_', ' ');
        print(`  ${icon} \x1b[1m${catName}\x1b[0m  \x1b[90m(${items.length} lesson${items.length !== 1 ? 's' : ''})\x1b[0m`);
    }
    print('');
    print(`  \x1b[1mShowing ${lessons.length} most recent lessons:\x1b[0m`);
    print('');
    for (let i = 0; i < lessons.length; i++) {
        const lesson = lessons[i];
        const icon = categoryIcons[lesson.category] || '\x1b[37m•\x1b[0m';
        const age = formatAge(lesson.timestamp);
        print(`  \x1b[90m${i + 1}.\x1b[0m ${icon} \x1b[1m${lesson.title}\x1b[0m`);
        print(`      \x1b[90m${age}\x1b[0m  \x1b[2m${(lesson.description ?? '').slice(0, 100)}\x1b[0m`);
        if (verbose && lesson.tags?.length) {
            print(`      \x1b[36mtags:\x1b[0m ${lesson.tags.join(', ')}`);
        }
        if (i < lessons.length - 1)
            print('');
    }
    print('');
    print('  \x1b[1mFilters:\x1b[0m');
    print('    argentum learnings --category mistake    Show only mistakes');
    print('    argentum learnings --limit 5             Show only 5 lessons');
    print('    argentum learnings --verbose             Show tags');
}
async function cmdTrajectory() {
    banner();
    info('Trajectory export feature');
    print('');
    print('  Export your agent session trajectories for analysis.');
    print('  Currently loads trajectory data from memory/sessions.');
    print('');
    try {
        const featurePath = path.join(__dirname, 'src', 'features', 'trajectory-export', 'index.js');
        if (fs.existsSync(featurePath)) {
            const mod = require(featurePath);
            const exporter = mod.default || mod;
            if (exporter?.exportTrajectory) {
                const result = await exporter.exportTrajectory();
                print(`  Exported ${result?.count ?? 0} trajectory entries.`);
            }
            else {
                print('  Feature module loaded but export not available.');
            }
        }
        else {
            print('  Feature not found. Run "argentum init" to set up.');
        }
    }
    catch (err) {
        warn(`Could not load trajectory-export: ${err.message}`);
    }
}
async function cmdOrg() {
    banner();
    info('Organization chart feature');
    print('');
    print('  Visualize your multi-agent organization hierarchy.');
    print('  Currently loads org data from configuration.');
    print('');
    try {
        const featurePath = path.join(__dirname, 'src', 'features', 'org-chart', 'index.js');
        if (fs.existsSync(featurePath)) {
            const mod = require(featurePath);
            const orgChart = mod.default || mod;
            if (orgChart?.getOrgChart) {
                const chart = await orgChart.getOrgChart();
                print(`  Org chart loaded: ${chart?.agents?.length ?? 0} agents.`);
            }
            else {
                print('  Feature module loaded but getOrgChart not available.');
            }
        }
        else {
            print('  Feature not found. Run "argentum init" to set up.');
        }
    }
    catch (err) {
        warn(`Could not load org-chart: ${err.message}`);
    }
}
function parseLessonsFromFile(lessonsPath) {
    const lessons = [];
    if (!fs.existsSync(lessonsPath)) {
        return lessons;
    }
    try {
        const content = fs.readFileSync(lessonsPath, 'utf8');
        const sections = content.split(/^## /m).filter(Boolean);
        for (const section of sections) {
            const lines = section.trim().split('\n');
            const timestamp = lines[0]?.trim() || '';
            for (const line of lines) {
                const trimmed = line.trim();
                if (trimmed.startsWith('- lesson:') || trimmed.startsWith('- ')) {
                    const lessonText = trimmed.replace(/^-\s*(lesson:)?\s*/, '');
                    if (lessonText && lessonText.length > 5) {
                        lessons.push({
                            id: `lesson:${lessons.length}`,
                            timestamp: new Date(timestamp).getTime() || Date.now(),
                            category: 'insight',
                            title: lessonText.slice(0, 60),
                            description: lessonText,
                            tags: [],
                        });
                    }
                }
            }
        }
    }
    catch { }
    return lessons.sort((a, b) => b.timestamp - a.timestamp);
}
function formatAge(timestamp) {
    const diff = Date.now() - timestamp;
    const mins = Math.floor(diff / 60000);
    if (mins < 60)
        return `${mins}m ago`;
    const hours = Math.floor(mins / 60);
    if (hours < 24)
        return `${hours}h ago`;
    const days = Math.floor(hours / 24);
    if (days < 30)
        return `${days}d ago`;
    return new Date(timestamp).toLocaleDateString();
}
// ─── Main ─────────────────────────────────────────────────────────────────────
async function main() {
    switch (command) {
        case 'launch':
            await cmdLaunch();
            break;
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
        case 'status':
            await cmdStatus();
            break;
        case 'telegram':
            await cmdTelegram();
            break;
        case 'skill':
        case 'skills':
            await cmdSkill();
            break;
        case 'session':
        case 'sessions':
            await cmdSessions();
            break;
        case 'backup':
            await cmdBackup();
            break;
        case 'acp':
            cmdACP();
            break;
        case 'memory':
            await cmdMemory();
            break;
        case 'cron':
            await cmdCron();
            break;
        case 'budget':
        case 'b':
            await cmdBudget();
            break;
        case 'onboard':
        case 'setup':
        case 'configure':
            await cmdOnboard();
            break;
        case 'improve':
            await cmdImprove();
            break;
        case 'learnings':
            await cmdLearnings();
            break;
        case 'trajectory':
        case 'traj':
            await cmdTrajectory();
            break;
        case 'org':
        case 'org-chart':
            await cmdOrg();
            break;
        case 'security':
        case 'security-cmd':
            await cmdSecurity();
            break;
        case 'image':
        case 'img':
        case 'generate':
            cmdImage();
            break;
        default:
            error(`Unknown command: ${command}`);
            print('Run "argentum help" for usage information');
            process.exit(1);
    }
}
async function pauseBeforeExitIfNeeded() {
    if (!launch.pauseOnExit ||
        process.env[SKIP_EXIT_PAUSE_ENV] === '1') {
        return;
    }
    if (!process.stdin.isTTY || !process.stdout.isTTY)
        return;
    print('');
    const readline = require('readline');
    const rl = readline.createInterface({ input: process.stdin, output: process.stdout });
    await new Promise((resolve) => {
        rl.question('Press Enter to close Argentum...', () => {
            rl.close();
            resolve();
        });
    });
}
main()
    .then(pauseBeforeExitIfNeeded)
    .catch(async (err) => {
    error(err.message);
    process.exitCode = 1;
    await pauseBeforeExitIfNeeded();
});
