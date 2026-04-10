/**
 * Markdown Memory Feature
 *
 * File-system based memory using Markdown files with frontmatter.
 * Human-readable, git-friendly storage for knowledge and notes.
 */
import { type FeatureModule, type FeatureContext, type FeatureMeta, type HealthStatus } from '../../core/plugin-loader';
/** Feature configuration */
export interface MarkdownMemoryConfig {
    basePath?: string;
}
/**
 * MarkdownMemoryFeature — markdown file-based memory store.
 *
 * Stores memories as .md files with YAML frontmatter.
 * Provides write, read, append, and full-text search.
 */
declare class MarkdownMemoryFeature implements FeatureModule {
    readonly meta: FeatureMeta;
    private config;
    private ctx;
    private storage;
    constructor();
    init(config: Record<string, unknown>, context: FeatureContext): Promise<void>;
    start(): Promise<void>;
    stop(): Promise<void>;
    healthCheck(): Promise<HealthStatus>;
    /** Write a new markdown file (or replace existing) */
    write(filename: string, content: string, metadata?: Record<string, string>, tags?: string[]): string;
    /** Read a markdown file */
    read(filename: string): {
        content: string;
        metadata: Record<string, string>;
        tags: string[];
    } | null;
    /** Append a section to an existing markdown file */
    append(filename: string, section: string): boolean;
    /** Full-text search across markdown files */
    search(query: string, limit?: number): Array<{
        filename: string;
        title: string;
        content: string;
        tags: string[];
        relevance: number;
    }>;
    /** List all files */
    listAll(): string[];
    /** Get by tag */
    getByTag(tag: string): Array<{
        filename: string;
        title: string;
        content: string;
        tags: string[];
    }>;
    /** Delete a file */
    delete(filename: string): boolean;
}
declare const _default: MarkdownMemoryFeature;
export default _default;
//# sourceMappingURL=index.d.ts.map