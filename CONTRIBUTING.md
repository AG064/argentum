# Contributing

The active product is the native Cargo workspace. The CLI is its default member;
Slint/Android are separately qualified modules. Earlier TypeScript code is in
history, not a runtime dependency. Keep changes focused and preserve dirty work.

Use Rust 1.98.0, rustfmt, Clippy, Git and platform build tools. Run qualification
in a disposable checkout with output/fixtures outside protected user stores:

```powershell
cargo fetch --locked
cargo fmt --all -- --check
cargo check --workspace --all-targets --locked
cargo test --workspace --all-targets --locked -- --test-threads=1
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo deny check
```

Ordinary tests use synthetic or credential-free loopback inputs. Ignored live
provider/local-model tests require explicit separate authorization; do not run
`--ignored` as a release shortcut. Compiler/test caches are generated outputs,
not required source or readiness evidence. Packaging uses the portable script
in `scripts/`, from a clean committed checkout, and never publishes automatically.

Every commit requires a [DCO](DCO.md) sign-off under the repository's existing
policy. Use `git commit -s` only when entitled to make that attestation; it is
not a GPG signature. Use scoped Conventional Commit messages and describe
actual authorship. Do not attribute supervisor-authored infrastructure to a model.

Review must state behavior, source scope, security/data/platform impact, exact
tests and exclusions. Native UI changes need their own visible acceptance;
compilation alone is insufficient. Separate observed results from provider/model
quality, autonomy or release claims. Preserve stale-write, policy, explicit
approval, containment, cancellation and accounting semantics.

Never commit credentials, user/session/authority stores, private prompts or
campaign evidence, signing material, downloaded models, local runtimes or
compiler outputs. Future optional adapters must be governed by the native
authority/lifecycle boundary, not an ungoverned execution world.

The project uses the exact MIT-derived acknowledgement-bearing [LICENSE](LICENSE),
referenced through Cargo `license-file`, not unmodified MIT. Contributions keep
that grant. Third-party files retain their own texts and provenance. CLI packages
retain applicable dependency notices; a future UI distribution must separately
resolve Slint's distribution options and attribution. Report vulnerabilities
privately through [SECURITY.md](SECURITY.md), not public issues.
