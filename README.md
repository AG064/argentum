# AG-Claw

<h3 align="center">

**Modular AI Agent Framework** — 59 pluggable features, production-ready security, and zero vendor lock-in.

*OpenClaw, evolved.*

</h3>

<p align="center">

[![Version](https://img.shields.io/badge/version-0.2.0-blue.svg?style=flat-square)](#)
[![Node](https://img.shields.io/badge/node-%3E%3D18.0.0-green.svg?style=flat-square)](#)
[![License](https://img.shields.io/badge/license-MIT-orange.svg?style=flat-square)](./LICENSE)
[![TypeScript](https://img.shields.io/badge/typescript-5.3-blue.svg?style=flat-square)](#)
[![Build](https://img.shields.io/badge/build-typescript-blue.svg?style=flat-square)](#)

[![Telegram](https://img.shields.io/badge/Channel-Telegram-26A5E4?logo=telegram&style=flat-square)](https://t.me/agclaw)
[![GitHub Repo](https://img.shields.io/badge/GitHub-AG064%2Fag--claw-181717?logo=github&style=flat-square)](https://github.com/AG064/ag-claw)

</p>

---

## What Is AG-Claw?

AG-Claw is a self-hosted AI agent framework built on the OpenClaw runtime. It gives you a fully modular system where you toggle on exactly the capabilities you need — communication channels, memory backends, automation tools, security layers, and more.

Your data never leaves your machine. No cloud dependency. No subscription. Just a powerful agent that runs wherever you do.

---

## Why AG-Claw?

| Capability | OpenClaw | AG-Claw |
|---|---|---|
| Features | Fixed, monolithic | **59 modular plugins** |
| Security | Basic token auth | **AES-256, audit log, rate limiting, allowlists, policy engine** |
| Memory | Single SQLite store | **SQLite, markdown, semantic search, knowledge graph, compression, self-evolving** |
| Channels | Telegram only | **Telegram, Discord, Slack, WhatsApp, email, SMS, Webchat, mobile push** |
| Deployment | Manual scripts | **Docker, health monitoring, auto-update** |
| CLI | `openclaw` | **`agclaw` — same commands + more** |

---

## Quick Start

### 3 Steps to Running

```bash
# 1. Clone and install
git clone https://github.com/AG064/ag-claw.git
cd ag-claw
npm install

# 2. Link the CLI globally
npm link

# 3. Initialize and launch
agclaw init
agclaw gateway start --port 3000
```

Your agent is live. Health check it:

```bash
curl http://localhost:3000/health
```

Set your API key and you're fully operational:

```bash
export OPENROUTER_API_KEY=sk-or-...
# or
export ANTHROPIC_API_KEY=sk-ant-...
```

---

## Features at a Glance

### Communication Channels
| | | |
|---|---|---|
| ![Telegram](https://img.icons8.com/color/24/telegram-app--v1.png) Telegram | ![Discord](https://img.icons8.com/color/24/discord-logo.png) Discord | ![Slack](https://img.icons8.com/color/24/slack.png) Slack |
| ![WhatsApp](https://img.icons8.com/color/24/whatsapp.png) WhatsApp | ![Email](https://img.icons8.com/color/24/email.png) Email (IMAP/SMTP) | ![SMS](https://img.icons8.com/color/24/sms.png) SMS |
| ![Web](https://img.icons8.com/color/24/browser--v1.png) Webchat (WebSocket) | ![Mobile](https://img.icons8.com/color/24/iphone.png) Mobile Push | |

### Memory & Knowledge
| | | |
|---|---|---|
| 🧠 SQLite Memory | 📄 Markdown Memory | 🔍 Semantic Search |
| 🕸️ Knowledge Graph | 📦 Memory Compression | 🧬 Self-Evolving Memory |
| 🔎 Multimodal Memory | ☁️ Supabase Cloud Backup | |

### Automation
| | | |
|---|---|---|
| ⏰ Cron Scheduler | 🔀 Mesh Workflows | 👁️ File Watcher |
| 🪝 Webhooks | 🌐 Browser Automation | 📦 Container Sandbox |
| ✈️ Air-Gapped Mode | 🎛️ Task Checkout | |

### Voice & Audio
| | | |
|---|---|---|
| 🔊 TTS (ElevenLabs, Google, Azure) | 🎤 STT (Whisper, Google) | 👋 Wake Word Detection |

### Daily Intelligence
| | | |
|---|---|---|
| 🌅 Morning Briefing | 🌙 Evening Recap | 💡 Smart Recommendations |
| 📅 Calendar Integration | 🌦️ Weather Alerts | 📰 News Digest |

### Collaboration
| | | |
|---|---|---|
| 🤖 Multi-Agent Coordination | 🔐 Role-Based Access | 📚 Shared Knowledge Base |
| 👥 Group Management | 🎯 Goal Decomposition | 🏠 Life Domains |
| 🛠️ Skills Library | | |

### Security
| | | |
|---|---|---|
| 🔒 AES-256 Encrypted Secrets | 🛡️ Secure Profile | 📋 Allowlists/Denylists |
| ⚡ Rate Limiting | 📝 Audit Logging | 🔎 Content Filtering |
| 📜 Policy Engine | | |

### Creative & Multimodal
| | | |
|---|---|---|
| 🎨 Image Generation | 🎬 Video Processing | 📄 Document Analysis |
| 🎨 Live Canvas | 💻 Code Execution Sandbox | |

### Platform
| | | |
|---|---|---|
| 🚪 API Gateway | 💰 Budget Enforcement | 🏢 Tenant Isolation |
| ❤️‍🩹 Health Monitoring | 🔄 Auto-Update | 🐳 Docker Deployment |

---

## Architecture

```
┌─────────────────────────────────────────────────────────────────┐
│                          AG-Claw Gateway                        │
├─────────────────────────────────────────────────────────────────┤
│  ┌─────────────┐  ┌─────────────┐  ┌─────────────────────────┐  │
│  │   Channels  │  │   Features   │  │    Plugin Loader        │  │
│  │  (Telegram, │  │  (59 modules) │  │  (load/enable/health)   │  │
│  │  Discord,   │  │              │  │                         │  │
│  │  Webchat)   │  │              │  │                         │  │
│  └──────┬──────┘  └──────┬──────┘  └────────────┬────────────┘  │
│         │                │                      │               │
│         └────────────────┼──────────────────────┘               │
│                          ▼                                      │
│  ┌────────────────────────────────────────────────────────────┐ │
│  │                     Agentic Tool Loop                      │ │
│  │  ┌──────────────┐    ┌──────────────┐    ┌─────────────┐  │ │
│  │  │  LLM Provider │◄──►│    Tools     │◄──►│   Memory    │  │ │
│  │  │ (OpenRouter, │    │ (registered  │    │ (semantic,  │  │ │
│  │  │  Anthropic)  │    │  by features)│    │  graph)     │  │ │
│  │  └──────────────┘    └──────────────┘    └─────────────┘  │ │
│  └────────────────────────────────────────────────────────────┘ │
│                          ▼                                      │
│  ┌────────────────────────────────────────────────────────────┐ │
│  │                  Security Layer                             │ │
│  │  Audit Log | Rate Limiting | Allowlists | Policy Engine    │ │
│  └────────────────────────────────────────────────────────────┘ │
└─────────────────────────────────────────────────────────────────┘
```

### Core Components

| Component | Location | Purpose |
|---|---|---|
| **CLI** | `src/cli.ts` | `agclaw` command-line interface |
| **Plugin Loader** | `src/core/plugin-loader.ts` | Loads, enables, and manages 59 feature modules |
| **Agent** | `src/index.ts` | Agentic Tool Loop — the core agent logic |
| **LLM Provider** | `src/core/llm-provider.ts` | Abstraction over OpenRouter, Anthropic, OpenAI |
| **Memory** | `src/memory/` | Semantic search, knowledge graph, SQLite stores |
| **Channels** | `src/channels/` | Telegram, Discord, Webchat adapters |
| **Features** | `src/features/` | 59 individual feature modules |
| **Security** | `src/security/` | Policy engine, encrypted secrets, allowlists |

---

## Comparison with Similar Projects

| Project | Type | Memory | Channels | Extensibility | Self-Hosted |
|---|---|---|---|---|---|
| **AG-Claw** | Agent Framework | Semantic, graph, SQLite, self-evolving | 8+ channels | 59 plugins + custom | ✅ Full |
| **OpenClaw** | Agent Framework | Single store | Telegram only | None (monolithic) | ✅ Full |
| **LangGraph** | DAG-based agents | Bring your own | Bring your own | Via code | ✅ Full |
| **AutoGen** | Multi-agent | Bring your own | Chat-based | Via code | ✅ Full |
| **Dify** | LLM app platform | Built-in | Built-in | Plugins | ✅ Full |
| **n8n** | Workflow automation | Workers | 400+ integrations | Nodes | ✅ Full |
| **ChatGPT Plugins** | Agent plugins | Via plugin | ChatGPT | Plugins | ❌ Cloud |
| **CrewAI** | Multi-agent | Bring your own | Via code | Via code | ✅ Full |

AG-Claw sits at the sweet spot: as easy to deploy as OpenClaw, as extensible as LangGraph, with production-grade security and 59 built-in features that would take weeks to implement from scratch.

---

## CLI Reference

```bash
# Core commands
agclaw init                        # Initialize in current directory
agclaw gateway start               # Start gateway (background)
agclaw gateway stop                # Stop gateway
agclaw gateway restart             # Restart gateway
agclaw gateway status              # Check if running (PID)
agclaw gateway logs               # View server logs

# Feature management
agclaw tools                       # List all 59 features
agclaw feature <name>              # Show feature details
agclaw feature <name> enable      # Enable a feature
agclaw feature <name> disable     # Disable a feature

# Agent & memory
agclaw agents                      # List configured agents
agclaw sessions                    # View conversation sessions
agclaw memory search <query>       # Search semantic memory
agclaw memory stats                # Show memory statistics

# Configuration
agclaw config                      # Show full config
agclaw config <key>                # Show specific key
agclaw config <key> <value>        # Set a config value

# Diagnostics
agclaw doctor                      # Diagnose issues
agclaw connect                     # Setup integrations

# Docker
agclaw docker:build               # Build Docker image
agclaw docker:up                  # Start containers
agclaw docker:down                # Stop containers
```

---

## Configuration

After `agclaw init`, edit `agclaw.json`:

```json
{
  "name": "My AG-Claw Instance",
  "server": {
    "port": 3000,
    "host": "0.0.0.0"
  },
  "model": {
    "provider": "openrouter",
    "defaultModel": "anthropic/claude-sonnet-4-20250514"
  },
  "features": {
    "sqlite-memory": { "enabled": true },
    "semantic-search": { "enabled": true },
    "cron-scheduler": { "enabled": true },
    "morning-briefing": { "enabled": true },
    "webchat": { "enabled": false },
    "discord-bot": { "enabled": false }
  },
  "channels": {
    "telegram": {
      "enabled": true,
      "allowedUsers": []
    }
  },
  "security": {
    "auditLog": true,
    "rateLimit": { "windowMs": 60000, "maxRequests": 100 }
  }
}
```

Toggle any feature on or off, then run `agclaw gateway restart` to apply.

---

## Environment Variables

| Variable | Description | Default |
|---|---|---|
| `OPENROUTER_API_KEY` | OpenRouter API key (recommended) | — |
| `ANTHROPIC_API_KEY` | Anthropic API key | — |
| `OPENAI_API_KEY` | OpenAI API key (for Whisper, image gen) | — |
| `AGCLAW_TELEGRAM_TOKEN` | Telegram bot token | — |
| `AGCLAW_FCM_KEY` | Firebase Cloud Messaging key (mobile push) | — |
| `AGCLAW_DB_PATH` | Path to SQLite database | `./data/agclaw.db` |
| `AGCLAW_PORT` | Gateway HTTP port | `18789` |
| `AGCLAW_LOG_LEVEL` | Logging level | `info` |
| `SUPABASE_URL` | Supabase project URL (optional) | — |
| `SUPABASE_KEY` | Supabase anon key (optional) | — |

---

## Documentation

| Guide | Description |
|---|---|
| [Quick Start](./docs/QUICK_START.md) | Up and running in 5 minutes |
| [User Guide](./docs/USER_GUIDE.md) | Everything you need to operate AG-Claw |
| [Developer Guide](./docs/DEVELOPER_GUIDE.md) | Contributing, adding features, testing |
| [API Reference](./docs/API.md) | REST endpoints, WebSocket, config schema |
| [Migration from OpenClaw](./docs/MIGRATION_FROM_OPENCLAW.md) | Switching from OpenClaw |
| [Tutorial: First Agent](./docs/tutorials/01-first-agent.md) | Build your first agent step by step |
| [Tutorial: Memory Management](./docs/tutorials/02-memory-management.md) | Master AG-Claw's memory system |
| [Tutorial: Skill Development](./docs/tutorials/03-skill-development.md) | Create your own skills |
| [Tutorial: Deployment](./docs/tutorials/04-deployment.md) | Docker, VPS, production setup |
| [Tutorial: Advanced Patterns](./docs/tutorials/05-advanced-patterns.md) | Multi-agent, workflows, scaling |

---

## Contributing

We welcome contributions of all sizes. Here's how to get started:

```bash
# 1. Fork and clone
git clone https://github.com/YOUR_USERNAME/ag-claw.git
cd ag-claw

# 2. Install dependencies
npm install

# 3. Build
npm run build

# 4. Run tests
npm test

# 5. Create a feature branch
git checkout -b feature/your-feature-name
```

Read the full [Developer Guide](./docs/DEVELOPER_GUIDE.md) for coding standards, testing guidelines, and the release process.

---

## Security

AG-Claw takes security seriously. If you discover a vulnerability, please report it privately rather than filing a public issue. See our [Security Policy](./config/security-policy.yaml) for details.

Key security features:
- **AES-256 encrypted secrets** — API keys never stored in plain text
- **Audit logging** — every tool call traced and logged immutably
- **Rate limiting** — configurable request throttling per user/channel
- **Allowlists/denylists** — restrict access by user ID or chat ID
- **Content filtering** — sanitize inputs before they reach the agent
- **Policy engine** — define custom security policies in YAML

---

## License

AG-Claw is open source under the **MIT License**.

```
MIT License
Copyright (c) 2024–2026 AG064
Based on OpenClaw (MIT) by nickarora
```

---

<p align="center">

**Built with precision by AG064.**  
Questions? Open an issue on [GitHub](https://github.com/AG064/ag-claw/issues).

</p>
