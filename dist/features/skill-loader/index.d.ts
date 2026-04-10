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
import { type FeatureModule, type FeatureContext, type FeatureMeta, type HealthStatus } from '../../core/plugin-loader';
export interface SkillContext {
    skillName: string;
    content: string;
    rootDir: string;
}
export interface SkillLoaderConfig {
    enabled: boolean;
    featuresPath?: string;
}
declare class SkillLoaderFeature implements FeatureModule {
    readonly meta: FeatureMeta;
    private ctx;
    private featuresPath;
    private loadedSkills;
    init(config: Record<string, unknown>, context: FeatureContext): Promise<void>;
    start(): Promise<void>;
    stop(): Promise<void>;
    healthCheck(): Promise<HealthStatus>;
    /**
     * Scan all feature directories for SKILL.md files
     */
    private scanForSkills;
    /**
     * Load SKILL.md from a feature directory
     */
    loadSkillFromFeature(featureDir: string): SkillContext | null;
    /**
     * Inject skills into agent context for a given feature set
     */
    injectSkillsIntoContext(featureDirs: string[], context: Record<string, unknown>): Record<string, unknown>;
    /**
     * Get all loaded skills
     */
    getLoadedSkills(): SkillContext[];
    /**
     * Get skills formatted as markdown string (for context injection)
     */
    getSkillsAsText(): string;
}
export declare const skillLoader: SkillLoaderFeature;
export default skillLoader;
//# sourceMappingURL=index.d.ts.map