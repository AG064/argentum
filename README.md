# Argentum

<p align="center">
  <img src="assets/brand/argentum.png" alt="Argentum icon" width="160">
</p>

Argentum is a local-first AI agent framework for people who want a capable assistant they can own, inspect, and extend. It combines a TypeScript agent runtime, modular features, memory backends, communication channels, and security controls into one self-hosted system.

[![Version](https://img.shields.io/badge/version-v0.0.5-blue.svg?style=flat-square)](https://github.com/AG064/argentum/releases)
[![Node](https://img.shields.io/badge/node-%3E%3D18.0.0-green.svg?style=flat-square)](https://nodejs.org)
[![License](https://img.shields.io/badge/license-MIT-orange.svg?style=flat-square)](./LICENSE)
[![CI](https://img.shields.io/github/actions/workflow/status/AG064/argentum/ci.yml?style=flat-square)](https://github.com/AG064/argentum/actions)
[![Docker](https://img.shields.io/badge/docker-ready-blue?style=flat-square)](https://hub.docker.com/r/ag064/argentum)

## Install First

### Windows Setup

Download the latest Windows desktop setup executable from the release page:

[Argentum_0.0.5_x64-setup.exe](https://github.com/AG064/argentum/releases/latest)

This is the normal GUI app. The setup wizard installs Argentum like a regular Windows program:

- shows the license agreement
- shows the default install location, usually `C:\Program Files\Argentum`
- lets you change the install folder before installation
- adds Argentum to the Windows Start Menu
- adds a desktop shortcut
- offers to launch the Argentum desktop interface when setup completes

After setup, launch Argentum from the Start Menu. The desktop app opens the onboarding flow first, then continues into the main interface.

## Provider Status

Stable providers in v0.0.5:

| Provider | Status | Auth modes | Notes |
| --- | --- | --- | --- |
| ChatGPT / OpenAI | Stable | API key, ChatGPT/Codex browser-account authorization | Live chat, model picker, provider test, and usage/error reporting are wired. |
| MiniMax | Stable | API key | Live chat, MiniMax Token Plan usage checks, reset cadence, and M2.7 best-practice context are wired. |

Testing providers:

Anthropic Claude, Google Gemini, OpenRouter, NVIDIA, Groq, Ollama/local, and Custom endpoint remain available under Testing. They can be selected and tested, but they are not promoted as stable release routes yet.

## Supported OS

| Platform | Supported release target | Release assets | Runtime notes |
| --- | --- | --- | --- |
| Windows | Windows 10/11 x64 | `Argentum_0.0.5_x64-setup.exe`, `Argentum_0.0.5_x64_en-US.msi`, optional `argentum-cli-v0.0.5-win-x64.exe` | Uses Microsoft Edge WebView2. Windows 11 includes it; Windows 10 1803+ usually has it, and the installer can install it if missing. |
| Linux | Ubuntu 22.04+ x64 and compatible desktop distributions with WebKitGTK 4.1 | `Argentum_0.0.5_amd64.AppImage`, `.deb`, `.rpm`, optional `argentum-v0.0.5-linux-x64` | Requires the normal Tauri/WebKitGTK desktop stack. Ubuntu/Debian/Fedora/Arch/openSUSE are the intended targets. |
| macOS | macOS 10.15+ on Intel and Apple Silicon | `Argentum_0.0.5_x64.dmg`, `Argentum_0.0.5_aarch64.dmg`, optional `argentum-v0.0.5-macos-x64` | Uses the system WKWebView. Apple Silicon users should prefer the `aarch64` DMG. |

## Hardware Requirements

Hardware requirements for v0.0.5 are intentionally modest because the desktop app uses native webviews and hosted providers by default.

| Level | Requirement |
| --- | --- |
| Minimum | 64-bit CPU, 4 GB RAM, 500 MB free disk space, working system webview, internet access for hosted providers. |
| Recommended | 4-core CPU, 8 GB RAM, 1 GB free disk space, stable broadband, and an SSD-backed workspace. |
| Gateway | Localhost port `3000` available by default, with provider/network access only when you configure and approve it. |
| Provider usage | ChatGPT/OpenAI usage follows the selected account or API plan. MiniMax Token Plan usage is shown in Diagnostics and reset information is surfaced to the model context. |

### Windows Portable CLI

Use this only when you explicitly want the terminal CLI without the desktop app:

[argentum-cli-v0.0.5-win-x64.exe](https://github.com/AG064/argentum/releases/latest)

```powershell
.\argentum-cli-v0.0.5-win-x64.exe onboard
.\argentum-cli-v0.0.5-win-x64.exe doctor
```

### Linux and macOS Binaries

Download the matching release asset, make it executable, then run onboarding:

```bash
chmod +x argentum-v0.0.5-linux-x64
./argentum-v0.0.5-linux-x64 onboard
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

Double-clicking the installed Windows shortcut opens the Argentum desktop interface. The CLI binaries are terminal tools and are intentionally separate from the GUI installer.

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

| Guide                                          | Use it for                         |
| ---------------------------------------------- | ---------------------------------- |
| [Quick Start](docs/QUICK_START.md)             | Getting running quickly            |
| [User Guide](docs/USER_GUIDE.md)               | Daily operation                    |
| [Developer Guide](docs/DEVELOPER_GUIDE.md)     | Extending Argentum                 |
| [API Reference](docs/API.md)                   | HTTP API and config details        |
| [Release Packaging](docs/RELEASE_PACKAGING.md) | Binary and installer build details |
| [Security](SECURITY.md)                        | Security model and reporting       |
| [Features](docs/FEATURES.md)                   | Feature catalog                    |

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

This builds the Windows desktop app installers through Tauri. For the optional portable CLI, run `npm run package:win:cli`.

## License

MIT. Copyright 2024-2026 AG064. Based on OpenClaw by nickarora.
