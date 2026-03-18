import Database from 'better-sqlite3';
import path from 'path';

class RateLimitingFeature {
  db: Database.Database;

  constructor() {
    const dbPath = process.env.AGCLAW_DB_PATH || path.join(process.cwd(), 'data', 'agclaw.db');
    this.db = new Database(dbPath);
    this.init();
  }

  init() {
    this.db.prepare(`
      CREATE TABLE IF NOT EXISTS rate_windows (
        key TEXT NOT NULL,
        timestamp INTEGER NOT NULL
      );
    `).run();
  }

  start() {}
  stop() { this.db.close(); }
  healthCheck() { return { ok: true }; }

  // Sliding window: store timestamps and count how many within window
  check(key: string, limit: number, windowMs: number) {
    const now = Date.now();
    const cutoff = now - windowMs;
    const insert = this.db.prepare('INSERT INTO rate_windows (key, timestamp) VALUES (?, ?)');
    insert.run(key, now);
    const count = this.db.prepare('SELECT COUNT(*) as c FROM rate_windows WHERE key = ? AND timestamp >= ?').get(key, cutoff).c;
    return { allowed: count <= limit, count };
  }

  reset(key: string) {
    const info = this.db.prepare('DELETE FROM rate_windows WHERE key = ?').run(key);
    return { changes: info.changes };
  }

  getStats(key: string) {
    const rows = this.db.prepare('SELECT timestamp FROM rate_windows WHERE key = ? ORDER BY timestamp DESC LIMIT 1000').all(key);
    return rows.map((r: any) => r.timestamp);
  }
}

export default new RateLimitingFeature();
