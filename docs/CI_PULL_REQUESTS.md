# Argentum on pull requests

The Argentum pull request action runs Argentum headless in GitHub Actions. It can
review a pull request, or try to fix a failing check, and report the result on
the pull request. It runs on Linux runners and builds the Argentum CLI from the
action's own source, which takes about ten minutes on a hosted runner.

Every authority the run has comes from your workflow file: the mode, the
permission profile, and whether it may change files or push. Nothing the model
writes, and nothing in the pull request, can widen it. The model's answer is
posted as a quoted comment, not as a verified result.

## Review a pull request

```yaml
name: Argentum review
on:
  pull_request:
permissions:
  contents: read
  pull-requests: write
jobs:
  review:
    runs-on: ubuntu-24.04
    timeout-minutes: 45
    steps:
      - uses: actions/checkout@v5
        with:
          persist-credentials: false
          ref: ${{ github.event.pull_request.head.sha }}
          fetch-depth: 0
      - uses: AG064/argentum@<commit>
        with:
          provider: deepseek
          api-key: ${{ secrets.DEEPSEEK_API_KEY }}
```

Pin the action to a full commit hash. Review mode:

- selects the `read-only` permission profile, so the model can read, search and
  inspect Git history in the checkout, and cannot change files, run commands or
  reach the network;
- refuses to run if that profile does not apply;
- gives the model the pull request title, description and diff (up to 80 KB,
  the rest it can read from the files), marked as data written by the author;
- posts one comment and updates it on later runs, instead of adding a new one.
  `post: summary` writes to the job summary only, with no comment.

GitHub does not give secrets to `pull_request` runs from forks. For those, the
action prints a notice and runs nothing.

## Fix a failing check

Fix mode changes files in the job's checkout, so it needs `allow-write: true`.
It never runs on a pull request from a fork.

```yaml
      - name: Tests
        id: tests
        continue-on-error: true
        run: cargo test --locked 2>&1 | tee "$RUNNER_TEMP/tests.log"
      - uses: AG064/argentum@<commit>
        if: steps.tests.outcome == 'failure'
        with:
          mode: fix
          allow-write: true
          check-log: ${{ runner.temp }}/tests.log
          provider: deepseek
          api-key: ${{ secrets.DEEPSEEK_API_KEY }}
```

`fix-profile` chooses what the run may do:

| Profile | The run can |
| --- | --- |
| `verification` (default) | Read files, make guarded edits, and run the fixed Rust checks (`cargo fmt --check`, `cargo check`, `cargo test`, `cargo clippy`). Any other process still needs an approval, which a headless run cannot give, so it is refused |
| `automatic` | Read files and make edits. Every process is refused |
| `full-access` | Use every enabled tool without approval, including shell commands, inside this job's runner. Commands get a cleared environment without the provider key, but they can reach the network. Use it only on pull requests you trust |

After the run, the action looks at what changed in the checkout:

- with no change, it posts the model's answer;
- with changes and no `allow-push`, it posts the changes as review suggestions
  where they fall on lines of the pull request, and the full patch in a comment
  and in the job's `patch-file` output. Nobody's branch changes until a person
  accepts a suggestion or applies the patch;
- with `allow-push: true`, and only if the run completed, it commits the change
  and pushes it to the pull request branch. This needs `permissions:
  contents: write` and a branch in this repository. Set `commit-author`, and
  `sign-off: true` if your repository requires DCO sign-off.

A push made with the default `GITHUB_TOKEN` does not start new workflow runs.
That is GitHub's rule, and it stops a fix from triggering itself. Re-run the
checks, or push with a token of your own if you want them to run.

Fix mode is built with the CLI's `benchmark-unattended` feature, which is the
only way a headless process can select a permission profile wider than
read-only. The action sets that feature's disposable-environment marker for the
one `permissions select` call, and only in fix mode with `allow-write: true`.
Treat the runner as disposable, which GitHub-hosted runners are.

## Inputs

| Input | Default | Meaning |
| --- | --- | --- |
| `mode` | `review` | `review` or `fix` |
| `provider` | `deepseek` | A preset id from `argentum provider presets`. With `endpoint`, the id for your own profile |
| `model` | preset default | The exact model id |
| `endpoint` | | Your own OpenAI-compatible server, instead of a preset |
| `limits` | | `TOTAL,INPUT,MAX_OUTPUT,REQUESTED_OUTPUT,UNCERTAINTY`, for a model whose limits Argentum does not know |
| `api-key` | | The provider key, from a secret. It goes only to the Argentum process, in the preset's key variable |
| `github-token` | `github.token` | Reads the pull request and posts |
| `instructions` | | Your extra instructions for the review or the fix |
| `check-log` | | Fix mode: a file with the failing output |
| `pull-request` | | The number, for events other than `pull_request` |
| `post` | `comment` | `comment` or `summary` |
| `allow-write` | `false` | Fix mode: let the run change files |
| `allow-push` | `false` | Fix mode: push the fix. Needs `allow-write` |
| `fix-profile` | `verification` | `verification`, `automatic` or `full-access` |
| `commit-author` | `github-actions[bot]` | The pushed commit's author |
| `sign-off` | `false` | Add `Signed-off-by` to the pushed commit |
| `allow-pull-request-target` | `false` | See below |
| `argentum-path` | | Review mode: a ready Linux CLI instead of building one |

Outputs: `ran`, `run-state`, `result-file` (the final-result JSON), `changed`,
`patch-file` and `pushed`.

## pull_request_target

`pull_request_target` runs with your secrets and a write token on a pull request
from anyone, so checking out its code and running a model on it is the pattern
behind most leaked tokens in GitHub Actions. The action refuses that event
unless you set `allow-pull-request-target: true`. If you do, use review mode
only, keep `persist-credentials: false`, and give the job only
`pull-requests: write`.

## What is tested

The `Pull request action` workflow in this repository runs the action on its own
pull requests against a local fake provider, in review and in fix mode, with no
secrets and no comments posted. It checks the tools offered in each mode and the
patch the fix produces. Runs with real providers have not been recorded yet.
