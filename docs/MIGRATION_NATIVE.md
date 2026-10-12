# Native transition and coexistence

## Repository and product decision, 2026-10-12

[AG064/argentum](https://github.com/AG064/argentum) remains the canonical
repository for the native Argentum platform. Argentum Genesis is the CLI/TUI
product name; `argentum` remains the executable and `argentum-cli` the Cargo
package. The CLI, future desktop frontend and optional browser/extension
capabilities share the existing native core. Genesis may release independently
of desktop/browser readiness, without claiming computer use or Codex replacement.

Use feature branches for reviewable development, and release-maintenance
branches only when an actual supported version needs maintenance. The current
RELEASE01 and NEXT01 draft branches are review branches, not permanent CLI
product lines. Do not create an `argentum-genesis` repository, a permanent
CLI-only branch or separate frontend copies of the runtime. Existing Cargo
boundaries are preserved; no cosmetic crate moves or renames are required.
See [architecture and verified default selection](architecture.md).

## Legacy TypeScript product: preserved references

The previous TypeScript/Node/Tauri product is legacy, not another maintained
native runtime or the source of current native capability claims. These
references were checked against GitHub on 2026-10-12:

| Preserved reference | Exact identity and link |
| --- | --- |
| Existing pre-transition branch | [development](https://github.com/AG064/argentum/tree/development), still the default branch at this inspection; branch tips can change |
| Inspected branch snapshot | [`c01a1c4d3019e85b72b332a7abd3e5695150dcc9`](https://github.com/AG064/argentum/tree/c01a1c4d3019e85b72b332a7abd3e5695150dcc9), with the legacy Node/TypeScript package |
| Published legacy v0.0.9 source | Annotated tag object `4b2ef41789a15666743dd2a694dc5dc461a26733`, resolving to [commit `5c9d066c04a8ff87f4a3f9d6e466aa794c92023f`](https://github.com/AG064/argentum/tree/5c9d066c04a8ff87f4a3f9d6e466aa794c92023f) |
| Existing legacy releases | [v0.0.9](https://github.com/AG064/argentum/releases/tag/v0.0.9), [v0.0.7](https://github.com/AG064/argentum/releases/tag/v0.0.7), [v0.0.5](https://github.com/AG064/argentum/releases/tag/v0.0.5), [v0.0.4](https://github.com/AG064/argentum/releases/tag/v0.0.4), [v0.0.3](https://github.com/AG064/argentum/releases/tag/v0.0.3), [v0.0.2](https://github.com/AG064/argentum/releases/tag/v0.0.2), [v0.0.1](https://github.com/AG064/argentum/releases/tag/v0.0.1) |

The immutable snapshot and release tag retain legacy source even after a
future separately approved default-branch transition. No branch, tag, release,
default setting or historical outcome was changed for this documentation.

## Transition scope

The native transition is reviewed on a non-default draft branch. The default
branch, tags and earlier releases stay intact until an explicit cutover/release
decision. Earlier `v0.0.x` documentation describes the TypeScript product, not
Genesis or the native platform. Use the verified legacy references above;
it is not a second active root build here.

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
and recoverable local workflows. Argentum platform / Genesis CLI prerelease."

Suggested topics: `rust`, `cli`, `coding-agent`, `local-first`, `verification`.
Keep `development` as the default unless a separate reviewed branch decision is
made. Do not imply Codex replacement, browser automation or general desktop use.

RELEASE01 is core CLI source/package qualification, not NEXT01 feature
acceptance. NEXT01's governed extension/browser development stays in its
separate draft review. Its frozen live deadline failure, unknown final usage
and unavailable private-action evidence remain unchanged. Passing core CI
does not turn that run into a successful completion. This documentation
authorizes no cutover, release, feature integration or accounting resolution.

## Authorship and retained evidence

The native implementation includes Argentum-authored REAL/MILESTONE changes and
separately authored supervisor infrastructure. RELEASE01 source reconciliation,
portable tests, metadata, documentation, CI and packaging are supervisor release
engineering. MILESTONE01 is assisted delivery. Existing authorship evidence and
source/postimage mappings stay private; the public transition does not rename
them as unassisted agent work.
