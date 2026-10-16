# Argentum platform / Genesis CLI two-track roadmap

Dates and source presence are not acceptance. Windows CLI release readiness and
readiness to replace the owner's Codex environment are separate gates. The
owner has not adopted Argentum as the daily environment.

## Repository and release organization

`AG064/argentum` is the canonical native platform repository. Argentum Genesis
is its CLI/TUI product, not a separate repository, permanent CLI branch or
runtime fork. CLI, future desktop and optional browser/extension work share
the existing native core and keep useful Cargo boundaries.
Use feature branches for reviewed development and release-maintenance branches
only for an actual supported version that needs them.

Genesis may release independently of desktop/browser acceptance. The default
CLI build/package selects only its required dependency graph, not every
workspace member; see [architecture](docs/architecture.md). The previous
TypeScript product is explicitly legacy, with
[verified preserved source and release links](docs/MIGRATION_NATIVE.md).

## Track A: Genesis native CLI release (RELEASE01)

`0.1.0-rc.1` reconciles the accepted native implementation into a clean reviewed
branch, preserving the old product in history. Required gates are locked fresh
build, complete applicable offline tests, format/strict lint, source-policy and
license/advisory review, meaningful source review, safe CI, versioned package,
checksums/provenance, isolated first-run/restart controls and outside-checkout
launch including spaces/non-ASCII paths. The versioned
[RC record](docs/releases/0.1.0-rc.1.md) reports actual results and gaps.

Publishing remains a deliberate maintainer decision after draft source review.
It is not performed by CI. Signing, installer, Slint visual acceptance, Android
device/release qualification and other desktop platforms have their own gates.
MILESTONE01 remains assisted delivery. No new live benchmark is authorized by
this roadmap or release preparation.

The final RELEASE01 core source qualification at `9eb7e44` passed hosted
checks and unsigned packaging on 2026-10-12; exact source, run and artifact
identities are in the RC record. This does not publish a release, qualify
desktop/browser work or change any earlier failed result.

## Existing foundations, not missing-work claims

Native `argentum-harness` and `argentum-lifecycle` already implement registry
composition, in-process generations, retirement, disposal/recovery and bounded
state. `argentum-workspaces::skills` supports bounded local skill selection;
instruction projection, providers, jobs, policy, accounting, results and secure
credential paths also exist. None is a general external plugin runtime.

The historical `src/core/plugin-loader.ts`, `src/mcp/{server,types}.ts`,
`src/features/browser-automation` (Playwright) and `computer-control` modules
exist in the pre-transition history. They are reference designs, not native
implementations to expose through an ungoverned Node runtime.

## Track B: shared-platform capability and personal replacement (NEXT01 onward)

NEXT01's optional governed extension/browser implementation remains separate
from the RELEASE01 core RC. Its packaged successor and supplemental contribution
checks do not erase the frozen live `goal_deadline_exceeded` outcomes. Exact
owner-approved dispositions retain missing-usage reservations without inventing
provider usage. The supported local-browser contribution reached post-recovery
native completion on 2026-10-13 with current source verification and model
feedback, plus independent rendered acceptance. This is assisted completion of
retained work, not an uninterrupted run or a representative reliability rate.
See [the retained NEXT01 acceptance record](docs/NEXT01_ACCEPTANCE.md).
The [separate NEXT01 draft](https://github.com/AG064/argentum/pull/681) remains
feature review, not core release inclusion.
Desktop/computer-use qualification and daily Codex replacement remain later
gates, not consequences of the platform/product naming decision.

| Milestone | Dependency and boundary | Real acceptance workflow |
| --- | --- | --- |
| B1: extension/MCP foundations | Native lifecycle/policy plus portable manifests/skills; explicit transport/version and secure auth; bounded discovery/install/update/disable/removal | Install a licensed public synthetic plugin, discover and invoke a scoped MCP tool in a normal task, deny excess authority, revoke/disable during work, restart without stale handle/credential leakage, complete an ordinary coding task using its result |
| B2: complete browser-assisted development | B1 external-process/network governance, selected maintained browser adapter, isolated owner-authorized profile | Implement an unseen local web change, launch bounded preview, inspect DOM/accessibility/screenshots and console/network failures, exercise interaction, repair a real defect, verify source and rendered result; separately test owner sign-in without exporting session material |
| B3: connected apps and artifacts | B1 auth/resources plus typed artifact identity, edit/review and publication boundaries | Read an authorized document, make an explicit scoped edit, render/verify an artifact and preserve unrelated content; deny unauthorized sharing/export and revoked access |
| B4: reliable Windows computer use | B1 action broker plus trusted observation/target identity and Windows UI Automation/OS adapters | Complete a bounded non-browser task in actual desktop apps; prove fresh HWND/PID/element targeting, DPI/multi-monitor behavior, focus-loss/stale-target denial, no secure-desktop/elevation bypass and cancellation with no late action |
| B5: practical daily environment and model choices | Accepted B2-B4 workflows, durable recovery/task context, optional alternative/local routes with their own evidence | Complete unseen multi-hour representative coding/browser/document/desktop work, restart/recover safely, separate required approvals from avoidable owner corrections; qualify each provider/model without inheriting another route's evidence |

Browser development follows extension foundations before general desktop use.
Connected apps can follow B1 independently where auth/artifact needs do not
depend on B2. Large capabilities should land as complete vertical workflows,
not dozens of registered names with no end-to-end path. Modules stay optional
and disabled until their actual policy/permission and installation checks pass.

### Compatibility and maintained foundations

Portable Agent Plugins use root `plugin.json`, optional `mcp.json`, `skills/`
and assets. OpenAI-specific hooks/presentation/mappings are host extensions;
`.codex-plugin` is a compatibility layout, not universal portability. Inventory
and reject unsupported host features explicitly. Never redistribute proprietary
bundles or copy another application's credentials or browser sessions.
[Official packaging](https://developers.openai.com/plugins/build/plugins),
[skills](https://developers.openai.com/plugins/build/skills).

MCP supplies a protocol, not installation, sandboxing or a complete plugin
system. Pin an explicitly supported specification/profile and maintained Rust
SDK. The current 2026-07-28 specification has stateless self-contained requests
and per-request capabilities; optional extensions and older handshake profiles
must be explicitly negotiated, not assumed interchangeable. Apply credential,
origin/issuer, size, time, cancellation and result-disclosure controls around it.
[MCP specification](https://modelcontextprotocol.io/specification/2026-07-28).

Prefer a version-pinned Playwright adapter over a browser engine. Separate
ephemeral browser contexts from explicitly owner-authorized persistent profiles.
Authentication state is sensitive and must not be committed or exported.
[Contexts](https://playwright.dev/docs/browser-contexts),
[authentication](https://playwright.dev/docs/auth).

Prefer Windows UI Automation semantic patterns, then narrow OS input only when
necessary. Require observed target identity/focus before effects. OS integrity,
secure desktop and user intervention remain boundaries, not permissions to
disable security. [UI Automation](https://learn.microsoft.com/en-us/windows/win32/winauto/uiauto-usefortesting),
[SendInput restrictions](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-sendinput).

## Competitive acceptance

"Better than Codex" means completed unseen representative workflows with
independent correctness/result quality, necessary security approvals versus
avoidable supervision, recovery quality, tool availability, elapsed time,
resource use and latency reported separately. Freeze model/provider/settings
and disclose feature differences. Exposed benchmark wins or passing synthetic
checks cannot establish product superiority or personal replacement readiness.

The [B1/B2 brief](docs/NEXT_EXTENSION_BROWSER_MILESTONE.md) and dated NEXT01
evidence preserve the delivered local profile and its limits. The separate
Windows desktop milestone's declared native-editor profile reached assisted
post-repair completion on 2026-10-16; see [NEXT02 acceptance](docs/NEXT02_ACCEPTANCE.md).
This does not pass every B4 display/platform/reliability gate or start B5.
The roadmap adds no live allowance, later campaign or core release feature.
