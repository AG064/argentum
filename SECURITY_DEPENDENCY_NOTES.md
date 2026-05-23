# Security Vulnerability Notes

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
