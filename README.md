<p align="center">
  <h1 align="center">AG-Claw</h1>
  <p align="center"><strong>Modular AI Agent Framework. Self-hosted. Zero subscriptions.</strong></p>
</p>

<p align="center">

[![Version](https://img.shields.io/badge/version-0.4.0-blue.svg?style=flat-square)](https://github.com/AG064/ag-claw/releases)
[![Node](https://img.shields.io/badge/node-%3E%3D20.0.0-green.svg?style=flat-square)](https://nodejs.org)
[![License](https://img.shields.io/badge/license-MIT-orange.svg?style=flat-square)](./LICENSE)
[![TypeScript](https://img.shields.io/badge/typescript-5.3-blue.svg?style=flat-square)](https://www.typescriptlang.org)
[![CI](https://img.shields.io/github/actions/workflow/status/AG064/ag-claw/ci.yml?style=flat-square)](https://github.com/AG064/ag-claw/actions)
[![Build](https://img.shields.io/github/actions/workflow/status/AG064/ag-claw/ci.yml?style=flat-square&label=build)](https://github.com/AG064/ag-claw/actions)
[![Stars](https://img.shields.io/github/stars/AG064/ag-claw?style=flat-square)](https://github.com/AG064/ag-claw/stargazers)
[![Forks](https://img.shields.io/github/forks/AG064/ag-claw?style=flat-square)](https://github.com/AG064/ag-claw/network/members)
[![Telegram](https://img.shields.io/badge/Telegram-26A5E4?style=flat&logo=telegram)](https://t.me/ag_claw)
[![Docker](https://img.shields.io/badge/docker-ready-blue?style=flat-square)](https://github.com/AG064/ag-claw/actions/workflows/ci.yml)

</p>

---

## 📊 Project Statistics

| Metric | Value |
|--------|-------|
| **Features** | 65+ modular plugins |
| **Channels** | 8 (Telegram, Discord, Email, Webchat, WhatsApp, Slack, SMS, Signal) |
| **Memory Backends** | 5 (SQLite, Semantic, Knowledge Graph, Hierarchical, Git Sync) |
| **Lines of Code** | 45,000+ TypeScript |
| **TypeScript Errors** | 0 |
| **Test Coverage** | 15+ security tests |
| **Docker** | ✅ Ready |
| **Self-hosted** | ✅ Full |
| **Subscriptions** | 0 |



---

## Quick Start (30 seconds)

**Option 1 — From GitHub (recommended)**
```bash
git clone https://github.com/AG064/ag-claw.git
cd ag-claw
npm install
npm run dev
```

**Option 2 — Download binary**
```bash
# Download from releases:
# https://github.com/AG064/ag-claw/releases/latest

chmod +x agclaw-linux-x64
./agclaw-linux-x64 --help
```

**Option 3 — From npm (coming soon)**
```bash
npm install -g ag-claw
```

Your AI agent is live. [→ Full documentation](docs/USER_GUIDE.md)

---

## Why AG-Claw?

| Feature | OpenClaw | LangChain | AutoGen | CrewAI | AG-Claw |
|---------|----------|-----------|---------|--------|---------|
| Self-hosted | ⚠️ | ❌ | ❌ | ❌ | ✅ |
| Modular plugins | ❌ | ⚠️ | ❌ | ❌ | ✅ 59 features |
| 8+ channels | ⚠️ | ❌ | ❌ | ❌ | ✅ |
| Security-first | ❌ | ❌ | ❌ | ❌ | ✅ |
| Memory backends | 1 | 1 | 1 | 1 | **5** |
| TypeScript-first | ❌ | ⚠️ | ❌ | ❌ | ✅ |
| Docker ready | ⚠️ | ⚠️ | ⚠️ | ⚠️ | ✅ |
| Audit logging | ❌ | ❌ | ❌ | ❌ | ✅ |

---

![AG-Claw demo](docs/demo.gif)

---

## What Is AG-Claw?

AG-Claw is a self-hosted AI agent framework built on the OpenClaw runtime. It gives you a fully modular system where you toggle on exactly the capabilities you need — communication channels, memory backends, automation tools, security layers, and more.

Your data never leaves your machine. No cloud dependency. No subscription. Just a powerful agent that runs wherever you do.

---

## Remote Access

### Option 1: SSH Tunnel (Recommended)

```bash
ssh -L 3000:localhost:3000 user@your-server
http://localhost:3000
```

### Option 2: Tailscale (VPN)

```bash
tailscale up
tailscale serve https
```

### Option 3: Cloudflare Tunnel (Zero Config)

```bash
./cloudflared tunnel --url http://localhost:3000
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

### Security
| | | |
|---|---|---|
| 🔒 AES-256 Encrypted Secrets | 🛡️ Secure Profile | 📋 Allowlists/Denylists |
| ⚡ Rate Limiting | 📝 Audit Logging | 🔎 Content Filtering |
| 📜 Policy Engine | | |

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

---

## Documentation

| Guide | Description |
|---|---|
| [Quick Start](./docs/QUICK_START.md) | Up and running in 5 minutes |
| [User Guide](./docs/USER_GUIDE.md) | Everything you need to operate AG-Claw |
| [Developer Guide](./docs/DEVELOPER_GUIDE.md) | Contributing, adding features, testing |
| [API Reference](./docs/API.md) | REST endpoints, WebSocket, config schema |
| [Migration from OpenClaw](./docs/MIGRATION_FROM_OPENCLAW.md) | Switching from OpenClaw |
| [Security](./SECURITY.md) | Security features and best practices |
| [Features](./docs/FEATURES.md) | All 59 features documented |

---

## Contributing

```bash
git clone https://github.com/AG064/ag-claw.git
cd ag-claw
npm install
npm run build
npm test
git checkout -b feature/your-feature-name
```

---

## License

MIT License — Copyright (c) 2024–2026 AG064. Based on OpenClaw (MIT) by nickarora.

---

<p align="center">

**Built with precision by AG064.**  
Questions? Open an issue on [GitHub](https://github.com/AG064/ag-claw/issues).

</p>
