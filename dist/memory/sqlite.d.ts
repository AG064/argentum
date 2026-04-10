/**
 * SQLite Memory Backend
 *
 * Persistent memory storage using SQLite with WAL mode.
 * Supports keyword search, access tracking, and metadata.
 */
/** Memory entry stored in SQLite */
export interface SQLiteMemoryEntry {
    id: string;
    content: string;
    embedding: Buffer | null;
    created_at: number;
    accessed_at: number;
    access_count: number;
    metadata: string;
}
/** Store options */
export interface StoreOptions {
    embedding?: Buffer;
    metadata?: Record<string, unknown>;
}
/** Search options */
export interface SearchOptions {
    limit?: number;
    offset?: number;
}
/**
 * SQLite-backed memory store.
 *
 * Uses WAL journal mode for better concurrency and supports
 * keyword-based full-text search via FTS5.
 */
export declare class SQLiteMemory {
    private dbPath;
    private db;
    private initialized;
    constructor(dbPath?: string);
    /** Initialize the database and create tables */
    init(): void;
    /** Store a memory entry */
    store(content: string, options?: StoreOptions): SQLiteMemoryEntry;
    /** Search memories by keyword using FTS5 */
    search(query: string, limit?: number): SQLiteMemoryEntry[];
    /** Fallback LIKE-based search */
    private searchByLike;
    /** Get recent memories */
    getRecent(limit?: number): SQLiteMemoryEntry[];
    /** Get a specific memory by ID */
    getById(id: string): SQLiteMemoryEntry | null;
    /** Delete a memory by ID */
    delete(id: string): boolean;
    /** Get total count of memories */
    count(): number;
    /** Clear all memories */
    clear(): void;
    /** Close the database connection */
    close(): void;
    /** Ensure database is initialized */
    private ensureInit;
}
/** Get or create the global memory instance */
export declare function getSQLiteMemory(dbPath?: string): SQLiteMemory;
//# sourceMappingURL=sqlite.d.ts.map