/**
 * Skills Library Feature (SQLite)
 *
 * Stores skill records with simple versioning using better-sqlite3.
 * Note: code is stored as text only and never executed by this module.
 */

import { randomUUID } from 'crypto';
import { mkdirSync, existsSync } from 'fs';
import { dirname, resolve } from 'path';

import Database from 'better-sqlite3';

import type {
  FeatureModule,
  FeatureMeta,
  FeatureContext,
  HealthStatus,
} from '../../core/plugin-loader';

export interface SkillRecord {
  id: string;
  name: string;
  version: string;
  description: string;
  code: string;
  tags: string[];
  author?: string;
  createdAt: number;
  updatedAt: number;
}

export interface SkillsLibraryConfig {
  enabled: boolean;
  dbPath: string;
  storageDir: string;
}

const DEFAULT_CONFIG: SkillsLibraryConfig = {
  enabled: false,
  dbPath: './data/skills-library.db',
  storageDir: './data/skills-storage',
};

class SkillsLibraryFeature implements FeatureModule {
  readonly meta: FeatureMeta = {
    name: 'skills-library',
    version: '0.0.4',
    description: 'Library of agent skills with versioning (SQLite)',
    dependencies: [],
  };

  private config: SkillsLibraryConfig = { ...DEFAULT_CONFIG };
  private ctx!: FeatureContext;
  private db!: Database.Database;

  async init(config: Record<string, unknown>, context: FeatureContext): Promise<void> {
    this.ctx = context;
    this.config = { ...this.config, ...(config as Partial<SkillsLibraryConfig>) };
    this.initDatabase();
  }

  async start(): Promise<void> {
    /* nothing */
  }
  async stop(): Promise<void> {
    this.db?.close();
  }

  async healthCheck(): Promise<HealthStatus> {
    const count = (this.db.prepare('SELECT COUNT(*) as c FROM skills').get() as { c: number }).c;
    return { healthy: true, details: { totalSkills: count } };
  }

  // ── Core API required by task ─────────────────────────────────────────

  registerSkill(payload: {
    name: string;
    version?: string;
    description?: string;
    code: string;
    tags?: string[];
    author?: string;
  }): SkillRecord {
    const id = randomUUID();
    const now = Date.now();
    const version = payload.version ?? '1.0.0';

    this.db
      .prepare(
        `INSERT INTO skills (id, name, version, description, code, tags, author, created_at, updated_at)
       VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)`,
      )
      .run(
        id,
        payload.name,
        version,
        payload.description ?? '',
        payload.code,
        JSON.stringify(payload.tags ?? []),
        payload.author ?? '',
        now,
        now,
      );

    const rec: SkillRecord = {
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

  getSkill(id: string): SkillRecord | null {
    const row = this.db.prepare('SELECT * FROM skills WHERE id = ?').get(id) as any | undefined;
    if (!row) return null;
    return this.rowToSkill(row);
  }

  listSkills(): SkillRecord[] {
    const rows = this.db.prepare('SELECT * FROM skills ORDER BY updated_at DESC').all() as any[];
    return rows.map((r) => this.rowToSkill(r));
  }

  searchSkills(query: string): SkillRecord[] {
    const q = `%${query.toLowerCase()}%`;
    const rows = this.db
      .prepare(
        `SELECT * FROM skills WHERE LOWER(name) LIKE ? OR LOWER(description) LIKE ? OR LOWER(tags) LIKE ? ORDER BY updated_at DESC LIMIT 100`,
      )
      .all(q, q, q) as any[];
    return rows.map((r) => this.rowToSkill(r));
  }

  updateSkill(
    id: string,
    patch: Partial<Omit<SkillRecord, 'id' | 'createdAt'>>,
  ): SkillRecord | null {
    const existing = this.getSkill(id);
    if (!existing) return null;
    const updated = { ...existing, ...patch, updatedAt: Date.now() } as SkillRecord;

    this.db
      .prepare(
        `UPDATE skills SET name = ?, version = ?, description = ?, code = ?, tags = ?, author = ?, updated_at = ? WHERE id = ?`,
      )
      .run(
        updated.name,
        updated.version,
        updated.description,
        updated.code,
        JSON.stringify(updated.tags),
        updated.author ?? '',
        updated.updatedAt,
        id,
      );

    this.ctx.logger?.info('Skill updated', { id });
    return updated;
  }

  removeSkill(id: string): boolean {
    const res = this.db.prepare('DELETE FROM skills WHERE id = ?').run(id);
    return res.changes > 0;
  }

  // ── Private helpers ──────────────────────────────────────────────────

  private initDatabase(): void {
    const full = resolve(this.config.dbPath);
    const dir = dirname(full);
    if (!existsSync(dir)) mkdirSync(dir, { recursive: true });

    this.db = new Database(full);
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

  private rowToSkill(row: any): SkillRecord {
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

  private safeParse(s: string): string[] {
    try {
      return JSON.parse(s);
    } catch {
      return [];
    }
  }
}

export default new SkillsLibraryFeature();
