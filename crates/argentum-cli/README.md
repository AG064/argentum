# Argentum Genesis

Argentum Genesis is the terminal product of Argentum: a chat shell, headless runs, and
a JSONL server for working with an AI coding agent inside a project folder. The
executable is `argentum` and the Cargo package is `argentum-cli`.

[Back to the project overview](../../README.md) |
[Argentum desktop](../argentum-app/README.md) |
[All documentation](../../docs/README.md)

![A conversation in the terminal shell](../../docs/screenshots/cli-chat.png)

Screenshots on this page are real console windows on a throwaway project. The conversation
and permission images come from a live DeepSeek session. The others use an offline demo
provider.

## Status

[0.1.0-rc.1](https://github.com/AG064/argentum/releases/tag/v0.1.0-rc.1) is a pre-release for Windows x64: download
`argentum-genesis-0.1.0-rc.1-windows-x64.zip`. Packages are not code signed. Exact qualification is in
[the release record](../../docs/releases/0.1.0-rc.1.md), and what each platform and
provider has been tested on is in [the support matrix](../../docs/SUPPORT_MATRIX.md).
Linux and macOS are not release qualified.

## Install from source

You need Rust 1.98.0, Git, and the Windows MSVC C++ build tools. No Node.js is needed.

```powershell
git clone https://github.com/AG064/argentum.git
cd argentum
cargo build -p argentum-cli --bin argentum --release --locked
& '.\target\release\argentum.exe' --help
```

The result carries the C runtime it needs, so a clean PC does not need the Visual C++ runtime
installed.

To build a portable package with checksums and source provenance, use
`scripts/package-windows-cli.ps1 -OutputDirectory C:\temporary\package`. It refuses a
dirty checkout, and it neither signs nor publishes anything.

## First run

Pass the same workspace and database to every command. The process working directory
is not the model's workspace. For a first try, use a new database.

```powershell
$exe = 'C:\tools\argentum\argentum.exe'
$ws = 'C:\projects\example'
$db = 'C:\state\argentum\example.sqlite3'
& $exe setup --workspace $ws --database $db
& $exe doctor --workspace $ws --database $db
& $exe chat --workspace $ws --database $db
```

Setup chooses a provider, model, secure credentials, a model budget, and the workspace
policy. Never put a key in a command, prompt, URL, or config file. Use the masked key
prompt, `provider credential set` reading from stdin, or
[ChatGPT sign-in](../../docs/CHATGPT_SIGN_IN.md) in the browser.

![The first choice in setup](../../docs/screenshots/cli-setup.png)

![The doctor command on a configured project](../../docs/screenshots/cli-doctor.png)

## Three ways to run

| Mode | Command | Use it for |
| --- | --- | --- |
| Chat shell | `argentum chat` | Interactive work, with selectable panels |
| Headless | `argentum run --prompt TEXT --json` | One task, streamed as JSON |
| Server | `argentum serve` | Protocol v1 JSONL on stdin and stdout |

In chat, type `/` to search every command. `/thinking off`, `/thinking high`, and
`/thinking max` set how much DeepSeek reasons, and its thinking appears above each answer. `/providers`, `/models`, `/setup`, `/context`,
`/tools`, `/verification`, and `/sessions` open panels, and `/changes` reviews stored
changes. Esc or `/stop` asks the run to cancel and drains owned work. The footer always
shows the model, access level, and context use.

When the agent needs permission, a panel shows the exact action and target. Choose with the
arrows and Enter, or type `y` to approve once or `n` to reject. Reject is selected by default,
and Esc rejects too.

![The permission panel](../../docs/screenshots/cli-approval.png)

Add `--final-result` to `run` for one settled result document, and `--answer-schema` to
validate that answer against a schema file in the workspace. `--json` failures follow
[the JSON error contract](../../docs/CLI_GUIDE.md#json-error-contract).

## Common commands

| Area | Commands |
| --- | --- |
| Health | `doctor`, `doctor --probe-local`, `status`, `commands` |
| Providers | `provider list`, `presets`, `add`, `select`, `models`, `model`, `budget`, `probe` |
| Sessions | `sessions`, `session inspect`, `pin`, `archive`, `fork` |
| Goals | `goal set`, `limits`, `criteria`, `attest`, `supervise`, `deliver` |
| Policy | `permissions list`, `select`, `grants`, `revoke` |
| Verification | `verification list`, `inspect`, `approve`, `status` |
| Changes | `changes list`, `diff`, `restore`, `recovery` |
| Recovery | `recovery inspect`, `reconcile`, `review` |
| Run details | `result`, `diagnostics`, `context`, `instructions`, `compact` |
| Composition | `harness status`, `profile`, `surface`, `execution`, `capability`, `module` |

`argentum commands` prints the full list with what this build has available. Optional
families show as unavailable until you build them in, and reserved names are listed as
reserved.

![The end of the commands list, on a default build](../../docs/screenshots/cli-commands.png)

The [CLI guide](../../docs/CLI_GUIDE.md) covers each of these in depth.

## How it stays in your control

- **Confirmation first:** a new setup asks before changes. Inspect the effective policy
  with `permissions list` before choosing any other profile.
- **Guarded edits:** reads return a full-source SHA-256, and edits must match it. A stale
  or ambiguous edit is rejected rather than merged.
- **Bound verification:** fixed runners (`cargo-fmt-check`, `cargo-check-locked`,
  `cargo-test-locked`, and `cargo-clippy-strict`) produce receipts tied to the source
  they checked. Project runners need your exact approval.
- **No self-granted authority:** `run` and `serve` cannot approve their own actions. If
  an action needs an owner who is not there, it fails with `approval_required`.
- **Explicit recovery:** after an unclean exit, `recovery inspect` shows the state and
  takeover is never automatic.

The full trust boundary is in [SECURITY.md](../../SECURITY.md).

## Optional features

A default build includes none of these. Each is a separate choice at build time, and
installing something is never approval to run it.

| Feature | Adds | Guide |
| --- | --- | --- |
| `extensions` | Portable skills, MCP, and a local browser adapter | [Extensions and browser](../../docs/EXTENSIONS_BROWSER.md) |
| `desktop-control` | Governed Windows desktop commands, each confirmed for one exact window | [Windows application control](../../docs/WINDOWS_APPLICATION_CONTROL.md) |
| `artifacts` | Local document artifacts | [Local artifacts](../../docs/LOCAL_ARTIFACTS.md) |
| `local-llama-cpp` | A build switch for local llama.cpp support in the providers crate | [Support matrix](../../docs/SUPPORT_MATRIX.md) |

```powershell
cargo build -p argentum-cli --bin argentum --release --locked --features extensions
```
