/**
 * skill-evolution/index.ts
 *
 * Bridge between Argentum and OpenSpace self-evolving skills engine.
 * OpenSpace: https://github.com/AG064/OpenSpace (AG064 fork)
 *
 * Provides:
 * - AUTO-FIX: repair broken SKILL.md
 * - AUTO-IMPROVE: evolve successful skills
 * - AUTO-LEARN: capture new skills from workflows
 *
 * LLM: MiniMax (primary), OpenRouter (fallback), any OpenAI-compatible API
 */
export interface SkillEvolutionConfig {
    /** Path to OpenSpace installation (default: ~/OpenSpace) */
    openSpacePath?: string;
    /** LLM provider: 'minimax' | 'openrouter' | 'nvidia' | 'local' */
    llmProvider?: 'minimax' | 'openrouter' | 'nvidia' | 'local';
    /** API key for the LLM provider */
    apiKey?: string;
    /** Custom LLM endpoint (for local models) */
    llmEndpoint?: string;
    /** Skills directory to monitor */
    skillsDir?: string;
}
export interface DiagnoseResult {
    skill: string;
    health: 'healthy' | 'broken' | 'needs-improvement';
    issues: string[];
    suggestions: string[];
}
export interface EvolveResult {
    skill: string;
    originalVersion: number;
    newVersion: number;
    changes: string[];
    improved: boolean;
}
export interface LearnResult {
    newSkillName: string;
    capturedFrom: string;
    confidence: number;
    autoInstall: boolean;
}
export declare class SkillEvolution {
    private openSpacePath;
    private config;
    constructor(config?: SkillEvolutionConfig);
    /**
     * Diagnose a skill - check if it's healthy, broken, or needs improvement
     */
    diagnose(skillName: string): Promise<DiagnoseResult>;
    /**
     * AUTO-FIX: Repair a broken skill
     */
    fixSkill(skillPath: string): Promise<{
        success: boolean;
        changes: string[];
        error?: string;
    }>;
    /**
     * AUTO-IMPROVE: Evolve a skill based on usage patterns
     */
    evolveSkill(skillName: string, metrics?: Record<string, number>): Promise<EvolveResult>;
    /**
     * AUTO-LEARN: Capture a new skill from successful execution
     */
    learnFromWorkflow(executionLog: string, options?: {
        autoInstall?: boolean;
    }): Promise<LearnResult>;
    /**
     * Run OpenSpace CLI command
     */
    private runOpenSpace;
    /**
     * Check if OpenSpace is installed and configured
     */
    isConfigured(): Promise<{
        available: boolean;
        error?: string;
    }>;
}
/**
 * Create SkillEvolution instance from Argentum credentials
 * Reads API keys from ~/.openclaw/credentials/telegram.json
 */
export declare function createSkillEvolution(): Promise<SkillEvolution>;
//# sourceMappingURL=index.d.ts.map