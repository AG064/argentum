# Argentum

<p align="center">
  <img src="assets/brand/argentum.png" alt="Argentum icon" width="160">
</p>

[![Version](https://img.shields.io/badge/version-v0.0.7-blue.svg?style=flat-square)](https://github.com/AG064/argentum/releases)
[![OpenSSF Best Practices](https://www.bestpractices.dev/projects/12957/badge)](https://www.bestpractices.dev/projects/12957)
[![OpenSSF Baseline](https://www.bestpractices.dev/projects/12957/baseline)](https://www.bestpractices.dev/projects/12957)
[![Node](https://img.shields.io/badge/node-%3E%3D18.0.0-green.svg?style=flat-square)](https://nodejs.org)
[![License](https://img.shields.io/badge/license-MIT-orange.svg?style=flat-square)](./LICENSE)
[![CI](https://img.shields.io/github/actions/workflow/status/AG064/argentum/ci.yml?style=flat-square)](https://github.com/AG064/argentum/actions)
[![Docker](https://img.shields.io/badge/docker-ready-blue?style=flat-square)](https://hub.docker.com/r/ag064/argentum)

Argentum is a local-first AI workspace. It runs on your own machine so your data stays with you. You can chat with AI providers you choose, route conversations through Telegram, Discord, or other channels, keep memory across sessions, and use a full desktop app instead of juggling browser tabs.

## Install

### Quick Start

**Download and run (Windows):**

1. Download `Argentum_0.0.7_x64-setup.exe` from [releases](https://github.com/AG064/argentum/releases/latest)
2. Run the installer, launch from Start Menu
3. Follow onboarding, connect a messaging channel, start chatting

**Build from source (any platform):**

```bash
git clone https://github.com/AG064/argentum.git
cd argentum
npm install && npm run build
```

### Windows Setup

For most people on Windows, use the desktop installer:

[Argentum_0.0.7_x64-setup.exe](https://github.com/AG064/argentum/releases/latest)

The setup wizard behaves like a normal Windows installer:

- shows the license agreement
- shows the default install location, usually `C:\Program Files\Argentum`
- lets you change the install folder before installation
- adds Argentum to the Windows Start Menu
- adds a desktop shortcut
- offers to launch the Argentum desktop interface when setup completes

After setup, launch Argentum from the Start Menu. First launch opens onboarding. After that, Argentum opens directly into the desktop app.

### What Changed In v0.0.7

v0.0.7 is a desktop-focused release. It cleans up the chat surface, makes onboarding harder to get stuck in, adds the Monaspace Krypton font, keeps reasoning output separate from normal replies, and improves Telegram session handling. It also keeps the recent security cleanup from `development`.

This release also introduces the optional Argentum llama.cpp local server path. The Windows setup executable can install the local server binaries when you tick the installer checkbox; otherwise Argentum stays lightweight and the local server can be installed later from inside the app.

## Provider Status

### Stable Providers

| Provider                        | Status | Auth modes                                             | Notes                                                                                                                                  |
| ------------------------------- | ------ | ------------------------------------------------------ | -------------------------------------------------------------------------------------------------------------------------------------- |
| ChatGPT / OpenAI                | Stable | API key, ChatGPT/Codex browser-account authorization   | Live chat, model picker, provider test, and usage/error reporting are wired.                                                           |
| MiniMax                         | Stable | API key                                                | Live chat, MiniMax Token Plan usage checks, reset cadence, and M2.7 best-practice context are wired.                                   |
| Argentum llama.cpp local server | Stable | Local endpoint, optional bundled/in-app binary install | Runs local GGUF models through Argentum's local server controls. The installer checkbox is optional; in-app install is also available. |

### Testing Providers

Anthropic Claude, Google Gemini, OpenRouter, NVIDIA, Groq, external Ollama/local endpoints, and Custom endpoint are still under Testing. They are visible for configuration and provider tests, but they are not stable desktop routes yet.

## Supported OS

| Platform | Current support         | Assets                                                         | Runtime notes                                                                                                                               |
| -------- | ----------------------- | -------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------- |
| Windows  | Windows 10/11 x64       | `Argentum_0.0.7_x64-setup.exe`, `Argentum_0.0.7_x64_en-US.msi` | Uses Microsoft Edge WebView2. Windows 11 includes it; Windows 10 1803+ usually has it, and the installer can install it if missing.         |
| Linux    | Source-supported target | Linux desktop packages are planned for release automation      | Requires the normal Tauri/WebKitGTK desktop stack. Ubuntu 22.04+ and similar Debian/Fedora/Arch/openSUSE desktops are the intended targets. |
| macOS    | Source-supported target | macOS DMGs are planned for release automation                  | Uses the system WKWebView. The intended targets are macOS 10.15+ on Intel and Apple Silicon.                                                |

## Hardware Requirements

Argentum uses the system webview and hosted providers by default, so the desktop app does not need a large local GPU.

| Level          | Requirement                                                                                                                                                             |
| -------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Minimum        | 64-bit CPU, 4 GB RAM, 500 MB free disk space, working system webview, internet access for hosted providers.                                                             |
| Recommended    | 4-core CPU, 8 GB RAM, 1 GB free disk space, stable broadband, and an SSD-backed workspace.                                                                              |
| Gateway        | Localhost port `3000` available by default, with provider/network access only when you configure and approve it.                                                        |
| Provider usage | ChatGPT/OpenAI usage follows the selected account or API plan. MiniMax Token Plan usage is shown in Diagnostics and reset information is surfaced to the model context. |

### Portable CLI

Use the CLI when you want the terminal workflow instead of the desktop app. Windows CLI binaries are release assets when built; Linux and macOS CLI binaries can be built from source until CI packaging is enabled.

```bash
argentum onboard
argentum doctor
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

## What Argentum Includes

The current project includes:

- local configuration and data ownership
- 65+ modular features you can enable as needed
- Telegram session routing, plus Discord, Slack, WhatsApp, email, SMS, webchat, and mobile channel modules
- SQLite, semantic search, knowledge graph, markdown, and hierarchical memory options
- encrypted secrets, allowlists, policy controls, audit logs, and rate limiting
- a desktop shell, a CLI, Docker support, and release packaging scripts

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

MIT. Copyright 2024-2026 AG064
