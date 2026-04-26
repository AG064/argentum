"use strict";
/**
 * Content Filtering Feature
 *
 * Filters content for profanity, PII (emails, phone numbers, credit cards, SSN),
 * and sensitive data. Supports custom regex-based rules.
 *
 * All filtering is done client-side with no external API calls.
 */
var __importDefault = (this && this.__importDefault) || function (mod) {
    return (mod && mod.__esModule) ? mod : { "default": mod };
};
Object.defineProperty(exports, "__esModule", { value: true });
const fs_1 = require("fs");
const path_1 = require("path");
const better_sqlite3_1 = __importDefault(require("better-sqlite3"));
/**
 * ContentFiltering — comprehensive content filtering system.
 *
 * Provides:
 * - Built-in PII detection (email, phone, credit card, SSN)
 * - Custom regex-based rules
 * - Profanity filtering (extensible)
 * - SQLite-backed rule storage
 */
class ContentFilteringFeature {
    meta = {
        name: 'content-filtering',
        version: '0.0.1',
        description: 'Content filtering for profanity, PII, and sensitive data with custom rules',
        dependencies: [],
    };
    config;
    ctx;
    db;
    // Built-in patterns
    builtInRules = [
        {
            id: 'pii-email',
            type: 'pii',
            name: 'Email Address',
            pattern: /[a-zA-Z0-9._%+-]+@[a-zA-Z0-9.-]+\.[a-zA-Z]{2,}/g,
        },
        {
            id: 'pii-phone-us',
            type: 'pii',
            name: 'US Phone Number',
            pattern: /(?:\+1\s?)?(?:\(\d{3}\)|\d{3})[\s.-]?\d{3}[\s.-]?\d{4}/g,
        },
        {
            id: 'pii-phone-intl',
            type: 'pii',
            name: 'International Phone',
            pattern: /\+\d{1,3}[\s.-]?\d{1,4}[\s.-]?\d{1,4}[\s.-]?\d{1,9}/g,
        },
        {
            id: 'pii-credit-card',
            type: 'pii',
            name: 'Credit Card',
            pattern: /\b(?:\d{4}[-\s]?){3}\d{4}\b/g,
        },
        {
            id: 'pii-ssn',
            type: 'pii',
            name: 'Social Security Number',
            pattern: /\b\d{3}-\d{2}-\d{4}\b/g,
        },
        {
            id: 'pii-ip-address',
            type: 'pii',
            name: 'IP Address',
            pattern: /\b(?:\d{1,3}\.){3}\d{1,3}\b/g,
        },
    ];
    constructor() {
        this.config = {
            dbPath: './data/content-filtering.db',
            defaultReplacement: '[REDACTED]',
            autoFilterProfanity: true,
            enabledRuleTypes: ['pii', 'profanity', 'custom'],
        };
    }
    async init(config, context) {
        this.ctx = context;
        this.config = {
            dbPath: config['dbPath'] ?? this.config['dbPath'],
            defaultReplacement: config['defaultReplacement'] ?? this.config['defaultReplacement'],
            autoFilterProfanity: config['autoFilterProfanity'] ?? this.config['autoFilterProfanity'],
            enabledRuleTypes: config['enabledRuleTypes'] ?? this.config['enabledRuleTypes'],
        };
        this.initDatabase();
        this.initBuiltInRules();
    }
    async start() {
        this.ctx.logger.info('ContentFiltering active', {
            dbPath: this.config.dbPath,
        });
    }
    async stop() {
        if (this.db) {
            this.db.close();
            this.ctx.logger.info('ContentFiltering stopped');
        }
    }
    async healthCheck() {
        try {
            const ruleCount = this.db.prepare('SELECT COUNT(*) as c FROM rules').get()
                .c;
            const customCount = this.db.prepare('SELECT COUNT(*) as c FROM rules WHERE type = "custom"').get().c;
            return {
                healthy: true,
                details: {
                    totalRules: ruleCount,
                    customRules: customCount,
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
     * Check text for violations and optionally filter it.
     *
     * @param text - Input text to check
     * @param filter - If true, returns filtered text with violations replaced
     * @returns ContentCheckResult with violations and optionally filtered text
     */
    check(text, filter = true) {
        const violations = [];
        let filteredText = text;
        let offset = 0;
        // Check enabled built-in rules
        for (const rule of this.builtInRules) {
            if (!this.config.enabledRuleTypes.includes(rule.type))
                continue;
            const customRule = this.getCustomRule(rule.id);
            if (customRule && !customRule.enabled)
                continue;
            const pattern = customRule?.pattern ? new RegExp(customRule.pattern, 'g') : rule.pattern;
            const replacement = customRule?.replacement ?? this.config.defaultReplacement;
            let match;
            while ((match = pattern.exec(text)) !== null) {
                violations.push({
                    type: rule.type,
                    ruleId: rule.id,
                    ruleName: customRule?.name ?? rule.name,
                    matchedText: match[0],
                    position: match.index,
                });
                if (filter) {
                    filteredText = this.replaceAt(filteredText, match.index + offset, match[0].length, replacement);
                    offset += replacement.length - match[0].length;
                }
            }
        }
        // Check custom rules
        const customRules = this.listRules().filter((r) => r.type === 'custom' && r.enabled);
        for (const rule of customRules) {
            try {
                const pattern = new RegExp(rule.pattern, 'g');
                let match;
                while ((match = pattern.exec(text)) !== null) {
                    violations.push({
                        type: rule.type,
                        ruleId: rule.id,
                        ruleName: rule.name,
                        matchedText: match[0],
                        position: match.index,
                    });
                    if (filter) {
                        filteredText = this.replaceAt(filteredText, match.index + offset, match[0].length, rule.replacement ?? this.config.defaultReplacement);
                        offset += (rule.replacement ?? this.config.defaultReplacement).length - match[0].length;
                    }
                }
            }
            catch {
                this.ctx.logger.warn('Invalid regex pattern', { ruleId: rule.id, pattern: rule.pattern });
            }
        }
        return {
            clean: violations.length === 0,
            violations,
            filteredText,
        };
    }
    /**
     * Add a custom filtering rule.
     *
     * @param type - Rule type (custom only for user rules)
     * @param pattern - Regex pattern string
     * @param name - Human-readable name
     * @param replacement - Replacement text (optional, uses default if not set)
     * @returns The created rule with generated ID
     */
    addRule(type, pattern, name, replacement) {
        const id = `custom-${Date.now()}-${Math.random().toString(36).substr(2, 9)}`;
        const now = Date.now();
        try {
            // Validate regex
            new RegExp(pattern);
        }
        catch (err) {
            throw new Error(`Invalid regex pattern: ${err.message}`);
        }
        const rule = {
            id,
            type,
            name,
            pattern,
            enabled: true,
            replacement,
            created_at: now,
        };
        const stmt = this.db.prepare(`
      INSERT INTO rules (id, type, name, pattern, enabled, replacement, created_at)
      VALUES (?, ?, ?, ?, ?, ?, ?)
    `);
        stmt.run(rule.id, rule.type, rule.name, rule.pattern, rule.enabled ? 1 : 0, rule.replacement ?? null, rule.created_at);
        this.ctx.logger.info('Custom rule added', { ruleId: id, name });
        return rule;
    }
    /**
     * Remove a custom rule by ID.
     *
     * @param id - Rule ID to remove
     * @returns true if rule was removed, false if not found
     */
    removeRule(id) {
        const result = this.db.prepare('DELETE FROM rules WHERE id = ?').run(id);
        if (result.changes > 0) {
            this.ctx.logger.info('Custom rule removed', { ruleId: id });
            return true;
        }
        return false;
    }
    /**
     * List all custom rules.
     *
     * @returns Array of custom rules
     */
    listRules() {
        const rows = this.db
            .prepare('SELECT * FROM rules WHERE type = "custom" ORDER BY created_at DESC')
            .all();
        return rows.map((r) => ({
            id: r.id,
            type: r.type,
            name: r.name,
            pattern: r.pattern,
            enabled: r.enabled === 1,
            replacement: r.replacement ?? undefined,
            created_at: r.created_at,
        }));
    }
    /**
     * Enable or disable a rule.
     *
     * @param id - Rule ID
     * @param enabled - true to enable, false to disable
     */
    setRuleEnabled(id, enabled) {
        const result = this.db
            .prepare('UPDATE rules SET enabled = ? WHERE id = ?')
            .run(enabled ? 1 : 0, id);
        if (result.changes > 0) {
            this.ctx.logger.info('Rule state changed', { ruleId: id, enabled });
            return true;
        }
        return false;
    }
    /** Get built-in rule by ID */
    _getBuiltInRule(id) {
        return this.builtInRules.find((r) => r.id === id);
    }
    /** Get custom rule by ID (from DB) */
    getCustomRule(id) {
        const row = this.db.prepare('SELECT * FROM rules WHERE id = ? AND enabled = 1').get(id);
        if (!row)
            return undefined;
        try {
            return {
                id: row.id,
                type: row.type,
                name: row.name,
                pattern: row.pattern,
                enabled: row.enabled === 1,
                replacement: row.replacement ?? undefined,
                created_at: row.created_at,
            };
        }
        catch {
            return undefined;
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
      CREATE TABLE IF NOT EXISTS rules (
        id TEXT PRIMARY KEY,
        type TEXT NOT NULL CHECK(type IN ('profanity', 'pii', 'custom')),
        name TEXT NOT NULL,
        pattern TEXT NOT NULL,
        enabled INTEGER NOT NULL DEFAULT 1,
        replacement TEXT,
        created_at INTEGER NOT NULL
      );

      CREATE INDEX IF NOT EXISTS idx_rules_type ON rules(type);
      CREATE INDEX IF NOT EXISTS idx_rules_enabled ON rules(enabled);
    `);
    }
    /** Insert built-in rules as defaults if not already present */
    initBuiltInRules() {
        for (const rule of this.builtInRules) {
            const existing = this.db.prepare('SELECT 1 FROM rules WHERE id = ?').get(rule.id);
            if (!existing) {
                this.db
                    .prepare(`
          INSERT INTO rules (id, type, name, pattern, enabled, created_at)
          VALUES (?, ?, ?, ?, ?, ?)
        `)
                    .run(rule.id, rule.type, rule.name, rule.pattern.toString(), 1, Date.now());
            }
        }
    }
    /** Replace a substring at a specific position */
    replaceAt(str, pos, len, replacement) {
        return str.substring(0, pos) + replacement + str.substring(pos + len);
    }
}
exports.default = new ContentFilteringFeature();
//# sourceMappingURL=index.js.map