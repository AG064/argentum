# Platforms

Argentum keeps platform behavior behind `argentum-platform` and uses Slint's
native backends for the desktop app.

| Platform | Release files | Notes |
| --- | --- | --- |
| Windows x64 | Installer, portable zip, CLI zips | Desktop app and CLI |
| Android arm64 | APK | Same interface at narrow width. See [Argentum on Android](../docs/ANDROID_HOST.md) |
| Linux, macOS | None | The CLI builds from source. The desktop app is not tested there |
| iOS | None | No app host exists |

Support details are in [the support matrix](../docs/SUPPORT_MATRIX.md).

Android has a native activity, safe-area and keyboard handling, Keystore and
workspace bridges, and optional app-private on-device inference. Release APKs are
signed in CI with the project key. The repository holds no signing key.

## Disposable desktop state

The desktop host uses the operating-system application-data directory by
default. For development and screenshots, set `ARGENTUM_DATA_DIR` to an absolute
directory to relocate `argentum.db`, `logs`, `workspace.json`, local runtime
state and model paths for that process. A relative value is rejected. If the
variable is unset, the app uses the default location.

```powershell
$root = 'C:\temp\argentum-desktop-run'
$env:ARGENTUM_DATA_DIR = Join-Path $root 'data'
$env:ARGENTUM_WORKSPACE = Join-Path $root 'workspace'
cargo run -p argentum-app --locked
```

`ARGENTUM_DATA_DIR` relocates application state only. `ARGENTUM_WORKSPACE`
selects the workspace separately. Point both at disposable absolute paths when
testing, so your real data is not touched.

