<p align="center">
  <h1 align="center">Argentum</h1>
  <p align="center">Modular AI agent framework. Self-hosted. No subscriptions.</p>
</p>

<p align="center">

[![Version](https://img.shields.io/badge/version-v0.0.2-blue.svg?style=flat-square)](https://github.com/AG064/argentum/releases)
[![Node](https://img.shields.io/badge/node-%3E%3D20.0.0-green.svg?style=flat-square)](https://nodejs.org)
[![License](https://img.shields.io/badge/license-MIT-orange.svg?style=flat-square)](./LICENSE)
[![CI](https://img.shields.io/github/actions/workflow/status/AG064/argentum/ci.yml?style=flat-square)](https://github.com/AG064/argentum/actions)
[![Stars](https://img.shields.io/github/stars/AG064/argentum?style=flat-square)](https://github.com/AG064/argentum/stargazers)
[![Telegram](https://img.shields.io/badge/Telegram-26A5E4?style=flat&logo=telegram)](https://t.me/ag_claw)
[![Docker](https://img.shields.io/badge/docker-ready-blue?style=flat-square)](https://github.com/AG064/argentum/actions/workflows/ci.yml)
[![DockerHub](https://img.shields.io/docker/image-size/ag064/argentum?style=flat-square)](https://hub.docker.com/r/ag064/argentum)

</p>

---

## Get Started in 30 Seconds

```bash
# Clone and run
git clone https://github.com/AG064/argentum.git
cd argentum
npm install
npm run onboard
npm run dev
```

`npm run onboard` launches the interactive setup wizard and writes your first `argentum.json`.

`npm run dev` starts Argentum using the saved configuration. If you skip the wizard, Argentum will boot with defaults and immediately start the server/webchat.

Or use Docker:

```bash
docker run -it ag064/argentum
```

Or grab a binary from the [latest release](https://github.com/AG064/argentum/releases/latest).

---

## What Is This?

Argentum is an AI agent framework built on top of OpenClaw. It gives you a modular system where you enable exactly the capabilities you need — Telegram bots, memory backends, automation tools, security layers, and more.

Your data stays on your machine. No cloud dependency. No subscriptions.

---

## What's Inside

**Communication:** Telegram, Discord, Slack, WhatsApp, Email (IMAP/SMTP), SMS, Webchat, Signal.

**Memory:** SQLite, semantic search, knowledge graph, hierarchical memory, git sync.

**Automation:** Cron scheduler, mesh workflows, file watcher, webhooks, browser automation, container sandbox.

**Security:** Encrypted secrets, rate limiting, allowlists, policy engine, audit logging.

**Tools:** AI image generation with fallback chains, YouTube shorts processing, skill loader, self-improving agent.

---

## Why Argentum

You get 65+ features as plugins. Toggle what you need, ignore the rest. Everything runs locally. No vendor lock-in.

| What you get | Other frameworks |
|---|---|
| 65+ ready features | You build everything from scratch |
| 8 communication channels | Usually one, if any |
| 5 memory backends | Usually one |
| TypeScript throughout | Often JavaScript or wrappers |
| Security-first design | Security as an afterthought |
| Docker ready | Often requires manual setup |

---

## Installation Options

**From source** (recommended for development):
```bash
git clone https://github.com/AG064/argentum.git
cd argentum
npm install
npm run onboard   # optional, but recommended for first-time setup
npm run build
npm start
```

If you want the interactive onboarding wizard without building first, use:

```bash
npm run onboard
```

`npm start` starts the server/runtime, not the setup wizard.

**Docker** (recommended for production):
```bash
docker run -it ag064/argentum
```

**Windows installer or portable binary**:
Download `argentum-v0.0.2-win-x64.exe` for the graphical setup flow, or `argentum-v0.0.2-win-x64.msi` for Windows Installer directly, from [github.com/AG064/argentum/releases/latest](https://github.com/AG064/argentum/releases/latest).

**Portable binary** (no dependencies):
Use `argentum-v0.0.2-win-x64-portable.exe` on Windows when you want the console CLI without installing. For Linux and macOS, pick the matching portable binary.
Download from [github.com/AG064/argentum/releases/latest](https://github.com/AG064/argentum/releases/latest) and pick the one for your OS:
```bash
chmod +x argentum-*
./argentum-* --help
```

Release packaging details live in [docs/RELEASE_PACKAGING.md](docs/RELEASE_PACKAGING.md).

**npm** (coming soon):
```bash
npm install -g argentum
```

---

## CLI Commands

```bash
argentum init                    # Initialize in current directory
argentum onboard                 # Interactive setup wizard
argentum gateway start           # Start gateway
argentum gateway stop            # Stop gateway
argentum gateway status          # Check if running
argentum gateway logs           # View logs

argentum tools                   # List all features
argentum feature <name> enable   # Enable a feature
argentum feature <name> disable # Disable a feature

argentum agents                  # List configured agents
argentum sessions                # View conversation sessions
argentum memory search <query>   # Search memory
argentum memory stats            # Memory statistics

argentum config                  # Show config
argentum config <key>           # Get specific value
argentum config <key> <value>   # Set a value

argentum doctor                  # Diagnose issues
argentum connect                 # Setup integrations
```

---

## Architecture

```
Argentum Gateway
├── Channels (Telegram, Discord, Webchat...)
├── Features (65+ plugins)
│   ├── computer-control
│   ├── image-generation
│   ├── skill-evolution
│   ├── knowledge-graph
│   └── ...59 more
├── Agentic Tool Loop
│   ├── LLM Provider (OpenRouter, Anthropic, Ollama...)
│   ├── Tools (registered by features)
│   └── Memory (semantic, graph, SQLite)
└── Security Layer
    ├── Audit Log
    ├── Rate Limiting
    ├── Allowlists / Denylists
    └── Policy Engine
```

---

## Remote Access

**SSH tunnel** (recommended for local networks):
```bash
ssh -L 3000:localhost:3000 user@your-server
# open http://localhost:3000
```

**Tailscale** (VPN, works anywhere):
```bash
tailscale up
tailscale serve https
```

**Cloudflare Tunnel** (zero config):
```bash
cloudflared tunnel --url http://localhost:3000
```

---

## Documentation

| Guide | What it's for |
|---|---|
| [Quick Start](docs/QUICK_START.md) | Up and running in 5 minutes |
| [User Guide](docs/USER_GUIDE.md) | Operating Argentum day to day |
| [Developer Guide](docs/DEVELOPER_GUIDE.md) | Adding features and contributing |
| [API Reference](docs/API.md) | REST endpoints and config schema |
| [Migration Guide](docs/MIGRATION_FROM_OPENCLAW.md) | Switching from OpenClaw |
| [Security](SECURITY.md) | Security features and best practices |
| [Features](docs/FEATURES.md) | All 65 features documented |

---

## Contributing

```bash
git clone https://github.com/AG064/argentum.git
cd argentum
npm install
npm run build
npm test
git checkout -b feature/your-feature-name
```

---
git clone https://github.com/AG064/argentum.git
cd argentum
npm install
npm run build
npm test
git checkout -b feature/your-feature-name
```

---

## License

MIT. Copyright 2024-2026 AG064. Based on OpenClaw by nickarora.

---
MIT. Copyright 2024-2026 AG064. Based on OpenClaw by nickarora.

---

<p align="center">
Questions? Open an issue on <a href="https://github.com/AG064/argentum/issues">GitHub</a>.
</p>
<p align="center">
Questions? Open an issue on <a href="https://github.com/AG064/argentum/issues">GitHub</a>.
</p>
