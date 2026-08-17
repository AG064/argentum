# Argentum Premium Agent Harness Plan

Status: implementation baseline, native Rust rewrite in progress

Date: 2026-08-02

Branch: `UI_redesign`

Research review: 2026-08-17, including current Codex, ZCode, and the public
DeepSeek Harness architecture

## Product decision

Argentum will become a premium agent harness, not a settings dashboard.
The main product object is an active session that can plan, execute, ask for
approval, show its work, and make changes reviewable.

The primary product loop is:

```text
Project -> Session -> [Goal] -> Plan -> Run -> Approval -> Changes -> Verify
                                  ^                                  |
                                  +------ Continue or finish --------+
```

A goal is optional for short work. When present, it is a persisted session-level
contract that can drive multiple verified runs or iterations. It is not prompt
text and it cannot be completed by the model declaring success.

The desktop app is the full supervision and review workspace. The mobile app is
the compact companion for starting sessions, monitoring runs, approving actions,
and continuing conversations.

## Non-negotiable outcomes

- Premium, minimal, calm visual language based on Argentum black, gray, white,
  silver, and red.
- Task-first information architecture inspired by Codex.
- Flexible workspace surfaces inspired by Claude Code Desktop.
- Clear project, session, model, and background-work topology inspired by LM
  Studio Bionic.
- Persisted, budgeted, and evidence-verified long-running goals informed by
  ZCode, without copying ZCode's visual shell.
- Tool activity stays collapsed by default, but plans, changes, approvals, and
  verification remain easy to reach.
- Permission profiles are visible at the composer and enforced by the Rust
  capability broker. A visual mode selector never grants authority by itself.
- Desktop and mobile use the same domain model and event protocol.
- Production runtime, security, persistence, providers, orchestration, CLI,
  and UI source move to Rust.
- A small Rust composition registry makes capabilities and surfaces
  inspectable without adding a JavaScript plugin runtime.
- Availability, enablement, readiness, permission, and visibility remain
  separate facts. Optional modules can be shown, hidden, or unavailable with a
  reason, but never represented by a decorative toggle.
- No Node.js runtime is shipped with the completed application.
- No feature is called ready without an end-to-end security, error, and platform
  test.

## ZCode research decisions

ZCode validates the agent-workbench direction but does not replace Codex as the
primary interaction reference. Argentum will borrow execution and supervision
patterns, not product styling or provider positioning.

Adopt into the target contract:

- a session-level goal contract with objective, lifecycle, budgets, iteration
  count, next action, and verification history;
- automatic continuation only while the goal is active, within budget, and not
  blocked by an approval or user decision;
- completion verification based on changed files, command output, test results,
  or explicit acceptance checks, with failure treated as incomplete;
- task rows that expose running, waiting, unread, failed, and changed-file state
  without opening every session;
- searchable workspace and changed-file views, real review, an integrated
  terminal, and a command center spanning tasks, files, and actions;
- execution profiles near the composer, with `Confirm Before Changes` as the
  safe default and every profile backed by persisted runtime policy.

Defer until the underlying task, Git, search, and workspace services are real:

- custom task groups, color labels, workspace view, timeline view, sorting,
  archive automation, and pinning;
- repository wiki generation and a read-only Git graph;
- background subagents, remote control, bot channels, and cross-device goal
  continuation.

Do not adopt:

- ZCode's blurred, glowing, large-serif empty-state treatment or any other
  styling that weakens the restrained `Ag / 47` identity;
- prominent `Full Access` presentation or any permission mode that is not
  derived from the capability broker;
- automatic continuation of unanswered ordinary questions by default;
- verification that relies only on a model-authored checklist, elapsed effort,
  or a conclusive-sounding response.

## Current baseline and constraints

The active branch is a native Rust workspace with a Slint desktop UI and a
shared CLI command host. The TypeScript, Node, Tauri, and historical Android
sources are quarantined under `legacy/` and are not runtime dependencies.

The existing repository contains many optional feature modules. Their presence
does not establish product readiness. The migration must port verified behavior
behind stable contracts, not blindly translate every source directory.

Existing release and security behavior must be preserved during migration:

- default-deny policy and persisted authorization remain authoritative;
- optional modules remain disabled until configured and tested;
- secrets are never copied into UI state or logs;
- local package validation is separate from publication claims;
- startup failures remain visible and actionable;
- dynamic runtime loading is replaced with a safe, explicit mechanism.

The detailed composition, inspection, and extension contract is in
[`HARNESS_MODULARITY.md`](HARNESS_MODULARITY.md). The active implementation
borrows profiles, service seams, and durable versus live event separation from
DeepSeek Harness. It does not copy Cordis or ship an in-process JavaScript
plugin host.

## Product information architecture

### Primary navigation

The global rail contains only:

- Projects
- Sessions
- Running
- Approvals
- Search or command palette

Settings, diagnostics, providers, updates, and integrations live in secondary
menus. They must not compete with the active task.

The project and session pane is also the task queue. Each row can show one
compact state marker, relative update time, waiting-for-approval or unread state,
and changed-file counts. Search is required before custom grouping. Grouped,
workspace, and timeline views are scale features, not reasons to crowd the first
release.

### Domain model

```text
Workspace
  Project
    Session
      Goal contract (optional)
      Run or goal iteration
        Plan step
        Tool event
        Approval request
        Change set
        Verification result
```

Definitions:

- Workspace: a bounded local or remote execution context.
- Project: a durable group of related sessions, files, configuration, and
  optional branch or worktree information.
- Session: the user-facing conversation and its context history.
- Goal contract: an optional persisted objective with active, paused,
  budget-limited, or complete state; time, token, and tool budgets; iteration
  count; next action; and verification history.
- Run: one execution attempt with an immutable event stream and a resumable
  lifecycle. A run can be independent or one iteration of a goal.
- Change set: files and edits produced by a run, with review state.

### Desktop shell

Default desktop layout:

```text
┌ rail ┬ project/session list ┬ conversation + plan ┬ optional work pane ┐
└──────┴──────────────────────┴─────────────────────┴─────────────────────┘
```

The optional work pane can contain Goal, Changes, Files, Terminal, Preview,
Activity, or Approvals. Panels can be opened, closed, resized, split, and
reordered. The layout is persisted per project and restored without blocking
session startup. Goal is a summary and audit surface, not a second transcript.

### Mobile shell

Mobile opens directly into the active session or the running-task queue.
Secondary surfaces are sheets rather than permanent columns:

- session switcher;
- plan and run activity;
- approvals;
- changes and verification;
- model, workspace, and permission context.

Touch targets are at least 44 px. The mobile surface never renders a squeezed
desktop rail or a permanent inspector.

## Golden path acceptance flow

The first fully working vertical slice must support this flow:

1. Create or select a project.
2. Start a session.
3. Select environment, branch or worktree, model, and permission profile in one
   coherent composer.
4. Submit a short task or set an optional verifiable goal with explicit resource
   limits.
5. See a live plan with explicit steps.
6. Watch tool activity appear as compact, collapsible events.
7. Approve a scoped action with command, path, network, and duration visible.
8. Inspect changed files and a unified or split diff.
9. Run verification and see the exact evidence and result.
10. For an active goal, continue to the recorded next action only when
    verification says the objective is incomplete and the budget permits it.
11. Continue, accept, retry, pause, cancel, archive, or hand off the work.

No broad visual rewrite is considered successful until this path works with a
real provider and a safe local test workspace.

## Agent lifecycle

Runs use explicit states:

```text
draft -> queued -> planning -> running -> waiting_for_approval
      -> reviewing -> verifying -> complete
      -> paused | cancelled | failed
```

Every transition is persisted and emits a typed event. A user can resume a
paused or interrupted run without reconstructing state from the transcript.

Goals use a separate lifecycle:

```text
unset -> active -> complete
          |  ^
          v  |
        paused
          |
          v
    budget_limited
```

`budget_limited` can return to `active` only after an explicit budget change or
resume decision. Stopping a running task pauses its goal. Clearing a goal removes
the controller only after preserving its audit history. A goal reaches
`complete` only after an evidence-backed verification event passes and no
required plan item remains open.

## Full Rust rewrite target architecture

### Target rule

The completed product is a Rust application for Windows, macOS, Linux, Android,
and iOS. Production source code for the runtime, UI, state, persistence,
providers, security, tools, CLI, scheduler, and platform orchestration is Rust.

The following are not allowed in the shipped product:

- TypeScript or JavaScript application logic;
- a Node.js process or JavaScript sidecar;
- JavaScript plugin execution inside the Argentum process;
- a web-only UI that has no mobile and desktop capability model;
- platform-specific product behavior hidden in an untyped frontend bridge.

CSS, images, fonts, icons, localization data, generated bindings, and platform
manifests remain assets or build inputs. A platform may still require a minimal
generated Kotlin, Swift, Objective-C, or Java wrapper for OS registration. Such
wrappers contain no Argentum product logic and call into the shared Rust core.

### Platform and UI decision

The production renderer is native Slint. Tauri, Wry, WebView, Dioxus, and Iced
are not part of the shipped application.

```text
Rust domain and runtime
  -> Rust UI projection and commands
  -> Slint declarative markup compiled into Rust
  -> native platform backend and input layer
```

Slint was selected because the release target requires native desktop and
mobile rendering, Rust integration, accessibility, responsive layout, and a
single shared component contract. All application behavior remains Rust. The
`.slint` files are view markup, not an application runtime or scripting layer.

The first native shell is implemented under `crates/argentum-ui` and `ui/`.
Future rich text, diff, terminal, and mobile surfaces must use the same typed
command and event protocol rather than creating a second bridge.

### Cargo workspace shape

```text
Cargo.toml
crates/
  argentum-domain/       projects, sessions, runs, events, errors
  argentum-runtime/      agent loop, cancellation, streaming, orchestration
  argentum-providers/    provider traits and HTTP/SSE adapters
  argentum-tools/        typed registry, execution, limits, result capture
  argentum-security/     policy, capabilities, approvals, audit records
  argentum-store/        SQLite, migrations, projections, search
  argentum-workspaces/   roots, branches, worktrees, file boundaries
  argentum-scheduler/    background work and missed-run policy
  argentum-ui/           Rust components, theme tokens, view projections
  argentum-platform/     window, clipboard, notifications, file and mobile APIs
legacy/src/desktop/      archived Tauri host and old desktop bridge
platforms/               target packaging and thin OS adapters
```

The host or native window crate must be a thin adapter. It owns window creation,
platform permissions, native dialogs, update hooks, notifications, and mobile
entry points. It must not contain the agent loop or business rules.

### Platform parity

All five targets share the same Rust domain, event protocol, security policy,
storage schema, provider interfaces, and UI component contracts:

| Platform | Primary product role | Required Rust parity |
| --- | --- | --- |
| Windows | full harness and local execution | complete |
| macOS | full harness and local execution | complete |
| Linux | full harness and local execution | complete |
| Android | mobile supervision and remote or bounded local work | complete companion |
| iOS | mobile supervision and remote or bounded local work | complete companion |

Mobile does not mean a shrunken desktop build. It uses the same Rust events and
permissions but a mobile-specific surface model with sheets, queues,
notifications, approvals, and session continuation.

### State and IPC contract

```text
UI command -> typed command bus -> Rust domain service
Rust domain event -> event store -> projection -> UI event stream
```

Rules:

- UI code cannot call providers, the filesystem, subprocesses, or databases
  directly.
- Commands and events use versioned Rust types with `serde` DTOs.
- Provider streams use bounded channels and cancellation tokens.
- Large files and binary data use streamed responses or asset handles, not
  base64 JSON copies.
- Errors are typed, redacted, actionable, and safe to display.
- The event stream is the source of truth for run activity; transcript text is a
  projection, not the execution log.
- Goal state, budgets, iterations, next action, and verification evidence are
  persisted domain records. The transcript and UI cannot author or widen them.
- Completion verification consumes typed evidence from change sets, commands,
  tests, and acceptance checks. Missing, stale, cancelled, or failed evidence
  cannot produce a complete goal.
- Execution profiles resolve to explicit capability policy. Switching a visual
  profile cannot bypass an approval already required by policy.

Current 0.1.0 implementation boundary: `Read Only`, `Confirm Before Changes`,
and exact custom enablement are persisted per project for `read_text` and
`write_text`. The runtime checks the resolved policy before model advertisement,
manual requests, approval resume, and execution. Command, network,
external-process, full-access, timeout, and session-grant profiles remain
target-state work.

### Rust migration order

1. Create the Cargo workspace and domain event model.
2. Port configuration, error types, logging, IDs, and lifecycle state.
3. Port the security policy, capability broker, approvals, and audit records.
4. Port SQLite persistence, migrations, search, and session projections.
5. Port provider traits and streaming adapters for OpenAI-compatible, Anthropic,
   LM Studio, local llama.cpp, and custom endpoints.
6. Port the agent loop, goal controller, tool registry, cancellation, budgets,
   and checkpoints.
7. Port workspace boundaries, file operations, diff generation, terminal limits,
   and verification runners.
8. Port the Rust CLI and background scheduler.
9. Build the renderer spike and Rust UI vertical slice on the stable command and
   event protocol.
10. Extend the native Slint UI one capability at a time, keeping unverified
    modules disabled.
11. Remove the shipped Node runtime, JavaScript sidecar, and dynamic JavaScript
    plugin loading.

### Plugin and extension policy

The current dynamic JavaScript plugin model must not be reproduced inside the
Rust process. New extensions use one of these boundaries:

- built-in Rust crate with an explicit capability declaration;
- signed or integrity-recorded WASM/WASI module with a restricted host API;
- isolated sidecar process with a versioned protocol and resource limits.

An extension cannot gain authority by returning a wider permission request than
the persisted policy allows.

## Performance plan

“Fast as lightning” is measured separately from provider latency. The app must
feel instant even while a model or tool is slow.

### Budgets for the first release target

- interactive shell after process start: under 1.5 seconds on the reference
  Windows machine;
- first usable frame: under 250 ms after the window is created;
- idle memory excluding a model server: under 150 MB;
- no UI task blocks the main thread for more than 50 ms;
- command round trip for a small local query: p95 under 10 ms;
- restore a session with 2,000 messages: under 300 ms to first visible content;
- maintain 60 FPS while appending 1,000 activity events;
- stream token updates in coalesced batches rather than one render per token.

Budgets are gates to measure, not claims to make before profiling.

### Implementation rules

- Tokio for asynchronous I/O and structured cancellation.
- `spawn_blocking` or dedicated workers for SQLite, diff parsing, indexing, and
  other blocking work.
- SQLite WAL mode, prepared statements, migrations, FTS5 indexes, and bounded
  transactions.
- Virtualized transcript, activity, file, and diff lists.
- Lazy-load providers, model catalogs, diagnostics, and optional feature data.
- Persist projections incrementally instead of rebuilding all state on startup.
- Use bounded queues and backpressure for model streams and tool output.
- Cap subprocess output, duration, memory, and concurrency.
- Keep the main window free of background polling loops when an event stream is
  available.
- Build release binaries with LTO, stripped symbols, one codegen unit where
  measured beneficial, and platform-appropriate panic settings.
- Remove the current packaging warning around dynamic `require('s')` before
  calling the Rust-first build release-ready.

## Design system implementation gates

The complete token and component contract lives in
`docs/ARGENTUM_DESIGN_SYSTEM.md`.

The UI implementation must provide:

- one token source for color, type, spacing, motion, radii, and elevation;
- stateful components with keyboard, screen reader, touch, and reduced-motion
  behavior defined before styling;
- persisted pane layouts and responsive breakpoints;
- no hard-coded status colors outside semantic tokens;
- no fake readiness or success states;
- visual regression captures at 1440, 1280, 1024, 768, 430, and 360 px;
- real task fixtures with loading, running, approval, failure, empty, and
  completed states.
- goal fixtures for active, paused, budget-limited, verification-failed, and
  evidence-complete states;
- task rows with waiting, unread, failure, change-count, archive, and search
  states;
- permission-profile fixtures that prove visible labels match effective Rust
  capability policy;
- changed-file, diff, terminal, and verification-evidence fixtures using real
  bounded data rather than summary placeholders.

## Delivery phases

### Phase 0: Product and architecture lock

- approve the domain model and golden path;
- freeze the current shell as a reference only;
- verify the native Slint shell against the desktop and mobile fixture;
- establish reference hardware, performance harness, and fixture data;
- record the security and packaging migration boundaries.

Exit: one selected Rust UI path, measured baseline, and no open ambiguity about
whether Node is part of the shipped runtime.

### Phase 1: Rust foundation

- create the Cargo workspace;
- implement domain types, event bus, error model, config, logging, and IDs;
- implement persisted goal contracts, goal lifecycle events, budgets, and
  iteration linkage;
- port security policy and approval semantics;
- add migrations and a session/run store;
- expose typed Slint commands and Rust events.

Exit: a Rust test can create a project, session, optional goal, linked run,
approval, change set, and verification record without any JavaScript or Node
process.

### Phase 2: End-to-end vertical slice

- implement one provider adapter;
- implement a safe file read and bounded command tool;
- implement live plan, tool event, approval, diff, and verification projections;
- prove that failed or missing evidence cannot complete a goal;
- ship the first Rust UI surface for the golden path.

Exit: a real local test workspace can complete the golden path with no demo
success data.

### Phase 3: Harness workspace

- add project and session navigation;
- add flexible panes and layout persistence;
- add terminal, files, changes, preview, activity, and approval surfaces;
- add goal summary, iteration history, resource usage, and pause or resume
  controls;
- add task search plus running, waiting, unread, failed, and change-count row
  states;
- add the permission-aware command center and execution-profile selector;
- add worktree or branch metadata where supported;
- add background runs, resume, cancellation, retry, and archive.

Only after these services are stable, add grouped, workspace, and timeline task
views, changed-files-only filtering, repository orientation, and a read-only Git
graph.

Exit: desktop supervision and review are faster and clearer than the current
chat-only shell.

### Phase 4: Provider and extension migration

- migrate local and hosted providers;
- migrate model discovery and provider capability probes;
- migrate scheduler and verified optional features;
- replace JavaScript plugins with Rust/WASM/sidecar boundaries;
- port the CLI and gateway behavior.

Exit: all supported production paths run without Node.

### Phase 5: Mobile companion

- implement active session, running queue, approvals, notifications, and
  compact changes/verification sheets;
- use mobile-specific capability and permission profiles;
- test Android and iOS builds against the same event protocol.

Exit: mobile is useful for supervision and continuation, not just visually
responsive.

### Phase 6: Release hardening

- run Rust format, lint, unit, integration, security, and dependency gates;
- run desktop and mobile UI regression suites;
- profile startup, memory, event throughput, and long-running sessions;
- verify clean installers and runtime behavior;
- investigate every bundler warning, including dynamic loader warnings;
- publish only after exact artifact and branch verification.

## Definition of done

The redesign is complete only when:

- the primary screen is a task workspace, not a configuration dashboard;
- users can see the plan, run state, approvals, changes, and verification;
- users can set, inspect, pause, resume, replace, or clear a persisted goal and
  see its budget, iterations, next action, and verification history;
- no goal completes without current evidence and no automatic continuation runs
  past its configured budget or a pending approval;
- task rows expose actionable state without requiring every session to be
  opened;
- panels are modular on desktop and sheets are intentional on mobile;
- the design system is implemented from shared tokens;
- the shipped runtime is Rust-based and no Node process is required;
- security boundaries are enforced in Rust and survive UI restarts;
- the measured performance budgets pass on reference hardware;
- Windows, Android, and supported additional targets pass their own real build
  and runtime checks;
- no placeholder data, fake readiness, or unsupported capability is presented as
  complete.

## Reference inputs

- [Codex app overview](https://openai.com/index/introducing-the-codex-app/)
- [Codex environments](https://learn.chatgpt.com/docs/environments/modes)
- [Codex worktrees](https://learn.chatgpt.com/docs/environments/git-worktrees)
- [Codex integrated terminal](https://learn.chatgpt.com/docs/integrated-terminal)
- [Codex code review](https://learn.chatgpt.com/docs/code-review)
- [ZCode task and file management](https://zcode.z.ai/en/docs/task-management)
- [ZCode Goal Mode](https://zcode.z.ai/en/docs/goal)
- [ZCode safety confirmation](https://zcode.z.ai/en/docs/safety-confirm)
- [ZCode ADE tools](https://zcode.z.ai/en/docs/ADE-tools)
- [LM Studio Bionic](https://lmstudio.ai/docs/bionic)
- [LM Studio Projects and Sessions](https://lmstudio.ai/docs/bionic/projects-and-sessions)
- [Claude Code Desktop](https://code.claude.com/docs/en/desktop)
- [Slint Rust integration](https://docs.slint.dev/latest/docs/rust/slint/)
- [Slint accessibility](https://docs.slint.dev/latest/docs/slint/reference/common/)
