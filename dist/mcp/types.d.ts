/**
 * MCP (Model Context Protocol) Types and Interfaces
 * Based on MCP Specification 2025-06-18
 */
import { z } from 'zod';
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
export declare const MCP_PROTOCOL_VERSION = "2025-06-18";
export declare const InitializeRequestSchema: z.ZodObject<{
    protocolVersion: z.ZodOptional<z.ZodString>;
    capabilities: z.ZodOptional<z.ZodObject<{
        tools: z.ZodOptional<z.ZodObject<{
            listChanged: z.ZodOptional<z.ZodBoolean>;
        }, z.core.$strip>>;
        resources: z.ZodOptional<z.ZodObject<{
            subscribe: z.ZodOptional<z.ZodBoolean>;
            listChanged: z.ZodOptional<z.ZodBoolean>;
        }, z.core.$strip>>;
        prompts: z.ZodOptional<z.ZodObject<{
            listChanged: z.ZodOptional<z.ZodBoolean>;
        }, z.core.$strip>>;
        elicitation: z.ZodOptional<z.ZodObject<{}, z.core.$strip>>;
        sampling: z.ZodOptional<z.ZodObject<{}, z.core.$strip>>;
        logging: z.ZodOptional<z.ZodObject<{}, z.core.$strip>>;
    }, z.core.$strip>>;
    clientInfo: z.ZodOptional<z.ZodObject<{
        name: z.ZodString;
        version: z.ZodString;
    }, z.core.$strip>>;
}, z.core.$strip>;
export type InitializeRequest = z.infer<typeof InitializeRequestSchema>;
export declare const InitializeResponseSchema: z.ZodObject<{
    protocolVersion: z.ZodString;
    capabilities: z.ZodObject<{
        tools: z.ZodOptional<z.ZodObject<{
            listChanged: z.ZodOptional<z.ZodBoolean>;
        }, z.core.$strip>>;
        resources: z.ZodOptional<z.ZodObject<{
            subscribe: z.ZodOptional<z.ZodBoolean>;
            listChanged: z.ZodOptional<z.ZodBoolean>;
        }, z.core.$strip>>;
        prompts: z.ZodOptional<z.ZodObject<{
            listChanged: z.ZodOptional<z.ZodBoolean>;
        }, z.core.$strip>>;
        sampling: z.ZodOptional<z.ZodObject<{}, z.core.$strip>>;
        logging: z.ZodOptional<z.ZodObject<{}, z.core.$strip>>;
    }, z.core.$strip>;
    serverInfo: z.ZodObject<{
        name: z.ZodString;
        version: z.ZodString;
    }, z.core.$strip>;
}, z.core.$strip>;
export type InitializeResponse = z.infer<typeof InitializeResponseSchema>;
export declare const MCPToolSchema: z.ZodObject<{
    name: z.ZodString;
    description: z.ZodOptional<z.ZodString>;
    title: z.ZodOptional<z.ZodString>;
    inputSchema: z.ZodObject<{
        type: z.ZodLiteral<"object">;
        properties: z.ZodRecord<z.ZodString, z.ZodUnknown>;
        required: z.ZodOptional<z.ZodArray<z.ZodString>>;
    }, z.core.$strip>;
}, z.core.$strip>;
export type MCPTool = z.infer<typeof MCPToolSchema>;
export declare const ToolsListRequestSchema: z.ZodObject<{}, z.core.$strip>;
export declare const ToolsListResponseSchema: z.ZodObject<{
    tools: z.ZodArray<z.ZodObject<{
        name: z.ZodString;
        description: z.ZodOptional<z.ZodString>;
        title: z.ZodOptional<z.ZodString>;
        inputSchema: z.ZodObject<{
            type: z.ZodLiteral<"object">;
            properties: z.ZodRecord<z.ZodString, z.ZodUnknown>;
            required: z.ZodOptional<z.ZodArray<z.ZodString>>;
        }, z.core.$strip>;
    }, z.core.$strip>>;
}, z.core.$strip>;
export type ToolsListResponse = z.infer<typeof ToolsListResponseSchema>;
export declare const ToolsCallRequestSchema: z.ZodObject<{
    name: z.ZodString;
    arguments: z.ZodOptional<z.ZodRecord<z.ZodString, z.ZodUnknown>>;
}, z.core.$strip>;
export type ToolsCallRequest = z.infer<typeof ToolsCallRequestSchema>;
export declare const ToolResultContentSchema: z.ZodObject<{
    type: z.ZodEnum<{
        text: "text";
        resource: "resource";
        image: "image";
    }>;
    text: z.ZodOptional<z.ZodString>;
    data: z.ZodOptional<z.ZodString>;
    mimeType: z.ZodOptional<z.ZodString>;
    uri: z.ZodOptional<z.ZodString>;
}, z.core.$strip>;
export type ToolResultContent = z.infer<typeof ToolResultContentSchema>;
export declare const ToolsCallResponseSchema: z.ZodObject<{
    content: z.ZodArray<z.ZodObject<{
        type: z.ZodEnum<{
            text: "text";
            resource: "resource";
            image: "image";
        }>;
        text: z.ZodOptional<z.ZodString>;
        data: z.ZodOptional<z.ZodString>;
        mimeType: z.ZodOptional<z.ZodString>;
        uri: z.ZodOptional<z.ZodString>;
    }, z.core.$strip>>;
}, z.core.$strip>;
export type ToolsCallResponse = z.infer<typeof ToolsCallResponseSchema>;
export declare const ResourceSchema: z.ZodObject<{
    uri: z.ZodString;
    name: z.ZodOptional<z.ZodString>;
    description: z.ZodOptional<z.ZodString>;
    mimeType: z.ZodOptional<z.ZodString>;
}, z.core.$strip>;
export type Resource = z.infer<typeof ResourceSchema>;
export declare const ResourceTemplateSchema: z.ZodObject<{
    uriTemplate: z.ZodString;
    name: z.ZodOptional<z.ZodString>;
    description: z.ZodOptional<z.ZodString>;
    mimeType: z.ZodOptional<z.ZodString>;
}, z.core.$strip>;
export type ResourceTemplate = z.infer<typeof ResourceTemplateSchema>;
export declare const ResourcesListRequestSchema: z.ZodObject<{}, z.core.$strip>;
export declare const ResourcesListResponseSchema: z.ZodObject<{
    resources: z.ZodArray<z.ZodObject<{
        uri: z.ZodString;
        name: z.ZodOptional<z.ZodString>;
        description: z.ZodOptional<z.ZodString>;
        mimeType: z.ZodOptional<z.ZodString>;
    }, z.core.$strip>>;
    resourceTemplates: z.ZodOptional<z.ZodArray<z.ZodObject<{
        uriTemplate: z.ZodString;
        name: z.ZodOptional<z.ZodString>;
        description: z.ZodOptional<z.ZodString>;
        mimeType: z.ZodOptional<z.ZodString>;
    }, z.core.$strip>>>;
}, z.core.$strip>;
export type ResourcesListResponse = z.infer<typeof ResourcesListResponseSchema>;
export declare const ResourcesReadRequestSchema: z.ZodObject<{
    uri: z.ZodString;
}, z.core.$strip>;
export declare const ResourceContentSchema: z.ZodObject<{
    uri: z.ZodString;
    mimeType: z.ZodOptional<z.ZodString>;
    text: z.ZodOptional<z.ZodString>;
    blob: z.ZodOptional<z.ZodString>;
}, z.core.$strip>;
export declare const ResourcesReadResponseSchema: z.ZodObject<{
    contents: z.ZodArray<z.ZodObject<{
        uri: z.ZodString;
        mimeType: z.ZodOptional<z.ZodString>;
        text: z.ZodOptional<z.ZodString>;
        blob: z.ZodOptional<z.ZodString>;
    }, z.core.$strip>>;
}, z.core.$strip>;
export type ResourcesReadResponse = z.infer<typeof ResourcesReadResponseSchema>;
export declare const PromptSchema: z.ZodObject<{
    name: z.ZodString;
    description: z.ZodOptional<z.ZodString>;
    arguments: z.ZodOptional<z.ZodArray<z.ZodObject<{
        name: z.ZodString;
        description: z.ZodOptional<z.ZodString>;
        required: z.ZodOptional<z.ZodBoolean>;
    }, z.core.$strip>>>;
}, z.core.$strip>;
export type Prompt = z.infer<typeof PromptSchema>;
export declare const PromptsListRequestSchema: z.ZodObject<{}, z.core.$strip>;
export declare const PromptsListResponseSchema: z.ZodObject<{
    prompts: z.ZodArray<z.ZodObject<{
        name: z.ZodString;
        description: z.ZodOptional<z.ZodString>;
        arguments: z.ZodOptional<z.ZodArray<z.ZodObject<{
            name: z.ZodString;
            description: z.ZodOptional<z.ZodString>;
            required: z.ZodOptional<z.ZodBoolean>;
        }, z.core.$strip>>>;
    }, z.core.$strip>>;
}, z.core.$strip>;
export type PromptsListResponse = z.infer<typeof PromptsListResponseSchema>;
export declare const PromptsGetRequestSchema: z.ZodObject<{
    name: z.ZodString;
    arguments: z.ZodOptional<z.ZodRecord<z.ZodString, z.ZodString>>;
}, z.core.$strip>;
export declare const PromptMessageSchema: z.ZodObject<{
    role: z.ZodEnum<{
        user: "user";
        assistant: "assistant";
    }>;
    content: z.ZodObject<{
        type: z.ZodEnum<{
            text: "text";
            resource: "resource";
            image: "image";
        }>;
        text: z.ZodOptional<z.ZodString>;
        data: z.ZodOptional<z.ZodString>;
        mimeType: z.ZodOptional<z.ZodString>;
        uri: z.ZodOptional<z.ZodString>;
    }, z.core.$strip>;
}, z.core.$strip>;
export declare const PromptsGetResponseSchema: z.ZodObject<{
    messages: z.ZodArray<z.ZodObject<{
        role: z.ZodEnum<{
            user: "user";
            assistant: "assistant";
        }>;
        content: z.ZodObject<{
            type: z.ZodEnum<{
                text: "text";
                resource: "resource";
                image: "image";
            }>;
            text: z.ZodOptional<z.ZodString>;
            data: z.ZodOptional<z.ZodString>;
            mimeType: z.ZodOptional<z.ZodString>;
            uri: z.ZodOptional<z.ZodString>;
        }, z.core.$strip>;
    }, z.core.$strip>>;
}, z.core.$strip>;
export type PromptsGetResponse = z.infer<typeof PromptsGetResponseSchema>;
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
export declare const ErrorCodes: {
    readonly ParseError: -32700;
    readonly InvalidRequest: -32600;
    readonly MethodNotFound: -32601;
    readonly InvalidParams: -32602;
    readonly InternalError: -32603;
    readonly ServerError: -32000;
};
export declare const NotificationMethods: {
    readonly Initialized: "notifications/initialized";
    readonly ToolsListChanged: "notifications/tools/list_changed";
    readonly ResourcesListChanged: "notifications/resources/list_changed";
    readonly ResourcesUpdated: "notifications/resources/updated";
    readonly PromptsListChanged: "notifications/prompts/list_changed";
};
export declare const RequestMethods: {
    readonly Initialize: "initialize";
    readonly ToolsList: "tools/list";
    readonly ToolsCall: "tools/call";
    readonly ResourcesList: "resources/list";
    readonly ResourcesRead: "resources/read";
    readonly ResourcesSubscribe: "resources/subscribe";
    readonly ResourcesUnsubscribe: "resources/unsubscribe";
    readonly PromptsList: "prompts/list";
    readonly PromptsGet: "prompts/get";
};
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
//# sourceMappingURL=types.d.ts.map