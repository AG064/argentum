/**
 * Pre-Reasoning Hook (ReMe Pattern)
 *
 * Called before each reasoning step to compact tool results and prevent context overflow.
 * Extracts goal, progress, decisions, next steps, and critical context from tool results.
 */
export interface ReasoningContext {
    goal: string;
    progress: string;
    decisions: string[];
    nextSteps: string;
    criticalContext: string;
}
/**
 * Pre-Reasoning Hook - called before each reasoning step
 * Compacts tool results to prevent context overflow
 *
 * @param toolResults - Array of tool result strings
 * @param maxTokens - Maximum tokens for progress summary (default 500)
 * @returns ReasoningContext with goal, progress, decisions, next steps, critical context
 */
export declare function preReasoningHook(toolResults: string[], maxTokens?: number): ReasoningContext;
//# sourceMappingURL=pre-reasoning-hook.d.ts.map