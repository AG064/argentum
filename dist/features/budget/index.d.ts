/**
 * Budget Enforcement Feature
 *
 * Tracks token usage and cost per agent with SQLite persistence.
 * Supports monthly/daily limits, per-agent caps, and configurable hard stops.
 *
 * Integration points:
 *  - Before LLM call: checkBudget(agentId) → reject if exhausted
 *  - After LLM call: recordCost(agentId, usage, provider, model) → store cost
 *  - Heartbeat: getBudgetStatus() → report if threshold reached
 *  - System prompt: getBudgetSummary() → inject budget status
 */
import { type BudgetConfig, type BudgetConfigInput, type BudgetStatus, type BudgetReport, type BudgetCheckResult, type TokenUsage } from './types';
import { type FeatureModule, type FeatureContext, type FeatureMeta, type HealthStatus } from '../../core/plugin-loader';
export { MODEL_PRICING } from './types';
export declare const _MODEL_PRICING_MAP: Record<string, {
    input: number;
    output: number;
}>;
/**
 * Calculate cost in USD for a given model and token usage.
 * Falls back to wildcard patterns then to 'unknown' pricing.
 */
export declare function calculateCost(model: string, usage: TokenUsage): number;
export interface ValidationResult {
    valid: boolean;
    error?: string;
}
export declare function validateBudgetConfig(input: BudgetConfigInput): ValidationResult;
declare class BudgetFeature implements FeatureModule {
    readonly meta: FeatureMeta;
    private config;
    private db;
    private ctx;
    private alertedAgents;
    init(config: Record<string, unknown>, context: FeatureContext): Promise<void>;
    start(): Promise<void>;
    stop(): Promise<void>;
    healthCheck(): Promise<HealthStatus>;
    updateConfig(input: BudgetConfigInput): {
        success: boolean;
        error?: string;
    };
    getConfig(): BudgetConfig;
    getConfigDisplay(): Record<string, {
        value: unknown;
        default: unknown;
    }>;
    recordCost(agentId: string, usage: TokenUsage, provider: string, model: string): number;
    checkBudget(agentId: string): BudgetCheckResult;
    canProceed(agentId: string): boolean;
    getUsage(agentId: string): BudgetStatus;
    getBudgetReport(): BudgetReport;
    getHistory(days?: number): Array<{
        date: string;
        totalCost: number;
        totalTokens: number;
        requestCount: number;
    }>;
    reset(): void;
    resetAgent(agentId: string): {
        changes: number;
    };
    getBudgetSummary(): string;
    private ensureDb;
    private log;
    private initDatabase;
    private getAgentDailyUsage;
    private getAgentMonthlyUsage;
    private getDailyUsage;
    private getMonthlyUsage;
    private getDayStart;
    private getMonthStart;
    private checkAndAlert;
}
declare const _default: BudgetFeature;
export default _default;
//# sourceMappingURL=index.d.ts.map