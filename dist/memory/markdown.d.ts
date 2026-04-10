/**
 * Markdown Memory Backend
 *
 * File-system based memory using Markdown files in a directory tree.
 * Human-readable, git-friendly, and great for documentation-style memories.
 */
/** Markdown memory entry */
export interface MarkdownEntry {
    filename: string;
    title: string;
    content: string;
    tags: string[];
    metadata: Record<string, string>;
    createdAt: Date;
    updatedAt: Date;
}
/**
 * Markdown-based memory backend.
 *
 * Stores memories as .md files in a directory structure. Each file
 * has YAML frontmatter for metadata and Markdown body for content.
 * Perfect for git versioning and human readability.
 */
export declare class MarkdownMemory {
    private basePath;
    constructor(basePath?: string);
    /** Initialize the memory directory */
    init(): void;
    /** Parse a markdown file into an entry */
    private parseMarkdown;
    /** Build YAML frontmatter string */
    private buildFrontmatter;
    /** Store or update a memory entry */
    save(entry: Omit<MarkdownEntry, 'filename' | 'createdAt' | 'updatedAt'> & {
        filename?: string;
    }): string;
    /** Retrieve a memory entry by filename */
    get(filename: string): MarkdownEntry | null;
    /** Delete a memory entry */
    delete(filename: string): boolean;
    /** List all memory entries */
    list(): MarkdownEntry[];
    /** Search entries by content or title */
    search(query: string, limit?: number): MarkdownEntry[];
    /** Get entries by tag */
    getByTag(tag: string): MarkdownEntry[];
    /** Sanitize filename */
    private sanitize;
}
//# sourceMappingURL=markdown.d.ts.map