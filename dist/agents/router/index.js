"use strict";
/**
 * Router Agent — Central routing for multi-user Argentum
 *
 * Receives all messages, determines routing rules,
 * and forwards to appropriate agent session.
 *
 * IMPORTANT: All routing IDs must be configured via config file.
 * No user/chat IDs should be hardcoded in this module.
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
exports.defaultRouterConfig = exports.RouterAgent = void 0;
exports.normalizeId = normalizeId;
exports.idMatches = idMatches;
exports.loadRouterConfig = loadRouterConfig;
const events_1 = require("events");
const fs = __importStar(require("fs"));
const path = __importStar(require("path"));
// ─── ID Resolution ────────────────────────────────────────────────────────
/**
 * Normalize an ID to numeric format
 * Accepts: numeric (123456789), with prefix (telegram:123456789), or mapped name (anneka)
 */
function normalizeId(id, config) {
    // Already numeric
    if (/^\d+$/.test(id)) {
        return id;
    }
    // Has platform prefix (e.g., telegram:123456789)
    if (id.includes(':')) {
        const parts = id.split(':');
        return parts[1] ?? parts[0] ?? id;
    }
    // Check ID mappings (friendly names)
    if (config.idMappings?.[id]) {
        return config.idMappings[id].numericId;
    }
    // Return as-is (might be username or other identifier)
    return id;
}
/**
 * Check if an ID matches, handling normalization
 */
function idMatches(ruleValue, contextId, config) {
    const normalizedRule = normalizeId(ruleValue, config);
    const normalizedContext = normalizeId(contextId, config);
    return normalizedRule === normalizedContext;
}
// ─── Router Agent ─────────────────────────────────────────────────────────
class RouterAgent extends events_1.EventEmitter {
    config;
    activeSessions = new Map(); // agentId → sessionKey
    agentWorkspaces = new Map();
    constructor(config) {
        super();
        this.validateConfig(config);
        this.config = config;
    }
    /**
     * Validate that config has required fields
     */
    validateConfig(config) {
        if (!config.rules || !Array.isArray(config.rules)) {
            throw new Error('Router config must have a "rules" array');
        }
        if (!config.defaultAgent || typeof config.defaultAgent !== 'string') {
            throw new Error('Router config must have a "defaultAgent" string');
        }
        for (const rule of config.rules) {
            if (!rule.condition || !rule.targetAgent) {
                throw new Error('Each routing rule must have "condition" and "targetAgent"');
            }
            if (!['sender_id', 'chat_id', 'chat_type', 'keyword', 'always'].includes(rule.condition)) {
                throw new Error(`Invalid condition type: ${rule.condition}`);
            }
        }
    }
    /**
     * Register an agent with its default workspace
     */
    registerAgent(agentId, workspace) {
        this.agentWorkspaces.set(agentId, workspace);
    }
    /**
     * Route a message and return the target agent + session
     */
    async route(ctx) {
        // Find matching rule
        for (const rule of this.config.rules) {
            if (this.matches(rule, ctx)) {
                const targetWorkspace = rule.targetWorkspace ?? this.agentWorkspaces.get(rule.targetAgent);
                // Get or create session for target agent
                const sessionKey = await this.getOrCreateSession(rule.targetAgent, targetWorkspace);
                return {
                    agent: rule.targetAgent,
                    workspace: targetWorkspace,
                    sessionKey,
                };
            }
        }
        // Default fallback
        return {
            agent: this.config.defaultAgent,
            workspace: this.agentWorkspaces.get(this.config.defaultAgent),
        };
    }
    /**
     * Forward message to routed agent
     */
    async forwardMessage(ctx, response) {
        const route = await this.route(ctx);
        if (route.sessionKey) {
            this.emit('message:forwarded', { ctx, response, route });
        }
        else {
            this.emit('message:routed', { ctx, response, route });
        }
    }
    /**
     * Check if a rule matches the context
     */
    matches(rule, ctx) {
        switch (rule.condition) {
            case 'sender_id':
                return idMatches(rule.value, ctx.sender.id, this.config);
            case 'chat_id':
                return idMatches(rule.value, ctx.chat.id, this.config);
            case 'chat_type':
                return ctx.chat.type === rule.value;
            case 'keyword':
                if (Array.isArray(rule.value)) {
                    return rule.value.some(k => ctx.message.toLowerCase().includes(k.toLowerCase()));
                }
                if (rule.value instanceof RegExp) {
                    return rule.value.test(ctx.message);
                }
                return ctx.message.includes(rule.value);
            case 'always':
                return true;
            default:
                return false;
        }
    }
    /**
     * Get existing session for agent or create new one
     */
    async getOrCreateSession(agentId, workspace) {
        const cacheKey = `${agentId}:${workspace ?? 'default'}`;
        if (this.activeSessions.has(cacheKey)) {
            return this.activeSessions.get(cacheKey);
        }
        // Create new session via sessions_spawn
        // Note: This is a stub - actual implementation would use sessions_spawn
        return undefined;
    }
}
exports.RouterAgent = RouterAgent;
// ─── Config Loading ────────────────────────────────────────────────────────
/**
 * Load router config from JSON file
 * Returns default config if file doesn't exist (graceful degradation)
 */
function loadRouterConfig(configPath) {
    const defaultConfigPath = path.resolve(process.cwd(), 'config/router.json');
    const finalPath = configPath ?? defaultConfigPath;
    try {
        if (fs.existsSync(finalPath)) {
            const content = fs.readFileSync(finalPath, 'utf-8');
            const config = JSON.parse(content);
            // Validate loaded config
            if (!config.rules || !Array.isArray(config.rules)) {
                console.error(`[Router] Invalid config at ${finalPath}: missing "rules" array`);
                return getEmptyConfig();
            }
            if (!config.defaultAgent) {
                console.error(`[Router] Invalid config at ${finalPath}: missing "defaultAgent"`);
                return getEmptyConfig();
            }
            console.info(`[Router] Loaded config from ${finalPath}`);
            return config;
        }
        else {
            console.warn(`[Router] Config not found at ${finalPath}, using empty config`);
            return getEmptyConfig();
        }
    }
    catch (error) {
        console.error(`[Router] Failed to load config from ${finalPath}:`, error);
        return getEmptyConfig();
    }
}
/**
 * Get an empty/default config that only allows default routing
 */
function getEmptyConfig() {
    return {
        rules: [],
        defaultAgent: 'agx',
    };
}
// ─── Default Configuration ────────────────────────────────────────────────
//
// DEFAULT CONFIG IS NOW EMPTY - ALL IDs MUST BE CONFIGURED VIA CONFIG FILE
// See config/router.example.json for reference
//
exports.defaultRouterConfig = {
    rules: [],
    defaultAgent: 'agx',
};
exports.default = RouterAgent;
//# sourceMappingURL=index.js.map