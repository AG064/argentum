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
import * as fs from 'fs';
import * as path from 'path';

// ─── Types ────────────────────────────────────────────────────────────────

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

// ─── ID Resolution ────────────────────────────────────────────────────────

/**
 * Normalize an ID to numeric format
 * Accepts: numeric (123456789), with prefix (telegram:123456789), or mapped name (anneka)
 */
export function normalizeId(
  id: string,
  config: RouterConfig
): string {
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
export function idMatches(
  ruleValue: string,
  contextId: string,
  config: RouterConfig
): boolean {
  const normalizedRule = normalizeId(ruleValue, config);
  const normalizedContext = normalizeId(contextId, config);
  return normalizedRule === normalizedContext;
}

// ─── Router Agent ─────────────────────────────────────────────────────────

export class RouterAgent extends EventEmitter {
  private config: RouterConfig;
  private activeSessions: Map<string, string> = new Map(); // agentId → sessionKey
  private agentWorkspaces: Map<string, string> = new Map();

  constructor(config: RouterConfig) {
    super();
    this.validateConfig(config);
    this.config = config;
  }

  /**
   * Validate that config has required fields
   */
  private validateConfig(config: RouterConfig): void {
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
  registerAgent(agentId: string, workspace: string): void {
    this.agentWorkspaces.set(agentId, workspace);
  }

  /**
   * Route a message and return the target agent + session
   */
  async route(ctx: MessageContext): Promise<RouteResult> {
    // Find matching rule
    for (const rule of this.config.rules) {
      if (this.matches(rule, ctx)) {
        const targetWorkspace = rule.targetWorkspace
          || this.agentWorkspaces.get(rule.targetAgent);

        // Get or create session for target agent
        const sessionKey = await this.getOrCreateSession(
          rule.targetAgent,
          targetWorkspace
        );

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
  async forwardMessage(ctx: MessageContext, response: string): Promise<void> {
    const route = await this.route(ctx);

    if (route.sessionKey) {
    }
  }

  /**
   * Check if a rule matches the context
   */
  private matches(rule: RoutingRule, ctx: MessageContext): boolean {
    switch (rule.condition) {
      case 'sender_id':
        return idMatches(rule.value as string, ctx.sender.id, this.config);

      case 'chat_id':
        return idMatches(rule.value as string, ctx.chat.id, this.config);

      case 'chat_type':
        return ctx.chat.type === rule.value;

      case 'keyword':
        if (Array.isArray(rule.value)) {
          return rule.value.some(k =>
            ctx.message.toLowerCase().includes(k.toLowerCase())
          );
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
  private async getOrCreateSession(
    agentId: string,
    workspace?: string
  ): Promise<string | undefined> {
    const cacheKey = `${agentId}:${workspace || 'default'}`;

    if (this.activeSessions.has(cacheKey)) {
      return this.activeSessions.get(cacheKey);
    }

    // Create new session via sessions_spawn
    // Note: This is a stub - actual implementation would use sessions_spawn
    return undefined;
  }
}

// ─── Config Loading ────────────────────────────────────────────────────────

/**
 * Load router config from JSON file
 * Returns default config if file doesn't exist (graceful degradation)
 */
export function loadRouterConfig(configPath?: string): RouterConfig {
  const defaultConfigPath = path.resolve(process.cwd(), 'config/router.json');
  const finalPath = configPath || defaultConfigPath;

  try {
    if (fs.existsSync(finalPath)) {
      const content = fs.readFileSync(finalPath, 'utf-8');
      const config = JSON.parse(content) as RouterConfig;

      // Validate loaded config
      if (!config.rules || !Array.isArray(config.rules)) {
        console.error(`[Router] Invalid config at ${finalPath}: missing "rules" array`);
        return getEmptyConfig();
      }
      if (!config.defaultAgent) {
        console.error(`[Router] Invalid config at ${finalPath}: missing "defaultAgent"`);
        return getEmptyConfig();
      }

      console.log(`[Router] Loaded config from ${finalPath}`);
      return config;
    } else {
      console.warn(`[Router] Config not found at ${finalPath}, using empty config`);
      return getEmptyConfig();
    }
  } catch (error) {
    console.error(`[Router] Failed to load config from ${finalPath}:`, error);
    return getEmptyConfig();
  }
}

/**
 * Get an empty/default config that only allows default routing
 */
function getEmptyConfig(): RouterConfig {
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

export const defaultRouterConfig: RouterConfig = {
  rules: [],
  defaultAgent: 'agx',
};

export default RouterAgent;
