# Security Policy

## Supported versions

Argentum is pre-1.0. Security fixes target the latest published release and the
current `development` release candidate. Older builds may not receive patches.

## Report a vulnerability

Email **agdroke064@gmail.com** with a description, affected version, impact, and
minimal reproduction. Do not include real credentials or private user data. Do
not open a public issue until the maintainer has assessed the report. You should
receive an acknowledgement within seven days; remediation timing depends on
severity and reproducibility.

## Security boundaries

Argentum is a local-first application, not a hardened multi-user isolation
boundary. It protects against accidental capability expansion and untrusted
model/tool input, but it cannot protect data from a compromised OS account,
administrator, malicious dependency, or provider that receives an authorized
request.

Default behavior:

- network listeners bind to loopback and use explicit CORS origins;
- optional features, dashboard, browser automation, computer control, cron, and
  auto-update are disabled by default;
- the persisted workspace configuration is the authority for context, channels,
  and security profile; a webview request can reduce but not expand it;
- model-call tools are selected by an allowlist and checked again at execution;
- `restricted` is the default profile;
- file read/write and loopback HTTP tools are exposed only with persisted
  `trusted` + `tool-state` configuration;
- loopback HTTP calls do not follow redirects, have a ten-second timeout, and
  stop reading after 48 KiB so local endpoints cannot pivot to public hosts or
  return unbounded model context;
- `ask` and `session` do not expose privileged model-call tools until a real
  interactive approval gate exists;
- workspace paths are canonicalized and path traversal/symlink escape is denied;
- hosted providers receive only the context categories enabled for the workspace;
- request size, history size, attachment rules, and a conservative model context
  budget are enforced in the Rust desktop bridge before a provider call;
- secrets and provider errors are redacted from application logs where supported.

Models and remote content are untrusted input. Prompt text, tool arguments,
plugin metadata, model cards, web pages, attachments, and retrieved context must
not be treated as authorization. Permission decisions come from local policy.

## Secret storage

The Node runtime includes an encrypted vault using AES-256-GCM or
ChaCha20-Poly1305 with an `ARGENTUM_MASTER_KEY`. The current desktop onboarding
path instead writes provider values to `workspace/secrets.env`; it is outside
YAML and ignored by Git, but is **not application-encrypted**. Its confidentiality
therefore depends on the local account/filesystem permissions. Do not share or
commit that file. Moving desktop secrets to OS secure storage/encrypted vault is
a release hardening item.

Never store real credentials in configuration, screenshots, bug reports, model
prompts, plugin manifests, or GitHub Actions logs. Android and Tauri release
signing private keys belong only in protected CI secrets and offline backups.

## AI context and usage limits

The desktop UI estimates context use and compacts at a threshold; the Rust bridge
also rejects oversized messages/history and requests over a conservative input
budget with an output reserve. Estimates are not provider billing truth because
tokenization differs by model. Provider quotas are displayed only when returned
by response headers/body or an official quota endpoint. Argentum must not invent
remaining tokens, costs, resets, or plan limits.

## Plugins, skills, MCP, browser, and computer use

Extensions are disabled until explicitly installed/enabled. Before activation,
Argentum must record source, immutable revision, license, declared capabilities,
network destinations, executable hooks, and integrity information. Unknown or
commercially incompatible licenses are blocked pending review. Computer/browser
control requires per-session targets, visible warnings, audit logs, and explicit
approval; it must remain off by default.

## Updates and release integrity

v0.0.9 checks GitHub Releases and opens the latest release page. It does not yet
perform an in-place install or rollback and must not report either as successful.
The planned updater requires signed artifacts, checksum/signature verification,
atomic platform installation, persistent key custody, and a recovery path. See
[docs/UPDATE_ARCHITECTURE.md](docs/UPDATE_ARCHITECTURE.md).

Android release APKs must use one persistent signing identity. CI fails closed
when signing secrets are missing; generating a new key for every release would
make installed builds non-upgradeable.

## Dependency tracking

Current scanner findings, exploitability notes, and temporary exceptions belong
in [SECURITY_DEPENDENCY_NOTES.md](SECURITY_DEPENDENCY_NOTES.md). Treat dates and
upstream-version statements there as snapshots and revalidate them against the
lockfiles and current advisories before a release.
