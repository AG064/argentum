# Argentum Workspaces And Agents

Argentum currently opens one active workspace at a time. That is an implementation limit for the desktop MVP, not the long-term shape of the product.

## Data Model

- Workspace: root folder, config, secret references, logs, data, sessions, and capability policy.
- Session: one conversation thread inside one workspace. It has a stable ID, message history, compacted summary, provider metadata, and channel links.
- Agent: named behavior profile with a system prompt, model preference, thinking level, context permissions, and enabled capabilities.
- Provider config: workspace-scoped provider, endpoint, model, auth method, and secret reference.
- Permissions: workspace policy plus per-session grants and audit entries.
- Logs: redacted events for app start, settings, provider tests, chat, Telegram, gateway, diagnostics, and failures.

## Relationship

One workspace can contain many sessions and many agents. A session belongs to one workspace and may be attached to more than one channel, such as desktop chat and Telegram. Agents can be reused across sessions, but permissions are still checked against the active workspace and session.

## Planned Multi-Workspace Flow

1. The workspace switcher lists known workspaces and their health.
2. Switching workspace reloads sessions, providers, logs, and permissions.
3. Telegram sessions resolve by channel ID plus workspace ID, not only Telegram chat ID.
4. Agents live under `workspace/data/agents/` and can be selected per session.
5. Cross-workspace memory stays off by default. It should require export/import or a named shared-memory grant.

## Safety Boundary

Argentum must not silently repair, self-modify, or act across workspaces. Repair actions need explicit user approval, must name the workspace, and must stay inside the approved folder or capability.
