/**
 * Skills Library Feature
 *
 * Collects, saves and reuses successful skills, plans and code patterns.
 * Allows fast lookup for agent to reapply previously successful workflows or solutions.
 */

import { v4 as uuidv4 } from 'uuid';
import { existsSync, mkdirSync, readFileSync, writeFileSync, readdirSync } from 'fs';
import { join, resolve, dirname } from 'path';
import type { FeatureModule, FeatureMeta, FeatureContext, HealthStatus } from '../../core/plugin-loader';

// ─── Types ───────────────────────────────────────────────────────────────

export interface SkillRecord {
  id: string;
  name: string;
  summary: string;
  tags: string[];
  code: string;
  description?: string;
  source?: string;
  createdAt: number;
  updatedAt: number;
  successCount: number;
  usageHistory: Array<{ usedAt: number; context: string }>;
}

export interface SkillsLibraryConfig {
  enabled: boolean;
  storageDir: string;
}

// ─── Feature Core ────────────────────────────────────────────────────────

const DEFAULT_CONFIG: SkillsLibraryConfig = {
  enabled: false,
  storageDir: './data/skills-library',
};

class SkillsLibraryFeature implements FeatureModule {
  readonly meta: FeatureMeta = {
    name: 'skills-library',
    version: '0.1.0',
    description: 'Knowledge base of successful skills, code, agent routines',
    dependencies: [],
  };

  private config: SkillsLibraryConfig = { ...DEFAULT_CONFIG };
  private ctx!: FeatureContext;

  async init(config: Record<string, unknown>, context: FeatureContext): Promise<void> {
    this.ctx = context;
    this.config = { ...this.config, ...(config as Partial<SkillsLibraryConfig>) };
    this.ensureStorage();
  }

  async start(): Promise<void> {
    this.ensureStorage();
  }

  async stop(): Promise<void> { /* nothing needed */ }

  async healthCheck(): Promise<HealthStatus> {
    const skillCount = this.listSkills().length;
    return { healthy: true, details: { totalSkills: skillCount } };
  }

  // ── Skill Storage Helpers ──────────────────────────────────────────────

  private ensureStorage(): void {
    const dir = resolve(this.config.storageDir);
    if (!existsSync(dir)) {
      mkdirSync(dir, { recursive: true });
      this.ctx.logger?.info('Created skills library storage', { dir });
    }
  }

  private skillPath(id: string): string {
    return resolve(this.config.storageDir, `${id}.json`);
  }

  // ─── Core API ──────────────────────────────────────────────────────────

  /** Save new skill to library */
  addSkill(skill: Omit<SkillRecord, 'id' | 'createdAt' | 'updatedAt' | 'successCount' | 'usageHistory'>): SkillRecord {
    const id = uuidv4();
    const now = Date.now();
    const record: SkillRecord = {
      ...skill,
      id,
      createdAt: now,
      updatedAt: now,
      successCount: 1,
      usageHistory: [],
    };
    writeFileSync(this.skillPath(id), JSON.stringify(record, null, 2), 'utf8');
    return record;
  }

  getSkill(id: string): SkillRecord | null {
    try {
      return JSON.parse(readFileSync(this.skillPath(id), 'utf8')) as SkillRecord;
    } catch {
      return null;
    }
  }

  listSkills(): SkillRecord[] {
    const dir = resolve(this.config.storageDir);
    if (!existsSync(dir)) return [];
    return readdirSync(dir)
      .filter(f => f.endsWith('.json'))
      .map(f => {
        try { return JSON.parse(readFileSync(join(dir, f), 'utf8')) as SkillRecord; }
        catch { return null; }
      })
      .filter(Boolean) as SkillRecord[];
  }

  /** Search skills by tag or name */
  searchSkills(query: string): SkillRecord[] {
    const q = query.toLowerCase();
    return this.listSkills().filter(skill =>
      skill.name.toLowerCase().includes(q) ||
      skill.tags.some(t => t.toLowerCase().includes(q))
    );
  }

  /** Record usage of a skill */
  recordUsage(id: string, context: string): void {
    const skill = this.getSkill(id);
    if (!skill) return;
    skill.successCount++;
    skill.usageHistory.push({ usedAt: Date.now(), context });
    skill.updatedAt = Date.now();
    writeFileSync(this.skillPath(id), JSON.stringify(skill, null, 2), 'utf8');
  }

  updateSkill(id: string, patch: Partial<Omit<SkillRecord, 'id'>>): SkillRecord | null {
    const skill = this.getSkill(id);
    if (!skill) return null;
    const updated = { ...skill, ...patch, updatedAt: Date.now() };
    writeFileSync(this.skillPath(id), JSON.stringify(updated, null, 2), 'utf8');
    return updated;
  }

  removeSkill(id: string): boolean {
    try {
      const path = this.skillPath(id);
      if (existsSync(path)) {
        require('fs').unlinkSync(path);
        return true;
      }
      return false;
    } catch {
      return false;
    }
  }
}

export default new SkillsLibraryFeature();
