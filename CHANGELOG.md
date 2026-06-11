# Changelog

All notable changes to this project are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.0.8-alpha] - 2026-06-10

### THIS IS A PRE-RELEASE, NOT READY FOR PRODUCTION

> **Note:** This release focuses on build system fixes, internationalization (i18n), and UI/UX improvements. All desktop platforms now build successfully.

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
