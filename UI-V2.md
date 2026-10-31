# UI V2 - Bold Showcase, Slice 1

Status: local implementation on `feature/ui-v2`. Not pushed or published.
Base commit: `34551b518bd319f09b0a9bb579faf4f17106b8a9`.
Worktree: `A:\CodexData\temp\argentum-ui-v2-20261031`.

## Direction

The first slice is a product showcase, not a dense control panel.

- Canvas: near-black `#0B0B0E`, with a slightly lighter navigation rail and inspector.
- Type: warm bone `#F2EBDD` on black, large 54 px display headings, 9-13 px micro labels with measured letter spacing.
- Signal: one chartreuse accent `#D8FF57` for primary action, active navigation, and live state.
- Approval uses amber `#FFB86B`; failures use coral `#FF8577`. Red is not the primary identity.
- Structure is held by hairlines and editorial spacing. No glassmorphism, no rounded dashboard cards, no external fonts, no network assets.
- Existing native SVG icons under `ui/assets/icons` are reused through a new icon wrapper.

## Shell

The new body replaces the old visual tree. Only `view-models.slint` and the public `MainWindow` declarations are carried forward.

- Left navigation rail:
  - custom Argentum mark and wordmark;
  - workspace block bound to `project_name` and `workspace_label`;
  - new-session action;
  - session list bound to `session_items`;
  - provider state, settings action, and a compact build label.
- Center conversation:
  - session header and run-state plate;
  - empty hero with the display statement "Build what / holds up.", project context, and three prompt actions that fill `draft`;
  - real message list bound to `conversation_messages`, including role, state, reasoning, provider, model, and token metadata;
  - streaming transcript and current reasoning blocks for active runs;
  - bottom composer bound to `draft`, `running`, `submit_task`, and `cancel_run`.
- Right inspector:
  - fixed tabs for Changes, Tools, Verification, and Activity;
  - Changes binds `change_files`, `selected_diff`, verification, recovery, and restore actions;
  - Tools binds `activity_items` and `activity_summary`;
  - Verification binds `verification_state`, `verification_summary`, `plan_steps`, and `plan_summary`;
  - Activity binds the same activity model in a timeline treatment;
  - `active_inspector` drives the selected tab when the bridge changes it.
- Approval:
  - a dedicated approval block above the composer, bound to `approval_open`, `approval_action`, `approval_target`, `approval_reason`, and `approval_response_pending`;
  - the conversation viewport reserves space for the approval or error block, so the block does not cover message text;
  - Approve and Reject call the existing bridge callbacks.
- Motion:
  - status dot pulse for running and approval states;
  - short background and border transitions;
  - every animation and the pulse timer are disabled when `reduced_motion` is true.

## Live preview

### Plumbing

`crates/argentum-ui/Cargo.toml`:

```toml
[features]
live-preview = ["slint/live-preview"]
```

`crates/argentum-app/Cargo.toml`:

```toml
[features]
live-preview = ["argentum-ui/live-preview"]
```

`crates/argentum-ui/src/bin/ui-preview.rs` creates the generated `MainWindow`, applies local showcase models, and runs it. It does not open a workspace database, provider store, credential store, or network connection.

Modes:

```powershell
$env:SLINT_LIVE_PREVIEW='1'
cargo run -p argentum-ui --bin ui-preview --features live-preview -- empty
cargo run -p argentum-ui --bin ui-preview --features live-preview -- conversation
```

The additional preview modes `tools`, `verification`, `activity`, and `approval` select the inspector view or approval state for visual review.

### Proof before the redesign

The old visual tree was still in place. With the preview process running:

- Process ID: `19904`.
- Edit: `ui/tokens.slint`, `canvas: #090A0C` changed to `#2E1A0E`.
- Reload: `Reloaded component MainWindow from ...\ui\app.slint`.
- Measured latency: `2971 ms`.
- Process ID after reload: `19904` (unchanged).
- The token was restored and the restored window was captured.
- Evidence:
  - `evidence/live-reload-old-before.png`
  - `evidence/live-reload-old-after.png`
  - `evidence/live-reload-old-restored.png`
  - `evidence/live-reload-old.log`

### Proof on the new shell

With the new showcase shell running:

- Process ID: `9372`.
- Process start: `2026-10-31T20:18:24.7562139+02:00`.
- Edit: `ui/tokens.slint`, `accent: #D8FF57` changed to `#FFCE6A`.
- Reload: `Reloaded component MainWindow from ...\ui\app.slint`.
- Measured latency to changed pixels: `1751 ms`.
- Process ID after reload: `9372` (unchanged and alive).
- The accent was restored in `558 ms`.
- Evidence:
  - `evidence/ui-v2-live-reload-before.png`
  - `evidence/ui-v2-live-reload-after.png` (gold accent visible in the same window)
  - `evidence/ui-v2-live-reload-restored.png`
  - `evidence/live-reload-new.log`

### Renderer note

The default OpenGL path returned a blank client area in this desktop session after repeated preview window creation. The window chrome and process were healthy, but the client did not present. Final captures and the new-shell reload proof used Slint's software renderer:

```powershell
$env:SLINT_BACKEND='software'
```

The UI source, live-preview feature, and reload plumbing are renderer independent. The earlier reload proof on the old shell used the default renderer and rendered correctly, which is why both proofs are kept.

## Real-app isolated run

The real desktop host can be run with disposable state without touching the
owner's application-data directory. `ARGENTUM_DATA_DIR` accepts an absolute
path and relocates `argentum.db`, `logs`, `workspace.json`, local runtime state
and model paths for that process. A relative path is rejected with a clear
startup error. Leaving the variable unset keeps the existing ProjectDirs
behavior.

```powershell
$root = 'C:\temp\argentum-desktop-run'
$env:ARGENTUM_DATA_DIR = Join-Path $root 'data'
$env:ARGENTUM_WORKSPACE = Join-Path $root 'workspace'
cargo run -p argentum-app --locked
```

The desktop startup notes in `platforms/README.md` document the same contract.
The real-app render evidence is recorded in `REAL-APP-RENDER.md` and
`evidence/real-app-render.json`. In this desktop session the real host rendered
the empty state under `SLINT_BACKEND=software`; the default OpenGL/femtovg path
was blank while the process and window remained healthy. The production backend
was not changed. Real-app live reload also worked without restarting the process.

## Interface preservation

The public `MainWindow` interface was compared with the base commit before the visual rewrite.

- `191` `in-out` property and callback declarations in the new `MainWindow`.
- `191` in the base `MainWindow`.
- Exact declaration diff: `0`.
- `ui/view-models.slint` is unchanged.
- `cargo build -p argentum-app --locked --offline -j 1` passes, proving the real bridge still compiles against the generated window.
- Remaining interface properties that the first slice does not render are still declared and available to the bridge.

## Verification

Commands are run from the v2 worktree with the disposable build lane target directory.

| Command | Result |
| --- | --- |
| `cargo fmt --all -- --check` | PASS |
| `cargo build -p argentum-ui --bin ui-preview --locked --offline -j 1` | PASS, 57.35s on the final source |
| `cargo build -p argentum-app --locked --offline -j 1` | PASS, 1m 37s on the final source |
| `cargo build -p argentum-ui --bin ui-preview --locked --offline -j 1 --features live-preview` | PASS, 18.61s on the final source |
| `cargo check -p argentum-ui --bin ui-preview` | PASS |
| Interface declaration diff | PASS, 0 differences |
| Live reload, old shell | PASS, same PID, 2971 ms |
| Live reload, new shell | PASS, same PID, 1751 ms |

Full gate output is recorded in `evidence/final-build.log`. The shared `Local\ArgentumBuildLane` mutex was held by an unrelated release build during part of the session; the final gate sequence ran after the lane became available again.

## Screenshots

All paths are relative to the repository root.

- `evidence/ui-v2-empty.png`: bold empty state, navigation, inspector, composer.
- `evidence/ui-v2-conversation.png`: populated conversation, reasoning, file list, diff.
- `evidence/ui-v2-inspector-tools.png`: Tools tab.
- `evidence/ui-v2-inspector-verification.png`: Verification tab with passed state and plan stages.
- `evidence/ui-v2-inspector-activity.png`: Activity timeline.
- `evidence/ui-v2-approval.png`: amber approval block with reserved conversation space.
- `evidence/ui-v2-live-reload-before.png`, `evidence/ui-v2-live-reload-after.png`, `evidence/ui-v2-live-reload-restored.png`: new-shell reload proof.

## Remaining work

The first slice deliberately does not implement the full product surface.

- Settings and About windows, command palette, model menu, and context menu.
- Goal controls and goal budget editing.
- Full Files, Terminal, Preview, and Approvals inspector surfaces.
- Trajectory as a separate durable-record view; the tab currently maps activity-facing data.
- Responsive and mobile layouts. The new shell has a 1040 px minimum width and a single inspector breakpoint at 1200 px.
- Keyboard focus traversal audit and screen-reader pass on the new custom controls.
- Re-run visual QA with the default renderer on a desktop where the OpenGL path presents normally.
- Wire the remaining interface callbacks when their owning screens are designed.
- Add UI tests or screenshot regression checks for the new shell.

## Boundaries

- No changes were made to `A:\ag064\argentum` or any other worktree.
- No files from the rejected UI iterations, backup patches, or old UI branch were used.
- No owner data, credentials, provider keys, or network services were opened.
- The preview binary is development-only and contains local showcase data.
- The commit is local on `feature/ui-v2`; it is not pushed.
