# Security Policy

**Last updated: 2026-03-23**

## Supported Versions

| Version  | Status        |
|----------|---------------|
| v0.2.x   | ✅ Active     |
| v0.1.x   | 🔒 Security fixes only |
| < v0.1   | ❌ Not supported |

## Reporting a Vulnerability

Found something that shouldn't be there? Here's what to do:

**DO NOT open a public GitHub issue for security problems.**

Instead, reach out directly:

- **GitHub Security Advisories** — Use the [Security Advisories](https://github.com/AG064/ag-claw/security/advisories/new) page (private, this creates a draft advisory without alerting everyone)

- **Email** — If you prefer: `@ag_064` on Telegram

### What to include

When reporting, try to include:

- Description of the issue
- Steps to reproduce (or a PoC if you have one)
- Affected component(s)
- Any mitigations you've already tried

### What to expect

- **Acknowledgment** — within 24–48 hours
- **Initial assessment** — within 3–5 days
- **Fix timeline** — depends on severity, but high-severity gets priority
- **Credit** — if you want it, let me know how to credit you in the release notes

## Scope

AG-Claw handles sensitive data (messages, credentials, files). Security means:

- **No data leaves your machine** unless you explicitly configure it to
- **Secrets stay encrypted** in memory and at rest
- **Sandboxed execution** for untrusted code or agents
- **Audit logging** for sensitive operations

That said, AG-Claw is a framework — the security of your deployment also depends on how you configure it (network access, plugin permissions, credential storage, etc.).

## Security Features Built In

These are already implemented — you don't need to configure anything:

- Input sanitization (XSS prevention)
- Rate limiting (API endpoints and agent interactions)
- JWT-based authentication for web endpoints
- Credential manager with short-lived keys
- Container sandboxing for agent execution
- Audit logging for all sensitive operations
- Webhook SSRF protection
- Allowlist-based permission system (default-deny)

## Known Limitations

- AG-Claw runs on your local network — if your machine is compromised, the agent has your access level
- Plugins you install yourself are not audited — only use trusted sources
- If you expose the web dashboard publicly, use strong credentials (don't rely on the default setup for public deployments)

## Dependencies

Security vulnerabilities in third-party packages are tracked via GitHub Dependabot and resolved as soon as patches are available.
