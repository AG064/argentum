"use strict";
/**
 * Smart Recommendations Feature
 *
 * Proactive recommendation engine with behavior tracking,
 * pattern learning, morning/evening briefings integration,
 * and adaptive suggestions based on past interactions.
 */
Object.defineProperty(exports, "__esModule", { value: true });
const SUGGESTION_RULES = [
    {
        id: 'morning-briefing',
        category: 'productivity',
        condition: (ctx) => ctx.timeOfDay === 'morning' && ctx.dayOfWeek >= 1 && ctx.dayOfWeek <= 5,
        generate: () => ({
            id: '',
            category: 'productivity',
            title: 'Start your day with a briefing',
            description: 'Get a summary of your calendar, tasks, and news for today',
            confidence: 0.8,
            reason: 'Morning routine detected',
            priority: 'high',
            action: { type: 'trigger', params: { feature: 'morning-briefing', action: 'generate' } },
            metadata: {},
            createdAt: Date.now(),
        }),
    },
    {
        id: 'evening-recap',
        category: 'productivity',
        condition: (ctx) => ctx.timeOfDay === 'evening',
        generate: () => ({
            id: '',
            category: 'productivity',
            title: 'Review your day',
            description: 'See what you accomplished and plan for tomorrow',
            confidence: 0.7,
            reason: 'Evening routine detected',
            priority: 'medium',
            action: { type: 'trigger', params: { feature: 'evening-recap', action: 'generate' } },
            metadata: {},
            createdAt: Date.now(),
        }),
    },
    {
        id: 'learn-preference',
        category: 'learning',
        condition: (_ctx, profiles) => {
            const learningProfiles = profiles.filter((p) => p.category === 'learning');
            return learningProfiles.length > 0 && learningProfiles.every((p) => p.score > 0.6);
        },
        generate: (_ctx, profiles) => {
            const topLearning = profiles
                .filter((p) => p.category === 'learning')
                .sort((a, b) => b.score - a.score)[0];
            if (!topLearning)
                return null;
            return {
                id: '',
                category: 'learning',
                title: `Continue learning ${topLearning.value}`,
                description: `You've shown strong interest in ${topLearning.value}`,
                confidence: topLearning.score,
                reason: `${topLearning.eventCount} learning interactions`,
                priority: 'low',
                metadata: { topic: topLearning.value },
                createdAt: Date.now(),
            };
        },
    },
    {
        id: 'frequent-task',
        category: 'automation',
        condition: (_ctx, profiles) => profiles.some((p) => p.eventCount > 10 && p.positiveCount / Math.max(1, p.eventCount) > 0.7),
        generate: (_ctx, profiles) => {
            const repetitive = profiles
                .filter((p) => p.eventCount > 10 && p.positiveCount / p.eventCount > 0.7)
                .sort((a, b) => b.eventCount - a.eventCount)[0];
            if (!repetitive)
                return null;
            return {
                id: '',
                category: 'automation',
                title: `Automate "${repetitive.value}"?`,
                description: `You do this frequently (${repetitive.eventCount} times). Consider automating it.`,
                confidence: 0.65,
                reason: 'Detected repetitive positive task',
                priority: 'medium',
                action: { type: 'suggest_automation', params: { task: repetitive.value } },
                metadata: { task: repetitive.value, count: repetitive.eventCount },
                createdAt: Date.now(),
            };
        },
    },
];
/**
 * Smart Recommendations feature — proactive suggestion engine.
 *
 * Tracks behavior patterns, learns preferences, and generates
 * proactive suggestions including morning/evening briefings.
 */
class SmartRecommendationsFeature {
    meta = {
        name: 'smart-recommendations',
        version: '0.0.4',
        description: 'Proactive recommendations with behavior learning and briefings',
        dependencies: [],
    };
    config = {
        enabled: false,
        maxRecommendations: 10,
        minConfidence: 0.3,
        learningRate: 0.1,
        decayFactor: 0.95,
        categories: ['content', 'action', 'workflow', 'learning', 'productivity', 'automation'],
        behaviorWindowDays: 30,
        proactiveCheckIntervalMs: 1800000, // 30 min
    };
    ctx;
    behaviorHistory = [];
    profiles = new Map();
    recommendations = new Map();
    suggestionTimer = null;
    onSuggestionCallback;
    async init(config, context) {
        this.ctx = context;
        this.config = { ...this.config, ...config };
    }
    async start() {
        // Start proactive suggestion checks
        this.suggestionTimer = setInterval(() => {
            this.runProactiveCheck().catch((err) => {
                this.ctx.logger.error('Proactive check error', {
                    error: err instanceof Error ? err.message : String(err),
                });
            });
        }, this.config.proactiveCheckIntervalMs);
        this.ctx.logger.info('Smart Recommendations active', {
            categories: this.config.categories,
            proactiveInterval: `${this.config.proactiveCheckIntervalMs / 1000}s`,
        });
    }
    async stop() {
        if (this.suggestionTimer) {
            clearInterval(this.suggestionTimer);
            this.suggestionTimer = null;
        }
    }
    async healthCheck() {
        return {
            healthy: true,
            details: {
                profiles: this.profiles.size,
                behaviorEvents: this.behaviorHistory.length,
                activeRecommendations: this.recommendations.size,
            },
        };
    }
    /** Register callback for proactive suggestions */
    onSuggestion(callback) {
        this.onSuggestionCallback = callback;
    }
    /** Record a user behavior event */
    recordEvent(event) {
        this.behaviorHistory.push(event);
        const key = `${event.category}:${event.value}`;
        const existing = this.profiles.get(key);
        if (existing) {
            existing.score = existing.score * (1 - this.config.learningRate) + this.config.learningRate;
            existing.lastUpdated = Date.now();
            existing.eventCount++;
            if (event.outcome === 'positive')
                existing.positiveCount++;
            if (event.outcome === 'negative')
                existing.negativeCount++;
        }
        else {
            this.profiles.set(key, {
                category: event.category,
                value: event.value,
                score: this.config.learningRate,
                lastUpdated: Date.now(),
                eventCount: 1,
                positiveCount: event.outcome === 'positive' ? 1 : 0,
                negativeCount: event.outcome === 'negative' ? 1 : 0,
            });
        }
        // Decay other profiles in same category
        for (const [, profile] of this.profiles) {
            if (profile.category === event.category && profile.lastUpdated < event.timestamp) {
                profile.score *= this.config.decayFactor;
            }
        }
        // Trim history
        const maxHistory = 10000;
        if (this.behaviorHistory.length > maxHistory) {
            this.behaviorHistory = this.behaviorHistory.slice(-maxHistory / 2);
        }
    }
    /** Generate recommendations based on current profiles and context */
    async generateRecommendations(ctx) {
        const suggestionCtx = this.buildSuggestionContext(ctx);
        const results = [];
        const profiles = Array.from(this.profiles.values());
        // Apply suggestion rules
        for (const rule of SUGGESTION_RULES) {
            try {
                if (rule.condition(suggestionCtx, profiles)) {
                    const rec = rule.generate(suggestionCtx, profiles);
                    if (rec) {
                        rec.id = `rec_${Date.now()}_${Math.random().toString(36).slice(2, 8)}`;
                        this.recommendations.set(rec.id, rec);
                        results.push(rec);
                    }
                }
            }
            catch (ruleError) {
                this.ctx.logger.warn('Suggestion rule failed', {
                    error: ruleError instanceof Error ? ruleError.message : String(ruleError),
                });
            }
        }
        // Profile-based recommendations
        const sortedProfiles = Array.from(this.profiles.entries()).sort((a, b) => b[1].score - a[1].score);
        for (const [key, profile] of sortedProfiles.slice(0, this.config.maxRecommendations)) {
            if (profile.score < this.config.minConfidence)
                continue;
            const [, value] = key.split(':');
            const rec = {
                id: `rec_${Date.now()}_${Math.random().toString(36).slice(2, 8)}`,
                category: profile.category,
                title: `Based on your ${profile.category} preferences`,
                description: `You've shown interest in "${value}" (${profile.eventCount} interactions, ${Math.round(profile.score * 100)}% confidence)`,
                confidence: Math.round(profile.score * 100) / 100,
                reason: `${profile.eventCount} interactions, ${profile.positiveCount} positive`,
                priority: profile.score > 0.7 ? 'high' : profile.score > 0.4 ? 'medium' : 'low',
                metadata: { key, eventCount: profile.eventCount },
                createdAt: Date.now(),
            };
            this.recommendations.set(rec.id, rec);
            results.push(rec);
        }
        return results.slice(0, this.config.maxRecommendations);
    }
    /** Run proactive suggestion check */
    async runProactiveCheck() {
        const suggestions = await this.generateRecommendations();
        const highPriority = suggestions.filter((s) => s.priority === 'high' && s.confidence >= 0.6);
        for (const suggestion of highPriority) {
            if (this.onSuggestionCallback) {
                try {
                    this.onSuggestionCallback(suggestion);
                }
                catch (callbackError) {
                    this.ctx.logger.warn('Suggestion callback failed', {
                        suggestionId: suggestion.id,
                        error: callbackError instanceof Error ? callbackError.message : String(callbackError),
                    });
                }
            }
            await this.ctx.emit('suggestion:proactive', suggestion);
        }
    }
    /** Build suggestion context from current state */
    buildSuggestionContext(partial) {
        const now = new Date();
        const hour = now.getHours();
        let timeOfDay;
        if (hour < 6)
            timeOfDay = 'night';
        else if (hour < 12)
            timeOfDay = 'morning';
        else if (hour < 18)
            timeOfDay = 'afternoon';
        else
            timeOfDay = 'evening';
        return {
            timeOfDay,
            dayOfWeek: now.getDay(),
            lastActions: this.behaviorHistory.slice(-10).map((e) => e.value),
            ...partial,
        };
    }
    /** Provide feedback on a recommendation */
    provideFeedback(recommendationId, accepted, score) {
        const rec = this.recommendations.get(recommendationId);
        if (!rec)
            return;
        rec.accepted = accepted;
        rec.feedbackScore = score;
        const key = rec.metadata['key'];
        if (key) {
            const profile = this.profiles.get(key);
            if (profile) {
                const adjustment = accepted ? this.config.learningRate : -this.config.learningRate;
                profile.score = Math.max(0, Math.min(1, profile.score + adjustment));
                if (accepted)
                    profile.positiveCount++;
                else
                    profile.negativeCount++;
            }
        }
        // Record feedback as behavior event
        this.recordEvent({
            type: 'feedback',
            category: rec.category,
            value: rec.title,
            timestamp: Date.now(),
            outcome: accepted ? 'positive' : 'negative',
        });
    }
    /** Get top preference profiles */
    getTopPreferences(limit = 10) {
        return Array.from(this.profiles.entries())
            .sort((a, b) => b[1].score - a[1].score)
            .slice(0, limit)
            .map(([key, profile]) => {
            const [, value] = key.split(':');
            return {
                category: profile.category,
                value: value ?? '',
                score: profile.score,
                eventCount: profile.eventCount,
            };
        });
    }
    /** Get behavior statistics */
    getStats() {
        const categories = {};
        for (const event of this.behaviorHistory) {
            categories[event.category] = (categories[event.category] ?? 0) + 1;
        }
        return { totalEvents: this.behaviorHistory.length, profiles: this.profiles.size, categories };
    }
}
exports.default = new SmartRecommendationsFeature();
//# sourceMappingURL=index.js.map