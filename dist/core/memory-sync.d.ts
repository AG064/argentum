/**
 * Git-Based Memory Sync for AG-Claw
 *
 * Exports memories as compressed chunks, commits to git for cross-machine sync.
 * Uses checksums for deduplication.
 */
export interface MemoryChunk {
    id: string;
    type: 'entity' | 'relation' | 'episode' | 'skill' | 'context';
    content: string;
    checksum: string;
    createdAt: number;
    metadata: Record<string, unknown>;
}
export interface GitSyncConfig {
    enabled: boolean;
    repoPath: string;
    branch?: string;
    commitMessage?: string;
    compress?: boolean;
    maxChunkSize?: number;
    autoSyncInterval?: number;
}
export declare class MemoryGitSync {
    private config;
    private chunksDir;
    private indexFile;
    constructor(config: GitSyncConfig);
    /**
     * Generate checksum for content deduplication
     */
    private checksum;
    /**
     * Compress content with gzip
     */
    private compress;
    /**
     * Export memories to git
     */
    export(chunks: MemoryChunk[]): Promise<number>;
    /**
     * Load existing index
     */
    private loadIndex;
    /**
     * Import memories from git
     */
    import(): Promise<MemoryChunk[]>;
    /**
     * Sync: export local, pull remote, push
     */
    sync(): Promise<{
        exported: number;
        imported: number;
        conflicts: number;
    }>;
    /**
     * Push to remote
     */
    push(): Promise<void>;
    /**
     * Pull from remote
     */
    pull(): Promise<void>;
    /**
     * Get sync status
     */
    status(): Promise<{
        lastSync: number | null;
        pending: number;
        conflicts: number;
    }>;
}
export default MemoryGitSync;
//# sourceMappingURL=memory-sync.d.ts.map