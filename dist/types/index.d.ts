/**
 * Types module — shared TypeScript type definitions across the project.
 *
 * Avoids circular dependencies by keeping all shared interfaces and
 * type aliases in one place. Core types referenced by multiple modules
 * (e.g., FeatureManifest, MemoryEntry) live here.
 */
export type { FeatureManifest, FeatureConfig, FeatureHooks, FeatureInstance } from './feature';
export { Logger, createLogger } from '../core/logger';
export type { Message, ToolDefinition, LLMResponse, LLMProvider } from '../core/llm-provider';
export type { FeatureMeta, FeatureModule, HealthStatus, FeatureContext, FeatureState, } from '../core/plugin-loader';
//# sourceMappingURL=index.d.ts.map