# Tutorial 1: Your First Agent

*Estimated time: 15 minutes*

In this tutorial, you'll build your first AG-Claw agent from scratch, configure it, and have your first conversation with it.

---

## What You'll Learn

- How to initialize an AG-Claw instance
- How to configure the agent's personality and capabilities
- How to connect a communication channel (Telegram)
- How to verify everything is working

---

## Step 1 — Initialize

If you haven't already, clone and install AG-Claw:

```bash
git clone https://github.com/AG064/ag-claw.git
cd ag-claw
npm install
npm link
```

Initialize in a new directory:

```bash
mkdir my-agent && cd my-agent
agclaw init
```

This creates:
- `agclaw.json` — your configuration file
- `data/` — database storage
- `.agclaw/` — runtime state

---

## Step 2 — Set Your API Key

AG-Claw needs an LLM to think. Get an API key from [OpenRouter](https://openrouter.ai/) (recommended — supports many models).

```bash
export OPENROUTER_API_KEY=sk-or-v1-...
```

Add it permanently by adding the line to your shell profile (`~/.zshrc` or `~/.bashrc`):

```bash
echo 'export OPENROUTER_API_KEY=sk-or-v1-...' >> ~/.zshrc
source ~/.zshrc
```

---

## Step 3 — Configure Your Agent

Open `agclaw.json` and customize it:

```json
{
  "name": "My First Agent",
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
    "morning-briefing": { "enabled": false },
    "evening-recap": { "enabled": false },
    "webchat": { "enabled": true }
  },
  "channels": {
    "telegram": { "enabled": false },
    "webchat": { "enabled": true, "port": 3001 }
  }
}
```

Key choices:
- **Name**: Give your agent an identity
- **Model**: `claude-sonnet-4` is fast and capable; try `claude-opus-4` for harder tasks
- **Memory**: Keep `sqlite-memory` and `semantic-search` enabled — they're the brain
- **Channels**: Start with `webchat` for easy testing; add Telegram later

---

## Step 4 — Start and Verify

Start the gateway:

```bash
agclaw gateway start --port 3000
```

Check health:

```bash
curl http://localhost:3000/health
```

You should see:

```json
{
  "status": "ok",
  "version": "0.2.0",
  "features": {
    "total": 59,
    "active": 6,
    "unhealthy": []
  }
}
```

---

## Step 5 — Test via Webchat

Open `http://localhost:3001` in your browser (or send a test message):

```bash
curl -X POST http://localhost:3000/chat \
  -H "Content-Type: application/json" \
  -d '{
    "message": "Hello! What is your name?",
    "userId": "test-user"
  }'
```

The agent should respond with its name. Now try:

```
User: Remember that my favorite programming language is Rust.
Agent: Got it! I've stored that your favorite programming language is Rust.

User: What is my favorite programming language?
Agent: Your favorite programming language is Rust.
```

The memory system is working — the agent remembered what you told it.

---

## Step 6 — Connect Telegram (Optional)

Want to chat via Telegram? Here's how:

1. Open Telegram and start a chat with [@BotFather](https://t.me/BotFather)
2. Send `/newbot` and follow the prompts
3. Copy the token BotFather gives you
4. Update `agclaw.json`:

```json
{
  "channels": {
    "telegram": {
      "enabled": true,
      "token": "7123456789:AAFxxxxxxxxxxxxxxxxxxxxxx"
    }
  }
}
```

5. Restart: `agclaw gateway restart`
6. Open Telegram and find your bot — send `/start`

---

## What Just Happened

Here's what AG-Claw did when your message came in:

```
User message
    │
    ▼
┌─────────────────┐
│ Channel Adapter │  (normalizes Telegram/webchat/etc. into a standard format)
└────────┬────────┘
         │
         ▼
┌─────────────────┐
│  Security Layer │  (rate limit, allowlist check)
└────────┬────────┘
         │
         ▼
┌─────────────────┐
│  Auto-Capture   │  (detected: "Remember that..." → stores as memory)
└────────┬────────┘
         │
         ▼
┌─────────────────┐
│   LLM Provider  │  (sends message + conversation history + tools)
└────────┬────────┘
         │
         ▼
┌─────────────────┐
│  Tool Executor  │  (called memory_store with the preference)
└────────┬────────┘
         │
         ▼
┌─────────────────┐
│   Response      │  (text returned to user via channel)
└─────────────────┘
```

---

## Next Steps

- **[Tutorial 2: Memory Management](./02-memory-management.md)** — Learn how AG-Claw's multi-layered memory works and how to use it effectively
- **[Tutorial 3: Skill Development](./03-skill-development.md)** — Create custom tools that extend your agent's capabilities
- **[Tutorial 4: Deployment](./04-deployment.md)** — Deploy your agent with Docker

---

## Troubleshooting

| Problem | Solution |
|---|---|
| "API key not set" error | Make sure `OPENROUTER_API_KEY` is exported before starting |
| Empty response from agent | Check `agclaw gateway logs` for errors |
| Webchat not loading | Ensure `webchat` is enabled in features and channels |
| Telegram not responding | Verify the token is correct; check `agclaw gateway logs` |
| Port already in use | Change port: `agclaw gateway start --port 4000` |

---

*Questions? Open an issue on [GitHub](https://github.com/AG064/ag-claw/issues).*
