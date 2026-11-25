# Security model

Argentum is a single-user app. It limits what the agent can do for you, and it
treats everything that comes from a model or a tool as untrusted.

## What protects you

- **The saved policy decides.** Prompts, model output, tool arguments and web
  content cannot grant more than the project policy allows.
- **Files stay in the project.** Paths that leave the project folder, or pass
  through a symbolic link, are refused.
- **Changes need approval.** Writing files, running commands, network access and
  starting programs ask first, unless your approval mode allows them. An approval
  covers that exact action.
- **Edits are checked.** An edit must match the file the agent read. If the file
  changed in the meantime, the edit is refused. Every change is recorded and can be
  restored.
- **Commands are contained.** They run with a cleared environment and limited
  output. On Windows they run in a job object with CPU and memory caps, and stop
  when Argentum exits.
- **No general network tool.** The agent reaches the network only through the
  provider you chose.
- **Keys are kept by the system,** in Windows Credential Manager or the Android
  Keystore, and each key is only sent to its own provider.
- **Exports are redacted.** Saved keys and known secret values are removed.

## What it does not protect against

- Argentum is not an operating-system sandbox. A program you approve runs with your
  account's access, and so does anything in Full Access.
- What you send to a provider is handled under that provider's terms.
- History is stored in a local SQLite database, protected by your account's file
  permissions, not encrypted by Argentum.
- Malware or another person using the same account can read Argentum's data.

## What Argentum contacts

- the provider you set up, at the address in its settings
- GitHub, only when you check for updates
- the add-on download addresses, only when you install the local model add-on
- a web page, only when you open it in the page reader

There is no telemetry. This documentation site has none either.

The full policy, including how to report a vulnerability privately, is in
[SECURITY.md](https://github.com/AG064/argentum/blob/development/SECURITY.md). The
assurance case is in
[ASSURANCE_CASE.md](https://github.com/AG064/argentum/blob/development/docs/ASSURANCE_CASE.md).
