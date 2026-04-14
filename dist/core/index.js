"use strict";
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
Object.defineProperty(exports, "__esModule", { value: true });
exports.MemoryGitSync = exports.MemoryTier = exports.HierarchicalMemoryStore = exports.DEFAULT_SCORING_WEIGHTS = exports.resetModelRouter = exports.getModelRouter = exports.ModelRouter = exports.createLLMProvider = exports.PluginLoader = exports.resetLogger = exports.featureLogger = exports.createLogger = exports.Logger = exports.ConfigSchema = exports.getConfig = exports.ConfigManager = void 0;
var config_1 = require("./config");
Object.defineProperty(exports, "ConfigManager", { enumerable: true, get: function () { return config_1.ConfigManager; } });
Object.defineProperty(exports, "getConfig", { enumerable: true, get: function () { return config_1.getConfig; } });
Object.defineProperty(exports, "ConfigSchema", { enumerable: true, get: function () { return config_1.ConfigSchema; } });
var logger_1 = require("./logger");
Object.defineProperty(exports, "Logger", { enumerable: true, get: function () { return logger_1.Logger; } });
Object.defineProperty(exports, "createLogger", { enumerable: true, get: function () { return logger_1.createLogger; } });
Object.defineProperty(exports, "featureLogger", { enumerable: true, get: function () { return logger_1.featureLogger; } });
Object.defineProperty(exports, "resetLogger", { enumerable: true, get: function () { return logger_1.resetLogger; } });
var plugin_loader_1 = require("./plugin-loader");
Object.defineProperty(exports, "PluginLoader", { enumerable: true, get: function () { return plugin_loader_1.PluginLoader; } });
var llm_provider_1 = require("./llm-provider");
Object.defineProperty(exports, "createLLMProvider", { enumerable: true, get: function () { return llm_provider_1.createLLMProvider; } });
var model_router_1 = require("./model-router");
Object.defineProperty(exports, "ModelRouter", { enumerable: true, get: function () { return model_router_1.ModelRouter; } });
Object.defineProperty(exports, "getModelRouter", { enumerable: true, get: function () { return model_router_1.getModelRouter; } });
Object.defineProperty(exports, "resetModelRouter", { enumerable: true, get: function () { return model_router_1.resetModelRouter; } });
Object.defineProperty(exports, "DEFAULT_SCORING_WEIGHTS", { enumerable: true, get: function () { return model_router_1.DEFAULT_SCORING_WEIGHTS; } });
var hierarchical_memory_1 = require("./hierarchical-memory");
Object.defineProperty(exports, "HierarchicalMemoryStore", { enumerable: true, get: function () { return hierarchical_memory_1.HierarchicalMemoryStore; } });
Object.defineProperty(exports, "MemoryTier", { enumerable: true, get: function () { return hierarchical_memory_1.MemoryTier; } });
var memory_sync_1 = require("./memory-sync");
Object.defineProperty(exports, "MemoryGitSync", { enumerable: true, get: function () { return memory_sync_1.MemoryGitSync; } });
//# sourceMappingURL=index.js.map