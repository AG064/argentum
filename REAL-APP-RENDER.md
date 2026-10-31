# Real App Render Evidence

Worktree: `A:\CodexData\temp\argentum-ui-v2-20261031`
Branch: `feature/ui-v2`
Base: `fff3dfd1`

## Data directory override

`ARGENTUM_DATA_DIR` is now read by `AppPaths::discover()`:

- Set to an absolute path: that directory supplies `argentum.db`, `logs`,
  `workspace.json`, local runtime state and model paths.
- Set to a relative path: startup fails with
  `PlatformError::DataDirectoryOverrideNotAbsolute` and the desktop shows:
  `ARGENTUM_DATA_DIR must be an absolute path. Set it to a disposable absolute directory or unset it, then restart Argentum.`
- Unset: the existing `ProjectDirs` behavior is unchanged.

The helper is `AppPaths::discover_with_override`, with the environment read in
`discover()`. The focused tests cover all three branches.

### Test result

Command:

```powershell
cargo test -p argentum-platform --lib
```

Result: `62 passed; 0 failed`.

New tests:

- `tests::app_paths_discover_uses_an_absolute_data_dir_override`: PASS
- `tests::app_paths_discover_rejects_a_relative_data_dir_override`: PASS
- `tests::app_paths_discover_without_override_keeps_project_dirs`: PASS

Documentation was added to `UI-V2.md` and `platforms/README.md`.

## Isolated real-app run

The real `argentum-app` binary was launched against disposable absolute paths.
`LOCALAPPDATA`, `TEMP` and `TMP` were also redirected so the legacy pointer and
temporary files did not touch the owner's profile.

```powershell
$root = 'A:\CodexData\temp\argentum-ui-v2-20261031\run\real-app-software-800'
$env:ARGENTUM_DATA_DIR = "$root\data"
$env:ARGENTUM_WORKSPACE = "$root\workspace"
$env:LOCALAPPDATA = "$root\localappdata"
$env:TEMP = "$root\temp"
$env:TMP = "$root\temp"
$env:ARGENTUM_UI_SIZE = '1280x800'
$env:SLINT_BACKEND = 'software' # diagnostic only
& 'A:\CodexData\temp\argentum-ui-v2-20261031\target\01a08f3b-910f-7cb3-8e40-12f6128b3f32\debug\argentum.exe'
```

Observed isolation:

- `run\real-app-software-1280\data\argentum.db`
- `run\real-app-software-1280\data\argentum.db-shm`
- `run\real-app-software-1280\data\argentum.db-wal`
- No owner application-data path was used for the database.
- The disposable run directories were removed after the evidence capture.

No provider request was made. No message was seeded. No credential was
inspected, exported, seeded or used by this exercise. The app's normal startup
path was otherwise unchanged.

## Backend result

| Backend | PID | Window | Client area | Result | Evidence |
| --- | --- | --- | --- | --- | --- |
| `femtovg` | 30248 | `Argentum`, 1296x839 | 1280x800 | Blank white client area | `evidence/real-app-empty-femtovg.png` |
| `winit-femtovg` | 38300 | `Argentum`, 1296x839 | 1280x800 | Blank white client area | Same renderer path as `femtovg` |
| `software` | 31716 | `Argentum`, 1296x839 | 1280x800 | Rendered empty state | `evidence/real-app-empty-software.png` |

The blank femtovg window had a valid title, chrome, process and event loop. It
was responsive and wrote no startup error to stderr. The software renderer
rendered the same source with the same data paths.

Diagnosis: this desktop session has a Meta Virtual Monitor and a Virtual Desktop
Monitor in an error state alongside the NVIDIA adapter. The preview shell also
rendered with the default renderer earlier in the session, then new OpenGL
windows went white after repeated window creation. The new UI tree is not the
differentiator: the same tree renders under software. The blank is specific to
the session's OpenGL/femtovg path.

No production backend change was made. On Windows the app still selects
`femtovg` when `SLINT_BACKEND` is unset. Software is recorded only as the
confirmed working diagnostic backend for visual QA in this session.

## Live reload in the real app

The app-level feature forwarding works.

- Build: `SLINT_LIVE_PREVIEW=1 cargo build -p argentum-app --locked --offline -j 1 --features live-preview`
- Run: `SLINT_LIVE_PREVIEW=1`, `SLINT_BACKEND=software`, `ARGENTUM_UI_SIZE=1280x800`
- Process ID: `31716`
- Process start: `2026-10-31T21:19:40.0286438+02:00`
- Edit: `ui/tokens.slint`, `accent: #D8FF57` to `#FFCE6A`
- Reload: `Reloaded component MainWindow from ...\ui\app.slint`
- Latency: `1177 ms`
- Process ID after reload: `31716` (unchanged and alive)
- Restore latency: `400 ms`
- Evidence:
  - `evidence/real-app-live-reload-before.png`
  - `evidence/real-app-live-reload-after.png` (gold accent in the same process)
  - `evidence/real-app-live-reload-restored.png`
  - `evidence/real-app-live-reload.log`

## Window size note

The primary capture uses a 1280x800 client area so the full composer remains in
the visible window on this desktop. A 1280x900 run clipped the final composer
row in this desktop window presentation. That is a window-size/display-path
issue to recheck on a normal desktop, separate from the renderer result.

## Verification

| Command | Result |
| --- | --- |
| `cargo fmt --all -- --check` | PASS |
| `cargo test -p argentum-platform --lib` | PASS, 62 tests |
| `cargo build -p argentum-app --locked --offline -j 1` | PASS, 46.04 s |
| `cargo clippy -p argentum-platform -p argentum-app --all-targets --locked --offline -j 1 -- -D warnings` | PASS, 1 m 06 s |
| App live-preview feature build | PASS, 4 m 06 s |

## Remaining limitations

- Default-renderer visual acceptance remains open in this desktop session. The
  OpenGL/femtovg path is blank; software renders correctly.
- Only the empty state was reachable without a provider. Populated visuals remain
  covered by the store-free preview mock.
- Strict credential-store isolation is not part of `ARGENTUM_DATA_DIR`. The app's
  normal startup path still constructs the OS secret store. A separate opt-in
  development override would be required for strict credential isolation, and
  was not added because this slice must not change credential behavior.
- Re-check the 900 px height presentation on a normal display.
