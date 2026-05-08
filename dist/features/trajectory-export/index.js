"use strict";
/**
 * Trajectory Export Feature
 *
 * Records conversation trajectories for RL fine-tuning data preparation.
 * Supports JSONL export with optional gzip compression.
 *
 * Stores entries in SQLite alongside the sessions database for easy joining.
 */
var __createBinding = (this && this.__createBinding) || (Object.create ? (function(o, m, k, k2) {
    if (k2 === undefined) k2 = k;
    var desc = Object.getOwnPropertyDescriptor(m, k);
    if (!desc || ("get" in desc ? !m.__esModule : desc.writable || desc.configurable)) {
      desc = { enumerable: true, get: function() { return m[k]; } };
    }
    Object.defineProperty(o, k2, desc);
}) : (function(o, m, k, k2) {
    if (k2 === undefined) k2 = k;
    o[k2] = m[k];
}));
var __setModuleDefault = (this && this.__setModuleDefault) || (Object.create ? (function(o, v) {
    Object.defineProperty(o, "default", { enumerable: true, value: v });
}) : function(o, v) {
    o["default"] = v;
});
var __importStar = (this && this.__importStar) || (function () {
    var ownKeys = function(o) {
        ownKeys = Object.getOwnPropertyNames || function (o) {
            var ar = [];
            for (var k in o) if (Object.prototype.hasOwnProperty.call(o, k)) ar[ar.length] = k;
            return ar;
        };
        return ownKeys(o);
    };
    return function (mod) {
        if (mod && mod.__esModule) return mod;
        var result = {};
        if (mod != null) for (var k = ownKeys(mod), i = 0; i < k.length; i++) if (k[i] !== "default") __createBinding(result, mod, k[i]);
        __setModuleDefault(result, mod);
        return result;
    };
})();
var __importDefault = (this && this.__importDefault) || function (mod) {
    return (mod && mod.__esModule) ? mod : { "default": mod };
};
Object.defineProperty(exports, "__esModule", { value: true });
const fs_1 = require("fs");
const path_1 = require("path");
const better_sqlite3_1 = __importDefault(require("better-sqlite3"));
// ─── Feature ─────────────────────────────────────────────────────────────────
class TrajectoryExportFeature {
    meta = {
        name: 'trajectory-export',
        version: '0.0.4',
        description: 'Export conversation trajectories as JSONL for RL fine-tuning',
        dependencies: [],
    };
    config = {
        enabled: false,
        dbPath: '',
        compressExports: false,
        defaultFormat: 'jsonl',
    };
    ctx;
    db;
    dbPath;
    constructor() {
        // Auto-init for CLI usage (singleton bypasses plugin loader)
        const workDir = process.env.AGCLAW_WORKDIR ?? process.cwd();
        this.dbPath = (0, path_1.resolve)((0, path_1.join)(workDir, 'data', 'trajectory.db'));
        this.config.dbPath = this.dbPath;
        this.initDatabase();
    }
    async init(config, context) {
        this.ctx = context;
        this.config = {
            enabled: true,
            dbPath: (0, path_1.resolve)(config['dbPath'] ?? './data/trajectory.db'),
            compressExports: config['compressExports'] ?? false,
            defaultFormat: config['defaultFormat'] ?? 'jsonl',
        };
        this.dbPath = this.config.dbPath;
        this.initDatabase();
    }
    async start() {
        this.ctx.logger.info('Trajectory export feature started', {
            dbPath: this.dbPath,
            compressExports: this.config.compressExports,
        });
    }
    async stop() {
        this.db?.close();
    }
    async healthCheck() {
        try {
            const row = this.db.prepare('SELECT COUNT(*) as c FROM trajectories').get();
            return { healthy: true, message: `${row.c} trajectory entries` };
        }
        catch {
            return { healthy: false, message: 'Trajectory database unavailable' };
        }
    }
    // ─── Public API ──────────────────────────────────────────────────────────
    /**
     * Record a trajectory entry after a conversation completes.
     */
    record(entry) {
        const stmt = this.db.prepare(`
      INSERT INTO trajectories (timestamp, session_id, messages, metadata)
      VALUES (?, ?, ?, ?)
    `);
        stmt.run(entry.timestamp, entry.sessionId, JSON.stringify(entry.messages), JSON.stringify(entry.metadata));
    }
    /**
     * Record a raw conversation as a trajectory.
     */
    recordConversation(sessionId, messages, metadata) {
        this.record({
            timestamp: new Date().toISOString(),
            sessionId,
            messages,
            metadata,
        });
    }
    /**
     * Export trajectories matching the given options.
     * Returns the path to the exported file.
     */
    async export(options = {}) {
        const entries = this.queryEntries(options);
        const format = options.format ?? this.config.defaultFormat;
        const gzip = options.gzip ?? this.config.compressExports;
        const timestamp = new Date().toISOString().replace(/[:.]/g, '-').slice(0, 19);
        const ext = gzip ? 'jsonl.gz' : 'jsonl';
        const exportDir = (0, path_1.resolve)((0, path_1.dirname)(this.dbPath));
        const outPath = (0, path_1.resolve)(exportDir, `trajectory-export-${timestamp}.${ext}`);
        if (!(0, fs_1.existsSync)(exportDir)) {
            (0, fs_1.mkdirSync)(exportDir, { recursive: true });
        }
        if (format === 'jsonl') {
            const content = entries.map((e) => JSON.stringify(e)).join('\n');
            if (gzip) {
                const { writeFileSync } = await Promise.resolve().then(() => __importStar(require('fs')));
                const zlib = await Promise.resolve().then(() => __importStar(require('zlib')));
                const gz = zlib.createGzip();
                const buf = Buffer.from(content, 'utf8');
                const result = await new Promise((resolve, reject) => {
                    const chunks = [];
                    gz.on('data', (chunk) => chunks.push(chunk));
                    gz.on('end', () => resolve(Buffer.concat(chunks)));
                    gz.on('error', reject);
                    gz.end(buf);
                });
                writeFileSync(outPath, result);
            }
            else {
                const { writeFileSync } = await Promise.resolve().then(() => __importStar(require('fs')));
                writeFileSync(outPath, content, 'utf8');
            }
        }
        else {
            const { writeFileSync } = await Promise.resolve().then(() => __importStar(require('fs')));
            writeFileSync(outPath, JSON.stringify(entries, null, 2), 'utf8');
        }
        this.ctx.logger.info('Trajectory export complete', { path: outPath, count: entries.length });
        return outPath;
    }
    /**
     * Export a single session as JSONL string.
     */
    exportSession(sessionId) {
        return this.queryEntries({ sessionId });
    }
    /**
     * Get aggregate statistics across all trajectories.
     */
    getStats(options = {}) {
        const entries = this.queryEntries(options);
        const stats = {
            totalSessions: new Set(entries.map((e) => e.sessionId)).size,
            totalMessages: entries.reduce((sum, e) => sum + e.messages.length, 0),
            totalTokens: entries.reduce((sum, e) => sum + e.metadata.tokens, 0),
            totalCost: entries.reduce((sum, e) => sum + e.metadata.cost, 0),
            byAgent: {},
            byTag: {},
        };
        for (const entry of entries) {
            const agentId = entry.metadata.agentId ?? 'unknown';
            stats.byAgent[agentId] ??= { sessions: 0, messages: 0, tokens: 0, cost: 0 };
            stats.byAgent[agentId].sessions++;
            stats.byAgent[agentId].messages += entry.messages.length;
            stats.byAgent[agentId].tokens += entry.metadata.tokens;
            stats.byAgent[agentId].cost += entry.metadata.cost;
            for (const tag of entry.metadata.tags) {
                stats.byTag[tag] ??= { sessions: 0, messages: 0 };
                stats.byTag[tag].sessions++;
                stats.byTag[tag].messages += entry.messages.length;
            }
        }
        return stats;
    }
    // ─── Private helpers ─────────────────────────────────────────────────────
    initDatabase() {
        const fullPath = this.dbPath;
        if (!(0, fs_1.existsSync)((0, path_1.dirname)(fullPath))) {
            (0, fs_1.mkdirSync)((0, path_1.dirname)(fullPath), { recursive: true });
        }
        this.db = new better_sqlite3_1.default(fullPath);
        this.db.pragma('journal_mode = WAL');
        this.db.exec(`
      CREATE TABLE IF NOT EXISTS trajectories (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        timestamp TEXT NOT NULL,
        session_id TEXT NOT NULL,
        messages TEXT NOT NULL,
        metadata TEXT NOT NULL
      );

      CREATE INDEX IF NOT EXISTS idx_trajectories_session ON trajectories(session_id);
      CREATE INDEX IF NOT EXISTS idx_trajectories_timestamp ON trajectories(timestamp);
      CREATE INDEX IF NOT EXISTS idx_trajectories_agent ON trajectories(metadata);
    `);
    }
    queryEntries(options) {
        const conditions = [];
        const params = [];
        if (options.sessionId) {
            conditions.push('session_id = ?');
            params.push(options.sessionId);
        }
        if (options.since) {
            conditions.push('timestamp >= ?');
            params.push(options.since.toISOString());
        }
        if (options.until) {
            conditions.push('timestamp <= ?');
            params.push(options.until.toISOString());
        }
        const where = conditions.length > 0 ? `WHERE ${conditions.join(' AND ')}` : '';
        const rows = this.db
            .prepare(`SELECT * FROM trajectories ${where} ORDER BY timestamp ASC`)
            .all(...params);
        return rows
            .map((row) => {
            let entry;
            try {
                const messages = JSON.parse(row.messages);
                const metadata = JSON.parse(row.metadata);
                entry = {
                    timestamp: row.timestamp,
                    sessionId: row.session_id,
                    messages,
                    metadata,
                };
            }
            catch {
                return null;
            }
            // Filter by agentId if specified
            if (options.agentId && entry.metadata.agentId !== options.agentId) {
                return null;
            }
            // Filter by tags if specified
            if (options.tags && options.tags.length > 0) {
                const hasTag = options.tags.some((t) => entry.metadata.tags.includes(t));
                if (!hasTag)
                    return null;
            }
            return entry;
        })
            .filter((e) => e !== null);
    }
}
exports.default = new TrajectoryExportFeature();
//# sourceMappingURL=index.js.map