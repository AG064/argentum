# Router Agent — Multi-User Orchestrator for AG-Claw

## Overview

Router Agent is the central entry point that receives ALL messages and routes them to the appropriate agent based on user/chat identification.

## Architecture

```
Message → Router → [AGX | Anneka | Home | ...] → Response
                 ↓
           Routing Rules
```

## Routing Rules

| Condition | Target Agent | Workspace | Description |
|-----------|-------------|-----------|-------------|
| `sender.id == ЛЁША_ID` | AGX | workspace/ | Лёша's personal agent |
| `sender.id == АНЯ_ID` | Anneka | workspace-anneka/ | Аня's personal agent |
| `chat.id == HOME_CHAT_ID` | Home | workspace-home/ | Shared family chat |
| `default` | AGX | workspace/ | Fallback |

## Privacy Model

```
┌─────────────────────────────────────────────┐
│                  Router                       │
│         (no memory, pure routing)            │
└───────────────┬─────────────────────────────┘
                │
    ┌───────────┼───────────┐
    │           │           │
    ▼           ▼           ▼
┌──────┐  ┌──────┐  ┌──────────┐
│ AGX  │  │Anneka│  │   Home   │
│(Лёша)│  │ (Аня) │  │ (shared)│
└──┬───┘  └───┬───┘  └────┬─────┘
   │          │            │
   └────┬─────┴────────────┘
        │
        └── Shared context (AGX ↔ Home, Anneka ↔ Home)
            AGX ↔ Anneka = ISOLATED
```

## Implementation

```typescript
// src/agents/router/index.ts

export interface RouterConfig {
  rules: RoutingRule[];
  defaultAgent: string;
}

export interface RoutingRule {
  condition: 'sender_id' | 'chat_id' | 'keyword' | 'always';
  value: string | string[] | RegExp;
  targetAgent: string;
  targetWorkspace?: string;
}

export class RouterAgent {
  constructor(private config: RouterConfig) {}

  route(context: MessageContext): RouteResult {
    for (const rule of this.config.rules) {
      if (this.evaluate(rule, context)) {
        return {
          agent: rule.targetAgent,
          workspace: rule.targetWorkspace,
        };
      }
    }
    return { agent: this.config.defaultAgent };
  }

  private evaluate(rule: RoutingRule, ctx: MessageContext): boolean {
    switch (rule.condition) {
      case 'sender_id':
        return ctx.sender.id === rule.value;
      case 'chat_id':
        return ctx.chat.id === rule.value;
      case 'keyword':
        return Array.isArray(rule.value) 
          && rule.value.some(k => ctx.message.includes(k));
      case 'always':
        return true;
    }
  }
}
```

## Session Management

When routing to a target agent:
1. Check if active session exists for that agent
2. If yes → send to existing session
3. If no → create new session with target workspace
4. Sessions persist across messages (per chat)

## AG-Claw Integration

Add to `config/default.json`:
```json
{
  "agents": {
    "router": {
      "enabled": true,
      "routes": [
        { "sender_id": "836565331", "target": "anneka" },
        { "chat_id": "-100HOMECHAT", "target": "home" },
        { "always": true, "target": "agx" }
      ]
    }
  }
}
```

## Future Extensibility

- Database backend for routing rules
- Learning-based routing (router learns user preferences)
- Priority queues per user
- Rate limiting per route
