/**
 * Features module — optional plugin-like capabilities that extend AG-Claw.
 *
 * Each feature lives in its own subdirectory under src/features/<name>/.
 * Features are discovered and loaded by PluginLoader based on config.
 *
 * Naming convention: kebab-case directory names match config keys.
 * Feature interfaces are defined in src/types/.
 */

export type { FeatureManifest, FeatureConfig, FeatureHooks } from '../types/feature';
