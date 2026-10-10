# Native transition and coexistence

The native transition is reviewed on a non-default draft branch. The default
branch, tags and earlier releases stay intact until an explicit cutover/release
decision. Earlier `v0.0.x` documentation describes the TypeScript product, not
the new CLI. Inspect that product through its existing release tags or the
pre-transition Git revision; it is not a second active root build here.

The native root is Cargo/Rust. Node package metadata, installers, dashboards,
bundled llama.cpp binaries, automatic Docker/package publishers and obsolete
TypeScript CI are removed from this proposed tree, not from Git history.
Native Slint assets and Android source remain separately qualified modules.

## Safe coexistence

Keep old binaries, stores and workspace backups. Put the native executable in
a distinct directory and use a new explicitly selected database during evaluation.
Do not copy credentials, grants or authority databases between applications or
projects. Do not point both writable hosts at a store simultaneously.

Native SQLite schema changes occur only through its supported writable host.
Historical diagnostic commands are read-only; they do not create/migrate stores.
Never silently recover an unrelated store. On a proven abandoned host, inspect
and perform supported exact recovery through the owner command path before
resumption. A copied database does not prove abandoned ownership.

There is no automatic TypeScript-store import, approval migration or API-key
export. Provider selections, credentials, budgets and policy must use the native
configuration path. Keep deliberate existing settings; never apply one model's
budget to unrelated models.

## Prepared cutover, not performed

After source review and the explicit release decision, the maintainer can merge
the native PR using normal protected-branch rules, publish the reviewed RC and
update the repository description/topics consistently. Proposed description:
"Native Rust coding-agent harness with scoped tools, guarded edits, verification
and recoverable local workflows. Windows CLI prerelease."

Suggested topics: `rust`, `cli`, `coding-agent`, `local-first`, `verification`.
Keep `development` as the default unless a separate reviewed branch decision is
made. Do not imply Codex replacement, browser automation or general desktop use.

## Authorship and retained evidence

The native implementation includes Argentum-authored REAL/MILESTONE changes and
separately authored supervisor infrastructure. RELEASE01 source reconciliation,
portable tests, metadata, documentation, CI and packaging are supervisor release
engineering. MILESTONE01 is assisted delivery. Existing authorship evidence and
source/postimage mappings stay private; the public transition does not rename
them as unassisted agent work.
