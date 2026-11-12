# Contributing

Thank you for helping with Argentum. Bug reports, fixes and improvements are
welcome as issues and pull requests in this repository.

## Before you start

- For a large change, open an issue first so we can agree on the approach.
- Report security problems privately, as described in [SECURITY.md](SECURITY.md),
  never in a public issue.
- Follow the [code of conduct](CODE_OF_CONDUCT.md).

## Contributor agreement

Every contribution needs the [contributor agreement](CONTRIBUTOR_AGREEMENT.md).
You keep the copyright in your work. The agreement lets the project ship it
under the AGPL-3.0 and under separate commercial licenses. To accept it, put
this line in your first pull request's description:

> I have read the Argentum Contributor Agreement 1.0 and I agree to it.

## Building and checking

The project is a Rust workspace. Use the toolchain pinned in
`rust-toolchain.toml`, with rustfmt and Clippy. Before opening a pull request,
run:

```powershell
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --all-targets --locked
cargo deny check
```

Some tests compile and run small Rust programs and need time. If they time out
on a busy machine, run them again with `-- --test-threads=1`.

Ordinary tests use local fake servers and need no network or API keys. Tests
marked `#[ignore]` need extra setup, such as a provider key, a local model,
pinned document assets, a Node browser, or a Windows desktop session. Run them
only when you have that setup.

The desktop app is `cargo run -p argentum-app`. The Android app is described in
[docs/ANDROID_HOST.md](docs/ANDROID_HOST.md).

## Pull requests

- Keep each pull request to one change, on a branch named for its purpose, such
  as `fix/session-search` or `docs/android-setup`.
- Use [Conventional Commit](https://www.conventionalcommits.org/) messages, for
  example `fix(ui): keep the composer above the keyboard`.
- Describe what changed, why, and how you tested it. A change to the interface
  needs a screenshot of the result.
- Do not weaken approvals, the workspace boundary or other safety checks.
- Never commit API keys, signing keys, personal data, downloaded models or build
  output.

## Licensing of your contribution

Argentum is licensed under the AGPL-3.0, with the additional terms in
[NOTICE.md](NOTICE.md). Third-party files keep their own licenses, listed in
[THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md). If your contribution includes
code from somewhere else, say where it came from and what its license is.
