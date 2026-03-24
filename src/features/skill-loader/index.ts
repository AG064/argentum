/**
 * AG-Claw Skill Loader
 *
 * Loads SKILL.md files from AG-Claw feature directories and injects them
 * into agent context. This is the OpenFang "Hand" pattern - domain expertise
 * bundled with features.
 *
 * Unlike the global skills-loader (which loads from ~/.openclaw/workspace/skills/),
 * this feature loads SKILL.md from AG-Claw's own feature directories.
 */

import { readFileSync, existsSync, readdirSync } from 'fs';
import { join, dirname, resolve } from 'path';

import {
  type FeatureModule,
  type FeatureContext,
  type FeatureMeta,
  type HealthStatus,
} from '../../core/plugin-loader';

// ─── Types ───────────────────────────────────────────────────────────────────

export interface SkillContext {
  skillName: string;
  content: string;
  rootDir: string;
}

export interface SkillLoaderConfig {
  enabled: boolean;
  featuresPath?: string;
}

// ─── Feature ─────────────────────────────────────────────────────────────────

class SkillLoaderFeature implements FeatureModule {
  readonly meta: FeatureMeta = {
    name: 'skill-loader',
    version: '0.1.0',
    description: 'Loads SKILL.md from feature directories into agent context (OpenFang Hand pattern)',
    dependencies: [],
  };

  private ctx: FeatureContext | null = null;
  private featuresPath: string = '';
  private loadedSkills: SkillContext[] = [];

  async init(config: Record<string, unknown>, context: FeatureContext): Promise<void> {
    this.ctx = context;
    this.featuresPath =
      (config['featuresPath'] as string) ||
      resolve(__dirname, '../../features');

    await this.scanForSkills();
  }

  async start(): Promise<void> {}

  async stop(): Promise<void> {
    this.loadedSkills = [];
  }

  async healthCheck(): Promise<HealthStatus> {
    return {
      healthy: true,
      message: `Loaded ${this.loadedSkills.length} feature skills`,
      details: {
        featuresPath: this.featuresPath,
        skillCount: this.loadedSkills.length,
      },
    };
  }

  /**
   * Scan all feature directories for SKILL.md files
   */
  private async scanForSkills(): Promise<void> {
    if (!existsSync(this.featuresPath)) {
      this.ctx?.logger?.warn?.('Features directory not found', { path: this.featuresPath });
      return;
    }

    const entries = readdirSync(this.featuresPath, { withFileTypes: true });
    let count = 0;

    for (const entry of entries) {
      if (!entry.isDirectory()) continue;
      const skill = this.loadSkillFromFeature(join(this.featuresPath, entry.name));
      if (skill) {
        this.loadedSkills.push(skill);
        count++;
      }
    }

    this.ctx?.logger?.info?.(`Found ${count} feature skills in ${this.featuresPath}`);
  }

  /**
   * Load SKILL.md from a feature directory
   */
  loadSkillFromFeature(featureDir: string): SkillContext | null {
    const skillPath = join(featureDir, 'SKILL.md');
    if (!existsSync(skillPath)) return null;

    const content = readFileSync(skillPath, 'utf-8');
    const featureName = dirname(featureDir).split('/').pop() || 'unknown';

    return {
      skillName: featureName,
      content,
      rootDir: featureDir,
    };
  }

  /**
   * Inject skills into agent context for a given feature set
   */
  injectSkillsIntoContext(
    featureDirs: string[],
    context: Record<string, unknown>
  ): Record<string, unknown> {
    const skills: SkillContext[] = [];

    for (const dir of featureDirs) {
      const skill = this.loadSkillFromFeature(dir);
      if (skill) skills.push(skill);
    }

    return {
      ...context,
      skills: skills.map(s => `# ${s.skillName}\n\n${s.content}`).join('\n\n---\n\n'),
      _skills: skills,
    };
  }

  /**
   * Get all loaded skills
   */
  getLoadedSkills(): SkillContext[] {
    return [...this.loadedSkills];
  }

  /**
   * Get skills formatted as markdown string (for context injection)
   */
  getSkillsAsText(): string {
    return this.loadedSkills
      .map(s => `# ${s.skillName}\n\n${s.content}`)
      .join('\n\n---\n\n');
  }
}

export const skillLoader = new SkillLoaderFeature();
export default skillLoader;
