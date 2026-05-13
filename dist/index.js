"use strict";
/**
 * Argentum Entry Point
 *
 * Bootstraps the Argentum agent framework:
 * - Loads configuration
 * - Initializes logging
 * - Starts the plugin loader
 * - Enables configured features
 * - Implements Agentic Tool Loop
 * - Sets up graceful shutdown
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
exports.Argentum = exports.Agent = void 0;
exports.createBuiltinTools = createBuiltinTools;
exports.getArgentum = getArgentum;
require("dotenv/config");
const config_1 = require("./core/config");
const llm_provider_1 = require("./core/llm-provider");
const logger_1 = require("./core/logger");
const plugin_loader_1 = require("./core/plugin-loader");
const graph_1 = require("./memory/graph");
const semantic_1 = require("./memory/semantic");
const capability_broker_1 = require("./security/capability-broker");
// ─── Agent Core ───────────────────────────────────────────────────────────────
class Agent {
    tools = new Map();
    model;
    logger;
    maxIterations = 10;
    systemPrompt;
    constructor(model, systemPrompt) {
        this.model = model;
        this.logger = (0, logger_1.createLogger)().child({ feature: 'agent' });
        this.systemPrompt =
            systemPrompt ??
                `You are Argentum, a helpful AI assistant. You have access to tools that you can use to help answer questions and complete tasks. When you need to use a tool, call it. When you have enough information, respond directly to the user.`;
    }
    registerTool(tool) {
        this.tools.set(tool.name, tool);
        this.logger.debug(`Tool registered: ${tool.name}`);
    }
    unregisterTool(name) {
        this.tools.delete(name);
    }
    getToolNames() {
        return Array.from(this.tools.keys());
    }
    async handleMessage(message, conversationHistory = []) {
        this.logger.info('Processing message', { length: message.length });
        // Build initial messages array
        const messages = [
            { role: 'system', content: this.systemPrompt },
            ...conversationHistory,
            { role: 'user', content: message },
        ];
        // Convert tools to LLM format
        const toolDefs = Array.from(this.tools.values()).map((t) => ({
            type: 'function',
            function: {
                name: t.name,
                description: t.description,
                parameters: {
                    type: 'object',
                    properties: t.parameters,
                    required: Object.keys(t.parameters).filter((k) => t.parameters[k]['required'] === true),
                },
            },
        }));
        let iterations = 0;
        while (iterations < this.maxIterations) {
            iterations++;
            this.logger.debug(`Agent iteration ${iterations}/${this.maxIterations}`);
            // Send to LLM
            let response;
            try {
                response = await this.model.chat(messages, toolDefs.length > 0 ? toolDefs : undefined);
            }
            catch (err) {
                this.logger.error('LLM call failed', {
                    error: err instanceof Error ? err.message : String(err),
                });
                throw err;
            }
            // Track usage
            this.logger.debug('LLM usage', response.usage);
            // If we have tool calls, execute them
            if (response.toolCalls && response.toolCalls.length > 0) {
                // Add assistant message with tool calls
                const assistantMsg = {
                    role: 'assistant',
                    content: response.content ?? '',
                    tool_calls: response.toolCalls.map((tc) => ({
                        id: tc.id,
                        type: 'function',
                        function: {
                            name: tc.name,
                            arguments: JSON.stringify(tc.arguments),
                        },
                    })),
                };
                messages.push(assistantMsg);
                // Execute each tool call
                for (const toolCall of response.toolCalls) {
                    const tool = this.tools.get(toolCall.name);
                    if (!tool) {
                        this.logger.warn(`Unknown tool called: ${toolCall.name}`);
                        messages.push({
                            role: 'tool',
                            content: `Error: Tool '${toolCall.name}' not found. Available tools: ${Array.from(this.tools.keys()).join(', ')}`,
                            tool_call_id: toolCall.id,
                        });
                        continue;
                    }
                    this.logger.info(`Executing tool: ${toolCall.name}`, { args: toolCall.arguments });
                    try {
                        const result = await tool.execute(toolCall.arguments);
                        messages.push({
                            role: 'tool',
                            content: result,
                            tool_call_id: toolCall.id,
                        });
                    }
                    catch (err) {
                        const errMsg = err instanceof Error ? err.message : String(err);
                        this.logger.error(`Tool execution failed: ${toolCall.name}`, { error: errMsg });
                        messages.push({
                            role: 'tool',
                            content: `Error executing tool: ${errMsg}`,
                            tool_call_id: toolCall.id,
                        });
                    }
                }
                // Continue loop — send results back to LLM
                continue;
            }
            // No tool calls — we have a final response
            if (response.content) {
                this.logger.info('Agent response ready', { iterations, length: response.content.length });
                return response.content;
            }
            // Empty response — shouldn't happen but handle it
            this.logger.warn('Empty LLM response, retrying');
            continue;
        }
        // Max iterations reached
        this.logger.warn('Max agent iterations reached');
        return 'I apologize, but I was unable to complete your request after multiple attempts. Please try rephrasing your question.';
    }
}
exports.Agent = Agent;
function isEnvEnabled(name) {
    const value = process.env[name]?.toLowerCase();
    return value === 'true' || value === '1' || value === 'yes';
}
function resolveBuiltinToolWorkspaceRoot(options) {
    return options.workspaceRoot ?? process.env.ARGENTUM_TOOL_ROOT ?? process.cwd();
}
function createBuiltinTools(options = {}) {
    const workspaceRoot = resolveBuiltinToolWorkspaceRoot(options);
    const capabilityBroker = options.capabilityBroker ??
        (0, capability_broker_1.createCapabilityBroker)({ workspaceRoot, auditPath: options.capabilityAuditPath });
    if (options.enableShellTool && !options.capabilityBroker) {
        capabilityBroker.grant({
            action: 'shell.execute',
            resource: 'exec://*',
            scope: 'current-session',
            grantedBy: 'runtime',
            reason: 'Shell tool was explicitly enabled for this Argentum runtime session.',
        });
    }
    const tools = [
        {
            name: 'web_search',
            description: 'Search the web for information. Returns search results with titles and snippets.',
            parameters: {
                query: { type: 'string', description: 'The search query', required: true },
            },
            execute: async (params) => {
                const query = params['query'];
                if (!query)
                    return 'Error: query parameter is required';
                // Use DuckDuckGo instant answer API as a lightweight search
                try {
                    const response = await fetch(`https://api.duckduckgo.com/?q=${encodeURIComponent(query)}&format=json&no_html=1&skip_disambig=1`);
                    const data = (await response.json());
                    const abstract = data.Abstract;
                    const heading = data.Heading;
                    const related = (data.RelatedTopics ?? [])
                        .slice(0, 5)
                        .map((t) => t.Text)
                        .join('\n');
                    if (abstract) {
                        return `Search result for "${query}":\n${heading ? `${heading}\n` : ''}${abstract}`;
                    }
                    else if (related) {
                        return `Search results for "${query}":\n${related}`;
                    }
                    else {
                        return `No direct results for "${query}". Try a different search query.`;
                    }
                }
                catch (err) {
                    return `Search failed: ${err instanceof Error ? err.message : String(err)}`;
                }
            },
        },
        {
            name: 'get_current_time',
            description: 'Get the current date and time in ISO format.',
            parameters: {},
            execute: async () => {
                return new Date().toISOString();
            },
        },
    ];
    if (options.enableFilesystemTools) {
        tools.push({
            name: 'read_file',
            description: 'Read the contents of a file. Only safe paths are allowed.',
            parameters: {
                path: { type: 'string', description: 'File path to read', required: true },
            },
            execute: async (params) => {
                const { readFileSync, existsSync } = await Promise.resolve().then(() => __importStar(require('fs')));
                const filePath = params['path'];
                if (!filePath)
                    return 'Error: path parameter is required';
                const decision = capabilityBroker.authorize({
                    action: 'file.read',
                    resource: filePath,
                    requester: 'builtin.read_file',
                });
                if (!decision.allowed) {
                    if (decision.reason === 'outside-workspace') {
                        return `Error: Path is outside the configured workspace: ${capabilityBroker.workspaceRoot}`;
                    }
                    return `Error: Access denied (${decision.reason}): ${filePath}`;
                }
                const safePath = decision.resolvedPath;
                if (!safePath) {
                    return `Error: Path could not be resolved: ${filePath}`;
                }
                if (!existsSync(safePath)) {
                    return `Error: File not found: ${safePath}`;
                }
                try {
                    const content = readFileSync(safePath, 'utf-8');
                    return content.length > 5000 ? `${content.slice(0, 5000)}\n... (truncated)` : content;
                }
                catch (err) {
                    return `Error reading file: ${err instanceof Error ? err.message : String(err)}`;
                }
            },
        }, {
            name: 'write_file',
            description: 'Write content to a file. Creates directories as needed.',
            parameters: {
                path: { type: 'string', description: 'File path to write', required: true },
                content: { type: 'string', description: 'Content to write', required: true },
            },
            execute: async (params) => {
                const { writeFileSync, mkdirSync } = await Promise.resolve().then(() => __importStar(require('fs')));
                const { dirname } = await Promise.resolve().then(() => __importStar(require('path')));
                const filePath = params['path'];
                const content = params['content'];
                if (!filePath || content === undefined)
                    return 'Error: path and content parameters are required';
                const decision = capabilityBroker.authorize({
                    action: 'file.write',
                    resource: filePath,
                    requester: 'builtin.write_file',
                });
                if (!decision.allowed) {
                    if (decision.reason === 'outside-workspace') {
                        return `Error: Path is outside the configured workspace: ${capabilityBroker.workspaceRoot}`;
                    }
                    return `Error: Access denied (${decision.reason}): ${filePath}`;
                }
                const safePath = decision.resolvedPath;
                if (!safePath) {
                    return `Error: Path could not be resolved: ${filePath}`;
                }
                try {
                    mkdirSync(dirname(safePath), { recursive: true });
                    writeFileSync(safePath, content, 'utf-8');
                    return `File written successfully: ${safePath} (${content.length} chars)`;
                }
                catch (err) {
                    return `Error writing file: ${err instanceof Error ? err.message : String(err)}`;
                }
            },
        });
    }
    if (options.enableShellTool) {
        tools.push({
            name: 'run_command',
            description: 'Execute a shell command. Returns stdout and stderr.',
            parameters: {
                command: { type: 'string', description: 'Shell command to execute', required: true },
            },
            execute: async (params) => {
                const { spawnSync } = await Promise.resolve().then(() => __importStar(require('child_process')));
                const cmd = params['command'];
                if (!cmd)
                    return 'Error: command parameter is required';
                const tokenize = (input) => {
                    const tokens = [];
                    let current = '';
                    let quote = null;
                    for (let i = 0; i < input.length; i++) {
                        const ch = input[i];
                        if (quote === null) {
                            if (ch === "'" || ch === '"') {
                                quote = ch === "'" ? 'single' : 'double';
                                continue;
                            }
                            if (ch === '\\') {
                                const next = input[i + 1];
                                if (next === undefined)
                                    return { error: 'Trailing backslash in command' };
                                current += next;
                                i++;
                                continue;
                            }
                            if (/\s/.test(ch)) {
                                if (current.length > 0) {
                                    tokens.push(current);
                                    current = '';
                                }
                                continue;
                            }
                            current += ch;
                            continue;
                        }
                        if (quote === 'single') {
                            if (ch === "'") {
                                quote = null;
                                continue;
                            }
                            current += ch;
                            continue;
                        }
                        // double quote
                        if (ch === '"') {
                            quote = null;
                            continue;
                        }
                        if (ch === '\\') {
                            const next = input[i + 1];
                            if (next === undefined)
                                return { error: 'Trailing backslash in command' };
                            current += next;
                            i++;
                            continue;
                        }
                        current += ch;
                    }
                    if (quote !== null)
                        return { error: 'Unterminated quote in command' };
                    if (current.length > 0)
                        tokens.push(current);
                    return tokens;
                };
                const parsed = tokenize(cmd.trim());
                if (!Array.isArray(parsed))
                    return `Error: ${parsed.error}`;
                // Intentionally restrict supported commands: this tool is a privileged escape hatch
                // and should not provide a general-purpose shell surface area.
                const [program, ...args] = parsed;
                if (program !== 'node') {
                    return 'Error: Only "node -e <code>" and "node -v/--version" are supported.';
                }
                if (!(args.length === 1 && (args[0] === '-v' || args[0] === '--version')) &&
                    !(args.length === 2 && args[0] === '-e' && typeof args[1] === 'string')) {
                    return 'Error: Only "node -e <code>" and "node -v/--version" are supported.';
                }
                if (args[0] === '-e' && args[1].length > 10_000) {
                    return 'Error: Inline node script is too large.';
                }
                const decision = capabilityBroker.authorize({
                    action: 'shell.execute',
                    resource: cmd,
                    requester: 'builtin.run_command',
                    metadata: { command: cmd },
                });
                if (!decision.allowed) {
                    return `Error: Command is not permitted by current capability policy: ${decision.reason}`;
                }
                try {
                    const result = spawnSync(program, args, {
                        cwd: capabilityBroker.workspaceRoot,
                        timeout: 30000,
                        encoding: 'utf-8',
                        maxBuffer: 1024 * 1024,
                        shell: false,
                    });
                    if (result.error) {
                        return `Command failed: ${result.error.message}`;
                    }
                    const stdout = String(result.stdout ?? '');
                    const stderr = String(result.stderr ?? '');
                    if (result.status === 0) {
                        const output = stdout || '(no output)';
                        return output.length > 5000 ? `${output.slice(0, 5000)}\n... (truncated)` : output;
                    }
                    return `Command failed (exit code ${result.status ?? 1}).\n${stderr || stdout}`;
                }
                catch (err) {
                    return `Command failed: ${err instanceof Error ? err.message : String(err)}`;
                }
            },
        });
    }
    if (options.enableImageTool) {
        tools.push({
            name: 'generate_image',
            description: 'Generate an AI image from a text prompt using Gemini 3 Pro Image (with automatic SiliconFlow FLUX.1-dev fallback on quota errors). Supports resolutions 1K, 2K, 4K and image editing.',
            parameters: {
                prompt: {
                    type: 'string',
                    description: 'Text description of the image to generate',
                    required: true,
                },
                filename: {
                    type: 'string',
                    description: 'Output filename (e.g., image.png)',
                    required: true,
                },
                resolution: {
                    type: 'string',
                    description: 'Image resolution: 1K (1024x1024, default), 2K (2048x2048), 4K',
                    required: false,
                },
                inputImage: {
                    type: 'string',
                    description: 'Optional input image filename (simple filename only, no path separators) for editing',
                    required: false,
                },
            },
            execute: async (params) => {
                const { spawn } = await Promise.resolve().then(() => __importStar(require('child_process')));
                const { existsSync } = await Promise.resolve().then(() => __importStar(require('fs')));
                const { join, basename: pathBasename } = await Promise.resolve().then(() => __importStar(require('path')));
                const homeDir = process.env.HOME ?? '/home/ag064';
                const scriptPath = join(homeDir, '.openclaw', 'workspace', 'skills', 'image-gen', 'scripts', 'generate_image.py');
                if (!existsSync(scriptPath)) {
                    return `Error: generate_image.py not found at ${scriptPath}`;
                }
                const prompt = params['prompt'];
                const filename = params['filename'];
                const resolution = params['resolution'] || '1K';
                const inputImage = params['inputImage'];
                if (!prompt?.trim())
                    return 'Error: prompt is required';
                if (!filename?.trim())
                    return 'Error: filename is required';
                // Returns true only if the value contains '..' as a path segment (not as part of a filename)
                const hasParentTraversalSegment = (value) => {
                    const normalized = value.replace(/\\/g, '/');
                    return normalized.split('/').some((segment) => segment === '..');
                };
                // Validate filename and inputImage to prevent path traversal:
                // basename check rejects any path with directory components on any platform
                if (pathBasename(filename) !== filename || hasParentTraversalSegment(filename)) {
                    return 'Error: filename must be a simple filename without path separators or traversal sequences';
                }
                if (inputImage && (pathBasename(inputImage) !== inputImage || hasParentTraversalSegment(inputImage))) {
                    return 'Error: inputImage must be a simple filename without path separators or traversal sequences';
                }
                const args = [
                    'run', 'python3', scriptPath,
                    '--prompt', prompt,
                    '--filename', filename,
                    '--resolution', resolution,
                ];
                if (inputImage) {
                    args.push('--input-image', inputImage);
                }
                return new Promise((resolve) => {
                    const env = {
                        ...process.env,
                        ...(process.env.GEMINI_API_KEY ? { GEMINI_API_KEY: process.env.GEMINI_API_KEY } : {}),
                    };
                    const proc = spawn('uv', args, { env });
                    let stdout = '';
                    let stderr = '';
                    const timer = setTimeout(() => {
                        proc.kill('SIGKILL');
                        resolve('Error: Image generation timed out after 180s');
                    }, 180_000);
                    proc.stdout?.on('data', (d) => { stdout += d.toString(); });
                    proc.stderr?.on('data', (d) => { stderr += d.toString(); });
                    proc.on('close', (code) => {
                        clearTimeout(timer);
                        if (code === 0) {
                            const usedFallback = stderr.includes('[SiliconFlow') || stderr.includes('fallback');
                            resolve(`Image generated successfully and saved to: ${filename}\nProvider: ${usedFallback ? 'SiliconFlow FLUX.1-dev (fallback)' : 'Gemini 3 Pro Image'}`);
                        }
                        else {
                            resolve(`Image generation failed (exit code ${code}). Check stderr: ${stderr.slice(0, 500)}`);
                        }
                    });
                    proc.on('error', (err) => {
                        clearTimeout(timer);
                        resolve(`Error: Failed to start uv - ${err.message}. Make sure "uv" is installed.`);
                    });
                });
            },
        });
    }
    return tools;
}
// ─── Argentum Main Application ────────────────────────────────────────────────
class Argentum {
    config;
    logger;
    pluginLoader;
    agent;
    llmProvider;
    shuttingDown = false;
    semanticMemory;
    memoryGraph;
    constructor() {
        const configManager = (0, config_1.getConfig)();
        this.config = configManager.get();
        this.logger = (0, logger_1.createLogger)({
            level: this.config.logging.level,
            format: this.config.logging.format,
        });
        this.pluginLoader = new plugin_loader_1.PluginLoader(this.config);
        // Initialize OMEGA Memory subsystem
        this.semanticMemory = (0, semantic_1.getSemanticMemory)();
        this.memoryGraph = (0, graph_1.getMemoryGraph)();
        this.logger.info('OMEGA Memory initialized (semantic + graph)');
    }
    /** Get the agent instance (for channels to use) */
    getAgent() {
        return this.agent;
    }
    /** Get the config */
    getConfig() {
        return this.config;
    }
    /** Get the semantic memory instance */
    getSemanticMemory() {
        return this.semanticMemory;
    }
    /** Get the memory graph instance */
    getMemoryGraph() {
        return this.memoryGraph;
    }
    /** Start the Argentum framework */
    async start() {
        this.logger.info('Starting Argentum Framework', {
            version: '0.0.5',
            nodeVersion: process.version,
            platform: process.platform,
        });
        // Register shutdown handlers
        this.registerShutdownHandlers();
        // Initialize LLM provider
        try {
            this.llmProvider = (0, llm_provider_1.createLLMProvider)({
                llm: this.config.llm,
            });
            this.logger.info(`LLM provider initialized: ${this.llmProvider.name} (${this.llmProvider.model})`);
        }
        catch (err) {
            this.logger.warn('LLM provider not configured, agent will have limited capabilities', {
                error: err instanceof Error ? err.message : String(err),
            });
            // Create a stub provider for testing
            this.llmProvider = {
                name: 'stub',
                model: 'none',
                baseUrl: '',
                async chat(_messages) {
                    return {
                        content: 'I am running in stub mode. Please configure OPENROUTER_API_KEY or ANTHROPIC_API_KEY to enable full capabilities.',
                        usage: { prompt: 0, completion: 0 },
                    };
                },
            };
        }
        // Initialize Agent
        this.agent = new Agent(this.llmProvider);
        const enableFilesystemTools = isEnvEnabled('ARGENTUM_ENABLE_FILESYSTEM_TOOLS');
        const enableShellTool = isEnvEnabled('ARGENTUM_ENABLE_SHELL_TOOL');
        const enableImageTool = isEnvEnabled('ARGENTUM_ENABLE_IMAGE_TOOL');
        if (enableFilesystemTools || enableShellTool) {
            this.logger.warn('Optional built-in filesystem or shell tools are enabled by environment override');
        }
        // Register built-in tools
        for (const tool of createBuiltinTools({
            enableFilesystemTools,
            enableShellTool,
            enableImageTool,
            workspaceRoot: process.env.ARGENTUM_TOOL_ROOT ??
                this.config.security.capabilities.workspaceRoot,
            capabilityAuditPath: this.config.security.capabilities.auditPath,
        })) {
            this.agent.registerTool(tool);
        }
        // Register OMEGA Memory tools
        this.registerMemoryTools();
        // Load and enable features
        await this.pluginLoader.loadAll();
        await this.pluginLoader.enableAll();
        // Register webchat message handler so messages reach the agent
        this.pluginLoader.registerHook('webchat:message', async (data) => {
            const { content, userId, roomId } = data;
            this.logger.debug('Webchat message received', { userId, roomId });
            try {
                const response = await this.agent.handleMessage(content ?? '');
                const webchatFeature = this.pluginLoader.getFeature('webchat');
                webchatFeature?.sendAssistantMessage(roomId, response);
            }
            catch (err) {
                this.logger.error('Webchat agent error', { error: err instanceof Error ? err.message : String(err) });
            }
        });
        // Emit startup hook for OMEGA Memory features (auto-capture, consolidation, checkpoint)
        (await this.pluginLoader['emitHook']?.('system:start', { timestamp: Date.now() })) ??
            this.logger.debug('Hook emission not available on plugin loader');
        // Start channels
        await this.startChannels();
        const features = this.pluginLoader.listFeatures();
        const activeCount = features.filter((f) => f.state === 'active').length;
        const totalCount = features.length;
        this.logger.info(`ARGENTUM started successfully`, {
            features: `${activeCount}/${totalCount} active`,
            tools: this.agent.getToolNames().length,
            port: this.config.server.port,
        });
        // Start health check interval
        this.startHealthChecks();
    }
    /** Start configured channels (Telegram, Webchat) */
    async startChannels() {
        const channels = this.config.channels;
        // Start Telegram if configured
        if (channels?.['telegram']?.['enabled'] === true) {
            const token = channels?.['telegram']?.['token'] ??
                process.env.ARGENTUM_TELEGRAM_TOKEN ??
                process.env.TELEGRAM_BOT_TOKEN;
            if (token) {
                try {
                    await this.startTelegram(token, channels?.['telegram']);
                }
                catch (err) {
                    this.logger.error('Failed to start Telegram channel', {
                        error: err instanceof Error ? err.message : String(err),
                    });
                }
            }
            else {
                this.logger.info('Telegram channel enabled but no token provided (set ARGENTUM_TELEGRAM_TOKEN or TELEGRAM_BOT_TOKEN)');
            }
        }
        // Webchat could be started here too
        if (channels?.['webchat']?.['enabled'] === true) {
            this.logger.info('Webchat channel enabled (not yet implemented in entry point)');
        }
    }
    /** Start Telegram bot using grammY */
    async startTelegram(token, channelConfig) {
        try {
            const { Bot } = await Promise.resolve().then(() => __importStar(require('grammy')));
            const bot = new Bot(token);
            const allowedUsers = channelConfig?.['allowedUsers'] ?? [];
            const allowedChats = channelConfig?.['allowedChats'] ?? [];
            const allowAll = channelConfig?.['allowAll'] === true;
            if (!allowAll && allowedUsers.length === 0 && allowedChats.length === 0) {
                this.logger.warn('Telegram channel has no allowlist; messages will be rejected');
            }
            // Check access
            const isAllowed = (ctx) => {
                if (allowAll)
                    return true;
                const userId = ctx.from?.id;
                const chatId = ctx.chat?.id;
                if (userId && allowedUsers.includes(userId))
                    return true;
                if (chatId && allowedChats.includes(chatId))
                    return true;
                return false;
            };
            // Handle text messages
            bot.on('message:text', async (ctx) => {
                if (!isAllowed(ctx)) {
                    this.logger.warn('Unauthorized access attempt', {
                        userId: ctx.from?.id,
                        chatId: ctx.chat?.id,
                    });
                    return;
                }
                const text = ctx.message.text;
                if (!text)
                    return;
                if (text.length > 10_000) {
                    await ctx.reply('Message is too long.');
                    return;
                }
                this.logger.info('Telegram message received', {
                    userId: ctx.from?.id,
                    chatId: ctx.chat?.id,
                    length: text.length,
                });
                // Auto-capture: analyze incoming message for decisions/lessons/errors
                try {
                    const autoCapture = (await Promise.resolve().then(() => __importStar(require('./features/auto-capture')))).default;
                    const captures = autoCapture.detectCaptures(text, `telegram:${ctx.from?.id}`);
                    if (captures.length > 0) {
                        this.logger.info('Auto-captured items', {
                            count: captures.length,
                            types: captures.map((c) => c.type),
                        });
                        for (const capture of captures) {
                            await this.semanticMemory.store(capture.type, capture.content, {
                                source: 'telegram',
                                userId: ctx.from?.id,
                                confidence: capture.confidence,
                            });
                        }
                    }
                }
                catch (err) {
                    this.logger.debug('Auto-capture skipped', {
                        error: err instanceof Error ? err.message : String(err),
                    });
                }
                // Emit message received hook
                await this.pluginLoader['emitHook']?.('message:received', {
                    text,
                    userId: ctx.from?.id,
                    chatId: ctx.chat?.id,
                });
                // Show typing indicator
                await ctx.replyWithChatAction('typing');
                try {
                    const response = await this.agent.handleMessage(text);
                    await ctx.reply(response);
                }
                catch (err) {
                    this.logger.error('Agent error', {
                        error: err instanceof Error ? err.message : String(err),
                    });
                    await ctx.reply('Sorry, I encountered an error processing your request. Please try again.');
                }
            });
            // Handle voice messages
            bot.on('message:voice', async (ctx) => {
                if (!isAllowed(ctx))
                    return;
                this.logger.info('Voice message received', {
                    userId: ctx.from?.id,
                    chatId: ctx.chat?.id,
                });
                try {
                    // Download voice file
                    const file = await ctx.getFile();
                    const fileUrl = `https://api.telegram.org/file/bot${token}/${file.file_path}`;
                    const response = await fetch(fileUrl);
                    const audioBuffer = Buffer.from(await response.arrayBuffer());
                    // Try to transcribe with Whisper if available
                    const openaiKey = process.env.OPENAI_API_KEY;
                    if (openaiKey) {
                        await ctx.replyWithChatAction('typing');
                        const formData = new FormData();
                        const blob = new Blob([audioBuffer], { type: 'audio/ogg' });
                        formData.append('file', blob, 'voice.ogg');
                        formData.append('model', 'whisper-1');
                        const transcribeResponse = await fetch('https://api.openai.com/v1/audio/transcriptions', {
                            method: 'POST',
                            headers: { Authorization: `Bearer ${openaiKey}` },
                            body: formData,
                        });
                        if (transcribeResponse.ok) {
                            const { text } = (await transcribeResponse.json());
                            this.logger.info('Voice transcribed', { text: text.slice(0, 100) });
                            const agentResponse = await this.agent.handleMessage(text);
                            await ctx.reply(`🎤 *Transcription:* ${text}\n\n${agentResponse}`, {
                                parse_mode: 'Markdown',
                            });
                        }
                        else {
                            await ctx.reply('Sorry, I was unable to transcribe your voice message.');
                        }
                    }
                    else {
                        await ctx.reply('I received your voice message, but voice transcription is not configured (OPENAI_API_KEY not set).');
                    }
                }
                catch (err) {
                    this.logger.error('Voice processing error', {
                        error: err instanceof Error ? err.message : String(err),
                    });
                    await ctx.reply('Sorry, I had trouble processing your voice message.');
                }
            });
            // Handle /start command
            bot.command('start', async (ctx) => {
                await ctx.reply('🤖 Welcome to Argentum!\n\n' +
                    'I am an AI assistant with tool-use capabilities. Send me a message and I will do my best to help.\n\n' +
                    `Available tools: ${this.agent.getToolNames().join(', ')}`);
            });
            // Handle /help command
            bot.command('help', async (ctx) => {
                const tools = this.agent.getToolNames();
                await ctx.reply(`🤖 *Argentum Help*\n\n` +
                    `Just send me any message and I will respond.\n\n` +
                    `*Available tools:*\n${tools.map((t) => `• /${t}`).join('\n')}\n\n*Commands:*\n` +
                    `/start - Welcome message\n` +
                    `/help - This help message\n` +
                    `/status - Bot status`, { parse_mode: 'Markdown' });
            });
            // Handle /status command
            bot.command('status', async (ctx) => {
                const features = this.pluginLoader.listFeatures();
                const activeCount = features.filter((f) => f.state === 'active').length;
                await ctx.reply('📊 *Argentum Status*\n\n' +
                    `LLM: ${this.llmProvider.name}\n` +
                    `Tools: ${this.agent.getToolNames().length}\n` +
                    `Features: ${activeCount}/${features.length} active\n` +
                    `Uptime: ${Math.floor(process.uptime())}s`, { parse_mode: 'Markdown' });
            });
            // Start bot
            void bot
                .start({
                onStart: (info) => {
                    this.logger.info('Telegram bot started', { username: info.username });
                },
            })
                .catch((error) => {
                this.logger.error('Telegram bot stopped with an error', {
                    error: error instanceof Error ? error.message : String(error),
                });
            });
            this.logger.info('Telegram channel started', { username: 'starting...' });
        }
        catch (err) {
            this.logger.error('Failed to import grammY', {
                error: err instanceof Error ? err.message : String(err),
            });
            throw err;
        }
    }
    /** Periodic health checks for active features */
    startHealthChecks() {
        setInterval(() => {
            void (async () => {
                if (this.shuttingDown)
                    return;
                const results = await this.pluginLoader.healthCheckAll();
                for (const [name, status] of results) {
                    if (!status.healthy) {
                        this.logger.warn(`Feature unhealthy: ${name}`, { message: status.message });
                    }
                }
            })().catch((error) => {
                this.logger.error('Health check failed', {
                    error: error instanceof Error ? error.message : String(error),
                });
            });
        }, 60_000); // Every minute
    }
    /** Register OMEGA Memory tools for the agent */
    registerMemoryTools() {
        const self = this;
        this.agent.registerTool({
            name: 'memory_search',
            description: 'Search semantic memory for relevant past conversations, decisions, and lessons.',
            parameters: {
                query: { type: 'string', description: 'Search query', required: true },
                limit: { type: 'number', description: 'Max results (default 5)' },
            },
            execute: async (params) => {
                const query = params['query'];
                const limit = params['limit'] ?? 5;
                const results = await self.semanticMemory.search(query, limit);
                if (results.length === 0)
                    return 'No memories found matching your query.';
                return results
                    .map((r) => `[${r.type}] ${r.content.slice(0, 200)} (accessed ${r.access_count}x)`)
                    .join('\n');
            },
        });
        this.agent.registerTool({
            name: 'memory_store',
            description: 'Store a new memory entry (decision, lesson, error, preference, or general note).',
            parameters: {
                type: {
                    type: 'string',
                    description: 'Type: decision, lesson, error, preference, general',
                    required: true,
                },
                content: { type: 'string', description: 'Content to remember', required: true },
            },
            execute: async (params) => {
                const type = params['type'];
                const content = params['content'];
                const id = await self.semanticMemory.store(type, content, { source: 'agent_tool' });
                return `Memory stored: ${id} (${type})`;
            },
        });
        this.agent.registerTool({
            name: 'memory_checkpoint',
            description: 'Save a checkpoint for a task to resume later.',
            parameters: {
                taskId: { type: 'string', description: 'Unique task identifier', required: true },
                state: { type: 'string', description: 'JSON state to save', required: true },
            },
            execute: async (params) => {
                const taskId = params['taskId'];
                const state = JSON.parse(params['state']);
                await self.semanticMemory.checkpoint(taskId, state);
                return `Checkpoint saved for task: ${taskId}`;
            },
        });
        this.agent.registerTool({
            name: 'memory_resume',
            description: 'Resume a previously checkpointed task.',
            parameters: {
                taskId: { type: 'string', description: 'Task identifier to resume', required: true },
            },
            execute: async (params) => {
                const taskId = params['taskId'];
                const state = await self.semanticMemory.resume(taskId);
                if (!state)
                    return `No checkpoint found for task: ${taskId}`;
                return `Resumed task ${taskId}:\n${JSON.stringify(state, null, 2)}`;
            },
        });
        this.logger.info('OMEGA Memory tools registered', {
            tools: ['memory_search', 'memory_store', 'memory_checkpoint', 'memory_resume'],
        });
    }
    /** Register signal handlers for graceful shutdown */
    registerShutdownHandlers() {
        const signals = ['SIGINT', 'SIGTERM', 'SIGUSR2'];
        for (const signal of signals) {
            process.on(signal, () => {
                if (this.shuttingDown)
                    return;
                this.shuttingDown = true;
                void (async () => {
                    this.logger.info(`Received ${signal}, shutting down gracefully...`);
                    await this.shutdown();
                    process.exit(0);
                })().catch((error) => {
                    this.logger.error('Shutdown failed', {
                        error: error instanceof Error ? error.message : String(error),
                    });
                    process.exit(1);
                });
            });
        }
        process.on('uncaughtException', (err) => {
            this.logger.error('Uncaught exception', { error: err.message, stack: err.stack });
        });
        process.on('unhandledRejection', (reason) => {
            this.logger.error('Unhandled rejection', {
                reason: reason instanceof Error ? reason.message : String(reason),
            });
        });
    }
    /** Graceful shutdown */
    async shutdown() {
        const features = this.pluginLoader.listFeatures();
        const active = features.filter((f) => f.state === 'active');
        this.logger.info(`Stopping ${active.length} active features...`);
        for (const feature of active.reverse()) {
            try {
                await this.pluginLoader.disableFeature(feature.name);
            }
            catch (err) {
                this.logger.error(`Error stopping feature: ${feature.name}`, {
                    error: err instanceof Error ? err.message : String(err),
                });
            }
        }
        // Close OMEGA Memory
        this.semanticMemory.close();
        this.logger.info('OMEGA Memory closed');
        this.logger.info('ARGENTUM shutdown complete');
    }
}
exports.Argentum = Argentum;
// ─── CLI Entry Point ──────────────────────────────────────────────────────────
// Singleton instance for import by other modules
let appInstance = null;
function getArgentum() {
    appInstance ??= new Argentum();
    return appInstance;
}
// Start if run directly
if (require.main === module) {
    const app = new Argentum();
    app.start().catch((err) => {
        console.error('Failed to start ARGENTUM:', err);
        process.exit(1);
    });
}
//# sourceMappingURL=index.js.map