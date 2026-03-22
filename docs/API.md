# API Reference

Complete reference for the AG-Claw REST API, WebSocket events, environment variables, and configuration schema.

---

## Table of Contents

1. [REST API Overview](#1-rest-api-overview)
2. [Authentication](#2-authentication)
3. [REST Endpoints](#3-rest-endpoints)
4. [WebSocket Events](#4-websocket-events)
5. [Error Codes](#5-error-codes)
6. [Environment Variables](#6-environment-variables)
7. [Configuration Schema](#7-configuration-schema)

---

## 1. REST API Overview

AG-Claw runs an HTTP gateway on port 18789 by default. All endpoints return JSON. The base URL for local development:

```
http://localhost:18789
```

In production, set `server.host` to your domain and use a reverse proxy (nginx, Caddy) with HTTPS.

### Request Format

For `POST` and `PATCH` endpoints, send JSON:

```bash
curl -X POST http://localhost:18789/chat \
  -H "Content-Type: application/json" \
  -H "Authorization: Bearer <token>" \
  -d '{"message": "Hello", "userId": "alice"}'
```

### Response Format

All responses follow this shape:

```json
{
  "success": true,
  "data": { ... },
  "error": null
}
```

On failure:

```json
{
  "success": false,
  "data": null,
  "error": {
    "code": "RATE_LIMITED",
    "message": "Too many requests. Retry after 30 seconds."
  }
}
```

---

## 2. Authentication

Currently, AG-Claw uses a simple bearer token scheme. Set the token in `agclaw.json`:

```json
{
  "server": {
    "auth": {
      "token": "your-secret-token"
    }
  }
}
```

Pass it on every request:

```bash
curl -H "Authorization: Bearer <token>" http://localhost:18789/health
```

For production, put AG-Claw behind a reverse proxy that handles OAuth2/OIDC authentication.

---

## 3. REST Endpoints

### Health & Status

#### `GET /health`

Health check. Returns system status and active feature count.

```bash
curl http://localhost:18789/health
```

```json
{
  "success": true,
  "data": {
    "status": "ok",
    "version": "0.2.0",
    "uptime": 3600,
    "features": "12/59 active",
    "memory": {
      "semantic": 1247,
      "knowledge_graph": 342,
      "sessions": 8
    }
  },
  "error": null
}
```

#### `GET /metrics`

Prometheus-compatible metrics endpoint.

```bash
curl http://localhost:18789/metrics
```

Returns text in Prometheus exposition format:

```
# HELP agclaw_messages_total Total messages processed
# TYPE agclaw_messages_total counter
agclaw_messages_total 4821
# HELP agclaw_tool_calls_total Tool invocations
# TYPE agclaw_tool_calls_total counter
agclaw_tool_calls_total{model="claude-sonnet-4",tool="read"} 1203
```

---

### Chat

#### `POST /chat`

Send a message to the agent. This is the main interaction endpoint.

**Request body:**

```typescript
{
  message: string;       // The user's message (required)
  userId: string;       // User identifier (required)
  sessionId?: string;   // Continue existing session (optional)
  model?: string;       // Override default model (optional)
  temperature?: number; // Override default temperature (optional)
  maxTokens?: number;   // Override max tokens (optional)
  stream?: boolean;     // Enable streaming response (default: false)
}
```

**Example:**

```bash
curl -X POST http://localhost:18789/chat \
  -H "Content-Type: application/json" \
  -d '{"message": "What is the weather in Tallinn?", "userId": "alice"}'
```

**Response:**

```json
{
  "success": true,
  "data": {
    "reply": "The current weather in Tallinn is partly cloudy, 12°C...",
    "sessionId": "sess_abc123",
    "model": "anthropic/claude-sonnet-4-20250514",
    "tokens": { "prompt": 142, "completion": 87, "total": 229 },
    "toolCalls": [],
    "latencyMs": 1243
  },
  "error": null
}
```

#### `POST /chat/stream`

Streaming version. Uses Server-Sent Events (SSE). Each event is a JSON fragment:

```bash
curl -X POST http://localhost:18789/chat/stream \
  -H "Content-Type: application/json" \
  -d '{"message": "Explain quantum computing", "userId": "alice"}'
```

Events:

```
event: token
data: {"token": "Quantum"}

event: token
data: {"token": " computing"}

event: tool_call
data: {"tool": "web_search", "input": {"query": "quantum computing basics"}}

event: done
data: {"totalTokens": 1842}
```

---

### Memory

#### `GET /memory/search`

Search semantic memory.

**Query parameters:**

| Parameter | Type | Description |
|---|---|---|
| `q` | string | Search query (required) |
| `limit` | number | Max results (default: 10) |
| `threshold` | number | Similarity threshold 0-1 (default: 0.6) |

```bash
curl "http://localhost:18789/memory/search?q=project%20meetings&limit=5"
```

```json
{
  "success": true,
  "data": {
    "results": [
      {
        "id": "mem_001",
        "content": "Weekly project sync every Monday at 10:00",
        "score": 0.92,
        "createdAt": "2026-03-20T14:30:00Z"
      }
    ],
    "total": 1
  }
}
```

#### `POST /memory/store`

Store a new memory entry.

**Request body:**

```typescript
{
  content: string;       // Memory text (required)
  userId?: string;       // Associated user (optional)
  tags?: string[];       // Categorization tags (optional)
  metadata?: object;     // Arbitrary key-value data (optional)
}
```

```bash
curl -X POST http://localhost:18789/memory/store \
  -H "Content-Type: application/json" \
  -d '{"content": "Aleksey prefers morning meetings", "tags": ["preference", "schedule"]}'
```

#### `GET /memory/graph`

Query the knowledge graph.

```bash
curl "http://localhost:18789/memory/graph?entity=aleksey"
```

```json
{
  "success": true,
  "data": {
    "entities": [
      { "id": "entity_001", "type": "person", "name": "Aleksey", "properties": {} }
    ],
    "relations": [
      { "from": "entity_001", "type": "prefers", "to": "entity_002" }
    ]
  }
}
```

#### `DELETE /memory/entry/:id`

Delete a memory entry by ID.

```bash
curl -X DELETE http://localhost:18789/memory/entry/mem_001
```

---

### Agents

#### `GET /agents`

List all configured agents.

```bash
curl http://localhost:18789/agents
```

```json
{
  "success": true,
  "data": {
    "agents": [
      { "id": "default", "name": "AG-Claw Assistant", "active": true }
    ]
  }
}
```

#### `POST /agents`

Create a new agent profile.

```bash
curl -X POST http://localhost:18789/agents \
  -H "Content-Type: application/json" \
  -d '{
    "name": "coding-assistant",
    "systemPrompt": "You are an expert programmer...",
    "model": "anthropic/claude-sonnet-4-20250514"
  }'
```

#### `PATCH /agents/:id`

Update an agent's configuration.

#### `DELETE /agents/:id`

Delete an agent.

---

### Features

#### `GET /features`

List all available features with their status.

```bash
curl http://localhost:18789/features
```

```json
{
  "success": true,
  "data": {
    "features": [
      { "name": "audit-log", "enabled": true, "version": "0.1.0" },
      { "name": "sqlite-memory", "enabled": true, "version": "0.1.0" },
      { "name": "telegram", "enabled": false, "version": "0.1.0" }
    ],
    "total": 59,
    "active": 12
  }
}
```

#### `POST /features/:name`

Enable or disable a feature.

```bash
# Enable
curl -X POST http://localhost:18789/features/semantic-search \
  -H "Content-Type: application/json" \
  -d '{"enabled": true}'

# Disable
curl -X POST http://localhost:18789/features/semantic-search \
  -H "Content-Type: application/json" \
  -d '{"enabled": false}'
```

---

### Configuration

#### `GET /config`

Get the current full configuration (secrets redacted).

```bash
curl http://localhost:18789/config
```

#### `PATCH /config`

Update configuration values. Supports dot-notation keys.

```bash
curl -X PATCH http://localhost:18789/config \
  -H "Content-Type: application/json" \
  -d '{"agent.maxIterations": 15, "model.temperature": 0.5}'
```

Changes take effect immediately (hot-reload).

---

### Sessions

#### `GET /sessions`

List recent conversation sessions.

```bash
curl "http://localhost:18789/sessions?limit=10"
```

#### `GET /sessions/:id`

Get a session's full message history.

#### `DELETE /sessions/:id`

Clear a session's history.

---

## 4. WebSocket Events

AG-Claw supports real-time communication via WebSocket at `/ws`.

### Connecting

```javascript
const ws = new WebSocket('ws://localhost:18789/ws?token=your-token');

ws.on('open', () => {
  console.log('Connected to AG-Claw');
});

ws.on('message', (data) => {
  const event = JSON.parse(data);
  console.log('Event:', event);
});
```

### Client to Server Messages

#### `{ type: "chat", message: string, userId: string }`

Send a chat message over WebSocket.

```json
{ "type": "chat", "message": "Hello!", "userId": "alice" }
```

#### `{ type: "ping" }`

Keepalive ping. Server responds with `pong`.

### Server to Client Events

#### `{ type: "token", data: string }`

Streamed token from the model.

```json
{ "type": "token", "data": "The weather" }
```

#### `{ type: "tool_call", data: { tool: string, input: object } }`

Model requested a tool execution.

```json
{ "type": "tool_call", "data": { "tool": "web_search", "input": { "query": "..." } } }
```

#### `{ type: "tool_result", id: string, result: string }`

Tool execution result.

```json
{ "type": "tool_result", "id": "call_001", "result": "13°C, partly cloudy" }
```

#### `{ type: "done", reply: string, tokens: object }`

Final response complete.

```json
{ "type": "done", "reply": "The weather is nice today.", "tokens": { "total": 342 } }
```

#### `{ type: "pong" }`

Response to client ping.

#### `{ type: "error", message: string }`

Error occurred.

---

## 5. Error Codes

| Code | HTTP Status | Description |
|---|---|---|
| `INVALID_REQUEST` | 400 | Malformed request body or missing required fields |
| `UNAUTHORIZED` | 401 | Missing or invalid auth token |
| `FORBIDDEN` | 403 | Action not permitted by policy |
| `NOT_FOUND` | 404 | Resource does not exist |
| `RATE_LIMITED` | 429 | Too many requests. Check `Retry-After` header |
| `MODEL_UNAVAILABLE` | 502 | LLM provider is down. Try fallback model |
| `TOOL_EXECUTION_FAILED` | 500 | A tool call threw an exception |
| `INTERNAL_ERROR` | 500 | Unexpected server error |

---

## 6. Environment Variables

These variables are read at startup. Set them in `.env` or your shell.

| Variable | Type | Default | Description |
|---|---|---|---|
| `OPENROUTER_API_KEY` | string | — | OpenRouter API key for model access |
| `ANTHROPIC_API_KEY` | string | — | Anthropic API key (alternative to OpenRouter) |
| `OPENAI_API_KEY` | string | — | OpenAI key for Whisper STT, DALL-E image gen |
| `AGCLAW_PORT` | number | `18789` | Gateway HTTP port |
| `AGCLAW_HOST` | string | `0.0.0.0` | Gateway bind address |
| `AGCLAW_DB_PATH` | string | `./data/agclaw.db` | SQLite database path |
| `AGCLAW_CONFIG_PATH` | string | `./agclaw.json` | Config file path |
| `AGCLAW_LOG_LEVEL` | string | `info` | Log level: `debug`, `info`, `warn`, `error` |
| `AGCLAW_LOG_FORMAT` | string | `pretty` | Log format: `pretty` or `json` |
| `AGCLAW_TELEGRAM_TOKEN` | string | — | Telegram bot token |
| `AGCLAW_FCM_KEY` | string | — | Firebase Cloud Messaging key for mobile push |
| `AGCLAW_SESSION_SECRET` | string | auto-generated | Secret for session encryption |
| `SUPABASE_URL` | string | — | Supabase project URL (for cloud memory) |
| `SUPABASE_KEY` | string | — | Supabase anon key |
| `NODE_ENV` | string | `development` | Environment: `development` or `production` |

---

## 7. Configuration Schema

The `agclaw.json` configuration file follows this JSON Schema:

```json
{
  "$schema": "http://json-schema.org/draft-07/schema#",
  "title": "AGClawConfig",
  "type": "object",
  "properties": {
    "name": {
      "type": "string",
      "description": "Instance display name",
      "default": "AG-Claw"
    },
    "version": {
      "type": "string",
      "description": "Config format version",
      "const": "0.2.0"
    },
    "server": {
      "type": "object",
      "properties": {
        "port": {
          "type": "integer",
          "minimum": 1,
          "maximum": 65535,
          "default": 18789,
          "description": "HTTP gateway port"
        },
        "host": {
          "type": "string",
          "default": "0.0.0.0",
          "description": "Bind address"
        },
        "cors": {
          "type": "object",
          "properties": {
            "enabled": { "type": "boolean", "default": false },
            "origins": { "type": "array", "items": { "type": "string" }, "default": [] }
          }
        },
        "rateLimit": {
          "type": "object",
          "properties": {
            "enabled": { "type": "boolean", "default": true },
            "windowMs": { "type": "integer", "default": 60000 },
            "maxRequests": { "type": "integer", "default": 100 }
          }
        },
        "auth": {
          "type": "object",
          "properties": {
            "token": { "type": "string", "description": "Bearer token for API auth" }
          }
        }
      }
    },
    "agent": {
      "type": "object",
      "properties": {
        "name": { "type": "string", "default": "AG-Claw Assistant" },
        "systemPrompt": { "type": "string" },
        "maxIterations": { "type": "integer", "minimum": 1, "maximum": 50, "default": 10 },
        "temperature": { "type": "number", "minimum": 0, "maximum": 2, "default": 0.7 },
        "tools": {
          "type": "array",
          "items": { "type": "string" },
          "description": "Enabled tool names"
        }
      }
    },
    "model": {
      "type": "object",
      "properties": {
        "provider": {
          "type": "string",
          "enum": ["openrouter", "anthropic", "openai"],
          "default": "openrouter"
        },
        "defaultModel": { "type": "string" },
        "fallbackModel": { "type": "string" },
        "maxTokens": { "type": "integer", "minimum": 1, "default": 8192 },
        "temperature": { "type": "number", "minimum": 0, "maximum": 2, "default": 0.7 },
        "retryAttempts": { "type": "integer", "minimum": 0, "default": 3 },
        "retryDelayMs": { "type": "integer", "minimum": 0, "default": 1000 }
      },
      "required": ["provider", "defaultModel"]
    },
    "features": {
      "type": "object",
      "additionalProperties": {
        "type": "object",
        "properties": {
          "enabled": { "type": "boolean" },
          "_extends": { "type": "string", "description": "Path to feature config file" }
        }
      },
      "default": {}
    },
    "channels": {
      "type": "object",
      "properties": {
        "telegram": {
          "type": "object",
          "properties": {
            "enabled": { "type": "boolean" },
            "token": { "type": "string" },
            "allowedUsers": { "type": "array", "items": { "type": "integer" } }
          }
        },
        "webchat": {
          "type": "object",
          "properties": {
            "enabled": { "type": "boolean" },
            "port": { "type": "integer" },
            "maxConnections": { "type": "integer", "default": 100 }
          }
        },
        "discord": {
          "type": "object",
          "properties": {
            "enabled": { "type": "boolean" },
            "token": { "type": "string" },
            "guildId": { "type": "string" }
          }
        }
      }
    },
    "memory": {
      "type": "object",
      "properties": {
        "primary": {
          "type": "string",
          "enum": ["sqlite", "markdown", "supabase"],
          "default": "sqlite"
        },
        "path": { "type": "string", "default": "./data" },
        "selfEvolving": { "type": "boolean", "default": true },
        "compressionThreshold": { "type": "integer", "default": 50000 },
        "supabaseUrl": { "type": "string" },
        "supabaseKey": { "type": "string" }
      }
    },
    "security": {
      "type": "object",
      "properties": {
        "policy": {
          "type": "string",
          "enum": ["permissive", "strict"],
          "default": "permissive"
        },
        "secrets": {
          "type": "string",
          "enum": ["encrypted", "plain"],
          "default": "encrypted"
        },
        "auditLog": { "type": "boolean", "default": true },
        "allowlistMode": {
          "type": "string",
          "enum": ["permissive", "strict"],
          "default": "permissive"
        }
      }
    },
    "backup": {
      "type": "object",
      "properties": {
        "enabled": { "type": "boolean", "default": false },
        "intervalHours": { "type": "integer", "default": 24 },
        "retentionDays": { "type": "integer", "default": 7 },
        "path": { "type": "string", "default": "./backups" }
      }
    }
  }
}
```

Example `agclaw.json`:

```json
{
  "name": "AG-Claw",
  "version": "0.2.0",
  "server": {
    "port": 18789,
    "host": "0.0.0.0",
    "cors": { "enabled": true, "origins": ["https://yourdomain.com"] },
    "rateLimit": { "enabled": true, "windowMs": 60000, "maxRequests": 100 }
  },
  "agent": {
    "name": "AG-Claw Assistant",
    "maxIterations": 10,
    "temperature": 0.7
  },
  "model": {
    "provider": "openrouter",
    "defaultModel": "anthropic/claude-sonnet-4-20250514",
    "fallbackModel": "openai/gpt-4o",
    "maxTokens": 8192,
    "temperature": 0.7,
    "retryAttempts": 3
  },
  "features": {
    "audit-log": { "enabled": true },
    "sqlite-memory": { "enabled": true },
    "semantic-search": { "enabled": true },
    "morning-briefing": { "enabled": true },
    "cron-scheduler": { "enabled": true }
  },
  "memory": {
    "primary": "sqlite",
    "path": "./data",
    "selfEvolving": true,
    "compressionThreshold": 50000
  },
  "security": {
    "policy": "permissive",
    "secrets": "encrypted",
    "auditLog": true
  },
  "backup": {
    "enabled": true,
    "intervalHours": 24,
    "retentionDays": 7,
    "path": "./backups"
  }
}
```
