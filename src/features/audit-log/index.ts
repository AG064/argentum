import Database from 'better-sqlite3';
import path from 'path';

class AuditLogFeature {
  db: Database.Database;

  constructor() {
    const dbPath = process.env.AGCLAW_DB_PATH || path.join(process.cwd(), 'data', 'agclaw.db');
    this.db = new Database(dbPath);
    this.init();
  }

  init() {
    this.db.prepare(`
      CREATE TABLE IF NOT EXISTS audit_log (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        timestamp INTEGER NOT NULL,
        action TEXT NOT NULL,
        actor TEXT,
        details TEXT,
        ip TEXT
      );
    `).run();
  }

  start() {}
  stop() { this.db.close(); }
  healthCheck() { return { ok: true }; }

  log(action: string, details: Record<string, any> | string, actor?: string, ip?: string) {
    const t = Date.now();
    const stmt = this.db.prepare('INSERT INTO audit_log (timestamp, action, actor, details, ip) VALUES (?, ?, ?, ?, ?)');
    stmt.run(t, action, actor || null, typeof details === 'string' ? details : JSON.stringify(details), ip || null);
    return { timestamp: t };
  }

  query(filters: { action?: string; actor?: string; since?: number; until?: number } = {}) {
    let sql = 'SELECT * FROM audit_log WHERE 1=1';
    const params: any[] = [];
    if (filters.action) { sql += ' AND action = ?'; params.push(filters.action); }
    if (filters.actor) { sql += ' AND actor = ?'; params.push(filters.actor); }
    if (filters.since) { sql += ' AND timestamp >= ?'; params.push(filters.since); }
    if (filters.until) { sql += ' AND timestamp <= ?'; params.push(filters.until); }
    sql += ' ORDER BY timestamp DESC';
    return this.db.prepare(sql).all(...params);
  }

  export(start?: number, end?: number) {
    const rows = this.db.prepare('SELECT * FROM audit_log WHERE timestamp >= COALESCE(?, 0) AND timestamp <= COALESCE(?, 9223372036854775807) ORDER BY timestamp ASC').all(start || 0, end || Number.MAX_SAFE_INTEGER);
    return JSON.stringify(rows, null, 2);
  }
}

export default new AuditLogFeature();
