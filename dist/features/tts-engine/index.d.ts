/**
 * TTS Engine Feature
 *
 * Text-to-Speech synthesis with multiple provider support.
 * Provides audio caching and voice management.
 * Real API calls not implemented - structure only.
 */
import { type FeatureModule, type FeatureContext, type FeatureMeta, type HealthStatus } from '../../core/plugin-loader';
/** Supported TTS providers */
export type TtsProvider = 'elevenlabs' | 'google' | 'azure' | 'local';
/** Audio format */
export type AudioFormat = 'mp3' | 'wav' | 'ogg' | 'pcm';
/** Voice definition */
export interface Voice {
    id: string;
    name: string;
    provider: TtsProvider;
    language: string;
    gender?: 'male' | 'female' | 'neutral';
    description?: string;
    previewUrl?: string;
}
/** TTS synthesis request */
export interface SynthesisRequest {
    text: string;
    voiceId?: string;
    format?: AudioFormat;
    speed?: number;
    pitch?: number;
    sampleRate?: number;
}
/** TTS synthesis result */
export interface SynthesisResult {
    audioPath: string;
    format: AudioFormat;
    duration?: number;
    size: number;
    cached: boolean;
    voiceId: string;
    provider: TtsProvider;
}
/** Feature configuration */
export interface TtsEngineConfig {
    cacheDir?: string;
    maxCacheSizeMB?: number;
    defaultProvider?: TtsProvider;
    defaultVoiceId?: string;
    defaultFormat?: AudioFormat;
}
/**
 * TtsEngine — multi-provider text-to-speech synthesis.
 *
 * Provides:
 * - Provider abstraction (ElevenLabs, Google, Azure, Local)
 * - Audio file caching with content-addressed filenames
 * - Voice listing and management
 * - Synthesis with customizable parameters
 *
 * Real API integration is not implemented; providers should be subclassed/implemented
 * by the user or via skill plugins.
 */
declare class TtsEngineFeature implements FeatureModule {
    readonly meta: FeatureMeta;
    private config;
    private ctx;
    private provider;
    private apiKey;
    private voices;
    private defaultVoice;
    private cacheDir;
    private cacheIndex;
    constructor();
    init(config: Record<string, unknown>, context: FeatureContext): Promise<void>;
    start(): Promise<void>;
    stop(): Promise<void>;
    healthCheck(): Promise<HealthStatus>;
    /**
     * Configure the TTS provider.
     *
     * @param provider - Provider name
     * @param apiKey - API key for the provider
     */
    configure(provider: TtsProvider, apiKey: string): void;
    /**
     * List available voices for the current provider.
     *
     * @returns Array of Voice objects
     */
    listVoices(): Voice[];
    /**
     * Set the default voice ID.
     */
    setDefaultVoice(voiceId: string): boolean;
    /**
     * Synthesize text to speech.
     *
     * @param request - Synthesis request parameters
     * @returns SynthesisResult with path to audio file
     */
    synthesize(request: SynthesisRequest): Promise<SynthesisResult>;
    /**
     * Clear the audio cache.
     *
     * @param olderThanMs - Optional: only delete files older than this (ms)
     * @returns number of files deleted
     */
    clearCache(olderThanMs?: number): number;
    /**
     * Get cache statistics.
     */
    getCacheStats(): {
        entries: number;
        sizeMB: number;
    };
    /**
     * Internal synthesis stub - creates empty/silent file.
     * Real implementation would call provider API and save audio.
     */
    private synthesizeInternal;
    /** Generate a content-addressed cache key */
    private generateCacheKey;
    /** Initialize cache directory */
    private initCache;
    /** Initialize built-in voices for each provider */
    private initializeBuiltInVoices;
}
declare const _default: TtsEngineFeature;
export default _default;
//# sourceMappingURL=index.d.ts.map