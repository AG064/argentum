"use strict";
/**
 * Skills Library Feature (SQLite)
 *
 * Stores skill records with simple versioning using better-sqlite3.
 * Note: code is stored as text only and never executed by this module.
 */
var __importDefault = (this && this.__importDefault) || function (mod) {
    return (mod && mod.__esModule) ? mod : { "default": mod };
};
Object.defineProperty(exports, "__esModule", { value: true });
const crypto_1 = require("crypto");
const fs_1 = require("fs");
const path_1 = require("path");
const better_sqlite3_1 = __importDefault(require("better-sqlite3"));
const DEFAULT_CONFIG = {
    enabled: false,
    dbPath: './data/skills-library.db',
    storageDir: './data/skills-storage',
};
class SkillsLibraryFeature {
    meta = {
        name: 'skills-library',
        version: '0.0.2',
        description: 'Library of agent skills with versioning (SQLite)',
        dependencies: [],
    };
    config = { ...DEFAULT_CONFIG };
    ctx;
    db;
    async init(config, context) {
        this.ctx = context;
        this.config = { ...this.config, ...config };
        this.initDatabase();
    }
    async start() {
        /* nothing */
    }
    async stop() {
        this.db?.close();
    }
    async healthCheck() {
        const count = this.db.prepare('SELECT COUNT(*) as c FROM skills').get().c;
        return { healthy: true, details: { totalSkills: count } };
    }
    // ── Core API required by task ─────────────────────────────────────────
    registerSkill(payload) {
        const id = (0, crypto_1.randomUUID)();
        const now = Date.now();
        const version = payload.version ?? '1.0.0';
        this.db
            .prepare(`INSERT INTO skills (id, name, version, description, code, tags, author, created_at, updated_at)
       VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)`)
            .run(id, payload.name, version, payload.description ?? '', payload.code, JSON.stringify(payload.tags ?? []), payload.author ?? '', now, now);
        const rec = {
            id,
            name: payload.name,
            version,
            description: payload.description ?? '',
            code: payload.code,
            tags: payload.tags ?? [],
            author: payload.author,
            createdAt: now,
            updatedAt: now,
        };
        this.ctx.logger?.info('Skill registered', { id, name: rec.name, version: rec.version });
        return rec;
    }
    getSkill(id) {
        const row = this.db.prepare('SELECT * FROM skills WHERE id = ?').get(id);
        if (!row)
            return null;
        return this.rowToSkill(row);
    }
    listSkills() {
        const rows = this.db.prepare('SELECT * FROM skills ORDER BY updated_at DESC').all();
        return rows.map((r) => this.rowToSkill(r));
    }
    searchSkills(query) {
        const q = `%${query.toLowerCase()}%`;
        const rows = this.db
            .prepare(`SELECT * FROM skills WHERE LOWER(name) LIKE ? OR LOWER(description) LIKE ? OR LOWER(tags) LIKE ? ORDER BY updated_at DESC LIMIT 100`)
            .all(q, q, q);
        return rows.map((r) => this.rowToSkill(r));
    }
    updateSkill(id, patch) {
        const existing = this.getSkill(id);
        if (!existing)
            return null;
        const updated = { ...existing, ...patch, updatedAt: Date.now() };
        this.db
            .prepare(`UPDATE skills SET name = ?, version = ?, description = ?, code = ?, tags = ?, author = ?, updated_at = ? WHERE id = ?`)
            .run(updated.name, updated.version, updated.description, updated.code, JSON.stringify(updated.tags), updated.author ?? '', updated.updatedAt, id);
        this.ctx.logger?.info('Skill updated', { id });
        return updated;
    }
    removeSkill(id) {
        const res = this.db.prepare('DELETE FROM skills WHERE id = ?').run(id);
        return res.changes > 0;
    }
    // ── Private helpers ──────────────────────────────────────────────────
    initDatabase() {
        const full = (0, path_1.resolve)(this.config.dbPath);
        const dir = (0, path_1.dirname)(full);
        if (!(0, fs_1.existsSync)(dir))
            (0, fs_1.mkdirSync)(dir, { recursive: true });
        this.db = new better_sqlite3_1.default(full);
        this.db.pragma('journal_mode = WAL');
        this.db.pragma('synchronous = NORMAL');
        this.db.exec(`
      CREATE TABLE IF NOT EXISTS skills (
        id TEXT PRIMARY KEY,
        name TEXT NOT NULL,
        version TEXT NOT NULL,
        description TEXT DEFAULT '',
        code TEXT DEFAULT '',
        tags TEXT DEFAULT '[]',
        author TEXT DEFAULT '',
        created_at INTEGER NOT NULL,
        updated_at INTEGER NOT NULL
      );
      CREATE INDEX IF NOT EXISTS idx_skills_name ON skills(name);
      CREATE INDEX IF NOT EXISTS idx_skills_updated ON skills(updated_at DESC);
    `);
    }
    rowToSkill(row) {
        return {
            id: row.id,
            name: row.name,
            version: row.version,
            description: row.description,
            code: row.code,
            tags: this.safeParse(row.tags),
            author: row.author || undefined,
            createdAt: row.created_at,
            updatedAt: row.updated_at,
        };
    }
    safeParse(s) {
        try {
            return JSON.parse(s);
        }
        catch {
            return [];
        }
    }
}
exports.default = new SkillsLibraryFeature();
//# sourceMappingURL=index.js.map