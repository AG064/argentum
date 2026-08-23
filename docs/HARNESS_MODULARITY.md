# Argentum modular harness contract

Status: 0.1.0 draft architecture and migration contract

Argentum uses a small Rust command host and typed service seams. The target is
not a large core with optional controls painted over it. Providers, tools,
storage, approvals, execution backends, and UI surfaces must be registered,
inspectable, replaceable at a defined boundary, and removable when they are not
needed.

This contract starts from the public DeepSeek Harness architecture, especially
its plugin tree, profiles and bundles, durable session log, and capability
seams. Argentum does not copy Cordis or add a JavaScript runtime. It applies the
same composition ideas to the existing Rust workspace and improves the product
contract with explicit availability, readiness, permission, and visibility
state.

Official reference material:

- [DeepSeek Harness repository](https://github.com/deepseek-ai/deepseek-harness)
- [DeepSeek Harness architecture](https://github.com/deepseek-ai/deepseek-harness/blob/master/docs/architecture.md)
- [DeepSeek Harness web guide](https://github.com/deepseek-ai/deepseek-harness/blob/master/docs/user/guide/index.md)
- [Zagens runtime architecture](https://github.com/didclawapp-ai/zagens)
- [DeepSeek agent integration catalog](https://github.com/deepseek-ai/awesome-deepseek-agent)

DeepSeek Harness is currently a developer preview and warns that compatibility
can break. Argentum therefore adopts the architectural lessons, not its package
format or unstable APIs.

Research was rechecked on 2026-08-18. The direct mapping is:

| Public harness pattern | Argentum 0.1.0 interpretation |
| --- | --- |
| Replaceable plugin services | Typed Rust registrations with stable IDs and explicit owners |
| Ordered profiles and bundles | Persisted presentation and execution profiles with deterministic resolution |
| Durable session events | Project and session scoped SQLite facts used for restore and inspection |
| Live agent and capability events | Transient streaming, provider, catalog, and active-run projections |
| Session log as model context source | Only stored conversation records enter later model history |
| Capability seams | Separate definition, provider, consumer, policy, and readiness facts |

Argentum does not claim dynamic plugin parity in 0.1.0. Built-ins are statically
linked for predictable latency and review. External modules remain unavailable
until an isolated, bounded, revocable process protocol is implemented. Zagens
and the wider DeepSeek agent catalog are comparison sources for replay,
approval, CLI, and multi-client ergonomics. They are not copied dependencies.

## Product rules

1. Every capability has a stable ID, owner, state, dependencies, and security
   boundary.
2. Availability, enablement, readiness, authority, and UI visibility are
   separate facts.
3. A hidden surface does not disable its service. A disabled service does not
   gain authority because a surface is visible.
4. Unavailable work remains inspectable with a factual reason. It is never
   represented as complete, connected, verified, or ready.
5. The focused default hides secondary detail, while one action can reveal the
   exact plan, tool trace, reasoning, usage, approval, or review state that
   exists.
6. Provider-reported reasoning is separate from answer text. Argentum does not
   invent reasoning, token counts, context limits, tool calls, change sets, or
   verification results.
7. Anything sent to a model must be reconstructable from durable session state
   or a named, captured input. Secrets are the only deliberate exception and
   stay in a secure credential boundary.
8. Built-in capabilities may run in process for speed. Untrusted extensions
   require an isolated process, bounded protocol, explicit capabilities, and a
   revocable lifecycle.

## Terms

- **Harness kernel:** owns registration, dependency resolution, profile
  composition, lifecycle, and state publication. It does not implement model,
  tool, storage, or UI behavior itself.
- **Capability:** a service seam with a stable interface. A complete seam has a
  definition, provider, and consumer.
- **Module:** one implementation that registers capabilities, commands,
  durable event projectors, live observers, or surfaces.
- **Profile:** a named, ordered composition of built-in modules and surface
  defaults.
- **Bundle:** a distributable set of module declarations and profile patches.
  Bundles are a future extension boundary, not an active 0.1.0 package format.
- **Surface:** a user-facing projection such as Conversation, Activity, or
  Changes. Surface visibility is presentation state only.
- **Durable event:** a fact needed after restart, for replay, audit, or model
  context.
- **Live event:** bounded process state such as a stream delta, request
  interception, or transient connectivity result.

## Rust composition

```text
CLI, desktop, mobile, automation
              |
        typed AppCommand
              |
         CommandHost
              |
      HarnessRegistry
       /     |      \
 profiles  services  surfaces
       \     |      /
        RuntimeService
              |
 providers, tools, security, store, workspace
              |
          AppEvent
```

The kernel is intentionally small. It resolves declarations and publishes an
honest snapshot. Runtime crates keep their existing ownership:

- `argentum-providers` owns model transports and catalog discovery.
- `argentum-tools` owns typed model-facing tools.
- `argentum-security` owns authorization and approval policy.
- `argentum-store` owns durable facts and projection state.
- `argentum-runtime` owns the run loop and event ordering.
- `argentum-ui` renders domain state and dispatches commands only.

No frontend imports a provider, database, filesystem, subprocess, or secret
implementation directly.

## State model

Each capability reports:

- `available`: the implementation exists in this build;
- `enabled`: the current composition includes it;
- `configurable`: a user action can safely change enablement;
- `detail`: the bounded factual scope of the implementation;
- `unavailable_reason`: why it cannot be enabled, when unavailable;
- `dependencies`: stable IDs required by the capability.

Readiness and permission remain owned by the implementing service. For example,
an OpenAI-compatible provider module can be available and enabled while its
selected profile still needs a credential. A write tool can be available and
enabled while execution is waiting for approval.

Each surface reports:

- availability in the active build;
- current visibility;
- whether visibility is user configurable;
- a factual detail or unavailable reason.

The first implementation slice permits surface composition. The second slice
adds policy-backed enablement only for the existing read and write text tools.
Every model schema, manual tool command, approval path, and execution path must
resolve the same persisted project policy. A visual toggle is never authority.

## Execution policy contract

Execution policy is separate from presentation. It is persisted per project
and has two selectable built-in profiles:

| Profile | Read text | Write text | Approval behavior |
| --- | --- | --- | --- |
| Read Only | Enabled | Disabled | No write request can be created |
| Confirm Before Changes | Enabled | Enabled | Every write requires approval once |
| Custom | Exact saved tool enablement | Exact saved tool enablement | Writes still require approval |

`Confirm Before Changes` is the default. The current product does not expose a
full-access profile, approval timeouts, session-wide grants, command execution,
network access, or external-process access. Enabling a tool cannot enable an
unavailable dependency. Disabling a tool removes it from the model schema and
causes manual requests for that tool to fail before execution or approval.

The active execution profile, effective tool states, and approval behavior are
visible in Settings, the composer context, CLI human output, and protocol JSON.
Capability changes are rejected while a run or approval is active so one run
cannot observe a policy that changes midway through execution.

## Built-in profiles

The initial profiles change presentation only:

| Profile | Purpose | Optional surfaces |
| --- | --- | --- |
| Focused | Quiet session work | Activity hidden, Changes hidden |
| Standard | Default desktop work | Activity hidden, Changes hidden |
| Review | Inspect the current result | Changes visible |
| Trace | Inspect durable session facts | Trajectory visible |
| Full | Supervise work and review | Activity visible, Trajectory visible, Changes visible |
| Custom | Result of individual visibility changes | Exact saved visibility |

Focused and Standard are deliberately close in the first slice because density
is not yet enforced by every component. Their distinction becomes active only
after the density token reaches the whole shell. Argentum does not claim a
visual difference before that migration is complete.

Execution profiles are backed by the Rust harness and security boundary. They
are not aliases for presentation profiles.

## Initial truthful catalog

Capabilities available in the current Rust product include streamed model
turns, durable sessions, bounded conversation history, exact-profile provider
selection, model catalogs, provider-reported reasoning and usage, read and
write text tools, write approval, cancellation, and session goals.

The catalog must also show known missing capabilities, including command
execution, a persistent terminal, network tools, browser control, external
extensions, real diff review, and verification runners. They remain
unavailable with reasons until their full configuration, security, lifecycle,
error, and test paths exist.

Current visible surfaces are Conversation, Plan, Activity, Trajectory, Changes
summary, and approval. Files, Terminal, Preview, and full per-file Changes
review remain unavailable. Approval visibility is lifecycle-driven and cannot
be hidden while an action needs a decision.

## Event and context contract

Argentum keeps two event domains:

- durable domain events and normalized records for sessions, messages, tools,
  approvals, changes, verification, and goals;
- transient runtime events for streaming deltas, live provider status,
  catalogs, active-run snapshots, and harness inspection snapshots.

The durable log is the source for restored conversation and audit state. Raw
provider reasoning can be shown when explicitly returned, but it is not added
to later user messages. Provider usage is shown only when reported, and context
limits are shown only when the provider or a verified catalog supplies one.

The target trajectory inspector may eventually project:

- the exact input admitted to a turn, with secret fields redacted;
- plan and lifecycle transitions;
- visible answer and provider reasoning streams;
- tool schemas, calls, results, and approval decisions;
- exact usage and known context limits;
- changed-file and verification records when those records are real.

It must not expose credentials, hidden provider transport fields, fabricated
chain-of-thought, or guessed token counts.

The first 0.1.0 trajectory slice is narrower. It reads one bounded,
project-and-session-scoped window from the durable event log and normalized
assistant records. It shows only task acceptance, plan counts, lifecycle
transitions, tool state, approval requests and decisions, run errors, exact
change and verification summaries, and stored provider/model usage. It does not
show prompt text, reasoning text, tool arguments, tool results, provider request
bodies, or an inferred model context. A truncated snapshot says so explicitly.
Live Activity remains a separate transient surface. Opening, refresh, restart,
and lag recovery use a bounded trajectory snapshot. Each newly persisted fact
publishes one ordered transient entry projection, including while the surface
is hidden. A later snapshot replaces the projection authoritatively.

## Extension security

Built-in Rust modules are statically linked and reviewed. A later external
module format must meet all of these gates before installation is offered:

1. Signed manifest with stable module ID and version.
2. Explicit requested capabilities and dependency bounds.
3. Separate process by default, with no inherited credential environment.
4. Length-bounded, versioned IPC with cancellation and backpressure.
5. Workspace path mediation through the capability broker.
6. Network and subprocess access denied unless separately granted.
7. Observable start, stop, crash, timeout, and permission states.
8. Clean unload that removes registrations and leaves durable state valid.

Native dynamic libraries are not an extension boundary for untrusted modules.
An ABI crash would compromise the host, and unload behavior is too fragile for
the required safety contract.

## 0.0.8 and 0.0.9 continuity

The v0.0.8 release notes describe desktop provider workflows, local llama.cpp,
an Android app, onboarding, secure key entry, agents, and broad packaging. Some
claims exceeded the end-to-end evidence later documented for v0.0.9. The
active migration matrix therefore uses v0.0.9 as the stronger evidence source,
while preserving every verified v0.0.8 workflow as a parity requirement.

The modular architecture does not reduce the parity target. It gives every
restored feature a place and an off switch:

- provider onboarding becomes profile, credential, and catalog modules;
- local GGUF discovery and managed llama.cpp become separate modules;
- skills become reviewed tool bundles with explicit capabilities;
- mobile is a client profile over the same commands and durable events;
- channels, browser control, computer control, and scheduling stay disabled
  until their permission and lifecycle paths are complete.

## Delivery plan

### Slice 1: inspectable composition

- add the Rust harness registry and typed snapshot;
- publish built-in and unavailable capabilities truthfully;
- persist a selected presentation profile with layout state;
- expose explicit surface visibility commands;
- add CLI inspection and selection;
- add a desktop Settings section for profile and surface visibility.

### Slice 2: execution gates

- resolve tool and provider registrations from the selected capability set;
- persist capability overrides per project;
- require dependency and security validation before enablement;
- expose `Confirm Before Changes` as the first policy-backed execution profile.

The first bounded implementation covers `read_text` and `write_text`. Provider
selection remains a separate exact-profile setting. Command, network,
external-process, browser, and terminal capabilities remain unavailable.

### Slice 3: trajectory and context inspector

- add a bounded durable session trajectory for existing event and message
  records;
- expose it through one additive command/event contract, CLI output, and an
  optional surface shared by desktop and mobile layouts;
- keep exact model-visible context, full tool exchanges, disclosure modes, and
  deterministic export unavailable until their records and redaction contract
  are complete.

### Slice 4: verified parity modules

- native onboarding and workspace picker;
- local GGUF discovery and managed llama.cpp;
- real file changes, diff review, restore, and verification;
- reviewed skills and optional integrations;
- active mobile packaging after secure storage and network routing are solved.

### Slice 5: external module boundary

- signed manifests and isolated process host;
- bounded extension protocol and lifecycle supervision;
- capability review, compatibility checks, and safe uninstall;
- no marketplace claim until hostile-module tests pass.

## Acceptance gates

- A snapshot lists every registered capability and surface with a factual state.
- Unavailable or nonconfigurable entries cannot be enabled by CLI, JSONL, or UI.
- Profile selection and individual surface visibility survive restart.
- UI visibility changes never grant runtime authority.
- A hidden surface continues to receive durable state and renders current state
  when reopened.
- No secrets enter events, SQLite, logs, or inspection output.
- Protocol additions are additive in version 1.
- Workspace tests cover dependency failure, unavailable modules, invalid IDs,
  custom visibility, restart persistence, and redaction.
