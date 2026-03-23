/**
 * Budget Feature Types
 */

// ─── Configuration ─────────────────────────────────────────────────────────────

export interface BudgetConfig {
  globalMonthlyLimit: number; // USD per month
  globalDailyLimit: number; // USD per day
  perAgentLimit?: number; // USD per subagent
  alertThreshold: number; // 0.8 = 80%
  blockOnExhausted: boolean; // hard stop when budget hit
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

// ─── Usage Records ─────────────────────────────────────────────────────────────

export interface BudgetUsage {
  agentId: string;
  month: string; // "2026-03"
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

// ─── Cost Tracking ─────────────────────────────────────────────────────────────

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

// ─── Model Pricing (USD per 1M tokens) ───────────────────────────────────────

export interface ModelPricing {
  input: number;
  output: number;
}

export const MODEL_PRICING: Record<string, ModelPricing> = {
  'minimax/m2': { input: 0.1, output: 0.5 },
  'openai/gpt-4o': { input: 2.5, output: 10 },
  'openai/gpt-4o-mini': { input: 0.15, output: 0.6 },
  'openai/gpt-4-turbo': { input: 10, output: 30 },
  'anthropic/claude-sonnet-4-20250514': { input: 3, output: 15 },
  'anthropic/claude-opus-4-20250514': { input: 15, output: 75 },
  'anthropic/claude-3-5-sonnet-latest': { input: 3, output: 15 },
  'anthropic/claude-3-5-haiku-latest': { input: 0.8, output: 4 },
  'google/gemini-2.5-flash': { input: 0.1, output: 0.4 },
  'google/gemini-2.5-pro': { input: 1.25, output: 5 },
  'nvidia/*': { input: 0, output: 0 },
  'openrouter/free': { input: 0, output: 0 },
  'deepseek-ai/deepseek-v3.2': { input: 0.1, output: 0.5 },
};

// ─── Budget Check Result ──────────────────────────────────────────────────────

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
