# Feature Status

This document distinguishes source presence from supported behavior. The
`src/features` tree currently contains 73 module directories; that count is not
a claim that 73 production-ready features exist. A capability is release-ready
only when its configuration, lifecycle, permission boundary, error state, and
tests are wired end to end.

All non-core capabilities must remain optional and disabled by default.

## v0.0.9 functionality

| Area                         | Status                  | Boundary                                                                                                                                                             |
| ---------------------------- | ----------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Desktop onboarding/settings  | Candidate               | Persists one workspace configuration; desktop secrets currently use local `secrets.env`.                                                                             |
| Hosted providers             | Candidate/testing       | OpenAI, Anthropic, Google, OpenRouter, NVIDIA, Groq, and MiniMax are configurable. Availability still depends on credentials, account, endpoint, and selected model. |
| Local providers              | Candidate               | LM Studio/OpenAI-compatible, managed llama.cpp, Ollama, and custom OpenAI/Anthropic-compatible endpoints are selectable.                                             |
| Provider test                | Candidate               | Calls the provider model catalog/endpoint; it cannot prove every chat/tool capability.                                                                               |
| Desktop chat                 | Candidate               | OpenAI-compatible, Anthropic-compatible, Codex browser-account, attachments where supported, streaming for local OpenAI-compatible endpoints.                        |
| AI context limits            | Candidate               | UI estimate/compaction plus Rust message/history/context-budget enforcement. Provider tokenization remains approximate.                                              |
| AI tool security             | Candidate               | Persisted policy is authoritative; default-deny allowlist is checked at exposure and execution. Privileged model tools require `trusted`.                            |
| llama.cpp                    | Candidate               | Install/preparation, localhost start/stop/status/logs, GGUF file or Hub download, common server tuning, Hub search, and bounded local model scan.                    |
| Hugging Face picker          | Candidate               | Curated GGUF presets plus live Hub GGUF search. Each model's license/gating must be reviewed separately.                                                             |
| Gateway controls             | Candidate               | Fixed desktop actions for start/stop/status/logs; arbitrary shell is not exposed to the model.                                                                       |
| System Dashboard desktop tab | Candidate               | In-app, read-only local system telemetry while selected/allowed.                                                                                                     |
| Standalone web dashboard     | Experimental/disabled   | Separate network surface. Placeholder API success data was removed; unconnected modules return unavailable.                                                          |
| Update check                 | Candidate               | Numeric version check against GitHub Releases and browser handoff. No in-place install/rollback yet.                                                                 |
| Help/feedback/FAQ            | Candidate               | Context help, curatable FAQ, and GitHub issue/feature links without workspace path disclosure.                                                                       |
| OpenClaw migration           | Candidate               | Imports allowlisted content; conflicts are preserved and untrusted content is not auto-activated.                                                                    |
| Android client               | Pre-release test target | Native client exists and can build when Android tooling is available. A persistent release signing key is required. Parity and sync are incomplete.                  |

“Candidate” means the implementation is wired and covered by available tests,
not that every provider/account/platform combination has been live-tested.

## Optional backend modules

Feature modules are discovered through the plugin loader and activated only when
their `features.<name>.enabled` configuration is true. Important examples:

- security/audit: allowlists, audit-log, content-filtering, secure-profile,
  rate-limiting, container-sandbox, tenant-isolation;
- memory/knowledge: sqlite-memory, markdown-memory, semantic-search,
  knowledge-graph, consolidation, self-evolving-memory;
- automation: cron-scheduler, browser-automation, computer-control, webhooks,
  file-watcher, mesh-workflows;
- communication: Telegram, Discord, webchat, WhatsApp bridge, Slack, SMS,
  mobile push, email/calendar;
- orchestration: goals, task checkout, multi-agent coordination, role access,
  governance, health monitoring, budget.

Some modules are prototypes or backend-only and do not have supported desktop
controls. Their directory name alone is not an availability promise. When a
dependency, credential, or adapter is missing, the module must remain disabled
or report unavailable—never return demo success data.

## Planned release work

### v0.1.0

- signed platform updater foundation and verified release metadata;
- Android internal-testing readiness and persistent signing/release runbook;
- explicit approval UX for `ask`/`session` AI tool grants;
- provider capability probes and broader live-provider test matrix;
- permissions-first MCP/skill/plugin import with license and integrity records;
- modular browser/computer use adapters, off by default;
- supported scheduled-task UI after the core safety gates are complete.

### v0.1.1

- opt-in end-to-end encrypted device pairing and workspace/chat/file sync;
- independent sync scopes for projects, workspaces, chats, and files;
- conflict handling, revocation, key rotation, audit history, and recovery;
- platform coverage for Windows, Linux, macOS, and Android.

See [ROADMAP.md](../ROADMAP.md),
[AI providers and extensions](AI_PROVIDERS_AND_EXTENSIONS.md), and
[update architecture](UPDATE_ARCHITECTURE.md) for design gates and limitations.
