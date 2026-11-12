// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 AG064

fn main() {
    println!("cargo:rerun-if-changed=../../ui/gallery");
    println!("cargo:rerun-if-changed=../../ui/assets/icons");
    println!("cargo:rerun-if-changed=../../ui/tokens.slint");
    slint_build::compile("../../ui/gallery/icon-gallery.slint")
        .expect("failed to compile the Argentum icon gallery");
}
