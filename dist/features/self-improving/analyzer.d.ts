/**
 * Error Analyzer
 *
 * Phase 1 of the self-improving loop.
 * Reviews failed tasks, user corrections, and identifies patterns.
 */
import type { ErrorAnalysis } from './types';
export declare class ErrorAnalyzer {
    private sessionsDbPath;
    private memoryDir;
    private lessonsPath;
    constructor(sessionsDbPath: string, memoryDir: string);
    /**
     * Analyze errors from session history and lessons log
     */
    analyzeErrors(): Promise<ErrorAnalysis[]>;
    /**
     * Find user corrections from recent sessions
     */
    private findUserCorrections;
    /**
     * Find failed tasks from existing lessons
     */
    private findFailedTasks;
    /**
     * Load existing lessons from log
     */
    private loadExistingLessons;
    /**
     * Categorize a correction into a pattern
     */
    private categorizeCorrection;
    /**
     * Categorize correction text
     */
    private categorizeCorrectionText;
    /**
     * Extract context around a message
     */
    private extractContext;
    /**
     * Describe a pattern in human-readable form
     */
    private describePattern;
    /**
     * Infer root cause from pattern
     */
    private inferRootCause;
    /**
     * Extract lessons from pattern
     */
    private extractLessons;
    /**
     * Suggest a fix for pattern
     */
    private suggestFix;
    /**
     * Log lessons learned to lessons.md
     */
    logLessons(analyses: ErrorAnalysis[]): void;
}
//# sourceMappingURL=analyzer.d.ts.map