/**
 * Consolidation Feature
 *
 * OMEGA Memory integration — periodic memory consolidation.
 * Deduplication, decay, merge, and pruning of memory entries.
 */
import { type FeatureModule, type FeatureContext, type FeatureMeta, type HealthStatus } from '../../core/plugin-loader';
/** Consolidation configuration */
export interface ConsolidationConfig {
    enabled: boolean;
    intervalMs: number;
    similarityThreshold: number;
    decayRate: number;
    pruneWeightThreshold: number;
    maxMemories: number;
    dryRun: boolean;
}
/** Consolidation result */
export interface ConsolidationResult {
    deduplicated: number;
    decayed: number;
    merged: number;
    pruned: number;
    totalBefore: number;
    totalAfter: number;
    durationMs: number;
}
/**
 * Consolidation — periodic memory maintenance.
 *
 * Runs on a schedule to:
 * 1. Deduplicate — remove exact hash matches
 * 2. Decay — reduce weight of old, rarely accessed entries
 * 3. Merge — combine similar entries (semantic similarity > threshold)
 * 4. Prune — remove entries below weight threshold
 */
declare class ConsolidationFeature implements FeatureModule {
    readonly meta: FeatureMeta;
    private config;
    private ctx;
    private timer;
    private lastRun;
    private lastResult;
    private runCount;
    init(config: Record<string, unknown>, context: FeatureContext): Promise<void>;
    start(): Promise<void>;
    stop(): Promise<void>;
    healthCheck(): Promise<HealthStatus>;
    /** Run full consolidation cycle */
    run(): Promise<ConsolidationResult>;
    /** Remove exact content hash duplicates */
    private deduplicate;
    /** Apply weight decay to old, rarely accessed memories */
    private applyDecay;
    /** Merge similar entries using Jaccard similarity */
    private mergeSimilar;
    /** Prune entries below weight threshold */
    private prune;
    /** Enforce maximum memory limit */
    private enforceMaxLimit;
    /** Compute Jaccard similarity between two strings */
    private computeSimilarity;
    /** Merge two pieces of content */
    private mergeContent;
    /** Manual run trigger */
    runNow(): Promise<ConsolidationResult>;
    /** Get consolidation stats */
    getStats(): {
        lastRun: number;
        runCount: number;
        lastResult: ConsolidationResult | null;
    };
}
declare const _default: ConsolidationFeature;
export default _default;
//# sourceMappingURL=index.d.ts.map