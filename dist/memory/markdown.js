"use strict";
/**
 * Markdown Memory Backend
 *
 * File-system based memory using Markdown files in a directory tree.
 * Human-readable, git-friendly, and great for documentation-style memories.
 */
Object.defineProperty(exports, "__esModule", { value: true });
exports.MarkdownMemory = void 0;
const fs_1 = require("fs");
const path_1 = require("path");
/**
 * Markdown-based memory backend.
 *
 * Stores memories as .md files in a directory structure. Each file
 * has YAML frontmatter for metadata and Markdown body for content.
 * Perfect for git versioning and human readability.
 */
class MarkdownMemory {
    basePath;
    constructor(basePath = './data/markdown-memory') {
        this.basePath = (0, path_1.resolve)(basePath);
    }
    /** Initialize the memory directory */
    init() {
        if (!(0, fs_1.existsSync)(this.basePath)) {
            (0, fs_1.mkdirSync)(this.basePath, { recursive: true });
        }
    }
    /** Parse a markdown file into an entry */
    parseMarkdown(filename) {
        try {
            // Prevent path traversal: resolve and ensure file stays within basePath
            const fullPath = (0, path_1.resolve)(this.basePath, filename);
            if (!fullPath.startsWith(`${this.basePath}/`)) {
                // allow exact match if equals basePath file
                if (fullPath !== this.basePath)
                    return null;
            }
            const raw = (0, fs_1.readFileSync)(fullPath, 'utf-8');
            // Parse YAML frontmatter
            const frontmatterMatch = raw.match(/^---\n([\s\S]*?)\n---\n([\s\S]*)$/);
            let content = raw;
            const metadata = {};
            const tags = [];
            let title = filename.replace(/\.md$/, '');
            if (frontmatterMatch) {
                const frontmatter = frontmatterMatch[1];
                content = frontmatterMatch[2].trim();
                for (const line of frontmatter.split('\n')) {
                    const idx = line.indexOf(':');
                    if (idx > 0) {
                        const key = line.slice(0, idx).trim();
                        const value = line.slice(idx + 1).trim();
                        if (key === 'tags') {
                            tags.push(...value.split(',').map(t => t.trim()).filter(Boolean));
                        }
                        else if (key === 'title') {
                            title = value;
                        }
                        else {
                            metadata[key] = value;
                        }
                    }
                }
            }
            const stat = require('fs').statSync(fullPath);
            return {
                filename,
                title,
                content,
                tags,
                metadata,
                createdAt: stat.birthtime,
                updatedAt: stat.mtime,
            };
        }
        catch {
            return null;
        }
    }
    /** Build YAML frontmatter string */
    buildFrontmatter(entry) {
        const lines = [`title: ${entry.title ?? 'Untitled'}`];
        if (entry.tags?.length) {
            lines.push(`tags: ${entry.tags.join(', ')}`);
        }
        if (entry.metadata) {
            for (const [k, v] of Object.entries(entry.metadata)) {
                if (k !== 'title' && k !== 'tags') {
                    lines.push(`${k}: ${v}`);
                }
            }
        }
        return `---\n${lines.join('\n')}\n---\n\n`;
    }
    /** Store or update a memory entry */
    save(entry) {
        this.init();
        const filename = entry.filename ?? `${Date.now()}-${this.sanitize(entry.title)}.md`;
        // Basic validation: filename should not contain path separators and be reasonably short
        if (filename.includes('..') || filename.includes('/') || filename.includes('\\')) {
            throw new Error('Invalid filename');
        }
        if (filename.length > 255)
            throw new Error('Filename too long');
        const fullPath = (0, path_1.resolve)(this.basePath, filename);
        if (!fullPath.startsWith(`${this.basePath}/`)) {
            throw new Error('Invalid filename path');
        }
        const exists = (0, fs_1.existsSync)(fullPath);
        const existing = exists ? this.parseMarkdown(filename) : null;
        const fullEntry = {
            filename,
            title: entry.title,
            content: entry.content,
            tags: entry.tags,
            metadata: { ...entry.metadata },
            createdAt: existing?.createdAt ?? new Date(),
            updatedAt: new Date(),
        };
        const body = this.buildFrontmatter(fullEntry) + fullEntry.content;
        (0, fs_1.writeFileSync)(fullPath, body, 'utf-8');
        return filename;
    }
    /** Retrieve a memory entry by filename */
    get(filename) {
        // Validate path and prevent traversal
        const fullPath = (0, path_1.resolve)(this.basePath, filename);
        if (!fullPath.startsWith(`${this.basePath}/`))
            return null;
        if (!(0, fs_1.existsSync)(fullPath))
            return null;
        return this.parseMarkdown(filename);
    }
    /** Delete a memory entry */
    delete(filename) {
        const fullPath = (0, path_1.resolve)(this.basePath, filename);
        if (!fullPath.startsWith(`${this.basePath}/`))
            return false;
        if (!(0, fs_1.existsSync)(fullPath))
            return false;
        (0, fs_1.unlinkSync)(fullPath);
        return true;
    }
    /** List all memory entries */
    list() {
        if (!(0, fs_1.existsSync)(this.basePath))
            return [];
        return (0, fs_1.readdirSync)(this.basePath)
            .filter(f => f.endsWith('.md'))
            .map(f => this.parseMarkdown(f))
            .filter((e) => e !== null)
            .sort((a, b) => b.updatedAt.getTime() - a.updatedAt.getTime());
    }
    /** Search entries by content or title */
    search(query, limit = 20) {
        const q = query.toLowerCase();
        return this.list()
            .filter(entry => entry.title.toLowerCase().includes(q) ||
            entry.content.toLowerCase().includes(q) ||
            entry.tags.some(t => t.toLowerCase().includes(q)))
            .slice(0, limit);
    }
    /** Get entries by tag */
    getByTag(tag) {
        return this.list().filter(e => e.tags.includes(tag));
    }
    /** Sanitize filename */
    sanitize(text) {
        return text.toLowerCase().replace(/[^a-z0-9]+/g, '-').replace(/^-|-$/g, '').slice(0, 60);
    }
}
exports.MarkdownMemory = MarkdownMemory;
//# sourceMappingURL=markdown.js.map