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
    includeDefaultModels?: boolean;
    defaultWeights?: ScoringWeights;
}
export declare const DEFAULT_SCORING_WEIGHTS: ScoringWeights;
export declare class ModelRouter {
    private models;
    private weights;
    private cache;
    private cacheMs;
    private defaultWeights;
    constructor(config?: Partial<ModelRouterConfig>);
    /**
     * Register a model with the router
     */
    registerModel(model: ModelScore): void;
    /**
     * Register multiple models at once
     */
    registerModels(models: ModelScore[]): void;
    /**
     * Get all registered models
     */
    getModels(): ModelScore[];
    /**
     * Select the best model for given criteria
     * Auto-selects cheapest capable model when preferCheap is true
     */
    selectModel(criteria: RoutingCriteria): string;
    /**
     * Score a model across 15 dimensions
     */
    scoreModel(model: ModelScore, criteria: RoutingCriteria): number;
    private scoreCostEfficiency;
    private scoreLatency;
    private scoreCapabilityMatch;
    private scoreContextLengthFit;
    private scoreToolSupport;
    private scoreSuccessRate;
    private scoreTokenEfficiency;
    private scoreSpecializationMatch;
    private scoreReliability;
    private scoreThroughput;
    private scoreContextAvailability;
    private scorePriority;
    private scoreLoadBalancing;
    private scoreFreshness;
    private getEligibleModels;
    /**
     * Record a successful request for a model
     */
    recordSuccess(modelId: string): void;
    /**
     * Record a failed request for a model
     */
    recordFailure(modelId: string): void;
    /**
     * Update model metrics
     */
    updateMetrics(modelId: string, metrics: Partial<ModelScore>): void;
    /**
     * Clear score cache
     */
    private invalidateCache;
    /**
     * Clear all caches
     */
    clearCache(): void;
    /**
     * Update scoring weights
     */
    setWeights(weights: Partial<ScoringWeights>): void;
    /**
     * Get current weights
     */
    getWeights(): ScoringWeights;
}
export declare function getModelRouter(config?: Partial<ModelRouterConfig>): ModelRouter;
export declare function resetModelRouter(): void;
//# sourceMappingURL=model-router.d.ts.map