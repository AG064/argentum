# Authority and lifecycle contract

This document records the authority and lifecycle behavior implemented for the
Windows local runtime and optional Android in-process inference. It does not
mark packaging, signing, emulator inference, or physical-device evidence ready.

## Effective authority

Argentum records five independent authority dimensions. Authority-sensitive
command admission applies the dimensions relevant to the exact action:

| Dimension | Decision |
| --- | --- |
| Capability enablement | Whether the active execution policy enables the exact capability. |
| Sandbox scope | Which paths and resources Argentum may target. |
| Approval mode | When Argentum asks again for an enabled capability. |
| Platform authority | Which measured operating-system identity and capability set executes the operation. |
| Lifecycle generation | Whether every authority, module, process, model, and stream handle is still current. |

Effective authority is the intersection of all five dimensions and the
operating system's current decision. A wider value in one dimension does not
widen another. A presentation setting, prompt, model response, provider
message, extension payload, JSONL request, stored record, or tool argument
cannot increase authority.

## Local authority tickets

`crates/argentum-security/src/authority.rs` defines the production ticket
contract.

- A host authority channel creates a random issuer ID, a random authority
  epoch, and generation ordinal 1.
- Each ticket has a random ticket ID and is bound to its issuer, one exact
  `LocalAuthorityAction`, and the full authority generation. The full
  generation contains both the private epoch and public ordinal.
- The production ticket lifetime is 30 seconds. The internal lifetime is
  capped at 60 seconds.
- The issuer rejects a generation from another epoch.
- The verifier consumes the ticket by value. It checks issuer, full
  generation, exact action, and expiry before returning a proof.
- The proof is then consumed by `authorize` immediately before the protected
  mutation. That second check repeats the exact action, full generation, and
  expiry checks.
- `LocalAuthorityTicket` and `VerifiedLocalAuthority` are opaque. They do not
  implement `Clone`, `Serialize`, or `Deserialize`.
- Their manual `Debug` output is bounded to the action kind and generation
  ordinal. It omits issuer ID, ticket ID, epoch, timestamps, and proof
  internals.

Tickets are not fields in `AppCommand`, `AppEvent`, SQLite records, protocol
v1, provider requests, or model messages. `InProcessClient`, JSONL, machine
output, noninteractive input, and stored state use the untrusted dispatch path
and cannot mint tickets. Provider and model content remains untrusted runtime
input; it cannot carry host tickets or call the owner API. `LocalCommandHandle`
is an owner API that receives the issuer only inside the host, and tests can
construct it directly. The production presentation surface routes
authority-widening actions to that handle only after direct local TTY
interaction. Policy
reductions and capability disablement do not mint a ticket. Full Access
selection, capability enablement, tool approval, and local-runtime activation
require the exact action when they widen authority.

## Windows Standard authority

The first platform bridge is Windows only:

| Field | Current value |
| --- | --- |
| Platform | `windows` |
| Bridge | `windows.current-user` |
| Requested tier | `standard` |
| Active tier | `standard` while the lease is current |
| Observed identity | The current process token user SID in bounded `S-1-...` form |
| Observed UID | None |
| SELinux context | None |

The Windows probe advertises exactly these sorted capability IDs after the
required Windows APIs initialize:

```text
filesystem.user
network.user
process.user
windows.job-object
windows.local-runtime
```

The safe public projection contains only platform, requested tier, active
tier, state, generation ordinal, the bounded probe, and bounded detail text.
It does not expose lease ID, bridge handle, authority epoch, issuer ID,
ticket, expiry internals, or cancellation token.

Shizuku shell and root are domain values only. There is no Windows UID or
SELinux claim, and there is no implemented elevated bridge in this milestone.

## Lifecycle composition and retirement

The lifecycle kernel binds every mutation to a random kernel epoch and exact
generation. It validates dependencies before setup, retains each acquired
disposer immediately, publishes only after setup succeeds, rolls back effects
in reverse order, rejects foreign and stale handles, and keeps bounded live
snapshots and events.

`LifecycleKernel::retire_tree` computes the complete dependent closure. It
retires deepest dependents first. Branches at the same depth use descending
mount generation and then module ID as the final stable tie breaker. Effects
inside each module retire in reverse acquisition order.

`ComposableHarness` stages registry changes and publishes them only after a
mount succeeds. Tree retirement removes affected capabilities from public
admission before effect disposal starts. If cleanup fails, already retired
modules stay retired, failed and unattempted disposer state stays visible, and
an explicit retry recomputes and finishes the remaining tree without
remounting retired work.

The platform broker uses the same composition for
`platform.authority.standard` and its authority-dependent services. Retirement
closes authority admission, publishes `recovery_pending`, revokes the current
lease, retires `provider.local-runtime`, stops and removes the sidecar module,
then releases `platform.authority.standard`. Success publishes `revoked`.
Failure keeps `recovery_pending`; the same owner can retry without relaunching
an already retired service.

The current host shutdown order is:

1. The sole mutable `CommandHost` requests shutdown. Runtime command admission
   closes and active runs receive normal cancellation.
2. TTY and JSONL surfaces drain their owned command tasks. One-shot operations
   finish before owner shutdown. A shutdown call fails while an active run is
   still registered.
3. The authority broker retires the provider dependency. The local runtime
   cancels streams, stops the sidecar, clears its provider registration, and
   removes its lifecycle module.
4. The Windows Standard authority bridge retires after every dependent.
5. The runtime unmounts its composed harness modules, including
   `first-party.review-diff`.
6. A successful repeated shutdown is idempotent. A failed cleanup remains
   retryable through the same non-clone owner.

`CommandHost` is not cloneable. `LocalCommandHandle`, `InProcessClient`, and
the observer cannot request or complete shutdown.

## Public and private state

Public and durable state may include checked authority snapshots, requested
policy, module intent, artifact metadata, and bounded audit evidence. The
current Milestone 1 implementation does not persist authority tickets,
verified proofs, issuer IDs, private epochs, platform leases, process handles,
open file leases, listeners, cancellation tokens, bearer keys, model contexts,
effect disposers, or executable closures. Restart must acquire new runtime
authority and a new lifecycle epoch. It cannot replay old handles as live.

## Windows local runtime

The local provider, model, runtime bundle, and process are optional. Declining
installation or either launch confirmation leaves the CLI usable. The first
implementation is Windows x86_64 with the pinned Vulkan sidecar. No local
provider or model is required for normal CLI operation.

In production presentation surfaces, only direct local TTY interaction routes
local-runtime activation through the owner authority API. `CommandHost::local_handle`
is the owner API, and Rust tests can construct it without observing an OS TTY.
JSONL, machine-readable output, noninteractive input, model output, provider
content, and persisted state cannot approve or replay activation. The approval
binds the exact launch-plan digest and is consumed against the current platform
authority generation.

### Process and network boundary

The sidecar receives a fixed argument vector:

```text
--model <verified GGUF path>
--host 127.0.0.1
--port <reserved ephemeral port>
--api-prefix <random generation path>
--api-key-file <private key path>
--ctx-size 8192
--n-gpu-layers 99
--parallel 1
--predict 512
--reasoning off
--no-webui
--no-slots
--offline
```

The child environment is cleared. Only these host variables may be inherited:
`PATH`, `PATHEXT`, `SYSTEMROOT`, `WINDIR`, `COMSPEC`, `TEMP`, `TMP`,
`USERPROFILE`, `LOCALAPPDATA`, `APPDATA`, `PROGRAMFILES`,
`PROGRAMFILES(X86)`, `CARGO_HOME`, `RUSTUP_HOME`, `LANG`, and `LC_ALL`.
Additional process environment entries remain subject to the typed validated
map. The production local launch plan supplies no arbitrary shell command.

The launcher first reserves an IPv4 `127.0.0.1` port. A running sidecar is
accepted only when the same PID and process start identity own exactly one TCP
listener, that listener is IPv4 loopback on the approved port, and no IPv6
listener exists for the process. A non-loopback IPv4 listener, any IPv6
listener, another port, a changed process identity, no listener at readiness,
or more than one listener fails closed.

Sidecar HTTP uses HTTP/1 only, ignores system proxies, refuses every redirect,
uses no idle connection pool, and has a two-second connect timeout. The
generation stream has no whole-request timeout because cancellation and lease
retirement bound its lifetime. Probe calls retain their bounded request
timeout.

The bearer key is 32 random bytes encoded as 64 lowercase hexadecimal
characters. It is held as a redacted secret and written to a create-new
owner-only file. The value is passed only through the key file, is not printed
in the launch plan, status, event, or normal error, and the file is removed
when its owner drops.

The disabled feature projection is exact:

```text
web-ui
slots-endpoint
remote-downloads
tools
mcp
agent
router
media
reasoning
```

Optional sidecar metrics are also not enabled by the fixed argument vector.
`metrics` is not part of the exact `disabled_features` projection above.

### Downloads, paths, and artifact identity

The model download has a two-hour whole-transfer deadline. The runtime bundle
download has a 20-minute whole-transfer deadline. Both use a 30-second connect
timeout. Their redirect policies allow only the exact approved HTTPS host
families. The deadline covers request send, body streaming, file sync,
integrity inspection, and publication.

Both downloads count and hash every byte, require the offer's exact size and
SHA-256, use create-new part files, and publish with a no-clobber hard link. A
timeout, redirect rejection, body error, size error, hash error, sync error,
inspection error, or publication race removes the part file and does not
replace an existing target.

On Windows, model roots, runtime roots, extracted session directories, part
files, archives, launchers, model files, and the key file receive a protected
owner-only DACL that is queried after application. Existing path components
are opened without following reparse points. Archive, launcher, and model
files are held by verified leases with canonical path, file identity, exact
size, and SHA-256. The archive, launcher, and model are rechecked immediately
before spawn. Their identities are checked again after the contained child
resumes. A failed check terminates the child and cleans the extracted session.

### Model and process limits

The selected model maximum defaults to 64 GiB. Configuration accepts values
from 1 MiB through 256 GiB. The current Qwen offer is exactly 833,592,096
bytes, has an offer hard limit of 2 GiB, and must also fit the configured
maximum. The runtime archive hard limit is 64 MiB.

The optional Windows offer remains Qwen3.5 0.8B Q8_0. Android uses the separate
Qwen3.5 0.8B Q4_0 offer at exactly 563,036,064 bytes with SHA-256
`57d1997790d1744fba5b40a7317df71ea5e2acee28c47e78f0cce39c0703f8cf`.
The Android model is absent by default and is not packaged in the APK.

### Android inference lifecycle

The Android module stores the full opaque `AuthorityGeneration`, not only its
public ordinal. Preparation, one-use approval, activation, and event admission
must match that exact generation. Two independent authority epochs with the
same ordinal do not match.

The module verifies one regular app-private GGUF leaf before load. The worker
repeats the full hash immediately before llama.cpp opens the path and again
after the native load returns. A mismatch unloads the native model and prevents
publication. The single FFI worker owns backend, model, context, sampling, and
unload. Replacement cancels the current stream and unloads the old model before
loading the replacement. Explicit unload and owner shutdown clear the active
model and backend projection.

The direct Android Settings control may request a visible non-exported
foreground service using the numeric generation. That number coordinates
start and exact stop only. It is not an authority token and cannot approve a
model or tool. Standard, Shizuku shell, and root remain separate authority
tiers. Only Standard is implemented in Milestone 3.

Every sidecar receives these managed limits:

- active process limit: 4 total processes in the Job Object;
- CPU hard cap: 95 percent, encoded as Windows CPU rate 9500;
- process and job memory:
  `clamp(model_size * 2 + 2 GiB, 4 GiB, 512 GiB)` using checked arithmetic;
- retained output limit: 1 MiB per stream for this launch plan; and
- SHA-256 work limit: the first 4 MiB per stream.

The current Qwen offer receives the 4 GiB minimum process and job memory
limit. Managed output continues draining after the hash prefix. Exit evidence
records complete stdout and stderr byte counts, hashed byte counts, digests,
truncation facts, and separate completion flags. A digest with
`hash_complete == false` is a prefix digest. Both capture tasks share one
five-second settlement deadline. A settlement timeout aborts and joins both
tasks instead of reporting incomplete evidence as complete.

On Windows, the child is created with `CREATE_SUSPENDED` and a new process
group. Before resume, Argentum creates the Job Object, applies and queries
kill-on-close, active-process, process-memory, job-memory, and CPU hard-cap
limits, assigns the suspended child, and runs the final authority check. Any
limit, query, assignment, or resume failure terminates and reaps the child.

## Temporal composition relationship

Argentum now has properties similar in principle to the spatial and temporal
composition described by the [DeepSeek Harness core documentation](https://deepseek-harness.github.io/deepseek-harness/en/reference/subsystems/core)
and the [Cordis paper](https://arxiv.org/abs/2608.25512): checked dependency
composition, atomic publication after setup, generation-bound handles,
reversible effects, reverse rollback and teardown, dependent-first retirement,
cooperative cancellation, stale-handle rejection, and retryable cleanup.

Argentum remains an independent Rust implementation. It has no DeepSeek
runtime dependency and does not claim parity with either system. Package
discovery, executable extension transport, signatures, persisted remount
intent, health supervision, bounded restart policy, process isolation for
third-party modules, forced task-abort recovery, and broad first-party module
migration remain deferred.

## Deferred work and readiness evidence

The following table is authoritative for deferred platform work. Compilation
or a source implementation alone does not satisfy these gates.

| Area | Current state | Evidence required before readiness can be stated |
| --- | --- | --- |
| Android host | Deferred | Active Gradle and manifest project, shared Rust host bridge, bounded JNI and Binder contracts, Android target checks, emulator install and launch, and ARM64 physical-device install and launch. |
| Android on-device inference | Deferred | Verified pinned native source and artifacts, inference service, Qwen3.5 0.8B Q4_0 offer, model hash check, streamed answer, cancellation, replacement, full unload, backend report, emulator evidence, and ARM64 device evidence. |
| Shizuku | Deferred | Direct user grant, observed UID 2000, bounded capability probe, command evidence, revoke and Binder-death cleanup, no late events, and proof that shell mode cannot read another application's private data. |
| Root | Deferred | Controlled rooted emulator or device, direct user grant, observed UID 0, SELinux context, bounded capability probe, tool evidence, revoke and service-death cleanup, and no security-control bypass. |
| Extensions | Deferred | Versioned bounded manifest, signature or explicit development trust, isolated host, capability subset enforcement, install, invoke, disable, update, rollback, remove, hostile-package rejection, orphan cleanup, and no late events. |
| Self-development and update | Deferred | Selected source workspace, reviewed changes, format, build, lint, test, package, artifact inspection, expected application identity and version, valid signing decision, exact update authority, staged install, health check, and rollback. The running program is never overwritten in place. |
| Native Slint UI | Deferred and excluded from the active workspace | Reactivated crates with dependency checks, one mutable `CommandHost` retained through the event loop, UI command-task drain, owner shutdown before async runtime release, desktop and narrow-layout visual checks, and interaction checks for task, approval, settings, and shutdown flows. |
| Packaging, signing, and device tests | Deferred | Clean-staging Windows artifact launch with hash, Android package and native-library inspection, explicit signing decision, emulator install and launch, ARM64 physical-device evidence, startup and restart checks, and recorded artifact hashes and sizes. |
| Linux | Deferred | Target-specific authority, secure storage, path, process, runtime, packaging, and runtime tests on Linux. Portable target guards do not establish product support. |
| macOS | Deferred | Target-specific authority, secure storage, path, process, runtime, application packaging, signing, and runtime tests on macOS. Portable target guards do not establish product support. |

Later milestone gates include the following commands and reviews. This list is
a requirement, not a statement that this documentation worker ran them:

```text
cargo fmt --all -- --check
cargo check --workspace --all-targets --locked
cargo test --workspace --all-targets --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo deny check licenses advisories bans sources
cargo check -p argentum-domain -p argentum-lifecycle -p argentum-security -p argentum-harness -p argentum-platform --target aarch64-linux-android --locked
```

Each later milestone also requires active-path secret, signing-material,
unsafe-code, Unicode-style, and binary-artifact scans; a security diff review;
exact scoped change inspection; artifact hashes; and the runtime or device
evidence named in the table. A missing physical device, root-capable test
environment, Shizuku setup, signing decision, or hardware backend remains
blocked or unverified. Desktop compilation, shared Android target compilation,
or emulator success does not substitute for the missing evidence.
