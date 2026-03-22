/**
 * MCP (Model Context Protocol) Types and Interfaces
 * Based on MCP Specification 2025-06-18
 */

import { z } from 'zod';

// ─── JSON-RPC 2.0 Base Types ───────────────────────────────────────────────────

export interface JSONRPCRequest {
  jsonrpc: '2.0';
  id: string | number | null;
  method: string;
  params?: Record<string, unknown>;
}

export interface JSONRPCResponse {
  jsonrpc: '2.0';
  id: string | number | null;
  result?: unknown;
  error?: JSONRPCError;
}

export interface JSONRPCError {
  code: number;
  message: string;
  data?: unknown;
}

export interface JSONRPCNotification {
  jsonrpc: '2.0';
  method: string;
  params?: Record<string, unknown>;
}

// ─── Protocol Version ───────────────────────────────────────────────────────────

export const MCP_PROTOCOL_VERSION = '2025-06-18';

// ─── Initialize Request/Response ───────────────────────────────────────────────

export const InitializeRequestSchema = z.object({
  protocolVersion: z.string().optional(),
  capabilities: z.object({
    tools: z.object({
      listChanged: z.boolean().optional(),
    }).optional(),
    resources: z.object({
      subscribe: z.boolean().optional(),
      listChanged: z.boolean().optional(),
    }).optional(),
    prompts: z.object({
      listChanged: z.boolean().optional(),
    }).optional(),
    elicitation: z.object({}).optional(),
    sampling: z.object({}).optional(),
    logging: z.object({}).optional(),
  }).optional(),
  clientInfo: z.object({
    name: z.string(),
    version: z.string(),
  }).optional(),
});

export type InitializeRequest = z.infer<typeof InitializeRequestSchema>;

export const InitializeResponseSchema = z.object({
  protocolVersion: z.string(),
  capabilities: z.object({
    tools: z.object({
      listChanged: z.boolean().optional(),
    }).optional(),
    resources: z.object({
      subscribe: z.boolean().optional(),
      listChanged: z.boolean().optional(),
    }).optional(),
    prompts: z.object({
      listChanged: z.boolean().optional(),
    }).optional(),
    sampling: z.object({}).optional(),
    logging: z.object({}).optional(),
  }),
  serverInfo: z.object({
    name: z.string(),
    version: z.string(),
  }),
});

export type InitializeResponse = z.infer<typeof InitializeResponseSchema>;

// ─── Tools ───────────────────────────────────────────────────────────────────────

export const MCPToolSchema = z.object({
  name: z.string(),
  description: z.string().optional(),
  title: z.string().optional(),
  inputSchema: z.object({
    type: z.literal('object'),
    properties: z.record(z.unknown()),
    required: z.array(z.string()).optional(),
  }),
});

export type MCPTool = z.infer<typeof MCPToolSchema>;

export const ToolsListRequestSchema = z.object({
  // Empty params
});

export const ToolsListResponseSchema = z.object({
  tools: z.array(MCPToolSchema),
});

export type ToolsListResponse = z.infer<typeof ToolsListResponseSchema>;

export const ToolsCallRequestSchema = z.object({
  name: z.string(),
  arguments: z.record(z.unknown()).optional(),
});

export type ToolsCallRequest = z.infer<typeof ToolsCallRequestSchema>;

export const ToolResultContentSchema = z.object({
  type: z.enum(['text', 'image', 'resource']),
  text: z.string().optional(),
  data: z.string().optional(),
  mimeType: z.string().optional(),
  uri: z.string().optional(),
});

export type ToolResultContent = z.infer<typeof ToolResultContentSchema>;

export const ToolsCallResponseSchema = z.object({
  content: z.array(ToolResultContentSchema),
});

export type ToolsCallResponse = z.infer<typeof ToolsCallResponseSchema>;

// ─── Resources ─────────────────────────────────────────────────────────────────

export const ResourceSchema = z.object({
  uri: z.string(),
  name: z.string().optional(),
  description: z.string().optional(),
  mimeType: z.string().optional(),
});

export type Resource = z.infer<typeof ResourceSchema>;

export const ResourceTemplateSchema = z.object({
  uriTemplate: z.string(),
  name: z.string().optional(),
  description: z.string().optional(),
  mimeType: z.string().optional(),
});

export type ResourceTemplate = z.infer<typeof ResourceTemplateSchema>;

export const ResourcesListRequestSchema = z.object({
  // Empty params
});

export const ResourcesListResponseSchema = z.object({
  resources: z.array(ResourceSchema),
  resourceTemplates: z.array(ResourceTemplateSchema).optional(),
});

export type ResourcesListResponse = z.infer<typeof ResourcesListResponseSchema>;

export const ResourcesReadRequestSchema = z.object({
  uri: z.string(),
});

export const ResourceContentSchema = z.object({
  uri: z.string(),
  mimeType: z.string().optional(),
  text: z.string().optional(),
  blob: z.string().optional(), // base64 encoded
});

export const ResourcesReadResponseSchema = z.object({
  contents: z.array(ResourceContentSchema),
});

export type ResourcesReadResponse = z.infer<typeof ResourcesReadResponseSchema>;

// ─── Prompts ───────────────────────────────────────────────────────────────────

export const PromptSchema = z.object({
  name: z.string(),
  description: z.string().optional(),
  arguments: z.array(z.object({
    name: z.string(),
    description: z.string().optional(),
    required: z.boolean().optional(),
  })).optional(),
});

export type Prompt = z.infer<typeof PromptSchema>;

export const PromptsListRequestSchema = z.object({
  // Empty params
});

export const PromptsListResponseSchema = z.object({
  prompts: z.array(PromptSchema),
});

export type PromptsListResponse = z.infer<typeof PromptsListResponseSchema>;

export const PromptsGetRequestSchema = z.object({
  name: z.string(),
  arguments: z.record(z.string()).optional(),
});

export const PromptMessageSchema = z.object({
  role: z.enum(['user', 'assistant']),
  content: z.object({
    type: z.enum(['text', 'image', 'resource']),
    text: z.string().optional(),
    data: z.string().optional(),
    mimeType: z.string().optional(),
    uri: z.string().optional(),
  }),
});

export const PromptsGetResponseSchema = z.object({
  messages: z.array(PromptMessageSchema),
});

export type PromptsGetResponse = z.infer<typeof PromptsGetResponseSchema>;

// ─── Server Capabilities ───────────────────────────────────────────────────────

export interface MCPServerCapabilities {
  tools?: {
    listChanged?: boolean;
  };
  resources?: {
    subscribe?: boolean;
    listChanged?: boolean;
  };
  prompts?: {
    listChanged?: boolean;
  };
  sampling?: Record<string, never>;
  logging?: Record<string, never>;
}

// ─── Error Codes ───────────────────────────────────────────────────────────────

export const ErrorCodes = {
  ParseError: -32700,
  InvalidRequest: -32600,
  MethodNotFound: -32601,
  InvalidParams: -32602,
  InternalError: -32603,
  ServerError: -32000,
} as const;

// ─── Notification Methods ───────────────────────────────────────────────────────

export const NotificationMethods = {
  Initialized: 'notifications/initialized',
  ToolsListChanged: 'notifications/tools/list_changed',
  ResourcesListChanged: 'notifications/resources/list_changed',
  ResourcesUpdated: 'notifications/resources/updated',
  PromptsListChanged: 'notifications/prompts/list_changed',
} as const;

// ─── Request Methods ────────────────────────────────────────────────────────────

export const RequestMethods = {
  Initialize: 'initialize',
  ToolsList: 'tools/list',
  ToolsCall: 'tools/call',
  ResourcesList: 'resources/list',
  ResourcesRead: 'resources/read',
  ResourcesSubscribe: 'resources/subscribe',
  ResourcesUnsubscribe: 'resources/unsubscribe',
  PromptsList: 'prompts/list',
  PromptsGet: 'prompts/get',
} as const;

// ─── Legacy Type Aliases (for compatibility) ────────────────────────────────────

/** @deprecated Use MCPTool instead */
export type Tool = MCPTool;

/** @deprecated Use ToolsCallRequest instead */
export type ToolCall = ToolsCallRequest;

/** @deprecated Use ToolsCallResponse or ToolResultContent instead */
export interface ToolResult {
  tool: string;
  success: boolean;
  result?: unknown;
  error?: string;
}
