# Third-party notices

## Optional local artifact assets

The explicitly selected local artifact package includes PDFium chromium/8100
(Windows x64, non-V8/non-XFA) from bblanchon/pdfium-binaries and Noto Sans from
Google Fonts revision 2984c575fdce412ee02b2baaba67672b9a9434d8. Exact upstream
URLs, byte bounds and SHA-256 pins are in `third-party/local-artifact-assets.json`.
PDFium's upstream license and bundled component notices are retained with the
optional package. Noto Sans is under SIL Open Font License 1.1; its exact OFL
text is included as `artifacts/OFL-NotoSans.txt`. These are not default core
startup dependencies, owner credentials, connected accounts or model downloads.

Argentum by AG064 (https://github.com/AG064/argentum).
First-party code uses the exact root LICENSE: MIT-derived terms with an additional
mandatory acknowledgement. Cargo `license-file` points to that preserved text;
no unqualified SPDX MIT claim is made. RELEASE01 intentionally retains the
acknowledgement under the maintainer's selected custom-license direction.

Cargo.lock pins Rust package versions/checksums. The portable CLI package
contains `dependency-notices.json` for its selected dependency closure and full
license/notice files under `licenses/`. Build-only dependencies are included
conservatively; this does not imply they are all linked into the executable.
Dependency policy is in deny.toml; its outcome and advisory database freshness
belong in the RC evidence. A source policy pass is not legal certification.

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
| Slint 1.17.1 | `GPL-3.0-only OR LicenseRef-Slint-Royalty-free-2.0 OR LicenseRef-Slint-Software-3.0`; preserve upstream options, do not simplify to MIT; UI distribution decision is outside the CLI package |
| Optional Windows llama.cpp b10621 | MIT upstream runtime; explicit separately pinned download, not bundled in the CLI package |
| LLVM OpenMP in the optional runtime archive | Apache-2.0 with LLVM Exceptions; installer preserves `LICENSE-LLVM-OpenMP` in extracted runtime |
| Optional Qwen3.5-0.8B GGUF offers | Apache-2.0 model terms; source/revision/hash/size metadata is embedded in the bounded offers; no model bytes packaged |
| Android llama-cpp-2 / llama-cpp-sys-2 0.1.156 | MIT OR Apache-2.0 crates with vendored upstream source; target-specific Android module, not CLI runtime payload |
| Gradle 9.1.0 wrapper | Apache-2.0; retained wrapper source/JAR and pinned distribution checksum under platforms/android; not CLI runtime payload |
| Public ring signing-test DER fixture | Deliberately public test key, never a user credential; provenance is in crates/argentum-providers/tests/fixtures/chatgpt/README.md |

Third-party notices are not rewritten as project-owned licenses. Download/install
consent is not authority to redistribute optional models/runtimes. Before another
platform package is distributed, include its actual component notices and review
its applicable upstream terms. Historical Node/Tauri dependency terms remain in
their versioned history, not in the active native CLI dependency graph.
