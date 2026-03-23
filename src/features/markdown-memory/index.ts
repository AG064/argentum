/**
 * Markdown Memory Feature
 *
 * File-system based memory using Markdown files with frontmatter.
 * Human-readable, git-friendly storage for knowledge and notes.
 */

import {
  type FeatureModule,
  type FeatureContext,
  type FeatureMeta,
  type HealthStatus,
} from '../../core/plugin-loader';
import { MarkdownMemory } from '../../memory/markdown';

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
class MarkdownMemoryFeature implements FeatureModule {
  readonly meta: FeatureMeta = {
    name: 'markdown-memory',
    version: '0.1.0',
    description: 'Markdown file-based memory store with frontmatter',
    dependencies: [],
  };

  private config: Required<MarkdownMemoryConfig>;
  private ctx!: FeatureContext;
  private storage!: MarkdownMemory;

  constructor() {
    this.config = {
      basePath: './data/markdown-memory',
    };
  }

  async init(config: Record<string, unknown>, context: FeatureContext): Promise<void> {
    this.ctx = context;
    this.config = {
      basePath: (config['basePath'] as string) ?? this.config['basePath'],
    };

    this.storage = new MarkdownMemory(this.config.basePath);
    this.storage.init();
  }

  async start(): Promise<void> {
    this.ctx.logger.info('MarkdownMemory active', {
      basePath: this.config.basePath,
    });
  }

  async stop(): Promise<void> {
    this.ctx.logger.info('MarkdownMemory stopped');
  }

  async healthCheck(): Promise<HealthStatus> {
    try {
      const entries = this.storage.list();
      return {
        healthy: true,
        details: {
          totalFiles: entries.length,
          basePath: this.config.basePath,
        },
      };
    } catch (err) {
      return {
        healthy: false,
        message: err instanceof Error ? err.message : String(err),
      };
    }
  }

  /** Write a new markdown file (or replace existing) */
  write(
    filename: string,
    content: string,
    metadata?: Record<string, string>,
    tags?: string[],
  ): string {
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
  read(
    filename: string,
  ): { content: string; metadata: Record<string, string>; tags: string[] } | null {
    const entry = this.storage.get(filename);
    if (!entry) return null;
    return {
      content: entry.content,
      metadata: entry.metadata,
      tags: entry.tags,
    };
  }

  /** Append a section to an existing markdown file */
  append(filename: string, section: string): boolean {
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
  search(
    query: string,
    limit = 20,
  ): Array<{
    filename: string;
    title: string;
    content: string;
    tags: string[];
    relevance: number;
  }> {
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
  listAll(): string[] {
    return this.storage.list().map((e) => e.filename);
  }

  /** Get by tag */
  getByTag(
    tag: string,
  ): Array<{ filename: string; title: string; content: string; tags: string[] }> {
    const entries = this.storage.getByTag(tag);
    return entries.map((entry) => ({
      filename: entry.filename,
      title: entry.title,
      content: entry.content,
      tags: entry.tags,
    }));
  }

  /** Delete a file */
  delete(filename: string): boolean {
    const result = this.storage.delete(filename);
    if (result) {
      this.ctx.logger.debug('Markdown deleted', { filename });
    }
    return result;
  }
}

export default new MarkdownMemoryFeature();
