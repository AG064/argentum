# Goals

A goal gives a session one objective and a set of limits. The agent can then work toward it
across several runs, and Argentum counts the tokens, model requests, tool calls and time
those runs use. Limits stop work when they are reached. A run that ends normally does not
mean the goal is achieved. A goal is done only when your declared checks pass, or when you
confirm it.

Goals are for work that takes more than one run. Ordinary prompts are not part of a goal
unless you send them as goal tasks, as described below. Token counts are estimates made
before a request and settled with the provider's reported usage afterward. They are not a
billing guarantee, and Argentum does not price usage in money.

## Set a goal and its limits

```powershell
argentum goal set --objective 'Migrate the parser and keep the tests passing' --token-budget 1000000 --tool-budget 200 --time-budget 3600
argentum goal status --json
```

`goal set` creates the session's goal, or changes the objective and budgets of the current
goal without erasing the work already recorded. `--time-budget` is in seconds. `goal status
--json` shows the goal's ID, limits, counters, the estimated allowance left, and the most
recent runs: up to 16 runs and 128 request records, with unresolved requests listed first.
Counters cover the whole goal, even when the lists are cut off.

For finer control, `goal limits` takes nine positive numbers, separated by commas, in this
order:

1. Tokens for the whole goal.
2. Model requests for the whole goal.
3. Runs (iterations) for the whole goal.
4. Tool calls for the whole goal.
5. Seconds for the whole goal.
6. Tokens per run.
7. Model requests per run.
8. Tool calls per run.
9. Seconds per run.

```powershell
argentum goal limits --goal-id GOAL_ID --limits 1000000,256,128,768,3600,500000,32,256,300 --json
```

Raising a limit never resets what a goal has used. Lowering a limit below what it has used
stops further work. Limits can only narrow what the workspace policy already allows. They
never grant file, process, network or approval authority. The model cannot raise limits.

`goal pause`, `goal resume` and `goal clear` control the goal's state. Pausing and resuming
never start work by themselves. Clearing archives the goal, and a new goal starts at zero.

The objective is shown to the model in every run of the session while the goal is active.
It is labelled as your text and grants no authority. A paused, completed or cleared goal is
not sent.

## Run work against a goal

- Terminal: type `/goal-run GOAL_ID TASK` in chat. Each queued task keeps its own goal ID.
  From the command line, use `argentum run --goal GOAL_ID --prompt TEXT --json`.
- Desktop and Android: the goal panel sits above the composer. While the Goal module is on
  and the goal is active, each task you send counts against the goal. If the goal was
  cleared, replaced or paused, Argentum rejects the task instead of using another goal.
  With the goal paused, or the module off, tasks run as ordinary runs outside the goal.

Manual `compact` runs are maintenance and do not count against a goal. Use
`argentum compact --goal GOAL_ID --json` to charge a summary to the goal.

## Completion

### Checks

Declare one to four verification runners as the goal's criteria:

```powershell
argentum goal criteria --goal-id GOAL_ID --runners cargo-fmt-check,cargo-check-locked --json
```

The runners are `cargo-fmt-check`, `cargo-check-locked`, `cargo-test-locked` and
`cargo-clippy-strict`. Approved project runners are written `project:ID`. See
[Goal delivery](NATIVE_DELIVERY.md). Changing the criteria creates a new version. Add
`--owner-attestation-required` if the work also needs your own sign-off, and give it with
`goal attest --goal-id GOAL_ID --summary TEXT`.

A check passes only when its runner completed against the current files. A result from
before a file changed, a skipped run, a failure, a cancellation or a missing result is not
a pass. The files are fingerprinted before and after the runner, and a change in between
makes the result out of date. The fingerprint covers all regular files in the workspace
except `.git` and `target`, up to 16,384 files, 64 MiB in total and 16 MiB per file. If the
workspace is larger, the result is unknown, not passing.

When a goal completes, Argentum records "verified against the declared criteria" together
with the evidence. Later changes to files or criteria do not rewrite that record.

### Supervise

`goal supervise` runs a goal in the foreground until the checks pass or a limit is reached:

```powershell
argentum goal supervise --goal-id GOAL_ID --iterations 8 --elapsed-seconds 3600 --json
```

`--iterations` is from 1 to 16 and `--elapsed-seconds` is from 1 to 86,400. They narrow the
goal's limits for this invocation and never reset them. Each pass first checks the current
evidence. If every check already passes, the goal completes without another model request.
Otherwise a repairable failure starts one more run with the objective and the unresolved
results. The invocation stops when anything is uncertain: a runtime, provider, storage,
approval, usage, context or deadline problem, or a cancellation. It also stops when two
checkpoints in a row show the same source and the same check results, so repeating work
without progress does not use up your budget. Argentum does not retry a failed provider
request automatically. To continue, run `goal supervise` again.

### Deliver

`goal deliver` is the staged version, with a separate coding turn, check runs and a
feedback turn. See [Goal delivery](NATIVE_DELIVERY.md).

### Model completion reports

In a run tied to a goal, the model can call `report_goal_complete` with a summary of up to
1 KiB. This records a report on the goal and tells the model the goal is not complete yet.
It does not complete the goal. The goal panel shows the report with two buttons. **Confirm
done** marks the goal complete. **Not yet** drops the report and keeps the goal active.
Changing the objective clears a pending report. A report from a run outside the goal, for a
goal that has since changed, or with an empty or oversized summary, is refused and the run
continues.

## Limits that apply

Time counts model work, summaries, tools and approval waits, and not time spent waiting in
the queue. When time runs out, Argentum cancels the run and waits for its work to finish.
That is not instant. The run ends as Failed with `goal_deadline_exceeded`, and any
completed work is kept. An iteration is one goal run that started, including failed and
cancelled ones. A request that is rejected before it starts is not an iteration. A tool
call counts once it is validated and authorized, even if it then fails.

Each request reserves its expected usage before it is sent. If the allowance is too small,
Argentum does not send it and does not shrink the output or drop context to make it fit. A
request that was sent but never reported its usage stays unresolved and blocks that goal.
Cancelling, timing out or restarting does not prove that no tokens were used. Usage above
the limit is recorded in full and blocks more work.

Common stop reasons are `goal_budget_exhausted`, `goal_request_limit`,
`goal_iteration_limit`, `goal_tool_limit`, `goal_deadline_exceeded`,
`goal_usage_unresolved`, `goal_usage_overrun`, `goal_accounting_unresolved` and
`goal_unfinished_run`.

Only one Argentum process can run a goal at a time. Another process that tries to use the
same session is refused. Network drives are not supported for the workspace or database
folder, because this protection works only on the same computer. After a crash, Argentum
pauses affected goals and does not restart them. It never replays a request or assumes a
refund. Recovered work can be restarted with `goal supervise` or `goal deliver`.

## Resolve unknown usage

If a request's usage is unknown, `argentum accounting inspect --session-id SESSION_ID
--goal-id GOAL_ID --request-id REQUEST_ID --json` shows that request without changing
anything. `argentum accounting resolve --resolution FILE --json` records your decision,
either the usage found in a saved settlement event, or, for a missing report, accepting the
whole reservation as the charge with `--confirm-reservation UNITS`. Accepting a reservation
is a budget decision, not a statement of what the provider billed. Both commands are for
you only and are not available to the model, JSONL or a paired device. Resolving does not
resume the goal. After that, run `goal deliver` or send a new goal task.
