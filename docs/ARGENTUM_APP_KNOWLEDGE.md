# Argentum App Knowledge

This file lists the app facts Argentum may summarize when the user enables app context. It is not a permission grant by itself.

## Current Desktop Surfaces

- Chat: local and provider-backed conversation, attachments, model selection, thinking level, context indicator, reasoning display, and regeneration.
- Gateway: start, status, stop, and logs through fixed desktop bridge commands.
- Security: workspace boundary, context approvals, capability posture, and audit state.
- Settings: workspace, provider, model, context, chat display, Telegram, security, and advanced sections.
- Diagnostics: provider usage, Telegram status, gateway state, workspace health, Argentum System Dashboard, and recent redacted activity.
- Activity Logs: gateway, audit, and structured app events with secrets removed.

## What The Assistant May Know When Approved

- App version, active page, view mode, workspace path, session ID, provider, model, thinking level, and security profile.
- Gateway PID, status, health URL, and log path when available.
- Provider usage counters when the provider exposes them.
- Telegram diagnostics: configured state, last update, last session, last response status, and last error.
- Recent UI and backend event names and statuses, without secrets or full private message content.

## Privacy Boundary

Argentum does not expose API keys, tokens, raw secrets, browser sessions, RAM, arbitrary filesystem locations, or OS control by default. Logs should help debugging, but they must redact secrets and stay inside approved context.
