/**
 * Custom Error Classes for AG-Claw
 *
 * Hierarchical error system with codes, context, and proper typing.
 * All errors extend DomainError for consistent handling.
 */
/** Base error class for all AG-Claw errors */
export declare abstract class DomainError extends Error {
    readonly code: string;
    readonly context: Record<string, unknown>;
    readonly timestamp: number;
    readonly isOperational: boolean;
    constructor(message: string, code: string, context?: Record<string, unknown>, isOperational?: boolean);
    toJSON(): Record<string, unknown>;
}
/** Configuration related errors */
export declare class ConfigurationError extends DomainError {
    constructor(message: string, context?: Record<string, unknown>);
}
/** LLM provider errors */
export declare class LLMProviderError extends DomainError {
    readonly provider: string;
    readonly statusCode?: number;
    constructor(message: string, provider: string, statusCode?: number, context?: Record<string, unknown>);
}
/** Feature loading and lifecycle errors */
export declare class FeatureError extends DomainError {
    readonly feature: string;
    readonly phase: 'load' | 'init' | 'start' | 'stop' | 'health';
    constructor(message: string, feature: string, phase: FeatureError['phase'], context?: Record<string, unknown>);
}
/** Memory operation errors */
export declare class MemoryError extends DomainError {
    readonly operation: string;
    readonly memoryType: string;
    constructor(message: string, operation: string, memoryType: string, context?: Record<string, unknown>);
}
/** Tool execution errors */
export declare class ToolExecutionError extends DomainError {
    readonly tool: string;
    readonly args: Record<string, unknown>;
    constructor(message: string, tool: string, args?: Record<string, unknown>, context?: Record<string, unknown>);
}
/** Validation errors */
export declare class ValidationError extends DomainError {
    readonly field: string;
    readonly value: unknown;
    constructor(message: string, field: string, value: unknown, context?: Record<string, unknown>);
}
/** Agent execution errors */
export declare class AgentError extends DomainError {
    readonly iteration: number;
    readonly phase: 'planning' | 'execution' | 'reasoning' | 'tool_call';
    constructor(message: string, iteration: number, phase: AgentError['phase'], context?: Record<string, unknown>);
}
/** Channel errors (Telegram, Discord, etc.) */
export declare class ChannelError extends DomainError {
    readonly channel: string;
    readonly event: string;
    constructor(message: string, channel: string, event: string, context?: Record<string, unknown>);
}
/** Multi-agent coordination errors */
export declare class CoordinationError extends DomainError {
    readonly agentId: string;
    readonly action: string;
    constructor(message: string, agentId: string, action: string, context?: Record<string, unknown>);
}
/** Database/Persistence errors */
export declare class PersistenceError extends DomainError {
    readonly operation: string;
    readonly dbPath?: string;
    constructor(message: string, operation: string, dbPath?: string, context?: Record<string, unknown>);
}
/** Cache errors */
export declare class CacheError extends DomainError {
    readonly key: string;
    readonly operation: 'get' | 'set' | 'delete' | 'clear';
    constructor(message: string, key: string, operation: CacheError['operation'], context?: Record<string, unknown>);
}
/** Rate limiting errors */
export declare class RateLimitError extends DomainError {
    readonly limit: number;
    readonly windowMs: number;
    readonly retryAfterMs?: number;
    constructor(message: string, limit: number, windowMs: number, retryAfterMs?: number, context?: Record<string, unknown>);
}
/** Error codes constants */
export declare const ErrorCodes: {
    readonly CONFIG_ERROR: "CONFIG_ERROR";
    readonly LLM_PROVIDER_ERROR: "LLM_PROVIDER_ERROR";
    readonly FEATURE_ERROR: "FEATURE_ERROR";
    readonly MEMORY_ERROR: "MEMORY_ERROR";
    readonly TOOL_EXECUTION_ERROR: "TOOL_EXECUTION_ERROR";
    readonly VALIDATION_ERROR: "VALIDATION_ERROR";
    readonly AGENT_ERROR: "AGENT_ERROR";
    readonly CHANNEL_ERROR: "CHANNEL_ERROR";
    readonly COORDINATION_ERROR: "COORDINATION_ERROR";
    readonly PERSISTENCE_ERROR: "PERSISTENCE_ERROR";
    readonly CACHE_ERROR: "CACHE_ERROR";
    readonly RATE_LIMIT_ERROR: "RATE_LIMIT_ERROR";
    readonly UNKNOWN_ERROR: "UNKNOWN_ERROR";
};
//# sourceMappingURL=errors.d.ts.map