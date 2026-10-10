# Argentum native CLI

Argentum is a local-first Rust coding-agent harness. The `0.1.0-rc.1` source
transition is a Windows x64 CLI prerelease candidate, not a published release
or a claim that it replaces Codex. Argentum by AG064
(https://github.com/AG064/argentum).

The native command host owns providers, scoped policy, guarded file changes,
verification, cancellation, durable history and recovery. The Slint and Android
hosts remain source modules, not release-qualified products in this CLI package.
The historical TypeScript product remains in earlier Git revisions and releases;
its plugins, channels and browser features are not native CLI capabilities.

## Build from a fresh checkout

Install Rust 1.98.0 with rustfmt and Clippy, Git, and the Windows MSVC C++ build
tools. No Node.js, private compiler cache, model download or old executable is
required. Cargo.lock pins the dependency resolution.

```powershell
git clone https://github.com/AG064/argentum.git
cd argentum
# Until the native transition is merged, select the reviewed native PR branch.
git switch codex/release01-native-cli-20261010
cargo build -p argentum-cli --release --locked
& '.\target\release\argentum.exe' --help
```

To prepare a versioned portable package, use
`scripts/package-windows-cli.ps1 -OutputDirectory C:\temporary\package`.
The script refuses dirty source and existing output, produces checksums and
source provenance, and neither signs nor publishes anything.

## First run

Run from any directory. Supply the same workspace and database on every command
when using an explicit store. Choose a new database for initial evaluation,
rather than copying another application's or another project owner's store.

```powershell
$exe = 'C:\tools\argentum\argentum.exe'
$ws = 'C:\projects\example'
$db = 'C:\state\argentum\example.sqlite3'
& $exe setup --workspace $ws --database $db
& $exe status --workspace $ws --database $db --json
& $exe chat --workspace $ws --database $db
```

Setup selects a provider/model, secure credentials, explicit model budget and
workspace policy. A saved selection is not readiness, entitlement or permission.
Use masked secure key entry or native ChatGPT browser consent, never a key in a
command, chat prompt, URL or configuration file. See [the CLI guide](docs/CLI_GUIDE.md)
and [ChatGPT sign-in](docs/CHATGPT_SIGN_IN.md).

The default requires confirmation before changes. Bounded search and UTF-8
reads provide exact source identities. Edits require SHA-bound preimages and
reject stale or ambiguous selections. Fixed and exactly approved project
verification produce revision-bound receipts. Supplemental Rust checks are
optional executable input with their own owner-enabled boundary and limited
project support; a passing check is not proof of every public requirement.

Headless `run` and JSONL `serve` cannot grant themselves authority. An action
requiring an unavailable owner responder fails explicitly with
`approval_required`, rather than waiting forever or silently approving it.

## Evidence and limits

This transition reuses accepted native workflow evidence and adds clean-source,
package and credential-free loopback qualification. It makes no new live
provider requests. MILESTONE01 was an assisted delivery with review and recovery
interventions, not unassisted autonomous reliability evidence.

Read [release status](docs/releases/0.1.0-rc.1.md),
[platform/provider support](docs/SUPPORT_MATRIX.md),
[migration](docs/MIGRATION_NATIVE.md), and [the two-track roadmap](ROADMAP.md).
Browser automation, external plugins/MCP and general computer use are not
implemented in this native release. No hidden telemetry upload, self-update,
model-owned shell approval, push, merge or publication capability is added.

## Development and licensing

[CONTRIBUTING.md](CONTRIBUTING.md) describes focused tests and contribution
requirements. [SECURITY.md](SECURITY.md) defines the actual trust boundary and
private vulnerability reporting.

The project license is MIT-derived with an additional mandatory acknowledgement,
not unmodified SPDX MIT. Cargo inherits the exact [LICENSE](LICENSE) through
`license-file`. Keep its text and the acknowledgement in distributions.
[THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md) identifies separate third-party
terms. The CLI package does not bundle Slint UI, a model, llama.cpp or an Android
runtime. No public release, tag or package publication is automatic.
