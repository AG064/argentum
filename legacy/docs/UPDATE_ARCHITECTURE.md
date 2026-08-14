# Signed and Modular Update Architecture

## Current v0.0.9 behavior

Argentum checks the latest GitHub Release with numeric semantic-version
comparison and opens the latest release page. It does not install, replace,
restart, or roll back the application. The optional Node auto-update feature is
disabled and also reports installation/rollback as unavailable.

This is intentional fail-closed behavior until release signing is provisioned.

## v0.1.0 updater gate

Tauri v2 updater signatures are mandatory. A maintainer must generate one stable
Tauri signing identity, store the private key/password only in protected CI
secrets/offline backup, and commit only the public key. Release CI must then:

1. build each platform artifact from the release commit;
2. create updater artifacts and signatures;
3. publish checksums, signatures, and a versioned `latest.json` manifest;
4. verify artifact/version parity before publishing;
5. fail if signing material or an expected platform artifact is missing.

The app must verify signature, platform, architecture, version, and download size
before offering installation. See the
[Tauri updater documentation](https://v2.tauri.app/plugin/updater/).

## Graceful modular update design

AI work and downloads can continue concurrently, but code replacement cannot be
claimed as live before a safe handoff:

```text
check -> stage signed artifact -> verify -> request restart window
      -> persist chats/jobs -> stop accepting side effects -> drain/cancel tools
      -> platform installer/atomic swap -> relaunch -> health check -> commit state
      -> recovery/previous installer if health check fails
```

Requirements:

- app data/workspaces remain outside replaceable install resources;
- update staging uses a versioned temporary directory with bounded size;
- active provider streams may finish or be explicitly cancelled before restart;
- new side-effecting tools are blocked during drain;
- resumable background tasks persist explicit checkpoints, not process memory;
- platform installation uses Tauri/OS-supported semantics rather than overwriting
  an executing binary;
- an installation is successful only after the relaunched version passes health
  checks;
- rollback uses a verified previous artifact/OS installer—not a marker file;
- the updater itself is small, independently testable, and cannot update policy
  or plugins without their own signatures/manifests.

“Same folder” is platform-dependent: an existing installer location should be
preserved where the OS installer supports it, but Windows/macOS package rules and
permissions take precedence. Portable builds need a separate atomic-swap helper.

## Component updates after v0.1.0

Models, skills/plugins, UI assets, sidecars, and the core app require separate
manifests and trust roots. Each component stays optional and independently
rollbackable. An AI task may continue while an unused component downloads, but
loading new executable code waits for the next isolated process/session.

## Test matrix

- upgrade from the two latest supported versions on Windows, Linux, and macOS;
- interrupted/corrupted/oversized/wrong-architecture download;
- invalid/expired/missing signature and manifest downgrade attempt;
- install path with spaces/non-ASCII and insufficient permissions;
- active chat stream, active local llama.cpp process, and pending tool call;
- recovery after crash between staging, install, and first relaunch;
- no loss of workspace, chat, settings, secrets, models, or logs.
