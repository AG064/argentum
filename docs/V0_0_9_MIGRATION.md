# Argentum 0.0.9 to 0.1.0 migration matrix

Status: working product boundary for the 0.1.0 draft

The legacy source tree is evidence, not a parity checklist. A feature is treated
as supported only when configuration, lifecycle, permission handling, error
states, and tests work end to end in the active Rust product.

## Status vocabulary

- **Migrated:** implemented through the active command host and covered by
  focused validation.
- **Partial:** useful infrastructure exists, but the complete user workflow is
  not ready.
- **Planned:** evidenced 0.0.9 behavior selected for a later 0.1.x slice.
- **Experimental:** legacy implementation or test coverage existed, but it was
  not part of the reliable desktop release core.
- **Not carried:** placeholder, misleading, or out of scope for the native
  draft.

## Core matrix

| 0.0.9 capability | 0.1.0 draft state | Active direction |
| --- | --- | --- |
| Desktop chat and streamed responses | Partial | Selected OpenAI-compatible profiles stream through `CommandHost`, preserve bounded ordered multi-turn context, and restore prior turns. Tagged and explicit provider reasoning is separated from answer text and shown in a collapsed disclosure. Exact reported usage and known context limits persist with assistant turns. Attachments remain pending. |
| Workspace onboarding and settings | Partial | Desktop accepts `ARGENTUM_WORKSPACE`, loads the persisted native workspace selection, migrates the saved v0.0.9 workspace pointer when present, and otherwise uses the launch directory. Durable provider settings are active, but a native workspace picker and first-run flow remain pending. |
| Sessions and conversation history | Migrated | User and assistant messages are durable, project and session scoped, ordered, restored after restart, and sent back as bounded model context. One active run is allowed per session. In-progress process recovery remains separate release-hardening work. |
| Provider test and model catalog | Partial | Workspace-scoped profiles, durable atomic selection, selected-provider routing, bounded non-billable probes, bounded model catalogs, and exact-profile model selection are migrated. OpenAI, MiniMax, DeepSeek, generic OpenAI-compatible, and LM Studio catalogs use one command boundary. Exact-profile credentials load from the operating-system keyring or bounded compatibility sources. Native credential entry, automatic legacy credential migration, and GGUF filesystem discovery remain pending. |
| Model-driven tools | Migrated | OpenAI-compatible providers can request the bounded built-in `read_text` and `write_text` tools. Results return through a capped multi-round loop. Writes pause for explicit approval, rejection returns a bounded result, and cancellation removes pending work. |
| File permission policy and approvals | Migrated | Workspace path containment, read policy, write approval, approve, and reject paths exist. Session-scoped grants and restart recovery remain pending. |
| Local LM Studio path | Partial | An already-running OpenAI-compatible endpoint is supported. Managed model download and server lifecycle are not. |
| OpenAI-compatible and Anthropic providers | Partial | Local, generic OpenAI-compatible, OpenAI, MiniMax, and DeepSeek profile IDs can be saved, selected, probed, and used for streaming when their exact credentials are configured. Anthropic remains unavailable without a dedicated configured credential path. |
| Local model discovery and GGUF picker | Planned | A bounded scanner can be migrated before any managed binary lifecycle. |
| OpenClaw import | Planned | Import must preserve unknown IDs and conflicts and must never overwrite settings silently. |
| Update check | Planned | A signed metadata check and browser handoff may return. In-place apply and rollback need a separate secure updater design. |
| Startup diagnostics | Partial | Windows startup failures use a safe native error dialog, command failures reach the UI, and event-lag recovery republishes current state. A richer copy and report workflow remains pending. |
| Localization foundation | Planned | Migrate after visible product copy stabilizes. Partial translation must stay labeled partial. |
| Built-in skills catalog | Planned | Only reviewed built-ins with explicit capabilities can enter the Rust registry. External installation remains gated. |
| Optional module visibility and profiles | Partial | The Rust harness registry lists available and unavailable capabilities with reasons. Focused, Standard, Review, Trace, Full, and Custom presentation profiles persist Activity and Changes visibility. Execution capability toggles, signed external modules, and isolated extension hosting remain pending. |
| Android companion | Planned | The historical Kotlin app remains archival. No active mobile artifact is claimed by the Rust draft. |

## Experimental legacy areas

The following source areas do not define 0.1.0 parity:

- router-agent decision logging without real isolated dispatch;
- knowledge-graph consolidation groundwork;
- standalone dashboard fixtures;
- Discord, Slack, and WhatsApp connection stubs;
- broad optional feature directories that were disabled by default;
- MCP, mesh, browser, computer-control, scheduler, and container integrations
  without a complete permission and lifecycle path in the active host;
- placeholder Android agent data.

## Migration order

1. Stable workspace, project, session, task, and run ownership.
2. Provider profiles, safe probes, explicit selection, exact-profile
   credentials, and user-managed operating-system keyring storage. Native
   credential entry and automatic legacy migration remain pending.
3. Persisted messages, separated reasoning, exact reported usage, and bounded
   history context. This slice is active.
4. Provider tool-call parsing through the existing approval boundary. This
   slice is active for `read_text` and `write_text`.
5. Provider-scoped model catalogs and durable model selection. This slice is
   active for OpenAI-compatible and LM Studio providers.
6. Real file-change records, diffs, review, restore, and verification.
7. Import, update, localization, and local-model lifecycle features.
8. Reviewed extensions and companion clients only after the same safety contract
   can be preserved.

The active parity target also covers verified v0.0.8 workflows. Its broad
release notes are discovery evidence, while v0.0.9 tests and release notes are
the stronger readiness boundary. Optional modules remain disabled until their
configuration, lifecycle, permission, failure, and test paths work end to end.
See [the modular harness contract](HARNESS_MODULARITY.md).

## Verified configured-provider path

The 0.1.0 draft can reuse the configured v0.0.9 MiniMax credential without
copying it into a profile, event, log, or SQLite record. The compatibility path
reads only `MINIMAX_API_KEY` from the selected legacy workspace and keeps it in
memory for the exact `minimax` profile ID. A bounded smoke script uses a
temporary database, verifies the models endpoint and one streamed response,
then removes its temporary state:

```powershell
.\scripts\test-configured-minimax.ps1
```

That script sends one short billable model request. OpenAI and DeepSeek use the
same exact-profile boundary when `OPENAI_API_KEY` or `DEEPSEEK_API_KEY` is
configured, but neither provider is claimed as live-tested by this draft.

## Identity migration

The canonical v0.0.9 master is
`legacy/assets/brand/argentum.png`, SHA-256
`9d492768039dc93dd6c74bbf1bb15092d02e1fda5b68c946d86ff4ff3466b0e3`.
The active product preserves this source unchanged and derives deterministic UI
and platform sizes from it. The A/G topology must not be recreated with text,
recolored as a generic system icon, or replaced with a new generated logo.
