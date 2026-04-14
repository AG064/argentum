import { type FeatureModule, type FeatureContext, type FeatureMeta, type HealthStatus } from '../../core/plugin-loader';
export interface UpdateInfo {
    version: string;
    url: string;
    releaseNotes: string;
    publishedAt: string;
    isBeta: boolean;
}
export interface UpdateResult {
    success: boolean;
    version: string;
    message: string;
    rollbackAvailable?: boolean;
}
export interface AutoUpdateConfig {
    enabled: boolean;
    dbPath: string;
    repoOwner: string;
    repoName: string;
    checkIntervalHours: number;
    autoApply: boolean;
    backupBeforeUpdate: boolean;
    backupPath: string;
}
declare class AutoUpdateFeature implements FeatureModule {
    readonly meta: FeatureMeta;
    private config;
    private ctx;
    private currentVersion;
    private lastCheck;
    private latestRelease;
    private updateHistory;
    private checkTimer;
    constructor();
    init(config: Record<string, unknown>, context: FeatureContext): Promise<void>;
    start(): Promise<void>;
    stop(): Promise<void>;
    healthCheck(): Promise<HealthStatus>;
    /** Check for updates */
    checkUpdates(): Promise<UpdateInfo | null>;
    /** Get the latest release info (if newer) */
    getChangelog(): Promise<string | null>;
    /** Apply update */
    applyUpdate(_component?: string): Promise<UpdateResult>;
    /** Rollback to previous version */
    rollback(_component?: string): Promise<UpdateResult>;
    /** Get update history */
    getUpdateHistory(limit?: number): Promise<typeof this.updateHistory>;
    private fetchLatestRelease;
    private isNewerVersion;
    private createBackup;
    private restoreBackup;
    private initDatabase;
    private loadUpdateHistory;
    private recordUpdate;
    private db;
}
declare const _default: AutoUpdateFeature;
export default _default;
//# sourceMappingURL=index.d.ts.map