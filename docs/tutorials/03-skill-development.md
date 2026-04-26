# Tutorial 3: Skill Development

*Estimated time: 30 minutes*

In this tutorial, you'll create a custom skill that adds new capabilities to your agent. Skills are the primary way to extend AG-Claw — they register tools that the agent can call during conversations.

---

## What is a Skill?

A skill is a feature module that registers one or more **tools** with the agent. Tools are functions the agent can call while processing a conversation. When the agent decides a tool would help answer your question, it calls the tool and incorporates the result into its response.

Example built-in tools:
- `web_search` — searches the web
- `memory_search` — searches semantic memory
- `run_command` — executes a shell command
- `read_file` / `write_file` — file operations

---

## Project: Building a "Git Assistant" Skill

We'll build a skill that helps the agent interact with Git repositories. The skill will register tools to:

- `git_status` — show the current git status
- `git_log` — show recent commits
- `git_branch` — list all branches

### Step 1 — Create the Skill Directory

```bash
mkdir -p src/features/git-assistant
```

### Step 2 — Implement the Skill Module

Create `src/features/git-assistant/index.ts`:

```typescript
import { FeatureModule, FeatureMeta, FeatureContext, HealthStatus } from '../../core/types';
import { execSync } from 'child_process';
import { existsSync } from 'fs';

const meta: FeatureMeta = {
  name: 'git-assistant',
  version: '0.0.1',
  description: 'Git repository assistant — status, log, branches, and more',
  dependencies: [],
};

class GitAssistant implements FeatureModule {
  readonly meta = meta;
  private ctx!: FeatureContext;
  private repoPath: string = process.cwd();

  async init(config: Record<string, unknown>, context: FeatureContext): Promise<void> {
    this.ctx = context;
    this.repoPath = (config.repoPath as string) ?? process.cwd();
    this.ctx.logger.info('GitAssistant initialized', { repoPath: this.repoPath });
  }

  async start(): Promise<void> {
    // Verify this is a git repo
    if (!existsSync(`${this.repoPath}/.git`)) {
      this.ctx.logger.warn('Not a git repository', { path: this.repoPath });
    }
    this.ctx.logger.info('GitAssistant started');
  }

  async stop(): Promise<void> {
    this.ctx.logger.info('GitAssistant stopped');
  }

  async healthCheck(): Promise<HealthStatus> {
    const isRepo = existsSync(`${this.repoPath}/.git`);
    return {
      healthy: true,
      message: isRepo ? `Git repo at ${this.repoPath}` : `Not a git repo: ${this.repoPath}`,
    };
  }

  // Tool: git_status
  private async gitStatus(): Promise<string> {
    try {
      const output = execSync('git status --short', {
        cwd: this.repoPath,
        encoding: 'utf-8',
        maxBuffer: 1024 * 1024,
      });
      return output.trim() || 'Working tree clean';
    } catch (err: unknown) {
      const e = err as { message: string };
      return `Git error: ${e.message}`;
    }
  }

  // Tool: git_log
  private async gitLog(limit: number = 10): Promise<string> {
    try {
      const output = execSync(`git log --oneline -n ${limit}`, {
        cwd: this.repoPath,
        encoding: 'utf-8',
        maxBuffer: 1024 * 1024,
      });
      return output.trim();
    } catch (err: unknown) {
      const e = err as { message: string };
      return `Git error: ${e.message}`;
    }
  }

  // Tool: git_branch
  private async gitBranch(): Promise<string> {
    try {
      const output = execSync('git branch -a', {
        cwd: this.repoPath,
        encoding: 'utf-8',
        maxBuffer: 1024 * 1024,
      });
      return output.trim();
    } catch (err: unknown) {
      const e = err as { message: string };
      return `Git error: ${e.message}`;
    }
  }

  // Called by the plugin loader to get tool definitions
  getTools() {
    return [
      {
        name: 'git_status',
        description: 'Show the current git repository status (short format). Returns modified, added, and deleted files.',
        parameters: {},
        execute: async () => this.gitStatus(),
      },
      {
        name: 'git_log',
        description: 'Show recent git commit history.',
        parameters: {
          limit: { type: 'number', description: 'Number of commits to show (default: 10)', required: false },
        },
        execute: async (params) => this.gitLog((params.limit as number) ?? 10),
      },
      {
        name: 'git_branch',
        description: 'List all local and remote git branches. Current branch is marked with an asterisk.',
        parameters: {},
        execute: async () => this.gitBranch(),
      },
    ];
  }
}

export default new GitAssistant();
```

### Step 3 — Register Tools with the Agent

The skill needs to register its tools with the agent. Update your feature's `init()` method to do this:

```typescript
async init(config: Record<string, unknown>, context: FeatureContext): Promise<void> {
  this.ctx = context;

  // Register tools with the agent
  for (const tool of this.getTools()) {
    this.ctx.registerTool(tool);
  }

  this.ctx.logger.info('GitAssistant initialized', { repoPath: this.repoPath });
}
```

The exact API for `registerTool` depends on the FeatureContext interface. If your context doesn't have `registerTool`, the skill can instead emit a hook that the agent listens to:

```typescript
// Alternative: emit a hook
await context.emit('skill:register', {
  name: this.meta.name,
  tools: this.getTools(),
});
```

### Step 4 — Add Configuration

Update `config/default.yaml`:

```yaml
features:
  git-assistant:
    enabled: false
    repoPath: "."   # Path to the git repository
```

### Step 5 — Enable and Test

```bash
# Rebuild after adding the feature
npm run build

# Enable the feature
agclaw feature git-assistant enable

# Restart the gateway
agclaw gateway restart

# Test via chat
curl -X POST http://localhost:3000/chat \
  -H "Content-Type: application/json" \
  -d '{"message": "What is our git status?", "userId": "test"}'
```

Expected response:
```
Based on the git status:
 M README.md
?? docs/
```

---

## Anatomy of a Tool

Each tool in a skill has four parts:

```typescript
{
  name: 'tool_name',           // Unique identifier (used in LLM prompts)
  description: 'What it does', // Description for the LLM (be clear!)
  parameters: {                // JSON Schema for parameters
    type: 'object',
    properties: {
      input: {
        type: 'string',
        description: 'What to process',
        required: true          // Required vs optional
      },
      limit: {
        type: 'number',
        description: 'Max items',
        required: false
      }
    },
    required: ['input']        // List required parameters
  },
  execute: async (params) => {  // The actual implementation
    const input = params.input as string;
    const result = doSomething(input);
    return JSON.stringify(result);  // Must return a string
  }
}
```

### Writing Good Tool Descriptions

The description is critical — it tells the LLM when to use the tool. Be specific:

```typescript
// Bad — too vague
description: 'Search something'

// Good — specific
description: 'Search the web for information. Returns titles and snippets from search results.'

// Good — explains input format
description: 'Search git history. Input should be a grep pattern to match against commit messages.'

// Good — explains output format
description: 'List directory contents. Returns a table with columns: name, size, modified.'
```

### Error Handling in Tools

Tools should return error messages as strings, not throw exceptions:

```typescript
execute: async (params) => {
  try {
    const result = riskyOperation(params.input);
    return JSON.stringify(result);
  } catch (err) {
    // Return error as string — the agent can then explain it to the user
    return `Error: ${err instanceof Error ? err.message : String(err)}`;
  }
}
```

---

## Passing Context to Tools

Tools often need access to more than just their parameters. The skill can store context in `init()`:

```typescript
class MySkill implements FeatureModule {
  private ctx!: FeatureContext;
  private db!: Database;

  async init(config: Record<string, unknown>, context: FeatureContext): Promise<void> {
    this.ctx = context;
    this.db = new Database(config.dbPath as string);
  }

  // Tool can access this.db
  execute: async (params) => {
    const results = this.db.query(params.sql as string);
    return JSON.stringify(results);
  }
}
```

---

## Best Practices for Skills

### 1. Keep Tools Focused

Each tool should do one thing well. Don't create a `doEverything` tool. Instead:

```typescript
// Bad
{ name: 'filesystem', execute: async (params) => { /* does everything */ } }

// Good
{ name: 'read_file', execute: async (params) => { /* reads one file */ } }
{ name: 'write_file', execute: async (params) => { /* writes one file */ } }
{ name: 'list_directory', execute: async (params) => { /* lists one dir */ } }
```

### 2. Validate Parameters

```typescript
execute: async (params) => {
  if (!params.path || typeof params.path !== 'string') {
    return 'Error: path parameter is required and must be a string';
  }
  // Proceed...
}
```

### 3. Limit Output Size

Long outputs can overwhelm the LLM context. Truncate large results:

```typescript
execute: async (params) => {
  const result = hugeOperation();
  if (result.length > 5000) {
    return result.slice(0, 5000) + '\n... (truncated)';
  }
  return result;
}
```

### 4. Log Wisely

Log tool invocations for debugging, but never log parameter values that might contain secrets:

```typescript
execute: async (params) => {
  this.ctx.logger.info('Tool called', { name: 'my_tool' });
  // Don't log: this.ctx.logger.info('Tool called', { apiKey: params.apiKey });

  const result = doWork(params);
  return result;
}
```

### 5. Always Return Strings

The tool executor expects string returns. Convert everything to strings:

```typescript
execute: async (params) => {
  const num = someNumber;
  return String(num);                    // Good
  return JSON.stringify({ value: num }); // Good for structured data
  return num;                            // Bad — must be string
}
```

---

## Publishing Your Skill

Once your skill works, consider sharing it:

1. Add it to the AG-Claw repository via a pull request
2. Document it in the skill library (`skills-library` feature)
3. Write a usage guide

---

## Next Steps

- **[Tutorial 4: Deployment](./04-deployment.md)** — Deploy your agent with Docker
- **[Tutorial 5: Advanced Patterns](./05-advanced-patterns.md)** — Multi-agent coordination, mesh workflows

---

## Troubleshooting

| Problem | Solution |
|---|---|
| Tool not appearing | Rebuild with `npm run build` and restart the gateway |
| Tool not called by agent | Improve the tool description — the LLM needs to understand when to use it |
| Tool always returns error | Check `agclaw gateway logs` for the actual exception |
| Feature won't load | Run `agclaw doctor` to diagnose dependency issues |

---

*Questions? Open an issue on [GitHub](https://github.com/AG064/ag-claw/issues).*
