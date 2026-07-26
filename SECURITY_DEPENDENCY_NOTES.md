# Security Vulnerability Notes

> **⚠️ VEX (Vulnerability Exploitability eXchange):** This document serves as a VEX for Argentum, providing exploitability assessments for known vulnerabilities. It augments vulnerability reports with non-exploitability details and mitigations.

## Validation snapshot - 2026-07-26

- `npm audit --omit=dev`: 0 known vulnerabilities across the resolved production tree after the v0.0.9 lockfile refresh.
- Full `npm audit` still reports high-severity development-toolchain advisories through the pinned ESLint and Jest dependency graph. The available automatic fix requires the breaking `eslint-plugin-unicorn@72` upgrade, so this remains a separate maintenance task rather than a release-time forced upgrade.
- Resolved checks: `body-parser@1.20.6`, `postcss@8.5.23`, `sanitize-html@2.17.4`, `ip-address@10.2.0`, `picomatch@4.0.4`, `brace-expansion@2.1.2`, `@anthropic-ai/sdk@0.95.1`, `tauri@2.11.2`.
- `cargo tree --target all -p glib --depth 0`: `glib@0.18.5`; Dependabot alert 16 remains open for the Linux-only Tauri/GTK dependency path documented below.
- `cargo-audit` and `osv-scanner` were not installed in this local environment, so Rust advisory status was not independently rescanned here. CI scanners remain required for release.

This is a time-bound lockfile snapshot, not a guarantee that no vulnerability exists.

## SCA Remediation Policy

### Severity Threshold for Remediations

| Severity                 | Remediation Timeline | Notes                                     |
| ------------------------ | -------------------- | ----------------------------------------- |
| Critical (CVSS 9.0-10.0) | Within 24 hours      | Emergency patch or mitigation required    |
| High (CVSS 7.0-8.9)      | Within 7 days        | Fix or documented exception required      |
| Medium (CVSS 4.0-6.9)    | Within 30 days       | Fix or risk acceptance with justification |
| Low (CVSS 0.1-3.9)       | Best effort          | Fix when feasible, document if not        |
| Info (CVSS 0.0)          | No action required   | Acknowledge and ignore                    |

### License Policy

| License Type                                        | Policy                                                             |
| --------------------------------------------------- | ------------------------------------------------------------------ |
| MIT, Apache 2.0, BSD-2/3, ISC, Unlicense, CC0, 0BSD | Generally allowed; preserve required notices/attribution           |
| GPL, LGPL, MPL, EPL and other reciprocal licenses   | Legal/maintainer review required before use or distribution        |
| AGPL, SSPL, BSL, noncommercial, source-available    | Blocked unless an explicit compatible commercial grant is approved |
| Unknown/Custom                                      | Legal review required before use                                   |

### Process for SCA Findings

1. **Identify:** Trivy scans run in CI on every push to development/main
2. **Prioritize:** Severity level determines remediation timeline (see table above)
3. **Remediate:** Fix via `npm audit fix`, update package.json overrides, or bump dependency version
4. **Document:** If not remediated, add to this document with justification (e.g., bundled npm modules, false positive)
5. **Verify:** Re-run Trivy to confirm finding is resolved

### Exceptions

- **Bundled npm modules** (inside `node_modules/npm/`): Not remediated directly — mitigated via `package.json` overrides where applicable
- **Rust advisories inherited through framework dependencies**: Do not blanket-ignore them. Record the exact affected target, reachable dependency path, upstream constraint, and remediation trigger below; keep the scanner finding open until it is fixed or formally accepted.
- **False positives**: Document as "Not a vulnerability" or "Not exploitable via project"

## Open Rust finding: `glib::VariantStrIter` unsoundness

- **Tracking:** GitHub Dependabot alert 16; medium severity; affected range `glib >=0.15.0, <0.20.0`; first patched version `0.20.0`.
- **Resolved version:** `glib@0.18.5` through `tauri -> gtk` on `x86_64-unknown-linux-gnu`.
- **Target scope:** `cargo tree --target x86_64-pc-windows-msvc -i glib` reports no dependency. The crate is present in the Linux Tauri/GTK graph and must be treated as affected for Linux builds.
- **Reachability:** Argentum does not directly import `glib` or use `VariantStrIter`. This reduces direct exposure but is not proof that the transitive GUI stack cannot reach the affected implementation.
- **Current constraint:** The Tauri Linux stack currently resolves the `gtk-rs 0.18` family, which cannot accept `glib 0.20` as a lockfile-only patch. Forcing only `glib` to `0.20` would create an incompatible dependency graph.
- **Disposition:** Open and monitored, not ignored and not marked non-exploitable. Upgrade the Tauri/GTK dependency family when it supports `glib >=0.20`, then run Linux desktop tests and close the alert only after the patched graph is verified.

## Overview

Trivy scans of the Docker image may report vulnerabilities in bundled npm internal modules. These are not direct project vulnerabilities and are documented here for tracking.

## Trivy Alerts (MEDIUM)

### CVE-2026-42338 — ip-address (10.1.0)

- **Location:** `node_modules/npm/node_modules/ip-address/package.json`
- **Issue:** Version 10.1.0 found in npm's bundled modules inside the Docker image
- **Project override:** `package.json` `overrides.ip-address` is set to `^10.2.0`
- **Status:** The project's direct dependency is correctly overridden. The Trivy finding is in npm's own internal cache, not a direct project dependency. This is inherited from the Docker base image and npm's bundling. No additional action required — the project override ensures the correct version is used when `npm install` runs.

### CVE-2026-42338 — picomatch (4.0.3)

- **Location:** `node_modules/npm/node_modules/picomatch/package.json`
- **Issue:** Version 4.0.3 found in npm's bundled modules inside the Docker image
- **Project override:** `package.json` `overrides.picomatch` is set to `^4.0.4`
- **Status:** Same situation as ip-address. The project override correctly pins picomatch to 4.0.4. Trivy scans npm's internal bundle, not the project's resolved dependency tree. No additional action required.

### CVE-2026-42338 — brace-expansion (denial of service via zero step value)

- **Location:** Transitive dependency via `minimatch@9.0.9`
- **Issue:** DoS via zero step value in brace-expansion
- **Project override:** `package.json` `overrides.brace-expansion` is set to `^2.0.2`
- **Actual resolved version:** `brace-expansion@2.1.0` (via minimatch/node_modules)
- **Status:** Project override is set to `^2.0.2`, and npm resolves to `2.1.0` which includes the fix. No additional action required.

## Resolution

All three vulnerabilities are already addressed via `package.json` overrides. The Trivy scan finds these in npm's internal bundled modules inside the Docker image because:

1. `npm ci` copies npm's internal bundled dependencies into `node_modules/.npm` cache
2. Trivy's container scan reads these bundled modules directly
3. The project-level overrides in `package.json` correctly resolve to patched versions during `npm install`

No further changes to the dependency chain are required.
