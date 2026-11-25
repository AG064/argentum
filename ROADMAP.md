# Roadmap

What Argentum has today, and where it goes next. Plans can change, and nothing
here is a promise of a date.

## In 0.1.0

The first stable release, for Windows x64 and Android arm64:

- the desktop and Android apps, with optional modules and five themes;
- Argentum Genesis, the CLI, with chat, headless runs and a JSONL server;
- more than 40 provider presets, local models and an optional local model add-on;
- the project policy, approvals, guarded file changes, restore and recovery;
- goals with limits, and context compaction;
- pairing a phone with a computer to drive one from the other;
- replies that read in the order they happened: each step's thinking, the
  answer it wrote, then the tools it ran, each group at its own point, folded
  into a "Worked for" block above the final answer when the run finishes;
- steering notes that show in the chat with their state, approvals that show
  the full command, and shell failures that report why they failed;
- a rejected approval goes back to the agent as an answer, so it carries on with
  what it can do or tells you what was rejected;
- goal reports that say whether a goal is done, partly done, or needs your
  input;
- starting normally after a crash or a forced close, with no recovery command;
- opening another workspace in the same window, without restarting the app.

## In 0.1.1 (on the development branch, not released)

- **Lighter requests:** shorter tool descriptions, and Git and Rust check tools
  offered only in a Git repository or a Cargo project. The built-in tool list
  is about a third smaller, and about half the size in a plain folder.
- **Faster compaction:** when a request fills 85% of the context window, older
  tool output is cut to its first lines with no model call, before the window
  is full. A request that no longer fits still gets the model summary.
- **Clearer diagnostics:** run details say why a run stopped, including for an
  approval or a question, and the tokens its compactions used. Usage totals
  include compaction.
- **Interface polish:** notices sit under the header instead of over the
  composer, Settings reads more plainly, Appearance keeps a history of
  interface changes with an Undo for each, and long session titles stop before
  their menu button.
- **Session list:** each session shows whether it is working, waiting for you
  or unread, and how many files it changed.
- **Android Back button:** Back closes menus, sheets, Settings, the drawer and a
  panel over the chat before it leaves the app.
- **Phone text fields:** server addresses, model names, paths and ids are not
  capitalized or corrected by the keyboard.
- **Dependencies:** jiff replaces the unmaintained chrono crate for local reply
  times.
- **Plan:** the stages of the current task as the agent plans them, in the
  app's Plan module and in `argentum chat`.
- **Files panel:** browse the workspace's files, open one as text, and see the
  files this session wrote, from the side panel. Paired devices cannot use it.
- **Questions from the agent:** the agent can ask you a question, with choices,
  and the run waits: your answer is your next message.
- **Search in sessions:** find a sentence in past sessions, from a full-text
  index, with the matching words shown under the session.
- **Changes panel:** every file a session changed, across all of its runs.
- **Phone touch:** taller tap targets, the context share shown beside its ring,
  and fast swipes that always scroll a long session.
- **Run details for the agent:** the model that answered and why the run
  stopped.
- **Usage across restarts:** the context meter shows the latest reported use
  after the app opens again, in the app and in `argentum chat`.
- **Linux and macOS:** the desktop app and the CLI build and pass their tests
  on Linux x64 and macOS (Apple silicon and Intel) in CI, and the release
  workflow makes their files: tarballs, a `.deb` for Linux, and an unsigned
  `Argentum.app`. Commands run in their own process group, without the Windows
  job limits. Supplemental Rust checks, the local model add-on, desktop control
  and installing updates from the app stay Windows-only for now. See
  [the support matrix](docs/SUPPORT_MATRIX.md#linux-and-macos).
- **Pull requests:** a GitHub Action (`action.yml`) that runs Argentum headless
  on a pull request, to review it read-only or to fix a failing check, and posts
  a comment or review suggestions. Changing files and pushing each need an
  explicit setting in the workflow file. See
  [Argentum on pull requests](docs/CI_PULL_REQUESTS.md).
- **Execution environment:** a plain description of what commands can reach on
  each platform, for the agent and in Settings > Permissions.
- **Tool output kept:** directory listings and file metadata stay available to
  the agent in later turns, shortened to their first and last lines. Command,
  shell and Git output stay private to the turn that made it.
- **Context breakdown:** what fills the context window, by part: messages,
  system prompt, tool descriptions, skills, project instructions and free
  space, with how much is left before compaction starts. Counts come from the
  usage each provider reports, and are marked as estimates until it does.
- **Account balances:** the Usage panel reads the balance of a DeepSeek or
  OpenRouter account when you ask, with the key you saved for it.
- **Spend limit:** a monthly token limit for each provider, which warns as it
  gets close and stops new requests when it is reached.
- **Dependencies:** argon2 0.6, chacha20poly1305 0.11, getrandom 0.4 and base64
  0.23. Data stored by 0.1.0 stays readable.
- **No telemetry check:** CI fails if an analytics, telemetry or
  crash-reporting dependency or host appears, and `SECURITY.md` states the
  rule.

## Planned for 0.1.1

- **Exact token counts:** counting with each provider's own tokenizer where one
  is available offline.
- **Provider limits:** each provider's own usage limits where it reports them,
  such as requests and tokens per minute or a subscription's usage window,
  with how much is left and when it resets.
- **Dependencies:** sha2 0.11, and Android Gradle plugin 9.5 with Gradle 9.9,
  once their checksums are added to the Android build's dependency
  verification.

## Planned for 0.1.5

Larger tool limits, faster long sessions, and clearer control over how models
are used, including ideas from Claude Code and Codex.

### Speed measured first

- **Benchmarks before each release:** long-session tests that time requests,
  drawing, memory and startup, so every change below is checked against numbers.

### Tools and limits

- **Larger limits:** limits that grow with the model's context window, such as
  reading about 2,000 lines at a time, writing files up to 1 MB, and searching
  the whole workspace page by page instead of stopping at a fixed number of
  files.
- **Long output kept:** output past about 30 to 50 KB is saved to a file the
  agent can read in pages, instead of being cut off.
- **Long commands:** a command that runs past its time limit carries on in the
  background, and the agent checks on it later.
- **Run budgets:** a run stops at a token, time or cost budget you set, in place
  of the fixed limit of 120 model requests per run.
- **Helpers:** the agent can hand part of a task to a helper with its own
  context window and budget, and gets back only the result.
- **Limits page:** Settings shows every limit and lets you change it, and
  `argentum` accepts the same settings from a file and on the command line.

### Model use

- **Prompt caching:** requests mark the parts that stay the same, so providers
  that cache can reuse them, which makes long sessions cheaper and faster.
- **Parallel reads:** the agent reads and searches several files at once.
- **Smaller requests:** request bodies are compressed for providers that accept
  it.
- **One picker for model and thinking:** the model chooser sets the thinking
  level for every provider that has levels, not only DeepSeek, and asks whether
  a switch is for this session or the new default.
- **A model for side work:** a cheaper model of your choice writes titles,
  recaps and compaction summaries.
- **Plan and work models:** one model plans the task and another carries it
  out.
- **Retries you can see:** the app shows "Retrying in 12s, attempt 2 of 6" and
  waits as long as the provider asks, up to 2 minutes. Spend limits and plan
  limits are never retried.
- **At a usage limit:** a warning as a provider's limit gets close, and when it
  is reached a choice to wait and carry on when it resets, switch model, or
  stop. Waiting still keeps to goal budgets, the spend limit and approvals.
- **Fallback model:** when a model is overloaded or retired, the run moves to
  another model from the same provider and says so. Falling back to a different
  provider happens only if you set it up, because it sends the conversation to
  another company. Switching model never changes permissions.

### Seeing and controlling usage

- **Usage while you work:** the context meter shows how much is left and how far
  it is until compaction, and the line under a working run shows its time and
  tokens so far.
- **Session cost:** a session's usage shows tokens by model, cache hits, and an
  estimated cost from prices you can edit.
- **Status bar:** you choose what it shows, such as model, thinking level,
  context left, cost, limits and cache hits.
- **Context control:** compact with a note on what to keep, summarize from or up
  to a chosen message, and set where compaction starts for each model.
- **Recaps:** a short summary of a session without compacting it, and one when
  you come back to a session after time away. You can turn them off.
- **Rewind:** from any message, go back with its files and conversation, the
  conversation only, or the files only, then edit that prompt and send it again.
  Changes made by shell commands cannot be restored, and the menu says so.
- **Side questions:** ask something quick that does not stay in the session.
- **Drafts kept:** an unsent or cleared draft can be brought back.

### Speed

- **Long sessions:** the conversation draws only the messages on screen,
  streamed text is added in place instead of copying the whole reply, and
  session history loads in pages.
- **Storage:** database work runs off the interface thread, with one connection
  for writing and a pool for reading.
- **Startup and idle:** less work when the app starts and while it sits idle.

## Planned for 0.2.0

- **Accessibility:** every control reachable by keyboard and named for screen
  readers on Windows and Android, visible focus everywhere, text that scales,
  contrast that meets WCAG 2.1 AA in every theme, and reduced motion respected.
- **Languages:** every interface string translatable, with the language chosen
  in Settings or taken from the system, dates and numbers in the local format,
  and layouts that hold up with longer text.
- **Marketplace:** browse and install add-ons, such as modules, themes, skills and
  MCP servers, and local models from inside the app, with each item's source,
  permissions and checksum shown before it installs.
- **Selecting reply text on a phone:** pick part of a reply with two handles,
  instead of copying the whole reply.
- **Checks on the phone:** a way for the agent to run and verify its work on
  Android, as it can on Windows.
- **Undo for tools:** step back an action the agent took, beyond restoring files.

## Planned for future builds

- **Computer use:** the agent works in desktop applications, with each action
  shown and approved.
- **Browser:** a full web browser in the side panel, with normal pages and
  sign-in, in its own profile separate from your usual browser. Until then the
  Browser module is listed as coming soon.
- **Browser use:** the agent opens and works in web pages, with the same
  approvals as other tools.
- **Customizable themes:** create your own themes, beyond the five built in.
- **Longer sessions:** sessions that keep working well over many more turns
  before they need compaction.
- **Saved setups:** presets of model, thinking level, permission profile and
  instructions that you pick for a session in one click, such as a reviewer or
  a careful refactorer.
- **Interface:** a refreshed interface for desktop and phone, including
  two-handle text selection on Android.
- **Agent-made widgets:** the agent adds its own widgets to the interface and
  the conversation, such as a progress bar for a long task in colors it picks,
  with the same checks and approvals as other interface changes.
- **Android:** working with folders on shared storage, and Termux as an optional
  shell.
- **Platforms:** Windows features on Linux and macOS (resource limits for
  commands, supplemental Rust checks, the local model add-on, installing
  updates from the app), a signed and notarized macOS app, then an iOS app.
- **Signing:** code-signed Windows releases.
- **Extensions:** MCP servers and portable skills in the standard build, with the
  same approvals as built-in tools.
- **Images:** attach screenshots and pictures to a message, for models that can
  read them.
- **Web fetch:** a tool that lets the agent read a web page as text, with
  approval, using the same reader as the Browser module.
- **Terminal sessions:** long-running commands the agent can start, read and
  type into, such as a dev server or a test watcher.
- **Stronger verification:** checks that a verified file is not a link and is
  not changed while it is checked.
- **Notifications:** a notice on the desktop and the phone when a run finishes
  or an approval waits.
- **Voice:** talking to the agent and having replies read aloud.
- **Benchmarks:** published results against other coding agents on public task sets.

More will be added as it is planned.

## Support the project

Argentum is free and stays free. Support pays for Windows code signing, a Google
Play listing and test devices:
[GitHub Sponsors](https://github.com/sponsors/AG064),
[Patreon](https://www.patreon.com/ag064) and [Ko-fi](https://ko-fi.com/ag064).

## Release process

How a release is built, checked and published is in
[docs/RELEASING.md](docs/RELEASING.md).
