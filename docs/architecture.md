# Argentum native architecture

The active Argentum application is a Rust workspace. The old TypeScript,
Node, Tauri, and Android application remains under `legacy/` and is not a
runtime dependency.

```text
CLI command or Slint view
  -> AppCommand
  -> argentum-cli CommandHost
  -> argentum-harness composition registry
  -> argentum-runtime RuntimeService
  -> providers, tools, workspaces, security, store
  -> AppEvent
  -> CLI stream or UI projection and SQLite event log
```

## Harness composition boundary

`argentum-harness` is the small composition layer between the command host and
runtime services. It owns stable capability and surface declarations, built-in
presentation and execution profiles, dependency metadata, policy resolution,
and truthful snapshot projection. It does not execute a model request, tool,
database query, or UI action.

Availability, enablement, readiness, authority, and surface visibility are
separate states. The current bounded execution slice controls `read_text` and
`write_text`. Policy is project scoped and enforced when model tools are
advertised, when manual tool requests are accepted, when approvals resume, and
immediately before tool execution. A presentation profile cannot change
execution authority. See [the modular harness contract](HARNESS_MODULARITY.md).

## CLI-first boundary

`argentum-cli` owns the shared product entry point. It provides three access
paths over one command host, plus focused inspection commands:

- `run` submits one task and streams its result;
- `status` reports the workspace and configured providers;
- `serve` keeps the runtime alive and exchanges versioned JSONL messages over
  standard input and output.
- `sessions` lists the durable sessions scoped to the selected workspace.
- `session select` changes the active session through `AppCommand`.
- `provider probe` performs a bounded, non-billable connectivity check through
  the same provider registry used for task execution.
- `provider list`, `provider save`, and `provider select` manage durable,
  workspace-scoped provider profiles through the same command host.
- `provider models` returns a bounded catalog for one exact profile.
- `provider model` persists a model on one exact profile without changing the
  selected provider profile.
- `workspace status` and `workspace set PATH` inspect and persist the canonical
  workspace used by the desktop host.
- `harness status`, `harness profile PROFILE_ID`, and `harness surface SURFACE
  show|hide` inspect and change the persisted presentation composition without
  changing execution authority.
- `harness execution PROFILE_ID` selects `read-only` or
  `confirm-before-changes`. `harness capability CAPABILITY_ID enable|disable`
  creates an exact custom policy for configurable built-ins. Policy changes are
  rejected while a project run or approval is active.
- `harness trajectory [SESSION_ID]` loads a bounded, durable, session-scoped
  record projection through `AppCommand::LoadTrajectory`. The native desktop
  and phone layouts use the same command and event contract. Open, refresh,
  restart, and lag recovery use a bounded snapshot. Newly persisted facts use
  one small ordered entry event, so tool-heavy runs do not rebuild the full
  trajectory after every record.
- `provider credential set PROFILE_ID` reads one credential from standard input
  and stores it in the operating-system keyring. `provider credential clear`
  removes it. Credential values never cross the typed command protocol.

The native app uses `InProcessClient`, which dispatches the same `AppCommand`
values to the same `CommandHost`. This path does not spawn a CLI process and
does not serialize commands, so the architectural boundary does not add
transport overhead to native interaction. JSONL is reserved for process and
automation boundaries.

The serve protocol bounds request size, command concurrency, and response
buffering. Commands run concurrently so a long provider stream cannot prevent
cancellation or approval commands from being accepted.

### Durable workspace state

The store resolves a stable project from the canonical workspace root and
creates one initial session when needed. Projects and sessions are normalized
SQLite records. Every durable event records payload version, workspace,
project, session, and run scope where available. A new session and its
`SessionCreated` event commit in one transaction before publication.

`WorkspaceStateLoaded` is a transient projection snapshot. Publishing initial
state or selecting a session does not append replay-like events to the log.
Clients use that snapshot to render the ordered session index and current
selection. Raw event replay never recreates approvals or reruns tools.

Provider profiles are normalized project-scoped SQLite records. Each profile
contains a stable ID, label, usable provider kind, safe endpoint, and model.
Exactly one profile is selected for a project. Save and select operations are
atomic, selection survives restart, and the runtime resolves the selected
profile for every submitted task. Endpoints must use HTTP or HTTPS with a host
and cannot contain URL credentials, a query, or a fragment. Profile records do
not contain API keys or other secret fields.

Hosted credentials are resolved separately from the profile record. The host
loads only the exact canonical profile IDs `openai`, `minimax`, and `deepseek`
from the operating-system keyring or matching environment variables. The
runtime keeps the credential in a redacted in-memory provider map, and sends it
only after the provider endpoint has passed the canonical-origin policy. Secure
store failures are reported without including credential values. Generic
OpenAI-compatible profiles remain unauthenticated unless a provider-specific
integration is registered.

Conversation messages persist visible text, separately typed reasoning, exact
reported usage, provider profile ID, and model ID. Stored usage is rejected if
its arithmetic, cached-token bounds, reasoning-token bounds, or context limit
is inconsistent. Normal multi-turn history sends only visible user and
assistant text. Prior reasoning is never inserted into a later user turn.

### Provider probes

`ProbeProvider` is an additive protocol-v1 command. OpenAI-compatible and LM
Studio providers request the safely joined models endpoint with redirects
disabled, a three-second timeout, a 256 KiB response limit, and bounded JSON
validation. Status URLs are stripped of embedded credentials, query strings,
and fragments before publication. Anthropic reports that a safe non-billable
probe is unavailable instead of sending a model message.

`ListProviderModels` and `SelectProviderModel` are additive protocol-v1
commands. Catalogs are bounded, sorted, deduplicated, and scoped to the exact
saved profile and credential. LM Studio prefers its native model catalog so a
loaded instance can report its configured context length, then falls back to
the OpenAI-compatible endpoint. Canonical hosted context limits are applied
only to known model IDs. A provider-reported limit is preserved when no
canonical value exists.

Provider streams emit answer deltas, reasoning deltas, tool calls, exact usage,
and one completion event. Raw or fragmented `<think>` markers are removed from
the answer stream and their content is emitted as reasoning. Usage is never
estimated or summed across tool rounds. The final provider round either stores
its own usage or records that usage was not reported.

OpenAI-compatible reasoning fields are normalized before they reach the
runtime. `reasoning_content` is treated as a delta. `reasoning_details` accepts
both the cumulative snapshots shown in provider examples and the incremental
chunks returned by current MiniMax streams. The adapter detects the stream
shape, keeps the combined reasoning under its byte limit, and never copies raw
reasoning markers into answer text.

### Tool requests and approvals

Tools cross the same boundary as every other product action. A protocol v1
read request has this exact envelope shape:

```json
{"protocol_version":1,"request_id":"read-1","type":"command","command":{"kind":"request_tool","request":{"call_id":"00000000-0000-0000-0000-000000000001","run_id":"00000000-0000-0000-0000-000000000002","input":{"kind":"read_text","path":"README.md"}}}}
```

A write request uses the same command with a typed `write_text` input:

```json
{"protocol_version":1,"request_id":"write-1","type":"command","command":{"kind":"request_tool","request":{"call_id":"00000000-0000-0000-0000-000000000003","run_id":"00000000-0000-0000-0000-000000000002","input":{"kind":"write_text","path":"notes.txt","content":"reviewed"}}}}
```

Reads are allowed by the default host policy. Writes emit an
`ApprovalRequested` event and do not touch the file until the client sends an
approval command using the returned approval ID:

```json
{"protocol_version":1,"request_id":"approve-1","type":"command","command":{"kind":"approve_tool","approval_id":"00000000-0000-0000-0000-000000000004","scope":"Once"}}
```

The same pending request can be rejected without executing it:

```json
{"protocol_version":1,"request_id":"reject-1","type":"command","command":{"kind":"reject_tool","approval_id":"00000000-0000-0000-0000-000000000004"}}
```

Every file path is validated against the configured workspace root before the
tool can read or write it.

## Crate boundaries

- `argentum-domain` contains serializable commands, events, lifecycle states,
  layout profiles, and product records.
- `argentum-harness` owns capability and surface registration, dependency
  metadata, built-in presentation and execution profiles, and policy
  resolution. It does not own execution.
- `argentum-cli` owns the command host, in-process client, executable commands,
  and versioned JSONL transport.
- `argentum-runtime` owns task orchestration, cancellation, provider selection,
  event publication, and lifecycle transitions.
- `argentum-providers` owns provider protocols and redacts credentials from
  application-facing status.
- `argentum-tools` owns typed tool descriptors and execution contracts.
- `argentum-security` owns workspace boundaries, capabilities, approvals, and
  secret redaction.
- `argentum-store` owns SQLite migrations, event persistence, and layout
  profiles.
- `argentum-ui` owns Slint markup and a projection of domain events. It does not
  call the filesystem, providers, subprocesses, or database directly.
- `argentum-platform` owns OS paths and target-specific secure-storage seams.

## Renderer rule

The production renderer is native Slint. No Tauri, Wry, WebView, JavaScript,
TypeScript, Node process, or JavaScript plugin runtime is part of the new app.
Slint markup is compiled into Rust during the Cargo build.

## Verification rule

Compilation is not runtime proof. Windows releases must launch the actual
packaged executable from a clean staging directory, and the evidence must name
the artifact, hash, commit, and observed runtime behavior.
