"use strict";
/**
 * News Digest Feature
 *
 * RSS/Atom feed aggregation with caching. Supports adding/removing sources
 * and generating a digest of recent articles.
 */
var __importDefault = (this && this.__importDefault) || function (mod) {
    return (mod && mod.__esModule) ? mod : { "default": mod };
};
Object.defineProperty(exports, "__esModule", { value: true });
const path_1 = __importDefault(require("path"));
const better_sqlite3_1 = __importDefault(require("better-sqlite3"));
/**
 * News Digest feature — RSS/Atom feed aggregation.
 *
 * Fetches and caches articles from configured sources, provides digests
 * with latest articles sorted by publication date.
 */
class NewsDigestFeature {
    meta = {
        name: 'news-digest',
        version: '0.0.4',
        description: 'RSS/Atom feed aggregation with caching',
        dependencies: [],
    };
    config = {
        enabled: false,
        dbPath: path_1.default.join(process.cwd(), 'data', 'news.db'),
        cacheMinutes: 15,
        userAgent: 'Argentum/0.1 (RSS Reader)',
        maxArticlesPerSource: 50,
    };
    ctx;
    db;
    _active = false;
    async init(config, context) {
        this.ctx = context;
        this.config = { ...this.config, ...config };
    }
    async start() {
        this.initDb();
        this._active = true;
        this.ctx.logger.info('News Digest started', {
            cacheMinutes: this.config.cacheMinutes,
            maxArticles: this.config.maxArticlesPerSource,
        });
    }
    async stop() {
        this._active = false;
        this.db?.close();
        this.ctx.logger.info('News Digest stopped');
    }
    async healthCheck() {
        try {
            const sources = this.db.prepare('SELECT COUNT(*) as c FROM news_sources').get().c;
            const articles = this.db.prepare('SELECT COUNT(*) as c FROM articles').get().c;
            return {
                healthy: true,
                message: 'News Digest OK',
                details: { sources, articles },
            };
        }
        catch (err) {
            return { healthy: false, message: 'News Digest error' };
        }
    }
    initDb() {
        const dbDir = path_1.default.dirname(this.config.dbPath);
        try {
            const { mkdirSync, existsSync } = require('fs');
            if (!existsSync(dbDir)) {
                mkdirSync(dbDir, { recursive: true });
            }
        }
        catch { }
        this.db = new better_sqlite3_1.default(this.config.dbPath);
        this.db.pragma('journal_mode = WAL');
        this.db.exec(`
      CREATE TABLE IF NOT EXISTS news_sources (
        id TEXT PRIMARY KEY,
        url TEXT NOT NULL UNIQUE,
        name TEXT,
        added_at INTEGER NOT NULL,
        last_fetched INTEGER,
        last_article_count INTEGER DEFAULT 0
      );
    `);
        this.db.exec(`
      CREATE TABLE IF NOT EXISTS articles (
        id TEXT PRIMARY KEY,
        source_id TEXT NOT NULL,
        link TEXT NOT NULL,
        title TEXT NOT NULL,
        description TEXT,
        content TEXT,
        published_at INTEGER,
        fetched_at INTEGER NOT NULL,
        FOREIGN KEY(source_id) REFERENCES news_sources(id) ON DELETE CASCADE,
        UNIQUE(source_id, link)
      );
    `);
        this.db.exec(`
      CREATE INDEX IF NOT EXISTS idx_articles_pub ON articles(published_at DESC);
      CREATE INDEX IF NOT EXISTS idx_articles_source ON articles(source_id);
    `);
    }
    /** Add a new feed source */
    addSource(url, name) {
        const id = `src_${Date.now()}_${Math.random().toString(36).slice(2, 8)}`;
        const now = Date.now();
        try {
            this.db
                .prepare(`
        INSERT INTO news_sources (id, url, name, added_at) VALUES (?, ?, ?, ?)
      `)
                .run(id, url, name ?? null, now);
            this.ctx.logger.info('News source added', { id, url, name });
        }
        catch (err) {
            if (err.message?.includes('UNIQUE constraint')) {
                throw new Error(`Source with URL already exists: ${url}`);
            }
            throw err;
        }
        return id;
    }
    /** Remove a feed source and its articles */
    removeSource(id) {
        const info = this.db.prepare('DELETE FROM news_sources WHERE id = ?').run(id);
        return info.changes > 0;
    }
    /** Get all sources */
    getSources() {
        const rows = this.db
            .prepare('SELECT * FROM news_sources ORDER BY added_at DESC')
            .all();
        return rows.map((row) => ({
            id: row.id,
            url: row.url,
            name: row.name,
            addedAt: row.added_at,
            lastFetched: row.last_fetched,
            lastArticleCount: row.last_article_count,
        }));
    }
    /** Get a digest of recent articles */
    async getDigest(sources, limit = 20) {
        const sourceIds = sources ?? this.getSources().map((s) => s.id);
        if (sourceIds.length === 0) {
            return [];
        }
        // Fetch fresh articles for sources that are stale
        await this.fetchSourcesIfNeeded(sourceIds);
        // Build query
        const placeholders = sourceIds.map(() => '?').join(',');
        const stmt = this.db.prepare(`
      SELECT a.*, s.name as source_name FROM articles a
      JOIN news_sources s ON a.source_id = s.id
      WHERE a.source_id IN (${placeholders})
      ORDER BY a.published_at DESC NULLS LAST
      LIMIT ?
    `);
        const rows = stmt.all(...sourceIds, limit);
        return rows.map((row) => ({
            id: row.id,
            sourceId: row.source_id,
            sourceName: row.source_name,
            title: row.title,
            link: row.link,
            description: row.description,
            content: row.content,
            publishedAt: row.published_at,
            fetchedAt: row.fetched_at,
        }));
    }
    /** Check if a source needs fetching */
    needsFetch(sourceId) {
        const source = this.db
            .prepare('SELECT last_fetched FROM news_sources WHERE id = ?')
            .get(sourceId);
        if (!source?.last_fetched)
            return true;
        const age = Date.now() - source.last_fetched;
        return age > this.config.cacheMinutes * 60 * 1000;
    }
    async fetchSourcesIfNeeded(sourceIds) {
        const toFetch = sourceIds.filter((id) => this.needsFetch(id));
        if (toFetch.length === 0) {
            return;
        }
        this.ctx.logger.debug('Fetching stale sources', { count: toFetch.length });
        for (const sourceId of toFetch) {
            try {
                const source = this.db
                    .prepare('SELECT url FROM news_sources WHERE id = ?')
                    .get(sourceId);
                if (!source)
                    continue;
                await this.fetchSource(sourceId, source.url);
            }
            catch (err) {
                this.ctx.logger.error('Source fetch failed', {
                    sourceId,
                    error: err instanceof Error ? err.message : String(err),
                });
            }
        }
    }
    /** Fetch and parse a single source (RSS or Atom) */
    async fetchSource(sourceId, url) {
        this.ctx.logger.debug('Fetching feed', { sourceId, url });
        const response = await fetch(url, {
            headers: { 'User-Agent': this.config.userAgent },
        });
        if (!response.ok) {
            throw new Error(`HTTP ${response.status}: ${response.statusText}`);
        }
        const contentType = response.headers.get('content-type') ?? '';
        const body = await response.text();
        const articles = contentType.includes('atom') ? this.parseAtom(body) : this.parseRSS(body);
        const now = Date.now();
        const insert = this.db.prepare(`
      INSERT OR IGNORE INTO articles (id, source_id, link, title, description, content, published_at, fetched_at)
      VALUES (?, ?, ?, ?, ?, ?, ?, ?)
    `);
        let count = 0;
        for (const article of articles.slice(0, this.config.maxArticlesPerSource)) {
            const articleId = `art_${Date.now()}_${Math.random().toString(36).slice(2, 8)}`;
            insert.run(articleId, sourceId, article.link, article.title, article.description ?? null, article.content ?? null, article.publishedAt, now);
            count++;
        }
        // Update source last_fetched and count
        this.db
            .prepare(`
      UPDATE news_sources SET last_fetched = ?, last_article_count = ? WHERE id = ?
    `)
            .run(now, count, sourceId);
        this.ctx.logger.debug('Source fetch complete', { sourceId, articlesAdded: count });
    }
    /** Parse RSS 2.0 feed */
    parseRSS(xml) {
        const items = [];
        // Simple regex extraction for <item> blocks
        const itemRegex = /<item[^>]*>([\s\S]*?)<\/item>/gi;
        let match;
        while ((match = itemRegex.exec(xml)) !== null) {
            const itemXml = match[1];
            const getTag = (tag) => {
                const re = new RegExp(`<${tag}[^>]*>([\\s\\S]*?)<\\/${tag}>`, 'i');
                const m = re.exec(itemXml ?? '');
                return (m?.[1] ?? null)?.trim() ?? null;
            };
            const linkMatch = /<link[^>]*>([\s\S]*?)<\/link>/i.exec(itemXml ?? '');
            const link = (linkMatch?.[1] ?? null)?.trim() ?? null;
            const title = getTag('title');
            const description = getTag('description') ?? undefined;
            const content = (getTag('content:encoded') || getTag('content')) ?? undefined;
            // Parse pubDate
            const pubDateStr = getTag('pubDate');
            const publishedAt = pubDateStr ? new Date(pubDateStr).getTime() : Date.now();
            if (title && link) {
                items.push({
                    title,
                    link,
                    description: description ?? undefined,
                    content: content ?? undefined,
                    publishedAt,
                });
            }
        }
        return items;
    }
    /** Parse Atom 1.0 feed */
    parseAtom(xml) {
        const entries = [];
        const entryRegex = /<entry[^>]*>([\s\S]*?)<\/entry>/gi;
        let match;
        while ((match = entryRegex.exec(xml)) !== null) {
            const entryXml = match[1];
            const getTag = (tag) => {
                const re = new RegExp(`<${tag}[^>]*>([\\s\\S]*?)<\\/${tag}>`, 'i');
                const m = re.exec(entryXml ?? '');
                return (m?.[1] ?? null)?.trim() ?? null;
            };
            // For Atom, <link rel="alternate" href="..."/>
            const linkMatch = /<link[^>]*rel="alternate"[^>]*href="([^"]+)"[^>]*>/i.exec(entryXml ?? '');
            const link = linkMatch ? (linkMatch[1] ?? null) : null;
            const title = getTag('title');
            const summary = getTag('summary') ?? undefined;
            const content = getTag('content') ?? undefined;
            // Parse updated or published
            const updatedStr = getTag('updated') || getTag('published');
            const publishedAt = updatedStr ? new Date(updatedStr).getTime() : Date.now();
            if (title && link) {
                entries.push({
                    title,
                    link,
                    description: summary,
                    content,
                    publishedAt,
                });
            }
        }
        return entries;
    }
    /** Force refresh all sources */
    async refreshAll() {
        const sources = this.getSources();
        for (const source of sources) {
            try {
                await this.fetchSource(source.id, source.url);
            }
            catch (err) {
                this.ctx.logger.error('Refresh failed', { sourceId: source.id, url: source.url });
            }
        }
    }
    /** Get source by ID */
    getSource(id) {
        const row = this.db.prepare('SELECT * FROM news_sources WHERE id = ?').get(id);
        if (!row)
            return null;
        return {
            id: row.id,
            url: row.url,
            name: row.name,
            addedAt: row.added_at,
            lastFetched: row.last_fetched,
            lastArticleCount: row.last_article_count,
        };
    }
}
exports.default = new NewsDigestFeature();
//# sourceMappingURL=index.js.map