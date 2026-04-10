/**
 * Morning Briefing Feature
 *
 * Generates a daily briefing with calendar events, weather,
 * news, tasks, and personalized insights every morning.
 */
import { type FeatureModule, type FeatureContext, type FeatureMeta, type HealthStatus } from '../../core/plugin-loader';
/** Morning briefing configuration */
export interface MorningBriefingConfig {
    enabled: boolean;
    deliveryTime: string;
    timezone: string;
    includeWeather: boolean;
    includeCalendar: boolean;
    includeNews: boolean;
    includeTasks: boolean;
    includeInsights: boolean;
    weatherLocation: string;
    newsTopics: string[];
    maxNewsItems: number;
}
/** Briefing section */
export interface BriefingSection {
    type: 'weather' | 'calendar' | 'news' | 'tasks' | 'insights' | 'custom';
    title: string;
    items: BriefingItem[];
    priority: number;
}
/** Briefing item */
export interface BriefingItem {
    title: string;
    detail?: string;
    time?: string;
    icon?: string;
    link?: string;
    metadata?: Record<string, unknown>;
}
/** Generated briefing */
export interface MorningBriefing {
    id: string;
    date: string;
    generatedAt: number;
    sections: BriefingSection[];
    summary: string;
    delivered: boolean;
}
/** Briefing handler — called when briefing is generated */
export type BriefingHandler = (briefing: MorningBriefing) => Promise<void>;
/**
 * Morning Briefing feature — daily personalized morning summary.
 *
 * Aggregates calendar, weather, news, and tasks into a concise
 * morning briefing delivered at a configured time.
 */
declare class MorningBriefingFeature implements FeatureModule {
    readonly meta: FeatureMeta;
    private config;
    private ctx;
    private handlers;
    private briefingHistory;
    private timer;
    init(config: Record<string, unknown>, context: FeatureContext): Promise<void>;
    start(): Promise<void>;
    stop(): Promise<void>;
    healthCheck(): Promise<HealthStatus>;
    /** Register a handler for briefing delivery */
    onBriefing(handler: BriefingHandler): void;
    /** Generate a morning briefing */
    generateBriefing(): Promise<MorningBriefing>;
    /** Get briefing history */
    getHistory(limit?: number): MorningBriefing[];
    /** Schedule next briefing */
    private scheduleNextBriefing;
    private getWeatherSection;
    private getCalendarSection;
    private getNewsSection;
    private getTasksSection;
    private getInsightsSection;
    private generateSummary;
}
declare const _default: MorningBriefingFeature;
export default _default;
//# sourceMappingURL=index.d.ts.map