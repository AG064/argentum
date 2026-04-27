/**
 * Argentum MCP Server
 *
 * Model Context Protocol (MCP) server implementation for Argentum.
 * Provides tools and resources compatible with Claude Code.
 */

import { EventEmitter } from 'events';

import type {
  MCPTool,
  ToolsCallRequest,
  Resource,
  ResourceTemplate,
  ToolResultContent,
} from './types';

import { Logger } from '../core/logger';

// Re-export for backwards compatibility
export type Tool = MCPTool;
export type ToolCall = ToolsCallRequest;
export type { Resource, ResourceTemplate };

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
  tools?: MCPTool[];
  resources?: Resource[];
  resourceTemplates?: ResourceTemplate[];
}

/**
 * MCP Server implementation
 */
export class MCPServer extends EventEmitter {
  public readonly config: MCPServerConfig;
  private tools: Map<string, MCPTool> = new Map();
  private resources: Map<string, Resource> = new Map();
  private resourceTemplates: Map<string, ResourceTemplate> = new Map();
  private logger: Logger;

  constructor(config: MCPServerConfig, options: MCPServerOptions = {}, logger?: Logger) {
    super();
    this.config = config;
    this.logger = logger || new Logger({ level: 'info', format: 'pretty' });

    if (options.tools) {
      for (const tool of options.tools) {
        this.registerTool(tool);
      }
    }

    if (options.resources) {
      for (const resource of options.resources) {
        this.registerResource(resource);
      }
    }

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

  public registerTool(tool: MCPTool): void {
    if (this.tools.has(tool.name)) {
      this.logger.warn(`Tool ${tool.name} already registered, overwriting`);
    }
    this.tools.set(tool.name, tool);
  }

  public registerResource(resource: Resource): void {
    this.resources.set(resource.uri, resource);
  }

  public registerResourceTemplate(template: ResourceTemplate): void {
    this.resourceTemplates.set(template.uriTemplate, template);
  }

  public async handleToolCall(call: ToolsCallRequest): Promise<{ content: ToolResultContent[] }> {
    const tool = this.tools.get(call.name);

    if (!tool) {
      return {
        content: [
          {
            type: 'text',
            data: JSON.stringify({ error: `Tool not found: ${call.name}` }),
          },
        ],
      };
    }

    try {
      const input = call.arguments || {};
      this.logger.debug(`Executing tool: ${call.name}`, { input });
      // eslint-disable-next-line @typescript-eslint/no-explicit-any
      const result = await (tool as any).handler(input);

      this.emit('tool-call', { tool: call.name, input, result });

      return {
        content: [
          {
            type: 'text',
            data: typeof result === 'string' ? result : JSON.stringify(result, null, 2),
          },
        ],
      };
    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : String(error);
      this.logger.error(`Tool ${call.name} failed`, { error: errorMessage });

      return {
        content: [
          {
            type: 'text',
            data: JSON.stringify({ error: errorMessage }),
          },
        ],
      };
    }
  }

  public listTools(): MCPTool[] {
    return Array.from(this.tools.values());
  }

  public listResources(): Resource[] {
    return Array.from(this.resources.values());
  }

  public listResourceTemplates(): ResourceTemplate[] {
    return Array.from(this.resourceTemplates.values());
  }

  public getCapabilities(): MCPServerConfig['capabilities'] {
    return this.config.capabilities;
  }

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
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  inputSchema: Record<string, any>,
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  handler: (input: Record<string, any>) => Promise<any>,
): MCPTool {
  return {
    name,
    description,
    inputSchema,
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    handler: handler as any,
  } as unknown as MCPTool;
}

/**
 * Pre-built tools for Argentum
 */
export const builtInTools = {
  Read: createTool(
    'Read',
    'Read file contents',
    { type: 'object', properties: { file_path: { type: 'string' } }, required: ['file_path'] },
    async ({ file_path }: any) => {
      const fs = await import('fs/promises');
      const content = await fs.readFile(file_path as string, 'utf-8');
      return { content, size: content.length };
    },
  ),

  Write: createTool(
    'Write',
    'Write file contents',
    {
      type: 'object',
      properties: { file_path: { type: 'string' }, content: { type: 'string' } },
      required: ['file_path', 'content'],
    },
    async ({ file_path, content }: any) => {
      const fs = await import('fs/promises');
      await fs.writeFile(file_path as string, content as string, 'utf-8');
      return { success: true, path: file_path };
    },
  ),

  Edit: createTool(
    'Edit',
    'Edit file contents',
    {
      type: 'object',
      properties: {
        file_path: { type: 'string' },
        old_string: { type: 'string' },
        new_string: { type: 'string' },
      },
      required: ['file_path', 'old_string', 'new_string'],
    },
    async ({ file_path, old_string, new_string }: any) => {
      const fs = await import('fs/promises');
      let content = await fs.readFile(file_path as string, 'utf-8');
      if (!content.includes(old_string as string)) {
        throw new Error('old_string not found in file');
      }
      content = content.replace(old_string as string, new_string as string);
      await fs.writeFile(file_path as string, content, 'utf-8');
      return { success: true, replacements: 1 };
    },
  ),

  Bash: createTool(
    'Bash',
    'Execute bash command',
    {
      type: 'object',
      properties: { command: { type: 'string' }, timeout: { type: 'number' } },
      required: ['command'],
    },
    async ({ command, timeout = 60000 }: any) => {
      const { exec } = await import('child_process');
      const { promisify } = await import('util');
      const execAsync = promisify(exec);

      try {
        const { stdout, stderr } = await execAsync(command as string, {
          timeout: timeout as number,
        });
        return { stdout, stderr, exitCode: 0 };
      } catch (error: any) {
        return {
          stdout: error.stdout || '',
          stderr: error.stderr || error.message,
          exitCode: error.code || 1,
        };
      }
    },
  ),

  Grep: createTool(
    'Grep',
    'Search pattern in files',
    {
      type: 'object',
      properties: { pattern: { type: 'string' }, path: { type: 'string' } },
      required: ['pattern', 'path'],
    },
    async ({ pattern, path }: any) => {
      const { exec } = await import('child_process');
      const { promisify } = await import('util');
      const execAsync = promisify(exec);

      const { stdout } = await execAsync(
        `grep -r "${pattern as string}" ${path as string} 2>/dev/null || true`,
      );
      const matches = stdout.trim().split('\n').filter(Boolean);
      return { matches, count: matches.length };
    },
  ),
};

export default MCPServer;
