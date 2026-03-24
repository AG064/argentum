/**
 * AG-Claw Plugin Loader
 *
 * Dynamically loads and manages feature modules at runtime.
 * Supports enable/disable, lifecycle hooks, and dependency resolution.
 */

import { readdirSync, existsSync } from 'fs';
import { resolve, join } from 'path';

import { type AGClawConfig } from './config';
import { type Logger, featureLogger } from './logger';

/** Feature lifecycle states */
export type FeatureState = 'unloaded' | 'loading' | 'active' | 'error' | 'disabled';

/** Feature metadata */
export interface FeatureMeta {
  name: string;
  version: string;
  description: string;
  dependencies: string[];
}

/** Feature lifecycle interface */
export interface FeatureModule {
  /** Feature metadata */
  readonly meta: FeatureMeta;

  /** Called when feature is loaded */
  init(config: Record<string, unknown>, context: FeatureContext): Promise<void>;

  /** Called when feature is started */
  start?(): Promise<void>;

  /** Called when feature is stopped (cleanup) */
  stop?(): Promise<void>;

  /** Health check */
  healthCheck?(): Promise<HealthStatus>;
}

/** Health status result */
export interface HealthStatus {
  healthy: boolean;
  message?: string;
  details?: Record<string, unknown>;
}

/** Context passed to features on init */
export interface FeatureContext {
  logger: Logger;
  config: AGClawConfig;
  registerHook: (event: string, handler: HookHandler) => void;
  emit: (event: string, data: unknown) => Promise<void>;
}

/** Hook handler type */
export type HookHandler = (data: unknown) => Promise<void | unknown>;

/** Feature registry entry */
interface FeatureEntry {
  module: FeatureModule;
  state: FeatureState;
  config: Record<string, unknown>;
}

/**
 * Plugin loader for AG-Claw features.
 *
 * Scans the features directory, loads modules, resolves dependencies,
 * and manages feature lifecycle.
 *
 * @example
 * ```ts
 * const loader = new PluginLoader(config);
 * await loader.loadAll();
 * loader.enableFeature('webchat');
 * ```
 */
export class PluginLoader {
  private features: Map<string, FeatureEntry> = new Map();
  private hooks: Map<string, HookHandler[]> = new Map();
  private featuresPath: string;
  private config: AGClawConfig;
  private logger: Logger;

  constructor(config: AGClawConfig, featuresPath?: string) {
    this.config = config;
    this.featuresPath = featuresPath ?? resolve(__dirname, '../features');
    this.logger = featureLogger('plugin-loader');
  }

  /** Load all features from the features directory */
  async loadAll(): Promise<void> {
    if (!existsSync(this.featuresPath)) {
      this.logger.warn('Features directory not found', { path: this.featuresPath });
      return;
    }

    const entries = readdirSync(this.featuresPath, { withFileTypes: true });
    const featureDirs = entries.filter((e) => e.isDirectory());

    this.logger.info(`Scanning ${featureDirs.length} feature directories`);

    for (const dir of featureDirs) {
      const featurePath = join(this.featuresPath, dir.name, 'index.js');
      const featureTsPath = join(this.featuresPath, dir.name, 'index.ts');

      if (existsSync(featurePath) || existsSync(featureTsPath)) {
        try {
          await this.loadFeature(dir.name, join(this.featuresPath, dir.name));
        } catch (err) {
          this.logger.error(`Failed to load feature: ${dir.name}`, {
            error: err instanceof Error ? err.message : String(err),
          });
        }
      }
    }

    this.resolveDependencies();
  }

  /** Load a single feature module */
  async loadFeature(name: string, path: string): Promise<void> {
    this.logger.debug(`Loading feature: ${name}`, { path });

    // Dynamic import of the feature module
    const mod = await import(path);
    const featureModule: FeatureModule =
      mod.default ??
      mod[name] ??
      Object.values(mod).find(
        (v): v is FeatureModule =>
          typeof v === 'object' && v !== null && 'meta' in v && 'init' in v,
      );

    if (!featureModule?.meta) {
      throw new Error(`Invalid feature module: ${name} - missing meta export`);
    }

    const featureConfig = this.getFeatureConfig(name);
    const enabled = featureConfig['enabled'] === true;

    this.features.set(name, {
      module: featureModule,
      state: enabled ? 'unloaded' : 'disabled',
      config: featureConfig,
    });

    this.logger.info(`Feature registered: ${name} v${featureModule.meta.version}`, {
      enabled,
      state: enabled ? 'unloaded' : 'disabled',
    });
  }

  /** Enable and initialize a feature */
  async enableFeature(name: string): Promise<void> {
    const entry = this.features.get(name);
    if (!entry) {
      throw new Error(`Feature not found: ${name}`);
    }

    if (entry.state === 'active') {
      return;
    }

    // Check dependencies
    for (const dep of entry.module.meta.dependencies) {
      const depEntry = this.features.get(dep);
      if (depEntry?.state !== 'active') {
        this.logger.info(`Loading dependency: ${dep} for ${name}`);
        await this.enableFeature(dep);
      }
    }

    entry.state = 'loading';
    this.logger.info(`Initializing feature: ${name}`);

    const context: FeatureContext = {
      logger: featureLogger(name),
      config: this.config,
      registerHook: (event, handler) => this.registerHook(event, handler),
      emit: (event, data) => this.emitHook(event, data),
    };

    try {
      await entry.module.init(entry.config, context);
      if (entry.module.start) {
        await entry.module.start();
      }
      entry.state = 'active';
      this.logger.info(`Feature active: ${name}`);
    } catch (err) {
      entry.state = 'error';
      throw err;
    }
  }

  /** Disable a feature */
  async disableFeature(name: string): Promise<void> {
    const entry = this.features.get(name);
    if (entry?.state !== 'active') return;

    if (entry.module.stop) {
      await entry.module.stop();
    }
    entry.state = 'disabled';
    this.logger.info(`Feature disabled: ${name}`);
  }

  /** Enable all features with enabled: true in config */
  async enableAll(): Promise<void> {
    for (const [name, entry] of this.features) {
      if (entry.state === 'unloaded') {
        try {
          await this.enableFeature(name);
        } catch (err) {
          this.logger.error(`Failed to enable feature: ${name}`, {
            error: err instanceof Error ? err.message : String(err),
          });
        }
      }
    }
  }

  /** Run health checks on all active features */
  async healthCheckAll(): Promise<Map<string, HealthStatus>> {
    const results = new Map<string, HealthStatus>();

    for (const [name, entry] of this.features) {
      if (entry.state === 'active' && entry.module.healthCheck) {
        try {
          results.set(name, await entry.module.healthCheck());
        } catch (err) {
          results.set(name, {
            healthy: false,
            message: err instanceof Error ? err.message : String(err),
          });
        }
      }
    }

    return results;
  }

  /** Get feature state */
  getFeatureState(name: string): FeatureState | undefined {
    return this.features.get(name)?.state;
  }

  /** List all features and their states */
  listFeatures(): Array<{ name: string; state: FeatureState; version: string }> {
    return Array.from(this.features.entries()).map(([name, entry]) => ({
      name,
      state: entry.state,
      version: entry.module.meta.version,
    }));
  }

  /** Register a hook handler */
  private registerHook(event: string, handler: HookHandler): void {
    const handlers = this.hooks.get(event) ?? [];
    handlers.push(handler);
    this.hooks.set(event, handlers);
  }

  /** Emit a hook event */
  private async emitHook(event: string, data: unknown): Promise<void> {
    const handlers = this.hooks.get(event) ?? [];
    for (const handler of handlers) {
      try {
        await handler(data);
      } catch (err) {
        this.logger.error(`Hook handler error for event: ${event}`, {
          error: err instanceof Error ? err.message : String(err),
        });
      }
    }
  }

  /** Get feature config from main config */
  private getFeatureConfig(name: string): Record<string, unknown> {
    const features = this.config.features as Record<string, Record<string, unknown>>;
    return features[name] ?? { enabled: false };
  }

  /** Resolve feature dependencies (topological sort) */
  private resolveDependencies(): void {
    const visited = new Set<string>();
    const temp = new Set<string>();
    const order: string[] = [];

    const visit = (name: string): void => {
      if (temp.has(name)) {
        throw new Error(`Circular dependency detected: ${name}`);
      }
      if (visited.has(name)) return;

      temp.add(name);
      const entry = this.features.get(name);
      if (entry) {
        for (const dep of entry.module.meta.dependencies) {
          if (this.features.has(dep)) {
            visit(dep);
          }
        }
      }
      temp.delete(name);
      visited.add(name);
      order.push(name);
    };

    for (const name of this.features.keys()) {
      visit(name);
    }

    this.logger.debug('Dependency order resolved', { order });
  }
}
