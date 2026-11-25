<!-- SPDX-License-Identifier: AGPL-3.0-only -->
<!-- Copyright (C) 2026 AG064 -->

# Agent modules

Agent modules add capabilities that the agent itself uses, such as reading a web
page. They are different from interface modules (`docs/UI_MODULES.md`), which only
change what is shown. Every agent module is off by default, has its own switch, and
shows its permissions before it is turned on.

## One mechanism for every module

A module is a static manifest in `argentum-domain` (`modules.rs`) and an
implementation in the `argentum-modules` crate. The manifest declares:

| Field | Meaning |
| --- | --- |
| `id` | Stable module id, such as `web-fetch` |
| `capability_id` | The policy key that is the module's switch, such as `module.web-fetch` |
| `label`, `summary` | What Settings shows before the module is turned on |
| `permissions` | The `Capability` values its tools need, such as `Network` |
| `tools` | Each tool's model-facing name, title, risk and whether it asks first |
| `settings` | Owner settings, each with a key, label and kind |
| `surfaces` | Interface pieces it fills, as `SurfaceId` values, if any |

The harness turns every manifest into a configurable capability that starts
disabled. Settings > Capabilities lists it with its summary and permissions, and
`argentum harness capability <capability_id> enable` turns it on from the CLI.

All module tools share one tool input, `ToolInput::Module { tool, arguments }`.
The implementation of each tool provides four things:

1. a JSON schema for the model;
2. a parser that checks the arguments and returns them in a normal form, or
   refuses them before any prompt;
3. an approval key, such as `host example.com`, which is what an approval
   covers;
4. the work itself, which runs only after the runtime has authorized the call.

The runtime does not know module tools by name. It asks the module registry to
parse a call, to describe it for the approval prompt, and to run it. Adding a
module adds a manifest and an implementation, not new branches in the runtime.

## Turning a module on and off

- The switch is the module's capability in the persisted workspace policy.
  Turning it on is a policy elevation, so it needs local owner authority, the
  same as any other capability. Model output, tool arguments, remote clients,
  paired devices and page content cannot turn it on.
- A run reads the policy when it builds its tool list. A module that is off
  offers no tools, and a call to one of its tools is refused as a disabled
  capability, even if the model names it.
- Turning a module off takes effect at the next tool list and the next call. A
  run that is waiting on an approval keeps the policy unchanged until the approval
  is answered, as for every other capability.

## Authority stays with the policy

- A module never adds a grant. It only declares which capabilities its tools
  need, and the existing `ExecutionAuthorizer` decides each call against the
  saved policy and the owner's grants.
- The approval key is computed by the module from parsed arguments, never from
  free model text. The approval prompt shows the full request beside it.
- Network is never allowed by the profile alone. Every network call needs an
  owner grant, and only grants bound to one action (one host) or one tool call
  count. A grant for a whole run, session or workspace does not open the network.
- What a module returns is untrusted data. It reaches the model marked as such
  and cannot change permissions, settings or the policy.
- Module settings are owner settings, stored per project. Changing one needs
  local owner authority. A setting such as a search address chooses where a
  request goes, and the request still needs its own approval for that host.

## Modules and their state

| Order | Module | State |
| --- | --- | --- |
| 1 | `web-fetch`, `web-search` | Built in this change |
| 2 | Images in messages | Planned. The provider layer already carries images for desktop screenshots. The module adds attachments from the composer for models whose catalog entry accepts images |
| 3 | MCP servers and skills | Planned. Today they are in the `extensions` build only. The module moves the existing extension host into the standard build behind `module.extensions` |
| 4 | Skills marketplace | Planned. See below |
| 5 | Helpers (subagents) | Planned. See below |
| 6 | Hooks | Planned. Owner-written commands before or after a tool, at run start and at run stop, run through the existing process runner and approvals |
| 7 | Worktree per session | Planned. Uses the existing `git worktree` operation, with the session bound to its own worktree root |
| 8 | Language server diagnostics | Planned. A language server runs as an approved process and reports errors, types and symbols |
| 9 | Scheduled tasks, approval reviewer, run timeline, fork and replay | Planned. The reviewer only suggests and never grants |

## Tools that need the runtime

Web fetch needs only the network. Helpers, scheduled tasks and the reviewer need
the runtime itself: a model request, a child run, a session. They use the same
manifest and the same `ToolInput::Module`, and their `run` gets a narrow host
interface from `ToolContext` (planned as `module_host`) instead of the whole
runtime. The host offers only what the module declares, for example "start a
child run with this brief, this tool list and this budget, and return its final
answer". A helper's tools are the parent's tools as the policy allows them, never
more, and every approval in a helper goes to the owner as it does in the parent.
The existing `delegate_readonly` tool becomes the first user of that interface.

## MCP servers and skills: configuration

The MCP module builds on the import format of the import change
(`argentum_domain::imports::ImportedMcpServer`, stored per project in
`imported_mcp_servers`): name, origin, `stdio { command, args }` or
`http { url }` transport, and the names of environment variables and headers,
never their values. Imported servers are drafts with `enabled: false`. Turning
one on is the MCP module's own owner action, which pins the executable or the
endpoint as the `extensions` build does today. Values for environment variables
and headers go to the system credential store. Skills live in
`.agents/skills/<name>/SKILL.md` with the frontmatter fields `name`,
`description`, `license`, `compatibility` and `metadata`. `allowed-tools` is not
accepted: a skill cannot widen what a tool may do.

## Skills marketplace

- The catalog is a JSON index in a GitHub repository that the project controls.
  Each entry names the skill, its source repository and commit, the SHA-256 of
  its package, its license, and what it asks for (for example "reads files" or
  "uses the web-fetch module").
- Browsing reads the index only when the owner opens the marketplace. There is no
  account, no sign-in, no background check and no telemetry.
- Before an install the owner sees the source, the checksum and what the skill
  can do. The package is downloaded, checked against its SHA-256, and installed
  into `.agents/skills/` through the same path as any skill. A package that does
  not match is discarded.
- Installing grants nothing. An installed skill runs through the skills module,
  and every tool it uses asks as any tool does.
- The index format has a `kind` field for every entry (`skill` first, then
  `module`, `theme`, `mcp-server` and `model` in 0.2.0), so the 0.2.0
  marketplace reuses the index, the checksum step and the review screen.

## Web fetch and web search

`web_fetch` reads one http or https page as text, using the same reader as the
Browser module. `web_search` sends a query to a search address that the owner
sets, which must answer in the SearXNG JSON format (`?q=...&format=json`).

- Each request needs an approval for its host. Any approval longer than once,
  such as for this session, allows that host for this tool from then on, and
  nothing else.
- Redirects are followed only within the approved host. A redirect to another
  host is returned to the agent as the new address, which needs its own approval.
- Addresses with credentials, non-http schemes, and hosts that are or resolve to
  loopback, private, link-local or other non-public addresses are refused, unless
  the owner turns on the module's local addresses setting. The resolved address
  is pinned for the request.
- The Observe profile has no network. Choosing a permission profile sets every
  module switch to that profile's value, which is off, as for other optional
  capabilities.
- No cookies are kept and no script runs. A page is cut at 2 MiB before parsing
  and the text returned to the model is cut at 48 KiB.
- Tests use fake servers on 127.0.0.1. No test reaches the network.

From the CLI:

```powershell
argentum harness capability module.web-fetch enable
argentum harness capability module.web-search enable
argentum harness module-setting web-search search-address=https://search.example/search
argentum harness module-setting web-fetch allow-local-addresses=true
```

Both commands need local owner authority. An empty value removes a setting.
