# Argentum

Argentum 0.1.0 is a CLI-first Rust agent harness with a native desktop client.
This checkout is an engineering draft, not a published release. It is
designed as a calm task workspace where task stages, provider activity,
permissions, and command events remain visible without turning the product
into a dashboard.

## Current rewrite

The command host is the product boundary. The CLI and native Slint client use
the same typed commands, event stream, runtime, providers, tools, security, and
storage. The native client links to the command host in process, so it does not
start a subprocess or serialize JSON for each action. External clients can use
the versioned JSONL protocol exposed by `argentum-cli serve`.

OpenAI-compatible task runs can use the built-in `read_text` and `write_text`
tools through the same command host. Reads remain inside the active workspace.
Writes pause for explicit approval before execution. Tool rounds, calls,
arguments, results, prompt, history, and visible output all have hard limits.

Slint files contain declarative view markup compiled during the Rust build.

The previous TypeScript, Node, Tauri, Android, and web application tree is kept
under [`legacy/`](legacy/) for reference. It is not used by the new build.

## Quick start

Install the Rust toolchain, then check the command host:

```powershell
cargo check --workspace
cargo test --workspace
cargo run -p argentum-cli -- status
```

Submit one task from the CLI:

```powershell
cargo run -p argentum-cli -- run --prompt "Summarize this workspace"
```

Start the persistent JSONL command server for another client:

```powershell
cargo run -p argentum-cli -- serve
```

Inspect or select durable sessions for the current workspace:

```powershell
cargo run -p argentum-cli -- sessions
cargo run -p argentum-cli -- session select SESSION_ID
```

Conversation turns are stored in SQLite by project and session. Selecting or
reopening a session restores its ordered user and assistant messages, and the
same visible history is supplied to the selected provider on the next turn.
Provider reasoning is stored separately from answer text and appears in a
collapsed disclosure. Reported token usage and an exact known context limit are
stored with the assistant turn. Argentum does not estimate missing usage.

Test the configured LM Studio endpoint without sending a model prompt:

```powershell
cargo run -p argentum-cli -- provider probe lm-studio
```

List, save, and select durable provider profiles for the current workspace:

```powershell
cargo run -p argentum-cli -- provider list
cargo run -p argentum-cli -- provider save local-secondary --label "Local secondary" --kind lm-studio --endpoint "http://127.0.0.1:2234/v1/" --model "qwen-local" --select
cargo run -p argentum-cli -- provider select local-secondary
cargo run -p argentum-cli -- provider models local-secondary
cargo run -p argentum-cli -- provider model local-secondary --model "qwen-local"
```

Persist or inspect the workspace used by the desktop host:

```powershell
cargo run -p argentum-cli -- workspace status
cargo run -p argentum-cli -- workspace set "A:\path\to\workspace"
```

Store a hosted-provider credential in the operating-system keyring. The command
reads the credential from standard input and never accepts it as a command-line
argument:

```powershell
cargo run -p argentum-cli -- provider credential set minimax
cargo run -p argentum-cli -- provider credential clear minimax
```

Start the native client:

```powershell
cargo run -p argentum-app
```

By default, Argentum creates a local LM Studio-compatible profile at
`http://127.0.0.1:1234/v1/`. Provider profiles are scoped to the workspace and
the selected profile drives task execution immediately. Profile records contain
only a label, provider kind, endpoint, and model. Credentials are not accepted
by profile save and are never stored in SQLite or events. Canonical profile IDs
`openai`, `minimax`, and `deepseek` receive only their matching
`OPENAI_API_KEY`, `MINIMAX_API_KEY`, or `DEEPSEEK_API_KEY` from the host
environment or from the operating-system keyring through `provider credential
set`. Missing credentials fail before network access. The connectivity
probe uses the profile's bounded models endpoint and does not send a billable
model request. A canonical credential is sent only to its approved HTTPS origin
with the default TLS port. A saved canonical profile cannot redirect that key
to a custom host. Model catalogs and model selection are scoped to the exact
profile. Selecting a model does not silently select its provider profile.

On Windows, the desktop host also recognizes the saved v0.0.9 workspace pointer
and can reuse its configured MiniMax credential in memory. This compatibility
path never rewrites the legacy secrets file. To run the bounded configured
MiniMax check in an isolated temporary database:

```powershell
.\scripts\test-configured-minimax.ps1
```

The script performs a non-billable models probe and one short billable streamed
response. It does not print the credential, removes its temporary database, and
restores the previous provider selection.

## Windows release validation

Build the portable native executable and write its SHA-256 file:

```powershell
.\scripts\build-windows-release.ps1
```

Launch that exact artifact from a clean staging directory and verify that it
creates a native window:

```powershell
.\scripts\validate-windows-release.ps1
```

## Workspace

- `crates/argentum-domain`: versioned product types and commands/events.
- `crates/argentum-cli`: shared command host, in-process client, CLI, and JSONL
  protocol.
- `crates/argentum-runtime`: task lifecycle and event orchestration.
- `crates/argentum-providers`: provider contracts and HTTP adapters.
- `crates/argentum-tools`: capability-scoped built-in tools.
- `crates/argentum-security`: workspace boundaries, policy, and redaction.
- `crates/argentum-store`: SQLite event log and layout persistence.
- `crates/argentum-ui`: native Slint shell and UI projection.
- `crates/argentum-platform`: OS paths and secure-storage boundary.
- `ui/`: shared Slint view markup and Argentum tokens.

## Design contract

The visual system is documented in
[`docs/ARGENTUM_DESIGN_SYSTEM.md`](docs/ARGENTUM_DESIGN_SYSTEM.md). The rewrite
plan and acceptance gates are in
[`docs/PREMIUM_AGENT_HARNESS_PLAN.md`](docs/PREMIUM_AGENT_HARNESS_PLAN.md).
The current draft boundary and the evidence-based v0.0.9 migration matrix are
in [`docs/releases/v0.1.0-draft.md`](docs/releases/v0.1.0-draft.md) and
[`docs/V0_0_9_MIGRATION.md`](docs/V0_0_9_MIGRATION.md).

## Security boundary

The model cannot grant itself permissions. File operations are checked against
the active workspace root, write and process capabilities require policy, and
secrets use a dedicated platform boundary. The old runtime is archival and is
never loaded by the new application.
