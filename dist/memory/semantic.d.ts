/**
 * Semantic Memory Module
 *
 * OMEGA Memory integration — semantic search using embeddings.
 * Uses SQLite with FTS5 for full-text search and stores embeddings
 * for semantic similarity matching.
 */
import Database from 'better-sqlite3';
/** Memory result returned from searches */
export interface MemoryResult {
    id: string;
    type: string;
    content: string;
    embedding: Buffer | null;
    created_at: number;
    accessed_at: number;
    access_count: number;
    metadata: Record<string, unknown>;
    similarity?: number;
}
/** Edge in the memory graph */
export interface MemoryEdge {
    source_id: string;
    target_id: string;
    relation_type: string;
    weight: number;
}
/**
 * Semantic Memory — stores memories with embeddings for semantic search.
 *
 * Provides full-text search via FTS5 and semantic similarity via
 * vector embeddings (bge-small ONNX model, 384-dim).
 */
export declare class SemanticMemory {
    private dbPath;
    private db;
    private initialized;
    constructor(dbPath?: string);
    /** Initialize database with schema */
    init(): void;
    /** Store a memory entry */
    store(type: string, content: string, metadata?: Record<string, unknown>): Promise<string>;
    /** Search memories by keyword (FTS5) and semantic similarity */
    search(query: string, limit?: number): Promise<MemoryResult[]>;
    /** Get recent memories from last N hours */
    getRecent(hours: number): Promise<MemoryResult[]>;
    /** Consolidate memories — deduplicate and decay weights */
    consolidate(): Promise<void>;
    /** Save checkpoint state for a task */
    checkpoint(taskId: string, state: unknown): Promise<void>;
    /** Resume a checkpointed task */
    resume(taskId: string): Promise<unknown>;
    /** Get a memory by ID */
    getById(id: string): Promise<MemoryResult | null>;
    /** Delete a memory */
    delete(id: string): Promise<boolean>;
    /** Get total memory count */
    count(): Promise<number>;
    /** Get memories by type */
    getByType(type: string, limit?: number): Promise<MemoryResult[]>;
    /** Close database */
    close(): void;
    /** Get raw DB instance (for graph module) */
    getDb(): Database.Database;
    /** Ensure initialized */
    private ensureInit;
    /** Fallback LIKE search */
    private searchByLike;
    /** Generate embedding for text — stub for ONNX bge-small model */
    private generateEmbedding;
    /** Cosine similarity between two vectors */
    private cosineSimilarity;
    /** SHA256 hash of content */
    private hashContent;
    /** Convert DB row to MemoryResult */
    private rowToResult;
    /** Safe JSON parse */
    private parseJson;
}
export declare function getSemanticMemory(dbPath?: string): SemanticMemory;
//# sourceMappingURL=semantic.d.ts.map