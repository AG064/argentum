# Computer-use foundation

Status: optional foundation behind `desktop-control`. This is not B4
qualification and is not a general computer-use sandbox.

The Windows implementation lives in `crates/argentum-extensions/src/desktop.rs`
and is compiled only with `--features desktop-control` on Windows. The default
build, the `extensions` build and every non-Windows target omit it. The CLI
feature is `argentum-cli/desktop-control`; it enables
`argentum-extensions/desktop-control`, `argentum-tools/desktop-control`,
`argentum-runtime/desktop-control` and `argentum-harness/desktop-control`.

## Tools

All tools use the dedicated `desktop.control` capability and the
`ControlDesktop` policy capability. The capability is registered disabled and
must be enabled explicitly. Every desktop call requires a current owner grant;
the security authorizer never returns `AllowedByPolicy` for `ControlDesktop`,
even under `approval_mode=never` or a full-access profile.

| Tool | Risk | Effect |
| --- | --- | --- |
| `desktop_list_windows` | read-only | Enumerate up to 64 top-level windows with HWND, PID, title, class, screen bounds as reported to the process, visible/responding/minimized/cloaked/elevated flags. |
| `desktop_inspect` | read-only | Bounded UI Automation control-view tree for one observed window: name, control type, automation id, class, bounds, enabled/focusable/focused/offscreen/password flags and safe pattern names. Depth 1-8, at most 256 elements, 64 KiB. |
| `desktop_screenshot` | read-only | PrintWindow capture of one observed visible window into a PNG bounded to 32 KiB after bounded downscaling (source at most 8 MP). Rejected if a password element is visible in the bounded tree. |
| `desktop_invoke` | mutation | Semantic activation through UI Automation `InvokePattern` or `SelectionItemPattern.Select`. No pointer input. |
| `desktop_type_text` | mutation | At most 1024 UTF-16 code units. Default `mode=value` writes through `ValuePattern` with no global input. `mode=keys` sends bounded Unicode key events only when the element already has focus and the target is already foreground. |
| `desktop_press_keys` | mutation | 1-8 bounded chords to the already focused element in the already foreground target. Closed vocabulary: Ctrl/Shift/Alt plus letters, digits, navigation/editing keys and F1-F12. |

`desktop status --json` is the discovery path. It reports whether the binary
was built with the feature, the capability state and the registered desktop
tool descriptors. It does not enumerate windows or send input.

## Security checks

Window identity is the exact observed HWND, PID, title, class and bounds.
Element identity is the bounded tree path, automation id, control type, class,
name and bounds. Every effect repeats the following against the live desktop
immediately before its primitive:

- HWND still exists and still belongs to the recorded PID.
- Title, class and bounds are unchanged.
- The window is visible, not minimized, not cloaked and responds to
  `WM_NULL` within 100 ms.
- The input desktop is the interactive `Default` desktop, not a secure or
  disconnected desktop.
- The target is in the same Windows session.
- The target process is not elevated when this process is not elevated.
- For key input, the target window is already foreground and the element
  already has keyboard focus. The tool never calls `SetForegroundWindow`.
- The element still matches the recorded tree path and properties, belongs to
  the target process, is enabled, is not offscreen and is not a password
  surface.
- The element's native HWND, when present, resolves to the target root.

Additional bounds and behavior:

- Cancellation is observed before every tree node, every text code unit and
  every key chord. No new primitive starts after cancellation.
- Text is bounded to 1024 UTF-16 code units. Control characters other than
  newline and tab are rejected.
- Key chords are bounded to 8. The Windows key, Alt+Tab, Alt+Esc, Alt+F4,
  Ctrl+Esc, Ctrl+Shift+Esc, Ctrl+Alt+Delete and clipboard shortcuts
  (Ctrl+C/X/V, Shift+Delete) are rejected.
- UIA connection and transaction timeouts are set to 2 s and 5 s.
- COM runs on a blocking worker, not the async runtime thread.
- The tool returns bounded JSON. Screenshot bytes are bounded before base64
  expansion so the model-visible result stays under the existing 64 KiB limit.
- A successful COM call is an API return, not proof that the application
  changed state. Reobserve after every effect.

## Deliberately omitted

- **Coordinate mouse clicks and synthetic pointer input.** A coordinate click
  is global input and can hit a different surface if the target moves between
  validation and injection. `desktop_invoke` uses semantic UI Automation
  patterns instead. Pointer input is omitted rather than approximated.
- **Focus stealing.** The tools refuse a target that is not already
  foreground. `SetForegroundWindow` is not called.
- **Screen-region capture and BitBlt fallback.** Only PrintWindow is used.
  A fallback screen grab could capture an occluding window or unrelated
  content.
- **Clipboard reads or writes.** No clipboard API is used. Clipboard key
  shortcuts are rejected.
- **Process launch, shell, file dialogs and filesystem access.** No launch or
  file API is implemented.
- **Network access.** No network API is implemented. An application may react
  to an invoked element or typed text with its own network activity; that is
  outside this tool's control and is not claimed to be sandboxed.
- **Credential reads.** Element values are never read. Password elements are
  excluded from actions and blanked in observations. Screenshot capture is
  rejected when a password element is visible in the bounded tree, but this
  check is heuristic: providers that do not expose `IsPassword` cannot be
  detected.
- **Elevation, UIAccess, secure desktop and cross-session control.** These are
  rejected, not implemented.
- **Arbitrary key codes or scan codes.** Only the closed chord vocabulary is
  available.
- **Element value or document text extraction.** Observations return metadata
  only. Element names are untrusted application text and may themselves
  contain sensitive content.
- **Attaching to an existing third-party process by name or title.** Targets
  are caller-observed window identities; there is no search-and-attach
  authority.

## Why this is not B4 qualified

B4 requires demonstrated, reproducible evidence for a specific product
workflow, including a reviewed adapter, an owned synthetic application,
current screenshots or runtime traces, teardown, accounting and acceptance
records. This foundation provides none of those:

- No live desktop input was performed during development.
- No owned synthetic application, native adapter, campaign ledger or
  qualification run exists for these tools.
- The screenshot path is bounded and PrintWindow-only, but it has not been
  visually qualified across DPI, compositor, multi-monitor or accessibility
  configurations.
- The password-surface exclusion is heuristic.
- UIA providers vary; a provider may ignore patterns, expose stale geometry or
  block inside a COM call despite the configured timeouts.
- Application callbacks can have effects outside the tool's observation, so
  partial or uncertain effects remain possible.
- The foundation has compile and lint evidence only. It is not a release
  candidate, not a general computer-use sandbox and not an acceptance claim.

## First-party extension gate

`ReviewedWindowsApplication` admission in
`crates/argentum-extensions/src/configuration.rs` keeps every existing
package, digest, pin, namespace, lease and scope check. The placeholder
rejection is replaced only when `desktop_control_available()` is true, which
requires the real Windows module to have been compiled. Third-party package
trust checks are unchanged.

## Verification scope

This foundation is expected to pass formatting, compilation and strict clippy
for `argentum-extensions`, `argentum-tools` and `argentum-cli` with
`--features desktop-control --all-targets --locked --offline`. The default
build without the feature must also compile. No desktop tool is executed and no
live desktop input is sent by these checks.
