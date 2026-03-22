/**
 * AG-Claw MCP Server
 * 
 * Model Context Protocol (MCP) server implementation for AG-Claw.
 * Provides tools and resources compatible with Claude Code.
 */

import { EventEmitter } from 'events';
import { Logger } from '../core/logger';
import { Tool, ToolCall, ToolResult, Resource, ResourceTemplate } from './types';

export interface MCPServerConfig {
  name: string;
  version: string;
  capabilities: {
    tools?: boolean;
    resources?: boolean;
    prompts?: boolean;
  };
}

export interface MCPServerOptions {
  tools?: Tool[];
  resources?: Resource[];
  resourceTemplates?: ResourceTemplate[];
}

/**
 * MCP Server implementation
 */
export class MCPServer extends EventEmitter {
  public readonly config: MCPServerConfig;
  private tools: Map<string, Tool> = new Map();
  private resources: Map<string, Resource> = new Map();
  private resourceTemplates: Map<string, ResourceTemplate> = new Map();
  private logger: Logger;

  constructor(config: MCPServerConfig, options: MCPServerOptions = {}, logger?: Logger) {
    super();
    this.config = config;
    this.logger = logger || new Logger({ name: 'mcp-server' });

    // Register tools
    if (options.tools) {
      for (const tool of options.tools) {
        this.registerTool(tool);
      }
    }

    // Register resources
    if (options.resources) {
      for (const resource of options.resources) {
        this.registerResource(resource);
      }
    }

    // Register resource templates
    if (options.resourceTemplates) {
      for (const template of options.resourceTemplates) {
        this.registerResourceTemplate(template);
      }
    }

    this.logger.info('MCP Server initialized', {
      tools: this.tools.size,
      resources: this.resources.size,
    });
  }

  /**
   * Register a tool
   */
  public registerTool(tool: Tool): void {
    if (this.tools.has(tool.name)) {
      this.logger.warn(`Tool ${tool.name} already registered, overwriting`);
    }
    this.tools.set(tool.name, tool);
    this.logger.debug(`Tool registered: ${tool.name}`);
  }

  /**
   * Register a resource
   */
  public registerResource(resource: Resource): void {
    if (this.resources.has(resource.uri)) {
      this.logger.warn(`Resource ${resource.uri} already registered`);
      return;
    }
    this.resources.set(resource.uri, resource);
    this.logger.debug(`Resource registered: ${resource.uri}`);
  }

  /**
   * Register a resource template
   */
  public registerResourceTemplate(template: ResourceTemplate): void {
    if (this.resourceTemplates.has(template.uriTemplate)) {
      this.logger.warn(`Resource template ${template.uriTemplate} already registered`);
      return;
    }
    this.resourceTemplates.set(template.uriTemplate, template);
    this.logger.debug(`Resource template registered: ${template.uriTemplate}`);
  }

  /**
   * Handle incoming tool call from client
   */
  public async handleToolCall(call: ToolCall): Promise<ToolResult> {
    const tool = this.tools.get(call.name);

    if (!tool) {
      return {
        tool: call.name,
        success: false,
        error: `Tool not found: ${call.name}`,
      };
    }

    try {
      this.logger.debug(`Executing tool: ${call.name}`, { input: call.input });
      const result = await tool.handler(call.input);
      
      this.emit('tool-call', { tool: call.name, input: call.input, result });

      return {
        tool: call.name,
        success: true,
        result,
      };
    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : String(error);
      this.logger.error(`Tool ${call.name} failed`, { error: errorMessage });
      
      return {
        tool: call.name,
        success: false,
        error: errorMessage,
      };
    }
  }

  /**
   * Handle list tools request
   */
  public listTools(): Tool[] {
    return Array.from(this.tools.values());
  }

  /**
   * Handle list resources request
   */
  public listResources(): Resource[] {
    return Array.from(this.resources.values());
  }

  /**
   * Handle list resource templates request
   */
  public listResourceTemplates(): ResourceTemplate[] {
    return Array.from(this.resourceTemplates.values());
  }

  /**
   * Get server capabilities
   */
  public getCapabilities(): MCPServerConfig['capabilities'] {
    return this.config.capabilities;
  }

  /**
   * Handle ping request
   */
  public ping(): { pong: boolean; server: string; version: string } {
    return {
      pong: true,
      server: this.config.name,
      version: this.config.version,
    };
  }
}

/**
 * Create a tool definition helper
 */
export function createTool(
  name: string,
  description: string,
  inputSchema: Record<string, unknown>,
  handler: (input: Record<string, unknown>) => Promise<unknown>
): Tool {
  return {
    name,
    description,
    inputSchema,
    handler,
  };
}

/**
 * Pre-built tools for AG-Claw
 */
export const builtInTools = {
  // File operations
  Read: createTool(
    'Read',
    'Read the complete contents of a file',
    {
      type: 'object',
      properties: {
        file_path: { type: 'string', description: 'Path to the file to read' },
      },
      required: ['file_path'],
    },
    async ({ file_path }) => {
      const fs = await import('fs/promises');
      const content = await fs.readFile(file_path as string, 'utf-8');
      return { content, size: content.length };
    }
  ),

  Write: createTool(
    'Write',
    'Create a new file or overwrite an existing one',
    {
      type: 'object',
      properties: {
        file_path: { type: 'string', description: 'Path to the file to write' },
        content: { type: 'string', description: 'Content to write' },
      },
      required: ['file_path', 'content'],
    },
    async ({ file_path, content }) => {
      const fs = await import('fs/promises');
      await fs.writeFile(file_path as string, content as string, 'utf-8');
      return { success: true, path: file_path, size: (content as string).length };
    }
  ),

  Edit: createTool(
    'Edit',
    'Make specific edits to a file',
    {
      type: 'object',
      properties: {
        file_path: { type: 'string', description: 'Path to the file to edit' },
        old_string: { type: 'string', description: 'The exact text to replace' },
        new_string: { type: 'string', description: 'The replacement text' },
      },
      required: ['file_path', 'old_string', 'new_string'],
    },
    async ({ file_path, old_string, new_string }) => {
      const fs = await import('fs/promises');
      let content = await fs.readFile(file_path as string, 'utf-8');
      if (!content.includes(old_string as string)) {
        throw new Error(`old_string not found in file`);
      }
      content = content.replace(old_string as string, new_string as string);
      await fs.writeFile(file_path as string, content, 'utf-8');
      return { success: true, replacements: 1 };
    }
  ),

  Bash: createTool(
    'Bash',
    'Execute a bash command',
    {
      type: 'object',
      properties: {
        command: { type: 'string', description: 'The bash command to execute' },
        timeout: { type: 'number', description: 'Timeout in milliseconds', default: 60000 },
      },
      required: ['command'],
    },
    async ({ command, timeout = 60000 }) => {
      const { exec } = await import('child_process');
      const { promisify } = await import('util');
      const execAsync = promisify(exec);
      
      try {
        const { stdout, stderr } = await execAsync(command as string, { timeout });
        return { stdout, stderr, exitCode: 0 };
      } catch (error: any) {
        return {
          stdout: error.stdout || '',
          stderr: error.stderr || error.message,
          exitCode: error.code || 1,
        };
      }
    }
  ),

  Grep: createTool(
    'Grep',
    'Search for patterns in files',
    {
      type: 'object',
      properties: {
        pattern: { type: 'string', description: 'Pattern to search for' },
        path: { type: 'string', description: 'Directory or file to search in' },
        recursive: { type: 'boolean', default: true },
      },
      required: ['pattern', 'path'],
    },
    async ({ pattern, path, recursive = true }) => {
      const { exec } = await import('child_process');
      const { promisify } = await import('util');
      const execAsync = promisify(exec);
      
      const flag = recursive ? '-r' : '';
      const { stdout } = await execAsync(`grep ${flag} "${pattern}" ${path} 2>/dev/null || true`);
      const matches = stdout.trim().split('\n').filter(Boolean);
      return { matches, count: matches.length };
    }
  ),

  Glob: createTool(
    'Glob',
    'List files matching a glob pattern',
    {
      type: 'object',
      properties: {
        pattern: { type: 'string', description: 'Glob pattern (e.g., **/*.ts)' },
        cwd: { type: 'string', description: 'Working directory' },
      },
      required: ['pattern'],
    },
    async ({ pattern, cwd = '.' }) => {
      const { glob } = await import('glob');
      const files = await glob(pattern as string, { cwd: cwd as string });
      return { files, count: files.length };
    }
  ),

  WebFetch: createTool(
    'WebFetch',
    'Fetch content from a URL',
    {
      type: 'object',
      properties: {
        url: { type: 'string', description: 'URL to fetch' },
        prompt: { type: 'string', description: 'What to extract from the page' },
      },
      required: ['url'],
    },
    async ({ url, prompt }) => {
      const response = await fetch(url as string);
      const text = await response.text();
      return { content: text, status: response.status, url };
    }
  ),
};

export default MCPServer;
