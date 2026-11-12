# Agent benchmark

A repeatable way to compare Argentum Genesis, Claude Code, Codex, OpenCode and Pi on the
same tasks with the same model.

**No results yet.** No agent has been run against these tasks, so this page makes no
claim that any tool is better. Results will be added only from real runs, with the
command, versions and model recorded.

This is a small smoke test that runs on your own machine. For a comparison on real
software tasks, see [public/README.md](public/README.md).

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
cost are not recorded, because each tool reports them differently.

There is deliberately no combined score. The report shows pass counts and median time, and
you decide what matters. Five small tasks can show a tool failing, but they cannot show that
one tool is better in general.

## Run it

You need Python 3.11 or newer, Git, the agents you want to test, and their API keys in your
environment. The script does not store or upload keys.

> **Warning:** to run unattended, these agents get permission to edit files and run
> commands without asking. Use a disposable machine or VM.

Check the harness first. This needs no agent, model or key:

```bash
python benchmarks/test_harness.py
```

Then run a comparison:

```bash
python benchmarks/run.py --agents argentum claude-code codex opencode pi --model MODEL_ID --repeats 3
```

The report is saved in `benchmarks/results/`, which Git ignores. Raw logs in that folder can
contain whatever an agent printed, so read them before sharing.

The commands for each agent are in [`agents.json`](agents.json). They follow each tool's
documented usage and have not been run against every tool. Flags change between versions,
so check each one with the tool's `--help` and correct the file if needed. The Pi entry is
the least certain.

### Argentum setup

Each attempt uses a new workspace and a new database, so Argentum needs two variables. The
key stays in the operating system's secure store, saved earlier by Argentum:

| Variable | Meaning |
| --- | --- |
| `ARGENTUM_BENCH_PRESET` | A provider preset whose key is already saved, such as `deepseek` |
| `ARGENTUM_BENCH_PROFILE` | A permission profile that can work unattended |

`automatic` is the intended profile. It edits inside the workspace without asking, while
running outside programs still needs approval. Headless Argentum never approves its own
actions, so an agent that tries to run a program gets `approval_required`. Other tools may be
free to run tests, which is a real difference to note when reading results.

## Keeping it fair

- Use the same model for every agent. Where a tool cannot use it, say so in the results.
- Use each tool's default configuration, with no custom instructions or plugins.
- Publish the whole report, including failures and tools that could not run.
- Say who ran it. A run by Argentum's own authors is not independent evidence.
