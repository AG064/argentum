import { type FeatureModule, type FeatureContext, type FeatureMeta, type HealthStatus } from '../../core/plugin-loader';
export interface CronJob {
    id: string;
    name: string;
    cronExpr: string;
    handlerId: string;
    enabled: boolean;
    lastRun: number | null;
    lastError: string | null;
    createdAt: number;
}
export interface JobRun {
    jobId: string;
    startedAt: number;
    completedAt: number | null;
    success: boolean;
    error: string | null;
}
export interface CronSchedulerConfig {
    enabled: boolean;
    dbPath: string;
    timezone?: string;
    maxJobs: number;
}
export type CronHandler = (jobId: string) => Promise<void> | void;
declare class CronSchedulerFeature implements FeatureModule {
    readonly meta: FeatureMeta;
    private config;
    private ctx;
    private db;
    private jobs;
    private handlers;
    private cronJobs;
    private schedulerStartTime;
    init(config: Record<string, unknown>, context: FeatureContext): Promise<void>;
    start(): Promise<void>;
    stop(): Promise<void>;
    healthCheck(): Promise<HealthStatus>;
    /** Register a handler function that can be called by jobs */
    registerHandler(handlerId: string, handler: CronHandler): void;
    /** Unregister a handler */
    unregisterHandler(handlerId: string): boolean;
    /** Add a new cron job */
    addJob(name: string, cronExpr: string, handlerId: string, enabled?: boolean): Promise<CronJob>;
    /** Remove a job */
    removeJob(jobId: string): Promise<boolean>;
    /** List all jobs */
    listJobs(): CronJob[];
    /** Get job by ID */
    getJob(jobId: string): Promise<CronJob | null>;
    /** Enable a job */
    enableJob(jobId: string): Promise<boolean>;
    /** Disable a job */
    disableJob(jobId: string): Promise<boolean>;
    /** Run a job immediately (bypassing schedule) */
    runJob(jobId: string): Promise<boolean>;
    /** Trigger a job run by name */
    triggerJobByName(name: string): Promise<number>;
    private scheduleJob;
    private executeHandler;
    private getRecentlyFailed;
    getJobRuns(jobId: string, limit?: number): Promise<JobRun[]>;
    private initDatabase;
    private loadJobsFromDb;
}
declare const _default: CronSchedulerFeature;
export default _default;
//# sourceMappingURL=index.d.ts.map