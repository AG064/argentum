/**
 * Hierarchical Memory System (MemOS Pattern)
 *
 * Three-tier memory architecture:
 *   - SHORT: Current task context, auto-evicted at 50 entries
 *   - MID:   Recent sessions, preserved longer, auto-promotes accessed >3 times
 *   - LONG:  Important facts (importance >0.7), never auto-evicted
 *
 * Promotion: entry with accessCount >5 AND importance >0.5 moves up a tier
 * Demotion: long-term entries with importance <0.3 move to mid-tier
 * Prune:    removes oldest short/mid entries beyond tier limits
 * Compact:  merges entries with >0.85 similarity in the same tier
 */
export declare enum MemoryTier {
    SHORT = "short",
    MID = "mid",
    LONG = "long"
}
export interface MemoryEntry {
    id: string;
    tier: MemoryTier;
    content: string;
    importance: number;
    accessCount: number;
    createdAt: number;
    lastAccessed: number;
    tags: string[];
}
export interface HierarchicalMemory {
    store(entry: MemoryEntry): MemoryEntry;
    retrieve(query: string, maxResults: number): MemoryEntry[];
    promote(id: string): void;
    demote(id: string): void;
    prune(): void;
    compact(): void;
}
export declare class HierarchicalMemoryStore {
    private db;
    private flushPending;
    constructor(dbPath?: string);
    private initSchema;
    store(entry: MemoryEntry): MemoryEntry;
    retrieve(query: string, maxResults: number): MemoryEntry[];
    promote(id: string): void;
    private promoteInternal;
    demote(id: string): void;
    prune(): void;
    compact(): void;
    private enforceShortCap;
    private countTier;
    private rowToEntry;
    /** Close the database connection */
    close(): void;
    /** Get stats for all tiers */
    stats(): {
        tier: MemoryTier;
        count: number;
    }[];
}
//# sourceMappingURL=hierarchical-memory.d.ts.map