# Security Policy

## Supported versions

Argentum is pre-1.0. Security fixes target the current development branch and
the latest published release candidate. Older builds may not receive patches.

## Report a vulnerability

Email **report@ag064.eu** with the affected version, impact, and a minimal
reproduction. Do not include real credentials or private user data. Do not open
a public issue until the maintainer has assessed the report. You should receive
an acknowledgement within seven days; remediation timing depends on severity
and reproducibility.

## Security boundaries

Argentum is a local-first application, not a hardened multi-user isolation
boundary. It protects against accidental capability expansion and untrusted
model or tool input, but it cannot protect data from a compromised operating
system account, administrator, malicious dependency, or provider that receives
an authorized request.

Default behavior:

- Rust owns provider, tool, storage, security, and platform behavior.
- The shipped application has no JavaScript runtime or dynamic web content
  execution path.
- The persisted workspace policy is authoritative. UI state cannot grant a
  capability that the policy does not allow.
- Workspace paths are canonicalized and path traversal and symlink escapes are
  denied.
- File writes, shell commands, network access, and external processes require
  an explicit capability and approval decision.
- Provider requests use the Rust HTTP client with TLS and bounded request and
  response handling.
- Provider content, model output, tool arguments, attachments, and retrieved
  context are untrusted input and cannot authorize themselves.
- Secrets use the platform secure-storage boundary and zeroization helpers.
  They never enter SQLite event data or ordinary tracing output.
- Cancellation is propagated through provider, storage, and tool operations.
- Errors are redacted before they are persisted or presented outside the
  relevant workspace context.

Permission decisions come from local policy, not from prompt text, model output,
tool arguments, provider metadata, or remote content.

## Secret storage

Never store real credentials in configuration files, screenshots, bug reports,
model prompts, workspace event logs, or CI output. Android and Apple signing
private keys belong only in protected build environments and offline backups.
The local `release.keystore.b64` file and Android keystore paths are ignored and
must not be read, moved into source control, or included in a Rust build.

## Tools and extensions

The first release uses built-in Rust tools with typed capability descriptors.
There is no unconstrained plugin runtime. A future extension protocol must be
signed, capability-scoped, versioned, and disabled by default. Before activation
it must record source, revision, license, declared capabilities, network
destinations, executable hooks, and integrity information.

## Dependency and release integrity

Every change must pass the Rust format, check, test, clippy, advisory, license,
and source-policy gates. The repository policy is in `deny.toml`; CI runs
`cargo deny check`, and local release checks may also run `cargo audit`.

Windows releases must be built from the root Cargo workspace, copied to the
artifact directory with a SHA-256 sidecar, launched from a clean staging
directory, and checked for a native window before they are called ready. See
the scripts in `scripts/` and the release gates in `ROADMAP.md`.

## AI context and usage limits

Context estimates are not provider billing truth because tokenization differs by
model. Argentum must not invent remaining tokens, costs, resets, or plan limits.
Provider quotas are displayed only when returned by a response or an official
quota endpoint.
