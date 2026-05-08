/**
 * File Watcher Feature
 *
 * Monitors file system changes with configurable paths and patterns.
 * Uses chokidar for cross-platform file watching.
 */

import chokidar, { type FSWatcher } from 'chokidar';

import {
  type FeatureModule,
  type FeatureContext,
  type FeatureMeta,
  type HealthStatus,
} from '../../core/plugin-loader';

/** File watcher configuration */
export interface FileWatcherConfig {
  enabled: boolean;
  ignored?: string | RegExp | (string | RegExp)[];
  persistent?: boolean;
  ignoreInitial?: boolean;
  awaitWriteFinish?: {
    stabilityThreshold: number;
    pollInterval: number;
  };
}

/** Public watch info (returned by listWatches) */
export interface WatchInfo {
  id: string;
  path: string;
  pattern?: string | RegExp | (string | RegExp)[];
}

/** Internal watch entry with active watcher */
interface InternalWatchEntry {
  id: string;
  path: string;
  pattern?: string | RegExp | (string | RegExp)[];
  userCallback: (event: 'change' | 'create' | 'delete', filePath: string) => void;
  watcher: FSWatcher;
}

/**
 * File Watcher feature — monitor file system changes.
 *
 * Provides watch/unwatch functionality with chokidar backend.
 * Reports events: changed, created, deleted (add, unlink, change).
 */
class FileWatcherFeature implements FeatureModule {
  readonly meta: FeatureMeta = {
    name: 'file-watcher',
    version: '0.0.5',
    description: 'File system monitoring with chokidar',
    dependencies: [],
  };

  private config: FileWatcherConfig = {
    enabled: false,
    persistent: true,
    ignoreInitial: true,
    awaitWriteFinish: {
      stabilityThreshold: 200,
      pollInterval: 100,
    },
  };
  private ctx!: FeatureContext;
  private watches: Map<string, InternalWatchEntry> = new Map();
  private active = false;
  private watchCounter = 0;

  async init(config: Record<string, unknown>, context: FeatureContext): Promise<void> {
    this.ctx = context;
    this.config = { ...this.config, ...(config as Partial<FileWatcherConfig>) };
  }

  async start(): Promise<void> {
    this.active = true;
    this.ctx.logger.info('File Watcher started', {
      ignored: this.config.ignored,
      persistent: this.config.persistent,
    });
  }

  async stop(): Promise<void> {
    // Stop all active watchers
    for (const [, entry] of this.watches) {
      await this.closeWatcher(entry);
    }
    this.watches.clear();
    this.active = false;
    this.ctx.logger.info('File Watcher stopped', { totalWatches: 0 });
  }

  async healthCheck(): Promise<HealthStatus> {
    return {
      healthy: this.active,
      message: this.active ? `Watching ${this.watches.size} paths` : 'Inactive',
      details: {
        activeWatches: this.watches.size,
      },
    };
  }

  /** Start watching a path with an optional glob pattern */
  watch(
    path: string,
    pattern?: string | RegExp | (string | RegExp)[],
    callback?: (event: 'change' | 'create' | 'delete', filePath: string) => void,
  ): string {
    if (!this.active) {
      throw new Error('File Watcher not active. Call start() first.');
    }

    const id = `watch_${++this.watchCounter}`;

    // Create a separate watcher for this watch entry
    const watcher = chokidar.watch(path, {
      ignored: this.config.ignored,
      persistent: this.config.persistent,
      ignoreInitial: this.config.ignoreInitial,
      awaitWriteFinish: this.config.awaitWriteFinish,
    });

    const entry: InternalWatchEntry = {
      id,
      path,
      pattern,
      userCallback: callback ?? (() => {}),
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
  unwatch(id: string): boolean {
    const entry = this.watches.get(id);
    if (!entry) {
      return false;
    }

    this.closeWatcherInBackground(entry);
    this.watches.delete(id);
    this.ctx.logger.debug('Watch removed', { id });
    return true;
  }

  /** Stop watching a specific path (all watches for that path) */
  unwatchPath(targetPath: string): number {
    let count = 0;
    for (const [id, entry] of this.watches) {
      if (entry.path === targetPath) {
        this.closeWatcherInBackground(entry);
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
  listWatches(): WatchInfo[] {
    return Array.from(this.watches.values()).map((entry) => ({
      id: entry.id,
      path: entry.path,
      pattern: entry.pattern,
    }));
  }

  /** Get watch count */
  getWatchCount(): number {
    return this.watches.size;
  }

  private async closeWatcher(entry: InternalWatchEntry): Promise<void> {
    try {
      await entry.watcher.close();
    } catch (error) {
      this.ctx.logger.warn('Failed to close file watcher', {
        id: entry.id,
        path: entry.path,
        error: error instanceof Error ? error.message : String(error),
      });
    }
  }

  private closeWatcherInBackground(entry: InternalWatchEntry): void {
    void this.closeWatcher(entry);
  }
}

export default new FileWatcherFeature();
