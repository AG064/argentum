fn main() {
    println!("cargo:rerun-if-changed=../../ui");
    println!("cargo:rerun-if-changed=../../assets/brand");
    std::thread::Builder::new()
        .name("slint-ui-compiler".into())
        .stack_size(32 * 1024 * 1024)
        .spawn(|| slint_build::compile("../../ui/app.slint"))
        .expect("failed to start Argentum Slint UI compiler")
        .join()
        .expect("Argentum Slint UI compiler panicked")
        .expect("failed to compile Argentum Slint UI");
}
