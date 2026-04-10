"use strict";
/**
 * AG-Claw Structured Logger
 *
 * Pino-based logging with contextual metadata, log levels, and multiple transports.
 * Supports JSON output for production and pretty-print for development.
 */
var __importDefault = (this && this.__importDefault) || function (mod) {
    return (mod && mod.__esModule) ? mod : { "default": mod };
};
Object.defineProperty(exports, "__esModule", { value: true });
exports.Logger = void 0;
exports.createLogger = createLogger;
exports.featureLogger = featureLogger;
exports.resetLogger = resetLogger;
const pino_1 = __importDefault(require("pino"));
/** Default logger configuration */
const DEFAULT_CONFIG = {
    level: 'info',
    format: 'pretty',
};
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
class Logger {
    constructor(config = {}) {
        const merged = { ...DEFAULT_CONFIG, ...config };
        this.context = merged.context ?? {};
        const options = {
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
        this.pino = (0, pino_1.default)(options);
        if (merged.file) {
            // File transport is added via pino.destination in production
            // For now, pretty + file requires multi-stream setup
        }
    }
    /** Create a child logger with additional context */
    child(context) {
        const child = new Logger();
        child.pino = this.pino.child(context);
        child.context = { ...this.context, ...context };
        return child;
    }
    /** Log at debug level */
    debug(message, data) {
        this.pino.debug({ ...this.context, ...data }, message);
    }
    /** Log at info level */
    info(message, data) {
        this.pino.info({ ...this.context, ...data }, message);
    }
    /** Log at warn level */
    warn(message, data) {
        this.pino.warn({ ...this.context, ...data }, message);
    }
    /** Log at error level */
    error(message, data) {
        this.pino.error({ ...this.context, ...data }, message);
    }
    /** Log at fatal level */
    fatal(message, data) {
        this.pino.fatal({ ...this.context, ...data }, message);
    }
    /** Log an error object with stack trace */
    exception(error, message, data) {
        this.pino.error({
            ...this.context,
            ...data,
            err: {
                message: error.message,
                stack: error.stack,
                name: error.name,
            },
        }, message ?? error.message);
    }
    /** Set log level dynamically */
    setLevel(level) {
        this.pino.level = level;
    }
    /** Get current log level */
    getLevel() {
        return this.pino.level;
    }
}
exports.Logger = Logger;
/** Global logger instance */
let globalLogger = null;
/** Create or get the global logger */
function createLogger(config) {
    if (!globalLogger) {
        globalLogger = new Logger(config);
    }
    return globalLogger;
}
/** Create a feature-scoped logger */
function featureLogger(feature, extraContext) {
    const base = createLogger();
    return base.child({ feature, ...extraContext });
}
/** Reset global logger (for testing) */
function resetLogger() {
    globalLogger = null;
}
