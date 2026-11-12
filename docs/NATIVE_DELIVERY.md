# Goal delivery

`goal deliver` is a command you run yourself, in the foreground, to work toward a goal in
stages: write code, run the checks you declared, and feed any failures back to the model
until the checks pass or a limit is reached. Other commands, including `run`, chat and
`goal supervise`, work as before. For goals in general, see [Goals](GOAL_ACCOUNTING.md).

## Set up and run

Use the workspace and database you already configured. Select a provider and a permission
profile first. The provider needs a saved model budget. The example uses a project runner
you have already approved. Listing a runner as a criterion does not approve it.

```powershell
$exe = 'C:\path\to\argentum.exe'
$ws = 'C:\path\to\project'
$db = 'C:\path\to\argentum.sqlite3'
$goal = (& $exe goal set --objective 'Your objective and acceptance requirements' --token-budget 10000000 --tool-budget 768 --time-budget 7200 --workspace $ws --database $db --json | ConvertFrom-Json).goal.id
& $exe goal limits --goal-id $goal --limits 10000000,256,128,768,7200,10000000,32,256,300 --workspace $ws --database $db --json
& $exe goal criteria --goal-id $goal --runners project:YOUR-APPROVED-RUNNER,cargo-check-locked --workspace $ws --database $db --json
& $exe goal deliver --goal-id $goal --iterations 16 --elapsed-seconds 7200 --workspace $ws --database $db --json
```

The numbers are examples. Choose limits that fit your task. `goal deliver` needs limits for
the whole goal and for each run (the nine values of `goal limits`). The `--iterations` and
`--elapsed-seconds` values of one invocation can only narrow what the goal has left.
Charges from earlier invocations still count.

## What happens

Each stage, the coding turn, every required check, and the feedback turn, is a separate
run inside one invocation. Argentum starts a fresh handoff before time or context runs out,
as long as the full request still fits. It does not drop required messages to make room.

At the end of a coding or feedback turn, Argentum holds back part of the run's time so the
model can write a work report: a third of the run's time, rounded up and at most 90
seconds. If the time left in the invocation drops below that, the invocation stops as
incomplete with `invocation_time_reserve`, without starting another coding request. Your
work and its accounting are kept. Start a new invocation to continue the same goal.

The model ends each turn with a short JSON work report: a status of `ready`, `continue` or
`blocked`, a list of what remains, and an optional summary. The report is the model's
claim, not proof. A report that is malformed, or that lists unfinished work, keeps the goal
open even when the checks pass.

After a check runs, its output goes to the model as feedback tied to the source it checked,
so the model can fix a failure without asking you. Raw check output is not saved to
history. After a restart, only the saved result records remain.

A passing result from an earlier task is history, even if the files still match. Delivery
runs the checks again for the current invocation. Each declared check runs on its own. A
check the model already ran in this invocation can be reused only if the source and runner
are identical.

The goal is complete when all of these hold:

- Every declared check passes against the current files and runner definitions.
- All token and request accounting has settled.
- The model's report acknowledges the latest feedback and lists no unfinished work.
- If you required an attestation, you gave it. Neither the model nor the supervisor can.

Complete means verified against your declared checks. It is not proof that every
requirement in your objective is met.

## Status, cancel and resume

`goal status --json` shows the invocation, each stage attempt, the criteria, the stop
reason and the accounting. Reading status never starts model work. Pausing, cancelling or
closing Argentum waits for running jobs to finish. After an interruption, follow the
recovery steps in the [CLI guide](CLI_GUIDE.md#inspect-and-recover), resume the goal, and
run `goal deliver` again with the same goal ID and the allowance you still want to give.
Unfinished work is kept. Earlier file changes and checks are not repeated. If the source or
the verification setup changed, the checks run again. If a runner changed, you must approve
it again. Updating the criteria does not restore an approval.

Delivery also stops when usage is unknown, a runner definition changed, evidence is out of
date, context summarizing makes no progress, required content does not fit, or the model
makes no progress. There is no background scheduler.

## Project runners with declared inputs

A project runner is a command you define in `.agents/verification.yaml`. You can add an
`inputs` section that lists the installed dependencies and generated files the command
uses. Without `inputs`, the runner checks the source files only: up to 16,384 files, 64 MiB
in total and 16 MiB per file, skipping `.git` and `target`.

```json
{
  "version": 1,
  "runners": [{
    "id": "web-check",
    "label": "Web project tests and build",
    "program": "node.exe",
    "args": [".agents/check.mjs"],
    "cwd": ".",
    "timeout_ms": 180000,
    "output_limit": 65536,
    "inputs": {
      "program": { "sha256": "PROGRAM_SHA256", "bytes": 123456 },
      "fixed_files": [
        { "path": ".agents/check.mjs", "sha256": "FILE_SHA256", "bytes": 2048 }
      ],
      "dependency_roots": ["node_modules"],
      "generated_output_roots": ["build/src", "dist"],
      "dependency_limits": { "max_files": 16384, "max_total_bytes": 268435456, "max_file_bytes": 134217728 },
      "generated_limits": { "max_files": 4096, "max_total_bytes": 67108864, "max_file_bytes": 16777216 },
      "inventory_limits": { "max_entries": 16384, "max_depth": 64 }
    }
  }]
}
```

The program and each fixed file are pinned by SHA-256 and size. Dependency and generated
roots must be folders inside the workspace, with at most eight roots in total. List only
real output folders as generated. Do not list a folder that mixes source and output. Every
limit is required. Each role can hold up to 65,536 files, 1 GiB in total and 256 MiB per
file, and the whole declaration up to 2 GiB. The folder inventory is capped at 131,072
entries and a depth of 128. Links, special files, overlapping roots and missing inputs are
rejected.

Argentum reads the dependency and output files themselves, including native libraries. A
lockfile does not stand in for them. A build that writes into the declared output folders
is allowed. If those outputs are later swapped or removed, the earlier result is out of date.

Approval is exact. `verification approve RUNNER` shows the full escaped declaration in an
interactive console and requires you to type `APPROVE`. If you change any root, limit,
pinned file or the program, you must approve again. Only approve runners whose code you
have reviewed. Runner code runs with your user's permissions. The inputs list does not
sandbox it, give it network access, or prove your tests are good.
