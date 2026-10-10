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

The model reports a bounded JSON work status: `ready`, `continue` or `blocked`,
an explicit `remaining` list, and an optional short summary. This report is an
untrusted work claim. Prose, plans, malformed reports and unfinished work cannot
make passing declared checks sufficient for completion.

Current verifier output is supplied as untrusted, source-bound feedback to the
next model request through the shared request builder. The model can correct a
repairable failure without another owner prompt. Raw verifier output is not a
new durable-history source. After restart only actually retained receipt metadata
is available; the application does not reconstruct missing diagnostics.

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
