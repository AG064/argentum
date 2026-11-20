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

## Planned for 0.1.1

- **Lighter requests:** shorter tool descriptions and only the tools a session
  needs, to roughly halve the fixed cost of each request.
- **Faster compaction:** old tool output is trimmed first, with no model call,
  and compaction starts before the context is full.
- **Exact token counts:** counting with each provider's own tokenizer where one
  is available, and learning from the usage each provider reports.
- **Clearer diagnostics:** runs that stop for an approval, and the tokens a
  compaction used, show up in run details and usage totals.
- **Interface polish:** notices that no longer cover the composer, clearer
  wording in Settings, a history of interface changes you can undo, and long
  session titles that no longer run into their menu button.
- **Session list:** each session shows whether it is working, waiting for you,
  unread, and how many files it changed.
- **Android Back button:** Back closes panels, settings and menus before it
  leaves the app.
- **Phone text fields:** server addresses and model names are not
  auto-capitalized by the keyboard.
- **Dependencies:** replace the unmaintained chrono crate, used for local reply
  times, with jiff, and take the major updates of the crypto and encoding crates
  (argon2, chacha20poly1305, sha2, getrandom, base64) after their own review,
  and Android Gradle plugin 9.5 with Gradle 9.9, once their checksums are added
  to the Android build's dependency verification.
- **Plan:** the real stages of the current task as the agent plans them, in the
  app's Plan module and in `argentum chat`. Until then the Plan module is listed
  as coming soon.
- **Files panel:** browse the workspace's files and the files a session wrote,
  from the side panel.
- **Questions from the agent:** the agent asks you when something is unclear and
  waits for your answer before it goes on.
- **Search in sessions:** find a sentence or a message in past sessions, from an
  index that stays fast as history grows.
- **Changes panel:** every file a session changed, not only the latest run's.
- **Phone touch:** larger touch targets, a clearer context meter, and fast
  swipes that always scroll a long session.
- **Run details for the agent:** the model that actually answered, why a run
  stopped, and tool output that stays available after the turn that made it.
- **Usage across restarts:** a session's context use and token totals kept when
  the app closes and opens again.
- **Context breakdown:** what fills the context window, by part: messages,
  system prompt, tool descriptions, skills, project instructions and free
  space, with how much is left before compaction starts.
- **Provider limits and balances:** each provider's own usage limits where it
  reports them, such as requests and tokens per minute or a subscription's
  usage window, with how much is left and when it resets; the account balance
  for providers that report one, such as DeepSeek and OpenRouter; and a spend
  limit you set for each provider, which warns as it gets close and stops new
  requests when it is reached.
- **Execution environment:** a plain description, for the agent and for you, of
  what the shell can reach on each platform.

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
- **System prompt presets:** saved instructions you can pick for a session, such
  as a reviewer or a careful refactorer.
- **Interface:** a refreshed interface for desktop and phone, including
  two-handle text selection on Android.
- **Agent-made widgets:** the agent adds its own widgets to the interface and
  the conversation, such as a progress bar for a long task in colors it picks,
  with the same checks and approvals as other interface changes.
- **Android:** working with folders on shared storage, and Termux as an optional
  shell.
- **Platforms:** release files for Linux and macOS, then an iOS app.
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
- **Phone notifications:** a notice when a run finishes or an approval waits.
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
