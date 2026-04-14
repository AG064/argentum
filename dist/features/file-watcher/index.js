"use strict";
/**
 * File Watcher Feature
 *
 * Monitors file system changes with configurable paths and patterns.
 * Uses chokidar for cross-platform file watching.
 */
var __importDefault = (this && this.__importDefault) || function (mod) {
    return (mod && mod.__esModule) ? mod : { "default": mod };
};
Object.defineProperty(exports, "__esModule", { value: true });
const chokidar_1 = __importDefault(require("chokidar"));
/**
 * File Watcher feature — monitor file system changes.
 *
 * Provides watch/unwatch functionality with chokidar backend.
 * Reports events: changed, created, deleted (add, unlink, change).
 */
class FileWatcherFeature {
    meta = {
        name: 'file-watcher',
        version: '0.1.0',
        description: 'File system monitoring with chokidar',
        dependencies: [],
    };
    config = {
        enabled: false,
        persistent: true,
        ignoreInitial: true,
        awaitWriteFinish: {
            stabilityThreshold: 200,
            pollInterval: 100,
        },
    };
    ctx;
    watches = new Map();
    active = false;
    watchCounter = 0;
    async init(config, context) {
        this.ctx = context;
        this.config = { ...this.config, ...config };
    }
    async start() {
        this.active = true;
        this.ctx.logger.info('File Watcher started', {
            ignored: this.config.ignored,
            persistent: this.config.persistent,
        });
    }
    async stop() {
        // Stop all active watchers
        for (const [, entry] of this.watches) {
            entry.watcher.close();
        }
        this.watches.clear();
        this.active = false;
        this.ctx.logger.info('File Watcher stopped', { totalWatches: 0 });
    }
    async healthCheck() {
        return {
            healthy: this.active,
            message: this.active ? `Watching ${this.watches.size} paths` : 'Inactive',
            details: {
                activeWatches: this.watches.size,
            },
        };
    }
    /** Start watching a path with an optional glob pattern */
    watch(path, pattern, callback) {
        if (!this.active) {
            throw new Error('File Watcher not active. Call start() first.');
        }
        const id = `watch_${++this.watchCounter}`;
        // Create a separate watcher for this watch entry
        const watcher = chokidar_1.default.watch(path, {
            ignored: this.config.ignored,
            persistent: this.config.persistent,
            ignoreInitial: this.config.ignoreInitial,
            awaitWriteFinish: this.config.awaitWriteFinish,
        });
        const entry = {
            id,
            path,
            pattern,
            userCallback: callback ?? (() => { }),
            watcher,
        };
        // Set up event listeners: map chokidar events to user events
        watcher
            .on('add', (filePath) => entry.userCallback('create', filePath))
            .on('change', (filePath) => entry.userCallback('change', filePath))
            .on('unlink', (filePath) => entry.userCallback('delete', filePath))
            .on('error', (error) => {
            this.ctx.logger.error('File watcher error', {
                id,
                path,
                error: error instanceof Error ? error.message : String(error),
            });
        });
        this.watches.set(id, entry);
        this.ctx.logger.debug('Watch added', { id, path, pattern });
        return id;
    }
    /** Stop watching by watch ID */
    unwatch(id) {
        const entry = this.watches.get(id);
        if (!entry) {
            return false;
        }
        entry.watcher.close();
        this.watches.delete(id);
        this.ctx.logger.debug('Watch removed', { id });
        return true;
    }
    /** Stop watching a specific path (all watches for that path) */
    unwatchPath(targetPath) {
        let count = 0;
        for (const [id, entry] of this.watches) {
            if (entry.path === targetPath) {
                entry.watcher.close();
                this.watches.delete(id);
                count++;
            }
        }
        if (count > 0) {
            this.ctx.logger.debug('Watches removed by path', { path: targetPath, count });
        }
        return count;
    }
    /** List all active watches */
    listWatches() {
        return Array.from(this.watches.values()).map((entry) => ({
            id: entry.id,
            path: entry.path,
            pattern: entry.pattern,
        }));
    }
    /** Get watch count */
    getWatchCount() {
        return this.watches.size;
    }
}
exports.default = new FileWatcherFeature();
//# sourceMappingURL=index.js.map