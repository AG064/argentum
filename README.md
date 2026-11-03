<p align="center">
  <img src="assets/brand/argentum-128.png" alt="Argentum logo" width="96" height="96">
</p>

<h1 align="center">Argentum</h1>

<p align="center">
  A local-first AI coding agent in native Rust, with a modular desktop app and a terminal product on one shared core.
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
- **Guarded changes:** edits must match the exact source the agent read, and changes
  can be reviewed and restored.
- **Checked work:** verification runners produce receipts tied to the source they
  checked.
- **Durable history:** sessions and runs survive restarts, and recovery after a crash
  is explicit, never automatic.
- **Your choice of model:** hosted providers, aggregators, your own server, or local
  llama.cpp.
- **Modular:** extensions, MCP, a browser adapter, desktop control, and local artifacts
  are optional at build time.

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
