# CLI polish roadmap

Status: active CLI plan for the current Genesis line. This file records what is
implemented and what remains; it does not rewrite the release roadmap, the
acceptance records, or the design system.

## Implemented

- Data-driven provider catalog with hosted, aggregator, local, custom, and
  unsupported groups, plus `provider presets` and catalog-driven `provider add`.
- Fresh-database schema and migration transactions, so first use does not commit
  every table and index separately.
- `doctor`, a read-only aggregate for workspace, database, providers, harness,
  verification, features, and recovery.
- Governed `desktop` commands for window listing, bounded inspection, bounded
  PNG capture, semantic invoke, bounded text, and bounded key chords.
- Approval-gated mutating desktop actions with a typed `--confirm HWND` that must
  match the target.
- Feature-gated desktop build reporting. A binary without `desktop-control`
  reports unavailable and exits nonzero instead of implying support.

## This task

Implemented:

- `argentum doctor [--json]`.
- `argentum desktop list [--json]`.
- `argentum desktop inspect --hwnd <0x...> [--depth N] [--json]`.
- `argentum desktop screenshot --hwnd <0x...> --output ABSOLUTE_FILE [--json]`.
- `argentum desktop invoke --hwnd <0x...> --element <json-or-selector> --confirm <HWND> [--json]`.
- `argentum desktop type --hwnd <0x...> --element <json-or-selector> --text TEXT [--mode value|keys] --confirm <HWND> [--json]`.
- `argentum desktop keys --hwnd <0x...> --element <json-or-selector> --chord CHORD [--chord CHORD ...] --confirm <HWND> [--json]`.
- Element selectors that accept a bounded inspection identity as JSON or a
  `path=0/1/2` selector with optional automation id, control type, class, name,
  and bounds filters.
- Help text no longer describes `doctor` as planned.

Counters and bounds:

- `--depth` 1 through 8.
- Inspection at most 256 elements and 64 KiB.
- Capture at most 8 MP and 32 KiB PNG, written with create-new semantics.
- `--chord` at most 8.
- Text at most 1024 UTF-16 code units.
- Window list at most 64 windows.

`doctor` required checks are database existence, read-only openability, and the
current schema. Workspace, provider, harness, and recovery checks are advisory;
`healthy` follows the required checks and the process exits nonzero when one
fails. The report never reads credentials, opens a provider connection, or
writes to the store.

## Remaining

### Doctor

- Add an explicit `--probe-local` mode only if it can stay read-only and free of
  provider requests.
- Report recovery detail beyond unfinished change operations once a read-only
  store surface exists.
- Add stable check identifiers and document the exit contract in `docs/CLI_GUIDE.md`.
- Add integration coverage for unreadable, outdated, and newer schemas.

### Desktop

- Add inspection filters for very large trees and a selector that can match on
  automation id alone without a path (requires a bounded lookup design).
- Add a capture comparison command only after a reviewed image-diff policy
  exists.
- Add an explicit dry-run mode for mutating commands that revalidates and prints
  the target without calling the pattern.
- Document the desktop threat model and per-command failure taxonomy in
  `docs/WINDOWS_APPLICATION_CONTROL.md`.

### General CLI

- Unify `--json` envelopes and exit codes across inspection commands.
- Add shell completion and a generated command reference.
- Audit every `Unavailable` stub and either implement it or remove it from help.
- Keep provider, credential, and recovery output free of secrets and private
  paths where the operator does not need them.
- Extend focused parser tests whenever a command grows a new flag family.

## Non-goals

- No UI changes in this line. UI work is tracked in `docs/plans/UI-REWORK.md`.
- No hosted test claim from local checks alone.
- No mutation hidden behind a read-only command.
- No desktop input in automated tests.
