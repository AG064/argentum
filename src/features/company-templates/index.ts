/**
 * Company Templates Feature
 *
 * Export and import company/organization configurations as portable JSON bundles.
 * Scrubs secrets automatically. Supports versioned template bundles.
 */

import { mkdirSync, existsSync, readdirSync, readFileSync, writeFileSync, unlinkSync } from 'fs';
import { resolve, join } from 'path';

import {
  type FeatureModule,
  type FeatureContext,
  type FeatureMeta,
  type HealthStatus,
} from '../../core/plugin-loader';

// ─── Interfaces ──────────────────────────────────────────────────────────────

export interface TemplateBundle {
  version: string;
  name: string;
  exportedAt: string;
  organization: OrganizationConfig;
  agents: AgentConfig[];
  goals: GoalConfig[];
  skills: SkillConfig[];
  workflows: WorkflowConfig[];
  metadata: Record<string, unknown>;
}

export interface OrganizationConfig {
  name: string;
  timezone: string;
  locale: string;
  settings: Record<string, unknown>;
}

export interface AgentConfig {
  name: string;
  role: string;
  model: string;
  systemPrompt: string;
  tools: string[];
  enabled: boolean;
}

export interface GoalConfig {
  title: string;
  description: string;
  status: string;
  metrics: Record<string, unknown>;
}

export interface SkillConfig {
  name: string;
  version: string;
  enabled: boolean;
  config: Record<string, unknown>;
}

export interface WorkflowConfig {
  name: string;
  trigger: string;
  steps: Array<Record<string, unknown>>;
  enabled: boolean;
}

export interface TemplateInfo {
  name: string;
  version: string;
  exportedAt: string;
  size: number;
  agentCount: number;
  goalCount: number;
}

interface TemplateStorage {
  name: string;
  path: string;
  savedAt: number;
}

// ─── Secrets Patterns ────────────────────────────────────────────────────────

const SECRET_PATTERNS = [
  /api[_-]?key/i,
  /secret/i,
  /password/i,
  /token/i,
  /credential/i,
  /private[_-]?key/i,
  /auth/i,
  /bearer/i,
  /sk-[a-zA-Z0-9]+/,
  /ghp_[a-zA-Z0-9]+/,
  /gho_[a-zA-Z0-9]+/,
];

function isSecretKey(key: string): boolean {
  return SECRET_PATTERNS.some((p) => p.test(key));
}

function scrubSecrets(obj: unknown): unknown {
  if (typeof obj === 'string') {
    // Check if the value itself looks like a secret
    if (/^(sk-|ghp_|gho_|xoxb-|xoxp-)/.test(obj)) {
      return '***REDACTED***';
    }
    return obj;
  }

  if (Array.isArray(obj)) {
    return obj.map(scrubSecrets);
  }

  if (typeof obj === 'object' && obj !== null) {
    const result: Record<string, unknown> = {};
    for (const [key, value] of Object.entries(obj)) {
      if (isSecretKey(key)) {
        result[key] = '***REDACTED***';
      } else {
        result[key] = scrubSecrets(value);
      }
    }
    return result;
  }

  return obj;
}

// ─── Feature ─────────────────────────────────────────────────────────────────

class CompanyTemplatesFeature implements FeatureModule {
  readonly meta: FeatureMeta = {
    name: 'company-templates',
    version: '0.0.5',
    description: 'Portable company configuration templates with secret scrubbing',
    dependencies: [],
  };

  private config: { enabled: boolean; templatesPath: string } = {
    enabled: false,
    templatesPath: './data/templates',
  };
  private ctx!: FeatureContext;
  private templates: Map<string, TemplateStorage> = new Map();

  async init(config: Record<string, unknown>, context: FeatureContext): Promise<void> {
    this.ctx = context;
    this.config = { ...this.config, ...(config as Partial<typeof this.config>) };
    this.ensureTemplatesDir();
    this.loadExistingTemplates();
  }

  async start(): Promise<void> {
    this.ctx.logger.info('Company templates active', {
      path: this.config.templatesPath,
      templates: this.templates.size,
    });
  }

  async stop(): Promise<void> {
    // No cleanup needed
  }

  async healthCheck(): Promise<HealthStatus> {
    return {
      healthy: existsSync(this.config.templatesPath),
      details: {
        templateCount: this.templates.size,
        path: this.config.templatesPath,
      },
    };
  }

  // ─── Public API ──────────────────────────────────────────────────────────

  /** Export a company configuration as a portable template bundle */
  async exportCompany(name: string): Promise<TemplateBundle> {
    const ctx = this.ctx;
    const config = ctx.config;

    const bundle: TemplateBundle = {
      version: '0.0.5',
      name,
      exportedAt: new Date().toISOString(),
      organization: {
        name,
        timezone: ((config as Record<string, unknown>)['timezone'] as string) ?? 'UTC',
        locale: ((config as Record<string, unknown>)['locale'] as string) ?? 'en',
        settings: scrubSecrets(
          ((config as Record<string, unknown>)['organization'] as Record<string, unknown>) ?? {},
        ) as Record<string, unknown>,
      },
      agents: this.extractAgents(config),
      goals: [], // Would be populated from goals feature
      skills: this.extractSkills(config),
      workflows: [],
      metadata: {
        agClawVersion: '0.0.5',
        exportedBy: 'company-templates',
      },
    };

    // Scrub all secrets from the bundle
    const scrubbed = scrubSecrets(bundle) as TemplateBundle;

    this.ctx.logger.info('Company exported', { name, size: JSON.stringify(scrubbed).length });
    return scrubbed;
  }

  /** Import a company template bundle */
  async importCompany(bundle: TemplateBundle, name: string): Promise<void> {
    // Validate bundle
    if (!bundle.version || !bundle.organization) {
      throw new Error('Invalid template bundle: missing required fields');
    }

    // Save to disk
    const fileName = `${this.sanitizeFileName(name)}.json`;
    const filePath = join(this.config.templatesPath, fileName);

    const importBundle = {
      ...bundle,
      name,
      importedAt: new Date().toISOString(),
    };

    writeFileSync(filePath, JSON.stringify(importBundle, null, 2), 'utf-8');

    this.templates.set(name, {
      name,
      path: filePath,
      savedAt: Date.now(),
    });

    this.ctx.logger.info('Company imported', { name, version: bundle.version });
  }

  /** List all saved templates */
  async listTemplates(): Promise<TemplateInfo[]> {
    const infos: TemplateInfo[] = [];

    for (const [, tmpl] of this.templates) {
      try {
        const content = readFileSync(tmpl.path, 'utf-8');
        const bundle = JSON.parse(content) as TemplateBundle;

        infos.push({
          name: bundle.name ?? tmpl.name,
          version: bundle.version ?? 'unknown',
          exportedAt: bundle.exportedAt ?? new Date(tmpl.savedAt).toISOString(),
          size: content.length,
          agentCount: bundle.agents?.length ?? 0,
          goalCount: bundle.goals?.length ?? 0,
        });
      } catch {
        // Skip corrupted templates
      }
    }

    return infos.sort((a, b) => a.name.localeCompare(b.name));
  }

  /** Get a specific template bundle */
  async getTemplate(name: string): Promise<TemplateBundle | null> {
    const tmpl = this.templates.get(name);
    if (!tmpl) return null;

    try {
      const content = readFileSync(tmpl.path, 'utf-8');
      return JSON.parse(content) as TemplateBundle;
    } catch {
      return null;
    }
  }

  /** Delete a template */
  async deleteTemplate(name: string): Promise<boolean> {
    const tmpl = this.templates.get(name);
    if (!tmpl) return false;

    try {
      unlinkSync(tmpl.path);
      this.templates.delete(name);
      this.ctx.logger.info('Template deleted', { name });
      return true;
    } catch {
      return false;
    }
  }

  /** Export template to a portable file path */
  async exportToFile(name: string, outputPath: string): Promise<void> {
    const bundle = await this.exportCompany(name);
    const dir = dirname(outputPath);
    if (!existsSync(dir)) {
      mkdirSync(dir, { recursive: true });
    }
    writeFileSync(outputPath, JSON.stringify(bundle, null, 2), 'utf-8');
    this.ctx.logger.info('Template exported to file', { name, outputPath });
  }

  /** Import template from a file path */
  async importFromFile(filePath: string, name?: string): Promise<void> {
    const content = readFileSync(filePath, 'utf-8');
    const bundle = JSON.parse(content) as TemplateBundle;
    const templateName = name ?? bundle.name ?? 'imported';
    await this.importCompany(bundle, templateName);
  }

  // ─── Private helpers ─────────────────────────────────────────────────────

  private ensureTemplatesDir(): void {
    const fullPath = resolve(this.config.templatesPath);
    if (!existsSync(fullPath)) {
      mkdirSync(fullPath, { recursive: true });
    }
  }

  private loadExistingTemplates(): void {
    const fullPath = resolve(this.config.templatesPath);
    if (!existsSync(fullPath)) return;

    const files = readdirSync(fullPath).filter((f) => f.endsWith('.json'));
    for (const file of files) {
      const name = file.replace('.json', '');
      this.templates.set(name, {
        name,
        path: join(fullPath, file),
        savedAt: Date.now(),
      });
    }
  }

  private sanitizeFileName(name: string): string {
    return name.replace(/[^a-zA-Z0-9_-]/g, '_').toLowerCase();
  }

  private extractAgents(config: Record<string, unknown>): AgentConfig[] {
    const agents = config['agents'] as Record<string, unknown>[] | undefined;
    if (!agents) return [];

    return agents.map((a) => ({
      name: (a['name'] as string) ?? 'unnamed',
      role: (a['role'] as string) ?? 'assistant',
      model: (a['model'] as string) ?? 'default',
      systemPrompt: '***REDACTED***',
      tools: (a['tools'] as string[]) ?? [],
      enabled: (a['enabled'] as boolean) ?? true,
    }));
  }

  private extractSkills(config: Record<string, unknown>): SkillConfig[] {
    const features = config['features'] as Record<string, Record<string, unknown>> | undefined;
    if (!features) return [];

    return Object.entries(features).map(([name, cfg]) => ({
      name,
      version: (cfg['version'] as string) ?? '0.0.5',
      enabled: (cfg['enabled'] as boolean) ?? false,
      config: scrubSecrets(cfg) as Record<string, unknown>,
    }));
  }
}

// Helper for dirname import
function dirname(path: string): string {
  return path.replace(/[/\\][^/\\]*$/, '');
}

export default new CompanyTemplatesFeature();
