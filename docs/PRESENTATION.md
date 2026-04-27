# Conference Presentation: Argentum

## Slide Deck Outline

Use this outline to prepare a conference talk or workshop about Argentum. Each section includes timing suggestions and key talking points.

**Target Duration:** 30–45 minutes (adjustable)
**Audience:** Developers, AI enthusiasts, technical decision-makers
**Prerequisites:** Basic understanding of AI agents, Node.js, or chatbot frameworks

---

## Slide 1 — Title Slide

**Title:** Argentum: Modular AI Agent Framework — OpenClaw, Evolved

**Subtitle:** 59 Pluggable Features, Production-Ready Security, Zero Vendor Lock-In

**Presenter:** [Your Name]
**Event:** [Conference Name] | [Date]
**Demo Repo:** github.com/AG064/argentum

---

## Slide 2 — The Problem

**Title:** Why Another Agent Framework?

**Content:**
- OpenClaw is great but... monolithic, limited channels, basic security
- LangGraph/AutoGen are powerful but... you build everything from scratch
- n8n/AutoGPT are hosted — your data leaves your machine
- We wanted: the flexibility of LangGraph with the simplicity of OpenClaw, in production from day one

**Talking Points:**
- Personal anecdote about frustration with existing options
- Data privacy concern: why self-hosted matters
- Real cost of building from scratch (time, complexity)

---

## Slide 3 — What Is Argentum?

**Title:** Argentum at a Glance

**Content:**
- Modular AI agent framework built on OpenClaw
- 59 pluggable features you can toggle on/off
- Self-hosted: your data stays on your machine
- Production-ready security built in
- Channels: Telegram, Discord, Slack, WhatsApp, Email, SMS, Webchat, Mobile Push

**Demo Hook:** "Let me show you what a fresh Argentum install looks like in 60 seconds."

---

## Slide 4 — Live Demo 1: 60-Second Setup

**Title:** From Zero to Talking Agent in 60 Seconds

**Content:**
- Show terminal: clone, install, init, start
- Chat via webchat or Telegram
- Show memory working: "remember X" → "what did I tell you?"
- Show feature list

**Notes:**
- Do this live, not recorded, for credibility
- Have a pre-made Telegram bot token ready
- Backup plan: pre-recorded video if demo fails

---

## Slide 5 — Architecture Overview

**Title:** How Argentum Works

**Content:**
- Gateway + Plugin Loader + Agentic Tool Loop
- Four memory layers: Semantic, Knowledge Graph, Markdown, Self-Evolving
- Security layer: audit log, rate limiting, allowlists, encryption
- Channel adapters: normalize all communication protocols

**Diagram:** (See architecture diagram in README.md)

**Talking Points:**
- Walk through a single message flow: user → channel → security → LLM → tools → memory → response
- Explain why the Agentic Tool Loop matters (iterative tool use)

---

## Slide 6 — The Memory System

**Title:** Argentum Remembers: A Four-Layer Memory Architecture

**Content:**
- Layer 1: Semantic Memory (fast, searchable)
- Layer 2: Knowledge Graph (entities, relationships)
- Layer 3: Markdown Memory (human-readable, git-friendly)
- Layer 4: Self-Evolving Memory (auto-compression, consolidation)

**Demo Hook:** "Let me show you what the agent remembers from our conversation."

**Talking Points:**
- How memory enables long-term context
- Auto-capture: agent automatically stores decisions, lessons, errors
- Why multiple layers beat a single vector store

---

## Slide 7 — Security Built In

**Title:** Security That Doesn't Slow You Down

**Content:**
- AES-256 encrypted secrets
- Immutable audit logging (every tool call traced)
- Per-user rate limiting
- Allowlists/denylists (user ID, chat ID)
- Policy engine (YAML-defined rules)
- Content filtering

**Comparison Table:**

| Feature | OpenClaw | LangGraph | Argentum |
|---|---|---|---|
| Encrypted secrets | ❌ | ❌ | ✅ |
| Audit logging | ❌ | ❌ | ✅ |
| Rate limiting | ❌ | ❌ | ✅ |
| Allowlists | ❌ | ❌ | ✅ |

---

## Slide 8 — 59 Features

**Title:** Everything You Need, Nothing You Don't

**Content (grouped by category):**

**Communication:** Telegram, Discord, Slack, WhatsApp, Email, SMS, Webchat
**Voice:** TTS (ElevenLabs, Google), STT (Whisper), Wake Word
**Memory:** SQLite, Semantic Search, Knowledge Graph, Compression, Self-Evolving
**Automation:** Cron, Mesh Workflows, File Watcher, Webhooks, Browser Automation
**Intelligence:** Morning Briefing, Evening Recap, Smart Recommendations
**Security:** Encrypted Secrets, Audit Log, Rate Limiting, Policy Engine

**Talking Points:**
- "Toggle on what you need, toggle off what you don't"
- "No feature bloat — every feature is independently loadable"

---

## Slide 9 — Live Demo 2: Memory & Tools

**Title:** Teaching the Agent: Memory + Custom Tools

**Content:**
- Store a decision manually
- Show semantic search finding it later
- Show the Git Assistant skill (if built in Tutorial 3)
- Show knowledge graph relationships

**Talking Points:**
- "This is how you build institutional knowledge"
- "The agent gets smarter over time, not just within one session"

---

## Slide 10 — Multi-Agent Pattern

**Title:** When One Agent Isn't Enough

**Content:**
- Coordinator agent + specialist agents
- Shared memory for collaboration
- Parallel task execution
- Example: Coding agent + Research agent + Review agent

**Diagram:**
```
Coordinator → delegates to → [Coding] [Research] [Review]
                    ↓
              Synthesizes results
```

**Talking Points:**
- "Specialization beats generalization for complex tasks"
- "Argentum makes this trivial to set up"

---

## Slide 11 — Deployment Options

**Title:** From Laptop to Production

**Content:**
- **Local:** `npm start` — for development
- **Docker:** `docker compose up` — single server production
- **VPS:** Ubuntu + Docker + Caddy/nginx for HTTPS
- **Cloud:** Any cloud that runs Docker (AWS, GCP, Hetzner, DigitalOcean)

**Key Features:**
- Health monitoring endpoint
- Prometheus metrics
- Automated backups
- Log rotation

---

## Slide 12 — Docker Demo

**Title:** Production Deployment in 30 Seconds

**Content:**
- Show docker-compose.yml
- Run `docker compose up`
- Show health endpoint
- Show logs

**Talking Points:**
- "One command to production"
- "Health checks built in"

---

## Slide 13 — Comparison with Alternatives

**Title:** How Argentum Compares

| Feature | OpenClaw | LangGraph | AutoGen | Argentum |
|---|---|---|---|---|
| Self-hosted | ✅ | ✅ | ✅ | ✅ |
| Features | Fixed | 0 (build yourself) | 0 | **59** |
| Memory | Single store | Bring your own | Bring your own | **4 layers** |
| Channels | Telegram only | 0 | 0 | **8+** |
| Security | Basic | None | None | **Full suite** |
| Setup time | 5 min | Days | Days | **5 min** |

**Talking Points:**
- "Argentum gives you LangGraph's flexibility with OpenClaw's simplicity"
- "59 features you'd spend months building from scratch"

---

## Slide 14 — Migration from OpenClaw

**Title:** Switching from OpenClaw? It's Easy.

**Content:**
- Same configuration format (mostly)
- Same CLI commands (just `argentum` instead of `openclaw`)
- Copy your data files over
- Run `argentum init` and restart

**Timeline:**
- 0–5 min: Install and link
- 5–10 min: Transfer config and data
- 10–15 min: Verify everything works

---

## Slide 15 — Contributing & Roadmap

**Title:** Built in the Open, For Everyone

**Content:**
- Open source (MIT license)
- Based on OpenClaw (MIT), OMEGA Memory, PaperCLIP
- Contributing: fork → PR → review → merge
- 59 features and growing
- Active development by AG064

**Roadmap (suggested topics):**
- Kubernetes Helm chart
- Additional LLM providers
- More channel integrations
- Enterprise SSO

---

## Slide 16 — Get Started

**Title:** Try It Now

**Content:**
```bash
git clone https://github.com/AG064/argentum.git
cd argentum
npm install && npm link
argentum init
argentum gateway start
```

**Resources:**
- Docs: [Argentum GitHub](./docs/)
- Quick Start: [QUICK_START.md](./docs/QUICK_START.md)
- User Guide: [USER_GUIDE.md](./docs/USER_GUIDE.md)
- Issues: [github.com/AG064/argentum/issues](https://github.com/AG064/argentum/issues)

---

## Slide 17 — Q&A

**Title:** Questions?

**Content:**
- Live Q&A
- Show demo again if needed
- Share Discord/Telegram community links

---

## Presenter Notes

### Demo Scripts

**Demo 1 (60-second setup):**
```
Terminal recording or live:
1. git clone https://github.com/AG064/argentum.git
2. cd argentum && npm install
3. npm link
4. argentum init
5. argentum gateway start --port 3000
6. curl http://localhost:3000/health
7. curl -X POST http://localhost:3000/chat -d '{"message": "Hello!", "userId": "demo"}'
```

**Demo 2 (memory):**
```
1. curl -X POST http://localhost:3000/chat -d '{"message": "Remember that my favorite color is blue", "userId": "demo"}'
2. curl -X POST http://localhost:3000/chat -d '{"message": "What is my favorite color?", "userId": "demo"}'
3. argentum memory stats
```

### Timing Budget

| Section | Minutes |
|---|---|
| Title + Problem | 3 |
| What Is Argentum | 2 |
| Demo 1: Setup | 5 |
| Architecture | 4 |
| Memory System | 4 |
| Security | 3 |
| 59 Features | 2 |
| Demo 2: Memory | 5 |
| Multi-Agent | 3 |
| Deployment | 3 |
| Comparison | 2 |
| Migration | 2 |
| Get Started | 2 |
| Q&A | 5 |
| **Total** | **45** |

### Common Questions to Prepare For

**Q: How does this compare to LangGraph?**
A: LangGraph is a framework for building agent graphs — you build everything (memory, tools, channels) from scratch. Argentum gives you 59 pre-built features that work together out of the box.

**Q: Is this production-ready?**
A: Yes. It has Docker deployment, health monitoring, rate limiting, audit logging, and automated backups. People are using it in production today.

**Q: How does memory work at scale?**
A: The self-evolving memory feature automatically compresses old entries. For very large deployments, you can switch to Supabase as a shared memory backend for multi-instance deployments.

**Q: Can I use my own LLM?**
A: Yes. Supports OpenRouter (recommended), Anthropic, and OpenAI. You can also add custom LLM providers.

**Q: What's the pricing?**
A: Argentum itself is free (MIT). You just pay for your LLM API usage (OpenRouter, Anthropic, or OpenAI).
