# UI rework plan

Status: active. The Workbench shell in `ui/workbench/` replaces the earlier shell
and carries every command the earlier shell exposed. Progress is recorded per
workstream below. "Done" means implemented with tests, not visually qualified on a
real device unless the line says so.

| Workstream | State |
| --- | --- |
| 1. Desktop target and approval workflow | Partial. Status, tools, and revalidation rules are shown. Choosing a window and element remains CLI only. |
| 2. Browser activation | Done for status. Adapter, definition, configuration, and activation are shown separately. Activation stays an owner console action. |
| 3. ChatGPT account UI | Partial. The route is labeled as browser sign-in. Sign-in itself remains `provider login chatgpt`. |
| 4. Provider setup polish | Done. Grouped catalog with endpoint, model, credential need, docs, and honest unsupported rows. |
| 5. Responsive and mobile | Partial. One shell serves every width, checked headless. Not checked on a device. |
| 6. Accessibility | Partial. Keyboard actions and names on custom controls, with a source guard test. No screen-reader pass yet. |
| 7. Default-renderer visual QA | Open. Headless software snapshots exist. Default renderer QA remains. |
| 8. Live-preview integration | Open. The preview binary still builds. |
| 9. Screenshot regression tests | Partial. `crates/argentum-ui/examples/ui_snapshot.rs` renders fixed scenes. No stored references yet. |
| 10. Design-system enforcement | Done. `scripts/validate-design-system.ps1`, `validate-icon-system.ps1`, and `validate-mobile-design.ps1` run in CI. |

The contract remains `docs/ARGENTUM_DESIGN_SYSTEM.md` and the approved
black/red/silver shell. This plan schedules the deferred work; it does not
replace the design system, the roadmap, or any milestone record. Where this plan
and the design system disagree, the design system wins until it is formally
revised.

## Non-goals

- No renderer replacement and no second UI framework.
- No token, palette, or brand-mark changes outside an explicit design-system
  revision. The Workbench amendment in the design system is that revision.
- No UI claim for a capability that Rust does not already expose as a durable
  command or read model.
- No fake readiness, fake connectivity, or placeholder data.

## Workstreams

### 1. Desktop target and approval workflow

Goal: make desktop-control targets reviewable before any effect.

- Show the observed window identity, process, bounds, and capability state.
- Present the bounded inspection tree with tree path, automation id, control
  type, name, and bounds.
- Require an explicit owner confirmation for invoke, type, and keys that cannot
  be inherited from the model or a previous run.
- Show the exact revalidation failure when a target moves, closes, minimizes,
  becomes cloaked, loses foreground, or changes process.
- Never preselect a mutating action from ambient focus.

Acceptance: a reviewer can identify the target, see why it is eligible or not,
and confirm one bounded action without pointer coordinates or hidden retries.

Dependencies: the CLI desktop commands in this task and the existing
`crates/argentum-extensions/src/desktop.rs` validation path.

### 2. Browser activation

Goal: make optional browser activation a visible, owner-approved step.

- Show adapter availability separately from browser installation state.
- Explain that activation is not installation approval.
- Surface the captured image lease, origin policy, and navigation limits before
  the first action.
- Keep unadmitted subresource counts and reviewed-stylesheet evidence visible in
  diagnostics.

Acceptance: no browser process starts, no page opens, and no capture is admitted
without a visible owner action and the existing policy checks.

Dependencies: `docs/EXTENSIONS_BROWSER.md` and the extensions lifecycle.

### 3. ChatGPT account UI

Goal: expose browser sign-in and account selection without implying an API key.

- Show sign-in state, account label, and expiry as separate facts.
- Explain that ChatGPT plan uses browser consent, not an API key.
- Show model catalogue refresh state and the selected model.
- Keep account switching, logout, and re-authentication explicit.

Acceptance: a user can distinguish "signed in", "account selected", "model
catalogue stale", and "credential missing" without opening logs.

Dependencies: `docs/CHATGPT_SIGN_IN.md` and the host account commands.

### 4. Provider setup polish

Goal: make the provider catalog usable without reading CLI help.

- Group hosted, aggregator, local, custom, and unsupported providers.
- Show endpoint, default model, credential requirement, and docs link before
  saving a profile.
- Keep the existing shortcuts for MiniMax, DeepSeek, ChatGPT, custom server, and
  local llama.cpp.
- Mark unsupported routes honestly instead of offering a dead save path.
- Never render a secret, a key fragment, or a credential-store detail.

Acceptance: a first-time user can choose a provider, understand its credential
requirement, and reach a saved profile without a hidden step.

### 5. Responsive and mobile

Goal: keep the same hierarchy when the window narrows.

- Define breakpoints for navigation collapse, composer height, and review panes.
- Keep one primary action and one active work surface at every width.
- Do not hide approvals, verification, or failures behind a collapsed menu
  without a visible indicator.
- Treat mobile as a target contract, not a current claim.

Acceptance: narrow-width visual QA shows no clipped controls, overlapping text,
or unreachable approval actions.

### 6. Accessibility

Goal: make the shell operable without a pointer and readable with assistive
technology.

- Keyboard order follows the visual hierarchy.
- Focus is visible and never trapped.
- Controls expose names, roles, and state.
- Status is conveyed by text or shape, not colour alone.
- Motion respects a reduced-motion preference.
- Contrast is checked against the design tokens.

Acceptance: keyboard-only traversal and a screen-reader pass cover the main
task, approval, verification, and settings flows.

### 7. Default-renderer visual QA

Goal: qualify the renderer that ships, not a substitute.

- Capture the approved shell on the default renderer at defined window sizes.
- Compare against the design-system tokens and approved reference images.
- Record font fallback, scaling, and DPI behaviour.
- Reject a pass that relies on a debug-only renderer or a stale screenshot.

Acceptance: a repeatable visual QA record exists for the shipped renderer.

### 8. Live-preview integration

Goal: connect the live-preview loop to the approved shell without changing the
contract.

- Keep preview state separate from durable task state.
- Show preview refresh, stale preview, and failure explicitly.
- Do not let preview changes bypass approval or verification.

Acceptance: the preview loop runs against the approved shell and reports its own
state truthfully.

### 9. Screenshot regression tests

Goal: catch unintended visual changes.

- Store a small, reviewed set of reference screenshots.
- Capture deterministic windows with fixed data and no live provider output.
- Diff with a documented threshold and a human-review path for intentional
  changes.
- Fail closed on missing references rather than silently accepting a new image.

Acceptance: a visual change produces a reviewable diff, and an intentional
change requires an explicit reference update.

### 10. Design-system enforcement

Goal: keep the implementation tied to the contract.

- Lint token usage and forbid ad hoc colours, radii, and spacing.
- Check brand-mark usage against `assets/brand/manifest.json`.
- Check that unavailable, pending, and failed states have text, not only colour.
- Review new components against the design system before merge.

Acceptance: a component that bypasses the tokens or the identity rules fails a
check or a review gate.

## Sequencing

1. Design-system enforcement and screenshot regression scaffolding.
2. Desktop target and approval workflow, reusing the CLI command contracts.
3. ChatGPT account and provider setup polish.
4. Browser activation.
5. Responsive, mobile, and accessibility passes.
6. Default-renderer visual QA and live-preview integration.

## Evidence required

- Visual QA on the default renderer at defined sizes.
- Keyboard and assistive-technology notes.
- Screenshot diffs for intentional changes.
- Command-level evidence that approval, cancellation, and unavailable states
  remain truthful.

## Risks

- A UI may expose a control before the Rust command is durable.
- A renderer-specific fix may not survive the default renderer.
- A responsive change may hide approvals or verification.
- Screenshot tests can become stale and give false confidence.

None of these risks is resolved by this plan alone; each workstream needs its
own acceptance evidence.
