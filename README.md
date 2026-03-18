# AG-Claw

**Modular AI Agent Framework** — OpenClaw, but with plugins, real security, and 59 features you can toggle on and off.

Self-hosted. No cloud. Your data stays on your machine.

## What's different from OpenClaw?

| | OpenClaw | AG-Claw |
|---|---|---|
| Features | Fixed | 59 modular plugins |
| Security | Basic | AES-256, audit log, rate limiting, allowlists |
| CLI | `openclaw` | `agclaw` — same commands + more |
| Memory | Single store | SQLite, markdown, semantic search, compression |
| Channels | Telegram | Telegram, Discord, Slack, WhatsApp, email, SMS |
| Deployment | Manual | Docker, auto-update, health monitoring |

## Quick Start

```bash
# Clone and install
git clone https://github.com/AG064/ag-claw.git
cd ag-claw
npm install
npm link

# Initialize and start
agclaw init
agclaw gateway start --port 3000
```

## CLI

```
agclaw init                  # Set up in current directory
agclaw gateway start         # Start server (background)
agclaw gateway stop          # Stop server
agclaw gateway restart       # Restart server
agclaw gateway status        # Check if running (PID)
agclaw gateway logs          # View server logs
agclaw tools                 # List all 59 features
agclaw feature <name>        # Show feature details
agclaw config [key] [value]  # Show or set config
agclaw doctor                # Diagnose issues
agclaw connect               # Setup integrations
agclaw agents                # List agents
agclaw sessions              # View sessions
```

Health check endpoint: `curl http://localhost:3000/health`

## Features (59 total)

### Communication
- Telegram, Discord, Slack, WhatsApp, Email (IMAP/SMTP), SMS
- Webchat (WebSocket), Mobile Push

### Voice & Audio
- TTS (ElevenLabs, Google, Azure), STT (Whisper, Google)
- Wake Word Detection

### Memory & Knowledge
- SQLite Memory (namespace-based), Markdown Memory
- Semantic Search, Knowledge Graph, Memory Compression
- Self-Evolving Memory (auto-consolidation), Multimodal Memory
- Supabase Memory (configurable)

### Automation
- Cron Scheduler (with atomic task checkout)
- Mesh Workflows (jsep expression parser)
- File Watcher, Webhooks, Browser Automation
- Container Sandbox, Air-Gapped Mode

### Daily Intelligence
- Morning Briefing, Evening Recap
- Smart Recommendations, Calendar Integration
- Weather Alerts, News Digest

### Collaboration
- Multi-Agent Coordination, Role-Based Access
- Shared Knowledge Base, Group Management
- Goal Decomposition, Life Domains, Skills Library

### Security (NemoClaw-inspired)
- AES-256 Encrypted Secrets, Secure Profile
- Allowlists/Denylists, Rate Limiting
- Audit Logging (immutable, tool-call tracing)
- Content Filtering, Policy Engine

### Creative & Multimodal
- Image Generation (DALL-E, Stable Diffusion, Midjourney)
- Video Processing (FFmpeg), Document Analysis
- Live Canvas, Code Execution Sandbox

### Platform
- API Gateway, Budget Enforcement, Tenant Isolation
- Health Monitoring, Auto-Update
- Docker Deployment

## Configuration

Edit `agclaw.json` (created by `agclaw init`):

```json
{
  "name": "My AG-Claw Instance",
  "server": { "port": 3000 },
  "features": {
    "life-domains": { "enabled": true },
    "sqlite-memory": { "enabled": true },
    "cron-scheduler": { "enabled": true },
    "webchat": { "enabled": false },
    "api-gateway": { "enabled": false }
  }
}
```

Toggle features on/off, restart gateway to apply.

## Project Structure

```
src/
├── cli.ts              # CLI entry point
├── core/               # Config, plugin loader, LLM provider
├── features/           # 59 feature modules
│   └── <feature>/
│       └── index.ts    # FeatureModule interface
├── memory/             # Semantic memory, graph
└── index.ts            # Server entry point
```

Each feature implements `init()`, `start()`, `stop()`, `healthCheck()`.

## Development

```bash
npm install        # Install dependencies
npm run build      # Compile TypeScript
npm test           # Run tests
npm run dev        # Watch mode
```

Adding a feature? See [CONTRIBUTING.md](./CONTRIBUTING.md).

## References

Based on patterns from:
- [OpenClaw](https://github.com/nickarora/openclaw) — core runtime
- [OMEGA Memory](https://github.com/omega-memory/omega-memory) — semantic search, consolidation
- [PaperCLIP](https://github.com/paperclipai/paperclip) — task checkout, budget, org hierarchy

## License

MIT

Based on [OpenClaw](https://github.com/nickarora/openclaw) (MIT).
