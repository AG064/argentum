/**
 * Self-Evolving Memory Feature
 *
 * Memory system that learns, consolidates, and evolves over time.
 * Merges related memories, extracts patterns, and prunes stale data.
 */
import { type FeatureModule, type FeatureContext, type FeatureMeta, type HealthStatus } from '../core/plugin-loader';
/** Self-evolving memory configuration */
export interface SelfEvolvingConfig {
    enabled: boolean;
    consolidationIntervalMs: number;
    similarityThreshold: number;
    minAccessCount: number;
    maxMemories: number;
    pruneAfterDays: number;
}
/** Evolved memory entry */
export interface EvolvedMemory {
    id: string;
    content: string;
    sourceMemories: string[];
    embeddings: number[];
    importance: number;
    accessCount: number;
    consolidated: boolean;
    createdAt: number;
    updatedAt: number;
}
/** Memory pattern */
export interface MemoryPattern {
    id: string;
    type: 'recurring' | 'temporal' | 'causal' | 'associative';
    description: string;
    memoryIds: string[];
    confidence: number;
    discoveredAt: number;
}
/**
 * Self-Evolving memory — memory consolidation and pattern discovery.
 *
 * Periodically reviews stored memories, merges similar ones,
 * discovers patterns, and prunes stale entries to maintain
 * a relevant and efficient memory store.
 */
declare class SelfEvolvingMemoryFeature implements FeatureModule {
    readonly meta: FeatureMeta;
    private config;
    private ctx;
    private memories;
    private patterns;
    private timer;
    init(config: Record<string, unknown>, context: FeatureContext): Promise<void>;
    start(): Promise<void>;
    stop(): Promise<void>;
    healthCheck(): Promise<HealthStatus>;
    /** Add a new memory */
    addMemory(content: string, sourceIds?: string[], importance?: number): EvolvedMemory;
    /** Access a memory (boosts its importance) */
    accessMemory(id: string): void;
    /** Run consolidation cycle */
    runConsolidation(): Promise<void>;
    /** Discover patterns in memory */
    private discoverPatterns;
    /** Prune stale memories based on age and access count */
    private pruneStale;
    /** Get all patterns */
    getPatterns(): MemoryPattern[];
    /** Get memories related to a pattern */
    getPatternMemories(patternId: string): EvolvedMemory[];
    /** Simple text similarity (Jaccard on words) */
    private computeSimilarity;
}
declare const _default: SelfEvolvingMemoryFeature;
export default _default;
//# sourceMappingURL=self-evolving.d.ts.map