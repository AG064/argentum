"use strict";
/**
 * Wake Word Detection Feature
 *
 * Simple pattern-based wake word detection (no ML).
 * Monitors text/audio streams for configured wake words.
 * Emits 'detected' event when wake word is found.
 */
var __importDefault = (this && this.__importDefault) || function (mod) {
    return (mod && mod.__esModule) ? mod : { "default": mod };
};
Object.defineProperty(exports, "__esModule", { value: true });
const events_1 = require("events");
const fs_1 = require("fs");
const path_1 = require("path");
const better_sqlite3_1 = __importDefault(require("better-sqlite3"));
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
class WakeWordFeature extends events_1.EventEmitter {
    meta = {
        name: 'wake-word',
        version: '0.0.5',
        description: 'Pattern-based wake word detection with event notifications',
        dependencies: [],
    };
    config;
    ctx;
    db;
    wakeWords = [];
    running = false;
    defaultSensitivity;
    // Event types
    static EVENT_DETECTED = 'detected';
    constructor() {
        super();
        this.config = {
            dbPath: './data/wake-word.db',
            defaultSensitivity: 0.7,
            maxContextChars: 50,
        };
    }
    async init(config, context) {
        this.ctx = context;
        this.config = {
            dbPath: config['dbPath'] ?? this.config['dbPath'],
            defaultSensitivity: config['defaultSensitivity'] ?? this.config['defaultSensitivity'],
            maxContextChars: config['maxContextChars'] ?? this.config['maxContextChars'],
        };
        this.defaultSensitivity = this.config.defaultSensitivity;
        this.initDatabase();
        this.loadWakeWords();
    }
    async start() {
        this.running = true;
        this.ctx.logger.info('WakeWord active', {
            dbPath: this.config.dbPath,
            activeWords: this.wakeWords.filter((w) => w.enabled).length,
        });
    }
    async stop() {
        this.running = false;
        this.ctx.logger.info('WakeWord stopped');
    }
    async healthCheck() {
        try {
            const total = this.wakeWords.length;
            const enabled = this.wakeWords.filter((w) => w.enabled).length;
            return {
                healthy: true,
                details: {
                    totalWords: total,
                    enabledWords: enabled,
                    running: this.running,
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
     * Add a new wake word.
     *
     * @param word - The wake word phrase
     * @param sensitivity - 0-1, higher = more strict (more likely to trigger)
     * @param caseSensitive - Case-sensitive matching
     * @returns The created WakeWord object
     */
    addWord(word, sensitivity = this.defaultSensitivity, caseSensitive = false) {
        if (!word.trim()) {
            throw new Error('Wake word cannot be empty');
        }
        const id = `ww-${Date.now()}-${Math.random().toString(36).substr(2, 9)}`;
        const _flags = caseSensitive ? '' : 'i';
        const regex = this.buildRegex(word, sensitivity, caseSensitive);
        const wakeWord = {
            id,
            word,
            sensitivity: Math.max(0, Math.min(1, sensitivity)),
            caseSensitive,
            pattern: regex,
            enabled: true,
            created_at: Date.now(),
        };
        this.db
            .prepare(`
      INSERT INTO wake_words (id, word, sensitivity, case_sensitive, pattern, enabled, created_at)
      VALUES (?, ?, ?, ?, ?, ?, ?)
    `)
            .run(id, word, wakeWord.sensitivity, caseSensitive ? 1 : 0, regex.toString(), wakeWord.enabled ? 1 : 0, wakeWord.created_at);
        this.wakeWords.push(wakeWord);
        this.ctx.logger.info('Wake word added', { word, sensitivity, caseSensitive });
        return wakeWord;
    }
    /**
     * Remove a wake word by ID.
     */
    removeWord(id) {
        const index = this.wakeWords.findIndex((w) => w.id === id);
        if (index === -1)
            return false;
        this.wakeWords.splice(index, 1);
        this.db.prepare('DELETE FROM wake_words WHERE id = ?').run(id);
        this.ctx.logger.info('Wake word removed', { id });
        return true;
    }
    /**
     * List all configured wake words.
     */
    listWords() {
        return [...this.wakeWords];
    }
    /**
     * Enable or disable a wake word.
     */
    setWordEnabled(id, enabled) {
        const word = this.wakeWords.find((w) => w.id === id);
        if (!word)
            return false;
        word.enabled = enabled;
        this.db.prepare('UPDATE wake_words SET enabled = ? WHERE id = ?').run(enabled ? 1 : 0, id);
        this.ctx.logger.info('Wake word state changed', { id, enabled });
        return true;
    }
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
    process(text, source) {
        if (!this.running)
            return [];
        const detections = [];
        const _lowerText = this.config.maxContextChars > 0 ? text.substring(0, this.config.maxContextChars * 2) : text;
        for (const word of this.wakeWords) {
            if (!word.enabled)
                continue;
            let match;
            while ((match = word.pattern.exec(text)) !== null) {
                const detection = {
                    wordId: word.id,
                    word: word.word,
                    matchedText: match[0],
                    position: match.index,
                    timestamp: Date.now(),
                    context: this.extractContext(text, match.index, this.config.maxContextChars),
                };
                detections.push(detection);
                // Emit event
                this.emit(WakeWordFeature.EVENT_DETECTED, {
                    ...detection,
                    source,
                });
                // Reset regex lastIndex for global searches
                word.pattern.lastIndex = 0;
            }
        }
        if (detections.length > 0) {
            this.ctx.logger.debug('Wake words detected', {
                count: detections.length,
                words: detections.map((d) => d.word).join(', '),
            });
        }
        return detections;
    }
    /**
     * Simulate detection (for testing or manual trigger).
     */
    simulateDetection(wordId, matchedText) {
        const word = this.wakeWords.find((w) => w.id === wordId);
        if (!word?.enabled)
            return false;
        const detection = {
            wordId: word.id,
            word: word.word,
            matchedText: matchedText ?? word.word,
            position: 0,
            timestamp: Date.now(),
            context: matchedText ?? word.word,
        };
        this.emit(WakeWordFeature.EVENT_DETECTED, detection);
        return true;
    }
    /**
     * Build a regex pattern from a wake word with sensitivity-based fuzziness.
     *
     * For simplicity, sensitivity is used to adjust regex pattern length matching.
     * In a real implementation, sensitivity would control audio threshold, not regex.
     */
    buildRegex(word, sensitivity, caseSensitive) {
        // Escape regex special chars in the wake word
        const escaped = word.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
        // Simple implementation: direct match. Sensitivity affects min match length?
        // In a true audio-based detector, sensitivity would be a signal threshold.
        // For text-based pattern matching, we could allow partial matches based on sensitivity.
        // Here we just use the full word.
        const flags = caseSensitive ? '' : 'i';
        return new RegExp(escaped, flags);
    }
    /** Extract context around a position */
    extractContext(text, position, contextLength) {
        const start = Math.max(0, position - contextLength);
        const end = Math.min(text.length, position + contextLength);
        const ctx = text.substring(start, end);
        return start > 0 ? `...${ctx}` : ctx + (end < text.length ? '...' : '');
    }
    /** Load wake words from database */
    loadWakeWords() {
        const rows = this.db.prepare('SELECT * FROM wake_words ORDER BY created_at').all();
        for (const row of rows) {
            const flags = row.case_sensitive ? '' : 'i';
            try {
                const pattern = new RegExp(row.pattern, flags);
                this.wakeWords.push({
                    id: row.id,
                    word: row.word,
                    sensitivity: row.sensitivity,
                    caseSensitive: row.case_sensitive === 1,
                    pattern,
                    enabled: row.enabled === 1,
                    created_at: row.created_at,
                });
            }
            catch (err) {
                this.ctx.logger.warn('Invalid wake word pattern', {
                    id: row.id,
                    error: err instanceof Error ? err.message : String(err),
                });
            }
        }
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
      CREATE TABLE IF NOT EXISTS wake_words (
        id TEXT PRIMARY KEY,
        word TEXT NOT NULL,
        sensitivity REAL NOT NULL DEFAULT 0.7,
        case_sensitive INTEGER NOT NULL DEFAULT 0,
        pattern TEXT NOT NULL,
        enabled INTEGER NOT NULL DEFAULT 1,
        created_at INTEGER NOT NULL
      );

      CREATE INDEX IF NOT EXISTS idx_wake_enabled ON wake_words(enabled);
    `);
    }
}
exports.default = new WakeWordFeature();
//# sourceMappingURL=index.js.map