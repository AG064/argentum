# User Guide

A comprehensive guide to operating AG-Claw in production and daily use. This document covers architecture, configuration, memory management, security, deployment, and more.

---

## Table of Contents

1. [Architecture Overview](#1-architecture-overview)
2. [Agent Creation and Management](#2-agent-creation-and-management)
3. [Memory System Explained](#3-memory-system-explained)
4. [Skills and How to Use Them](#4-skills-and-how-to-use-them)
5. [Security Best Practices](#5-security-best-practices)
6. [Deployment Options](#6-deployment-options)
7. [Monitoring and Logging](#7-monitoring-and-logging)
8. [Backup and Recovery](#8-backup-and-recovery)
9. [API Reference](#9-api-reference)
10. [Configuration Reference](#10-configuration-reference)

---

## 1. Architecture Overview

### Design Philosophy

AG-Claw is built around three principles:

1. **Modularity over monolith** — every capability is a feature module you can enable or disable independently
2. **Memory as a first-class citizen** — the agent doesn't just process messages; it remembers, learns, and evolves
3. **Security by default** — encryption, audit logging, and policy enforcement are baked in, not bolted on

### System Diagram

```
┌──────────────────────────────────────────────────────────────────┐
│                        Your Infrastructure                        │
│                                                                  │
│   ┌─────────────────────────────────────────────────────────┐    │
│   │                    AG-Claw Gateway                       │    │
│   │                        :18789                            │    │
│   ├───────────────┬────────────────────┬────────────────────┤    │
│   │  Channels      │   Feature Modules  │   Core Framework    │    │
│   │               │                    │                     │    │
│   │  • Telegram    │  • sqlite-memory  │  • Plugin Loader   │    │
│   │  • Discord     │  • semantic-search │  • LLM Provider    │    │
│   │  • Webchat     │  • cron-scheduler │  • Config Manager  │    │
│   │  • Slack       │  • morning-brief  │  • Logger          │    │
│   │  • WhatsApp    │  • audit-log      │  • Security Layer   │    │
│   │  • Email       │  • mesh-workflows │                    │    │
│   │  • SMS         │  • ... (53 more)  │                     │    │
│   └───────┬───────┴────────┬──────────┴──────────┬──────────┘    │
│           │                 │                      │               │
│           │     ┌───────────▼────────────────────▼─────────┐     │
│           │     │           Agentic Tool Loop               │     │
│           │     │                                            │     │
│           │     │   Message → LLM → Tool Calls → Memory      │     │
│           │     │                                            │     │
│           │     │   ┌─────────────────────────────────────┐ │     │
│           │     │   │  Semantic Memory  │  Knowledge Graph │ │     │
│           │     │   │  SQLite Store    │  Checkpoint      │ │     │
│           │     │   └─────────────────────────────────────┘ │     │
│           │     └────────────────────────────────────────────┘     │
│           │                                                       │
│   ┌───────▼────────┐                            ┌────────────────▼┐│
│   │   End Users    │                            │  External APIs ││
│   │ Telegram, etc. │                            │ OpenRouter,    ││
│   └────────────────┘                            │ Anthropic, etc ││
│                                                  └────────────────┘│
└──────────────────────────────────────────────────────────────────┘
```

### Core Components

| Component | File(s) | Role |
|---|---|---|
| **Gateway** | `src/index.ts` | HTTP server, boots all features, manages lifecycle |
| **CLI** | `src/cli.ts` | User-facing command interface |
| **Plugin Loader** | `src/core/plugin-loader.ts` | Discovers, loads, starts, and health-checks 59 feature modules |
| **Agent** | `src/index.ts` (class `Agent`) | Agentic Tool Loop — orchestrates LLM + tools + memory |
| **LLM Provider** | `src/core/llm-provider.ts` | Abstraction over OpenRouter, Anthropic, OpenAI |
| **Memory** | `src/memory/` | Semantic search, knowledge graph, SQLite persistence |
| **Channels** | `src/channels/` | Protocol adapters (Telegram, Discord, Webchat) |
| **Features** | `src/features/*/index.ts` | 59 individual modules |
| **Security** | `src/security/` | Policy engine, encrypted secrets, allowlists |

### The Agentic Tool Loop

When a user sends a message, AG-Claw processes it through this loop:

1. **Message received** — channel adapter normalizes the input
2. **Security check** — allowlists, rate limiting, content filtering
3. **Auto-capture** — feature detects decisions, lessons, errors in the message
4. **LLM call** — message sent to the model with conversation history and available tools
5. **Tool execution** — if the model calls a tool, it executes and results are fed back
6. **Memory update** — semantic memory stores the interaction; knowledge graph updates
7. **Response** — final text returned to the user via the channel adapter

The loop runs up to 10 iterations per message to handle complex multi-step tasks.

---

## 2. Agent Creation and Management

### Creating Your First Agent

After `agclaw init`, your agent is ready to use. The default configuration creates a general-purpose agent. To customize:

```bash
# View current configuration
agclaw config

# Set a custom agent name
agclaw config name "My Personal Assistant"

# Set a custom system prompt (via environment)
export AGCLAW_SYSTEM_PROMPT="You are a helpful coding assistant..."
```

### Managing Multiple Agents

AG-Claw supports multiple agent profiles. Create a new agent:

```bash
# Create a named agent profile
agclaw agents create --name "coding-assistant" \
  --system-prompt "You are an expert programmer..." \
  --model "anthropic/claude-sonnet-4-20250514"
```

List agents:

```bash
agclaw agents list
```

Switch between agents:

```bash
agclaw agents use "coding-assistant"
```

### Agent Configuration

Key agent settings in `agclaw.json`:

```json
{
  "agent": {
    "name": "AG-Claw Assistant",
    "systemPrompt": "You are a helpful AI assistant...",
    "maxIterations": 10,
    "temperature": 0.7,
    "tools": ["web_search", "read_file", "write_file", "run_command"]
  },
  "model": {
    "provider": "openrouter",
    "defaultModel": "anthropic/claude-sonnet-4-20250514",
    "fallbackModel": "openai/gpt-4o",
    "maxTokens": 8192,
    "temperature": 0.7,
    "retryAttempts": 3
  }
}
```

### Conversation Sessions

AG-Claw maintains conversation history per user:

```bash
# View recent sessions
agclaw sessions list

# View a specific session
agclaw sessions view <session-id>

# Clear a session
agclaw sessions clear <session-id>
```

Sessions are stored in `data/sessions.db` and include full message history, tool calls, and LLM usage stats.

---

## 3. Memory System Explained

AG-Claw has a multi-layered memory architecture. Each layer serves a different purpose.

### Memory Layers

```
┌────────────────────────────────────────────┐
│           Semantic Memory                  │  ← Fast, keyword + vector search
│     (SQLite + embedding models)            │     Used every conversation
├────────────────────────────────────────────┤
│           Knowledge Graph                  │  ← Entity relationships
│    (nodes + edges, graph traversal)       │     Used for reasoning
├────────────────────────────────────────────┤
│         Markdown Memory                    │  ← Human-readable notes
│      (files on disk, watched)             │     Used for long-term facts
├────────────────────────────────────────────┤
│         Self-Evolving Memory               │  ← Auto-consolidation
│  (periodic compression + abstraction)     │     Keeps memory efficient
└────────────────────────────────────────────┘
```

### Semantic Memory

The primary working memory. Stores interactions, learnings, decisions.

```bash
# Store a memory manually
agclaw memory store "decision" "Use PostgreSQL for the main database"

# Search memory
agclaw memory search "database decisions"

# View recent memories
agclaw memory recent --limit 10

# Memory statistics
agclaw memory stats
```

Via the agent tool `memory_search`:

```
User: What decisions did we make about the API design?
Agent: Let me check... [calls memory_search tool]
     Found 3 relevant memories:
     [decision] Use REST over GraphQL for the public API (accessed 5x)
     [decision] Version all endpoints under /api/v1/ prefix (accessed 3x)
     [decision] Use JWT tokens with 1-hour expiry (accessed 2x)
```

### Knowledge Graph

Stores entities and their relationships. Enables reasoning about connections.

```bash
# View knowledge graph stats
agclaw memory graph stats

# Export graph
agclaw memory graph export > graph.json
```

### Memory Compression

When semantic memory exceeds the compression threshold (default: 10,000 entries), the self-evolving-memory feature automatically:

1. Identifies related entries
2. Consolidates them into higher-level abstractions
3. Deletes redundant entries
4. Preserves the essential information in condensed form

Configure it:

```json
{
  "features": {
    "self-evolving-memory": {
      "enabled": true,
      "compressionThreshold": 10000,
      "consolidationIntervalHours": 24
    }
  }
}
```

### Checkpointing

Save and resume long-running tasks:

```bash
# Save a checkpoint (via agent tool)
Agent: memory_checkpoint(taskId="build-2024-01", state={...})

# Resume
Agent: memory_resume(taskId="build-2024-01")
```

---

## 4. Skills and How to Use Them

Skills are reusable capability packages that extend what your agent can do. AG-Claw ships with a built-in skills library.

### Built-in Skills

| Skill | What it does |
|---|---|
| `web_search` | Search the web via DuckDuckGo |
| `get_current_time` | Return current date/time |
| `read_file` | Read a file from disk |
| `write_file` | Write content to a file |
| `run_command` | Execute a shell command |
| `memory_search` | Search semantic memory |
| `memory_store` | Store a new memory |
| `memory_checkpoint` | Save a task checkpoint |
| `memory_resume` | Resume a checkpointed task |

### Installing Additional Skills

Browse the skills library:

```bash
agclaw skills list
```

Install a skill:

```bash
agclaw skills install <skill-name>
```

### Creating Custom Skills

Create `src/features/skills-library/<my-skill>/index.ts`:

```typescript
import { SkillModule } from '../../types';

const mySkill: SkillModule = {
  name: 'my-skill',
  version: '0.1.0',
  description: 'Does something useful',

  tools: [{
    name: 'my_tool',
    description: 'Does something useful',
    parameters: {
      input: { type: 'string', required: true }
    },
    execute: async (params) => {
      return `Processed: ${params.input}`;
    }
  }],

  init: async () => {},
  start: async () => {},
  stop: async () => {},
};

export default mySkill;
```

Register it in `config/default.yaml` or `agclaw.json`:

```json
{
  "features": {
    "skills-library": {
      "enabled": true,
      "skills": ["my-skill"]
    }
  }
}
```

---

## 5. Security Best Practices

### Essential Security Steps

**1. Never commit API keys to git**

Use environment variables, not hardcoded tokens:

```bash
# Good
export OPENROUTER_API_KEY=sk-or-v1-...

# Bad — will end up on GitHub
echo '"token": "sk-or-v1-..."' >> agclaw.json
```

Add `agclaw.json` to `.gitignore`:

```gitignore
agclaw.json
data/
*.db
.env
```

**2. Use encrypted secrets storage**

```bash
# Encrypt a secret
agclaw secrets set OPENROUTER_API_KEY "sk-or-v1-..."
```

**3. Configure allowlists**

Restrict Telegram access to specific user IDs:

```json
{
  "channels": {
    "telegram": {
      "allowedUsers": [123456789, 987654321]
    }
  }
}
```

**4. Enable rate limiting**

```json
{
  "security": {
    "rateLimit": {
      "windowMs": 60000,
      "maxRequests": 30
    }
  }
}
```

**5. Review audit logs regularly**

```bash
agclaw audit log --last 24h
```

### Security Features Reference

| Feature | File | Purpose |
|---|---|---|
| Encrypted secrets | `src/security/encrypted-secrets.ts` | AES-256 encryption for API keys |
| Policy engine | `src/security/policy-engine.ts` | YAML-defined security policies |
| Allowlists | `src/security/allowlists.ts` | User/chat whitelist |
| Rate limiting | `src/features/rate-limiting/` | Per-user request throttling |
| Audit logging | `src/features/audit-log/` | Immutable tool-call records |
| Content filtering | `src/features/content-filtering/` | Input sanitization |

---

## 6. Deployment Options

### Local Development

```bash
npm install
npm run build
npm start
```

### Docker (Recommended for Production)

```bash
# Build the image
npm run docker:build

# Start containers
npm run docker:up

# View logs
docker compose -f docker/docker-compose.yml logs -f

# Stop
npm run docker:down
```

### Docker Compose Configuration

Edit `docker/docker-compose.yml` for your environment:

```yaml
services:
  ag-claw:
    image: ag-claw:latest
    ports:
      - "3000:3000"
    environment:
      - OPENROUTER_API_KEY=${OPENROUTER_API_KEY}
      - AGCLAW_PORT=3000
    volumes:
      - ./data:/app/data
      - ./agclaw.json:/app/agclaw.json:ro
    restart: unless-stopped
    healthcheck:
      test: ["CMD", "curl", "-f", "http://localhost:3000/health"]
      interval: 30s
      timeout: 10s
      retries: 3
```

### VPS Deployment

1. SSH into your VPS
2. Install Docker: `curl -fsSL get.docker.com | bash`
3. Clone the repo: `git clone https://github.com/AG064/ag-claw.git`
4. Copy `.env` file with your API keys
5. Run `npm run docker:build && npm run docker:up`
6. Set up a reverse proxy (nginx/Caddy) for HTTPS
7. Point your domain to the VPS

### Health Monitoring

AG-Claw exposes a health endpoint:

```bash
curl http://localhost:3000/health
```

The `health-monitoring` feature runs periodic checks on all active features and alerts if any become unhealthy.

---

## 7. Monitoring and Logging

### Log Levels

Configure via `agclaw.json` or `AGCLAW_LOG_LEVEL`:

```bash
export AGCLAW_LOG_LEVEL=debug  # debug, info, warn, error
```

### Viewing Logs

```bash
# Real-time gateway logs
agclaw gateway logs

# Filter by level
agclaw gateway logs --level error

# Follow mode
agclaw gateway logs --follow

# Export logs
agclaw gateway logs --export ./logs/$(date +%Y%m%d).log
```

### Structured Log Format

Logs are output as JSON in production (`format: json`) and pretty-printed in development (`format: pretty`):

```json
{
  "level": "info",
  "time": "2026-03-23T01:00:00.000Z",
  "feature": "agent",
  "msg": "Processing message",
  "length": 142
}
```

### Metrics Endpoint

```bash
curl http://localhost:3000/metrics
```

Returns prometheus-compatible metrics including:
- `agclaw_messages_total` — total messages processed
- `agclaw_tool_calls_total` — tool invocations by name
- `agclaw_llm_tokens_total` — tokens used (prompt + completion)
- `agclaw_memory_entries_total` — entries in semantic memory
- `agclaw_features_active` — number of active features

---

## 8. Backup and Recovery

### Manual Backup

```bash
# AG-Claw creates timestamped backups
ls ./backups/

# Create a manual backup
agclaw backup create

# Restore from backup
agclaw backup restore backup-2026-03-18T18-58-44
```

### What Gets Backed Up

- `data/agclaw.db` — main SQLite database
- `data/semantic-memory.db` — semantic memory
- `data/knowledge.db` — knowledge graph
- `data/sessions.db` — conversation sessions
- `data/skills-library.db` — installed skills
- `data/goals.db` — goals and decomposition
- `data/life-domains.db` — life domains
- `agclaw.json` — configuration

### Automated Backups

Configure in `agclaw.json`:

```json
{
  "backup": {
    "enabled": true,
    "intervalHours": 24,
    "retentionDays": 7,
    "path": "./backups"
  }
}
```

### Disaster Recovery

1. Stop the gateway: `agclaw gateway stop`
2. Restore files from backup: `agclaw backup restore <backup-name>`
3. Restart: `agclaw gateway start`
4. Verify: `curl http://localhost:3000/health`

---

## 9. API Reference

See the full [API Reference](./API.md) for complete REST endpoint documentation, WebSocket events, error codes, and configuration schema.

Quick reference:

| Endpoint | Method | Description |
|---|---|---|
| `/health` | GET | Health check |
| `/metrics` | GET | Prometheus metrics |
| `/chat` | POST | Send a message |
| `/memory/search` | GET | Search memory |
| `/memory/store` | POST | Store a memory entry |
| `/agents` | GET | List agents |
| `/features` | GET | List all features |
| `/features/:name` | POST | Enable/disable feature |
| `/config` | GET/PATCH | View/update config |

---

## 10. Configuration Reference

### Full Configuration Schema

```typescript
interface AGClawConfig {
  name: string;                          // Instance name
  server: {
    port: number;                        // Gateway port (default: 18789)
    host: string;                        // Bind address (default: 0.0.0.0)
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
  model: {
    provider: 'openrouter' | 'anthropic' | 'openai';
    defaultModel: string;
    fallbackModel?: string;
    maxTokens: number;
    temperature: number;
    retryAttempts: number;
    retryDelayMs: number;
  };
  features: Record<string, {
    enabled: boolean;
    [key: string]: unknown;
  }>;
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
    };
    // ... other channels
  };
  memory: {
    primary: 'sqlite' | 'markdown' | 'supabase';
    path: string;
    selfEvolving: boolean;
    compressionThreshold: number;
    supabaseUrl?: string;
    supabaseKey?: string;
  };
  security: {
    policy: string;
    secrets: 'encrypted' | 'plain';
    auditLog: boolean;
    allowlistMode: 'permissive' | 'strict';
  };
}
```

### Environment Variables

| Variable | Description | Required |
|---|---|---|
| `OPENROUTER_API_KEY` | OpenRouter API key | Yes (unless using another provider) |
| `ANTHROPIC_API_KEY` | Anthropic API key | No |
| `OPENAI_API_KEY` | OpenAI API key | No |
| `AGCLAW_TELEGRAM_TOKEN` | Telegram bot token | No |
| `AGCLAW_FCM_KEY` | Firebase Cloud Messaging | No |
| `AGCLAW_DB_PATH` | SQLite database path | No |
| `AGCLAW_PORT` | Gateway port | No |
| `AGCLAW_LOG_LEVEL` | Log level | No |
| `SUPABASE_URL` | Supabase project URL | No |
| `SUPABASE_KEY` | Supabase anon key | No |

---

*For advanced topics like multi-agent coordination, mesh workflows, and scaling, see [Tutorial 5: Advanced Patterns](./tutorials/05-advanced-patterns.md).*
