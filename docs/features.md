# Features — All 59 of Them

AG-Claw comes with 59 production-ready features, organized into 7 categories.

![TypeScript](https://img.shields.io/badge/TypeScript-5.3-blue?style=flat-square)
![59 Features](https://img.shields.io/badge/59%20Features-Modular-green?style=flat-square)
![Security First](https://img.shields.io/badge/Security-AES256%20%7C%20Audit%20Log%20%7C%20Sandbox-red?style=flat-square)

---

## 🔐 Security — 8 features

| Badge | Feature | Description |
|-------|---------|-------------|
| 🛡️ secure | `audit-log` | Every sensitive operation logged with timestamps and user context |
| 🛡️ secure | `rate-limiting` | Configurable request throttling per user, channel, and endpoint |
| 🛡️ secure | `allowlists` | Default-deny mode — only explicitly allowed users/commands pass |
| 🛡️ secure | `encrypted-secrets` | AES-256-GCM encryption for all credentials at rest |
| 🛡️ secure | `container-sandbox` | Untrusted agent code runs in isolated containers |
| 🛡️ secure | `policy-engine` | YAML-defined permission rules for tools, channels, and resources |
| 🛡️ secure | `ssrf-protection` | Webhook DNS rebinding blocked — internal endpoints protected |
| 🛡️ secure | `content-filtering` | Input sanitization prevents XSS and injection attacks |

---

## 💬 Channels — 8 features

| Badge | Feature | Description |
|-------|---------|-------------|
| 💬 channels | `telegram` | Telegram bot with commands, inline queries, and conversations |
| 💬 channels | `discord` | Discord bot with slash commands, message intents, and webhooks |
| 💬 channels | `slack` | Slack app with events, commands, and interactive messages |
| 💬 channels | `whatsapp` | WhatsApp Business API integration |
| 💬 channels | `email` | IMAP/SMTP email — read, send, thread with AI |
| 💬 channels | `sms` | SMS via Twilio or similar providers |
| 💬 channels | `webchat` | WebSocket-based chat widget for your website |
| 💬 channels | `mobile-push` | Firebase Cloud Messaging for push notifications |

---

## 🧠 Memory — 7 features

| Badge | Feature | Description |
|-------|---------|-------------|
| 🧠 memory | `sqlite-memory` | Fast SQLite-backed conversation and fact store |
| 🧠 memory | `markdown-memory` | Persistent markdown-based knowledge with frontmatter |
| 🧠 memory | `semantic-search` | Vector embeddings for semantic memory recall |
| 🧠 memory | `knowledge-graph` | Graph-based relationship tracking between entities |
| 🧠 memory | `memory-compression` | Automatic summarization of old conversation history |
| 🧠 memory | `self-evolving-memory` | Agent improves its own memory strategy over time |
| 🧠 memory | `multimodal-memory` | Stores images, audio, and documents alongside text |

---

## 🤖 Agents — 7 features

| Badge | Feature | Description |
|-------|---------|-------------|
| 🤖 ai | `multi-agent-coordination` | Multiple agents working together on complex tasks |
| 🤖 ai | `role-based-access` | Agents and users get scoped permissions by role |
| 🤖 ai | `mesh-workflows` | DAG-based task routing between agents and tools |
| 🤖 ai | `goal-decomposition` | Agent breaks high-level goals into executable sub-tasks |
| 🤖 ai | `shared-knowledge-base` | Agents share context and learned information |
| 🤖 ai | `group-management` | Manage multi-user sessions and team workspaces |
| 🤖 ai | `life-domains` | Organize agent knowledge by life areas (work, health, finance) |

---

## ⚡ Automation — 8 features

| Badge | Feature | Description |
|-------|---------|-------------|
| ⚡ automation | `cron-scheduler` | Time-based task scheduling with cron expressions |
| ⚡ automation | `file-watcher` | Trigger agent actions when files change |
| ⚡ automation | `webhook` | Receive and respond to external HTTP events |
| ⚡ automation | `api-gateway` | Expose agent capabilities via REST API |
| ⚡ automation | `health-monitoring` | Automatic health checks and self-healing |
| ⚡ automation | `auto-update` | Pull and apply updates without manual intervention |
| ⚡ automation | `task-checkout` | Queue-based task distribution with concurrency control |
| ⚡ automation | `docker-deployment` | One-command Docker setup with docker-compose |

---

## 🛠️ Tools — 8 features

| Badge | Feature | Description |
|-------|---------|-------------|
| 🛠️ tools | `mcp` | Model Context Protocol — connect external data sources |
| 🛠️ tools | `skills-library` | Reusable skill modules for common tasks |
| 🛠️ tools | `browser-automation` | Headless browser control for web scraping and testing |
| 🛠️ tools | `code-execution` | Sandboxed code execution for Python, JavaScript, etc. |
| 🛠️ tools | `image-generation` | Generate images via DALL-E, Stable Diffusion, etc. |
| 🛠️ tools | `document-analysis` | Parse PDFs, DOCX, spreadsheets with AI |
| 🛠️ tools | `live-canvas` | Collaborative drawing and visualization canvas |
| 🛠️ tools | `video-processing` | Transcode, clip, and caption videos |

---

## 📱 Intelligence & Voice — 7 features

| Badge | Feature | Description |
|-------|---------|-------------|
| 📱 other | `morning-briefing` | Daily summary of calendar, weather, news at a set time |
| 📱 other | `evening-recap` | End-of-day summary of accomplishments and tomorrow's focus |
| 📱 other | `weather-alerts` | Proactive weather notifications based on location |
| 📱 other | `news-digest` | Curated news summaries from your preferred sources |
| 📱 other | `calendar-integration` | Google Calendar and Cal.com event management |
| 📱 other | `tts` | Text-to-speech via ElevenLabs, Google, or Azure |
| 📱 other | `stt` | Speech-to-text via Whisper or Google Speech |

---

## 🎯 Additional Platform Features — 6 features

| Badge | Feature | Description |
|-------|---------|-------------|
| ⚡ automation | `budget-enforcement` | Set spending limits on API calls and prevent runaway costs |
| ⚡ automation | `tenant-isolation` | Multi-tenant deployment with data isolation |
| 🤖 ai | `voice-wake-word` | Always-listening wake word detection |
| 📱 other | `smart-recommendations` | AI-powered suggestions based on context and history |
| 🛠️ tools | `air-gapped-mode` | Fully offline operation with local model support |
| 🧠 memory | `supabase-backup` | Cloud backup and sync via Supabase |

---

## Quick Enable

After `agclaw init`, edit `agclaw.json`:

```json
{
  "features": {
    "audit-log": { "enabled": true },
    "rate-limiting": { "enabled": true },
    "telegram": { "enabled": true },
    "sqlite-memory": { "enabled": true },
    "semantic-search": { "enabled": true },
    "cron-scheduler": { "enabled": true }
  }
}
```

Run `agclaw gateway restart` to apply changes.

---

_59 features. Zero bloat. Every one production-ready._
