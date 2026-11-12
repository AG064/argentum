use std::env;
#[cfg(windows)]
use std::path::PathBuf;

fn main() {
    println!("cargo:rerun-if-changed=../../assets/brand/argentum.ico");

    let targets_windows = env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows");
    if !targets_windows {
        return;
    }

    #[cfg(not(windows))]
    panic!("Windows release resources require a Windows build host");

    #[cfg(windows)]
    embed_windows_resources();
}

#[cfg(windows)]
fn embed_windows_resources() {
    let icon_path =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../assets/brand/argentum.ico");
    let icon_path = icon_path
        .canonicalize()
        .expect("failed to resolve the canonical Argentum Windows icon");
    let icon_path = icon_path
        .to_str()
        .expect("Argentum Windows icon path is not valid UTF-8");

    let version = env::var("CARGO_PKG_VERSION").expect("Cargo package version is unavailable");
    let mut resource = winresource::WindowsResource::new();
    resource
        .set_icon(icon_path)
        .set("ProductName", "Argentum")
        .set("FileDescription", "Argentum native task workbench")
        .set("OriginalFilename", "argentum.exe")
        .set("ProductVersion", &version)
        .set("FileVersion", &version);
    resource
        .compile()
        .expect("failed to compile Argentum Windows identity resources");
}
