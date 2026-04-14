"use strict";
/**
 * MCP (Model Context Protocol) Types and Interfaces
 * Based on MCP Specification 2025-06-18
 */
Object.defineProperty(exports, "__esModule", { value: true });
exports.RequestMethods = exports.NotificationMethods = exports.ErrorCodes = exports.PromptsGetResponseSchema = exports.PromptMessageSchema = exports.PromptsGetRequestSchema = exports.PromptsListResponseSchema = exports.PromptsListRequestSchema = exports.PromptSchema = exports.ResourcesReadResponseSchema = exports.ResourceContentSchema = exports.ResourcesReadRequestSchema = exports.ResourcesListResponseSchema = exports.ResourcesListRequestSchema = exports.ResourceTemplateSchema = exports.ResourceSchema = exports.ToolsCallResponseSchema = exports.ToolResultContentSchema = exports.ToolsCallRequestSchema = exports.ToolsListResponseSchema = exports.ToolsListRequestSchema = exports.MCPToolSchema = exports.InitializeResponseSchema = exports.InitializeRequestSchema = exports.MCP_PROTOCOL_VERSION = void 0;
const zod_1 = require("zod");
// ─── Protocol Version ───────────────────────────────────────────────────────────
exports.MCP_PROTOCOL_VERSION = '2025-06-18';
// ─── Initialize Request/Response ───────────────────────────────────────────────
exports.InitializeRequestSchema = zod_1.z.object({
    protocolVersion: zod_1.z.string().optional(),
    capabilities: zod_1.z
        .object({
        tools: zod_1.z
            .object({
            listChanged: zod_1.z.boolean().optional(),
        })
            .optional(),
        resources: zod_1.z
            .object({
            subscribe: zod_1.z.boolean().optional(),
            listChanged: zod_1.z.boolean().optional(),
        })
            .optional(),
        prompts: zod_1.z
            .object({
            listChanged: zod_1.z.boolean().optional(),
        })
            .optional(),
        elicitation: zod_1.z.object({}).optional(),
        sampling: zod_1.z.object({}).optional(),
        logging: zod_1.z.object({}).optional(),
    })
        .optional(),
    clientInfo: zod_1.z
        .object({
        name: zod_1.z.string(),
        version: zod_1.z.string(),
    })
        .optional(),
});
exports.InitializeResponseSchema = zod_1.z.object({
    protocolVersion: zod_1.z.string(),
    capabilities: zod_1.z.object({
        tools: zod_1.z
            .object({
            listChanged: zod_1.z.boolean().optional(),
        })
            .optional(),
        resources: zod_1.z
            .object({
            subscribe: zod_1.z.boolean().optional(),
            listChanged: zod_1.z.boolean().optional(),
        })
            .optional(),
        prompts: zod_1.z
            .object({
            listChanged: zod_1.z.boolean().optional(),
        })
            .optional(),
        sampling: zod_1.z.object({}).optional(),
        logging: zod_1.z.object({}).optional(),
    }),
    serverInfo: zod_1.z.object({
        name: zod_1.z.string(),
        version: zod_1.z.string(),
    }),
});
// ─── Tools ───────────────────────────────────────────────────────────────────────
exports.MCPToolSchema = zod_1.z.object({
    name: zod_1.z.string(),
    description: zod_1.z.string().optional(),
    title: zod_1.z.string().optional(),
    inputSchema: zod_1.z.object({
        type: zod_1.z.literal('object'),
        properties: zod_1.z.record(zod_1.z.string(), zod_1.z.unknown()),
        required: zod_1.z.array(zod_1.z.string()).optional(),
    }),
});
exports.ToolsListRequestSchema = zod_1.z.object({
// Empty params
});
exports.ToolsListResponseSchema = zod_1.z.object({
    tools: zod_1.z.array(exports.MCPToolSchema),
});
exports.ToolsCallRequestSchema = zod_1.z.object({
    name: zod_1.z.string(),
    arguments: zod_1.z.record(zod_1.z.string(), zod_1.z.unknown()).optional(),
});
exports.ToolResultContentSchema = zod_1.z.object({
    type: zod_1.z.enum(['text', 'image', 'resource']),
    text: zod_1.z.string().optional(),
    data: zod_1.z.string().optional(),
    mimeType: zod_1.z.string().optional(),
    uri: zod_1.z.string().optional(),
});
exports.ToolsCallResponseSchema = zod_1.z.object({
    content: zod_1.z.array(exports.ToolResultContentSchema),
});
// ─── Resources ─────────────────────────────────────────────────────────────────
exports.ResourceSchema = zod_1.z.object({
    uri: zod_1.z.string(),
    name: zod_1.z.string().optional(),
    description: zod_1.z.string().optional(),
    mimeType: zod_1.z.string().optional(),
});
exports.ResourceTemplateSchema = zod_1.z.object({
    uriTemplate: zod_1.z.string(),
    name: zod_1.z.string().optional(),
    description: zod_1.z.string().optional(),
    mimeType: zod_1.z.string().optional(),
});
exports.ResourcesListRequestSchema = zod_1.z.object({
// Empty params
});
exports.ResourcesListResponseSchema = zod_1.z.object({
    resources: zod_1.z.array(exports.ResourceSchema),
    resourceTemplates: zod_1.z.array(exports.ResourceTemplateSchema).optional(),
});
exports.ResourcesReadRequestSchema = zod_1.z.object({
    uri: zod_1.z.string(),
});
exports.ResourceContentSchema = zod_1.z.object({
    uri: zod_1.z.string(),
    mimeType: zod_1.z.string().optional(),
    text: zod_1.z.string().optional(),
    blob: zod_1.z.string().optional(), // base64 encoded
});
exports.ResourcesReadResponseSchema = zod_1.z.object({
    contents: zod_1.z.array(exports.ResourceContentSchema),
});
// ─── Prompts ───────────────────────────────────────────────────────────────────
exports.PromptSchema = zod_1.z.object({
    name: zod_1.z.string(),
    description: zod_1.z.string().optional(),
    arguments: zod_1.z
        .array(zod_1.z.object({
        name: zod_1.z.string(),
        description: zod_1.z.string().optional(),
        required: zod_1.z.boolean().optional(),
    }))
        .optional(),
});
exports.PromptsListRequestSchema = zod_1.z.object({
// Empty params
});
exports.PromptsListResponseSchema = zod_1.z.object({
    prompts: zod_1.z.array(exports.PromptSchema),
});
exports.PromptsGetRequestSchema = zod_1.z.object({
    name: zod_1.z.string(),
    arguments: zod_1.z.record(zod_1.z.string(), zod_1.z.string()).optional(),
});
exports.PromptMessageSchema = zod_1.z.object({
    role: zod_1.z.enum(['user', 'assistant']),
    content: zod_1.z.object({
        type: zod_1.z.enum(['text', 'image', 'resource']),
        text: zod_1.z.string().optional(),
        data: zod_1.z.string().optional(),
        mimeType: zod_1.z.string().optional(),
        uri: zod_1.z.string().optional(),
    }),
});
exports.PromptsGetResponseSchema = zod_1.z.object({
    messages: zod_1.z.array(exports.PromptMessageSchema),
});
// ─── Error Codes ───────────────────────────────────────────────────────────────
exports.ErrorCodes = {
    ParseError: -32700,
    InvalidRequest: -32600,
    MethodNotFound: -32601,
    InvalidParams: -32602,
    InternalError: -32603,
    ServerError: -32000,
};
// ─── Notification Methods ───────────────────────────────────────────────────────
exports.NotificationMethods = {
    Initialized: 'notifications/initialized',
    ToolsListChanged: 'notifications/tools/list_changed',
    ResourcesListChanged: 'notifications/resources/list_changed',
    ResourcesUpdated: 'notifications/resources/updated',
    PromptsListChanged: 'notifications/prompts/list_changed',
};
// ─── Request Methods ────────────────────────────────────────────────────────────
exports.RequestMethods = {
    Initialize: 'initialize',
    ToolsList: 'tools/list',
    ToolsCall: 'tools/call',
    ResourcesList: 'resources/list',
    ResourcesRead: 'resources/read',
    ResourcesSubscribe: 'resources/subscribe',
    ResourcesUnsubscribe: 'resources/unsubscribe',
    PromptsList: 'prompts/list',
    PromptsGet: 'prompts/get',
};
//# sourceMappingURL=types.js.map