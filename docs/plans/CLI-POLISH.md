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
- A stable JSON error envelope for `--json` inspection failures, with stable
  codes, sanitized messages, and a no-double-print rule for commands that already
  emit a structured report.

## Doctor and desktop work

Implemented:

- `argentum doctor [--json]`.
- `argentum doctor --probe-local`, a read-only local readiness probe for the
  optional browser and desktop capabilities.
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

`--probe-local` adds advisory checks for a configured Node executable,
`browser-control.stdio` definition, browser adapter files, Playwright modules,
the configured Chromium cache, and the desktop-control and artifacts features.
The probe inspects bounded local files and manifests only. It never spawns Node,
npm, npx or a browser and never uses the network, so it cannot change the exit
contract.

## Discoverability

- `argentum commands [--json]` reports the catalog without executing anything.
  Each entry has a group, command path, summary, runtime availability
  (`available`, `reserved`, `unavailable`), feature gate (`extensions`,
  `desktop-control`, `artifacts`, or none) and mutation flag.
- The catalog lives in `crates/argentum-cli/src/command_catalog.rs`. It has 117
  entries covering top-level commands and the provider, session, goal, harness,
  desktop, permissions, verification, changes, recovery, extensions and
  artifacts families.
- `docs/CLI_GUIDE.md` carries the command index generated from the catalog. A
  test fails if a catalog path is missing from the index.
- Shell completion is deferred. The catalog does not carry flag arity,
  positional types or value suggestions, so a generated PowerShell script would
  be incomplete and a hand-maintained list would drift from the parser. The
  limitation is documented instead of shipping a fragile list.

## Reserved-command audit

The parser recognizes these names but returns `Arguments::Unavailable`. None is
advertised as working. Help lists them once as reserved.

| Command | Classification | Reason and replacement |
| --- | --- | --- |
| `model` | reserved | Alias not planned as a separate command; use `provider models` or `provider model`. |
| `profile` | reserved | Alias not planned as a separate command; use `provider save` and `provider select`. |
| `module` | reserved | Alias not planned as a separate command; use `harness module`. |
| `plugin` | reserved | Package lifecycle is owned by the extensions family; use `extensions install` and `extensions configure`. |
| `mcp` | reserved | Server lifecycle is owned by the extensions family. |
| `migrate` | reserved | Schema migration runs on store open. An explicit command would need backup and verification design. |
| `prompt` | reserved | Prompt-template management has no current contract. |
| `self` | reserved | Self-management has no current contract. |
| `update` | reserved | Self-update needs signed artifact verification and rollback design; not implemented. |

## Remaining

### Doctor

- Report recovery detail beyond unfinished change operations once a read-only
  store surface exists.

### Desktop

- Add inspection filters for very large trees and a selector that can match on
  automation id alone without a path (requires a bounded lookup design).
- Add a capture comparison command only after a reviewed image-diff policy
  exists.

### General CLI

- Continue applying the documented JSON error contract to new commands as they
  add `--json`; success envelopes remain command-specific unless a compatible
  migration is designed.
- Keep provider, credential, and recovery output free of secrets and private
  paths where the operator does not need them.
- Extend focused parser tests whenever a command grows a new flag family.

## Non-goals

- UI work is tracked in `docs/plans/UI-REWORK.md`.
- No hosted test claim from local checks alone.
- No mutation hidden behind a read-only command.
- No desktop input in automated tests.
