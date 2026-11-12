# Argentum on Android

Android runs the same runtime, store, harness and interface as the desktop app.
Platform code lives in `crates/argentum-android` (activity, insets, back button,
startup), `crates/argentum-platform` (Android Keystore and app-private storage)
and `platforms/android` (the Gradle project and Java bridge).

## What works

- First run: choose a provider and save a key in Android Keystore, or describe an
  OpenAI compatible server with its address, model and context size.
- Chat with streamed replies.
- The keyboard opens only when you tap a field, and the layout moves above it.
- Rotation keeps the conversation. Landscape uses the wide layout.
- Settings, provider budgets, and the Tested and Testing provider tabs.
- Force-stopping the app and opening it again. Startup recovers the previous run
  once the database lock shows it has ended. A second running copy is refused.

The app was tested by the maintainer on a Xiaomi 13T Pro with Android 16 and on
an Android 16 emulator. On-device models are an optional build feature (see
below) and are not in the release APK.

## Build

You need a JDK 17, the Android SDK with NDK 27.0.12077973, `cargo-ndk`, and the
Rust targets `aarch64-linux-android` and `x86_64-linux-android`. Set `ANDROID_HOME`.
`GRADLE_USER_HOME`, `ANDROID_USER_HOME` and `ANDROID_AVD_HOME` move the Gradle and
Android user folders if you want them elsewhere.

```powershell
platforms\android\gradlew.bat -p platforms\android --no-daemon --dependency-verification=strict assembleDebug
```

This builds a debug APK with both ABIs, which is large and slow to start. For a
phone, build only arm64 with an optimized native library and package it as a debug
APK:

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

## Commands

The agent can run commands on Android with `execute_argv` and `execute_shell`,
with the same approval as on desktop. Android's app sandbox is the outer
boundary: commands run as the app's own Linux user and SELinux domain. Each
command gets:

- the workspace as its working directory: the app's private folder by default,
  or the folder chosen in Internal storage (see Workspace folder below),
- `/system/bin/sh` as the shell,
- an environment cleared to Android's fixed system `PATH`, a few Android folder
  variables and the allowed overrides,
- the desktop timeout and output limits,
- its own process group, killed on cancel, timeout or finish.

Only Android's built-in tools are available (toybox: `ls`, `cat`, `grep`, `sed`,
`find`, `uname`, `getprop` and similar). Android does not let apps run binaries
from their own storage, so a program saved in the workspace cannot run, though
`sh script.sh` can. A process that deliberately leaves its process group can
outlive the command, but stays inside the app sandbox and ends when Android stops
the app. Git and the verification tools are not offered, because Android has no
Git or Rust toolchain.

The process tests run on a connected device. Build them, push the binary, and run
it from the shell user:

```powershell
cargo ndk -t arm64-v8a -P 28 test -p argentum-execution --lib --locked --no-run
adb push <test binary from the output> /data/local/tmp/argentum-execution-tests
adb shell chmod 755 /data/local/tmp/argentum-execution-tests
adb shell "cd /data/local/tmp && TMPDIR=/data/local/tmp ./argentum-execution-tests process_group_tests"
```

## Workspace folder

Argentum works in its own private folder by default. Other apps cannot see it.

To work in a folder other apps can also open, such as one under Documents:

1. Open Settings > Workspace and tap Turn on in Android settings. Android opens
   the All files access screen for Argentum. Turn it on and come back.
2. Tap Choose folder and pick a folder under Internal storage, or type its path,
   such as `/storage/emulated/0/Documents/my-app`.
3. Tap Open and restart. Argentum shuts down cleanly and opens again in that
   folder.

With All files access on, Android lets the Argentum app read and change every
file in Internal storage. The agent itself stays limited to the chosen folder by
the same broker, approvals and change records as on desktop. The file tools,
commands, Git inspection and change review work there unchanged, since it is an
ordinary path.

Refused: folders on an SD card or USB drive, Downloads and cloud providers in the
picker (they have no real path), the whole of Internal storage, and anything
under `Android/`, where other apps keep their files.

If All files access is turned off later, or the folder is deleted, the next start
opens the app's own folder and says why. Use Argentum's own folder switches back
on purpose. Sessions stay with the folder they belong to.

Commands still run as the app's own user with Android's built-in tools. Shared
storage does not allow running programs from it, so scripts there run as
`sh script.sh`.

Argentum is distributed as an APK from GitHub, not Google Play, which restricts
All files access.

## Session export

Settings > About > Export this session, or Export in a conversation's actions,
writes the session as JSON: messages, reasoning, usage, and every stored event.
Saved provider keys, provider keys set in the environment, and the device
identity key are removed by exact value, along with the values of fields named
like credentials and text shaped like common keys or tokens. If a saved key
would still be in the output, the export stops.

On Android the file goes to the app's cache and the share sheet opens, so you
can save it to Files or attach it to a bug report. Only the latest export is
kept. A private content provider (`ArgentumExportProvider`, not exported)
serves it read only, and only to the app you pick, through the share intent's
one-time read grant. Nothing leaves the phone until you pick a destination.

## Try it on a phone

1. Enable USB debugging, then `adb install -r <apk>`, or copy the APK to the phone
   and open it.
2. On first run, pick DeepSeek or MiniMax and paste your key, or pick OpenAI
   compatible to use a server on your PC.
3. For a server on your PC, start it on the network, for example
   `llama-server -m model.gguf --host 0.0.0.0 --port 8080 --ctx-size 32768`, then
   enter `http://<pc-address>:8080/v1`, the model name, and the same context size.
   Argentum accepts `http://` and `https://` addresses for this server. Use plain
   `http://` only on a network you trust.

Release APKs are signed in CI with the project key. A debug build is signed with
the local debug key and is for testing.
