/**
 * User Modeling Feature (Honcho-style)
 *
 * Tracks user preferences and communication patterns over time.
 * Builds a model of the user's preferences for personalized responses.
 *
 * Similar to Hermes/Honcho dialectic user modeling but simplified.
 *
 * Tracks:
 * - Preferred response length (brief, medium, detailed)
 * - Language preferences
 * - Topics of interest
 * - Communication style
 * - Activity patterns
 */
import type { FeatureModule, FeatureContext, FeatureMeta, HealthStatus } from '../../core/plugin-loader';
export interface UserPreferences {
    responseLength: 'brief' | 'medium' | 'detailed';
    formalityLevel: 'casual' | 'neutral' | 'formal';
    emojiUsage: 'minimal' | 'moderate' | 'frequent';
    preferredLanguage: string;
    languagesSpoken: string[];
    topicsOfInterest: string[];
    communicationStyle: 'questioner' | 'directive' | 'collaborative' | 'mixed';
    prefersExplanations: boolean;
    technicalLevel: 'beginner' | 'intermediate' | 'advanced';
    activeHours: number[];
    sessionFrequency: 'daily' | 'few-times-week' | 'weekly' | 'occasional';
    firstSeen: number;
    lastUpdated: number;
    totalInteractions: number;
}
declare class UserModelingFeature implements FeatureModule {
    readonly meta: FeatureMeta;
    private ctx;
    private modelPath;
    private preferences;
    private samples;
    private initialized;
    init(config: Record<string, unknown>, context: FeatureContext): Promise<void>;
    start(): Promise<void>;
    stop(): Promise<void>;
    healthCheck(): Promise<HealthStatus>;
    /**
     * Record a conversation sample to improve user model
     */
    recordSample(sample: {
        messageLength: number;
        hasQuestions?: boolean;
        hasTechnicalTerms?: boolean;
        language?: string;
        topics?: string[];
    }): void;
    /**
     * Get current user preferences
     */
    getPreferences(): UserPreferences;
    /**
     * Get user model formatted for system prompt injection
     */
    getModelForPrompt(): string;
    /**
     * Update specific preference manually
     */
    updatePreference<K extends keyof UserPreferences>(key: K, value: UserPreferences[K]): void;
    /**
     * Get active hours (hours when user typically interacts)
     */
    getActiveHours(): number[];
    private loadModel;
    private saveModel;
    private parseModelFile;
    private serializeModel;
    private parseSimpleYaml;
    private updatePreferencesFromSample;
}
declare const _default: UserModelingFeature;
export default _default;
//# sourceMappingURL=index.d.ts.map