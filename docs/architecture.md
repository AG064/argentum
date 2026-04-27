# Argentum Architecture

```
╔══════════════════════════════════════════════════════════════════╗
║   ██████╗ ██████╗  ██████╗██╗  ██╗██╗   ██╗███████╗          ║
║   ██╔══██╗██╔══██╗██╔════╝██║  ██║██║   ██║██╔════╝          ║
║   ██████╔╝██████╔╝██║     ███████║██║   ██║███████╗          ║
║   ██╔══██╗██╔══██╗██║     ██╔══██║██║   ██║╚════██║          ║
║   ██║  ██║██║  ██║╚██████╗██║  ██║╚██████╔╝███████║          ║
║   ╚═╝  ╚═╝╚═╝  ╚═╝ ╚═════╝╚═╝  ╚═╝ ╚═════╝ ╚══════╝          ║
║                  Modular AI Agent Framework                       ║
╚══════════════════════════════════════════════════════════════════╝
```

How Argentum extends OpenClaw with a modular plugin system, multi-channel support, and a security layer inspired by NemoClaw.

---

## High-Level Overview

```
┌─────────────────────────────────────────────────────────────┐
│                      Argentum Framework                       │
│                                                              │
│  ┌────────────────────────────────────────────────────────┐  │
│  │                  Core Layer                            │  │
│  │  ┌──────────┐  ┌──────────────┐  ┌─────────────────┐  │  │
│  │  │  Config   │  │ Plugin Loader│  │  Security Layer │  │  │
│  │  │  Manager  │  │              │  │  (NemoClaw)     │  │  │
│  │  │  (YAML +  │  │  Dynamic     │  │  ┌───────────┐  │  │  │
│  │  │   Env)    │  │  Feature     │  │  │ Allowlist │  │  │  │
│  │  │           │  │  Loading     │  │  │ Engine    │  │  │  │
│  │  │  Hot      │  │  + Lifecycle │  │  ├───────────┤  │  │  │
│  │  │  Reload   │  │  + Dep       │  │  │ Encrypted │  │  │  │
│  │  │           │  │  Resolution  │  │  │ Secrets   │  │  │  │
│  │  │           │  │              │  │  ├───────────┤  │  │  │
│  │  │           │  │              │  │  │ Policy    │  │  │  │
│  │  │           │  │              │  │  │ Engine    │  │  │  │
│  │  └──────────┘  └──────────────┘  │  └───────────┘  │  │  │
│  │                                   └─────────────────┘  │  │
│  └────────────────────────────────────────────────────────┘  │
│                                                              │
│  ┌────────────────────────────────────────────────────────┐  │
│  │                Feature Registry                        │  │
│  │                                                        │  │
│  │  Features are self-contained modules with:             │  │
│  │  - Metadata (name, version, dependencies)              │  │
│  │  - Lifecycle hooks (init, start, stop, healthCheck)    │  │
│  │  - Own configuration from YAML                         │  │
│  │  - Access to shared context (logger, hooks, emit)      │  │
│  │                                                        │  │
│  │  ┌──────────────┐  ┌──────────────┐  ┌─────────────┐  │  │
│  │  │ Webchat      │  │ Voice        │  │ Knowledge   │  │  │
│  │  │ Browser Auto │  │ Workflows    │  │ Graph       │  │  │
│  │  │ Webhooks     │  │ Live Canvas  │  │ Morning     │  │  │
│  │  │ Container SB │  │ Air-Gapped   │  │ Briefing    │  │  │
│  │  │ ...55 total  │  │              │  │ ...         │  │  │
│  │  └──────────────┘  └──────────────┘  └─────────────┘  │  │
│  └────────────────────────────────────────────────────────┘  │
│                                                              │
│  ┌────────────────────────────────────────────────────────┐  │
│  │               Channel Layer                            │  │
│  │  ┌──────────┐  ┌──────────┐  ┌──────────┐             │  │
│  │  │ Telegram │  │ Webchat  │  │ Mobile   │  ...        │  │
│  │  │ (Bot API)│  │ (WS)     │  │ (Push)   │             │  │
│  │  └──────────┘  └──────────┘  └──────────┘             │  │
│  └────────────────────────────────────────────────────────┘  │
│                                                              │
│  ┌────────────────────────────────────────────────────────┐  │
│  │               Memory Layer                             │  │
│  │  ┌──────────┐  ┌──────────┐  ┌──────────────────────┐ │  │
│  │  │ SQLite   │  │ Markdown │  │ Supabase (pgvector)  │ │  │
│  │  │ Local DB │  │ Git-fri- │  │ Cloud-hosted with    │ │  │
│  │  │ + FTS5   │  │ endly    │  │ semantic search      │ │  │
│  │  └──────────┘  └──────────┘  └──────────────────────┘ │  │
│  │                                                        │  │
│  │  ┌──────────────────────────────────────────────────┐  │  │
│  │  │ Self-Evolving Memory Layer                       │  │  │
│  │  │ Consolidation · Pattern Discovery · Decay        │  │  │
│  │  └──────────────────────────────────────────────────┘  │  │
│  └────────────────────────────────────────────────────────┘  │
└──────────────────────────────────────────────────────────────┘
```

---

## How Argentum Extends OpenClaw

[OpenClaw](https://github.com/nickarora/openclaw) handles the core AI agent runtime: model communication, session management, and tool integration. Argentum builds on top of that.

| Layer | OpenClaw | Argentum Extension |
|---|---|---|
| **Runtime** | Node.js agent runtime | Same runtime, wrapped with framework |
| **Config** | Environment variables | YAML config with Zod validation, hot-reload |
| **Features** | Built-in tools | Modular plugin system with lifecycle management |
| **Channels** | Telegram (basic) | Multi-channel: Telegram, Webchat, Mobile, Discord, Slack |
| **Memory** | File-based memory | SQLite, Markdown, Supabase, Self-evolving |
| **Security** | Basic allowlist | Full policy engine, encrypted secrets, audit logging |
| **Deployment** | Manual | Docker, install script, auto-update |

Argentum does not fork OpenClaw. It wraps and extends it as a dependency, so you can still update the underlying runtime.

---

## Plugin System

### Feature Module Interface

Every feature implements the `FeatureModule` interface:

```typescript
interface FeatureModule {
  readonly meta: FeatureMeta;           // name, version, description, dependencies
  init(config, context): Promise<void>; // Called when loaded
  start?(): Promise<void>;             // Called when enabled
  stop?(): Promise<void>;              // Called when disabled (cleanup)
  healthCheck?(): Promise<HealthStatus>; // Periodic health check
}
```

### Lifecycle

```
unloaded -> loading -> active -> disabled
                 \-> error
```

1. **Scan** - `PluginLoader` scans `src/features/` for `index.ts` files
2. **Register** - Each feature module is registered with its metadata
3. **Resolve** - Dependencies are resolved via topological sort (circular deps detected)
4. **Init** - Features are initialized with their YAML config and a shared context
5. **Start** - Feature's `start()` hook is called
6. **Health** - Periodic health checks every 60 seconds
7. **Stop** - Graceful shutdown calls `stop()` in reverse order

### Shared Context

Features receive a shared context on init:

```typescript
interface FeatureContext {
  logger: Logger;                              // Feature-scoped logger
  config: ArgentumConfig;                      // Full config object
  registerHook(event, handler): void;          // Listen to events
  emit(event, data): Promise<void>;            // Emit events to other features
```

This lets features talk to each other without direct imports. Loose coupling.

### Adding a New Feature

1. Create `src/features/my-feature/index.ts`
2. Export a `FeatureModule` as default
3. Add config to `config/default.yaml`:
   ```yaml
   features:
     my-feature:
       enabled: true
       customOption: value
   ```
4. Restart Argentum. The plugin loader picks it up automatically.

---

## Security Model

### Defense in Depth

Argentum's security is layered, inspired by NemoClaw's isolation approach:

```
┌──────────────────────────────────────────┐
│  Layer 1: Allowlist / Denylist           │
│  Pattern-based access control            │
│  (exact, prefix, glob, regex)            │
├──────────────────────────────────────────┤
│  Layer 2: Policy Engine                  │
│  Condition-based rules with priorities   │
│  Rate limiting and quotas                │
├──────────────────────────────────────────┤
│  Layer 3: Encrypted Secrets              │
│  AES-256-GCM, PBKDF2 key derivation     │
│  Secrets decrypted only at runtime       │
├──────────────────────────────────────────┤
│  Layer 4: Audit Logging                  │
│  All security decisions logged           │
│  Searchable, exportable                  │
├──────────────────────────────────────────┤
│  Layer 5: Container Sandbox              │
│  Isolated execution for untrusted code   │
│  Resource limits, network restrictions   │
└──────────────────────────────────────────┘
```

### Allowlist Manager

Controls access through configurable rules with pattern matching. Deny rules always override allow rules regardless of priority.

- **exact** - Exact string match
- **prefix** - Starts-with match
- **glob** - Wildcard pattern (`*.example.com`)
- **regex** - Regular expression

Two modes:
- **permissive** - Default allow, explicit denies block
- **strict** - Default deny, only explicit allows pass

### Policy Engine

YAML-based security policies evaluated at runtime:

- **Conditions** - Match against context fields (user, action, resource, channel)
- **Operators** - equals, not_equals, contains, matches, greater_than, less_than, in
- **Actions** - allow, deny, audit, rate_limit
- **Priorities** - Higher priority rules evaluated first
- **Rate Limits** - Per-key sliding window rate limiting

### Encrypted Secrets

- **Algorithm:** AES-256-GCM with unique IV per secret
- **Key Derivation:** PBKDF2 with 100,000 iterations
- **Storage:** Encrypted JSON file (`data/secrets.enc`)
- **Runtime:** Secrets decrypted only when needed, never stored in plaintext on disk

### Command Sandboxing

When `container-sandbox` feature is enabled:

- Commands run in isolated Docker containers
- Resource limits (CPU, memory, timeout)
- Read-only root filesystem
- Network access configurable per policy
- Temporary filesystem for working data

---

## Memory Architecture

### Storage Backends

Argentum supports pluggable memory backends:

```
┌──────────────────────────────────────────┐
│            Memory Interface              │
│  store() · retrieve() · search() · list()│
└──────────┬───────────┬──────────┬────────┘
           │           │          │
    ┌──────▼──┐  ┌─────▼────┐  ┌─▼────────┐
    │ SQLite  │  │ Markdown │  │ Supabase  │
    │ Local   │  │ Git-     │  │ Cloud +   │
    │ + FTS5  │  │ friendly │  │ pgvector  │
    └─────────┘  └──────────┘  └───────────┘
```

**SQLite** - Default. Fast local storage with FTS5 full-text search. Best for single-user deployments.

**Markdown** - Plain text files organized by date/topic. Git-trackable, human-readable, no dependencies.

**Supabase** - Cloud-hosted PostgreSQL with pgvector for semantic search. Best for multi-device or team use.

### Self-Evolving Memory

An overlay system that runs on top of any backend:

1. **Consolidation** - Merges similar memories (Jaccard similarity >= threshold)
2. **Pattern Discovery** - Identifies recurring themes across memories
3. **Importance Scoring** - Boosts frequently accessed memories, decays unused ones
4. **Pruning** - Removes stale, low-importance memories to stay within size limits

Runs on a configurable interval (default: 1 hour).

---

## Configuration Flow

```
config/default.yaml
        │
        ▼
┌───────────────┐     ┌──────────────┐
│  ConfigManager │────▶│ Zod Schema   │
│  (YAML + Env) │     │ Validation   │
└───────┬───────┘     └──────────────┘
        │
        ▼
  ArgentumConfig (typed)
        │
        ├──▶ PluginLoader  (feature toggles)
        ├──▶ Features      (per-feature config)
        ├──▶ Channels      (channel config)
        ├──▶ Memory        (backend config)
        └──▶ Security      (policy config)
```

Environment variables override YAML values:

| Env Var | YAML Path |
|---|---|
| `AGCLAW_PORT` | `server.port` |
| `AGCLAW_LOG_LEVEL` | `logging.level` |
| `AGCLAW_TELEGRAM_TOKEN` | `channels.telegram.token` |
| `AGCLAW_SUPABASE_URL` | `memory.supabaseUrl` |

---

## Deployment Models

### 1. Single Node (Default)

```
User --> Argentum (Node.js) --> LLM API
              │
              ├── SQLite (local)
              └── Features (in-process)
```

Everything runs in a single Node.js process. Simple, low overhead.

### 2. Docker

```
User --> Argentum Container --> LLM API
              │
              ├── Volume: /app/data
              ├── Volume: /app/memory
              └── Optional: Redis Container
```

Isolated, reproducible, easy to deploy and update.

### 3. Distributed (Planned)

```
         ┌──▶ Worker 1 (heavy tasks)
User ──▶ │
Gateway  ├──▶ Worker 2 (browser automation)
         │
         └──▶ Worker 3 (voice processing)
              │
              └──▶ Shared Redis / Supabase
```

Multiple Argentum instances sharing state via Redis and Supabase. Gateway routes tasks to specialized workers.

---

## Design Principles

1. **Modular by default** - Every feature is opt-in. Ship only what you need.
2. **Security first** - Deny by default in strict mode. Audit everything.
3. **Type-safe** - Zod schemas validate all configuration at startup.
4. **Hot-reload** - Configuration changes apply without restart.
5. **Portable** - Runs on a laptop, a VPS, or in Docker. No cloud dependency required.
6. **Extensible** - Adding a feature means creating one file. The plugin loader does the rest.
