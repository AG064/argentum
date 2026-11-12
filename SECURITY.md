# Security policy

## Supported versions

| Version | Security fixes |
| --- | --- |
| 0.1.x | Yes, in the latest 0.1 release |
| Older builds | No |

Fixes land on the `development` branch first and ship in the next release.

## Reporting a vulnerability

Report it privately, in one of two ways:

- GitHub: open the repository's **Security** tab and choose **Report a vulnerability**.
- Email: contact@ag064.eu

Include the Argentum version, your platform, the steps to reproduce, and what an attacker could do. Do not include real API keys or private data. Please do not open a public issue or pull request about it until the report has been assessed. The maintainer aims to reply within seven days.

## Scope

In scope:

- the Windows desktop app and installer
- the Android app
- the Argentum Genesis command-line agent (`argentum`)
- the release files and the in-app update flow

Out of scope:

- the model providers you connect, and what they do with what you send them
- extensions and programs you install and approve yourself
- actions you approve, including anything done in Full Access
- a compromised operating system, administrator account, or device

## What Argentum protects against

Argentum is a single-user app. It limits what the agent can do on your behalf and treats everything that comes from a model or a tool as untrusted.

- **The saved policy decides.** The project's saved policy and permission profile decide what the agent may do. Prompts, model output, tool arguments, provider metadata, and web content cannot grant more.
- **Files stay in the project.** File tools resolve paths inside the project folder. Paths that go up a level, leave the folder, or pass through a symbolic link are refused.
- **Changes need approval.** Writing files, running shell commands, network access, and starting other programs ask first, unless your profile allows them. An approval covers the exact action it was given for. A changed command, argument, or path needs a new one.
- **Edits are checked.** An edit must match the file the agent read. If the file changed in the meantime, the edit is refused. Every change is recorded and can be restored.
- **Child processes are contained.** Commands run with a cleared environment and limited output. On Windows they run in a job object with CPU and memory caps, and they are stopped when Argentum exits.
- **Git is limited.** The agent can read status, diffs, logs, and branches, and with approval can add, commit, restore, and create branches and worktrees. It has no push, merge, rebase, reset, tag, or remote commands.
- **The agent has no general network tool.** It reaches the network only through the model provider you chose.
- **Full Access is local only.** Only you can choose Full Access, in the app or at the local command line. Remote clients, paired devices, and scripted clients cannot. Operating-system permissions still apply in Full Access.
- **Keys are stored by the system.** Provider keys are kept in Windows Credential Manager or the Android Keystore, and each key is only sent to the address of its own provider.
- **Exports are redacted.** Session exports remove saved keys and known secret values. Review an export before you share it.

## What Argentum does not protect against

- Argentum is not an operating-system sandbox. A program you approve runs with your account's access.
- Anything you send to a model provider is handled under that provider's terms.
- Conversation history is stored in a local SQLite database protected by your account's file permissions. It is not encrypted by Argentum.
- A hostile user or malware on the same account can read Argentum's data.

## Optional features and their defaults

| Feature | Default |
| --- | --- |
| Page reader module | Off. Loads a page as plain text only when you click Open. It runs no scripts, keeps no cookies, and never sends page content to a model. The agent cannot use it. |
| Local model add-on (Windows) | Not installed. Downloads llama.cpp and a model only when you ask, from pinned addresses, and checks their SHA-256. It runs as a separate process on 127.0.0.1. |
| Interface changes by the agent | Each change asks first and can be undone. The agent cannot change permissions or the approval prompt. |
| Device pairing | Off until you pair a device. A paired device cannot choose Full Access, and the device doing the work keeps its own approvals. |
| Android "All files access" | Off. When you turn it on, the app can reach shared storage, and file tools still stay inside the folder you chose. |
| Windows desktop control and extensions | Only in the separate `argentum-genesis-desktop-control` CLI download. Desktop control can inspect windows and send keys to a Windows application you name. Every desktop action asks first, in every permission profile, and password fields are never read or captured. Extensions, MCP servers, and the Playwright browser adapter run as local programs with your account's access, only after you review and activate them. Node.js and the browser are installed separately. |
| Local PDF artifacts | Not in any release file. Optional at build time. |

## Network use

Argentum contacts:

- the model provider you set up, at the address in its settings
- GitHub, only when you check for updates
- the download addresses of the local model add-on, only when you install it
- a web page, only when you open it in the page reader

There is no telemetry.

## Release integrity

- Every release has a `SHA256SUMS.txt` list, a GitHub build provenance attestation, and an SBOM.
- Check a file with `Get-FileHash FILE` on Windows or `sha256sum -c SHA256SUMS.txt` elsewhere, and check where it was built with `gh attestation verify FILE --repo AG064/argentum`.
- The Windows files are not code signed yet, so Windows SmartScreen may warn the first time.
- The Android APK is signed with the project's key in the release workflow. You can view the signer with `apksigner verify --print-certs FILE`.
- The in-app updater checks only when you ask. It verifies the download against `SHA256SUMS.txt` from the same release and rolls back if the install fails. That proves the file matches the release, not who published it.

## For contributors

- Never commit API keys, tokens, signing keys, keystores, or private data. Signing keys belong only in protected CI secrets and offline backups.
- Pull requests run formatting, Clippy, tests, and `cargo deny` against `deny.toml` in CI.
