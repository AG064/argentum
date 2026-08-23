# Argentum Premium Agent Harness Design System

Status: Argentum 0.1.0 draft desktop implementation contract

The design system is a product contract, not a CSS mood board. Every surface
must make the agent's current task, authority, work, and next decision clear.
Rust owns the application and runtime state. Slint authors the native desktop
components. The 0.1.0 draft does not claim a mobile implementation or complete
coverage of every target component contract.

## Contract scope

### Implemented in the 0.1.0 draft

- a native desktop shell authored in Slint and hosted by Rust;
- one CLI and command-host path for sessions, providers, runs, tools, approvals,
  and workspace state;
- conversation, run-stage, activity, review-summary, settings, command-palette,
  and approval surfaces;
- the canonical Argentum identity assets and token source.

### Target contract

Later sections describe the intended complete product where they use words such
as `must`, `every`, or `provides`. Those statements are acceptance requirements,
not claims that the draft already implements mobile presentation, generalized
docking, session pinning, full diff review, session-wide approval, or policy
editing. The target also includes persisted goal supervision, task views, real
file and terminal surfaces, and execution profiles backed by Rust policy. A
capability is release-ready only when its implementation and verification are
present.

## Design principles

1. **Task before configuration.** The active project and session always outrank
   global settings.
2. **Calm surface, visible work.** The conversation is quiet, but the plan,
   changes, approvals, and verification are never ambiguous.
3. **Progressive detail, not hidden capability.** Tool calls can be collapsed.
   Important decisions must remain one interaction away.
4. **One visual hierarchy.** A screen has one primary action, one active state,
   and one clear place where work is happening.
5. **Argentum restraint.** Black, gray, white, silver, and red do the work. Use
   texture, gradients, and decoration sparingly.
6. **Motion explains state.** Animation shows progress, expansion, completion, or
   attention. It never runs continuously for decoration.
7. **Truth over polish.** Unavailable, untested, pending, and failed states are
   explicit and actionable.
8. **Protocol before chrome.** Goal, permission, verification, and task state are
   real Rust domain records before the UI exposes controls for them.

## Brand character

Argentum should feel like a precision instrument:

- premium, quiet, exact, and confident;
- technical without looking like a terminal;
- minimal without looking empty;
- powerful without exposing every mechanism at once.

Avoid:

- generic assistant bubbles;
- dashboard grids as the primary experience;
- excessive uppercase micro-labels;
- fake green readiness lights;
- version strings in the brand lockup;
- gradients, glass effects, blurred glow, and decorative noise that reduce
  clarity;
- oversized decorative serif empty-state copy or background marks that compete
  with the task composer.

## Identity system

The canonical Argentum mark is an identity asset, not a generic interface icon.
Use `BrandMark` for the symbol and `BrandLockup` for the symbol with the product
name. Never reconstruct the mark with type, including an `Ag` text substitute.
Do not tint, redraw, stretch, apply an ad hoc crop, or sample the mark as
interface decoration.

The source of truth is `assets/brand/argentum-source.png`. Its exact legacy
provenance, SHA-256 hash, alpha bounds, deterministic crop, and all derived
variant hashes are recorded in `assets/brand/manifest.json`. Run
`python scripts/brand_assets.py` to verify the complete identity set. Use
`python scripts/brand_assets.py --write` only with the Pillow version pinned in
the manifest to regenerate the nine declared transparent PNG variants.

Product identity uses:

- the transparent PNG variants for native UI surfaces and the window icon;
- `argentum.ico` for the Windows executable resource;
- `argentum.icns` for future macOS packaging;
- the real mark at 20, 32, or 64 px in application surfaces.

The mark contains its own restrained red edge. Outside that canonical artwork,
red is limited to primary actions, attention, and errors. Selection, connected,
ready, running, and completed states use silver or white.

## Color tokens

### Primitive palette

| Token | Value | Use |
| --- | --- | --- |
| `canvas` | `#090A0C` | app canvas |
| `chrome` | `#0D0F12` | header and deepest controls |
| `navigation` | `#111419` | navigation and panes |
| `surface` | `#101318` | main surface |
| `surface-soft` | `#15191E` | selected and grouped surface |
| `surface-line` | `#1C2229` | inline message surface |
| `raised` | `#222930` | raised panel |
| `elevated` | `#293139` | pressed and elevated surface |
| `hover` | `#242B32` | hover surface |
| `silver-700` | `#59636D` | low-emphasis border and icon |
| `silver-600` | `#737D87` | inactive icon |
| `silver-500` | `#929CA6` | muted text |
| `silver-400` | `#B4BDC6` | secondary icon |
| `silver-300` | `#C8CFD6` | connected and selected state |
| `silver-200` | `#DEE3E7` | strong secondary text |
| `white-100` | `#F5F6F7` | primary text |
| `white-000` | `#FFFFFF` | highest emphasis |
| `red-700` | `#7E252D` | deepest signal surface |
| `red-600` | `#A8323C` | strong border accent |
| `red-500` | `#C64049` | signal accent |
| `red-400` | `#E16870` | hover and active accent |
| `red-100` | `#F6D9DB` | accent text on dark |

### Semantic tokens

```text
border-faint        #DEE3E70F
border-subtle       #DEE3E721
border-default      #DEE3E738
border-strong       #DEE3E75C
focus-ring          #F6D9DB
focus-ring-inner    #AEB6BF
text-primary        #F5F6F7
text-strong         #FFFFFF
text-secondary      #DEE3E7
text-muted          #929CA6
text-tertiary       #7D8791
text-danger         #E16870
attention-surface   #C8444C1F
scrim               #050607D9
control-fill        #1A1F25
control-hover       #242B32
control-pressed     #111419
control-selected    #252D35
control-disabled    #12161A
primary-fill        #B93641
primary-hover       #C03B46
primary-pressed     #982A34
state-neutral       #B4BDC6
state-active        #FFFFFF
state-attention     #E16870
state-danger        #DC626B
state-complete      #DEE3E7
```

All semantic text colors clear WCAG AA contrast on their intended dark
surfaces. Every primary fill clears WCAG AA with `text-primary`. Success,
selection, connection, readiness, and running use neutral silver or white. Red
means primary action, attention, danger, blocked progress, or error. Do not use
color alone to communicate state.

## Typography

### Families

- Current desktop UI: `Segoe UI Variable`, with renderer fallback when the font
  is unavailable.
- Current code token: `Cascadia Code`, with renderer fallback when unavailable.
- Future platform adapters may select an equivalent native sans or monospace
  family without changing the type roles.

### Scale

| Role | Size | Weight | Line height |
| --- | ---: | ---: | ---: |
| display | 30 px | 700 | 1.1 |
| page title | 22 px | 700 | 1.2 |
| dialog title | 18 px | 700 | 1.2 |
| section title | 16 px | 700 | 1.3 |
| lead | 15 px | 600 | 1.4 |
| body | 14 px | 400 | 1.5 |
| body strong | 14 px | 600 | 1.5 |
| compact | 12 px | 500 | 1.4 |
| metadata | 11 px | 500 | 1.3 |
| code | 12.5 px | 400 | 1.6 |

Body text must not fall below 14 px on desktop or mobile. Metadata may be 11 px
when it is not the only way to understand an action or state. Use sentence case
for labels. Reserve uppercase for short status badges only.

## Spacing and geometry

Use a 4 px base grid:

```text
0, 4, 8, 12, 16, 20, 24, 28, 32, 36, 40, 48, 64
```

All spacing and padding declarations use named values from `ui/tokens.slint`.
The only off-grid spacing token is `optical-tight-gap` at 2 px. It is limited to
paired labels and dense event stacks where a 4 px gap weakens perceived vertical
centering. The validator requires zero raw spacing declarations outside the
token source.

### Radii

| Token | Value | Use |
| --- | ---: | --- |
| `radius-xs` | 2 px | status blocks, composer, panels, sheets, and edge details |
| `radius-sm` | 3 px | compact optical details |
| `radius-md` | 4 px | buttons, inputs, and reusable surfaces |
| `radius-lg` | 6 px | reserved for a verified large-surface need |
| `radius-xl` | 8 px | reserved and unused in the current draft |

Pill and oval containers are not part of the Argentum system. Status uses a
compact rectangular cluster, a text label, and a square marker or edge accent.

### Borders and elevation

Use one-pixel borders with semantic opacity. Elevation comes from surface steps,
not heavy shadows. Shadows are allowed only for floating sheets, menus, and
command palettes.

## Layout system

### Desktop breakpoints

```text
compact desktop: 1024-1199 px
standard desktop: 1200-1599 px
wide desktop: 1600 px and above
```

Default standard desktop widths:

```text
global rail: 56 px
project/session pane: 264 px
main work surface: flexible, never below 560 px
optional work pane: 372 px
maximum reading and composer width: 820 px
```

The shell navigation collapses below 900 px. The review pane docks at 1240 px
and above. The composer uses its compact arrangement below 560 px. These primary
values are semantic aliases in `ui/tokens.slint`.

Overlay, menu, drawer, header, and dialog thresholds use purpose-specific named
breakpoints in `ui/tokens.slint`. The validator requires zero local raw width or
height breakpoint comparisons outside the token source.

The optional work pane is not rendered when closed. The center surface expands
into the freed space rather than leaving an empty inspector column.

### Target mobile breakpoints

```text
small: 0-379 px
standard: 380-767 px
tablet: 768-1023 px
```

These values define the target mobile contract. The 0.1.0 draft does not ship or
claim a mobile artifact. On small and standard mobile, use one primary column
with bottom sheets. On tablet, allow a two-column session and detail layout when
there is enough room.

## Navigation and surfaces

### Current 0.1.0 draft navigation

The desktop shell has an icon-led global rail, a project and recent-session pane,
the main conversation canvas, an activity overlay, and a review summary that
docks only at its semantic breakpoint. Closing the review summary returns its
space to the conversation canvas. The current session pane does not provide
session search, pinning, or branch metadata.

### Target navigation and registered surfaces

The rail remains icon-led and contains no more than five primary destinations.
The project and session pane adds active branch or worktree data, session search,
recent sessions, pinned sessions, and running state. Settings do not appear as a
peer session item. Each task row may show one compact lifecycle marker, relative
update time, waiting-for-approval or unread state, and changed-file counts.
Search, pinning, and archive precede custom organization. Grouped, workspace,
and timeline views appear only when they can be backed by real persisted task
metadata and remain understandable without color alone.

The target work canvas is composed from registered surfaces:

- `conversation`
- `goal`
- `plan`
- `changes`
- `files`
- `terminal`
- `preview`
- `activity`
- `approvals`

Each registered surface declares a title, icon, minimum size, mobile
presentation, keyboard shortcut, and persistence key. A future layout manager
owns docking, splitting, resizing, and restoration. The 0.1.0 draft uses explicit
Slint layout state and does not claim a generalized surface registry or layout
manager.

## Component contracts

The current behavior and the target contract are separated below. Target
requirements remain release gates, not descriptions of unfinished behavior.

### App shell

The current shell provides project and session context, visible run state, a
single run or stop action, recent activity, review summary, approvals, and
responsive desktop layout.

The target shell must provide:

- current project and session context;
- visible run state;
- one primary action;
- a route to search, approvals, and running tasks;
- responsive layout without duplicated status strips.

### Session header

The current header shows session and project context, workspace context, run
state, and compact actions. The target header also shows branch or worktree when
applicable and provides a compact overflow menu. Provider and permission details
belong in the composer context menu, not a row of repeated status containers.

### Task and session navigator

The navigator is the supervision queue, not merely conversation history. The
target row contract includes title, project or workspace context when needed,
relative update time, lifecycle state, waiting-for-approval or unread state, and
optional changed-file counts. Running, waiting, failed, and unread states remain
distinguishable without relying on color.

The first release requires search, pin, archive, and restore. Custom groups,
drag ordering, workspace view, and timeline view are later scale features. A
group deletion never deletes its tasks. Automatic archival must be predictable,
reversible, and disabled until retention policy is implemented and tested.

### Goal summary

The goal surface shows the persisted objective, active, paused, budget-limited,
or complete state, elapsed time, resource use, iteration count, next action,
open requirements, and verification history. It groups work by the run or
iteration that produced it so later completion does not rewrite earlier history.

The goal surface provides pause, resume, replace, and clear actions. Stop pauses
an active goal. Clear preserves audit history. Completion is shown only after a
current evidence-backed verification event passes and all required work is
closed. The surface does not render private chain-of-thought or accept a model's
plain-text completion claim as state.

### Plan

The current draft shows bounded run stages and state. The target plan shows the
current step, completed steps, blocked steps, and the next decision. It remains
compact and must not expose private chain-of-thought.

### Tool event

The current activity surface shows bounded event summaries and state. The target
tool event is collapsed by default and shows tool name, safe summary, duration,
result state, and an expand control. Expanded content is redacted and bounded.
Long output is viewed in a dedicated surface.

### Approval card

The current approval card is visible while a request is pending. It shows the
action, target, and reason, with exact controls for Reject and Approve once. The
target contract adds displayed scope and expiration, session-wide approval, and
policy editing only after the runtime implements and verifies those choices.

### Changes

The current review summary shows changed file count, additions and removals, and
verification state. It does not claim a full diff viewer. The target Changes
surface adds a real diff with file navigation, comments, restore, and editor
handoff after those actions are implemented and verified.

The target file navigator can filter to changed files, shows added, modified,
deleted, and renamed state, and aggregates change state on directories. A file
can be previewed, opened in the configured editor, copied by path, or explicitly
added as task context. Adding context is visible and never uploads or transmits a
file without the active provider and permission boundary allowing it.

### Terminal and preview

The terminal is scoped to the exact project directory or worktree bound to the
session. It supports bounded command output, cancellation, background process
state, and a clear indication of which environment receives the command. The
agent can inspect terminal output only through the typed runtime boundary.

Preview supports files and local application surfaces without turning the app
into an editor clone. Browser, document, image, table, and diagram previews are
separate registered capabilities with explicit platform and permission state.
Unavailable formats say so directly.

### Composer

The current composer is the primary action surface. It contains:

- task input;
- attachment and context control;
- workspace and permission context;
- model and provider selector;
- run or queue action;
- cancel or stop state while running.

It must not expose internal configuration as a repeated status dashboard.

The target composer also exposes environment, branch or worktree, and effective
execution profile in one compact context row. It can create a short task or an
optional verifiable goal with resource limits. Goal controls move to the goal
surface after submission so the composer remains the primary input rather than
a permanent status dashboard.

### Execution profile

`Confirm Before Changes` is the default profile. `Auto Edit`, `Plan`, and `Full
Access` may appear only after the Rust capability broker implements their exact
semantics, persistence scope, and approval interaction. The current effective
profile stays visible while a task runs. Higher authority receives persistent
attention treatment, not promotional emphasis.

A pending permission request blocks accidental task submission and displays the
exact command, path, file action, network target, or tool operation. Allow once,
session-scoped allow, project-scoped allow, reject, and persistent deny controls
appear only when their scopes are enforced and auditable. Ordinary questions do
not automatically continue on timeout by default. Permission and plan approvals
never time out into consent.

### Command palette

The current command palette provides a bounded set of navigation and power
actions without adding permanent sidebar items. The target command center
searches commands, tasks, and files from one entry point. It makes every command
permission-aware and reports whether it is unavailable, disabled, or requires
approval. Search results name their scope and do not mix destructive actions
with passive navigation without clear separation.

## Target state patterns

Components must define each applicable state from this set before final release:

- empty;
- loading;
- ready;
- running;
- paused;
- budget limited;
- waiting for user;
- waiting for approval;
- success or complete;
- partial success;
- verification failed;
- failed;
- unavailable;
- offline;
- permission denied.

No component may render a success state from fixture data in a production path.
Goal completion also requires a current verification record and no open required
work. Expired budgets, unanswered questions, rejected approvals, and failed
verification remain visible non-success states.

## Motion and icons

### Motion

- micro interaction: 100-140 ms;
- menu and popover: 140-180 ms;
- sheet or pane: 180-240 ms;
- state transition: 220-300 ms;
- use a reduced-motion mode for every animated state.

Animated icons are reserved for running, syncing, loading, recording, and
attention states. A static icon is preferred for navigation and decoration.

### Icons

- 16 px for inline metadata;
- 18 px for standard controls;
- 20 px for primary navigation;
- 24 px for mobile sheet headers;
- stroke icons use a 1.7 px stroke and consistent optical weight;
- `more.svg` deliberately uses three filled circles because stroked micro-dots
  lose clarity at the 16 px inline size;
- every icon button has an accessible name and visible focus state.

The candidate generalized icon system uses pinned, unmodified Lucide SVGs under
`ui/assets/icons/lucide`. Exact upstream names, commit, licenses, and SHA-256
values are recorded in `ui/assets/icons/catalog.json`. These candidates remain
separate from the production `AgIcon` mapping until they receive visual
acceptance in the native gallery. Run the gallery with:

```powershell
cargo run --locked -p argentum-icon-gallery
```

The gallery renders the real desktop and mobile anchors with rest, hover,
focus, selected, and attention treatments. It is the icon acceptance surface,
not an image-generated mockup. Validate source integrity with
`scripts/validate-icon-system.ps1`.

## Accessibility

These requirements are release acceptance criteria. Draft status does not imply
that keyboard, screen-reader, contrast, touch, or reduced-motion verification is
complete.

- WCAG AA contrast for body text and controls;
- keyboard navigation for every desktop action;
- visible two-layer focus treatment using silver and Argentum red;
- minimum 44 px touch target on mobile;
- no color-only state communication;
- reduced-motion support;
- screen reader labels for icon-only controls;
- diff and activity content remains navigable without pointer hover;
- errors state what happened, what is safe, and what the user can do next.

## Native UI and token source

The production UI contains no TypeScript or JavaScript component runtime. UI
components are authored in Slint, while application and runtime state is owned
by Rust and projected into the UI. Platform-specific code is limited to the
selected renderer and small operating-system adapters.

The implementation uses one token source compiled with the native UI:

```text
ui/tokens.slint
  primitives
  semantic colors
  typography
  spacing
  radii
  motion
  breakpoints
```

Component code must not introduce new raw color, spacing, or breakpoint values
without adding a named token. Validator ceilings for raw spacing declarations
and raw local breakpoint comparisons are both zero.

## Target visual acceptance matrix

Before final release, every major surface must be reviewed at:

- 1440 x 900;
- 1280 x 800;
- 1024 x 768;
- 768 x 1024;
- 430 x 800;
- 360 x 740.

Each review includes empty, active, running, paused, budget-limited, waiting for
user, approval, verification failure, long content, offline, and reduced-motion
states. Reviews cover the task navigator, goal summary, execution profile,
composer, plan, approval, changed-file tree, diff, terminal, preview, activity,
and verification evidence. Narrow layouts must retain task state, authority, and
completion evidence without relying on hover or hidden color cues.

The premium bar is not “dark and polished.” The bar is clear hierarchy, visible
agent work, trustworthy state, excellent spacing, and no unnecessary surface.
