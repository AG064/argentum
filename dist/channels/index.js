"use strict";
/**
 * Channels module — protocol adapters for external messaging platforms.
 *
 * Each channel is responsible for:
 *   - Receiving inbound messages from the platform
 *   - Routing them to the Agent
 *   - Sending responses back to the platform
 *
 * Supported channels:
 *   - Telegram (grammy-based bot)
 *   - Webchat (WebSocket + HTTP server)
 *   - Mobile (FCM push notifications)
 */
Object.defineProperty(exports, "__esModule", { value: true });
exports.Agent = exports.createLogger = exports.Logger = void 0;
var logger_1 = require("../core/logger");
Object.defineProperty(exports, "Logger", { enumerable: true, get: function () { return logger_1.Logger; } });
Object.defineProperty(exports, "createLogger", { enumerable: true, get: function () { return logger_1.createLogger; } });
var index_1 = require("../index");
Object.defineProperty(exports, "Agent", { enumerable: true, get: function () { return index_1.Agent; } });
//# sourceMappingURL=index.js.map