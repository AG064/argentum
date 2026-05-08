import { mkdirSync } from 'fs';
import path from 'path';

import Database from 'better-sqlite3';

type BindValue = string | number | bigint | Buffer | null;
type JsonRecord = Record<string, unknown>;

class AuditLogFeature {
  db: Database.Database;

  constructor() {
    const dataDir = path.join(process.cwd(), 'data');
    mkdirSync(dataDir, { recursive: true });
    const dbPath = process.env.AGCLAW_DB_PATH ?? path.join(dataDir, 'agclaw.db');
    this.db = new Database(dbPath);
    this.init();
  }

  init() {
    this.db
      .prepare(
        `
      CREATE TABLE IF NOT EXISTS audit_log (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        timestamp INTEGER NOT NULL,
        action TEXT NOT NULL,
        actor TEXT,
        details TEXT,
        ip TEXT,
        immutable INTEGER NOT NULL DEFAULT 1
      );
    `,
      )
      .run();

    this.db
      .prepare(
        `
      CREATE TABLE IF NOT EXISTS tool_calls (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        timestamp INTEGER NOT NULL,
        actor TEXT,
        tool TEXT NOT NULL,
        input TEXT,
        output TEXT,
        success INTEGER,
        meta TEXT
      );
    `,
      )
      .run();

    this.db
      .prepare(
        `
      CREATE TABLE IF NOT EXISTS decisions (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        timestamp INTEGER NOT NULL,
        actor TEXT,
        decision TEXT NOT NULL,
        reason TEXT,
        meta TEXT
      );
    `,
      )
      .run();

    this.db.prepare('CREATE INDEX IF NOT EXISTS idx_audit_timestamp ON audit_log(timestamp)').run();
    this.db.prepare('CREATE INDEX IF NOT EXISTS idx_tool_calls_tool ON tool_calls(tool)').run();
    this.db
      .prepare('CREATE INDEX IF NOT EXISTS idx_decisions_timestamp ON decisions(timestamp)')
      .run();
  }

  start() {}
  stop() {
    this.db.close();
  }
  healthCheck() {
    return { ok: true };
  }

  log(action: string, details: JsonRecord | string, actor?: string, ip?: string) {
    const t = Date.now();
    const stmt = this.db.prepare(
      'INSERT INTO audit_log (timestamp, action, actor, details, ip, immutable) VALUES (?, ?, ?, ?, ?, 1)',
    );
    stmt.run(
      t,
      action,
      actor ?? null,
      typeof details === 'string' ? details : JSON.stringify(details),
      ip ?? null,
    );
    return { timestamp: t };
  }

  logToolCall(
    tool: string,
    input: unknown,
    output: unknown,
    actor?: string,
    success: boolean = true,
    meta?: JsonRecord,
  ) {
    const t = Date.now();
    this.db
      .prepare(
        'INSERT INTO tool_calls (timestamp, actor, tool, input, output, success, meta) VALUES (?, ?, ?, ?, ?, ?, ?)',
      )
      .run(
        t,
        actor ?? null,
        tool,
        JSON.stringify(input ?? null),
        JSON.stringify(output ?? null),
        success ? 1 : 0,
        meta ? JSON.stringify(meta) : null,
      );
    // Also add a summary into audit_log for quick searches
    this.log('tool_call', { tool, success, meta }, actor);
    return { timestamp: t };
  }

  logDecision(decision: string, reason: string, actor?: string, meta?: JsonRecord) {
    const t = Date.now();
    this.db
      .prepare(
        'INSERT INTO decisions (timestamp, actor, decision, reason, meta) VALUES (?, ?, ?, ?, ?)',
      )
      .run(t, actor ?? null, decision, reason, meta ? JSON.stringify(meta) : null);
    this.log('decision', { decision, reason, meta }, actor);
    return { timestamp: t };
  }

  query(filters: { action?: string; actor?: string; since?: number; until?: number } = {}) {
    let sql = 'SELECT * FROM audit_log WHERE 1=1';
    const params: BindValue[] = [];
    if (filters.action) {
      sql += ' AND action = ?';
      params.push(filters.action);
    }
    if (filters.actor) {
      sql += ' AND actor = ?';
      params.push(filters.actor);
    }
    if (filters.since) {
      sql += ' AND timestamp >= ?';
      params.push(filters.since);
    }
    if (filters.until) {
      sql += ' AND timestamp <= ?';
      params.push(filters.until);
    }
    sql += ' ORDER BY timestamp DESC';
    return this.db.prepare(sql).all(...params);
  }

  export(start?: number, end?: number) {
    const rows = this.db
      .prepare(
        'SELECT * FROM audit_log WHERE timestamp >= COALESCE(?, 0) AND timestamp <= COALESCE(?, 9223372036854775807) ORDER BY timestamp ASC',
      )
      .all(start ?? 0, end ?? Number.MAX_SAFE_INTEGER);
    return JSON.stringify(rows, null, 2);
  }
}

export default new AuditLogFeature();
