# Changelog

All notable changes to this project are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### v0.0.9 release candidate

### Added

- **i18n foundation** — locale and formatting infrastructure
  - `src/i18n/index.js` — locale core: `t()` translation, `setLocale`/`getLocale`, `formatNumber`, `formatDate`, `formatRelativeTime`, `textDirection`
  - `src/i18n/en.json` — English catalog for strings migrated to the translation layer
  - `src/i18n/et.json` — Estonian locale (partial override)
  - Language metadata picker in Settings → Appearance (English, Eesti); full visible-string translation remains planned
  - RTL infrastructure: `dir` attribute on `<html>`, CSS logical properties documented in `styles.css`
  - Locale preference persisted via localStorage UI preferences

- **In-app OpenClaw migration** — allowlisted import directly from the app
  - Automatic detection on first run after workspace selection
  - Settings → Migration section for re-scanning and re-importing anytime
  - Skills merge into the workspace skill directory without overwriting conflicts
  - Memory, persona, workspace files, database, credentials, and configuration are
    copied into explicit OpenClaw archive folders for review; they are not activated automatically
  - Migration item IDs are allowlisted in Rust; interface-supplied filesystem paths are not trusted
  - Hermes → Argentum migration deferred to v0.1.0

  - **CI / Release pipeline** — `attach-installers` step now gracefully skips cleanup
    when the git tag has no matching GitHub release (lightweight tags for CI-only builds)

- **In-app feedback mechanism** — Settings → Help & Feedback: "Report a bug" opens a
  GitHub issue with version and workspace-presence metadata pre-filled, without exposing its path; "Request a feature" opens GitHub
  Discussions with a feature request template; private security issues directed to email

- **Help panel doc links** — the `?` help panel opens tracked documentation on the
  repository's development branch

- **Update mechanism** — `check_for_updates` calls the GitHub Releases API and uses
  numeric semantic-version ordering; the follow-up action opens the releases page

- **Router decision telemetry** — `src/agents/router/` is initialized in the main agent loop;
  top-level YAML and nested JSON configuration are validated and routing decisions are logged.
  Multi-agent dispatch and workspace/session isolation remain planned for v0.1.0

- **Knowledge Graph consolidation groundwork** — `GraphBackend` interface exported as a named export
  from `features/knowledge-graph`; shared type re-export at `src/core/graph-types.ts`;
  approach documented (three implementations aligned with the canonical interface)

- **Security hardening** — `ip-address` override changed to `>= 10.1.1` (patches
  CVE-2026-42338); `sanitize-html` constraint tightened to `>= 2.17.2` (patches
  CVE-2026-40186); SECURITY.md updated with fixed status for both CVEs

- **Tauri version synced** — `tauri.conf.json` version bumped from 0.0.8 to 0.0.9

- **Thinking levels** — Anthropic Claude 4 requests use the documented manual extended-thinking
  shape and token constraints; Codex and OpenAI receive their provider-specific effort fields

- **Built-in skills catalog** — lists the four SKILL.md files actually bundled in this repository:
  browser automation, computer control, skill loader, and YouTube Shorts guidance

- **Model discovery** — Settings can search the Hugging Face model API for GGUF repositories and
  perform a bounded quick scan of `<workspace>/models` for common local-model file extensions;
  only GGUF files are offered to llama.cpp

- **Release validation and size reporting** — quick and pre-push validation commands, a real
  pre-push hook, and a reproducible project/dependency/artifact size report

- **Release planning documentation** — provider and extension compatibility, updater architecture,
  Android signing, optional device sync, scheduled-task boundaries, FAQ, and current feature maturity
  are documented without presenting planned work as implemented

### Changed

- Desktop AI requests now treat persisted context, channel, and security settings as authoritative.
  Request payloads can narrow those permissions but cannot expand them; tool definitions and execution
  enforce the same default-deny policy.
- Message, history, summary, and conservative per-model context limits are enforced before provider
  requests. Desktop `secrets.env` files use owner-only permissions on Unix.
- The managed llama.cpp route no longer exposes a nonexistent Argentum model. It starts from a real
  configurable third-party model identifier or a selected local GGUF file.
- Curated llama.cpp quick-downloads are live-checked GGUF repositories reporting Apache-2.0; broader
  Hugging Face results expose reported license/gating metadata and require model-card review.
- Optional runtime modules, including the standalone dashboard and model routing, are disabled by
  default. The gateway binds to loopback with explicit loopback CORS defaults.
- Android release publishing now requires persistent signing secrets and fails closed when they are
  absent. Per-run signing keys are not generated because they would make upgrades impossible.
- Android version synchronization now updates both `versionName` and monotonic `versionCode`; the
  Windows Gradle wrapper is included, and Android CI runs tests on the actual `development` branch.
- External skill catalogs remain browseable, but installation is disabled until an immutable revision,
  license decision, content digest, and capability review are available.

### Fixed

- Desktop release assets now include generated JavaScript for the skills catalog and localization
  modules, with a drift check that fails when the browser module graph is incomplete
- OpenClaw migration no longer accepts arbitrary source or destination paths and preserves existing data
- Update checks correctly recognize releases such as `0.0.10` as newer than `0.0.9`
- Update apply and rollback no longer fabricate successful installation or backup state; signed in-place
  installation remains unavailable until release signing infrastructure is provisioned
- Loopback model tools no longer follow redirects and cap response time/body size; generated NSIS llama.cpp
  hooks no longer embed a developer-machine path
- Unconnected dashboard endpoints no longer return fabricated budgets, agent counts, trajectories,
  organization data, or successful actions

## [0.0.8] - 2026-06-15

> **Note:** First non-alpha cut of the 0.0.8 line. Adds the Argentum Android app (Kotlin + Jetpack Compose, liquid-glass theme), a real release-APK pipeline, and a polished onboarding flow. Desktop builds remain unchanged from 0.0.8-alpha. All desktop platforms (Windows NSIS/MSI, macOS DMG x64+arm64, Linux AppImage/deb/rpm) and Android APKs are produced by the release workflow on every `v*` tag.

### Added

- **Argentum Android App** — first-party Android client
  - Kotlin + Jetpack Compose, Material 3, `minSdk 26` / `targetSdk 34`
  - Liquid-glass theme (silver/crimson Argentum palette, blur surfaces, animated highlights)
  - Onboarding flow: welcome → provider pick → API key → ready, persisted via DataStore
  - Chat, Settings, and (optional) Agents screens, wired to the desktop API
  - Animated button component with press scale + haptics
  - Unit tests for `ChatViewModel`, `SettingsViewModel`, `AgentsViewModel`
  - See `docs/ANDROID_BUILD.md` for the full build/install/sign story
- **Android Release Pipeline** — `release.yml` now builds and signs a release APK on every `v*` tag
  - Candidate asset: `argentum-{version}-android.apk` (universal APK; publication requires the persistent release signing key)
  - Signing: CI-managed keystore (see Android docs); falls back to the debug key when no keystore is provided so the workflow always produces an installable artifact
- **Desktop Build Docs** — `docs/RELEASE_PACKAGING.md` already covers Windows / macOS / Linux; the Android doc mirrors it
- **Default Provider**: `minimax` (MiniMax-M2.7) is the first onboarding option; `openai` and `local` (llama.cpp) remain stable alternatives

### Changed

- **Version**: `0.0.8-alpha` → `0.0.8` (non-alpha)
- **Android `versionName`/`versionCode`**: now mirrors the Argentum release version (`0.0.8` / `8`) instead of the placeholder `1.0.0` / `1`
- **Tauri Windows WiX version**: `0.0.8.1` → `0.0.8` to match the product version

### Fixed

- **Android CI**: workflow now also produces a release APK (it previously only produced a debug APK as a CI artifact)
- **Android signing config**: release build no longer hard-fails when no keystore is present

### Known Issues

- The release APK is signed with a CI-managed keystore by default. This is fine for sideloading and personal use; replace with your own keystore (see `docs/ANDROID_BUILD.md`) before publishing to the Play Store.
- The Android `Chat` screen talks to the local agent over HTTP on the host machine. Out-of-the-box this only works on emulators and rooted/debuggable devices; see the Android docs for the secure tunneling options.

## [0.0.8-alpha] - 2026-06-10

### THIS WAS A PRE-RELEASE. SUPERSEDED BY 0.0.8.

> **Note:** This pre-release focused on build system fixes, internationalization (i18n), and UI/UX improvements. All desktop platforms built successfully.

### Added

- **i18n Infrastructure**: Partial support for internationalization
- **Russian Language**: Partial Russian language support
- **Estonian Language**: Partial Estonian language support
- **Linux Build**: Working x86_64 Arch Linux build, tested on LainOS
- **MiniMax M3 Support**: New model added
- **MiniMax M2.7-highspeed Support**: New model added
- **Claude Opus 4.8, 4.7**: Partial support added to testing phase
- **Dashboard Server**: Starts together with agent on same port in dev mode
- **Default Port Change**: Server port changed from 18789 to 3000, host 0.0.0.0

### Accessibility

- **Screen Reader Support**: Full ARIA labels, roles, and live regions throughout the UI
- **In-app Update Mechanism**: Updates section in navigation with version check and download
- **Red Update Indicator**: Pulsing red dot on Updates nav button when update available
- **Android Mobile Build**: Target added to CI workflow for APK generation
- **Mobile Responsive UI**: CSS breakpoints for phone screens (768px, 480px)
- **Crypto Algorithm Agility**: AES-256-GCM and ChaCha20-Poly1305 support: AES-256-GCM and ChaCha20-Poly1305 support via ARGENTUM_ENCRYPTION_ALGORITHM

- **Accent Color Picker**: Choose from 8 preset colors or pick custom color in Appearance settings section
- **High Contrast Mode**: Accessibility toggle for better visibility
- **Chat Help Button**: Help button added to chat composer toolbar
- **Keyboard Shortcuts Panel**: Help panel now shows all keyboard shortcuts

- **Keyboard Navigation**: Full keyboard navigation support
  - Skip-to-content link for keyboard users
  - Ctrl+1-9 for quick section navigation
  - Ctrl+, to open Settings
  - ? key to open context-sensitive help
  - Esc to close any open panel (help, notifications, settings, workspace menu)
  - Focus trap for modal dialogs
  - Visible focus indicators on all interactive elements
  - ARIA labels on all icon-only buttons
  - Proper focus management for keyboard users

### Changed

- **Chat Workspace UI**: Updates to remove clutter
- **Header**: Now persistent across the app
- **Security and Permissions View**: UI updated slightly
- **Settings View**: Updated
- **Local Server View**: Updated
- **Log View**: Updated
- **GPT 4.1**: Deprecated and removed (model no longer available)
- **Tauri Workspace Path**: Now uses backend default instead of hardcoded Windows LOCALAPPDATA

### Fixed

- **Windows NSIS/MSI Bundling**: Proper installers now generated
- **macOS DMG Builds**: Both x86_64 and aarch64 (Apple Silicon) now work
- **Linux AppImage and deb Builds**: Fixed
- **Docker Build Retry**: Added retry logic for transient network timeouts
- **Pre-release Version Handling**: sync-version script now properly handles alpha versions
- **Windows Build Runner**: Uses windows-2022 with Visual Studio Build Tools
- **Jest Module Resolution**: Fixed by removing .js extension from ui/server import

### Security

- **Dependency Updates**: ws and better-sqlite3 upgraded for security
- **CodeQL Alerts**: Multiple alerts addressed and suppressed where appropriate
- **Shell Injection Prevention**: Uses execFileSync instead of execSync
- **Token Permissions**: Minimal permissions set in workflows
- **GitHub Actions Pinned**: All actions pinned to commit SHAs

### Documentation

- **OpenSSF Badges**: All baseline level badges added
- **SECURITY.md**: Added with vulnerability reporting and known issues
- **ROADMAP.md**: Expanded with Russian/Estonian language support, accessibility improvements
- **CONTRIBUTING.md**: Added coding standards section
- **GOVERNANCE.md**: Added for Silver badge compliance
- **CODE_OF_CONDUCT.md**: Added
- **CONTINUITY.md**: Added
- **MEMBERS.md**: Added with roles and access inventory
- **SUBPROJECTS.md**: Added policy document
- **SUPPORT.md**: Added

### CI/CD

- **CodeQL Workflow**: Added with default queries
- **Dependency Security Scan**: Added with Trivy SCA
- **SBOM Generation**: CycloneDX JSON for release artifacts
- **Sigstore Cosign Signing**: Added for release artifacts
- **Vulnerability Monitor**: Daily workflow checking npm audit, OSV.dev, GitHub Advisories
- **npm Version Check**: Daily workflow to check for npm updates
- **Coverage Threshold**: Raised to 90% for Silver badge

### Internal

- **Conventional Commits**: Enforced
- **Fast-check Fuzz Tests**: Added for validation and allowlist functions
- **SPDX License**: Added to all source files
- **Copyright Headers**: Added to all source files
- **AGCLAW* to ARGENTUM***: Env prefix renamed for consistency

## [0.0.7] - 2026-05-23

### Added

- **Desktop Polish**: Chat uses cleaner developer-workspace layout with calmer conversation rail
- **Empty State**: Empty chats are easier to recognize
- **Onboarding Controller**: Dedicated controller for onboarding flow
- **Local Llama Server**: First stable Argentum llama.cpp local server path
- **Provider Setup**: Improved provider onboarding
- **Telegram Session Handling**: Better session management
- **App Diagnostics**: Improved visible diagnostics

### Security

- **Security Cleanup**: Security fixes merged into development
- **Shell Execution**: Hardened command execution

### Downloads

- Windows: `Argentum_0.0.7_x64-setup.exe`, `Argentum_0.0.7_x64_en-US.msi`
- Linux: AppImage, deb, rpm
- macOS: DMG for x86_64 and aarch64

## [0.0.6] - 2026-05-17

### Added

- **Desktop Chat and Onboarding**: Polish for v0.0.6
- **AGX Dash UI**: System monitor embedded in desktop
- **Husky + lint-staged**: Pre-commit hook added
- **CLI Smoke Tests**: Added

### Changed

- **Babel to Jest-esbuild**: Performance improvement, replaced babel-jest/ts-jest with jest-esbuild
- **Babel Plugin Istanbul**: Upgraded to v8, test-exclude to v7 for Node 25 compatibility

### Fixed

- **sanitize-html**: Updated
- **rimraf**: Removed via clean script

## [0.0.5] - 2026-05-08

### Added

- **Desktop Gateway**: Complete Argentum desktop MVP gateway
- **Provider OAuth**: OpenAI Codex OAuth provider added
- **OpenAI Model Auth Selector**: Added
- **Desktop Chat Context**: Enriched

### Fixed

- **Provider Onboarding**: Refined
- **Browser Auth Chat**: Routed through codex
- **Desktop Onboarding**: Stabilized

## [0.0.4] - 2026-05-04

### Added

- **Anthropic SDK Security Patch**: Update
- **Desktop Control Center**: Expanded

### Changed

- **Windows Release**: Now GUI-first

## [0.0.3] - 2026-04-29

### Added

- **Desktop and Security Foundation**: Initial desktop build infrastructure
- **CI Upload**: Only desktop installer artifacts

## [0.0.2] - 2026-04-14

Initial Argentum release (based on OpenClaw v0.0.9).

---

## Legacy (OpenClaw)

For earlier OpenClaw changelog, see [OpenClaw releases](https://github.com/openclaw/openclaw/releases).
