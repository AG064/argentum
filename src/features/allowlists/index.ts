import Database from 'better-sqlite3';
import path from 'path';

class AllowlistsFeature {
  db: Database.Database;

  constructor() {
    const dbPath = process.env.AGCLAW_DB_PATH || path.join(process.cwd(), 'data', 'agclaw.db');
    this.db = new Database(dbPath);
    this.init();
  }

  init() {
    this.db.prepare(`
      CREATE TABLE IF NOT EXISTS rules (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        pattern TEXT NOT NULL,
        type TEXT NOT NULL,
        action TEXT NOT NULL,
        createdAt INTEGER NOT NULL
      );
    `).run();
  }

  start() {
    // nothing to start for now
  }

  stop() {
    this.db.close();
  }

  healthCheck() {
    return { ok: true };
  }

  addRule(pattern: string, type: 'url' | 'command' | 'user', action: 'allow' | 'deny') {
    const stmt = this.db.prepare('INSERT INTO rules (pattern, type, action, createdAt) VALUES (?, ?, ?, ?)');
    const info = stmt.run(pattern, type, action, Date.now());
    return { id: info.lastInsertRowid };
  }

  check(item: { value: string; type: 'url' | 'command' | 'user' }) {
    const rows = this.db.prepare('SELECT * FROM rules WHERE type = ?').all(item.type);
    for (const r of rows as any[]) {
      try {
        const re = new RegExp(r.pattern);
        if (re.test(item.value)) {
          return { matched: true, action: r.action, rule: r };
        }
      } catch (e) {
        // fallback to simple equality
        if (r.pattern === item.value) {
          return { matched: true, action: r.action, rule: r };
        }
      }
    }
    return { matched: false, action: 'allow' };
  }

  removeRule(id: number) {
    const stmt = this.db.prepare('DELETE FROM rules WHERE id = ?');
    const info = stmt.run(id);
    return { changes: info.changes };
  }
}

export default new AllowlistsFeature();
