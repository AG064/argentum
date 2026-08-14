# Project and Installed Size Report

Measured on Windows x64 on 2026-07-17 from the v0.0.9 development workspace.
Run `npm run size:report` to reproduce the workspace/component measurement.

## Current workspace

| Category                                                                              |                         Size |
| ------------------------------------------------------------------------------------- | ---------------------------: |
| Source/config/assets excluding common output, caches, dependencies, Git, runtime data |                   274.69 MiB |
| Prepared optional Windows x64 llama.cpp payload copied by the NSIS hook               |                    44.11 MiB |
| npm `node_modules`                                                                    |                   365.62 MiB |
| Rust `src/desktop/target` build output                                                |                 8,833.36 MiB |
| Local build/package caches in the workspace                                           |                   409.75 MiB |
| Retained release artifacts                                                            |                   170.77 MiB |
| Git metadata                                                                          |                   131.26 MiB |
| Compiled Node `dist`                                                                  |                     3.21 MiB |
| Android local Gradle metadata/build output                                            |                     1.08 MiB |
| Runtime data/backups                                                                  |                     0.09 MiB |
| **Entire current workspace**                                                          | **10,189.85 MiB (9.95 GiB)** |

The Rust target dominates developer disk usage and is safe to regenerate. This
report does not include globally installed Node/Rust/Android toolchains, Cargo's
global registry/cache, Android SDKs, JDK, system WebView, or OS package caches.

## Windows release and clean install

| Item                                                     |                     Size |
| -------------------------------------------------------- | -----------------------: |
| NSIS setup                                               |                98.58 MiB |
| MSI installer                                            |                95.84 MiB |
| Desktop executable                                       |                42.65 MiB |
| CLI sidecar bundled with desktop                         |                89.86 MiB |
| Component-sum core payload                               |               132.50 MiB |
| **Measured clean MSI administrative install**            | **132.83 MiB (3 files)** |
| Estimated NSIS installed payload with optional llama.cpp |               176.94 MiB |

The clean MSI measurement extracted the built MSI into an empty directory and
counted the two installed executables plus MSI administrative metadata. The
optional llama.cpp estimate adds the 44.11 MiB Windows x64 directory copied by
the current NSIS hook to that clean measurement. Duplicate preparation paths
and Linux binaries are not added to the Windows installed estimate. The NSIS
optional install path should be measured again from the final tagged artifact
because hook contents can change.

Downloaded GGUF models, provider caches, chats, workspaces, logs, and user data
are excluded from installed size. Model weights can range from hundreds of MiB
to many GiB and must always be reported separately from the app.

Linux, macOS, and Android installed sizes are not reported because final v0.0.9
artifacts were not built on those platforms in this workspace. Release CI should
publish per-artifact compressed size and a clean-install measurement before the
release is called cross-platform ready.
