/**
 * Argentum Structured Logger
 *
 * Pino-based logging with contextual metadata, log levels, and multiple transports.
 * Supports JSON output for production and pretty-print for development.
 */

import pino, { type Logger as PinoLogger, type LoggerOptions } from 'pino';

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

/** Default logger configuration */
const DEFAULT_CONFIG: LoggerConfig = {
  level: 'info',
  format: 'pretty',
};

/**
 * Structured logger wrapping Pino with Argentum conventions.
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
export class Logger {
  private pino: PinoLogger;
  private context: LogContext;

  constructor(config: Partial<LoggerConfig> = {}) {
    const merged = { ...DEFAULT_CONFIG, ...config };
    if (
      process.env.ARGENTUM_LOG_FORMAT === 'json' ||
      process.env.ARGENTUM_LOG_FORMAT === 'pretty'
    ) {
      merged.format = process.env.ARGENTUM_LOG_FORMAT;
    }
    this.context = merged.context ?? {};

    const options: LoggerOptions = {
      level: merged.level,
    };

    if (merged.format === 'pretty') {
      options.transport = {
        target: 'pino-pretty',
        options: {
          colorize: true,
          translateTime: 'SYS:yyyy-mm-dd HH:MM:ss',
          ignore: 'pid,hostname',
        },
      };
    }

    this.pino = pino(options);

    if (merged.file) {
      // File transport is added via pino.destination in production
      // For now, pretty + file requires multi-stream setup
    }
  }

  /** Create a child logger with additional context */
  child(context: LogContext): Logger {
    const child = new Logger();
    child.pino = this.pino.child(context);
    child.context = { ...this.context, ...context };
    return child;
  }

  /** Log at debug level */
  debug(message: string, data?: Record<string, unknown>): void {
    this.pino.debug({ ...this.context, ...data }, message);
  }

  /** Log at info level */
  info(message: string, data?: Record<string, unknown>): void {
    this.pino.info({ ...this.context, ...data }, message);
  }

  /** Log at warn level */
  warn(message: string, data?: Record<string, unknown>): void {
    this.pino.warn({ ...this.context, ...data }, message);
  }

  /** Log at error level */
  error(message: string, data?: Record<string, unknown>): void {
    this.pino.error({ ...this.context, ...data }, message);
  }

  /** Log at fatal level */
  fatal(message: string, data?: Record<string, unknown>): void {
    this.pino.fatal({ ...this.context, ...data }, message);
  }

  /** Log an error object with stack trace */
  exception(error: Error, message?: string, data?: Record<string, unknown>): void {
    this.pino.error(
      {
        ...this.context,
        ...data,
        err: {
          message: error.message,
          stack: error.stack,
          name: error.name,
        },
      },
      message ?? error.message,
    );
  }

  /** Set log level dynamically */
  setLevel(level: LogLevel): void {
    this.pino.level = level;
  }

  /** Get current log level */
  getLevel(): string {
    return this.pino.level;
  }
}

/** Global logger instance */
let globalLogger: Logger | null = null;

/** Create or get the global logger */
export function createLogger(config?: Partial<LoggerConfig>): Logger {
  if (!globalLogger) {
    globalLogger = new Logger(config);
  }
  return globalLogger;
}

/** Create a feature-scoped logger */
export function featureLogger(feature: string, extraContext?: Record<string, unknown>): Logger {
  const base = createLogger();
  return base.child({ feature, ...extraContext });
}

/** Reset global logger (for testing) */
export function resetLogger(): void {
  globalLogger = null;
}
