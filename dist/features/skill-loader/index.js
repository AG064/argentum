"use strict";
/**
 * Argentum Skill Loader
 *
 * Loads SKILL.md files from Argentum feature directories and injects them
 * into agent context. This is the OpenFang "Hand" pattern - domain expertise
 * bundled with features.
 *
 * Unlike the global skills-loader (which loads from ~/.openclaw/workspace/skills/),
 * this feature loads SKILL.md from Argentum's own feature directories.
 */
Object.defineProperty(exports, "__esModule", { value: true });
exports.skillLoader = void 0;
const fs_1 = require("fs");
const path_1 = require("path");
// ─── Feature ─────────────────────────────────────────────────────────────────
class SkillLoaderFeature {
    meta = {
        name: 'skill-loader',
        version: '0.0.4',
        description: 'Loads SKILL.md from feature directories into agent context (OpenFang Hand pattern)',
        dependencies: [],
    };
    ctx = null;
    featuresPath = '';
    loadedSkills = [];
    async init(config, context) {
        this.ctx = context;
        this.featuresPath =
            config['featuresPath'] ||
                (0, path_1.resolve)(__dirname, '../../features');
        await this.scanForSkills();
    }
    async start() { }
    async stop() {
        this.loadedSkills = [];
    }
    async healthCheck() {
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
    async scanForSkills() {
        if (!(0, fs_1.existsSync)(this.featuresPath)) {
            this.ctx?.logger?.warn?.('Features directory not found', { path: this.featuresPath });
            return;
        }
        const entries = (0, fs_1.readdirSync)(this.featuresPath, { withFileTypes: true });
        let count = 0;
        for (const entry of entries) {
            if (!entry.isDirectory())
                continue;
            const skill = this.loadSkillFromFeature((0, path_1.join)(this.featuresPath, entry.name));
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
    loadSkillFromFeature(featureDir) {
        const skillPath = (0, path_1.join)(featureDir, 'SKILL.md');
        if (!(0, fs_1.existsSync)(skillPath))
            return null;
        const content = (0, fs_1.readFileSync)(skillPath, 'utf-8');
        const featureName = (0, path_1.dirname)(featureDir).split('/').pop() || 'unknown';
        return {
            skillName: featureName,
            content,
            rootDir: featureDir,
        };
    }
    /**
     * Inject skills into agent context for a given feature set
     */
    injectSkillsIntoContext(featureDirs, context) {
        const skills = [];
        for (const dir of featureDirs) {
            const skill = this.loadSkillFromFeature(dir);
            if (skill)
                skills.push(skill);
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
    getLoadedSkills() {
        return [...this.loadedSkills];
    }
    /**
     * Get skills formatted as markdown string (for context injection)
     */
    getSkillsAsText() {
        return this.loadedSkills
            .map(s => `# ${s.skillName}\n\n${s.content}`)
            .join('\n\n---\n\n');
    }
}
exports.skillLoader = new SkillLoaderFeature();
exports.default = exports.skillLoader;
//# sourceMappingURL=index.js.map