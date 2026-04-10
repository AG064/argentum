/**
 * Trajectory Export Feature
 *
 * Records conversation trajectories for RL fine-tuning data preparation.
 * Supports JSONL export with optional gzip compression.
 *
 * Stores entries in SQLite alongside the sessions database for easy joining.
 */
import { type TrajectoryEntry, type TrajectoryMessage, type TrajectoryMetadata, type TrajectoryExportOptions, type TrajectoryStats } from './types';
import { type FeatureModule, type FeatureContext, type FeatureMeta, type HealthStatus } from '../../core/plugin-loader';
declare class TrajectoryExportFeature implements FeatureModule {
    readonly meta: FeatureMeta;
    private config;
    private ctx;
    private db;
    private dbPath;
    constructor();
    init(config: Record<string, unknown>, context: FeatureContext): Promise<void>;
    start(): Promise<void>;
    stop(): Promise<void>;
    healthCheck(): Promise<HealthStatus>;
    /**
     * Record a trajectory entry after a conversation completes.
     */
    record(entry: TrajectoryEntry): void;
    /**
     * Record a raw conversation as a trajectory.
     */
    recordConversation(sessionId: string, messages: TrajectoryMessage[], metadata: TrajectoryMetadata): void;
    /**
     * Export trajectories matching the given options.
     * Returns the path to the exported file.
     */
    export(options?: TrajectoryExportOptions): Promise<string>;
    /**
     * Export a single session as JSONL string.
     */
    exportSession(sessionId: string): TrajectoryEntry[];
    /**
     * Get aggregate statistics across all trajectories.
     */
    getStats(options?: TrajectoryExportOptions): TrajectoryStats;
    private initDatabase;
    private queryEntries;
}
declare const _default: TrajectoryExportFeature;
export default _default;
//# sourceMappingURL=index.d.ts.map