"use strict";
/**
 * Self-Evolving Memory Feature
 *
 * Memory system that learns, consolidates, and evolves over time.
 * Merges related memories, extracts patterns, and prunes stale data.
 */
Object.defineProperty(exports, "__esModule", { value: true });
/**
 * Self-Evolving memory — memory consolidation and pattern discovery.
 *
 * Periodically reviews stored memories, merges similar ones,
 * discovers patterns, and prunes stale entries to maintain
 * a relevant and efficient memory store.
 */
class SelfEvolvingMemoryFeature {
    meta = {
        name: 'self-evolving-memory',
        version: '0.0.3',
        description: 'Memory consolidation, pattern discovery, and evolution',
        dependencies: [],
    };
    config = {
        enabled: false,
        consolidationIntervalMs: 3600000, // 1 hour
        similarityThreshold: 0.85,
        minAccessCount: 2,
        maxMemories: 50000,
        pruneAfterDays: 90,
    };
    ctx;
    memories = new Map();
    patterns = new Map();
    timer = null;
    async init(config, context) {
        this.ctx = context;
        this.config = { ...this.config, ...config };
    }
    async start() {
        this.timer = setInterval(() => {
            this.runConsolidation();
        }, this.config.consolidationIntervalMs);
        this.ctx.logger.info('Self-Evolving Memory active', {
            consolidationInterval: `${this.config.consolidationIntervalMs / 1000}s`,
            similarityThreshold: this.config.similarityThreshold,
        });
    }
    async stop() {
        if (this.timer) {
            clearInterval(this.timer);
            this.timer = null;
        }
    }
    async healthCheck() {
        return {
            healthy: true,
            details: {
                memories: this.memories.size,
                patterns: this.patterns.size,
                consolidated: Array.from(this.memories.values()).filter(m => m.consolidated).length,
            },
        };
    }
    /** Add a new memory */
    addMemory(content, sourceIds = [], importance = 0.5) {
        const id = `evo_${Date.now()}_${Math.random().toString(36).slice(2, 8)}`;
        const memory = {
            id,
            content,
            sourceMemories: sourceIds,
            embeddings: [], // Would be populated by embedding model
            importance: Math.max(0, Math.min(1, importance)),
            accessCount: 0,
            consolidated: false,
            createdAt: Date.now(),
            updatedAt: Date.now(),
        };
        this.memories.set(id, memory);
        return memory;
    }
    /** Access a memory (boosts its importance) */
    accessMemory(id) {
        const memory = this.memories.get(id);
        if (memory) {
            memory.accessCount++;
            memory.importance = Math.min(1, memory.importance + 0.01);
            memory.updatedAt = Date.now();
        }
    }
    /** Run consolidation cycle */
    async runConsolidation() {
        this.ctx.logger.debug('Starting memory consolidation', {
            memories: this.memories.size,
        });
        let mergedCount = 0;
        const toRemove = [];
        // Find similar memories to merge
        const allMemories = Array.from(this.memories.values());
        for (let i = 0; i < allMemories.length; i++) {
            for (let j = i + 1; j < allMemories.length; j++) {
                const a = allMemories[i];
                const b = allMemories[j];
                if (a.consolidated && b.consolidated)
                    continue;
                const similarity = this.computeSimilarity(a.content, b.content);
                if (similarity >= this.config.similarityThreshold) {
                    // Merge b into a
                    a.content = `[Merged] ${a.content}\n---\n${b.content}`;
                    a.sourceMemories = [...new Set([...a.sourceMemories, ...b.sourceMemories, b.id])];
                    a.importance = Math.max(a.importance, b.importance);
                    a.consolidated = true;
                    a.updatedAt = Date.now();
                    toRemove.push(b.id);
                    mergedCount++;
                }
            }
        }
        // Remove merged memories
        for (const id of toRemove) {
            this.memories.delete(id);
        }
        // Discover patterns
        await this.discoverPatterns();
        // Prune stale memories
        await this.pruneStale();
        this.ctx.logger.info('Consolidation complete', {
            merged: mergedCount,
            pruned: toRemove.length,
            patterns: this.patterns.size,
            remaining: this.memories.size,
        });
    }
    /** Discover patterns in memory */
    async discoverPatterns() {
        const memories = Array.from(this.memories.values());
        // Simple frequency-based pattern discovery
        const wordCounts = new Map();
        for (const mem of memories) {
            const words = mem.content.toLowerCase().split(/\s+/);
            for (const word of words) {
                if (word.length > 4) {
                    wordCounts.set(word, (wordCounts.get(word) ?? 0) + 1);
                }
            }
        }
        // Find recurring themes
        for (const [word, count] of wordCounts) {
            if (count >= 5) { // Appears in at least 5 memories
                const matchingMemories = memories
                    .filter(m => m.content.toLowerCase().includes(word))
                    .map(m => m.id);
                const patternId = `pattern_${word}`;
                this.patterns.set(patternId, {
                    id: patternId,
                    type: 'recurring',
                    description: `Recurring theme: "${word}" appears in ${count} memories`,
                    memoryIds: matchingMemories,
                    confidence: Math.min(1, count / memories.length),
                    discoveredAt: Date.now(),
                });
            }
        }
    }
    /** Prune stale memories based on age and access count */
    async pruneStale() {
        const now = Date.now();
        const maxAge = this.config.pruneAfterDays * 24 * 60 * 60 * 1000;
        for (const [id, memory] of this.memories) {
            const age = now - memory.createdAt;
            if (age > maxAge && memory.accessCount < this.config.minAccessCount && !memory.consolidated) {
                this.memories.delete(id);
            }
        }
        // Enforce max memories by removing lowest importance
        while (this.memories.size > this.config.maxMemories) {
            let lowest = null;
            for (const mem of this.memories.values()) {
                if (!lowest || mem.importance < lowest.importance) {
                    lowest = mem;
                }
            }
            if (lowest) {
                this.memories.delete(lowest.id);
            }
            else {
                break;
            }
        }
    }
    /** Get all patterns */
    getPatterns() {
        return Array.from(this.patterns.values());
    }
    /** Get memories related to a pattern */
    getPatternMemories(patternId) {
        const pattern = this.patterns.get(patternId);
        if (!pattern)
            return [];
        return pattern.memoryIds
            .map(id => this.memories.get(id))
            .filter((m) => m !== undefined);
    }
    /** Simple text similarity (Jaccard on words) */
    computeSimilarity(a, b) {
        const wordsA = new Set(a.toLowerCase().split(/\s+/));
        const wordsB = new Set(b.toLowerCase().split(/\s+/));
        const intersection = new Set([...wordsA].filter(w => wordsB.has(w)));
        const union = new Set([...wordsA, ...wordsB]);
        return union.size === 0 ? 0 : intersection.size / union.size;
    }
}
exports.default = new SelfEvolvingMemoryFeature();
//# sourceMappingURL=self-evolving.js.map