# Argentum

<p align="center">
  <img src="assets/brand/argentum.png" alt="Argentum icon" width="160">
</p>

Argentum is a local-first AI agent framework for people who want a capable assistant they can own, inspect, and extend. It combines a TypeScript agent runtime, modular features, memory backends, communication channels, and security controls into one self-hosted system.

[![Version](https://img.shields.io/badge/version-v0.0.3-blue.svg?style=flat-square)](https://github.com/AG064/argentum/releases)
[![Node](https://img.shields.io/badge/node-%3E%3D18.0.0-green.svg?style=flat-square)](https://nodejs.org)
[![License](https://img.shields.io/badge/license-MIT-orange.svg?style=flat-square)](./LICENSE)
[![CI](https://img.shields.io/github/actions/workflow/status/AG064/argentum/ci.yml?style=flat-square)](https://github.com/AG064/argentum/actions)
[![Docker](https://img.shields.io/badge/docker-ready-blue?style=flat-square)](https://hub.docker.com/r/ag064/argentum)

## Install First

### Windows Setup

Download the latest Windows setup executable:

[argentum-v0.0.3-win-x64.exe](https://github.com/AG064/argentum/releases/latest)

The setup wizard installs Argentum like a regular Windows program:

- shows the license agreement
- shows the default install location, usually `C:\Program Files\Argentum`
- lets you change the install folder before installation
- adds Argentum to the Windows Start Menu
- adds a desktop shortcut
- offers to launch Argentum when setup completes

After setup, launch Argentum from the Start Menu or run:

```powershell
argentum onboard
```

Onboarding is the first-run configuration wizard. It creates your workspace, provider settings, enabled features, and local config under your Argentum workspace, normally `%USERPROFILE%\.argentum`.

### Windows Portable CLI

Use this when you want no installer and no Start Menu integration:

[argentum-v0.0.3-win-x64-portable.exe](https://github.com/AG064/argentum/releases/latest)

```powershell
.\argentum-v0.0.3-win-x64-portable.exe onboard
.\argentum-v0.0.3-win-x64-portable.exe doctor
```

### Linux and macOS Binaries

Download the matching release asset, make it executable, then run onboarding:

```bash
chmod +x argentum-v0.0.3-linux-x64
./argentum-v0.0.3-linux-x64 onboard
```

### Docker

```bash
docker run -it ag064/argentum
```

### From Source

```bash
git clone https://github.com/AG064/argentum.git
cd argentum
npm install
npm run onboard
npm run dev
```

## What Argentum Gives You

Argentum is built for practical, self-hosted agent workflows:

- local configuration and data ownership
- 65+ modular features you can enable as needed
- Telegram, Discord, Slack, WhatsApp, email, SMS, webchat, and mobile channels
- SQLite, semantic search, knowledge graph, markdown, and hierarchical memory options
- encrypted secrets, allowlists, policy controls, audit logs, and rate limiting
- Docker and binary release paths for production-style deployment

## Everyday Commands

```bash
argentum onboard                 # Run first-time setup
argentum doctor                  # Check configuration and dependencies
argentum gateway start           # Start the API/web gateway
argentum gateway stop            # Stop the gateway
argentum gateway status          # Show gateway status
argentum tools                   # List available features
argentum feature <name> enable   # Enable a feature
argentum feature <name> disable  # Disable a feature
argentum config                  # Print current config
argentum memory search <query>   # Search memory
argentum help                    # Show all commands
```

Double-clicking the installed Windows shortcut starts the first-run setup if no Argentum configuration exists yet. If setup already exists, it opens diagnostics and shows the next useful commands.

## Architecture

```text
Argentum
|-- Channels
|   |-- Telegram, Discord, Slack, WhatsApp, email, SMS, webchat, mobile
|-- Agent Runtime
|   |-- LLM providers, model routing, tools, sessions
|-- Features
|   |-- automation, media, integrations, skills, workflows
|-- Memory
|   |-- SQLite, semantic search, graph, markdown, hierarchical memory
|-- Security
|   |-- encrypted secrets, allowlists, rate limiting, policy engine, audit logs
```

## Documentation

| Guide | Use it for |
| --- | --- |
| [Quick Start](docs/QUICK_START.md) | Getting running quickly |
| [User Guide](docs/USER_GUIDE.md) | Daily operation |
| [Developer Guide](docs/DEVELOPER_GUIDE.md) | Extending Argentum |
| [API Reference](docs/API.md) | HTTP API and config details |
| [Release Packaging](docs/RELEASE_PACKAGING.md) | Binary and installer build details |
| [Security](SECURITY.md) | Security model and reporting |
| [Features](docs/FEATURES.md) | Feature catalog |

## Development

```bash
npm install
npm run typecheck
npm run lint
npm test
npm run build
```

Build Windows release assets from Windows:

```powershell
npm run package:win
```

This produces the setup executable, MSI, portable Windows CLI, and checksums in `artifacts/release`.

## License

MIT. Copyright 2024-2026 AG064. Based on OpenClaw by nickarora.
