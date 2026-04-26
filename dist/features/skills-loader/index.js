"use strict";
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
var __createBinding = (this && this.__createBinding) || (Object.create ? (function(o, m, k, k2) {
    if (k2 === undefined) k2 = k;
    var desc = Object.getOwnPropertyDescriptor(m, k);
    if (!desc || ("get" in desc ? !m.__esModule : desc.writable || desc.configurable)) {
      desc = { enumerable: true, get: function() { return m[k]; } };
    }
    Object.defineProperty(o, k2, desc);
}) : (function(o, m, k, k2) {
    if (k2 === undefined) k2 = k;
    o[k2] = m[k];
}));
var __setModuleDefault = (this && this.__setModuleDefault) || (Object.create ? (function(o, v) {
    Object.defineProperty(o, "default", { enumerable: true, value: v });
}) : function(o, v) {
    o["default"] = v;
});
var __importStar = (this && this.__importStar) || (function () {
    var ownKeys = function(o) {
        ownKeys = Object.getOwnPropertyNames || function (o) {
            var ar = [];
            for (var k in o) if (Object.prototype.hasOwnProperty.call(o, k)) ar[ar.length] = k;
            return ar;
        };
        return ownKeys(o);
    };
    return function (mod) {
        if (mod && mod.__esModule) return mod;
        var result = {};
        if (mod != null) for (var k = ownKeys(mod), i = 0; i < k.length; i++) if (k[i] !== "default") __createBinding(result, mod, k[i]);
        __setModuleDefault(result, mod);
        return result;
    };
})();
Object.defineProperty(exports, "__esModule", { value: true });
const child_process_1 = require("child_process");
const fs = __importStar(require("fs"));
const path = __importStar(require("path"));
// ─── YAML Frontmatter Parser ─────────────────────────────────────────────────
/**
 * Parse YAML frontmatter from SKILL.md content.
 * Matches Hermes/agentskills.io format:
 * ---
 * name: my-skill
 * description: Brief description
 * version: 0.0.1
 * ---
 */
function parseFrontmatter(content) {
    const match = content.match(/^---\n([\s\S]*?)\n---\n/);
    if (!match)
        return null;
    try {
        // Simple YAML parser for frontmatter keys
        const lines = match[1].split('\n');
        const result = {};
        for (const line of lines) {
            const colonIdx = line.indexOf(':');
            if (colonIdx === -1)
                continue;
            const key = line.slice(0, colonIdx).trim();
            const value = line.slice(colonIdx + 1).trim();
            if (!key)
                continue;
            // Handle array values like [macos, linux]
            if (value.startsWith('[') && value.endsWith(']')) {
                result[key] = value
                    .slice(1, -1)
                    .split(',')
                    .map((s) => s.trim());
            }
            else if (value === '' || value === 'null' || value === '~') {
                // Skip empty/null values or continue to check for nested
            }
            else {
                result[key] = value.replace(/^["']|["']$/g, ''); // Strip quotes
            }
        }
        // Handle nested metadata.hermes object
        if (result['metadata']) {
            try {
                result['metadata'] = JSON.parse(result['metadata']);
            }
            catch {
                // Keep as string if not valid JSON
            }
        }
        return result;
    }
    catch {
        return null;
    }
}
/** Extract body content after frontmatter */
function extractBody(content) {
    const match = content.match(/^---\n[\s\S]*?\n---\n([\s\S]*)$/);
    return match ? match[1] : content;
}
// ─── Feature ─────────────────────────────────────────────────────────────────
class SkillsLoaderFeature {
    meta = {
        name: 'skills-loader',
        version: '0.0.1',
        description: 'Load OpenClaw skills with Hermes-style progressive disclosure',
        dependencies: [],
    };
    ctx = null;
    skillsDir = '';
    // Cache for full skill data (lazy loaded at Level 1)
    skillsCache = new Map();
    async init(config, context) {
        this.ctx = context;
        this.skillsDir =
            config['skillsDir'] ||
                path.join(process.env.HOME || '~', '.openclaw', 'workspace', 'skills');
        // Initial scan - just count skills (Level 0 is lightweight)
        await this.scanSkillsCount();
    }
    async scanSkillsCount() {
        if (!fs.existsSync(this.skillsDir)) {
            this.ctx?.logger?.warn?.('Skills directory not found', { dir: this.skillsDir });
            return;
        }
        const entries = fs.readdirSync(this.skillsDir, { withFileTypes: true });
        let count = 0;
        for (const entry of entries) {
            if (!entry.isDirectory())
                continue;
            const skillMdPath = path.join(this.skillsDir, entry.name, 'SKILL.md');
            if (fs.existsSync(skillMdPath)) {
                count++;
            }
        }
        this.ctx?.logger?.info?.(`Found ${count} skills in ${this.skillsDir}`);
    }
    async start() { }
    async stop() { }
    async healthCheck() {
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
    skillsList() {
        if (!fs.existsSync(this.skillsDir))
            return [];
        const entries = fs.readdirSync(this.skillsDir, { withFileTypes: true });
        const summaries = [];
        for (const entry of entries) {
            if (!entry.isDirectory())
                continue;
            const skillPath = path.join(this.skillsDir, entry.name);
            const skillMdPath = path.join(skillPath, 'SKILL.md');
            const referencesDir = path.join(skillPath, 'references');
            const scriptsDir = path.join(skillPath, 'scripts');
            if (!fs.existsSync(skillMdPath))
                continue;
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
                }
                else if (frontmatter?.metadata?.clawdbot?.category) {
                    category = frontmatter.metadata.clawdbot.category;
                }
                else if (frontmatter?.category) {
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
            }
            catch {
                // Skip malformed skills
            }
        }
        return summaries.sort((a, b) => a.name.localeCompare(b.name));
    }
    /**
     * Level 1: Full skill view - loads complete SKILL.md + metadata
     */
    skillView(name) {
        // Check cache first
        if (this.skillsCache.has(name)) {
            return this.skillsCache.get(name);
        }
        const skillPath = path.join(this.skillsDir, name);
        const skillMdPath = path.join(skillPath, 'SKILL.md');
        if (!fs.existsSync(skillMdPath))
            return null;
        try {
            const content = fs.readFileSync(skillMdPath, 'utf8');
            const frontmatter = parseFrontmatter(content) || {
                name,
                description: '',
                version: '0.0.1',
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
            }
            else if (frontmatter.metadata?.clawdbot?.category) {
                category = frontmatter.metadata.clawdbot.category;
            }
            else if (frontmatter.category) {
                category = frontmatter.category;
            }
            // List scripts
            const scriptsDir = path.join(skillPath, 'scripts');
            const scripts = [];
            if (fs.existsSync(scriptsDir)) {
                scripts.push(...fs
                    .readdirSync(scriptsDir)
                    .filter((f) => f.endsWith('.sh') || f.endsWith('.js') || f.endsWith('.py') || f.endsWith('.ts')));
            }
            // List references
            const referencesDir = path.join(skillPath, 'references');
            const references = [];
            if (fs.existsSync(referencesDir)) {
                references.push(...fs
                    .readdirSync(referencesDir)
                    .filter((f) => f.endsWith('.md') || f.endsWith('.txt') || f.endsWith('.json')));
            }
            // List templates
            const templatesDir = path.join(skillPath, 'templates');
            const templates = [];
            if (fs.existsSync(templatesDir)) {
                templates.push(...fs.readdirSync(templatesDir));
            }
            const meta = {
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
        }
        catch {
            return null;
        }
    }
    /**
     * Level 2: View specific reference file within a skill
     */
    skillViewRef(name, refPath) {
        const skillPath = path.join(this.skillsDir, name);
        const fullPath = path.join(skillPath, refPath);
        // Security: ensure path is within skill directory
        if (!fullPath.startsWith(skillPath))
            return null;
        if (!fs.existsSync(fullPath))
            return null;
        try {
            return fs.readFileSync(fullPath, 'utf8');
        }
        catch {
            return null;
        }
    }
    // ══════════════════════════════════════════════════════════════════════════
    // LEGACY API (kept for backward compatibility)
    // ══════════════════════════════════════════════════════════════════════════
    listSkills() {
        return this.skillsList();
    }
    getSkill(name) {
        return this.skillView(name) ?? undefined;
    }
    getSkillMd(name) {
        const skill = this.skillView(name);
        return skill?.fullContent ?? null;
    }
    getScripts(name) {
        const skill = this.skillView(name);
        return skill?.scripts ?? [];
    }
    runScript(skillName, scriptName, args = []) {
        const skill = this.skillView(skillName);
        if (!skill)
            throw new Error(`Skill '${skillName}' not found`);
        const scriptsDir = path.resolve(skill.path, 'scripts');
        const scriptPath = path.resolve(scriptsDir, scriptName);
        if (!scriptPath.startsWith(`${scriptsDir}${path.sep}`)) {
            throw new Error(`Invalid script path for skill '${skillName}'`);
        }
        if (!fs.existsSync(scriptPath))
            throw new Error(`Script '${scriptName}' not found in skill '${skillName}'`);
        try {
            let command;
            let commandArgs;
            if (scriptName.endsWith('.sh')) {
                command = 'bash';
                commandArgs = [scriptPath, ...args];
            }
            else if (scriptName.endsWith('.js')) {
                command = 'node';
                commandArgs = [scriptPath, ...args];
            }
            else if (scriptName.endsWith('.py')) {
                command = process.platform === 'win32' ? 'python' : 'python3';
                commandArgs = [scriptPath, ...args];
            }
            else if (scriptName.endsWith('.ts')) {
                command = process.platform === 'win32' ? 'npx.cmd' : 'npx';
                commandArgs = ['tsx', scriptPath, ...args];
            }
            else {
                throw new Error(`Unsupported script type: ${scriptName}`);
            }
            return (0, child_process_1.execFileSync)(command, commandArgs, {
                cwd: skill.path,
                timeout: 30000,
                encoding: 'utf8',
            });
        }
        catch (err) {
            throw new Error(`Script failed: ${err.message}`);
        }
    }
    searchSkills(query) {
        const q = query.toLowerCase();
        return this.skillsList().filter((s) => s.name.toLowerCase().includes(q) || s.description.toLowerCase().includes(q));
    }
    getManifest(name) {
        const skill = this.skillView(name);
        if (!skill)
            return null;
        return {
            name: skill.name,
            version: skill.version,
            description: skill.description,
            scripts: skill.scripts,
        };
    }
    /** Get list of all skill directories (raw, for advanced use) */
    getSkillPaths() {
        if (!fs.existsSync(this.skillsDir))
            return [];
        return fs
            .readdirSync(this.skillsDir, { withFileTypes: true })
            .filter((e) => e.isDirectory())
            .map((e) => path.join(this.skillsDir, e.name));
    }
}
exports.default = new SkillsLoaderFeature();
//# sourceMappingURL=index.js.map