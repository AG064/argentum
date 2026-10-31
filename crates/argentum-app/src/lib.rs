pub mod native_host;
pub mod ui_bridge;

pub use native_host::{
    run_native_host, NativeHostError, NativeHostOptions, NativeInferenceForeground,
};

#[cfg(test)]
mod tests {
    use argentum_ui::display_workspace_label;
    use std::path::Path;

    #[test]
    fn workspace_labels_use_the_final_directory_name() {
        let path = Path::new("projects").join("workspace");
        assert_eq!(display_workspace_label(&path), "workspace");
    }

    #[cfg(windows)]
    #[test]
    fn workspace_labels_strip_verbatim_prefixes() {
        assert_eq!(
            display_workspace_label(Path::new(r"\\?\A:\CodexData\temp\example\workspace")),
            "workspace"
        );
        assert_eq!(
            display_workspace_label(Path::new(r"\\?\UNC\server\share\project")),
            "project"
        );
        assert_eq!(
            display_workspace_label(Path::new(r"C:\projects\example")),
            "example"
        );
    }
}
