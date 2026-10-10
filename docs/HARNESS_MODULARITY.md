# Argentum modular harness contract

Status: 0.1.0 draft architecture and migration contract

Argentum uses a small Rust command host and typed service seams. The target is
not a large core with optional controls painted over it. Providers, tools,
storage, approvals, execution backends, and UI surfaces must be registered,
inspectable, replaceable at a defined boundary, and have a defined removal
boundary. In 0.1.0, built-in composition can disable supported capabilities and
surfaces. Phase 4 adds the complete static developer execution toolkit, but
keeps its product implementations static. The separately authorized temporal
lifecycle work adds bounded in-process mount and unload primitives. The
`review.diff` registration is the first first-party registration migrated to
that lifecycle, and the active CLI runtime now retains its owner handle and
performs bounded shutdown. The optional `first-party.local-runtime` module is
the second lifecycle consumer and owns a contained llama.cpp process effect.
No external package loading is added.

This contract starts from the public DeepSeek Harness architecture, especially
its plugin tree, profiles and bundles, durable session log, and capability
seams. Argentum does not copy Cordis or add a JavaScript runtime. It applies the
same composition ideas to the existing Rust workspace and improves the product
contract with explicit availability, readiness, permission, and visibility
state.

Official reference material:

- [DeepSeek Harness repository](https://github.com/deepseek-ai/deepseek-harness)
- [DeepSeek Harness architecture](https://github.com/deepseek-ai/deepseek-harness/blob/master/docs/architecture.md)
- [DeepSeek Harness Cordis primer](https://deepseek-harness.github.io/deepseek-harness/en/reference/cordis-primer)
- [DeepSeek Harness web guide](https://github.com/deepseek-ai/deepseek-harness/blob/master/docs/user/guide/index.md)
- [Zagens runtime architecture](https://github.com/didclawapp-ai/zagens)
- [DeepSeek agent integration catalog](https://github.com/deepseek-ai/awesome-deepseek-agent)

DeepSeek Harness is currently a developer preview and warns that compatibility
can break. Argentum therefore adopts the architectural lessons, not its package
format or unstable APIs.

Research was rechecked on 2026-08-30. The direct mapping is:

| Public harness pattern | Argentum 0.1.0 interpretation |
| --- | --- |
| Replaceable plugin services | Typed Rust registrations with stable IDs and explicit owners |
| Ordered profiles and bundles | Persisted presentation and execution profiles with deterministic resolution |
| Durable session events | Project and session scoped SQLite facts used for restore and inspection |
| Live agent and capability events | Transient streaming, provider, catalog, and active-run projections |
| Session log as model context source | Only stored conversation records enter later model history |
| Capability seams | Separate definition, provider, consumer, policy, and readiness facts |
| Reversible registration effects | Generation-bound Rust modules with owner handles and staged registry publication |

Argentum does not claim dynamic plugin parity in 0.1.0. Built-ins are statically
linked for predictable latency and review. External modules remain unavailable
until an isolated, bounded, revocable process protocol is implemented. Zagens
and the wider DeepSeek agent catalog are comparison sources for replay,
approval, CLI, and multi-client ergonomics. They are not copied dependencies.

## Spatiotemporal composability audit

Status: bounded in-process temporal foundation implemented, with product and
external integration still partial.

DeepSeek Harness and its Cordis runtime describe two related composition
properties. Spatial composition covers dependency-aware activation and scoped
registrations. Temporal composition covers removal that completely reverses a
component's effects, including registrations, observers, and other resources.
The official documentation also describes composing from an exact parent
generation and returning a disposer for reversible scope cleanup.

Argentum currently adopts the following spatial properties:

- typed stable registrations with explicit owners and dependency validation;
- deterministic presentation and execution profiles;
- separate availability, readiness, authority, and visibility state; and
- bounded provider, model-catalog, and harness snapshots.

`argentum-lifecycle` now implements these in-process temporal properties:

- random kernel epochs and exact parent-generation binding;
- dependency-aware dynamic module mount and unmount;
- module, disposer, and effect handles that reject foreign, stale, disposed, and
  prior-mount use;
- async reversible effects, reverse rollback and teardown, explicit effect
  disposal, and retained recovery state after cleanup failure;
- cooperative cancellation before and between effects, with the cancellation
  token available to each disposer; and
- bounded live module snapshots and a 256-entry event ring.

`ComposableHarness` stages capability additions and removals in a cloned
registry. It validates capability and module dependencies before lifecycle
effects, commits additions only after mount success, rejects surviving
capability or execution-profile references before removal, and commits removal
only after teardown succeeds.

The first bounded first-party migration is `first-party.review-diff`:

- the static `review.diff` registration is withheld only inside the dedicated
  constructor's private staged registry;
- the module mounts the exact built-in registration as one reversible effect
  against an exact parent generation;
- successful mount publishes the same capability snapshot as
  `HarnessRegistry::built_in()` and returns the owner handle;
- teardown through that handle removes only `review.diff`, while
  `changes.accounting` and `approval.write` remain registered;
- remount requires a fresh generation and produces a fresh handle; and
- pre-cancelled setup returns an error without publishing the staged registry
  or a live module.

The active CLI command host now constructs `RuntimeService` through this
composed path. The runtime retains the module owner, roots tracked run
cancellation in a runtime shutdown token, rejects new commands once shutdown
starts, and refuses to unmount while a tracked run remains active. Teardown
removes handles only after successful unmount, so failed or cancelled cleanup
can be retried. Shared shutdown invokes normal per-run cancellation, including
negative approval resolution, cancelled tool evidence, and policy-run grant
revocation. Run admission checks shutdown again while holding the active-run
lock, which prevents registration after shutdown observes no active runs.
Successful unmount refreshes the public registry from the lifecycle-owned
registry, so `review.diff` is no longer reported after removal. The interactive
shell and JSONL server own the command tasks they start and drain those tasks
before lifecycle teardown. The interactive terminal reaps completed detached
tasks while active, drains remaining tasks on every exit, and retains late
command failures. JSONL clean end-of-input allows a 250 millisecond
natural-completion grace before shared cancellation and draining; transport
failures request cancellation immediately. Event-forwarder and writer failures
are combined with the primary transport result. Short-lived CLI commands use a
scoped host owner, and onboarding cleans failed hosts plus the old owner before
a workspace transition. No path relies on dropping a detached task to make
teardown safe.

The optional local provider adds a separate lifecycle kernel scoped to its
contained sidecar. `first-party.local-runtime` binds an exact runtime archive,
freshly extracted launcher, selected GGUF identity and hash, fixed argument
vector, cleared environment, random epoch, generation, loopback port, and
disabled features. It publishes a client only after process containment,
PID-owned listener checks, authenticated readiness, and model revalidation.
One model reuses one mounted generation. A model switch completes reversible
teardown before mounting a fresh generation. Runtime shutdown drains tracked
runs, unmounts this provider process effect, and only then unmounts the composed
harness registration modules.

Android local inference is a separate in-process module rather than a reuse of
the Windows sidecar. It binds preparation and activation to the full opaque
platform-authority generation, publishes only after pre-open and post-open
model integrity checks, owns all llama.cpp objects on one worker, cancels the
one active stream before replacement or unload, and is retired by provider
shutdown before platform authority. Its foreground-service generation is an
operating-system coordination value, not a lifecycle or authority handle.

This follows the relevant DeepSeek Harness and Cordis lifecycle semantics:
dependency validation, setup before publication, owner-held disposal,
reversible registration effects, and deterministic teardown. It is an
Argentum-native Rust implementation. No DeepSeek Harness source was copied and
no DeepSeek or Cordis runtime dependency was added. DeepSeek Harness is MIT
licensed, but its developer-preview package APIs are not adopted as an
Argentum compatibility contract.

The remaining temporal boundary is explicit:

- `review.diff` registration ownership and the local sidecar process effect are
  lifecycle-owned. Their Rust adapters and implementations, tools, storage,
  security broker, and other first-party registrations remain statically linked;
- the synchronous `RuntimeService` constructor still uses
  `HarnessRegistry::built_in()` as a compatibility and test fallback;
- signed package discovery, AEP, MCP, and general external-module supervision do
  not exist. The sidecar is one pinned first-party process integration, not a
  generic extension host;
- closures and resources are process-local and are not persisted or remounted
  after restart; and
- cancellation is cooperative. Operation futures must reach a terminal result.
  F08 can reconcile persisted runtime runs after proven Windows store-owner loss,
  but it does not recover or resume arbitrary module-local operation state.

Durable event replay restores product state. It is not a module teardown
mechanism and must not be described as lifecycle recovery. Phase 4 keeps
first-party providers, models, tools, and the review implementation statically
linked. Windows delegates the optional llama.cpp provider to its
lifecycle-owned external process. Android delegates it to the separate owned
in-process module. Neither path is a generic extension runtime.
Signed external packages still require isolated AEP transport, health
supervision, and hostile-module tests. Argentum must not claim full Cordis
parity until the applicable checks exist.

References: [DeepSeek Harness core subsystem
documentation](https://deepseek-harness.github.io/deepseek-harness/en/reference/subsystems/core)
and the [Cordis paper](https://arxiv.org/abs/2608.25512).

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
  composition, and state publication. `ComposableHarness` is the active CLI
  runtime adapter for the first-party `review.diff` registration.
  `HarnessRegistry` remains the synchronous compatibility and test fallback.
- **Lifecycle kernel:** owns in-process generation binding, module dependencies,
  reversible effects, cleanup recovery, and bounded live lifecycle projection.
  It does not grant authority or execute product behavior by itself.
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
      RuntimeService
      / active CLI             \ synchronous fallback
ComposableHarness              HarnessRegistry::built_in
   /          \
staged registry LifecycleKernel
                    |
          generation-bound effects
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
- `argentum-lifecycle` owns process-local temporal composition and cleanup.
- `argentum-execution` owns suspended child creation, Job Object containment,
  private files, and process and model identity checks.
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

The first implementation slice permits surface composition. Phase 4 extends
policy-backed enablement to all 17 built-in developer tools. Every model schema,
manual tool command, approval path, and execution path resolves the same
persisted project policy. A visual toggle is never authority.

## Execution policy contract

Execution policy is separate from presentation. It is persisted per project
and resolves sandbox scope separately from approval mode:

| Profile | Sandbox | Approval mode | Purpose |
| --- | --- | --- | --- |
| Observe | ReadOnly | Always | Bounded file and fixed Git inspection |
| Guided | WorkspaceWrite | Always | Default explicit approval for executable or mutating work |
| Supervised | WorkspaceWrite | OnRisk | Automatic inspection with approval for risky work |
| Automatic | WorkspaceWrite | Never | Enabled in-process workspace tools without prompts; host processes still prompt |
| Full Access | Unrestricted | Never | Enabled tools outside workspace containment for the current OS account |
| Developer | WorkspaceWrite | OnRisk | First-party source, Git, build, test, lint, and artifact work |
| Custom | Persisted exact scope | Persisted exact mode | Exact project-scoped capability overrides |

`Guided` is the default. Full Access can be selected only through local
authority and does not elevate operating-system privileges. Grants may be
scoped to one action, tool call, run, session, workspace, or persistent profile
and remain subordinate to sandbox and capability hard denies. Action grants
bind the complete normalized tool payload. Ordinary host processes always need
approval outside Full Access, including under Automatic and OnFailure. Enabling
a tool cannot enable an unavailable dependency. Disabling a tool removes it
from the model schema and causes manual requests for that tool to fail before
execution or approval. Network and persistent terminal capabilities remain
unavailable.

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
selection, model catalogs, provider-reported reasoning and usage, 17 typed
developer tools, scoped grants, execution audit, cancellation, fixed local Git,
fixed verification runners, and session goals.

The catalog must also show known missing capabilities, including a persistent
terminal, generic network tools, browser control, external extensions, and full
per-file review. They remain unavailable with reasons until their complete
configuration, security, lifecycle, error, and test paths exist.

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

The initial bounded implementation covered `read_text` and `write_text`.
Phase 4 completes this slice with typed filesystem inspection and mutation,
contained argv and shell execution, fixed Git, fixed verification, scoped
grants, audit, cancellation, and all execution profiles. Provider selection
remains a separate exact-profile setting. Generic network, browser, extension,
and persistent terminal capabilities remain unavailable.

### Slice 2.5: in-process temporal lifecycle

- bind mutations to a random kernel epoch and exact generation;
- mount and unmount bounded module declarations with dependency checks;
- retain async effect disposers and run rollback or teardown in reverse order;
- preserve failed cleanup as retryable recovery state;
- reject stale handles after disposal, unmount, or remount;
- stage dynamic capability registry changes through `ComposableHarness`;
- route active CLI startup through composed runtime ownership and retain the
  synchronous static constructor as a compatibility and test fallback;
- cancel tracked runs before teardown and retain handles for a fresh retry when
  unmount fails or is cancelled;
- retain, reap, and drain command tasks at the interactive terminal and JSONL
  transport boundaries before lifecycle teardown;
- preserve pending-approval cancellation evidence, refresh public registration
  state after unmount, retain late transport task failures, and explicitly
  teardown one-shot and onboarding host owners;
- lifecycle-own the optional local runtime process, bind it to one exact launch
  generation, and unmount it before harness registration teardown;
- keep every other built-in static until its migration has explicit ownership
  and teardown evidence.

This slice, the active CLI runtime connection, and the bounded local sidecar
consumer are implemented. They do not make external modules loadable and do not
provide durable remount or generic forced-abort recovery. F08 abandoned-run
reconciliation is limited to supported persisted runtime metadata and does not
recreate module-local resources.

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
- local GGUF discovery and optional contained llama.cpp;
- file mutation, durable change accounting, restore, and fixed verification;
- reviewed skills and optional integrations;
- active mobile packaging after secure storage and network routing are solved.

Native onboarding, local model selection, explicit pinned runtime and model
installation, contained local inference, file mutation, restore infrastructure,
and fixed verification now exist. Managed model updates, full review, skills,
integrations, cross-platform runtime bundles, and packaging remain open.

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
- Lifecycle tests cover exact generations, foreign and stale handles,
  dependencies, bounds, reverse rollback and teardown, cancellation, failed
  cleanup recovery, explicit effect disposal, and bounded live events.
- Harness adapter tests prove atomic capability mount and removal, declared
  dynamic dependencies, reference blocking, cancellation, and unchanged static
  built-in snapshots.
