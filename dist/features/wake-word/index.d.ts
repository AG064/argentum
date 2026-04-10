/**
 * Wake Word Detection Feature
 *
 * Simple pattern-based wake word detection (no ML).
 * Monitors text/audio streams for configured wake words.
 * Emits 'detected' event when wake word is found.
 */
import { EventEmitter } from 'events';
import { type FeatureModule, type FeatureContext, type FeatureMeta, type HealthStatus } from '../../core/plugin-loader';
/** Wake word definition */
export interface WakeWord {
    word: string;
    id: string;
    sensitivity: number;
    caseSensitive: boolean;
    pattern: RegExp;
    enabled: boolean;
    created_at: number;
}
/** Detection result */
export interface Detection {
    wordId: string;
    word: string;
    matchedText: string;
    position: number;
    timestamp: number;
    context: string;
}
/** Feature configuration */
export interface WakeWordConfig {
    dbPath?: string;
    defaultSensitivity?: number;
    maxContextChars?: number;
}
/**
 * WakeWord — simple pattern-based wake word detection.
 *
 * Provides:
 * - Wake word registration and management
 * - Pattern matching with configurable sensitivity
 * - Event-driven detection notifications
 * - SQLite storage for configured wake words
 *
 * Note: This is a text-based stub. Real wake word detection would integrate
 * with audio processing (VAD, stream analysis) using Porcupine, Snowboy, etc.
 */
declare class WakeWordFeature extends EventEmitter implements FeatureModule {
    readonly meta: FeatureMeta;
    private config;
    private ctx;
    private db;
    private wakeWords;
    private running;
    private defaultSensitivity;
    static readonly EVENT_DETECTED = "detected";
    constructor();
    init(config: Record<string, unknown>, context: FeatureContext): Promise<void>;
    start(): Promise<void>;
    stop(): Promise<void>;
    healthCheck(): Promise<HealthStatus>;
    /**
     * Add a new wake word.
     *
     * @param word - The wake word phrase
     * @param sensitivity - 0-1, higher = more strict (more likely to trigger)
     * @param caseSensitive - Case-sensitive matching
     * @returns The created WakeWord object
     */
    addWord(word: string, sensitivity?: number, caseSensitive?: boolean): WakeWord;
    /**
     * Remove a wake word by ID.
     */
    removeWord(id: string): boolean;
    /**
     * List all configured wake words.
     */
    listWords(): WakeWord[];
    /**
     * Enable or disable a wake word.
     */
    setWordEnabled(id: string, enabled: boolean): boolean;
    /**
     * Process text/audio for wake words.
     *
     * In a real implementation, this would be called continuously with audio stream.
     * Here it processes a text string (for testing or text-based triggers).
     *
     * @param text - Input text to scan
     * @param source - Optional source identifier (e.g., 'mic-1')
     * @returns Array of Detection results
     */
    process(text: string, source?: string): Detection[];
    /**
     * Simulate detection (for testing or manual trigger).
     */
    simulateDetection(wordId: string, matchedText?: string): boolean;
    /**
     * Build a regex pattern from a wake word with sensitivity-based fuzziness.
     *
     * For simplicity, sensitivity is used to adjust regex pattern length matching.
     * In a real implementation, sensitivity would control audio threshold, not regex.
     */
    private buildRegex;
    /** Extract context around a position */
    private extractContext;
    /** Load wake words from database */
    private loadWakeWords;
    /** Initialize database and create tables */
    private initDatabase;
}
declare const _default: WakeWordFeature;
export default _default;
//# sourceMappingURL=index.d.ts.map