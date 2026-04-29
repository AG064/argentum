# User Guide

A comprehensive guide to operating Argentum in production and daily use. This document covers architecture, configuration, memory management, security, deployment, monitoring, and more.

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

Argentum is built around three principles:

1. **Modularity over monolith** — every capability is a feature module you can enable or disable independently
2. **Memory as a first-class citizen** — the agent does not just process messages; it remembers, learns, and evolves
3. **Security by default** — encryption, audit logging, and policy enforcement are baked in, not bolted on

### System Diagram

```
┌──────────────────────────────────────────────────────────────────┐
│                        Your Infrastructure                        │
│                                                                  │
│   ┌─────────────────────────────────────────────────────────┐    │
│   │                    Argentum Gateway                       │    │
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
| **Channels** | `src/channels/` | Protocol adapters (Telegram, Discord, Webchat, SMS, Email) |
| **Features** | `src/features/*/index.ts` | 59 individual modules |
| **Security** | `src/security/` | Policy engine, encrypted secrets, allowlists |

### The Agentic Tool Loop

When a user sends a message, Argentum processes it through this loop:

1. **Message received** — channel adapter normalizes the input into a standard `Message` format
2. **Security check** — allowlists, rate limiting, content filtering enforced
3. **Auto-capture** — feature detects decisions, lessons, errors in the message automatically
4. **LLM call** — message sent to the model with conversation history and available tools
5. **Tool execution** — if the model calls a tool, it executes and results are fed back into the context
6. **Memory update** — semantic memory stores the interaction; knowledge graph updates entity relationships
7. **Response** — final text returned to the user via the channel adapter

The loop runs up to 10 iterations per message to handle complex multi-step tasks. You can configure this with `agent.maxIterations`.

### Feature System

Each feature is a self-contained module in `src/features/<name>/index.ts`. Features are independent and configurable. They can:
- Expose tools to the LLM
- Run background jobs on a schedule
- React to events in the system
- Store and retrieve data

Enabled features are listed in `argentum.json` under the `features` key. You can enable or disable any feature without touching code.

Current feature count: **59 features** including audit logging, semantic search, cron scheduling, morning briefings, mesh workflows, encrypted secrets, goal tracking, and many more.

---

## 2. Agent Creation and Management

### Creating Your First Agent

After running `argentum init`, your agent is ready to use. The default configuration creates a general-purpose agent with sensible defaults. To customize:

```bash
# View current configuration
argentum config

# Set a custom agent name
argentum config name "My Personal Assistant"

# Set a custom system prompt
argentum config agent.systemPrompt "You are a helpful coding assistant..."

# Show full config as JSON
cat argentum.json
```

The first time you start, the agent will guide you through setting up your API key. You can also set it as an environment variable before starting:

```bash
export OPENROUTER_API_KEY=sk-or-v1-...
argentum gateway start
```

### Managing Multiple Agents

Argentum supports multiple agent profiles for different use cases. Create a specialized agent:

```bash
# Create a coding-focused agent
argentum agents create --name "coding-assistant" \
  --system-prompt "You are an expert programmer specializing in TypeScript and Python..." \
  --model "anthropic/claude-sonnet-4-20250514"
```

List all configured agents:

```bash
argentum agents list
```

Output:

```
NAME                MODEL                           ACTIVE
default             claude-sonnet-4-20250514        yes
coding-assistant    claude-sonnet-4-20250514        no
```

Switch to a different agent:

```bash
argentum agents use "coding-assistant"
```

Delete an agent profile:

```bash
argentum agents delete "coding-assistant"
```

### Agent Configuration

Key agent settings in `argentum.json`:

```json
{
  "agent": {
    "name": "Argentum Assistant",
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
    "retryAttempts": 3,
    "retryDelayMs": 1000
  }
}
```

### Conversation Sessions

Argentum maintains conversation history per user in `data/sessions.db`. Sessions include full message history, tool calls made, and LLM usage statistics.

```bash
# View recent sessions
argentum sessions list

# View a specific session's full history
argentum sessions view <session-id>

# Clear a session's history (keeps the user, resets conversation)
argentum sessions clear <session-id>

# Export a session as JSON
argentum sessions export <session-id> > session.json
```

Sessions are useful for:
- Continuing long conversations across restarts
- Analyzing how the agent approaches certain problems
- Auditing what information was provided to the model

### Changing Models

You can override the default model per request via the API, or permanently via config:

```bash
# Use a different model for this session
argentum config model.defaultModel "openai/gpt-4o"

# Restart to apply
argentum gateway restart
```

Available providers:
- **OpenRouter** — Recommended. Access to 100+ models including Claude, GPT-4, Llama, Mistral
- **Anthropic** — Direct API access to Claude models
- **OpenAI** — GPT-4, GPT-4o, GPT-3.5 Turbo

---

## 3. Memory System Explained

Argentum has a multi-layered memory architecture. Each layer serves a different purpose, from short-term context to long-term knowledge.

### Memory Layers

```
┌────────────────────────────────────────────┐
│           Semantic Memory                  │  ← Fast, keyword + vector-style search
│     (SQLite + embedding models)             │     Used every conversation
├────────────────────────────────────────────┤
│           Knowledge Graph                  │  ← Entity relationships
│    (nodes + edges, graph traversal)         │     Used for reasoning
├────────────────────────────────────────────┤
│         Markdown Memory                    │  ← Human-readable notes
│      (files on disk, watched)              │     Used for long-term facts
├────────────────────────────────────────────┤
│          Session Memory                   │  ← Current conversation
│        (in-memory, per session)            │     Used for context window
└────────────────────────────────────────────┘
```

### Semantic Memory

The primary memory layer. Stores facts, conversations, and learned information in SQLite with full-text search.

```bash
# Store something explicitly
curl -X POST http://localhost:18789/memory/store \
  -H "Content-Type: application/json" \
  -d '{"content": "Alice is learning TypeScript", "tags": ["person", "learning"]}'

# Search memory
curl "http://localhost:18789/memory/search?q=Alice%20TypeScript"
```

Key characteristics:
- Automatic indexing by the `self-evolving-memory` feature
- Entries tagged and categorized automatically
- Configurable compression when entries exceed threshold
- Persistent across restarts (SQLite)

### Knowledge Graph

Stores entities and their relationships. Enables reasoning about connected facts.

```bash
# Query by entity
curl "http://localhost:18789/memory/graph?entity=alice"
```

Example knowledge graph entry:
```
Entity: Alice (type: person)
  - works_at: Acme Corp (type: company)
  - speaks: English, Spanish
  - interested_in: programming, AI, space, photography
```

The graph is updated automatically as the agent processes messages and detects entities.

### Markdown Memory

Flat files on disk in the `data/memory/` directory. Watched by a file watcher — changes are picked up immediately without restart.

```bash
# Create a memory file
cat > data/memory/facts.md << 'EOF'
# Facts about Alice

## Preferences
- Morning person
- Prefers dark mode
- Uses Arch Linux

## Goals
- Find a programming job by end of 2026
- Learn Rust
EOF
```

Markdown files are parsed and integrated into the agent's context. This is useful for:
- Manually curated facts
- Notes from other systems
- Information that should survive memory compression

### Memory Management

```bash
# Show memory statistics
argentum memory stats

# Clear old entries (before a certain date)
argentum memory purge --before 2026-01-01

# Export all memories
argentum memory export > memories.json

# Import memories
argentum memory import memories.json
```

The `self-evolving-memory` feature automatically:
- Consolidates similar memories to save space
- Discovers patterns in stored information
- Applies configurable decay to low-relevance entries

---

## 4. Skills and How to Use Them

Skills are reusable capability packs that extend what your agent can do. They live in the `skills/` directory and can be installed, updated, and removed independently of the core framework.

### Built-in Skills

Argentum ships with several skills already installed:

| Skill | What It Does |
|---|---|
| `weather` | Current weather and forecasts via wttr.in or Open-Meteo |
| `summarize` | Summarize URLs, PDFs, images, audio, YouTube videos |
| `gog` | Google Workspace: Gmail, Calendar, Drive, Sheets, Docs |
| `xurl` | Twitter/X API: post, reply, search, DMs, media |
| `himalaya` | CLI email client via IMAP/SMTP |
| `telegram` | Telegram Bot API workflows |

### Listing Installed Skills

```bash
argentum skills list
```

Output:

```
NAME                VERSION   ENABLED
summarize           1.0.0     yes
weather             1.0.0     yes
gog                 1.0.0     yes
xurl                1.0.0     yes
```

### Installing New Skills

```bash
# Install from a URL or local path
argentum skills install https://github.com/example/skill-repo

# Install from ClawHub marketplace
argentum skill install my-skill
```

### Creating Custom Skills

See the [Developer Guide](./DEVELOPER_GUIDE.md#7-how-to-create-a-new-skill) for the full process. In brief:

1. Create `skills/my-skill/SKILL.md`
2. Write `skills/my-skill/src/index.ts` with your logic
3. Add metadata for CLI integration

### Invoking Skills

Skills are typically invoked by the agent when relevant, but you can also call them directly:

```bash
argentum skills run weather --location "London"
```

---

## 5. Security Best Practices

Argentum includes multiple security layers. Below are recommended practices for production deployments.

### Enable Strict Mode

Start with `security.policy: "strict"` in `argentum.json`:

```json
{
  "security": {
    "policy": "strict",
    "auditLog": true,
    "allowlistMode": "strict"
  }
}
```

In strict mode, all actions that are not explicitly allowed are denied. In permissive mode (default), everything is allowed unless blocked by a rule.

### User Allowlisting

Limit access to specific users by their platform ID:

```json
{
  "channels": {
    "telegram": {
      "enabled": true,
      "token": "...",
      "allowedUsers": [123456789, 987654321]
    }
  }
}
```

This prevents unauthorized users from interacting with your agent even if they know the Telegram bot token.

### Encrypted Secrets

Argentum encrypts secrets at rest using AES-256-GCM. Set a secret key:

```bash
export AGCLAW_SESSION_SECRET=$(openssl rand -hex 32)
```

Never commit API keys to version control. Use environment variables or the encrypted secrets feature:

```bash
# Store a secret securely
argentum secrets set OPENROUTER_API_KEY "sk-or-v1-..."

# List stored secrets (values hidden)
argentum secrets list
```

### Audit Logging

Enable the audit log to track all actions:

```json
{
  "security": {
    "auditLog": true
  }
}
```

View audit logs:

```bash
argentum audit list --limit 50
argentum audit search --actor alice --since 2026-03-22
```

The audit log is stored in `data/argentum.db` in an immutable table — entries cannot be deleted or modified after the fact.

### Rate Limiting

Protect against abuse with built-in rate limiting:

```json
{
  "server": {
    "rateLimit": {
      "enabled": true,
      "windowMs": 60000,
      "maxRequests": 100
    }
  }
}
```

This limits each IP to 100 requests per minute to the HTTP API.

### Content Filtering

The `content-filtering` feature scans messages and tool outputs for sensitive data patterns. Enable it:

```json
{
  "features": {
    "content-filtering": { "enabled": true }
  }
}
```

It automatically redacts:
- API keys and tokens
- Credit card numbers
- Social security numbers
- Email addresses and phone numbers

---

## 6. Deployment Options

### Local Development

```bash
npm install
npm link
argentum init
argentum gateway start
```

That's the entire setup. The gateway runs at `http://localhost:18789`.

### Docker (Recommended for Production)

A production-ready Docker setup is included:

```bash
cd argentum/docker
cp .env.example .env  # fill in your API keys
docker compose up -d
```

The Docker setup includes:
- Argentum gateway container
- Health check endpoint
- Volume mounts for data persistence
- Restart policy

### Systemd Service (Linux)

For bare-metal Linux servers, create a systemd unit:

```ini
# /etc/systemd/system/argentum.service
[Unit]
Description=Argentum AI Agent
After=network.target

[Service]
Type=simple
User=ag064
WorkingDirectory=/home/ag064/argentum
ExecStart=/home/ag064/argentum/bin/argentum.js gateway start
Restart=always
RestartSec=5
Environment=OPENROUTER_API_KEY=sk-or-v1-...
Environment=AGCLAW_PORT=18789

[Install]
WantedBy=multi-user.target
```

```bash
sudo systemctl enable argentum
sudo systemctl start argentum
sudo systemctl status argentum
```

### Reverse Proxy Setup

For production, put Argentum behind nginx or Caddy with HTTPS:

```nginx
# /etc/nginx/sites-available/argentum
server {
    listen 443 ssl;
    server_name yourdomain.com;

    ssl_certificate /etc/letsencrypt/live/yourdomain.com/fullchain.pem;
    ssl_certificate_key /etc/letsencrypt/live/yourdomain.com/privkey.pem;

    location / {
        proxy_pass http://127.0.0.1:18789;
        proxy_http_version 1.1;
        proxy_set_header Upgrade $http_upgrade;
        proxy_set_header Connection 'upgrade';
        proxy_set_header Host $host;
        proxy_set_header X-Real-IP $remote_addr;
        proxy_cache_bypass $http_upgrade;
    }
}
```

### Environment-Specific Configuration

Use environment variables to override config for different environments:

```bash
# Production
AGCLAW_LOG_LEVEL=warn
AGCLAW_PORT=18789
NODE_ENV=production

# Development
AGCLAW_LOG_LEVEL=debug
AGCLAW_PORT=3000
NODE_ENV=development
```

---

## 7. Monitoring and Logging

### Log Levels

Set via `AGCLAW_LOG_LEVEL` or `argentum.json`:

```bash
export AGCLAW_LOG_LEVEL=debug  # debug, info, warn, error
```

In development, use `pretty` format for human-readable output:

```bash
export AGCLAW_LOG_FORMAT=pretty
```

In production, use `json` format for structured log aggregation:

```bash
export AGCLAW_LOG_FORMAT=json
```

### Viewing Logs

```bash
# Real-time gateway logs
argentum gateway logs

# Filter by level
argentum gateway logs --level error

# Follow mode (like tail -f)
argentum gateway logs --follow

# Export logs to file
argentum gateway logs --export ./logs/$(date +%Y%m%d).log

# Last 100 lines
argentum gateway logs --lines 100
```

### Structured Log Format

JSON logs are structured for easy parsing by log aggregators:

```json
{
  "level": "info",
  "time": "2026-03-23T01:00:00.000Z",
  "feature": "agent",
  "msg": "Processing message",
  "userId": "alice",
  "sessionId": "sess_abc123",
  "length": 142
}
```

### Prometheus Metrics

```bash
curl http://localhost:18789/metrics
```

Returns Prometheus-compatible metrics:

```
# HELP argentum_messages_total Total messages processed
# TYPE argentum_messages_total counter
argentum_messages_total 4821

# HELP argentum_tool_calls_total Tool invocations
# TYPE argentum_tool_calls_total counter
argentum_tool_calls_total{model="claude-sonnet-4",tool="read"} 1203
argentum_tool_calls_total{model="claude-sonnet-4",tool="web_search"} 387

# HELP argentum_llm_tokens_total Tokens used
# TYPE argentum_llm_tokens_total counter
argentum_llm_tokens_total{prompt_or_completion="prompt"} 892340
argentum_llm_tokens_total{prompt_or_completion="completion"} 412850

# HELP argentum_memory_entries_total Memory entries
# TYPE argentum_memory_entries_total gauge
argentum_memory_entries_total 1247

# HELP argentum_features_active Currently active features
# TYPE argentum_features_active gauge
argentum_features_active 12
```

### Health Checks

```bash
# Quick health check
curl http://localhost:18789/health

# Detailed status
argentum status
```

### Debugging Feature Issues

List all features with their health status:

```bash
argentum features list --verbose
```

Output:

```
NAME                    STATUS      HEALTH        VERSION
sqlite-memory           active      ok            0.0.3
semantic-search         active      ok            0.0.3
audit-log               active      ok            0.0.3
telegram                inactive    -             0.0.3
morning-briefing         active      ok            0.0.3
```

---

## 8. Backup and Recovery

### Manual Backup

```bash
# List existing backups
ls ./backups/

# Create a manual backup now
argentum backup create

# Restore from a specific backup
argentum backup restore backup-2026-03-18T18-58-44
```

### What Gets Backed Up

Argentum backs up all critical data files:

| File | Description |
|---|---|
| `data/argentum.db` | Main SQLite database (audit log, decisions) |
| `data/semantic-memory.db` | Semantic memory entries |
| `data/knowledge.db` | Knowledge graph |
| `data/sessions.db` | Conversation sessions |
| `data/skills-library.db` | Installed skills |
| `data/goals.db` | Goals and decomposition |
| `data/life-domains.db` | Life domain tracking |
| `argentum.json` | Configuration |

### Automated Backups

Enable in `argentum.json`:

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

Backups run on the schedule you specify. Old backups beyond `retentionDays` are automatically cleaned up.

### Disaster Recovery Procedure

1. Stop the gateway cleanly:
   ```bash
   argentum gateway stop
   ```

2. Restore files from a known-good backup:
   ```bash
   argentum backup restore <backup-name>
   ```

3. Verify the restore:
   ```bash
   argentum gateway start
   curl http://localhost:18789/health
   ```

4. Check session continuity:
   ```bash
   argentum sessions list
   ```

5. Confirm memory is intact:
   ```bash
   argentum memory stats
   ```

### Migration Between Machines

Copy the `data/` directory and `argentum.json` to the new machine. Ensure the same `AGCLAW_SESSION_SECRET` is set to decrypt any encrypted secrets. Then run `argentum gateway start`.

---

## 9. API Reference

See the full [API Reference](./API.md) for complete REST endpoint documentation, WebSocket events, error codes, and configuration schema.

Quick reference for common endpoints:

| Endpoint | Method | Description |
|---|---|---|
| `/health` | GET | Health check with version and feature count |
| `/metrics` | GET | Prometheus-compatible metrics |
| `/chat` | POST | Send a message to the agent |
| `/chat/stream` | POST | Streaming response (Server-Sent Events) |
| `/memory/search` | GET | Search semantic memory |
| `/memory/store` | POST | Store a new memory entry |
| `/memory/graph` | GET | Query the knowledge graph |
| `/agents` | GET/POST | List or create agents |
| `/features` | GET | List all features with status |
| `/features/:name` | POST | Enable or disable a feature |
| `/config` | GET/PATCH | View or update configuration |
| `/sessions` | GET | List recent sessions |

---

## 10. Configuration Reference

### Full Configuration Schema

```typescript
interface ArgentumConfig {
  name: string;                          // Instance name
  version: string;                       // Config format version (const: "0.0.3")

  server: {
    port: number;                         // Gateway port (default: 18789)
    host: string;                         // Bind address (default: 0.0.0.0)
    cors: {
      enabled: boolean;
      origins: string[];
    };
    rateLimit: {
      enabled: boolean;
      windowMs: number;                   // Time window in ms
      maxRequests: number;                // Max requests per window
    };
    auth: {
      token?: string;                     // Bearer token for API auth
    };
  };

  agent: {
    name: string;                         // Display name
    systemPrompt?: string;                 // Override default system prompt
    maxIterations: number;                 // Max tool loop iterations (default: 10)
    temperature: number;                  // LLM temperature 0-2 (default: 0.7)
    tools: string[];                      // Enabled tool names
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
    discord?: {
      enabled: boolean;
      token?: string;
      guildId?: string;
    };
    slack?: {
      enabled: boolean;
      token?: string;
      channelId?: string;
    };
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
    policy: 'permissive' | 'strict';
    secrets: 'encrypted' | 'plain';
    auditLog: boolean;
    allowlistMode: 'permissive' | 'strict';
  };

  backup: {
    enabled: boolean;
    intervalHours: number;
    retentionDays: number;
    path: string;
  };
}
```

### Environment Variables

| Variable | Type | Default | Description |
|---|---|---|---|
| `OPENROUTER_API_KEY` | string | — | OpenRouter API key (required for most models) |
| `ANTHROPIC_API_KEY` | string | — | Anthropic API key (direct Claude access) |
| `OPENAI_API_KEY` | string | — | OpenAI key (Whisper STT, DALL-E) |
| `AGCLAW_PORT` | number | `18789` | Gateway HTTP port |
| `AGCLAW_HOST` | string | `0.0.0.0` | Gateway bind address |
| `AGCLAW_DB_PATH` | string | `./data/argentum.db` | SQLite database path |
| `AGCLAW_CONFIG_PATH` | string | `./argentum.json` | Config file path |
| `AGCLAW_LOG_LEVEL` | string | `info` | Log level: `debug`, `info`, `warn`, `error` |
| `AGCLAW_LOG_FORMAT` | string | `pretty` | Log format: `pretty` or `json` |
| `AGCLAW_TELEGRAM_TOKEN` | string | — | Telegram bot token |
| `AGCLAW_FCM_KEY` | string | — | Firebase Cloud Messaging key |
| `AGCLAW_SESSION_SECRET` | string | auto-generated | Secret for session encryption |
| `SUPABASE_URL` | string | — | Supabase project URL |
| `SUPABASE_KEY` | string | — | Supabase anon key |
| `NODE_ENV` | string | `development` | Environment mode |

### Hot-Reload

Argentum watches `argentum.json` for changes and applies them without restart. For environment variable changes, a restart is required:

```bash
argentum gateway restart
```

---

*For tutorials and step-by-step guides, see the [tutorials directory](./tutorials/).*
