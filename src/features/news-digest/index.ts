/**
 * News Digest Feature
 *
 * RSS/Atom feed aggregation with caching. Supports adding/removing sources
 * and generating a digest of recent articles.
 */

import path from 'path';

import Database from 'better-sqlite3';

import { type FeatureModule, type FeatureContext, type FeatureMeta, type HealthStatus } from '../../core/plugin-loader';

/** News digest configuration */
export interface NewsDigestConfig {
  enabled: boolean;
  dbPath: string;
  cacheMinutes: number;
  userAgent: string;
  maxArticlesPerSource: number;
}

/** News article */
export interface Article {
  id: string;
  sourceId: string;
  sourceName?: string;
  title: string;
  link: string;
  content?: string;
  description?: string;
  publishedAt: number;
  fetchedAt: number;
}

/** News source */
export interface NewsSource {
  id: string;
  url: string;
  name?: string;
  addedAt: number;
  lastFetched?: number;
  lastArticleCount: number;
}

/**
 * News Digest feature — RSS/Atom feed aggregation.
 *
 * Fetches and caches articles from configured sources, provides digests
 * with latest articles sorted by publication date.
 */
class NewsDigestFeature implements FeatureModule {
  readonly meta: FeatureMeta = {
    name: 'news-digest',
    version: '0.1.0',
    description: 'RSS/Atom feed aggregation with caching',
    dependencies: [],
  };

  private config: NewsDigestConfig = {
    enabled: false,
    dbPath: path.join(process.cwd(), 'data', 'news.db'),
    cacheMinutes: 15,
    userAgent: 'AG-Claw/0.1 (RSS Reader)',
    maxArticlesPerSource: 50,
  };
  private ctx!: FeatureContext;
  private db!: Database.Database;
  private _active = false;

  async init(config: Record<string, unknown>, context: FeatureContext): Promise<void> {
    this.ctx = context;
    this.config = { ...this.config, ...(config as Partial<NewsDigestConfig>) };
  }

  async start(): Promise<void> {
    this.initDb();
    this._active = true;
    this.ctx.logger.info('News Digest started', {
      cacheMinutes: this.config.cacheMinutes,
      maxArticles: this.config.maxArticlesPerSource,
    });
  }

  async stop(): Promise<void> {
    this._active = false;
    this.db?.close();
    this.ctx.logger.info('News Digest stopped');
  }

  async healthCheck(): Promise<HealthStatus> {
    try {
      const sources = (this.db.prepare('SELECT COUNT(*) as c FROM news_sources').get() as { c: number }).c;
      const articles = (this.db.prepare('SELECT COUNT(*) as c FROM articles').get() as { c: number }).c;
      return {
        healthy: true,
        message: 'News Digest OK',
        details: { sources, articles },
      };
    } catch (err) {
      return { healthy: false, message: 'News Digest error' };
    }
  }

  private initDb(): void {
    const dbDir = path.dirname(this.config.dbPath);
    try {
      const { mkdirSync, existsSync } = require('fs');
      if (!existsSync(dbDir)) {
        mkdirSync(dbDir, { recursive: true });
      }
    } catch {}

    this.db = new Database(this.config.dbPath);
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
  addSource(url: string, name?: string): string {
    const id = `src_${Date.now()}_${Math.random().toString(36).slice(2, 8)}`;
    const now = Date.now();

    try {
      this.db.prepare(`
        INSERT INTO news_sources (id, url, name, added_at) VALUES (?, ?, ?, ?)
      `).run(id, url, name ?? null, now);
      this.ctx.logger.info('News source added', { id, url, name });
    } catch (err: any) {
      if (err.message?.includes('UNIQUE constraint')) {
        throw new Error(`Source with URL already exists: ${url}`);
      }
      throw err;
    }

    return id;
  }

  /** Remove a feed source and its articles */
  removeSource(id: string): boolean {
    const info = this.db.prepare('DELETE FROM news_sources WHERE id = ?').run(id);
    return info.changes > 0;
  }

  /** Get all sources */
  getSources(): NewsSource[] {
    const rows = this.db.prepare('SELECT * FROM news_sources ORDER BY added_at DESC').all() as any[];
    return rows.map(row => ({
      id: row.id,
      url: row.url,
      name: row.name,
      addedAt: row.added_at,
      lastFetched: row.last_fetched,
      lastArticleCount: row.last_article_count,
    }));
  }

  /** Get a digest of recent articles */
  async getDigest(sources?: string[], limit: number = 20): Promise<Article[]> {
    const sourceIds = sources ?? this.getSources().map(s => s.id);
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

    const rows = stmt.all(...sourceIds, limit) as any[];
    return rows.map(row => ({
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
  private needsFetch(sourceId: string): boolean {
    const source = this.db.prepare('SELECT last_fetched FROM news_sources WHERE id = ?').get(sourceId) as any;
    if (!source?.last_fetched) return true;
    const age = Date.now() - source.last_fetched;
    return age > this.config.cacheMinutes * 60 * 1000;
  }

  private async fetchSourcesIfNeeded(sourceIds: string[]): Promise<void> {
    const toFetch = sourceIds.filter(id => this.needsFetch(id));
    if (toFetch.length === 0) {
      return;
    }

    this.ctx.logger.debug('Fetching stale sources', { count: toFetch.length });

    for (const sourceId of toFetch) {
      try {
        const source = this.db.prepare('SELECT url FROM news_sources WHERE id = ?').get(sourceId) as any;
        if (!source) continue;
        await this.fetchSource(sourceId, source.url);
      } catch (err) {
        this.ctx.logger.error('Source fetch failed', { sourceId, error: err instanceof Error ? err.message : String(err) });
      }
    }
  }

  /** Fetch and parse a single source (RSS or Atom) */
  private async fetchSource(sourceId: string, url: string): Promise<void> {
    this.ctx.logger.debug('Fetching feed', { sourceId, url });

    const response = await fetch(url, {
      headers: { 'User-Agent': this.config.userAgent },
    });

    if (!response.ok) {
      throw new Error(`HTTP ${response.status}: ${response.statusText}`);
    }

    const contentType = response.headers.get('content-type') ?? '';
    const body = await response.text();

    const articles = contentType.includes('atom')
      ? this.parseAtom(body)
      : this.parseRSS(body);

    const now = Date.now();
    const insert = this.db.prepare(`
      INSERT OR IGNORE INTO articles (id, source_id, link, title, description, content, published_at, fetched_at)
      VALUES (?, ?, ?, ?, ?, ?, ?, ?)
    `);

    let count = 0;
    for (const article of articles.slice(0, this.config.maxArticlesPerSource)) {
      const articleId = `art_${Date.now()}_${Math.random().toString(36).slice(2, 8)}`;
      insert.run(
        articleId,
        sourceId,
        article.link,
        article.title,
        article.description ?? null,
        article.content ?? null,
        article.publishedAt,
        now
      );
      count++;
    }

    // Update source last_fetched and count
    this.db.prepare(`
      UPDATE news_sources SET last_fetched = ?, last_article_count = ? WHERE id = ?
    `).run(now, count, sourceId);

    this.ctx.logger.debug('Source fetch complete', { sourceId, articlesAdded: count });
  }

  /** Parse RSS 2.0 feed */
  private parseRSS(xml: string): Array<{ title: string; link: string; description?: string; content?: string; publishedAt: number }> {
    const items: Array<{ title: string; link: string; description?: string; content?: string; publishedAt: number }> = [];

    // Simple regex extraction for <item> blocks
    const itemRegex = /<item[^>]*>([\s\S]*?)<\/item>/gi;
    let match;
    while ((match = itemRegex.exec(xml)) !== null) {
      const itemXml = match[1];

      const getTag = (tag: string): string | null => {
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
  private parseAtom(xml: string): Array<{ title: string; link: string; description?: string; content?: string; publishedAt: number }> {
    const entries: Array<{ title: string; link: string; description?: string; content?: string; publishedAt: number }> = [];

    const entryRegex = /<entry[^>]*>([\s\S]*?)<\/entry>/gi;
    let match;
    while ((match = entryRegex.exec(xml)) !== null) {
      const entryXml = match[1];

      const getTag = (tag: string): string | null => {
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
  async refreshAll(): Promise<void> {
    const sources = this.getSources();
    for (const source of sources) {
      try {
        await this.fetchSource(source.id, source.url);
      } catch (err) {
        this.ctx.logger.error('Refresh failed', { sourceId: source.id, url: source.url });
      }
    }
  }

  /** Get source by ID */
  getSource(id: string): NewsSource | null {
    const row = this.db.prepare('SELECT * FROM news_sources WHERE id = ?').get(id) as any;
    if (!row) return null;
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

export default new NewsDigestFeature();