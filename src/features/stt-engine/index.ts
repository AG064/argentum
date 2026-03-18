/**
 * STT Engine Feature
 *
 * Speech-to-Text transcription with multiple provider support.
 * Supports audio file transcription with language selection.
 * Transcription history is stored in SQLite.
 */

import Database from 'better-sqlite3';
import { mkdirSync, existsSync, statSync } from 'fs';
import { dirname, resolve, basename } from 'path';
import crypto from 'crypto';
import { FeatureModule, FeatureContext, FeatureMeta, HealthStatus } from '../../core/plugin-loader';

/** Supported STT providers */
export type SttProvider = 'whisper' | 'google' | 'azure' | 'local';

/** Transcription request */
export interface TranscriptionRequest {
  audioPath: string;
  language?: string; // e.g., 'en', 'ru', 'es'
  model?: string; // provider-specific model name
  enableTimestamps?: boolean; // word-level timestamps
}

/** Transcription result */
export interface TranscriptionResult {
  text: string;
  language?: string;
  provider: SttProvider;
  duration?: number; // audio duration in seconds
  words?: Array<{ word: string; start: number; end: number }>;
  confidence?: number; // 0-1
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
  cacheDir?: string; // For caching intermediate results
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
class SttEngineFeature implements FeatureModule {
  readonly meta: FeatureMeta = {
    name: 'stt-engine',
    version: '0.1.0',
    description: 'Speech-to-text engine with provider abstraction and history logging',
    dependencies: [],
  };

  private config: Required<SttEngineConfig>;
  private ctx!: FeatureContext;
  private db!: Database.Database;
  private provider: SttProvider = 'whisper';
  private apiKey: string = '';

  constructor() {
    this.config = {
      dbPath: './data/stt-engine.db',
      defaultProvider: 'whisper',
      defaultLanguage: 'en',
      maxHistoryEntries: 10000,
      cacheDir: './data/stt-cache',
    };
  }

  async init(config: Record<string, unknown>, context: FeatureContext): Promise<void> {
    this.ctx = context;
    this.config = {
      dbPath: (config.dbPath as string) ?? this.config.dbPath,
      defaultProvider: (config.defaultProvider as SttProvider) ?? this.config.defaultProvider,
      defaultLanguage: (config.defaultLanguage as string) ?? this.config.defaultLanguage,
      maxHistoryEntries: (config.maxHistoryEntries as number) ?? this.config.maxHistoryEntries,
      cacheDir: (config.cacheDir as string) ?? this.config.cacheDir,
    };

    this.provider = this.config.defaultProvider;
    this.initDatabase();

    if (!existsSync(this.config.cacheDir)) {
      mkdirSync(this.config.cacheDir, { recursive: true });
    }
  }

  async start(): Promise<void> {
    this.ctx.logger.info('SttEngine active', {
      provider: this.provider,
      dbPath: this.config.dbPath,
    });
  }

  async stop(): Promise<void> {
    if (this.db) {
      this.db.close();
      this.ctx.logger.info('SttEngine stopped');
    }
  }

  async healthCheck(): Promise<HealthStatus> {
    try {
      const historyCount = (this.db.prepare('SELECT COUNT(*) as c FROM transcription_history').get() as { c: number }).c;
      const todayCount = (this.db.prepare(`
        SELECT COUNT(*) as c FROM transcription_history
        WHERE transcribed_at > ?
      `).get(Date.now() - 86400000) as { c: number }).c;

      return {
        healthy: true,
        details: {
          provider: this.provider,
          totalTranscriptions: historyCount,
          transcribedToday: todayCount,
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
   * Configure the STT provider.
   *
   * @param provider - Provider name
   * @param apiKey - API key for the provider (if required)
   */
  configure(provider: SttProvider, apiKey?: string): void {
    this.provider = provider;
    if (apiKey) {
      this.apiKey = apiKey;
    }
    this.ctx.logger.info('STT provider configured', { provider });
  }

  /**
   * Transcribe an audio file.
   *
   * @param request - Transcription request
   * @returns TranscriptionResult with text and metadata
   */
  async transcribe(request: TranscriptionRequest): Promise<TranscriptionResult> {
    const { audioPath, language = this.config.defaultLanguage, enableTimestamps = false } = request;

    // Validate audio file exists
    if (!existsSync(audioPath)) {
      throw new Error(`Audio file not found: ${audioPath}`);
    }

    const audioInfo = statSync(audioPath);
    const startTime = Date.now();

    this.ctx.logger.debug('Transcribing audio', { audioPath, provider: this.provider, language });

    // Check cache first
    const cacheKey = this.getAudioHash(audioPath);
    const cached = this.db.prepare('SELECT * FROM transcription_cache WHERE audio_hash = ? AND provider = ?').get(cacheKey, this.provider) as
      | { text: string; language: string; cached_at: number }
      | undefined;

    if (cached) {
      const processingTime = Date.now() - startTime;
      this.ctx.logger.debug('Transcription cache hit', { audioPath });

      // Still log to history if not duplicate today
      this.logHistory(request, cached.text, audioInfo.size, processingTime, cached.language, 1.0);

      return {
        text: cached.text,
        language: cached.language,
        provider: this.provider,
        processingTimeMs: processingTime,
        cached: true,
      };
    }

    // Real transcription would happen here via provider API
    // For stub, we return a placeholder text
    const stubText = `[${this.provider.toUpperCase()} TRANSCRIPTION STUB] Audio file: ${basename(audioPath)} (${(audioInfo.size / 1024).toFixed(1)} KB)`;
    const processingTime = Date.now() - startTime;

    // Simulate some variability
    const confidence = 0.85 + Math.random() * 0.14;

    const result: TranscriptionResult = {
      text: stubText,
      language: language,
      provider: this.provider,
      duration: 0, // Could extract from audio metadata in real implementation
      processingTimeMs: processingTime,
      cached: false,
      confidence,
    };

    // Cache the result
    this.db.prepare(`
      INSERT INTO transcription_cache (audio_hash, provider, text, language, cached_at)
      VALUES (?, ?, ?, ?, ?)
    `).run(cacheKey, this.provider, result.text, result.language, Date.now());

    // Log to history
    this.logHistory(request, result.text, audioInfo.size, processingTime, language, confidence);

    this.ctx.logger.info('Transcription complete (stub)', { audioPath, provider: this.provider, chars: result.text.length });

    return result;
  }

  /**
   * Get transcription history.
   *
   * @param limit - Max entries to return
   * @returns History entries ordered by timestamp descending
   */
  getHistory(limit: number = 100): TranscriptionHistory[] {
    const rows = this.db.prepare(`
      SELECT * FROM transcription_history
      ORDER BY transcribed_at DESC
      LIMIT ?
    `).all(limit) as Array<{
      id: string;
      audio_filename: string;
      audio_size: number;
      text: string;
      language: string;
      provider: string;
      duration: number;
      processing_time_ms: number;
      confidence: number;
      transcribed_at: number;
    }>;

    return rows.map(r => ({
      id: r.id,
      audioFilename: r.audio_filename,
      audioSize: r.audio_size,
      text: r.text,
      language: r.language,
      provider: r.provider as SttProvider,
      duration: r.duration,
      processingTimeMs: r.processing_time_ms,
      confidence: r.confidence,
      transcribed_at: r.transcribed_at,
    }));
  }

  /**
   * Get transcription statistics.
   */
  getStats(): {
    total: number;
    byProvider: Record<string, number>;
    avgProcessingTimeMs: number;
    avgConfidence: number;
  } {
    const total = (this.db.prepare('SELECT COUNT(*) as c FROM transcription_history').get() as { c: number }).c;

    const byProviderRows = this.db.prepare(`
      SELECT provider, COUNT(*) as count FROM transcription_history GROUP BY provider
    `).all() as Array<{ provider: string; count: number }>;

    const byProvider: Record<string, number> = {};
    for (const row of byProviderRows) {
      byProvider[row.provider] = row.count;
    }

    const avg = this.db.prepare(`
      SELECT AVG(processing_time_ms) as avgTime, AVG(confidence) as avgConf
      FROM transcription_history
    `).get() as { avgTime: number | null; avgConf: number | null } | undefined;

    return {
      total,
      byProvider,
      avgProcessingTimeMs: avg?.avgTime ?? 0,
      avgConfidence: avg?.avgConf ?? 0,
    };
  }

  /**
   * Clear old cache entries.
   *
   * @param olderThanDays - Delete cache entries older than this many days
   * @returns number of entries deleted
   */
  clearCache(olderThanDays: number = 7): number {
    const cutoff = Date.now() - olderThanDays * 24 * 60 * 60 * 1000;
    const result = this.db.prepare('DELETE FROM transcription_cache WHERE cached_at < ?').run(cutoff);
    if (result.changes > 0) {
      this.ctx.logger.info('STT cache cleared', { deleted: result.changes, olderThanDays });
    }
    return result.changes;
  }

  /** Log transcription to history */
  private logHistory(
    request: TranscriptionRequest,
    text: string,
    audioSize: number,
    processingTimeMs: number,
    language: string,
    confidence: number
  ): void {
    const id = `stt-${Date.now()}-${crypto.randomBytes(8).toString('hex')}`;

    this.db.prepare(`
      INSERT INTO transcription_history (id, audio_filename, audio_size, text, language, provider, duration, processing_time_ms, confidence, transcribed_at)
      VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
    `).run(
      id,
      basename(request.audioPath),
      audioSize,
      text,
      language,
      this.provider,
      0, // duration unknown
      processingTimeMs,
      confidence,
      Date.now()
    );

    // Enforce max history
    if (this.config.maxHistoryEntries > 0) {
      const count = (this.db.prepare('SELECT COUNT(*) as c FROM transcription_history').get() as { c: number }).c;
      if (count > this.config.maxHistoryEntries) {
        const deleteCount = count - this.config.maxHistoryEntries;
        this.db.prepare('DELETE FROM transcription_history WHERE id IN (SELECT id FROM transcription_history ORDER BY transcribed_at ASC LIMIT ?)').run(deleteCount);
      }
    }
  }

  /** Compute SHA256 hash of audio file for cache key */
  private getAudioHash(audioPath: string): string {
    const fs = require('fs');
    const hash = crypto.createHash('sha256');
    const buffer = fs.readFileSync(audioPath);
    hash.update(buffer);
    return hash.digest('hex');
  }

  /** Initialize database and create tables */
  private initDatabase(): void {
    const fullPath = resolve(this.config.dbPath);
    if (!existsSync(dirname(fullPath))) {
      mkdirSync(dirname(fullPath), { recursive: true });
    }

    this.db = new Database(fullPath);
    this.db.pragma('journal_mode = WAL');
    this.db.pragma('synchronous = NORMAL');
    this.db.pragma('foreign_keys = ON');

    this.db.exec(`
      CREATE TABLE IF NOT EXISTS transcription_history (
        id TEXT PRIMARY KEY,
        audio_filename TEXT NOT NULL,
        audio_size INTEGER NOT NULL,
        text TEXT NOT NULL,
        language TEXT NOT NULL,
        provider TEXT NOT NULL,
        duration REAL DEFAULT 0,
        processing_time_ms INTEGER NOT NULL,
        confidence REAL DEFAULT 0,
        transcribed_at INTEGER NOT NULL
      );

      CREATE TABLE IF NOT EXISTS transcription_cache (
        audio_hash TEXT PRIMARY KEY,
        provider TEXT NOT NULL,
        text TEXT NOT NULL,
        language TEXT NOT NULL,
        cached_at INTEGER NOT NULL
      );

      CREATE INDEX IF NOT EXISTS idx_history_transcribed_at ON transcription_history(transcribed_at DESC);
      CREATE INDEX IF NOT EXISTS idx_history_provider ON transcription_history(provider);
      CREATE INDEX IF NOT EXISTS idx_cache_provider ON transcription_cache(provider);
    `);
  }
}

export default new SttEngineFeature();
