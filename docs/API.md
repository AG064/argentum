# API Reference

Complete REST API and WebSocket reference for Argentum Gateway.

---

## Base URL

```
http://localhost:3000    # Default port
http://localhost:18789   # Default from config
```

All endpoints return JSON. All request bodies must be `Content-Type: application/json`.

---

## Authentication

Currently uses token-based auth via the `Authorization` header. Set the token in `argentum.json`:

```json
{
  "security": {
    "apiToken": "your-secret-token"
  }
}
```

Include it in requests:

```
Authorization: Bearer <token>
```

For local development, the token is optional (set `security.auth: false` to disable).

---

## REST Endpoints

### Health & Status

#### `GET /health`

Health check endpoint. Returns status of the gateway and all active features.

**Response `200 OK`:**

```json
{
  "status": "ok",
  "version": "0.0.3",
  "uptime": 3600,
  "node": "v20.19.0",
  "features": {
    "total": 59,
    "active": 12,
    "unhealthy": []
  },
  "memory": {
    "entries": 1542,
    "lastConsolidation": "2026-03-22T08:00:00Z"
  }
}
```

**Response `503 Service Unavailable`** (when features are unhealthy):

```json
{
  "status": "degraded",
  "features": {
    "unhealthy": ["mesh-workflows"]
  }
}
```

---

#### `GET /metrics`

Prometheus-compatible metrics endpoint.

**Response `200 OK`:**

```
# HELP argentum_messages_total Total messages processed
# TYPE argentum_messages_total counter
argentum_messages_total{channel="telegram"} 1542
argentum_messages_total{channel="webchat"} 238

# HELP argentum_tool_calls_total Total tool invocations
# TYPE argentum_tool_calls_total counter
argentum_tool_calls_total{tool="web_search"} 89
argentum_tool_calls_total{tool="memory_search"} 312

# HELP argentum_llm_tokens_total Total LLM tokens used
# TYPE argentum_llm_tokens_total counter
argentum_llm_tokens_total{type="prompt"} 456789
argentum_llm_tokens_total{type="completion"} 123456

# HELP argentum_memory_entries_total Memory entries by type
# TYPE argentum_memory_entries_total gauge
argentum_memory_entries_total{type="decision"} 234
argentum_memory_entries_total{type="lesson"} 567

# HELP argentum_features_active Number of active features
# TYPE argentum_features_active gauge
argentum_features_active 12
```

---

### Chat

#### `POST /chat`

Send a message to the agent and receive a response.

**Request:**

```json
{
  "message": "What decisions have we made about the API design?",
  "userId": "user-123",
  "sessionId": "session-456",
  "channel": "webchat",
  "context": {
    "customKey": "customValue"
  }
}
```

| Field | Type | Required | Description |
|---|---|---|---|
| `message` | string | Yes | The user's message |
| `userId` | string | Yes | Unique user identifier |
| `sessionId` | string | No | Conversation session ID (auto-generated if omitted) |
| `channel` | string | No | Channel source (default: `api`) |
| `context` | object | No | Additional context passed to the agent |

**Response `200 OK`:**

```json
{
  "response": "Based on your memory, you made three key decisions about the API design...",
  "sessionId": "session-456",
  "toolCalls": [
    {
      "tool": "memory_search",
      "arguments": { "query": "API design decisions" },
      "result": "Found 3 entries..."
    }
  ],
  "tokens": {
    "prompt": 1243,
    "completion": 342,
    "total": 1585
  },
  "latencyMs": 2341
}
```

**Response `400 Bad Request`:**

```json
{
  "error": "INVALID_REQUEST",
  "message": "Field 'message' is required",
  "details": { "field": "message" }
}
```

**Response `429 Too Many Requests`:**

```json
{
  "error": "RATE_LIMITED",
  "message": "Rate limit exceeded. Try again in 30 seconds.",
  "retryAfter": 30
}
```

---

#### `POST /chat/stream`

Streaming chat response via Server-Sent Events.

**Request:** Same as `POST /chat`.

**Response:** `Content-Type: text/event-stream`

```
event: start
data: {"sessionId": "session-456"}

event: token
data: {"content": "Based"}

event: token
data: {"content": " on your"}

event: token
data: {"content": " memory,"}

event: tool_call
data: {"tool": "memory_search", "arguments": {"query": "API"}}

event: tool_result
data: {"tool": "memory_search", "result": "Found 3 entries..."}

event: done
data: {"tokens": {"prompt": 1243, "completion": 342, "total": 1585}}
```

---

### Memory

#### `GET /memory/search`

Search semantic memory.

**Query parameters:**

| Parameter | Type | Default | Description |
|---|---|---|---|
| `q` | string | — | Search query (required) |
| `limit` | number | 5 | Max results |
| `type` | string | all | Filter by type: `decision`, `lesson`, `error`, `preference`, `general` |
| `userId` | string | all | Filter by user |

**Example:**

```
GET /memory/search?q=API+design&limit=5&type=decision
```

**Response `200 OK`:**

```json
{
  "results": [
    {
      "id": "mem_abc123",
      "type": "decision",
      "content": "Use REST over GraphQL for the public API",
      "createdAt": "2026-03-20T10:30:00Z",
      "accessedAt": "2026-03-23T01:00:00Z",
      "accessCount": 5,
      "metadata": {
        "source": "telegram:123456"
      }
    }
  ],
  "total": 1,
  "query": "API design",
  "latencyMs": 23
}
```

---

#### `POST /memory/store`

Store a new memory entry.

**Request:**

```json
{
  "type": "decision",
  "content": "Use PostgreSQL as the primary database",
  "userId": "user-123",
  "metadata": {
    "project": "backend",
    "priority": "high"
  }
}
```

| Field | Type | Required | Description |
|---|---|---|---|
| `type` | string | Yes | One of: `decision`, `lesson`, `error`, `preference`, `general` |
| `content` | string | Yes | The memory content |
| `userId` | string | No | Associated user ID |
| `metadata` | object | No | Additional key-value metadata |

**Response `201 Created`:**

```json
{
  "id": "mem_def456",
  "type": "decision",
  "content": "Use PostgreSQL as the primary database",
  "createdAt": "2026-03-23T01:30:00Z",
  "metadata": {
    "project": "backend",
    "priority": "high"
  }
}
```

---

#### `GET /memory/recent`

Get recent memory entries.

**Query parameters:**

| Parameter | Type | Default | Description |
|---|---|---|---|
| `limit` | number | 10 | Max results (max 100) |
| `type` | string | all | Filter by type |

**Response `200 OK`:**

```json
{
  "entries": [
    {
      "id": "mem_def456",
      "type": "decision",
      "content": "Use PostgreSQL as the primary database",
      "createdAt": "2026-03-23T01:30:00Z",
      "accessedAt": "2026-03-23T01:30:00Z",
      "accessCount": 1
    }
  ],
  "total": 1
}
```

---

#### `DELETE /memory/:id`

Delete a memory entry.

**Response `200 OK`:**

```json
{
  "deleted": true,
  "id": "mem_def456"
}
```

---

### Agents

#### `GET /agents`

List all configured agents.

**Response `200 OK`:**

```json
{
  "agents": [
    {
      "id": "default",
      "name": "Argentum Assistant",
      "model": "anthropic/claude-sonnet-4-20250514",
      "tools": ["web_search", "read_file", "write_file", "run_command", "memory_search"],
      "status": "active"
    }
  ],
  "active": "default"
}
```

---

#### `GET /agents/:id`

Get details of a specific agent.

**Response `200 OK`:**

```json
{
  "id": "default",
  "name": "Argentum Assistant",
  "model": "anthropic/claude-sonnet-4-20250514",
  "systemPrompt": "You are a helpful AI assistant...",
  "maxIterations": 10,
  "temperature": 0.7,
  "tools": ["web_search", "read_file", "write_file", "run_command", "memory_search"],
  "status": "active",
  "sessions": 1542,
  "totalMessages": 8934
}
```

---

### Sessions

#### `GET /sessions`

List conversation sessions.

**Query parameters:**

| Parameter | Type | Default | Description |
|---|---|---|---|
| `userId` | string | all | Filter by user |
| `limit` | number | 20 | Max results |
| `offset` | number | 0 | Pagination offset |

**Response `200 OK`:**

```json
{
  "sessions": [
    {
      "id": "session-456",
      "userId": "user-123",
      "channel": "telegram",
      "messageCount": 42,
      "createdAt": "2026-03-20T10:30:00Z",
      "lastMessageAt": "2026-03-23T01:00:00Z",
      "tokens": {
        "prompt": 45678,
        "completion": 12345
      }
    }
  ],
  "total": 156,
  "limit": 20,
  "offset": 0
}
```

---

#### `GET /sessions/:id`

Get a specific session with full message history.

**Response `200 OK`:**

```json
{
  "id": "session-456",
  "userId": "user-123",
  "channel": "telegram",
  "createdAt": "2026-03-20T10:30:00Z",
  "lastMessageAt": "2026-03-23T01:00:00Z",
  "messages": [
    {
      "role": "user",
      "content": "What should we use for the API?",
      "timestamp": "2026-03-23T01:00:00Z"
    },
    {
      "role": "assistant",
      "content": "I recommend REST for its simplicity...",
      "timestamp": "2026-03-23T01:00:01Z",
      "toolCalls": [
        {
          "tool": "memory_search",
          "arguments": { "query": "API decisions" },
          "result": "Found 2 entries"
        }
      ]
    }
  ],
  "tokens": {
    "prompt": 45678,
    "completion": 12345
  }
}
```

---

### Features

#### `GET /features`

List all available features.

**Response `200 OK`:**

```json
{
  "features": [
    {
      "name": "sqlite-memory",
      "version": "0.0.3",
      "description": "SQLite-backed semantic memory",
      "state": "active",
      "enabled": true,
      "dependencies": []
    },
    {
      "name": "knowledge-graph",
      "version": "0.0.3",
      "description": "Entity relationship graph",
      "state": "active",
      "enabled": true,
      "dependencies": ["sqlite-memory"]
    }
  ],
  "total": 59,
  "active": 12
}
```

---

#### `POST /features/:name`

Enable or disable a feature.

**Request:**

```json
{
  "action": "enable"
}
```

Valid actions: `enable`, `disable`

**Response `200 OK`:**

```json
{
  "name": "webchat",
  "state": "active",
  "previousState": "disabled",
  "message": "Feature enabled successfully"
}
```

---

#### `GET /features/:name`

Get details of a specific feature.

**Response `200 OK`:**

```json
{
  "name": "sqlite-memory",
  "version": "0.0.3",
  "description": "SQLite-backed semantic memory",
  "state": "active",
  "enabled": true,
  "dependencies": [],
  "config": {
    "path": "./data/memory.db",
    "enabled": true
  },
  "health": {
    "healthy": true,
    "message": "Database accessible",
    "lastCheck": "2026-03-23T01:00:00Z"
  }
}
```

---

### Configuration

#### `GET /config`

Get current configuration (secrets masked).

**Response `200 OK`:**

```json
{
  "name": "My Argentum",
  "server": {
    "port": 3000,
    "host": "0.0.0.0"
  },
  "model": {
    "provider": "openrouter",
    "defaultModel": "anthropic/claude-sonnet-4-20250514"
  },
  "security": {
    "apiToken": "********"
  }
}
```

---

#### `PATCH /config`

Update configuration at runtime. Only certain keys are hot-reloadable.

**Request:**

```json
{
  "name": "Updated Name",
  "features": {
    "webchat": { "enabled": true }
  }
}
```

**Response `200 OK`:**

```json
{
  "updated": ["name", "features.webchat"],
  "restartRequired": ["features.webchat"],
  "message": "Configuration updated. Restart required for webchat."
}
```

---

### Webhooks

#### `POST /webhooks/incoming/:feature`

Receive an incoming webhook for a specific feature.

**Request:** Feature-specific payload

**Response `200 OK`:**

```json
{
  "received": true,
  "feature": "webhooks",
  "processed": true
}
```

---

## WebSocket Events

Argentum supports real-time communication via WebSocket at `/ws`.

### Connection

```javascript
const ws = new WebSocket('ws://localhost:3000/ws?token=your-token');

// Receive welcome
ws.on('open', () => {
  ws.send(JSON.stringify({ type: 'auth', token: 'your-token' }));
});
```

### Client → Server Events

| Event | Payload | Description |
|---|---|---|
| `auth` | `{ token: string }` | Authenticate the connection |
| `chat` | `{ message: string, userId: string, sessionId?: string }` | Send a chat message |
| `ping` | `{}` | Keepalive ping |

### Server → Client Events

| Event | Payload | Description |
|---|---|---|
| `auth_ok` | `{ userId: string }` | Authentication successful |
| `auth_error` | `{ message: string }` | Authentication failed |
| `chat_start` | `{ sessionId: string }` | Chat processing started |
| `chat_token` | `{ content: string }` | Streaming response token |
| `chat_tool_call` | `{ tool: string, arguments: object }` | Tool being executed |
| `chat_tool_result` | `{ tool: string, result: string }` | Tool execution result |
| `chat_done` | `{ response: string, tokens: object }` | Response complete |
| `chat_error` | `{ message: string }` | Error during processing |
| `pong` | `{}` | Pong response to ping |
| `memory_updated` | `{ id: string, type: string }` | New memory stored |
| `feature_status` | `{ name: string, state: string }` | Feature state changed |

---

## Environment Variables

| Variable | Type | Description |
|---|---|---|
| `OPENROUTER_API_KEY` | string | OpenRouter API key |
| `ANTHROPIC_API_KEY` | string | Anthropic API key |
| `OPENAI_API_KEY` | string | OpenAI API key |
| `AGCLAW_TELEGRAM_TOKEN` | string | Telegram bot token |
| `AGCLAW_FCM_KEY` | string | Firebase Cloud Messaging key |
| `AGCLAW_DB_PATH` | string | Path to main SQLite DB |
| `AGCLAW_PORT` | number | Gateway HTTP port |
| `AGCLAW_HOST` | string | Gateway bind address |
| `AGCLAW_LOG_LEVEL` | string | Log level: `debug`, `info`, `warn`, `error` |
| `AGCLAW_LOG_FORMAT` | string | Log format: `json`, `pretty` |
| `AGCLAW_API_TOKEN` | string | API authentication token |
| `SUPABASE_URL` | string | Supabase project URL |
| `SUPABASE_KEY` | string | Supabase anon key |
| `AGCLAW_SQL_LOG` | string | Set to `debug` to log SQL queries |

---

## Error Codes

All API errors return a JSON body:

```json
{
  "error": "ERROR_CODE",
  "message": "Human-readable description",
  "details": { }
}
```

| HTTP Status | Error Code | Description |
|---|---|---|
| `400` | `INVALID_REQUEST` | Missing or malformed request fields |
| `400` | `VALIDATION_ERROR` | Request passed validation but business logic rejected it |
| `401` | `UNAUTHORIZED` | Missing or invalid authentication token |
| `403` | `FORBIDDEN` | Authenticated but not authorized for this action |
| `404` | `NOT_FOUND` | Resource does not exist |
| `409` | `CONFLICT` | Resource already exists or state conflict |
| `422` | `UNPROCESSABLE` | Request is well-formed but cannot be processed |
| `429` | `RATE_LIMITED` | Too many requests; check `retryAfter` |
| `500` | `INTERNAL_ERROR` | Unexpected server error |
| `503` | `FEATURE_UNAVAILABLE` | Required feature is disabled or unhealthy |

---

## Configuration Schema

Full TypeScript interface in `src/core/config.ts`. Key structure:

```typescript
interface ArgentumConfig {
  // Instance identity
  name: string;

  // HTTP server
  server: {
    port: number;
    host: string;
    cors: {
      enabled: boolean;
      origins: string[];
    };
    rateLimit: {
      enabled: boolean;
      windowMs: number;
      maxRequests: number;
    };
  };

  // LLM configuration
  model: {
    provider: 'openrouter' | 'anthropic' | 'openai';
    defaultModel: string;
    fallbackModel?: string;
    maxTokens: number;
    temperature: number;
    retryAttempts: number;
    retryDelayMs: number;
  };

  // Feature toggles
  features: Record<string, {
    enabled: boolean;
    [key: string]: unknown;
  }>;

  // Communication channels
  channels: {
    telegram?: {
      enabled: boolean;
      token?: string;
      allowedUsers?: number[];
      allowedChats?: number[];
    };
    webchat?: {
      enabled: boolean;
      port: number;
      maxConnections: number;
      messageHistory: number;
    };
    discord?: {
      enabled: boolean;
      token?: string;
    };
  };

  // Memory backends
  memory: {
    primary: 'sqlite' | 'markdown' | 'supabase';
    path: string;
    selfEvolving: boolean;
    compressionThreshold: number;
    supabaseUrl?: string;
    supabaseKey?: string;
  };

  // Security settings
  security: {
    policy: string;
    secrets: 'encrypted' | 'plain';
    auditLog: boolean;
    allowlistMode: 'permissive' | 'strict';
    apiToken?: string;
  };

  // Backup settings
  backup?: {
    enabled: boolean;
    intervalHours: number;
    retentionDays: number;
    path: string;
  };
}
```
