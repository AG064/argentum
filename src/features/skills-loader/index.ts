 
/**
 * AG-Claw Skills Loader
 *
 * Loads OpenClaw skills into AG-Claw ecosystem.
 * Any SKILL.md in ~/.openclaw/workspace/skills/ becomes available.
 *
 * Supports Hermes-style progressive disclosure (3 levels):
 * - Level 0: skillsList() → [{name, description, category}] (~3k tokens)
 * - Level 1: skillView(name) → Full content + metadata
 * - Level 2: skillView(name, path) → Specific reference file
 *
 * Compatible with agentskills.io open standard.
 */

import { execSync } from 'child_process';
import * as fs from 'fs';
import * as path from 'path';

// ─── Types ───────────────────────────────────────────────────────────────────

/** YAML frontmatter extracted from SKILL.md */
export interface SkillFrontmatter {
  name: string;
  description: string;
  version?: string;
  platforms?: string[];
  category?: string;
  tags?: string[];
  triggers?: string[];
  required_environment_variables?: Array<{
    name: string;
    prompt?: string;
    help?: string;
    required_for?: string;
  }>;
  metadata?: {
    hermes?: {
      tags?: string[];
      category?: string;
      fallback_for_toolsets?: string[];
      requires_toolsets?: string[];
      fallback_for_tools?: string[];
      requires_tools?: string[];
    };
    clawdbot?: {
      emoji?: string;
      category?: string;
    };
    [key: string]: unknown;
  };
  homepage?: string;
}

/** Level 0: lightweight skill summary for listing */
export interface SkillSummary {
  name: string;
  description: string;
  category: string;
  version: string;
  platforms: string[];
  hasReferences: boolean;
  hasScripts: boolean;
}

/** Full skill metadata (Level 1) */
export interface SkillMeta extends SkillSummary {
  path: string;
  fullContent: string;
  frontmatter: SkillFrontmatter;
  scripts: string[];
  references: string[];
  templates: string[];
}

interface SkillManifest {
  name: string;
  version?: string;
  description?: string;
  scripts?: string[];
  commands?: Record<string, string>;
  triggers?: string[];
  dependencies?: string[];
}

// ─── YAML Frontmatter Parser ─────────────────────────────────────────────────

/**
 * Parse YAML frontmatter from SKILL.md content.
 * Matches Hermes/agentskills.io format:
 * ---
 * name: my-skill
 * description: Brief description
 * version: 1.0.0
 * ---
 */
function parseFrontmatter(content: string): SkillFrontmatter | null {
  const match = content.match(/^---\n([\s\S]*?)\n---\n/);
  if (!match) return null;

  try {
    // Simple YAML parser for frontmatter keys
    const lines = match[1]!.split('\n');
    const result: Record<string, unknown> = {};

    for (const line of lines) {
      const colonIdx = line.indexOf(':');
      if (colonIdx === -1) continue;

      const key = line.slice(0, colonIdx).trim();
      const value = line.slice(colonIdx + 1).trim();

      if (!key) continue;

      // Handle array values like [macos, linux]
      if (value.startsWith('[') && value.endsWith(']')) {
        result[key] = value
          .slice(1, -1)
          .split(',')
          .map((s) => s.trim());
      } else if (value === '' || value === 'null' || value === '~') {
        // Skip empty/null values or continue to check for nested
      } else {
        result[key] = value.replace(/^["']|["']$/g, ''); // Strip quotes
      }
    }

    // Handle nested metadata.hermes object
    if (result['metadata']) {
      try {
        result['metadata'] = JSON.parse(result['metadata'] as string);
      } catch {
        // Keep as string if not valid JSON
      }
    }

    return result as unknown as SkillFrontmatter;
  } catch {
    return null;
  }
}

/** Extract body content after frontmatter */
function extractBody(content: string): string {
  const match = content.match(/^---\n[\s\S]*?\n---\n([\s\S]*)$/);
  return match ? match[1]! : content;
}

// ─── Feature ─────────────────────────────────────────────────────────────────

class SkillsLoaderFeature {
  readonly meta = {
    name: 'skills-loader',
    version: '0.3.0',
    description: 'Load OpenClaw skills with Hermes-style progressive disclosure',
    dependencies: [],
  };

  private ctx: any = null;
  private skillsDir: string = '';
  // Cache for full skill data (lazy loaded at Level 1)
  private skillsCache: Map<string, SkillMeta> = new Map();

  async init(config: Record<string, unknown>, context: any): Promise<void> {
    this.ctx = context;
    this.skillsDir =
      (config['skillsDir'] as string) ||
      path.join(process.env.HOME || '~', '.openclaw', 'workspace', 'skills');

    // Initial scan - just count skills (Level 0 is lightweight)
    await this.scanSkillsCount();
  }

  private async scanSkillsCount(): Promise<void> {
    if (!fs.existsSync(this.skillsDir)) {
      this.ctx?.logger?.warn?.('Skills directory not found', { dir: this.skillsDir });
      return;
    }

    const entries = fs.readdirSync(this.skillsDir, { withFileTypes: true });
    let count = 0;

    for (const entry of entries) {
      if (!entry.isDirectory()) continue;
      const skillMdPath = path.join(this.skillsDir, entry.name, 'SKILL.md');
      if (fs.existsSync(skillMdPath)) {
        count++;
      }
    }

    this.ctx?.logger?.info?.(`Found ${count} skills in ${this.skillsDir}`);
  }

  async start(): Promise<void> {}

  async stop(): Promise<void> {}

  async healthCheck(): Promise<{ healthy: boolean; details: Record<string, unknown> }> {
    const count = this.skillsCache.size;
    return {
      healthy: true,
      details: {
        skillsDir: this.skillsDir,
        skillCount: count,
      },
    };
  }

  // ══════════════════════════════════════════════════════════════════════════
  // PROGRESSIVE DISCLOSURE API (Hermes-style)
  // ══════════════════════════════════════════════════════════════════════════

  /**
   * Level 0: Lightweight list of all skills (name, description, category only)
   * This is ~3k tokens for 100 skills - minimal token cost
   */
  skillsList(): SkillSummary[] {
    if (!fs.existsSync(this.skillsDir)) return [];

    const entries = fs.readdirSync(this.skillsDir, { withFileTypes: true });
    const summaries: SkillSummary[] = [];

    for (const entry of entries) {
      if (!entry.isDirectory()) continue;

      const skillPath = path.join(this.skillsDir, entry.name);
      const skillMdPath = path.join(skillPath, 'SKILL.md');
      const referencesDir = path.join(skillPath, 'references');
      const scriptsDir = path.join(skillPath, 'scripts');

      if (!fs.existsSync(skillMdPath)) continue;

      try {
        const content = fs.readFileSync(skillMdPath, 'utf8');
        const frontmatter = parseFrontmatter(content);
        const body = extractBody(content);

        // Extract description from frontmatter or first paragraph
        let description = frontmatter?.description || '';
        if (!description) {
          // Fallback to first non-header, non-empty line
          const lines = body.split('\n');
          for (const line of lines) {
            const trimmed = line.trim();
            if (trimmed && !trimmed.startsWith('#')) {
              description = trimmed.slice(0, 200);
              break;
            }
          }
        }

        // Get category from various possible locations
        let category = 'general';
        if (frontmatter?.metadata?.hermes?.category) {
          category = frontmatter.metadata.hermes.category;
        } else if (frontmatter?.metadata?.clawdbot?.category) {
          category = frontmatter.metadata.clawdbot.category;
        } else if (frontmatter?.category) {
          category = frontmatter.category;
        }

        summaries.push({
          name: frontmatter?.name || entry.name,
          description: description.slice(0, 200),
          category,
          version: frontmatter?.version || '1.0.0',
          platforms: frontmatter?.platforms || [],
          hasReferences: fs.existsSync(referencesDir),
          hasScripts: fs.existsSync(scriptsDir),
        });
      } catch {
        // Skip malformed skills
      }
    }

    return summaries.sort((a, b) => a.name.localeCompare(b.name));
  }

  /**
   * Level 1: Full skill view - loads complete SKILL.md + metadata
   */
  skillView(name: string): SkillMeta | null {
    // Check cache first
    if (this.skillsCache.has(name)) {
      return this.skillsCache.get(name)!;
    }

    const skillPath = path.join(this.skillsDir, name);
    const skillMdPath = path.join(skillPath, 'SKILL.md');

    if (!fs.existsSync(skillMdPath)) return null;

    try {
      const content = fs.readFileSync(skillMdPath, 'utf8');
      const frontmatter = parseFrontmatter(content) || {
        name,
        description: '',
        version: '1.0.0',
      };
      const body = extractBody(content);

      // Extract description
      let description = frontmatter.description || '';
      if (!description) {
        const lines = body.split('\n');
        for (const line of lines) {
          const trimmed = line.trim();
          if (trimmed && !trimmed.startsWith('#')) {
            description = trimmed.slice(0, 200);
            break;
          }
        }
      }

      // Get category
      let category = 'general';
      if (frontmatter.metadata?.hermes?.category) {
        category = frontmatter.metadata.hermes.category;
      } else if (frontmatter.metadata?.clawdbot?.category) {
        category = frontmatter.metadata.clawdbot.category;
      } else if (frontmatter.category) {
        category = frontmatter.category;
      }

      // List scripts
      const scriptsDir = path.join(skillPath, 'scripts');
      const scripts: string[] = [];
      if (fs.existsSync(scriptsDir)) {
        scripts.push(
          ...fs
            .readdirSync(scriptsDir)
            .filter(
              (f) =>
                f.endsWith('.sh') || f.endsWith('.js') || f.endsWith('.py') || f.endsWith('.ts'),
            ),
        );
      }

      // List references
      const referencesDir = path.join(skillPath, 'references');
      const references: string[] = [];
      if (fs.existsSync(referencesDir)) {
        references.push(
          ...fs
            .readdirSync(referencesDir)
            .filter((f) => f.endsWith('.md') || f.endsWith('.txt') || f.endsWith('.json')),
        );
      }

      // List templates
      const templatesDir = path.join(skillPath, 'templates');
      const templates: string[] = [];
      if (fs.existsSync(templatesDir)) {
        templates.push(...fs.readdirSync(templatesDir));
      }

      const meta: SkillMeta = {
        name: frontmatter.name || name,
        description: description.slice(0, 200),
        category,
        version: frontmatter.version || '1.0.0',
        platforms: frontmatter.platforms || [],
        hasReferences: references.length > 0,
        hasScripts: scripts.length > 0,
        path: skillPath,
        fullContent: content,
        frontmatter,
        scripts,
        references,
        templates,
      };

      // Cache for subsequent Level 1 calls
      this.skillsCache.set(name, meta);
      return meta;
    } catch {
      return null;
    }
  }

  /**
   * Level 2: View specific reference file within a skill
   */
  skillViewRef(name: string, refPath: string): string | null {
    const skillPath = path.join(this.skillsDir, name);
    const fullPath = path.join(skillPath, refPath);

    // Security: ensure path is within skill directory
    if (!fullPath.startsWith(skillPath)) return null;

    if (!fs.existsSync(fullPath)) return null;

    try {
      return fs.readFileSync(fullPath, 'utf8');
    } catch {
      return null;
    }
  }

  // ══════════════════════════════════════════════════════════════════════════
  // LEGACY API (kept for backward compatibility)
  // ══════════════════════════════════════════════════════════════════════════

  listSkills(): SkillSummary[] {
    return this.skillsList();
  }

  getSkill(name: string): SkillMeta | undefined {
    return this.skillView(name) ?? undefined;
  }

  getSkillMd(name: string): string | null {
    const skill = this.skillView(name);
    return skill?.fullContent ?? null;
  }

  getScripts(name: string): string[] {
    const skill = this.skillView(name);
    return skill?.scripts ?? [];
  }

  runScript(skillName: string, scriptName: string, args: string[] = []): string {
    const skill = this.skillView(skillName);
    if (!skill) throw new Error(`Skill '${skillName}' not found`);

    const scriptPath = path.join(skill.path, 'scripts', scriptName);
    if (!fs.existsSync(scriptPath))
      throw new Error(`Script '${scriptName}' not found in skill '${skillName}'`);

    try {
      let command: string;
      if (scriptName.endsWith('.sh')) {
        command = `bash "${scriptPath}" ${args.map((a) => `"${a}"`).join(' ')}`;
      } else if (scriptName.endsWith('.js')) {
        command = `node "${scriptPath}" ${args.map((a) => `"${a}"`).join(' ')}`;
      } else if (scriptName.endsWith('.py')) {
        command = `python3 "${scriptPath}" ${args.map((a) => `"${a}"`).join(' ')}`;
      } else if (scriptName.endsWith('.ts')) {
        command = `npx tsx "${scriptPath}" ${args.map((a) => `"${a}"`).join(' ')}`;
      } else {
        throw new Error(`Unsupported script type: ${scriptName}`);
      }

      /* nosemgrep: javascript.lang.security.detect-child-process.detect-child-process */
      return execSync(command, {
        cwd: skill.path,
        timeout: 30000,
        encoding: 'utf8',
      });
    } catch (err: any) {
      throw new Error(`Script failed: ${err.message}`);
    }
  }

  searchSkills(query: string): SkillSummary[] {
    const q = query.toLowerCase();
    return this.skillsList().filter(
      (s) => s.name.toLowerCase().includes(q) || s.description.toLowerCase().includes(q),
    );
  }

  getManifest(name: string): SkillManifest | null {
    const skill = this.skillView(name);
    if (!skill) return null;

    return {
      name: skill.name,
      version: skill.version,
      description: skill.description,
      scripts: skill.scripts,
    };
  }

  /** Get list of all skill directories (raw, for advanced use) */
  getSkillPaths(): string[] {
    if (!fs.existsSync(this.skillsDir)) return [];

    return fs
      .readdirSync(this.skillsDir, { withFileTypes: true })
      .filter((e) => e.isDirectory())
      .map((e) => path.join(this.skillsDir, e.name));
  }
}

export default new SkillsLoaderFeature();
