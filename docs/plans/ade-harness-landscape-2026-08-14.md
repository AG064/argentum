# Argentum ADE / Agent Harness Landscape Plan

Status: structured research and structured plans, separate plan flow
Date: 2026-08-14
Branch: `plan/ade-harness-landscape-2026-08-14`
Owner: TBD (see §9)
Companion to: [`docs/PREMIUM_AGENT_HARNESS_PLAN.md`](../PREMIUM_AGENT_HARNESS_PLAN.md) (master plan)
Companion to: [`docs/V0_0_9_MIGRATION.md`](../V0_0_9_MIGRATION.md) (migration matrix)
Companion to: [`docs/architecture.md`](../architecture.md) (native architecture)

---

## 0. Why this plan exists

The master plan already cites four ADE / agent-harness references
(Codex, ZCode, LM Studio Bionic, Claude Code Desktop). The 2026 agentic
landscape is far larger and the field keeps moving. This document does
five things and only five things:

1. inventories the other ADEs and agent harnesses that matter to Argentum;
2. extracts borrowable patterns and anti-patterns from the field;
3. proposes structured plans A–N across the Argentum implementation;
4. maps each plan to the existing Phases 0–6 of the master plan;
5. stays on a separate plan branch so it does not disrupt the active
   rewrite that is in flight on `UI_redesign`.

This plan does **not** modify the rewrite, does **not** introduce code,
and does **not** change any committed artefact on `UI_redesign`. It
lives on its own branch and is meant to be reviewed, edited, and then
backed into the master plan one plan at a time.

---

## 1. How the plan is organized

- §2 vocabulary and Argentum's own position
- §3 the inventory matrix (~30 entries)
- §4 per-category notes (terminal, IDE, BYOK, autonomous, open,
  messaging, browser, frameworks, goal-driven, memory)
- §5 borrowable patterns (synthesized)
- §6 anti-patterns to avoid (synthesized)
- §7 structured plans A–N (objective, scope, owned crates,
  acceptance, phase)
- §8 phase mapping matrix
- §9 open questions for the owner
- §10 reference inputs
- §11 definition of done for this plan

---

## 2. Vocabulary

- **ADE (Agentic Development Environment)** — a tool that helps a
  developer build software with AI agents. Typically an IDE, an
  editor, or a terminal product. Examples: Cursor, Windsurf, Zed,
  Claude Code, Aider, Codex.
- **Agent harness** — the runtime layer that wraps a model in an
  execution loop and manages tools, context, state, and safety.
  Examples: Claude Code, Codex CLI, OpenHands, Aider, Goose, SWE-agent.
- **Framework (vs harness)** — a library the operator assembles into a
  harness (LangChain, AutoGen, Deep Agents, OpenAI Agents SDK).
- **Goal-driven harness** — a harness that owns a long-running,
  budget-bound, evidence-verified objective (ZCode Goal Mode, Codex
  `/goal`).
- **Argentum's position** — a Rust-first agent harness with a native
  Slint desktop client and a mobile companion. The harness boundary is
  `argentum-cli` + `argentum-runtime`; the desktop shell is
  `argentum-ui` over `argentum-platform`. The product target is a
  premium task workspace, not a settings dashboard.

---

## 3. Inventory matrix

The matrix below lists ADEs and harnesses the landscape study
considered, with their public form factor, license, what they
genuinely do better than the average, and how Argentum should
relate to each. "Borrow" is a yes/no on whether Argentum should
adopt the pattern; "Avoid" is a yes/no on whether the entry exhibits
an anti-pattern Argentum must guard against.

### 3.1 Terminal-first agent harnesses

| Name | Vendor | License | Distinct approach | Borrow | Avoid |
| --- | --- | --- | --- | --- | --- |
| **Claude Code** | Anthropic | Proprietary | 30 hook events, subagents, Dynamic Workflows, MCP, Skills (SKILL.md), CLAUDE.md, 1M context | yes | no |
| **OpenAI Codex CLI** | OpenAI | Apache-2.0 (CLI) | Sandbox-first, AGENTS.md, multi-surface (CLI/cloud/app), `/goal` | yes | no |
| **Aider** | Open source | Apache-2.0 | Architect/Editor split, repo map, git-native, BYO model | yes | no |
| **Goose** | Block | Apache-2.0 (Rust) | 70+ MCP extensions, Recipes (YAML shareable workflows), local-first | yes | no |
| **OpenCode** | Open source | MIT (TypeScript) | 75+ providers, Plan/Build modes, Scout subagent, AGENTS.md, headless server | yes | no |
| **Gemini CLI** | Google | Apache-2.0 | Gemini 3.x default, generous free tier, GitHub Actions support | reference | no |
| **Muse Code** | Meta Superintelligence Labs | Proprietary (beta 2026-08) | Persistent background sub-agents in isolated Git worktrees | reference | no |
| **Hermes Agent** | Community | Open | Interactive + scheduled + messaging gateways | reference | no |

### 3.2 IDE-native ADEs

| Name | Vendor | License | Distinct approach | Borrow | Avoid |
| --- | --- | --- | --- | --- | --- |
| **Cursor** | Anysphere | Proprietary | Agents Window, Composer 2.5, parallel agents, worktree isolation, BugBot | reference | yes (credit pools) |
| **Windsurf** | Cognition (ex-Codeium) | Proprietary | Cascade, Codemaps, SWE-1.6, Flow awareness | reference | no |
| **Zed** | Zed Industries | Open (GPL) | Rust/GPUI native, co-author of Agent Client Protocol (ACP), hosts Claude Code/Codex/Gemini/Goose | reference (UX) | no |
| **AWS Kiro** | Amazon | Proprietary | Spec-driven (requirements → design → tasks), hooks | reference | no |
| **Trae** | ByteDance | Proprietary (free) | SOLO builder mode, broad free model access | reference | no |
| **PearAI** | Open source | Open | BYOK Cursor alternative, multi-backend (Claude Code/Codex/OpenAI) | reference | no |
| **Void** | Open source | Open (VS Code fork) | Local models, full data control, BYOK | reference | no |
| **JetBrains Junie** | JetBrains | Proprietary | Native agent inside IntelliJ/PyCharm | reference | no |
| **Google Antigravity** | Google | Proprietary (free preview) | Multi-agent parallel workspaces, browser-linked | reference | yes (governance) |
| **Sourcegraph Amp** | Sourcegraph | Proprietary | Repo-graph semantic context, thread sharing, no hard token caps | reference | no |

### 3.3 VS Code extensions (BYOK)

| Name | License | Distinct approach | Borrow | Avoid |
| --- | --- | --- | --- | --- |
| **Cline** | Apache-2.0 | Plan/Act loop, MCP-native, step-wise planning, auditable autonomy | reference | no |
| **Roo Code** | Apache-2.0 (Cline fork) | Multi-persona agent system | reference | no |
| **Continue.dev** | Apache-2.0 | Open-source, BYOK, custom model providers | reference | no |
| **Kilo Code** | Apache-2.0 | Multi-mode, telemetry-aware | reference | no |

### 3.4 Autonomous / cloud agents

| Name | Vendor | License | Distinct approach | Borrow | Avoid |
| --- | --- | --- | --- | --- | --- |
| **Devin** | Cognition | Proprietary | Parallel cloud VMs, full ticket autonomy, SWE-1.x | reference | yes (oversight) |
| **Replit Agent** | Replit | Proprietary | Cloud IDE + agent in one | reference | no |
| **v0** | Vercel | Proprietary | UI generation, design-system aware | deferred | no |
| **Bolt.new** | StackBlitz | Proprietary | Full-stack web in browser, instant dev server | deferred | no |
| **Lovable** | Lovable | Proprietary | App generation, design focus | deferred | no |
| **Manus** | Manus | Proprietary | Autonomous general agent | reference | yes (oversight) |
| **Google Jules** | Google | Proprietary | Asynchronous coding agent, GitHub-native | reference | no |

### 3.5 Open-source autonomous

| Name | License | Distinct approach | Borrow | Avoid |
| --- | --- | --- | --- | --- |
| **OpenHands (ex-OpenDevin)** | MIT | Event-stream architecture, BrowserGym, multi-agent, 77K+ stars | reference | no |
| **SWE-agent** | MIT (Princeton/Stanford) | Agent-Computer Interface (ACI), Mini-SWE-Agent, EnIGMA security mode | reference | no |
| **Open Interpreter** | MIT | Local code execution, computer use, language-agnostic | reference | yes (no sandbox by default) |

### 3.6 Messaging / multi-channel

| Name | License | Distinct approach | Borrow | Avoid |
| --- | --- | --- | --- | --- |
| **OpenClaw** | MIT | Multi-channel personal assistant gateway, capability registration | yes (already imported via `feature/openclaw-new-features`) | no |
| **Hermes Agent** | Open | Interactive + scheduled + messaging gateways | reference | no |
| **MCP bridges** | Open (protocol) | Standard interface for messaging agents | yes | no |

### 3.7 Browser / computer-use

| Name | Vendor | License | Distinct approach | Borrow | Avoid |
| --- | --- | --- | --- | --- | --- |
| **ChatGPT Agent** | OpenAI | Proprietary | CUA model, 87% on complex JS, 58% WebArena | reference | yes (no supervision yet) |
| **Anthropic Computer Use API** | Anthropic | Proprietary | Computer / Text Editor / Bash tool trio, zoom action | reference | yes (oversight) |
| **Google Project Mariner** | Google | Proprietary | 83.5% WebVoyager, 84% ScreenSpot | reference | yes (oversight) |
| **Perplexity Comet** | Perplexity | Proprietary | Agentic browser | reference | yes (oversight) |
| **Browser Use** | Open | MIT | Open-source, Playwright-based, accessibility tree | deferred | no |
| **Stagehand** | Open | MIT | Browser-use by AI, structured actions | deferred | no |

### 3.8 Frameworks (vs harnesses)

| Name | License | Distinct approach | Borrow | Avoid |
| --- | --- | --- | --- | --- |
| **LangChain / LangGraph** | MIT | Most popular framework | reference | no |
| **AutoGen** | MIT (Microsoft) | Multi-agent, conversational | reference | no |
| **CrewAI** | MIT | Role-based multi-agent | reference | no |
| **Magentic-One** | MIT (Microsoft) | Multi-agent orchestrator | reference | no |
| **Letta** | Apache-2.0 | Memory-first agents | reference | no |
| **Deep Agents** | MIT | Harness scaffolding as a library | reference | no |
| **OpenAI Agents SDK** | Apache-2.0 | Agents, handoffs, guardrails | reference | no |

### 3.9 Goal / plan-driven (already in master plan)

- **ZCode** (Z.ai) — Goal Mode, task/file management, safety confirmation, ADE tools.
- **Codex `/goal`** — long-horizon background runs, budget-bound.
- **AWS Kiro** — spec-driven phases (requirements → design → tasks).

### 3.10 Memory / persistence patterns

- **AGENTS.md / CLAUDE.md / SKILL.md** — project memory files. Adopted
  by Claude Code, Codex, OpenCode, Goose. Strong convention.
- **Goose Recipes** — YAML shareable workflows.
- **Letta** — memory-first agents with archival.
- **OpenClaw** — gateway-wide capability registration.
- **Cursor memory** — code-aware retrieval.

### 3.11 Mobile / companion

- **Claude Code mobile** — session + approval + push.
- **Codex mobile** — same pattern.
- **Argentum mobile** — already on the master plan roadmap.

---

## 4. Per-category notes

### 4.1 Terminal-first harnesses — what to learn

- **Claude Code** sets the bar for a typed event surface. The 30 hook
  events are a useful lower bound for Argentum's hooks system.
  CLAUDE.md and SKILL.md are the strongest conventions in the field
  and the cheapest to adopt; see Plan A.
- **Codex CLI** is the closest reference architecture in shape to
  Argentum: Rust, sandboxed, multi-surface (CLI + cloud + app). Its
  `/goal` mechanism maps cleanly onto Argentum's goal contract.
- **Aider** shows the value of an explicit Architect/Editor split
  (different model roles, different responsibilities). Argentum can
  keep the harness monorole but expose this as an option; see Plan E.
- **Goose** Recipes are the most practical "named workflow" pattern:
  YAML, shareable, version-controlled. Argentum can carry a similar
  construct once the extension boundary is stable; see Plan F.
- **OpenCode** proves a 75-provider harness is feasible. Argentum
  should not chase 75 providers, but the model-agnostic adapter trait
  is the right shape; see existing `argentum-providers` design.
- **Muse Code**'s "persistent background sub-agents in isolated Git
  worktrees" is the same idea as Argentum's worktree-aware runs (Plan
  C) plus subagent delegation (Plan I). Worth tracking as a public
  reference.
- **Hermes Agent** demonstrates that "interactive + scheduled +
  messaging" is a coherent product surface. Argentum already has
  scheduled work on the roadmap; messaging is out of scope for 0.1.x.

### 4.2 IDE-native ADEs — what to learn

- **Cursor**'s Agents Window is the strongest in-product proof that
  "agent orchestration" is the future of the IDE. Argentum is not an
  IDE, but the **worktree-per-task** idea and the **typed change set**
  are worth borrowing; see Plan C.
- **Windsurf** Codemaps and Flow are useful UI patterns for "show me
  what's about to change." Argentum's plan surface can borrow the
  spirit; implementation is in `argentum-ui`.
- **Zed** is the strongest open-source reference for a fast native
  UI. Argentum's choice (Slint) is a different stack but the goal
  (sub-1.5s shell, 60 FPS, no web view) is identical. The **Agent
  Client Protocol (ACP)** Zed co-authored is the most interesting
  interop target in the field; see Plan G.
- **AWS Kiro**'s spec-driven workflow is the closest cousin to
  Argentum's goal contract. The master plan already covers this in
  Phase 1; Plan D codifies it.
- **Trae / PearAI / Void** show the BYOK / open-source tiers of the
  market. Argentum's exact-profile credential pattern (only the
  matching canonical credential is sent to its approved origin) is
  safer than the BYOK default and should be advertised.
- **JetBrains Junie / Google Antigravity / Sourcegraph Amp** are
  useful references but not direct competitors to Argentum's product
  shape.

### 4.3 VS Code extensions (BYOK) — what to learn

- **Cline**'s Plan/Act loop is the simplest "stepwise planning"
  pattern in the field. Argentum's `argentum-runtime` already has
  explicit lifecycle states (draft → queued → planning → running →
  …) so the pattern is already in the master plan; reinforce the
  audit trail and the user-visible plan surface.
- **Roo Code**'s multi-persona system is a useful reference if
  Argentum later adds named agent roles (architect / editor /
  verifier). See Plan E.

### 4.4 Autonomous / cloud agents — what to learn

- **Devin** is the strongest "unattended ticket" reference, but its
  parallel-VM model does not match Argentum's local-first,
  workspace-bound contract. Borrow the audit trail and the change
  set, not the autonomy.
- **Replit Agent / v0 / Bolt.new / Lovable** are full app
  generators. Out of scope for 0.1.x. Track as future optional
  capability if the goal-driven flow matures.

### 4.5 Open-source autonomous — what to learn

- **OpenHands**' event-stream architecture is the cleanest
  observability model in the field. Argentum already has an event log
  in `argentum-store`; reinforce the read-only projection and the
  replay story.
- **SWE-agent**'s Agent-Computer Interface (ACI) is a useful mental
  model for "what does the harness expose to the model" — the
  Argentum equivalent is the typed tool registry in
  `argentum-tools`. Keep the surface small and explicit.
- **Open Interpreter** shows the danger of "no sandbox by default."
  Reinforce Argentum's `default-deny` boundary.

### 4.6 Messaging / multi-channel — what to learn

- **OpenClaw** is already partially imported via the
  `feature/openclaw-new-features` branch. Continue to keep its
  contract in a Rust adapter and never replicate its dynamic
  JavaScript plugin model (master plan already enforces this).
- **MCP bridges** are a stable interop target. Argentum can host
  MCP servers via `argentum-platform`'s process boundary when
  needed, but must never embed a JS runtime.

### 4.7 Browser / computer-use — what to learn

- Web browser agents are the **most mature** computer-use category in
  2026, but still production-ready only for *supervised* and
  *constrained* tasks. Argentum should not ship a browser or
  computer-use capability in 0.1.x; if added later, it must inherit
  the capability-broker policy and require approval for every
  irreversible action.
- The "Computer / Text Editor / Bash" trio from Anthropic's
  Computer Use API is a useful shape: a small, well-named tool
  surface, no implicit permission.

### 4.8 Frameworks (vs harnesses) — what to learn

- **LangChain / AutoGen / CrewAI / Magentic-One** are
  *frameworks*, not harnesses. Argentum is a harness. A user
  building on top of Argentum should not need any of these.
- **Deep Agents** and **OpenAI Agents SDK** are the closest
  examples of "harness scaffolding as a library." Argentum's
  `argentum-runtime` and `argentum-domain` are the same idea, but
  in Rust and tied to the typed event log. Reinforce the typed
  contract; do not expose a free-form graph.
- **Letta**'s memory-first model is a useful reference for Plan M.

### 4.9 Goal / plan-driven

- ZCode and Codex `/goal` are already cited in the master plan.
  AWS Kiro's spec-driven workflow is a third, slightly different
  reference. Argentum's goal contract is closer to ZCode's Goal
  Mode; do not import Kiro's "spec/requirements" documents
  wholesale, but the principle "the harness must not declare
  success on its own" is shared and important.

### 4.10 Memory / persistence

- **AGENTS.md / SKILL.md** are the strongest convention in the
  field. Cheap to adopt. See Plan A.
- **Goose Recipes** are the most practical workflow bundle. See
  Plan F.
- **OpenClaw's** capability registration is a useful mental model
  for Plan M's memory tiers.

### 4.11 Mobile / companion

- Claude Code and Codex mobile both follow the pattern: open
  straight into the active session or the running queue, with
  approval as a primary surface. Argentum's mobile shell already
  documents this. See Plan J.

---

## 5. Borrowable patterns (synthesized)

This section lists patterns the field has converged on, with
provenance and a pointer into §7's structured plans.

| # | Pattern | Origin(s) | Argentum application | Plan |
| --- | --- | --- | --- | --- |
| 1 | Project memory file | Claude Code (`CLAUDE.md`), Codex (`AGENTS.md`), OpenCode | Detect and load a `AGENTS.md` / `SKILL.md` at the workspace root | A |
| 2 | Skill file convention | Claude Code, OpenCode (`SKILL.md`) | A typed `argentum-skills` capability that loads `SKILL.md` from the workspace | A |
| 3 | Lifecycle hooks (30 events) | Claude Code | A `argentum-hooks` event surface, versioned, external subscription safe | B |
| 4 | Worktree-per-run | Cursor, Codex, Superset, Muse Code | First-class `Worktree` in `argentum-workspaces`; shown at the composer | C |
| 5 | Architect/Editor split | Aider, Roo Code | Optional provider role separation in `argentum-providers` | E |
| 6 | Named YAML workflows (Recipes) | Goose | Typed, shareable, versioned recipe bundle in `argentum-runtime` | F |
| 7 | Spec-driven phases | AWS Kiro | Goal-contract elaboration (already in master plan) | D |
| 8 | Agent Client Protocol (ACP) | Zed | `argentum-acp` adapter to host Argentum as an ACP agent | G |
| 9 | Observable context compaction | Claude Code, OpenCode | Explicit `Compact` command, pinned segments, audit trail | H |
| 10 | Bounded subagent delegation | Claude Code, Goose, OpenHands | `Delegate` command with budget + capability subset | I |
| 11 | Browser/computer-use (deferred) | ChatGPT Agent, Anthropic CU | Optional, sandboxed, approval-gated; not in 0.1.x | (deferred) |
| 12 | Multi-agent orchestration (deferred) | Cursor 3, Antigravity, Superset | Deferred per master plan | (deferred) |
| 13 | Memory tiers | Letta, OpenClaw | `argentum-store` durable memory: workspace / project / session / run | M |
| 14 | Mobile companion | Claude Code, Codex | Mobile shell with sheets, queue, approvals, deep links | J |
| 15 | Visual regression & fixtures | Industry norm | Real fixtures for every state; reference harnesses as cross-checks | K |
| 16 | ADE-aware onboarding | Field convention | Detect `.claude`, `.cursor`, `.codex`, `.continue`, `AGENTS.md`, `CLAUDE.md`, `SKILL.md`, `.clinerules`, `.cursorrules` and offer a one-time migration | L |
| 17 | Protocol + event-log versioning | Claude Code, Codex | Versioned protocol, versioned event log, versioned profile schema, migration scripts | N |
| 18 | Explicit lifecycle state machine | Field norm | Already in master plan; reinforce audit trail | (existing) |
| 19 | Default-deny capability broker | Argentum-original, ZCode-compatible | Already in master plan; reinforce | (existing) |
| 20 | Exact-profile credentials only | Argentum-original, Kiro-like | Already in master plan; advertise as differentiator | (existing) |

---

## 6. Anti-patterns to avoid (synthesized)

These are the failure modes the field has hit. Each entry is
backed by a specific observation from the inventory; each maps to
an existing master-plan clause that must remain enforced, or to a
new plan in §7.

| # | Anti-pattern | Where it shows up | Existing or new guard |
| --- | --- | --- | --- |
| 1 | Model-declared success | Cursor, Aider, OpenHands, Manus | Master plan "Goal contract completion"; Plan D |
| 2 | Implicit permission grant from UI mode | ZCode "Full Access" | Master plan "Capability broker"; existing |
| 3 | JS plugin runtime inside the host process | Cursor, Continue, Cline fork variants | Master plan "Plugin and extension policy"; existing |
| 4 | Web-only UI | Bolt, v0, Lovable | Master plan "Renderer rule"; existing |
| 5 | Fake readiness / placeholder data | Field-wide | Master plan "Product standard"; existing |
| 6 | Background subagent runaway | Claude Code, OpenHands | Plan I (budget + capability subset) |
| 7 | Auto-merge from agent | Cursor, Devin | Master plan "Definition of done"; existing (humans review) |
| 8 | Untyped IPC bridge | Field-wide | Master plan "State and IPC contract"; existing |
| 9 | Silent context compaction losing user-visible state | Claude Code, OpenCode | Plan H (explicit, user-visible) |
| 10 | Credential stored in profile / event / DB | Field-wide | Master plan "Security boundary"; existing |
| 11 | Saved profile redirecting canonical credential | Malicious config | Master plan "Canonical credential rule"; existing |
| 12 | Unbounded screenshot / DOM scrape | Browser-use harnesses | Master plan "Performance budgets"; Plan J (mobile) |
| 13 | Sandbox escape via subprocess | Open Interpreter default | Master plan "Tool requests and approvals"; existing |
| 14 | Goal completion without evidence | ZCode (rejected), Kiro (partial) | Master plan "Goal contract"; Plan D |
| 15 | BYOK default to user-supplied endpoint | Aider, Cline default | Argentum exact-profile pattern; existing |
| 16 | Hidden background telemetry | Roo Code, Kilo Code | Master plan "Verification rule"; existing |
| 17 | Vendor lock-in via proprietary event format | Claude Code, Cursor | Plan N (versioned, documented) |
| 18 | "Bigger context" as the differentiator | Windsurf 1M, Claude Code 1M | Argentum's diff: small, calm, evidence-verified, not the largest context |

---

## 7. Structured plans A–N

Each plan has: **objective**, **scope (in / out)**, **owned crates**,
**acceptance criteria**, **phase**, and **references**.

### Plan A — `AGENTS.md` / `SKILL.md` project memory

- **Objective:** support the `AGENTS.md`, `SKILL.md`, and similar
  project memory conventions at the workspace root. Read on session
  start; surface in the session header; expose via a typed command.
- **In scope:** parser, versioned types, watcher for change, a
  single canonical "Argentum project memory" view that is the
  ordered union of `AGENTS.md` (if present), `SKILL.md` files
  (if present), and the legacy v0.0.9 `MEMBERS.md` (if present).
- **Out of scope:** running instructions from the file automatically;
  only humans, the composer, and the onboarding step see them.
- **Owned crates:** `argentum-workspaces` (load), `argentum-domain`
  (ProjectMemory type), `argentum-ui` (display), `argentum-cli`
  (`memory show` / `memory set` commands).
- **Acceptance:**
  - a workspace-root `AGENTS.md` is parsed, versioned, and shown
    in the session header;
  - a `SKILL.md` is registered into the typed skill registry;
  - a missing file does not error;
  - redaction is applied to anything that looks like a credential
    or a URL credential.
- **Phase:** 1 (Rust foundation) → 2 (golden path).
- **References:** Claude Code `CLAUDE.md`, Codex `AGENTS.md`,
  OpenCode `AGENTS.md`, Goose `SKILL.md`-compatible.

### Plan B — Lifecycle hooks system

- **Objective:** let advanced users and extensions subscribe to
  typed runtime events without owning a Rust build. A stable,
  versioned event surface with veto and cancel semantics.
- **In scope:** an event enum that mirrors the master plan
  lifecycle plus the goal lifecycle; a config file
  (`argentum.hooks.toml`); a process-boundary subscription so
  external scripts can react; an ordering guarantee.
- **Out of scope:** allowing hooks to widen the capability set;
  allowing hooks to declare success; allowing hooks to bypass
  policy.
- **Owned crates:** new `argentum-hooks` or a `hooks` module in
  `argentum-runtime`; config in `argentum-platform`; UI
  inspection in `argentum-ui`.
- **Acceptance:**
  - 30 typed event kinds exposed (matches Claude Code as a lower
    bound);
  - can subscribe from a separate process;
  - ordering is preserved;
  - can cancel a queued tool call;
  - can veto an approval;
  - cannot widen a capability.
- **Phase:** 4 (extension migration).
- **References:** Claude Code 30 hook events, Codex CLI
  `AGENTS.md` actions, Kiro hooks.

### Plan C — Worktree isolation in `argentum-workspaces`

- **Objective:** per-run worktree binding for the workspace,
  matching Codex, Cursor, and Muse Code. A run records the
  worktree it ran against; the UI shows it at the composer.
- **In scope:** `Worktree` as a first-class workspace type;
  binding to a run on creation; integration with the v0.0.9
  workspace pointer; a `worktree` subcommand on the CLI.
- **Out of scope:** automatic worktree creation; cloud VMs;
  remote worktrees.
- **Owned crates:** `argentum-workspaces`, `argentum-domain`
  (Worktree type), `argentum-ui` (composer), `argentum-cli`
  (subcommand).
- **Acceptance:**
  - a run can be created against a worktree;
  - the worktree is recorded in the run record;
  - the UI shows it at the composer and on the run row;
  - cleanup of merged worktrees is a user action, not a system
    action.
- **Phase:** 3 (harness workspace).
- **References:** Cursor `/worktree`, Codex worktrees,
  Superset, Muse Code.

### Plan D — Goal contract & verification (extends master plan)

- **Objective:** codify the master-plan goal contract in types
  and tests. Map to ZCode Goal Mode and Codex `/goal`.
- **In scope:** persisted goal contract, lifecycle, budgets
  (time, tokens, tool rounds, iterations), iteration linkage,
  verification record, audit history.
- **Out of scope:** auto-continuation that violates budget;
  completion on user-declared success.
- **Owned crates:** `argentum-domain`, `argentum-runtime`,
  `argentum-store`, `argentum-ui`.
- **Acceptance:**
  - a Rust test creates a goal, an iteration, an approval, a
    change set, and a verification record;
  - missing, stale, cancelled, or failed evidence cannot
    complete the goal;
  - the audit log records every transition.
- **Phase:** 1 (foundation) → 6 (release hardening).
- **References:** ZCode Goal Mode, Codex `/goal`, master plan
  §"Agent lifecycle" and §"Goal contract".

### Plan E — Architect/Editor role split (provider-side)

- **Objective:** optionally separate model roles (Architect for
  planning, Editor for diffs). Aider-inspired.
- **In scope:** a typed `ProviderRole` enum (`Architect`,
  `Editor`, `Verifier`); per-call selection; profile binding
  per role; UI picker.
- **Out of scope:** enforcing role-specific providers; the
  default is monorole.
- **Owned crates:** `argentum-providers`, `argentum-domain`,
  `argentum-ui`, `argentum-cli`.
- **Acceptance:**
  - a run can declare architect and editor provider profiles;
  - the runtime selects them per turn;
  - the same provider can be used for all roles;
  - a per-role profile is a workspace-scoped profile, not a
    global one.
- **Phase:** 4 (extension migration).
- **References:** Aider Architect/Editor, Roo Code personas.

### Plan F — Named workflow recipes

- **Objective:** YAML-named, shareable, versioned workflows
  (Goose Recipes pattern) compiled into Rust and stored durably.
- **In scope:** a schema in `argentum-domain`; a loader in
  `argentum-store`; a runner in `argentum-runtime`; a picker
  in `argentum-ui`; a `recipe` subcommand on the CLI.
- **Out of scope:** recipes that grant capabilities; recipes
  that declare success; recipes that bypass approval.
- **Owned crates:** `argentum-domain`, `argentum-store`,
  `argentum-runtime`, `argentum-ui`, `argentum-cli`.
- **Acceptance:**
  - a recipe is a typed command sequence;
  - can be invoked from CLI and UI;
  - can be paused and resumed;
  - cannot grant a capability the policy does not allow;
  - cannot declare a goal complete.
- **Phase:** 4 (extension migration).
- **References:** Goose Recipes, Codex `/goal` long-horizon
  background runs.

### Plan G — Agent Client Protocol (ACP) host

- **Objective:** optionally host Argentum as an ACP agent
  inside Zed or any ACP-compatible client. This is the strongest
  interop target in the field and the most credible way to
  reach IDE-native users.
- **In scope:** a thin `argentum-acp` adapter that implements
  the ACP server role over the same command host; a feature
  flag.
- **Out of scope:** Argentum becoming a full ACP client (it
  has its own native shell).
- **Owned crates:** new `argentum-acp`, or feature flag in
  `argentum-cli`.
- **Acceptance:**
  - Zed (or any ACP client) can list Argentum projects, start
    a session, stream events, and approve actions;
  - no capability is widened by the ACP boundary;
  - the ACP server uses the same command host as the desktop.
- **Phase:** 4 → 5 (mobile).
- **References:** Agent Client Protocol (Zed, JetBrains,
  others), Zed AI agents panel.

### Plan H — Context compaction

- **Objective:** explicit, observable, user-controlled context
  compaction (Claude Code five-layer, OpenCode auto-compact).
- **In scope:** a `Compact` command; a clear user-visible
  boundary; pinned segments preserved; transcript re-anchored
  correctly.
- **Out of scope:** silent compaction; compaction that loses
  pinned segments; compaction that hides user-visible state.
- **Owned crates:** `argentum-runtime`, `argentum-store`,
  `argentum-ui`.
- **Acceptance:**
  - a session can compact on user request;
  - the user sees a "before / after" summary;
  - pinned segments are preserved;
  - the event log records the compaction;
  - a compact cannot grant a capability.
- **Phase:** 3 (harness workspace).
- **References:** Claude Code five-layer compaction, OpenCode
  auto-compact, LangChain summarizers.

### Plan I — Subagent / delegation protocol

- **Objective:** explicit, bounded subagent invocation; never
  exceeds the parent budget; never widens capability.
- **In scope:** a `Delegate` command that opens a child run
  with an explicit budget, capability subset, and audit link.
- **Out of scope:** background subagents that outlive the
  parent run; subagents that grant themselves capabilities.
- **Owned crates:** `argentum-runtime`, `argentum-security`,
  `argentum-store`, `argentum-ui`.
- **Acceptance:**
  - a subagent cannot exceed the parent's budget;
  - cannot use a capability the parent did not grant;
  - result is folded back into the parent run;
  - audit log records delegation and result;
  - parent can cancel a subagent.
- **Phase:** 4 (extension migration).
- **References:** Claude Code subagents, Goose subagents,
  OpenHands multi-agent, Muse Code persistent background
  sub-agents.

### Plan J — Mobile companion (extends master plan)

- **Objective:** align with Claude Code and Codex mobile
  patterns. Sheets-only UX. Per-surface capability profiles.
  Push notifications and deep links for approvals.
- **In scope:** iOS and Android shells; active session entry
  point; running queue; approvals sheet; notification surface;
  deep links for "open in Argentum."
- **Out of scope:** a shrunken desktop build; a permanent
  inspector column; a web view of the desktop UI.
- **Owned crates:** `argentum-app` (mobile variant),
  `argentum-platform`, `argentum-ui`, `argentum-security`.
- **Acceptance:**
  - iOS and Android shell opens into the active session or
    queue;
  - approvals arrive as a notification with a deep link;
  - the mobile shell never renders a squeezed desktop rail;
  - touch targets are at least 44 px;
  - safe-area, IME, and orientation are handled in the native
    shell, not in a web view.
- **Phase:** 5 (mobile companion).
- **References:** Claude Code mobile, Codex mobile, master
  plan §"Mobile shell".

### Plan K — Visual regression & fixture contract (extends master plan)

- **Objective:** capture real, not synthetic, fixtures for the
  design system; reuse across desktop, mobile, and reference
  harnesses.
- **In scope:** a fixture library covering loading, running,
  approval, error, empty, completed, paused, budget-limited,
  verification-failed, evidence-complete. Cross-platform capture
  at 1440, 1280, 1024, 768, 430, and 360 px.
- **Out of scope:** synthetic Lorem-ipsum fixtures; fixtures
  that misrepresent the state.
- **Owned crates:** `argentum-ui` (test assets), `scripts/validate-*`.
- **Acceptance:**
  - fixtures are generated from real runs, not hand-written;
  - every state has at least one fixture;
  - visual regression catches accidental regressions;
  - fixtures are versioned with the schema.
- **Phase:** 1 → 6.
- **References:** master plan §"Design system implementation
  gates".

### Plan L — ADE-aware onboarding

- **Objective:** detect existing ADE configuration in the
  workspace and offer a one-time, non-destructive migration
  prompt.
- **In scope:** detection of `.claude/`, `.cursor/`, `.codex/`,
  `.continue/`, `AGENTS.md`, `CLAUDE.md`, `SKILL.md`,
  `.clinerules`, `.cursorrules`. A single approval card; a
  preview; a confirmation.
- **Out of scope:** automatic overwrite of any file; silent
  import.
- **Owned crates:** new `argentum-onboarding` or module in
  `argentum-workspaces`.
- **Acceptance:**
  - first-run detection shows a single approval card;
  - the user can decline without losing the source files;
  - the original file is preserved (or moved, never overwritten);
  - the import is auditable in the event log.
- **Phase:** 1 → 2.
- **References:** Claude Code first-run, Codex first-run,
  Kiro first-run.

### Plan M — Persistent memory tiers

- **Objective:** durable workspace, project, session, and run
  memory. Each layer is typed, auditable, and redactable.
- **In scope:** three named layers (`workspace`, `project`,
  `session`); each with its own retention and redaction
  policy; visible in UI; exportable.
- **Out of scope:** memory that can be widened by the model;
  memory that contains credentials; memory that survives
  uninstall.
- **Owned crates:** `argentum-store`, `argentum-domain`
  (MemoryRecord).
- **Acceptance:**
  - three named layers, each with its own retention and
    redaction policy;
  - visible in UI;
  - exportable as JSONL with the event-log schema;
  - redaction applied to anything that looks like a credential
    or a URL credential.
- **Phase:** 1 (foundation).
- **References:** Letta, OpenClaw capability registration,
  Claude Code CLAUDE.md.

### Plan N — Release & versioning compatibility

- **Objective:** versioned protocol, versioned event log,
  versioned profile schema. Each version has a migration
  script.
- **In scope:** protocol v1 documented; event log records
  version; profile schema versioned; migration tested.
- **Out of scope:** breaking changes without a protocol
  version bump; silent schema changes.
- **Owned crates:** `argentum-domain` (versioning),
  `argentum-store` (migrations), `argentum-cli` (version
  flag).
- **Acceptance:**
  - protocol v1 is documented in `docs/architecture.md`;
  - event log entries record schema version;
  - profile schema is versioned;
  - a migration script is tested for every schema bump;
  - additive event variants remain backward compatible;
  - breaking request or response changes require a
    protocol-version decision.
- **Phase:** 1 → 6.
- **References:** Codex protocol versioning, master plan
  §"Draft versioning".

---

## 8. Phase mapping

The table below maps each plan to the master plan's Phases 0–6.
"S" = spec, "B" = build, "T" = test, "R" = refine, "G" = release
gate. Empty cells mean the plan does not touch that phase.

| Plan | Phase 0 (lock) | Phase 1 (foundation) | Phase 2 (vertical slice) | Phase 3 (harness workspace) | Phase 4 (extension) | Phase 5 (mobile) | Phase 6 (hardening) |
| --- | --- | --- | --- | --- | --- | --- | --- |
| A — AGENTS.md / SKILL.md | S | B | T | R | | | G |
| B — Hooks | | S | | | B | | G |
| C — Worktree | | S | | B | R | | G |
| D — Goal contract & verification | S | B | T | R | | | G |
| E — Architect/Editor | | | | | B | | G |
| F — Recipes | | | | | B | | G |
| G — ACP host | | | | | B | T | G |
| H — Context compaction | | | B | R | | | G |
| I — Subagent delegation | | S | | | B | | G |
| J — Mobile companion | | | | | | B | G |
| K — Visual regression & fixtures | S | seed | T | R | R | R | G |
| L — ADE-aware onboarding | | | B | R | | | G |
| M — Memory tiers | S | B | | R | | | G |
| N — Versioning | S | B | G | G | G | G | G |

The "G" markers in Phase 6 are intentionally identical across
plans: the master plan's "Definition of done" is the universal
gate.

---

## 9. Open questions for the owner

These are the questions this plan cannot answer on its own. The
owner (Aleksey / AG 064) should answer or defer each one
explicitly.

1. **Interop vs native.** Of the 30+ ADEs / harnesses surveyed,
   which should Argentum *natively* support (provider adapter,
   import, exporter), and which should it be *interoperable
   with* (ACP, MCP, JSONL, hooks)? Specifically:
   - Should Argentum host an ACP server (Plan G)?
   - Should Argentum accept MCP servers in 0.1.x, or only as
     sidecar processes?
   - Should the JSONL protocol become a published standard, or
     stay internal?
2. **Naming.** Should Argentum use `AGENTS.md` (Codex / OpenCode
   convention) or its own name (`ARGENTUM.md` / `SKILL.md`)?
   The field has converged on `AGENTS.md`; using it lowers
   friction.
3. **Recipes scope.** Should the recipes feature ship in 0.1.x,
   or wait for a stable extension boundary? Shipping earlier
   locks in the schema.
4. **Mobile pattern.** Should the mobile companion follow Claude
   Code / Codex (notification + deep link) or be entirely
   native (no notifications, only a foreground app)? Native
   matches Argentum's "calm, no fake readiness" tone.
5. **Onboarding aggression.** How aggressively should Argentum
   detect and migrate existing ADE files in onboarding? A
   conservative default is "show one card, never auto-import."
6. **Architect/Editor default.** Should Architect/Editor be a
   monorole by default with an opt-in to split, or should it
   be split by default for tasks above a token threshold? The
   master plan is silent here; Plan E is the carrier.
7. **Worktree creation.** Should Argentum ever create a
   worktree on its own (e.g., for a sandboxed run), or is
   worktree creation always a user action? The master plan
   implies the latter; Plan C is the carrier.
8. **Subagent budget.** When a parent run has 10 minutes of
   budget left, what fraction may a subagent consume? 50%? 30%?
   Plan I must answer this.
9. **Goal contract elaboration.** Should the goal contract
   support AWS Kiro-style "requirements / design / tasks" sub-
   documents, or stay flat? Plan D is the carrier.
10. **Memory tier retention.** What are the default retention
    policies for `workspace`, `project`, and `session` memory?
    Plan M is the carrier.

---

## 10. Reference inputs

The following sources informed this plan. They are listed in
declaration order in the research review, not in priority order.

- Codex app overview, Codex environments, Codex worktrees, Codex
  integrated terminal, Codex code review.
- ZCode task and file management, ZCode Goal Mode, ZCode safety
  confirmation, ZCode ADE tools.
- LM Studio Bionic, LM Studio Projects and Sessions.
- Claude Code Desktop, Claude Code harness design (Dive into
  Claude Code, arXiv 2604.14228), Claude Computer Use API.
- Claude Code vs Cursor vs Codex vs Aider (Requesty, 2026).
- Best AI Coding Agents in 2026 (Seekvana, 2026).
- The 13 Best Agentic IDEs in 2026 (DataCamp).
- 8 AI IDEs That Replaced VS Code Workflows (SSOJet, 2026).
- The Complete Guide to Agentic Coding Tools in 2026
  (Iceberg Lakehouse).
- Best AI Coding IDEs 2026: Cursor, Windsurf, Kiro, Zed,
  Copilot (Awesome Agents).
- Kiro vs Zed vs Cursor vs Windsurf (CodeMySpec, 2026).
- Is Zed ready for AI power users in 2026? (Builder.io).
- Best Agentic IDE in 2026: Multi-Agent Coding Tools Compared
  (Superset).
- Top 10 AI Agent Harnesses — Open vs Closed 2026 (ExplainX).
- Best AI Coding Agents in 2026: Harness, Cost, and Tokens
  (Firecrawl).
- Best Agent Harnesses 2026: A Builder's Field Guide
  (Future AGI).
- 10 Agent Harnesses Every AI Builder Should Know in 2026
  (The Tool Nerd).
- Code Execution Agents 2026: Scaffolds Beat the Model
  (Best AI Web).
- Agent Tools: Beyond Claude Code (Claude Code Ultimate Guide
  on GitHub).
- OpenHands: An Open Platform for AI Software Developers as
  Generalist Agents (arXiv 2407.16741).
- The OpenHands Software Agent SDK: A Composable and Extensible
  Framework (arXiv 2511.03690).
- OpenHands vs SWE-Agent 2026 (Local AI Master).
- Computer Use and GUI Agents in 2026: State of the Art
  (Zylos.ai, 2026-02-08).
- Best AI Coding Agents in 2026 (Context Studios).
- AI Coding Agents 2026: Claude Code, Cursor, Muse Code
  (Coder Sera).
- The 2026 Tool-Use and Computer Agent Landscape (GitHub
  ombharatiya/ai-system-design-guide).
- Agentic Coding Harnesses: A Comparison (Paul Cullen Rowe,
  Medium).

Internal references:

- `docs/PREMIUM_AGENT_HARNESS_PLAN.md` (master plan).
- `docs/V0_0_9_MIGRATION.md` (migration matrix).
- `docs/architecture.md` (native architecture).
- `docs/ARGENTUM_DESIGN_SYSTEM.md` (design contract).
- `ROADMAP.md` (current roadmap).

---

## 11. Definition of done

This plan is considered "done" when **all** of the following
are true:

1. All 14 plans A–N have an **owner**, a **target phase**, and
   an **acceptance test** in `tests/`.
2. Each plan is reflected in `ROADMAP.md` or the master plan
   (`docs/PREMIUM_AGENT_HARNESS_PLAN.md`).
3. The owner has answered the open questions in §9, or
   explicitly deferred each one with a deadline.
4. The plan is referenced from the master plan and from
   `ROADMAP.md` (one-line backlink each).
5. The plan has been merged into `development` (or its
   successor) at least once, so the rewrite picks it up
   on the next rebase.
6. The plan is reviewed by at least one independent ADE
   (Codex CLI / GitHub Copilot Coding Agent / OpenHands /
   OpenCode / Aider / Goose) before final acceptance.

---

## Appendix A — Why this plan does not change `UI_redesign`

The active rewrite on `UI_redesign` is in flight: 394 files
staged for rename into `legacy/`, 38 unstaged modifications, and
the new untracked tree under `crates/`, `platforms/`, `ui/`,
`assets/`, `scripts/`, `docs/`, plus the new `Cargo.toml`,
`Cargo.lock`, `deny.toml`, and root `README.md`. This plan
deliberately does not touch any of that. The plan lives on a
separate branch (`plan/ade-harness-landscape-2026-08-14`) backed
by a separate worktree, so the rewrite can continue without
interruption.

When a plan from §7 is ready to merge, the preferred path is:

1. Open a PR from `plan/ade-harness-landscape-2026-08-14` to
   `development` (or to a feature branch off `development`).
2. Carry **only** the doc changes back. No code lands in this
   plan; the doc is the deliverable.
3. Each plan A–N becomes its own follow-up PR with code,
   tests, and a release-gate update.

## Appendix B — How to extend this plan

When a new ADE or harness is significant enough to deserve a
row in §3, add it. When a new pattern or anti-pattern is
discovered, add it to §5 or §6 with a source. When a new
plan is needed, add it as Plan O, P, Q, ... and update §8.
Keep the "owner" and "phase" columns honest; an unowned plan
is not a real plan.
