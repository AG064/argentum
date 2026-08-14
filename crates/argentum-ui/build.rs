fn main() {
    println!("cargo:rerun-if-changed=../../ui");
    println!("cargo:rerun-if-changed=../../assets/brand");
    slint_build::compile("../../ui/app.slint").expect("failed to compile Argentum Slint UI");
}
