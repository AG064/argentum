/**
 * Shared feature system types.
 * Used by PluginLoader and all feature implementations.
 */

import type { Logger } from '../core/logger';

/** Feature lifecycle states */
export type FeatureState = 'registered' | 'loaded' | 'active' | 'stopped' | 'error';

/** Feature manifest — the contract each feature must expose */
export interface FeatureManifest {
  /** Unique identifier, must match directory name */
  readonly name: string;
  /** Human-readable description */
  readonly description: string;
  /** Semantic version */
  readonly version: string;
  /** Dependencies on other features by name */
  readonly dependencies?: readonly string[];
  /** Whether this feature can run without an LLM */
  readonly standaloneCapable?: boolean;
  /** Hooks called during feature lifecycle */
  readonly hooks?: FeatureHooks;
}

/** Lifecycle hooks a feature can implement */
export interface FeatureHooks {
  /** Called when feature is first loaded */
  onLoad?(ctx: FeatureContext): Promise<void>;
  /** Called when feature transitions to active state */
  onEnable?(ctx: FeatureContext): Promise<void>;
  /** Called when feature transitions to stopped state */
  onDisable?(ctx: FeatureContext): Promise<void>;
  /** Called on system events (message:received, system:start, etc.) */
  onHook?(event: string, data: unknown, ctx: FeatureContext): Promise<void>;
  /** Periodic health check — return healthy = true */
  healthCheck?(ctx: FeatureContext): Promise<{ healthy: boolean; message?: string }>;
  /** Called when Argentum is shutting down */
  onShutdown?(ctx: FeatureContext): Promise<void>;
}

/** Context object passed to all feature hook calls */
export interface FeatureContext {
  /** The feature's own logger (child of global logger with feature name) */
  logger: Logger;
  /** Raw config section for this feature (may be typed) */
  config: FeatureConfig;
  /** Reference back to the plugin loader for inter-feature communication */
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  pluginLoader?: any;
}

/** Arbitrary config object — each feature defines its own schema */
export interface FeatureConfig {
  enabled?: boolean;
  [key: string]: unknown;
}

/** Instance of a loaded feature with runtime state */
export interface FeatureInstance {
  readonly manifest: FeatureManifest;
  state: FeatureState;
  error?: string;
}
