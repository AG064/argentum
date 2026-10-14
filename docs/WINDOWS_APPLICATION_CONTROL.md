# Optional Windows application control

NEXT02 is implementation work, separate from completed NEXT01 and the core RC.
Real-editor/model acceptance is not yet qualified. Do not infer desktop parity
or a general computer-use sandbox from source or synthetic controls.

The explicit `argentum-cli/desktop-control` feature uses the existing extension
broker, lifecycle, exact local activation, process leases, policy, accounting,
current images and native goal delivery. Default CLI startup and package are
unchanged and inert. It cannot discover or start desktop work automatically.
The optional packager switch is `-DesktopControl`; no editor or owner state is
bundled. The adapter needs Windows 10 1703 or newer and installed .NET Framework
4.8 managed UI Automation. Current acceptance must name the tested OS version.

## Exact authority and disclosure

`reviewed_windows_application` is distinct from browser/code approval and is
rejected by builds without the optional Windows capability. Normal extensions
installation/configuration and a real local `ACTIVATE` review remain necessary.
The executable, configuration and application inputs must be SHA-pinned in the
definition. Configuration fixes revoke the previous activation.

The adapter admits only one newly launched, non-personal process, exact executable,
start identity, Windows session and default interactive desktop. No attach by
name/title and no search across desktop windows is available. Its actual owned
root and owner-chain dialogs are checked before actions. Allowed dialog titles,
control names, text-control classes and actions are finite owner selections,
not title-based authority on their own. Replaced elements, current bounds,
runtime identity and structure generations invalidate references. The UIA worker
owns all COM observations in one MTA, away from the status UI.

Owner lookup walks bounded actual top-level parent and `GW_OWNER` links, with
the pinned process required at every link and the exact root required at the end.
This supports explicitly owned overlapped dialogs without admitting unrelated
same-process windows. `GA_ROOTOWNER` alone can omit an overlapped window's explicit
owner. A null foreground during activation is unavailable, not proof of takeover;
no effect is admitted. Real input or a nonzero foreign foreground still latches
pause and requires owner resume. Input time is not reset by ordinary actions.

This is reviewed ordinary-user code and application policy, not an OS filesystem
or network sandbox. An ordinary editor can still access resources available to
that OS account. No elevation, UIAccess, secure desktop, shell launch, settings,
clipboard, password entry or general key/coordinate endpoint is supported.
The minimal visible status window has stop and explicit owner-resume controls.
Input/focus takeover pauses admission. The initial owned launch may focus once;
there is no loop fighting the owner for foreground. Global focus/input checks
are not atomic isolation. The text fallback addresses the exact current edit
HWND, not global SendInput. Its first supported scope is synthetic documents.

## Normal model tools

- `desktop_launch`: one exact admitted document and a new owned application.
- `desktop_observe`: bounded semantic UIA state, opaque references, current
  generation, supported patterns and untrusted document text. Responses page
  within the actual duplicated MCP byte bound. Follow `next_offset`; a new
  observation expires all earlier references. Page truncation and incomplete
  traversal are distinct. Internal save/capture checks retain the bounded full
  current observation, not only the returned page.
- `desktop_action`: explicit invoke, expand/collapse, select, toggle, writable
  ValuePattern, TextPattern selection/search and ScrollPattern operations.
- `desktop_screenshot`: fresh admitted-window PrintWindow PNG, physical window
  geometry and separately reported logical image dimensions, timestamp/hash.
- `desktop_close`: only the actual owned application, never unrelated processes.

Observe after each effect and every handoff. No implied fallback is performed.
`replace_selection` is separately owner-enabled targeted text input. It accepts
at most 256 UTF-16 code units, LF as one native Enter and Tab; CR and other
control characters are rejected. It uses no clipboard or editor-internal API.
When that unchanged route is enabled and its current HWND/class/focus checks pass,
observation advertises `TargetedTextInput`, `targeted_text_input_enabled: true`
and the 256-code-unit bound. It remains usable without UIA TextPattern, while an
unfocused, unadmitted or disabled control advertises no such route. Capability
metadata never grants authority and every effect repeats the actual validation.
The submitted observation still passes the full structure/name/geometry guard
before admission. Within one admitted text batch, each character uses a fresh
same-target identity snapshot. This keeps PID, HWND, runtime ID, automation ID,
class, role, geometry, focus, password and enabled-state checks while allowing
the text's own accessible-name and unrelated chrome updates. It does not replace
the observation, rebaseline user input, or authorize a changed target. Unavailable
identity or a changed target remains an uncertain partial effect, not success.
An unpredictable per-batch window property also detects target destruction and
HWND recycling; it is removed at cleanup and is never model supplied. An admitted
message can trigger application callbacks, so uncertain partial effects are not
claimed to have left the application's internal state unchanged.
For the explicitly admitted Scintilla control, LF and Tab use only targeted
standard Enter/Tab down/up messages; ordinary text uses WM_CHAR. Both messages
are reserved and counted as separate primitive API returns. No global keyboard
state, shortcut combination or arbitrary key code is available. Native editor
indentation/tab preferences still apply and API returns do not certify content.
Every primitive is admitted against a durable finite campaign ledger before
input; API-return counts and observed outcomes remain separate. A text batch
reserves all its characters once, with interruptible per-character checks. An
unused reservation is not a claim that the input was applied. Counter records
bind the explicit campaign, application hash and actual task directory. Missing,
malformed, wrong-scope or exclusively owned ledgers fail closed, not reset to zero.
Typed receipts keep reserved primitives separate from producer-reported API
returns. Neither is proof of an observed document change. File-dialog path checks
require the actual writable, owned filename edit, not a similarly named label.
Teardown is idempotent across the UI watchdog and worker. Incomplete cleanup is
reported as uncertain rather than a closed application.

For native editors without TextPattern, `targeted_menu_input` and `menu_paths`
require separate exact owner activation and default to disabled. The first scope
is five closed English standard menu paths: File/Open..., File/Save As...,
Edit/Select All, Search/Find..., and Search/Find Next. Every path component must
also be owner-admitted. There are no caller-supplied command IDs, arbitrary menu
paths, application-internal APIs, shell/run commands or keyboard sequences.

Observation exposes available commands from the current owned standard Win32
menu as `Win32.MenuCommand` with `TargetedMenuInvoke`; only `invoke` is accepted.
These are current menu metadata, not claims that a popup is visibly expanded.
Their reported geometry is the owned root, never a click coordinate. References
bind root/runtime/structure/geometry plus exact menu handles, path, enabled state
and command identity. A changed menu is rejected until a new observation.
Select All additionally requires the current admitted non-sensitive text control
to have focus. Normal UIA text-selection forms remain valid when supported.

The adapter posts one bounded standard WM_COMMAND to that exact owned HWND;
it does not synthesize global input or rebaseline the owner-input guard. A queued
command's API return is not its observed completion. Reobserve after every effect;
file-dialog destination checks, accounting, cancellation and owned teardown still
apply. Receipts retain only a closed menu-action enum, not menu text, command IDs,
paths or document content, and never turn producer claims into verification.
Freshness and provider-failure categories are bounded and body-free. Transient
read-only inspection failure requires fresh inspection, not reuse of old targets
or an automatic repeat of a mutation whose effects could be unknown.

Images use the already supported current run/context/activation image-input
path. The marker origin `https://windows.argentum.invalid` is a disclosure identity,
not a contacted network endpoint. Captures never begin with a full desktop image.
Unavailable, minimized, cloaked, protected or changed surfaces do not return old
pixels. Password surfaces omit text and deny capture. Partial or timed-out UIA
effects are uncertain, retire admission and must not be blindly retried. A
successful API return is not saved-artifact acceptance or verification.

The real-editor demonstration exposes desktop operations, bounded text reading
needed for task instructions, and the exact approved artifact verifier. All
filesystem mutation, scripts and shell shortcuts remain disabled and cannot author
the result. Fixed verification is independent of generic argv enablement; project
runners still require exact owner approval. Current verifier feedback and model acknowledgement
are required before `goal deliver` can complete. A save dialog closing is not a
verification receipt. Private documents, captures, exact local paths and account
state belong outside public source/ordinary logs.
