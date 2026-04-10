/**
 * SQLite Memory Feature
 *
 * Namespace-aware key-value store using SQLite with FTS5 full-text search.
 * Supports multiple namespaces: conversations, facts, preferences, tasks.
 */
import { type FeatureModule, type FeatureContext, type FeatureMeta, type HealthStatus } from '../../core/plugin-loader';
/** Memory entry */
export interface MemoryEntry {
    namespace: string;
    key: string;
    value: string;
    created_at: number;
    updated_at: number;
}
/** Feature configuration */
export interface SQLiteMemoryConfig {
    dbPath?: string;
    namespaces?: string[];
}
/**
 * SQLiteMemory — persistent namespace key-value store.
 *
 * Provides memory storage with full-text search across values.
 * Supports isolated namespaces for different memory types.
 */
declare class SQLiteMemoryFeature implements FeatureModule {
    readonly meta: FeatureMeta;
    private config;
    private ctx;
    private db;
    private defaultNamespaces;
    constructor();
    init(config: Record<string, unknown>, context: FeatureContext): Promise<void>;
    start(): Promise<void>;
    stop(): Promise<void>;
    healthCheck(): Promise<HealthStatus>;
    /** Store a value under a namespace and key */
    store(namespace: string, key: string, value: string): void;
    /** Get a value by namespace and key */
    get(namespace: string, key: string): string | null;
    /** Search values by full-text query across all namespaces */
    search(query: string, limit?: number): Array<{
        namespace: string;
        key: string;
        value: string;
        rank: number;
    }>;
    /** Delete a specific key in a namespace */
    delete(namespace: string, key: string): boolean;
    /** List all keys in a namespace */
    list(namespace: string): string[];
    /** Get all entries in a namespace */
    getAll(namespace: string): MemoryEntry[];
    /** Clear a namespace (delete all keys) */
    clear(namespace: string): void;
    /** Initialize database and create tables */
    private initDatabase;
    /** Fallback LIKE search */
    private searchByLike;
}
declare const _default: SQLiteMemoryFeature;
export default _default;
//# sourceMappingURL=index.d.ts.map