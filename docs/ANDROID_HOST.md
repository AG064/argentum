# Retained Android native source

Android is a separate source module, not part of the Windows CLI package or its
release qualification. The activity/Keystore/storage/insets bridge and optional
app-private in-process GGUF runtime are retained under crates/argentum-android,
crates/argentum-android-inference and platforms/android. Historical emulator and
engine tests do not establish current physical-device or production-path readiness.

The Gradle project pins its wrapper/distribution and dependency verification
metadata. Building Android additionally requires an appropriate JDK, Android
SDK/NDK, cargo-ndk and the native toolchain. Configure those paths in the build
environment, not personal absolute paths in source. Rust's Android dependency
is target-specific; the Windows CLI does not bundle it or a GGUF.

From a configured clean checkout:

```powershell
platforms\android\gradlew.bat -p platforms\android --no-daemon --dependency-verification=strict assembleDebug
```

This command is not run by RELEASE01 and no APK is published. Ordinary debug
and release native code must exclude the dedicated instrumentation/evidence
hook. The app runs under its ordinary private UID and uses Android Keystore;
Shizuku/root are unavailable, not implicit fallbacks. Optional installation and
activation require distinct owner decisions and exact model identity checks.

Before an Android release claim, qualify package/SDK/ABI/permission/exported
component metadata, actual install/restart/lifecycle/cancellation, native
inference and backend identity where selected, minimum-SDK and physical ARM64
production paths, resource/size limits, dependency/license checks and exact
artifact identity. Signing and device evidence need explicit separate decisions.
No engine-only hook, unsigned build or emulator observation satisfies all gates.
