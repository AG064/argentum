# Architecture

## One core, several frontends

[AG064/argentum](https://github.com/AG064/argentum) holds the whole platform. The
desktop app and the CLI compose the same domain, harness, runtime, providers,
tools, store, security and platform code. Neither keeps a separate copy of that
core.

`argentum-cli` owns the command host and the terminal and JSONL output.
`argentum-app` and `argentum-ui` are the Slint frontend. The Android app
(`argentum-android`) uses the same command host. General computer use is not
supported in 0.1.0.

## CLI dependencies and packaging

The Cargo workspace has `default-members = ["crates/argentum-cli"]`, and the CLI
has `default = []`. A default CLI build does not include `argentum-app`,
`argentum-ui`, Slint, `argentum-android`, JNI, Android activity, native llama.cpp
or external MCP. It does include the shared Rust types of
`argentum-android-inference`; that crate's llama.cpp dependency applies only to
Android targets. To see the selected graph:

```powershell
cargo tree -p argentum-cli --target x86_64-pc-windows-msvc --edges normal,build --locked --offline
```

`scripts/package-windows-cli.ps1` builds
`cargo build -p argentum-cli --bin argentum --release --locked` and adds features
only through switches:

- `-Extensions` builds with `argentum-cli/extensions`: `argentum-extensions` and
  its MCP client (`rmcp`), which are absent from the default build.
- `-DesktopControl` builds with `argentum-cli/desktop-control`, which includes
  `extensions` (Windows desktop control for one declared workflow).
- `-Artifacts` builds with `argentum-cli/artifacts`, which also includes
  `extensions`.

The release has two CLI zips. `argentum-genesis-0.1.0-windows-x64.zip` is the
default build. `argentum-genesis-desktop-control-0.1.0-windows-x64.zip` is built
with `-DesktopControl`, so it includes the extensions feature. The Playwright
adapter and browser installation are optional setup and are not in either zip.
See [EXTENSIONS_BROWSER.md](EXTENSIONS_BROWSER.md).

## Runtime and authority

```text
trusted local owner path        task / JSONL client
         |                            |
 local authority handle        typed AppCommand
         +---------------+------------+
                         |
              argentum-cli CommandHost
                         |
                ComposableHarness
                         |
                 RuntimeService
                 /     |      \
           providers  tools   store
                 \     |      /
               security and platform authority
                         |
                      AppEvent
```

The owner path and the task/protocol path are separate. Serialized input, model
output and presentation state cannot create an approval. Headless frontends have
no owner approval responder, so a new approval they need is refused with an
error. Workspace policy, source identity and cancellation are checked again when
a request is admitted and when an effect runs. Read-only inspection is separate
from writable ownership, migration and recovery.

Crates and what they own:

- `argentum-domain`: typed state and shared types.
- `argentum-cli`: executable commands, terminal and JSONL.
- `argentum-runtime`: request, context, goal and tool orchestration.
- `argentum-providers`: provider protocols and credential bindings.
- `argentum-store`: history, accounting and receipts. It does not store
  credentials.
- `argentum-workspaces`: contained file observations and edit preparation.
- `argentum-changes`: change journal and diffs.
- `argentum-tools`: tool brokering for the enabled capabilities.
- `argentum-execution`: processes, jobs and verifiers, plus the optional
  restricted supplemental-check sandbox.
- `argentum-security` and `argentum-platform`: policy, approvals and operating
  system boundaries.
- `argentum-harness` and `argentum-lifecycle`: the harness composes capabilities
  in stages around the lifecycle's generation, retirement and recovery. The
  first-party lifecycle modules are not an external plugin loader.

Instruction and skill text from the project is untrusted request input with its
source recorded. It never grants authority.

Hosted model requests use explicit budget, account and endpoint bindings. Tool
schemas reflect the enabled capabilities. Continuation and verifier diagnostics
are private request state and are not exported as reasoning. When accounting or
an effect is unknown, the run stops.

The desktop and Android frontends share the command host. Optional extensions
enter through the same approval, lifecycle and result system. See
[ROADMAP.md](../ROADMAP.md).
