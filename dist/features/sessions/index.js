"use strict";
/**
 * Argentum Sessions
 *
 * Session management for conversations and agent interactions.
 * Stores messages, tracks context, manages history.
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
Object.defineProperty(exports, "__esModule", { value: true });
const crypto = __importStar(require("crypto"));
const fs = __importStar(require("fs"));
const path = __importStar(require("path"));
// ─── Feature ─────────────────────────────────────────────────────────────────
class SessionsFeature {
    meta = {
        name: 'sessions',
        version: '0.0.3',
        description: 'Session management for conversations and agent interactions',
        dependencies: ['sqlite-memory'],
    };
    _ctx = null;
    db = null;
    async init(config, context) {
        this._ctx = context;
        const dbPath = config['dbPath'] || './data/sessions.db';
        const dir = path.dirname(dbPath);
        if (!fs.existsSync(dir))
            fs.mkdirSync(dir, { recursive: true });
        const Database = require('better-sqlite3');
        this.db = new Database(dbPath);
        this.db.pragma('journal_mode = WAL');
        this.db.exec(`
      CREATE TABLE IF NOT EXISTS sessions (
        id TEXT PRIMARY KEY,
        title TEXT NOT NULL DEFAULT 'New Session',
        created_at INTEGER NOT NULL,
        updated_at INTEGER NOT NULL,
        model TEXT DEFAULT '',
        status TEXT NOT NULL DEFAULT 'active',
        tags TEXT DEFAULT '[]',
        metadata TEXT DEFAULT '{}'
      );
      CREATE INDEX IF NOT EXISTS idx_sessions_status ON sessions(status);
      CREATE INDEX IF NOT EXISTS idx_sessions_updated ON sessions(updated_at);

      CREATE TABLE IF NOT EXISTS messages (
        id TEXT PRIMARY KEY,
        session_id TEXT NOT NULL,
        role TEXT NOT NULL,
        content TEXT NOT NULL,
        timestamp INTEGER NOT NULL,
        tool_calls TEXT,
        metadata TEXT DEFAULT '{}',
        FOREIGN KEY (session_id) REFERENCES sessions(id)
      );
      CREATE INDEX IF NOT EXISTS idx_messages_session ON messages(session_id);
      CREATE INDEX IF NOT EXISTS idx_messages_ts ON messages(timestamp);

      CREATE TABLE IF NOT EXISTS session_state (
        session_id TEXT PRIMARY KEY,
        key TEXT NOT NULL,
        value TEXT NOT NULL,
        updated_at INTEGER NOT NULL,
        FOREIGN KEY (session_id) REFERENCES sessions(id)
      );
    `);
    }
    async start() { }
    async stop() {
        if (this.db)
            this.db.close();
    }
    async healthCheck() {
        try {
            const sessionCount = this.db.prepare('SELECT COUNT(*) as c FROM sessions').get().c;
            const messageCount = this.db.prepare('SELECT COUNT(*) as c FROM messages').get().c;
            return {
                healthy: true,
                details: { sessions: sessionCount, messages: messageCount },
            };
        }
        catch {
            return { healthy: false, details: { error: 'Database error' } };
        }
    }
    // ─── Session CRUD ───────────────────────────────────────────────────────
    create(title, model) {
        const id = crypto.randomUUID();
        const now = Date.now();
        const session = {
            id,
            title: title || 'New Session',
            createdAt: now,
            updatedAt: now,
            messageCount: 0,
            model: model || '',
            status: 'active',
            tags: [],
            metadata: {},
        };
        this.db
            .prepare('INSERT INTO sessions (id, title, created_at, updated_at, model, status) VALUES (?, ?, ?, ?, ?, ?)')
            .run(id, session.title, now, now, session.model, 'active');
        return session;
    }
    get(id) {
        const row = this.db.prepare('SELECT * FROM sessions WHERE id = ?').get(id);
        if (!row)
            return null;
        const msgCount = this.db
            .prepare('SELECT COUNT(*) as c FROM messages WHERE session_id = ?')
            .get(id).c;
        return {
            id: row.id,
            title: row.title,
            createdAt: row.created_at,
            updatedAt: row.updated_at,
            messageCount: msgCount,
            model: row.model,
            status: row.status,
            tags: JSON.parse(row.tags || '[]'),
            metadata: JSON.parse(row.metadata || '{}'),
        };
    }
    list(options) {
        let query = 'SELECT * FROM sessions';
        const params = [];
        if (options?.status) {
            query += ' WHERE status = ?';
            params.push(options.status);
        }
        query += ' ORDER BY updated_at DESC';
        if (options?.limit) {
            query += ' LIMIT ?';
            params.push(options.limit);
            if (options.offset) {
                query += ' OFFSET ?';
                params.push(options.offset);
            }
        }
        const rows = this.db.prepare(query).all(...params);
        return rows.map((row) => {
            const msgCount = this.db
                .prepare('SELECT COUNT(*) as c FROM messages WHERE session_id = ?')
                .get(row.id).c;
            return {
                id: row.id,
                title: row.title,
                createdAt: row.created_at,
                updatedAt: row.updated_at,
                messageCount: msgCount,
                model: row.model,
                status: row.status,
                tags: JSON.parse(row.tags || '[]'),
                metadata: JSON.parse(row.metadata || '{}'),
            };
        });
    }
    update(id, updates) {
        const sets = [];
        const params = [];
        if (updates.title !== undefined) {
            sets.push('title = ?');
            params.push(updates.title);
        }
        if (updates.status !== undefined) {
            sets.push('status = ?');
            params.push(updates.status);
        }
        if (updates.tags !== undefined) {
            sets.push('tags = ?');
            params.push(JSON.stringify(updates.tags));
        }
        if (sets.length === 0)
            return false;
        sets.push('updated_at = ?');
        params.push(Date.now());
        params.push(id);
        const result = this.db
            .prepare(`UPDATE sessions SET ${sets.join(', ')} WHERE id = ?`)
            .run(...params);
        return result.changes > 0;
    }
    archive(id) {
        return this.update(id, { status: 'archived' });
    }
    delete(id) {
        this.db.prepare('DELETE FROM messages WHERE session_id = ?').run(id);
        this.db.prepare('DELETE FROM session_state WHERE session_id = ?').run(id);
        const result = this.db.prepare('DELETE FROM sessions WHERE id = ?').run(id);
        return result.changes > 0;
    }
    // ─── Messages ───────────────────────────────────────────────────────────
    addMessage(sessionId, role, content, toolCalls) {
        const id = crypto.randomUUID();
        const now = Date.now();
        this.db
            .prepare('INSERT INTO messages (id, session_id, role, content, timestamp, tool_calls) VALUES (?, ?, ?, ?, ?, ?)')
            .run(id, sessionId, role, content, now, toolCalls ? JSON.stringify(toolCalls) : null);
        // Update session timestamp
        this.db.prepare('UPDATE sessions SET updated_at = ? WHERE id = ?').run(now, sessionId);
        return {
            id,
            sessionId,
            role: role,
            content,
            timestamp: now,
            toolCalls,
        };
    }
    getMessages(sessionId, options) {
        let query = 'SELECT * FROM messages WHERE session_id = ?';
        const params = [sessionId];
        if (options?.role) {
            query += ' AND role = ?';
            params.push(options.role);
        }
        query += ' ORDER BY timestamp ASC';
        if (options?.limit) {
            query += ' LIMIT ? OFFSET ?';
            params.push(options.limit, options.offset || 0);
        }
        const rows = this.db.prepare(query).all(...params);
        return rows.map((row) => ({
            id: row.id,
            sessionId: row.session_id,
            role: row.role,
            content: row.content,
            timestamp: row.timestamp,
            toolCalls: row.tool_calls ? JSON.parse(row.tool_calls) : undefined,
            metadata: JSON.parse(row.metadata || '{}'),
        }));
    }
    search(query, limit = 20) {
        const rows = this.db
            .prepare('SELECT m.id as message_id, m.session_id, m.content, m.role, m.timestamp FROM messages m WHERE m.content LIKE ? ORDER BY m.timestamp DESC LIMIT ?')
            .all(`%${query}%`, limit);
        return rows.map((row) => ({
            sessionId: row.session_id,
            messageId: row.message_id,
            content: row.content.slice(0, 200),
            role: row.role,
            timestamp: row.timestamp,
        }));
    }
    // ─── Session State ──────────────────────────────────────────────────────
    setState(sessionId, key, value) {
        this.db
            .prepare('INSERT OR REPLACE INTO session_state (session_id, key, value, updated_at) VALUES (?, ?, ?, ?)')
            .run(sessionId, key, value, Date.now());
    }
    getState(sessionId, key) {
        const row = this.db
            .prepare('SELECT value FROM session_state WHERE session_id = ? AND key = ?')
            .get(sessionId, key);
        return row?.value ?? null;
    }
    getAllState(sessionId) {
        const rows = this.db
            .prepare('SELECT key, value FROM session_state WHERE session_id = ?')
            .all(sessionId);
        return Object.fromEntries(rows.map((r) => [r.key, r.value]));
    }
    // ─── Export/Import ──────────────────────────────────────────────────────
    export(sessionId) {
        const session = this.get(sessionId);
        if (!session)
            return null;
        return {
            session,
            messages: this.getMessages(sessionId),
            state: this.getAllState(sessionId),
        };
    }
    // ─── Stats ──────────────────────────────────────────────────────────────
    getStats() {
        const totalSessions = this.db.prepare('SELECT COUNT(*) as c FROM sessions').get().c;
        const activeSessions = this.db
            .prepare("SELECT COUNT(*) as c FROM sessions WHERE status = 'active'")
            .get().c;
        const totalMessages = this.db.prepare('SELECT COUNT(*) as c FROM messages').get().c;
        return {
            totalSessions,
            activeSessions,
            totalMessages,
            avgMessagesPerSession: totalSessions > 0 ? Math.round(totalMessages / totalSessions) : 0,
        };
    }
}
exports.default = new SessionsFeature();
//# sourceMappingURL=index.js.map