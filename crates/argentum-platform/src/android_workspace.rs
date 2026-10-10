use std::fs::{self, File};
use std::io::Read;
use std::path::{Path, PathBuf};

#[cfg(windows)]
use std::fs::OpenOptions;
#[cfg(windows)]
use std::os::windows::fs::OpenOptionsExt;
#[cfg(windows)]
use std::os::windows::io::AsRawHandle;

use crate::{
    metadata_is_path_redirect, AppPaths, PlatformError, WorkspaceConfig, MAX_WORKSPACE_CONFIG_BYTES,
};

pub fn android_app_private_workspace(paths: &AppPaths) -> Result<PathBuf, PlatformError> {
    android_app_private_workspace_with_all_hooks(
        paths,
        PrivateRootProvenance::CallerSupplied,
        || Ok(()),
        || {},
        || {},
        |_| {},
    )
}

#[cfg(target_os = "android")]
pub fn android_app_private_workspace_for_app(
    app: &android_activity::AndroidApp,
) -> Result<ValidatedAndroidAppPaths, PlatformError> {
    let data_dir = app
        .internal_data_path()
        .ok_or(PlatformError::MissingDataDirectory)?;
    let paths = AppPaths::from_data_dir(data_dir);
    let workspace = android_app_private_workspace_with_all_hooks(
        &paths,
        PrivateRootProvenance::AndroidOs,
        || Ok(()),
        || {},
        || {},
        |_| {},
    )?;
    Ok(ValidatedAndroidAppPaths { workspace, paths })
}

/// Proof that a workspace and its Android app paths passed private-root validation together.
///
/// This token cannot be assembled from independent paths.
///
/// ```compile_fail
/// use std::path::PathBuf;
/// use argentum_platform::{AppPaths, ValidatedAndroidAppPaths};
///
/// let paths = AppPaths::from_data_dir("/data/user/0/com.argentum/files");
/// let token = ValidatedAndroidAppPaths {
///     workspace: PathBuf::from("/independent/workspace"),
///     paths,
/// };
/// ```
pub struct ValidatedAndroidAppPaths {
    workspace: PathBuf,
    paths: AppPaths,
}

impl ValidatedAndroidAppPaths {
    pub fn into_parts(self) -> (PathBuf, AppPaths) {
        (self.workspace, self.paths)
    }
}

#[derive(Clone, Copy)]
enum PrivateRootProvenance {
    CallerSupplied,
    #[cfg_attr(not(any(test, target_os = "android")), allow(dead_code))]
    AndroidOs,
}

#[cfg(test)]
fn android_app_private_workspace_with_hooks<F, G>(
    paths: &AppPaths,
    after_canonicalize: F,
    after_commit: G,
) -> Result<PathBuf, PlatformError>
where
    F: FnOnce(),
    G: FnOnce(),
{
    android_app_private_workspace_with_all_hooks(
        paths,
        PrivateRootProvenance::CallerSupplied,
        || Ok(()),
        after_canonicalize,
        after_commit,
        |_| {},
    )
}

fn android_app_private_workspace_with_all_hooks<R, F, G, C>(
    paths: &AppPaths,
    provenance: PrivateRootProvenance,
    after_root_scan: R,
    after_canonicalize: F,
    after_commit: G,
    after_marker_verified: C,
) -> Result<PathBuf, PlatformError>
where
    R: FnOnce() -> Result<(), PlatformError>,
    F: FnOnce(),
    G: FnOnce(),
    C: FnOnce(&Path),
{
    android_app_private_workspace_with_saved_load_hook(
        paths,
        provenance,
        after_root_scan,
        after_canonicalize,
        after_commit,
        after_marker_verified,
        |_| {},
    )
}

fn android_app_private_workspace_with_saved_load_hook<R, F, G, C, L>(
    paths: &AppPaths,
    provenance: PrivateRootProvenance,
    after_root_scan: R,
    after_canonicalize: F,
    after_commit: G,
    after_marker_verified: C,
    after_saved_load: L,
) -> Result<PathBuf, PlatformError>
where
    R: FnOnce() -> Result<(), PlatformError>,
    F: FnOnce(),
    G: FnOnce(),
    C: FnOnce(&Path),
    L: FnOnce(&Path),
{
    let (trusted, root_guard) =
        paths.validate_android_private_layout_with_hook(provenance, after_root_scan)?;
    let root = trusted.data_dir.clone();

    root_guard.verify()?;
    if let Some(saved) = trusted.load_workspace()? {
        after_saved_load(&saved);
        root_guard.verify()?;
        let saved = validate_private_descendant(&root, &saved)?;
        root_guard.verify()?;
        return Ok(saved);
    }

    root_guard.verify()?;
    let candidate = root.join("workspace");
    match fs::create_dir(&candidate) {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
        Err(_) => return Err(PlatformError::WorkspaceUnavailable),
    }
    let workspace = validate_private_descendant(&root, &candidate)?;
    let operation_marker = new_operation_marker()?;
    root_guard.verify()?;
    let persisted = trusted.save_workspace_with_operation_hooks(
        &workspace,
        Some(&operation_marker),
        after_canonicalize,
        after_commit,
    )?;
    match validate_persisted_identity(&trusted, &root, &workspace, &persisted) {
        Ok(validated) => Ok(validated),
        Err(error) => {
            if root_guard.verify().is_ok() {
                quarantine_owned_workspace_config_safely(
                    &trusted,
                    &operation_marker,
                    after_marker_verified,
                );
            }
            Err(error)
        }
    }
}

fn validate_private_descendant(root: &Path, candidate: &Path) -> Result<PathBuf, PlatformError> {
    let canonical = fs::canonicalize(candidate).map_err(|_| PlatformError::WorkspaceUnavailable)?;
    if canonical == root || !canonical.starts_with(root) || !canonical.is_dir() {
        return Err(PlatformError::WorkspaceUnavailable);
    }
    Ok(canonical)
}

fn validate_persisted_identity(
    paths: &AppPaths,
    root: &Path,
    expected: &Path,
    returned: &Path,
) -> Result<PathBuf, PlatformError> {
    let returned = validate_private_descendant(root, returned)?;
    if returned != expected {
        return Err(PlatformError::InvalidWorkspaceConfiguration);
    }
    let persisted = paths
        .load_workspace()?
        .ok_or(PlatformError::InvalidWorkspaceConfiguration)?;
    let persisted = validate_private_descendant(root, &persisted)?;
    if persisted != expected {
        return Err(PlatformError::InvalidWorkspaceConfiguration);
    }
    Ok(expected.to_path_buf())
}

fn new_operation_marker() -> Result<String, PlatformError> {
    let mut bytes = [0_u8; 16];
    getrandom::fill(&mut bytes).map_err(|_| PlatformError::InvalidWorkspaceConfiguration)?;
    let mut marker = String::with_capacity(bytes.len() * 2);
    const HEX: &[u8; 16] = b"0123456789abcdef";
    for byte in bytes {
        marker.push(HEX[(byte >> 4) as usize] as char);
        marker.push(HEX[(byte & 0x0f) as usize] as char);
    }
    Ok(marker)
}

fn quarantine_owned_workspace_config_safely<C>(
    paths: &AppPaths,
    operation_marker: &str,
    after_marker_verified: C,
) where
    C: FnOnce(&Path),
{
    if paths.workspace_config.parent() != Some(paths.data_dir.as_path()) {
        return;
    }
    let quarantine = paths
        .data_dir
        .join(format!(".workspace-cleanup-{operation_marker}"));
    if fs::symlink_metadata(&quarantine).is_ok() {
        return;
    }
    if fs::rename(&paths.workspace_config, &quarantine).is_err() {
        return;
    }
    if workspace_config_has_marker(&quarantine, operation_marker) {
        after_marker_verified(&quarantine);
        return;
    }
    if fs::symlink_metadata(&paths.workspace_config).is_err() {
        let _ = fs::rename(&quarantine, &paths.workspace_config);
    }
}

fn workspace_config_has_marker(path: &Path, operation_marker: &str) -> bool {
    let Ok(metadata) = fs::symlink_metadata(path) else {
        return false;
    };
    if metadata_is_path_redirect(&metadata)
        || !metadata.is_file()
        || metadata.len() > MAX_WORKSPACE_CONFIG_BYTES
    {
        return false;
    }
    let Ok(file) = File::open(path) else {
        return false;
    };
    let mut payload = String::new();
    if file
        .take(MAX_WORKSPACE_CONFIG_BYTES + 1)
        .read_to_string(&mut payload)
        .is_err()
        || payload.len() as u64 > MAX_WORKSPACE_CONFIG_BYTES
    {
        return false;
    }
    matches!(
        serde_json::from_str::<WorkspaceConfig>(&payload),
        Ok(config) if config.operation_marker.as_deref() == Some(operation_marker)
    )
}

impl AppPaths {
    pub fn validate_android_private_layout(&self) -> Result<Self, PlatformError> {
        self.validate_android_private_layout_with_hook(
            PrivateRootProvenance::CallerSupplied,
            || Ok(()),
        )
        .map(|(paths, _guard)| paths)
    }

    fn validate_android_private_layout_with_hook<F>(
        &self,
        provenance: PrivateRootProvenance,
        after_final_scan: F,
    ) -> Result<(Self, PrivateRootGuard), PlatformError>
    where
        F: FnOnce() -> Result<(), PlatformError>,
    {
        // Android supplies an existing app-private directory. This adapter does
        // not create that authority root because creation cannot be identity-bound.
        let declared = Self::from_data_dir(&self.data_dir);
        if self.workspace_config != declared.workspace_config
            || self.database != declared.database
            || self.logs_dir != declared.logs_dir
        {
            return Err(PlatformError::InvalidWorkspaceConfiguration);
        }

        let declared_guard = PrivateRootGuard::open(&self.data_dir)?;
        validate_ancestors_for_provenance(provenance, || {
            validate_existing_ancestors_no_redirect(&self.data_dir)
        })?;
        after_final_scan()?;
        let declared_guard = declared_guard.reopen_declared(&self.data_dir)?;
        let root =
            fs::canonicalize(&self.data_dir).map_err(|_| PlatformError::MissingDataDirectory)?;
        validate_existing_path(&root, ExistingPathKind::Directory)
            .map_err(|_| PlatformError::MissingDataDirectory)?;
        let root_guard = declared_guard.bind_canonical(&root)?;

        let trusted = Self::from_data_dir(root);
        validate_existing_path(&trusted.workspace_config, ExistingPathKind::File)?;
        validate_existing_path(&trusted.database, ExistingPathKind::File)?;
        validate_existing_path(&trusted.logs_dir, ExistingPathKind::Directory)?;
        root_guard.verify()?;
        Ok((trusted, root_guard))
    }
}

fn validate_ancestors_for_provenance<F>(
    provenance: PrivateRootProvenance,
    validate: F,
) -> Result<(), PlatformError>
where
    F: FnOnce() -> Result<(), PlatformError>,
{
    match provenance {
        PrivateRootProvenance::CallerSupplied => validate(),
        PrivateRootProvenance::AndroidOs => Ok(()),
    }
}

#[cfg(windows)]
struct PrivateRootGuard {
    declared: File,
    reopened: File,
    canonical: File,
    canonical_path: PathBuf,
    identity: WindowsFileIdentity,
}

#[cfg(windows)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct WindowsFileIdentity {
    volume_serial: u32,
    file_index: u64,
}

#[cfg(windows)]
impl PrivateRootGuard {
    fn open(path: &Path) -> Result<PendingPrivateRootGuard, PlatformError> {
        let declared = open_windows_directory_guard(path)?;
        let metadata = declared
            .metadata()
            .map_err(|_| PlatformError::MissingDataDirectory)?;
        if metadata_is_path_redirect(&metadata) || !metadata.is_dir() {
            return Err(PlatformError::MissingDataDirectory);
        }
        let identity = windows_file_identity(&declared)?;
        Ok(PendingPrivateRootGuard {
            declared,
            reopened: None,
            identity,
        })
    }

    fn verify(&self) -> Result<(), PlatformError> {
        let declared = self
            .declared
            .metadata()
            .map_err(|_| PlatformError::MissingDataDirectory)?;
        let canonical = self
            .canonical
            .metadata()
            .map_err(|_| PlatformError::MissingDataDirectory)?;
        if metadata_is_path_redirect(&declared)
            || metadata_is_path_redirect(&canonical)
            || !declared.is_dir()
            || !canonical.is_dir()
            || windows_file_identity(&self.declared)? != self.identity
            || windows_file_identity(&self.reopened)? != self.identity
            || windows_file_identity(&self.canonical)? != self.identity
        {
            return Err(PlatformError::MissingDataDirectory);
        }
        let current = open_windows_directory_guard(&self.canonical_path)?;
        if windows_file_identity(&current)? != self.identity {
            return Err(PlatformError::MissingDataDirectory);
        }
        Ok(())
    }
}

#[cfg(windows)]
struct PendingPrivateRootGuard {
    declared: File,
    reopened: Option<File>,
    identity: WindowsFileIdentity,
}

#[cfg(windows)]
impl PendingPrivateRootGuard {
    fn reopen_declared(mut self, path: &Path) -> Result<Self, PlatformError> {
        let reopened = open_windows_directory_guard(path)?;
        let metadata = reopened
            .metadata()
            .map_err(|_| PlatformError::MissingDataDirectory)?;
        if metadata_is_path_redirect(&metadata)
            || !metadata.is_dir()
            || windows_file_identity(&reopened)? != self.identity
        {
            return Err(PlatformError::MissingDataDirectory);
        }
        self.reopened = Some(reopened);
        Ok(self)
    }

    fn bind_canonical(self, canonical_path: &Path) -> Result<PrivateRootGuard, PlatformError> {
        let canonical = open_windows_directory_guard(canonical_path)?;
        let metadata = canonical
            .metadata()
            .map_err(|_| PlatformError::MissingDataDirectory)?;
        if metadata_is_path_redirect(&metadata)
            || !metadata.is_dir()
            || windows_file_identity(&canonical)? != self.identity
        {
            return Err(PlatformError::MissingDataDirectory);
        }
        Ok(PrivateRootGuard {
            declared: self.declared,
            reopened: self.reopened.ok_or(PlatformError::MissingDataDirectory)?,
            canonical,
            canonical_path: canonical_path.to_path_buf(),
            identity: self.identity,
        })
    }
}

#[cfg(windows)]
fn open_windows_directory_guard(path: &Path) -> Result<File, PlatformError> {
    use windows_sys::Win32::Storage::FileSystem::{
        FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OPEN_REPARSE_POINT, FILE_SHARE_READ, FILE_SHARE_WRITE,
    };

    let mut options = OpenOptions::new();
    options
        .access_mode(0)
        .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE)
        .custom_flags(FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT)
        .open(path)
        .map_err(|_| PlatformError::MissingDataDirectory)
}

#[cfg(windows)]
fn windows_file_identity(file: &File) -> Result<WindowsFileIdentity, PlatformError> {
    use windows_sys::Win32::Storage::FileSystem::{
        GetFileInformationByHandle, BY_HANDLE_FILE_INFORMATION,
    };

    let mut information = BY_HANDLE_FILE_INFORMATION::default();
    // SAFETY: `file` owns a live Windows handle for the duration of this call,
    // and `information` is a valid writable output buffer of the required type.
    let result = unsafe { GetFileInformationByHandle(file.as_raw_handle(), &mut information) };
    if result == 0 {
        return Err(PlatformError::MissingDataDirectory);
    }
    Ok(WindowsFileIdentity {
        volume_serial: information.dwVolumeSerialNumber,
        file_index: ((information.nFileIndexHigh as u64) << 32) | information.nFileIndexLow as u64,
    })
}

#[cfg(not(windows))]
struct PrivateRootGuard {
    directory: File,
    reopened: File,
    path: PathBuf,
    identity: UnixFileIdentity,
}

#[cfg(not(windows))]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct UnixFileIdentity {
    device: u64,
    inode: u64,
}

#[cfg(not(windows))]
impl PrivateRootGuard {
    fn open(path: &Path) -> Result<PendingPrivateRootGuard, PlatformError> {
        let metadata =
            fs::symlink_metadata(path).map_err(|_| PlatformError::MissingDataDirectory)?;
        if metadata_is_path_redirect(&metadata) || !metadata.is_dir() {
            return Err(PlatformError::MissingDataDirectory);
        }
        let directory = File::open(path).map_err(|_| PlatformError::MissingDataDirectory)?;
        let identity = unix_file_identity(&metadata);
        if unix_file_identity(
            &directory
                .metadata()
                .map_err(|_| PlatformError::MissingDataDirectory)?,
        ) != identity
        {
            return Err(PlatformError::MissingDataDirectory);
        }
        Ok(PendingPrivateRootGuard {
            directory,
            reopened: None,
            identity,
        })
    }

    fn verify(&self) -> Result<(), PlatformError> {
        let metadata =
            fs::symlink_metadata(&self.path).map_err(|_| PlatformError::MissingDataDirectory)?;
        if metadata_is_path_redirect(&metadata)
            || !metadata.is_dir()
            || unix_file_identity(
                &self
                    .directory
                    .metadata()
                    .map_err(|_| PlatformError::MissingDataDirectory)?,
            ) != self.identity
            || unix_file_identity(
                &self
                    .reopened
                    .metadata()
                    .map_err(|_| PlatformError::MissingDataDirectory)?,
            ) != self.identity
            || unix_file_identity(&metadata) != self.identity
        {
            return Err(PlatformError::MissingDataDirectory);
        }
        Ok(())
    }
}

#[cfg(not(windows))]
struct PendingPrivateRootGuard {
    directory: File,
    reopened: Option<File>,
    identity: UnixFileIdentity,
}

#[cfg(not(windows))]
impl PendingPrivateRootGuard {
    fn reopen_declared(mut self, path: &Path) -> Result<Self, PlatformError> {
        let metadata =
            fs::symlink_metadata(path).map_err(|_| PlatformError::MissingDataDirectory)?;
        if metadata_is_path_redirect(&metadata)
            || !metadata.is_dir()
            || unix_file_identity(&metadata) != self.identity
        {
            return Err(PlatformError::MissingDataDirectory);
        }
        let reopened = File::open(path).map_err(|_| PlatformError::MissingDataDirectory)?;
        if unix_file_identity(
            &reopened
                .metadata()
                .map_err(|_| PlatformError::MissingDataDirectory)?,
        ) != self.identity
        {
            return Err(PlatformError::MissingDataDirectory);
        }
        self.reopened = Some(reopened);
        Ok(self)
    }

    fn bind_canonical(self, canonical_path: &Path) -> Result<PrivateRootGuard, PlatformError> {
        let metadata = fs::symlink_metadata(canonical_path)
            .map_err(|_| PlatformError::MissingDataDirectory)?;
        if metadata_is_path_redirect(&metadata)
            || !metadata.is_dir()
            || unix_file_identity(&metadata) != self.identity
        {
            return Err(PlatformError::MissingDataDirectory);
        }
        Ok(PrivateRootGuard {
            directory: self.directory,
            reopened: self.reopened.ok_or(PlatformError::MissingDataDirectory)?,
            path: canonical_path.to_path_buf(),
            identity: self.identity,
        })
    }
}

#[cfg(not(windows))]
fn unix_file_identity(metadata: &fs::Metadata) -> UnixFileIdentity {
    use std::os::unix::fs::MetadataExt;

    UnixFileIdentity {
        device: metadata.dev(),
        inode: metadata.ino(),
    }
}

fn validate_existing_ancestors_no_redirect(path: &Path) -> Result<(), PlatformError> {
    if !path.is_absolute() {
        return Err(PlatformError::MissingDataDirectory);
    }
    let mut ancestors = path.ancestors().collect::<Vec<_>>();
    ancestors.reverse();
    for ancestor in ancestors {
        let metadata = match fs::symlink_metadata(ancestor) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(_) => return Err(PlatformError::MissingDataDirectory),
        };
        if metadata_is_path_redirect(&metadata) || !metadata.is_dir() {
            return Err(PlatformError::MissingDataDirectory);
        }
    }
    Ok(())
}

#[derive(Clone, Copy)]
enum ExistingPathKind {
    File,
    Directory,
}

fn validate_existing_path(path: &Path, expected: ExistingPathKind) -> Result<(), PlatformError> {
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(_) => return Err(PlatformError::InvalidWorkspaceConfiguration),
    };
    if metadata_is_path_redirect(&metadata) {
        return Err(PlatformError::InvalidWorkspaceConfiguration);
    }
    let matches_kind = match expected {
        ExistingPathKind::File => metadata.is_file(),
        ExistingPathKind::Directory => metadata.is_dir(),
    };
    matches_kind
        .then_some(())
        .ok_or(PlatformError::InvalidWorkspaceConfiguration)
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::PathBuf;
    use std::sync::mpsc;
    use std::thread;

    use super::{
        android_app_private_workspace, android_app_private_workspace_with_all_hooks,
        android_app_private_workspace_with_hooks,
        android_app_private_workspace_with_saved_load_hook, validate_ancestors_for_provenance,
        AppPaths, PlatformError, PrivateRootProvenance, ValidatedAndroidAppPaths,
    };

    #[test]
    fn default_android_workspace_is_created_below_the_private_root() {
        let data = tempfile::tempdir().expect("data root");
        let paths = AppPaths::from_data_dir(data.path());

        let workspace = android_app_private_workspace(&paths).expect("workspace");
        let root = fs::canonicalize(data.path()).expect("canonical data root");

        assert_eq!(
            workspace,
            fs::canonicalize(data.path().join("workspace")).expect("canonical workspace")
        );
        assert!(workspace.starts_with(&root));
        assert_ne!(workspace, root);
        assert_eq!(
            paths.load_workspace().expect("saved workspace"),
            Some(workspace)
        );
    }

    #[test]
    fn android_os_root_does_not_depend_on_system_ancestor_metadata() {
        let result = validate_ancestors_for_provenance(PrivateRootProvenance::AndroidOs, || {
            Err(PlatformError::MissingDataDirectory)
        });

        assert!(result.is_ok());
        assert!(matches!(
            validate_ancestors_for_provenance(PrivateRootProvenance::CallerSupplied, || Err(
                PlatformError::MissingDataDirectory
            ),),
            Err(PlatformError::MissingDataDirectory)
        ));
    }

    #[test]
    fn validated_android_token_owns_workspace_and_app_paths_together() {
        let data = tempfile::tempdir().expect("data root");
        let workspace = data.path().join("workspace");
        fs::create_dir(&workspace).expect("workspace");
        let workspace = fs::canonicalize(workspace).expect("canonical workspace");
        let paths = AppPaths::from_data_dir(data.path());
        let expected_data_dir = paths.data_dir.clone();
        let expected_database = paths.database.clone();
        let token = ValidatedAndroidAppPaths {
            workspace: workspace.clone(),
            paths,
        };

        let (owned_workspace, owned_paths) = token.into_parts();

        assert_eq!(owned_workspace, workspace);
        assert_eq!(owned_paths.data_dir, expected_data_dir);
        assert_eq!(owned_paths.database, expected_database);
    }

    #[test]
    fn android_os_root_still_rejects_a_redirected_root() {
        let declared_parent = tempfile::tempdir().expect("declared parent");
        let outside = tempfile::tempdir().expect("outside root");
        let marker = outside.path().join("unchanged.txt");
        fs::write(&marker, b"unchanged").expect("outside marker");
        let redirected_root = declared_parent.path().join("private-data");
        if !create_directory_link(outside.path(), &redirected_root) {
            return;
        }
        let paths = AppPaths::from_data_dir(&redirected_root);

        let result = android_app_private_workspace_with_all_hooks(
            &paths,
            PrivateRootProvenance::AndroidOs,
            || Ok(()),
            || {},
            || {},
            |_| {},
        );

        assert!(result.is_err());
        assert_private_outputs_absent(outside.path());
        assert_eq!(
            fs::read(&marker).expect("outside marker remains"),
            b"unchanged"
        );
    }

    #[test]
    fn saved_outside_workspace_is_rejected_without_modifying_the_target() {
        let data = tempfile::tempdir().expect("data root");
        let outside = tempfile::tempdir().expect("outside root");
        let marker = outside.path().join("unchanged.txt");
        fs::write(&marker, b"unchanged").expect("outside marker");
        let paths = AppPaths::from_data_dir(data.path());
        paths
            .save_workspace(outside.path())
            .expect("save outside fixture");

        assert!(android_app_private_workspace(&paths).is_err());
        assert_eq!(
            fs::read(&marker).expect("outside marker remains"),
            b"unchanged"
        );
        assert!(!outside.path().join("workspace").exists());
    }

    #[test]
    fn saved_data_root_is_rejected_without_creating_a_workspace() {
        let data = tempfile::tempdir().expect("data root");
        let paths = AppPaths::from_data_dir(data.path());
        paths
            .save_workspace(data.path())
            .expect("save root fixture");

        assert!(android_app_private_workspace(&paths).is_err());
        assert!(!data.path().join("workspace").exists());
    }

    #[test]
    fn nonexistent_saved_workspace_is_rejected_without_creating_it() {
        let data = tempfile::tempdir().expect("data root");
        let missing = data.path().join("missing");
        let paths = AppPaths::from_data_dir(data.path());
        fs::create_dir_all(&paths.data_dir).expect("data root");
        write_workspace_fixture(&paths, &missing);

        assert!(android_app_private_workspace(&paths).is_err());
        assert!(!missing.exists());
    }

    #[test]
    fn saved_directory_link_escape_is_rejected_where_supported() {
        let data = tempfile::tempdir().expect("data root");
        let outside = tempfile::tempdir().expect("outside root");
        let marker = outside.path().join("unchanged.txt");
        fs::write(&marker, b"unchanged").expect("outside marker");
        let link = data.path().join("linked-workspace");
        if !create_directory_link(outside.path(), &link) {
            return;
        }
        let paths = AppPaths::from_data_dir(data.path());
        write_workspace_fixture(&paths, &link);

        assert!(android_app_private_workspace(&paths).is_err());
        assert_eq!(
            fs::read(&marker).expect("outside marker remains"),
            b"unchanged"
        );
        assert!(!outside.path().join("workspace").exists());
    }

    #[test]
    fn forged_android_layout_fields_are_rejected_before_outside_mutation() {
        for field in ["workspace_config", "database", "logs_dir"] {
            let data = tempfile::tempdir().expect("data root");
            let outside = tempfile::tempdir().expect("outside root");
            let marker = outside.path().join("unchanged.txt");
            fs::write(&marker, b"unchanged").expect("outside marker");
            let redirected = outside.path().join(format!("redirected-{field}"));
            let mut paths = AppPaths::from_data_dir(data.path());
            match field {
                "workspace_config" => paths.workspace_config = redirected.clone(),
                "database" => paths.database = redirected.clone(),
                "logs_dir" => paths.logs_dir = redirected.clone(),
                _ => unreachable!(),
            }

            assert!(android_app_private_workspace(&paths).is_err(), "{field}");
            assert!(!redirected.exists(), "{field}");
            assert!(!data.path().join("workspace").exists(), "{field}");
            assert_eq!(
                fs::read(&marker).expect("outside marker remains"),
                b"unchanged",
                "{field}"
            );
        }
    }

    #[test]
    fn manually_constructed_inconsistent_android_layout_is_rejected() {
        let data = tempfile::tempdir().expect("data root");
        let outside = tempfile::tempdir().expect("outside root");
        let paths = AppPaths {
            data_dir: data.path().to_path_buf(),
            database: outside.path().join("argentum.db"),
            logs_dir: data.path().join("not-logs"),
            workspace_config: data.path().join("not-workspace.json"),
        };

        assert!(paths.validate_android_private_layout().is_err());
        assert!(!outside.path().join("argentum.db").exists());
        assert!(!data.path().join("not-logs").exists());
        assert!(!data.path().join("not-workspace.json").exists());
    }

    #[test]
    fn redirected_workspace_config_is_rejected_without_touching_its_target() {
        let data = tempfile::tempdir().expect("data root");
        let outside = tempfile::tempdir().expect("outside root");
        let target = outside.path().join("outside-config.json");
        fs::write(&target, b"unchanged").expect("outside config marker");
        let paths = AppPaths::from_data_dir(data.path());
        if !create_file_link(&target, &paths.workspace_config) {
            return;
        }

        assert!(android_app_private_workspace(&paths).is_err());
        assert_eq!(
            fs::read(&target).expect("outside config marker remains"),
            b"unchanged"
        );
        assert!(!data.path().join("workspace").exists());
    }

    #[test]
    fn redirected_database_and_logs_are_rejected_without_touching_targets() {
        let data = tempfile::tempdir().expect("data root");
        let outside = tempfile::tempdir().expect("outside root");
        let database_target = outside.path().join("outside.db");
        let logs_target = outside.path().join("outside-logs");
        let marker = logs_target.join("unchanged.txt");
        fs::write(&database_target, b"unchanged").expect("outside database marker");
        fs::create_dir(&logs_target).expect("outside logs");
        fs::write(&marker, b"unchanged").expect("outside logs marker");
        let paths = AppPaths::from_data_dir(data.path());
        if !create_file_link(&database_target, &paths.database)
            || !create_directory_link(&logs_target, &paths.logs_dir)
        {
            return;
        }

        assert!(paths.validate_android_private_layout().is_err());
        assert_eq!(
            fs::read(&database_target).expect("outside database remains"),
            b"unchanged"
        );
        assert_eq!(
            fs::read(&marker).expect("outside logs marker remains"),
            b"unchanged"
        );
    }

    #[test]
    fn redirected_data_dir_is_rejected_before_private_path_mutation() {
        let declared_parent = tempfile::tempdir().expect("declared parent");
        let outside = tempfile::tempdir().expect("outside root");
        let marker = outside.path().join("unchanged.txt");
        fs::write(&marker, b"unchanged").expect("outside marker");
        let redirected_root = declared_parent.path().join("private-data");
        assert!(
            create_directory_link(outside.path(), &redirected_root),
            "directory links must be available for this platform test"
        );
        let paths = AppPaths::from_data_dir(&redirected_root);

        assert!(android_app_private_workspace(&paths).is_err());
        assert_private_outputs_absent(outside.path());
        assert_eq!(
            fs::read(&marker).expect("outside marker remains"),
            b"unchanged"
        );
    }

    #[test]
    fn redirected_existing_ancestor_is_rejected_before_private_root_creation() {
        let declared_parent = tempfile::tempdir().expect("declared parent");
        let outside = tempfile::tempdir().expect("outside root");
        let marker = outside.path().join("unchanged.txt");
        fs::write(&marker, b"unchanged").expect("outside marker");
        let redirected_ancestor = declared_parent.path().join("redirected-ancestor");
        assert!(
            create_directory_link(outside.path(), &redirected_ancestor),
            "directory links must be available for this platform test"
        );
        let paths = AppPaths::from_data_dir(redirected_ancestor.join("private-data"));

        assert!(android_app_private_workspace(&paths).is_err());
        assert!(!outside.path().join("private-data").exists());
        assert_private_outputs_absent(outside.path());
        assert_eq!(
            fs::read(&marker).expect("outside marker remains"),
            b"unchanged"
        );
    }

    #[test]
    fn synchronized_root_substitution_after_final_scan_fails_before_mutation() {
        let declared_parent = tempfile::tempdir().expect("declared parent");
        let data_root = declared_parent.path().join("private-data");
        fs::create_dir(&data_root).expect("declared data root");
        let outside = tempfile::tempdir().expect("outside root");
        let marker = outside.path().join("unchanged.txt");
        fs::write(&marker, b"unchanged").expect("outside marker");
        assert_directory_links_available(declared_parent.path(), outside.path());
        let paths = AppPaths::from_data_dir(&data_root);

        let outside_path = outside.path().to_path_buf();
        let (scanned_tx, scanned_rx) = mpsc::sync_channel(0);
        let (substituted_tx, substituted_rx) = mpsc::sync_channel(0);
        let replacer = thread::spawn(move || {
            scanned_rx.recv().expect("root scanned signal");
            let substituted = fs::remove_dir(&data_root).is_ok()
                && create_directory_link(&outside_path, &data_root);
            substituted_tx
                .send(())
                .expect("root substitution completed");
            substituted
        });

        let result = android_app_private_workspace_with_all_hooks(
            &paths,
            PrivateRootProvenance::CallerSupplied,
            || {
                scanned_tx.send(()).expect("root scanned signal");
                substituted_rx.recv().expect("root substitution completed");
                Ok(())
            },
            || {},
            || {},
            |_| {},
        );
        let substituted = replacer.join().expect("root replacer");

        if substituted {
            assert!(result.is_err());
        } else {
            let workspace = result.expect("pinned original root remains usable");
            assert!(workspace
                .starts_with(fs::canonicalize(&paths.data_dir).expect("canonical original root")));
        }
        assert_private_outputs_absent(outside.path());
        assert_eq!(
            fs::read(&marker).expect("outside marker remains"),
            b"unchanged"
        );
    }

    #[test]
    fn synchronized_ordinary_root_substitution_after_final_scan_fails_before_mutation() {
        let declared_parent = tempfile::tempdir().expect("declared parent");
        let data_root = declared_parent.path().join("private-data");
        let original_root = declared_parent.path().join("private-data-original");
        fs::create_dir(&data_root).expect("declared data root");
        let replacement = tempfile::tempdir().expect("replacement root source");
        let marker = replacement.path().join("unchanged.txt");
        fs::write(&marker, b"unchanged").expect("replacement marker");
        let paths = AppPaths::from_data_dir(&data_root);

        let replacement_path = replacement.path().to_path_buf();
        let data_root_for_replacer = data_root.clone();
        let original_root_for_replacer = original_root.clone();
        let (scanned_tx, scanned_rx) = mpsc::sync_channel(0);
        let (substituted_tx, substituted_rx) = mpsc::sync_channel(0);
        let replacer = thread::spawn(move || {
            scanned_rx.recv().expect("root scanned signal");
            let substituted = fs::rename(&data_root_for_replacer, &original_root_for_replacer)
                .is_ok()
                && fs::create_dir(&data_root_for_replacer).is_ok();
            substituted_tx
                .send(())
                .expect("ordinary root substitution completed");
            substituted
        });

        let result = android_app_private_workspace_with_all_hooks(
            &paths,
            PrivateRootProvenance::CallerSupplied,
            || {
                scanned_tx.send(()).expect("root scanned signal");
                substituted_rx
                    .recv()
                    .expect("ordinary root substitution completed");
                Ok(())
            },
            || {},
            || {},
            |_| {},
        );
        let substituted = replacer.join().expect("ordinary root replacer");

        if substituted {
            assert!(result.is_err());
            assert_private_outputs_absent(&data_root);
            assert_private_outputs_absent(&original_root);
        } else {
            let workspace = result.expect("pinned original root remains usable");
            assert!(workspace
                .starts_with(fs::canonicalize(&paths.data_dir).expect("canonical original root")));
        }
        assert_private_outputs_absent(&replacement_path);
        assert_eq!(
            fs::read(&marker).expect("replacement marker remains"),
            b"unchanged"
        );
    }

    #[test]
    fn saved_workspace_root_substitution_during_load_validation_fails_closed() {
        let declared_parent = tempfile::tempdir().expect("declared parent");
        let data_root = declared_parent.path().join("private-data");
        let original_root = declared_parent.path().join("private-data-original");
        let workspace = data_root.join("workspace");
        fs::create_dir_all(&workspace).expect("saved workspace");
        let paths = AppPaths::from_data_dir(&data_root);
        paths
            .save_workspace(&workspace)
            .expect("saved workspace config");

        let data_root_for_hook = data_root.clone();
        let original_root_for_hook = original_root.clone();
        let mut substituted = false;
        let result = android_app_private_workspace_with_saved_load_hook(
            &paths,
            PrivateRootProvenance::CallerSupplied,
            || Ok(()),
            || {},
            || {},
            |_| {},
            |_| {
                substituted = fs::rename(&data_root_for_hook, &original_root_for_hook).is_ok()
                    && fs::create_dir(&data_root_for_hook).is_ok()
                    && fs::create_dir(data_root_for_hook.join("workspace")).is_ok();
            },
        );

        if substituted {
            assert!(result.is_err());
            assert!(data_root.join("workspace").is_dir());
            assert!(!data_root.join("workspace.json").exists());
            assert!(!data_root.join("argentum.db").exists());
            assert!(!data_root.join("logs").exists());
            assert!(original_root.join("workspace.json").is_file());
            assert!(original_root.join("workspace").is_dir());
        } else {
            assert_eq!(
                result.expect("original pinned root remains usable"),
                fs::canonicalize(workspace).expect("canonical saved workspace")
            );
        }
    }

    #[test]
    fn synchronized_ordinary_ancestor_substitution_after_final_scan_fails_before_mutation() {
        let declared_parent = tempfile::tempdir().expect("declared parent");
        let ancestor = declared_parent.path().join("app-private");
        let original_ancestor = declared_parent.path().join("app-private-original");
        let data_root = ancestor.join("private-data");
        fs::create_dir(&ancestor).expect("declared ancestor");
        fs::create_dir(&data_root).expect("declared data root");
        let outside = tempfile::tempdir().expect("outside root");
        let marker = outside.path().join("unchanged.txt");
        fs::write(&marker, b"unchanged").expect("outside marker");
        let paths = AppPaths::from_data_dir(&data_root);

        let ancestor_for_replacer = ancestor.clone();
        let original_ancestor_for_replacer = original_ancestor.clone();
        let data_root_for_replacer = data_root.clone();
        let (scanned_tx, scanned_rx) = mpsc::sync_channel(0);
        let (substituted_tx, substituted_rx) = mpsc::sync_channel(0);
        let replacer = thread::spawn(move || {
            scanned_rx.recv().expect("ancestor scanned signal");
            let substituted = fs::rename(&ancestor_for_replacer, &original_ancestor_for_replacer)
                .is_ok()
                && fs::create_dir(&ancestor_for_replacer).is_ok()
                && fs::create_dir(&data_root_for_replacer).is_ok();
            substituted_tx
                .send(())
                .expect("ordinary ancestor substitution completed");
            substituted
        });

        let result = android_app_private_workspace_with_all_hooks(
            &paths,
            PrivateRootProvenance::CallerSupplied,
            || {
                scanned_tx.send(()).expect("ancestor scanned signal");
                substituted_rx
                    .recv()
                    .expect("ordinary ancestor substitution completed");
                Ok(())
            },
            || {},
            || {},
            |_| {},
        );
        let substituted = replacer.join().expect("ordinary ancestor replacer");

        let original_data_root = original_ancestor.join("private-data");
        if substituted {
            assert!(result.is_err());
            assert_private_outputs_absent(&data_root);
            assert_private_outputs_absent(&original_data_root);
        } else {
            let workspace = result.expect("pinned original root remains usable");
            assert!(workspace
                .starts_with(fs::canonicalize(&paths.data_dir).expect("canonical original root")));
        }
        assert_private_outputs_absent(outside.path());
        assert_eq!(
            fs::read(&marker).expect("outside marker remains"),
            b"unchanged"
        );
    }

    #[test]
    fn synchronized_precommit_workspace_substitution_quarantines_owned_config() {
        let data = tempfile::tempdir().expect("data root");
        let outside = tempfile::tempdir().expect("outside root");
        let marker = outside.path().join("unchanged.txt");
        fs::write(&marker, b"unchanged").expect("outside marker");
        let paths = AppPaths::from_data_dir(data.path());

        let probe = data.path().join("link-support-probe");
        if !create_directory_link(outside.path(), &probe) {
            return;
        }
        fs::remove_dir(&probe).expect("remove link support probe");

        let candidate = data.path().join("workspace");
        let outside_path = outside.path().to_path_buf();
        let (validated_tx, validated_rx) = mpsc::sync_channel(0);
        let (substituted_tx, substituted_rx) = mpsc::sync_channel(0);
        let replacer = thread::spawn(move || {
            validated_rx.recv().expect("validated signal");
            fs::remove_dir(&candidate).expect("remove validated workspace");
            assert!(create_directory_link(&outside_path, &candidate));
            substituted_tx.send(()).expect("substituted signal");
        });

        let result = android_app_private_workspace_with_hooks(
            &paths,
            || {
                validated_tx.send(()).expect("validated signal");
                substituted_rx.recv().expect("substituted signal");
            },
            || {},
        );
        replacer.join().expect("workspace replacer");

        assert!(result.is_err());
        assert!(!paths.workspace_config.exists());
        assert_one_owned_quarantine(data.path());
        assert_eq!(
            fs::read(&marker).expect("outside marker remains"),
            b"unchanged"
        );
    }

    #[test]
    fn synchronized_postcommit_workspace_substitution_quarantines_owned_config() {
        let data = tempfile::tempdir().expect("data root");
        let outside = tempfile::tempdir().expect("outside root");
        let marker = outside.path().join("unchanged.txt");
        fs::write(&marker, b"unchanged").expect("outside marker");
        let paths = AppPaths::from_data_dir(data.path());
        assert_directory_links_available(data.path(), outside.path());

        let candidate = data.path().join("workspace");
        let outside_path = outside.path().to_path_buf();
        let (committed_tx, committed_rx) = mpsc::sync_channel(0);
        let (substituted_tx, substituted_rx) = mpsc::sync_channel(0);
        let replacer = thread::spawn(move || {
            committed_rx.recv().expect("committed signal");
            fs::remove_dir(&candidate).expect("remove committed workspace");
            assert!(create_directory_link(&outside_path, &candidate));
            substituted_tx.send(()).expect("substituted signal");
        });

        let result = android_app_private_workspace_with_hooks(
            &paths,
            || {},
            || {
                committed_tx.send(()).expect("committed signal");
                substituted_rx.recv().expect("substituted signal");
            },
        );
        replacer.join().expect("workspace replacer");

        assert!(result.is_err());
        assert!(!paths.workspace_config.exists());
        assert_one_owned_quarantine(data.path());
        assert_eq!(
            fs::read(&marker).expect("outside marker remains"),
            b"unchanged"
        );
    }

    #[test]
    fn synchronized_swapped_config_is_not_deleted_during_cleanup() {
        let data = tempfile::tempdir().expect("data root");
        let outside = tempfile::tempdir().expect("outside root");
        let marker = outside.path().join("unchanged.txt");
        fs::write(&marker, b"unchanged").expect("outside marker");
        let paths = AppPaths::from_data_dir(data.path());
        assert_directory_links_available(data.path(), outside.path());

        let candidate = data.path().join("workspace");
        let config = paths.workspace_config.clone();
        let outside_path = outside.path().to_path_buf();
        let (committed_tx, committed_rx) = mpsc::sync_channel(0);
        let (substituted_tx, substituted_rx) = mpsc::sync_channel(0);
        let replacer = thread::spawn(move || {
            committed_rx.recv().expect("committed signal");
            fs::remove_dir(&candidate).expect("remove committed workspace");
            assert!(create_directory_link(&outside_path, &candidate));
            fs::remove_file(&config).expect("remove owned config fixture");
            fs::write(&config, b"attacker-owned").expect("write attacker config fixture");
            substituted_tx.send(()).expect("substituted signal");
        });

        let result = android_app_private_workspace_with_hooks(
            &paths,
            || {},
            || {
                committed_tx.send(()).expect("committed signal");
                substituted_rx.recv().expect("substituted signal");
            },
        );
        replacer.join().expect("workspace and config replacer");

        assert!(result.is_err());
        assert_eq!(
            fs::read(&paths.workspace_config).expect("attacker config remains"),
            b"attacker-owned"
        );
        assert_no_cleanup_config(data.path());
        assert_eq!(
            fs::read(&marker).expect("outside marker remains"),
            b"unchanged"
        );
    }

    #[test]
    fn synchronized_swapped_config_link_does_not_touch_outside_target() {
        let data = tempfile::tempdir().expect("data root");
        let outside = tempfile::tempdir().expect("outside root");
        let outside_config = outside.path().join("outside-config.json");
        fs::write(&outside_config, b"outside-unchanged").expect("outside config marker");
        let paths = AppPaths::from_data_dir(data.path());
        assert_directory_links_available(data.path(), outside.path());

        let candidate = data.path().join("workspace");
        let config = paths.workspace_config.clone();
        let outside_path = outside.path().to_path_buf();
        let (committed_tx, committed_rx) = mpsc::sync_channel(0);
        let (substituted_tx, substituted_rx) = mpsc::sync_channel(0);
        let replacer = thread::spawn(move || {
            committed_rx.recv().expect("committed signal");
            fs::remove_dir(&candidate).expect("remove committed workspace");
            assert!(create_directory_link(&outside_path, &candidate));
            fs::remove_file(&config).expect("remove owned config fixture");
            assert!(create_file_link(&outside_config, &config));
            substituted_tx.send(()).expect("substituted signal");
        });

        let result = android_app_private_workspace_with_hooks(
            &paths,
            || {},
            || {
                committed_tx.send(()).expect("committed signal");
                substituted_rx.recv().expect("substituted signal");
            },
        );
        replacer.join().expect("workspace and config link replacer");

        assert!(result.is_err());
        assert!(fs::symlink_metadata(&paths.workspace_config)
            .expect("swapped config link remains")
            .file_type()
            .is_symlink());
        assert_eq!(
            fs::read(outside.path().join("outside-config.json")).expect("outside config remains"),
            b"outside-unchanged"
        );
        assert_no_cleanup_config(data.path());
    }

    #[test]
    fn synchronized_regular_quarantine_swap_after_verification_is_preserved() {
        let data = tempfile::tempdir().expect("data root");
        let outside = tempfile::tempdir().expect("outside root");
        let marker = outside.path().join("unchanged.txt");
        fs::write(&marker, b"unchanged").expect("outside marker");
        let paths = AppPaths::from_data_dir(data.path());
        assert_directory_links_available(data.path(), outside.path());

        let candidate = data.path().join("workspace");
        let outside_path = outside.path().to_path_buf();
        let (verified_tx, verified_rx) = mpsc::sync_channel::<PathBuf>(0);
        let (substituted_tx, substituted_rx) = mpsc::sync_channel(0);
        let replacer = thread::spawn(move || {
            let quarantine = verified_rx.recv().expect("verified quarantine path");
            fs::remove_file(&quarantine).expect("remove owned quarantine fixture");
            fs::write(&quarantine, b"attacker-quarantine")
                .expect("write attacker quarantine fixture");
            substituted_tx.send(()).expect("quarantine substituted");
            quarantine
        });

        let result = android_app_private_workspace_with_all_hooks(
            &paths,
            PrivateRootProvenance::CallerSupplied,
            || Ok(()),
            || {},
            || {
                fs::remove_dir(&candidate).expect("remove committed workspace");
                assert!(create_directory_link(&outside_path, &candidate));
            },
            |quarantine| {
                verified_tx
                    .send(quarantine.to_path_buf())
                    .expect("verified quarantine path");
                substituted_rx.recv().expect("quarantine substituted");
            },
        );
        let quarantine = replacer.join().expect("quarantine replacer");

        assert!(result.is_err());
        assert!(!paths.workspace_config.exists());
        assert_eq!(
            fs::read(&quarantine).expect("attacker quarantine remains"),
            b"attacker-quarantine"
        );
        assert_eq!(
            fs::read(&marker).expect("outside marker remains"),
            b"unchanged"
        );
    }

    #[test]
    fn synchronized_link_quarantine_swap_after_verification_is_preserved() {
        let data = tempfile::tempdir().expect("data root");
        let outside = tempfile::tempdir().expect("outside root");
        let outside_config = outside.path().join("outside-config.json");
        fs::write(&outside_config, b"outside-unchanged").expect("outside config marker");
        let paths = AppPaths::from_data_dir(data.path());
        assert_directory_links_available(data.path(), outside.path());

        let candidate = data.path().join("workspace");
        let outside_path = outside.path().to_path_buf();
        let (verified_tx, verified_rx) = mpsc::sync_channel::<PathBuf>(0);
        let (substituted_tx, substituted_rx) = mpsc::sync_channel(0);
        let replacer = thread::spawn(move || {
            let quarantine = verified_rx.recv().expect("verified quarantine path");
            fs::remove_file(&quarantine).expect("remove owned quarantine fixture");
            assert!(create_file_link(&outside_config, &quarantine));
            substituted_tx.send(()).expect("quarantine substituted");
            quarantine
        });

        let result = android_app_private_workspace_with_all_hooks(
            &paths,
            PrivateRootProvenance::CallerSupplied,
            || Ok(()),
            || {},
            || {
                fs::remove_dir(&candidate).expect("remove committed workspace");
                assert!(create_directory_link(&outside_path, &candidate));
            },
            |quarantine| {
                verified_tx
                    .send(quarantine.to_path_buf())
                    .expect("verified quarantine path");
                substituted_rx.recv().expect("quarantine substituted");
            },
        );
        let quarantine = replacer.join().expect("quarantine link replacer");

        assert!(result.is_err());
        assert!(!paths.workspace_config.exists());
        assert!(fs::symlink_metadata(&quarantine)
            .expect("attacker quarantine link remains")
            .file_type()
            .is_symlink());
        assert_eq!(
            fs::read(outside.path().join("outside-config.json")).expect("outside config remains"),
            b"outside-unchanged"
        );
    }

    fn assert_private_outputs_absent(root: &std::path::Path) {
        for name in ["workspace", "workspace.json", "argentum.db", "logs"] {
            assert!(
                !root.join(name).exists(),
                "unexpected private output: {name}"
            );
        }
    }

    fn assert_no_cleanup_config(root: &std::path::Path) {
        assert!(cleanup_configs(root).is_empty(), "unexpected cleanup file");
    }

    fn assert_one_owned_quarantine(root: &std::path::Path) {
        let cleanup = cleanup_configs(root);
        assert_eq!(cleanup.len(), 1, "expected one retained quarantine");
        assert!(fs::symlink_metadata(&cleanup[0])
            .expect("owned quarantine metadata")
            .is_file());
    }

    fn cleanup_configs(root: &std::path::Path) -> Vec<PathBuf> {
        fs::read_dir(root)
            .expect("read private root")
            .filter_map(Result::ok)
            .filter_map(|entry| {
                entry
                    .file_name()
                    .to_string_lossy()
                    .starts_with(".workspace-cleanup-")
                    .then_some(entry.path())
            })
            .collect()
    }

    fn assert_directory_links_available(root: &std::path::Path, target: &std::path::Path) {
        let probe = root.join("link-support-probe");
        assert!(
            create_directory_link(target, &probe),
            "directory links must be available for this platform test"
        );
        fs::remove_dir(&probe).expect("remove link support probe");
    }

    fn write_workspace_fixture(paths: &AppPaths, path: &std::path::Path) {
        fs::create_dir_all(&paths.data_dir).expect("data root");
        let payload = serde_json::to_vec(&crate::WorkspaceConfig {
            path: path.to_path_buf(),
            operation_marker: None,
        })
        .expect("serialize workspace fixture");
        fs::write(&paths.workspace_config, payload).expect("write workspace fixture");
    }

    #[cfg(windows)]
    fn create_directory_link(target: &std::path::Path, link: &std::path::Path) -> bool {
        std::os::windows::fs::symlink_dir(target, link).is_ok()
    }

    #[cfg(windows)]
    fn create_file_link(target: &std::path::Path, link: &std::path::Path) -> bool {
        std::os::windows::fs::symlink_file(target, link).is_ok()
    }

    #[cfg(unix)]
    fn create_directory_link(target: &std::path::Path, link: &std::path::Path) -> bool {
        std::os::unix::fs::symlink(target, link).is_ok()
    }

    #[cfg(unix)]
    fn create_file_link(target: &std::path::Path, link: &std::path::Path) -> bool {
        std::os::unix::fs::symlink(target, link).is_ok()
    }

    #[cfg(not(any(windows, unix)))]
    fn create_directory_link(_target: &std::path::Path, _link: &std::path::Path) -> bool {
        false
    }

    #[cfg(not(any(windows, unix)))]
    fn create_file_link(_target: &std::path::Path, _link: &std::path::Path) -> bool {
        false
    }
}
