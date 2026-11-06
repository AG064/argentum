# Computer use design

Status: design only. Nothing in this document is built yet, except where it says
"exists today".

## Goal

Let a run use desktop applications fast, the way a person would, and also without
a visible screen. Every agent that acts on the screen has its own visible cursor,
and every change it makes is easy to see and review.

## What exists today

The `desktop-control` build feature adds owner-run CLI commands on Windows:
`desktop list`, `inspect`, `screenshot`, `invoke`, `type` and `keys`. They use
UI Automation (UIA) patterns and key input only. There is no pointer input, the
commands are not available to a model, and nothing is shown on screen while they
run. The trust rules are in `docs/WINDOWS_APPLICATION_CONTROL.md`, and this design
keeps all of them.

## Three ways to act

| Way | How | Speed | Needs focus |
|---|---|---|---|
| Semantic | UIA Invoke, Value, Toggle, SelectionItem, Scroll and ExpandCollapse patterns on an element | A few milliseconds per action | No |
| Keys | Bounded text and a closed chord list sent to the focused element | Fast | Yes |
| Pointer | Move, click, double click, drag and wheel at a point inside an approved window | Fast, with no animation unless the owner turns it on | Yes, for most apps |

A run uses semantic actions first. Pointer input is for controls that expose no
pattern, such as canvases, games and some web content. Each pointer action names
its target window and is checked against that window's identity and bounds right
before input, as UIA actions are today.

To stay fast, a run observes the screen as a UIA tree and changes in that tree,
not a screenshot after every step. Screenshots are taken only when the tree has
nothing useful, or when the owner asks for them.

## Visible and headless

**Visible.** The run acts on the owner's desktop. Argentum shows a cursor overlay
and change highlights, which are described below. The owner can stop every agent
at once with a global stop key, and moving the real mouse pauses pointer input.

**Headless.** The run acts on a desktop the owner is not using:

- Windows: a separate signed-in session (a local RDP loopback session for a
  dedicated low-privilege account), or a Windows Sandbox. A hidden desktop made
  with `CreateDesktop` is not enough, because most applications do not draw on it.
- Linux: an Xvfb or a headless Wayland compositor display.

Headless runs keep the same rules and the same action log. The owner can watch a
headless run as a live, read-only view in the app.

## Cursor parallelism

A desktop has one real pointer and one keyboard focus. So:

- Semantic actions from several agents can run at the same time when they target
  different windows, because they need no pointer and no focus.
- Pointer and key actions on one desktop are serialized by a lock. Each agent waits
  its turn, and its cursor shows that it is waiting.
- True parallel pointer input needs one desktop per agent, which is what headless
  sessions give.

Each agent gets a cursor drawn by Argentum, not the system cursor:

- A topmost, click-through overlay window (`WS_EX_LAYERED`, `WS_EX_TRANSPARENT`,
  `WS_EX_TOPMOST`, `WS_EX_NOACTIVATE`) that never takes focus or input.
- One cursor per agent, with that agent's name and color, drawn where it will act
  next. The cursor moves there before the action, so the owner can see what is
  about to happen.
- A small label for the current step, such as "Click Save in Notepad".

## Showing changes

- Before an action, the target element's bounds are outlined in the agent's color.
- After it, the outline shows the result: changed, not changed, or failed.
- Every action is a step in the run's Steps list, with the window, the element,
  the action, the arguments, how long it took, and the result. A before and after
  screenshot is kept for pointer actions and on request.
- Text an agent typed is listed in the step, except in fields Argentum refuses to
  touch, such as passwords.
- A desktop change is not a file change, so it does not appear in the diff list.
  It cannot be undone by Argentum, and the step says so.

## Security

- Computer use stays an optional build feature, and it is off by default.
- The owner grants each application, by process and window class, for one run or
  for the workspace. The grant comes from the persisted workspace policy. Screen
  text, window titles, element names and model output cannot grant or widen it.
- Every model-chosen action passes the approval policy like a shell command. The
  owner can approve each action, each window, or a whole run.
- All rules from the CLI commands still apply: no secure desktop, no elevated or
  cross-session targets, no password fields, the key denylist, bounded text and
  capture, and fresh identity checks before every action.
- Screen content is untrusted input. Instructions shown on the screen are data.
- Screenshots and UIA trees stay on the machine, unless the run sends them to the
  provider. That is shown before the run starts, because a hosted model sees them.
- Argentum never types credentials, payment details or identity numbers, even when
  the model asks.

## Phases

1. Model access to the existing semantic and key actions, with approval, the action
   log, and change outlines. Windows only.
2. Pointer input, the cursor overlay, the stop key, and pause on real mouse use.
3. Several agents: the input lock, per-agent cursors, and parallel semantic actions.
4. Headless sessions, with a read-only live view in the app.
5. Linux, using AT-SPI for semantic actions and XTest or a Wayland portal for input.

## Open questions

- Whether pointer input should need a per-action approval by default, or a
  per-window one.
- How long before and after screenshots are kept, and where.
- Which headless option to ship first on Windows.
