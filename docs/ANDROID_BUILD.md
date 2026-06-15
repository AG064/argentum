# Android Build Guide

The Argentum Android app is a native Kotlin + Jetpack Compose client that ships
alongside the desktop app. It shares the same Argentum theme (silver + crimson,
liquid-glass surfaces) and the same provider stack (MiniMax, OpenAI, local
llama.cpp), with an onboarding flow that walks a new user through the first-time
setup.

This guide mirrors `docs/RELEASE_PACKAGING.md` (desktop) and covers prerequisites,
local development, the release pipeline, signing, and Play Store notes.

## Overview

- **Language / framework**: Kotlin 1.9.x, Jetpack Compose (Material 3)
- **Min SDK / Target SDK**: 26 (Android 8.0) / 34 (Android 14)
- **Build system**: Gradle 8.5, Android Gradle Plugin 8.2.x
- **Module**: `android/`
- **Application ID**: `com.argentum`
- **Versioning**: mirrors the Argentum release version (`versionName` = `0.0.8`,
  `versionCode` = `8` for v0.0.8)

The Android app is intentionally thin. The heavy lifting (LLM calls, tool calling,
session management, audit log) lives in the Argentum gateway. The Android app is
a polished, secure, mobile-first shell.

## Layout

```
android/
├── app/
│   ├── build.gradle.kts            # AGP config, signing, dependencies
│   ├── proguard-rules.pro          # shrinker rules for release
│   └── src/
│       ├── main/
│       │   ├── AndroidManifest.xml
│       │   ├── java/com/argentum/
│       │   │   ├── ArgentumApplication.kt
│       │   │   ├── MainActivity.kt
│       │   │   ├── data/
│       │   │   │   ├── api/MiniMaxApi.kt
│       │   │   │   ├── model/Result.kt
│       │   │   │   └── repository/{Agents,Chat,Settings}Repository.kt
│       │   │   ├── ui/
│       │   │   │   ├── components/{AnimatedButton,GlassComponents}.kt
│       │   │   │   ├── screens/{Agents,Chat,Onboarding,Settings}Screen.kt
│       │   │   │   └── theme/{Color,Theme,Type}.kt
│       │   │   └── viewmodel/{Agents,Chat,Onboarding,Settings}ViewModel.kt
│       │   └── res/                # icons, strings, colors, themes
│       └── test/                   # JVM unit tests
├── build.gradle.kts
├── settings.gradle.kts
├── gradle.properties
├── gradle/wrapper/                 # gradle-wrapper.jar + properties
└── gradlew
```

## Prerequisites

- **JDK 17** (Temurin recommended). `JAVA_HOME` must point at a JDK 17 install.
- **Android SDK** with:
  - Platform 34 (`platforms;android-34`)
  - Build tools 34.0.0
  - Command-line tools (`cmdline-tools;latest`)
- The Android SDK can be installed via:
  - Android Studio's SDK manager (easiest for local dev), or
  - `sdkmanager` from the command-line tools, or
  - `android-actions/setup-android@v3` (used by CI)
- Disk space: ~3 GB for SDK + Gradle caches on first build.

Set `ANDROID_HOME` (or `ANDROID_SDK_ROOT`) to your SDK location and add
`$ANDROID_HOME/platform-tools` to your `PATH`.

## Local development

```bash
# from the repo root
cd android
./gradlew --no-daemon assembleDebug
# APK lands at: android/app/build/outputs/apk/debug/app-debug.apk
```

To install on a connected device or running emulator:

```bash
adb install -r android/app/build/outputs/apk/debug/app-debug.apk
```

To run the JVM unit tests:

```bash
cd android && ./gradlew --no-daemon test
```

### Hot reload / live development

`./gradlew --no-daemon installDebug` is the most reliable local loop. Compose
preview support and live edit work inside Android Studio, but aren't required.

## Release builds

The release pipeline lives in `.github/workflows/release.yml` (`android-apk` job).
On every `v*` tag push it:

1. Sets up JDK 17 and the Android SDK on Ubuntu
2. Generates a CI-managed keystore (see [Signing](#signing) below)
3. Runs `./gradlew assembleRelease`
4. Verifies the APK signature with `apksigner verify`
5. Renames the APK to `argentum-v{version}-android.apk`
6. Writes a SHA256 checksum
7. Uploads the APK to the GitHub Release

The debug CI workflow (`.github/workflows/android.yml`) runs on every push to
`main` / `develop` and uploads the **debug** APK as a workflow artifact named
`argentum-debug-apk`.

## Signing

Argentum ships with a built-in **CI keystore** that the release workflow
generates deterministically. This is fine for sideloading and personal use,
and ensures every release produces a real, installable APK even when no
keystore secrets are configured.

For Play Store publishing, replace the CI keystore with your own. Add these
repository secrets in **Settings → Secrets and variables → Actions**:

| Secret                      | Description                                             |
| --------------------------- | ------------------------------------------------------- |
| `ANDROID_KEYSTORE_BASE64`   | Base64-encoded contents of your `release.keystore` file |
| `ANDROID_KEYSTORE_PASSWORD` | Password for the keystore                               |
| `ANDROID_KEY_ALIAS`         | Alias of the key inside the keystore                    |
| `ANDROID_KEY_PASSWORD`      | Password for the key                                    |

To produce the base64 string for `ANDROID_KEYSTORE_BASE64`:

```bash
base64 -w 0 release.keystore > release.keystore.b64
# then paste the contents of release.keystore.b64 into the secret
```

When all four secrets are present, the release workflow uses your keystore
instead of the CI-generated one. See `android/app/build.gradle.kts`
(`signingConfigs.release`) for the resolution order.

### Important: keep your keystore safe

If you lose your keystore, **you cannot publish updates to an existing Play
Store listing** — Google will treat the new APK as a different app. Keep
backups of your keystore + passwords in a secure location (1Password, Bitwarden,
encrypted disk, etc.).

## Play Store notes (future)

When you're ready to publish:

1. Create a Google Play Console account and app entry for `com.argentum`
2. Replace the CI keystore with your own (see above)
3. Build an Android App Bundle (`.aab`) instead of a universal APK:
   ```bash
   cd android && ./gradlew bundleRelease
   # output: android/app/build/outputs/bundle/release/app-release.aab
   ```
4. Upload the `.aab` to the Play Console internal testing track first, then
   promote to production

The current `release.yml` builds a universal APK. To produce an AAB, add a
separate workflow that runs `bundleRelease` and uploads the `.aab` instead.

## Troubleshooting

### `SDK location not found`

`local.properties` must point at your SDK. The CI workflow writes this from
`$ANDROID_HOME`. Locally, create `android/local.properties` with:

```
sdk.dir=/path/to/Android/Sdk
```

### `Could not find :app:` or Gradle wrapper issues

Make sure `gradle/wrapper/gradle-wrapper.jar` is present. The repo tracks it, so
a fresh clone should have it.

### `Unsupported class file major version`

You're on the wrong JDK. Install JDK 17 (Temurin recommended) and point
`JAVA_HOME` at it.

### `INSTALL_FAILED_UPDATE_INCOMPATIBLE`

You're trying to install an APK signed with a different key over an existing
install. Uninstall the old version first:

```bash
adb uninstall com.argentum
```

## Where to look in the code

- **Theme**: `android/app/src/main/java/com/argentum/ui/theme/`
  - `Color.kt` — Argentum palette (silver, crimson, dark surfaces, glass tints)
  - `Theme.kt` — Material 3 light/dark schemes
  - `Type.kt` — typography scale
- **Components**: `android/app/src/main/java/com/argentum/ui/components/`
  - `GlassComponents.kt` — `GlassCard`, `GlassSurface`, `GlassButton`
  - `AnimatedButton.kt` — press-scale + haptic feedback
- **Screens**: `android/app/src/main/java/com/argentum/ui/screens/`
  - `OnboardingScreen.kt` — first-run flow (welcome → provider → API key → done)
  - `ChatScreen.kt` — conversation list + composer
  - `SettingsScreen.kt` — theme, accent, version
  - `AgentsScreen.kt` — agent picker
- **ViewModels**: `android/app/src/main/java/com/argentum/viewmodel/`
- **API client**: `android/app/src/main/java/com/argentum/data/api/MiniMaxApi.kt`

## See also

- `docs/RELEASE_PACKAGING.md` — desktop packaging
- `docs/QUICK_START.md` — first-time user walkthrough
- `docs/USER_GUIDE.md` — Argentum usage
- `docs/architecture.md` — Argentum architecture overview
