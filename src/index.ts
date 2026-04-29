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

import 'dotenv/config';

import { getConfig, type ArgentumConfig } from './core/config';
import {
  type LLMProvider,
  type Message,
  type ToolDefinition,
  type LLMResponse,
  createLLMProvider,
} from './core/llm-provider';
import { createLogger, type Logger } from './core/logger';
import { PluginLoader } from './core/plugin-loader';
import { getMemoryGraph, type MemoryGraph } from './memory/graph';
import { getSemanticMemory, type SemanticMemory } from './memory/semantic';
import {
  createCapabilityBroker,
  type CapabilityBroker,
} from './security/capability-broker';

// ─── Tool Interface ───────────────────────────────────────────────────────────

export interface Tool {
  name: string;
  description: string;
  parameters: Record<string, unknown>;
  execute: (params: Record<string, unknown>) => Promise<string>;
}

// ─── Agent Core ───────────────────────────────────────────────────────────────

export class Agent {
  private tools: Map<string, Tool> = new Map();
  private model: LLMProvider;
  private logger: Logger;
  private maxIterations = 10;
  private systemPrompt: string;

  constructor(model: LLMProvider, systemPrompt?: string) {
    this.model = model;
    this.logger = createLogger().child({ feature: 'agent' });
    this.systemPrompt =
      systemPrompt ??
      `You are Argentum, a helpful AI assistant. You have access to tools that you can use to help answer questions and complete tasks. When you need to use a tool, call it. When you have enough information, respond directly to the user.`;
  }

  registerTool(tool: Tool): void {
    this.tools.set(tool.name, tool);
    this.logger.debug(`Tool registered: ${tool.name}`);
  }

  unregisterTool(name: string): void {
    this.tools.delete(name);
  }

  getToolNames(): string[] {
    return Array.from(this.tools.keys());
  }

  async handleMessage(message: string, conversationHistory: Message[] = []): Promise<string> {
    this.logger.info('Processing message', { length: message.length });

    // Build initial messages array
    const messages: Message[] = [
      { role: 'system', content: this.systemPrompt },
      ...conversationHistory,
      { role: 'user', content: message },
    ];

    // Convert tools to LLM format
    const toolDefs: ToolDefinition[] = Array.from(this.tools.values()).map((t) => ({
      type: 'function' as const,
      function: {
        name: t.name,
        description: t.description,
        parameters: {
          type: 'object',
          properties: t.parameters,
          required: Object.keys(t.parameters).filter(
            (k) => (t.parameters[k] as Record<string, unknown>)['required'] === true,
          ),
        },
      },
    }));

    let iterations = 0;

    while (iterations < this.maxIterations) {
      iterations++;
      this.logger.debug(`Agent iteration ${iterations}/${this.maxIterations}`);

      // Send to LLM
      let response: LLMResponse;
      try {
        response = await this.model.chat(messages, toolDefs.length > 0 ? toolDefs : undefined);
      } catch (err) {
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
        const assistantMsg: Message = {
          role: 'assistant',
          content: response.content ?? '',
          tool_calls: response.toolCalls.map((tc) => ({
            id: tc.id,
            type: 'function' as const,
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
          } catch (err) {
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

// ─── Memory Backend Interface ─────────────────────────────────────────────────

export interface MemoryBackend {
  store(entry: MemoryEntry): Promise<void>;
  search(query: string, limit?: number): Promise<MemoryEntry[]>;
  getRecent(limit?: number): Promise<MemoryEntry[]>;
  delete(id: string): Promise<boolean>;
}

export interface MemoryEntry {
  id: string;
  content: string;
  createdAt: number;
  accessedAt: number;
  accessCount: number;
  metadata?: Record<string, unknown>;
}

// ─── Built-in Tools ───────────────────────────────────────────────────────────

export interface BuiltinToolOptions {
  enableFilesystemTools?: boolean;
  enableShellTool?: boolean;
  enableImageTool?: boolean;
  workspaceRoot?: string;
  capabilityAuditPath?: string;
  capabilityBroker?: CapabilityBroker;
}

function isEnvEnabled(name: string): boolean {
  const value = process.env[name]?.toLowerCase();
  return value === 'true' || value === '1' || value === 'yes';
}

function resolveBuiltinToolWorkspaceRoot(options: BuiltinToolOptions): string {
  return options.workspaceRoot ?? process.env.ARGENTUM_TOOL_ROOT ?? process.cwd();
}

export function createBuiltinTools(options: BuiltinToolOptions = {}): Tool[] {
  const workspaceRoot = resolveBuiltinToolWorkspaceRoot(options);
  const capabilityBroker =
    options.capabilityBroker ??
    createCapabilityBroker({ workspaceRoot, auditPath: options.capabilityAuditPath });
  if (options.enableShellTool && !options.capabilityBroker) {
    capabilityBroker.grant({
      action: 'shell.execute',
      resource: 'exec://*',
      scope: 'current-session',
      grantedBy: 'runtime',
      reason: 'Shell tool was explicitly enabled for this Argentum runtime session.',
    });
  }
  const tools: Tool[] = [
    {
      name: 'web_search',
      description:
        'Search the web for information. Returns search results with titles and snippets.',
      parameters: {
        query: { type: 'string', description: 'The search query', required: true },
      },
      execute: async (params) => {
        const query = params['query'] as string;
        if (!query) return 'Error: query parameter is required';

        // Use DuckDuckGo instant answer API as a lightweight search
        try {
          const response = await fetch(
            `https://api.duckduckgo.com/?q=${encodeURIComponent(query)}&format=json&no_html=1&skip_disambig=1`,
          );
          const data = (await response.json()) as Record<string, unknown>;
          const abstract = data.Abstract as string;
          const heading = data.Heading as string;
          const related = ((data.RelatedTopics as Array<{ Text: string }>) ?? [])
            .slice(0, 5)
            .map((t) => t.Text)
            .join('\n');

          if (abstract) {
            return `Search result for "${query}":\n${heading ? `${heading}\n` : ''}${abstract}`;
          } else if (related) {
            return `Search results for "${query}":\n${related}`;
          } else {
            return `No direct results for "${query}". Try a different search query.`;
          }
        } catch (err) {
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
    tools.push(
    {
      name: 'read_file',
      description: 'Read the contents of a file. Only safe paths are allowed.',
      parameters: {
        path: { type: 'string', description: 'File path to read', required: true },
      },
      execute: async (params) => {
        const { readFileSync, existsSync } = await import('fs');
        const filePath = params['path'] as string;

        if (!filePath) return 'Error: path parameter is required';

        const decision = capabilityBroker.authorize({
          action: 'file.read',
          resource: filePath,
          requester: 'builtin.read_file',
        });
        if (!decision.allowed) {
          return `Error: Path is outside the configured workspace: ${capabilityBroker.workspaceRoot}`;
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
        } catch (err) {
          return `Error reading file: ${err instanceof Error ? err.message : String(err)}`;
        }
      },
    },
    {
      name: 'write_file',
      description: 'Write content to a file. Creates directories as needed.',
      parameters: {
        path: { type: 'string', description: 'File path to write', required: true },
        content: { type: 'string', description: 'Content to write', required: true },
      },
      execute: async (params) => {
        const { writeFileSync, mkdirSync } = await import('fs');
        const { dirname } = await import('path');
        const filePath = params['path'] as string;
        const content = params['content'] as string;

        if (!filePath || content === undefined)
          return 'Error: path and content parameters are required';

        const decision = capabilityBroker.authorize({
          action: 'file.write',
          resource: filePath,
          requester: 'builtin.write_file',
        });
        if (!decision.allowed) {
          return `Error: Path is outside the configured workspace: ${capabilityBroker.workspaceRoot}`;
        }
        const safePath = decision.resolvedPath;
        if (!safePath) {
          return `Error: Path could not be resolved: ${filePath}`;
        }

        try {
          mkdirSync(dirname(safePath), { recursive: true });
          writeFileSync(safePath, content, 'utf-8');
          return `File written successfully: ${safePath} (${content.length} chars)`;
        } catch (err) {
          return `Error writing file: ${err instanceof Error ? err.message : String(err)}`;
        }
      },
    },
    );
  }

  if (options.enableShellTool) {
    tools.push({
      name: 'run_command',
      description: 'Execute a shell command. Returns stdout and stderr.',
      parameters: {
        command: { type: 'string', description: 'Shell command to execute', required: true },
      },
      execute: async (params) => {
        const { execSync } = await import('child_process');
        const cmd = params['command'] as string;

        if (!cmd) return 'Error: command parameter is required';

        // Block dangerous commands
        const blocked = ['rm -rf /', 'mkfs', 'dd if=', ':(){ :|:& };:', 'chmod 777'];
        if (blocked.some((b) => cmd.includes(b))) {
          return 'Error: This command is blocked for safety reasons.';
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
          const output = execSync(cmd, {
            cwd: capabilityBroker.workspaceRoot,
            timeout: 30000,
            encoding: 'utf-8',
            maxBuffer: 1024 * 1024,
          });
          return output.length > 5000
            ? `${output.slice(0, 5000)}\n... (truncated)`
            : output || '(no output)';
        } catch (err: unknown) {
          const e = err as { message: string; stderr?: string };
          return `Command failed: ${e.message}\n${e.stderr ?? ''}`;
        }
      },
    });
  }

  if (options.enableImageTool) {
    tools.push({
      name: 'generate_image',
      description:
        'Generate an AI image from a text prompt using Gemini 3 Pro Image (with automatic SiliconFlow FLUX.1-dev fallback on quota errors). Supports resolutions 1K, 2K, 4K and image editing.',
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
        const { spawn } = await import('child_process');
        const { existsSync } = await import('fs');
        const { join, basename: pathBasename } = await import('path');

        const homeDir = process.env.HOME || '/home/ag064';
        const scriptPath = join(
          homeDir,
          '.openclaw',
          'workspace',
          'skills',
          'image-gen',
          'scripts',
          'generate_image.py',
        );

        if (!existsSync(scriptPath)) {
          return `Error: generate_image.py not found at ${scriptPath}`;
        }

        const prompt = params['prompt'] as string;
        const filename = params['filename'] as string;
        const resolution = (params['resolution'] as string) || '1K';
        const inputImage = params['inputImage'] as string | undefined;

        if (!prompt?.trim()) return 'Error: prompt is required';
        if (!filename?.trim()) return 'Error: filename is required';

        // Returns true only if the value contains '..' as a path segment (not as part of a filename)
        const hasParentTraversalSegment = (value: string): boolean => {
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

          proc.stdout?.on('data', (d: Buffer) => { stdout += d.toString(); });
          proc.stderr?.on('data', (d: Buffer) => { stderr += d.toString(); });

          proc.on('close', (code) => {
            clearTimeout(timer);
            if (code === 0) {
              const usedFallback = stderr.includes('[SiliconFlow') || stderr.includes('fallback');
              resolve(`Image generated successfully and saved to: ${filename}\nProvider: ${usedFallback ? 'SiliconFlow FLUX.1-dev (fallback)' : 'Gemini 3 Pro Image'}`);
            } else {
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
  private config: ArgentumConfig;
  private logger: Logger;
  private pluginLoader: PluginLoader;
  private agent!: Agent;
  private llmProvider!: LLMProvider;
  private shuttingDown = false;
  private semanticMemory!: SemanticMemory;
  private memoryGraph!: MemoryGraph;

  constructor() {
    const configManager = getConfig();
    this.config = configManager.get();
    this.logger = createLogger({
      level: this.config.logging.level,
      format: this.config.logging.format,
    });
    this.pluginLoader = new PluginLoader(this.config);

    // Initialize OMEGA Memory subsystem
    this.semanticMemory = getSemanticMemory();
    this.memoryGraph = getMemoryGraph();
    this.logger.info('OMEGA Memory initialized (semantic + graph)');
  }

  /** Get the agent instance (for channels to use) */
  getAgent(): Agent {
    return this.agent;
  }

  /** Get the config */
  getConfig(): ArgentumConfig {
    return this.config;
  }

  /** Get the semantic memory instance */
  getSemanticMemory(): SemanticMemory {
    return this.semanticMemory;
  }

  /** Get the memory graph instance */
  getMemoryGraph(): MemoryGraph {
    return this.memoryGraph;
  }

  /** Start the Argentum framework */
  async start(): Promise<void> {
    this.logger.info('Starting Argentum Framework', {
      version: '0.0.3',
      nodeVersion: process.version,
      platform: process.platform,
    });

    // Register shutdown handlers
    this.registerShutdownHandlers();

    // Initialize LLM provider
    try {
      const llmConfig = (this.config as Record<string, unknown>)['llm'] as any;
      this.llmProvider = createLLMProvider({
        llm: llmConfig,
      });
      this.logger.info(
        `LLM provider initialized: ${this.llmProvider.name} (${this.llmProvider.model})`,
      );
    } catch (err) {
      this.logger.warn('LLM provider not configured, agent will have limited capabilities', {
        error: err instanceof Error ? err.message : String(err),
      });
      // Create a stub provider for testing
      this.llmProvider = {
        name: 'stub',
        model: 'none',
        baseUrl: '',
        async chat(_messages: Message[]): Promise<LLMResponse> {
          return {
            content:
              'I am running in stub mode. Please configure OPENROUTER_API_KEY or ANTHROPIC_API_KEY to enable full capabilities.',
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
      workspaceRoot:
        process.env.ARGENTUM_TOOL_ROOT ??
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
    this.pluginLoader.registerHook('webchat:message', async (data: unknown) => {
      const { content, userId, roomId } = data as { content: string; userId: string; roomId: string };
      this.logger.debug('Webchat message received', { userId, roomId });
      try {
        const response = await this.agent.handleMessage(content ?? '');
        const webchatFeature = this.pluginLoader.listFeatures().find((f) => f.name === 'webchat');
        if (webchatFeature && (webchatFeature as any).sendAssistantMessage) {
          (webchatFeature as any).sendAssistantMessage(roomId, response);
        }
      } catch (err) {
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
  private async startChannels(): Promise<void> {
    const channels = this.config.channels as Record<string, Record<string, unknown>> | undefined;

    // Start Telegram if configured
    if (channels?.['telegram']?.['enabled'] === true) {
      const token =
        (channels?.['telegram']?.['token'] as string) ??
        process.env.ARGENTUM_TELEGRAM_TOKEN ??
        process.env.TELEGRAM_BOT_TOKEN;
      if (token) {
        try {
          await this.startTelegram(token, channels?.['telegram']);
        } catch (err) {
          this.logger.error('Failed to start Telegram channel', {
            error: err instanceof Error ? err.message : String(err),
          });
        }
      } else {
        this.logger.info(
          'Telegram channel enabled but no token provided (set ARGENTUM_TELEGRAM_TOKEN or TELEGRAM_BOT_TOKEN)',
        );
      }
    }

    // Webchat could be started here too
    if (channels?.['webchat']?.['enabled'] === true) {
      this.logger.info('Webchat channel enabled (not yet implemented in entry point)');
    }
  }

  /** Start Telegram bot using grammY */
  private async startTelegram(
    token: string,
    channelConfig?: Record<string, unknown>,
  ): Promise<void> {
    try {
      const { Bot } = await import('grammy');
      const bot = new Bot(token);

      const allowedUsers = (channelConfig?.['allowedUsers'] as number[] | undefined) ?? [];
      const allowedChats = (channelConfig?.['allowedChats'] as number[] | undefined) ?? [];
      const allowAll = channelConfig?.['allowAll'] === true;
      if (!allowAll && allowedUsers.length === 0 && allowedChats.length === 0) {
        this.logger.warn('Telegram channel has no allowlist; messages will be rejected');
      }

      // Check access
      const isAllowed = (ctx: { from?: { id: number }; chat?: { id: number } }): boolean => {
        if (allowAll) return true;
        const userId = ctx.from?.id;
        const chatId = ctx.chat?.id;
        if (userId && allowedUsers.includes(userId)) return true;
        if (chatId && allowedChats.includes(chatId)) return true;
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
        if (!text) return;
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
          const autoCapture = (await import('./features/auto-capture')).default;
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
        } catch (err) {
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
        } catch (err) {
          this.logger.error('Agent error', {
            error: err instanceof Error ? err.message : String(err),
          });
          await ctx.reply(
            'Sorry, I encountered an error processing your request. Please try again.',
          );
        }
      });

      // Handle voice messages
      bot.on('message:voice', async (ctx) => {
        if (!isAllowed(ctx)) return;

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

            const transcribeResponse = await fetch(
              'https://api.openai.com/v1/audio/transcriptions',
              {
                method: 'POST',
                headers: { Authorization: `Bearer ${openaiKey}` },
                body: formData,
              },
            );

            if (transcribeResponse.ok) {
              const { text } = (await transcribeResponse.json()) as { text: string };
              this.logger.info('Voice transcribed', { text: text.slice(0, 100) });

              const agentResponse = await this.agent.handleMessage(text);
              await ctx.reply(`🎤 *Transcription:* ${text}\n\n${agentResponse}`, {
                parse_mode: 'Markdown',
              });
            } else {
              await ctx.reply('Sorry, I was unable to transcribe your voice message.');
            }
          } else {
            await ctx.reply(
              'I received your voice message, but voice transcription is not configured (OPENAI_API_KEY not set).',
            );
          }
        } catch (err) {
          this.logger.error('Voice processing error', {
            error: err instanceof Error ? err.message : String(err),
          });
          await ctx.reply('Sorry, I had trouble processing your voice message.');
        }
      });

      // Handle /start command
      bot.command('start', async (ctx) => {
        await ctx.reply(
          '🤖 Welcome to Argentum!\n\n' +
            'I am an AI assistant with tool-use capabilities. Send me a message and I will do my best to help.\n\n' +
            `Available tools: ${this.agent.getToolNames().join(', ')}`,
        );
      });

      // Handle /help command
      bot.command('help', async (ctx) => {
        const tools = this.agent.getToolNames();
        await ctx.reply(
          `🤖 *Argentum Help*\n\n` +
            `Just send me any message and I will respond.\n\n` +
            `*Available tools:*\n${tools.map((t) => `• /${t}`).join('\n')}\n\n*Commands:*\n` +
            `/start - Welcome message\n` +
            `/help - This help message\n` +
            `/status - Bot status`,
          { parse_mode: 'Markdown' },
        );
      });

      // Handle /status command
      bot.command('status', async (ctx) => {
        const features = this.pluginLoader.listFeatures();
        const activeCount = features.filter((f) => f.state === 'active').length;
        await ctx.reply(
          '📊 *Argentum Status*\n\n' +
            `LLM: ${this.llmProvider.name}\n` +
            `Tools: ${this.agent.getToolNames().length}\n` +
            `Features: ${activeCount}/${features.length} active\n` +
            `Uptime: ${Math.floor(process.uptime())}s`,
          { parse_mode: 'Markdown' },
        );
      });

      // Start bot
      bot.start({
        onStart: (info) => {
          this.logger.info('Telegram bot started', { username: info.username });
        },
      });

      this.logger.info('Telegram channel started', { username: 'starting...' });
    } catch (err) {
      this.logger.error('Failed to import grammY', {
        error: err instanceof Error ? err.message : String(err),
      });
      throw err;
    }
  }

  /** Periodic health checks for active features */
  private startHealthChecks(): void {
    setInterval(async () => {
      if (this.shuttingDown) return;

      const results = await this.pluginLoader.healthCheckAll();
      for (const [name, status] of results) {
        if (!status.healthy) {
          this.logger.warn(`Feature unhealthy: ${name}`, { message: status.message });
        }
      }
    }, 60_000); // Every minute
  }

  /** Register OMEGA Memory tools for the agent */
  private registerMemoryTools(): void {
    const self = this;

    this.agent.registerTool({
      name: 'memory_search',
      description:
        'Search semantic memory for relevant past conversations, decisions, and lessons.',
      parameters: {
        query: { type: 'string', description: 'Search query', required: true },
        limit: { type: 'number', description: 'Max results (default 5)' },
      },
      execute: async (params) => {
        const query = params['query'] as string;
        const limit = (params['limit'] as number) ?? 5;
        const results = await self.semanticMemory.search(query, limit);
        if (results.length === 0) return 'No memories found matching your query.';
        return results
          .map((r) => `[${r.type}] ${r.content.slice(0, 200)} (accessed ${r.access_count}x)`)
          .join('\n');
      },
    });

    this.agent.registerTool({
      name: 'memory_store',
      description:
        'Store a new memory entry (decision, lesson, error, preference, or general note).',
      parameters: {
        type: {
          type: 'string',
          description: 'Type: decision, lesson, error, preference, general',
          required: true,
        },
        content: { type: 'string', description: 'Content to remember', required: true },
      },
      execute: async (params) => {
        const type = params['type'] as string;
        const content = params['content'] as string;
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
        const taskId = params['taskId'] as string;
        const state = JSON.parse(params['state'] as string);
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
        const taskId = params['taskId'] as string;
        const state = await self.semanticMemory.resume(taskId);
        if (!state) return `No checkpoint found for task: ${taskId}`;
        return `Resumed task ${taskId}:\n${JSON.stringify(state, null, 2)}`;
      },
    });

    this.logger.info('OMEGA Memory tools registered', {
      tools: ['memory_search', 'memory_store', 'memory_checkpoint', 'memory_resume'],
    });
  }
  /** Register signal handlers for graceful shutdown */
  private registerShutdownHandlers(): void {
    const signals: NodeJS.Signals[] = ['SIGINT', 'SIGTERM', 'SIGUSR2'];

    for (const signal of signals) {
      process.on(signal, async () => {
        if (this.shuttingDown) return;
        this.shuttingDown = true;

        this.logger.info(`Received ${signal}, shutting down gracefully...`);
        await this.shutdown();
        process.exit(0);
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
  private async shutdown(): Promise<void> {
    const features = this.pluginLoader.listFeatures();
    const active = features.filter((f) => f.state === 'active');

    this.logger.info(`Stopping ${active.length} active features...`);

    for (const feature of active.reverse()) {
      try {
        await this.pluginLoader.disableFeature(feature.name);
      } catch (err) {
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

// ─── CLI Entry Point ──────────────────────────────────────────────────────────

// Singleton instance for import by other modules
let appInstance: Argentum | null = null;

export function getArgentum(): Argentum {
  if (!appInstance) {
    appInstance = new Argentum();
  }
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

export { Argentum };
