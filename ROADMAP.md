# Roadmap

Roadmap items are plans, not shipped-feature claims. Dates are intentionally not
promised before release gates pass.

## Current status

- v0.0.9 is a release candidate on `development`.
- v0.0.7 is the latest published release as of this review.
- Desktop Windows packaging has been locally validated; other release platforms
  and hosted-provider combinations still require their CI/live test matrices.

## v0.0.9 release gate

- provider setup/test/live-chat paths for candidate providers;
- managed llama.cpp settings, curated Hub presets, Hub GGUF search, and bounded
  local model scan with no fictitious Argentum model;
- default-deny AI context/tool enforcement in the Rust bridge;
- server-side message/history/context limits and honest provider quota display;
- all optional features/listeners disabled by default;
- help, curatable FAQ, privacy-preserving bug/feature reports;
- GitHub update check with browser handoff only;
- truthful feature/security/release/contribution documentation;
- persistent Android signing identity configured before any release APK is
  published;
- all pre-push, CI, Rust, packaging, and dependency gates green.

## v0.1.0

- Android internal-testing client: authenticated gateway connection, core chat,
  settings, safe update/install documentation, and repeatable device tests;
- signed Tauri update artifacts and verified updater manifest foundation;
- interactive per-action approval for `ask` and `session` model tools;
- provider capability probes and a maintained provider/model test matrix;
- permissions/license/integrity manifest for skills, plugins, hooks, MCP, LSP,
  agents, and monitors;
- read-only skills/MCP first; browser/computer adapters only after approval and
  prompt-injection tests;
- dashboard backend adapters connected to real optional modules, with each module
  independently enabled; unconnected APIs remain unavailable;
- supported scheduled-task UI after task ownership, permission, missed-run,
  concurrency, cancellation, and audit semantics are verified;
- localization coverage and Android accessibility checks.

## v0.1.1

Secure, opt-in device sync across Windows, Linux, macOS, and Android:

- device pairing with end-to-end encrypted transport and explicit trust;
- independent scopes for project files, workspaces, chats, and settings;
- local-first operation, conflict copies/merge strategy, resumable transfer;
- device revocation, key rotation, recovery, audit history, and storage quotas;
- no provider API keys or signing keys synced by default;
- user-selectable self-hosted relay/direct transport where practical.

## Later

- componentized signed updates for models, sidecars, skills/plugins, and UI assets;
- background staging while AI work continues, followed by graceful checkpoint,
  drain, verified restart, health check, and recovery;
- Tauri v3 evaluation after stable release and dependency/security review;
- permission-gated recurring automations and cross-device execution ownership;
- broader messaging/provider/platform coverage driven by tested demand.

## Non-negotiable release principles

- functionality before visual claims;
- optional modules disabled until configured;
- no demo data or placeholder success in production paths;
- persisted local policy—not model output or webview state—authorizes actions;
- commercial/license review per imported component and model;
- no update, sync, provider, or platform is called ready without an end-to-end
  test on that target.

Detailed plans: [feature status](docs/FEATURES.md),
[AI providers/extensions](docs/AI_PROVIDERS_AND_EXTENSIONS.md),
[update architecture](docs/UPDATE_ARCHITECTURE.md), and
[Android build guide](docs/ANDROID_BUILD.md).
