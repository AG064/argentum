/**
 * Auto-Capture Feature
 *
 * OMEGA Memory integration — automatically detects and captures
 * decisions, lessons, errors, and preferences from conversations.
 */
import { type FeatureModule, type FeatureContext, type FeatureMeta, type HealthStatus } from '../../core/plugin-loader';
/** Capture configuration */
export interface AutoCaptureConfig {
    enabled: boolean;
    captureDecisions: boolean;
    captureLessons: boolean;
    captureErrors: boolean;
    capturePreferences: boolean;
    minConfidence: number;
}
/** Detected capture item */
export interface CaptureItem {
    type: 'decision' | 'lesson' | 'error' | 'preference' | 'general';
    content: string;
    confidence: number;
    source: string;
}
/**
 * AutoCapture — detects valuable information in conversation text.
 *
 * Patterns for detection:
 * - Decisions: "let's use", "I'll go with", "we decided", "the plan is"
 * - Lessons: "I learned", "don't do", "the trick is", "pro tip"
 * - Errors: "the error was", "fixed by", "the issue was", "bug was"
 * - Preferences: "I prefer", "always use", "never use", "best practice"
 */
declare class AutoCaptureFeature implements FeatureModule {
    readonly meta: FeatureMeta;
    private config;
    private ctx;
    private capturedCount;
    private patterns;
    init(config: Record<string, unknown>, context: FeatureContext): Promise<void>;
    start(): Promise<void>;
    healthCheck(): Promise<HealthStatus>;
    /** Initialize detection patterns */
    private initPatterns;
    /** Handle incoming message */
    private handleMessage;
    /** Handle outgoing response */
    private handleResponse;
    /** Extract text from hook data */
    private extractText;
    /** Detect captures from text */
    detectCaptures(text: string, source: string): CaptureItem[];
    /** Save a captured item to semantic memory */
    private saveCapture;
    /** Manually analyze text (for external use) */
    analyzeText(text: string, source?: string): Promise<CaptureItem[]>;
    /** Get capture statistics */
    getStats(): {
        total: number;
        patterns: number;
    };
}
declare const _default: AutoCaptureFeature;
export default _default;
//# sourceMappingURL=index.d.ts.map