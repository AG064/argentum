/**
 * Cost-Aware Model Router
 *
 * 15-dimension scoring system for intelligent model selection based on:
 * 1. Cost efficiency
 * 2. Latency
 * 3. Capability match
 * 4. Context length fit
 * 5. Tool support
 * 6. Recent success rate
 * 7. Token efficiency
 * 8. Specialization match
 * 9. Reliability score
 * 10. Custom criteria (extensible)
 * 11. Throughput
 * 12. Context availability
 * 13. Priority weighting
 * 14. Load balancing
 * 15. Freshness/preference
 */

import { createLogger } from './logger';

const logger = createLogger();

// ─── Interfaces ─────────────────────────────────────────────────────────────

export interface ModelScore {
  modelId: string;
  score: number;
  costPer1K: number;
  latency: number;
  capabilities: string[];
  contextLength?: number;
  toolSupport?: boolean;
  reliability?: number;
  specialization?: string[];
  throughput?: number;
  lastSuccessAt?: number;
  lastFailureAt?: number;
  totalRequests: number;
  successfulRequests: number;
}

export interface RoutingCriteria {
  maxCost?: number;
  requiredCapabilities?: string[];
  maxLatency?: number;
  preferFast?: boolean;
  preferCheap?: boolean;
  requiredTools?: boolean;
  minContextLength?: number;
  preferredCapabilities?: string[];
  taskType?: string;
  weights?: Partial<ScoringWeights>;
}

export interface ScoringWeights {
  costEfficiency: number;
  latency: number;
  capabilityMatch: number;
  contextLengthFit: number;
  toolSupport: number;
  recentSuccessRate: number;
  tokenEfficiency: number;
  specializationMatch: number;
  reliability: number;
  throughput: number;
  customWeight: number;
}

export interface ModelRouterConfig {
  weights?: Partial<ScoringWeights>;
  cacheScoresMs?: number;
  defaultWeights: ScoringWeights;
}

// ─── Default Weights (15 dimensions) ───────────────────────────────────────

export const DEFAULT_SCORING_WEIGHTS: ScoringWeights = {
  costEfficiency: 1.2,       // Lower cost = higher score
  latency: 1.0,              // Lower latency = higher score
  capabilityMatch: 1.5,     // Match required capabilities strongly
  contextLengthFit: 0.8,    // Don't over-provision context length
  toolSupport: 1.3,         // Tool support is important
  recentSuccessRate: 1.4,   // Track actual performance
  tokenEfficiency: 0.7,      // Prefer efficient models
  specializationMatch: 1.1, // Domain-specific advantage
  reliability: 1.3,         // Avoid flaky models
  throughput: 0.6,          // Speed matters less if accuracy is fine
  customWeight: 1.0,        // Extensible
};

// ─── Model Registry ─────────────────────────────────────────────────────────

const DEFAULT_MODELS: ModelScore[] = [
  {
    modelId: 'anthropic/claude-sonnet-4-20250514',
    score: 0,
    costPer1K: 3.0,
    latency: 800,
    capabilities: ['reasoning', 'coding', 'analysis', 'creative'],
    contextLength: 200000,
    toolSupport: true,
    reliability: 0.98,
    specialization: ['general', 'coding', 'reasoning'],
    throughput: 50,
    totalRequests: 0,
    successfulRequests: 0,
  },
  {
    modelId: 'openai/gpt-4o',
    score: 0,
    costPer1K: 2.5,
    latency: 600,
    capabilities: ['reasoning', 'coding', 'analysis', 'creative'],
    contextLength: 128000,
    toolSupport: true,
    reliability: 0.97,
    specialization: ['general', 'coding'],
    throughput: 60,
    totalRequests: 0,
    successfulRequests: 0,
  },
  {
    modelId: 'deepseek-ai/deepseek-v3.2',
    score: 0,
    costPer1K: 0.1,
    latency: 700,
    capabilities: ['reasoning', 'coding', 'analysis'],
    contextLength: 64000,
    toolSupport: true,
    reliability: 0.95,
    specialization: ['coding', 'reasoning'],
    throughput: 80,
    totalRequests: 0,
    successfulRequests: 0,
  },
];

// ─── Model Router ────────────────────────────────────────────────────────────

export class ModelRouter {
  private models: Map<string, ModelScore>;
  private weights: ScoringWeights;
  private cache: Map<string, { score: number; timestamp: number }> = new Map();
  private cacheMs: number;
  private defaultWeights: ScoringWeights;

  constructor(config?: Partial<ModelRouterConfig>) {
    this.defaultWeights = config?.defaultWeights ?? DEFAULT_SCORING_WEIGHTS;
    this.weights = { ...this.defaultWeights, ...config?.weights };
    this.cacheMs = config?.cacheScoresMs ?? 60_000; // 1 minute cache
    this.models = new Map();

    // Initialize with default models
    for (const model of DEFAULT_MODELS) {
      this.models.set(model.modelId, { ...model });
    }
  }

  /**
   * Register a model with the router
   */
  registerModel(model: ModelScore): void {
    this.models.set(model.modelId, { ...model, totalRequests: 0, successfulRequests: 0 });
  }

  /**
   * Register multiple models at once
   */
  registerModels(models: ModelScore[]): void {
    for (const model of models) {
      this.registerModel(model);
    }
  }

  /**
   * Get all registered models
   */
  getModels(): ModelScore[] {
    return Array.from(this.models.values());
  }

  /**
   * Select the best model for given criteria
   * Auto-selects cheapest capable model when preferCheap is true
   */
  selectModel(criteria: RoutingCriteria): string {
    const candidates = this.getEligibleModels(criteria);

    if (candidates.length === 0) {
      logger.warn('[ModelRouter] No eligible models found, returning first available');
      const first = this.models.values().next().value;
      return first?.modelId ?? 'unknown';
    }

    // If preferCheap, auto-select cheapest capable model
    if (criteria.preferCheap) {
      const cheapest = candidates.reduce((a, b) =>
        a.costPer1K <= b.costPer1K ? a : b
      );
      logger.debug(`[ModelRouter] Cheapest capable: ${cheapest.modelId} (${cheapest.costPer1K}/1K)`);
      return cheapest.modelId;
    }

    // Score all candidates and pick the best
    const scored = candidates.map(model => ({
      model,
      score: this.scoreModel(model, criteria),
    }));

    scored.sort((a, b) => b.score - a.score);
    const top = scored[0];
    if (!top) return '';
    logger.debug(`[ModelRouter] Selected: ${top.model.modelId} (score: ${top.score.toFixed(2)})`);
    return top.model.modelId;
  }

  /**
   * Score a model across 15 dimensions
   */
  scoreModel(model: ModelScore, criteria: RoutingCriteria): number {
    const cacheKey = `${model.modelId}:${JSON.stringify(criteria)}`;

    // Check cache
    const cached = this.cache.get(cacheKey);
    if (cached && Date.now() - cached.timestamp < this.cacheMs) {
      return cached.score;
    }

    const weights = { ...this.weights, ...criteria.weights };

    // 1. Cost Efficiency (lower cost = higher score, normalized 0-1)
    const costScore = this.scoreCostEfficiency(model.costPer1K);

    // 2. Latency (lower latency = higher score, normalized 0-1)
    const latencyScore = this.scoreLatency(model.latency, criteria.maxLatency);

    // 3. Capability Match (required + preferred)
    const capabilityScore = this.scoreCapabilityMatch(model.capabilities, criteria);

    // 4. Context Length Fit (don't over-provision)
    const contextScore = this.scoreContextLengthFit(model.contextLength ?? 32000, criteria.minContextLength);

    // 5. Tool Support
    const toolScore = this.scoreToolSupport(model.toolSupport ?? false, criteria.requiredTools ?? false);

    // 6. Recent Success Rate (exponential moving average feel)
    const successScore = this.scoreSuccessRate(model);

    // 7. Token Efficiency (lower cost per token = better)
    const tokenEfficiencyScore = this.scoreTokenEfficiency(model.costPer1K);

    // 8. Specialization Match
    const specializationScore = this.scoreSpecializationMatch(
      model.specialization ?? [],
      criteria.taskType
    );

    // 9. Reliability Score
    const reliabilityScore = this.scoreReliability(model.reliability ?? 0.95);

    // 10. Custom Weight (extensible, default 1.0)
    const customScore = weights.customWeight;

    // 11. Throughput Score
    const throughputScore = this.scoreThroughput(model.throughput ?? 50);

    // 12. Context Availability (how much headroom)
    const contextAvailabilityScore = this.scoreContextAvailability(
      model.contextLength ?? 32000,
      criteria.minContextLength
    );

    // 13. Priority Weighting (for preferFast, preferCheap flags)
    const priorityScore = this.scorePriority(weights, criteria);

    // 14. Load Balancing (slight randomization to avoid hot models)
    const loadBalanceScore = this.scoreLoadBalancing(model);

    // 15. Freshness (prefer models that haven't been used recently)
    const freshnessScore = this.scoreFreshness(model);

    // Calculate weighted sum
    const totalScore =
      costScore * weights.costEfficiency +
      latencyScore * weights.latency +
      capabilityScore * weights.capabilityMatch +
      contextScore * weights.contextLengthFit +
      toolScore * weights.toolSupport +
      successScore * weights.recentSuccessRate +
      tokenEfficiencyScore * weights.tokenEfficiency +
      specializationScore * weights.specializationMatch +
      reliabilityScore * weights.reliability +
      throughputScore * weights.throughput +
      customScore * weights.customWeight +
      contextAvailabilityScore * 0.5 +
      priorityScore * 0.8 +
      loadBalanceScore * 0.3 +
      freshnessScore * 0.4;

    // Cache the score
    this.cache.set(cacheKey, { score: totalScore, timestamp: Date.now() });

    return totalScore;
  }

  // ─── Individual Dimension Scorers ─────────────────────────────────────────

  private scoreCostEfficiency(costPer1K: number): number {
    // Lower cost = higher score. Using inverse scaling.
    // $0.1/1K = 1.0, $10/1K = 0.01
    return Math.max(0, Math.min(1, 1 - Math.log10(costPer1K + 0.01) / 3));
  }

  private scoreLatency(latency: number, maxLatency?: number): number {
    // Lower latency = higher score
    // If maxLatency specified, models over it get 0
    if (maxLatency && latency > maxLatency) return 0;
    return Math.max(0, Math.min(1, 1 - latency / 5000));
  }

  private scoreCapabilityMatch(
    capabilities: string[],
    criteria: RoutingCriteria
  ): number {
    let score = 0.5; // base

    // Check required capabilities (must have all)
    if (criteria.requiredCapabilities?.length) {
      const hasAll = criteria.requiredCapabilities.every(cap =>
        capabilities.includes(cap)
      );
      if (!hasAll) return 0;
      score += 0.3;
    }

    // Check preferred capabilities (bonus for matches)
    if (criteria.preferredCapabilities?.length) {
      const matchCount = criteria.preferredCapabilities.filter(cap =>
        capabilities.includes(cap)
      ).length;
      score += (matchCount / criteria.preferredCapabilities.length) * 0.2;
    }

    return Math.min(1, score);
  }

  private scoreContextLengthFit(contextLength: number, minRequired?: number): number {
    // Score is highest when context is just enough (not over-provisioned)
    // Very high context length gets slight penalty (waste of resources)
    if (minRequired && contextLength < minRequired) return 0;
    if (!minRequired) return 0.7; // neutral

    const ratio = contextLength / minRequired;
    if (ratio > 8) return 0.6;
    if (ratio > 4) return 0.8;
    if (ratio > 2) return 1.0;
    if (ratio > 1) return 0.9;
    return 0.5;
  }

  private scoreToolSupport(hasTools: boolean, required: boolean): number {
    if (required && !hasTools) return 0;
    return hasTools ? 1 : 0.3;
  }

  private scoreSuccessRate(model: ModelScore): number {
    if (model.totalRequests === 0) return 0.7; // neutral for new models
    const rate = model.successfulRequests / model.totalRequests;
    // Exponential weighting toward recent success
    const recencyBonus = model.lastSuccessAt
      ? Math.max(0, 1 - (Date.now() - model.lastSuccessAt) / 3600000) * 0.2
      : 0;
    return Math.min(1, rate + recencyBonus);
  }

  private scoreTokenEfficiency(costPer1K: number): number {
    // Similar to cost but more aggressive scaling
    return Math.max(0, 1 - costPer1K / 10);
  }

  private scoreSpecializationMatch(specializations: string[], taskType?: string): number {
    if (!taskType || specializations.length === 0) return 0.5;
    return specializations.includes(taskType) ? 1.0 : 0.5;
  }

  private scoreReliability(reliability: number): number {
    return Math.max(0, Math.min(1, reliability));
  }

  private scoreThroughput(throughput: number): number {
    // Higher throughput is better
    return Math.max(0, Math.min(1, throughput / 100));
  }

  private scoreContextAvailability(contextLength: number, minRequired?: number): number {
    if (!minRequired) return 0.7;
    const headroom = (contextLength - minRequired) / minRequired;
    return Math.max(0, Math.min(1, headroom));
  }

  private scorePriority(weights: ScoringWeights, criteria: RoutingCriteria): number {
    let score = 0.5;
    if (criteria.preferFast) {
      // Boost latency weight contribution
      score += (weights.latency / 2) * 0.2;
    }
    if (criteria.preferCheap) {
      // Boost cost weight contribution
      score += (weights.costEfficiency / 2) * 0.2;
    }
    return Math.min(1, score);
  }

  private scoreLoadBalancing(model: ModelScore): number {
    // Small random factor to avoid always picking the same model
    // Based on request count - models with fewer requests get slight boost
    const requestPenalty = Math.min(0.2, model.totalRequests / 1000);
    return 0.9 + Math.random() * 0.1 - requestPenalty;
  }

  private scoreFreshness(model: ModelScore): number {
    // Prefer models that haven't been used in a while
    if (!model.lastSuccessAt && !model.lastFailureAt) return 0.7;
    const lastUse = model.lastSuccessAt ?? model.lastFailureAt ?? 0;
    const idleMs = Date.now() - lastUse;
    // After 5 min idle, start increasing score
    return Math.min(1, 0.5 + idleMs / 300000);
  }

  // ─── Helper Methods ───────────────────────────────────────────────────────

  private getEligibleModels(criteria: RoutingCriteria): ModelScore[] {
    return Array.from(this.models.values()).filter(model => {
      // Filter by max cost
      if (criteria.maxCost && model.costPer1K > criteria.maxCost) {
        return false;
      }

      // Filter by max latency
      if (criteria.maxLatency && model.latency > criteria.maxLatency) {
        return false;
      }

      // Filter by required capabilities
      if (criteria.requiredCapabilities?.length) {
        const hasAll = criteria.requiredCapabilities.every(cap =>
          model.capabilities.includes(cap)
        );
        if (!hasAll) return false;
      }

      // Filter by required tools
      if (criteria.requiredTools && !model.toolSupport) {
        return false;
      }

      // Filter by min context length
      if (criteria.minContextLength) {
        const ctxLen = model.contextLength ?? 32000;
        if (ctxLen < criteria.minContextLength) return false;
      }

      return true;
    });
  }

  /**
   * Record a successful request for a model
   */
  recordSuccess(modelId: string): void {
    const model = this.models.get(modelId);
    if (model) {
      model.successfulRequests++;
      model.totalRequests++;
      model.lastSuccessAt = Date.now();
      this.invalidateCache(modelId);
    }
  }

  /**
   * Record a failed request for a model
   */
  recordFailure(modelId: string): void {
    const model = this.models.get(modelId);
    if (model) {
      model.totalRequests++;
      model.lastFailureAt = Date.now();
      this.invalidateCache(modelId);
    }
  }

  /**
   * Update model metrics
   */
  updateMetrics(modelId: string, metrics: Partial<ModelScore>): void {
    const model = this.models.get(modelId);
    if (model) {
      Object.assign(model, metrics);
      this.invalidateCache(modelId);
    }
  }

  /**
   * Clear score cache
   */
  private invalidateCache(modelId: string): void {
    for (const key of this.cache.keys()) {
      if (key.startsWith(modelId + ':')) {
        this.cache.delete(key);
      }
    }
  }

  /**
   * Clear all caches
   */
  clearCache(): void {
    this.cache.clear();
  }

  /**
   * Update scoring weights
   */
  setWeights(weights: Partial<ScoringWeights>): void {
    this.weights = { ...this.weights, ...weights };
    this.clearCache();
  }

  /**
   * Get current weights
   */
  getWeights(): ScoringWeights {
    return { ...this.weights };
  }
}

// ─── Factory ────────────────────────────────────────────────────────────────

let routerInstance: ModelRouter | null = null;

export function getModelRouter(config?: Partial<ModelRouterConfig>): ModelRouter {
  if (!routerInstance) {
    routerInstance = new ModelRouter(config);
  }
  return routerInstance;
}

export function resetModelRouter(): void {
  routerInstance = null;
}
