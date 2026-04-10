# Router Agent — Multi-User Orchestrator for AG-Claw

## Overview

Router Agent is the central entry point that receives ALL messages and routes them to the appropriate agent based on user/chat identification.

**IMPORTANT: All user and chat IDs must be configured via config file. No IDs are hardcoded in the router code.**

## Architecture

```
Message → Router → [AGX | Anneka | Home | ...] → Response
                 ↓
           Routing Rules (from config)
```

## Configuration

All routing is controlled via `config/router.json`:

```json
{
  "router": {
    "enabled": true,
    "defaultAgent": "agx",
    "idMappings": {
      "anneka": {
        "numericId": "836565331",
        "username": "@anneka",
        "description": "Anneka's Telegram account"
      },
      "homeChat": {
        "numericId": "123456789",
        "description": "Family group chat"
      }
    },
    "rules": [
      {
        "condition": "sender_id",
        "value": "anneka",
        "targetAgent": "anneka"
      },
      {
        "condition": "chat_id",
        "value": "homeChat",
        "targetAgent": "home"
      },
      {
        "condition": "always",
        "value": "",
        "targetAgent": "agx"
      }
    ]
  }
}
```

### ID Formats

The router accepts IDs in multiple formats:

| Format | Example | Resolution |
|--------|---------|------------|
| Numeric | `123456789` | Used directly |
| Platform prefix | `telegram:123456789` | Strips prefix |
| Friendly name | `anneka` | Looks up in `idMappings` |
| Username | `@xiwka` | Resolves via Telegram API (future) |

### Condition Types

| Condition | Description |
|-----------|-------------|
| `sender_id` | Match message sender |
| `chat_id` | Match chat where message was sent |
| `chat_type` | Match chat type (`direct`, `group`, `channel`) |
| `keyword` | Match message content (string or array) |
| `always` | Always match (fallback) |

### ID Mappings

Use `idMappings` to define friendly names for IDs:

```json
{
  "idMappings": {
    "FRIENDLY_NAME": {
      "numericId": "123456789",
      "username": "@username",
      "description": "Optional description"
    }
  }
}
```

Then reference friendly names in rules:
```json
{
  "condition": "sender_id",
  "value": "FRIENDLY_NAME",
  "targetAgent": "target"
}
```

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
└──┬───┘  └───┬───┘  └────┬─────┘
   │          │            │
   └────┬─────┴────────────┘
        │
        └── Shared context (configurable per agent)
            AGX ↔ Anneka = ISOLATED (unless configured)
```

## Example Configurations

### Basic Single User

```json
{
  "router": {
    "defaultAgent": "agx",
    "rules": [
      {
        "condition": "always",
        "value": "",
        "targetAgent": "agx"
      }
    ]
  }
}
```

### Multi-User with Specific Routing

```json
{
  "router": {
    "defaultAgent": "agx",
    "idMappings": {
      "alice": { "numericId": "111111111" },
      "bob": { "numericId": "222222222" },
      "familyGroup": { "numericId": "-1003333333333" }
    },
    "rules": [
      { "condition": "sender_id", "value": "alice", "targetAgent": "alice-agent" },
      { "condition": "sender_id", "value": "bob", "targetAgent": "bob-agent" },
      { "condition": "chat_id", "value": "familyGroup", "targetAgent": "home" },
      { "condition": "always", "value": "", "targetAgent": "agx" }
    ]
  }
}
```

### Keyword-Based Routing

```json
{
  "router": {
    "defaultAgent": "agx",
    "rules": [
      {
        "condition": "keyword",
        "value": ["admin", "/admin"],
        "targetAgent": "admin"
      },
      {
        "condition": "keyword",
        "value": ["help", "/help"],
        "targetAgent": "helpdesk"
      },
      {
        "condition": "always",
        "value": "",
        "targetAgent": "agx"
      }
    ]
  }
}
```

## Adding to AG-Claw

1. Create `config/router.json` with your routing rules
2. Register agents with their workspaces:

```typescript
import { RouterAgent, loadRouterConfig } from './agents/router';

const config = loadRouterConfig();
const router = new RouterAgent(config);

router.registerAgent('agx', '/path/to/workspace');
router.registerAgent('anneka', '/path/to/workspace-anneka');
router.registerAgent('home', '/path/to/workspace-home');
```

## Graceful Degradation

If the config file is missing or invalid:
- Router logs a warning
- Falls back to empty rules array
- All messages route to `defaultAgent`
- No crash occurs

## Best Practices

1. **Never hardcode IDs** - Always use `idMappings` for readability
2. **Use friendly names** - Makes config self-documenting
3. **Keep rules in priority order** - First match wins
4. **Always have a fallback** - `always` condition as last rule
5. **Document new mappings** - Add `description` field for future reference

## File Structure

```
ag-claw/
├── config/
│   └── router.json          # Your routing configuration
├── src/
│   └── agents/
│       └── router/
│           ├── index.ts     # Router implementation
│           └── README.md    # This file
```
