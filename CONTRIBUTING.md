# Contributing to Argentum

Argentum is a native Rust agent harness. Keep changes focused, preserve the
workspace security boundary, and describe unfinished work as unfinished.

## Developer Certificate of Origin

Every commit must be signed off under the [DCO](DCO.md):

```text
Signed-off-by: Name <email@example.com>
```

Use `git commit -s` to add the line. This is a DCO sign-off, not a GPG
signature.

## Setup

Install the Rust stable toolchain with the `rustfmt` and `clippy` components.
Platform SDKs are required only for the platform being built. The root
application does not require Node.js, npm, a JavaScript runtime, or a web
runtime.

Useful checks:

```powershell
cargo fmt --all -- --check
cargo check --workspace
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo deny check
```

For Windows packaging, run both scripts:

```powershell
.\scripts\build-windows-release.ps1
.\scripts\validate-windows-release.ps1
```

Do not commit `.env`, provider credentials, Android or Apple signing keys,
downloaded models, `target`, `artifacts`, local databases, or generated logs.
The `legacy/` tree is archival and must not be added as a runtime dependency.

## Commit flow

Argentum uses focused Conventional Commit messages. Examples:

```text
feat(runtime): add cancellable approval requests
fix(security): reject symlink escapes from workspace tools
docs: clarify release verification
```

Pull requests should explain:

- the behavior and reason for the change;
- security, data, migration, and platform impact;
- tests run and tests not run;
- screenshots or recordings when the native UI changes;
- follow-up work that remains intentionally out of scope.

New behavior needs targeted tests when practical. A bug fix should include a
regression test. Do not add fake provider results, placeholder success states,
dead controls, or production dependencies without a license and advisory
review.

## Native architecture requirements

- Keep application logic in Rust crates under `crates/`.
- Keep view markup in `ui/` and expose typed callbacks and properties through
  `argentum-ui`.
- Use bounded channels and cancellation for network, storage, and tool work.
- Keep file, shell, network, and process capabilities behind explicit policy
  checks.
- Keep secrets out of SQLite, event logs, diagnostics, and screenshots.
- Do not reintroduce a JavaScript runtime, web content surface, or dynamic
  extension runtime into the shipped application.

## Licensing

Contributions are licensed under the project license. Third-party code, assets,
models, and platform components need source, revision, license, and notice
information. The allowed dependency policy is recorded in `deny.toml` and must
pass `cargo deny check`. Slint attribution must remain available in the About
surface before release.

## Security reports

Do not open a public issue for a vulnerability. Follow [SECURITY.md](SECURITY.md).
