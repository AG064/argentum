# Native architecture

## Platform and frontend ownership

[AG064/argentum](https://github.com/AG064/argentum) is the canonical native
platform repository. Argentum Genesis names the CLI/TUI frontend, not another
runtime. Future desktop frontends and optional browser/extension capabilities
compose the same domain, harness, runtime, providers, tools, store, security and
platform authority. They must not maintain separate copies of that core.

Existing crate names and useful boundaries remain. In particular,
`argentum-cli` owns the command host and terminal/JSONL presentation;
`argentum-app` and `argentum-ui` retain the Slint frontend. Sharing the command
host does not make desktop visual/device qualification part of Genesis
release acceptance. General computer use and full Codex replacement remain
separate unqualified workflows.

## Default CLI dependency and package boundary

Verified on 2026-10-12 without a rebuild: the Cargo workspace has
`default-members = ["crates/argentum-cli"]`, and the CLI has `default = []`.
The Windows normal/build selection was inspected with:

```powershell
cargo tree -p argentum-cli --target x86_64-pc-windows-msvc --edges normal,build --locked --offline
```

This selected graph excludes `argentum-app`, `argentum-ui`, Slint and its
`i-slint-*` crates, `argentum-android`, JNI, Android activity, native llama.cpp
and external MCP. It does include `argentum-android-inference`'s shared Rust
contracts; that crate's llama.cpp dependency is Android-target-specific.
Its name is not a reason to move or duplicate working code.

`scripts/package-windows-cli.ps1` selects
`cargo build -p argentum-cli --bin argentum --release --locked -j $Jobs`
without `--workspace` or optional features. Merely defining a workspace
dependency, retaining a workspace member or listing it in Cargo.lock does not
mean it is linked into the CLI. Whole-workspace and feature-enabled builds
have different selections and require their own dependency/license review.

NEXT01's `extensions` feature explicitly selects `argentum-extensions` and
its MCP client through the existing runtime. A separate offline feature-graph
check confirmed that `rmcp` is absent by default and present with
`--features extensions`; desktop/Android native dependencies remain absent.
Playwright's adapter and browser installation are separate optional preparation,
not default CLI runtime/package payloads.

RELEASE01 does not include NEXT01's external extension/browser implementation.
That optional development work uses the same governed runtime and has separate
acceptance and packaging; it is not a Genesis runtime fork.

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

The owner path and untrusted task/protocol path are distinct. Serialized input,
model output and presentation state cannot manufacture an authority ticket.
Headless frontends disconnect the owner approval responder; a needed new
approval fails explicitly. Workspace policy, source identity and cancellation
are rechecked at admission and effects. Read-only inspection is separate from
writable ownership/migration/recovery.

`argentum-domain` defines typed state/contracts. `argentum-cli` owns executable
commands, terminal and JSONL. `argentum-runtime` owns request/context/goal/tool
orchestration. `argentum-providers` owns bounded protocols and secure bindings.
`argentum-store` persists history/accounting/receipts, not credentials.
`argentum-workspaces` supplies contained observations and edit preparation;
`argentum-changes` supplies journal/diff semantics. `argentum-tools` brokers
capabilities; `argentum-execution` owns bounded processes/jobs/verifiers and
the optional restricted supplemental-check boundary. `argentum-security` and
`argentum-platform` enforce policy/authority and OS boundaries.

`argentum-harness` stages capability composition around `argentum-lifecycle`'s
generation, retirement and recovery. Existing first-party lifecycle modules are
not an external plugin loader. Local instruction/skill text remains untrusted,
bounded, provenance-labelled request input, never authority.

Hosted model requests use explicit effective budget/account/endpoint bindings.
Tool schemas reflect enabled capabilities. Transient continuation and verifier
diagnostics are private bounded request state, not exported reasoning or
invented historical evidence. Unknown accounting and effects remain stops.

Native Slint and Android source reuse the shared command host but have separate
platform/visual/release gates. Historical Node browser/MCP/plugin modules are
not runtime dependencies. Future optional extensions must enter this same
governed authority/lifecycle/result system. See [HARNESS_MODULARITY.md](HARNESS_MODULARITY.md)
and [ROADMAP.md](../ROADMAP.md).
