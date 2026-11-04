# Benchmark plan

Saved for later. Nothing here has been run for a score, and no result exists yet.

## Goal

Compare Argentum with other harnesses on the same model, using public benchmarks and fair rules.

## Recommended run

[DeepSeek's V4.1 Flash model card](https://huggingface.co/deepseek-ai/DeepSeek-V4.1-Flash) publishes a
harness table, so the other tools may not need rerunning.

| Step | Runs | Rough use of the OpenCode Go limit |
| --- | --- | --- |
| Argentum on all 113 DeepSWE v1.1 tasks | 113 | $57, over 2 weeks at $50 a week |
| mini-swe-agent on 20 of those tasks, as a control | 20 | $10 |
| Terminal-Bench 2.1, if the first two look sound | Not sized | Not estimated |

These costs are estimates from the older `deepseek-v4-flash` row on the DeepSWE leaderboard.

If the control lands far from what DeepSeek's 74.2 would predict, our setup differs from theirs, and
Argentum's score must not be placed in their table.

### DeepSeek's published scores

Max reasoning effort, `max_steps=500`, 1M-token context, temperature 1.0, `top_p` 0.95. DeepSWE uses 8
samples per task and Terminal-Bench 2.1 uses 3.

| Harness | DeepSWE v1.1 | Terminal-Bench 2.1 |
| --- | --- | --- |
| mini-SWE | 74.2 | 90.3 |
| DSH Minimal | 72.6 | 90.6 |
| DSH Standard | 70.5 | 85.8 |
| Claude Code | 69.8 | 88.0 |
| DSH PTC | 67.6 | 85.8 |
| Pi | 66.2 | 86.1 |
| Codex | 65.6 | 84.1 |
| OpenCode | 65.5 | 85.0 |

DeepSeek ran these itself. The card does not state the build or API route.

## Before any scored run

- [ ] Raise Argentum's 32-round limit to 500 to match DeepSeek's `max_steps`. The database enforces the
      limit with a `CHECK` on `next_ordinal`, so this needs a schema migration, not just a constant.
- [ ] Send temperature 1.0 and `top_p` 0.95 from the benchmark build, and confirm Go accepts them.
- [ ] Add a Harbor adapter for Terminal-Bench, if it is being run.
- [ ] Pin the DeepSWE corpus and Pier versions, and record them.
- [ ] Freeze one Argentum commit, and use only that commit for the whole run.
- [ ] Decide whether Go's route is acceptable, or whether to pay DeepSeek directly.

## Fairness rules

See [Keeping it fair](README.md#keeping-it-fair).

## Other options

- **Six tools on 20 tasks:** about $60 and two weeks, for results run entirely by us.
- **[FrontierHarness Eval](https://github.com/frontier-harness-eval/eval):** built for this question, but it
  uses Kimi K3. Go's $15 monthly Kimi limit covers about 3 runs by rough estimate, and placing a result next
  to its table needs a matched control. Its repository lists no license.
- **DeepSeek Harness:** left out until its session-log upload is proven off, because DeepSWE's data must
  stay out of training corpora.
- **Free OpenCode models:** not used, because some may use your data.

## What the first task found

Running one task, `fd-deterministic-multi-key-sorting`, found these. That task is not scored for anyone.

- Task images have an older C library than CI, so the benchmark build must be static.
- Pier builds install steps before a trial starts, so the adapter copies the binary in during setup.
- OpenCode Go sends `null` for empty lists, which Argentum rejected.
- A 200,000 default context cap stopped a run near 45,000 real tokens, because accounting counts bytes.
- A 32-round limit stopped runs far below the leaderboard's average of about 150 steps.

The round limit is still in place, so no complete run has been graded yet.

## Setup on this machine

- Pier, DeepSWE, and the static build are in `~/argentum-bench` in the Ubuntu WSL distribution, on A:.
- Use Ubuntu's Docker. Docker Desktop's disk is on C:, which is reserved for the system.
- Keep every cache, build, and temp file off C:, including AppData and temp folders.
- The OpenCode Go key is saved as `provider/opencode-go`, and is passed to one process at a time.
