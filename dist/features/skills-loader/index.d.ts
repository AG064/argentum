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
declare class SkillsLoaderFeature {
    readonly meta: {
        name: string;
        version: string;
        description: string;
        dependencies: never[];
    };
    private ctx;
    private skillsDir;
    private skillsCache;
    init(config: Record<string, unknown>, context: any): Promise<void>;
    private scanSkillsCount;
    start(): Promise<void>;
    stop(): Promise<void>;
    healthCheck(): Promise<{
        healthy: boolean;
        details: Record<string, unknown>;
    }>;
    /**
     * Level 0: Lightweight list of all skills (name, description, category only)
     * This is ~3k tokens for 100 skills - minimal token cost
     */
    skillsList(): SkillSummary[];
    /**
     * Level 1: Full skill view - loads complete SKILL.md + metadata
     */
    skillView(name: string): SkillMeta | null;
    /**
     * Level 2: View specific reference file within a skill
     */
    skillViewRef(name: string, refPath: string): string | null;
    listSkills(): SkillSummary[];
    getSkill(name: string): SkillMeta | undefined;
    getSkillMd(name: string): string | null;
    getScripts(name: string): string[];
    runScript(skillName: string, scriptName: string, args?: string[]): string;
    searchSkills(query: string): SkillSummary[];
    getManifest(name: string): SkillManifest | null;
    /** Get list of all skill directories (raw, for advanced use) */
    getSkillPaths(): string[];
}
declare const _default: SkillsLoaderFeature;
export default _default;
//# sourceMappingURL=index.d.ts.map