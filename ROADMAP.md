# Argentum roadmap

Argentum is being rebuilt as a native Rust agent harness. Dates are not release
claims. Each stage requires a real build, runtime, security, and visual check.

## Implemented foundation

- Old application source and tooling quarantined under `legacy/`.
- Cargo workspace with domain, runtime, provider, tool, security, storage,
  workspace, platform, scheduler, and UI crates.
- Native Slint shell with Argentum black, graphite, silver, white, and red
  tokens.
- Responsive desktop rail, session pane, task composer, plan surface, mobile
  drawer, work pane, and activity drawer.
- Typed command and event flow with SQLite event persistence.
- Stable workspace and project identity with durable sessions, safe selection,
  and CLI session listing.
- Non-billable, bounded connectivity probes for OpenAI-compatible and LM Studio
  providers.
- Workspace-scoped provider profiles with safe endpoint validation, durable
  selection, CLI management, native settings editing, and selected-provider
  task routing.
- Secure exact-profile credentials through the operating-system keyring and
  environment compatibility path, without secret fields in SQLite or events.
- Durable restored conversation history, separated provider reasoning, exact
  reported usage, bounded context, and one active run per session.
- Bounded model-driven read and write tools through the approval boundary.
- Provider-scoped model catalogs and durable model selection.
- Workspace path validation, default-deny write policy, secret redaction, and
  provider boundaries.
- LM Studio-compatible, OpenAI-compatible, and Anthropic provider foundations.
- Canonical Argentum A/G identity in the native UI and Windows executable.

## Next vertical slices

- An inspectable Rust harness registry with named presentation profiles,
  truthful capability availability, and explicit surface visibility.
- Native first-run workspace and credential setup without exposing secrets to
  Slint state.
- Resumable in-progress run state on top of durable sessions.
- Approval policy completion for command, network, and external-process actions.
- File changes, diff review, restore actions, and verification runners.
- Optional persisted goal contracts with explicit lifecycle, budgets, iteration
  linkage, next action, and verification history.
- Evidence-driven goal completion that fails closed when required checks are
  missing, stale, or failed.
- Task rows with running, waiting, unread, failed, and changed-file state plus
  search, pin, and archive foundations.
- An effective execution-profile selector backed by the Rust capability broker,
  with `Confirm Before Changes` as the default.
- Platform secure-store verification for macOS, Linux, Android, and iOS.

## Harness workspace

- Named Focused, Standard, Review, Trace, Full, and Custom presentation
  profiles, with visibility independent from execution authority.
- An optional trajectory and context inspector derived from real session,
  provider, tool, approval, usage, change, and verification records.
- Real Changes, Files, Terminal, Preview, Activity, Approval, and Goal surfaces
  driven by runtime events rather than production fixtures.
- Exact local, remote, branch, and worktree binding shown at the composer and
  run boundary.
- A permission-aware command center for commands, tasks, and files.
- Automatic goal continuation only while the goal is active, within budget, and
  not blocked by an approval or user decision.
- Grouped, workspace, and timeline task views after search, pin, archive, and
  state markers are reliable.
- Changed-only navigation, repository orientation, and Git graph after the core
  execution and review loop is complete.

## Platform delivery

- Windows native release binary and installer.
- macOS application bundle and signed disk image.
- Linux packages and portable artifact.
- Android companion build with safe-area, notification, and approval flows.
- iOS companion build with the same Rust domain and event protocol.

## Release gates

- `cargo fmt --check`
- `cargo check --workspace`
- `cargo test --workspace`
- `cargo clippy --workspace --all-targets -- -D warnings`
- dependency license and advisory checks;
- visual checks at desktop, tablet, 430 pixel, and 360 pixel widths;
- actual packaged executable launch from a clean staging directory;
- recorded artifact hashes and platform-specific runtime evidence.

## Product standard

Argentum is not ready when the shell merely looks polished. It is ready when a
real task can be planned, executed within explicit authority, reviewed, verified,
cancelled, or resumed without fake success states or hidden permission changes.
A goal is complete only when current evidence passes and no required work stays
open. Timeouts never count as permission, plan approval, or ordinary user
consent.

The composition and extension contract is documented in
[`docs/HARNESS_MODULARITY.md`](docs/HARNESS_MODULARITY.md).
