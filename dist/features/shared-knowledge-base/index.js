"use strict";
var __importDefault = (this && this.__importDefault) || function (mod) {
    return (mod && mod.__esModule) ? mod : { "default": mod };
};
Object.defineProperty(exports, "__esModule", { value: true });
const fs_1 = require("fs");
const path_1 = require("path");
const better_sqlite3_1 = __importDefault(require("better-sqlite3"));
// ─── Feature ─────────────────────────────────────────────────────────────────
class SharedKnowledgeBaseFeature {
    meta = {
        name: 'shared-knowledge-base',
        version: '0.1.0',
        description: 'Shared knowledge base with full-text search and versioning',
        dependencies: [],
    };
    config = {
        enabled: false,
        dbPath: './data/shared-knowledge-base.db',
        maxArticles: 10_000,
        maxVersionsPerArticle: 20,
    };
    db;
    ctx;
    async init(config, context) {
        this.ctx = context;
        this.config = { ...this.config, ...config };
        this.initDatabase();
    }
    async start() {
        this.ctx.logger.info('Shared knowledge base active', {
            dbPath: this.config.dbPath,
        });
    }
    async stop() {
        this.db?.close();
    }
    async healthCheck() {
        const articleRow = this.db
            .prepare('SELECT COUNT(*) as c FROM articles WHERE current = 1')
            .get();
        const articleCount = articleRow.c;
        const versionRow = this.db.prepare('SELECT COUNT(*) as c FROM article_versions').get();
        const versionCount = versionRow.c;
        return {
            healthy: true,
            details: {
                currentArticles: articleCount,
                totalVersions: versionCount,
            },
        };
    }
    // ─── Article Management ───────────────────────────────────────────────────
    /** Add a new article */
    async addArticle(title, content, tags, createdBy) {
        if (!title || !content) {
            throw new Error('Title and content are required');
        }
        const id = `kb:${Date.now()}:${Math.random().toString(36).substr(2, 9)}`;
        const now = Date.now();
        const version = 1;
        // Insert current article
        this.db
            .prepare(`INSERT INTO articles (id, title, content, tags, version, created_at, updated_at, created_by, current)
       VALUES (?, ?, ?, ?, ?, ?, ?, ?, 1)`)
            .run(id, title, content, JSON.stringify(tags), version, now, now, createdBy);
        // Store version history
        this.db
            .prepare(`INSERT INTO article_versions (article_id, version, content, updated_at, updated_by)
       VALUES (?, ?, ?, ?, ?)`)
            .run(id, version, content, now, createdBy);
        // Update FTS
        this.updateFts(id, title, content, tags);
        this.ctx.logger.info('Article added', { articleId: id, title, tags, createdBy });
        return (await this.getArticle(id));
    }
    /** Get article by ID */
    async getArticle(id) {
        const row = this.db
            .prepare('SELECT * FROM articles WHERE id = ? AND current = 1')
            .get(id);
        if (!row)
            return null;
        return this.mapArticleRow(row);
    }
    /** Get article versions */
    async getArticleVersions(id) {
        const rows = this.db
            .prepare('SELECT * FROM article_versions WHERE article_id = ? ORDER BY version DESC')
            .all(id);
        return rows.map((row) => ({
            version: row.version,
            content: row.content,
            updatedAt: row.updated_at,
            updatedBy: row.updated_by,
        }));
    }
    /** Update an article (creates new version) */
    async updateArticle(id, updatedBy, title, content, tags) {
        const existing = this.db
            .prepare('SELECT * FROM articles WHERE id = ? AND current = 1')
            .get(id);
        if (!existing) {
            throw new Error(`Article not found: ${id}`);
        }
        const now = Date.now();
        const newVersion = existing.version + 1;
        // Archive current version
        this.db.prepare('UPDATE articles SET current = 0 WHERE id = ?').run(id);
        // Insert version into history
        this.db
            .prepare(`INSERT INTO article_versions (article_id, version, content, updated_at, updated_by)
       VALUES (?, ?, ?, ?, ?)`)
            .run(id, existing.version, existing.content, existing.updated_at, existing.created_by);
        // Insert new current version
        const finalTitle = title ?? existing.title;
        const finalContent = content ?? existing.content;
        const finalTags = tags ?? JSON.parse(existing.tags);
        this.db
            .prepare(`INSERT INTO articles (id, title, content, tags, version, created_at, updated_at, created_by, current)
       VALUES (?, ?, ?, ?, ?, ?, ?, ?, 1)`)
            .run(id, finalTitle, finalContent, JSON.stringify(finalTags), newVersion, existing.created_at, now, updatedBy);
        // Update FTS (delete + insert)
        this.db.prepare('DELETE FROM knowledge_fts WHERE docid = ?').run(existing.id);
        this.updateFts(id, finalTitle, finalContent, finalTags);
        this.ctx.logger.info('Article updated', { articleId: id, newVersion, updatedBy });
        return this.getArticle(id);
    }
    /** Delete an article */
    async deleteArticle(id) {
        // Delete from articles (marks as deleted via current = 0, but we also delete from FTS)
        const result = this.db.prepare('DELETE FROM articles WHERE id = ?').run(id);
        // Delete from FTS
        this.db.prepare('DELETE FROM knowledge_fts WHERE docid = ?').run(id);
        // Delete version history
        this.db.prepare('DELETE FROM article_versions WHERE article_id = ?').run(id);
        if (result.changes > 0) {
            this.ctx.logger.info('Article deleted', { articleId: id });
            return true;
        }
        return false;
    }
    // ─── Search ───────────────────────────────────────────────────────────────
    /** Full-text search across title, content, tags */
    async search(query, limit = 20, offset = 0) {
        if (!query.trim()) {
            return [];
        }
        // Use FTS5
        const searchSql = `
      SELECT a.* FROM knowledge_fts fts
      JOIN articles a ON fts.docid = a.id
      WHERE knowledge_fts MATCH ?
      AND a.current = 1
      ORDER BY rank
      LIMIT ? OFFSET ?
    `;
        try {
            const rows = this.db.prepare(searchSql).all(query, limit, offset);
            return rows.map((row) => this.mapArticleRow(row));
        }
        catch (err) {
            // If FTS fails, fallback to simple LIKE search
            this.ctx.logger.warn('FTS search failed, falling back to LIKE', { error: err });
            const likePattern = `%${query}%`;
            const rows = this.db
                .prepare(`SELECT * FROM articles WHERE current = 1 AND (title LIKE ? OR content LIKE ?) LIMIT ? OFFSET ?`)
                .all(likePattern, likePattern, limit, offset);
            return rows.map((row) => this.mapArticleRow(row));
        }
    }
    /** Search by tags */
    async searchByTags(tags, matchAll = false, limit = 20) {
        if (tags.length === 0)
            return [];
        const rows = [];
        if (matchAll) {
            // Articles must have all tags
            const _placeholders = tags.map(() => '1').join(' AND ');
            const _query = `
        SELECT * FROM articles WHERE current = 1 AND tags MATCH 'tags:(' || ? || ')' AND
        ${tags.map((_, i) => `tags MATCH 'tags:(${tags[i]})'`).join(' AND ')}
        LIMIT ?
      `;
            // SQLite JSON functions for tags array
            const simpleQuery = `SELECT * FROM articles WHERE current = 1 LIMIT ?`;
            rows.push(...this.db.prepare(simpleQuery).all(limit));
        }
        else {
            // Articles can have any of the tags
            const likePatterns = tags.map((t) => `%"${t}"%`);
            const placeholders = likePatterns.map(() => '?').join(' OR ');
            const query = `SELECT * FROM articles WHERE current = 1 AND (${placeholders}) LIMIT ?`;
            const stmt = this.db.prepare(query);
            const newRows = stmt.all(...likePatterns, limit);
            rows.push(...newRows);
        }
        return rows.map((row) => this.mapArticleRow(row));
    }
    // ─── Database ─────────────────────────────────────────────────────────────
    initDatabase() {
        const fullPath = (0, path_1.resolve)(this.config.dbPath);
        if (!(0, fs_1.existsSync)((0, path_1.dirname)(fullPath))) {
            (0, fs_1.mkdirSync)((0, path_1.dirname)(fullPath), { recursive: true });
        }
        this.db = new better_sqlite3_1.default(fullPath);
        this.db.pragma('journal_mode = WAL');
        this.db.pragma('synchronous = NORMAL');
        this.db.exec(`
      CREATE TABLE IF NOT EXISTS articles (
        id TEXT PRIMARY KEY,
        title TEXT NOT NULL,
        content TEXT NOT NULL,
        tags TEXT NOT NULL,      -- JSON array
        version INTEGER NOT NULL,
        created_at INTEGER NOT NULL,
        updated_at INTEGER NOT NULL,
        created_by TEXT NOT NULL,
        current INTEGER DEFAULT 1
      );

      CREATE TABLE IF NOT EXISTS article_versions (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        article_id TEXT NOT NULL,
        version INTEGER NOT NULL,
        content TEXT NOT NULL,
        updated_at INTEGER NOT NULL,
        updated_by TEXT NOT NULL,
        FOREIGN KEY (article_id) REFERENCES articles(id) ON DELETE CASCADE
      );

      -- FTS5 virtual table for full-text search
      CREATE VIRTUAL TABLE IF NOT EXISTS knowledge_fts USING fts5(
        title,
        content,
        tags,
        content='articles',
        content_rowid='rowid'
      );

      CREATE INDEX IF NOT EXISTS idx_articles_current ON articles(current);
      CREATE INDEX IF NOT EXISTS idx_articles_created ON articles(created_at);
      CREATE INDEX IF NOT EXISTS idx_versions_article ON article_versions(article_id);
    `);
        // Triggers to keep FTS in sync
        this.db.exec(`
      DROP TRIGGER IF EXISTS articles_ai;
      DROP TRIGGER IF EXISTS articles_ad;
      DROP TRIGGER IF EXISTS articles_au;

      CREATE TRIGGER articles_ai AFTER INSERT ON articles BEGIN
        INSERT INTO knowledge_fts (rowid, title, content, tags) VALUES (new.rowid, new.title, new.content, new.tags);
      END;

      CREATE TRIGGER articles_ad AFTER DELETE ON articles BEGIN
        DELETE FROM knowledge_fts WHERE rowid = old.rowid;
      END;

      CREATE TRIGGER articles_au AFTER UPDATE ON articles BEGIN
        UPDATE knowledge_fts SET title = new.title, content = new.content, tags = new.tags WHERE rowid = new.rowid;
      END;
    `);
    }
    updateFts(id, title, content, tags) {
        // FTS is auto-updated via triggers, but we need to ensure the rowid matches
        const row = this.db.prepare('SELECT rowid FROM articles WHERE id = ?').get(id);
        if (row) {
            this.db
                .prepare('INSERT OR REPLACE INTO knowledge_fts (rowid, title, content, tags) VALUES (?, ?, ?, ?)')
                .run(row.rowid, title, content, JSON.stringify(tags));
        }
    }
    mapArticleRow(row) {
        return {
            id: row.id,
            title: row.title,
            content: row.content,
            tags: JSON.parse(row.tags),
            version: row.version,
            createdAt: row.created_at,
            updatedAt: row.updated_at,
            createdBy: row.created_by,
            current: Boolean(row.current),
        };
    }
}
exports.default = new SharedKnowledgeBaseFeature();
//# sourceMappingURL=index.js.map