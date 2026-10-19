# Argentum platform / Genesis CLI

Argentum is the local-first native Rust platform in the canonical
[AG064/argentum repository](https://github.com/AG064/argentum).
Argentum Genesis is its CLI/TUI product, not a separate repository or runtime
fork. The executable remains `argentum` and the Cargo package remains
`argentum-cli`. The `0.1.0-rc.1` transition is a Windows x64 CLI prerelease
candidate, not a published release or a claim that it replaces Codex.

The native command host owns providers, scoped policy, guarded file changes,
verification, cancellation, durable history and recovery. The Slint and Android
hosts remain source modules, not release-qualified products in this CLI package.
The CLI, future desktop frontend and optional browser/extension capabilities
share this native core. Genesis may release independently of desktop/browser
acceptance. The previous TypeScript/Node/Tauri product is explicitly legacy;
its preserved branch, immutable revisions and existing releases are linked in
[the migration record](docs/MIGRATION_NATIVE.md). Its feature list is not the
native CLI's feature list.

The Windows x64 package requires the Universal CRT and Microsoft Visual C++
x64 runtime (`VCRUNTIME140.dll`). These system/runtime components are not
redistributed in the package. The recorded local host is Windows build 26100;
this is not clean-machine or every-Windows-version qualification.

## Build from a fresh checkout

Install Rust 1.98.0 with rustfmt and Clippy, Git, and the Windows MSVC C++ build
tools. No Node.js, private compiler cache, model download or old executable is
required. Cargo.lock pins the dependency resolution.

```powershell
git clone https://github.com/AG064/argentum.git
cd argentum
# Until the native transition is merged, select the reviewed native PR branch.
git switch codex/release01-native-cli-20261010
cargo build -p argentum-cli --bin argentum --release --locked
& '.\target\release\argentum.exe' --help
```

To prepare a versioned portable package, use
`scripts/package-windows-cli.ps1 -OutputDirectory C:\temporary\package`.
The script refuses dirty source and existing output, produces checksums and
source provenance, and neither signs nor publishes anything.

The workspace defaults to `argentum-cli` with no default optional features.
Workspace membership does not include every crate in the CLI binary. The
Windows normal/build dependency selection excludes Slint, the desktop host,
Android JNI/llama.cpp and external MCP; packaging selects this same CLI target
without optional features. See [the verified boundaries](docs/architecture.md).
NEXT01 adds an explicit `extensions` feature in this development branch, not
in the RELEASE01 core package. Its supported local-browser contribution reached
post-recovery native completion on 2026-10-13, with current source verification,
model acknowledgement and independent rendered acceptance. Historical deadline
failures and estimated-usage dispositions remain recorded; this was assisted
completion of retained work, not uninterrupted autonomy or general reliability.
See [the dated NEXT01 evidence](docs/NEXT01_ACCEPTANCE.md).

NEXT02 adds an explicitly selected `desktop-control` feature, not a default core
dependency. Its declared native-editor workflow reached assisted post-repair
completion on 2026-10-16, with current verification, model acknowledgement,
independent saved-artifact review and scoped image evidence. This does not qualify
general desktop parity or change release status. See
[the dated NEXT02 evidence](docs/NEXT02_ACCEPTANCE.md) and
[its application boundary](docs/WINDOWS_APPLICATION_CONTROL.md).
The [separate dependency maintenance](docs/NEXT02_DEPENDENCY_MAINTENANCE.md)
preserves that acceptance and requires its own source/package qualification.

## First run

NEXT03's optional `artifacts` development feature completed one bounded local
Markdown-to-PDF workflow through the same native runtime on 2026-10-17.
It requires no Google account.
Its pinned worker, image and verification boundaries are described in
[local artifacts](docs/LOCAL_ARTIFACTS.md). Qualification is separate from the
accepted NEXT01/NEXT02 workflows, connected-app acceptance and the core RC.
See [the scoped acceptance record](docs/NEXT03_LOCAL_ACCEPTANCE.md), including
independent review, assistance and exact compiled-package provenance.

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

The [clean-source integration record](docs/GENESIS_INTEGRATION01.md) separates
accepted runtime repairs, the generic optional-adapter capture correction and
retained later native changes from task-local configuration and private
evidence. Its integrated packages require their own source-bound qualification;
historical live acceptance is not transferred to a new executable.

This transition reuses accepted native workflow evidence and adds clean-source,
package and credential-free loopback qualification. It makes no new live
provider requests. MILESTONE01 was an assisted delivery with review and recovery
interventions, not unassisted autonomous reliability evidence.

Read [release status](docs/releases/0.1.0-rc.1.md),
[platform/provider support](docs/SUPPORT_MATRIX.md),
[migration](docs/MIGRATION_NATIVE.md), and [the two-track roadmap](ROADMAP.md).
Browser automation, external plugins/MCP and general computer use are not
included in the RELEASE01 core release. NEXT01's optional implementation and
bounded post-recovery acceptance remain separate. No hidden telemetry upload, self-update,
model-owned shell approval, push, merge or publication capability is added.

## Development and licensing

Use reviewable feature branches in this repository. Create release-maintenance
branches only for an actual supported version that needs them. Do not create a
permanent CLI-only branch, a separate Genesis repository or frontend-specific
copies of the shared runtime. Existing transition branches are review branches,
not permanent product lines.

[CONTRIBUTING.md](CONTRIBUTING.md) describes focused tests and contribution
requirements. [SECURITY.md](SECURITY.md) defines the actual trust boundary and
private vulnerability reporting.

The project license is MIT-derived with an additional mandatory acknowledgement,
not unmodified SPDX MIT. Cargo inherits the exact [LICENSE](LICENSE) through
`license-file`. Keep its text and the acknowledgement in distributions.
[THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md) identifies separate third-party
terms. The CLI package does not bundle Slint UI, a model, llama.cpp or an Android
runtime. No public release, tag or package publication is automatic.
