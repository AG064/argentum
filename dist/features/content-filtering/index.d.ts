/**
 * Content Filtering Feature
 *
 * Filters content for profanity, PII (emails, phone numbers, credit cards, SSN),
 * and sensitive data. Supports custom regex-based rules.
 *
 * All filtering is done client-side with no external API calls.
 */
import { type FeatureModule, type FeatureContext, type FeatureMeta, type HealthStatus } from '../../core/plugin-loader';
/** Content filtering rule */
export interface FilterRule {
    id: string;
    type: 'profanity' | 'pii' | 'custom';
    name: string;
    pattern: string;
    enabled: boolean;
    replacement?: string;
    created_at: number;
}
/** Content check result */
export interface ContentCheckResult {
    clean: boolean;
    violations: Array<{
        type: string;
        ruleId: string;
        ruleName: string;
        matchedText: string;
        position: number;
    }>;
    filteredText: string;
}
/** Feature configuration */
export interface ContentFilteringConfig {
    dbPath?: string;
    defaultReplacement?: string;
    autoFilterProfanity?: boolean;
    enabledRuleTypes?: string[];
}
/**
 * ContentFiltering — comprehensive content filtering system.
 *
 * Provides:
 * - Built-in PII detection (email, phone, credit card, SSN)
 * - Custom regex-based rules
 * - Profanity filtering (extensible)
 * - SQLite-backed rule storage
 */
declare class ContentFilteringFeature implements FeatureModule {
    readonly meta: FeatureMeta;
    private config;
    private ctx;
    private db;
    private readonly builtInRules;
    constructor();
    init(config: Record<string, unknown>, context: FeatureContext): Promise<void>;
    start(): Promise<void>;
    stop(): Promise<void>;
    healthCheck(): Promise<HealthStatus>;
    /**
     * Check text for violations and optionally filter it.
     *
     * @param text - Input text to check
     * @param filter - If true, returns filtered text with violations replaced
     * @returns ContentCheckResult with violations and optionally filtered text
     */
    check(text: string, filter?: boolean): ContentCheckResult;
    /**
     * Add a custom filtering rule.
     *
     * @param type - Rule type (custom only for user rules)
     * @param pattern - Regex pattern string
     * @param name - Human-readable name
     * @param replacement - Replacement text (optional, uses default if not set)
     * @returns The created rule with generated ID
     */
    addRule(type: 'custom', pattern: string, name: string, replacement?: string): FilterRule;
    /**
     * Remove a custom rule by ID.
     *
     * @param id - Rule ID to remove
     * @returns true if rule was removed, false if not found
     */
    removeRule(id: string): boolean;
    /**
     * List all custom rules.
     *
     * @returns Array of custom rules
     */
    listRules(): FilterRule[];
    /**
     * Enable or disable a rule.
     *
     * @param id - Rule ID
     * @param enabled - true to enable, false to disable
     */
    setRuleEnabled(id: string, enabled: boolean): boolean;
    /** Get built-in rule by ID */
    private _getBuiltInRule;
    /** Get custom rule by ID (from DB) */
    private getCustomRule;
    /** Initialize database and create tables */
    private initDatabase;
    /** Insert built-in rules as defaults if not already present */
    private initBuiltInRules;
    /** Replace a substring at a specific position */
    private replaceAt;
}
declare const _default: ContentFilteringFeature;
export default _default;
//# sourceMappingURL=index.d.ts.map