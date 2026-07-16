# Roadmap

## Current Status

Argentum v0.0.9 is in development. The project is actively maintained.

## v0.0.9 — Done

- **In-app OpenClaw migration** — one-click import from `~/.openclaw/`: skills, memory, SOUL.md, workspace files, memory.db, Telegram credentials, and config. Detects automatically on first run and accessible from Settings → Migration.
- **In-app feedback mechanism** — Settings → Help & Feedback: "Report a bug" and "Request a feature" open GitHub issue/discussion URLs with version and workspace info pre-filled. Security contact email also listed.
- **Help panel doc links** — clicking `?` shows the help panel with a docs link specific to the current section (Chat, Gateway, Settings, etc.).
- **Update mechanism** — `check_for_updates` and `download_update` Rust Tauri commands wired to GitHub Releases API; "Download update" opens the releases page in browser.
- **Security hardening** — `ip-address` override fixed to require `>= 10.1.1` (CVE-2026-42338); `sanitize-html` constraint tightened to `>= 2.17.2` (CVE-2026-40186); SECURITY.md updated.
- **Router Agent wired in** — `src/agents/router/` integrated into the main agent loop; YAML config support (`config/router.yaml`) added; `config/router.yaml.example` created.
- **Knowledge Graph consolidation** — `GraphBackend` interface exported from `features/knowledge-graph`; shared type re-export at `src/core/graph-types.ts`; three-implementation approach documented.
- **Thinking mode wired to all providers** — thinking level (fast/balanced/deep) now passed to Anthropic (adaptive thinking budget), Codex (reasoning effort), and OpenAI (reasoning_effort); Settings UI shows green note for reasoning-capable models.
- **Skills catalog Built-in tab** — new `source: 'argentum'` type; red "Built-in" badge; Built-in tab renders first by default; no install/GitHub link for bundled skills.
- **New Claude models** — `claude-sonnet-4-20250514` and `claude-opus-4-20250514` added to model catalog with 200k context and frontier reasoning capability.

## v0.1.0 (Next 3-6 months)

- **Hermes → Argentum migration** — full parity import of skills, memories, SOUL.md, config.yaml, `.env` secrets, MCP servers, TTS, and messaging tokens from `~/.hermes/`
- **Knowledge Graph full consolidation** — refactor `core/knowledge-graph.ts` and `memory/graph.ts` to implement the shared `GraphBackend` interface; migrate `memory/graph.ts` to delegate traversal/pathfinding on top of the canonical SQLite backend
- **Router Agent multi-user testing** — test with 2+ Telegram users; workspace-per-user isolation; document migration path from single-user to multi-user
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
