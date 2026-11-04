# Public benchmark: DeepSWE

Compares agent harnesses on one model using [DeepSWE](https://github.com/datacurve-ai/deep-swe),
a public set of 113 software tasks from real open source projects in five languages. Its tasks
are graded in a separate clean container, so no agent can tune its work to the checks.

**No results yet.** Nothing here has been run. The adapters for Argentum, Pi, and DeepSeek
Harness have not run against a real task, and no number here says any tool is better.

| Harness | How it runs |
| --- | --- |
| mini-swe-agent | Built into Pier. The control, because DeepSWE's leaderboard uses it for every model |
| OpenCode | Built into Pier |
| Claude Code | Built into Pier, pointed at DeepSeek's Anthropic-compatible route |
| Codex | Built into Pier, pointed at DeepSeek's Responses API |
| Argentum | [agents.py](agents.py), using the benchmark build below |
| Pi | [agents.py](agents.py) |
| DeepSeek Harness | [agents.py](agents.py). It uploads session logs to DeepSeek by default |

## The benchmark build

Argentum normally needs its owner at a real console to allow unattended work, and a benchmark
container has no console. The `benchmark-unattended` Cargo feature exists only for this:

- It is off by default, and no release script or workflow turns it on.
- It does not compile for Windows or any other non-Linux target.
- It also needs `ARGENTUM_BENCHMARK_DISPOSABLE=1` at runtime. Without that, it refuses like any other build.
- `argentum --version` says "benchmark build".

With it, the agent gets the `full-access` profile: no approvals and no workspace limits. **Only run
it in a disposable container.** The `benchmark-build` workflow builds it and tests all of this.

## What you need

- Python 3.12 or newer, [Pier](https://pypi.org/project/datacurve-pier/) newer than 0.3.0, and a clone of DeepSWE.
- Docker on Linux or WSL, or a [Modal](https://modal.com) account. Each task uses up to 2 CPUs,
  8 GB of memory, and 20 GB of disk. Harbor's documentation does not mention native Windows.
- `DEEPSEEK_API_KEY` in your shell, and `ARGENTUM_BENCHMARK_BINARY` set to the benchmark build.

## Run it

Without `--go` this only prints the commands:

```bash
python benchmarks/public/run_deepswe.py --tasks deep-swe/tasks --n-tasks 20
```

Add `--go` to run them. That spends real money on DeepSeek, and on Modal if you use it. The
[leaderboard](https://deepswe.datacurve.ai/) lists about $0.46 per task for the older
`deepseek-v4-flash` on mini-swe-agent, which is only a rough guide for another build or harness.
The same `--seed` gives every agent the same tasks.

## Keeping it fair

- Compare only runs with the same tasks, seed, and model build, and record that build.
- DeepSeek's [published 77.0](https://api-docs.deepseek.com/updates) used its own Mini-SWE-V2 setup,
  so only compare with it if the control comes close.
- Each harness runs with its own default reasoning settings. Argentum's default is high.
- A 20-task pilot shows whether the setup works. It is too small to rank tools.
- Publish results only with the command, versions, and trajectories.
