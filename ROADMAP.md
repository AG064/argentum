# Roadmap

## Current Status

Argentum v0.0.9 is a release candidate on `development`. The latest published release is v0.0.7.

## v0.0.9 — Release Gate

- **In-app OpenClaw migration** — imports allowlisted items from `~/.openclaw/`. Skills merge without overwriting conflicts; other files are archived for review and are not activated automatically.
- **In-app feedback mechanism** — GitHub issue/discussion links include version and workspace-presence metadata without exposing the local workspace path.
- **Help panel doc links** — clicking `?` shows the help panel with a docs link specific to the current section (Chat, Gateway, Settings, etc.).
- **Update mechanism** — GitHub Releases check with numeric semantic-version comparison; the update action opens the releases page in the browser.
- **Security hardening** — `ip-address` override fixed to require `>= 10.1.1` (CVE-2026-42338); `sanitize-html` constraint tightened to `>= 2.17.2` (CVE-2026-40186); SECURITY.md updated.
- **Router Agent decision layer** — YAML and nested JSON rules are validated and decisions are logged before the existing single-agent dispatch path.
- **Knowledge Graph consolidation groundwork** — canonical interface exports and shared type re-export are in place; implementation convergence remains planned.
- **Thinking mode wired to providers** — Anthropic Claude 4 uses manual extended-thinking budgets; Codex and OpenAI use provider-specific reasoning-effort fields.
- **Skills catalog Built-in tab** — lists the four SKILL.md files actually bundled with Argentum and states external dependency requirements.
- **Localization foundation** — English/Estonian catalogs, locale persistence, formatting helpers, and direction metadata are packaged. Translating all visible desktop strings remains planned.
- **New Claude models** — `claude-sonnet-4-20250514` and `claude-opus-4-20250514` added to model catalog with 200k context and frontier reasoning capability.

## v0.1.0 (Next 3-6 months)

- **Hermes → Argentum migration** — full parity import of skills, memories, SOUL.md, config.yaml, `.env` secrets, MCP servers, TTS, and messaging tokens from `~/.hermes/`
- **Knowledge Graph full consolidation** — refactor `core/knowledge-graph.ts` and `memory/graph.ts` to implement the shared `GraphBackend` interface; migrate `memory/graph.ts` to delegate traversal/pathfinding on top of the canonical SQLite backend
- **Router Agent multi-user testing** — test with 2+ Telegram users; workspace-per-user isolation; document migration path from single-user to multi-user
- **Router Agent dispatch** — create and select real agent sessions/workspaces from routing decisions instead of telemetry-only routing
- **Complete desktop localization** — route visible strings through `t()` and add translation regression coverage
- **Crypto algorithm agility** — already implemented (`AES-256-GCM` / `ChaCha20-Poly1305` via `ARGENTUM_ENCRYPTION_ALGORITHM`); verify all callers use the configured algorithm and document the switching process
- **Private security disclosure process** — formalize the practice for handling non-public security vulnerabilities

### Accessibility (ongoing requirement)

Every new UI element must ship with keyboard navigation (tabindex if interactive, visible `:focus-visible` style) and appropriate ARIA attributes. This is not a one-time sprint — it is a process requirement.

- **Keyboard navigation** — `:focus-visible` styles and `tabindex` on interactive elements throughout the app ✅
- **Help button `?`** — context-sensitive help panel with docs links ✅
- **In-app update mechanism** — GitHub API check + browser download ✅
- **Accent color picker** — fully persisted, 8 presets + custom hex ✅
- **Screen reader support** — ARIA labels on icon-only buttons and live regions ✅
- **Error/warning styling** — icons + text, not color alone ✅

## Future (6-12 months)

- Tauri v3.0 upgrade when stable (resolves glib RUSTSEC-2024-0429 vulnerability)
- Additional messaging channels as demand warrants
- Improved test coverage to 90% statement/branch/function/line coverage

## What We Will NOT Do

- Cloud hosting or managed service (project remains self-hosted only)
- Collecting user data or analytics
- Mobile-native app (desktop focus only)
- Windows/Mac exclusive features (cross-platform remains a goal)

## How Roadmap Is Updated

The roadmap is reviewed monthly and updated as priorities shift. Major changes are discussed in GitHub issues.
