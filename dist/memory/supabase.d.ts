/**
 * Supabase Memory Backend
 *
 * Cloud-backed memory storage using Supabase (PostgreSQL).
 * Provides real-time sync, row-level security, and full-text search.
 */
/** Memory row stored in Supabase */
export interface SupabaseMemoryRow {
    id: string;
    user_id: string;
    key: string;
    value: string;
    type: string;
    tags: string[];
    metadata: Record<string, unknown>;
    importance: number;
    created_at: string;
    updated_at: string;
}
/**
 * Supabase-backed memory store.
 *
 * Stores memories in a Supabase PostgreSQL database with full-text
 * search, real-time subscriptions, and row-level security.
 *
 * Requires SUPABASE_URL and SUPABASE_KEY environment variables.
 */
export declare class SupabaseMemory {
    private client;
    private tableName;
    constructor(tableName?: string);
    /** Initialize Supabase client */
    init(url?: string, key?: string): void;
    /** Store a memory entry */
    set(key: string, value: string, options?: {
        userId?: string;
        type?: string;
        tags?: string[];
        metadata?: Record<string, unknown>;
        importance?: number;
    }): Promise<SupabaseMemoryRow | null>;
    /** Retrieve memory by key */
    get(key: string, userId?: string): Promise<SupabaseMemoryRow | null>;
    /** Update a memory entry */
    update(key: string, value: string, userId?: string): Promise<boolean>;
    /** Delete a memory entry */
    delete(key: string, userId?: string): Promise<boolean>;
    /** Query memories with filtering */
    query(options?: {
        userId?: string;
        type?: string;
        limit?: number;
        offset?: number;
    }): Promise<SupabaseMemoryRow[]>;
    /** Full-text search */
    search(query: string, userId?: string, limit?: number): Promise<SupabaseMemoryRow[]>;
}
//# sourceMappingURL=supabase.d.ts.map