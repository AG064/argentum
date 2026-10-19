# Native bounded project delivery

`goal deliver` is an explicit foreground owner command. It reuses the existing
goal, supervisor, context, job, accounting, verification and recovery paths.
Ordinary `run`, chat and `goal supervise` keep their existing behavior.

## Configure and execute

Use the intended workspace and its existing database for every command. Select
the saved provider and execution policy using normal owner controls first.
Provider readiness requires a usable persisted, exact-profile model budget.
The following example selects an already approved project runner. A declaration
of criteria is not execution approval.

```powershell
$exe = 'C:\path\to\argentum.exe'
$ws = 'C:\path\to\project'
$db = 'C:\path\to\existing\argentum.sqlite3'
$goal = (& $exe goal set --objective 'Your project objective and public acceptance requirements' --token-budget 10000000 --tool-budget 768 --time-budget 7200 --workspace $ws --database $db --json | ConvertFrom-Json).goal.id
& $exe goal limits --goal-id $goal --limits 10000000,256,128,768,7200,10000000,32,256,300 --workspace $ws --database $db --json
& $exe goal criteria --goal-id $goal --runners project:YOUR-APPROVED-RUNNER,cargo-check-locked --workspace $ws --database $db --json
& $exe goal deliver --goal-id $goal --iterations 16 --elapsed-seconds 7200 --workspace $ws --database $db --json
```

These are finite example allowances, not provider capacity, billing guarantees
or authority. Choose tighter task-appropriate limits. Native delivery requires
explicit aggregate and per-run limits. Invocation limits only narrow the saved
allowance; subsequent invocations retain prior charges. Output reservation and
model capacity come from the selected provider's canonical budget resolver.

## Native stages and completion

Coding, each required verifier, and feedback receive separately admitted run
identities within the same active invocation. The workflow schedules handoffs
before the coding round, time or context reserve is exhausted. A context handoff
is scheduled only while the complete request still fits. It never discards
mandatory exchanges, repairs old digests or overrides a capacity failure.

Coding and feedback reserve a bounded third of the admitted per-run time,
rounded up and capped at 90 seconds, for a report-only turn carrying unseen
tool feedback and owned settlement. This does not increase any deadline,
request allowance or output limit. A provider can still exceed the remaining
time; missing terminal usage remains a hold, not a fabricated settlement.

The enclosing invocation does not receive a new clock after a handoff. Before
admitting a subsequent implementation stage, native delivery checks the actual
remaining parent time against that same per-run report reserve. Inside the
reserve it stops incomplete with `invocation_time_reserve`, preserving the work
report and settled accounting without dispatching another coding request. A
fresh explicitly authorized invocation can resume the same goal under its
remaining aggregate allowance. Initial short invocations, ordinary supervision,
verification and final feedback retain their existing admission/deadline rules.
This is not a prediction that the remaining checks will fit or a guarantee
against provider latency or missing terminal usage.

The model reports a bounded JSON work status: `ready`, `continue` or `blocked`,
an explicit `remaining` list, and an optional short summary. This report is an
untrusted work claim. Prose, plans, malformed reports and unfinished work cannot
make passing declared checks sufficient for completion.

Current verifier output is supplied as untrusted, source-bound feedback to the
next model request through the shared request builder. The model can correct a
repairable failure without another owner prompt. Raw verifier output is not a
new durable-history source. After restart only actually retained receipt metadata
is available; the application does not reconstruct missing diagnostics.

A passing receipt from an earlier task remains historical evidence, even when
its source still matches. Native verification obtains current evidence inside
this invocation before its feedback/completion stage. Multiple declared checks
retain separate verifier runs. A check already performed by the model in this
invocation may be reused only under the same canonical source/runner checks.

Completion requires passing declared checks against current inputs and runner
definitions, settled accounting, and a model work report acknowledging the
current feedback with no unfinished work. Any selected owner-attestation
requirement remains a separate exact owner action. Neither the supervisor nor
the model supplies that attestation. The result means verified against declared
checks, not independent proof of every natural-language requirement.

## Status, cancellation and resumption

`goal status --json` reports the invocation, individual stage attempts, current
criteria, stop reason and aggregate accounting. Inspecting or reopening does not
start provider work. Existing owner pause, cancellation and host shutdown paths
drain owned jobs. A real interruption retains ownership/recovery fences.

After supported recovery and explicit owner resumption, invoke `goal deliver`
again with the same goal ID and the remaining authorized allowance. Bound
unfinished work is retained, but old effects and verification are not replayed.
Changed source or verification configuration needs current evidence. Changed
runner bindings require the existing exact approval path, not a criteria update
that silently restores permission.

Unknown usage, revocation, definition changes, stale evidence, nonreducing
compaction, mandatory-content overflow and genuine no progress remain stops.
There is no background scheduler, unrestricted model shell, new authority store
or claim of general autonomous reliability.

## Explicit project input roles

Project runners may opt into `inputs` in `.agents/verification.yaml`. Without
it, the existing `workspace_revision_v1` source limits and semantics remain
unchanged. Opt-in is an owner-reviewed execution contract, not a package-manager
heuristic or a way for model arguments to exclude source.

The declaration contains the selected executable's `program` SHA-256 and byte
length, optional `fixed_files` with relative paths, SHA-256 and byte lengths,
explicit `dependency_roots` and `generated_output_roots`, and required
`dependency_limits`, `generated_limits` and `inventory_limits`. Identity limits
name `max_files`, `max_total_bytes` and `max_file_bytes`; inventory limits name
`max_entries` and `max_depth`. No opt-in limit has a silent default. The normal
bare executable name, fixed arguments, contained working directory, timeout and
output limit remain part of the runner declaration.

All remaining source, including new and untracked source files, is observed
under the unchanged 16,384-file, 64 MiB aggregate, 16 MiB per-file source bounds.
Only the pre-existing `.git` and `target` exclusions and the explicit separate
roles are outside that source set. Fixed controller/configuration files remain
source inputs and must match their declared content identities. Do not declare
an entire mixed source/output directory as generated. For example, if `build/`
also holds retained TypeScript, name only its actual generated subdirectories.

Installed inputs are streamed from their actual contained files, including
native libraries and package resolution metadata. A lockfile or cached manifest
does not stand in for those bytes. The resolved verifier executable is observed
in the same installed-input byte/file budget. Each declared role is bounded to
65,536 files, 1 GiB total and 256 MiB per file, with at most eight roots; the
combined declared source/dependency/output allowance cannot exceed 2 GiB.
Inventory is bounded to 131,072 entries and depth 128. Declarations may be
tighter. Links, special entries, missing required inputs, overlaps, unsupported
paths, exceeded bounds and incomplete capture fail closed. Streaming uses a
64 KiB buffer and checks cancellation between reads; individual OS file calls
are not preemptible. Metadata-only readbacks are bounded observations.

Generated outputs have separate before/after identities. A legitimate build
does not count as source mutation, but a later substituted, missing or changed
output makes its receipt non-current. Current completion checks the same source,
installed-input, runner, workspace and output identities, including after a
fresh process starts. Multiple criteria currently require one shared input
contract. Changing roles, bounds, fixed identities or the resolved executable
changes the exact approval binding and criteria definition. Scoped approval
prints the full escaped review in a real owner console and requires `APPROVE`;
old approval cannot authorize a changed binding.

These are endpoint observations, not proof that files did not change and change
back during a process. Existing outputs may be inputs to incremental work, so
their before identity is retained. This contract does not discover every
transitive host dependency, prove a test suite is adequate, restrict arbitrary
current-user verifier code, create an OS sandbox or grant network access.
Existing process, approval, cancellation, accounting and completion rules still
apply. Use only reviewed fixed verifier code and declare all relevant project
inputs. Built-in Rust checks and existing edit representations are unchanged.
