# Security Policy

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
