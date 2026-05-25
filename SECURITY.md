# Security Vulnerabilities — Known Issues

## Active

### 1. `glib` unsoundness in `VariantStrIter` (RUSTSEC-2024-0429 / GHSA-wrw7-89jp-8q8g)

**Severity:** MEDIUM (unsoundness, undefined behaviour)  
**Package:** `glib` v0.18.5  
**Introduced by:** `tauri` v2.x → `gtk` v0.18 → `glib` v0.18.5  
**Fixed in:** `glib` ≥ 0.20.0 (but `gtk 0.18` requires `glib ^0.18`, preventing automatic upgrade)

**Impact:** `VariantStrIter::impl_get` passes an immutable reference to a C function that mutates it in-place. On modern Rust compilers with optimizations, this is UB and causes NULL pointer dereference crashes.

**Upstream fix:** Tauri v3.0 will migrate from GTK3 (`gtk 0.18`) to GTK4 (`gtk4`/`gdk4`), which depends on `glib ≥ 0.20`.

**Status:** ⏳ Waiting for Tauri v3.0  
**Tracking:**

- Tauri issue: [tauri-apps/tauri#14684](https://github.com/tauri-apps/tauri/pull/14684) — `feat(linux): migrate to GTK4 and WebKitGTK 6.0`
- Tauri milestone: [v3.0](https://github.com/tauri-apps/tauri/milestone/5) (14% complete as of 2026-05-22)
- Advisory: [RUSTSEC-2024-0429](https://rustsec.org/advisories/RUSTSEC-2024-0429)
- Advisory: [GHSA-wrw7-89jp-8q8g](https://github.com/advisories/GHSA-wrw7-89jp-8q8g)

**Workaround:** Trivy dependency-scan workflow suppresses this vulnerability (GHSA-wrw7-89jp-8q8g) because no compatible fix exists upstream. Re-evaluate when Tauri v3.0 is released.

**What to do when Tauri v3.0 is out:**

1. Update `tauri` in `src/desktop/Cargo.toml` to v3.x
2. Run `cargo update` to refresh lockfile
3. Verify `glib` is ≥ 0.20.0 (`cargo tree -p glib`)
4. Remove suppression from `.github/workflows/dependency-scan.yml`
5. Run full Trivy scan to confirm no vulnerabilities

---

### 2. GTK3 / gtk-rs deprecation (RUSTSEC-2024-0412, -0413, -0416)

**Severity:** INFO / Warning (unmaintained)  
**Packages:** `atk`, `atk-sys`, `gdk`, `gtk`, `gtk-sys` at v0.18.x  
**Status:** These GTK3 bindings are unmaintained since 2024-03-04. Tauri v3.0 migrates to GTK4 which resolves this.

---

## Resolved

_(None currently — all known issues are either suppressed pending upstream fix or still under investigation)_
