"use strict";
/**
 * Company Templates Feature
 *
 * Export and import company/organization configurations as portable JSON bundles.
 * Scrubs secrets automatically. Supports versioned template bundles.
 */
Object.defineProperty(exports, "__esModule", { value: true });
const fs_1 = require("fs");
const path_1 = require("path");
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
function isSecretKey(key) {
    return SECRET_PATTERNS.some((p) => p.test(key));
}
function scrubSecrets(obj) {
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
        const result = {};
        for (const [key, value] of Object.entries(obj)) {
            if (isSecretKey(key)) {
                result[key] = '***REDACTED***';
            }
            else {
                result[key] = scrubSecrets(value);
            }
        }
        return result;
    }
    return obj;
}
// ─── Feature ─────────────────────────────────────────────────────────────────
class CompanyTemplatesFeature {
    meta = {
        name: 'company-templates',
        version: '0.0.4',
        description: 'Portable company configuration templates with secret scrubbing',
        dependencies: [],
    };
    config = {
        enabled: false,
        templatesPath: './data/templates',
    };
    ctx;
    templates = new Map();
    async init(config, context) {
        this.ctx = context;
        this.config = { ...this.config, ...config };
        this.ensureTemplatesDir();
        this.loadExistingTemplates();
    }
    async start() {
        this.ctx.logger.info('Company templates active', {
            path: this.config.templatesPath,
            templates: this.templates.size,
        });
    }
    async stop() {
        // No cleanup needed
    }
    async healthCheck() {
        return {
            healthy: (0, fs_1.existsSync)(this.config.templatesPath),
            details: {
                templateCount: this.templates.size,
                path: this.config.templatesPath,
            },
        };
    }
    // ─── Public API ──────────────────────────────────────────────────────────
    /** Export a company configuration as a portable template bundle */
    async exportCompany(name) {
        const ctx = this.ctx;
        const config = ctx.config;
        const bundle = {
            version: '0.0.4',
            name,
            exportedAt: new Date().toISOString(),
            organization: {
                name,
                timezone: config['timezone'] ?? 'UTC',
                locale: config['locale'] ?? 'en',
                settings: scrubSecrets(config['organization'] ?? {}),
            },
            agents: this.extractAgents(config),
            goals: [], // Would be populated from goals feature
            skills: this.extractSkills(config),
            workflows: [],
            metadata: {
                agClawVersion: '0.0.4',
                exportedBy: 'company-templates',
            },
        };
        // Scrub all secrets from the bundle
        const scrubbed = scrubSecrets(bundle);
        this.ctx.logger.info('Company exported', { name, size: JSON.stringify(scrubbed).length });
        return scrubbed;
    }
    /** Import a company template bundle */
    async importCompany(bundle, name) {
        // Validate bundle
        if (!bundle.version || !bundle.organization) {
            throw new Error('Invalid template bundle: missing required fields');
        }
        // Save to disk
        const fileName = `${this.sanitizeFileName(name)}.json`;
        const filePath = (0, path_1.join)(this.config.templatesPath, fileName);
        const importBundle = {
            ...bundle,
            name,
            importedAt: new Date().toISOString(),
        };
        (0, fs_1.writeFileSync)(filePath, JSON.stringify(importBundle, null, 2), 'utf-8');
        this.templates.set(name, {
            name,
            path: filePath,
            savedAt: Date.now(),
        });
        this.ctx.logger.info('Company imported', { name, version: bundle.version });
    }
    /** List all saved templates */
    async listTemplates() {
        const infos = [];
        for (const [, tmpl] of this.templates) {
            try {
                const content = (0, fs_1.readFileSync)(tmpl.path, 'utf-8');
                const bundle = JSON.parse(content);
                infos.push({
                    name: bundle.name ?? tmpl.name,
                    version: bundle.version ?? 'unknown',
                    exportedAt: bundle.exportedAt ?? new Date(tmpl.savedAt).toISOString(),
                    size: content.length,
                    agentCount: bundle.agents?.length ?? 0,
                    goalCount: bundle.goals?.length ?? 0,
                });
            }
            catch {
                // Skip corrupted templates
            }
        }
        return infos.sort((a, b) => a.name.localeCompare(b.name));
    }
    /** Get a specific template bundle */
    async getTemplate(name) {
        const tmpl = this.templates.get(name);
        if (!tmpl)
            return null;
        try {
            const content = (0, fs_1.readFileSync)(tmpl.path, 'utf-8');
            return JSON.parse(content);
        }
        catch {
            return null;
        }
    }
    /** Delete a template */
    async deleteTemplate(name) {
        const tmpl = this.templates.get(name);
        if (!tmpl)
            return false;
        try {
            (0, fs_1.unlinkSync)(tmpl.path);
            this.templates.delete(name);
            this.ctx.logger.info('Template deleted', { name });
            return true;
        }
        catch {
            return false;
        }
    }
    /** Export template to a portable file path */
    async exportToFile(name, outputPath) {
        const bundle = await this.exportCompany(name);
        const dir = dirname(outputPath);
        if (!(0, fs_1.existsSync)(dir)) {
            (0, fs_1.mkdirSync)(dir, { recursive: true });
        }
        (0, fs_1.writeFileSync)(outputPath, JSON.stringify(bundle, null, 2), 'utf-8');
        this.ctx.logger.info('Template exported to file', { name, outputPath });
    }
    /** Import template from a file path */
    async importFromFile(filePath, name) {
        const content = (0, fs_1.readFileSync)(filePath, 'utf-8');
        const bundle = JSON.parse(content);
        const templateName = name ?? bundle.name ?? 'imported';
        await this.importCompany(bundle, templateName);
    }
    // ─── Private helpers ─────────────────────────────────────────────────────
    ensureTemplatesDir() {
        const fullPath = (0, path_1.resolve)(this.config.templatesPath);
        if (!(0, fs_1.existsSync)(fullPath)) {
            (0, fs_1.mkdirSync)(fullPath, { recursive: true });
        }
    }
    loadExistingTemplates() {
        const fullPath = (0, path_1.resolve)(this.config.templatesPath);
        if (!(0, fs_1.existsSync)(fullPath))
            return;
        const files = (0, fs_1.readdirSync)(fullPath).filter((f) => f.endsWith('.json'));
        for (const file of files) {
            const name = file.replace('.json', '');
            this.templates.set(name, {
                name,
                path: (0, path_1.join)(fullPath, file),
                savedAt: Date.now(),
            });
        }
    }
    sanitizeFileName(name) {
        return name.replace(/[^a-zA-Z0-9_-]/g, '_').toLowerCase();
    }
    extractAgents(config) {
        const agents = config['agents'];
        if (!agents)
            return [];
        return agents.map((a) => ({
            name: a['name'] ?? 'unnamed',
            role: a['role'] ?? 'assistant',
            model: a['model'] ?? 'default',
            systemPrompt: '***REDACTED***',
            tools: a['tools'] ?? [],
            enabled: a['enabled'] ?? true,
        }));
    }
    extractSkills(config) {
        const features = config['features'];
        if (!features)
            return [];
        return Object.entries(features).map(([name, cfg]) => ({
            name,
            version: cfg['version'] ?? '0.0.4',
            enabled: cfg['enabled'] ?? false,
            config: scrubSecrets(cfg),
        }));
    }
}
// Helper for dirname import
function dirname(path) {
    return path.replace(/[/\\][^/\\]*$/, '');
}
exports.default = new CompanyTemplatesFeature();
//# sourceMappingURL=index.js.map