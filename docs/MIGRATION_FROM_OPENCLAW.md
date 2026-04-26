# Migration Guide: OpenClaw to AG-Claw

If you're currently running OpenClaw and want to switch to AG-Claw, this guide walks you through the process. The migration is designed to be straightforward, with most OpenClaw configurations being compatible with AG-Claw.

---

## Why Switch to AG-Claw?

| Benefit | What It Means for You |
|---|---|
| **59 modular features** | Enable exactly what you need, disable what you don't |
| **Production-grade security** | AES-256 encryption, audit logging, rate limiting, allowlists built in |
| **Multi-channel** | Use Telegram, Discord, Slack, WhatsApp, email, SMS, Webchat from one instance |
| **Advanced memory** | Semantic search, knowledge graph, self-evolving memory, markdown memory |
| **Docker-first** | Production-ready Docker deployment with health monitoring |
| **Active development** | Regular updates, bug fixes, and new features |
| **Self-hosted** | Your data never leaves your machine |

---

## Prerequisites

Before migrating, ensure you have:

- OpenClaw currently running (any version)
- Node.js 18+ installed
- A backup of your OpenClaw configuration and data
- AG-Claw installed (`git clone https://github.com/AG064/ag-claw.git && cd ag-claw && npm install`)

---

## Step 1 — Back Up OpenClaw

```bash
# Find your OpenClaw data directory
# Common locations:
# - ~/.openclaw/
# - ./data/
# - The directory where you ran `openclaw init`

# Create a backup
cp -r ~/.openclaw ~/openclaw-backup-$(date +%Y%m%d)

# Also backup environment variables
env | grep -i openclaw > ~/openclaw-env-backup.txt
```

Specifically back up:
- `agclaw.json` or `openclaw.json` (your config file)
- Database files (`*.db`)
- Memory files
- Any custom skills or plugins

---

## Step 2 — Map OpenClaw Concepts to AG-Claw

### Configuration Files

| OpenClaw | AG-Claw | Notes |
|---|---|---|
| `openclaw.json` | `agclaw.json` | Same format, same structure |
| `~/.openclaw/` | `~/.ag-claw/` or `./data/` | AG-Claw uses `data/` in the working directory by default |
| `config/default.json` | `config/default.yaml` | AG-Claw uses YAML for defaults |

### Environment Variables

| OpenClaw | AG-Claw |
|---|---|
| `OPENCLAW_PORT` | `AGCLAW_PORT` |
| `OPENCLAW_API_KEY` | `OPENROUTER_API_KEY` or `ANTHROPIC_API_KEY` |
| `TELEGRAM_BOT_TOKEN` | `AGCLAW_TELEGRAM_TOKEN` |
| `LOG_LEVEL` | `AGCLAW_LOG_LEVEL` |

### CLI Commands

| OpenClaw | AG-Claw |
|---|---|
| `openclaw init` | `agclaw init` |
| `openclaw gateway start` | `agclaw gateway start` |
| `openclaw gateway stop` | `agclaw gateway stop` |
| `openclaw config` | `agclaw config` |
| `openclaw tools` | `agclaw tools` |
| `openclaw status` | `agclaw gateway status` |

The CLI commands are nearly identical. If you have shell aliases for OpenClaw, simply replace `openclaw` with `agclaw`.

---

## Step 3 — Convert Your Configuration

Your existing `openclaw.json` should work largely as-is with AG-Claw. However, review these common differences:

### Before (OpenClaw)

```json
{
  "name": "My Bot",
  "server": {
    "port": 3000
  },
  "channels": {
    "telegram": {
      "enabled": true,
      "token": "YOUR_TOKEN"
    }
  }
}
```

### After (AG-Claw)

```json
{
  "name": "My Bot",
  "server": {
    "port": 3000
  },
  "channels": {
    "telegram": {
      "enabled": true,
      "token": "YOUR_TOKEN"
    }
  },
  "features": {
    "sqlite-memory": { "enabled": true },
    "semantic-search": { "enabled": true }
  }
}
```

The key addition in AG-Claw is the `features` section, where you explicitly enable the capabilities you want.

### Feature Mapping

OpenClaw's built-in capabilities map to AG-Claw features:

| OpenClaw Capability | AG-Claw Feature |
|---|---|
| Basic memory | `sqlite-memory` |
| Telegram bot | `telegram` (channel) |
| CLI commands | Built-in (`agclaw` CLI) |
| Plugin system | `skills-library` + 59 features |
| Webchat | `webchat` (channel) |
| Scheduled tasks | `cron-scheduler` |
| File watching | `file-watcher` |
| Webhooks | `webhooks` |

---

## Step 4 — Install and Initialize

```bash
cd ag-claw

# Link the CLI
npm link

# Initialize in a new directory (or use existing)
agclaw init

# Your existing agclaw.json will be detected and used
```

---

## Step 5 — Transfer Your Data

### Database Files

```bash
# Copy OpenClaw databases to AG-Claw data directory
cp ~/openclaw-backup/data/*.db ./data/

# If using SQLite (default), this should work directly
ls ./data/
# Should show: agclaw.db, sessions.db, memory.db, etc.
```

### Memory Files

```bash
# Copy memory files
cp -r ~/openclaw-backup/memory/ ./data/ 2>/dev/null || true
cp -r ~/openclaw-backup/*.md ./data/ 2>/dev/null || true
```

### Environment Variables

Update your shell profile with AG-Claw variable names:

```bash
# Old OpenClaw
export OPENCLAW_API_KEY=sk-or-v1-...

# New AG-Claw
export OPENROUTER_API_KEY=sk-or-v1-...

# Old
export TELEGRAM_BOT_TOKEN=...

# New
export AGCLAW_TELEGRAM_TOKEN=...
```

---

## Step 6 — Verify the Migration

### Run Diagnostics

```bash
agclaw doctor
```

Expected output:
```
✓ Node.js version (v20.x.x)
✓ npm installed
✓ Config file found
✓ Data directory exists
✓ Database initialized
✓ Gateway port available
✓ OPENROUTER_API_KEY configured
✓ Telegram token configured
```

### Start and Test

```bash
# Start the gateway
agclaw gateway start --port 3000

# Check health
curl http://localhost:3000/health

# Test via chat
curl -X POST http://localhost:3000/chat \
  -H "Content-Type: application/json" \
  -d '{"message": "Hello, are you working?", "userId": "migration-test"}'
```

### Check Feature Status

```bash
agclaw tools
```

This lists all 59 available features. Verify your expected features are active.

---

## Step 7 — Migrate Custom Plugins

If you had custom OpenClaw plugins, they need to be converted to AG-Claw skills/features.

### OpenClaw Plugin Pattern

```javascript
// OpenClaw plugin
module.exports = {
  name: 'my-plugin',
  init: async (ctx) => { /* ... */ },
  handleMessage: async (message) => { /* ... */ },
};
```

### AG-Claw Feature Pattern

```typescript
// AG-Claw feature
import { FeatureModule } from '../../core/types';

class MyFeature implements FeatureModule {
  readonly meta = {
    name: 'my-feature',
    version: '0.0.2',
    description: 'My converted plugin',
    dependencies: [],
  };

  async init(config: Record<string, unknown>, context): Promise<void> {
    // Your init logic
  }

  async start(): Promise<void> { /* ... */ }
  async stop(): Promise<void> { /* ... */ }
}

export default new MyFeature();
```

See [Developer Guide](./DEVELOPER_GUIDE.md#how-to-add-a-new-feature) for full details.

---

## Common Issues and Solutions

### "Config file not found"

```bash
# AG-Claw looks for agclaw.json in the current directory
# Copy your config to the new location
cp ~/openclaw-backup/openclaw.json ./agclaw.json
```

### "Database locked" errors

```bash
agclaw gateway stop
rm -f ./data/*.db-shm ./data/*.wal
agclaw gateway start
```

### "Feature not found"

Some OpenClaw features have different names in AG-Claw. Check `agclaw tools` for the exact name and enable it:

```bash
agclaw feature <name> enable
agclaw gateway restart
```

### "Telegram not responding"

```bash
# Verify token is set correctly
echo $AGCLAW_TELEGRAM_TOKEN

# Test the token
curl -s "https://api.telegram.org/bot$AGCLAW_TELEGRAM_TOKEN/getMe"

# If OpenClaw used TELEGRAM_BOT_TOKEN, update to AGCLAW_TELEGRAM_TOKEN
```

### "Memory search returns no results"

If you copied old database files, the schema might differ. AG-Claw will automatically migrate on first start, but if you see issues:

```bash
# Re-index semantic memory
agclaw memory reindex
```

---

## Post-Migration Checklist

- [ ] AG-Claw installed and linked (`agclaw --version` works)
- [ ] Configuration file migrated (`agclaw.json` in place)
- [ ] Database files copied and accessible
- [ ] Environment variables updated to AG-Claw names
- [ ] Gateway starts successfully (`agclaw gateway start`)
- [ ] Health check passes (`curl http://localhost:3000/health`)
- [ ] Telegram connection working (if used)
- [ ] Memory search working (`agclaw memory search test`)
- [ ] Custom plugins converted to AG-Claw features
- [ ] Old OpenClaw instance shut down (free up port 3000)

---

## Removing OpenClaw

After verifying AG-Claw is working correctly:

```bash
# Uninstall OpenClaw globally (if installed)
npm uninstall -g openclaw
# or
yarn global remove openclaw

# Optionally remove old files
rm -rf ~/.openclaw

# Remove old aliases from shell profile
# Edit ~/.zshrc or ~/.bashrc and remove any "alias openclaw=..." lines
source ~/.zshrc  # or source ~/.bashrc
```

---

## Getting Help

If you encounter issues not covered here:

- **GitHub Issues**: [AG064/ag-claw/issues](https://github.com/AG064/ag-claw/issues)
- **Documentation**: See [USER_GUIDE.md](./USER_GUIDE.md)
- **Quick Start**: See [QUICK_START.md](./QUICK_START.md)

---

## What's New After Migration

Once you've migrated, explore these AG-Claw capabilities that weren't available in OpenClaw:

### Enable Powerful Features

```bash
# Semantic search over your conversations
agclaw feature semantic-search enable

# Knowledge graph for entity relationships
agclaw feature knowledge-graph enable

# Morning briefings every day at 8am
agclaw feature morning-briefing enable

# Cron scheduling for automation
agclaw feature cron-scheduler enable

# Discord bot (new channel!)
agclaw feature discord-bot enable
```

### Try Advanced Memory

```bash
# Search memory with natural language
agclaw memory search "what did we decide about the API design"

# View memory statistics
agclaw memory stats

# View knowledge graph
agclaw memory graph stats
```

### Run Docker in Production

```bash
npm run docker:build
npm run docker:up
```

---

*Welcome to AG-Claw. Your agent just got a significant upgrade.*
