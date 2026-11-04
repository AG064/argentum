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
| DeepSeek Harness | **Left out for now.** It sends session logs to DeepSeek by default, and DeepSWE says its data must never reach a training corpus. OpenCode also lists its Go support as incomplete | No |

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
- The static build from the `benchmark-build` workflow. Task images can have an older C library.
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

DeepSWE publishes no general rulebook or step limit, so these rules come from
[its method](https://deepswe.datacurve.ai/blog/deepswe) and its [v1.1 notes](https://deepswe.datacurve.ai/blog/deepswe-v1-1).
They are set before any scored run.

**Held the same for every harness**

- Task files and verifiers are never changed, and grading stays in Pier's separate container.
- Same model, route, tasks, seed, Pier version, machine, and maximum reasoning effort.
- Only the committed patch counts. No adapter commits for an agent, because the task tells it to.
- Network access is limited to the model's API.

**Argentum gets no special treatment**

- No prompt, tool, or code knows about DeepSWE. Only `benchmark-run` differs, as described above.
- Argentum runs from one commit, chosen before the scored run, for the whole run.
- Its own limits count against it. A run stopped by its round limit, its context budget, or a
  crash is a failed task, not a transient failure.

**Scoring**

- Trials are excluded only for API errors, provider outages, or Pier failures, and the count is
  published for each harness. Setup failures are fixed and every affected harness is rerun.
- `fd-deterministic-multi-key-sorting` was used to debug Argentum, so it is not scored for anyone.
  If the seed picks it, it is replaced for every harness and that is stated.
- Publish pass rates with uncertainty, plus the command, versions, commit, and trajectories.
- 20 tasks is a pilot, with error bars of around 20 points. Small gaps are not a ranking.

**What these results are not**

- They are not DeepSWE leaderboard results. That board holds the harness fixed to compare models
  and runs on Modal, while this holds the model fixed to compare harnesses on local Docker.
- DeepSeek's published 77.0 used its own setup and a different route, so it is not a baseline.
- OpenCode Go's free models may use your data, so they are never used here. DeepSWE's data must
  stay out of training.
