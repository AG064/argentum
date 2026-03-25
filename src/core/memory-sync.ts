import { execSync } from 'child_process';
import { existsSync, mkdirSync, readFileSync, writeFileSync, readdirSync, statSync } from 'fs';
import { join, dirname } from 'path';
import { createGzip, gunzip } from 'zlib';
import { promisify } from 'util';

const gzip = promisify(createGzip);
const gunzipAsync = promisify(gunzip);

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
}

function md5(str: string): string {
  // Simple hash for checksum
  let hash = 0;
  for (let i = 0; i < str.length; i++) {
    const char = str.charCodeAt(i);
    hash = ((hash << 5) - hash) + char;
    hash = hash & hash;
  }
  return Math.abs(hash).toString(16);
}

export class MemoryGitSync {
  private config: GitSyncConfig;
  private chunksDir: string;

  constructor(config: GitSyncConfig) {
    this.config = {
      branch: 'main',
      commitMessage: 'chore: sync memory',
      compress: true,
      maxChunkSize: 10000,
      ...config,
    };
    this.chunksDir = join(this.config.repoPath, '.memory-chunks');
  }

  private async compress(data: string): Promise<Buffer> {
    if (!this.config.compress) return Buffer.from(data);
    return gzip(data);
  }

  private async decompress(data: Buffer): Promise<string> {
    if (!this.config.compress) return data.toString();
    const decompressed = await gunzipAsync(data);
    return decompressed.toString();
  }

  async export(chunks: MemoryChunk[]): Promise<void> {
    if (!existsSync(this.chunksDir)) {
      mkdirSync(this.chunksDir, { recursive: true });
    }

    for (const chunk of chunks) {
      const filename = `${chunk.type}_${chunk.id}.json.gz`;
      const filepath = join(this.chunksDir, filename);
      const content = JSON.stringify(chunk);
      const compressed = await this.compress(content);
      writeFileSync(filepath, compressed);
    }
  }

  async import(): Promise<MemoryChunk[]> {
    if (!existsSync(this.chunksDir)) return [];
    
    const chunks: MemoryChunk[] = [];
    const files = readdirSync(this.chunksDir);
    
    for (const file of files) {
      const filepath = join(this.chunksDir, file);
      const stat = statSync(filepath);
      if (!stat.isFile()) continue;
      
      const data = readFileSync(filepath);
      const content = await this.decompress(data);
      chunks.push(JSON.parse(content));
    }
    
    return chunks;
  }

  async sync(): Promise<{ exported: number; imported: number; conflicts: number }> {
    const before = await this.import();
    await this.export(before);
    
    // Git add, commit, push
    try {
      execSync('git add .', { cwd: this.config.repoPath, stdio: 'ignore' });
      execSync(`git commit -m "${this.config.commitMessage}"`, { 
        cwd: this.config.repoPath, 
        stdio: 'ignore' 
      });
      execSync(`git push origin ${this.config.branch}`, { 
        cwd: this.config.repoPath, 
        stdio: 'ignore' 
      });
    } catch {
      // No changes to commit
    }
    
    return { exported: before.length, imported: before.length, conflicts: 0 };
  }

  async push(): Promise<void> {
    execSync(`git push origin ${this.config.branch}`, { 
      cwd: this.config.repoPath, 
      stdio: 'inherit' 
    });
  }

  async pull(): Promise<void> {
    execSync(`git pull origin ${this.config.branch}`, { 
      cwd: this.config.repoPath, 
      stdio: 'inherit' 
    });
  }

  async status(): Promise<{ lastSync: number; pending: number; conflicts: number }> {
    try {
      const log = execSync(`git log -1 --format=%ct`, { 
        cwd: this.config.repoPath, 
        encoding: 'utf8' 
      }).trim();
      return { 
        lastSync: parseInt(log) * 1000, 
        pending: readdirSync(this.chunksDir).length, 
        conflicts: 0 
      };
    } catch {
      return { lastSync: 0, pending: 0, conflicts: 0 };
    }
  }
}

export default MemoryGitSync;
