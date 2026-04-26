# Tutorial 1: Your First Agent

In this tutorial, you will get AG-Claw running, configure your first agent, and have a complete conversation. By the end, you'll understand how messages flow through the system and how to customize behavior.

**Estimated time:** 15 minutes  
**Prerequisites:** Node.js 18+, an OpenRouter or Anthropic API key

---

## What You Will Build

A fully configured AG-Claw instance running locally, connected to a language model, with memory persistence and the ability to chat via the REST API or webchat interface.

---

## Step 1 — Install AG-Claw

```bash
git clone https://github.com/AG064/ag-claw.git
cd ag-claw
npm install
```

Link the CLI so you can run `agclaw` from anywhere:

```bash
npm link
```

Verify the installation:

```bash
agclaw --version
# 0.0.2
```

---

## Step 2 — Initialize the Project

```bash
agclaw init
```

This creates:
- `agclaw.json` — your configuration file
- `data/` — directory for SQLite databases and session data
- `backups/` — directory for automated backups

The init command also checks for required API keys and prompts you if they're missing.

---

## Step 3 — Set Your API Key

Create a `.env` file in the project root:

```bash
cat > .env << 'EOF'
OPENROUTER_API_KEY=sk-or-v1-your-key-here
EOF
```

Alternatively, export it directly:

```bash
export OPENROUTER_API_KEY=sk-or-v1-your-key-here
```

If you don't have an OpenRouter key, get one at [openrouter.ai](https://openrouter.ai). The free tier gives you access to several models with reasonable rate limits.

---

## Step 4 — Start the Gateway

```bash
agclaw gateway start
```

You should see:

```
✓ Config loaded (agclaw.json)
✓ 12 features enabled
✓ Gateway listening on :18789
✓ Agent ready
```

Check that it's healthy:

```bash
curl http://localhost:18789/health
```

```json
{
  "success": true,
  "data": {
    "status": "ok",
    "version": "0.0.2",
    "uptime": 3,
    "features": "12/59 active",
    "memory": { "semantic": 0, "knowledge_graph": 0, "sessions": 0 }
  }
}
```

---

## Step 5 — Send Your First Message

Use the `/chat` endpoint:

```bash
curl -X POST http://localhost:18789/chat \
  -H "Content-Type: application/json" \
  -d '{"message": "Hello! What can you do?", "userId": "tutorial-user"}'
```

You should get a response like:

```json
{
  "success": true,
  "data": {
    "reply": "Hello! I'm AG-Claw, a modular AI agent framework...",
    "sessionId": "sess_abc123",
    "model": "anthropic/claude-sonnet-4-20250514",
    "tokens": { "prompt": 87, "completion": 42, "total": 129 },
    "latencyMs": 1342
  }
}
```

The agent responded using Claude via OpenRouter. Your message and the response are now stored in session memory.

---

## Step 6 — Enable Webchat (Optional)

The webchat feature provides a browser-based chat interface. Enable it in `agclaw.json`:

```json
{
  "channels": {
    "webchat": {
      "enabled": true,
      "port": 3000,
      "maxConnections": 50
    }
  }
}
```

Restart the gateway:

```bash
agclaw gateway restart
```

Open `http://localhost:3000` in your browser to chat via the web interface.

---

## Step 7 — Enable Telegram (Optional)

To connect your agent to a Telegram bot:

1. Create a bot via [@BotFather](https://t.me/botfather) on Telegram
2. Copy the token BotFather gives you
3. Add it to `agclaw.json`:

```json
{
  "channels": {
    "telegram": {
      "enabled": true,
      "token": "7123456789:AAFxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx",
      "allowedUsers": []
    }
  }
}
```

4. Restart the gateway and send `/start` to your bot in Telegram

To restrict access to specific users, add their Telegram IDs to `allowedUsers`:

```json
{
  "channels": {
    "telegram": {
      "enabled": true,
      "token": "7123456789:AAF...",
      "allowedUsers": [123456789]
    }
  }
}
```

---

## Step 8 — Customize Your Agent

Change the agent's name and personality:

```bash
agclaw config name "Tutorial Bot"
agclaw config agent.systemPrompt "You are a friendly tutor who explains concepts clearly and gives examples. You always start with a brief overview before diving into details."
```

Hot-reload applies the changes immediately. Send another message:

```bash
curl -X POST http://localhost:18789/chat \
  -H "Content-Type: application/json" \
  -d '{"message": "Explain what a binary search tree is", "userId": "tutorial-user"}'
```

---

## Step 9 — Store a Memory

AG-Claw remembers things across sessions. Store a fact explicitly:

```bash
curl -X POST http://localhost:18789/memory/store \
  -H "Content-Type: application/json" \
  -d '{"content": "My name is Alex and I am learning about AG-Claw", "tags": ["intro", "identity"]}'
```

Search your memory:

```bash
curl "http://localhost:18789/memory/search?q=name%20AG-Claw"
```

Now in a new conversation, the agent can reference this information. Start a fresh session:

```bash
curl -X POST http://localhost:18789/chat \
  -H "Content-Type: application/json" \
  -d '{"message": "Do you know what my name is?", "userId": "new-session-user"}'
```

The agent retrieves the stored fact and answers correctly, even though this is a different `userId`.

---

## Step 10 — Explore Features

List all available features:

```bash
agclaw features list
```

Enable a new feature, like the morning briefing:

```bash
curl -X POST http://localhost:18789/features/morning-briefing \
  -H "Content-Type: application/json" \
  -d '{"enabled": true}'
```

Or via CLI:

```bash
agclaw features enable morning-briefing
```

---

## How It Works

Here's what happened behind the scenes:

1. **Your message** arrived at the HTTP gateway via curl
2. **Security checks** ran (rate limiting, allowlists)
3. **Auto-capture** scanned your message for facts to remember
4. **The Agentic Tool Loop** sent your message to Claude with available tools
5. **Memory** was checked for context about you
6. **A response** came back and was sent to you
7. **The conversation** was stored in the session database

```
You → HTTP POST → Gateway → Security → Agentic Tool Loop → LLM → Memory → You
```

---

## What You Learned

- How to install and initialize AG-Claw
- How to configure an API key
- How to start the gateway and verify health
- How to send messages via the REST API
- How to enable webchat and Telegram channels
- How to customize the agent's personality
- How to store and retrieve memories
- How to enable and disable features

---

## Next Steps

- **[Tutorial 2: Memory Management](./02-memory-management.md)** — Deep dive into AG-Claw's multi-layered memory system, including semantic search, knowledge graphs, and memory compression
- **[User Guide](../USER_GUIDE.md)** — Full reference for all AG-Claw features and configuration options
- **[Developer Guide](../DEVELOPER_GUIDE.md)** — Learn how to extend AG-Claw with custom features and channels
