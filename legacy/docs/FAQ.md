# Frequently Asked Questions

This file is the curatable source for Argentum's in-app FAQ. Answers describe
v0.0.9 release behavior unless a later version is named.

## Does Argentum upload my workspace automatically?

No. Argentum is local-first and does not sync a workspace automatically. Text,
attachments, and selected context are sent to the configured AI provider only
when you make a provider request. Review the provider's data policy before use.

## What information does Argentum collect?

Argentum has no background analytics or automatic workspace upload. Workspace
files, chats, logs, configuration, and model files stay local unless you use a
configured provider, channel, model service, or user-initiated bug report. GitHub
release checks do not send workspace contents. See the full [privacy and data
handling note](PRIVACY.md) for the exact boundaries.

## Which model-call permissions are enabled by default?

The default profile is `restricted`. Model-call tools are deny-by-default, and
the persisted workspace configuration is authoritative. File read/write and
loopback HTTP tools are exposed only by the `trusted` profile with tool context
enabled. Interactive approvals for `ask` and `session` profiles are not yet
implemented, so those profiles do not expose privileged tools.

## Can I run models locally?

Yes. The desktop app can manage a localhost llama.cpp server. Choose a curated
Hugging Face GGUF preset, search the Hub for GGUF repositories, browse to a GGUF
file, or scan `workspace/models`. Other discovered formats are inventory-only;
llama.cpp launch requires GGUF.

## Does Argentum install updates inside the app?

Not yet. v0.0.9 checks GitHub Releases and opens the latest release page. Signed,
atomic in-place installation and rollback are release-planned work; the app does
not claim an update succeeded when it only opened a download page.

## Is Android ready?

The Android client is pre-release and intended for testing on the road to v0.1.0.
It is not yet a substitute for the desktop app, and cross-device workspace/chat
sync is planned for v0.1.1.

## How do I report a bug?

Open **Settings → Help & Feedback**. The bug-report actions open a GitHub issue or
an email draft with diagnostics, app version, platform, and workspace-presence
status. They do not include the local workspace path by default. For a security
vulnerability, follow [SECURITY.md](../SECURITY.md) instead of filing a public issue.

## Are scheduled tasks available?

The backend cron-scheduler feature exists as an optional, disabled module. There
is no supported desktop scheduling UI in v0.0.9. A permission-gated task UI and
operational safety controls are planned after v0.1.0.
