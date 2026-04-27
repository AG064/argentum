"use strict";
/**
 * Argentum Plugin Loader
 *
 * Dynamically loads and manages feature modules at runtime.
 * Supports enable/disable, lifecycle hooks, and dependency resolution.
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
exports.PluginLoader = void 0;
exports.resolveFeatureEntryPath = resolveFeatureEntryPath;
exports.toModuleImportSpecifier = toModuleImportSpecifier;
const fs_1 = require("fs");
const path_1 = require("path");
const url_1 = require("url");
const logger_1 = require("./logger");
/** Resolve the entry file for a feature directory */
function resolveFeatureEntryPath(featureDirPath) {
    const jsEntry = (0, path_1.join)(featureDirPath, 'index.js');
    if ((0, fs_1.existsSync)(jsEntry)) {
        return jsEntry;
    }
    const tsEntry = (0, path_1.join)(featureDirPath, 'index.ts');
    if ((0, fs_1.existsSync)(tsEntry)) {
        return tsEntry;
    }
    return null;
}
/** Convert a filesystem path into a native ESM import specifier */
function toModuleImportSpecifier(modulePath) {
    return modulePath.startsWith('file:') ? modulePath : (0, url_1.pathToFileURL)(modulePath).href;
}
/**
 * Plugin loader for Argentum features.
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
class PluginLoader {
    constructor(config, featuresPath) {
        this.features = new Map();
        this.hooks = new Map();
        this.config = config;
        this.featuresPath = featuresPath ?? (0, path_1.resolve)(__dirname, '../features');
        this.logger = (0, logger_1.featureLogger)('plugin-loader');
    }
    /** Load all features from the features directory */
    async loadAll() {
        if (!(0, fs_1.existsSync)(this.featuresPath)) {
            this.logger.warn('Features directory not found', { path: this.featuresPath });
            return;
        }
        const entries = (0, fs_1.readdirSync)(this.featuresPath, { withFileTypes: true });
        const featureDirs = entries.filter((e) => e.isDirectory());
        this.logger.info(`Scanning ${featureDirs.length} feature directories`);
        for (const dir of featureDirs) {
            const featureDirPath = (0, path_1.join)(this.featuresPath, dir.name);
            const featureEntryPath = resolveFeatureEntryPath(featureDirPath);
            if (featureEntryPath) {
                try {
                    await this.loadFeature(dir.name, featureEntryPath);
                }
                catch (err) {
                    this.logger.error(`Failed to load feature: ${dir.name}`, {
                        error: err instanceof Error ? err.message : String(err),
                    });
                }
            }
        }
        this.resolveDependencies();
    }
    /** Load a single feature module */
    async loadFeature(name, modulePath) {
        this.logger.debug(`Loading feature: ${name}`, { path: modulePath });
        // Dynamic import of the feature module
        const mod = await Promise.resolve(`${modulePath}`).then(s => __importStar(require(s)));
        const featureModule = mod.default ??
            mod[name] ??
            Object.values(mod).find((v) => typeof v === 'object' && v !== null && 'meta' in v && 'init' in v);
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
    async enableFeature(name) {
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
        const context = {
            logger: (0, logger_1.featureLogger)(name),
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
        }
        catch (err) {
            entry.state = 'error';
            throw err;
        }
    }
    /** Disable a feature */
    async disableFeature(name) {
        const entry = this.features.get(name);
        if (entry?.state !== 'active')
            return;
        if (entry.module.stop) {
            await entry.module.stop();
        }
        entry.state = 'disabled';
        this.logger.info(`Feature disabled: ${name}`);
    }
    /** Enable all features with enabled: true in config */
    async enableAll() {
        for (const [name, entry] of this.features) {
            if (entry.state === 'unloaded') {
                try {
                    await this.enableFeature(name);
                }
                catch (err) {
                    this.logger.error(`Failed to enable feature: ${name}`, {
                        error: err instanceof Error ? err.message : String(err),
                    });
                }
            }
        }
    }
    /** Run health checks on all active features */
    async healthCheckAll() {
        const results = new Map();
        for (const [name, entry] of this.features) {
            if (entry.state === 'active' && entry.module.healthCheck) {
                try {
                    results.set(name, await entry.module.healthCheck());
                }
                catch (err) {
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
    getFeatureState(name) {
        return this.features.get(name)?.state;
    }
    /** List all features and their states */
    listFeatures() {
        return Array.from(this.features.entries()).map(([name, entry]) => ({
            name,
            state: entry.state,
            version: entry.module.meta.version,
        }));
    }
    /** Register a hook handler */
    registerHook(event, handler) {
        const handlers = this.hooks.get(event) ?? [];
        handlers.push(handler);
        this.hooks.set(event, handlers);
    }
    /** Emit a hook event */
    async emitHook(event, data) {
        const handlers = this.hooks.get(event) ?? [];
        for (const handler of handlers) {
            try {
                await handler(data);
            }
            catch (err) {
                this.logger.error(`Hook handler error for event: ${event}`, {
                    error: err instanceof Error ? err.message : String(err),
                });
            }
        }
    }
    /** Get feature config from main config */
    getFeatureConfig(name) {
        const features = this.config.features;
        return features[name] ?? { enabled: false };
    }
    /** Resolve feature dependencies (topological sort) */
    resolveDependencies() {
        const visited = new Set();
        const temp = new Set();
        const order = [];
        const visit = (name) => {
            if (temp.has(name)) {
                throw new Error(`Circular dependency detected: ${name}`);
            }
            if (visited.has(name))
                return;
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
exports.PluginLoader = PluginLoader;
