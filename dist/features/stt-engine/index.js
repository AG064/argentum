"use strict";
/**
 * STT Engine Feature
 *
 * Speech-to-Text transcription with multiple provider support.
 * Supports audio file transcription with language selection.
 * Transcription history is stored in SQLite.
 */
var __importDefault = (this && this.__importDefault) || function (mod) {
    return (mod && mod.__esModule) ? mod : { "default": mod };
};
Object.defineProperty(exports, "__esModule", { value: true });
const crypto_1 = __importDefault(require("crypto"));
const fs_1 = require("fs");
const path_1 = require("path");
const better_sqlite3_1 = __importDefault(require("better-sqlite3"));
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
class SttEngineFeature {
    meta = {
        name: 'stt-engine',
        version: '0.0.5',
        description: 'Speech-to-text engine with provider abstraction and history logging',
        dependencies: [],
    };
    config;
    ctx;
    db;
    provider = 'whisper';
    apiKey = '';
    constructor() {
        this.config = {
            dbPath: './data/stt-engine.db',
            defaultProvider: 'whisper',
            defaultLanguage: 'en',
            maxHistoryEntries: 10000,
            cacheDir: './data/stt-cache',
        };
    }
    async init(config, context) {
        this.ctx = context;
        this.config = {
            dbPath: config['dbPath'] ?? this.config['dbPath'],
            defaultProvider: config['defaultProvider'] ?? this.config['defaultProvider'],
            defaultLanguage: config['defaultLanguage'] ?? this.config['defaultLanguage'],
            maxHistoryEntries: config['maxHistoryEntries'] ?? this.config['maxHistoryEntries'],
            cacheDir: config['cacheDir'] ?? this.config['cacheDir'],
        };
        this.provider = this.config.defaultProvider;
        this.initDatabase();
        if (!(0, fs_1.existsSync)(this.config.cacheDir)) {
            (0, fs_1.mkdirSync)(this.config.cacheDir, { recursive: true });
        }
    }
    async start() {
        this.ctx.logger.info('SttEngine active', {
            provider: this.provider,
            dbPath: this.config.dbPath,
        });
    }
    async stop() {
        if (this.db) {
            this.db.close();
            this.ctx.logger.info('SttEngine stopped');
        }
    }
    async healthCheck() {
        try {
            const historyCount = this.db.prepare('SELECT COUNT(*) as c FROM transcription_history').get().c;
            const todayCount = this.db
                .prepare(`
        SELECT COUNT(*) as c FROM transcription_history
        WHERE transcribed_at > ?
      `)
                .get(Date.now() - 86400000).c;
            return {
                healthy: true,
                details: {
                    provider: this.provider,
                    totalTranscriptions: historyCount,
                    transcribedToday: todayCount,
                },
            };
        }
        catch (err) {
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
    configure(provider, apiKey) {
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
    async transcribe(request) {
        const { audioPath, language = this.config.defaultLanguage, enableTimestamps: _enableTimestamps = false, } = request;
        // Validate audio file exists
        if (!(0, fs_1.existsSync)(audioPath)) {
            throw new Error(`Audio file not found: ${audioPath}`);
        }
        const audioInfo = (0, fs_1.statSync)(audioPath);
        const startTime = Date.now();
        this.ctx.logger.debug('Transcribing audio', { audioPath, provider: this.provider, language });
        // Check cache first
        const cacheKey = this.getAudioHash(audioPath);
        const cached = this.db
            .prepare('SELECT * FROM transcription_cache WHERE audio_hash = ? AND provider = ?')
            .get(cacheKey, this.provider);
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
        const stubText = `[${this.provider.toUpperCase()} TRANSCRIPTION STUB] Audio file: ${(0, path_1.basename)(audioPath)} (${(audioInfo.size / 1024).toFixed(1)} KB)`;
        const processingTime = Date.now() - startTime;
        // Simulate some variability
        const confidence = 0.85 + Math.random() * 0.14;
        const result = {
            text: stubText,
            language,
            provider: this.provider,
            duration: 0, // Could extract from audio metadata in real implementation
            processingTimeMs: processingTime,
            cached: false,
            confidence,
        };
        // Cache the result
        this.db
            .prepare(`
      INSERT INTO transcription_cache (audio_hash, provider, text, language, cached_at)
      VALUES (?, ?, ?, ?, ?)
    `)
            .run(cacheKey, this.provider, result.text, result.language, Date.now());
        // Log to history
        this.logHistory(request, result.text, audioInfo.size, processingTime, language, confidence);
        this.ctx.logger.info('Transcription complete (stub)', {
            audioPath,
            provider: this.provider,
            chars: result.text.length,
        });
        return result;
    }
    /**
     * Get transcription history.
     *
     * @param limit - Max entries to return
     * @returns History entries ordered by timestamp descending
     */
    getHistory(limit = 100) {
        const rows = this.db
            .prepare(`
      SELECT * FROM transcription_history
      ORDER BY transcribed_at DESC
      LIMIT ?
    `)
            .all(limit);
        return rows.map((r) => ({
            id: r.id,
            audioFilename: r.audio_filename,
            audioSize: r.audio_size,
            text: r.text,
            language: r.language,
            provider: r.provider,
            duration: r.duration,
            processingTimeMs: r.processing_time_ms,
            confidence: r.confidence,
            transcribed_at: r.transcribed_at,
        }));
    }
    /**
     * Get transcription statistics.
     */
    getStats() {
        const total = this.db.prepare('SELECT COUNT(*) as c FROM transcription_history').get().c;
        const byProviderRows = this.db
            .prepare(`
      SELECT provider, COUNT(*) as count FROM transcription_history GROUP BY provider
    `)
            .all();
        const byProvider = {};
        for (const row of byProviderRows) {
            byProvider[row.provider] = row.count;
        }
        const avg = this.db
            .prepare(`
      SELECT AVG(processing_time_ms) as avgTime, AVG(confidence) as avgConf
      FROM transcription_history
    `)
            .get();
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
    clearCache(olderThanDays = 7) {
        const cutoff = Date.now() - olderThanDays * 24 * 60 * 60 * 1000;
        const result = this.db
            .prepare('DELETE FROM transcription_cache WHERE cached_at < ?')
            .run(cutoff);
        if (result.changes > 0) {
            this.ctx.logger.info('STT cache cleared', { deleted: result.changes, olderThanDays });
        }
        return result.changes;
    }
    /** Log transcription to history */
    logHistory(request, text, audioSize, processingTimeMs, language, confidence) {
        const id = `stt-${Date.now()}-${crypto_1.default.randomBytes(8).toString('hex')}`;
        this.db
            .prepare(`
      INSERT INTO transcription_history (id, audio_filename, audio_size, text, language, provider, duration, processing_time_ms, confidence, transcribed_at)
      VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
    `)
            .run(id, (0, path_1.basename)(request.audioPath), audioSize, text, language, this.provider, 0, // duration unknown
        processingTimeMs, confidence, Date.now());
        // Enforce max history
        if (this.config.maxHistoryEntries > 0) {
            const count = this.db.prepare('SELECT COUNT(*) as c FROM transcription_history').get().c;
            if (count > this.config.maxHistoryEntries) {
                const deleteCount = count - this.config.maxHistoryEntries;
                this.db
                    .prepare('DELETE FROM transcription_history WHERE id IN (SELECT id FROM transcription_history ORDER BY transcribed_at ASC LIMIT ?)')
                    .run(deleteCount);
            }
        }
    }
    /** Compute SHA256 hash of audio file for cache key */
    getAudioHash(audioPath) {
        const fs = require('fs');
        const hash = crypto_1.default.createHash('sha256');
        const buffer = fs.readFileSync(audioPath);
        hash.update(buffer);
        return hash.digest('hex');
    }
    /** Initialize database and create tables */
    initDatabase() {
        const fullPath = (0, path_1.resolve)(this.config.dbPath);
        if (!(0, fs_1.existsSync)((0, path_1.dirname)(fullPath))) {
            (0, fs_1.mkdirSync)((0, path_1.dirname)(fullPath), { recursive: true });
        }
        this.db = new better_sqlite3_1.default(fullPath);
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
exports.default = new SttEngineFeature();
//# sourceMappingURL=index.js.map