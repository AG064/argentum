# Argentum

<p align="center">
  <img src="assets/brand/argentum.png" alt="Argentum icon" width="160">
</p>

[![Development version](https://img.shields.io/badge/development-v0.0.9?style=flat-square)](https://github.com/AG064/argentum/tree/development)
[![OpenSSF Best Practices](https://www.bestpractices.dev/projects/12957/badge)](https://www.bestpractices.dev/projects/12957)
[![OpenSSF Baseline](https://www.bestpractices.dev/projects/12957/baseline)](https://www.bestpractices.dev/projects/12957)
[![Node](https://img.shields.io/badge/node-%3E%3D18.0.0-green.svg?style=flat-square)](https://nodejs.org)
[![License](https://img.shields.io/badge/license-MIT-orange.svg?style=flat-square)](./LICENSE)
[![CI](https://img.shields.io/github/actions/workflow/status/AG064/argentum/ci.yml?style=flat-square)](https://github.com/AG064/argentum/actions)
[![Docker](https://img.shields.io/badge/docker-ready-blue?style=flat-square)](https://hub.docker.com/r/ag064/argentum)

<img width="2559" height="1383" alt="image" src="https://github.com/user-attachments/assets/2085e372-4fae-4e11-8a83-471a0c6758a4" />

Argentum is a local-first AI workspace. It runs on your own machine so your data stays with you. You can chat with AI providers you choose, route conversations through Telegram, Discord, or other channels, keep memory across sessions, and use a full desktop app instead of juggling browser tabs.

Website: [ag064.eu](https://ag064.eu) | Bug reports: [GitHub Issues](https://github.com/AG064/argentum/issues) or [report@ag064.eu](mailto:report@ag064.eu) | [Privacy and data handling](docs/PRIVACY.md)

## Install

### Quick Start

**Download and run (Windows):**

1. Download the Windows setup executable from the [latest published release](https://github.com/AG064/argentum/releases/latest)
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

[Download the latest Windows installer](https://github.com/AG064/argentum/releases/latest)

The setup wizard behaves like a normal Windows installer:

- shows the license agreement
- shows the default install location, usually `C:\Program Files\Argentum`
- lets you change the install folder before installation
- adds Argentum to the Windows Start Menu
- adds a desktop shortcut
- offers to launch the Argentum desktop interface when setup completes

After setup, launch Argentum from the Start Menu. First launch opens onboarding. After that, Argentum opens directly into the desktop app.

### v0.0.9 Release

The published v0.0.9 release includes default-deny desktop AI tool/context enforcement, server-side context budgets, Hugging Face GGUF search, bounded local model scan, llama.cpp controls, a safe OpenClaw import/archive flow, help/FAQ feedback, numeric update checks, dependency fixes, startup diagnostics, and release-pipeline corrections. See the [v0.0.9 release notes](https://github.com/AG064/argentum/releases/tag/v0.0.9).

The release also contains the optional Argentum llama.cpp local server path. Installers and portable binaries are produced by release automation and attached to the GitHub release.

## Provider Status

### Release-candidate providers

| Provider                   | Status    | Auth modes                                             | Notes                                                                                                                     |
| -------------------------- | --------- | ------------------------------------------------------ | ------------------------------------------------------------------------------------------------------------------------- |
| ChatGPT / OpenAI           | Candidate | API key, ChatGPT/Codex browser-account authorization   | Live chat, model picker, provider test, and usage/error reporting are wired.                                              |
| MiniMax                    | Candidate | API key                                                | Live chat and optional Token Plan usage checks are wired; credentials/account combinations still require live validation. |
| Argentum-managed llama.cpp | Candidate | Local endpoint, optional bundled/in-app binary install | Runs selected third-party GGUF models; there is no custom Argentum model.                                                 |

### Testing Providers

Anthropic Claude, Google Gemini, OpenRouter, NVIDIA, Groq, external Ollama/LM Studio/local endpoints, and Custom endpoints are selectable under Testing. Adapter availability is not a claim that every credential, model, quota, attachment, and tool combination has been live-tested.

## Supported OS

| Platform | Current support         | Assets                                                    | Runtime notes                                                                                                                               |
| -------- | ----------------------- | --------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------- |
| Windows  | Windows 10/11 x64       | Latest published Windows setup executable and MSI         | Uses Microsoft Edge WebView2. Windows 11 includes it; Windows 10 1803+ usually has it, and the installer can install it if missing.         |
| Linux    | Source-supported target | Linux desktop packages are planned for release automation | Requires the normal Tauri/WebKitGTK desktop stack. Ubuntu 22.04+ and similar Debian/Fedora/Arch/openSUSE desktops are the intended targets. |
| macOS    | Source-supported target | macOS DMGs are planned for release automation             | Uses the system WKWebView. The intended targets are macOS 10.15+ on Intel and Apple Silicon.                                                |

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
- 73 optional feature source modules at the v0.0.9 review; see the status document before relying on one
- Telegram session routing, plus Discord, Slack, WhatsApp, email, SMS, webchat, and mobile channel modules
- SQLite, semantic search, knowledge graph, markdown, and hierarchical memory options
- encrypted-vault support plus desktop workspace `secrets.env`, allowlists, policy controls, audit logs, and rate limiting
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

| Guide                                                            | Use it for                                                      |
| ---------------------------------------------------------------- | --------------------------------------------------------------- |
| [Quick Start](docs/QUICK_START.md)                               | Getting running quickly                                         |
| [User Guide](docs/USER_GUIDE.md)                                 | Daily operation                                                 |
| [Developer Guide](docs/DEVELOPER_GUIDE.md)                       | Extending Argentum                                              |
| [API Reference](docs/API.md)                                     | HTTP API and config details                                     |
| [Release Packaging](docs/RELEASE_PACKAGING.md)                   | Binary and installer build details                              |
| [Security](SECURITY.md)                                          | Security model and reporting                                    |
| [Features](docs/FEATURES.md)                                     | Feature catalog                                                 |
| [AI Providers & Extensions](docs/AI_PROVIDERS_AND_EXTENSIONS.md) | Provider/plugin compatibility and licensing                     |
| [Update Architecture](docs/UPDATE_ARCHITECTURE.md)               | Signed updater plan and current limits                          |
| [FAQ](docs/FAQ.md)                                               | Curatable in-app help answers                                   |
| [Project Size](docs/PROJECT_SIZE.md)                             | Workspace, dependency, artifact, and clean-install measurements |

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
