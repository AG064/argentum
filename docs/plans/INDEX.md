# Argentum plans index

This directory holds long-form structured plans that are
**separate from the active rewrite** on `UI_redesign`. Each plan
lives on a dedicated branch and is intended to be reviewed,
edited, and backed into the master plan
([`docs/PREMIUM_AGENT_HARNESS_PLAN.md`](../PREMIUM_AGENT_HARNESS_PLAN.md))
one plan at a time.

## Active plans

| Plan | Date | Branch | Status |
| --- | --- | --- | --- |
| [ADE / Agent Harness Landscape Plan](ade-harness-landscape-2026-08-14.md) | 2026-08-14 | `plan/ade-harness-landscape-2026-08-14` | research and structured plans |

## Conventions

- A plan is a **structured research document**, not code.
- A plan lives on a branch whose name starts with `plan/`.
- A plan is **never** committed directly to `UI_redesign` or
  `development`; it is merged in via PR.
- A plan is **backed** into the master plan or the roadmap
  one plan at a time. Each plan A–N (or equivalent) in the
  ADE landscape plan becomes a follow-up PR with code, tests,
  and a release-gate update.
- A plan that has been **fully absorbed** into the master plan
  moves to "Archive" below.

## How to add a plan

1. Branch from the current default (`UI_redesign` or its
   successor) with a name like `plan/<topic>-<date>`.
2. Add a new file under `docs/plans/` with a stable
   `kebab-case-date.md` name.
3. Add a row to the "Active plans" table above.
4. Open a PR when the plan is ready for review.

## Archive

(none yet)
