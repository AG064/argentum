# Windows desktop control

Windows desktop control lets Argentum Genesis (the `argentum` CLI) look at and
operate Windows applications through UI Automation, the accessibility interface
Windows provides. It works in two ways:

- **Direct commands.** You run `argentum desktop ...` yourself to list windows,
  inspect their controls, take a screenshot, or activate a control or type into it.
- **Model tools.** The model operates one application that Argentum launches for
  it, using the tools `desktop_launch`, `desktop_observe`, `desktop_action`,
  `desktop_screenshot` and `desktop_close`.

Desktop control is not a general computer-use sandbox. It works through controls
that applications expose, not through the mouse or screen coordinates.

## Where it is available

It is in the `argentum-genesis-desktop-control-0.1.0-windows-x64.zip` download.
The default `argentum-genesis` download and the desktop app do not include it.
You can also build the CLI with `--features desktop-control` on Windows, or run
`scripts/package-windows-cli.ps1 -DesktopControl`. The package contains no
application, configuration, documents or account data.

In builds without the feature the `desktop` commands report that desktop control
is unavailable. `argentum desktop status --json` shows whether the build includes
it, the capability state and the registered tools. It lists no windows and sends
no input.

## Direct commands

You run these yourself at a terminal. A model cannot run them.

```powershell
argentum desktop list [--json]
argentum desktop inspect --hwnd 0x... [--depth N] [--json]
argentum desktop screenshot --hwnd 0x... --output C:\path\window.png [--json]
argentum desktop invoke --hwnd 0x... --element <selector> --confirm <HWND> [--dry-run] [--json]
argentum desktop type --hwnd 0x... --element <selector> --text TEXT [--mode value|keys] --confirm <HWND> [--dry-run] [--json]
argentum desktop keys --hwnd 0x... --element <selector> --chord CHORD [--chord CHORD ...] --confirm <HWND> [--dry-run] [--json]
```

- `list` shows up to 64 top-level windows with handle, process, title, class and
  position.
- `inspect` prints the control tree of one window: name, type, automation id,
  class, position and flags such as enabled, focused, offscreen and password.
  It reads at most 8 levels and 256 elements, and never reads control values.
- `screenshot` saves a PNG of one window. It never captures the whole desktop.
  The image is limited to 8 megapixels and 32 KiB.
- `invoke` activates a control with UI Automation Invoke or SelectionItem. It
  does not click a coordinate or move the pointer.
- `type --mode value` (the default) writes to a writable text control. It rejects
  read-only controls. `--mode keys` sends keystrokes to a control that already
  has focus in a window that is already in the foreground.
- `keys` sends key chords (such as `Ctrl+S`) to a focused control in a foreground
  window. At most 8 chords per command.
- `--confirm` takes the same window handle as `--hwnd`. It is required for
  `invoke`, `type` and `keys`.
- `--dry-run` finds and checks the window and control, then prints the action it
  would take, without performing it. `--confirm` is still required. JSON output
  then has `dry_run: true` and `applied: false`.

Text is limited to 1,024 characters, with newline and tab as the only control
characters. Element references are only valid for the window state you just inspected.

Before each action Argentum looks the window and control up again and checks that
nothing changed (title, class, position, process, control name, type and
position). It fails if any of those differ.

A successful command means Windows accepted the request, not that the application
did what you wanted. Inspect again after every action.

## Model tools

The model tools drive one application at a time, and only one that Argentum
launched itself from a configuration you reviewed. They cannot attach to
applications that are already running or look through your windows.

- `desktop_launch` starts the configured application with one document from a
  list you approved. The model picks a document name from that list, not a path.
- `desktop_observe` returns the application's controls, with references, and the
  document text. The text is untrusted. Results come in pages (follow
  `next_offset`). Every new observation makes earlier references invalid.
- `desktop_action` acts on one observed control: invoke, expand or collapse,
  select, toggle, set a value, select or search text, or scroll. For editors that
  expose no text interface, you can separately enable typing at the current
  selection (at most 256 characters; newline and tab only) and a short fixed list
  of standard menu commands (File/Open, File/Save As, Edit/Select All,
  Search/Find, Search/Find Next). Both are off by default.
- `desktop_screenshot` captures the application window. Password surfaces are not
  captured.
- `desktop_close` closes the application Argentum launched and nothing else.

To use them, install, configure and activate the desktop adapter as an
[extension](EXTENSIONS_BROWSER.md) (the `extensions/windows-desktop` folder in the
download). Configuration pins the application, the adapter and its documents by
SHA-256 and sets an action limit between 1 and 1,536 for the campaign. Changing
the configuration removes the activation.

Argentum shows a small status window with Stop and Resume controls. If it sees
real keyboard or mouse input from you, or another window takes the foreground, it
pauses until you resume. Nothing is retried automatically after an action whose
result is unknown.

## Approval

Every desktop action asks for your approval, in every permission profile. The
`never` approval mode and full-access profiles do not skip it. Even observing
needs approval, because window titles and text can be private. Model output,
window text and document content cannot grant approval.

## Limits

| Limit | Value |
| --- | --- |
| Windows listed | 64 |
| Control tree | 8 levels, 256 elements, 64 KiB |
| Text per command | 1,024 characters |
| Key chords per command | 8, each up to 32 characters |
| Screenshot | 8 megapixels in, 32 KiB PNG out |
| Text per `replace_selection` action | 256 characters |
| Model tool actions per campaign | 1 to 1,536, set in the configuration |

## Keys that are blocked

Key input allows Ctrl, Shift and Alt combined with letters, digits,
navigation and editing keys, and F1 to F12. These are rejected:

- the Windows key, Alt+Tab, Alt+Esc, Alt+F4, Ctrl+Esc and Ctrl+Shift+Esc
- Ctrl+Alt+Delete
- the clipboard shortcuts Ctrl+C, Ctrl+X, Ctrl+V and Shift+Delete
- bare modifier keys and arbitrary key codes

## Not supported

- **Pointer input.** Argentum does not click or move the mouse. A click at a
  point can land on a window that moved or appeared since you looked.
- **Focus stealing.** Key input goes only to a window that is already in the
  foreground, and Argentum never brings a window forward.
- **Password fields.** They are rejected for actions and screenshots, and
  their values are never read. Applications that do not tell Windows a field is a
  password cannot be detected.
- Control values are not read, and there is no clipboard access, shell, general
  file access or whole-desktop capture.
- Elevated applications are refused unless Argentum is also elevated. Windows in
  another session, on the secure desktop (such as the UAC prompt), minimized,
  hidden or not responding are refused.

An application can still do anything your Windows account can do in response to a
click or keystroke, including network access. Desktop control is not an operating
system sandbox.

## Privacy

Window titles, control names, document text and screenshots can contain private
information. In the model tools, observations and screenshots are sent to your
model provider. Do not share or commit them. Image input works only on the
ChatGPT route.

## Limitations

Desktop control is tested with automated checks. It has not been validated across
many different desktops, display scaling (DPI) settings and multi-monitor setups,
and applications differ in how well they support UI Automation.
