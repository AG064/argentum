// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 AG064

pub mod devices_ui;
mod local_addon;
pub mod native_host;
pub mod remote_control;
pub mod session_export;
pub mod startup_error;
pub mod ui_bridge;
#[cfg(windows)]
pub mod windows_dialogs;
pub mod workspace_choice;

pub use native_host::{
    run_native_host, NativeHostError, NativeHostOptions, NativeInferenceForeground,
};
pub use startup_error::{issue_report_url, startup_error_message};

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
