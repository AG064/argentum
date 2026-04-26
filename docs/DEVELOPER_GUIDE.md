# Developer Guide

This guide covers how to work with the AG-Claw codebase: project structure, development workflow, coding standards, testing, and how to extend the framework with new channels, features, and skills.

---

## Table of Contents

1. [Project Structure](#1-project-structure)
2. [Development Setup](#2-development-setup)
3. [Coding Standards](#3-coding-standards)
4. [Testing Guidelines](#4-testing-guidelines)
5. [How to Add a New Feature](#5-how-to-add-a-new-feature)
6. [How to Create a New Channel](#6-how-to-create-a-new-channel)
7. [How to Create a New Skill](#7-how-to-create-a-new-skill)
8. [Release Process](#8-release-process)

---

## 1. Project Structure

```
ag-claw/
├── bin/
│   └── agclaw.js              # CLI entry point (Unix shebang)
├── src/
│   ├── index.ts               # Gateway entry, Agent class, Tool Loop
│   ├── cli.ts                 # CLI implementation (agclaw command)
│   ├── core/                  # Non-feature core modules
│   │   ├── config.ts          # Config loading + Zod validation
│   │   ├── logger.ts          # Pino logger factory
│   │   ├── plugin-loader.ts   # Feature discovery + lifecycle
│   │   └── llm-provider.ts    # LLM abstraction (OpenRouter/Anthropic/OpenAI)
│   ├── channels/             # Channel adapters (protocol → internal msg)
│   │   ├── telegram.ts
│   │   ├── discord.ts
│   │   ├── webchat.ts
│   │   └── mobile.ts
│   ├── features/             # 59 self-contained feature modules
│   │   ├── audit-log/
│   │   │   └── index.ts
│   │   ├── sqlite-memory/
│   │   ├── semantic-search/
│   │   ├── cron-scheduler/
│   │   ├── mesh-workflows/
│   │   └── ... (55 more)
│   ├── memory/               # Memory subsystem
│   │   ├── semantic.ts        # SQLite + embedding store
│   │   └── graph.ts          # Knowledge graph (entities + relations)
│   ├── security/            # Security subsystem
│   │   ├── allowlist.ts
│   │   ├── policy-engine.ts
│   │   └── encrypted-secrets.ts
│   ├── types/                # Shared TypeScript interfaces
│   ├── utils/                # Pure utility functions
│   └── @shared/              # Shared code used across packages
│       ├── @application/     # Application-level shared
│       ├── @core/            # Core shared
│       ├── @infrastructure/  # Infrastructure shared
│       └── @interface/       # Interface definitions
├── tests/                    # Jest test suites
│   ├── unit/
│   ├── integration/
│   └── e2e/
├── scripts/                  # Build and deployment scripts
├── skills/                   # Skill definitions
├── docs/                     # Documentation
├── config/                   # Default config files
├── data/                     # Runtime data (gitignored)
├── dist/                     # Compiled JavaScript (gitignored)
├── docker/                   # Docker Compose for deployment
├── package.json
├── tsconfig.json
├── jest.config.js
├── .eslintrc.js              # ESLint config (TypeScript)
└── .prettierrc               # Prettier config
```

### Key Directories

**`src/core/`** — Framework internals. If you're modifying how features load or how the config system works, start here.

**`src/channels/`** — One file per messaging platform. Each adapter normalizes incoming messages to AG-Claw's internal `Message` format and handles sending responses back.

**`src/features/`** — The bulk of AG-Claw. Each feature is a directory with an `index.ts` that exports a lifecycle object. Features are independent and configurable.

**`src/memory/`** — The multi-layered memory system. `semantic.ts` handles vector-style search over SQLite. `graph.ts` manages the entity knowledge graph.

---

## 2. Development Setup

### Prerequisites

- Node.js 18+
- npm 9+

### Install and Build

```bash
git clone https://github.com/AG064/ag-claw.git
cd ag-claw
npm install
npm run build
```

### Development Workflow

For rapid iteration, use the TypeScript runner with hot reload:

```bash
npm run dev
```

This starts the gateway in watch mode. Any change to `src/` recompiles and restarts automatically.

### Running Tests

```bash
npm test              # All tests
npm run test:unit     # Unit tests only
npm run test:integration  # Integration tests
npm run test:watch    # Watch mode
npm run test:coverage # Coverage report
```

### Code Quality

```bash
npm run lint          # ESLint check
npm run lint:fix      # Auto-fix lint errors
npm run format        # Prettier format all source
npm run typecheck     # TypeScript type check only
```

### Environment Variables

Copy `.env.example` to `.env` and fill in your API keys:

```bash
cp .env.example .env
```

Required for development:
- `OPENROUTER_API_KEY` — or `ANTHROPIC_API_KEY`
- `AGCLAW_PORT` — gateway port (default 18789)

---

## 3. Coding Standards

AG-Claw uses ESLint and Prettier to maintain consistent style. Configuration files:
- ESLint: [`.eslintrc.js`](https://github.com/AG064/ag-claw/blob/main/.eslintrc.js)
- Prettier: [`.prettierrc`](https://github.com/AG064/ag-claw/blob/main/.prettierrc)
- Editor config: [`.editorconfig`](https://github.com/AG064/ag-claw/blob/main/.editorconfig)

### TypeScript Rules

- Strict mode is enabled in `tsconfig.json`
- Prefer `interface` over `type` for object shapes
- Use explicit return types on exported functions
- No `any` — use `unknown` when the type is truly unknown
- Use optional chaining (`?.`) and nullish coalescing (`??`) instead of verbose guards

### Naming Conventions

| Thing | Convention | Example |
|---|---|---|
| Files | kebab-case | `audit-log.ts` |
| Classes | PascalCase | `class AuditLogFeature` |
| Interfaces | PascalCase | `interface ToolDefinition` |
| Functions | camelCase | `function createLogger()` |
| Constants | SCREAMING_SNAKE | `MAX_ITERATIONS` |
| Private class members | `_underscore` | `this._logger` |

### Import Order

ESLint enforces this order automatically:

1. Node.js built-ins (`node:path`, `node:fs`)
2. External packages (`express`, `pino`)
3. Internal packages (`./core/config`, `@/features/audit-log`)
4. Type imports (`import type { Foo }`)
5. Relative imports (`./utils/helper`)

### Feature Module Shape

Every feature in `src/features/<name>/index.ts` must export:

```typescript
export default {
  name: string;           // Feature name (kebab-case)
  version: string;        // Semantic version
  dependencies?: string[]; // Other features this needs
  init?: () => void;      // Called once at startup (sync)
  start?: () => void;    // Called when feature is enabled
  stop?: () => void;      // Cleanup on shutdown
  healthCheck?: () => { ok: boolean; message?: string };
};
```

Example from `audit-log`:

```typescript
export default {
  name: 'audit-log',
  version: '0.0.1',
  init() {
    this.db = new Database(dbPath);
    this.initTables();
  },
  start() {},
  stop() {
    this.db.close();
  },
  healthCheck() {
    return { ok: true };
  },
};
```

---

## 4. Testing Guidelines

AG-Claw uses Jest with TypeScript support via `ts-jest`. Test files live alongside source files or in `tests/` directories.

### Test Structure

```bash
tests/
├── unit/
│   ├── core/
│   │   ├── config.test.ts
│   │   └── logger.test.ts
│   ├── features/
│   │   └── audit-log.test.ts
│   └── utils/
│       └── validation.test.ts
├── integration/
│   ├── chat.test.ts
│   └── memory.test.ts
└── e2e/
    └── full-cycle.test.ts
```

### Writing Unit Tests

Use dependency injection to isolate the unit under test:

```typescript
import { describe, it, expect, vi } from 'vitest';

describe('Config validation', () => {
  it('rejects invalid port numbers', () => {
    const result = validateConfig({ server: { port: 0 } });
    expect(result.success).toBe(false);
    expect(result.error?.path).toContain('server.port');
  });

  it('accepts valid model configuration', () => {
    const result = validateConfig({
      model: {
        provider: 'openrouter',
        defaultModel: 'anthropic/claude-sonnet-4-20250514',
        maxTokens: 8192,
        temperature: 0.7,
        retryAttempts: 3,
      },
    });
    expect(result.success).toBe(true);
  });
});
```

### Mocking

Use `vi.mock()` for external dependencies:

```typescript
vi.mock('better-sqlite3', () => {
  return {
    default: vi.fn().mockImplementation(() => ({
      prepare: vi.fn().mockReturnValue({ run: vi.fn() }),
      close: vi.fn(),
    })),
  };
});
```

### Integration Tests

Integration tests run against real components. Use a test database:

```typescript
describe('Memory store', () => {
  const testDb = new Database(':memory:');

  beforeEach(() => {
    initSchema(testDb);
  });

  afterEach(() => {
    testDb.close();
  });

  it('stores and retrieves a memory entry', async () => {
    const id = await storeMemory(testDb, { content: 'test memory' });
    const result = await searchMemory(testDb, 'test');
    expect(result[0].content).toBe('test memory');
  });
});
```

### Coverage Requirements

The CI pipeline requires:
- **Lines**: 70% minimum
- **Functions**: 70% minimum
- **Branches**: 60% minimum

Run coverage locally:

```bash
npm run test:coverage
```

---

## 5. How to Add a New Feature

Suppose you want to add a `stock-prices` feature that fetches stock data.

### Step 1 — Create the Feature Directory

```bash
mkdir -p src/features/stock-prices
```

### Step 2 — Write the Feature Module

```typescript
// src/features/stock-prices/index.ts
import * as https from 'node:https';

export default {
  name: 'stock-prices',
  version: '0.0.1',
  dependencies: [],  // optional: list required features

  init() {
    // Called once at startup. Use for setup that doesn't need config.
    // Keep this fast — it's called synchronously during boot.
  },

  start() {
    // Called when feature transitions from disabled → enabled.
    // Good for async initialization, starting background jobs.
  },

  stop() {
    // Cleanup. Called when the gateway shuts down or feature is disabled.
    // Close DB connections, stop timers, release resources.
  },

  healthCheck() {
    return { ok: true };
  },
};

// Tool exposed by this feature (convention: feature can export tools)
export interface StockPricesTool {
  fetchPrice: (symbol: string) => Promise<string>;
}

const fetchPrice = (symbol: string): Promise<string> => {
  return new Promise((resolve, reject) => {
    const url = `https://api.example.com/quote/${symbol}`;
    https.get(url, (res) => {
      let data = '';
      res.on('data', (chunk) => (data += chunk));
      res.on('end', () => {
        try {
          const json = JSON.parse(data);
          resolve(`${symbol}: $${json.price}`);
        } catch {
          reject(new Error('Failed to parse response'));
        }
      });
    }).on('error', reject);
  });
};

export const stockPricesTool = {
  name: 'stock_price',
  description: 'Get the current stock price for a given ticker symbol',
  parameters: {
    type: 'object',
    properties: {
      symbol: { type: 'string', description: 'Stock ticker symbol (e.g. AAPL)' },
    },
    required: ['symbol'],
  },
  execute: async ({ symbol }: { symbol: string }) => {
    return fetchPrice(symbol);
  },
};
```

### Step 3 — Register the Tool with the Agent

Modify `src/index.ts` to import and register the tool:

```typescript
import { stockPricesTool } from './features/stock-prices';

// Inside Agent class:
this.registerTool(stockPricesTool);
```

### Step 4 — Enable in Configuration

In `agclaw.json`:

```json
{
  "features": {
    "stock-prices": { "enabled": true }
  }
}
```

### Step 5 — Add Tests

```bash
# tests/unit/features/stock-prices.test.ts
import { describe, it, expect, vi } from 'vitest';

describe('stock-prices feature', () => {
  it('has correct metadata', () => {
    const feature = require('../../../src/features/stock-prices').default;
    expect(feature.name).toBe('stock-prices');
    expect(feature.version).toBe('0.0.1');
  });
});
```

---

## 6. How to Create a New Channel

Channels translate between messaging platform protocols and AG-Claw's internal message format.

### Internal Message Format

```typescript
interface Message {
  id: string;          // Unique message ID
  userId: string;      // Who sent it
  chatId: string;      // Which chat/conversation
  text: string;        // Message content
  timestamp: number;   // Unix timestamp in ms
  channel: string;    // 'telegram' | 'discord' | 'webchat' | ...
  metadata?: Record<string, unknown>;  // Channel-specific data
}
```

### Example: Creating a Slack Channel

### Step 1 — Create the Channel Directory

```bash
mkdir -p src/channels/slack
```

### Step 2 — Implement the Adapter

```typescript
// src/channels/slack/index.ts
import { WebClient } from '@slack/web-api';

interface SlackConfig {
  token: string;
  channelId: string;
}

export class SlackChannel {
  private client: WebClient;
  private config: SlackConfig;

  constructor(config: SlackConfig) {
    this.client = new WebClient(config.token);
    this.config = config;
  }

  // Called by the gateway to send a message
  async send(chatId: string, text: string): Promise<void> {
    await this.client.chat.postMessage({
      channel: chatId,
      text,
    });
  }

  // Called by the gateway to start listening
  async start(handler: (msg: Message) => void): Promise<void> {
    // In a real implementation, you would use Slack's Events API
    // or a Socket Mode connection. This is a simplified example.
    this.client.on('message', async (event) => {
      if (!event.text || event.subtype) return;

      const message: Message = {
        id: event.ts,
        userId: event.user,
        chatId: event.channel,
        text: event.text,
        timestamp: parseInt(event.ts, 10) * 1000,
        channel: 'slack',
        metadata: { threadTs: event.thread_ts },
      };

      handler(message);
    });
  }

  stop(): void {
    // Cleanup: close connections, remove listeners
  }
}
```

### Step 3 — Register with the Plugin Loader

In `src/core/plugin-loader.ts`, add:

```typescript
import { SlackChannel } from '../channels/slack';

case 'slack':
  return new SlackChannel(config);
```

### Step 4 — Configure

In `agclaw.json`:

```json
{
  "channels": {
    "slack": {
      "enabled": true,
      "token": "xoxb-...",
      "channelId": "C0123456789"
    }
  }
}
```

---

## 7. How to Create a New Skill

Skills are reusable capability packs that can be installed via `agclaw skill install`. They live in the `skills/` directory.

### Skill Structure

```
skills/
└── my-skill/
    ├── SKILL.md          # Required: description, usage, examples
    ├── src/
    │   └── index.ts      # Skill code
    ├── scripts/
    │   └── setup.sh      # Optional: setup steps
    └── references/       # Optional: documentation, data files
```

### SKILL.md Template

```markdown
# My Skill

Brief description of what this skill does.

## What It Does

- Capability 1
- Capability 2

## Usage

Describe how to invoke the skill.

## Examples

\`\`\`bash
agclaw skill run my-skill --arg value
\`\`\`

## Installation

\`\`\`bash
agclaw skill install my-skill
\`\`\`
```

### Skill Entry Point

```typescript
// skills/my-skill/src/index.ts
export interface SkillContext {
  logger: any;
  config: any;
  agent: any;
}

export async function execute(
  args: Record<string, unknown>,
  context: SkillContext
): Promise<void> {
  context.logger.info(`Running my-skill with args:`, args);
  // Your skill logic here
}

export const metadata = {
  name: 'my-skill',
  version: '0.0.1',
  description: 'Does something useful',
  author: 'Your Name',
};
```

### Publishing

Skills can be published to any registry. The current implementation supports local filesystem paths and URLs. Future versions will support publishing to a central skill marketplace.

---

## 8. Release Process

### Versioning

AG-Claw uses [Semantic Versioning](https://semver.org/):
- **MAJOR** — Breaking changes to the API or config schema
- **MINOR** — New features, backward-compatible
- **PATCH** — Bug fixes, no feature changes

### Release Steps

#### 1. Update Version

```bash
npm version patch     # patch release
npm version minor     # minor release
npm version major     # major release
```

This updates `version` in `package.json` and creates a git tag.

#### 2. Run Full Quality Checks

```bash
npm run typecheck
npm run lint
npm run format:check
npm run test:ci
npm run build
```

All must pass before tagging.

#### 3. Update CHANGELOG.md

Add entries under `## [unreleased]` with section headers:
- `### Added`, `### Changed`, `### Deprecated`, `### Removed`, `### Fixed`, `### Security`

#### 4. Commit and Tag

```bash
git add -A
git commit -m "Release v0.0.1"
git tag v0.0.1
git push --tags
```

#### 5. Publish to npm (for public packages)

```bash
npm publish --access public
```

#### 6. Create GitHub Release

Use the tag to create a GitHub Release with:
- Release title: `v0.0.1`
- Description from CHANGELOG.md
- Any special upgrade instructions

### Hotfix Procedure

For critical bug fixes between releases:

```bash
git checkout main
git checkout -b hotfix/fix-description
# Make minimal fix
npm version patch
git commit -m "HOTFIX: description"
git push -u origin hotfix/fix-description
# Open PR, review, merge, tag
```

### Pre-Release Checklist

- [ ] All tests pass on CI
- [ ] `CHANGELOG.md` updated
- [ ] Version bumped correctly
- [ ] No `console.log` or debug code left in source
- [ ] Documentation updated if public API changed
- [ ] Docker image builds successfully
- [ ] Migration guide written if schema changed

---

*For questions about contributing, open an issue at [github.com/AG064/ag-claw](https://github.com/AG064/ag-claw).*
