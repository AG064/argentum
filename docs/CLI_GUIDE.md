# Windows CLI operations

Use `argentum --help` for the version-bound command grammar. `$exe`, `$ws` and
`$db` below are absolute paths chosen by the owner. Workspace and database
selection must stay consistent across setup, configuration, chat and headless
commands. The process working directory is not the model workspace.

## Configure, close, reopen

```powershell
& $exe setup --workspace $ws --database $db
& $exe provider list --workspace $ws --database $db
& $exe provider models PROFILE_ID --workspace $ws --database $db
& $exe provider model PROFILE_ID --model EXACT_MODEL_ID --workspace $ws --database $db
& $exe provider select PROFILE_ID --workspace $ws --database $db
& $exe provider budget PROFILE_ID --limits TOTAL,INPUT,MAX_OUTPUT,REQUESTED_OUTPUT,UNCERTAINTY --workspace $ws --database $db
& $exe status --workspace $ws --database $db --json
```

`provider models` contacts the selected provider catalogue, not inference.
Do not run it when offline-only operation is required. A budget is an explicit
owner declaration bound to profile, model and endpoint, not independently
verified provider capacity. Invalid/missing effective limits fail closed.
Changing model or store does not transfer another model's budget. Check status
again in a fresh process after configuration. Status readiness is a local check,
not a live connectivity or entitlement proof.

New setup uses confirmation before changes. Inspect the effective policy with
`permissions list` before selecting any other profile. The verification profile permits
its enabled bounded reads, guarded writes and fixed verification; it does not
enable unrestricted shell. Project runners require an exact supported owner
approval. Model instructions cannot grant approval or expand policy.

## Coding and verification

```powershell
& $exe chat --workspace $ws --database $db
& $exe run --prompt 'Your bounded project task and public requirements' --workspace $ws --database $db --json
& $exe verification list --workspace $ws --database $db
```

Chat's `/providers`, `/models`, `/setup`, `/context`, `/tools`, `/verification`
and `/sessions` are selectable panels. `/changes` reviews stored changes and
verification. Escape returns without submitting the current draft. `/stop`
requests cancellation and drains owned work; do not kill the process to
simulate successful cancellation.

Search returns bounded selections. Reads return content, UTF-8 byte ranges,
coverage and full-source SHA-256. Edits require the observed SHA and an explicit
supported edit form. A conflict requires a new observation and a newly
submitted edit; the host never substitutes a current digest into an old request.

Fixed Rust runner IDs are `cargo-fmt-check`, `cargo-check-locked`,
`cargo-test-locked` and `cargo-clippy-strict`. Their receipts bind definitions
and source identities. A terminal job admission is not a passing receipt.
Failure feedback returns to the active model exchange but is not reconstructed
from historical reasoning after restart. See [native delivery](NATIVE_DELIVERY.md)
for explicit finite goals and current verification requirements.

Supplemental `check_rust` is disabled until explicitly owner-enabled. It supports
the admitted constrained Rust library-project shape, not this entire multi-crate
workspace or arbitrary manifests/build scripts/dependencies. It runs executable
input in the implemented Windows restricted process boundary. A temporary
directory or Cargo offline mode alone is not a sandbox. Do not use it to run
protected test changes, unrestricted shell or generic argv.

## Inspection and recovery

```powershell
& $exe changes page --run-id RUN_UUID --limit 64 --workspace $ws --database $db --json
& $exe diagnostics RUN_UUID --workspace $ws --database $db --json
& $exe recovery inspect --database $db --json
```

Changes and diagnostics open historical records read-only. They do not establish
current file/test freshness. Cursor pages are scoped to the selected run; missing,
filtered or truncated evidence is explicit. Review restore requires exact owner
approval and rechecks the recorded postimage before changing any file.

Recovery is never automatic takeover. If the writable host was genuinely
abandoned, close competing hosts and follow the exact trusted recovery command
shown by `--help` and the inspection output. Review uncertain effects before
resumption. Unknown usage stays unknown; supported full-reservation retention
is a nonrefundable client-budget disposition, not invented provider usage.
Never delete a store, clear grants or copy an authority database to remove a
hold. Keep original evidence, prior completed effects and deliberate settings.

## Headless and isolation

`serve` is JSONL protocol v1 on standard input/output, not an authenticated
network service. Treat its caller as untrusted and do not expose it remotely
without a separately reviewed boundary. It cannot perform owner authority
ceremonies. Headless actions requiring a new approval fail with
`approval_required`; interactive owner approval must be completed through the
normal local path. No auto-approve switch is provided.

For credential-free controls, use `--isolated` with absolute workspace/database
paths. Isolation excludes saved/environment provider credentials and the local
runtime and does not update the saved global workspace. It is not a filesystem
sandbox independent of the actual workspace policy. Fresh evaluation stores
must be separate from production history.
