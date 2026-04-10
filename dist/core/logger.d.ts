/**
 * AG-Claw Structured Logger
 *
 * Pino-based logging with contextual metadata, log levels, and multiple transports.
 * Supports JSON output for production and pretty-print for development.
 */
/** Log level enumeration */
export type LogLevel = 'debug' | 'info' | 'warn' | 'error' | 'fatal';
/** Logger context for structured logging */
export interface LogContext {
    feature?: string;
    channel?: string;
    userId?: string;
    sessionId?: string;
    requestId?: string;
    [key: string]: unknown;
}
/** Logger configuration */
export interface LoggerConfig {
    level: LogLevel;
    format: 'json' | 'pretty';
    file?: string;
    context?: LogContext;
}
/**
 * Structured logger wrapping Pino with AG-Claw conventions.
 *
 * Each feature and channel gets its own child logger with persistent context.
 *
 * @example
 * ```ts
 * const log = createLogger({ feature: 'webchat' });
 * log.info('User connected', { userId: 'u123' });
 * log.error('Connection failed', { error: err.message });
 * ```
 */
export declare class Logger {
    private pino;
    private context;
    constructor(config?: Partial<LoggerConfig>);
    /** Create a child logger with additional context */
    child(context: LogContext): Logger;
    /** Log at debug level */
    debug(message: string, data?: Record<string, unknown>): void;
    /** Log at info level */
    info(message: string, data?: Record<string, unknown>): void;
    /** Log at warn level */
    warn(message: string, data?: Record<string, unknown>): void;
    /** Log at error level */
    error(message: string, data?: Record<string, unknown>): void;
    /** Log at fatal level */
    fatal(message: string, data?: Record<string, unknown>): void;
    /** Log an error object with stack trace */
    exception(error: Error, message?: string, data?: Record<string, unknown>): void;
    /** Set log level dynamically */
    setLevel(level: LogLevel): void;
    /** Get current log level */
    getLevel(): string;
}
/** Create or get the global logger */
export declare function createLogger(config?: Partial<LoggerConfig>): Logger;
/** Create a feature-scoped logger */
export declare function featureLogger(feature: string, extraContext?: Record<string, unknown>): Logger;
/** Reset global logger (for testing) */
export declare function resetLogger(): void;
//# sourceMappingURL=logger.d.ts.map