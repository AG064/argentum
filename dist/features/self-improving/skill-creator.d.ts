/**
 * Skill Creator
 *
 * Phase 2 of the self-improving loop.
 * Creates reusable skills from repeated patterns or complex tasks.
 */
import type { SkillTemplate } from './types';
export declare class SkillCreator {
    private skillsDir;
    private memoryDir;
    private threshold;
    constructor(skillsDir: string, memoryDir: string, threshold?: number);
    /**
     * Analyze task history and create skills for recurring patterns
     */
    createSkills(complexTasks: SkillTemplate[]): Promise<SkillTemplate[]>;
    /**
     * Create a new skill from template
     */
    private createNewSkill;
    /**
     * Update an existing skill
     */
    private updateExistingSkill;
    /**
     * Build SKILL.md content from template
     */
    private buildSkillMd;
    /**
     * Detect common task patterns from session content
     */
    detectPatterns(sessionsContent: string[]): Promise<SkillTemplate[]>;
    /**
     * Infer category from pattern name
     */
    private inferCategory;
    /**
     * Infer triggers from pattern name
     */
    private inferTriggers;
    /**
     * Get list of auto-created skills
     */
    listAutoSkills(): string[];
}
//# sourceMappingURL=skill-creator.d.ts.map