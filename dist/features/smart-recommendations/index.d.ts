/**
 * Smart Recommendations Feature
 *
 * Proactive recommendation engine with behavior tracking,
 * pattern learning, morning/evening briefings integration,
 * and adaptive suggestions based on past interactions.
 */
import { type FeatureModule, type FeatureContext, type FeatureMeta, type HealthStatus } from '../../core/plugin-loader';
/** Smart recommendations configuration */
export interface SmartRecommendationsConfig {
    enabled: boolean;
    maxRecommendations: number;
    minConfidence: number;
    learningRate: number;
    decayFactor: number;
    categories: string[];
    behaviorWindowDays: number;
    proactiveCheckIntervalMs: number;
}
/** Recommendation */
export interface Recommendation {
    id: string;
    category: string;
    title: string;
    description: string;
    confidence: number;
    reason: string;
    action?: {
        type: string;
        params: Record<string, unknown>;
    };
    metadata: Record<string, unknown>;
    createdAt: number;
    accepted?: boolean;
    feedbackScore?: number;
    priority: 'low' | 'medium' | 'high';
    expiresAt?: number;
}
/** User behavior event */
export interface BehaviorEvent {
    type: string;
    category: string;
    value: string;
    timestamp: number;
    context?: Record<string, unknown>;
    outcome?: 'positive' | 'negative' | 'neutral';
}
/** Proactive suggestion context */
export interface SuggestionContext {
    timeOfDay: 'morning' | 'afternoon' | 'evening' | 'night';
    dayOfWeek: number;
    lastActions: string[];
    activeCategory?: string;
}
/**
 * Smart Recommendations feature — proactive suggestion engine.
 *
 * Tracks behavior patterns, learns preferences, and generates
 * proactive suggestions including morning/evening briefings.
 */
declare class SmartRecommendationsFeature implements FeatureModule {
    readonly meta: FeatureMeta;
    private config;
    private ctx;
    private behaviorHistory;
    private profiles;
    private recommendations;
    private suggestionTimer;
    private onSuggestionCallback?;
    init(config: Record<string, unknown>, context: FeatureContext): Promise<void>;
    start(): Promise<void>;
    stop(): Promise<void>;
    healthCheck(): Promise<HealthStatus>;
    /** Register callback for proactive suggestions */
    onSuggestion(callback: (rec: Recommendation) => void): void;
    /** Record a user behavior event */
    recordEvent(event: BehaviorEvent): void;
    /** Generate recommendations based on current profiles and context */
    generateRecommendations(ctx?: Partial<SuggestionContext>): Promise<Recommendation[]>;
    /** Run proactive suggestion check */
    private runProactiveCheck;
    /** Build suggestion context from current state */
    private buildSuggestionContext;
    /** Provide feedback on a recommendation */
    provideFeedback(recommendationId: string, accepted: boolean, score?: number): void;
    /** Get top preference profiles */
    getTopPreferences(limit?: number): Array<{
        category: string;
        value: string;
        score: number;
        eventCount: number;
    }>;
    /** Get behavior statistics */
    getStats(): {
        totalEvents: number;
        profiles: number;
        categories: Record<string, number>;
    };
}
declare const _default: SmartRecommendationsFeature;
export default _default;
//# sourceMappingURL=index.d.ts.map