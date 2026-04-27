/**
 * Custom Error Classes for Argentum
 *
 * Hierarchical error system with codes, context, and proper typing.
 * All errors extend DomainError for consistent handling.
 */

/** Base error class for all Argentum errors */
export abstract class DomainError extends Error {
  public readonly code: string;
  public readonly context: Record<string, unknown>;
  public readonly timestamp: number;
  public readonly isOperational: boolean;

  constructor(
    message: string,
    code: string,
    context: Record<string, unknown> = {},
    isOperational = true,
  ) {
    super(message);
    this.name = this.constructor.name;
    this.code = code;
    this.context = context;
    this.timestamp = Date.now();
    this.isOperational = isOperational;
    Error.captureStackTrace(this, this.constructor);
  }

  toJSON(): Record<string, unknown> {
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

/** Configuration related errors */
export class ConfigurationError extends DomainError {
  constructor(message: string, context: Record<string, unknown> = {}) {
    super(message, 'CONFIG_ERROR', context);
  }
}

/** LLM provider errors */
export class LLMProviderError extends DomainError {
  public readonly provider: string;
  public readonly statusCode?: number;

  constructor(
    message: string,
    provider: string,
    statusCode?: number,
    context: Record<string, unknown> = {},
  ) {
    super(message, 'LLM_PROVIDER_ERROR', { provider, statusCode, ...context });
    this.provider = provider;
    this.statusCode = statusCode;
  }
}

/** Feature loading and lifecycle errors */
export class FeatureError extends DomainError {
  public readonly feature: string;
  public readonly phase: 'load' | 'init' | 'start' | 'stop' | 'health';

  constructor(
    message: string,
    feature: string,
    phase: FeatureError['phase'],
    context: Record<string, unknown> = {},
  ) {
    super(message, 'FEATURE_ERROR', { feature, phase, ...context });
    this.feature = feature;
    this.phase = phase;
  }
}

/** Memory operation errors */
export class MemoryError extends DomainError {
  public readonly operation: string;
  public readonly memoryType: string;

  constructor(
    message: string,
    operation: string,
    memoryType: string,
    context: Record<string, unknown> = {},
  ) {
    super(message, 'MEMORY_ERROR', { operation, memoryType, ...context });
    this.operation = operation;
    this.memoryType = memoryType;
  }
}

/** Tool execution errors */
export class ToolExecutionError extends DomainError {
  public readonly tool: string;
  public readonly args: Record<string, unknown>;

  constructor(
    message: string,
    tool: string,
    args: Record<string, unknown> = {},
    context: Record<string, unknown> = {},
  ) {
    super(message, 'TOOL_EXECUTION_ERROR', { tool, args, ...context });
    this.tool = tool;
    this.args = args;
  }
}

/** Validation errors */
export class ValidationError extends DomainError {
  public readonly field: string;
  public readonly value: unknown;

  constructor(
    message: string,
    field: string,
    value: unknown,
    context: Record<string, unknown> = {},
  ) {
    super(message, 'VALIDATION_ERROR', { field, value, ...context });
    this.field = field;
    this.value = value;
  }
}

/** Agent execution errors */
export class AgentError extends DomainError {
  public readonly iteration: number;
  public readonly phase: 'planning' | 'execution' | 'reasoning' | 'tool_call';

  constructor(
    message: string,
    iteration: number,
    phase: AgentError['phase'],
    context: Record<string, unknown> = {},
  ) {
    super(message, 'AGENT_ERROR', { iteration, phase, ...context });
    this.iteration = iteration;
    this.phase = phase;
  }
}

/** Channel errors (Telegram, Discord, etc.) */
export class ChannelError extends DomainError {
  public readonly channel: string;
  public readonly event: string;

  constructor(
    message: string,
    channel: string,
    event: string,
    context: Record<string, unknown> = {},
  ) {
    super(message, 'CHANNEL_ERROR', { channel, event, ...context });
    this.channel = channel;
    this.event = event;
  }
}

/** Multi-agent coordination errors */
export class CoordinationError extends DomainError {
  public readonly agentId: string;
  public readonly action: string;

  constructor(
    message: string,
    agentId: string,
    action: string,
    context: Record<string, unknown> = {},
  ) {
    super(message, 'COORDINATION_ERROR', { agentId, action, ...context });
    this.agentId = agentId;
    this.action = action;
  }
}

/** Database/Persistence errors */
export class PersistenceError extends DomainError {
  public readonly operation: string;
  public readonly dbPath?: string;

  constructor(
    message: string,
    operation: string,
    dbPath?: string,
    context: Record<string, unknown> = {},
  ) {
    super(message, 'PERSISTENCE_ERROR', { operation, dbPath, ...context });
    this.operation = operation;
    this.dbPath = dbPath;
  }
}

/** Cache errors */
export class CacheError extends DomainError {
  public readonly key: string;
  public readonly operation: 'get' | 'set' | 'delete' | 'clear';

  constructor(
    message: string,
    key: string,
    operation: CacheError['operation'],
    context: Record<string, unknown> = {},
  ) {
    super(message, 'CACHE_ERROR', { key, operation, ...context });
    this.key = key;
    this.operation = operation;
  }
}

/** Rate limiting errors */
export class RateLimitError extends DomainError {
  public readonly limit: number;
  public readonly windowMs: number;
  public readonly retryAfterMs?: number;

  constructor(
    message: string,
    limit: number,
    windowMs: number,
    retryAfterMs?: number,
    context: Record<string, unknown> = {},
  ) {
    super(message, 'RATE_LIMIT_ERROR', { limit, windowMs, retryAfterMs, ...context });
    this.limit = limit;
    this.windowMs = windowMs;
    this.retryAfterMs = retryAfterMs;
  }
}

/** Error codes constants */
export const ErrorCodes = {
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
} as const;
