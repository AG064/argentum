/**
 * Router Agent — Central routing for multi-user Argentum
 *
 * Receives all messages, determines routing rules,
 * and forwards to appropriate agent session.
 *
 * IMPORTANT: All routing IDs must be configured via config file.
 * No user/chat IDs should be hardcoded in this module.
 */
import { EventEmitter } from 'events';
export type ConditionType = 'sender_id' | 'chat_id' | 'chat_type' | 'keyword' | 'always';
export interface RoutingRule {
    /** Match condition type */
    condition: ConditionType;
    /** Value to match against */
    value: string | string[] | RegExp;
    /** Target agent ID to route to */
    targetAgent: string;
    /** Optional target workspace (defaults to agent's default workspace) */
    targetWorkspace?: string;
}
export interface RouterConfig {
    /** Routing rules in priority order */
    rules: RoutingRule[];
    /** Default agent when no rules match */
    defaultAgent: string;
    /** ID mappings for friendly names (optional) */
    idMappings?: Record<string, IdMapping>;
}
export interface IdMapping {
    /** Numeric Telegram ID */
    numericId: string;
    /** Optional username (resolved via Telegram API if needed) */
    username?: string;
    /** Human-readable description */
    description?: string;
}
export interface MessageContext {
    /** Sender identifier (e.g. telegram:123456789 or just 123456789) */
    sender: {
        id: string;
        username?: string;
        name?: string;
    };
    /** Chat identifier */
    chat: {
        id: string;
        type: 'direct' | 'group' | 'channel';
        title?: string;
    };
    /** Raw message content */
    message: string;
    /** Platform (telegram, discord, etc.) */
    platform: string;
    /** Timestamp */
    timestamp: number;
}
export interface RouteResult {
    agent: string;
    workspace?: string;
    sessionKey?: string;
}
/**
 * Normalize an ID to numeric format
 * Accepts: numeric (123456789), with prefix (telegram:123456789), or mapped name (anneka)
 */
export declare function normalizeId(id: string, config: RouterConfig): string;
/**
 * Check if an ID matches, handling normalization
 */
export declare function idMatches(ruleValue: string, contextId: string, config: RouterConfig): boolean;
export declare class RouterAgent extends EventEmitter {
    private config;
    private activeSessions;
    private agentWorkspaces;
    constructor(config: RouterConfig);
    /**
     * Validate that config has required fields
     */
    private validateConfig;
    /**
     * Register an agent with its default workspace
     */
    registerAgent(agentId: string, workspace: string): void;
    /**
     * Route a message and return the target agent + session
     */
    route(ctx: MessageContext): Promise<RouteResult>;
    /**
     * Forward message to routed agent
     */
    forwardMessage(ctx: MessageContext, response: string): Promise<void>;
    /**
     * Check if a rule matches the context
     */
    private matches;
    /**
     * Get existing session for agent or create new one
     */
    private getOrCreateSession;
}
/**
 * Load router config from JSON file
 * Returns default config if file doesn't exist (graceful degradation)
 */
export declare function loadRouterConfig(configPath?: string): RouterConfig;
export declare const defaultRouterConfig: RouterConfig;
export default RouterAgent;
//# sourceMappingURL=index.d.ts.map