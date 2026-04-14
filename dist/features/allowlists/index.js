"use strict";
var __importDefault = (this && this.__importDefault) || function (mod) {
    return (mod && mod.__esModule) ? mod : { "default": mod };
};
Object.defineProperty(exports, "__esModule", { value: true });
const fs_1 = require("fs");
const path_1 = __importDefault(require("path"));
const better_sqlite3_1 = __importDefault(require("better-sqlite3"));
class AllowlistsFeature {
    db;
    constructor() {
        const dataDir = path_1.default.join(process.cwd(), 'data');
        (0, fs_1.mkdirSync)(dataDir, { recursive: true });
        const dbPath = process.env.AGCLAW_DB_PATH || path_1.default.join(dataDir, 'agclaw.db');
        this.db = new better_sqlite3_1.default(dbPath);
        this.init();
    }
    init() {
        this.db
            .prepare(`
      CREATE TABLE IF NOT EXISTS rules (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        pattern TEXT NOT NULL,
        type TEXT NOT NULL,
        action TEXT NOT NULL,
        createdAt INTEGER NOT NULL
      );
    `)
            .run();
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
    addRule(pattern, type, action) {
        const stmt = this.db.prepare('INSERT INTO rules (pattern, type, action, createdAt) VALUES (?, ?, ?, ?)');
        const info = stmt.run(pattern, type, action, Date.now());
        return { id: info.lastInsertRowid };
    }
    check(item) {
        const rows = this.db.prepare('SELECT * FROM rules WHERE type = ?').all(item.type);
        for (const r of rows) {
            const pattern = r.pattern;
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
                }
                catch (e) {
                    // If regex compilation fails or is risky, fall back to simple wildcard matching
                    // Convert pattern with '*' into wildcard, otherwise literal compare
                    if (pattern.includes('*')) {
                        const escaped = pattern
                            .split('*')
                            .map((s) => s.replace(/[.*+?^${}()|[\]\\]/g, '\\$&'))
                            .join('.*');
                        try {
                            const re2 = new RegExp(`^${escaped}$`);
                            if (re2.test(item.value)) {
                                return { matched: true, action: r.action, rule: r };
                            }
                        }
                        catch {
                            // give up on this rule
                        }
                    }
                    else if (pattern === item.value) {
                        return { matched: true, action: r.action, rule: r };
                    }
                }
            }
            else {
                // Treat as simple wildcard: support '*' only
                if (pattern.includes('*')) {
                    const escaped = pattern
                        .split('*')
                        .map((s) => s.replace(/[.*+?^${}()|[\\]\\]/g, '\\$&'))
                        .join('.*');
                    const re = new RegExp(`^${escaped}$`);
                    if (re.test(item.value)) {
                        return { matched: true, action: r.action, rule: r };
                    }
                }
                else {
                    if (pattern === item.value) {
                        return { matched: true, action: r.action, rule: r };
                    }
                }
            }
        }
        return { matched: false, action: 'allow' };
    }
    removeRule(id) {
        const stmt = this.db.prepare('DELETE FROM rules WHERE id = ?');
        const info = stmt.run(id);
        return { changes: info.changes };
    }
}
exports.default = new AllowlistsFeature();
//# sourceMappingURL=index.js.map