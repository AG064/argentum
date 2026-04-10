/**
 * Atomic Task Checkout Feature
 *
 * Prevents multiple agents from working on the same task simultaneously.
 * Uses SQLite atomic UPDATE with row locking for safety.
 */
import { type FeatureModule, type FeatureContext, type FeatureMeta, type HealthStatus } from '../../core/plugin-loader';
export interface TaskAssignment {
    taskId: string;
    agentId: string;
    checkedOutAt: number;
    expiresAt: number;
}
export interface TaskCheckoutConfig {
    enabled: boolean;
    dbPath: string;
    leaseDurationMs: number;
    maxLeasesPerAgent: number;
}
declare class TaskCheckoutFeature implements FeatureModule {
    readonly meta: FeatureMeta;
    private config;
    private db;
    private ctx;
    private cleanupTimer;
    init(config: Record<string, unknown>, context: FeatureContext): Promise<void>;
    start(): Promise<void>;
    stop(): Promise<void>;
    healthCheck(): Promise<HealthStatus>;
    /**
     * Atomically checkout a task for an agent.
     * Returns true if checkout succeeded, false if task is already taken.
     */
    checkout(taskId: string, agentId: string): Promise<boolean>;
    /** Release a task checkout */
    release(taskId: string): Promise<void>;
    /** Release all tasks for an agent */
    releaseAgent(agentId: string): Promise<number>;
    /** Get all currently checked out tasks */
    getCheckedOutTasks(): Promise<TaskAssignment[]>;
    /** Check if a task is available for checkout */
    isAvailable(taskId: string): Promise<boolean>;
    /** Get tasks checked out by a specific agent */
    getAgentTasks(agentId: string): Promise<TaskAssignment[]>;
    /** Extend a lease */
    extendLease(taskId: string): Promise<boolean>;
    private initDatabase;
    private getCheckedOutTasksSync;
    private getExpiredCount;
    private releaseExpired;
}
declare const _default: TaskCheckoutFeature;
export default _default;
//# sourceMappingURL=index.d.ts.map