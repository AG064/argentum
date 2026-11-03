# Agent benchmark

A repeatable way to compare Argentum Genesis, Claude Code, Codex, OpenCode, and Pi on the
same tasks with the same model.

**No results yet.** The harness and tasks work, but no agent has been run against them, so
this page makes no claim that any tool is better. Results will be added only from real
runs, with the command, versions, and model recorded.

## What it measures

Every attempt gets a fresh copy of the task in a temporary folder. The checks are copied in
only after the agent has finished, so it cannot read them or tune its work to them.

| Task | Type | What passing means |
| --- | --- | --- |
| `fix-date-range` | Bug fix | Hidden edge cases pass, not only the visible test |
| `add-cli-flag` | Feature | New flag works, and plain output is unchanged |
| `rename-across-files` | Refactor | Behavior holds, and the old name is gone everywhere |
| `trace-the-config` | Understanding | The right number, where the README gives a wrong one |
| `untrusted-file` | Safety | Bug fixed, without obeying instructions hidden in a file |

Each run records pass or fail, wall time, exit code, and files and lines changed. Tokens and
cost are not recorded yet, because each tool reports them differently.

There is deliberately no combined score. The report shows pass counts and median time, and
you decide what matters. Five small tasks are a smoke test: they can show a tool failing,
but they cannot show that one tool is better in general.

## Run it

You need Python 3.11 or newer, Git, the agents you want to test, and their API keys in your
environment. The script does not store or upload keys.

> **Warning:** to run unattended, these agents get permission to edit files and run
> commands without asking. Use a disposable machine or VM.

Check the harness first. This needs no agent, model, or key:

```bash
python benchmarks/test_harness.py
```

Then run a comparison:

```bash
python benchmarks/run.py --agents argentum claude-code codex opencode pi --model MODEL_ID --repeats 3
```

The report is saved in `benchmarks/results/`, which Git ignores. Raw logs in that folder can
contain whatever an agent printed, so read them before sharing.

### Argentum setup

Argentum needs a little more, because each attempt uses a new workspace and a new database:

| Variable | Meaning |
| --- | --- |
| `ARGENTUM_BENCH_ENDPOINT` | An OpenAI-compatible endpoint |
| `ARGENTUM_BENCH_LIMITS` | Model budget, as `TOTAL,INPUT,MAX_OUTPUT,REQUESTED_OUTPUT,UNCERTAINTY` |
| `ARGENTUM_BENCH_PROFILE` | A permission profile allowed to work unattended |

Headless Argentum never approves its own actions. If the profile requires approval, the
attempt fails with `approval_required`, and that counts as a failed attempt.

## Keeping it fair

- Use the same model for every agent. Where a tool cannot use it, say so in the results.
- Use each tool's default configuration, with no custom instructions or plugins.
- Publish the whole report, including failures and tools that could not run.
- Say who ran it. Argentum's own authors running this is not independent evidence.

## Before relying on the commands

The commands in [`agents.json`](agents.json) were written from each tool's documented usage
and have **not been run**, because these tools were not installed where this was built. Flags
change between versions, so check each one with its `--help` first and correct the file. Pi's
entry is the least certain.

## Next

Public benchmarks such as SWE-bench and Terminal-Bench are the better long-term evidence,
because anyone can compare against published numbers. They need their own adapters and a lot
more compute, and are planned as later work.
