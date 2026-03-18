# Contributing to AG-Claw

Thank you for your interest in contributing! AG-Claw is an open-source modular AI agent framework.

## Getting Started

1. Fork the repository
2. Clone your fork: `git clone https://github.com/YOUR_USERNAME/ag-claw.git`
3. Install dependencies: `npm install`
4. Build: `npm run build`
5. Run tests: `npm test`

## Project Structure

```
src/
├── cli.ts              # CLI entry point
├── core/               # Core framework (config, plugin-loader, LLM provider)
├── features/           # Feature modules (each in its own directory)
│   └── <feature>/
│       └── index.ts    # Feature implementation (FeatureModule interface)
├── memory/             # Memory systems (semantic, graph)
└── index.ts            # Main entry point
```

## Adding a New Feature

1. Create `src/features/<your-feature>/index.ts`
2. Implement the `FeatureModule` interface:

```typescript
import { FeatureModule, FeatureMeta, FeatureContext, HealthStatus } from '../../core/types';

class YourFeature implements FeatureModule {
  readonly meta: FeatureMeta = {
    name: 'your-feature',
    version: '0.1.0',
    description: 'What your feature does',
    dependencies: [],  // other features this depends on
  };

  private ctx!: FeatureContext;

  async init(config: Record<string, unknown>, context: FeatureContext): Promise<void> {
    this.ctx = context;
    // Initialize your feature
  }

  async start(): Promise<void> {
    // Start your feature
  }

  async stop(): Promise<void> {
    // Cleanup
  }

  async healthCheck(): Promise<HealthStatus> {
    return { healthy: true, details: {} };
  }
}

export default new YourFeature();
```

3. Add your feature to `README.md` feature table
4. Add tests in `tests/<your-feature>.test.ts`
5. Submit a PR

## Running Locally

```bash
# Build
npm run build

# Run CLI
node dist/cli.js help

# Start gateway
node dist/cli.js gateway start --port 3000

# Run tests
npm test
```

## Code Style

- TypeScript with strict mode
- 2-space indentation
- Semicolons required
- Use `async/await` over raw Promises
- Features should be self-contained (own SQLite tables, own logic)

## Security

- Never log API keys or secrets
- Use parameterized SQL queries (never concatenate user input)
- Validate all external input
- Report security issues privately to security@agclaw.dev

## License

By contributing, you agree that your contributions will be licensed under the MIT License.
