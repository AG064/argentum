# Windows CLI operations

The optional extension-enabled build has a separate [governed extension and
local browser guide](EXTENSIONS_BROWSER.md). Installation is not execution
approval. Default core startup remains inert with respect to extensions.

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

## Doctor contract

```powershell
& $exe doctor --workspace $ws --database $db --json
& $exe doctor --workspace $ws --database $db --json --probe-local
```

`doctor` is a read-only aggregate. It opens an existing store read-only, reads
the persisted workspace/provider/harness/recovery rows, and checks the fixed
verification runner registry and compile-time features. It does not start a
runtime or provider connection, read a credential, run a model, or write to the
store. A missing database is reported, not created.

The default report is fast. `--probe-local` adds advisory local readiness checks
for the optional browser and desktop capabilities. The probe only inspects
bounded local files, module manifests and the configured browser cache. It does
not execute Node, npm, npx or a browser, does not start a process, and does not
use the network. Without `--probe-local`, `local_probe` is `null`.

The JSON envelope is stable:

- `schema_version` is `1`.
- `command` is `doctor`.
- `ok` and `healthy` are true only when every required check passes.
- `ready` is true only when required and advisory checks pass.
- `checks` is ordered and each entry has `id`, `required`, `ok` and `detail`.
- `local_probe` is the local readiness report, or `null` when not requested.

Required check ids: `database_exists`, `database_openable`, `schema_current`.

Advisory check ids: `workspace_exists`, `workspace_initialized`,
`provider_ready`, `harness_configured`, `recovery_clear`.

Local probe check ids (all advisory): `node_present`,
`browser_definition_present`, `browser_adapter_present`, `playwright_present`,
`chromium_present`, `desktop_feature`, `artifact_feature`. A missing browser
definition is reported as a failed advisory check with a next action, not as an
error. The probe never changes `healthy` or the exit code.

Exit code is `0` only when all required checks pass. Any required failure exits
nonzero. The complete report is printed first, including in `--json` mode, so a
caller can inspect every check without a panic or a partial document. Advisory
failures do not change the exit code but set `ready` to false. Check ids are
stable and are also printed in text mode.

## JSON error contract

Commands that accept `--json` and fail emit exactly one structured JSON object
on stdout. In `--json` mode the failure record is the JSON object, not a plain
error sentence mixed with JSON. Text mode keeps the `argentum: ...` message on
stderr.

The stable failure envelope is:

```json
{
  "schema_version": 1,
  "ok": false,
  "command": "desktop inspect",
  "error": {
    "code": "invalid_argument",
    "message": "--hwnd requires a hexadecimal window handle"
  }
}
```

- `schema_version` is `1`.
- `ok` is `false`.
- `command` is the space-separated command path, for example `doctor`,
  `provider presets`, `desktop inspect` or `extensions browser check`.
- `error.code` is a stable identifier listed below.
- `error.message` is bounded and redacts absolute paths and URLs. It never
  includes credentials, provider payloads or raw dependency data.
- `checks` and `action` are optional and appear only where the command already
  provides them.

Stable codes:

- `invalid_argument`: missing, duplicate, malformed or conflicting arguments.
- `feature_unavailable`: the binary was built without the required feature.
- `database_unavailable`: the selected database is missing or cannot be opened.
- `required_check_failed`: a command's required checks failed, such as `doctor`.
- `browser_definition_missing`: `browser-control.stdio` is not installed.
- `browser_dependencies_missing`: the browser adapter dependencies are not ready.
- `browser_prepare_failed`: a browser dependency install or verification step
  failed.
- `confirmation_mismatch`: a desktop confirmation does not match its target.
- `interactive_required`: the command needs a direct interactive console.
- `not_found`: the requested run, session or record is not available.
- `command_failed`: a failure that has no more specific stable code.

Exit codes: argument failures exit `2`. Command failures exit `1`. Success keeps
the command's existing exit code.

Documented exceptions:

- `doctor --json` keeps its full diagnostic report. On a required failure the
  report has `ok: false` and adds a top-level `error` object with
  `code: "required_check_failed"`, plus the existing `checks`. The report
  includes the requested workspace and database paths by design.
- `desktop <action> --json` keeps its existing keys `action`, `dry_run`,
  `applied` and `built`, uses `command: "desktop.<action>"`, and changes `error`
  to an object with `code` and `message`. Successful desktop payloads are
  unchanged.
- `extensions browser check` and `extensions browser prepare` keep their full
  readiness reports and add an `error` object when they fail. Their
  `next_action` and `install.commands` fields may include the requested
  workspace and database paths because they are the repair instructions.
- `extensions browser status` always emits JSON. `--json` is accepted as an
  explicit selection and does not change the payload.
- Streaming machine protocols keep their documented frames. `run --json`,
  `provider probe --json` and `changes --json` write one terminal machine
  response and do not append a second envelope. `result` and `--final-result`
  keep the final-result envelope. `compact` and `context` keep their existing
  JSON status objects.
- Success payloads are not rewritten. `schema_version` and `ok` are added to
  failures, not retrofitted onto every success shape.

## Desktop commands

```powershell
& $exe desktop status --json
& $exe desktop list --json
& $exe desktop inspect --hwnd 0x1234 --depth 4 --json
& $exe desktop screenshot --hwnd 0x1234 --output C:\absolute\shot.png --json
& $exe desktop invoke --hwnd 0x1234 --element 'path=0/1' --confirm 0x1234 --json
& $exe desktop type --hwnd 0x1234 --element 'path=root' --text 'hello' --confirm 0x1234 --json
& $exe desktop keys --hwnd 0x1234 --element 'path=root' --chord Ctrl+S --confirm 0x1234 --dry-run --json
```

Desktop JSON uses a stable envelope with `schema_version`, `ok`, `command`
(`desktop.<action>`), `action`, `dry_run`, `applied`, `built`, and `result` or
`error`. `invoke`, `type` and `keys` require `--confirm HWND` matching the target.
`--dry-run` resolves and revalidates the window and element and prints the
planned pattern, mode, character count or chords, but calls no UIA pattern,
SendInput or screenshot API. It still requires the matching confirmation and
reports `dry_run: true`, `applied: false`. A real action reports `applied: true`
as an API return only; reobserve the application.

Desktop security, identity, bounds, failure taxonomy and the reasons pointer
injection and focus stealing are omitted are documented in
[Windows application control](WINDOWS_APPLICATION_CONTROL.md).

## Command index

The command catalog is the single source for this index. `argentum commands --json` emits the same entries with runtime availability. The stable envelope is `schema_version`, `command: "commands"`, `ok`, and `commands`, where each entry has `group`, `path`, `summary`, `availability`, `feature`, and `mutation`. `availability` is `available`, `reserved`, or `unavailable`; feature-gated entries report `unavailable` in a build without that feature. `feature` is null, `extensions`, `desktop-control`, or `artifacts`. `mutation` marks commands that can change durable state or write output.

Reserved names are recognized by the parser but return an unavailable error; they are not working commands. Shell completion is not shipped. The catalog does not carry flag arity, positional types, or value suggestions, and a hand-maintained completion list would drift from the parser.

| Command | Group | Summary | Feature | Mutation |
| --- | --- | --- | --- | --- |
| `commands` | core | List the CLI surface, availability and feature gates | none | no |
| `chat` | core | Open the interactive terminal shell | none | no |
| `setup` | core | Run deterministic first-run onboarding | none | yes |
| `doctor` | core | Read-only workspace, database, provider, harness and recovery diagnostics | none | no |
| `serve` | core | Keep one runtime alive and exchange protocol v1 JSONL on stdin/stdout | none | no |
| `run` | core | Submit one task and stream its output | none | yes |
| `result` | core | Read one settled run result without launching work | none | no |
| `diagnostics` | core | Read bounded metadata for one run | none | no |
| `accounting inspect` | core | Inspect accounting reservations read-only | none | no |
| `accounting resolve` | core | Resolve an accounting reservation with owner confirmation | none | yes |
| `skills list` | core | List project-local skills | none | no |
| `skills inspect` | core | Inspect one project-local skill | none | no |
| `status` | core | Show workspace and provider readiness | none | no |
| `terminal-diagnostic` | core | Write one synthetic terminal diagnostic | none | yes |
| `local-chat` | core | Open llama.cpp coding chat with policy-controlled tools | none | yes |
| `local-text` | core | Open explicit text-only local chat | none | no |
| `context` | core | Preview selected working-context metadata | none | no |
| `instructions` | core | Preview selected project instruction metadata | none | no |
| `compact` | core | Compact or deactivate the active session context | none | yes |
| `workspace status` | core | Show the persisted workspace selection | none | no |
| `workspace set` | core | Set the persisted workspace selection | none | yes |
| `sessions` | core | List durable sessions for the current workspace | none | no |
| `tool list` | core | List registered developer tools and availability | none | no |
| `provider probe` | provider | Probe one configured provider | none | no |
| `provider list` | provider | List provider profiles | none | no |
| `provider save` | provider | Save a provider profile | none | yes |
| `provider presets` | provider | List provider catalog presets | none | no |
| `provider add` | provider | Add a profile from a provider preset | none | yes |
| `provider login` | provider | Start ChatGPT browser sign-in | none | yes |
| `provider login-new` | provider | Start a new ChatGPT browser sign-in | none | yes |
| `provider accounts` | provider | List ChatGPT accounts | none | no |
| `provider account` | provider | Select a ChatGPT account | none | yes |
| `provider logout` | provider | Sign out of a ChatGPT account | none | yes |
| `provider select` | provider | Select a provider profile | none | yes |
| `provider models` | provider | List models for a provider profile | none | no |
| `provider model` | provider | Select a model for a provider profile | none | yes |
| `provider budget` | provider | Declare model budget limits | none | yes |
| `provider context` | provider | Configure local llama.cpp context | none | yes |
| `provider credential set` | provider | Save a secure provider credential | none | yes |
| `provider credential clear` | provider | Clear a secure provider credential | none | yes |
| `session select` | session | Select the active session | none | yes |
| `session inspect` | session | Inspect one session | none | no |
| `session pin` | session | Pin a session | none | yes |
| `session unpin` | session | Unpin a session | none | yes |
| `session archive` | session | Archive a session | none | yes |
| `session unarchive` | session | Unarchive a session | none | yes |
| `session title` | session | Set a session title | none | yes |
| `session fork` | session | Fork a session | none | yes |
| `goal status` | goal | Show the active goal contract | none | no |
| `goal limits` | goal | Amend aggregate goal limits | none | yes |
| `goal criteria` | goal | Set goal verification runners | none | yes |
| `goal attest` | goal | Attest goal completion | none | yes |
| `goal supervise` | goal | Supervise a finite goal | none | yes |
| `goal deliver` | goal | Deliver a finite multi-stage goal | none | yes |
| `goal set` | goal | Set a goal objective and budgets | none | yes |
| `goal pause` | goal | Pause the active goal | none | yes |
| `goal resume` | goal | Resume a paused goal | none | yes |
| `goal clear` | goal | Clear the active goal | none | yes |
| `harness status` | harness | Show harness composition and readiness | none | no |
| `harness profile` | harness | Select a harness profile | none | yes |
| `harness surface` | harness | Show or hide a harness surface | none | yes |
| `harness execution` | harness | Select an execution policy profile | none | yes |
| `harness capability` | harness | Enable or disable a capability | none | yes |
| `harness backend` | harness | Enable or disable a local backend | none | yes |
| `harness module` | harness | Install, enable, disable or remove a module | none | yes |
| `harness trajectory` | harness | Inspect a session trajectory | none | no |
| `desktop status` | desktop | Show desktop-control build and capability status | desktop-control | no |
| `desktop list` | desktop | List bounded top-level windows | desktop-control | no |
| `desktop inspect` | desktop | Inspect a bounded UIA element tree | desktop-control | no |
| `desktop screenshot` | desktop | Capture one bounded window PNG | desktop-control | yes |
| `desktop invoke` | desktop | Invoke one observed element | desktop-control | yes |
| `desktop type` | desktop | Type bounded text into one element | desktop-control | yes |
| `desktop keys` | desktop | Send bounded key chords | desktop-control | yes |
| `permissions list` | permissions | Inspect permission profiles | none | no |
| `permissions select` | permissions | Select a permission profile | none | yes |
| `permissions grants` | permissions | List approval grants | none | no |
| `permissions revoke` | permissions | Revoke an approval grant | none | yes |
| `verification list` | verification | List fixed verification runners | none | no |
| `verification inspect` | verification | Inspect verification runner state | none | no |
| `verification approve` | verification | Approve one verification runner | none | yes |
| `verification status` | verification | Show verification status | none | no |
| `changes page` | changes | Page durable change history | none | no |
| `changes list` | changes | List changes for one run | none | no |
| `changes diff` | changes | Show one change diff | none | no |
| `changes restore` | changes | Request an approved restore | none | yes |
| `changes recovery` | changes | Retry or restore a change operation | none | yes |
| `recovery inspect` | recovery | Inspect abandoned local owner state | none | no |
| `recovery reconcile` | recovery | Reconcile abandoned local owner state | none | yes |
| `recovery review` | recovery | Review one recovery revision | none | yes |
| `extensions inspect` | extensions | Inspect a package | extensions | no |
| `extensions install` | extensions | Install a reviewed package | extensions | yes |
| `extensions configure` | extensions | Configure an extension definition | extensions | yes |
| `extensions browser setup` | extensions | Configure the first-party browser adapter | extensions | yes |
| `extensions browser status` | extensions | Show browser adapter definition status | extensions | no |
| `extensions browser check` | extensions | Check browser runtime readiness | extensions | no |
| `extensions browser prepare` | extensions | Prepare browser dependencies with explicit network consent | extensions | yes |
| `extensions list` | extensions | List installed extension records | extensions | no |
| `extensions status` | extensions | Show extension records and readiness | extensions | no |
| `extensions receipts` | extensions | Show extension operation receipts | extensions | no |
| `extensions activate` | extensions | Activate an extension in the local owner console | extensions | yes |
| `extensions disable` | extensions | Disable an extension | extensions | yes |
| `extensions remove` | extensions | Remove an extension registration | extensions | yes |
| `extensions key` | extensions | Save a secure extension service key | extensions | yes |
| `extensions prepare-media` | extensions | Prepare a private extension image directory | extensions | yes |
| `artifacts tools` | artifacts | List local artifact tools | artifacts | no |
| `artifacts inspect` | artifacts | Inspect a local artifact configuration | artifacts | no |
| `artifacts serve` | artifacts | Serve a local artifact configuration | artifacts | no |
| `artifacts verify` | artifacts | Verify a local artifact configuration | artifacts | no |
| `model` | reserved | Reserved model-management alias; use provider models or provider model | none | no |
| `profile` | reserved | Reserved profile-management alias; use provider save and provider select | none | no |
| `module` | reserved | Reserved module-management alias; use harness module | none | no |
| `plugin` | reserved | Reserved plugin-management alias; use extensions install and configure | none | no |
| `mcp` | reserved | Reserved MCP server-management alias; use the extensions family | none | no |
| `migrate` | reserved | Reserved explicit store-migration command; schema migration runs on open | none | no |
| `prompt` | reserved | Reserved prompt-template management | none | no |
| `self` | reserved | Reserved self-management command | none | no |
| `update` | reserved | Reserved self-update command | none | no |
