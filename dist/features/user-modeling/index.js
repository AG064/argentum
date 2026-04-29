"use strict";
/**
 * User Modeling Feature (Honcho-style)
 *
 * Tracks user preferences and communication patterns over time.
 * Builds a model of the user's preferences for personalized responses.
 *
 * Similar to Hermes/Honcho dialectic user modeling but simplified.
 *
 * Tracks:
 * - Preferred response length (brief, medium, detailed)
 * - Language preferences
 * - Topics of interest
 * - Communication style
 * - Activity patterns
 */
var __createBinding = (this && this.__createBinding) || (Object.create ? (function(o, m, k, k2) {
    if (k2 === undefined) k2 = k;
    var desc = Object.getOwnPropertyDescriptor(m, k);
    if (!desc || ("get" in desc ? !m.__esModule : desc.writable || desc.configurable)) {
      desc = { enumerable: true, get: function() { return m[k]; } };
    }
    Object.defineProperty(o, k2, desc);
}) : (function(o, m, k, k2) {
    if (k2 === undefined) k2 = k;
    o[k2] = m[k];
}));
var __setModuleDefault = (this && this.__setModuleDefault) || (Object.create ? (function(o, v) {
    Object.defineProperty(o, "default", { enumerable: true, value: v });
}) : function(o, v) {
    o["default"] = v;
});
var __importStar = (this && this.__importStar) || (function () {
    var ownKeys = function(o) {
        ownKeys = Object.getOwnPropertyNames || function (o) {
            var ar = [];
            for (var k in o) if (Object.prototype.hasOwnProperty.call(o, k)) ar[ar.length] = k;
            return ar;
        };
        return ownKeys(o);
    };
    return function (mod) {
        if (mod && mod.__esModule) return mod;
        var result = {};
        if (mod != null) for (var k = ownKeys(mod), i = 0; i < k.length; i++) if (k[i] !== "default") __createBinding(result, mod, k[i]);
        __setModuleDefault(result, mod);
        return result;
    };
})();
Object.defineProperty(exports, "__esModule", { value: true });
const fs = __importStar(require("fs"));
const fs_1 = require("fs");
const path = __importStar(require("path"));
const DEFAULT_PREFERENCES = {
    responseLength: 'medium',
    formalityLevel: 'neutral',
    emojiUsage: 'moderate',
    preferredLanguage: 'en',
    languagesSpoken: ['en'],
    topicsOfInterest: [],
    communicationStyle: 'mixed',
    prefersExplanations: true,
    technicalLevel: 'intermediate',
    activeHours: [],
    sessionFrequency: 'daily',
    firstSeen: Date.now(),
    lastUpdated: Date.now(),
    totalInteractions: 0,
};
// ─── Feature ─────────────────────────────────────────────────────────────────
class UserModelingFeature {
    meta = {
        name: 'user-modeling',
        version: '0.0.4',
        description: 'Tracks user preferences and communication patterns (Honcho-style)',
        dependencies: [],
    };
    ctx;
    modelPath = '';
    preferences = { ...DEFAULT_PREFERENCES };
    samples = [];
    initialized = false;
    async init(config, context) {
        this.ctx = context;
        // Determine paths
        const workDir = config['workDir'] ||
            process.env.AGCLAW_WORKDIR ||
            path.join(process.env.HOME || '~', '.openclaw', 'workspace');
        const memoryDir = path.join(workDir, 'memory');
        if (!(0, fs_1.existsSync)(memoryDir)) {
            (0, fs_1.mkdirSync)(memoryDir, { recursive: true });
        }
        this.modelPath = path.join(memoryDir, 'user-modeling.md');
        this.loadModel();
        this.initialized = true;
        this.ctx.logger?.info('UserModeling initialized', {
            modelPath: this.modelPath,
            topics: this.preferences.topicsOfInterest.length,
        });
    }
    async start() {
        // Inject user model into system context if available
        const modelContent = this.getModelForPrompt();
        if (modelContent) {
            this.ctx.logger?.debug('User model loaded for prompt injection');
        }
    }
    async stop() {
        this.saveModel();
    }
    async healthCheck() {
        return {
            healthy: true,
            details: {
                modelPath: this.modelPath,
                totalInteractions: this.preferences.totalInteractions,
                topicsOfInterest: this.preferences.topicsOfInterest.length,
                lastUpdated: new Date(this.preferences.lastUpdated).toISOString(),
            },
        };
    }
    // ══════════════════════════════════════════════════════════════════════════
    // PUBLIC API
    // ══════════════════════════════════════════════════════════════════════════
    /**
     * Record a conversation sample to improve user model
     */
    recordSample(sample) {
        const convSample = {
            timestamp: Date.now(),
            messageLength: sample.messageLength,
            hasQuestions: sample.hasQuestions ?? false,
            hasTechnicalTerms: sample.hasTechnicalTerms ?? false,
            language: sample.language ?? 'en',
        };
        this.samples.push(convSample);
        // Keep only last 100 samples
        if (this.samples.length > 100) {
            this.samples = this.samples.slice(-100);
        }
        // Update preferences based on sample
        this.updatePreferencesFromSample(convSample, sample.topics);
        this.preferences.totalInteractions++;
        this.preferences.lastUpdated = Date.now();
        // Auto-save every 10 interactions
        if (this.preferences.totalInteractions % 10 === 0) {
            this.saveModel();
        }
    }
    /**
     * Get current user preferences
     */
    getPreferences() {
        return { ...this.preferences };
    }
    /**
     * Get user model formatted for system prompt injection
     */
    getModelForPrompt() {
        if (this.preferences.totalInteractions < 3) {
            return ''; // Not enough data yet
        }
        const lines = [];
        lines.push('## User Preferences');
        lines.push('');
        lines.push(`- Response style: ${this.preferences.responseLength}`);
        lines.push(`- Formality: ${this.preferences.formalityLevel}`);
        lines.push(`- Emoji usage: ${this.preferences.emojiUsage}`);
        lines.push(`- Preferred language: ${this.preferences.preferredLanguage}`);
        if (this.preferences.topicsOfInterest.length > 0) {
            lines.push(`- Interests: ${this.preferences.topicsOfInterest.slice(0, 10).join(', ')}`);
        }
        lines.push(`- Communication style: ${this.preferences.communicationStyle}`);
        lines.push(`- Technical level: ${this.preferences.technicalLevel}`);
        lines.push(`- Prefers explanations: ${this.preferences.prefersExplanations}`);
        return lines.join('\n');
    }
    /**
     * Update specific preference manually
     */
    updatePreference(key, value) {
        this.preferences[key] = value;
        this.preferences.lastUpdated = Date.now();
        this.saveModel();
    }
    /**
     * Get active hours (hours when user typically interacts)
     */
    getActiveHours() {
        if (this.samples.length < 5)
            return [];
        const hourCounts = new Map();
        for (const sample of this.samples) {
            const hour = new Date(sample.timestamp).getHours();
            hourCounts.set(hour, (hourCounts.get(hour) || 0) + 1);
        }
        // Return hours with >1 sample
        return [...hourCounts.entries()]
            .filter(([, count]) => count > 1)
            .sort((a, b) => b[1] - a[1])
            .map(([hour]) => hour);
    }
    // ══════════════════════════════════════════════════════════════════════════
    // PRIVATE
    // ══════════════════════════════════════════════════════════════════════════
    loadModel() {
        if (!(0, fs_1.existsSync)(this.modelPath)) {
            this.ctx.logger?.info('No existing user model found, starting fresh');
            return;
        }
        try {
            const content = fs.readFileSync(this.modelPath, 'utf8');
            this.parseModelFile(content);
            this.ctx.logger?.info('User model loaded', {
                interactions: this.preferences.totalInteractions,
            });
        }
        catch (err) {
            this.ctx.logger?.warn('Failed to load user model', { error: String(err) });
        }
    }
    saveModel() {
        try {
            const content = this.serializeModel();
            fs.writeFileSync(this.modelPath, content, 'utf8');
        }
        catch (err) {
            this.ctx.logger?.warn('Failed to save user model', { error: String(err) });
        }
    }
    parseModelFile(content) {
        // Parse YAML frontmatter if present
        const frontmatterMatch = content.match(/^---\n([\s\S]*?)\n---\n/);
        let body = content;
        if (frontmatterMatch) {
            try {
                const yaml = this.parseSimpleYaml(frontmatterMatch[1]);
                // Validate and assign typed values
                const responseLength = yaml['responseLength'];
                if (responseLength && ['brief', 'medium', 'detailed'].includes(responseLength)) {
                    this.preferences.responseLength = responseLength;
                }
                const formalityLevel = yaml['formalityLevel'];
                if (formalityLevel && ['casual', 'neutral', 'formal'].includes(formalityLevel)) {
                    this.preferences.formalityLevel = formalityLevel;
                }
                const emojiUsage = yaml['emojiUsage'];
                if (emojiUsage && ['minimal', 'moderate', 'frequent'].includes(emojiUsage)) {
                    this.preferences.emojiUsage = emojiUsage;
                }
                const preferredLanguage = yaml['preferredLanguage'];
                if (preferredLanguage && typeof preferredLanguage === 'string') {
                    this.preferences.preferredLanguage = preferredLanguage;
                }
                const languagesSpoken = yaml['languagesSpoken'];
                if (languagesSpoken && Array.isArray(languagesSpoken)) {
                    this.preferences.languagesSpoken = languagesSpoken;
                }
                const topicsOfInterest = yaml['topicsOfInterest'];
                if (topicsOfInterest && Array.isArray(topicsOfInterest)) {
                    this.preferences.topicsOfInterest = topicsOfInterest;
                }
                const communicationStyle = yaml['communicationStyle'];
                if (communicationStyle &&
                    ['questioner', 'directive', 'collaborative', 'mixed'].includes(communicationStyle)) {
                    this.preferences.communicationStyle = communicationStyle;
                }
                const prefersExplanations = yaml['prefersExplanations'];
                if (prefersExplanations !== undefined && prefersExplanations !== null) {
                    this.preferences.prefersExplanations = Boolean(prefersExplanations);
                }
                const technicalLevel = yaml['technicalLevel'];
                if (technicalLevel && ['beginner', 'intermediate', 'advanced'].includes(technicalLevel)) {
                    this.preferences.technicalLevel = technicalLevel;
                }
                const activeHours = yaml['activeHours'];
                if (activeHours && Array.isArray(activeHours)) {
                    this.preferences.activeHours = activeHours;
                }
                const sessionFrequency = yaml['sessionFrequency'];
                if (sessionFrequency &&
                    ['daily', 'few-times-week', 'weekly', 'occasional'].includes(sessionFrequency)) {
                    this.preferences.sessionFrequency = sessionFrequency;
                }
                const firstSeen = yaml['firstSeen'];
                if (typeof firstSeen === 'number') {
                    this.preferences.firstSeen = firstSeen;
                }
                const lastUpdated = yaml['lastUpdated'];
                if (typeof lastUpdated === 'number') {
                    this.preferences.lastUpdated = lastUpdated;
                }
                const totalInteractions = yaml['totalInteractions'];
                if (typeof totalInteractions === 'number') {
                    this.preferences.totalInteractions = totalInteractions;
                }
            }
            catch { }
            body = content.slice(frontmatterMatch[0].length);
        }
        // Parse topic suggestions from body
        const topicMatches = body.match(/#{1,2}\s*(?:topics?|interests?|learned about):?\s*([^\n#-]+)/gi);
        if (topicMatches) {
            for (const match of topicMatches) {
                const topics = match
                    .replace(/#{1,2}\s*(?:topics?|interests?|learned about):?\s*/i, '')
                    .split(/[,;]/)
                    .map((t) => t.trim().toLowerCase())
                    .filter((t) => t.length > 2);
                for (const topic of topics) {
                    if (!this.preferences.topicsOfInterest.includes(topic)) {
                        this.preferences.topicsOfInterest.push(topic);
                    }
                }
            }
        }
    }
    serializeModel() {
        const fm = [
            '---',
            `responseLength: ${this.preferences.responseLength}`,
            `formalityLevel: ${this.preferences.formalityLevel}`,
            `emojiUsage: ${this.preferences.emojiUsage}`,
            `preferredLanguage: ${this.preferences.preferredLanguage}`,
            `languagesSpoken: [${this.preferences.languagesSpoken.join(', ')}]`,
            `topicsOfInterest: [${this.preferences.topicsOfInterest.join(', ')}]`,
            `communicationStyle: ${this.preferences.communicationStyle}`,
            `prefersExplanations: ${this.preferences.prefersExplanations}`,
            `technicalLevel: ${this.preferences.technicalLevel}`,
            `activeHours: [${this.preferences.activeHours.join(', ')}]`,
            `sessionFrequency: ${this.preferences.sessionFrequency}`,
            `firstSeen: ${this.preferences.firstSeen}`,
            `lastUpdated: ${this.preferences.lastUpdated}`,
            `totalInteractions: ${this.preferences.totalInteractions}`,
            '---',
            '',
            '# User Modeling Data',
            '',
            'This file tracks user preferences inferred from conversation patterns.',
            'Do not edit manually unless necessary.',
            '',
        ].join('\n');
        return fm;
    }
    parseSimpleYaml(yaml) {
        const result = {};
        const lines = yaml.split('\n');
        for (const line of lines) {
            const colonIdx = line.indexOf(':');
            if (colonIdx === -1)
                continue;
            const key = line.slice(0, colonIdx).trim();
            const rawValue = line.slice(colonIdx + 1).trim();
            if (!key)
                continue;
            // Handle arrays
            if (rawValue.startsWith('[') && rawValue.endsWith(']')) {
                const arrContent = rawValue.slice(1, -1).trim();
                result[key] = arrContent
                    ? arrContent
                        .split(',')
                        .map((s) => s.trim())
                        .filter((s) => s.length > 0)
                    : [];
            }
            else if (rawValue === 'true') {
                result[key] = true;
            }
            else if (rawValue === 'false') {
                result[key] = false;
            }
            else if (rawValue !== '' && !isNaN(Number(rawValue))) {
                result[key] = Number(rawValue);
            }
            else if (rawValue !== '') {
                result[key] = rawValue;
            }
        }
        return result;
    }
    updatePreferencesFromSample(sample, topics) {
        // Update response length based on message length
        if (sample.messageLength < 100) {
            this.preferences.responseLength = 'brief';
        }
        else if (sample.messageLength < 500) {
            if (this.preferences.responseLength === 'brief') {
                this.preferences.responseLength = 'medium';
            }
        }
        else {
            this.preferences.responseLength = 'detailed';
        }
        // Update technical level
        if (sample.hasTechnicalTerms) {
            this.preferences.technicalLevel = 'advanced';
        }
        // Update topics of interest
        if (topics && topics.length > 0) {
            for (const topic of topics) {
                const normalized = topic.toLowerCase().trim();
                if (normalized.length > 2 && !this.preferences.topicsOfInterest.includes(normalized)) {
                    this.preferences.topicsOfInterest.push(normalized);
                }
            }
            // Keep only top 20 topics
            if (this.preferences.topicsOfInterest.length > 20) {
                this.preferences.topicsOfInterest = this.preferences.topicsOfInterest.slice(-20);
            }
        }
        // Update communication style
        if (sample.hasQuestions) {
            this.preferences.communicationStyle = 'questioner';
        }
        // Update active hours
        const hour = new Date(sample.timestamp).getHours();
        if (!this.preferences.activeHours.includes(hour)) {
            this.preferences.activeHours.push(hour);
            if (this.preferences.activeHours.length > 14) {
                this.preferences.activeHours = this.preferences.activeHours.slice(-14);
            }
        }
    }
}
exports.default = new UserModelingFeature();
//# sourceMappingURL=index.js.map