# Explicit goal accounting and bounded supervision (F07)

F07A provides client-side estimated admission with observed settlement, not a
provider billing ceiling. F07B adds one explicit finite foreground control loop
whose completion is limited to declared current evidence. No pricing feed,
dollar-cost enforcement, daemon, scheduler, general natural-language evaluator,
or autonomous background supervisor is implemented. A task finishing normally
does not establish that its goal was achieved.

## Owner controls and scope

`argentum goal set --objective TEXT --token-budget N` creates the current session's
goal, or changes that goal without resetting its recorded work. `goal status
--json` exposes its identity, accounting version/start point, supported limits,
counters, estimated remaining allowance, and bounded execution inspection.

`argentum run --goal GOAL_ID --prompt TEXT --json` explicitly associates only that
submission. The interactive composed-host controller supports `/goal-run GOAL_ID
TASK`; each queued entry retains its own goal identity. Ordinary prompts are not
automatically enrolled. The scripted client does not support this owner control
and rejects the association rather than silently running outside its quota.
F19A read-only delegation is unavailable inside supervised goal attempts, so a
child cannot create an untracked request or tool-accounting branch beneath the
finite foreground supervisor.

`argentum compact --goal GOAL_ID --json` charges manual summarization to that goal.
An owner-issued `compact` without a goal is maintenance outside goal quotas and
is labelled as such. It still obeys F06B. Deactivation and `/context` are
provider-free. Concurrent or ambiguous compaction fails existing admission.

`argentum goal limits --goal-id GOAL_ID --limits VALUES --json` accepts nine
positive comma-separated values in this order:

1. Cumulative goal tokens.
2. Goal model-request attempts.
3. Goal-run iterations.
4. Goal tool admissions.
5. Cumulative admitted goal seconds.
6. Per-run tokens.
7. Per-run model-request attempts.
8. Per-run tool admissions.
9. Per-run seconds.

Existing resource bounds still apply. Limits narrow those bounds; they do not
grant file, process, network or approval authority. These controls are trusted
owner paths, not new JSONL commands. Model text cannot raise limits or settle
charges. Pause/resume changes eligibility, never launches work. Budget increases
do not launch queued entries. A successful outer supervised invocation may release
its dependent queue entry. Incomplete, failed, cancelled, stalled, exhausted, or
review-required outer outcomes leave dependent work paused for explicit `/queue
continue`. That command resolves the same goal's current revision and performs
admission again. A cleared or replaced identity is not substituted.

## Bounded foreground supervision

An owner first declares one to four fixed typed verification criteria:

```text
argentum goal criteria --goal-id GOAL_ID --runners cargo-fmt-check[,cargo-check-locked,cargo-test-locked,cargo-clippy-strict] --json
```

Each selected runner is unique. The stored criterion binds its runner, registry
version, exact fixed definition digest, and verification input scope. Changing
the criteria creates a new version and rotates the goal definition. An optional
`--owner-attestation-required` flag keeps a separate bounded owner attestation
requirement for subjective work. Model text, repository text, summaries, protocol
payloads, and verifier selection do not grant execution authority or change
criteria.

The owner starts exactly one foreground invocation with:

```text
argentum goal supervise --goal-id GOAL_ID --iterations N --elapsed-seconds N --json
```

Iterations must be from 1 through 16. Elapsed seconds must be from 1 through
86,400. These caps narrow the existing cumulative and per-run F07A limits. They
never reset allowance between attempts. Goal resume only changes lifecycle
eligibility. A later invocation always requires another explicit supervise
command and fresh admission against the current definition, accounting revision,
session, canonical workspace, and durable ownership claim.

One invocation uses sequential linked goal-run attempts. Before each attempt it
evaluates current evidence. A current all-pass state can settle without another
model request. Otherwise, a normally settled repairable failure can create one
bounded continuation using the goal objective, unresolved outcomes, eligible
prior observations, and freshly loaded F05 instructions. Runtime, provider,
storage, approval, usage, policy, context, deadline, cancellation, or freshness
uncertainty stops the outer invocation. Provider failures and HTTP errors are not
retried by the supervisor.

The no-progress heuristic compares the relevant source snapshot and normalized
criterion outcomes. The second consecutive identical checkpoint stops the
invocation. Attempt IDs, timestamps, token use, summaries, and model assurances
do not count as progress. Only the newest eight progress checkpoints are kept.
The iteration and elapsed-time limits remain decisive.

The process-local owner claim holds the session across attempt transitions and
rejects competing ordinary or supervised admission. SQLite stores an active claim
so another process conservatively rejects reentry. F08 binds that claim to a
Windows local-store owner generation guarded by a physical-file identity and
named lifetime mutex. Network-backed and unknown volume types are unsupported
because the mutex is same-machine only. Live or ownership-unknown claims remain
fenced. Pre-F08 active invocations and unfinished goal runs remain explicit
legacy ambiguities even when no nonterminal status event was recorded. Proven
owner loss allows bounded reconciliation to retire the invocation claim, record
Interrupted, and pause the affected goal. There is no ownership timeout, durable
retry engine, or F09 replay protection. Session switching latches a pause before
any next attempt; switching back does not restart it. Stop, explicit goal pause,
and shutdown cancel and drain owned work before releasing live ownership.

## Acceptance evidence and freshness

A criterion passes only from a completed typed receipt for the expected runner,
registry definition, canonical workspace, input scope, and exact current source
snapshot. `workspace_revision_v1` hashes relevant regular-file paths and contents,
including dirty source and configuration, while excluding only `.git` and
`target`. It is bounded to 16,384 files, 64 MiB total, and 16 MiB per file.
Unavailable or exceeded coverage is Unknown and cannot be presented as current.

Verification captures this snapshot before and after the fixed runner. A changed
snapshot during verification is stale. The supervisor captures it again before
publishing completion. No receipt, skipped command, old pass, unrelated zero exit,
Failed, Cancelled, Unavailable, Unknown, or stale evidence is a pass. Empty or
unsupported criteria fail closed. A passing formatter receipt establishes only
the declared formatting criterion.

Completion is stored as `verified against the declared criteria` with the source
snapshot and exact evidence IDs. Later source or criteria changes leave that
receipt historical rather than rewriting it. The historical failed
`cargo-fmt-check` receipt remains Failed when a later run passes. Read-only status
may compare stored receipts with current bounded inputs, but it never executes a
verifier, model request, or repair.

## Counters and request settlement

An iteration is one admitted explicit goal-run attempt, including failed and
cancelled attempts. Validation rejection before admission is not an iteration.
Manual compaction is maintenance, not another iteration. Request attempts count
successfully persisted reservations, including an attempt proven not dispatched.
Tool admissions count invocations after validation and authorization; a started
tool which fails still counts. A synchronous failure before invocation releases
its unused tool slot. Denied or malformed proposals are not tool executions.

Every associated main call and automatic/manual summary reserves the F06B exact
profile's accounted input, output allowance and uncertainty before dispatch.
Insufficient allowance sends neither a main call nor a free summary. Output is
not reduced and context or tools are not dropped to evade the limit. The model's
context-window capacity, requested output bound, and cumulative goal allowance
are different quantities. Contained local accounting remains explicitly
unsupported and cannot bypass these checks.

Usage settlement uses final provider totals, not sums of cumulative snapshots.
Repeated equal reports settle once. Cache and reasoning details are components,
not additional charges. Known zero, total-only usage with unavailable components,
missing final usage, and invalid/conflicting final usage have separate labels.
Full trustworthy usage survives invalid summary output or subsequent failure.
Observed overrun is recorded in full and blocks more goal work, not capped at the
limit or represented as prevented expenditure.

Dispatched requests with unknown usage retain unresolved reservations and block
that goal. Cancellation, timeout, disconnect and restart do not prove inference
was free. Only evidence of no dispatch permits release without consumption.
Failure to persist a reservation prevents dispatch. A post-consumption settlement
failure retains its reservation and publishes bounded observed evidence; an
independent existing event write is attempted without repeating inference.
If storage itself is unavailable, durable preservation cannot be guaranteed.

## Time, edits and restart

Live deadlines use a monotonic clock from admission. They include model work,
summarization, tools and approval waits, but exclude queue residence. The admitted
run captures its time ceiling and remaining goal time; fractional remaining goal
seconds round down. Limit edits cannot extend this captured deadline. Subsequent
request/tool admissions use current limits as well as the captured per-run
ceilings. Goal time sums completed admitted intervals. Concurrent runs for one
goal are rejected, including unfinished intervals from a prior process.

Expiry initiates existing cancellation and owned-work drain. Unclaimed approvals
retire; claimed work must drain. It is not instantaneous termination of arbitrary
effects. Completed effects and receipts remain. The terminal run is Failed with
`goal_deadline_exceeded`, not successful task completion or goal achievement.

SQLite transactions serialize reservations and updates. Attempt identity and
goal/run/configuration bindings survive budget edits, clear, and late settlement.
Duplicate settlement does not recharge. A limit increase never resets counters;
lowering a limit below consumption prevents more work. Clear archives the old
identity. A fresh goal does not relabel old work as settled.

On restart, recorded totals, invocation outcomes, linked attempts, and completion
receipts reload without replay. A new explicit invocation can reuse exact current
passing receipts without inference or verifier execution. F08 recovery interrupts
unfinished generation-bound runs only after host loss is proven. A prepared but
undispatched request can release its reservation as NotDispatched. Dispatched,
partial, or otherwise uncertain usage stays unresolved. Because elapsed duration
for an abandoned run cannot be reconstructed, the run-level accounting state may
remain Unresolved even when its request charge is ProvenUndispatched. Settled
charges and terminal outcomes remain unchanged and are not charged twice.

Recovery pauses affected goals and retires their invocation claims, approvals,
responders, tickets, and grants. It never recreates memory-only queues, retries a
request, resumes a supervisor or compaction, restores authority, assumes a refund,
or labels an abandoned run complete or failed. Exact per-run review only clears
that session admission hold; unresolved usage continues to block dependent goal
work. Unrelated sessions remain usable. Legacy goals are migrated additively with
incomplete-history metadata; historical zeros are not proof of zero usage. Their
remaining allowance is unavailable, even after a budget increase.

## Inspection and errors

### Exact owner accounting recovery

`accounting inspect --workspace PATH --database PATH --session-id SESSION_ID
--goal-id GOAL_ID --request-id REQUEST_ID --json` opens the existing store
read-only. It returns bounded metadata for that exact reservation, including
physical store identity, the current accounting snapshot, original ownership
and effect state. It does not select a session, migrate, reconcile, contact a
provider or start work. Unknown, foreign and oversized records fail closed.

The direct local-owner command `accounting resolve --workspace PATH --database
PATH --resolution FILE --json` consumes a host-issued exact authority ticket.
The bounded JSON file contains `command_id`, the full inspected `binding`, and
one `decision`. These controls are absent from AppCommand, JSONL and model
tools. They do not replace or supply the physical owner confirmation required
by other controls, and they do not grant process or file authority.

For retained trustworthy usage, the decision is `authoritative_usage` with
`event_id` and `usage`. It must match a retained, request-bound typed settlement
event; operator-supplied token numbers alone are insufficient. A retained
observation must be reconciled before an estimate can be accepted.

For a specifically owner-authorized missing observation, the decision is
`owner_accept_reservation`. The command also requires
`--confirm-reservation EXACT_UNITS`. It retains the entire reservation as a
nonrefundable client-budget charge, not provider usage or a billing upper bound.
The original uncertainty, failed outcome and inactive summary remain retained.
Live ownership, unfinished effects, changed scope or snapshot and all remaining
admission guards still apply. Recovery acknowledgement is not a substitute for
this separate decision.

Charge, run, goal and durable command identity commit atomically. Equal delivery
does not charge again; a conflicting identity or stale new command fails.
Resolution never resumes a goal or repeats a request. After valid resolution,
use an explicit same-goal `goal deliver` or other supported owner submission.
Every other unresolved request remains a stop under the unchanged default.

Later authoritative evidence is retained separately through the same bounded
owner path or canonical settlement. It does not replace the original decision.
A lower observation does not refund the accepted charge. Higher consumption
increases the effective charge and retains the canonical overrun stop.
Conflicting observations are retained and stop admission. Goal status exposes
observed usage, owner-accepted units, effective charge and incomplete original
provider-usage coverage separately. A policy disposition is not verification.

Version1 goals remain readable when they contain no policy charges. Resolution
upgrades the goal to version2 with its required disposition-format marker.
Older executables cannot admit version2 goals. If an older writer strips the
marker, the successor also rejects that incomplete format instead of treating
the lost charge fields as zero.

`goal status --json` returns at most 16 recent runs and 128 request records,
prioritizing outstanding/unresolved requests, with explicit truncation flags.
Counters remain cumulative. Records contain identities, limits, method, observed
components, reservation quality and stop reason, not prompts, instructions,
arguments, credentials or summary bodies. Controller/native goal displays use
the same counters. Engine/controller checks are not packaged terminal visual QA.
The same JSON result also reports the latest invocation identity, declared
criteria, current or historical evidence state, linked attempts, bounded progress,
outer stop reason, and whether an interrupted claim blocks reentry. The private
owner claim token is never included.

Stable stops include `goal_budget_exhausted`, `goal_request_limit`,
`goal_iteration_limit`, `goal_tool_limit`, `goal_deadline_exceeded`,
`goal_usage_unresolved`, `goal_usage_overrun`, `goal_accounting_unresolved`,
`goal_unfinished_run`, and `legacy_usage_history_unavailable`. Existing F06B
profile/capacity errors remain unchanged. Remaining tokens are explicitly an
estimated admission allowance or unavailable, never a billing guarantee.

F09 external command replay, provider retries, in-run steering, subagents,
background work, general verification registration, contained-local accounting,
packaged TUI interaction, live-provider acceptance, and release acceptance remain
separate.
