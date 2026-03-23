/**
 * AG-Claw Skills Loader
 *
 * Loads OpenClaw skills into AG-Claw ecosystem.
 * Any SKILL.md in ~/.openclaw/workspace/skills/ becomes available.
 */

import { execSync } from 'child_process';
import * as fs from 'fs';
import * as path from 'path';

// ─── Types ───────────────────────────────────────────────────────────────────

interface SkillMeta {
  name: string;
  path: string;
  description: string;
  hasScripts: boolean;
  scriptCount: number;
  installDate?: string;
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

// ─── Feature ─────────────────────────────────────────────────────────────────

class SkillsLoaderFeature {
  readonly meta = {
    name: 'skills-loader',
    version: '0.2.0',
    description: 'Load OpenClaw skills into AG-Claw',
    dependencies: [],
  };

  private ctx: any = null;
  private skillsDir: string = '';
  private skills: Map<string, SkillMeta> = new Map();

  async init(config: Record<string, unknown>, context: any): Promise<void> {
    this.ctx = context;
    this['skillsDir'] = (config['skillsDir'] as string) ||
      path.join(process.env.HOME || '~', '.openclaw', 'workspace', 'skills');

    this.scanSkills();
  }

  private scanSkills(): void {
    if (!fs.existsSync(this.skillsDir)) {
      this.ctx?.logger?.warn?.('Skills directory not found', { dir: this.skillsDir });
      return;
    }

    const entries = fs.readdirSync(this.skillsDir, { withFileTypes: true });

    for (const entry of entries) {
      if (!entry.isDirectory()) continue;

      const skillPath = path.join(this.skillsDir, entry.name);
      const skillMdPath = path.join(skillPath, 'SKILL.md');
      const scriptsDir = path.join(skillPath, 'scripts');

      // Read description from SKILL.md
      let description = '';
      if (fs.existsSync(skillMdPath)) {
        const content = fs.readFileSync(skillMdPath, 'utf8');
        // Extract first paragraph or description
        const lines = content.split('\n');
        for (const line of lines) {
          const trimmed = line.trim();
          if (trimmed && !trimmed.startsWith('#') && !trimmed.startsWith('---')) {
            description = trimmed.slice(0, 200);
            break;
          }
        }
      }

      // Count scripts
      let scriptCount = 0;
      let hasScripts = false;
      if (fs.existsSync(scriptsDir)) {
        const scripts = fs.readdirSync(scriptsDir).filter(f =>
          f.endsWith('.sh') || f.endsWith('.js') || f.endsWith('.py') || f.endsWith('.ts')
        );
        scriptCount = scripts.length;
        hasScripts = scriptCount > 0;
      }

      this.skills.set(entry.name, {
        name: entry.name,
        path: skillPath,
        description,
        hasScripts,
        scriptCount,
      });
    }

    this.ctx?.logger?.info?.(`Loaded ${this.skills.size} skills from ${this.skillsDir}`);
  }

  async start(): Promise<void> {}

  async stop(): Promise<void> {}

  async healthCheck(): Promise<{ healthy: boolean; details: Record<string, unknown> }> {
    return {
      healthy: true,
      details: {
        skillsDir: this.skillsDir,
        skillCount: this.skills.size,
        skillsWithScripts: [...this.skills.values()].filter(s => s.hasScripts).length,
      },
    };
  }

  // ─── Public API ──────────────────────────────────────────────────────────

  listSkills(): SkillMeta[] {
    return [...this.skills.values()].sort((a, b) => a.name.localeCompare(b.name));
  }

  getSkill(name: string): SkillMeta | undefined {
    return this.skills.get(name);
  }

  getSkillMd(name: string): string | null {
    const skill = this.skills.get(name);
    if (!skill) return null;
    const mdPath = path.join(skill.path, 'SKILL.md');
    if (fs.existsSync(mdPath)) {
      return fs.readFileSync(mdPath, 'utf8');
    }
    return null;
  }

  getScripts(name: string): string[] {
    const skill = this.skills.get(name);
    if (!skill) return [];
    const scriptsDir = path.join(skill.path, 'scripts');
    if (!fs.existsSync(scriptsDir)) return [];
    return fs.readdirSync(scriptsDir).filter(f =>
      f.endsWith('.sh') || f.endsWith('.js') || f.endsWith('.py') || f.endsWith('.ts')
    );
  }

  runScript(skillName: string, scriptName: string, args: string[] = []): string {
    const skill = this.skills.get(skillName);
    if (!skill) throw new Error(`Skill '${skillName}' not found`);

    const scriptPath = path.join(skill.path, 'scripts', scriptName);
    if (!fs.existsSync(scriptPath)) throw new Error(`Script '${scriptName}' not found in skill '${skillName}'`);

    try {
      let command: string;
      if (scriptName.endsWith('.sh')) {
        command = `bash "${scriptPath}" ${args.map(a => `"${a}"`).join(' ')}`;
      } else if (scriptName.endsWith('.js')) {
        command = `node "${scriptPath}" ${args.map(a => `"${a}"`).join(' ')}`;
      } else if (scriptName.endsWith('.py')) {
        command = `python3 "${scriptPath}" ${args.map(a => `"${a}"`).join(' ')}`;
      } else if (scriptName.endsWith('.ts')) {
        command = `npx tsx "${scriptPath}" ${args.map(a => `"${a}"`).join(' ')}`;
      } else {
        throw new Error(`Unsupported script type: ${scriptName}`);
      }

      return execSync(command, {
        cwd: skill.path,
        timeout: 30000,
        encoding: 'utf8',
      });
    } catch (err: any) {
      throw new Error(`Script failed: ${err.message}`);
    }
  }

  searchSkills(query: string): SkillMeta[] {
    const q = query.toLowerCase();
    return this.listSkills().filter(s =>
      s.name.toLowerCase().includes(q) ||
      s.description.toLowerCase().includes(q)
    );
  }

  getManifest(name: string): SkillManifest | null {
    const skill = this.skills.get(name);
    if (!skill) return null;

    const manifest: SkillManifest = {
      name: skill.name,
      description: skill.description,
      scripts: this.getScripts(name),
    };

    // Try to read package.json if exists
    const pkgPath = path.join(skill.path, 'package.json');
    if (fs.existsSync(pkgPath)) {
      try {
        const pkg = JSON.parse(fs.readFileSync(pkgPath, 'utf8'));
        manifest.version = pkg.version;
        manifest.dependencies = pkg.dependencies ? Object.keys(pkg.dependencies) : [];
      } catch {}
    }

    return manifest;
  }
}

export default new SkillsLoaderFeature();
