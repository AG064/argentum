# Security Audit of ~/.openclaw/workspace/skills

Audit performed on: 2026-03-18

Notes: I scanned every SKILL.md and searched skill directories for risky patterns (eval, exec, subprocess, curl/wget, child_process, process.env, API keys, hardcoded tokens). Findings below per-skill.

## actual-self-improvement
- **Статус:** ✅ Чисто
- **AI slop:** None notable in SKILL.md
- **Безопасность:** No network calls, no secrets, local file-oriented. Safe.
- **Рекомендации:** None.

## agent-task-tracker
- **Статус:** ✅ Чисто
- **AI slop:** None
- **Безопасность:** Writes to memory/tasks.md as intended; no external calls.
- **Рекомендации:** Ensure memory file permissions are appropriate (user-only).

## ai-humanizer
- **Статус:** ✅ Чисто
- **AI slop:** Contains pattern lists and analyzers (expected). No eval/exec found in SKILL.md; repo contains src/cli.js but grep found only normal uses (process.env for NO_COLOR). No hardcoded secrets.
- **Безопасность:** Safe.
- **Рекомендации:** None.

## automation-workflows
- **Статус:** ✅ Чисто
- **AI slop:** None
- **Безопасность:** Guidance-only, no code executing network actions.
- **Рекомендации:** None.

## auto-updater
- **Статус:** ⚠️ Подозрительно
- **AI slop:** Documentation contains shell commands that call package managers and cron. Not malicious by itself but can be dangerous if run with elevated privileges.
- **Безопасность:** Running auto-update could change code and install packages; if cron runs as a privileged user it may allow supply-chain updates. No hardcoded creds found.
- **Рекомендации:** Run updates as non-root, review change log before applying, consider requiring manual approval for skill updates.

## coding
- **Статус:** ✅ Чисто
- **AI slop:** None
- **Безопасность:** Local-only storage in ~/coding. No network or secrets.
- **Рекомендации:** Ensure directory permissions are user-only.

## deep-research-pro
- **Статус:** ⚠️ Подозрительно
- **AI slop:** SKILL.md includes shell snippet that pipes curl output into an inline Python HTML stripper. Example uses curl -sL and python -c; it's an example but encourages fetching arbitrary URLs.
- **Безопасность:** Capability to fetch and deeply read arbitrary web pages may pull in large/untrusted HTML or execute secondary content. The SKILL.md asks to use curl; if implemented, ensure sandboxing. No hardcoded keys found.
- **Рекомендации:** Audit any runtime code that implements fetching, add timeouts, content-size limits, sanitize URLs, avoid executing fetched content, and run network fetches in isolated subprocesses/containers.

## deep-scraper
- **Статус:** ⚠️ Подозрительно
- **AI slop:** Skill claims capability to "penetrate protections" and use containerized Crawlee/Playwright to access complex sites. This is a high-risk capability.
- **Безопасность:** Requires Docker; scraping may violate Terms of Service and can access protected content. The SKILL.md warns about privacy but the tooling could be misused for exfiltration.
- **Рекомендации:** Limit who can run this skill, require user explicit consent, run in an isolated VM/container, add safeguards to block credentials/password-protected targets, and log/alert on attempts to scrape non-public resources.

## elevenlabs-api
- **Статус:** ✅ Чисто (with secrets required)
- **AI slop:** SKILL.md heavily documents API usage; no hardcoded keys in repo, but it requires MATON_API_KEY in env.
- **Безопасность:** Relies on MATON_API_KEY from environment. Ensure API key not committed and env is protected.
- **Рекомендации:** Rotate keys regularly, avoid echoing keys to logs. Consider using secrets manager.

## free-ride
- **Статус:** ✅ Чисто (but requires API key)
- **AI slop:** main.py checks OPENROUTER_API_KEY in env and prints guidance; no hardcoded secrets. Some CLI watcher/daemon code present.
- **Безопасность:** Requires OPENROUTER_API_KEY; code prints token guidance (fine). Running watcher/daemon could change openclaw.json (config) and restart gateway — privileged actions.
- **Рекомендации:** Gate config writes behind confirmation; don't run as root; audit gateway restart steps.

## humanizer
- **Статус:** ✅ Чисто
- **AI slop:** Large rule lists and examples (expected). No dangerous calls found.
- **Безопасность:** Safe.
- **Рекомендации:** None.

## self-improving
- **Статус:** ✅ Чисто
- **AI slop:** Documentation and local file storage patterns. No network calls.
- **Безопасность:** Writes to ~/self-improving; ensure no secrets stored.
- **Рекомендации:** Add explicit rule to never store credentials (some files mention boundaries.md already).

## summarize
- **Статус:** ⚠️ Подозрительно
- **AI slop:** Uses external CLI `summarize` and requires API keys for models (OpenAI, Anthropic, Google). May fetch web content and use third-party APIs.
- **Безопасность:** Requires API keys set in env; external tool may send content to third-party models — privacy risk.
- **Рекомендации:** Warn users before sending sensitive documents to external APIs; allow local-only mode or opt-out.

## telegram
- **Статус:** ✅ Чисто
- **AI slop:** Guidance-only skill for Telegram Bot API. SKILL.md explicitly instructs never to log tokens.
- **Безопасность:** Needs bot token to operate; ensure tokens not logged.
- **Рекомендации:** Enforce secret handling and webhook secret header usage as documented.

## translate
- **Статус:** ✅ Чисто
- **AI slop:** Documentation-only, no code. Good rules about preserving placeholders and not translating code.
- **Безопасность:** Safe.
- **Рекомендации:** None.

## writing-assistant
- **Статус:** ✅ Чисто
- **AI slop:** Template-based instructions; no external calls.
- **Безопасность:** Safe.
- **Рекомендации:** None.

## x-twitter
- **Статус:** ✅ Чисто (requires credentials)
- **AI slop:** CLI checks for TWITTER_BEARER_TOKEN and prints partial token. No hardcoded tokens but code echoes first 8 characters of token in logs which may be undesirable.
- **Безопасность:** Requires Twitter/X API credentials. Printing token fragments could leak secret in logs.
- **Рекомендации:** Remove or reduce token printing; avoid writing tokens to stdout/logs. Use permissioned config and env variables.


---
Summary and next steps:
- No files contained obvious hardcoded API keys or backdoors in the checked SKILL.md files.
- High-risk skills: deep-scraper (potential to access protected content), deep-research-pro and summarize (fetch arbitrary web content and run curl/python snippets), auto-updater and free-ride (can change system config / restart services). These should be run with caution and preferably in isolated/non-privileged environments.
- Actionable recommendations are in each skill section. No changes were made to files per constraints.

Audit completed.
