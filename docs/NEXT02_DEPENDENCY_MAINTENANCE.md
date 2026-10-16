# NEXT02 dependency maintenance, 2026-10-16

This is source maintenance after the completed, assisted NEXT02 editor workflow.
It does not reopen that task, add live acceptance, change account configuration,
backport the core RC or activate NEXT03. The original CLI/adapter/package and
their native acceptance remain bound to compiled commit
`183a0390ab1b80a715dbd6490f3eb048f6728fe1`, tree
`01f4a15460dd3af162b9e57c21a59aa0b783826d`.
See [the original acceptance](NEXT02_ACCEPTANCE.md).

## Dependency selection and exact scope

| Package | Before | Selected successor | Reason |
| --- | --- | --- | --- |
| rustls | 0.23.45 | 0.23.46 | Compatible patched 0.23 release for RUSTSEC-2026-0346/0347/0348/0349. |
| zip | 2.4.2 | 9.0.4 | Every registry 2.x release is yanked. The maintainer supports only the latest release; 9.0.4 supports the existing Rust 1.98 toolchain. |
| typed-path | Absent | 0.12.3 | Required by the selected zip reader's path implementation. |
| derive_arbitrary | 1.4.2 | Removed | No longer required by zip; the remaining arbitrary 1.4.2 loses only that derive edge. |

Every other existing registry package keeps its version. No unqualified whole-
workspace update or lockfile deletion is used. `reqwest` remains 0.12.28,
`hyper-rustls` 0.27.9 and `tokio-rustls` 0.26.4. The TLS backend remains ring,
with std/tls12. No certificate, hostname, redirect or credential policy is relaxed.

Checksums in the exact registry/lock records:

| Package/version | SHA-256 |
| --- | --- |
| rustls 0.23.45 | `0d41d731c7d2f962d1ccc364cec258de3c0e93b38c2fb3ba97ac74513048d634` |
| rustls 0.23.46 | `48e13bd8c0e9365c43cfa5c9e8f9ad49d3c8444926c9aac819e0e4dc503c8fdf` |
| zip 2.4.2 | `fabe6324e908f85a1c52063ce7aa26b68dcb7eb6dbc83a2d148403c9bc3eba50` |
| zip 9.0.4 | `a954114b47a54df49f83d2722814e86b23d4bb1b54a1b90fb7d92b41619ddc22` |
| typed-path 0.12.3 | `8e28f89b80c87b8fb0cf04ab448d5dd0dd0ade2f8891bae878de66a75a28600e` |

The default core, `extensions` and `desktop-control` selections all reach the
same Rustls/Zip through `argentum-providers`. Rustls is transitive through
reqwest, hyper-rustls and tokio-rustls; Zip is a workspace-declared normal provider
dependency used by the pinned runtime-bundle inspector/extractor. There is no
production Zip writer. Its writer is used only to construct test archives.

Stored/Deflate formats and the previous Rust DEFLATE backend are preserved.
Zip's explicit `deflate-flate2` and `deflate-zopfli` features avoid its newly
selected implicit zlib-rs backend. A direct flate2 declaration explicitly retains
`rust_backend`; flate2 1.1.10 and zopfli 0.8.3 were already normal dependencies.
No encryption or additional compression codec is enabled. The new direct
tokio-rustls declaration is dev-only for synthetic TLS services and reuses the
already locked version/backend. No new server is compiled into the product.

The C# desktop adapter is built independently from its unchanged source,
build script and installed .NET UI Automation references. It does not link Cargo
Rustls or Zip. The optional browser adapter remains on its unchanged pinned npm
graph. The distributable ZIP itself is produced by PowerShell Compress-Archive,
not by the Rust Zip dependency. Build inputs and emitted adapter hashes must be
recorded separately; unchanged source alone does not prove identical binaries.

## Compatibility and trust-boundary controls

Zip 9 makes the entry reader generic and filename decoding fallible. The
consumer maps decoding failures to its existing invalid-bundle error.
Its `enclosed_name` also normalizes root/drive prefixes and parent segments;
the old version rejected those or left them visible to the existing component
check. The consumer therefore checks original components and Windows drive
prefixes before library normalization, preserving rejection rather than silently
accepting a rewritten path. Existing post-normalization, symlink, encryption,
size, hash, private-directory and create-new checks remain.

Production-path controls cover Stored/Deflate Unicode extraction, malformed
archives, stale CRC despite matching host digests, per-entry and aggregate
declared size limits, entry count, traversal/root/drive-relative/backslash names,
symlink rejection and held launcher/archive identity. Unsafe test names are
injected into both literal ZIP headers because the newer writer normalizes them;
the controls confirm the reader sees the intended name before testing rejection.

Synthetic TLS controls run the actual prepared provider streaming path against
an owned loopback TLS service. They cover a trusted localhost connection and
synthetic credential routing, rejection of an untrusted root and wrong hostname
before HTTP, and cancellation after the first delta with no later event.
Only test clients trust the public synthetic root; no operating-system trust
store, production client setting, provider endpoint or secure binding changes.
The server uses an existing intentionally public ring documentation test key.

## Qualification and retained tracks

Initial current policy checks passed without exceptions for the workspace and
all three selected CLI profiles against RustSec database revision
`479c888ce4401313bca61ba4aba2d0f6244c4a09`, retrieved
2026-10-16 17:34:30 UTC. Final hosted qualification records its own current
database revisions, retrieval time and lock/policy hashes. An advisory match
alone is not exploit validation; disabled Brotli does not dismiss every advisory.

Required successor gates are combined credential-free tests, strict lint,
formatting, workspace/desktop compilation, exact packaged controls, notices and
checksums, Unicode/space-path startup, and complete hosted policy and packaging.
Results are reported by the maintenance follow-up PR and exact artifact
provenance. A source document or successful subset is not proof of those gates.
No new live autonomy result or full security certification is claimed.

Primary selection references: [Rustls 0.23.46 release](https://github.com/rustls/rustls/releases/tag/v%2F0.23.46),
[RustSec 0346](https://rustsec.org/advisories/RUSTSEC-2026-0346.html),
[0347](https://rustsec.org/advisories/RUSTSEC-2026-0347.html),
[0348](https://rustsec.org/advisories/RUSTSEC-2026-0348.html),
[0349](https://rustsec.org/advisories/RUSTSEC-2026-0349.html),
[Zip support policy](https://github.com/zip-rs/zip2/blob/master/SECURITY.md),
[Zip 9.0.4 release](https://github.com/zip-rs/zip2/releases/tag/v9.0.4), and
[official registry](https://crates.io/crates/zip/9.0.4).

The retained core RC also locks rustls 0.23.45 and zip 2.4.2. It is not silently
modified or newly qualified by this branch. Any backport remains a separate
maintainer decision. Closed PRs #690/#691 and all failed/cancelled qualification
runs remain historical; this maintenance uses a draft follow-up against the
retained NEXT02 branch, not a merge or repeated reopening.
