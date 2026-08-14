# Argentum 0.1.0 Mobile Design Draft

Status: interaction and architecture draft, not a shipped mobile application

Argentum can support an intentional phone interface without moving product
logic into the view layer. The shared command host, domain commands and events,
runtime, approvals, provider profiles, workspace boundary, and durable sessions
already provide the reusable product core. The mobile work is a new native view
composition and platform host over that core.

This document defines the bounded 0.1.0 mobile design slice. It does not claim
an Android or iOS build, package, signing pipeline, device runtime, safe-area
adapter, keyboard adapter, or remote pairing transport.

## Scope and truth boundary

### Implemented baseline

- `AppCommand` and `AppEvent` define the shared product protocol.
- `argentum-cli::CommandHost` is the shared entry point for sessions, providers,
  runs, tools, approvals, and workspace state.
- The desktop native app uses `InProcessClient`, so it does not spawn a CLI
  process or serialize commands for local interaction.
- Runtime, security, store, provider, and workspace behavior remain outside the
  Slint view layer.
- The design token source already defines the phone and tablet width boundaries.

### Added by this draft

- an intentional `MobileShell` contract for widths below 768 px;
- phone information architecture and component ownership;
- touch, safe-area, keyboard, sheet, and approval behavior requirements;
- portrait review fixtures at 360 px, 390 px, and 430 px wide;
- an Android-first platform sequence with explicit release gates.

### Not implemented or claimed

- an APK, Android Activity, Gradle project, mobile Cargo host, or device test;
- an iOS application target or signing configuration;
- production safe-area, IME, lifecycle, back-navigation, notification, or deep
  link integration;
- authenticated remote pairing between a phone and another Argentum host;
- direct provider access from a phone;
- mobile release readiness or mobile accessibility conformance.

## Product role

The first phone experience is a companion for active Argentum work. It should
let a user:

- start or continue a session;
- read the conversation and visible run progress;
- stop an active run;
- inspect and pause or resume an active goal within the authority granted by the
  host;
- see remaining goal budget, the next required action, and whether work is
  waiting for approval, user input, or verification;
- inspect bounded activity and verification summaries;
- review and resolve a pending approval;
- switch sessions and inspect provider and workspace context.

Full diff review, complex workspace management, provider credential setup, and
policy editing remain desktop-first until their mobile interaction and security
contracts are implemented and verified.

## Shell and navigation

At widths below 768 px, Argentum should render `MobileShell`, not a compressed
desktop rail and inspector arrangement. The phone shell has one primary column:

```text
safe area
session header
conversation and task state
composer
IME or bottom safe area
```

The 56 px session header contains the Argentum mark, a bounded session title,
run or goal state with text, and routes to sessions, goal, activity, and
overflow actions. The identity mark remains the canonical Argentum asset.
Status must never rely on color alone. Waiting, paused, budget-limited, failed,
and complete states remain distinguishable at the narrowest supported width.

The conversation uses the full available width with a 12 px gutter from 360 px
through 379 px, and a 16 px gutter from 380 px through 767 px. The narrow shell
must not preserve desktop spacer columns. User prompts use restrained graphite
surfaces. Assistant output is primarily frameless. Generic chat bubbles, oval
status pills, decorative gradients, and floating decoration are outside the
system.

There is no permanent bottom tab bar in the 0.1.0 draft. Sessions, activity,
run context, and overflow actions open from the header or composer. This keeps
the task and its next decision dominant.

## Composer

The composer is anchored above the IME and bottom safe area. It includes:

- a multiline prompt field with bounded growth;
- provider and workspace context behind one compact context action;
- the effective execution profile in the context summary;
- one primary action for Start or Stop;
- a visible approval-needed state that routes to the pending decision.

The phone may show the effective execution profile but does not offer policy
editing in this draft. `Confirm Before Changes` remains the default. Higher
authority cannot be presented unless the host enforces it through the shared
capability policy.

Every interactive target is at least 48 x 48 px in the phone shell. The prompt
remains recoverable when a sheet opens, the app backgrounds, or the keyboard
changes height. Sending does not depend on a hover state or desktop shortcut.

## Mobile sheets and routes

Sessions, goal summary, activity, run context, and compact provider context use
bottom sheets. A sheet uses an explicit title, close action, bounded height, and
a dimmed scrim. It is edge-aligned on small phones and may use a restrained
corner radius on its exposed top edge. A sheet typically occupies 70 to 85
percent of the usable height, but must yield to the IME and safe areas.

The goal summary shows the persisted objective, state, remaining budget,
iteration count, next action, open requirements, and verification history. It
does not expose private reasoning or treat assistant text as lifecycle state.
Stop pauses an active goal. Resume is disabled when the budget is exhausted or
an approval or user decision is still required.

Long changes, verification details, and provider configuration use full-screen
routes instead of stacking a desktop dialog inside a sheet. Android system Back
first closes the top sheet or route, then returns to the prior application
state. Back must not silently cancel a run, reject an approval, or discard a
draft.

## Approval safety

Approvals remain host-authoritative. The phone displays the exact pending
approval identifier indirectly through the shared state projection and shows:

- requested action;
- bounded target;
- reason;
- current supported scope;
- Reject and Approve once as explicit actions.

Approve once is consequential and must require a deliberate tap. Swipe gestures
cannot approve or reject. Tapping outside the sheet cannot resolve a request.
Session-wide approval, remembered policy, and expiration controls must not be
shown until the runtime implements and verifies those choices. If connection or
state freshness is uncertain, approval actions are disabled and the interface
asks the user to refresh the request.

Timeouts never approve a plan, permission request, or ordinary user question.
An unanswered request remains visible as waiting and blocks automatic goal
continuation when user input is required.

## View module boundary

The intended view organization keeps state projection separate from layout:

```text
ui/app.slint
  Rust-facing state and callbacks
ui/shells/desktop.slint
  desktop composition
ui/shells/mobile.slint
  phone composition below 768 px
ui/surfaces/
  conversation, sessions, activity, approvals, review, providers
ui/components/
  shared controls, task components, and adaptive sheets
ui/tokens.slint
  shared visual and responsive tokens
```

Desktop and mobile shells reuse the same surface components and projected
state. They may compose those surfaces differently. They must not duplicate
provider calls, approval rules, persistence, or runtime state machines in
Slint.

## Platform boundary

Android is the first mobile implementation target. The first host must provide:

- native window and renderer lifecycle;
- top and bottom safe-area insets;
- IME height, focus, and keyboard visibility updates;
- Android Back routing;
- pause, resume, and process-restoration behavior;
- device secure-storage integration for pairing credentials;
- accessible names and native accessibility verification.

The platform adapter publishes these values and events to the UI. Layout code
must not infer system insets from a device model or hard-coded status-bar size.

The first desktop application continues to use the in-process command client.
A later phone companion connects through an authenticated, versioned remote
client to the same `CommandHost`. That transport is a separate security and
performance milestone. It requires explicit pairing, encrypted transport,
revocation, bounded messages, streaming with backpressure, cancellation, and a
clear offline state. The phone must not call a model provider directly or store
provider API keys in view state.

## Responsive fixtures

The bounded portrait fixtures are:

| Fixture | Purpose |
| --- | --- |
| 360 x 620 | Minimum-height and narrow-width stress case |
| 390 x 844 | Standard phone composition |
| 430 x 800 | Wide phone composition at the current desktop QA height |
| 430 x 932 | Wide phone composition and long-line control |
| 800 x 360 | Phone landscape stress case using the mobile shell |

Each fixture must cover empty, active, running, paused, budget-limited, waiting
for user, approval, verification failure, long content, offline, and
keyboard-visible states. Review also includes:

- top and bottom safe areas;
- 48 px touch targets and spacing between consequential actions;
- text scaling without clipped controls;
- long session, workspace, provider, and target names;
- sheet opening, nested route return, and Android Back behavior;
- conditional autoscroll that does not pull the reader away from older content;
- composer recovery after IME resize and app backgrounding;
- no horizontal overflow at any fixture width.

Foldable, tablet two-column, screen-reader, and reduced-motion tests remain
release gates after the portrait shell is stable. A compiled desktop
window resized to phone dimensions is useful visual evidence, but it is not
device-runtime proof.

## Delivery sequence

1. Extract shared task, goal, approval, verification, and activity projections,
   then compose an intentional `MobileShell` below 768 px.
2. Verify the portrait fixtures on the desktop renderer with deterministic
   state fixtures.
3. Add the Android host and platform inset, IME, lifecycle, and Back adapters.
4. Verify on an Android emulator and at least one physical device.
5. Design and threat-model authenticated remote pairing over `CommandHost`.
6. Add packaging, signing, installation, upgrade, and release validation.

The mobile application can be called implemented only after steps 1 through 4
pass. It can be called a useful remote companion only after the pairing path in
step 5 is implemented and verified.
