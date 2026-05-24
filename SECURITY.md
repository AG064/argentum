# Security

Argentum is the only AI agent framework with defense-in-depth security from the ground up.

---

## Security Features

| Feature            | What it does                   | Why it matters                                       |
| ------------------ | ------------------------------ | ---------------------------------------------------- |
| AES-256 encryption | Encrypts credentials at rest   | Your API keys are never stored in plaintext          |
| Audit logging      | Logs every sensitive operation | Know exactly what your agent did and when            |
| Rate limiting      | Limits API calls per endpoint  | Prevents accidental or malicious resource exhaustion |
| Allowlist mode     | Default-deny, explicit allow   | Only approved commands/tools can run                 |
| Policy engine      | Configurable permission rules  | Define what agents can and cannot do                 |
| Container sandbox  | Isolate untrusted agent code   | Agent code runs in isolation                         |
| SSRF protection    | Blocks webhook DNS rebinding   | Prevents webhook-based internal access               |
| Credential manager | Short-lived key rotation       | API keys expire and rotate automatically             |

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

|                   | Argentum        | OpenClaw      | LangChain | CrewAI |
| ----------------- | --------------- | ------------- | --------- | ------ |
| Encrypted secrets | ✅ AES-256      | ❌            | ❌        | ❌     |
| Audit logging     | ✅ Full         | ⚠️ Token only | ❌        | ❌     |
| Rate limiting     | ✅ Configurable | ❌            | ❌        | ❌     |
| Allowlists        | ✅ Default-deny | ❌            | ❌        | ❌     |
| Policy engine     | ✅ YAML rules   | ❌            | ❌        | ❌     |
| Container sandbox | ✅              | ❌            | ❌        | ❌     |
| SSRF protection   | ✅              | ❌            | ❌        | ❌     |

---

## Get Started

See [SECURITY.md](./SECURITY.md) (this file) or the [User Guide](./docs/USER_GUIDE.md) for configuration examples.

---

## Security Risks & Threat Model

Understanding potential threats helps manage risk effectively. Below is our assessment of the most likely and impactful security problems in Argentum.

### Credential & Secret Risks

| Risk                                   | Likelihood | Impact   | Mitigation                                                         |
| -------------------------------------- | ---------- | -------- | ------------------------------------------------------------------ |
| API key theft from disk                | Medium     | Critical | AES-256-GCM encryption at rest; keys never stored in plaintext     |
| Environment variable exposure via logs | Low        | High     | Secrets never logged; env vars sanitized in output                 |
| Key rotation failure                   | Low        | Medium   | Automated rotation via credential manager; short-lived tokens      |
| Backups containing secrets             | Medium     | Critical | Encryption required for backups; access controls on backup storage |

### Authentication & Authorization Risks

| Risk                                          | Likelihood | Impact   | Mitigation                                                        |
| --------------------------------------------- | ---------- | -------- | ----------------------------------------------------------------- |
| Brute force against API token                 | Medium     | High     | Rate limiting on auth endpoints; lockout after failed attempts    |
| Token leakage via XSS in dashboard            | Medium     | Critical | CSP headers; input sanitization; token scoped to necessary routes |
| Privilege escalation via misconfigured policy | Medium     | Critical | Default-deny allowlist mode; YAML policy validation at startup    |
| Unauthorized agent tool access                | Medium     | High     | Policy engine enforces fine-grained permissions per user/channel  |
| Session hijacking                             | Low        | High     | Secure session handling; HttpOnly cookies; short session expiry   |

### Injection & Input Validation Risks

| Risk                                   | Likelihood | Impact   | Mitigation                                                                  |
| -------------------------------------- | ---------- | -------- | --------------------------------------------------------------------------- |
| Prompt injection via user input        | High       | Medium   | Input sanitization; allowlist-based tool execution; prompt isolation        |
| Command injection in CLI tools         | Medium     | Critical | shell:false default; execFileSync with array args; allowlist enforcement    |
| SQL injection in memory layer          | Low        | Critical | Parameterized queries; SQLite prepared statements                           |
| SSRF via webhook URLs                  | Medium     | High     | DNS rebinding protection; URL validation before fetch                       |
| File path traversal via agent requests | Medium     | High     | Path validation; sandboxed file access; allowlist for filesystem operations |
| ReDoS in regex patterns                | Low        | Medium   | Timeout on regex evaluation; precompiled patterns                           |

### Data & Privacy Risks

| Risk                                    | Likelihood | Impact   | Mitigation                                                              |
| --------------------------------------- | ---------- | -------- | ----------------------------------------------------------------------- |
| Memory data exfiltration                | Medium     | Critical | Encrypted SQLite; access logging; role-based memory visibility          |
| Session data leakage                    | Medium     | High     | Session encryption; automatic session expiry; secure deletion           |
| Log data containing sensitive info      | Medium     | High     | Secrets sanitized in logs; PII redaction; secure log storage            |
| Data retention beyond user expectation  | Medium     | Medium   | Configurable retention policies; user data export/deletion capabilities |
| Cross-tenant data access (shared infra) | Low        | Critical | Tenant isolation in Supabase; row-level security                        |

### Network & Communication Risks

| Risk                              | Likelihood | Impact | Mitigation                                                           |
| --------------------------------- | ---------- | ------ | -------------------------------------------------------------------- |
| MITM on LLM API communication     | Low        | High   | TLS required for all provider APIs; certificate validation           |
| Webhook DNS rebinding attack      | Medium     | High   | DNS pinning; request origin validation; short-lived webhook tokens   |
| Unencrypted channel communication | Low        | High   | TLS enforced for all external channels (Telegram, Discord, WhatsApp) |
| IP address exposure via logs      | Medium     | Low    | IP anonymization; PII redaction in logs                              |

### Dependency & Supply Chain Risks

| Risk                                   | Likelihood | Impact   | Mitigation                                                                     |
| -------------------------------------- | ---------- | -------- | ------------------------------------------------------------------------------ |
| Malicious npm package                  | Medium     | Critical | npm audit in CI; pinned versions; hash verification in Dockerfile              |
| Compromised GitHub workflow            | Low        | Critical | SHA-pinned actions; minimal permissions; id-token write restricted to sigstore |
| Vulnerability in transitive dependency | High       | Medium   | osv-scanner; dependabot alerts; .trivyignore for known issues                  |
| Typosquatting attack                   | Low        | Critical | Exact package names; no informal package references                            |

### DoS & Resource Exhaustion Risks

| Risk                                  | Likelihood | Impact | Mitigation                                                        |
| ------------------------------------- | ---------- | ------ | ----------------------------------------------------------------- |
| Infinite loop in agent code           | Medium     | Medium | Execution timeout; sandboxed worker threads; memory limits        |
| API rate limit exhaustion (provider)  | Medium     | Medium | Per-endpoint rate limiting; backoff strategies; usage monitoring  |
| Disk space exhaustion via logs        | Medium     | Low    | Log rotation; size limits; automatic cleanup of old logs          |
| Memory exhaustion via large responses | Medium     | Medium | Response size limits; streaming with chunked processing           |
| CPU exhaustion via crypto operations  | Low        | Low    | Efficient crypto implementations; caching of expensive operations |

### Channel-Specific Risks

| Risk                              | Likelihood | Impact   | Mitigation                                             |
| --------------------------------- | ---------- | -------- | ------------------------------------------------------ |
| Telegram bot token exposure       | Low        | Critical | Token encrypted at rest; short-lived session tokens    |
| Discord webhook hijacking         | Low        | High     | HMAC verification; origin validation                   |
| WhatsApp session hijacking        | Low        | High     | Encrypted session storage; device validation           |
| Channel-specific phishing attacks | Medium     | Medium   | Message content sanitization; anti-phishing heuristics |

### Desktop App Risks

| Risk                                    | Likelihood | Impact   | Mitigation                                                    |
| --------------------------------------- | ---------- | -------- | ------------------------------------------------------------- |
| Local llama.cpp binary execution        | Medium     | High     | Sandboxed execution; allowlist for model loading              |
| Desktop app update mechanism compromise | Low        | Critical | Signed updates; hash verification; update provenance tracking |
| Tauri IPC bridge exploitation           | Low        | High     | Context isolation in Tauri; permission model for IPC          |
| File system access via desktop features | Medium     | High     | User consent for file access; sandboxed operations            |

### Configuration & Operational Risks

| Risk                                  | Likelihood | Impact | Mitigation                                                         |
| ------------------------------------- | ---------- | ------ | ------------------------------------------------------------------ |
| Default configuration insecurity      | Medium     | High   | Secure defaults in config/default.yaml; explicit security warnings |
| Misconfiguration of allowlist         | Medium     | High   | YAML schema validation; security check on startup                  |
| Secret rotation without recovery plan | Low        | High   | Graceful degradation; backup keys; documented rotation procedures  |
| Insufficient monitoring/alerting      | Medium     | Medium | Health check endpoints; audit logging; anomaly detection           |

### Threat Actor Categories

| Actor                   | Capabilities                        | Motivation                                     |
| ----------------------- | ----------------------------------- | ---------------------------------------------- |
| External attacker       | Network access, exploit development | Financial gain, data theft, service disruption |
| Malicious contributor   | Code commit access                  | Supply chain attack, backdoor insertion        |
| Malicious insider       | Repository access, credentials      | Data exfiltration, unauthorized access         |
| Accidental insider      | Legitimate access                   | Misconfiguration, data leakage                 |
| Automated bots/scrapers | Web access                          | Resource exhaustion, data scraping             |

---

## Reporting Security Issues

Found a vulnerability? Do not open a public issue. Instead:

- **GitHub Security Advisories**: [Report privately](https://github.com/AG064/argentum/security/advisories/new)
- **Maintainer**: AG064 (GitHub: https://github.com/AG064)

Expected response: acknowledgment within 24–48 hours, fix timeline based on severity.
