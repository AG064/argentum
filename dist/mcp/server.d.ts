/**
 * Argentum MCP Server
 *
 * Model Context Protocol (MCP) server implementation for Argentum.
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
type ToolInput = Record<string, unknown>;
type ToolHandler = (input: ToolInput) => Promise<unknown>;
type ExecutableMCPTool = MCPTool & {
    handler: ToolHandler;
};
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
export declare function createTool(name: string, description: string, inputSchema: MCPTool['inputSchema'], handler: ToolHandler): ExecutableMCPTool;
/**
 * Pre-built tools for Argentum
 */
export declare const builtInTools: {
    Read: ExecutableMCPTool;
    Write: ExecutableMCPTool;
    Edit: ExecutableMCPTool;
};
export default MCPServer;
//# sourceMappingURL=server.d.ts.map