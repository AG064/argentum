"use strict";
/**
 * AG-Claw MCP Server
 *
 * Model Context Protocol (MCP) server implementation for AG-Claw.
 * Provides tools and resources compatible with Claude Code.
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
exports.builtInTools = exports.MCPServer = void 0;
exports.createTool = createTool;
const events_1 = require("events");
const logger_1 = require("../core/logger");
/**
 * MCP Server implementation
 */
class MCPServer extends events_1.EventEmitter {
    config;
    tools = new Map();
    resources = new Map();
    resourceTemplates = new Map();
    logger;
    constructor(config, options = {}, logger) {
        super();
        this.config = config;
        this.logger = logger || new logger_1.Logger({ level: 'info', format: 'pretty' });
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
    registerTool(tool) {
        if (this.tools.has(tool.name)) {
            this.logger.warn(`Tool ${tool.name} already registered, overwriting`);
        }
        this.tools.set(tool.name, tool);
    }
    registerResource(resource) {
        this.resources.set(resource.uri, resource);
    }
    registerResourceTemplate(template) {
        this.resourceTemplates.set(template.uriTemplate, template);
    }
    async handleToolCall(call) {
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
            const result = await tool.handler(input);
            this.emit('tool-call', { tool: call.name, input, result });
            return {
                content: [
                    {
                        type: 'text',
                        data: typeof result === 'string' ? result : JSON.stringify(result, null, 2),
                    },
                ],
            };
        }
        catch (error) {
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
    listTools() {
        return Array.from(this.tools.values());
    }
    listResources() {
        return Array.from(this.resources.values());
    }
    listResourceTemplates() {
        return Array.from(this.resourceTemplates.values());
    }
    getCapabilities() {
        return this.config.capabilities;
    }
    ping() {
        return {
            pong: true,
            server: this.config.name,
            version: this.config.version,
        };
    }
}
exports.MCPServer = MCPServer;
/**
 * Create a tool definition helper
 */
function createTool(name, description, 
// eslint-disable-next-line @typescript-eslint/no-explicit-any
inputSchema, 
// eslint-disable-next-line @typescript-eslint/no-explicit-any
handler) {
    return {
        name,
        description,
        inputSchema,
        // eslint-disable-next-line @typescript-eslint/no-explicit-any
        handler: handler,
    };
}
/**
 * Pre-built tools for AG-Claw
 */
exports.builtInTools = {
    Read: createTool('Read', 'Read file contents', { type: 'object', properties: { file_path: { type: 'string' } }, required: ['file_path'] }, async ({ file_path }) => {
        const fs = await Promise.resolve().then(() => __importStar(require('fs/promises')));
        const content = await fs.readFile(file_path, 'utf-8');
        return { content, size: content.length };
    }),
    Write: createTool('Write', 'Write file contents', {
        type: 'object',
        properties: { file_path: { type: 'string' }, content: { type: 'string' } },
        required: ['file_path', 'content'],
    }, async ({ file_path, content }) => {
        const fs = await Promise.resolve().then(() => __importStar(require('fs/promises')));
        await fs.writeFile(file_path, content, 'utf-8');
        return { success: true, path: file_path };
    }),
    Edit: createTool('Edit', 'Edit file contents', {
        type: 'object',
        properties: {
            file_path: { type: 'string' },
            old_string: { type: 'string' },
            new_string: { type: 'string' },
        },
        required: ['file_path', 'old_string', 'new_string'],
    }, async ({ file_path, old_string, new_string }) => {
        const fs = await Promise.resolve().then(() => __importStar(require('fs/promises')));
        let content = await fs.readFile(file_path, 'utf-8');
        if (!content.includes(old_string)) {
            throw new Error('old_string not found in file');
        }
        content = content.replace(old_string, new_string);
        await fs.writeFile(file_path, content, 'utf-8');
        return { success: true, replacements: 1 };
    }),
    Bash: createTool('Bash', 'Execute bash command', {
        type: 'object',
        properties: { command: { type: 'string' }, timeout: { type: 'number' } },
        required: ['command'],
    }, async ({ command, timeout = 60000 }) => {
        const { exec } = await Promise.resolve().then(() => __importStar(require('child_process')));
        const { promisify } = await Promise.resolve().then(() => __importStar(require('util')));
        const execAsync = promisify(exec);
        try {
            const { stdout, stderr } = await execAsync(command, {
                timeout: timeout,
            });
            return { stdout, stderr, exitCode: 0 };
        }
        catch (error) {
            return {
                stdout: error.stdout || '',
                stderr: error.stderr || error.message,
                exitCode: error.code || 1,
            };
        }
    }),
    Grep: createTool('Grep', 'Search pattern in files', {
        type: 'object',
        properties: { pattern: { type: 'string' }, path: { type: 'string' } },
        required: ['pattern', 'path'],
    }, async ({ pattern, path }) => {
        const { exec } = await Promise.resolve().then(() => __importStar(require('child_process')));
        const { promisify } = await Promise.resolve().then(() => __importStar(require('util')));
        const execAsync = promisify(exec);
        const { stdout } = await execAsync(`grep -r "${pattern}" ${path} 2>/dev/null || true`);
        const matches = stdout.trim().split('\n').filter(Boolean);
        return { matches, count: matches.length };
    }),
};
exports.default = MCPServer;
//# sourceMappingURL=server.js.map