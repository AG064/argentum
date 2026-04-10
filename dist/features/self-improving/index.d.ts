/**
 * Self-Improving Loop Feature
 *
 * The reflection engine that makes AG-Claw get smarter over time.
 * Runs 5 phases during idle time or on schedule.
 *
 * Phase 1: Error Analysis     — Review failed tasks, user corrections
 * Phase 2: Skill Creation    — Create reusable skills from patterns
 * Phase 3: Memory Consolidation — FTS5-powered memory refresh
 * Phase 4: User Model Update  — Refine understanding of user preferences
 * Phase 5: Self-Correction    — Update SOUL.md and response strategies
 *
 * CLI:
 *   agclaw improve           — Run full loop
 *   agclaw improve --phase skill   — Run specific phase
 *   agclaw improve --dry-run      — Show what would change
 *   agclaw improve --force        — Force run even if recently ran
 *   agclaw learnings           — Show accumulated lessons
 */
import type { SelfImprovingConfig, SelfImprovingResult, LessonEntry } from './types';
import type { FeatureModule, FeatureContext, FeatureMeta, HealthStatus } from '../../core/plugin-loader';
declare class SelfImprovingLoop implements FeatureModule {
    readonly meta: FeatureMeta;
    private ctx;
    private config;
    private workDir;
    private memoryDir;
    private skillsDir;
    private sessionsDbPath;
    private initialized;
    private lastRunTime;
    private analyzer;
    private skillCreator;
    init(config: Record<string, unknown>, context: FeatureContext): Promise<void>;
    start(): Promise<void>;
    stop(): Promise<void>;
    healthCheck(): Promise<HealthStatus>;
    /**
     * Run the full self-improving loop or a specific phase
     */
    run(phase?: 'all' | 'error' | 'skill' | 'memory' | 'model' | 'correction', options?: {
        dryRun?: boolean;
        force?: boolean;
        verbose?: boolean;
    }): Promise<SelfImprovingResult>;
    /**
     * Get formatted learnings/lessons log
     */
    getLearnings(): LessonEntry[];
    /**
     * Show what would change without making changes
     */
    dryRun(): Promise<SelfImprovingResult>;
    /**
     * Show current configuration
     */
    getConfig(): SelfImprovingConfig;
    private runErrorAnalysis;
    private runSkillCreation;
    private runMemoryConsolidation;
    private runUserModelUpdate;
    private runSelfCorrection;
    private log;
    private logResult;
    private gatherSessionContent;
    private findRecentMemoryFiles;
    private extractInsights;
    private mergeIntoMemory;
    private analyzeUserPatterns;
    private updateUserModelingMd;
    private determineCorrections;
    private patternToArea;
    private applyCorrections;
    private logSelfCorrection;
    private loadConfig;
    private saveConfig;
    private loadLastRun;
    private saveLastRun;
}
declare const instance: SelfImprovingLoop;
export default instance;
export type { SelfImprovingConfig, SelfImprovingResult, LessonEntry } from './types';
//# sourceMappingURL=index.d.ts.map