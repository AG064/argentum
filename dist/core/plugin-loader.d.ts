/**
 * AG-Claw Plugin Loader
 *
 * Dynamically loads and manages feature modules at runtime.
 * Supports enable/disable, lifecycle hooks, and dependency resolution.
 */
import { type AGClawConfig } from './config';
import { type Logger } from './logger';
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
/** Resolve the entry file for a feature directory */
export declare function resolveFeatureEntryPath(featureDirPath: string): string | null;
/** Convert a filesystem path into a native ESM import specifier */
export declare function toModuleImportSpecifier(modulePath: string): string;
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
export declare class PluginLoader {
    private features;
    private hooks;
    private featuresPath;
    private config;
    private logger;
    constructor(config: AGClawConfig, featuresPath?: string);
    /** Load all features from the features directory */
    loadAll(): Promise<void>;
    /** Load a single feature module */
    loadFeature(name: string, modulePath: string): Promise<void>;
    /** Enable and initialize a feature */
    enableFeature(name: string): Promise<void>;
    /** Disable a feature */
    disableFeature(name: string): Promise<void>;
    /** Enable all features with enabled: true in config */
    enableAll(): Promise<void>;
    /** Run health checks on all active features */
    healthCheckAll(): Promise<Map<string, HealthStatus>>;
    /** Get feature state */
    getFeatureState(name: string): FeatureState | undefined;
    /** List all features and their states */
    listFeatures(): Array<{
        name: string;
        state: FeatureState;
        version: string;
    }>;
    /** Register a hook handler */
    registerHook(event: string, handler: HookHandler): void;
    /** Emit a hook event */
    private emitHook;
    /** Get feature config from main config */
    private getFeatureConfig;
    /** Resolve feature dependencies (topological sort) */
    private resolveDependencies;
}
//# sourceMappingURL=plugin-loader.d.ts.map