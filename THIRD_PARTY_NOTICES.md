# Third-party notices

## Optional local artifact assets

The explicitly selected local artifact package includes PDFium chromium/8100
(Windows x64, non-V8/non-XFA) from bblanchon/pdfium-binaries and Noto Sans from
Google Fonts revision 2984c575fdce412ee02b2baaba67672b9a9434d8. Exact upstream
URLs, byte bounds and SHA-256 pins are in `third-party/local-artifact-assets.json`.
PDFium's upstream license and bundled component notices are retained with the
optional package. Noto Sans is under SIL Open Font License 1.1; its exact OFL
text is included in that package as `OFL-NotoSans.txt`. Neither is needed to
start Argentum.

Argentum by AG064 (https://github.com/AG064/argentum).
First-party code is licensed under `AGPL-3.0-only` ([LICENSE](LICENSE)), with the
additional terms in [NOTICE.md](NOTICE.md). This file lists the separate terms of
third-party components.

Cargo.lock pins Rust package versions/checksums. The portable CLI package
contains `dependency-notices.json` for its selected dependency closure and full
license/notice files under `licenses/`. Build-only dependencies are included
conservatively; this does not imply they are all linked into the executable.
Dependency license policy is checked with `cargo deny` against `deny.toml`.

`eventsource-stream` 0.2.3 and `reqwest-eventsource` 0.6.0 declare
`MIT OR Apache-2.0` but omit license text in both their registry archives and
the recorded upstream revisions. Packaging explicitly selects the declared
Apache-2.0 option for these exact revisions, retains full standard terms and
their original manifest/README grant and attribution metadata, and records the
selection. `third-party/missing-license-texts.json` pins the declaration sources
and retained term identity. An unknown missing notice still fails packaging.

## Separate source modules and optional downloads

| Component | Terms and handling |
| --- | --- |
| Lucide/selected Feather SVG assets in `ui/assets/icons/lucide` | ISC and retained MIT terms in that directory's LICENSE; retained with Slint source, not in the CLI binary package |
| Slint 1.18.1 | `GPL-3.0-only OR LicenseRef-Slint-Royalty-free-2.0 OR LicenseRef-Slint-Software-3.0`; preserve upstream options, do not simplify to MIT. Argentum uses Slint under its `GPL-3.0-only` option, which section 13 of the AGPL-3.0 allows to be combined with this project, and keeps the `AboutSlint` widget in Settings > About. The CLI package does not include Slint |
| Optional Windows llama.cpp b10621 | MIT upstream runtime; explicit separately pinned download, not bundled in the CLI package |
| rmcp 3.9.0 | Apache-2.0; only in builds with the optional `extensions` feature |
| Playwright 1.64.0 in `extensions/browser` | Apache-2.0; installed separately for the optional browser adapter, not bundled in any package |
| Optional Qwen3.5-0.8B GGUF offers | Apache-2.0 model terms; source/revision/hash/size metadata is embedded in the bounded offers; no model bytes packaged |
| Android llama-cpp-2 / llama-cpp-sys-2 0.1.156 | MIT OR Apache-2.0 crates with vendored upstream source; target-specific Android module, not CLI runtime payload |
| Gradle 9.1.0 wrapper | Apache-2.0; retained wrapper source/JAR and pinned distribution checksum under platforms/android; not CLI runtime payload |
| Public ring signing-test DER fixture | Deliberately public test key, never a user credential; provenance is in crates/argentum-providers/tests/fixtures/chatgpt/README.md |

Third-party notices are not rewritten as project-owned licenses. Download/install
consent is not authority to redistribute optional models/runtimes. Before another
platform package is distributed, include its actual component notices and review
its applicable upstream terms.
