"use strict";
/**
 * Git-Based Memory Sync for Argentum
 *
 * Exports memories as compressed chunks, commits to git for cross-machine sync.
 * Uses checksums for deduplication.
 */
var __createBinding = (this && this.__createBinding) || (Object.create ? (function(o, m, k, k2) {
    if (k2 === undefined) k2 = k;
    var desc = Object.getOwnPropertyDescriptor(m, k);
    if (!desc || ("get" in desc ? !m.__esModule : desc.writable || desc.configurable)) {
      desc = { enumerable: true, get: function() { return m[k]; } };
    }
    Object.defineProperty(o, k2, desc);
}) : (function(o, m, k, k2) {
    if (k2 === undefined) k2 = k;
    o[k2] = m[k];
}));
var __setModuleDefault = (this && this.__setModuleDefault) || (Object.create ? (function(o, v) {
    Object.defineProperty(o, "default", { enumerable: true, value: v });
}) : function(o, v) {
    o["default"] = v;
});
var __importStar = (this && this.__importStar) || (function () {
    var ownKeys = function(o) {
        ownKeys = Object.getOwnPropertyNames || function (o) {
            var ar = [];
            for (var k in o) if (Object.prototype.hasOwnProperty.call(o, k)) ar[ar.length] = k;
            return ar;
        };
        return ownKeys(o);
    };
    return function (mod) {
        if (mod && mod.__esModule) return mod;
        var result = {};
        if (mod != null) for (var k = ownKeys(mod), i = 0; i < k.length; i++) if (k[i] !== "default") __createBinding(result, mod, k[i]);
        __setModuleDefault(result, mod);
        return result;
    };
})();
Object.defineProperty(exports, "__esModule", { value: true });
exports.MemoryGitSync = void 0;
const child_process_1 = require("child_process");
const crypto_1 = require("crypto");
const fs_1 = require("fs");
const path_1 = require("path");
const zlib_1 = require("zlib");
class MemoryGitSync {
    config;
    chunksDir;
    indexFile;
    constructor(config) {
        const branch = config.branch ?? 'main';
        // Validate branch name: reject values starting with '-' or containing unsafe chars
        if (/^-|[^a-zA-Z0-9/_.-]/.test(branch)) {
            throw new Error(`Invalid branch name: ${branch}`);
        }
        this.config = {
            enabled: config.enabled ?? false,
            repoPath: config.repoPath,
            branch,
            commitMessage: config.commitMessage ?? 'chore(memory): sync memories',
            compress: config.compress ?? true,
            maxChunkSize: config.maxChunkSize ?? 1024 * 1024, // 1MB
            autoSyncInterval: config.autoSyncInterval ?? 3600000, // 1 hour
        };
        this.chunksDir = (0, path_1.join)(this.config.repoPath, '.memory-chunks');
        this.indexFile = (0, path_1.join)(this.config.repoPath, '.memory-index.json');
    }
    /**
     * Generate checksum for content deduplication
     */
    checksum(content) {
        return (0, crypto_1.createHash)('sha256').update(content).digest('hex').substring(0, 16);
    }
    /**
     * Compress content with gzip
     */
    async compress(content) {
        return new Promise((resolve, reject) => {
            const gzip = (0, zlib_1.createGzip)();
            const chunks = [];
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
    async export(chunks) {
        if (!this.config.enabled)
            return 0;
        // Ensure directories exist
        if (!(0, fs_1.existsSync)(this.chunksDir)) {
            (0, fs_1.mkdirSync)(this.chunksDir, { recursive: true });
        }
        let exported = 0;
        for (const chunk of chunks) {
            // Generate checksum if not present
            if (!chunk.checksum) {
                chunk.checksum = this.checksum(chunk.content);
            }
            const filename = `${chunk.type}_${chunk.id}.json${this.config.compress ? '.gz' : ''}`;
            const filepath = (0, path_1.join)(this.chunksDir, filename);
            const content = JSON.stringify(chunk);
            if (this.config.compress && content.length > 100) {
                const compressed = await this.compress(content);
                (0, fs_1.writeFileSync)(filepath, compressed);
            }
            else {
                (0, fs_1.writeFileSync)(filepath, content, 'utf8');
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
        (0, fs_1.writeFileSync)(this.indexFile, JSON.stringify(existingIndex, null, 2), 'utf8');
        // Git add
        try {
            (0, child_process_1.execFileSync)('git', ['add', this.chunksDir, this.indexFile], {
                cwd: this.config.repoPath,
                stdio: 'ignore',
            });
        }
        catch {
            // Git might not be initialized
        }
        return exported;
    }
    /**
     * Load existing index
     */
    loadIndex() {
        if ((0, fs_1.existsSync)(this.indexFile)) {
            try {
                return JSON.parse((0, fs_1.readFileSync)(this.indexFile, 'utf8'));
            }
            catch {
                return {};
            }
        }
        return {};
    }
    /**
     * Import memories from git
     */
    async import() {
        if (!(0, fs_1.existsSync)(this.chunksDir)) {
            return [];
        }
        const chunks = [];
        const files = (0, fs_1.readdirSync)(this.chunksDir);
        for (const file of files) {
            if (!file.endsWith('.json') && !file.endsWith('.gz'))
                continue;
            const filepath = (0, path_1.join)(this.chunksDir, file);
            const content = (0, fs_1.readFileSync)(filepath);
            let parsed;
            if (file.endsWith('.gz')) {
                // Decompress
                const { createGunzip } = await Promise.resolve().then(() => __importStar(require('zlib')));
                const decompressed = await new Promise((resolve, reject) => {
                    const gunzip = createGunzip();
                    const chunks = [];
                    gunzip.on('data', (c) => chunks.push(c));
                    gunzip.on('end', () => resolve(Buffer.concat(chunks)));
                    gunzip.on('error', reject);
                    gunzip.write(content);
                    gunzip.end();
                });
                parsed = JSON.parse(decompressed.toString('utf8'));
            }
            else {
                parsed = JSON.parse(content.toString('utf8'));
            }
            chunks.push(parsed);
        }
        return chunks;
    }
    /**
     * Sync: export local, pull remote, push
     */
    async sync() {
        if (!this.config.enabled) {
            return { exported: 0, imported: 0, conflicts: 0 };
        }
        // Pull first
        try {
            (0, child_process_1.execFileSync)('git', ['fetch', 'origin', this.config.branch], {
                cwd: this.config.repoPath,
                stdio: 'ignore',
            });
            (0, child_process_1.execFileSync)('git', ['stash'], {
                cwd: this.config.repoPath,
                stdio: 'ignore',
            });
            (0, child_process_1.execFileSync)('git', ['pull', 'origin', this.config.branch], {
                cwd: this.config.repoPath,
                stdio: 'ignore',
            });
            (0, child_process_1.execFileSync)('git', ['stash', 'pop'], {
                cwd: this.config.repoPath,
                stdio: 'ignore',
            });
        }
        catch {
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
        }
        catch {
            // Push might fail
        }
        return { exported, imported: 0, conflicts: 0 };
    }
    /**
     * Push to remote
     */
    async push() {
        try {
            (0, child_process_1.execFileSync)('git', ['add', '-A'], { cwd: this.config.repoPath, stdio: 'pipe' });
            (0, child_process_1.execFileSync)('git', ['commit', '-m', this.config.commitMessage], {
                cwd: this.config.repoPath,
                stdio: 'pipe',
            });
            (0, child_process_1.execFileSync)('git', ['push', 'origin', this.config.branch], {
                cwd: this.config.repoPath,
                stdio: 'pipe',
            });
        }
        catch (e) {
            const err = e;
            throw new Error(`Push failed: ${err?.message || String(e)}`);
        }
    }
    /**
     * Pull from remote
     */
    async pull() {
        try {
            (0, child_process_1.execFileSync)('git', ['pull', 'origin', this.config.branch], {
                cwd: this.config.repoPath,
                stdio: 'pipe',
            });
        }
        catch (e) {
            const err = e;
            throw new Error(`Pull failed: ${err?.message || String(e)}`);
        }
    }
    /**
     * Get sync status
     */
    async status() {
        try {
            const log = (0, child_process_1.execFileSync)('git', ['log', '-1', '--format=%ct'], {
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
        }
        catch {
            return { lastSync: null, pending: 0, conflicts: 0 };
        }
    }
}
exports.MemoryGitSync = MemoryGitSync;
exports.default = MemoryGitSync;
//# sourceMappingURL=memory-sync.js.map