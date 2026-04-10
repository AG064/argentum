/**
 * Budget Feature Types
 */
export interface BudgetConfig {
    globalMonthlyLimit: number;
    globalDailyLimit: number;
    perAgentLimit?: number;
    alertThreshold: number;
    blockOnExhausted: boolean;
    enabled: boolean;
    dbPath: string;
}
export interface BudgetConfigInput {
    globalMonthlyLimit?: number;
    globalDailyLimit?: number;
    perAgentLimit?: number;
    alertThreshold?: number;
    blockOnExhausted?: boolean;
    enabled?: boolean;
    dbPath?: string;
}
export interface BudgetUsage {
    agentId: string;
    month: string;
    cost: number;
    requests: number;
    tokens: number;
    lastUpdated: Date;
}
export interface BudgetStatus {
    agent: string;
    totalTokens: number;
    totalCost: number;
    limit: number;
    percentUsed: number;
    canProceed: boolean;
    period: 'daily' | 'monthly';
}
export interface BudgetReport {
    period: string;
    periodDay: string;
    totalTokens: number;
    totalCost: number;
    dailyCost: number;
    monthlyCost: number;
    byAgent: BudgetStatus[];
    alerts: string[];
    dailyLimit: number;
    monthlyLimit: number;
}
export interface TokenUsage {
    promptTokens: number;
    completionTokens: number;
    totalTokens: number;
}
export interface CostRecord {
    agentId: string;
    provider: string;
    model: string;
    promptTokens: number;
    completionTokens: number;
    costUsd: number;
    timestamp: number;
}
export interface ModelPricing {
    input: number;
    output: number;
}
export declare const MODEL_PRICING: Record<string, ModelPricing>;
export interface BudgetCheckResult {
    allowed: boolean;
    reason?: string;
    dailyCost: number;
    monthlyCost: number;
    dailyLimit: number;
    monthlyLimit: number;
    percentUsed: number;
    threshold: number;
    blocked?: boolean;
}
//# sourceMappingURL=types.d.ts.map