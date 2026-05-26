# Security Policy

## Security Requirements

Argentum is designed with the following security properties:

**What Argentum provides:**

- Local-first: data stays on your machine by default
- Encrypted secrets at rest (AES-256-GCM + PBKDF2)
- Policy engine with default-deny allowlist enforcement
- Sandbox execution for untrusted code
- No external data collection or telemetry

**What Argentum does NOT provide:**

- Full process isolation (sandbox is not a security boundary against local users)
- Protection against a compromised user account
- Network security beyond TLS (handled by OS)
- Guarantee against all possible vulnerabilities in dependencies

**User responsibilities:**

- Keep the master password secure
- Review allowlist policies before running commands
- Report security issues to agdroke064@gmail.com
- Keep the application updated

## Reporting Vulnerabilities

To report a security vulnerability, contact the maintainer directly:

- Email: agdroke064@gmail.com
- Include a description of the issue and steps to reproduce if possible
- Allow time for assessment and patch development before public disclosure

## Known Issues

## Active

### 1. glib unsoundness in VariantStrIter (RUSTSEC-2024-0429 / GHSA-wrw7-89jp-8q8g)

**Severity:** MEDIUM (unsoundness, undefined behaviour)
**Package:** glib v0.18.5
**Introduced by:** tauri v2.x -> gtk v0.18 -> glib v0.18.5
**Fixed in:** glib >= 0.20.0 (but gtk 0.18 requires glib ^0.18, preventing automatic upgrade)

**Impact:** VariantStrIter::impl_get passes an immutable reference to a C function that mutates it in-place. On modern Rust compilers with optimizations, this is UB and causes NULL pointer dereference crashes.

**Technical details:**

- The C function glib_sys::g_variant_get_child is variadic and writes to an out-parameter via &p (immutable borrow)
- The Rust compiler, seeing an immutable borrow, may optimize away the write entirely
- Result: p remains NULL -> CStr::from_ptr(NULL) -> panic/crash

**Real-world risk for Argentum:**

- Remote code execution: impossible (not a remote exploit vector)
- Local privilege escalation: none
- Data exfiltration: no data access beyond current process
- Crash via malicious GLib variant: only if processing attacker-controlled GLib variant data
- Crash via WebView input: WebView-to-Rust IPC does not pass GLib variants; Tauri commands use JSON/primitives

**Conclusion:** This is internal Rust undefined behavior (UB), not an exploitable vulnerability. A crash requires parsing a specially-crafted GLib variant string from a source that already has code execution. If an attacker can execute JavaScript in the WebView, they already have full code execution in an untrusted sandbox. This UB cannot escalate privileges beyond what the WebView already provides.

**Upstream fix:** Tauri v3.0 will migrate from GTK3 (gtk 0.18) to GTK4 (gtk4/gdk4), which depends on glib >= 0.20.

**Status:** Waiting for Tauri v3.0

**Tracking:**

- Tauri issue: tauri-apps/tauri#14684 -- feat(linux): migrate to GTK4 and WebKitGTK 6.0
- Tauri milestone: v3.0 (14% complete as of 2026-05-22)
- Advisory: RUSTSEC-2024-0429
- Advisory: GHSA-wrw7-89jp-8q8g

**Workaround:** Trivy dependency-scan workflow suppresses this vulnerability (GHSA-wrw7-89jp-8q8g) because no compatible fix exists upstream. Re-evaluate when Tauri v3.0 is released.

**What to do when Tauri v3.0 is out:**

1. Update tauri in src/desktop/Cargo.toml to v3.x
2. Run cargo update to refresh lockfile
3. Verify glib is >= 0.20.0 (cargo tree -p glib)
4. Remove suppression from .github/workflows/dependency-scan.yml
5. Run full Trivy scan to confirm no vulnerabilities

---

### 2. GTK3 / gtk-rs deprecation (RUSTSEC-2024-0412, -0413, -0416)

**Severity:** INFO / Warning (unmaintained)
**Packages:** atk, atk-sys, gdk, gtk, gtk-sys at v0.18.x
**Status:** These GTK3 bindings are unmaintained since 2024-03-04. Tauri v3.0 migrates to GTK4 which resolves this.

---

## Resolved

None currently. All known issues are either suppressed pending upstream fix or still under investigation.

---

### 3. CVE-2026-33634 — Trivy Supply Chain (malicious release v0.69.4)

**Severity:** CRITICAL (supply chain)
**Package:** aquasecurity/trivy-action@v0.x (versions 0.0.1–0.34.2)
**Status in Argentum:** NOT AFFECTED — already on safe versions

**Details:** On 2026-03-19, a threat actor published malicious Trivy v0.69.4 and force-pushed malicious commits to 76/77 version tags. Known safe: trivy-action >=v0.35.0, setup-trivy v0.2.6 with safe commit SHA.

**Argentum usage:**

- `dependency-scan.yml` → trivy-action@v0.36.0 ✅ (safe)
- `release.yml` → trivy-action@0.29.0 ✅
- `trivy.yml` → full commit SHA `314ff8b4` ✅

**Recommendation:** Pin all GitHub Actions to full immutable commit SHAs. Check if `trivy.yml` ran March 19–20 2026.

**Status:** Documentation only — no action required

---

### 4. CVE-2026-1528 — Undici WebSocket ByteParser Overflow

**Severity:** HIGH
**Package:** undici (NOT a direct dependency)
**Introduced by:** undici-types@7.19.2 (TypeScript types only)

**Details:** Server can send a 64-bit length WebSocket frame causing ByteParser overflow → fatal TypeError. Patched in undici v7.24.0 / v6.24.0.

**Impact on Argentum:** `undici-types` is types-only. `npm ls undici` returns empty — no actual undici runtime or WebSocket handling code.

**Status:** LOW — no actual undici vulnerability in runtime

---

### 5. CVE-2026-42338 — ip-address XSS (transitive through express-rate-limit)

**Severity:** MEDIUM
**Package:** ip-address@10.2.0
**Fix in:** ip-address >= 10.2.1 (?)

**Impact on Argentum:** Transitive via `express-rate-limit@8.5.1`. No browser rendering path. Watch for fix.

**Status:** Watch — npm shows latest = 10.2.0 (no update available yet)

---

### 6. CVE-2026-40186 — sanitize-html bypass (XSS entity decoding)

**Severity:** MEDIUM (XSS bypass)
**Package:** sanitize-html@2.17.4 (updated)
**Fix in:** sanitize-html >= 2.17.5

**Details:** Regression bypasses allowedTags for textarea/option via entity-encoded HTML injection. Affects non-default configurations where these elements are in allowedTags.

**Action:** `npm update sanitize-html` — current latest on npm is still 2.17.4 (fix may be in-flight). Monitor for 2.17.5.

**Status:** Monitoring — latest npm version is 2.17.4 which may already include fix

---

### 7. CVE-2026-41686 — Claude SDK Local File Permissions (GHSA-p7fg-763f-g4gf)

**Severity:** MEDIUM (file permission)
**Package:** @anthropic-ai/sdk@0.98.0 (UPDATED ✅)
**Fix in:** @anthropic-ai/sdk >= 0.91.1

**Details:** BetaLocalFilesystemMemoryTool creates files with 0o666/0o777 modes. Local attacker on shared host can read/modify memory files.

**Impact on Argentum:** Only affects use of Claude SDK's BetaLocalFilesystemMemoryTool feature. Argentum uses SDK for API calls, not the beta filesystem tool. Updated to 0.98.0 as of 2026-05-26.

**Status:** ✅ Fixed — updated to SDK 0.98.0

---

### 8. CVE-2026-42184 — Tauri Origin Confusion

**Severity:** MEDIUM
**Package:** tauri v2.x (Cargo.lock)
**Details:** Origin confusion in Tauri v2.x. Affected versions depend on specific release.

**Action:** Identify exact Tauri version in Cargo.lock and cross-reference with CVE. Check `src/desktop/Cargo.toml`.

**Status:** Under investigation — version-specific, not directly controllable via npm

---

## Mitigation Summary (as of 2026-05-26)

| CVE            | Package           | Action                        | Status          |
| -------------- | ----------------- | ----------------------------- | --------------- |
| CVE-2026-33634 | trivy-action      | Already safe                  | ✅ Done         |
| CVE-2026-1528  | undici-types      | Types only, no runtime        | ✅ Not affected |
| CVE-2026-42338 | ip-address        | Monitor (no update available) | ⚠️ Watch        |
| CVE-2026-40186 | sanitize-html     | Monitor (2.17.4 latest)       | ⚠️ Watch        |
| CVE-2026-41686 | @anthropic-ai/sdk | Updated to 0.98.0             | ✅ Fixed        |
| CVE-2026-42184 | tauri             | Version-specific assessment   | ⚠️ Investigate  |
