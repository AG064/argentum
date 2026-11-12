# Public benchmark: DeepSWE

Compares agent harnesses on one model using [DeepSWE](https://github.com/datacurve-ai/deep-swe),
a public set of 113 software tasks. Its tasks are graded in a separate clean container, so no
agent can tune its work to the checks.

**No results yet.** Nothing here has been run. The adapters for Argentum, Pi and DeepSeek
Harness have not run against a real task, and no number here says any tool is better.

Runs use [OpenCode Go](https://opencode.ai/docs/go/) by default (`--route opencode-go`).
`--route deepseek` uses DeepSeek's own API instead. Check the provider's current pricing and
limits before a long run.

| Harness | How it runs |
| --- | --- |
| mini-swe-agent | Built into Pier. The reference harness |
| OpenCode | [agents.py](agents.py), on top of Pier's agent |
| Claude Code | Built into Pier, through Go's Anthropic route |
| Codex | Built into Pier, through Go's OpenAI route |
| Pi | [agents.py](agents.py) |
| Argentum | [agents.py](agents.py), using the benchmark build below |
| DeepSeek Harness | Not run. It sends session logs to DeepSeek by default, which DeepSWE's rule that its data must not reach a training corpus does not allow, so the runner refuses it |

Results from different routes are not comparable. The two routes may serve different builds
of the model, so record the route with every result.

## The benchmark build

Argentum normally needs its owner at a real console to allow unattended work, and a benchmark
container has no console. The `benchmark-unattended` Cargo feature exists only for this:

- It is off by default, and no release script or workflow turns it on.
- It does not compile for Windows or any other non-Linux target.
- `argentum benchmark-run` exits with an error unless `ARGENTUM_BENCHMARK_DISPOSABLE=1` is set.
- `argentum --version` says "benchmark build".

It adds one command, `argentum benchmark-run`, which does setup and the task in one process. The
agent gets the `full-access` profile: no approvals and no workspace limits. **Only run it in a
disposable container.** The `benchmark-build` workflow builds it and tests all of this.

This build differs from the Windows build in two ways:

- **Saved database:** state is kept in memory, because a saved database is available only on
  Windows. A benchmark never restarts mid-task, so this should not affect its score, but recovery
  after a crash is not tested.
- **Process containment:** it uses a Linux process group instead of a Windows job object. A
  command can deliberately escape a group, which is why it relies on the container. Ordinary
  builds on other platforms still refuse to run commands.

Windows x64 is the only tested platform for normal use.

## What you need

- Python 3.11 or newer, [Pier](https://pypi.org/project/datacurve-pier/) (check its own Python
  requirement), and a clone of DeepSWE.
- Docker on Linux or WSL, or a [Modal](https://modal.com) account. Each task can use 8 GB of
  memory.
- The static build from the `benchmark-build` workflow. Task images can have an older C library.
- `OPENCODE_API_KEY` in your shell, or `DEEPSEEK_API_KEY` for `--route deepseek`, and
  `ARGENTUM_BENCHMARK_BINARY` set to the benchmark build.

## Run it

Without `--go` this only prints the commands:

```bash
python benchmarks/public/run_deepswe.py --tasks deep-swe/tasks --n-tasks 20
```

Add `--go` to run them. That uses your OpenCode Go quota, or real money on DeepSeek and on
Modal if you use them. The same `--seed` gives every agent the same tasks.

## Keeping it fair

These rules are set before any scored run.

**Held the same for every harness**

- Task files and verifiers are never changed, and grading stays in Pier's separate container.
- Same model, route, tasks, seed, Pier version, machine and maximum reasoning effort.
- Only the committed patch counts. The task tells each agent to commit its work, and no adapter
  commits for it.
- Network access is limited to the model's API.

**Argentum gets no special treatment**

- No prompt, tool or code knows about DeepSWE. Only `benchmark-run` differs, as described above.
- Argentum runs from one commit, chosen before the scored run, for the whole run.
- Its own limits count against it. A run stopped by its round limit, its context budget or a
  crash is a failed task, not a transient failure.

**Scoring**

- Trials are excluded only for API errors, provider outages or Pier failures, and the count is
  published for each harness. Setup failures are fixed and every affected harness is rerun.
- `fd-deterministic-multi-key-sorting` is excluded from scoring for every harness because it
  was used during Argentum's development. If the seed picks it, it is replaced for every harness
  and that is stated.
- Publish pass rates with uncertainty, plus the command, versions, commit and trajectories.
- 20 tasks is a pilot, with error bars of around 20 percentage points. Small gaps are not a
  ranking.

**What these results are not**

- They are not DeepSWE leaderboard results. They hold the model fixed to compare harnesses,
  and run on local Docker unless you pass `--env modal`.
- Free models are not used, because DeepSWE's data must stay out of training. The runner does not enforce this. Choose the model yourself and check that it is
  not a free tier.
