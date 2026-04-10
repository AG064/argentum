/**
 * Voice Feature for AG-Claw
 *
 * Text-to-Speech (TTS), Speech-to-Text (STT) via ElevenLabs and OpenAI.
 */
import { type FeatureModule, type FeatureContext, type FeatureMeta, type HealthStatus } from '../../core/plugin-loader';
/** Voice feature configuration */
export interface VoiceConfig {
    enabled: boolean;
    provider: 'elevenlabs' | 'openai' | 'local';
    voice: string;
    model: string;
    sttProvider: 'whisper' | 'google' | 'local';
    wakeWord?: string;
    apiKey?: string;
}
/** TTS request parameters */
export interface TTSRequest {
    text: string;
    voice?: string;
    model?: string;
    speed?: number;
    format?: 'mp3' | 'wav' | 'ogg';
}
/** TTS response */
export interface TTSResponse {
    audio: Buffer;
    format: string;
    duration: number;
    provider: string;
}
/** Voice info from provider */
export interface VoiceInfo {
    id: string;
    name: string;
    category?: string;
    description?: string;
    labels?: Record<string, string>;
}
/** Generate speech using ElevenLabs API */
export declare function generateSpeech(text: string, voiceId?: string, options?: {
    model?: string;
    stability?: number;
    similarityBoost?: number;
}): Promise<Buffer>;
/** List available voices from ElevenLabs */
export declare function listVoices(): Promise<VoiceInfo[]>;
/** Get voice info by ID or name */
export declare function getVoice(voiceId: string): Promise<VoiceInfo | null>;
declare class VoiceFeature implements FeatureModule {
    readonly meta: FeatureMeta;
    private config;
    private ctx;
    init(config: Record<string, unknown>, context: FeatureContext): Promise<void>;
    start(): Promise<void>;
    stop(): Promise<void>;
    healthCheck(): Promise<HealthStatus>;
    /** Generate speech audio */
    textToSpeech(text: string, voiceId?: string): Promise<Buffer>;
    /** List available voices */
    getVoices(): Promise<VoiceInfo[]>;
    /** Transcribe audio */
    speechToText(audio: Buffer, format?: string): Promise<string>;
}
declare const _default: VoiceFeature;
export default _default;
//# sourceMappingURL=index.d.ts.map