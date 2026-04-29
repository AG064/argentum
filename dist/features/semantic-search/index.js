"use strict";
/**
 * Semantic Search Feature
 *
 * Vector-based semantic memory search using embeddings.
 * Wraps the SemanticMemory backend with a simplified API.
 */
Object.defineProperty(exports, "__esModule", { value: true });
const semantic_1 = require("../../memory/semantic");
/**
 * SemanticSearchFeature — semantic similarity search over stored memories.
 *
 * Stores text with embeddings and supports both full-text and
 * semantic (vector) search. Ideal for finding related concepts
 * without exact keyword matches.
 */
class SemanticSearchFeature {
    meta = {
        name: 'semantic-search',
        version: '0.0.3',
        description: 'Semantic vector search with embedding-based similarity',
        dependencies: [],
    };
    config;
    ctx;
    memory;
    constructor() {
        this.config = {
            dbPath: './data/semantic-memory.db',
            defaultType: 'semantic',
        };
    }
    async init(config, context) {
        this.ctx = context;
        this.config = {
            dbPath: config['dbPath'] ?? this.config['dbPath'],
            defaultType: config['defaultType'] ?? this.config['defaultType'],
        };
        // Initialize the singleton with custom dbPath if needed
        this.memory = (0, semantic_1.getSemanticMemory)(this.config.dbPath);
        // The singleton ensures init() is called
    }
    async start() {
        this.ctx.logger.info('SemanticSearch active', {
            dbPath: this.config.dbPath,
            defaultType: this.config.defaultType,
        });
    }
    async stop() {
        this.memory.close();
        this.ctx.logger.info('SemanticSearch stopped');
    }
    async healthCheck() {
        try {
            const count = await this.memory.count();
            return {
                healthy: true,
                details: {
                    totalMemories: count,
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
    /** Index/store a text entry with optional metadata */
    async index(text, metadata) {
        return this.memory.store(this.config.defaultType, text, metadata);
    }
    /** Semantic + full-text search */
    async search(query, limit = 10) {
        return this.memory.search(query, limit);
    }
    /** Delete a memory by its ID */
    async delete(docId) {
        return this.memory.delete(docId);
    }
    /** Get a memory by ID */
    async get(docId) {
        return this.memory.getById(docId);
    }
    /** Get memories by type */
    async getByType(type, limit = 50) {
        return this.memory.getByType(type, limit);
    }
    /** Consolidate memory (deduplicate, decay) */
    async consolidate() {
        return this.memory.consolidate();
    }
    /** Save checkpoint for a task */
    async checkpoint(taskId, state) {
        return this.memory.checkpoint(taskId, state);
    }
    /** Resume a checkpointed task */
    async resume(taskId) {
        return this.memory.resume(taskId);
    }
}
exports.default = new SemanticSearchFeature();
//# sourceMappingURL=index.js.map