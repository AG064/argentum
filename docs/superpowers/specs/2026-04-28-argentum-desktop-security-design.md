# Argentum v0.0.5 Desktop and Security Design

Date: 2026-04-28
Status: Approved for implementation planning
Branch: development

## Summary

Argentum v0.0.5 should become a complete installed local application while keeping the CLI as a first-class interface. The Windows experience should be the flagship GUI path: a normal installer, a real app window, guided onboarding, chat, settings, skills, knowledge graph, agent runner, logs, and security approvals in one product. macOS and Linux should receive matching binaries where practical, with CLI binaries preserved for terminal-first users and server environments.

The security baseline is capability-based. By default, Argentum can only read and write inside one selected Argentum workspace/install data folder. The agent cannot browse the rest of the filesystem, read process memory, inspect unrelated processes, automate the OS, run arbitrary shell commands, or access external network/integration surfaces unless the user explicitly permits a scoped action.

## Goals

- Ship v0.0.5 with the product direction clearly represented in code, packaging, and release artifacts.
- Keep the current CLI style because it is already a good fit for terminal users.
- Add a Windows GUI app path so the installed binary behaves like a normal program, not a transient terminal window.
- Preserve complete CLI access for Linux users, macOS users, server users, and power users.
- Build the GUI and CLI on the same configuration, onboarding, runtime, and security policy engine.
- Make onboarding resumable and editable with Back, Review, and Finish states.
- Never show passwords, API keys, bot tokens, auth tokens, or similar secrets while typing.
- Keep config and data in the selected Argentum workspace/install data folder for now.
- Make the default agent security posture workspace-only.
- Require explicit user permission for anything outside the default workspace boundary.
- Audit all permission grants, denials, and privileged action attempts.

## Non-Goals

- v0.0.5 does not need cloud account sync.
- v0.0.5 does not need mobile apps.
- v0.0.5 does not need code signing or notarization if credentials are unavailable, but the packaging design should leave room for it.
- v0.0.5 does not need a marketplace payment system.
- v0.0.5 should not add broad trusted permissions by default.

## Product Shape

### Windows

Windows is the primary GUI target.

The release should include:

- A Windows installer.
- A Windows GUI app named Argentum.
- A Start Menu shortcut.
- An optional Desktop shortcut.
- A CLI executable in the installation folder.
- A clear install path selection step in the installer.
- A license agreement step in the installer.
- A launch-after-install option.
- A first-run GUI onboarding flow when no config exists.
- A way to open CLI mode from the GUI or install folder.

The GUI app should continue into the main Argentum interface after onboarding instead of closing. The app title should be Argentum, and the window/taskbar icon should use the Argentum icon.

### macOS

macOS should receive a GUI-capable binary or app bundle when practical, plus CLI binaries. If Apple signing and notarization are not available, the release can ship an unsigned archive first, with documentation that Gatekeeper may require manual approval.

### Linux

Linux should receive a GUI-capable binary where practical, plus a CLI binary. AppImage is preferred for desktop convenience, while tar archives remain useful for servers and terminal-first users.

### CLI

The CLI remains a first-class interface. It should keep its current style and improve completeness rather than becoming a fallback afterthought.

CLI requirements:

- Existing commands continue to work.
- Onboarding remains available through `argentum onboard`.
- CLI setup supports Back or Review/Edit before writing config where technically possible.
- Secret prompts do not echo sensitive values.
- Copy/paste behavior should be documented and improved where the terminal library allows it.
- CLI output should avoid closing the window after setup when launched through the GUI path.
- CLI should use the same workspace, config, and security policy as the GUI.

## GUI Application Design

The GUI should be a real desktop shell around the Argentum runtime. Tauri is the preferred implementation direction because it provides a native desktop shell, smaller bundles than Electron, a permission-oriented architecture, and a straightforward bridge to local commands and web UI assets.

The app should include these top-level sections:

- Onboarding
- Chat
- Agents
- Agent Runner
- Skills Library
- Webchat Settings
- Knowledge Graph
- Memory
- Activity Logs
- Security and Permissions
- Settings
- Diagnostics

The GUI should reuse the existing dashboard assets only where they fit. The app should not merely open the existing dashboard in a browser unless used as an interim fallback.

## Onboarding Flow

The onboarding flow should be shared by GUI and CLI through a core onboarding service.

Recommended GUI steps:

1. Welcome
2. Workspace and data location
3. Runtime mode
4. LLM provider
5. Secrets and credentials
6. Channels and webchat
7. Security posture
8. Review
9. Finish and launch

Requirements:

- The install/workspace path is visible and editable.
- Back works from every step after the first.
- Review shows all non-secret values before writing config.
- Secret fields show masked values only.
- Secret values are not stored in YAML.
- Finish writes config and data under the selected workspace.
- After Finish, the GUI proceeds into the main interface.
- Re-running onboarding allows editing existing settings.

## Data and Configuration Layout

For v0.0.5, all normal user data should live under the selected Argentum home folder. This resolves the conflict between "keep data with the installation" and Windows Program Files permissions by making the default install mode per-user and writable.

Recommended Windows default:

```text
%LOCALAPPDATA%\Programs\Argentum
```

The installer can still support a custom path. If a user selects a protected per-machine location such as Program Files, the setup flow should either create a writable `workspace` folder with the correct ACLs or ask the user to choose a writable Argentum home folder.

Suggested layout:

```text
Argentum/
  argentum.exe
  argentum-cli.exe
  workspace/
    config/
      default.yaml
    data/
      runtime/
      memory/
      sessions/
      skills/
      logs/
      audit/
    secrets/
      secrets.env
    cache/
    backups/
```

The exact executable names can vary by platform, but the separation should remain:

- The Argentum home contains binaries, static assets, and the default workspace in per-user installs.
- The workspace contains config, logs, memory, sessions, local skills, and secrets.
- Secrets must not be committed or included in example config output.

The existing `ARGENTUM_WORKDIR` environment variable should remain supported. Legacy `AGCLAW_*` variables can continue working for compatibility but should no longer be the primary documented names.

## Security Model

### Principle

Argentum should be powerful only by user consent. The default state is workspace-only. The user can permit broader actions, but every privileged action must be explicit, scoped, visible, and audited.

### Default Sandbox

By default, the agent may:

- Read and write inside the selected Argentum workspace.
- Read Argentum configuration and runtime files inside that workspace.
- Use enabled Argentum features that do not require additional external access.
- Access local logs and audit trails inside the workspace.

By default, the agent may not:

- Read files outside the selected workspace.
- Write files outside the selected workspace.
- Read process memory or inspect unrelated processes.
- Read browser profiles, password stores, SSH keys, OS credential stores, or unrelated app data.
- Run arbitrary shell commands.
- Use OS automation APIs.
- Access arbitrary network domains.
- Install dependencies or external tools.
- Use camera, microphone, clipboard, or screen capture.
- Modify system settings.

### Capability Gate

All privileged operations go through a capability broker. The broker is the only layer that can approve access to resources outside the default workspace.

Capability requests should include:

- Action type
- Tool or feature name
- Exact path, command, domain, device, or integration being requested
- Reason shown to the user
- Scope
- Expiration
- Risk level

Example grant scopes:

- One action
- One file
- One folder
- One domain
- One integration
- One terminal command
- Current session
- Always for this workspace

Broad grants are allowed only when the user chooses them explicitly. The UI should make broad grants visually distinct and easy to revoke.

### Permission Profiles

Recommended profiles:

- Restricted: default workspace-only mode.
- Ask Every Time: prompt for each privileged action.
- Session Grant: allow a scoped action until the current task/session ends.
- Trusted Mode: broad access after a clear warning, with audit logging and revocation.

Trusted Mode is not the default. The product should remain useful without it.

### Audit Trail

The audit trail should record:

- Allowed requests
- Denied requests
- Expired grants
- Revoked grants
- Privileged action attempts
- Shell command text
- File paths touched outside the workspace
- Network domains requested
- Integration names
- Timestamp, requesting feature, and user decision

Audit logs must not record raw secrets.

### Secret Handling

Secrets include API keys, passwords, tokens, bot tokens, auth tokens, signing keys, and similar credentials.

Requirements:

- Secret inputs are masked in GUI and CLI.
- Secrets are not printed after entry.
- Secrets are not written to YAML.
- Logs redact secrets.
- Test fixtures use obvious fake values only.
- The GUI can show whether a secret is present without revealing it.
- Secret rotation should be possible by re-entering the value.

## Runtime Architecture

Recommended process layout:

- GUI process: renders UI and handles user interaction.
- Runtime process: runs Argentum core services.
- Agent worker process: executes agent work under policy.
- Capability broker: mediates files, shell, network, OS, and integration access.
- Audit logger: records decisions and privileged attempts.

The GUI process should not directly perform privileged agent actions. It should ask the broker/runtime, which applies policy.

## Installer and Release

v0.0.5 should bump all repository version references through the existing version sync path.

Release artifacts should include, where practical:

- Windows installer executable
- Windows MSI if still useful
- Windows portable CLI binary
- macOS CLI binary
- macOS GUI artifact if available
- Linux CLI binary
- Linux GUI artifact if available
- SHA256 checksums
- Release notes that consistently say Argentum

The installer should:

- Show license agreement.
- Show install path after Next, not hidden only under Options.
- Show default install location.
- Offer Desktop shortcut.
- Add Start Menu entry.
- Offer launch-after-install.
- Use Argentum icon and window title.
- Install CLI executable(s) beside the GUI app.

## Testing Strategy

Required automated checks:

- Version sync check for v0.0.5.
- Onboarding unit tests for config generation.
- Secret masking tests.
- No-secrets-in-YAML tests.
- Workspace boundary tests.
- Capability broker allow/deny tests.
- Audit log tests.
- CLI launch routing tests.
- GUI packaging smoke tests where CI supports them.
- Release artifact naming tests.
- Installer metadata tests for product name, icon, and version.

Manual verification for v0.0.5:

- Install on Windows.
- Confirm Start Menu shortcut.
- Confirm Desktop shortcut option.
- Confirm GUI app opens without terminal flash.
- Complete GUI onboarding.
- Confirm Back and Review work.
- Confirm masked secret input.
- Confirm app enters main interface after onboarding.
- Confirm CLI still works from install folder.
- Confirm default workspace-only file restrictions.
- Confirm denied outside-workspace access is visible and audited.

## Implementation Phases

### Phase 1: Foundation

- Bump to v0.0.5 through version sync.
- Extract shared onboarding state and validation where needed.
- Add workspace path policy helpers.
- Add secret masking helpers.
- Add tests for workspace boundary and secret handling.

### Phase 2: Capability Broker

- Add a broker interface for file, shell, network, OS, and integration actions.
- Enforce workspace-only defaults.
- Add grant storage, expiration, revocation, and audit logging.
- Route high-risk features through the broker.

### Phase 3: Desktop Shell

- Add the GUI desktop app scaffold.
- Add onboarding UI.
- Add main navigation and core pages.
- Wire GUI to shared onboarding and runtime APIs.
- Keep existing CLI behavior intact.

### Phase 4: Packaging

- Update Windows installer to install GUI and CLI artifacts.
- Add Start Menu and Desktop shortcut behavior.
- Update release workflows for v0.0.5 artifacts.
- Add macOS and Linux GUI packaging where practical.

### Phase 5: Verification and Release

- Run local unit, typecheck, lint, build, and packaging smoke tests.
- Build Windows artifacts.
- Push development and main.
- Tag v0.0.5.
- Publish release notes and checksums.

## Implementation Decisions

- Use Tauri as the target GUI shell unless a build-blocking incompatibility appears during the implementation spike.
- Default Windows installation should be per-user and writable so the app and default workspace can live together under the Argentum home folder.
- Keep Trusted Mode in the design, but do not make it the default path. Restricted, Ask Every Time, and Session Grant must work first.
- macOS and Linux should receive CLI binaries and GUI-capable artifacts for v0.0.5. If platform GUI packaging hits an external signing or CI limitation, document the limitation in release notes and still ship the CLI binary.

## Approval Check

This design matches the chosen direction:

- Desktop shell is the target.
- CLI stays strong and familiar.
- Windows GUI is the priority.
- macOS and Linux keep binaries.
- Default security is workspace-only.
- The agent cannot behave like malware by default.
- User-granted power remains possible but visible, scoped, and audited.
