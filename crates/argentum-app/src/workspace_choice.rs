// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 AG064

//! What happens to a folder the owner picked in Settings > Workspace. The
//! picker only fills in the path. It goes through the same check as a typed
//! path, and nothing opens until the owner presses Open and restart, which
//! checks it again when the choice is saved.

use std::path::{Path, PathBuf};

use argentum_platform::{shared_path_from_tree, validate_shared_workspace, SharedWorkspaceRefusal};

/// The result of one use of the folder picker.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WorkspaceChoice {
    /// The owner closed the picker without choosing.
    Cancelled,
    /// A folder that passed the workspace check, as its resolved path.
    Ready(PathBuf),
    /// A choice that failed the check, with the reason to show.
    Refused(&'static str),
}

pub const NOT_A_FOLDER: &str = "That folder cannot be opened. Choose a folder on this computer.";
pub const PICKER_FAILED: &str = "The folder picker could not open. Type the path instead.";

/// Checks a picked folder the same way a typed path is checked.
pub fn check_picked_folder(picked: Option<&Path>) -> WorkspaceChoice {
    let Some(picked) = picked else {
        return WorkspaceChoice::Cancelled;
    };
    match argentum_platform::check_workspace_folder(picked) {
        Ok(path) => WorkspaceChoice::Ready(path),
        Err(_) => WorkspaceChoice::Refused(NOT_A_FOLDER),
    }
}

/// How a resolved path is shown in the text field. Windows resolves to a
/// `\\?\` path, which the owner should not have to read.
pub fn display_path(path: &Path) -> String {
    let text = path.display().to_string();
    if let Some(rest) = text.strip_prefix(r"\\?\UNC\") {
        return format!(r"\\{rest}");
    }
    text.strip_prefix(r"\\?\").unwrap_or(&text).to_owned()
}

/// What the Android folder picker returned.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PickedFolder {
    Cancelled,
    Invalid,
    Tree {
        authority: String,
        document_id: String,
    },
}

/// Reads the Android picker result string from `ArgentumSharedStorage`:
/// `cancel`, `invalid`, or `tree`, the authority and the tree document ID on
/// separate lines.
pub fn parse_picked(value: &str) -> PickedFolder {
    let mut parts = value.splitn(3, '\n');
    match (parts.next(), parts.next(), parts.next()) {
        (Some("cancel"), None, None) => PickedFolder::Cancelled,
        (Some("tree"), Some(authority), Some(document_id))
            if !authority.is_empty() && !document_id.contains('\n') =>
        {
            PickedFolder::Tree {
                authority: authority.to_owned(),
                document_id: document_id.to_owned(),
            }
        }
        _ => PickedFolder::Invalid,
    }
}

/// Maps an Android picker result to a checked folder path, as the text shown
/// in the field, or the reason it cannot be used. None when cancelled.
pub fn picked_to_path(
    picked: &PickedFolder,
    primary_root: Option<&Path>,
) -> Option<Result<String, SharedWorkspaceRefusal>> {
    match picked {
        PickedFolder::Cancelled => None,
        PickedFolder::Invalid => Some(Err(SharedWorkspaceRefusal::NotLocalStorage)),
        PickedFolder::Tree {
            authority,
            document_id,
        } => {
            let Some(root) = primary_root else {
                return Some(Err(SharedWorkspaceRefusal::OutsideSharedStorage));
            };
            Some(
                shared_path_from_tree(authority, document_id, root)
                    .and_then(|path| validate_shared_workspace(&path, root))
                    .map(|workspace| workspace.path().display().to_string()),
            )
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use argentum_platform::AppPaths;

    #[test]
    fn a_cancelled_picker_changes_nothing() {
        assert_eq!(check_picked_folder(None), WorkspaceChoice::Cancelled);
    }

    #[test]
    fn a_picked_folder_resolves_and_saves_through_the_existing_check() {
        let directory = tempfile::tempdir().expect("directory");
        let folder = directory.path().join("project");
        std::fs::create_dir(&folder).expect("folder");
        let WorkspaceChoice::Ready(resolved) = check_picked_folder(Some(&folder)) else {
            panic!("a real folder is accepted");
        };
        assert_eq!(resolved, std::fs::canonicalize(&folder).expect("canonical"));

        // The text field shows the plain path, and that path is what Open and
        // restart saves. It must pass the save check and load back the same.
        let shown = display_path(&resolved);
        assert!(!shown.starts_with(r"\\?\"));
        let paths = AppPaths::from_data_dir(directory.path().join("data"));
        let saved = paths.save_workspace(Path::new(&shown)).expect("saved");
        assert_eq!(saved, resolved);
        assert_eq!(paths.load_workspace().expect("load"), Some(resolved));
    }

    #[test]
    fn a_picked_file_or_missing_folder_is_refused() {
        let directory = tempfile::tempdir().expect("directory");
        let file = directory.path().join("notes.txt");
        std::fs::write(&file, "text").expect("file");
        assert_eq!(
            check_picked_folder(Some(&file)),
            WorkspaceChoice::Refused(NOT_A_FOLDER)
        );
        let missing = directory.path().join("gone");
        assert_eq!(
            check_picked_folder(Some(&missing)),
            WorkspaceChoice::Refused(NOT_A_FOLDER)
        );
        // A refused choice is never saved.
        let paths = AppPaths::from_data_dir(directory.path().join("data"));
        assert!(paths.save_workspace(&missing).is_err());
        assert_eq!(paths.load_workspace().expect("load"), None);
    }

    #[test]
    fn a_picked_folder_with_parent_steps_resolves_to_the_real_folder() {
        let directory = tempfile::tempdir().expect("directory");
        let folder = directory.path().join("a");
        std::fs::create_dir_all(folder.join("b")).expect("folders");
        let WorkspaceChoice::Ready(resolved) =
            check_picked_folder(Some(&folder.join("b").join("..")))
        else {
            panic!("accepted");
        };
        assert_eq!(resolved, std::fs::canonicalize(&folder).expect("canonical"));
    }

    #[test]
    fn android_picker_results_parse_strictly() {
        assert_eq!(parse_picked("cancel"), PickedFolder::Cancelled);
        assert_eq!(parse_picked("invalid"), PickedFolder::Invalid);
        assert_eq!(parse_picked(""), PickedFolder::Invalid);
        assert_eq!(parse_picked("tree\n\nprimary:x"), PickedFolder::Invalid);
        assert_eq!(parse_picked("cancel\nextra"), PickedFolder::Invalid);
        assert_eq!(
            parse_picked("tree\ncom.android.externalstorage.documents\nprimary:Documents/x"),
            PickedFolder::Tree {
                authority: "com.android.externalstorage.documents".into(),
                document_id: "primary:Documents/x".into(),
            }
        );
    }

    #[test]
    fn an_android_pick_is_mapped_and_checked_before_it_fills_the_field() {
        let directory = tempfile::tempdir().expect("directory");
        let root = directory.path().join("0");
        std::fs::create_dir_all(root.join("Documents").join("app")).expect("folders");
        let tree = |id: &str| PickedFolder::Tree {
            authority: argentum_platform::EXTERNAL_STORAGE_AUTHORITY.into(),
            document_id: id.into(),
        };
        assert_eq!(picked_to_path(&PickedFolder::Cancelled, Some(&root)), None);
        let Some(Ok(shown)) = picked_to_path(&tree("primary:Documents/app"), Some(&root)) else {
            panic!("a folder in shared storage is accepted");
        };
        assert_eq!(
            Path::new(&shown),
            std::fs::canonicalize(root.join("Documents").join("app")).expect("canonical")
        );
        assert_eq!(
            picked_to_path(&tree("primary:"), Some(&root)),
            Some(Err(SharedWorkspaceRefusal::StorageRoot))
        );
        assert_eq!(
            picked_to_path(&tree("ABCD-1234:x"), Some(&root)),
            Some(Err(SharedWorkspaceRefusal::OtherVolume))
        );
        assert_eq!(
            picked_to_path(
                &PickedFolder::Tree {
                    authority: "com.google.android.apps.docs.storage".into(),
                    document_id: "x".into(),
                },
                Some(&root)
            ),
            Some(Err(SharedWorkspaceRefusal::NotLocalStorage))
        );
        assert_eq!(
            picked_to_path(&tree("primary:Documents/app"), None),
            Some(Err(SharedWorkspaceRefusal::OutsideSharedStorage))
        );
        assert_eq!(
            picked_to_path(&PickedFolder::Invalid, Some(&root)),
            Some(Err(SharedWorkspaceRefusal::NotLocalStorage))
        );
    }

    #[cfg(windows)]
    #[test]
    fn verbatim_paths_are_shown_plainly() {
        assert_eq!(display_path(Path::new(r"\\?\C:\work\app")), r"C:\work\app");
        assert_eq!(
            display_path(Path::new(r"\\?\UNC\server\share\app")),
            r"\\server\share\app"
        );
        assert_eq!(display_path(Path::new(r"C:\work")), r"C:\work");
    }
}
