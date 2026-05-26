# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Fixed
- Removed duplicate `.test.js` files that were causing Jest ESM/CommonJS module mismatch errors
- Updated jest.config.js to only match `.test.ts` files, preventing CommonJS artifacts from being executed

### Changed
- Marked 34 features as TODO/Experimental to reduce default feature count to 0
- Core functionality now limited to essential features: tool calling, image/audio/video processing, web search
- All other features are optional and must be explicitly enabled via configuration

## [0.0.7] - 2026-05-26

### Changed
- **Breaking**: All features now disabled by default (zero features on fresh install)
- Reorganized feature categorization: Production Ready, TODO, Experimental, Deprecated
- No "core" features - everything must be explicitly enabled via config
- Updated FEATURE_STATUS.md with new philosophy and categories

### Added
- Feature maturity requirements table (test coverage, documentation, stability)
- Clear migration path for existing users
- Basic functionality category: tool calling, media processing, communication channels
- Documentation for enabling minimal feature sets

### Fixed
- Removed duplicate `.test.js` files causing Jest ESM/CommonJS module mismatch errors
- Updated jest.config.js to only match `.test.ts` files

### Deprecated
- `skills-loader` marked as deprecated (duplicate of `skill-loader`)

### Security
- Implemented content filtering and sanitization
- Added credential encryption with bcryptjs
- Configured rate limiting with express-rate-limit
- Added SSRF protection in webhook validation
- Implemented container sandbox for command execution

### Developer Experience
- TypeScript strict mode with zero compilation errors
- ESLint v9 flat configuration
- Jest testing framework with ESM support
- Prettier code formatting
- Husky git hooks for pre-commit checks
- Docker Compose configuration for deployment

### Known Issues
- llama.cpp integration incomplete (no server wrapper, no model download)
- LM Studio support requires separate provider implementation
- CLI ↔ Desktop config synchronization not implemented
- 34 features marked as TODO need completion

## [0.0.6] - 2026-MM-DD

### Added
- Initial plugin loader architecture
- Basic feature manifest system
- Configuration-driven feature activation

## [0.0.5] - 2026-MM-DD

### Added
- Core agent framework
- Basic tool calling mechanism
- SQLite memory persistence

## [0.0.4] - 2026-MM-DD

### Added
- Telegram channel integration
- Basic CLI interface

## [0.0.3] - 2026-MM-DD

### Added
- Project initialization
- OpenClaw fork baseline

---

## Version Numbering

- **Major**: Breaking changes to API or architecture
- **Minor**: New features, backward-compatible
- **Patch**: Bug fixes, performance improvements

## Tag Format

Git tags follow the format: `v<major>.<minor>.<patch>` (e.g., `v0.0.7`)

## Release Notes

Detailed release notes for each version can be found in:
- GitHub Releases: https://github.com/AG064/argentum/releases
- Docs: /docs/releases/
