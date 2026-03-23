/**
 * TTS Engine Feature
 *
 * Text-to-Speech synthesis with multiple provider support.
 * Provides audio caching and voice management.
 * Real API calls not implemented - structure only.
 */

import { mkdirSync, existsSync } from 'fs';
import { join } from 'path';
import crypto from 'crypto';
import { FeatureModule, FeatureContext, FeatureMeta, HealthStatus } from '../../core/plugin-loader';

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
  speed?: number; // 0.5-2.0
  pitch?: number; // -20 to +20 (semitones)
  sampleRate?: number; // e.g., 24000, 44100
}

/** TTS synthesis result */
export interface SynthesisResult {
  audioPath: string;
  format: AudioFormat;
  duration?: number; // seconds (if calculated)
  size: number; // bytes
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
class TtsEngineFeature implements FeatureModule {
  readonly meta: FeatureMeta = {
    name: 'tts-engine',
    version: '0.1.0',
    description: 'Text-to-speech engine with provider abstraction and audio caching',
    dependencies: [],
  };

  private config: Required<TtsEngineConfig>;
  private ctx!: FeatureContext;
  private provider: TtsProvider = 'elevenlabs';
  private apiKey: string = '';
  private voices: Voice[] = [];
  private defaultVoice: string = '';

  // Cache
  private cacheDir!: string;
  private cacheIndex: Map<string, { path: string; voice: string; format: AudioFormat; size: number; created: number }> = new Map();

  constructor() {
    this.config = {
      cacheDir: './data/tts-cache',
      maxCacheSizeMB: 1000,
      defaultProvider: 'elevenlabs',
      defaultVoiceId: '',
      defaultFormat: 'mp3',
    };
  }

  async init(config: Record<string, unknown>, context: FeatureContext): Promise<void> {
    this.ctx = context;
    this.config = {
      cacheDir: (config['cacheDir'] as string) ?? this.config['cacheDir'],
      maxCacheSizeMB: (config['maxCacheSizeMB'] as number) ?? this.config['maxCacheSizeMB'],
      defaultProvider: (config['defaultProvider'] as TtsProvider) ?? this.config['defaultProvider'],
      defaultVoiceId: (config['defaultVoiceId'] as string) ?? this.config['defaultVoiceId'],
      defaultFormat: (config['defaultFormat'] as AudioFormat) ?? this.config['defaultFormat'],
    };

    this.cacheDir = this.config.cacheDir;
    this.provider = this.config.defaultProvider;
    this.defaultVoice = this.config.defaultVoiceId ?? '';

    this.initCache();
    this.initializeBuiltInVoices();
  }

  async start(): Promise<void> {
    this.ctx.logger.info('TtsEngine active', {
      provider: this.provider,
      cacheDir: this.cacheDir,
      voiceCount: this.voices.length,
    });
  }

  async stop(): Promise<void> {
    this.ctx.logger.info('TtsEngine stopped');
  }

  async healthCheck(): Promise<HealthStatus> {
    try {
      const cacheFiles = this.cacheIndex.size;
      const cacheSizeMB = Array.from(this.cacheIndex.values()).reduce((acc, v) => acc + v.size, 0) / (1024 * 1024);

      return {
        healthy: true,
        details: {
          provider: this.provider,
          voicesAvailable: this.voices.length,
          cacheEntries: cacheFiles,
          cacheSizeMB: cacheSizeMB.toFixed(2),
        },
      };
    } catch (err) {
      return {
        healthy: false,
        message: err instanceof Error ? err.message : String(err),
      };
    }
  }

  /**
   * Configure the TTS provider.
   *
   * @param provider - Provider name
   * @param apiKey - API key for the provider
   */
  configure(provider: TtsProvider, apiKey: string): void {
    this.provider = provider;
    this.apiKey = apiKey;
    this.ctx.logger.info('TTS provider configured', { provider });
  }

  /**
   * List available voices for the current provider.
   *
   * @returns Array of Voice objects
   */
  listVoices(): Voice[] {
    if (this.voices.length === 0) {
      this.initializeBuiltInVoices();
    }
    return this.voices;
  }

  /**
   * Set the default voice ID.
   */
  setDefaultVoice(voiceId: string): boolean {
    const exists = this.voices.some(v => v.id === voiceId);
    if (exists) {
      this.defaultVoice = voiceId;
      this.ctx.logger.info('Default TTS voice set', { voiceId });
      return true;
    }
    return false;
  }

  /**
   * Synthesize text to speech.
   *
   * @param request - Synthesis request parameters
   * @returns SynthesisResult with path to audio file
   */
  async synthesize(request: SynthesisRequest): Promise<SynthesisResult> {
    const text = request.text.trim();
    if (!text) {
      throw new Error('Text cannot be empty');
    }

    const voiceId = request.voiceId ?? this.defaultVoice;
    if (!voiceId) {
      throw new Error('No voice specified and no default voice set');
    }

    const format = request.format ?? this.config.defaultFormat;
    const voice = this.voices.find(v => v.id === voiceId);

    if (!voice) {
      throw new Error(`Voice not found: ${voiceId}`);
    }

    // Generate cache key based on text, voice, and parameters
    const cacheKey = this.generateCacheKey(text, voiceId, request);

    // Check cache
    const cached = this.cacheIndex.get(cacheKey);
    if (cached) {
      this.ctx.logger.debug('TTS cache hit', { cacheKey });
      return {
        audioPath: cached.path,
        format: cached.format,
        cached: true,
        voiceId,
        provider: this.provider,
        size: cached.size,
      };
    }

    this.ctx.logger.debug('TTS cache miss, synthesizing', { cacheKey });

    // Real synthesis would happen here via provider API
    // For stub, we create a silent dummy audio file or simulate
    const audioPath = await this.synthesizeInternal(text, voice, request);

    // Get file size
    const stats = await import('fs').then(fs => fs.statSync(audioPath));
    const size = stats.size;

    // Cache the result
    this.cacheIndex.set(cacheKey, {
      path: audioPath,
      voice: voiceId,
      format,
      size,
      created: Date.now(),
    });

    // Persist cache index to SQLite? For now just in-memory; could add DB index

    this.ctx.logger.info('TTS synthesized', { cacheKey, voiceId, format, sizeBytes: size });

    return {
      audioPath,
      format,
      cached: false,
      voiceId,
      provider: this.provider,
      size,
    };
  }

  /**
   * Clear the audio cache.
   *
   * @param olderThanMs - Optional: only delete files older than this (ms)
   * @returns number of files deleted
   */
  clearCache(olderThanMs?: number): number {
    let deleted = 0;
    const now = Date.now();

    for (const [cacheKey, info] of this.cacheIndex) {
      if (olderThanMs && now - info.created < olderThanMs) {
        continue;
      }

      try {
        const fs = require('fs');
        if (fs.existsSync(info.path)) {
          fs.unlinkSync(info.path);
        }
        this.cacheIndex.delete(cacheKey);
        deleted++;
      } catch (err) {
        this.ctx.logger.warn('Failed to delete cached file', { path: info.path, error: err instanceof Error ? err.message : String(err) });
      }
    }

    if (deleted > 0) {
      this.ctx.logger.info('TTS cache cleared', { deleted });
    }

    return deleted;
  }

  /**
   * Get cache statistics.
   */
  getCacheStats(): { entries: number; sizeMB: number } {
    const size = Array.from(this.cacheIndex.values()).reduce((acc, v) => acc + v.size, 0);
    return {
      entries: this.cacheIndex.size,
      sizeMB: size / (1024 * 1024),
    };
  }

  /**
   * Internal synthesis stub - creates empty/silent file.
   * Real implementation would call provider API and save audio.
   */
  private async synthesizeInternal(text: string, voice: Voice, request: SynthesisRequest): Promise<string> {
    // Generate filename
    const ext = request.format ?? this.config.defaultFormat;
    const filename = `${crypto.randomBytes(16).toString('hex')}.${ext}`;
    const filepath = join(this.cacheDir, filename);

    // Ensure cache directory exists
    if (!existsSync(this.cacheDir)) {
      mkdirSync(this.cacheDir, { recursive: true });
    }

    // STUB: Create a silent/placeholder audio file
    // In real implementation, would fetch from provider and write binary audio
    // For now, write a text-based placeholder for structure validation
    const placeholder = `TTS STUB: ${this.provider} - Voice: ${voice.name} - Text: ${text}\n`;
    require('fs').writeFileSync(filepath, placeholder);

    return filepath;
  }

  /** Generate a content-addressed cache key */
  private generateCacheKey(text: string, voiceId: string, request: SynthesisRequest): string {
    const data = `${this.provider}:${voiceId}:${text}:${request.format ?? this.config.defaultFormat}:${request.speed ?? 1}:${request.pitch ?? 0}:${request.sampleRate ?? 24000}`;
    return crypto.createHash('sha256').update(data).digest('hex');
  }

  /** Initialize cache directory */
  private initCache(): void {
    if (!existsSync(this.cacheDir)) {
      mkdirSync(this.cacheDir, { recursive: true });
    }
  }

  /** Initialize built-in voices for each provider */
  private initializeBuiltInVoices(): void {
    // Default voices - in real implementation these would be fetched from provider APIs
    if (this.provider === 'elevenlabs') {
      this.voices = [
        { id: '21m00Tcm4TlvDq8ikWAM', name: 'Rachel', provider: 'elevenlabs', language: 'en', gender: 'female' },
        { id: 'AZnzlk1XvdvUeBnXmlld', name: 'Domi', provider: 'elevenlabs', language: 'en', gender: 'female' },
        { id: 'EXAVITQu4vr4xnSDxMaL', name: 'Bella', provider: 'elevenlabs', language: 'en', gender: 'female' },
        { id: 'ErXwoba YiNxjP1xXxNwZ', name: 'Antoni', provider: 'elevenlabs', language: 'en', gender: 'male' },
        { id: 'MF3mGyEYCl7XYWbV9V6O', name: 'Elli', provider: 'elevenlabs', language: 'en', gender: 'female' },
        { id: 'TxGEqnHWrfWFTfGW9XjX', name: 'Josh', provider: 'elevenlabs', language: 'en', gender: 'male' },
        { id: 'VR6AewLTigWG4x5ukaTc', name: 'Arnold', provider: 'elevenlabs', language: 'en', gender: 'male' },
        { id: 'pNInz6obpgDQGcFmaJgB', name: 'Adam', provider: 'elevenlabs', language: 'en', gender: 'male' },
        { id: 'yoZ06aMxZJJ28mfd3LOQ', name: 'Sam', provider: 'elevenlabs', language: 'en', gender: 'male' },
      ];
      this.defaultVoice = this.defaultVoice || '21m00Tcm4TlvDq8ikWAM';
    } else if (this.provider === 'google') {
      this.voices = [
        { id: 'en-US-Wavenet-D', name: 'Wavenet D', provider: 'google', language: 'en-US', gender: 'female' },
        { id: 'en-US-Wavenet-F', name: 'Wavenet F', provider: 'google', language: 'en-US', gender: 'male' },
        { id: 'en-US-Wavenet-J', name: 'Wavenet J', provider: 'google', language: 'en-US', gender: 'male' },
      ];
      this.defaultVoice = this.defaultVoice || 'en-US-Wavenet-D';
    } else if (this.provider === 'azure') {
      this.voices = [
        { id: 'en-US-JennyNeural', name: 'Jenny', provider: 'azure', language: 'en-US', gender: 'female' },
        { id: 'en-US-GuyNeural', name: 'Guy', provider: 'azure', language: 'en-US', gender: 'male' },
      ];
      this.defaultVoice = this.defaultVoice || 'en-US-JennyNeural';
    } else if (this.provider === 'local') {
      this.voices = [
        { id: 'local-default', name: 'Local System', provider: 'local', language: 'en', description: 'Uses system TTS engine' },
      ];
      this.defaultVoice = this.defaultVoice || 'local-default';
    }
  }
}

export default new TtsEngineFeature();
