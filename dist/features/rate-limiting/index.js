"use strict";
/**
 * Rate Limiting Feature
 *
 * Provides sliding-window rate limiting with configurable limits and keys.
 * Can be used by other features via direct import.
 */
var __importDefault = (this && this.__importDefault) || function (mod) {
    return (mod && mod.__esModule) ? mod : { "default": mod };
};
Object.defineProperty(exports, "__esModule", { value: true });
const fs_1 = require("fs");
const path_1 = __importDefault(require("path"));
const better_sqlite3_1 = __importDefault(require("better-sqlite3"));
class RateLimitingFeature {
    meta = {
        name: 'rate-limiting',
        version: '0.0.5',
        description: 'Sliding window rate limiting with configurable limits',
        dependencies: [],
    };
    db;
    ctx;
    config = {
        windowMs: 60_000,
        max: 100,
    };
    constructor() {
        const dataDir = path_1.default.join(process.cwd(), 'data');
        (0, fs_1.mkdirSync)(dataDir, { recursive: true });
        const dbPath = process.env.AGCLAW_DB_PATH ?? path_1.default.join(dataDir, 'agclaw.db');
        this.db = new better_sqlite3_1.default(dbPath);
    }
    async init(config, context) {
        this.ctx = context;
        if (config['windowMs'])
            this.config['windowMs'] = config['windowMs'];
        if (config['max'])
            this.config['max'] = config['max'];
        this.initDb();
    }
    initDb() {
        this.db
            .prepare(`
      CREATE TABLE IF NOT EXISTS rate_windows (
        key TEXT NOT NULL,
        timestamp INTEGER NOT NULL
      );
    `)
            .run();
        this.db
            .prepare('CREATE INDEX IF NOT EXISTS idx_rate_key_ts ON rate_windows(key, timestamp)')
            .run();
    }
    async start() {
        this.ctx.logger.info('Rate limiting started', {
            windowMs: this.config.windowMs,
            max: this.config.max,
        });
    }
    async stop() {
        this.db.close();
    }
    async healthCheck() {
        // Simple check: can we write/read?
        try {
            const testKey = `health_${Date.now()}`;
            this.checkInternal(testKey, 1, 1000);
            return { healthy: true, message: 'Rate limiting OK' };
        }
        catch (err) {
            return { healthy: false, message: 'Rate limiting check failed' };
        }
    }
    // Sliding window: store timestamps and count how many within window
    check(key, limit, windowMs) {
        return this.checkInternal(key, limit ?? this.config.max, windowMs ?? this.config.windowMs);
    }
    checkInternal(key, limit, windowMs) {
        const now = Date.now();
        const cutoff = now - windowMs;
        // Clean up old entries occasionally (every 1000th check)
        if (Math.random() < 0.001) {
            this.db
                .prepare('DELETE FROM rate_windows WHERE timestamp < ?')
                .run(Date.now() - windowMs * 2);
        }
        const insert = this.db.prepare('INSERT INTO rate_windows (key, timestamp) VALUES (?, ?)');
        insert.run(key, now);
        const row = this.db
            .prepare('SELECT COUNT(*) as c FROM rate_windows WHERE key = ? AND timestamp >= ?')
            .get(key, cutoff);
        const count = row.c;
        return { allowed: count <= limit, count };
    }
    reset(key) {
        const info = this.db.prepare('DELETE FROM rate_windows WHERE key = ?').run(key);
        return { changes: info.changes };
    }
    getStats(key) {
        const rows = this.db
            .prepare('SELECT timestamp FROM rate_windows WHERE key = ? ORDER BY timestamp DESC LIMIT 1000')
            .all(key);
        return rows.map((r) => r.timestamp);
    }
}
exports.default = new RateLimitingFeature();
//# sourceMappingURL=index.js.map