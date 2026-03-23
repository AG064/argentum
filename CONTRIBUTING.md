# Contributing to AG-Claw

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
git clone https://github.com/YOUR_USERNAME/ag-claw.git
cd ag-claw
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
npm run build        # production build
npm run dev          # watch mode with tsx
npm run docker:build # Docker image
```

## Branch Strategy

```
main          — stable, always releasable
feat/X        — new features
fix/X         — bug fixes
docs/X        — documentation only
```

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
- [ ] PR description explains *what* and *why*, not just *what changed*

## Things That Won't Get Merged

- PRs with commented-out code left in
- Changes that break the build
- Code that introduces new ESLint errors (not warnings)
- Huge PRs without explanation — split them up

## Architecture Notes

AG-Claw is built on OpenClaw. The key directories:

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
- Telegram: [@ag_064](https://t.me/ag_064)
- Check the [docs/](docs/) directory first — there's more detail there than in this file

## License

By contributing, you agree your work will be licensed under MIT.
