"use strict";
/**
 * Multimodal Memory Feature
 *
 * Stores and retrieves memories across text, images, audio, and documents.
 * Supports semantic search and temporal queries.
 */
Object.defineProperty(exports, "__esModule", { value: true });
/**
 * Multimodal Memory feature — stores and retrieves memories across modalities.
 *
 * Provides a unified memory system for text, images, audio, and documents
 * with semantic search and importance-based retention.
 */
class MultimodalMemoryFeature {
    meta = {
        name: 'multimodal-memory',
        version: '0.0.5',
        description: 'Cross-modal memory storage with semantic search',
        dependencies: [],
    };
    config = {
        enabled: false,
        maxMemorySize: 100000,
        embeddingDimension: 1536,
        retentionDays: 365,
    };
    ctx;
    memories = new Map();
    async init(config, context) {
        this.ctx = context;
        this.config = { ...this.config, ...config };
    }
    async start() {
        this.ctx.logger.info('Multimodal Memory active', {
            maxSize: this.config.maxMemorySize,
            retention: `${this.config.retentionDays}d`,
        });
    }
    async stop() {
        this.memories.clear();
    }
    async healthCheck() {
        const typeCounts = {};
        for (const mem of this.memories.values()) {
            typeCounts[mem.type] = (typeCounts[mem.type] ?? 0) + 1;
        }
        return {
            healthy: true,
            details: { total: this.memories.size, byType: typeCounts },
        };
    }
    /** Store a new memory */
    async store(type, content, metadata = {}, tags = [], importance = 0.5) {
        const id = `mem_${Date.now()}_${Math.random().toString(36).slice(2, 8)}`;
        const entry = {
            id,
            type,
            content,
            metadata,
            tags,
            importance: Math.max(0, Math.min(1, importance)),
            createdAt: Date.now(),
            accessedAt: Date.now(),
            accessCount: 0,
        };
        // Enforce max size by evicting least important entries
        if (this.memories.size >= this.config.maxMemorySize) {
            this.evictLeastImportant();
        }
        this.memories.set(id, entry);
        this.ctx.logger.debug('Memory stored', { id, type, importance });
        return entry;
    }
    /** Retrieve a memory by ID */
    async retrieve(id) {
        const entry = this.memories.get(id);
        if (!entry)
            return null;
        entry.accessedAt = Date.now();
        entry.accessCount++;
        return entry;
    }
    /** Search memories */
    async search(query) {
        const results = [];
        const limit = query.limit ?? 20;
        for (const entry of this.memories.values()) {
            if (query.type && entry.type !== query.type)
                continue;
            if (query.minImportance && entry.importance < query.minImportance)
                continue;
            if (query.startDate && entry.createdAt < query.startDate)
                continue;
            if (query.endDate && entry.createdAt > query.endDate)
                continue;
            if (query.tags?.length && !query.tags.some((t) => entry.tags.includes(t)))
                continue;
            let score = entry.importance;
            if (query.text) {
                const textMatch = this.computeTextSimilarity(query.text, entry.content);
                score = score * 0.3 + textMatch * 0.7;
            }
            results.push({ entry, score });
        }
        results.sort((a, b) => b.score - a.score);
        return results.slice(0, limit);
    }
    /** Delete a memory */
    async delete(id) {
        return this.memories.delete(id);
    }
    /** Evict least important memory */
    evictLeastImportant() {
        let leastImportant = null;
        for (const entry of this.memories.values()) {
            if (!leastImportant || entry.importance < leastImportant.importance) {
                leastImportant = entry;
            }
        }
        if (leastImportant) {
            this.memories.delete(leastImportant.id);
        }
    }
    /** Simple text similarity (Jaccard on word sets) */
    computeTextSimilarity(a, b) {
        const wordsA = new Set(a.toLowerCase().split(/\s+/));
        const wordsB = new Set(b.toLowerCase().split(/\s+/));
        const intersection = new Set([...wordsA].filter((w) => wordsB.has(w)));
        const union = new Set([...wordsA, ...wordsB]);
        return union.size === 0 ? 0 : intersection.size / union.size;
    }
}
exports.default = new MultimodalMemoryFeature();
//# sourceMappingURL=index.js.map