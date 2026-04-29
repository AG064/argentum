# Migration Guide: OpenClaw to Argentum

If you're currently running OpenClaw and want to switch to Argentum, this guide walks you through the process. The migration is designed to be straightforward, with most OpenClaw configurations being compatible with Argentum.

---

## Why Switch to Argentum?

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
- Argentum installed (`git clone https://github.com/AG064/argentum.git && cd argentum && npm install`)

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
- `argentum.json` or `openclaw.json` (your config file)
- Database files (`*.db`)
- Memory files
- Any custom skills or plugins

---

## Step 2 — Map OpenClaw Concepts to Argentum

### Configuration Files

| OpenClaw | Argentum | Notes |
|---|---|---|
| `openclaw.json` | `argentum.json` | Same format, same structure |
| `~/.openclaw/` | `~/.argentum/` or `./data/` | Argentum uses `data/` in the working directory by default |
| `config/default.json` | `config/default.yaml` | Argentum uses YAML for defaults |

### Environment Variables

| OpenClaw | Argentum |
|---|---|
| `OPENCLAW_PORT` | `AGCLAW_PORT` |
| `OPENCLAW_API_KEY` | `OPENROUTER_API_KEY` or `ANTHROPIC_API_KEY` |
| `TELEGRAM_BOT_TOKEN` | `AGCLAW_TELEGRAM_TOKEN` |
| `LOG_LEVEL` | `AGCLAW_LOG_LEVEL` |

### CLI Commands

| OpenClaw | Argentum |
|---|---|
| `openclaw init` | `argentum init` |
| `openclaw gateway start` | `argentum gateway start` |
| `openclaw gateway stop` | `argentum gateway stop` |
| `openclaw config` | `argentum config` |
| `openclaw tools` | `argentum tools` |
| `openclaw status` | `argentum gateway status` |

The CLI commands are nearly identical. If you have shell aliases for OpenClaw, simply replace `openclaw` with `argentum`.

---

## Step 3 — Convert Your Configuration

Your existing `openclaw.json` should work largely as-is with Argentum. However, review these common differences:

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

### After (Argentum)

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

The key addition in Argentum is the `features` section, where you explicitly enable the capabilities you want.

### Feature Mapping

OpenClaw's built-in capabilities map to Argentum features:

| OpenClaw Capability | Argentum Feature |
|---|---|
| Basic memory | `sqlite-memory` |
| Telegram bot | `telegram` (channel) |
| CLI commands | Built-in (`argentum` CLI) |
| Plugin system | `skills-library` + 59 features |
| Webchat | `webchat` (channel) |
| Scheduled tasks | `cron-scheduler` |
| File watching | `file-watcher` |
| Webhooks | `webhooks` |

---

## Step 4 — Install and Initialize

```bash
cd argentum

# Link the CLI
npm link

# Initialize in a new directory (or use existing)
argentum init

# Your existing argentum.json will be detected and used
```

---

## Step 5 — Transfer Your Data

### Database Files

```bash
# Copy OpenClaw databases to Argentum data directory
cp ~/openclaw-backup/data/*.db ./data/

# If using SQLite (default), this should work directly
ls ./data/
# Should show: argentum.db, sessions.db, memory.db, etc.
```

### Memory Files

```bash
# Copy memory files
cp -r ~/openclaw-backup/memory/ ./data/ 2>/dev/null || true
cp -r ~/openclaw-backup/*.md ./data/ 2>/dev/null || true
```

### Environment Variables

Update your shell profile with Argentum variable names:

```bash
# Old OpenClaw
export OPENCLAW_API_KEY=sk-or-v1-...

# New Argentum
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
✓ OPENROUTER_API_KEY configured
✓ Telegram token configured
```

### Start and Test

```bash
# Start the gateway
argentum gateway start --port 3000

# Check health
curl http://localhost:3000/health

# Test via chat
curl -X POST http://localhost:3000/chat \
  -H "Content-Type: application/json" \
  -d '{"message": "Hello, are you working?", "userId": "migration-test"}'
```

### Check Feature Status

```bash
argentum tools
```

This lists all 59 available features. Verify your expected features are active.

---

## Step 7 — Migrate Custom Plugins

If you had custom OpenClaw plugins, they need to be converted to Argentum skills/features.

### OpenClaw Plugin Pattern

```javascript
// OpenClaw plugin
module.exports = {
  name: 'my-plugin',
  init: async (ctx) => { /* ... */ },
  handleMessage: async (message) => { /* ... */ },
};
```

### Argentum Feature Pattern

```typescript
// Argentum feature
import { FeatureModule } from '../../core/types';

class MyFeature implements FeatureModule {
  readonly meta = {
    name: 'my-feature',
    version: '0.0.4',
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
# Argentum looks for argentum.json in the current directory
# Copy your config to the new location
cp ~/openclaw-backup/openclaw.json ./argentum.json
```

### "Database locked" errors

```bash
argentum gateway stop
rm -f ./data/*.db-shm ./data/*.wal
argentum gateway start
```

### "Feature not found"

Some OpenClaw features have different names in Argentum. Check `argentum tools` for the exact name and enable it:

```bash
argentum feature <name> enable
argentum gateway restart
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

If you copied old database files, the schema might differ. Argentum will automatically migrate on first start, but if you see issues:

```bash
# Re-index semantic memory
argentum memory reindex
```

---

## Post-Migration Checklist

- [ ] Argentum installed and linked (`argentum --version` works)
- [ ] Configuration file migrated (`argentum.json` in place)
- [ ] Database files copied and accessible
- [ ] Environment variables updated to Argentum names
- [ ] Gateway starts successfully (`argentum gateway start`)
- [ ] Health check passes (`curl http://localhost:3000/health`)
- [ ] Telegram connection working (if used)
- [ ] Memory search working (`argentum memory search test`)
- [ ] Custom plugins converted to Argentum features
- [ ] Old OpenClaw instance shut down (free up port 3000)

---

## Removing OpenClaw

After verifying Argentum is working correctly:

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

- **GitHub Issues**: [AG064/argentum/issues](https://github.com/AG064/argentum/issues)
- **Documentation**: See [USER_GUIDE.md](./USER_GUIDE.md)
- **Quick Start**: See [QUICK_START.md](./QUICK_START.md)

---

## What's New After Migration

Once you've migrated, explore these Argentum capabilities that weren't available in OpenClaw:

### Enable Powerful Features

```bash
# Semantic search over your conversations
argentum feature semantic-search enable

# Knowledge graph for entity relationships
argentum feature knowledge-graph enable

# Morning briefings every day at 8am
argentum feature morning-briefing enable

# Cron scheduling for automation
argentum feature cron-scheduler enable

# Discord bot (new channel!)
argentum feature discord-bot enable
```

### Try Advanced Memory

```bash
# Search memory with natural language
argentum memory search "what did we decide about the API design"

# View memory statistics
argentum memory stats

# View knowledge graph
argentum memory graph stats
```

### Run Docker in Production

```bash
npm run docker:build
npm run docker:up
```

---

*Welcome to Argentum. Your agent just got a significant upgrade.*
