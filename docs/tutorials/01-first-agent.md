# Tutorial 1: Creating Your First Agent

Learn how to create and configure your first AI agent with AG-Claw.

## 📚 What You'll Learn

- How to create an agent configuration
- Setting up basic capabilities
- Testing your agent
- Debugging common issues

## 🚀 Prerequisites

- AG-Claw installed ([Quick Start Guide](../QUICK_START.md))
- Basic understanding of JavaScript/TypeScript
- An API key (OpenRouter or Anthropic)

## Step 1 — Create Agent Config

Create a new file `agents/my-first-agent.ts`:

```typescript
import type { AgentConfig } from '@ag-claw/core';

export const myAgent: AgentConfig = {
  name: 'My First Agent',
  model: 'openrouter/anthropic/claude-sonnet-4',
  temperature: 0.7,
  maxTokens: 4096,
  
  // Capabilities
  capabilities: {
    webSearch: true,
    fileAccess: true,
    commandExecution: false, // Disabled for safety
  },
  
  // System prompt
  systemPrompt: `You are a helpful assistant that specializes in {{topic}}.`,
  
  // Memory settings
  memory: {
    type: 'episodic', // Store conversations
    retention: 7, // Days
  },
};

export default myAgent;
```

## Step 2 — Register the Agent

Add to your `agclaw.json`:

```json
{
  "agents": {
    "my-first-agent": {
      "enabled": true,
      "config": "./agents/my-first-agent.ts"
    }
  }
}
```

## Step 3 — Test Your Agent

Run your agent:

```bash
agclaw agent start my-first-agent
```

Send a test message:

```bash
agclaw agent chat my-first-agent --message "Hello!"
```

Expected output:

```
🤖 Agent: Hello! How can I help you today?
```

## Step 4 — Enable Additional Capabilities

Add web search:

```typescript
capabilities: {
  webSearch: {
    provider: 'duckduckgo',
    rateLimit: 10, // requests per minute
  },
}
```

Add file access:

```typescript
capabilities: {
  fileAccess: {
    allowedPaths: ['/home/user/projects'],
    readOnly: false,
  },
}
```

## Step 5 — Debug Issues

Check agent logs:

```bash
agclaw agent logs my-first-agent --tail 50
```

Common issues:

| Issue | Solution |
|-------|----------|
| Agent not responding | Check API key is valid |
| Slow responses | Reduce `maxTokens` |
| Memory errors | Increase context window |

## 🎉 Congratulations!

You've created your first AG-Claw agent. Next steps:

- [Memory Management](../02-memory-management.md) — Learn how agents remember
- [Skill Development](../03-skill-development.md) — Add custom capabilities
- [Deployment](../04-deployment.md) — Deploy to production

## 📞 Need Help?

- 📖 [User Guide](../USER_GUIDE.md)
- 💬 [Telegram Community](https://t.me/agclaw)
- 🐛 [Report a Bug](../../issues/new?template=bug_report.md)
