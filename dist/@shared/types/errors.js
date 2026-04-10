"use strict";
/**
 * Custom Error Classes for AG-Claw
 *
 * Hierarchical error system with codes, context, and proper typing.
 * All errors extend DomainError for consistent handling.
 */
Object.defineProperty(exports, "__esModule", { value: true });
exports.ErrorCodes = exports.RateLimitError = exports.CacheError = exports.PersistenceError = exports.CoordinationError = exports.ChannelError = exports.AgentError = exports.ValidationError = exports.ToolExecutionError = exports.MemoryError = exports.FeatureError = exports.LLMProviderError = exports.ConfigurationError = exports.DomainError = void 0;
/** Base error class for all AG-Claw errors */
class DomainError extends Error {
    code;
    context;
    timestamp;
    isOperational;
    constructor(message, code, context = {}, isOperational = true) {
        super(message);
        this.name = this.constructor.name;
        this.code = code;
        this.context = context;
        this.timestamp = Date.now();
        this.isOperational = isOperational;
        Error.captureStackTrace(this, this.constructor);
    }
    toJSON() {
        return {
            name: this.name,
            message: this.message,
            code: this.code,
            context: this.context,
            timestamp: this.timestamp,
            stack: this.stack,
        };
    }
}
exports.DomainError = DomainError;
/** Configuration related errors */
class ConfigurationError extends DomainError {
    constructor(message, context = {}) {
        super(message, 'CONFIG_ERROR', context);
    }
}
exports.ConfigurationError = ConfigurationError;
/** LLM provider errors */
class LLMProviderError extends DomainError {
    provider;
    statusCode;
    constructor(message, provider, statusCode, context = {}) {
        super(message, 'LLM_PROVIDER_ERROR', { provider, statusCode, ...context });
        this.provider = provider;
        this.statusCode = statusCode;
    }
}
exports.LLMProviderError = LLMProviderError;
/** Feature loading and lifecycle errors */
class FeatureError extends DomainError {
    feature;
    phase;
    constructor(message, feature, phase, context = {}) {
        super(message, 'FEATURE_ERROR', { feature, phase, ...context });
        this.feature = feature;
        this.phase = phase;
    }
}
exports.FeatureError = FeatureError;
/** Memory operation errors */
class MemoryError extends DomainError {
    operation;
    memoryType;
    constructor(message, operation, memoryType, context = {}) {
        super(message, 'MEMORY_ERROR', { operation, memoryType, ...context });
        this.operation = operation;
        this.memoryType = memoryType;
    }
}
exports.MemoryError = MemoryError;
/** Tool execution errors */
class ToolExecutionError extends DomainError {
    tool;
    args;
    constructor(message, tool, args = {}, context = {}) {
        super(message, 'TOOL_EXECUTION_ERROR', { tool, args, ...context });
        this.tool = tool;
        this.args = args;
    }
}
exports.ToolExecutionError = ToolExecutionError;
/** Validation errors */
class ValidationError extends DomainError {
    field;
    value;
    constructor(message, field, value, context = {}) {
        super(message, 'VALIDATION_ERROR', { field, value, ...context });
        this.field = field;
        this.value = value;
    }
}
exports.ValidationError = ValidationError;
/** Agent execution errors */
class AgentError extends DomainError {
    iteration;
    phase;
    constructor(message, iteration, phase, context = {}) {
        super(message, 'AGENT_ERROR', { iteration, phase, ...context });
        this.iteration = iteration;
        this.phase = phase;
    }
}
exports.AgentError = AgentError;
/** Channel errors (Telegram, Discord, etc.) */
class ChannelError extends DomainError {
    channel;
    event;
    constructor(message, channel, event, context = {}) {
        super(message, 'CHANNEL_ERROR', { channel, event, ...context });
        this.channel = channel;
        this.event = event;
    }
}
exports.ChannelError = ChannelError;
/** Multi-agent coordination errors */
class CoordinationError extends DomainError {
    agentId;
    action;
    constructor(message, agentId, action, context = {}) {
        super(message, 'COORDINATION_ERROR', { agentId, action, ...context });
        this.agentId = agentId;
        this.action = action;
    }
}
exports.CoordinationError = CoordinationError;
/** Database/Persistence errors */
class PersistenceError extends DomainError {
    operation;
    dbPath;
    constructor(message, operation, dbPath, context = {}) {
        super(message, 'PERSISTENCE_ERROR', { operation, dbPath, ...context });
        this.operation = operation;
        this.dbPath = dbPath;
    }
}
exports.PersistenceError = PersistenceError;
/** Cache errors */
class CacheError extends DomainError {
    key;
    operation;
    constructor(message, key, operation, context = {}) {
        super(message, 'CACHE_ERROR', { key, operation, ...context });
        this.key = key;
        this.operation = operation;
    }
}
exports.CacheError = CacheError;
/** Rate limiting errors */
class RateLimitError extends DomainError {
    limit;
    windowMs;
    retryAfterMs;
    constructor(message, limit, windowMs, retryAfterMs, context = {}) {
        super(message, 'RATE_LIMIT_ERROR', { limit, windowMs, retryAfterMs, ...context });
        this.limit = limit;
        this.windowMs = windowMs;
        this.retryAfterMs = retryAfterMs;
    }
}
exports.RateLimitError = RateLimitError;
/** Error codes constants */
exports.ErrorCodes = {
    CONFIG_ERROR: 'CONFIG_ERROR',
    LLM_PROVIDER_ERROR: 'LLM_PROVIDER_ERROR',
    FEATURE_ERROR: 'FEATURE_ERROR',
    MEMORY_ERROR: 'MEMORY_ERROR',
    TOOL_EXECUTION_ERROR: 'TOOL_EXECUTION_ERROR',
    VALIDATION_ERROR: 'VALIDATION_ERROR',
    AGENT_ERROR: 'AGENT_ERROR',
    CHANNEL_ERROR: 'CHANNEL_ERROR',
    COORDINATION_ERROR: 'COORDINATION_ERROR',
    PERSISTENCE_ERROR: 'PERSISTENCE_ERROR',
    CACHE_ERROR: 'CACHE_ERROR',
    RATE_LIMIT_ERROR: 'RATE_LIMIT_ERROR',
    UNKNOWN_ERROR: 'UNKNOWN_ERROR',
};
//# sourceMappingURL=errors.js.map