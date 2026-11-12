# Releasing Argentum

This is how a release is checked, built and published. Each check below names the
command or CI job that proves it. A check passes only with current evidence for
the exact commit being released: a run link, a command result, or a file hash.

Releases have no paid code-signing certificate and no Google Play listing. The
Windows files are unsigned. The Android APK is release-signed in CI with the
project key. Local llama.cpp is an optional add-on and is not in the installer,
the zips or the APK.

## Before tagging

1. Set `version` in `[workspace.package]` of `Cargo.toml` to the release version, and
   `versionName` in `platforms/android/app/build.gradle.kts` to match. The
   release workflow fails on a tag when they differ. It does not check
   `versionCode`, so raise that by hand for each release.
2. In the repository secrets, set `ANDROID_KEYSTORE_BASE64`,
   `ANDROID_KEYSTORE_PASSWORD`, `ANDROID_KEY_ALIAS` and `ANDROID_KEY_PASSWORD`.
   The `android` job fails without them.
3. Run the `Release` workflow by manual dispatch on the release commit, with
   `draft_release` off. It builds everything and publishes nothing. Use it as
   the dry run for the checks below.

## Checks

| Check | Proof | Where |
| --- | --- | --- |
| Locked fresh build | `cargo build --locked --release` on a clean runner | `Release` workflow, jobs `windows` and `android` |
| Complete applicable offline tests | `scripts/test-offline.ps1` | `Native CLI qualification`, job `windows`, step "Credential-free tests" |
| Store tests on Linux | `cargo test --locked -p argentum-store --lib` | `Android and Linux`, job `linux-store` |
| Format and strict lint | `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets --locked -- -D warnings` | `Native CLI qualification`, job `windows`; Android lint in `Android and Linux`, job `android-lint` |
| Source policy, license and advisory review | `cargo deny check` with `deny.toml` | `Native CLI qualification`, job `windows`, step "Advisory, source and license policy" |
| Source review | Merged pull requests to `development`; a security review of security-sensitive changes per `SECURITY.md` | Pull request history |
| Static analysis | CodeQL | `CodeQL` workflow on the release commit |
| Safe CI | Workflows pin actions by commit, use read-only tokens by default, and only `draft-release` gets `contents: write` | Run `actionlint .github/workflows/*.yml` locally (CI does not run it); review `.github/workflows/release.yml` |
| Release files | `Argentum-<version>-windows-x64.msi`, `Argentum-<version>-windows-x64.zip`, `Argentum.exe` (used by the in-app updater), `Argentum-<version>-android-arm64.apk`, `argentum-genesis-<version>-windows-x64.zip`, `argentum-genesis-desktop-control-<version>-windows-x64.zip`, `argentum-<version>-sbom.spdx.json`, `SHA256SUMS.txt`, `provenance.intoto.jsonl` | `Release` workflow, artifact `argentum-<version>-release` |
| Checksums | `SHA256SUMS.txt` covers the release files except itself and the provenance bundle; check with `sha256sum -c SHA256SUMS.txt` | `Release` workflow, job `release-files` |
| Provenance | Signed build provenance, saved as `provenance.intoto.jsonl`; check each file with `gh attestation verify <file> --repo AG064/argentum` | `Release` workflow, job `release-files` |
| SBOM | `argentum-<version>-sbom.spdx.json` (SPDX, from the source tree) | `Release` workflow, job `release-files` |
| Installer | Per-user install, Start menu entries, CLI on PATH, clean uninstall, user data kept, opt-in `REMOVEUSERDATA=1` | `Release` workflow, job `windows`, step "Install and uninstall the installer" (`scripts/test-windows-installer.ps1 -TestDataRemoval`); repeat locally with `scripts/test-windows-installer.ps1 -Installer <msi> -LogDirectory <folder>` without `-TestDataRemoval` |
| No llama.cpp in release files | Build and test scripts fail on llama.cpp or model files | `scripts/build-windows-installer.ps1`, `Release` job `android` step "Check the signed APK" |
| Isolated first-run and restart | Fresh profile first run, then restart, from the installed app and from the extracted zip | Manual, on a Windows PC; record the result in the release notes |
| Outside-checkout launch with spaces and non-ASCII paths | Extract the zip to a folder such as `D:\Release Test Ä\Argentum` and start `Argentum.exe` and `argentum.exe --version` | Manual, on a Windows PC |
| Android release build and signing | `./gradlew :app:assembleRelease` builds the native libraries with cargo-ndk, then CI signs the APK. `apksigner verify --print-certs`, only `arm64-v8a` libraries | `Release` workflow, job `android`, step "Check the signed APK" |
| Android device qualification | Install the signed APK on an arm64 device, first run, restart | Manual, on a device; record the device and Android version |
| Slint visual acceptance | Owner review of the desktop and Android UI on real screens | Manual, owner |
| Code signing | Windows files are not code signed | Release notes state it |

## Tag and draft

1. After every check above has current evidence, the maintainer creates and pushes
   the tag, for example `git tag -a v0.1.0 <commit> -m "Argentum 0.1.0"` then
   `git push origin v0.1.0`.
2. The tag push runs the `Release` workflow again and its `draft-release` job
   creates a draft release with all files attached. A draft is not public.
3. Download the draft files and run `sha256sum -c SHA256SUMS.txt` and
   `gh attestation verify` on at least the installer, the zip and the APK.
4. Replace the draft notes with the release notes.
5. Publishing the draft is a manual maintainer step on the GitHub release page.
   No workflow publishes it.

## Merging pull requests

Merge with the maintainer's public address as the commit author, so no private
address enters the history:

```powershell
gh pr merge <number> --squash --author-email contact@ag064.eu
```
