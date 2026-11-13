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

Download one of these from [Argentum 0.1.0](https://github.com/AG064/argentum/releases/tag/v0.1.0):

- `argentum-genesis-0.1.0-windows-x64.zip`: the command-line agent.
- `argentum-genesis-desktop-control-0.1.0-windows-x64.zip`: the same agent with Windows
  desktop control and extensions built in.

You can also install the desktop app, which puts `argentum` on your PATH. The files are not
code signed. What each platform and provider supports is in
[the support matrix](../../docs/SUPPORT_MATRIX.md). On Linux and macOS, build it from source.

## Install from source

You need Rust 1.98.0, Git, and the Windows MSVC C++ build tools. No Node.js is needed.

```powershell
git clone https://github.com/AG064/argentum.git
cd argentum
cargo build -p argentum-cli --bin argentum --release --locked
& '.\target\release\argentum.exe' --help
```

The C runtime is linked into the program, so a clean PC does not need the Visual C++
runtime installed.

To build a portable package with checksums and source provenance, use
`scripts/package-windows-cli.ps1 -OutputDirectory C:\temporary\package`. It refuses to
run on a checkout with uncommitted changes.

## First run

Pass the same workspace and database to every command. The process working directory
is not the model's workspace. For a first try, use a new database.

```powershell
$exe = 'C:\path\to\argentum.exe'
$ws = 'C:\path\to\project'
$db = 'C:\path\to\argentum.sqlite3'
& $exe setup --workspace $ws --database $db
& $exe doctor --workspace $ws --database $db
& $exe chat --workspace $ws --database $db
```

Setup chooses a provider, model, secure credentials, a model budget, and the workspace
policy. Never put a key in a command, prompt, URL, or config file. Use the hidden key
prompt in setup, `provider credential set` reading from standard input, or
[ChatGPT sign-in](../../docs/CHATGPT_SIGN_IN.md) in the browser.

![The first choice in setup](../../docs/screenshots/cli-setup.png)

![The doctor command on a configured project](../../docs/screenshots/cli-doctor.png)

## Three ways to run

| Mode | Command | Use it for |
| --- | --- | --- |
| Chat shell | `argentum chat` | Interactive work, with selectable panels |
| Headless | `argentum run --prompt TEXT --json` | One task, streamed as JSON |
| Server | `argentum serve` | Protocol v1 JSONL on stdin and stdout |

In chat, type `/` to search every command, or `/help` for the session commands. `/steer
TEXT` redirects the active run, `/queue` shows waiting prompts, and `/compact` summarizes
the context. `/thinking off`, `low`, `high`, and `max` set how much a model reasons, for
models that support it. `/providers`, `/models`, `/setup`, `/context`, `/tools`,
`/verification`, and `/sessions` open panels, and `/changes` reviews stored changes. Esc or
`/stop` cancels the run and waits for its work to finish.

When the agent needs permission, a panel shows the exact action and target. Reject is the
first choice. Choose with the arrows and Enter, or type `y` to approve once or `n` to
reject. Esc rejects too.

![The permission panel](../../docs/screenshots/cli-approval.png)

Add `--final-result` to `run` for one settled result document, and `--answer-schema` to
validate that answer against a schema file in the workspace. `--json` failures follow
[the JSON error contract](../../docs/CLI_GUIDE.md#json-error-contract).

## Common commands

| Area | Commands |
| --- | --- |
| Health | `doctor`, `doctor --probe-local`, `status`, `commands` |
| Providers | `provider list`, `provider presets`, `provider add`, `provider select`, `provider models`, `provider model`, `provider budget`, `provider probe` |
| Sessions | `sessions`, `session inspect`, `session pin`, `session archive`, `session fork` |
| Goals | `goal set`, `goal limits`, `goal criteria`, `goal attest`, `goal supervise`, `goal deliver` |
| Policy | `permissions list`, `permissions select`, `permissions grants`, `permissions revoke` |
| Verification | `verification list`, `verification inspect`, `verification approve`, `verification status` |
| Changes | `changes list`, `changes diff`, `changes restore`, `changes recovery` |
| Recovery | `recovery inspect`, `recovery reconcile`, `recovery review` |
| Run details | `result`, `diagnostics`, `context`, `instructions`, `compact` |
| Composition | `harness status`, `harness profile`, `harness surface`, `harness execution`, `harness capability`, `harness backend`, `harness module`, `harness trajectory` |

`argentum commands` prints the full list with what this build has available. Optional
families show as unavailable until you build them in, and reserved names are listed as
reserved.

![The end of the commands list, on a default build](../../docs/screenshots/cli-commands.png)

The [CLI guide](../../docs/CLI_GUIDE.md) covers each of these in depth.

A few behaviors worth knowing:

- A missing run, session, change, provider profile or skill fails with the `not_found`
  error code. Unusable values, such as an unknown permission profile or an endpoint with
  credentials in it, fail with `invalid_argument`.
- `changes list` and `changes page` return an empty list for a run that changed no files.
- `provider credential clear PROFILE` asks you to confirm in a terminal. Use `--yes` in
  scripts. It says whether a credential was removed.
- `verification approve` is for project runners only. Fixed runners need no approval.
- `goal attest` works only for a goal that requires owner attestation. Goal timestamps in
  `--json` output are RFC 3339 strings.
- `result` always prints JSON, so `result --json` is rejected.
- In a build without desktop control or extensions, `desktop status` and
  `extensions browser status` report `feature_unavailable`.

## How it stays in your control

- **Confirmation first:** a new setup asks before changes. Inspect the effective policy
  with `permissions list` before choosing any other profile.
- **Guarded edits:** reads return the file's SHA-256, and edits must match it. A stale
  or ambiguous edit is rejected rather than merged.
- **Verification tied to source:** fixed runners (`cargo-fmt-check`, `cargo-check-locked`,
  `cargo-test-locked`, and `cargo-clippy-strict`) produce results tied to the source
  they checked. Project runners need your exact approval.
- **No self-granted authority:** `run` and `serve` cannot approve their own actions. If
  an action needs an owner who is not there, it fails with `approval_required`.
- **Explicit recovery:** after an unclean exit, `recovery inspect` shows the state and
  takeover is never automatic.

The full trust boundary is in [SECURITY.md](../../SECURITY.md).

## Optional features

A default build includes none of these. Each is a build-time choice, and the
`argentum-genesis-desktop-control` download includes `extensions` and `desktop-control`.
Installing something is never approval to run it.

| Feature | Adds | Guide |
| --- | --- | --- |
| `extensions` | Portable skills, MCP, and a local browser adapter | [Extensions and browser](../../docs/EXTENSIONS_BROWSER.md) |
| `desktop-control` | Windows desktop commands. Each action needs `--confirm` with the target window handle | [Windows application control](../../docs/WINDOWS_APPLICATION_CONTROL.md) |
| `artifacts` | Local document artifacts | [Local artifacts](../../docs/LOCAL_ARTIFACTS.md) |
| `local-llama-cpp` | A build switch for local llama.cpp support in the providers crate | [Support matrix](../../docs/SUPPORT_MATRIX.md) |

```powershell
cargo build -p argentum-cli --bin argentum --release --locked --features extensions
```
