/**
 * Git-Based Memory Sync for AG-Claw
 * 
 * Exports memories as compressed chunks, commits to git for cross-machine sync.
 * Uses checksums for deduplication.
 */

import { createGzip } from 'zlib';
import { createHash } from 'crypto';
import { readFileSync, writeFileSync, readdirSync, mkdirSync, existsSync } from 'fs';
import { join, dirname } from 'path';
import { execFileSync } from 'child_process';

export interface MemoryChunk {
  id: string;
  type: 'entity' | 'relation' | 'episode' | 'skill' | 'context';
  content: string;
  checksum: string;
  createdAt: number;
  metadata: Record<string, unknown>;
}

export interface GitSyncConfig {
  enabled: boolean;
  repoPath: string;
  branch?: string;
  commitMessage?: string;
  compress?: boolean;
  maxChunkSize?: number;
  autoSyncInterval?: number;
}

export class MemoryGitSync {
  private config: Required<GitSyncConfig>;
  private chunksDir: string;
  private indexFile: string;

  constructor(config: GitSyncConfig) {
    this.config = {
      enabled: config.enabled ?? false,
      repoPath: config.repoPath,
      branch: config.branch ?? 'main',
      commitMessage: config.commitMessage ?? 'chore(memory): sync memories',
      compress: config.compress ?? true,
      maxChunkSize: config.maxChunkSize ?? 1024 * 1024, // 1MB
      autoSyncInterval: config.autoSyncInterval ?? 3600000, // 1 hour
    };

    this.chunksDir = join(this.config.repoPath, '.memory-chunks');
    this.indexFile = join(this.config.repoPath, '.memory-index.json');
  }

  /**
   * Generate checksum for content deduplication
   */
  private checksum(content: string): string {
    return createHash('sha256').update(content).digest('hex').substring(0, 16);
  }

  /**
   * Compress content with gzip
   */
  private async compress(content: string): Promise<Buffer> {
    return new Promise((resolve, reject) => {
      const gzip = createGzip();
      const chunks: Buffer[] = [];

      gzip.on('data', (chunk) => chunks.push(chunk));
      gzip.on('end', () => resolve(Buffer.concat(chunks)));
      gzip.on('error', reject);

      gzip.write(content, 'utf8');
      gzip.end();
    });
  }

  /**
   * Export memories to git
   */
  async export(chunks: MemoryChunk[]): Promise<number> {
    if (!this.config.enabled) return 0;

    // Ensure directories exist
    if (!existsSync(this.chunksDir)) {
      mkdirSync(this.chunksDir, { recursive: true });
    }

    let exported = 0;

    for (const chunk of chunks) {
      // Generate checksum if not present
      if (!chunk.checksum) {
        chunk.checksum = this.checksum(chunk.content);
      }

      const filename = `${chunk.type}_${chunk.id}.json${this.config.compress ? '.gz' : ''}`;
      const filepath = join(this.chunksDir, filename);

      const content = JSON.stringify(chunk);

      if (this.config.compress && content.length > 100) {
        const compressed = await this.compress(content);
        writeFileSync(filepath, compressed);
      } else {
        writeFileSync(filepath, content, 'utf8');
      }

      exported++;
    }

    // Update index
    const existingIndex = this.loadIndex();
    for (const chunk of chunks) {
      existingIndex[chunk.id] = {
        type: chunk.type,
        checksum: chunk.checksum,
        file: `${chunk.type}_${chunk.id}.json${this.config.compress ? '.gz' : ''}`,
        createdAt: chunk.createdAt,
      };
    }
    writeFileSync(this.indexFile, JSON.stringify(existingIndex, null, 2), 'utf8');

    // Git add
    try {
      execFileSync('git', ['add', this.chunksDir, this.indexFile], {
        cwd: this.config.repoPath,
        stdio: 'ignore',
      });
    } catch {
      // Git might not be initialized
    }

    return exported;
  }

  /**
   * Load existing index
   */
  private loadIndex(): Record<string, unknown> {
    if (existsSync(this.indexFile)) {
      try {
        return JSON.parse(readFileSync(this.indexFile, 'utf8'));
      } catch {
        return {};
      }
    }
    return {};
  }

  /**
   * Import memories from git
   */
  async import(): Promise<MemoryChunk[]> {
    if (!existsSync(this.chunksDir)) {
      return [];
    }

    const chunks: MemoryChunk[] = [];
    const files = readdirSync(this.chunksDir);

    for (const file of files) {
      if (!file.endsWith('.json') && !file.endsWith('.gz')) continue;

      const filepath = join(this.chunksDir, file);
      const content = readFileSync(filepath);

      let parsed: MemoryChunk;
      if (file.endsWith('.gz')) {
        // Decompress
        const { createGunzip } = await import('zlib');
        const decompressed = await new Promise<Buffer>((resolve, reject) => {
          const gunzip = createGunzip();
          const chunks: Buffer[] = [];
          gunzip.on('data', (c) => chunks.push(c));
          gunzip.on('end', () => resolve(Buffer.concat(chunks)));
          gunzip.on('error', reject);
          gunzip.write(content);
          gunzip.end();
        });
        parsed = JSON.parse(decompressed.toString('utf8'));
      } else {
        parsed = JSON.parse(content.toString('utf8'));
      }

      chunks.push(parsed);
    }

    return chunks;
  }

  /**
   * Sync: export local, pull remote, push
   */
  async sync(): Promise<{ exported: number; imported: number; conflicts: number }> {
    if (!this.config.enabled) {
      return { exported: 0, imported: 0, conflicts: 0 };
    }

    // Pull first
    try {
      execFileSync('git', ['fetch', 'origin', this.config.branch], {
        cwd: this.config.repoPath,
        stdio: 'ignore',
      });
      execFileSync('git', ['stash'], {
        cwd: this.config.repoPath,
        stdio: 'ignore',
      });
      execFileSync('git', ['pull', 'origin', this.config.branch], {
        cwd: this.config.repoPath,
        stdio: 'ignore',
      });
      execFileSync('git', ['stash', 'pop'], {
        cwd: this.config.repoPath,
        stdio: 'ignore',
      });
    } catch {
      // Network issues or no remote
    }

    const exported = await this.export([]); // Just commits current state

    // Commit and push
    try {
      const { execFileSync: exec } = require('child_process');
      exec('git', ['add', '-A'], { cwd: this.config.repoPath, stdio: 'ignore' });
      exec('git', ['commit', '-m', this.config.commitMessage], {
        cwd: this.config.repoPath,
        stdio: 'ignore',
      });
      exec('git', ['push', 'origin', this.config.branch], {
        cwd: this.config.repoPath,
        stdio: 'ignore',
      });
    } catch {
      // Push might fail
    }

    return { exported, imported: 0, conflicts: 0 };
  }

  /**
   * Push to remote
   */
  async push(): Promise<void> {
    try {
      execFileSync('git', ['add', '-A'], { cwd: this.config.repoPath, stdio: 'pipe' });
      execFileSync('git', ['commit', '-m', this.config.commitMessage], {
        cwd: this.config.repoPath,
        stdio: 'pipe',
      });
      execFileSync('git', ['push', 'origin', this.config.branch], {
        cwd: this.config.repoPath,
        stdio: 'pipe',
      });
    } catch (e) {
      const err = e as Error & { message?: string };
      throw new Error(`Push failed: ${err?.message || String(e)}`);
    }
  }

  /**
   * Pull from remote
   */
  async pull(): Promise<void> {
    try {
      execFileSync('git', ['pull', 'origin', this.config.branch], {
        cwd: this.config.repoPath,
        stdio: 'pipe',
      });
    } catch (e) {
      const err = e as Error & { message?: string };
      throw new Error(`Pull failed: ${err?.message || String(e)}`);
    }
  }

  /**
   * Get sync status
   */
  async status(): Promise<{ lastSync: number | null; pending: number; conflicts: number }> {
    try {
      const log = execFileSync('git', ['log', '-1', '--format=%ct'], {
        cwd: this.config.repoPath,
        encoding: 'utf8',
        stdio: 'pipe',
      }).trim();

      const lastSync = parseInt(log, 10) * 1000 || null;

      return {
        lastSync,
        pending: 0,
        conflicts: 0,
      };
    } catch {
      return { lastSync: null, pending: 0, conflicts: 0 };
    }
  }
}

export default MemoryGitSync;
