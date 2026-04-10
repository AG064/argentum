/**
 * Self-Evolving Memory Feature
 *
 * Automatically improves memory by analyzing patterns, deduplicating,
 * consolidating similar entries, and promoting important memories.
 */
import { type FeatureModule, type FeatureContext, type FeatureMeta, type HealthStatus } from '../../core/plugin-loader';
/** Feature configuration */
export interface SelfEvolvingConfig {
    dbPath?: string;
    autoEvolve?: boolean;
    evolveIntervalMs?: number;
    similarityThreshold?: number;
    decayRate?: number;
    weightBoost?: number;
}
/** Analysis result */
export interface AnalysisResult {
    totalMemories: number;
    byType: Record<string, number>;
    averageWeight: number;
    zeroWeightCount: number;
    topWords: Array<{
        word: string;
        frequency: number;
    }>;
    duplicateGroups: number;
}
/**
 * SelfEvolvingMemoryFeature — automatic memory optimization.
 *
 * Connects to a SemanticMemory database and provides:
 * - analyze(): collect statistics and pattern insights
 * - deduplicate(): remove exact content duplicates
 * - consolidate(): full cycle (dedup + decay + merge similar + prune)
 * - promote(pattern): boost weight of memories matching a text pattern
 *
 * Can run automatically on a schedule.
 */
declare class SelfEvolvingMemoryFeature implements FeatureModule {
    readonly meta: FeatureMeta;
    private config;
    private ctx;
    private db;
    private timer;
    private lastAnalysis;
    private lastConsolidation;
    constructor();
    init(config: Record<string, unknown>, context: FeatureContext): Promise<void>;
    start(): Promise<void>;
    stop(): Promise<void>;
    healthCheck(): Promise<HealthStatus>;
    /** Analyze memory store for patterns and statistics */
    analyze(): Promise<AnalysisResult>;
    /** Deduplicate exact content hashes */
    deduplicate(): Promise<number>;
    /** Full consolidation cycle */
    consolidate(): Promise<{
        dedup: number;
        merged: number;
        decayed: number;
        pruned: number;
    }>;
    /** Promote memories matching a text pattern by boosting weight */
    promote(pattern: string): Promise<number>;
    /** Initialize database connection */
    private initDatabase;
    /** Merge similar memory entries using Jaccard similarity */
    private mergeSimilar;
    /** Jaccard similarity on word sets */
    private jaccardSimilarity;
}
declare const _default: SelfEvolvingMemoryFeature;
export default _default;
//# sourceMappingURL=index.d.ts.map