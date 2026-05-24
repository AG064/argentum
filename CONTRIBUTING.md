# Contributing to Argentum

Here's the honest truth upfront: this project is maintained by one person (AG064) in their free time. Contributions are welcome, but response times may vary. That said, every PR gets read and every issue gets considered.

## What Can I Help With?

**Bugs** — If something breaks and you know why, a PR with a fix is the fastest path.

**Features** — Open an issue first to discuss before writing code. Big features might need a redesign, and it's better to find that out before you've written 500 lines.

**Documentation** — Missing something? Wrong? Boring? Fixes here are always appreciated and don't require deep codebase knowledge.

**Translations** — RU and ET are partially done. Other languages welcome.

**Tests** — The test suite exists but coverage is thin. Real-world bug reports with reproduction steps are often more valuable than unit tests.

## Getting Set Up

### Prerequisites

- Node.js 18+
- npm 9+

### Fork and Clone

```bash
# Fork on GitHub, then:
git clone https://github.com/YOUR_USERNAME/argentum.git
cd argentum
npm install
```

### Day-to-Day Commands

```bash
# Type check (do this before pushing, seriously)
npm run typecheck

# Lint (ESLint v9 flat config)
npm run lint

# Format code
npm run format

# Run tests
npm test

# Run specific test suites
npm run test:unit
npm run test:integration
```

### Build

```bash
# Install dependencies
npm install

# Production build (TypeScript compilation + CLI)
npm run build

# Development with watch mode
npm run dev

# Build Docker image
npm run docker:build

# Clean rebuild
npm run rebuild
```

### Desktop App (Optional)

For local llama.cpp server + desktop GUI:

```bash
# Prepare desktop sidecar binaries (llama.cpp server)
npm run prepare:llama-server

# Desktop development
npm run desktop:dev

# Desktop production build
npm run desktop:build
```

### Day-to-Day Commands

```
main          — stable, always releasable
feat/X        — new features
fix/X         — bug fixes
docs/X        — documentation only
```

## Testing

### How to Run Tests Locally

```bash
# Run all tests
npm test

# Run specific test suites
npm run test:unit        # Unit tests only
npm run test:integration # Integration tests only
npm run test:e2e         # End-to-end tests only

# Run tests with coverage report
npm run test:coverage

# Run tests in watch mode
npm run test:watch
```

### What the Tests Cover

| Test Suite | Purpose |
|------------|---------|
| Unit tests | Individual functions and modules (config, memory, security, tools) |
| Integration tests | Channel integrations (Telegram, etc.), API interactions |
| E2E tests | Full workflow from user input to response |
| CLI smoke tests | Binary execution and basic commands |
| GitHub workflow tests | Security configuration validation |

### Interpreting Results

```
Test Suites: 5 passed, 5 total
Tests:       65 passed, 65 total
```

- All tests passing: code changes don't break existing functionality
- Test failures: check error message, fix failing test or code
- Coverage: higher is better, focus on testing critical paths

### CI/CD Pipeline

Tests run automatically on every push to `development` and `main` branches, and on every Pull Request. Results visible in GitHub Actions. CI must pass before merging.

### Test Policy for Major Changes

**What constitutes a major change:**
- New features or capabilities
- Changes to public APIs
- Bug fixes that alter expected behavior
- Security-related changes
- Changes to build or release process

**What tests to add or update:**
- New functionality MUST include unit tests
- Bug fixes MUST include a test that reproduces the bug (regression test)
- API changes MUST update existing tests
- Security changes MUST include relevant security tests

**Policy:**
- PRs with new features that don't add tests will be asked to add them
- Bug fixes without regression tests may be accepted if adding tests is impractical (e.g., one-line fixes)
- Current test coverage is acknowledged as thin — real-world bug reports with reproduction steps are valuable

## Commit Messages

This project uses Conventional Commits (but not strictly enforced):

```
feat: add budget tracking feature
fix: escapeHtml quote handling in security module
docs: clarify Docker setup in README
test: add integration tests for mesh workflow
chore: update typescript-eslint to v8
```

Keep them short and honest. "WIP" commits are fine during review, just squash them before merging.

## Pull Request Checklist

Before requesting review:

- [ ] `npm run typecheck` passes
- [ ] `npm run lint` passes (warnings are OK for existing code, not for new)
- [ ] Tests added/updated if applicable
- [ ] Commit messages are clean
- [ ] PR description explains _what_ and _why_, not just _what changed_

## Things That Won't Get Merged

- PRs with commented-out code left in
- Changes that break the build
- Code that introduces new ESLint errors (not warnings)
- Huge PRs without explanation — split them up

## Architecture Notes

Argentum is built on OpenClaw. The key directories:

```
src/
  core/          — Core agent loop, memory, tools
  channels/      — Telegram, Discord, WhatsApp integrations
  features/      — Pluggable features (toggle in config)
  security/      — Policy engine, credential manager, sandbox
  mcp/           — MCP server implementation
  ui/dashboard/  — Web dashboard

agents/          — Specialized sub-agents (coder, researcher, etc.)
scripts/         — Automation scripts (backup, daily update, etc.)
```

If you're adding a feature, look at `src/features/` for the pattern.

## Questions?

- Open an issue for bugs or feature requests
- GitHub Discussions: https://github.com/AG064/argentum/discussions
- Check the [docs/](docs/) directory first — there's more detail there than in this file

## License

By contributing, you agree your work will be licensed under MIT.
