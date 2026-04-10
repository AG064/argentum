/**
 * File Watcher Feature
 *
 * Monitors file system changes with configurable paths and patterns.
 * Uses chokidar for cross-platform file watching.
 */
import { type FeatureModule, type FeatureContext, type FeatureMeta, type HealthStatus } from '../../core/plugin-loader';
/** File watcher configuration */
export interface FileWatcherConfig {
    enabled: boolean;
    ignored?: string | RegExp | (string | RegExp)[];
    persistent?: boolean;
    ignoreInitial?: boolean;
    awaitWriteFinish?: {
        stabilityThreshold: number;
        pollInterval: number;
    };
}
/** Public watch info (returned by listWatches) */
export interface WatchInfo {
    id: string;
    path: string;
    pattern?: string | RegExp | (string | RegExp)[];
}
/**
 * File Watcher feature — monitor file system changes.
 *
 * Provides watch/unwatch functionality with chokidar backend.
 * Reports events: changed, created, deleted (add, unlink, change).
 */
declare class FileWatcherFeature implements FeatureModule {
    readonly meta: FeatureMeta;
    private config;
    private ctx;
    private watches;
    private active;
    private watchCounter;
    init(config: Record<string, unknown>, context: FeatureContext): Promise<void>;
    start(): Promise<void>;
    stop(): Promise<void>;
    healthCheck(): Promise<HealthStatus>;
    /** Start watching a path with an optional glob pattern */
    watch(path: string, pattern?: string | RegExp | (string | RegExp)[], callback?: (event: 'change' | 'create' | 'delete', filePath: string) => void): string;
    /** Stop watching by watch ID */
    unwatch(id: string): boolean;
    /** Stop watching a specific path (all watches for that path) */
    unwatchPath(targetPath: string): number;
    /** List all active watches */
    listWatches(): WatchInfo[];
    /** Get watch count */
    getWatchCount(): number;
}
declare const _default: FileWatcherFeature;
export default _default;
//# sourceMappingURL=index.d.ts.map