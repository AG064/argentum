# Argentum on Android

Android runs the same runtime, store, harness and interface as the desktop app.
Platform code lives in `crates/argentum-android` (activity, insets, back button,
startup), `crates/argentum-platform` (Android Keystore and app-private storage)
and `platforms/android` (the Gradle project and Java bridge).

## What works

Checked on an Android 16 x86_64 emulator sized like a Xiaomi Mi 13T Pro
(1220x2712, 440 dpi):

- First run: choose a provider, save a key in Android Keystore, or describe an
  OpenAI-compatible server with its address, model and context size.
- Chat with streamed replies, against a local OpenAI-compatible test server.
- The keyboard opens only when you tap a field, and the layout moves above it.
- Rotation keeps the conversation. Landscape uses the wide layout.
- Settings, provider budgets, and the Tested and Testing provider tabs.
- Force-stopping the app and opening it again. Android ends app processes as a
  matter of routine, so startup reconciles the previous run after the database
  lock proves it has ended. A second live copy still fails closed.

Not checked yet:

- A physical arm64 phone. On the x86_64 emulator, the arm64 build runs through
  Android's ARM translation and aborts before Argentum's code starts: inside
  `android-activity`'s startup, where it looks up Android SDK classes with
  `FindClass`. The x86_64 build of the same code runs, and real arm64 phones have
  no translation layer, but this needs a real device to settle.
- On-device models. The llama.cpp engine is an optional build feature (below).

## Build

You need a JDK 17, the Android SDK with NDK 27.0.12077973, `cargo-ndk`, and the
Rust targets `aarch64-linux-android` and `x86_64-linux-android`. Set `ANDROID_HOME`
and keep the Gradle and Android user folders off the system drive if you like
(`GRADLE_USER_HOME`, `ANDROID_USER_HOME`, `ANDROID_AVD_HOME`).

```powershell
platforms\android\gradlew.bat -p platforms\android --no-daemon --dependency-verification=strict assembleDebug
```

This builds both ABIs in debug mode, which is large and slow to start. For a phone,
build only arm64 in release mode and package it:

```powershell
$env:ANDROID_JAR = "$env:ANDROID_HOME\platforms\android-36\android.jar"
cargo ndk -t arm64-v8a -P 28 -o platforms/android/app/build/generated/rustJniLibs/debug build -p argentum-android --lib --release --locked
platforms\android\gradlew.bat -p platforms\android --no-daemon assembleDebug -x buildRustDebug
```

Add `-PkeepNativeSymbols=true` to keep native symbols, so a Rust panic in logcat
has a readable backtrace.

Delete `platforms/android/app/build/outputs/apk/debug/app-debug.apk` first.
Incremental packaging otherwise keeps the old file's dead space, and the APK grows
to several times its real size.

### On-device inference

Off by default. It adds tens of megabytes and needs Vulkan C++ headers
(`VULKAN_INCLUDE_DIR`), which the NDK does not ship. Build it in with
`-PonDeviceInference=true` for Gradle, or `--features on-device-inference` for
`cargo ndk`. Without it, the llama.cpp choice is hidden.

## Try it on a phone

1. Enable USB debugging, then `adb install -r <apk>`, or copy the APK to the phone
   and open it.
2. On first run, pick DeepSeek or OpenCode Go and paste your key, or pick OpenAI
   compatible to use a server on your PC.
3. For a server on your PC, start it on the network, for example
   `llama-server -m model.gguf --host 0.0.0.0 --port 8080 --ctx-size 32768`, then
   enter `http://<pc-address>:8080/v1`, the model name, and the same context size.
   Android allows plain HTTP to it because Argentum makes its own connections.

The debug APK is signed with the local debug key. It is for testing, not for
publishing. Signing a release needs a separate owner decision.

## Release gates

Before an Android release claim, check package, SDK, ABI, permission and exported
component metadata, install, restart, lifecycle and cancellation, a physical arm64
device, size limits, dependency licenses, and exact artifact identity. No emulator
run satisfies all of these.
