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
- replies that read in the order they were written, each step's thinking
  followed by the answer it wrote;
- steering notes that show in the chat with their state, approvals that show
  the full command, and shell failures that report why they failed.

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
- **Plan in the terminal:** the stages of the current task in `argentum chat`,
  as the Plan module shows them in the app.
- **Changes panel:** every file a session changed, not only the latest run's.
- **Phone touch:** larger touch targets, a clearer context meter, and fast
  swipes that always scroll a long session.
- **Run details for the agent:** the model that actually answered, why a run
  stopped, and tool output that stays available after the turn that made it.
- **Usage across restarts:** a session's context use and token totals kept when
  the app closes and opens again.
- **Execution environment:** a plain description, for the agent and for you, of
  what the shell can reach on each platform.

## Planned for 0.2.0

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
