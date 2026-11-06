<p align="center">
  <img src="assets/brand/argentum-128.png" alt="Argentum logo" width="96" height="96">
</p>

<h1 align="center">Argentum</h1>

<p align="center">
  A local-first AI coding agent in native Rust, with a modular desktop app and a terminal product on one shared core.
</p>

<p align="center">
  <a href="https://github.com/AG064/argentum/actions/workflows/native-cli.yml"><img alt="CI" src="https://github.com/AG064/argentum/actions/workflows/native-cli.yml/badge.svg?branch=development"></a>
  <a href="https://github.com/AG064/argentum/actions/workflows/codeql.yml"><img alt="CodeQL" src="https://github.com/AG064/argentum/actions/workflows/codeql.yml/badge.svg?branch=development"></a>
  <a href="https://scorecard.dev/viewer/?uri=github.com/AG064/argentum"><img alt="OpenSSF Scorecard" src="https://api.scorecard.dev/projects/github.com/AG064/argentum/badge"></a>
</p>

![The Argentum desktop app with the Git diff module open](docs/screenshots/desktop-git.png)

## Two products, one core

<table>
  <tr>
    <th width="50%"><a href="crates/argentum-app/README.md">Argentum desktop</a></th>
    <th width="50%"><a href="crates/argentum-cli/README.md">Argentum Genesis</a></th>
  </tr>
  <tr>
    <td><a href="crates/argentum-app/README.md"><img src="docs/screenshots/desktop-minimal.png" alt="The minimal desktop interface"></a></td>
    <td><a href="crates/argentum-cli/README.md"><img src="docs/screenshots/cli-chat.png" alt="A conversation in the terminal shell"></a></td>
  </tr>
  <tr>
    <td>The native Slint app. It starts with a provider and a conversation. Everything else is an optional module: Git diff, a page reader, usage, goals, and more.</td>
    <td>The CLI and terminal shell, <code>argentum</code>. Chat interactively, run headless tasks with JSON output, or serve JSONL to another program.</td>
  </tr>
  <tr>
    <td><code>cargo run -p argentum-app</code></td>
    <td><code>cargo run -p argentum-cli --bin argentum</code></td>
  </tr>
  <tr>
    <td><b>Status:</b> built from source, no installer yet</td>
    <td><b>Status:</b> Windows x64 prerelease candidate, not published yet</td>
  </tr>
</table>

Both run on the same native core, which owns providers, scoped policy, guarded file
changes, verification, cancellation, durable history, and recovery. Genesis can release
on its own, and a default CLI build does not include the desktop code.

## What sets it apart

- **Local first:** your projects, sessions, and history stay on your machine. There is
  no hidden telemetry, and updates contact GitHub only when you ask.
- **You stay in control:** the saved project policy decides what the agent can do.
  Prompts, model output, and remote content cannot grant it more.
- **Durable history:** sessions and runs survive restarts, and recovery after a crash
  is explicit, never automatic.

## See what it did

Each response starts with one line saying what the agent did, with the size of the change. Open
it for the exact steps, and open a file to read its diff in place.

![A response with its steps and a file diff open](docs/screenshots/desktop-activity.png)

From a real run with DeepSeek:

```text
> Thought process
v Changed app.py, read 2 files, listed a folder, ran a command  +6
    Read app.py
    Listed the project folder
    Applied a patch
    Ran `python app.py`
    Read app.py
v app.py  +6
```

```diff
@@ -1,4 +1,5 @@
 def add(a, b):
+    """Return the sum of a and b."""
     return a + b

@@ -6,5 +7,10 @@
     return a - b

+def multiply(a, b):
+    return a * b
+
+
 print(add(2, 3))
 print(subtract(5, 3))
+print(multiply(2, 3))
```

The terminal shows the same steps as they happen:

![The same kind of task in the terminal shell](docs/screenshots/cli-activity.png)

Details are for your screen only. Command text is not saved, so after a restart you keep the
answer and the recorded changes, and the step list is gone.

## Features

### How a task runs

| Step | What happens |
| --- | --- |
| 1. You ask | Your message and the project's instructions go to the model, inside a checked context budget |
| 2. The model works in rounds | Each round is one model request, and it can make up to 8 tool calls |
| 3. Policy decides | The saved project policy allows a tool, asks you first, or refuses it. Nothing the model says can change that |
| 4. You see each step | Reads, searches, commands, and edits appear as they happen, and thinking streams above them |
| 5. Changes are recorded | Each edit is checked against the source the agent read, and saved so you can review or restore it |
| 6. It stops | A run ends with an answer, at Esc or `/stop`, at the limit of 120 rounds, or when the model asks for the same tool calls four rounds in a row. Long tasks can be continued, or run as a goal |

### Goals

Give a session one objective, and let it work across several runs within limits you set.

![The goal panel, with sample data](docs/screenshots/desktop-goal.png)

```powershell
argentum goal set "Migrate the parser and keep the tests passing" --token-budget 1000000 --tool-budget 200 --time-budget 3600
argentum goal status
argentum goal supervise
```

You can amend limits, require verification runners, attest completion, pause, resume, and clear a
goal. `goal deliver` runs a finite goal in stages. See [goal accounting](docs/GOAL_ACCOUNTING.md).

### Providers and models

The catalog covers hosted services, aggregators, your own server, and local models:

| Type | Providers |
| --- | --- |
| Tested with Argentum | DeepSeek, OpenCode Go |
| Hosted | MiniMax, OpenAI, Google Gemini, Groq, Mistral, xAI, Cerebras, Perplexity, Cohere, Moonshot, Z.AI, Qwen, SambaNova, NVIDIA NIM |
| Plans | ChatGPT plan, with browser sign-in |
| Aggregators | OpenRouter, Together, Fireworks, GitHub Models, OpenCode Zen, DeepInfra, Novita, Hugging Face, Chutes, Venice, Avian, Requesty, Vercel AI Gateway |
| On your computer | llama.cpp, LM Studio, Ollama, vLLM |
| Your own | Any OpenAI-compatible server |

- **Real model names:** the app shows what is running, such as `DeepSeek-V4.1-Flash-1017`, not only the ID it sends.
- **Thinking modes:** switch DeepSeek between off, high, and max from the composer, or with `/thinking`.
- **Secure keys:** API keys go in the operating system's credential store, and each is only sent to its provider's own address.
- **Tested and Testing tabs:** Settings > Providers lists tested providers first, and the rest on a Testing tab, so you know which have not been run against Argentum. First run shows only tested providers until you choose Show all.

![The Testing tab of provider settings](docs/screenshots/desktop-provider-tabs.png)

### Context and budget

Change these in Settings > Providers, or with `argentum provider budget`:

- **Budget:** total, input, output, and uncertainty limits for each model. Models with known limits work without setup.
- **Context size:** pick the size for a local model in the app, under Context size.
- **Context meter:** an optional module showing how full the context is, and the terminal footer always shows it.
- **Compaction:** when the next request would not fit, a run summarizes earlier work and continues. You can also run `argentum compact` yourself. The summary loses detail, and Argentum tells you when one is in use.

A request that would not fit is refused before it is sent.

### Control and safety

- **Permission profiles:** Observe, Guided, Automatic, Verification, Developer, and Full Access. Guided is the default.
- **Clear approvals:** a prompt shows the exact action and target, and you can approve once or reject.
- **Guarded edits:** an edit must match the exact source the agent read. A stale or ambiguous edit is rejected.
- **Review and restore:** look through the files a run changed, then restore them, checking first that they have not changed since.
- **Verification:** fixed runners produce receipts tied to the source they checked.
- **Recovery:** after a crash, `argentum recovery inspect` shows what happened, and takeover is never automatic.

### Desktop app

- **Modules:** turn on only what you want: Sessions, Context meter, Reasoning, Goal, Command palette, Status bar, Changes, Git diff, Browser, Usage, Plan, Activity, and Trajectory.
- **Sessions:** search, rename, pin, archive, and fork.
- **Themes:** Argentum, Obsidian, Graphite, Ember, and Silver, plus density, text size, corners, colors, and fonts.
- **Interface tools:** the agent can adjust the interface when you approve it, and can never touch permissions or the approval prompt.
- **Updates:** contacts GitHub only when you click, checks the SHA-256 sum, and rolls back if installing fails.

![The command palette, with sample data](docs/screenshots/desktop-palette.png)

### Terminal and automation

| Mode | Command | For |
| --- | --- | --- |
| Chat shell | `argentum chat` | Interactive work with selectable panels |
| Headless | `argentum run --prompt TEXT --json` | One task, streamed as JSON |
| Server | `argentum serve` | JSONL on stdin and stdout, for other programs |

`argentum run` cannot approve its own actions, so a task that needs you fails with
`approval_required`. Use `--final-result` for one settled result, and `--answer-schema` to check the
answer against a schema.

### Optional at build time

Extensions, MCP, a browser adapter, desktop control, and local artifacts can be built in. A default
CLI build leaves them out.

### Known limits

- A run stops after 120 rounds, and earlier if the model repeats the same tool calls four rounds in a row.
- The steps and diff list on each response last for the running session only.
- Windows x64 is the only tested platform, and it has not yet been tested on a clean machine.

## Quick start

You need Rust 1.98.0, Git, and on Windows the MSVC C++ build tools. No Node.js is
needed.

```powershell
git clone https://github.com/AG064/argentum.git
cd argentum
cargo run -p argentum-app
```

For the terminal product, and for setup details, see
[its page](crates/argentum-cli/README.md).

## Project status

No native release has been published. The latest published release, v0.0.9, is the
earlier TypeScript product, which is now legacy. Its source and releases are linked in
[the migration record](docs/MIGRATION_NATIVE.md).

Windows x64 is the only tested platform. It has been tested on one Windows build, not
on a clean machine or across Windows versions. macOS, Linux, Android, and iOS are
targets, not qualified products. Argentum is not yet a proven replacement for other
coding agents.

| Read this | For |
| --- | --- |
| [Support matrix](docs/SUPPORT_MATRIX.md) | What each platform and provider has been tested on |
| [Roadmap](ROADMAP.md) | What is next, and the release gates |
| [Release record](docs/releases/0.1.0-rc.1.md) | Exact results for `0.1.0-rc.1` |
| [Security policy](SECURITY.md) | The trust boundary, and how to report a vulnerability privately |
| [All documentation](docs/README.md) | Guides, architecture, and evidence |

## Repository layout

| Path | What |
| --- | --- |
| [`crates/argentum-app`](crates/argentum-app/README.md) | Desktop host |
| [`crates/argentum-cli`](crates/argentum-cli/README.md) | Genesis CLI and terminal shell |
| [`ui/`](ui/) | Slint interface, tokens, and icons |
| [`crates/`](crates/) | Shared runtime, store, tools, providers, and security |
| [`platforms/`](platforms/README.md) | Native platform targets |
| [`docs/`](docs/README.md) | Documentation |

## Contributing

Use a focused branch named `feature/`, `fix/`, or `docs/`. There is one repository and
one shared runtime, so please do not create separate CLI-only branches or copies for one
frontend. [CONTRIBUTING.md](CONTRIBUTING.md) describes the checks to run.

## License

The project license is MIT-derived with an additional mandatory acknowledgement, not
unmodified SPDX MIT. Cargo inherits the exact [LICENSE](LICENSE) through `license-file`.
Keep its text and the acknowledgement in distributions.
[THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md) identifies separate third-party terms.
The CLI package does not bundle the Slint UI, a model, llama.cpp, or an Android runtime.
