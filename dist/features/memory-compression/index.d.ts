/**
 * Memory Compression Feature
 *
 * Compresses old memories by deduplicating, merging similar entries,
 * and archiving outdated data to a separate table.
 */
import { type FeatureModule, type FeatureContext, type FeatureMeta, type HealthStatus } from '../../core/plugin-loader';
/** Feature configuration */
export interface MemoryCompressionConfig {
    dbPath?: string;
    archiveAfterDays?: number;
    similarityThreshold?: number;
}
/** Statistics */
export interface CompressionStats {
    totalActive: number;
    totalArchived: number;
    namespaces: Record<string, {
        active: number;
        archived: number;
    }>;
    lastCompress?: {
        namespace: string;
        olderThanDays: number;
        duplicatesMerged: number;
        similarMerged: number;
    };
}
/**
 * MemoryCompressionFeature — compress and archive old memories.
 *
 * Works with the kv_store table from sqlite-memory feature.
 * Provides deduplication, similarity merging, and archival.
 */
declare class MemoryCompressionFeature implements FeatureModule {
    readonly meta: FeatureMeta;
    private config;
    private ctx;
    private db;
    private lastCompressStats;
    constructor();
    init(config: Record<string, unknown>, context: FeatureContext): Promise<void>;
    start(): Promise<void>;
    stop(): Promise<void>;
    healthCheck(): Promise<HealthStatus>;
    /** Compress memories in a namespace older than specified days */
    compress(namespace: string, olderThanDays: number): Promise<{
        duplicatesMerged: number;
        similarMerged: number;
        entriesBefore: number;
        entriesAfter: number;
    }>;
    /** Archive entries older than configured threshold (or all if no threshold) */
    archive(namespace: string): Promise<number>;
    /** Get compression statistics */
    getStats(): CompressionStats;
    /** Initialize database connection and ensure tables */
    private initDatabase;
    /** Compute Jaccard similarity between two texts based on word sets */
    private jaccardSimilarity;
}
declare const _default: MemoryCompressionFeature;
export default _default;
//# sourceMappingURL=index.d.ts.map