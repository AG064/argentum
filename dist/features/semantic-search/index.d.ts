/**
 * Semantic Search Feature
 *
 * Vector-based semantic memory search using embeddings.
 * Wraps the SemanticMemory backend with a simplified API.
 */
import { type FeatureModule, type FeatureContext, type FeatureMeta, type HealthStatus } from '../../core/plugin-loader';
import { type MemoryResult } from '../../memory/semantic';
/** Feature configuration */
export interface SemanticSearchConfig {
    dbPath?: string;
    defaultType?: string;
}
/**
 * SemanticSearchFeature — semantic similarity search over stored memories.
 *
 * Stores text with embeddings and supports both full-text and
 * semantic (vector) search. Ideal for finding related concepts
 * without exact keyword matches.
 */
declare class SemanticSearchFeature implements FeatureModule {
    readonly meta: FeatureMeta;
    private config;
    private ctx;
    private memory;
    constructor();
    init(config: Record<string, unknown>, context: FeatureContext): Promise<void>;
    start(): Promise<void>;
    stop(): Promise<void>;
    healthCheck(): Promise<HealthStatus>;
    /** Index/store a text entry with optional metadata */
    index(text: string, metadata?: Record<string, unknown>): Promise<string>;
    /** Semantic + full-text search */
    search(query: string, limit?: number): Promise<Array<MemoryResult & {
        similarity?: number;
    }>>;
    /** Delete a memory by its ID */
    delete(docId: string): Promise<boolean>;
    /** Get a memory by ID */
    get(docId: string): Promise<MemoryResult | null>;
    /** Get memories by type */
    getByType(type: string, limit?: number): Promise<MemoryResult[]>;
    /** Consolidate memory (deduplicate, decay) */
    consolidate(): Promise<void>;
    /** Save checkpoint for a task */
    checkpoint(taskId: string, state: unknown): Promise<void>;
    /** Resume a checkpointed task */
    resume(taskId: string): Promise<unknown>;
}
declare const _default: SemanticSearchFeature;
export default _default;
//# sourceMappingURL=index.d.ts.map