/**
 * Life Domains Feature
 *
 * Structures memory and knowledge by life domains (work, health, finance,
 * relationships, learning, etc.). Enables domain-specific context retrieval
 * and cross-domain insights.
 */

import { randomUUID } from 'crypto';
import { mkdirSync, existsSync } from 'fs';
import { dirname, resolve } from 'path';

import Database from 'better-sqlite3';

import { type FeatureModule, type FeatureContext, type FeatureMeta, type HealthStatus } from '../../core/plugin-loader';

// ─── Interfaces ──────────────────────────────────────────────────────────────

export interface LifeDomain {
  id: string;
  name: string;
  description: string;
  color: string;
  icon: string;
  parentId: string | null;
  priority: number; // 1-10
  createdAt: number;
  updatedAt: number;
}

export interface DomainEntry {
  id: string;
  domainId: string;
  type: 'note' | 'goal' | 'insight' | 'decision' | 'resource' | 'habit';
  title: string;
  content: string;
  tags: string[];
  importance: number; // 0-1
  metadata: Record<string, unknown>;
  createdAt: number;
  updatedAt: number;
}

export interface DomainStats {
  domainId: string;
  domainName: string;
  entryCount: number;
  lastEntryAt: number | null;
  typeBreakdown: Record<string, number>;
  avgImportance: number;
}

export interface CrossDomainInsight {
  domains: string[];
  pattern: string;
  confidence: number;
  relatedEntries: string[];
}

export interface LifeDomainsConfig {
  enabled: boolean;
  dbPath: string;
  defaultDomains: Array<{ name: string; description: string; color: string; icon: string }>;
  autoClassify: boolean;
  crossDomainAnalysis: boolean;
}

interface DomainRow {
  id: string;
  name: string;
  description: string;
  color: string;
  icon: string;
  parent_id: string | null;
  priority: number;
  created_at: number;
  updated_at: number;
}

interface EntryRow {
  id: string;
  domain_id: string;
  type: string;
  title: string;
  content: string;
  tags: string;
  importance: number;
  metadata: string;
  created_at: number;
  updated_at: number;
}

// ─── Default Domains ─────────────────────────────────────────────────────────

const DEFAULT_DOMAINS = [
  { name: 'Work', description: 'Professional life, projects, career', color: '#4f46e5', icon: '💼' },
  { name: 'Health', description: 'Physical and mental wellness', color: '#10b981', icon: '🏃' },
  { name: 'Finance', description: 'Money, investments, budgeting', color: '#f59e0b', icon: '💰' },
  { name: 'Relationships', description: 'Family, friends, social connections', color: '#ec4899', icon: '❤️' },
  { name: 'Learning', description: 'Education, skills, knowledge growth', color: '#8b5cf6', icon: '📚' },
  { name: 'Projects', description: 'Personal projects and hobbies', color: '#06b6d4', icon: '🛠️' },
  { name: 'Home', description: 'Living space, household, daily logistics', color: '#84cc16', icon: '🏠' },
  { name: 'Creative', description: 'Art, writing, music, creative expression', color: '#f97316', icon: '🎨' },
];

// ─── Classification Keywords ─────────────────────────────────────────────────

const DOMAIN_KEYWORDS: Record<string, string[]> = {
  Work: ['meeting', 'deadline', 'project', 'client', 'manager', 'sprint', 'deploy', 'code', 'review', 'standup', 'jira', 'pull request', 'merge', 'release',
         'встреча', 'дедлайн', 'проект', 'клиент', 'менеджер', 'спринт', 'код', 'ревью', 'релиз', 'работа', 'офис', 'задача', 'коллега', 'начальник', 'совещание', 'отпуск'],
  Health: ['exercise', 'workout', 'gym', 'run', 'sleep', 'diet', 'doctor', 'medication', 'yoga', 'meditation', 'steps', 'calories', 'water',
           'спортзал', 'спорт', 'тренировка', 'бег', 'сон', 'диета', 'врач', 'доктор', 'йога', 'медитация', 'здоровье', 'калории', 'вода', 'лекарство', 'больница', 'анализы'],
  Finance: ['budget', 'expense', 'income', 'invest', 'stock', 'crypto', 'savings', 'rent', 'salary', 'tax', 'invoice', 'payment', 'bank',
            'бюджет', 'расход', 'доход', 'инвестиции', 'акции', 'крипто', 'сбережения', 'аренда', 'зарплата', 'налог', 'счёт', 'оплата', 'банк', 'деньги', 'карта', 'долг'],
  Relationships: ['friend', 'family', 'partner', 'date', 'birthday', 'call', 'visit', 'dinner', 'party', 'together', 'anniversary',
                  'друг', 'семья', 'партнёр', 'парень', 'девушка', 'день рождения', 'звонок', 'визит', 'ужин', 'вечеринка', 'вместе', 'годовщина', 'мама', 'папа', 'брат', 'сестра'],
  Learning: ['learn', 'study', 'course', 'tutorial', 'book', 'read', 'practice', 'exam', 'certification', 'skill', 'language', 'lesson',
             'учить', 'учиться', 'курс', 'учебник', 'книга', 'читать', 'практика', 'экзамен', 'сертификат', 'навык', 'язык', 'урок', 'лекция', 'домашка', 'школа', 'университет'],
  Projects: ['build', 'create', 'prototype', 'side project', 'weekend', 'hobby', '3d', 'print', 'arduino', 'raspberry pi',
             'создать', 'построить', 'прототип', 'хобби', 'выходные', 'проект', '3d', 'печать', 'самоделка', 'конструктор'],
  Home: ['clean', 'repair', 'furniture', 'kitchen', 'garden', 'laundry', 'grocery', 'shopping', 'appliance', 'decoration',
         'убрать', 'починить', 'мебель', 'кухня', 'сад', 'стирка', 'покупки', 'магазин', 'техника', 'декор', 'дом', 'квартира', 'ремонт'],
  Creative: ['write', 'draw', 'paint', 'compose', 'photo', 'video', 'edit', 'design', 'sketch', 'music', 'story', 'poem',
             'писать', 'рисовать', 'рисунок', 'композиция', 'фото', 'видео', 'монтаж', 'дизайн', 'эскиз', 'музыка', 'история', 'стих', 'творчество', 'арт'],
};

// ─── Feature ─────────────────────────────────────────────────────────────────

class LifeDomainsFeature implements FeatureModule {
  readonly meta: FeatureMeta = {
    name: 'life-domains',
    version: '0.1.0',
    description: 'Memory structured by life domains with auto-classification',
    dependencies: [],
  };

  private config: LifeDomainsConfig = {
    enabled: false,
    dbPath: './data/life-domains.db',
    defaultDomains: DEFAULT_DOMAINS,
    autoClassify: true,
    crossDomainAnalysis: true,
  };
  private db!: Database.Database;
  private ctx!: FeatureContext;

  async init(config: Record<string, unknown>, context: FeatureContext): Promise<void> {
    this.ctx = context;
    this.config = { ...this.config, ...(config as Partial<LifeDomainsConfig>) };
    this.initDatabase();
  }

  async start(): Promise<void> {
    // Seed default domains if none exist
    const count = (this.db.prepare('SELECT COUNT(*) as c FROM domains').get() as { c: number }).c;
    if (count === 0) {
      this.seedDefaultDomains();
    }

    const domainCount = (this.db.prepare('SELECT COUNT(*) as c FROM domains').get() as { c: number }).c;
    const entryCount = (this.db.prepare('SELECT COUNT(*) as c FROM domain_entries').get() as { c: number }).c;

    this.ctx.logger.info('Life Domains active', { domains: domainCount, entries: entryCount });
  }

  async stop(): Promise<void> {
    this.db?.close();
  }

  async healthCheck(): Promise<HealthStatus> {
    const domains = (this.db.prepare('SELECT COUNT(*) as c FROM domains').get() as { c: number }).c;
    const entries = (this.db.prepare('SELECT COUNT(*) as c FROM domain_entries').get() as { c: number }).c;
    return {
      healthy: true,
      details: { domains, entries },
    };
  }

  // ─── Domain CRUD ─────────────────────────────────────────────────────────

  /** Create a new life domain */
  createDomain(name: string, description: string, options?: { color?: string; icon?: string; parentId?: string; priority?: number }): LifeDomain {
    const id = randomUUID();
    const now = Date.now();

    const domain: LifeDomain = {
      id,
      name,
      description,
      color: options?.color ?? '#6b7280',
      icon: options?.icon ?? '📌',
      parentId: options?.parentId ?? null,
      priority: options?.priority ?? 5,
      createdAt: now,
      updatedAt: now,
    };

    this.db.prepare(
      `INSERT INTO domains (id, name, description, color, icon, parent_id, priority, created_at, updated_at)
       VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)`
    ).run(id, name, description, domain.color, domain.icon, domain.parentId, domain.priority, now, now);

    this.ctx.logger.info('Domain created', { id, name });
    return domain;
  }

  /** List all domains */
  listDomains(): LifeDomain[] {
    const rows = this.db.prepare('SELECT * FROM domains ORDER BY priority DESC, name ASC').all() as DomainRow[];
    return rows.map(this.rowToDomain);
  }

  /** Get a domain by ID */
  getDomain(id: string): LifeDomain | null {
    const row = this.db.prepare('SELECT * FROM domains WHERE id = ?').get(id) as DomainRow | undefined;
    return row ? this.rowToDomain(row) : null;
  }

  /** Get domain by name */
  getDomainByName(name: string): LifeDomain | null {
    const row = this.db.prepare('SELECT * FROM domains WHERE name = ? COLLATE NOCASE').get(name) as DomainRow | undefined;
    return row ? this.rowToDomain(row) : null;
  }

  /** Update a domain */
  updateDomain(id: string, updates: Partial<Omit<LifeDomain, 'id' | 'createdAt'>>): LifeDomain | null {
    const existing = this.getDomain(id);
    if (!existing) return null;

    const merged = { ...existing, ...updates, updatedAt: Date.now() };
    this.db.prepare(
      'UPDATE domains SET name=?, description=?, color=?, icon=?, priority=?, updated_at=? WHERE id=?'
    ).run(merged.name, merged.description, merged.color, merged.icon, merged.priority, merged.updatedAt, id);

    return merged;
  }

  /** Delete a domain and all its entries */
  deleteDomain(id: string): boolean {
    this.db.prepare('DELETE FROM domain_entries WHERE domain_id = ?').run(id);
    const result = this.db.prepare('DELETE FROM domains WHERE id = ?').run(id);
    return result.changes > 0;
  }

  // ─── Entry CRUD ──────────────────────────────────────────────────────────

  /** Add an entry to a domain */
  addEntry(
    domainId: string,
    type: DomainEntry['type'],
    title: string,
    content: string,
    options?: { tags?: string[]; importance?: number; metadata?: Record<string, unknown> }
  ): DomainEntry {
    const domain = this.getDomain(domainId);
    if (!domain) throw new Error(`Domain not found: ${domainId}`);

    const id = randomUUID();
    const now = Date.now();

    const entry: DomainEntry = {
      id,
      domainId,
      type,
      title,
      content,
      tags: options?.tags ?? [],
      importance: options?.importance ?? 0.5,
      metadata: options?.metadata ?? {},
      createdAt: now,
      updatedAt: now,
    };

    this.db.prepare(
      `INSERT INTO domain_entries (id, domain_id, type, title, content, tags, importance, metadata, created_at, updated_at)
       VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)`
    ).run(id, domainId, type, title, content, JSON.stringify(entry.tags), entry.importance, JSON.stringify(entry.metadata), now, now);

    this.ctx.logger.debug('Entry added', { domainId, type, title: title.slice(0, 50) });
    return entry;
  }

  /** List entries for a domain */
  listEntries(domainId: string, options?: { type?: DomainEntry['type']; minImportance?: number; limit?: number }): DomainEntry[] {
    let sql = 'SELECT * FROM domain_entries WHERE domain_id = ?';
    const params: unknown[] = [domainId];

    if (options?.type) {
      sql += ' AND type = ?';
      params.push(options.type);
    }
    if (options?.minImportance !== undefined) {
      sql += ' AND importance >= ?';
      params.push(options.minImportance);
    }

    sql += ' ORDER BY importance DESC, created_at DESC';

    if (options?.limit) {
      sql += ' LIMIT ?';
      params.push(options.limit);
    }

    const rows = this.db.prepare(sql).all(...params) as EntryRow[];
    return rows.map(this.rowToEntry);
  }

  /** Search entries across all domains */
  searchEntries(query: string, options?: { domainId?: string; type?: DomainEntry['type']; limit?: number }): DomainEntry[] {
    let sql = `SELECT * FROM domain_entries WHERE (title LIKE ? OR content LIKE ? OR tags LIKE ?)`;
    const params: unknown[] = [`%${query}%`, `%${query}%`, `%${query}%`];

    if (options?.domainId) {
      sql += ' AND domain_id = ?';
      params.push(options.domainId);
    }
    if (options?.type) {
      sql += ' AND type = ?';
      params.push(options.type);
    }

    sql += ' ORDER BY importance DESC, created_at DESC LIMIT ?';
    params.push(options?.limit ?? 20);

    const rows = this.db.prepare(sql).all(...params) as EntryRow[];
    return rows.map(this.rowToEntry);
  }

  /** Get an entry by ID */
  getEntry(id: string): DomainEntry | null {
    const row = this.db.prepare('SELECT * FROM domain_entries WHERE id = ?').get(id) as EntryRow | undefined;
    return row ? this.rowToEntry(row) : null;
  }

  /** Update an entry */
  updateEntry(id: string, updates: Partial<Omit<DomainEntry, 'id' | 'domainId' | 'createdAt'>>): DomainEntry | null {
    const existing = this.getEntry(id);
    if (!existing) return null;

    const merged = { ...existing, ...updates, updatedAt: Date.now() };
    this.db.prepare(
      'UPDATE domain_entries SET title=?, content=?, tags=?, importance=?, metadata=?, updated_at=? WHERE id=?'
    ).run(merged.title, merged.content, JSON.stringify(merged.tags), merged.importance, JSON.stringify(merged.metadata), merged.updatedAt, id);

    return merged;
  }

  /** Delete an entry */
  deleteEntry(id: string): boolean {
    const result = this.db.prepare('DELETE FROM domain_entries WHERE id = ?').run(id);
    return result.changes > 0;
  }

  // ─── Auto-Classification ─────────────────────────────────────────────────

  /** Auto-classify text into a domain */
  classifyText(text: string): Array<{ domain: string; confidence: number }> {
    const lower = text.toLowerCase();
    const scores: Array<{ domain: string; confidence: number }> = [];

    for (const [domainName, keywords] of Object.entries(DOMAIN_KEYWORDS)) {
      let matches = 0;
      for (const keyword of keywords) {
        if (lower.includes(keyword)) matches++;
      }
      if (matches > 0) {
        const confidence = Math.min(1, matches / 3); // 3+ keyword matches = 100%
        scores.push({ domain: domainName, confidence });
      }
    }

    return scores.sort((a, b) => b.confidence - a.confidence);
  }

  /** Auto-add entry to best matching domain */
  autoAddEntry(title: string, content: string, type: DomainEntry['type'] = 'note'): DomainEntry | null {
    if (!this.config.autoClassify) return null;

    const classifications = this.classifyText(`${title} ${content}`);
    if (classifications.length === 0) return null;

    const best = classifications[0]!;
    const domain = this.getDomainByName(best.domain);
    if (!domain) return null;

    return this.addEntry(domain.id, type, title, content, {
      importance: best.confidence,
      metadata: { autoClassified: true, confidence: best.confidence, alternatives: classifications.slice(1) },
    });
  }

  // ─── Statistics ──────────────────────────────────────────────────────────

  /** Get statistics for all domains */
  getDomainStats(): DomainStats[] {
    const domains = this.listDomains();
    return domains.map(domain => {
      const entries = this.listEntries(domain.id);
      const typeBreakdown: Record<string, number> = {};
      let totalImportance = 0;

      for (const entry of entries) {
        typeBreakdown[entry.type] = (typeBreakdown[entry.type] ?? 0) + 1;
        totalImportance += entry.importance;
      }

      const lastEntry = entries.length > 0 ? entries[0]!.createdAt : null;

      return {
        domainId: domain.id,
        domainName: domain.name,
        entryCount: entries.length,
        lastEntryAt: lastEntry,
        typeBreakdown,
        avgImportance: entries.length > 0 ? Math.round((totalImportance / entries.length) * 100) / 100 : 0,
      };
    });
  }

  /** Get recent activity across all domains */
  getRecentActivity(limit = 10): Array<DomainEntry & { domainName: string }> {
    const rows = this.db.prepare(`
      SELECT de.*, d.name as domain_name
      FROM domain_entries de
      JOIN domains d ON d.id = de.domain_id
      ORDER BY de.created_at DESC
      LIMIT ?
    `).all(limit) as Array<EntryRow & { domain_name: string }>;

    return rows.map(row => ({
      ...this.rowToEntry(row),
      domainName: row.domain_name,
    }));
  }

  // ─── Private Helpers ─────────────────────────────────────────────────────

  private initDatabase(): void {
    const fullPath = resolve(this.config.dbPath);
    if (!existsSync(dirname(fullPath))) {
      mkdirSync(dirname(fullPath), { recursive: true });
    }

    this.db = new Database(fullPath);
    this.db.pragma('journal_mode = WAL');
    this.db.pragma('synchronous = NORMAL');

    this.db.exec(`
      CREATE TABLE IF NOT EXISTS domains (
        id TEXT PRIMARY KEY,
        name TEXT NOT NULL UNIQUE,
        description TEXT DEFAULT '',
        color TEXT DEFAULT '#6b7280',
        icon TEXT DEFAULT '📌',
        parent_id TEXT,
        priority INTEGER DEFAULT 5,
        created_at INTEGER NOT NULL,
        updated_at INTEGER NOT NULL,
        FOREIGN KEY (parent_id) REFERENCES domains(id) ON DELETE SET NULL
      );

      CREATE INDEX IF NOT EXISTS idx_domains_name ON domains(name);
      CREATE INDEX IF NOT EXISTS idx_domains_parent ON domains(parent_id);

      CREATE TABLE IF NOT EXISTS domain_entries (
        id TEXT PRIMARY KEY,
        domain_id TEXT NOT NULL,
        type TEXT NOT NULL,
        title TEXT NOT NULL,
        content TEXT DEFAULT '',
        tags TEXT DEFAULT '[]',
        importance REAL DEFAULT 0.5,
        metadata TEXT DEFAULT '{}',
        created_at INTEGER NOT NULL,
        updated_at INTEGER NOT NULL,
        FOREIGN KEY (domain_id) REFERENCES domains(id) ON DELETE CASCADE
      );

      CREATE INDEX IF NOT EXISTS idx_entries_domain ON domain_entries(domain_id);
      CREATE INDEX IF NOT EXISTS idx_entries_type ON domain_entries(type);
      CREATE INDEX IF NOT EXISTS idx_entries_importance ON domain_entries(importance DESC);
      CREATE INDEX IF NOT EXISTS idx_entries_created ON domain_entries(created_at DESC);
    `);
  }

  private seedDefaultDomains(): void {
    const stmt = this.db.prepare(
      `INSERT INTO domains (id, name, description, color, icon, parent_id, priority, created_at, updated_at)
       VALUES (?, ?, ?, ?, ?, NULL, 5, ?, ?)`
    );
    const now = Date.now();

    for (const domain of this.config.defaultDomains) {
      stmt.run(randomUUID(), domain.name, domain.description, domain.color, domain.icon, now, now);
    }

    this.ctx.logger.info('Seeded default life domains', { count: this.config.defaultDomains.length });
  }

  private rowToDomain(row: DomainRow): LifeDomain {
    return {
      id: row.id,
      name: row.name,
      description: row.description,
      color: row.color,
      icon: row.icon,
      parentId: row.parent_id,
      priority: row.priority,
      createdAt: row.created_at,
      updatedAt: row.updated_at,
    };
  }

  private rowToEntry(row: EntryRow): DomainEntry {
    return {
      id: row.id,
      domainId: row.domain_id,
      type: row.type as DomainEntry['type'],
      title: row.title,
      content: row.content,
      tags: this.parseJsonArray(row.tags),
      importance: row.importance,
      metadata: this.parseJson(row.metadata),
      createdAt: row.created_at,
      updatedAt: row.updated_at,
    };
  }

  private parseJson(str: string): Record<string, unknown> {
    try { return JSON.parse(str); } catch { return {}; }
  }

  private parseJsonArray(str: string): string[] {
    try { return JSON.parse(str); } catch { return []; }
  }
}

export default new LifeDomainsFeature();
