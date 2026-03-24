/**
 * Router Agent — Central routing for multi-user AG-Claw
 * 
 * Receives all messages, determines routing rules,
 * and forwards to appropriate agent session.
 */

import { EventEmitter } from 'events';
import { sessions_spawn, sessions_send } from '../sessions';

// ─── Types ────────────────────────────────────────────────────────────────

export interface RoutingRule {
  /** Match condition type */
  condition: 'sender_id' | 'chat_id' | 'chat_type' | 'keyword' | 'always';
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
}

export interface MessageContext {
  /** Sender identifier (e.g. telegram:386565331) */
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

// ─── Router Agent ─────────────────────────────────────────────────────────

export class RouterAgent extends EventEmitter {
  private config: RouterConfig;
  private activeSessions: Map<string, string> = new Map(); // agentId → sessionKey
  private agentWorkspaces: Map<string, string> = new Map();

  constructor(config: RouterConfig) {
    super();
    this.config = config;
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
      await sessions_send(route.sessionKey, response);
    }
  }

  /**
   * Check if a rule matches the context
   */
  private matches(rule: RoutingRule, ctx: MessageContext): boolean {
    switch (rule.condition) {
      case 'sender_id':
        return ctx.sender.id === rule.value;

      case 'chat_id':
        return ctx.chat.id === rule.value;

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

// ─── Default Configuration ────────────────────────────────────────────────

export const defaultRouterConfig: RouterConfig = {
  rules: [
    // Аня — личный чат
    {
      condition: 'sender_id',
      value: 'telegram:836565331', // TODO: actual Anna ID
      targetAgent: 'anneka',
    },
    // Home чат
    {
      condition: 'chat_id',
      value: 'telegram:-100HOMECHAT', // TODO: actual Home chat ID
      targetAgent: 'home',
    },
    // Fallback
    {
      condition: 'always',
      value: '',
      targetAgent: 'agx',
    },
  ],
  defaultAgent: 'agx',
};

export default RouterAgent;
