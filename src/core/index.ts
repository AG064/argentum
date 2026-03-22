/**
 * Core module — shared infrastructure used across the entire framework.
 *
 * Exports:
 *   - ConfigManager & getConfig() — YAML config with env overrides
 *   - Logger, createLogger(), featureLogger() — structured logging
 *   - PluginLoader — feature loading and lifecycle management
 *   - LLMProvider, Message, ToolDefinition, LLMResponse, createLLMProvider() — LLM abstraction
 */

export { ConfigManager, getConfig, ConfigSchema, type AGClawConfig } from './config';

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

export {
  LLMProvider,
  createLLMProvider,
  type Message,
  type ToolDefinition,
  type LLMResponse,
  type ToolCall,
  type MessageRole,
  type ProviderConfig,
  type LLMConfig,
} from './llm-provider';
