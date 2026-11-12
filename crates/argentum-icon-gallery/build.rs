fn main() {
    println!("cargo:rerun-if-changed=../../ui/gallery");
    println!("cargo:rerun-if-changed=../../ui/assets/icons");
    println!("cargo:rerun-if-changed=../../ui/tokens.slint");
    slint_build::compile("../../ui/gallery/icon-gallery.slint")
        .expect("failed to compile the Argentum icon gallery");
}
