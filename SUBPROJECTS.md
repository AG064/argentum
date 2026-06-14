# Subproject Policy

## Scope

This policy applies if additional source code repositories are created and compiled into releases alongside or separately from the main Argentum codebase.

## Requirements for Subprojects

If subprojects are added to the Argentum ecosystem:

1. **Security Parity**: Each subproject MUST enforce security requirements at least as strict as the main codebase.

2. **Required Security Controls**:
   - CI pipeline with SAST (CodeQL or equivalent)
   - Dependency scanning (Trivy, osv-scanner, or equivalent)
   - Security policy (SECURITY.md, SUPPORT.md)
   - SBOM generation for all release artifacts
   - Signed releases (Sigstore cosign or equivalent)

3. **Review Requirements**:
   - Code review required before merge (PR + approval)
   - Security scan must pass before release
   - Vulnerability disclosure policy must be in place

4. **Documentation**:
   - Each subproject must have its own SECURITY.md
   - Each subproject must have its own SUPPORT.md
   - Each subproject must document its security architecture

## Current Status

Currently, Argentum is a single monorepo with no subprojects. All security requirements apply to the main repository only.