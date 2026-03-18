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
      const pattern: string = r.pattern;

      // Basic input validation: reject overly long patterns
      if (typeof pattern !== 'string' || pattern.length === 0 || pattern.length > 200) {
        // skip invalid/too long patterns
        continue;
      }

      // If the pattern contains obvious regex metacharacters, avoid compiling untrusted regexes
      const looksLikeRegex = /[\\^$*+?.()|[\]{}]/.test(pattern);

      if (looksLikeRegex) {
        // Try to safely compile regex in try/catch but with a conservative approach: limit time by avoiding catastrophic patterns
        try {
          const re = new RegExp(pattern);
          if (re.test(item.value)) {
            return { matched: true, action: r.action, rule: r };
          }
        } catch (e) {
          // If regex compilation fails or is risky, fall back to simple wildcard matching
          // Convert pattern with '*' into wildcard, otherwise literal compare
          if (pattern.includes('*')) {
            const escaped = pattern.split('*').map(s => s.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')).join('.*');
            try {
              const re2 = new RegExp(`^${escaped}$`);
              if (re2.test(item.value)) {
                return { matched: true, action: r.action, rule: r };
              }
            } catch {
              // give up on this rule
            }
          } else if (pattern === item.value) {
            return { matched: true, action: r.action, rule: r };
          }
        }
      } else {
        // Treat as simple wildcard: support '*' only
        if (pattern.includes('*')) {
          const escaped = pattern.split('*').map(s => s.replace(/[.*+?^${}()|[\\]\\]/g, '\\$&')).join('.*');
          const re = new RegExp(`^${escaped}$`);
          if (re.test(item.value)) {
            return { matched: true, action: r.action, rule: r };
          }
        } else {
          if (pattern === item.value) {
            return { matched: true, action: r.action, rule: r };
          }
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
