/**
 * Argentum Sessions
 *
 * Session management for conversations and agent interactions.
 * Stores messages, tracks context, manages history.
 */

import * as crypto from 'crypto';
import * as fs from 'fs';
import * as path from 'path';

import Database from 'better-sqlite3';

import { type FeatureContext } from '../../core/plugin-loader';

interface Session {
  id: string;
  title: string;
  createdAt: number;
  updatedAt: number;
  messageCount: number;
  model: string;
  status: 'active' | 'archived' | 'deleted';
  tags: string[];
  metadata: Record<string, unknown>;
}

interface Message {
  id: string;
  sessionId: string;
  role: 'user' | 'assistant' | 'system' | 'tool';
  content: string;
  timestamp: number;
  toolCalls?: Array<{ name: string; arguments: string; result?: string }>;
  metadata?: Record<string, unknown>;
}

type BindValue = string | number | bigint | Buffer | null;

interface CountRow {
  c: number;
}

interface SessionRow {
  id: string;
  title: string;
  created_at: number;
  updated_at: number;
  model: string;
  status: Session['status'];
  tags: string | null;
  metadata: string | null;
}

interface MessageRow {
  id: string;
  session_id: string;
  role: Message['role'];
  content: string;
  timestamp: number;
  tool_calls: string | null;
  metadata: string | null;
}

interface SearchRow {
  message_id: string;
  session_id: string;
  content: string;
  role: string;
  timestamp: number;
}

interface StateRow {
  key: string;
  value: string;
}

interface ValueRow {
  value: string;
}

class SessionsFeature {
  readonly meta = {
    name: 'sessions',
    version: '0.0.5',
    description: 'Session management for conversations and agent interactions',
    dependencies: ['sqlite-memory'],
  };

  private _ctx: FeatureContext | null = null;
  private db: Database.Database | null = null;

  async init(config: Record<string, unknown>, context: FeatureContext): Promise<void> {
    this._ctx = context;

    const configuredDbPath = config['dbPath'];
    const dbPath = typeof configuredDbPath === 'string' ? configuredDbPath : './data/sessions.db';
    const dir = path.dirname(dbPath);
    if (!fs.existsSync(dir)) fs.mkdirSync(dir, { recursive: true });

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

  async start(): Promise<void> {}

  async stop(): Promise<void> {
    if (this.db) this.db.close();
  }

  async healthCheck(): Promise<{ healthy: boolean; details: Record<string, unknown> }> {
    try {
      const sessionCount =
        this.database.prepare<[], CountRow>('SELECT COUNT(*) as c FROM sessions').get()?.c ?? 0;
      const messageCount =
        this.database.prepare<[], CountRow>('SELECT COUNT(*) as c FROM messages').get()?.c ?? 0;
      return {
        healthy: true,
        details: { sessions: sessionCount, messages: messageCount },
      };
    } catch {
      return { healthy: false, details: { error: 'Database error' } };
    }
  }

  create(title?: string, model?: string): Session {
    const id = crypto.randomUUID();
    const now = Date.now();
    const session: Session = {
      id,
      title: title ?? 'New Session',
      createdAt: now,
      updatedAt: now,
      messageCount: 0,
      model: model ?? '',
      status: 'active',
      tags: [],
      metadata: {},
    };
    this.database
      .prepare<[string, string, number, number, string, Session['status']]>(
        'INSERT INTO sessions (id, title, created_at, updated_at, model, status) VALUES (?, ?, ?, ?, ?, ?)',
      )
      .run(id, session.title, now, now, session.model, 'active');
    return session;
  }

  get(id: string): Session | null {
    const row = this.database
      .prepare<[string], SessionRow>('SELECT * FROM sessions WHERE id = ?')
      .get(id);
    if (!row) return null;

    const msgCount =
      this.database
        .prepare<[string], CountRow>('SELECT COUNT(*) as c FROM messages WHERE session_id = ?')
        .get(id)?.c ?? 0;
    return this.mapSessionRow(row, msgCount);
  }

  list(options?: { status?: string; limit?: number; offset?: number }): Session[] {
    let query = 'SELECT * FROM sessions';
    const params: BindValue[] = [];

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

    const rows = this.database.prepare<BindValue[], SessionRow>(query).all(...params);
    return rows.map((row) => {
      const msgCount =
        this.database
          .prepare<[string], CountRow>('SELECT COUNT(*) as c FROM messages WHERE session_id = ?')
          .get(row.id)?.c ?? 0;
      return this.mapSessionRow(row, msgCount);
    });
  }

  update(id: string, updates: Partial<Pick<Session, 'title' | 'status' | 'tags'>>): boolean {
    const sets: string[] = [];
    const params: BindValue[] = [];

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

    if (sets.length === 0) return false;

    sets.push('updated_at = ?');
    params.push(Date.now());
    params.push(id);

    const result = this.database
      .prepare<BindValue[]>(`UPDATE sessions SET ${sets.join(', ')} WHERE id = ?`)
      .run(...params);
    return result.changes > 0;
  }

  archive(id: string): boolean {
    return this.update(id, { status: 'archived' });
  }

  delete(id: string): boolean {
    this.database.prepare<[string]>('DELETE FROM messages WHERE session_id = ?').run(id);
    this.database.prepare<[string]>('DELETE FROM session_state WHERE session_id = ?').run(id);
    const result = this.database.prepare<[string]>('DELETE FROM sessions WHERE id = ?').run(id);
    return result.changes > 0;
  }

  addMessage(
    sessionId: string,
    role: Message['role'],
    content: string,
    toolCalls?: Message['toolCalls'],
  ): Message {
    const id = crypto.randomUUID();
    const now = Date.now();

    this.database
      .prepare<[string, string, Message['role'], string, number, string | null]>(
        'INSERT INTO messages (id, session_id, role, content, timestamp, tool_calls) VALUES (?, ?, ?, ?, ?, ?)',
      )
      .run(id, sessionId, role, content, now, toolCalls ? JSON.stringify(toolCalls) : null);

    this.database
      .prepare<[number, string]>('UPDATE sessions SET updated_at = ? WHERE id = ?')
      .run(now, sessionId);

    return {
      id,
      sessionId,
      role,
      content,
      timestamp: now,
      toolCalls,
    };
  }

  getMessages(
    sessionId: string,
    options?: { limit?: number; offset?: number; role?: Message['role'] },
  ): Message[] {
    let query = 'SELECT * FROM messages WHERE session_id = ?';
    const params: BindValue[] = [sessionId];

    if (options?.role) {
      query += ' AND role = ?';
      params.push(options.role);
    }

    query += ' ORDER BY timestamp ASC';

    if (options?.limit) {
      query += ' LIMIT ? OFFSET ?';
      params.push(options.limit, options.offset ?? 0);
    }

    const rows = this.database.prepare<BindValue[], MessageRow>(query).all(...params);
    return rows.map((row) => ({
      id: row.id,
      sessionId: row.session_id,
      role: row.role,
      content: row.content,
      timestamp: row.timestamp,
      toolCalls: row.tool_calls
        ? (JSON.parse(row.tool_calls) as Message['toolCalls'])
        : undefined,
      metadata: JSON.parse(row.metadata ?? '{}') as Record<string, unknown>,
    }));
  }

  search(
    query: string,
    limit = 20,
  ): Array<{
    sessionId: string;
    messageId: string;
    content: string;
    role: string;
    timestamp: number;
  }> {
    const rows = this.database
      .prepare<[string, number], SearchRow>(
        'SELECT m.id as message_id, m.session_id, m.content, m.role, m.timestamp FROM messages m WHERE m.content LIKE ? ORDER BY m.timestamp DESC LIMIT ?',
      )
      .all(`%${query}%`, limit);

    return rows.map((row) => ({
      sessionId: row.session_id,
      messageId: row.message_id,
      content: row.content.slice(0, 200),
      role: row.role,
      timestamp: row.timestamp,
    }));
  }

  setState(sessionId: string, key: string, value: string): void {
    this.database
      .prepare<[string, string, string, number]>(
        'INSERT OR REPLACE INTO session_state (session_id, key, value, updated_at) VALUES (?, ?, ?, ?)',
      )
      .run(sessionId, key, value, Date.now());
  }

  getState(sessionId: string, key: string): string | null {
    const row = this.database
      .prepare<[string, string], ValueRow>(
        'SELECT value FROM session_state WHERE session_id = ? AND key = ?',
      )
      .get(sessionId, key);
    return row?.value ?? null;
  }

  getAllState(sessionId: string): Record<string, string> {
    const rows = this.database
      .prepare<[string], StateRow>('SELECT key, value FROM session_state WHERE session_id = ?')
      .all(sessionId);
    return Object.fromEntries(rows.map((row) => [row.key, row.value]));
  }

  export(
    sessionId: string,
  ): { session: Session; messages: Message[]; state: Record<string, string> } | null {
    const session = this.get(sessionId);
    if (!session) return null;
    return {
      session,
      messages: this.getMessages(sessionId),
      state: this.getAllState(sessionId),
    };
  }

  getStats(): {
    totalSessions: number;
    activeSessions: number;
    totalMessages: number;
    avgMessagesPerSession: number;
  } {
    const totalSessions =
      this.database.prepare<[], CountRow>('SELECT COUNT(*) as c FROM sessions').get()?.c ?? 0;
    const activeSessions =
      this.database
        .prepare<[], CountRow>("SELECT COUNT(*) as c FROM sessions WHERE status = 'active'")
        .get()?.c ?? 0;
    const totalMessages =
      this.database.prepare<[], CountRow>('SELECT COUNT(*) as c FROM messages').get()?.c ?? 0;
    return {
      totalSessions,
      activeSessions,
      totalMessages,
      avgMessagesPerSession: totalSessions > 0 ? Math.round(totalMessages / totalSessions) : 0,
    };
  }

  private get database(): Database.Database {
    if (!this.db) {
      throw new Error('Sessions database is not initialized');
    }
    return this.db;
  }

  private mapSessionRow(row: SessionRow, messageCount: number): Session {
    return {
      id: row.id,
      title: row.title,
      createdAt: row.created_at,
      updatedAt: row.updated_at,
      messageCount,
      model: row.model,
      status: row.status,
      tags: JSON.parse(row.tags ?? '[]') as string[],
      metadata: JSON.parse(row.metadata ?? '{}') as Record<string, unknown>,
    };
  }
}

export default new SessionsFeature();
