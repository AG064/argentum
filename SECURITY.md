# Security

AG-Claw is the only AI agent framework with defense-in-depth security from the ground up.

---

## Security Features

| Feature | What it does | Why it matters |
|---------|-------------|-----------------|
| AES-256 encryption | Encrypts credentials at rest | Your API keys are never stored in plaintext |
| Audit logging | Logs every sensitive operation | Know exactly what your agent did and when |
| Rate limiting | Limits API calls per endpoint | Prevents accidental or malicious resource exhaustion |
| Allowlist mode | Default-deny, explicit allow | Only approved commands/tools can run |
| Policy engine | Configurable permission rules | Define what agents can and cannot do |
| Container sandbox | Isolate untrusted agent code | Agent code runs in isolation |
| SSRF protection | Blocks webhook DNS rebinding | Prevents webhook-based internal access |
| Credential manager | Short-lived key rotation | API keys expire and rotate automatically |

---

## How It Works

**1. Your credentials are encrypted from the moment they enter the system.**

AES-256-GCM encryption means API keys, tokens, and secrets never exist in plaintext on disk or in memory longer than necessary.

**2. Every operation is logged and traceable.**

The audit log records tool calls, channel messages, configuration changes, and agent decisions with timestamps and user context. You can replay any session.

**3. Access is controlled by explicit policy.**

Default-deny allowlists mean nothing runs unless you explicitly permit it. The policy engine lets you define fine-grained rules: which users can access which tools, which channels are allowed, what rate limits apply.

---

## Comparison

| | AG-Claw | OpenClaw | LangChain | CrewAI |
|--|---------|----------|-----------|--------|
| Encrypted secrets | ✅ AES-256 | ❌ | ❌ | ❌ |
| Audit logging | ✅ Full | ⚠️ Token only | ❌ | ❌ |
| Rate limiting | ✅ Configurable | ❌ | ❌ | ❌ |
| Allowlists | ✅ Default-deny | ❌ | ❌ | ❌ |
| Policy engine | ✅ YAML rules | ❌ | ❌ | ❌ |
| Container sandbox | ✅ | ❌ | ❌ | ❌ |
| SSRF protection | ✅ | ❌ | ❌ | ❌ |

---

## Get Started

See [SECURITY.md](./SECURITY.md) (this file) or the [User Guide](./docs/USER_GUIDE.md) for configuration examples.

---

## Reporting Security Issues

Found a vulnerability? Do not open a public issue. Instead:

- **GitHub Security Advisories**: [Report privately](https://github.com/AG064/ag-claw/security/advisories/new)
- **Telegram**: [REMOVED]

Expected response: acknowledgment within 24–48 hours, fix timeline based on severity.
