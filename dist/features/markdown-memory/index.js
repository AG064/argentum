"use strict";
/**
 * Markdown Memory Feature
 *
 * File-system based memory using Markdown files with frontmatter.
 * Human-readable, git-friendly storage for knowledge and notes.
 */
Object.defineProperty(exports, "__esModule", { value: true });
const markdown_1 = require("../../memory/markdown");
/**
 * MarkdownMemoryFeature — markdown file-based memory store.
 *
 * Stores memories as .md files with YAML frontmatter.
 * Provides write, read, append, and full-text search.
 */
class MarkdownMemoryFeature {
    meta = {
        name: 'markdown-memory',
        version: '0.0.3',
        description: 'Markdown file-based memory store with frontmatter',
        dependencies: [],
    };
    config;
    ctx;
    storage;
    constructor() {
        this.config = {
            basePath: './data/markdown-memory',
        };
    }
    async init(config, context) {
        this.ctx = context;
        this.config = {
            basePath: config['basePath'] ?? this.config['basePath'],
        };
        this.storage = new markdown_1.MarkdownMemory(this.config.basePath);
        this.storage.init();
    }
    async start() {
        this.ctx.logger.info('MarkdownMemory active', {
            basePath: this.config.basePath,
        });
    }
    async stop() {
        this.ctx.logger.info('MarkdownMemory stopped');
    }
    async healthCheck() {
        try {
            const entries = this.storage.list();
            return {
                healthy: true,
                details: {
                    totalFiles: entries.length,
                    basePath: this.config.basePath,
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
    /** Write a new markdown file (or replace existing) */
    write(filename, content, metadata, tags) {
        const entry = this.storage.save({
            filename,
            title: filename,
            content,
            metadata: metadata || {},
            tags: tags || [],
        });
        this.ctx.logger.debug('Markdown written', { filename });
        return entry;
    }
    /** Read a markdown file */
    read(filename) {
        const entry = this.storage.get(filename);
        if (!entry)
            return null;
        return {
            content: entry.content,
            metadata: entry.metadata,
            tags: entry.tags,
        };
    }
    /** Append a section to an existing markdown file */
    append(filename, section) {
        const entry = this.storage.get(filename);
        if (!entry) {
            this.ctx.logger.warn('Cannot append, file not found', { filename });
            return false;
        }
        // Append with a horizontal rule separator if content exists
        const separator = entry.content.length > 0 ? '\n\n---\n\n' : '';
        const newContent = entry.content + separator + section;
        this.storage.save({
            filename,
            title: filename,
            content: newContent,
            tags: entry.tags,
            metadata: entry.metadata,
        });
        this.ctx.logger.debug('Appended to markdown', { filename });
        return true;
    }
    /** Full-text search across markdown files */
    search(query, limit = 20) {
        const results = this.storage.search(query, limit);
        return results.map((entry) => ({
            filename: entry.filename,
            title: entry.title,
            content: entry.content,
            tags: entry.tags,
            // The backend doesn't compute a score, so we set a default
            relevance: 1.0,
        }));
    }
    /** List all files */
    listAll() {
        return this.storage.list().map((e) => e.filename);
    }
    /** Get by tag */
    getByTag(tag) {
        const entries = this.storage.getByTag(tag);
        return entries.map((entry) => ({
            filename: entry.filename,
            title: entry.title,
            content: entry.content,
            tags: entry.tags,
        }));
    }
    /** Delete a file */
    delete(filename) {
        const result = this.storage.delete(filename);
        if (result) {
            this.ctx.logger.debug('Markdown deleted', { filename });
        }
        return result;
    }
}
exports.default = new MarkdownMemoryFeature();
//# sourceMappingURL=index.js.map