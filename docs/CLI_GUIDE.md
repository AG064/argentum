# Command-line guide

This guide covers day-to-day use of the `argentum` command on Windows: configuring a
provider, chatting, running tasks, inspecting results, and the JSON output that scripts
can rely on. For the list of commands that a build supports, run `argentum --help` or
`argentum commands`.

The `argentum-genesis-desktop-control` download adds extensions and Windows desktop
control. See [Extensions and browser](EXTENSIONS_BROWSER.md). Installing an extension
never approves running it, and a default build does not start any.

In the examples, `$exe`, `$ws` and `$db` are absolute paths you choose: the program, the
project folder (the workspace) and the database file.

```powershell
$exe = 'C:\path\to\argentum.exe'
$ws = 'C:\path\to\project'
$db = 'C:\path\to\argentum.sqlite3'
```

Use the same workspace and database for every command on a project. The folder you run
the command from is not the model's workspace.

## Configure a provider

```powershell
& $exe setup --workspace $ws --database $db
& $exe provider list --workspace $ws --database $db
& $exe provider models PROFILE_ID --workspace $ws --database $db
& $exe provider model PROFILE_ID --model EXACT_MODEL_ID --workspace $ws --database $db
& $exe provider select PROFILE_ID --workspace $ws --database $db
& $exe provider budget PROFILE_ID --limits TOTAL,INPUT,MAX_OUTPUT,REQUESTED_OUTPUT,UNCERTAINTY --workspace $ws --database $db
& $exe status --workspace $ws --database $db --json
```

`provider models` asks the provider for its model list. It does not run a model, but it
does use the network, so skip it if you work offline.

Every model needs a token budget before it can run. For a model whose context limit is
built into Argentum, the budget is set from that limit automatically, unless you already
saved one. DeepSeek models use 1,000,000 tokens and MiniMax M2.x models use 204,800. For
any other model, set the budget yourself with `provider budget`. The five values are the
total context, the input limit, the maximum output, the requested output, and a margin for
counting error. Argentum does not check the numbers against what the provider allows, so
use the limits from your provider's documentation. A budget belongs to one profile, model
and endpoint. If you change the model, set the budget again.

`status` checks local configuration only. It does not contact the provider.

## Providers

`provider presets` lists the built-in providers, and `provider presets --json` adds a
`tested` field. `provider add PRESET_ID` creates a profile from a preset. The full list,
with addresses, default models and key variables, is in [Providers](PROVIDERS.md).

```powershell
& $exe provider presets --workspace $ws --database $db
& $exe provider add anthropic --workspace $ws --database $db
& $exe provider credential set anthropic --workspace $ws --database $db
```

`provider credential set` reads the key from standard input. You can also set the
environment variable listed for the provider. A key is only sent to its own provider's
address. Never put a key in a command line.

`provider credential clear PROFILE` removes the stored key for a profile. In an
interactive terminal it asks you to type the profile id or `y` first. Add `--yes` to skip
the question, which scripts and `--json` need. Without `--yes`, a command that is not
interactive refuses with `invalid_argument` and removes nothing. The command says whether
a key was removed or none was stored. With `--json`, the result has `provider_id`,
`configured` (always false) and `removed`.

When the selected profile is ready for coding, `provider list`, `provider add`,
`provider select` and `provider model` say so. A profile that runs on this computer prints
"Local coding configuration ready". A hosted profile prints "Provider ready". Neither
message means the provider was contacted.

Two presets work a little differently:

- Anthropic uses its native Messages API. Tools, approvals and output limits work the same
  as with other providers.
- Azure OpenAI has no default address. Save the profile with your own endpoint. The model
  is your deployment name. Keys are only sent to HTTPS hosts under `openai.azure.com`,
  `services.ai.azure.com` or `cognitiveservices.azure.com`, with the `/openai/v1/` path.

```powershell
& $exe provider save azure-openai --label "Azure OpenAI" --kind openai-compatible --endpoint https://NAME.openai.azure.com/openai/v1/ --model DEPLOYMENT_NAME --workspace $ws --database $db
```

DeepSeek and OpenCode Go are tested with Argentum. The app lists the other providers on
its Testing tab.

A new setup asks before it changes anything. Check the effective policy with
`permissions list` before you select another profile. Nothing the model writes can grant
approval or widen the policy.

## Chat and tasks

```powershell
& $exe chat --workspace $ws --database $db
& $exe run --prompt 'Your task and its requirements' --workspace $ws --database $db --json
& $exe verification list --workspace $ws --database $db
```

In chat:

- `/help` lists the session commands: `/new`, `/fork`, `/sessions`, `/resume SESSION_ID`,
  `/queue`, `/steer TEXT`, `/stop`, `/retry` and `/quit`.
- `/providers`, `/models`, `/setup`, `/context`, `/tools`, `/verification` and `/sessions`
  open selectable panels. `/changes` reviews recorded changes and verification.
- `/thinking off`, `/thinking low`, `/thinking high` and `/thinking max` set how much the
  model reasons. The composer offers only the levels the selected model supports.
- `/compact` summarizes the session context to free space. It uses the selected model and
  can consume usage. `/compact deactivate` turns the summary off without calling a model.
- `/steer TEXT` applies your text to the active run at its next request.
- `/queue` shows prompts waiting behind the active run. `/queue continue` and
  `/queue discard` resume or drop them. A failed or cancelled run pauses the queue.
- Esc closes a panel without sending your draft. During a run, Esc or `/stop` cancels the
  run and waits for its work to finish. Do not kill the process to cancel.

Search returns limited selections of a project. A read returns the content, the byte range,
how much of the file it covered, and the file's SHA-256. An edit must give the SHA-256 it
observed. If the file changed, the edit is rejected, and the model has to read again and
submit a new edit.

The fixed Rust verification runners are `cargo-fmt-check`, `cargo-check-locked`,
`cargo-test-locked` and `cargo-clippy-strict`. A passing result is tied to the source it
checked. If the source changes, the result is out of date. After a restart, only the
result records that were saved are available. See [Native delivery](NATIVE_DELIVERY.md)
for goals that run to completion.

The model's `check_rust` tool is off by default. It runs model-written Rust tests against a
single Rust library project, inside a restricted Windows process. It does not support
multi-crate workspaces, custom build scripts or arbitrary dependencies. A temporary folder
or Cargo's offline mode is not a sandbox, so do not use this tool to run shell commands.
To turn it on, run `harness capability verification.supplemental-rust enable`.

### Read-only subagent

The agent can hand one investigation to a read-only subagent, for example "read these
files and summarize each one". The subagent starts with a fresh context, can only read
and search files in the project, and returns a short report. It cannot write files, run
commands or reach the network. Its result is a lead for the main agent, not a verified
answer.

It is off by default. To turn it on, open `argentum chat`, go to `/tools`, select
"Read-only delegated investigation" and confirm, or run
`argentum harness capability model.delegate-readonly enable` yourself in an interactive
terminal. Turning a tool on raises what the agent may do, so Argentum accepts it only from
you at a terminal, never from `--json`, `--non-interactive` or a script.

Limits for each delegation: 4 model requests, 8 tool calls, 32,000 tokens, 1,024 output
tokens and 2 minutes. A run can delegate once. It works with providers that use the
OpenAI-compatible interface, such as DeepSeek and most hosted presets, Ollama, and
`llama-server` added as a custom server. It is not available with Anthropic, ChatGPT plan,
LM Studio or the built-in llama.cpp profile, or inside a goal.

## Inspect and recover

```powershell
& $exe changes page --run-id RUN_UUID --limit 64 --workspace $ws --database $db --json
& $exe diagnostics RUN_UUID --workspace $ws --database $db --json
& $exe recovery inspect --database $db --json
```

`changes` and `diagnostics` read saved records. They show what happened in a run, not the
current state of your files. A change page covers one run, and the output says when
records are missing, filtered or cut off. Restoring a change needs your exact approval,
and Argentum first checks that the file still matches what the change left behind.

A run that changed no files is a valid run. `changes list RUN` and `changes page --run-id
RUN` print an empty list for it. They report `not_found` only when the run is unknown to
this project.

In goal output, `--json` shows timestamps as RFC 3339 strings, such as
`2026-11-13T18:04:05.123Z`. The saved format is unchanged. `goal attest` is accepted only
for a goal whose criteria require owner attestation (`goal criteria
--owner-attestation-required`). For any other goal it fails with `invalid_argument`.

`verification approve` takes a project runner (`project:ID`). The fixed runners
(`cargo-fmt-check`, `cargo-check-locked`, `cargo-test-locked` and `cargo-clippy-strict`)
need no approval, and approving one fails with `invalid_argument`.

After a crash or a forced close, Argentum does not take over by itself. If the writable
session was truly abandoned, close any other Argentum windows and follow the recovery
command that `--help` and `recovery inspect` show. Review uncertain work before you
resume. Usage that could not be counted stays marked unknown. Never delete the database,
clear grants or copy the database to remove a hold.

## Headless use and isolation

`serve` speaks JSON Lines on standard input and output. It is not a network service and has
no sign-in, so treat whatever is connected to it as untrusted, and do not expose it
remotely. An action that needs a new approval fails with `approval_required`. There is no
auto-approve switch. Approve in an interactive session.

A request line that is not valid JSON gets an `invalid_json` error. A line that is valid
JSON but has an unknown field, an unsupported type or the wrong shape gets an
`invalid_request` error. Both are recoverable and the server keeps reading.

`--isolated` runs with the workspace and database paths you give and ignores saved and
environment provider credentials and the local runtime. It does not change the saved
workspace. It is not a filesystem sandbox: the workspace policy still decides what the
agent can touch. Use a separate database for evaluations.

## Doctor

```powershell
& $exe doctor --workspace $ws --database $db --json
& $exe doctor --workspace $ws --database $db --json --probe-local
```

`doctor` is read-only. It opens an existing database without changing it and reports the
saved workspace, provider, harness and recovery state, the fixed verification runners and
the features compiled into the build. It starts no model or provider connection and reads
no credential. A missing database is reported, not created.

`--probe-local` adds checks for the optional browser and desktop features. It looks at
local files and the configured browser cache. It does not run Node, npm, a browser or any
other program, and it uses no network. Without it, `local_probe` is `null`.

The JSON report has these fields:

- `schema_version`: `1`.
- `command`: `doctor`.
- `ok` and `healthy`: true only when every required check passes.
- `ready`: true only when required and advisory checks all pass.
- `checks`: an ordered list. Each entry has `id`, `required`, `ok` and `detail`.
- `local_probe`: the local report, or `null`.

Required checks: `database_exists`, `database_openable`, `schema_current`.

Advisory checks: `workspace_exists`, `workspace_initialized`, `provider_ready`,
`harness_configured`, `recovery_clear`.

Local probe checks, all advisory: `node_present`, `browser_definition_present`,
`browser_adapter_present`, `playwright_present`, `chromium_present`, `desktop_feature`,
`artifact_feature`. A missing browser definition shows as a failed check with a suggested
next step, not as an error. The probe never changes `healthy` or the exit code.

The exit code is `0` only when all required checks pass. The full report is printed first,
including with `--json`. A failed advisory check leaves the exit code at `0` and sets
`ready` to false.

## JSON errors

When a command that accepts `--json` fails, it prints exactly one JSON object on standard
output. Without `--json`, the error is a one-line `argentum: ...` message on standard
error.

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

- `command` is the command path with spaces, such as `doctor`, `provider presets`,
  `desktop inspect` or `extensions browser check`.
- `error.code` is one of the codes below.
- `error.message` is short and hides absolute paths and URLs. It never includes
  credentials or provider data.
- `checks` and `action` appear only for commands that produce them.

Error codes:

- `invalid_argument`: an argument is missing, repeated, malformed, conflicting or names a
  value that cannot be used. Examples are an unknown verification runner or permission
  profile, an endpoint with credentials in it, `--json` on `result`, and
  `provider credential clear` without `--yes` outside a terminal.
- `feature_unavailable`: this build does not include the feature.
- `database_unavailable`: the database is missing or cannot be opened.
- `required_check_failed`: a required check failed, as in `doctor`.
- `browser_definition_missing`: `browser-control.stdio` is not installed.
- `browser_dependencies_missing`: the browser adapter dependencies are not ready.
- `browser_prepare_failed`: installing or verifying a browser dependency failed.
- `confirmation_mismatch`: a desktop confirmation does not match its target.
- `interactive_required`: the command needs an interactive console.
- `not_found`: the run, session, change, provider profile, skill or other record does not
  exist in this project. A record that belongs to another project is reported the same
  way. `changes list`, `changes diff` and `changes restore` with `--json` print this code
  in their one machine response (`type: "command_failed"`, `code: "not_found"`).
- `command_failed`: any other failure.

Argument errors exit with `2`. Other failures exit with `1`.

Some commands keep their own output:

- `doctor --json` always prints its full report. On a required failure it adds a top-level
  `error` with code `required_check_failed`. The report includes the workspace and
  database paths you passed.
- `desktop <action> --json` keeps the keys `action`, `dry_run`, `applied` and `built`, and
  uses `command: "desktop.<action>"`. Its `error` is an object with `code` and `message`.
- `extensions browser check` and `extensions browser prepare` print their full readiness
  report and add an `error` object on failure. Their `next_action` and `install.commands`
  fields can include your workspace and database paths, because they are repair
  instructions.
- `extensions browser status` always prints JSON, with or without `--json`. In a build
  without extensions it prints the `feature_unavailable` error object and exits with `1`.
- `result` always prints one JSON result envelope, so `result --json` is rejected with
  `invalid_argument`. Remove `--json`.
- `desktop status` in a build without desktop control fails with `feature_unavailable`,
  like `desktop list`. This matches the `unavailable` entry in `commands --json`.
- `run --json`, `provider probe --json` and `changes --json` write one final machine
  response and no second error object. `result` and `--final-result` print the final-result
  document. `compact` and `context` print their own JSON status objects.
- Successful output is unchanged. `schema_version` and `ok` are added to errors only.

## Desktop commands

These are available in the `argentum-genesis-desktop-control` download.

```powershell
& $exe desktop status --json
& $exe desktop list --json
& $exe desktop inspect --hwnd 0x1234 --depth 4 --json
& $exe desktop screenshot --hwnd 0x1234 --output C:\absolute\shot.png --json
& $exe desktop invoke --hwnd 0x1234 --element 'path=0/1' --confirm 0x1234 --json
& $exe desktop type --hwnd 0x1234 --element 'path=root' --text 'hello' --confirm 0x1234 --json
& $exe desktop keys --hwnd 0x1234 --element 'path=root' --chord Ctrl+S --confirm 0x1234 --dry-run --json
```

Desktop JSON has `schema_version`, `ok`, `command` (`desktop.<action>`), `action`,
`dry_run`, `applied`, `built`, and `result` or `error`. `invoke`, `type` and `keys` need
`--confirm HWND` with the same window handle as `--hwnd`. With `--dry-run`, Argentum finds
and rechecks the window and element and prints what it would do, but sends no input and
takes no screenshot. The confirmation is still required, and the output shows
`dry_run: true` and `applied: false`. For a real action, `applied: true` only means the
call succeeded. Look at the application to see the result.

The safety rules for desktop control are in
[Windows application control](WINDOWS_APPLICATION_CONTROL.md).

## Command index

`argentum commands` lists every command with its group, summary and whether this build has
it. `argentum commands --json` prints the same list as an object with `schema_version`,
`command: "commands"`, `ok` and `commands`. Each entry has `group`, `path`, `summary`,
`availability`, `feature` and `mutation`.

- `availability` is `available`, `reserved` or `unavailable`. A command that needs a
  feature missing from the build is `unavailable`.
- `feature` is `null`, `extensions`, `desktop-control` or `artifacts`.
- `mutation` is true for commands that can change saved state or write files.

Reserved names are recognized but do nothing. They print "is not available in this version
of Argentum Genesis" and exit with `1`. The reserved names are `model`, `profile`,
`module`, `plugin`, `mcp`, `migrate`, `prompt`, `self` and `update`. Use
`provider models`, `provider save`, `harness module` and `extensions` for what the first
few suggest. Shell completion is not included.

| Command | Group | What it does | Needs |
| --- | --- | --- | --- |
| `commands` | core | List the CLI surface, availability and feature gates |  |
| `chat` | core | Open the interactive terminal shell |  |
| `setup` | core | Run deterministic first-run onboarding |  |
| `doctor` | core | Read-only workspace, database, provider, harness and recovery diagnostics |  |
| `serve` | core | Keep one runtime alive and exchange protocol v1 JSONL on stdin/stdout |  |
| `run` | core | Submit one task and stream its output |  |
| `result` | core | Read one settled run result without launching work |  |
| `diagnostics` | core | Read bounded metadata for one run |  |
| `accounting inspect` | core | Inspect accounting reservations read-only |  |
| `accounting resolve` | core | Resolve an accounting reservation with owner confirmation |  |
| `skills list` | core | List project-local skills |  |
| `skills inspect` | core | Inspect one project-local skill |  |
| `status` | core | Show workspace and provider readiness |  |
| `terminal-diagnostic` | core | Write one synthetic terminal diagnostic |  |
| `local-chat` | core | Open llama.cpp coding chat with policy-controlled tools |  |
| `local-text` | core | Open explicit text-only local chat |  |
| `context` | core | Preview selected working-context metadata |  |
| `instructions` | core | Preview selected project instruction metadata |  |
| `compact` | core | Compact or deactivate the active session context |  |
| `workspace status` | core | Show the persisted workspace selection |  |
| `workspace set` | core | Set the persisted workspace selection |  |
| `sessions` | core | List durable sessions for the current workspace |  |
| `tool list` | core | List registered developer tools and availability |  |
| `provider probe` | provider | Probe one configured provider |  |
| `provider list` | provider | List provider profiles |  |
| `provider save` | provider | Save a provider profile |  |
| `provider presets` | provider | List provider catalog presets |  |
| `provider add` | provider | Add a profile from a provider preset |  |
| `provider login` | provider | Start ChatGPT browser sign-in |  |
| `provider login-new` | provider | Start a new ChatGPT browser sign-in |  |
| `provider accounts` | provider | List ChatGPT accounts |  |
| `provider account` | provider | Select a ChatGPT account |  |
| `provider logout` | provider | Sign out of a ChatGPT account |  |
| `provider select` | provider | Select a provider profile |  |
| `provider models` | provider | List models for a provider profile |  |
| `provider model` | provider | Select a model for a provider profile |  |
| `provider budget` | provider | Declare model budget limits |  |
| `provider context` | provider | Configure local llama.cpp context |  |
| `provider credential set` | provider | Save a secure provider credential |  |
| `provider credential clear` | provider | Clear a secure provider credential |  |
| `session select` | session | Select the active session |  |
| `session inspect` | session | Inspect one session |  |
| `session pin` | session | Pin a session |  |
| `session unpin` | session | Unpin a session |  |
| `session archive` | session | Archive a session |  |
| `session unarchive` | session | Unarchive a session |  |
| `session title` | session | Set a session title |  |
| `session fork` | session | Fork a session |  |
| `goal status` | goal | Show the active goal contract |  |
| `goal limits` | goal | Amend aggregate goal limits |  |
| `goal criteria` | goal | Set goal verification runners |  |
| `goal attest` | goal | Attest goal completion |  |
| `goal supervise` | goal | Supervise a finite goal |  |
| `goal deliver` | goal | Deliver a finite multi-stage goal |  |
| `goal set` | goal | Set a goal objective and budgets |  |
| `goal pause` | goal | Pause the active goal |  |
| `goal resume` | goal | Resume a paused goal |  |
| `goal clear` | goal | Clear the active goal |  |
| `harness status` | harness | Show harness composition and readiness |  |
| `harness profile` | harness | Select a harness profile |  |
| `harness surface` | harness | Show or hide a harness surface |  |
| `harness execution` | harness | Select an execution policy profile |  |
| `harness capability` | harness | Enable or disable a capability |  |
| `harness backend` | harness | Enable or disable a local backend |  |
| `harness module` | harness | Install, enable, disable or remove a module |  |
| `harness trajectory` | harness | Inspect a session trajectory |  |
| `desktop status` | desktop | Show desktop-control build and capability status | `desktop-control` build |
| `desktop list` | desktop | List bounded top-level windows | `desktop-control` build |
| `desktop inspect` | desktop | Inspect a bounded UIA element tree | `desktop-control` build |
| `desktop screenshot` | desktop | Capture one bounded window PNG | `desktop-control` build |
| `desktop invoke` | desktop | Invoke one observed element | `desktop-control` build |
| `desktop type` | desktop | Type bounded text into one element | `desktop-control` build |
| `desktop keys` | desktop | Send bounded key chords | `desktop-control` build |
| `permissions list` | permissions | Inspect permission profiles |  |
| `permissions select` | permissions | Select a permission profile |  |
| `permissions grants` | permissions | List approval grants |  |
| `permissions revoke` | permissions | Revoke an approval grant |  |
| `verification list` | verification | List fixed verification runners |  |
| `verification inspect` | verification | Inspect verification runner state |  |
| `verification approve` | verification | Approve one verification runner |  |
| `verification status` | verification | Show verification status |  |
| `changes page` | changes | Page durable change history |  |
| `changes list` | changes | List changes for one run |  |
| `changes diff` | changes | Show one change diff |  |
| `changes restore` | changes | Request an approved restore |  |
| `changes recovery` | changes | Retry or restore a change operation |  |
| `device show` | core | Show this device's pairing fingerprint, creating its key on first use |  |
| `device list` | core | List the devices paired with this one |  |
| `device pair` | core | Pair with another device on the local network after comparing a code |  |
| `device forget` | core | Forget a paired device |  |
| `device serve` | core | Serve this device's command protocol to one paired device |  |
| `device connect` | core | Send command protocol requests to a paired device |  |
| `recovery inspect` | recovery | Inspect abandoned local owner state |  |
| `recovery reconcile` | recovery | Reconcile abandoned local owner state |  |
| `recovery review` | recovery | Review one recovery revision |  |
| `extensions inspect` | extensions | Inspect a package | `extensions` build |
| `extensions install` | extensions | Install a reviewed package | `extensions` build |
| `extensions configure` | extensions | Configure an extension definition | `extensions` build |
| `extensions browser setup` | extensions | Configure the first-party browser adapter | `extensions` build |
| `extensions browser status` | extensions | Show browser adapter definition status | `extensions` build |
| `extensions browser check` | extensions | Check browser runtime readiness | `extensions` build |
| `extensions browser prepare` | extensions | Prepare browser dependencies with explicit network consent | `extensions` build |
| `extensions list` | extensions | List installed extension records | `extensions` build |
| `extensions status` | extensions | Show extension records and readiness | `extensions` build |
| `extensions receipts` | extensions | Show extension operation receipts | `extensions` build |
| `extensions activate` | extensions | Activate an extension in the local owner console | `extensions` build |
| `extensions disable` | extensions | Disable an extension | `extensions` build |
| `extensions remove` | extensions | Remove an extension registration | `extensions` build |
| `extensions key` | extensions | Save a secure extension service key | `extensions` build |
| `extensions prepare-media` | extensions | Prepare a private extension image directory | `extensions` build |
| `artifacts tools` | artifacts | List local artifact tools | `artifacts` build |
| `artifacts inspect` | artifacts | Inspect a local artifact configuration | `artifacts` build |
| `artifacts serve` | artifacts | Serve a local artifact configuration | `artifacts` build |
| `artifacts verify` | artifacts | Verify a local artifact configuration | `artifacts` build |
| `model` | reserved | Reserved model-management alias; use provider models or provider model | Reserved |
| `profile` | reserved | Reserved profile-management alias; use provider save and provider select | Reserved |
| `module` | reserved | Reserved module-management alias; use harness module | Reserved |
| `plugin` | reserved | Reserved plugin-management alias; use extensions install and configure | Reserved |
| `mcp` | reserved | Reserved MCP server-management alias; use the extensions family | Reserved |
| `migrate` | reserved | Reserved explicit store-migration command; schema migration runs on open | Reserved |
| `prompt` | reserved | Reserved prompt-template management | Reserved |
| `self` | reserved | Reserved self-management command | Reserved |
| `update` | reserved | Reserved self-update command | Reserved |
