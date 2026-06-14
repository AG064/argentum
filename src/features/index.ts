// SPDX-License-Identifier: MIT
// Copyright (c) 2024-2026 AG064
/**
 * Features module — optional plugin-like capabilities that extend Argentum.
 *
 * Each feature lives in its own subdirectory under src/features/<name>/.
 * Features are discovered and loaded by PluginLoader based on config.
 *
 * Naming convention: kebab-case directory names match config keys.
 * Feature interfaces are defined in src/types/.
 */

export type { FeatureManifest, FeatureConfig, FeatureHooks } from '../types/feature';
