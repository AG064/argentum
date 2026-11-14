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
- pairing a phone with a computer to drive one from the other.

## Planned for 0.1.1

- **Lighter requests:** shorter tool descriptions and only the tools a session
  needs, to roughly halve the fixed cost of each request.
- **Faster compaction:** old tool output is trimmed first, with no model call,
  and compaction starts before the context is full.
- **Exact token counts:** counting with each provider's own tokenizer where one
  is available, and learning from the usage each provider reports.
- **Clearer diagnostics:** runs that stop for an approval, and the tokens a
  compaction used, show up in run details and usage totals.
- **Interface polish:** notices that no longer cover the composer, and clearer
  wording in Settings.

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
- **Platforms:** release files for Linux and macOS.
- **Signing:** code-signed Windows releases.
- **Extensions:** MCP servers and portable skills in the standard build, with the
  same approvals as built-in tools.
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
