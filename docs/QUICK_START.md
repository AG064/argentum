# Quick Start Guide

Get Argentum running in under 5 minutes. This guide walks you from installation to your first working agent.

---

## Prerequisites

- **Node.js** 18 or higher (`node --version` to check)
- **npm** 9 or higher (comes with Node.js)
- **Git** (to clone the repository)
- An API key: [OpenRouter](https://openrouter.ai/) (recommended) or [Anthropic](https://anthropic.com/)

---

## Step 1 — Install

```bash
git clone https://github.com/AG064/argentum.git
cd argentum
npm install
```

That's it. Argentum uses only fast, pure-JavaScript dependencies — no native compilation required.

---

## Step 2 — Link the CLI

```bash
npm link
```

This makes the `argentum` command available globally. Verify it:

```bash
argentum --version
```

---

## Step 3 — Initialize and Launch

```bash
argentum init
```

This creates an `argentum.json` config file and a `data/` directory. Then start the gateway:

```bash
argentum gateway start --port 3000
```

Check if it's healthy:

```bash
curl http://localhost:3000/health
```

You should see:

```json
{ "status": "ok", "version": "0.0.4", "features": "12/59 active" }
```

---

## Set Your API Key

The agent needs an LLM API key to think. Set it before chatting:

```bash
# Option A: OpenRouter (recommended — access to many models)
export OPENROUTER_API_KEY=sk-or-v1-...

# Option B: Anthropic directly
export ANTHROPIC_API_KEY=sk-ant-...

# Option C: OpenAI (for Whisper STT, image gen)
export OPENAI_API_KEY=sk-...
```

Restart to pick up the key:

```bash
argentum gateway restart
```

---

## First Conversation

Send a message via Telegram (if configured) or the webchat:

```bash
# Via webchat (enable in argentum.json first)
curl -X POST http://localhost:3000/chat \
  -H "Content-Type: application/json" \
  -d '{"message": "Hello, what can you do?", "userId": "test-user"}'
```

Try asking the agent something:

```
User: Hello
Agent: Hi! I'm Argentum, your AI assistant. I have access to tools that let me search
       the web, read and write files, run commands, and manage my own memory.
       What would you like to do?
```

---

## Basic Configuration

Edit `argentum.json` to customize your setup:

```json
{
  "name": "My Agent",
  "server": {
    "port": 3000
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
    "telegram": { "enabled": false },
    "webchat": { "enabled": true }
  },
  "channels": {
    "telegram": {
      "enabled": false,
      "allowedUsers": []
    }
  }
}
```

Key configuration sections:

| Section | Purpose |
|---|---|
| `server.port` | HTTP port for the gateway |
| `model.provider` | `openrouter`, `anthropic`, or `openai` |
| `model.defaultModel` | Which model to use |
| `features` | Toggle 59 features on/off |
| `channels` | Configure communication channels |
| `security` | Rate limits, allowlists, audit logging |

After editing, restart: `argentum gateway restart`

---

## Enable Telegram (Optional)

1. Create a bot via [@BotFather](https://t.me/BotFather) on Telegram
2. Copy your bot token
3. Add it to `argentum.json`:

```json
{
  "channels": {
    "telegram": {
      "enabled": true,
      "token": "YOUR_BOT_TOKEN_HERE",
      "allowedUsers": []
    }
  }
}
```

4. Restart: `argentum gateway restart`
5. Open Telegram, find your bot, and send `/start`

---

## Verify Your Setup

Run the built-in diagnostic:

```bash
argentum doctor
```

Expected output:

```
✓ Node.js version (v20.x.x)
✓ npm installed
✓ Config file found
✓ Data directory exists
✓ Database initialized
✓ Gateway port available
⚠  OPENROUTER_API_KEY not set (features will be limited)
✓ Telegram token configured (channel ready)
```

---

## Troubleshooting Common Issues

### "Gateway failed to start: port already in use"

```bash
# Find and kill the process using the port
lsof -ti:3000 | xargs kill -9
# Or use a different port
argentum gateway start --port 4000
```

### "LLM call failed: API key not set"

```bash
# Verify your key is set
echo $OPENROUTER_API_KEY
# If empty, set it again and restart
export OPENROUTER_API_KEY=sk-or-v1-...
argentum gateway restart
```

### "Feature failed to load: dependency not enabled"

Some features require others. Check dependencies:

```bash
argentum feature <feature-name>
# Example: argentum feature knowledge-graph
```

Enable missing dependencies, then restart.

### "Database locked" errors

```bash
# Stop the gateway first
argentum gateway stop
# Remove the lock file
rm -f ./data/*.db-shm ./data/*.wal
# Restart
argentum gateway start
```

### "Telegram bot not responding"

1. Verify token: `curl -s "https://api.telegram.org/botYOUR_TOKEN/getMe"`
2. Check logs: `argentum gateway logs`
3. Ensure `allowedUsers` is empty (accepts all) or contains your user ID

### "Memory search returns no results"

Memory needs time to accumulate. Try:

```bash
# Store a test memory directly
argentum memory store "test memory" "This is a test entry"
# Then search for it
argentum memory search "test"
```

---

## Next Steps

- Read the [User Guide](./USER_GUIDE.md) for a complete tour of all features
- Follow [Tutorial 1: First Agent](./tutorials/01-first-agent.md) to build your first agent
- Check [Deployment Guide](./tutorials/04-deployment.md) to deploy with Docker
- Browse [all 59 features](./README.md#features-at-a-glance) and enable what you need

---

*Having trouble? Open an issue on [GitHub](https://github.com/AG064/argentum/issues) or ask in the community.*
