//! An optional Android workspace on primary shared storage, such as
//! `/storage/emulated/0/Documents/project`.
//!
//! The app-private folder stays the default and the fallback. The owner can
//! choose a shared folder only after granting "All files access" in Android
//! settings. The choice is a real path, so the existing path-based workspace
//! code (broker, tools, approvals, change records) applies unchanged.
//!
//! The choice is kept in its own small file in the app-private data folder and
//! checked again at every start. A choice that fails the check, or a missing
//! permission, opens the app-private folder instead; nothing is deleted.

use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Component, Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::{metadata_is_path_redirect, AppPaths};

/// The documents provider for local shared storage. Trees from any other
/// provider (Downloads, cloud drives, other apps) have no real path.
pub const EXTERNAL_STORAGE_AUTHORITY: &str = "com.android.externalstorage.documents";

const CHOICE_FILE: &str = "shared-workspace.json";
const MAX_CHOICE_BYTES: u64 = 8 * 1024;
const MAX_PATH_BYTES: usize = 4096;

/// Why a picked or typed folder cannot be the workspace. Each message is
/// shown to the owner as is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SharedWorkspaceRefusal {
    /// The tree came from a provider other than local shared storage.
    NotLocalStorage,
    /// The tree is on an SD card or USB drive.
    OtherVolume,
    /// The path is outside primary shared storage.
    OutsideSharedStorage,
    /// The whole of shared storage, which is too broad for a workspace.
    StorageRoot,
    /// Under `Android/`, where other apps keep their own files.
    AppDataFolder,
    /// Missing, not a folder, or not readable.
    NotAFolder,
    /// All files access is off.
    PermissionMissing,
}

impl SharedWorkspaceRefusal {
    pub fn message(self) -> &'static str {
        match self {
            Self::NotLocalStorage => {
                "That folder is not on this phone's storage, so Argentum cannot use it as a workspace. Choose a folder under Internal storage."
            }
            Self::OtherVolume => {
                "Folders on an SD card or USB drive are not supported yet. Choose a folder under Internal storage."
            }
            Self::OutsideSharedStorage => {
                "Choose a folder under Internal storage, such as /storage/emulated/0/Documents/my-project."
            }
            Self::StorageRoot => {
                "The whole of Internal storage is too broad. Choose or create a folder inside it."
            }
            Self::AppDataFolder => {
                "Folders under Android/ belong to other apps. Choose a folder outside it."
            }
            Self::NotAFolder => "That folder cannot be opened.",
            Self::PermissionMissing => {
                "Turn on All files access for Argentum first. Argentum can then open folders in Internal storage."
            }
        }
    }
}

impl std::fmt::Display for SharedWorkspaceRefusal {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.message())
    }
}

impl std::error::Error for SharedWorkspaceRefusal {}

/// A shared-storage folder that passed [`validate_shared_workspace`].
///
/// It cannot be built from an arbitrary path:
///
/// ```compile_fail
/// use argentum_platform::ValidatedSharedWorkspace;
/// let workspace = ValidatedSharedWorkspace { path: "/".into() };
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidatedSharedWorkspace {
    path: PathBuf,
}

impl ValidatedSharedWorkspace {
    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn into_path(self) -> PathBuf {
        self.path
    }
}

/// Maps a folder picked in the system picker to its real path.
///
/// `tree_document_id` is the tree's document ID from
/// `DocumentsContract.getTreeDocumentId`, such as `primary:Documents/app`.
/// Only the `primary` volume, and `home` (which is `Documents` on primary), map
/// to a path. The result still needs [`validate_shared_workspace`].
pub fn shared_path_from_tree(
    authority: &str,
    tree_document_id: &str,
    primary_root: &Path,
) -> Result<PathBuf, SharedWorkspaceRefusal> {
    if authority != EXTERNAL_STORAGE_AUTHORITY {
        return Err(SharedWorkspaceRefusal::NotLocalStorage);
    }
    let (volume, relative) = tree_document_id
        .split_once(':')
        .ok_or(SharedWorkspaceRefusal::NotLocalStorage)?;
    let base = match volume {
        "primary" => primary_root.to_path_buf(),
        "home" => primary_root.join("Documents"),
        _ => return Err(SharedWorkspaceRefusal::OtherVolume),
    };
    if relative.len() > MAX_PATH_BYTES || relative.contains('\0') || relative.contains('\\') {
        return Err(SharedWorkspaceRefusal::NotAFolder);
    }
    let mut path = base;
    for segment in relative.split('/') {
        match segment {
            "" => continue,
            "." | ".." => return Err(SharedWorkspaceRefusal::NotAFolder),
            name => path.push(name),
        }
    }
    Ok(path)
}

/// Checks that `candidate` can be the workspace: a readable folder inside
/// primary shared storage, below its root, and not under `Android/`. Links are
/// resolved first, so a link cannot point the workspace anywhere else.
pub fn validate_shared_workspace(
    candidate: &Path,
    primary_root: &Path,
) -> Result<ValidatedSharedWorkspace, SharedWorkspaceRefusal> {
    if !candidate.is_absolute()
        || candidate.as_os_str().len() > MAX_PATH_BYTES
        || candidate
            .components()
            .any(|component| matches!(component, Component::ParentDir | Component::CurDir))
    {
        return Err(SharedWorkspaceRefusal::OutsideSharedStorage);
    }
    let root =
        fs::canonicalize(primary_root).map_err(|_| SharedWorkspaceRefusal::OutsideSharedStorage)?;
    let canonical = fs::canonicalize(candidate).map_err(|_| SharedWorkspaceRefusal::NotAFolder)?;
    let Ok(relative) = canonical.strip_prefix(&root) else {
        return Err(SharedWorkspaceRefusal::OutsideSharedStorage);
    };
    let mut components = relative.components();
    match components.next() {
        None => return Err(SharedWorkspaceRefusal::StorageRoot),
        Some(Component::Normal(first)) if first.eq_ignore_ascii_case("Android") => {
            return Err(SharedWorkspaceRefusal::AppDataFolder);
        }
        Some(Component::Normal(_)) => {}
        Some(_) => return Err(SharedWorkspaceRefusal::OutsideSharedStorage),
    }
    if !canonical.is_dir() || fs::read_dir(&canonical).is_err() {
        return Err(SharedWorkspaceRefusal::NotAFolder);
    }
    Ok(ValidatedSharedWorkspace { path: canonical })
}

#[derive(Debug, Serialize, Deserialize)]
struct SharedWorkspaceChoice {
    path: PathBuf,
}

impl AppPaths {
    fn shared_workspace_file(&self) -> PathBuf {
        self.data_dir.join(CHOICE_FILE)
    }

    /// The shared-storage folder the owner chose, if any. It is not checked
    /// here; pass it through [`validate_shared_workspace`] before use. An
    /// unreadable or malformed record reads as no choice.
    pub fn load_shared_workspace_choice(&self) -> Option<PathBuf> {
        let file = self.shared_workspace_file();
        let metadata = fs::symlink_metadata(&file).ok()?;
        if metadata_is_path_redirect(&metadata)
            || !metadata.is_file()
            || metadata.len() > MAX_CHOICE_BYTES
        {
            return None;
        }
        let mut payload = String::new();
        File::open(&file)
            .ok()?
            .take(MAX_CHOICE_BYTES + 1)
            .read_to_string(&mut payload)
            .ok()?;
        if payload.len() as u64 > MAX_CHOICE_BYTES {
            return None;
        }
        serde_json::from_str::<SharedWorkspaceChoice>(&payload)
            .ok()
            .map(|choice| choice.path)
    }

    /// Saves a checked shared-storage folder as the workspace for the next
    /// start. The app-private workspace record is not touched.
    pub fn save_shared_workspace_choice(
        &self,
        workspace: &ValidatedSharedWorkspace,
    ) -> Result<(), SharedWorkspaceRefusal> {
        let file = self.shared_workspace_file();
        let failed = |_| SharedWorkspaceRefusal::NotAFolder;
        fs::create_dir_all(&self.data_dir).map_err(failed)?;
        if let Ok(metadata) = fs::symlink_metadata(&file) {
            if metadata_is_path_redirect(&metadata) || !metadata.is_file() {
                return Err(SharedWorkspaceRefusal::NotAFolder);
            }
        }
        let payload = serde_json::to_vec(&SharedWorkspaceChoice {
            path: workspace.path.clone(),
        })
        .map_err(|_| SharedWorkspaceRefusal::NotAFolder)?;
        let temporary = file.with_file_name(format!("{CHOICE_FILE}.{}.tmp", std::process::id()));
        let _ = fs::remove_file(&temporary);
        let mut output = OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&temporary)
            .map_err(failed)?;
        if output
            .write_all(&payload)
            .and_then(|()| output.sync_all())
            .is_err()
        {
            drop(output);
            let _ = fs::remove_file(&temporary);
            return Err(SharedWorkspaceRefusal::NotAFolder);
        }
        drop(output);
        let _ = fs::remove_file(&file);
        fs::rename(&temporary, &file).map_err(|error| {
            let _ = fs::remove_file(&temporary);
            failed(error)
        })
    }

    /// Forgets the shared-storage folder, so the next start opens the
    /// app-private workspace.
    pub fn clear_shared_workspace_choice(&self) -> bool {
        let file = self.shared_workspace_file();
        match fs::symlink_metadata(&file) {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => true,
            Err(_) => false,
            Ok(metadata) if !metadata.is_file() && !metadata.file_type().is_symlink() => false,
            Ok(_) => fs::remove_file(&file).is_ok(),
        }
    }
}

/// What the Android host opens at start, given the saved choice and whether
/// All files access is on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SharedWorkspaceStart {
    /// No shared folder was chosen. Open the app-private workspace.
    AppPrivate,
    /// Open this shared folder.
    Shared(ValidatedSharedWorkspace),
    /// A shared folder was chosen but cannot be opened now. Open the
    /// app-private workspace and show the notice. The choice is kept, so
    /// turning the permission back on restores it.
    Fallback { chosen: PathBuf, notice: String },
}

/// Decides which workspace the Android host opens at start.
pub fn shared_workspace_at_start(
    paths: &AppPaths,
    all_files_access: bool,
    primary_root: Option<&Path>,
) -> SharedWorkspaceStart {
    let Some(chosen) = paths.load_shared_workspace_choice() else {
        return SharedWorkspaceStart::AppPrivate;
    };
    let shown = chosen.display().to_string();
    if !all_files_access {
        return SharedWorkspaceStart::Fallback {
            notice: format!(
                "All files access is off, so Argentum opened its own folder instead of {shown}. Turn it on in Settings > Workspace to go back."
            ),
            chosen,
        };
    }
    let Some(primary_root) = primary_root else {
        return SharedWorkspaceStart::Fallback {
            notice: format!(
                "Internal storage is not available, so Argentum opened its own folder instead of {shown}."
            ),
            chosen,
        };
    };
    match validate_shared_workspace(&chosen, primary_root) {
        Ok(workspace) => SharedWorkspaceStart::Shared(workspace),
        Err(refusal) => SharedWorkspaceStart::Fallback {
            notice: format!(
                "Argentum opened its own folder because {shown} cannot be used: {}",
                refusal.message()
            ),
            chosen,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Layout {
        _directory: tempfile::TempDir,
        root: PathBuf,
        paths: AppPaths,
    }

    /// A stand-in for /storage/emulated/0 and the app's private data folder.
    fn layout() -> Layout {
        let directory = tempfile::tempdir().expect("directory");
        let root = directory.path().join("emulated").join("0");
        for folder in ["Documents/app", "Download", "Android/data/other.app"] {
            fs::create_dir_all(root.join(folder)).expect("folder");
        }
        fs::write(root.join("Documents/notes.txt"), "text").expect("file");
        let paths = AppPaths::from_data_dir(directory.path().join("private").join("files"));
        Layout {
            _directory: directory,
            root,
            paths,
        }
    }

    #[test]
    fn primary_and_home_trees_map_to_real_paths() {
        let root = Path::new("/storage/emulated/0");
        assert_eq!(
            shared_path_from_tree(EXTERNAL_STORAGE_AUTHORITY, "primary:Documents/app", root),
            Ok(PathBuf::from("/storage/emulated/0/Documents/app"))
        );
        assert_eq!(
            shared_path_from_tree(EXTERNAL_STORAGE_AUTHORITY, "home:app", root),
            Ok(PathBuf::from("/storage/emulated/0/Documents/app"))
        );
        assert_eq!(
            shared_path_from_tree(EXTERNAL_STORAGE_AUTHORITY, "primary:", root),
            Ok(PathBuf::from("/storage/emulated/0"))
        );
    }

    #[test]
    fn trees_without_a_real_path_are_refused() {
        let root = Path::new("/storage/emulated/0");
        assert_eq!(
            shared_path_from_tree(
                "com.android.providers.downloads.documents",
                "raw:/storage/emulated/0/Download",
                root
            ),
            Err(SharedWorkspaceRefusal::NotLocalStorage)
        );
        assert_eq!(
            shared_path_from_tree("com.google.android.apps.docs.storage", "abc", root),
            Err(SharedWorkspaceRefusal::NotLocalStorage)
        );
        assert_eq!(
            shared_path_from_tree(EXTERNAL_STORAGE_AUTHORITY, "1A2B-3C4D:Music", root),
            Err(SharedWorkspaceRefusal::OtherVolume)
        );
        assert_eq!(
            shared_path_from_tree(EXTERNAL_STORAGE_AUTHORITY, "no-colon", root),
            Err(SharedWorkspaceRefusal::NotLocalStorage)
        );
        for traversal in [
            "primary:../../data",
            "primary:Documents/./x",
            "home:..",
            "primary:a\\b",
        ] {
            assert_eq!(
                shared_path_from_tree(EXTERNAL_STORAGE_AUTHORITY, traversal, root),
                Err(SharedWorkspaceRefusal::NotAFolder),
                "{traversal}"
            );
        }
    }

    #[test]
    fn a_folder_inside_shared_storage_is_accepted() {
        let layout = layout();
        let workspace = validate_shared_workspace(&layout.root.join("Documents/app"), &layout.root)
            .expect("accepted");
        assert_eq!(
            workspace.path(),
            fs::canonicalize(layout.root.join("Documents/app")).expect("canonical")
        );
    }

    #[test]
    fn containment_rules_refuse_everything_else() {
        let layout = layout();
        let root = &layout.root;
        let cases = [
            (root.clone(), SharedWorkspaceRefusal::StorageRoot),
            (root.join("Android"), SharedWorkspaceRefusal::AppDataFolder),
            (
                root.join("Android/data/other.app"),
                SharedWorkspaceRefusal::AppDataFolder,
            ),
            (
                root.join("Documents/notes.txt"),
                SharedWorkspaceRefusal::NotAFolder,
            ),
            (
                root.join("Documents/missing"),
                SharedWorkspaceRefusal::NotAFolder,
            ),
            (
                root.join("Documents/app/../../Download"),
                SharedWorkspaceRefusal::OutsideSharedStorage,
            ),
            (
                layout.paths.data_dir.clone(),
                SharedWorkspaceRefusal::NotAFolder,
            ),
            (
                PathBuf::from("relative/path"),
                SharedWorkspaceRefusal::OutsideSharedStorage,
            ),
        ];
        for (candidate, expected) in cases {
            assert_eq!(
                validate_shared_workspace(&candidate, root).map(|_| ()),
                Err(expected),
                "{}",
                candidate.display()
            );
        }
        // A real folder outside shared storage.
        let outside = layout._directory.path().join("outside");
        fs::create_dir(&outside).expect("outside");
        assert_eq!(
            validate_shared_workspace(&outside, root).map(|_| ()),
            Err(SharedWorkspaceRefusal::OutsideSharedStorage)
        );
    }

    #[cfg(unix)]
    #[test]
    fn a_link_out_of_shared_storage_is_refused() {
        let layout = layout();
        let outside = layout._directory.path().join("outside");
        fs::create_dir(&outside).expect("outside");
        let link = layout.root.join("Documents/link");
        std::os::unix::fs::symlink(&outside, &link).expect("link");
        assert_eq!(
            validate_shared_workspace(&link, &layout.root).map(|_| ()),
            Err(SharedWorkspaceRefusal::OutsideSharedStorage)
        );
    }

    #[test]
    fn the_choice_round_trips_and_clears() {
        let layout = layout();
        assert_eq!(layout.paths.load_shared_workspace_choice(), None);
        let workspace =
            validate_shared_workspace(&layout.root.join("Documents/app"), &layout.root).unwrap();
        layout
            .paths
            .save_shared_workspace_choice(&workspace)
            .expect("saved");
        assert_eq!(
            layout.paths.load_shared_workspace_choice().as_deref(),
            Some(workspace.path())
        );
        // The app-private workspace record is untouched.
        assert!(!layout.paths.workspace_config.exists());
        assert!(layout.paths.clear_shared_workspace_choice());
        assert_eq!(layout.paths.load_shared_workspace_choice(), None);
        assert!(layout.paths.clear_shared_workspace_choice());
    }

    #[test]
    fn a_malformed_or_oversized_record_reads_as_no_choice() {
        let layout = layout();
        fs::create_dir_all(&layout.paths.data_dir).unwrap();
        let file = layout.paths.data_dir.join(CHOICE_FILE);
        fs::write(&file, "not json").unwrap();
        assert_eq!(layout.paths.load_shared_workspace_choice(), None);
        fs::write(&file, vec![b' '; (MAX_CHOICE_BYTES + 1) as usize]).unwrap();
        assert_eq!(layout.paths.load_shared_workspace_choice(), None);
    }

    #[test]
    fn start_uses_the_shared_folder_only_with_access_and_a_valid_folder() {
        let layout = layout();
        assert_eq!(
            shared_workspace_at_start(&layout.paths, true, Some(&layout.root)),
            SharedWorkspaceStart::AppPrivate
        );
        let workspace =
            validate_shared_workspace(&layout.root.join("Documents/app"), &layout.root).unwrap();
        layout
            .paths
            .save_shared_workspace_choice(&workspace)
            .unwrap();

        assert_eq!(
            shared_workspace_at_start(&layout.paths, true, Some(&layout.root)),
            SharedWorkspaceStart::Shared(workspace.clone())
        );

        // Access revoked: fall back, keep the choice for later.
        let SharedWorkspaceStart::Fallback { notice, chosen } =
            shared_workspace_at_start(&layout.paths, false, Some(&layout.root))
        else {
            panic!("falls back without access");
        };
        assert!(notice.contains("All files access is off"));
        assert_eq!(chosen, workspace.path());
        assert!(layout.paths.load_shared_workspace_choice().is_some());

        // No shared storage at all.
        assert!(matches!(
            shared_workspace_at_start(&layout.paths, true, None),
            SharedWorkspaceStart::Fallback { .. }
        ));

        // The folder was deleted or became unreadable.
        fs::remove_dir_all(layout.root.join("Documents/app")).unwrap();
        let SharedWorkspaceStart::Fallback { notice, .. } =
            shared_workspace_at_start(&layout.paths, true, Some(&layout.root))
        else {
            panic!("falls back when the folder is gone");
        };
        assert!(notice.contains("cannot be used"));
    }
}
