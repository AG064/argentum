# Android and remote sessions plan

Goal for the 0.1.0 public release: Argentum works on Windows and Android, each on
its own, and one install can drive the other.

## Principles

- One product. Android runs the same runtime, store, harness and Slint UI as the
  desktop, through `argentum-android` and `run_native_host`. Platform code stays in
  `argentum-android`, `argentum-platform` and `platforms/android`.
- Plain chat first. A new install on either platform needs only a provider and a
  message. Everything else is an optional module, off until the owner adds it.
- Everything the UI can change, a command can change too. The same command set
  runs from the desktop CLI, from the in-app command palette on either platform,
  and from a paired device.
- The persisted workspace policy on each device is the authority for that
  device. A paired device can ask, and never grant itself anything.

## Phase 1: Android app for regular chat

- The debug APK builds from a clean checkout (`assembleDebug`) for arm64-v8a and
  x86_64.
- First run on a phone: pick a provider, save the key in Android Keystore, chat.
- The narrow layout: the sessions drawer, the full-width panel, the composer
  above the keyboard, safe-area insets, and back-button behavior.
- Lifecycle: rotate, background, return, and process death keep the session.
- Checked on an emulator sized like the Xiaomi Mi 13T Pro (1220x2712, 440 dpi),
  then by the owner on the phone.

## Phase 2: commands everywhere

Started: the palette runs `new`, `settings <page>`, `provider use <id>`,
`provider context <tokens>`, `module on|off <id>` and `theme <name>`.

- The command palette accepts the CLI command set (`provider`, `module`, `session`,
  `permissions show`, `usage` and so on) and shows the same output.
- Commands that need an owner console, such as `permissions select`, stay
  console-only.

## Phase 3: pairing and remote sessions

Started: `crates/argentum-devices` gives each install an Ed25519 identity in the
credential store, with a fingerprint to compare and signing for the pairing
challenge. `argentum device show` prints the fingerprint. The pairing exchange is
in `argentum_devices::pairing`: both devices sign a transcript of both keys and
nonces, verify each other, and show the same six-digit code for the owner to
confirm. Paired devices are kept in the store's `paired_devices` table, and
`argentum device list` and `argentum device forget` show and remove them.
`argentum_devices::exchange` runs the exchange over any stream as bounded JSON
lines, tested over a loopback TCP connection. `argentum device pair --listen`
waits up to two minutes for one device, `--connect HOST:PORT` connects to it,
and both ask the owner to type y when the codes match. The in-app pairing
screen and remote sessions are not built yet.

- Each install has a device identity: an Ed25519 key in the Windows credential
  store or Android Keystore.
- Pairing on the same network: one device shows a short code and a QR code, the
  other enters or scans it. Both confirm the other's fingerprint. Paired devices
  are stored in each device's policy and can be removed at any time.
- Transport: the existing JSONL command protocol (`serve_jsonl`) over an
  authenticated, encrypted connection (Noise XX over TCP on the local network).
  Connecting from outside the local network is left to a VPN the owner runs,
  such as WireGuard or Tailscale, in 0.1.0.
- A Devices module lists paired devices and their state. A session can run on
  this device or on a paired one. The UI then streams that device's events and
  sends its commands, and approvals show on the device that owns the action.

## Phase 4: one agent, both environments

- When devices are paired and online, a run gets a short "Environments" section:
  each device's name, platform, workspace and available tools.
- Remote tools are namespaced by device (`pc.read_text`, `phone.read_text`). Each
  call runs under the target device's own policy and approvals.
- Content from a remote device is untrusted input, like any tool result.

## Out of scope for 0.1.0

- A hosted relay service.
- Controlling Android system settings or other apps. That needs Accessibility or
  Shizuku, which `docs/ANDROID_HOST.md` rules out as implicit fallbacks.

## Release gates

The Android rows in `ROADMAP.md` still apply: package metadata, install, restart,
lifecycle, physical ARM64 checks, size limits, licenses and artifact identity.
Signing needs a separate owner decision.
