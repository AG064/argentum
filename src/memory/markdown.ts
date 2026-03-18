/**
 * Markdown Memory Backend
 *
 * File-system based memory using Markdown files in a directory tree.
 * Human-readable, git-friendly, and great for documentation-style memories.
 */

import { readFileSync, writeFileSync, mkdirSync, existsSync, readdirSync, unlinkSync } from 'fs';
import { join, resolve, relative } from 'path';

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
export class MarkdownMemory {
  private basePath: string;

  constructor(basePath: string = './data/markdown-memory') {
    this.basePath = resolve(basePath);
  }

  /** Initialize the memory directory */
  init(): void {
    if (!existsSync(this.basePath)) {
      mkdirSync(this.basePath, { recursive: true });
    }
  }

  /** Parse a markdown file into an entry */
  private parseMarkdown(filename: string): MarkdownEntry | null {
    try {
      // Prevent path traversal: resolve and ensure file stays within basePath
      const fullPath = resolve(this.basePath, filename);
      if (!fullPath.startsWith(this.basePath + '/')) {
        // allow exact match if equals basePath file
        if (fullPath !== this.basePath) return null;
      }

      const raw = readFileSync(fullPath, 'utf-8');

      // Parse YAML frontmatter
      const frontmatterMatch = raw.match(/^---\n([\s\S]*?)\n---\n([\s\S]*)$/);
      let content = raw;
      const metadata: Record<string, string> = {};
      const tags: string[] = [];
      let title = filename.replace(/\.md$/, '');

      if (frontmatterMatch) {
        const frontmatter = frontmatterMatch[1]!;
        content = frontmatterMatch[2]!.trim();

        for (const line of frontmatter.split('\n')) {
          const idx = line.indexOf(':');
          if (idx > 0) {
            const key = line.slice(0, idx).trim();
            const value = line.slice(idx + 1).trim();
            if (key === 'tags') {
              tags.push(...value.split(',').map(t => t.trim()).filter(Boolean));
            } else if (key === 'title') {
              title = value;
            } else {
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
    } catch {
      return null;
    }
  }

  /** Build YAML frontmatter string */
  private buildFrontmatter(entry: Partial<MarkdownEntry>): string {
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
  save(entry: Omit<MarkdownEntry, 'filename' | 'createdAt' | 'updatedAt'> & { filename?: string }): string {
    this.init();
    let filename = entry.filename ?? `${Date.now()}-${this.sanitize(entry.title)}.md`;

    // Basic validation: filename should not contain path separators and be reasonably short
    if (filename.includes('..') || filename.includes('/') || filename.includes('\\')) {
      throw new Error('Invalid filename');
    }
    if (filename.length > 255) throw new Error('Filename too long');

    const fullPath = resolve(this.basePath, filename);
    if (!fullPath.startsWith(this.basePath + '/')) {
      throw new Error('Invalid filename path');
    }

    const exists = existsSync(fullPath);
    const existing = exists ? this.parseMarkdown(filename) : null;

    const fullEntry: MarkdownEntry = {
      filename,
      title: entry.title,
      content: entry.content,
      tags: entry.tags,
      metadata: { ...entry.metadata },
      createdAt: existing?.createdAt ?? new Date(),
      updatedAt: new Date(),
    };

    const body = this.buildFrontmatter(fullEntry) + fullEntry.content;
    writeFileSync(fullPath, body, 'utf-8');

    return filename;
  }

  /** Retrieve a memory entry by filename */
  get(filename: string): MarkdownEntry | null {
    // Validate path and prevent traversal
    const fullPath = resolve(this.basePath, filename);
    if (!fullPath.startsWith(this.basePath + '/')) return null;
    if (!existsSync(fullPath)) return null;
    return this.parseMarkdown(filename);
  }

  /** Delete a memory entry */
  delete(filename: string): boolean {
    const fullPath = resolve(this.basePath, filename);
    if (!fullPath.startsWith(this.basePath + '/')) return false;
    if (!existsSync(fullPath)) return false;
    unlinkSync(fullPath);
    return true;
  }

  /** List all memory entries */
  list(): MarkdownEntry[] {
    if (!existsSync(this.basePath)) return [];
    return readdirSync(this.basePath)
      .filter(f => f.endsWith('.md'))
      .map(f => this.parseMarkdown(f))
      .filter((e): e is MarkdownEntry => e !== null)
      .sort((a, b) => b.updatedAt.getTime() - a.updatedAt.getTime());
  }

  /** Search entries by content or title */
  search(query: string, limit = 20): MarkdownEntry[] {
    const q = query.toLowerCase();
    return this.list()
      .filter(entry =>
        entry.title.toLowerCase().includes(q) ||
        entry.content.toLowerCase().includes(q) ||
        entry.tags.some(t => t.toLowerCase().includes(q))
      )
      .slice(0, limit);
  }

  /** Get entries by tag */
  getByTag(tag: string): MarkdownEntry[] {
    return this.list().filter(e => e.tags.includes(tag));
  }

  /** Sanitize filename */
  private sanitize(text: string): string {
    return text.toLowerCase().replace(/[^a-z0-9]+/g, '-').replace(/^-|-$/g, '').slice(0, 60);
  }
}
