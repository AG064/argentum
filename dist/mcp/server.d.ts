/**
 * AG-Claw MCP Server
 *
 * Model Context Protocol (MCP) server implementation for AG-Claw.
 * Provides tools and resources compatible with Claude Code.
 */
import { EventEmitter } from 'events';
import type { MCPTool, ToolsCallRequest, Resource, ResourceTemplate, ToolResultContent } from './types';
import { Logger } from '../core/logger';
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
export declare class MCPServer extends EventEmitter {
    readonly config: MCPServerConfig;
    private tools;
    private resources;
    private resourceTemplates;
    private logger;
    constructor(config: MCPServerConfig, options?: MCPServerOptions, logger?: Logger);
    registerTool(tool: MCPTool): void;
    registerResource(resource: Resource): void;
    registerResourceTemplate(template: ResourceTemplate): void;
    handleToolCall(call: ToolsCallRequest): Promise<{
        content: ToolResultContent[];
    }>;
    listTools(): MCPTool[];
    listResources(): Resource[];
    listResourceTemplates(): ResourceTemplate[];
    getCapabilities(): MCPServerConfig['capabilities'];
    ping(): {
        pong: boolean;
        server: string;
        version: string;
    };
}
/**
 * Create a tool definition helper
 */
export declare function createTool(name: string, description: string, inputSchema: Record<string, any>, handler: (input: Record<string, any>) => Promise<any>): MCPTool;
/**
 * Pre-built tools for AG-Claw
 */
export declare const builtInTools: {
    Read: {
        name: string;
        inputSchema: {
            type: "object";
            properties: Record<string, unknown>;
            required?: string[] | undefined;
        };
        description?: string | undefined;
        title?: string | undefined;
    };
    Write: {
        name: string;
        inputSchema: {
            type: "object";
            properties: Record<string, unknown>;
            required?: string[] | undefined;
        };
        description?: string | undefined;
        title?: string | undefined;
    };
    Edit: {
        name: string;
        inputSchema: {
            type: "object";
            properties: Record<string, unknown>;
            required?: string[] | undefined;
        };
        description?: string | undefined;
        title?: string | undefined;
    };
    Bash: {
        name: string;
        inputSchema: {
            type: "object";
            properties: Record<string, unknown>;
            required?: string[] | undefined;
        };
        description?: string | undefined;
        title?: string | undefined;
    };
    Grep: {
        name: string;
        inputSchema: {
            type: "object";
            properties: Record<string, unknown>;
            required?: string[] | undefined;
        };
        description?: string | undefined;
        title?: string | undefined;
    };
};
export default MCPServer;
//# sourceMappingURL=server.d.ts.map