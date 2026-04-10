/**
 * Evening Recap Feature
 *
 * End-of-day summary with accomplishments, pending tasks,
 * tomorrow's preview, and daily metrics.
 */
import { type FeatureModule, type FeatureContext, type FeatureMeta, type HealthStatus } from '../../core/plugin-loader';
/** Evening recap configuration */
export interface EveningRecapConfig {
    enabled: boolean;
    deliveryTime: string;
    timezone: string;
    includeAccomplishments: boolean;
    includePendingTasks: boolean;
    includeTomorrowPreview: boolean;
    includeMetrics: boolean;
    includeReflection: boolean;
}
/** Daily metrics */
export interface DailyMetrics {
    tasksCompleted: number;
    tasksPending: number;
    messagesExchanged: number;
    featuresUsed: string[];
    activeTimeMinutes: number;
    focusScore: number;
}
/** Evening recap */
export interface EveningRecap {
    id: string;
    date: string;
    generatedAt: number;
    accomplishments: string[];
    pendingTasks: Array<{
        title: string;
        priority: string;
        dueDate?: string;
    }>;
    tomorrowPreview: Array<{
        title: string;
        time?: string;
    }>;
    metrics: DailyMetrics;
    reflection: string;
    delivered: boolean;
}
/** Recap handler */
export type RecapHandler = (recap: EveningRecap) => Promise<void>;
/**
 * Evening Recap feature — end-of-day summary and reflection.
 *
 * Generates a nightly recap with accomplishments, pending work,
 * tomorrow's preview, and productivity metrics.
 */
declare class EveningRecapFeature implements FeatureModule {
    readonly meta: FeatureMeta;
    private config;
    private ctx;
    private handlers;
    private recapHistory;
    private timer;
    init(config: Record<string, unknown>, context: FeatureContext): Promise<void>;
    start(): Promise<void>;
    stop(): Promise<void>;
    healthCheck(): Promise<HealthStatus>;
    /** Register a recap delivery handler */
    onRecap(handler: RecapHandler): void;
    /** Generate evening recap */
    generateRecap(): Promise<EveningRecap>;
    /** Get recap history */
    getHistory(limit?: number): EveningRecap[];
    /** Record an accomplishment for today */
    recordAccomplishment(description: string): void;
    /** Schedule next recap */
    private scheduleNextRecap;
    private getAccomplishments;
    private getPendingTasks;
    private getTomorrowPreview;
    private getDailyMetrics;
    private generateReflection;
}
declare const _default: EveningRecapFeature;
export default _default;
//# sourceMappingURL=index.d.ts.map