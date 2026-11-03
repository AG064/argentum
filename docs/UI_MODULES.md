# Argentum interface: modules, customization, and updates

Status: 0.1.0 draft contract for the native Workbench shell in `ui/workbench/`.

## Shape of the interface

The core is always present and has no switch:

- the conversation and composer;
- the provider and model chooser;
- the approval prompt.

Everything else is an optional module. A new project starts with none of them, so
the first screen is the provider chooser and the next is a conversation. Modules
are harness surfaces (`SurfaceId`), so availability, visibility, and reasons follow
the rules in [the modular harness contract](HARNESS_MODULARITY.md).

| Module ID | Region | What it adds |
| --- | --- | --- |
| `sessions` | Sidebar | Sessions for this workspace, new chat, workspace entry |
| `context-meter` | Composer | Context use for the latest turn, when reported |
| `reasoning` | Message | Collapsed provider reasoning |
| `goal` | Above composer | Session objective, budgets, next action |
| `command-palette` | Overlay | Ctrl+K search over commands, modules, themes, sessions |
| `status-bar` | Bottom | Workspace, run state, changed files, provider |
| `changes` | Side panel | Files the agent changed, per-file diff, restore |
| `git` | Side panel | Working tree status and diff from Git |
| `browser` | Side panel | Reader-mode pages |
| `usage` | Side panel | Tokens reported per provider |
| `plan` | Side panel | Plan stages for the current task |
| `activity` | Side panel | Recent run, tool, and approval events |
| `trajectory` | Side panel | Durable records for one session |

`files`, `terminal`, and `preview` are registered as unavailable with a factual
reason. They stay visible in Settings as not in this build.

Module sets (`module_profile`) switch several at once: `focused` (shown as
Minimal), `standard`, `review`, `trace`, and `full` (shown as Everything). A layout
saved before modules existed keeps what used to be permanent: sessions, the context
meter, reasoning, and the command palette.

## A module never grants authority

Turning a module on changes what is shown. The capability behind it is decided
separately, by the persisted project policy.

- The `git` module runs `git` only after a click, and only when
  the `git.read` capability allows it. Otherwise it explains the block and links
  to Permissions. Diff requests accept only paths from the last status.
- The `browser` module fetches only after a user action, and only when
  the `browser.reader` capability allows it. It is off by default.

### What the Browser module is

The Browser is a reader, not a web engine. It fetches a page once over http or
https, keeps no cookies, runs no script, and reduces the page to text blocks.
`SECURITY.md` promises that the shipped application has no JavaScript runtime or
dynamic web content execution path, and this keeps that promise. Sites that need
JavaScript will not work in it. Page content stays in a transient event, is never
stored, and never reaches a model. Links in chat open here only on a direct click
and never hand off to another program.

An embedded engine such as WebView2 would need that promise changed first.

### What Git cannot fully prevent

Git commands run with fsmonitor, external diff, and textconv turned off. Filter
drivers defined in a repository's own config can still run during `git diff`, as
they would from a terminal. This is why the module stays behind the
external-process key.

## Customization

Appearance is a validated document, `UiPreferences`, stored once per installation
so it follows the user between workspaces. It holds the theme preset, color token
overrides, text size, corner roundness, density, fonts, column widths, panel side,
message bubble style, and reduced motion.

Every change is a `UiPatch`. The settings page, the JSONL protocol, and the agent
all send the same type down the same path, and a patch that fails validation
changes nothing. Validation enforces value ranges, known tokens only, and contrast:
primary and secondary text need 4.5:1 on their backgrounds, quiet text and the
accent 3:1. Translucent colors are composited first, so alpha cannot slip through.

## How the agent sees and changes the interface

The model gets two tools. Both are configurable in Settings under Permissions.

| Tool | Approval | Purpose |
| --- | --- | --- |
| `ui_inspect` | None | Every module and its state, the layout, every token with its role and value, presets, and the rules |
| `ui_update` | One per change | Apply a `UiPatch`, including modules |

Rules the agent cannot get around:

- `ui_update` shows a plain summary in the approval prompt, built from validated
  fields and never from free model text. Afterwards the app offers Undo.
- A bad patch is refused before any prompt, and the reason goes back to the model.
- The approval prompt uses fixed colors (`TrustTheme`), cannot be hidden, and paints
  above everything else. The conversation cannot be hidden either.
- There is no field for permissions, and none for interface text. The agent cannot
  mark first run complete, and it cannot reach updates.
- Read Only turns `ui_update` off along with writes.
- `ui_inspect` returns presentation facts only: no conversation text, paths,
  endpoints, or credentials.

## First run

1. Choose a provider from the catalog. The short list covers MiniMax, DeepSeek,
   OpenAI, ChatGPT, local llama.cpp, and a custom server, and "Show all" opens the
   rest. A provider that needs a key asks for it next. The key goes straight to the
   system credential store and the field is cleared.
2. Optionally turn on modules, or go straight to the conversation.

A workspace already using another provider skips this. Anthropic and unknown
provider kinds stay out of first run, as the first-run contract requires.

## Updates

Updates live in Settings rather than in a module, because keeping up to date should
not depend on which parts of the interface are on. Only the person clicking can
start a step. No tool, model, or protocol client can reach it, and Argentum does not
contact GitHub in the background.

1. **Check for updates** reads the latest published release from GitHub.
2. **Download** fetches `Argentum.exe` and `SHA256SUMS.txt` from that release. Assets
   must be https URLs on `github.com`, and a file that does not match its checksum is
   discarded.
3. **Install and restart** moves the running program aside, puts the new one in place,
   and starts it. If any step fails, the previous version is restored.

Development builds and copies running from a `target` folder never replace themselves.

The checksum proves a file arrived intact. It does not prove who built it. Releases
are not code signed yet, so authenticity rests on TLS to GitHub and on control of the
release account. A signature check belongs here before this is relied on outside
engineering previews.

## Running beside another build

A database written by a newer build cannot be opened by an older one, and all builds
share one data folder by default. Set `ARGENTUM_DATA_DIR` to an absolute path to give
a build its own folder. Appearance, sessions, and provider profiles live there. API
keys stay in the operating system's credential store, which is shared.

## Visual QA

`cargo run -p argentum-ui --example ui_snapshot -- OUT_DIR [FILTER]` renders every
shell state to PNG with the software renderer, with no window and no host. Its
fixture data exists only in that example. This is a review aid. It uses a different
text renderer from the shipped femtovg backend, so it does not replace checking the
real application.
