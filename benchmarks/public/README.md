# Public benchmark: DeepSWE

Compares agent harnesses on one model using [DeepSWE](https://github.com/datacurve-ai/deep-swe),
a public set of 113 software tasks from real open source projects in five languages. Its tasks
are graded in a separate clean container, so no agent can tune its work to the checks.

**No results yet.** Nothing here has been run. The adapters for Argentum, Pi, and DeepSeek
Harness have not run against a real task, and no number here says any tool is better.

Runs use [OpenCode Go](https://opencode.ai/docs/go/) by default, because its $100 monthly limit for
DeepSeek V4.1 Flash goes much further than prepaid credit. It is capped at $50 a week and $20 per
5 hours. `--route deepseek` uses DeepSeek's own API instead.

| Harness | How it runs | On OpenCode's validated list |
| --- | --- | --- |
| mini-swe-agent | Built into Pier. The control, because DeepSWE's leaderboard uses it for every model | No |
| OpenCode | [agents.py](agents.py), on top of Pier's agent | Its own client |
| Claude Code | Built into Pier, through Go's Anthropic route | Yes |
| Codex | Built into Pier, through Go's OpenAI route | Yes |
| Pi | [agents.py](agents.py) | Yes |
| Argentum | [agents.py](agents.py), using the benchmark build below | No |
| DeepSeek Harness | [agents.py](agents.py), `--route deepseek` only. OpenCode lists its Go support as incomplete, and it uploads session logs to DeepSeek by default | No |

Results from different routes are not comparable. I cannot confirm that Go's `deepseek-v4.1-flash`
is the same build as DeepSeek's own `deepseek-flash`, so record the route with every result.

## The benchmark build

Argentum normally needs its owner at a real console to allow unattended work, and a benchmark
container has no console. The `benchmark-unattended` Cargo feature exists only for this:

- It is off by default, and no release script or workflow turns it on.
- It does not compile for Windows or any other non-Linux target.
- It also needs `ARGENTUM_BENCHMARK_DISPOSABLE=1` at runtime. Without that, it refuses like any other build.
- `argentum --version` says "benchmark build".

It adds one command, `argentum benchmark-run`, which does setup and the task in one process. The
agent gets the `full-access` profile: no approvals and no workspace limits. **Only run it in a
disposable container.** The `benchmark-build` workflow builds it and tests all of this.

Two parts of Argentum are Windows-only today, so this build works around them:

- **Saved database:** this command keeps its state in memory instead. A benchmark never restarts
  mid-task, so that should not affect its score, but recovery after a crash is not being tested.
- **Process containment:** this build uses a Linux process group instead of a Windows job object. A
  command can deliberately escape a group, which is why it relies on the container. Ordinary
  builds on other platforms still refuse to run commands.

Windows x64 is still the only tested platform for normal use.

## What you need

- Python 3.12 or newer, [Pier](https://pypi.org/project/datacurve-pier/) newer than 0.3.0, and a clone of DeepSWE.
- Docker on Linux or WSL, or a [Modal](https://modal.com) account. Each task uses up to 2 CPUs,
  8 GB of memory, and 20 GB of disk. Harbor's documentation does not mention native Windows.
- `OPENCODE_API_KEY` in your shell, or `DEEPSEEK_API_KEY` for `--route deepseek`, and
  `ARGENTUM_BENCHMARK_BINARY` set to the benchmark build.
- On OpenCode Go, DeepSeek models need **Global** selected under Regions in Settings > Privacy.

## Run it

Without `--go` this only prints the commands:

```bash
python benchmarks/public/run_deepswe.py --tasks deep-swe/tasks --n-tasks 20
```

Add `--go` to run them. That uses your Go limit, or real money on DeepSeek and on Modal if you use
them. The [leaderboard](https://deepswe.datacurve.ai/) lists about $0.46 per task for the older
`deepseek-v4-flash` on mini-swe-agent, which is only a rough guide. On Go, run outside peak hours,
01:00-04:00 and 06:00-10:00 UTC on weekdays, when prices double. Go watches for abuse and has no
stated rule on benchmarks, so run slowly. The same `--seed` gives every agent the same tasks.

## Keeping it fair

- Compare only runs with the same tasks, seed, and model build, and record that build.
- DeepSeek's [published 77.0](https://api-docs.deepseek.com/updates) used its own Mini-SWE-V2 setup,
  so only compare with it if the control comes close.
- Each harness runs with its own default reasoning settings. Argentum's default is high.
- A 20-task pilot shows whether the setup works. It is too small to rank tools.
- Publish results only with the command, versions, and trajectories.
