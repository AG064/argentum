# Developer Guide

## Stack

- TypeScript/Node.js gateway and feature modules
- Jest 30 test runner with ESM support
- ESLint 10 flat configuration and Prettier 3
- Tauri v2 + Rust desktop bridge
- Vanilla JavaScript desktop UI in `src/ui/desktop`
- Kotlin/Jetpack Compose Android client in `android`
- npm for JavaScript dependencies, Cargo for desktop Rust, Gradle for Android

Important boundaries:

- `src/core`: configuration, provider abstraction, logging, plugin lifecycle;
- `src/features`: optional backend modules;
- `src/security`: capability, sandbox, allowlist, and secret utilities;
- `src/ui/desktop`: desktop views/state/controllers;
- `src/desktop/src/lib.rs`: Tauri commands and the desktop trust boundary;
- `src/ui/server`: optional standalone dashboard network surface;
- `tests`: Jest unit/integration/e2e suites.

## Setup and commands

```bash
npm install
npm run dev
npm run build
```

Validation:

```bash
npm run validate:quick   # typecheck, lint, desktop asset/version parity
npm run validate:push    # quick validation plus Jest
npm run format:check
cd src/desktop && cargo test --lib
```

Desktop:

```bash
npm run prepare:llama-server
npm run desktop:dev
npm run desktop:build
```

Android (JDK 17 + SDK 34):

```bash
cd android
./gradlew --no-daemon test assembleDebug
```

See [CONTRIBUTING.md](../CONTRIBUTING.md) for hooks/commits and
[RELEASE_PACKAGING.md](RELEASE_PACKAGING.md) for release artifacts.

## Adding a feature

1. Find the real behavior path and existing module convention.
2. Add a Zod configuration block with `enabled: false`.
3. Implement lifecycle start/stop/health cleanup. Disabled modules must not open
   listeners, start timers, access credentials, or load heavy dependencies.
4. Declare permissions, data/network boundaries, and missing-dependency behavior.
5. Fail closed and return unavailable/error instead of demo success.
6. Add targeted Jest/Rust tests and update feature/security documentation.

The plugin loader owns backend module lifecycle. The desktop section registry
owns desktop modules. A source directory alone does not make a feature supported.

## AI/provider changes

Treat the Rust desktop bridge and backend capability broker as enforcement
points. Webview state, prompts, retrieved text, model tool arguments, and provider
responses are untrusted. Persisted configuration can be narrowed by a request but
must not be widened by it. Check permissions both when exposing a tool and when
executing it.

Provider adapters must handle authentication/permission failures, invalid model,
rate limit, timeout, unavailable service, invalid response, cancellation, and
usage metadata that is absent. Never fabricate quota values or fall back to an
unrelated provider without explicit user configuration.

See [AI_PROVIDERS_AND_EXTENSIONS.md](AI_PROVIDERS_AND_EXTENSIONS.md).

## Desktop asset parity

Tauri packages generated desktop assets. After modifying `src/ui/desktop`, run:

```bash
npm run desktop-assets:sync
npm run desktop-assets:check
```

The pre-push gate checks parity. Do not edit generated copies as the source of
truth.

## Testing

Tests use Jest (`describe`, `it`/`test`, `expect`, and `jest.mock`). Do not use
Vitest APIs unless the test stack is deliberately migrated. Behavior changes
need regression tests at the closest reliable layer. Network tests should mock
remote providers unless the command is explicitly a live/manual test.

Minimum handoff:

- exact commands and results;
- platform/provider paths not tested and why;
- files changed and behavior verified;
- remaining limitations without “production ready” shorthand.

## Release process

Do not use `npm version` until the release commit is otherwise ready; it changes
package metadata and creates a tag. Release preparation requires version parity,
changelog/release notes, full validation, platform artifacts, signing/checksum
verification, and a rollback/recovery plan. Android requires a persistent signing
key; a future in-app desktop updater requires a persistent Tauri signing key.

Tag only the reviewed release commit and let CI build artifacts from that tag.
Never rebuild/sign a release asset from an uncommitted working tree.
