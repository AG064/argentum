/**
 * STT Engine Feature
 *
 * Speech-to-Text transcription with multiple provider support.
 * Supports audio file transcription with language selection.
 * Transcription history is stored in SQLite.
 */
import { type FeatureModule, type FeatureContext, type FeatureMeta, type HealthStatus } from '../../core/plugin-loader';
/** Supported STT providers */
export type SttProvider = 'whisper' | 'google' | 'azure' | 'local';
/** Transcription request */
export interface TranscriptionRequest {
    audioPath: string;
    language?: string;
    model?: string;
    enableTimestamps?: boolean;
}
/** Transcription result */
export interface TranscriptionResult {
    text: string;
    language?: string;
    provider: SttProvider;
    duration?: number;
    words?: Array<{
        word: string;
        start: number;
        end: number;
    }>;
    confidence?: number;
    processingTimeMs: number;
    cached: boolean;
}
/** Transcription history entry */
export interface TranscriptionHistory {
    id: string;
    audioFilename: string;
    audioSize: number;
    text: string;
    language: string;
    provider: SttProvider;
    duration: number;
    processingTimeMs: number;
    confidence: number;
    transcribed_at: number;
}
/** Feature configuration */
export interface SttEngineConfig {
    dbPath?: string;
    defaultProvider?: SttProvider;
    defaultLanguage?: string;
    maxHistoryEntries?: number;
    cacheDir?: string;
}
/**
 * SttEngine — multi-provider speech-to-text transcription.
 *
 * Provides:
 * - Provider abstraction (Whisper, Google, Azure, Local)
 * - Audio file processing structure
 * - Transcription history with metadata
 * - Language and model selection
 *
 * Real API integration is not implemented; providers are stubs.
 */
declare class SttEngineFeature implements FeatureModule {
    readonly meta: FeatureMeta;
    private config;
    private ctx;
    private db;
    private provider;
    private apiKey;
    constructor();
    init(config: Record<string, unknown>, context: FeatureContext): Promise<void>;
    start(): Promise<void>;
    stop(): Promise<void>;
    healthCheck(): Promise<HealthStatus>;
    /**
     * Configure the STT provider.
     *
     * @param provider - Provider name
     * @param apiKey - API key for the provider (if required)
     */
    configure(provider: SttProvider, apiKey?: string): void;
    /**
     * Transcribe an audio file.
     *
     * @param request - Transcription request
     * @returns TranscriptionResult with text and metadata
     */
    transcribe(request: TranscriptionRequest): Promise<TranscriptionResult>;
    /**
     * Get transcription history.
     *
     * @param limit - Max entries to return
     * @returns History entries ordered by timestamp descending
     */
    getHistory(limit?: number): TranscriptionHistory[];
    /**
     * Get transcription statistics.
     */
    getStats(): {
        total: number;
        byProvider: Record<string, number>;
        avgProcessingTimeMs: number;
        avgConfidence: number;
    };
    /**
     * Clear old cache entries.
     *
     * @param olderThanDays - Delete cache entries older than this many days
     * @returns number of entries deleted
     */
    clearCache(olderThanDays?: number): number;
    /** Log transcription to history */
    private logHistory;
    /** Compute SHA256 hash of audio file for cache key */
    private getAudioHash;
    /** Initialize database and create tables */
    private initDatabase;
}
declare const _default: SttEngineFeature;
export default _default;
//# sourceMappingURL=index.d.ts.map