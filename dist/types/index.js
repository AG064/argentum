"use strict";
/**
 * Types module — shared TypeScript type definitions across the project.
 *
 * Avoids circular dependencies by keeping all shared interfaces and
 * type aliases in one place. Core types referenced by multiple modules
 * (e.g., FeatureManifest, MemoryEntry) live here.
 */
Object.defineProperty(exports, "__esModule", { value: true });
exports.createLogger = exports.Logger = void 0;
var logger_1 = require("../core/logger");
Object.defineProperty(exports, "Logger", { enumerable: true, get: function () { return logger_1.Logger; } });
Object.defineProperty(exports, "createLogger", { enumerable: true, get: function () { return logger_1.createLogger; } });
//# sourceMappingURL=index.js.map