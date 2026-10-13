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
