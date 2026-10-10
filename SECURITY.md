# Security Policy

## Supported versions

Argentum is pre-1.0. Security fixes target the current development branch and
the latest published release candidate. Older builds may not receive patches.

## Report a vulnerability

Email **report@ag064.eu** with the affected version, impact, and a minimal
reproduction. Do not include real credentials or private user data. Do not open
a public issue until the maintainer has assessed the report. You should receive
an acknowledgement within seven days; remediation timing depends on severity
and reproducibility.

## Security boundaries

Argentum is a local-first application, not a hardened multi-user isolation
boundary. It protects against accidental capability expansion and untrusted
model or tool input, but it cannot protect data from a compromised operating
system account, administrator, malicious dependency, or provider that receives
an authorized request.

Default behavior:

- Rust owns provider, tool, storage, security, and platform behavior.
- The shipped application has no JavaScript runtime or dynamic web content
  execution path.
- The persisted workspace policy is authoritative. UI state cannot grant a
  capability that the policy does not allow.
- Relative paths resolve from the canonical workspace. Workspace-scoped
  profiles deny parent traversal, absolute escape, and symlink traversal.
  On Windows, model tool paths may use the explicit `/workspace` virtual root
  or the exact fully qualified current workspace root; these normalize to
  relative paths before validation and approval binding. Arbitrary `/home`
  aliases, drive-relative paths, traversal and outside-root paths are not
  remapped. File contents and command text are never rewritten as paths.
- Sandbox scope and approval mode are separate persisted values. A grant cannot
  widen ReadOnly or WorkspaceWrite, enable a disabled capability, or override
  a non-bypassable invariant.
- Reusable action grants contain a SHA-256 binding over the complete normalized
  tool input. A changed argument, shell command, path, timeout, environment,
  Git operation, patch, or verification selection requires a new decision.
- Full Access can be selected only by local authority. It removes Argentum's
  workspace containment and approval prompts for enabled actions, but does not
  elevate the operating-system account or bypass operating-system checks.
- File writes, shell commands, Git mutations, verification, and external
  processes require typed capabilities. Outside Full Access, every ordinary
  host process requires a matching local grant or prompt even when the selected
  workspace profile uses `Never` or `OnFailure` approval mode. Fixed hardened
  Git reads are the only no-prompt external-process exception.
- Provider requests use the Rust HTTP client with TLS and bounded request and
  response handling.
- Provider content, model output, tool arguments, attachments, and retrieved
  context are untrusted input and cannot authorize themselves.
- Read-only delegation is runtime-owned, disabled by default, and available only
  on the selected compatible HTTP coding route. A child reuses the exact parent
  provider binding but receives a fresh context and only the enabled read and
  search subset. It cannot write, execute, verify, grant authority, delegate,
  compact, or inherit a selected skill. The host revalidates policy, provider,
  budget, instructions, paths, observations, and usage at each boundary. Child
  prose is not evidence, and returned references are reconstructed from typed
  host observations. Cancellation drains the child before ownership is released.
- Secrets use the platform secure-storage boundary and zeroization helpers.
  They never enter SQLite event data or ordinary tracing output.
- File mutations use exact pre-images, stale checks, bounded rollback, durable
  change-operation records, and post-write readback.
- Child processes use direct argument vectors unless the separate shell tool is
  selected. Their environment is cleared and allowlisted, secret-like arguments
  and environment names are rejected, and output and duration are bounded.
  The fixed shell adapter, Git runner, and verification runners resolve
  canonical absolute executables outside the workspace and launch directory.
  A token cancelled before execution returns without attempting spawn.
- Managed Windows children are created suspended in a new process group. The
  Job Object receives kill-on-close, active-process, process-memory,
  job-memory, and CPU hard-cap limits. Argentum queries those limits and
  assigns the suspended child before resume. A limit, query, assignment,
  authority recheck, or resume failure terminates and reaps the child.
  Cancellation and timeout request group shutdown, then terminate and reap the
  assigned process tree if needed.
- The four built-in asynchronous Rust verification jobs use a separate
  eight-process profile for the installed Cargo/Clippy proxy chain. Local model
  sidecars and asynchronous approved project commands retain four processes. The verifier
  profile retains the 95 percent CPU cap, 4 GiB per-process and job memory caps,
  and bounded output capture. These asynchronous resource values are in fixed-verifier
  definition hashes, so an older receipt cannot satisfy a changed definition.
  This does not enable a tool or change approval, sandbox or executable selection.
- Git inputs map to a fixed local operation set. Push, force, branch deletion,
  remote changes, tag creation, merge, rebase, reset, and publication are not
  Phase 4 input variants. Fixed Git resolves a canonical absolute executable
  outside the workspace and launch directory. Read operations disable
  `core.fsmonitor`, external diff, and text conversion where applicable.
- Verification uses four fixed Rust runners and binds evidence to the workspace
  revision captured before and after execution. Customizing the owner-selected
  verification profile preserves its explicit source-profile provenance, so its
  fixed-runner exception survives capability toggles without extending to an
  unrelated custom policy or arbitrary process.
- Grants, execution audits, and verification evidence are project scoped and
  persisted without raw process output or credentials. Run cancellation revokes
  run-scoped policy grants.
- Cancellation is propagated through provider, storage, approval, and tool
  operations. File patches check cancellation before and between effects and
  roll back completed images in reverse order. Directory creation removes its
  newly created empty directories when cancellation wins the completion check.
- The in-process lifecycle kernel binds every mutation to a random kernel epoch
  and exact generation. Module, disposer, and effect handles are rejected when
  foreign, stale, already disposed, or from a prior mount. Dependent-tree
  retirement removes admission first, disposes deepest dependents before their
  root, and retains bounded recovery state for explicit retry.
- The optional local model runtime is a lifecycle-owned external process, not a
  model-granted tool. Installation and launch are separate decisions. Only a
  direct interactive local TTY production presentation path routes activation
  through the owner authority API. `CommandHost::local_handle` remains an owner
  API that Rust tests can construct directly. JSONL, machine output,
  noninteractive input, model output, provider content, and persisted state
  cannot create or replay an exact in-memory launch ticket. Declining leaves
  the CLI operational.
- Each local launch binds the immutable runtime bundle hash, extracted launcher
  hash, exact arguments, cleared environment hash, selected model path, model
  identity, full model hash, configured model maximum, managed-process limits,
  generation, epoch, loopback address, port, and disabled features. The process
  is created suspended and receives its queried Job Object limits before
  resume.
- The selected GGUF is held without write or delete sharing, rechecked after
  authenticated readiness, and revalidated through every client lease. The
  sidecar must own exactly one IPv4 listener at the approved literal
  `127.0.0.1` address and port, with no IPv6 listener.
- The sidecar bearer key is random, process-local, stored in a create-new file
  protected for the current owner, passed only through `--api-key-file`, and
  removed after process teardown. Argentum retains only bounded byte counts,
  hashes, truncation facts, and normalized diagnostics for sidecar output.
- Sidecar HTTP ignores system proxies, refuses redirects, uses HTTP/1 only,
  disables idle pooling, and has a two-second connect timeout. Runtime and
  model downloads use whole-transfer deadlines, exact size and SHA-256 checks,
  create-new part files, and atomic no-clobber publication with cleanup after
  every failure.
- Windows runtime, model, archive, launcher, session, part, and key paths use
  queried owner-only DACLs. Held archive, launcher, and model leases bind file
  identity, size, and SHA-256. They are rechecked immediately before spawn and
  after contained resume.
- The selected model maximum defaults to 64 GiB and accepts configured values
  from 1 MiB through 256 GiB. Sidecar memory is
  `clamp(model_size * 2 + 2 GiB, 4 GiB, 512 GiB)`. The Job Object allows four
  active processes and applies a 95 percent CPU hard cap. Managed stdout and
  stderr hash at most the first 4 MiB per stream while retaining complete byte
  counts and explicit complete-or-prefix flags. Both capture tasks share one
  five-second settlement deadline.
- The exact disabled feature projection is `web-ui`, `slots-endpoint`,
  `remote-downloads`, `server-side-tool-execution`, `mcp`, `agent`, `router`, `media`, and
  `reasoning`. Optional sidecar metrics are also not enabled by the fixed
  argument vector, but `metrics` is not part of that projection. The pinned
  launch configuration returns not
  found for unprefixed health and unauthorized for generation-prefixed health
  without the key. The real-runtime smoke test verifies both statuses.
- Errors are redacted before they are persisted or presented outside the
  relevant workspace context.

Permission decisions come from local policy, not from prompt text, model output,
tool arguments, provider metadata, or remote content.

## Authority tickets and platform state

Argentum evaluates capability enablement, sandbox scope, approval mode,
platform authority, and lifecycle generation independently. Effective
authority is their intersection plus the operating system's current decision.

Production local authority uses opaque host-issued tickets. Each ticket is
issuer-bound, has a random ID, binds one exact action and the full private epoch
plus ordinal generation, and expires after 30 seconds. The internal lifetime is
capped at 60 seconds. The verifier consumes a ticket and checks issuer,
generation, action, and expiry. The resulting non-clone proof is consumed and
checks action, generation, and expiry again immediately before mutation.
Tickets and proofs are not serializable or cloneable. Their manual `Debug`
output contains only the action kind and generation ordinal.

The current Windows Standard bridge is `windows.current-user`. It reports the
current process token user SID, no UID, no SELinux context, and exactly these
capabilities: `filesystem.user`, `network.user`, `process.user`,
`windows.job-object`, and `windows.local-runtime`. The serializable public
snapshot omits lease IDs, authority epochs, issuer IDs, tickets, expiry
internals, bridge handles, and cancellation tokens.

Platform retirement closes admission and revokes the lease before cleanup. It
retires `provider.local-runtime` and its sidecar before releasing
`platform.authority.standard`, then the runtime retires composed harness
modules. Cleanup failure stays visible as `recovery_pending` and the same sole
host owner can retry without relaunching retired services. Runtime handles,
open leases, process objects, listeners, secrets, cancellation tokens,
disposers, and closures are never persisted.

The complete implemented contract and deferred platform gates are in
[`docs/AUTHORITY_LIFECYCLE.md`](docs/AUTHORITY_LIFECYCLE.md).

## Android native host

The Android application runs under the ordinary application UID.
Its workspace is derived only from the operating system private data root and
validated before creation or persistence. Provider credentials use the bounded
Android Keystore bridge and do not enter commands, events, SQLite, logs, or UI
state after the save operation settles.

Java adapters carry bounded lifecycle, Keystore, Storage Access Framework,
notification, and Binder data only. Lifecycle messages do not grant authority.
Back handling defaults to Android unless a trusted local UI close action is
registered. The non-exported foreground service is started only by the direct
local-model Settings action and accepts only a positive exact generation. It
does not grant authority, execute tools, or expose a remote Binder.

Android local inference is a separate optional module. The default model is
absent. A direct local action downloads only the pinned HTTPS offer into the
app-private model directory. The file must be one regular GGUF leaf with the
exact configured byte count and SHA-256. It is verified before and after the
native runtime opens it. The activation approval is one-use and bound to the
full opaque authority generation and exact plan digest. Numeric generation
ordinals are used only for Android service coordination and are not authority
proof.

The pinned llama.cpp FFI objects remain on one owned worker thread. The module
admits one active model and one stream, bounds prompts, tokens, events, and
channels, suppresses token publication after cancellation, unloads before
replacement, and unloads during owner shutdown. The module opens no listener
and launches no child process. CPU fallback is a reported backend result, not
an authority downgrade. Stream cancellation is cooperative and checked between
native decode steps. Native load and unload calls are not preemptible, so owner
shutdown waits for the worker and has no hard wall-clock deadline.

The APK verifier requires the exact package, SDK levels, permission set,
exported activity, non-exported service, and ARM64 and x86_64 libraries. It
also requires at least one explicit trust anchor: the expected APK SHA-256 or
the expected signing-certificate SHA-256. The certificate subject DN is
informational and is not trusted as proof of a debug signer. Release signing
configuration and signing material are outside this milestone. Shizuku and
root remain distinct unavailable authority tiers. Each requires a separate
future local ceremony, manager grant, observed UID, capability probe, bounded
lease, revoke behavior, and service-death teardown. Extension loading,
self-update, and self-modification remain unavailable.

## Secret storage

Never store real credentials in configuration files, screenshots, bug reports,
model prompts, workspace event logs, or CI output. Android and Apple signing
private keys belong only in protected build environments and offline backups.
The local `release.keystore.b64` file and Android keystore paths are ignored and
must not be read, moved into source control, or included in a Rust build.

## Tools and extensions

The first release uses built-in Rust tools with typed capability descriptors.
There is no unconstrained plugin runtime. A future extension protocol must be
signed, capability-scoped, versioned, and disabled by default. Before activation
it must record source, revision, license, declared capabilities, network
destinations, executable hooks, and integrity information.

The Phase 4 toolkit has no generic network tool or persistent terminal. Its
product implementations remain statically linked. The separately reviewed
temporal lifecycle kernel provides bounded in-process mount, unload, disposer,
generation, rollback, cancellation, and reversible-effect behavior.
`ComposableHarness` stages capability registry changes and commits them only
after lifecycle success. The `first-party.review-diff` module owns only the
existing `review.diff` registration. Mount preserves the static public
snapshot, failed or cancelled setup publishes nothing, and teardown removes
only that registration. The underlying change and review code remains static.
The active CLI command host retains the module owner through `RuntimeService`.
Shutdown cancels tracked runs before teardown, rejects new commands once
shutdown starts, refuses to unmount while a tracked run remains active, and
retains the owner handle when teardown fails or is cancelled so cleanup can be
retried. Shared shutdown invokes the normal cancellation path for each active
run. Pending approvals therefore emit a negative resolution and cancelled tool
record before teardown, and policy-run grants are revoked. Run admission
rechecks shutdown while holding the active-run lock, so a run cannot register
after shutdown has observed an empty run set. Successful unmount updates both
the lifecycle projection and the public harness registry. The interactive
terminal retains ownership of every detached command task, reaps completed
tasks during the session, and drains remaining tasks on every exit before
unmount. Late command failures remain visible during cleanup. The JSONL server
retains ownership of every accepted command. Clean end-of-input gives those
commands a 250 millisecond natural-completion grace, then requests shared
cancellation and drains the remainder. A transport failure requests
cancellation immediately and drains all accepted tasks before teardown. Event
forwarder and response writer task failures are preserved alongside the primary
server result. One-shot commands explicitly shut down their host. Onboarding
also cleans failed hosts and shuts down the old owner before a workspace switch.
Lifecycle handles never widen sandbox or approval policy.

This kernel is not an external plugin security boundary. It does not load
packages, persist executable closures, survive forced task abort, isolate
untrusted code, or provide AEP or MCP transport. Its cancellation contract is
cooperative and operation futures must be driven to a terminal result. External
extensions remain unavailable until the signed, isolated, bounded, revocable
process protocol and hostile-module tests are complete.

## Dependency and release integrity

Every change must pass the Rust format, check, test, clippy, advisory, license,
and source-policy gates. The repository policy is in `deny.toml`; CI runs
`cargo deny check`, and local release checks may also run `cargo audit`.

Windows CLI candidates must be built from the committed root Cargo workspace,
packaged with SHA-256 and source provenance, and launched from clean staging
outside the checkout with an isolated store. The CLI gate is terminal/command
behavior, not a Slint window. Native graphical packages require their separate
visual and platform gates. See `scripts/package-windows-cli.ps1` and `ROADMAP.md`.

Android debug evidence must name the source commit, APK SHA-256 and size,
package metadata, exact emulator serial, API, ABI, lifecycle transitions, and
unverified device gates. API 36 x86_64 emulator success does not substitute for
API 28 or physical ARM64 evidence. The Milestone 2 debug APK is not a release
candidate, and its unstripped size is a known packaging concern. APK inspection
must bind either the expected artifact SHA-256 or expected signing-certificate
SHA-256 fingerprint. A certificate subject DN is informational and is not an
identity anchor. Lifecycle evidence must prove that Back reaches a non-resumed
stopped or removed state before relaunch, reject fatal exception,
AndroidRuntime, native crash, and tombstone signatures, and sanitize persisted
log evidence. It must bind the exact expected APK SHA-256 before installation,
keep the PID stable until the explicit force-stop, and inspect the full logcat
interval captured after clearing the device log. Fixture-mode results are not
runtime evidence.

Inference evidence must additionally bind the exact model hash and prove load,
strictly ordered stream sequence, cancellation, no late event, replacement,
unload, restart, and truthful backend selection. A physical device must reject
emulator indicators and report `arm64-v8a`; cross-compilation and emulator
results do not satisfy the physical ARM64 gate. An instrumentation hook is
engine-level evidence only. It must be confined to a dedicated evidence build
type, absent from ordinary debug and release packages, and cannot prove the
production Settings, authority, UI, or foreground-service path.

## AI context and usage limits

The contained Windows sidecar's Jinja tool representation is enabled explicitly.
Tools remain Argentum broker actions, never sidecar-owned shell or file actions.
Loaded-template capability checks and the authenticated pinned server's exact
chat-completions input-token count run before dispatch, using the same serialized
messages and catalog as generation. Lease, policy and request-fingerprint checks
remain mandatory. These metadata/tokenizer IPC calls do not run inference.

Local context may be configured to 8K, 16K, 32K, 64K, 128K or 256K tokens, with
8K unchanged as the default. The owner setting uses the existing model-bound
budget and does not start or authorize a process. Preparation places the
chosen size in the fixed argument vector and launch digest. A size change
requires a new exact launch approval; the host rejects a changed context setting after
plan preparation and a same-model runtime is reused only at the same size.
Authenticated loaded-context metadata must match that approved size. Existing
Job Object limits and all other fixed launch arguments remain unchanged.

Local output setting zero means no fixed response-token cap, not zero output.
It is valid only with the full matching loaded context for the sidecar. Context
shifting is disabled; goal accounting reserves the entire remaining context.
Positive owner limits and all task/tool/permission limits still apply. Truncated
responses never dispatch partial tool calls. Display/history byte bounds remain
separate from generation and do not prove that an incomplete JSON value is valid.

Context estimates are not provider billing truth because tokenization differs by
model. Argentum must not invent remaining tokens, costs, resets, or plan limits.
Provider quotas are displayed only when returned by a response or an official
quota endpoint.

F07B supervision is an explicit local-owner foreground operation, not model or
protocol authority. Its process-local claim holds the session across linked
attempts, while a durable active claim makes competing processes fail closed.
F08 binds every writable persistent Windows host to the physical database file,
a named lifetime mutex, a durable store identity, and a monotonically allocated
generation. Mutex ownership is the abandonment proof. PID and process-start data
are diagnostic only, so PID reuse, inaccessible processes, copied databases, and
legacy records cannot authorize takeover. The ownership mechanism accepts only
local fixed, removable, and RAM-disk volumes. UNC paths, mapped network drives,
and unknown volume types fail closed because a machine-local mutex cannot fence
another computer. Legacy admission checks include nonterminal run events, active
goal invocations, and unfinished goal runs. Old-generation request, terminal
event, effect, invocation, and finish callbacks fail closed after recovery.
Historical review events remain appendable without reviving a terminal claim.

Recovery is a bounded compare-and-swap mutation available only through the
trusted local store path after owner loss is proven. It records Interrupted
separately from request, effect, and accounting certainty. Prepared requests may
be proven undispatched; dispatched or partial requests retain unknown usage.
Known completed changes and terminal records are preserved. Uncertain external
effects remain explicit, approvals and grants are retired, affected goals pause,
and nothing is replayed automatically. Per-run review acknowledges the exact
recovery revision and clears only its admission hold. It cannot resolve unknown
usage or effects, restore authority, or resume work. A repeated review of the
same revision is stale and rejected.

F09 durable JSONL identities are project-scoped records inside that owned store.
The server validates and canonicalizes typed input, rejects ambiguous duplicate
members and unknown fields, checks an existing binding before mutable target
resolution, then claims the identity under the current host generation before
dispatch or durable acceptance. The versioned fingerprint includes all serialized
typed command fields. It is correlation, not authentication, approval, or a new
authority namespace. A different fingerprint under the same request ID conflicts
without replacing or disclosing the original input.

The first accepted record binds its resolved session, run, task, or cancellation
subject. Goal commands additionally bind the full relevant goal snapshot and
definition revision. Task commands bind the selected non-secret provider profile,
declared model budget, and reconciled execution policy. Provider probes bind the
requested stored profile. The server revalidates these snapshots after entering
durable execution. Task and probe paths carry the exact captured inputs to
the provider boundary. A durable model tool approval rechecks the complete
captured policy after the wait, and the guarded effect path repeats that
comparison immediately before authorization and execution. Goal mutation uses
an immediate compare-and-set transaction against the captured full goal. A
mismatch becomes the retained `durable_command_snapshot_changed` failure without
provider contact, tool effect, or goal replacement. Duplicate delivery cannot
retarget current state, create another owner,
or issue another provider, tool, verification, accounting, queue, or control
operation. Terminal result settlement precedes terminal delivery. Atomic
`NewSession` acceptance commits the session, event, result reference, and command
settlement in one SQLite transaction. Active claims and late finalizers are
generation-fenced. F08 reconciliation changes abandoned accepted claims to a
bounded interrupted state, or reconstructs success, failure, or cancellation only
from matching terminal run ownership plus the retained run/task lineage. A
recovered failure states that its original detail is unavailable. Reconciliation
never redispatches the command.

Conversation fork lineage is historical data, not authority. A fork pins bounded
message and F06 context-group identities from one unarchived source session in the
same project at its latest settled boundary. The child shares the current
workspace and current policy. It does not copy or restore files and does not
inherit grants, approvals, local-authority tickets, provider credentials, selected
providers, goals, queued work, steering, process jobs, reservations, or recovery
state. Historical tool arguments and results remain subject to current projection
policy and cannot authorize a new execution. Creation and durable JSONL settlement
are one SQLite transaction, do not update active session selection, and reject
active, unresolved, nested, stale-preview, malformed, or over-capacity sources.

Retained responses are bounded and filtered for credential-like failure detail.
Historical replay never restores an approval ticket or expired grant. Current
project and disclosure rules are checked when the body is delivered. Restriction
or pruning removes result content while retaining a non-executable fingerprint
tombstone. Unsupported identity versions, malformed records, unavailable identity
lookups, quota exhaustion, and settlement failure fail closed rather than becoming
cache misses. The inspection path returns only bounded command metadata and does
not reconcile or execute work. Full deliberate project/store deletion ends the
namespace. This is not an exactly-once guarantee for arbitrary external effects
or a defense against new intent expressed with a fresh identity.

Command retention is bounded both per project and across the physical store. A
project can retain 4,096 identities, 32 active admissions, and 512 result bodies.
The store can retain 16,384 identities, 128 active admissions, and 2,048 result
bodies. Admission checks run in the claim transaction. Result pruning removes
only bounded response content and preserves the identity tombstone.

Goal, criteria, accounting, session, canonical-workspace, and owner bindings are
rechecked before another attempt or verified completion.

Acceptance criteria can select fixed typed Rust verification runners and
supported exactly owner-approved project runners. A criteria selection cannot
approve a project runner or broaden its existing binding.
Selection does not grant process, filesystem, network, or approval capability.
Passing evidence must match the exact runner definition, canonical workspace,
bounded relevant-source snapshot before and after execution, and current goal
definition. Exceeded source coverage is Unknown. Failed, cancelled, unavailable,
unknown, stale, legacy, or model-authored claims cannot establish completion.
Public status contains bounded metadata and receipt identities, never the private
owner claim, prompts, instruction bodies, raw process output, or credentials.
