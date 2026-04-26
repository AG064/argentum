'use strict';
/**
 * Life Domains Feature
 *
 * Structures memory and knowledge by life domains (work, health, finance,
 * relationships, learning, etc.). Enables domain-specific context retrieval
 * and cross-domain insights.
 */
var __importDefault =
  (this && this.__importDefault) ||
  function (mod) {
    return mod && mod.__esModule ? mod : { default: mod };
  };
Object.defineProperty(exports, '__esModule', { value: true });
const better_sqlite3_1 = __importDefault(require('better-sqlite3'));
const fs_1 = require('fs');
const path_1 = require('path');
const crypto_1 = require('crypto');
// ─── Default Domains ─────────────────────────────────────────────────────────
const DEFAULT_DOMAINS = [
  {
    name: 'Work',
    description: 'Professional life, projects, career',
    color: '#4f46e5',
    icon: '💼',
  },
  { name: 'Health', description: 'Physical and mental wellness', color: '#10b981', icon: '🏃' },
  { name: 'Finance', description: 'Money, investments, budgeting', color: '#f59e0b', icon: '💰' },
  {
    name: 'Relationships',
    description: 'Family, friends, social connections',
    color: '#ec4899',
    icon: '❤️',
  },
  {
    name: 'Learning',
    description: 'Education, skills, knowledge growth',
    color: '#8b5cf6',
    icon: '📚',
  },
  { name: 'Projects', description: 'Personal projects and hobbies', color: '#06b6d4', icon: '🛠️' },
  {
    name: 'Home',
    description: 'Living space, household, daily logistics',
    color: '#84cc16',
    icon: '🏠',
  },
  {
    name: 'Creative',
    description: 'Art, writing, music, creative expression',
    color: '#f97316',
    icon: '🎨',
  },
];
// ─── Classification Keywords ─────────────────────────────────────────────────
const DOMAIN_KEYWORDS = {
  Work: [
    'meeting',
    'deadline',
    'project',
    'client',
    'manager',
    'sprint',
    'deploy',
    'code',
    'review',
    'standup',
    'jira',
    'pull request',
    'merge',
    'release',
  ],
  Health: [
    'exercise',
    'workout',
    'gym',
    'run',
    'sleep',
    'diet',
    'doctor',
    'medication',
    'yoga',
    'meditation',
    'steps',
    'calories',
    'water',
  ],
  Finance: [
    'budget',
    'expense',
    'income',
    'invest',
    'stock',
    'crypto',
    'savings',
    'rent',
    'salary',
    'tax',
    'invoice',
    'payment',
    'bank',
  ],
  Relationships: [
    'friend',
    'family',
    'partner',
    'date',
    'birthday',
    'call',
    'visit',
    'dinner',
    'party',
    'together',
    'anniversary',
  ],
  Learning: [
    'learn',
    'study',
    'course',
    'tutorial',
    'book',
    'read',
    'practice',
    'exam',
    'certification',
    'skill',
    'language',
    'lesson',
  ],
  Projects: [
    'build',
    'create',
    'prototype',
    'side project',
    'weekend',
    'hobby',
    '3d',
    'print',
    'arduino',
    'raspberry pi',
  ],
  Home: [
    'clean',
    'repair',
    'furniture',
    'kitchen',
    'garden',
    'laundry',
    'grocery',
    'shopping',
    'appliance',
    'decoration',
  ],
  Creative: [
    'write',
    'draw',
    'paint',
    'compose',
    'photo',
    'video',
    'edit',
    'design',
    'sketch',
    'music',
    'story',
    'poem',
  ],
};
// ─── Feature ─────────────────────────────────────────────────────────────────
class LifeDomainsFeature {
  constructor() {
    this.meta = {
      name: 'life-domains',
      version: '0.0.2',
      description: 'Memory structured by life domains with auto-classification',
      dependencies: [],
    };
    this.config = {
      enabled: false,
      dbPath: './data/life-domains.db',
      defaultDomains: DEFAULT_DOMAINS,
      autoClassify: true,
      crossDomainAnalysis: true,
    };
  }
  async init(config, context) {
    this.ctx = context;
    this.config = { ...this.config, ...config };
    this.initDatabase();
  }
  async start() {
    // Seed default domains if none exist
    const count = this.db.prepare('SELECT COUNT(*) as c FROM domains').get().c;
    if (count === 0) {
      this.seedDefaultDomains();
    }
    const domainCount = this.db.prepare('SELECT COUNT(*) as c FROM domains').get().c;
    const entryCount = this.db.prepare('SELECT COUNT(*) as c FROM domain_entries').get().c;
    this.ctx.logger.info('Life Domains active', { domains: domainCount, entries: entryCount });
  }
  async stop() {
    this.db?.close();
  }
  async healthCheck() {
    const domains = this.db.prepare('SELECT COUNT(*) as c FROM domains').get().c;
    const entries = this.db.prepare('SELECT COUNT(*) as c FROM domain_entries').get().c;
    return {
      healthy: true,
      details: { domains, entries },
    };
  }
  // ─── Domain CRUD ─────────────────────────────────────────────────────────
  /** Create a new life domain */
  createDomain(name, description, options) {
    const id = (0, crypto_1.randomUUID)();
    const now = Date.now();
    const domain = {
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
    this.db
      .prepare(
        `INSERT INTO domains (id, name, description, color, icon, parent_id, priority, created_at, updated_at)
       VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)`,
      )
      .run(
        id,
        name,
        description,
        domain.color,
        domain.icon,
        domain.parentId,
        domain.priority,
        now,
        now,
      );
    this.ctx.logger.info('Domain created', { id, name });
    return domain;
  }
  /** List all domains */
  listDomains() {
    const rows = this.db.prepare('SELECT * FROM domains ORDER BY priority DESC, name ASC').all();
    return rows.map(this.rowToDomain);
  }
  /** Get a domain by ID */
  getDomain(id) {
    const row = this.db.prepare('SELECT * FROM domains WHERE id = ?').get(id);
    return row ? this.rowToDomain(row) : null;
  }
  /** Get domain by name */
  getDomainByName(name) {
    const row = this.db.prepare('SELECT * FROM domains WHERE name = ? COLLATE NOCASE').get(name);
    return row ? this.rowToDomain(row) : null;
  }
  /** Update a domain */
  updateDomain(id, updates) {
    const existing = this.getDomain(id);
    if (!existing) return null;
    const merged = { ...existing, ...updates, updatedAt: Date.now() };
    this.db
      .prepare(
        'UPDATE domains SET name=?, description=?, color=?, icon=?, priority=?, updated_at=? WHERE id=?',
      )
      .run(
        merged.name,
        merged.description,
        merged.color,
        merged.icon,
        merged.priority,
        merged.updatedAt,
        id,
      );
    return merged;
  }
  /** Delete a domain and all its entries */
  deleteDomain(id) {
    this.db.prepare('DELETE FROM domain_entries WHERE domain_id = ?').run(id);
    const result = this.db.prepare('DELETE FROM domains WHERE id = ?').run(id);
    return result.changes > 0;
  }
  // ─── Entry CRUD ──────────────────────────────────────────────────────────
  /** Add an entry to a domain */
  addEntry(domainId, type, title, content, options) {
    const domain = this.getDomain(domainId);
    if (!domain) throw new Error(`Domain not found: ${domainId}`);
    const id = (0, crypto_1.randomUUID)();
    const now = Date.now();
    const entry = {
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
    this.db
      .prepare(
        `INSERT INTO domain_entries (id, domain_id, type, title, content, tags, importance, metadata, created_at, updated_at)
       VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)`,
      )
      .run(
        id,
        domainId,
        type,
        title,
        content,
        JSON.stringify(entry.tags),
        entry.importance,
        JSON.stringify(entry.metadata),
        now,
        now,
      );
    this.ctx.logger.debug('Entry added', { domainId, type, title: title.slice(0, 50) });
    return entry;
  }
  /** List entries for a domain */
  listEntries(domainId, options) {
    let sql = 'SELECT * FROM domain_entries WHERE domain_id = ?';
    const params = [domainId];
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
    const rows = this.db.prepare(sql).all(...params);
    return rows.map(this.rowToEntry);
  }
  /** Search entries across all domains */
  searchEntries(query, options) {
    let sql = `SELECT * FROM domain_entries WHERE (title LIKE ? OR content LIKE ? OR tags LIKE ?)`;
    const params = [`%${query}%`, `%${query}%`, `%${query}%`];
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
    const rows = this.db.prepare(sql).all(...params);
    return rows.map(this.rowToEntry);
  }
  /** Get an entry by ID */
  getEntry(id) {
    const row = this.db.prepare('SELECT * FROM domain_entries WHERE id = ?').get(id);
    return row ? this.rowToEntry(row) : null;
  }
  /** Update an entry */
  updateEntry(id, updates) {
    const existing = this.getEntry(id);
    if (!existing) return null;
    const merged = { ...existing, ...updates, updatedAt: Date.now() };
    this.db
      .prepare(
        'UPDATE domain_entries SET title=?, content=?, tags=?, importance=?, metadata=?, updated_at=? WHERE id=?',
      )
      .run(
        merged.title,
        merged.content,
        JSON.stringify(merged.tags),
        merged.importance,
        JSON.stringify(merged.metadata),
        merged.updatedAt,
        id,
      );
    return merged;
  }
  /** Delete an entry */
  deleteEntry(id) {
    const result = this.db.prepare('DELETE FROM domain_entries WHERE id = ?').run(id);
    return result.changes > 0;
  }
  // ─── Auto-Classification ─────────────────────────────────────────────────
  /** Auto-classify text into a domain */
  classifyText(text) {
    const lower = text.toLowerCase();
    const scores = [];
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
  autoAddEntry(title, content, type = 'note') {
    if (!this.config.autoClassify) return null;
    const classifications = this.classifyText(`${title} ${content}`);
    if (classifications.length === 0) return null;
    const best = classifications[0];
    const domain = this.getDomainByName(best.domain);
    if (!domain) return null;
    return this.addEntry(domain.id, type, title, content, {
      importance: best.confidence,
      metadata: {
        autoClassified: true,
        confidence: best.confidence,
        alternatives: classifications.slice(1),
      },
    });
  }
  // ─── Statistics ──────────────────────────────────────────────────────────
  /** Get statistics for all domains */
  getDomainStats() {
    const domains = this.listDomains();
    return domains.map((domain) => {
      const entries = this.listEntries(domain.id);
      const typeBreakdown = {};
      let totalImportance = 0;
      for (const entry of entries) {
        typeBreakdown[entry.type] = (typeBreakdown[entry.type] ?? 0) + 1;
        totalImportance += entry.importance;
      }
      const lastEntry = entries.length > 0 ? entries[0].createdAt : null;
      return {
        domainId: domain.id,
        domainName: domain.name,
        entryCount: entries.length,
        lastEntryAt: lastEntry,
        typeBreakdown,
        avgImportance:
          entries.length > 0 ? Math.round((totalImportance / entries.length) * 100) / 100 : 0,
      };
    });
  }
  /** Get recent activity across all domains */
  getRecentActivity(limit = 10) {
    const rows = this.db
      .prepare(
        `
      SELECT de.*, d.name as domain_name
      FROM domain_entries de
      JOIN domains d ON d.id = de.domain_id
      ORDER BY de.created_at DESC
      LIMIT ?
    `,
      )
      .all(limit);
    return rows.map((row) => ({
      ...this.rowToEntry(row),
      domainName: row.domain_name,
    }));
  }
  // ─── Private Helpers ─────────────────────────────────────────────────────
  initDatabase() {
    const fullPath = (0, path_1.resolve)(this.config.dbPath);
    if (!(0, fs_1.existsSync)((0, path_1.dirname)(fullPath))) {
      (0, fs_1.mkdirSync)((0, path_1.dirname)(fullPath), { recursive: true });
    }
    this.db = new better_sqlite3_1.default(fullPath);
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
  seedDefaultDomains() {
    const stmt = this.db
      .prepare(`INSERT INTO domains (id, name, description, color, icon, parent_id, priority, created_at, updated_at)
       VALUES (?, ?, ?, ?, ?, NULL, 5, ?, ?)`);
    const now = Date.now();
    for (const domain of this.config.defaultDomains) {
      stmt.run(
        (0, crypto_1.randomUUID)(),
        domain.name,
        domain.description,
        domain.color,
        domain.icon,
        now,
        now,
      );
    }
    this.ctx.logger.info('Seeded default life domains', {
      count: this.config.defaultDomains.length,
    });
  }
  rowToDomain(row) {
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
  rowToEntry(row) {
    return {
      id: row.id,
      domainId: row.domain_id,
      type: row.type,
      title: row.title,
      content: row.content,
      tags: this.parseJsonArray(row.tags),
      importance: row.importance,
      metadata: this.parseJson(row.metadata),
      createdAt: row.created_at,
      updatedAt: row.updated_at,
    };
  }
  parseJson(str) {
    try {
      return JSON.parse(str);
    } catch {
      return {};
    }
  }
  parseJsonArray(str) {
    try {
      return JSON.parse(str);
    } catch {
      return [];
    }
  }
}
exports.default = new LifeDomainsFeature();
