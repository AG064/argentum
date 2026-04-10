/**
 * Multimodal Memory Feature
 *
 * Stores and retrieves memories across text, images, audio, and documents.
 * Supports semantic search and temporal queries.
 */
import { type FeatureModule, type FeatureContext, type FeatureMeta, type HealthStatus } from '../../core/plugin-loader';
/** Multimodal memory configuration */
export interface MultimodalMemoryConfig {
    enabled: boolean;
    maxMemorySize: number;
    embeddingDimension: number;
    retentionDays: number;
}
/** Memory entry types */
export type MemoryType = 'text' | 'image' | 'audio' | 'document' | 'code';
/** Memory entry */
export interface MemoryEntry {
    id: string;
    type: MemoryType;
    content: string;
    metadata: Record<string, unknown>;
    embedding?: number[];
    tags: string[];
    importance: number;
    createdAt: number;
    accessedAt: number;
    accessCount: number;
}
/** Search query */
export interface MemorySearchQuery {
    text?: string;
    type?: MemoryType;
    tags?: string[];
    startDate?: number;
    endDate?: number;
    minImportance?: number;
    limit?: number;
}
/** Search result */
export interface MemorySearchResult {
    entry: MemoryEntry;
    score: number;
}
/**
 * Multimodal Memory feature — stores and retrieves memories across modalities.
 *
 * Provides a unified memory system for text, images, audio, and documents
 * with semantic search and importance-based retention.
 */
declare class MultimodalMemoryFeature implements FeatureModule {
    readonly meta: FeatureMeta;
    private config;
    private ctx;
    private memories;
    init(config: Record<string, unknown>, context: FeatureContext): Promise<void>;
    start(): Promise<void>;
    stop(): Promise<void>;
    healthCheck(): Promise<HealthStatus>;
    /** Store a new memory */
    store(type: MemoryType, content: string, metadata?: Record<string, unknown>, tags?: string[], importance?: number): Promise<MemoryEntry>;
    /** Retrieve a memory by ID */
    retrieve(id: string): Promise<MemoryEntry | null>;
    /** Search memories */
    search(query: MemorySearchQuery): Promise<MemorySearchResult[]>;
    /** Delete a memory */
    delete(id: string): Promise<boolean>;
    /** Evict least important memory */
    private evictLeastImportant;
    /** Simple text similarity (Jaccard on word sets) */
    private computeTextSimilarity;
}
declare const _default: MultimodalMemoryFeature;
export default _default;
//# sourceMappingURL=index.d.ts.map