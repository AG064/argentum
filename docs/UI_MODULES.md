# Interface modules

## Shape of the interface

The core is always present and has no switch:

- the conversation and composer;
- the provider and model chooser;
- the approval prompt.

Everything else is an optional module. New projects start in the Minimal profile,
with only collapsed reasoning on, so the first screen is the provider chooser and
the next is a conversation. Modules are harness surfaces (`SurfaceId`).

| Module ID | Region | What it adds |
| --- | --- | --- |
| `sessions` | Sidebar | Sessions for this workspace, new chat, workspace entry |
| `context-meter` | Composer | Context use for the latest turn, when reported |
| `reasoning` | Message | Provider reasoning, a one-line step summary, and changed files with diffs on each response |
| `goal` | Above composer | Session objective, state, progress against limits, Pause, Resume and Clear. Composer tasks run toward an active goal |
| `command-palette` | Overlay | Ctrl+K search over commands, modules, themes, sessions |
| `status-bar` | Bottom | Workspace, run state, changed files, provider. Desktop only |
| `changes` | Side panel | Files the agent changed, per-file diff, restore |
| `git` | Side panel | Working tree status and diff from Git |
| `browser` | Side panel | Reader-mode pages |
| `usage` | Side panel | Tokens reported per provider |
| `plan` | Side panel | Plan stages for the current task |
| `activity` | Side panel | A live list of requests, tools, approvals, and checks |
| `trajectory` | Side panel | The saved record of one session |
| `requests` | Side panel | Every model request in this session: when, how long, which endpoint, and what filled its context. Kept for this session only |

`files`, `terminal`, and `preview` are listed in Settings as not in this build.

Module sets (`module_profile`) switch several at once: `focused` (shown as
Minimal), `standard`, `review`, `trace`, and `full` (shown as Everything). The
Modules page shows what the chosen set includes.

Activity and Trajectory are filled by services of the same names under Settings >
Capabilities > Harness services. Turning either module on, by itself or with a set,
starts its service too. Both only read the session's own records.

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
Argentum has no JavaScript runtime and no path that runs web content
(see `SECURITY.md`). Sites that need JavaScript will not work in it. Page content stays in a transient event, is never
stored, and never reaches a model. Links in chat open here only on a direct click
and never hand off to another program.

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

The model gets two tools.

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
- `ui_inspect` returns presentation facts only: no conversation text, paths,
  endpoints, or credentials.

## First run

1. Choose a provider from the catalog. The short list shows the providers that have
   been tested, local llama.cpp, and a custom server. "Show all providers" opens the
   rest. A provider that needs a key asks for it next. The key goes straight to the
   system credential store and the field is cleared. "Continue without a provider"
   skips this; you can add one later in Settings > Providers.
2. Optionally pick a module set or turn on modules one by one, or go straight to the
   conversation.

A workspace already using a provider skips this.

## Updates

Updates live in Settings, not in a module. Only the person clicking can
start a step. No tool, model, or protocol client can reach it, and Argentum does not
contact GitHub in the background.

1. **Check for updates** reads the latest published release from GitHub.
2. **Download** fetches `Argentum.exe` and `SHA256SUMS.txt` from that release. Assets
   must be https URLs on `github.com`, and a file that does not match its checksum is
   discarded.
3. **Install and restart** moves the running program aside, puts the new one in place,
   and starts it. If any step fails, the previous version is restored.

Development builds and copies running from a `target` folder never replace themselves.

The checksum proves a file arrived intact. It does not prove who built it, because
the Windows files are not code signed. To check where a file was built, run
`gh attestation verify FILE --repo AG064/argentum` on it.

## Running beside another build

A database written by a newer build cannot be opened by an older one, and all builds
share one data folder by default. Set `ARGENTUM_DATA_DIR` to an absolute path to give
a build its own folder. Appearance, sessions, and provider profiles live there. API
keys stay in the operating system's credential store, which is shared.

## Screenshots for contributors

`cargo run -p argentum-ui --example ui_snapshot -- OUT_DIR [FILTER]` renders every
shell state to PNG with the software renderer, with no window. Its sample data
exists only in that example. It uses a different text renderer from the app, so
check changes in the running app too.
