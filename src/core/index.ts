/**
 * Core module — shared infrastructure used across the entire framework.
 *
 * Exports:
 *   - ConfigManager & getConfig() — YAML config with env overrides
 *   - Logger, createLogger(), featureLogger() — structured logging
 *   - PluginLoader — feature loading and lifecycle management
 *   - LLMProvider, Message, ToolDefinition, LLMResponse, createLLMProvider() — LLM abstraction
 *   - HierarchicalMemoryStore, MemoryTier, MemoryEntry — three-tier memory system
 */

export { ConfigManager, getConfig, ConfigSchema, type ArgentumConfig } from './config';

export {
  Logger,
  createLogger,
  featureLogger,
  resetLogger,
  type LoggerConfig,
  type LogContext,
  type LogLevel,
} from './logger';

export {
  PluginLoader,
  type FeatureState,
  type FeatureMeta,
  type FeatureModule,
  type HealthStatus,
  type FeatureContext,
  type HookHandler,
} from './plugin-loader';

export { createLLMProvider } from './llm-provider';

export type {
  LLMProvider,
  Message,
  ToolDefinition,
  LLMResponse,
  ToolCall,
  MessageRole,
  ProviderConfig,
  LLMConfig,
} from './llm-provider';

export {
  ModelRouter,
  getModelRouter,
  resetModelRouter,
  DEFAULT_SCORING_WEIGHTS,
  type ModelScore,
  type RoutingCriteria,
  type ScoringWeights,
  type ModelRouterConfig,
} from './model-router';

export {
  HierarchicalMemoryStore,
  MemoryTier,
  type MemoryEntry,
  type HierarchicalMemory,
} from './hierarchical-memory';

export { MemoryGitSync, type MemoryChunk, type GitSyncConfig } from './memory-sync';
