use std::fs::{self, File};
use std::io::Read;
use std::path::{Path, PathBuf};

use argentum_security::SecretValue;

const MAX_POINTER_BYTES: u64 = 32 * 1024;
const MAX_SECRETS_BYTES: u64 = 256 * 1024;

pub(crate) struct LegacyCompatibility {
    pub(crate) workspace: Option<PathBuf>,
    pub(crate) minimax_key: Option<SecretValue>,
}

pub(crate) fn discover() -> LegacyCompatibility {
    let workspace = legacy_workspace_pointer()
        .and_then(|pointer| read_bounded_text(&pointer, MAX_POINTER_BYTES))
        .map(|value| PathBuf::from(value.trim()))
        .filter(|path| path.is_absolute() && path.is_dir())
        .and_then(|path| fs::canonicalize(path).ok())
        .filter(|path| is_legacy_workspace(path));
    let minimax_key = workspace
        .as_deref()
        .and_then(|path| read_env_value_inside(path, &path.join("secrets.env"), "MINIMAX_API_KEY"));

    LegacyCompatibility {
        workspace,
        minimax_key,
    }
}

fn legacy_workspace_pointer() -> Option<PathBuf> {
    let local_app_data = std::env::var_os("LOCALAPPDATA")?;
    Some(
        PathBuf::from(local_app_data)
            .join("Programs")
            .join("Argentum")
            .join("workspace")
            .join("data")
            .join("desktop-workspace.txt"),
    )
}

fn is_legacy_workspace(path: &Path) -> bool {
    let marker = path.join("config").join("default.yaml");
    marker.is_file()
        && fs::symlink_metadata(marker)
            .ok()
            .is_some_and(|metadata| !metadata.file_type().is_symlink())
}

fn read_env_value_inside(root: &Path, path: &Path, name: &str) -> Option<SecretValue> {
    let root = fs::canonicalize(root).ok()?;
    let mut ancestor = Some(root.as_path());
    while let Some(current) = ancestor {
        if current.parent().is_none() {
            break;
        }
        if is_reparse_point(current) {
            return None;
        }
        ancestor = current.parent();
    }
    if fs::symlink_metadata(path).ok()?.file_type().is_symlink() {
        return None;
    }
    let path = fs::canonicalize(path).ok()?;
    if !path.starts_with(&root) {
        return None;
    }
    let contents = read_bounded_text(&path, MAX_SECRETS_BYTES)?;
    for line in contents.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let Some((candidate_name, value)) = line.split_once('=') else {
            continue;
        };
        if candidate_name.trim() != name {
            continue;
        }
        let value = unquote(value.trim());
        if value.len() < 16 || value.chars().any(char::is_control) {
            return None;
        }
        return Some(SecretValue::new(value));
    }
    None
}

#[cfg(windows)]
fn is_reparse_point(path: &Path) -> bool {
    use std::os::windows::fs::MetadataExt;

    const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x0000_0400;
    fs::symlink_metadata(path)
        .ok()
        .is_some_and(|metadata| metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0)
}

#[cfg(not(windows))]
fn is_reparse_point(path: &Path) -> bool {
    fs::symlink_metadata(path)
        .ok()
        .is_some_and(|metadata| metadata.file_type().is_symlink())
}

fn read_bounded_text(path: &Path, maximum_bytes: u64) -> Option<String> {
    if fs::symlink_metadata(path).ok()?.file_type().is_symlink() {
        return None;
    }
    let file = File::open(path).ok()?;
    let metadata = file.metadata().ok()?;
    if !metadata.is_file() || metadata.len() > maximum_bytes {
        return None;
    }
    if has_multiple_hard_links(path) {
        return None;
    }
    let mut contents = String::new();
    file.take(maximum_bytes + 1)
        .read_to_string(&mut contents)
        .ok()?;
    if contents.len() as u64 > maximum_bytes {
        return None;
    }
    Some(contents)
}

#[cfg(windows)]
fn has_multiple_hard_links(path: &Path) -> bool {
    use std::os::windows::ffi::OsStrExt;

    #[repr(C)]
    struct ByHandleFileInformation {
        file_attributes: u32,
        creation_time: [u32; 2],
        last_access_time: [u32; 2],
        last_write_time: [u32; 2],
        volume_serial_number: u32,
        file_size_high: u32,
        file_size_low: u32,
        number_of_links: u32,
        file_index_high: u32,
        file_index_low: u32,
    }

    const FILE_READ_ATTRIBUTES: u32 = 0x0080;
    const FILE_SHARE_READ: u32 = 0x0000_0001;
    const FILE_SHARE_WRITE: u32 = 0x0000_0002;
    const FILE_SHARE_DELETE: u32 = 0x0000_0004;
    const OPEN_EXISTING: u32 = 3;
    const FILE_FLAG_OPEN_REPARSE_POINT: u32 = 0x0020_0000;
    const INVALID_HANDLE_VALUE: *mut core::ffi::c_void = -1isize as _;

    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn CreateFileW(
            name: *const u16,
            access: u32,
            share: u32,
            security: *mut core::ffi::c_void,
            creation: u32,
            flags: u32,
            template: *mut core::ffi::c_void,
        ) -> *mut core::ffi::c_void;
        fn GetFileInformationByHandle(
            handle: *mut core::ffi::c_void,
            information: *mut ByHandleFileInformation,
        ) -> i32;
        fn CloseHandle(handle: *mut core::ffi::c_void) -> i32;
    }

    let wide = path
        .as_os_str()
        .encode_wide()
        .chain(Some(0))
        .collect::<Vec<_>>();
    let handle = unsafe {
        CreateFileW(
            wide.as_ptr(),
            FILE_READ_ATTRIBUTES,
            FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
            std::ptr::null_mut(),
            OPEN_EXISTING,
            FILE_FLAG_OPEN_REPARSE_POINT,
            std::ptr::null_mut(),
        )
    };
    if handle == INVALID_HANDLE_VALUE {
        return true;
    }
    let mut information = core::mem::MaybeUninit::<ByHandleFileInformation>::uninit();
    let succeeded = unsafe { GetFileInformationByHandle(handle, information.as_mut_ptr()) } != 0;
    unsafe {
        CloseHandle(handle);
    }
    if !succeeded {
        return true;
    }
    unsafe { information.assume_init() }.number_of_links > 1
}

#[cfg(not(windows))]
fn has_multiple_hard_links(path: &Path) -> bool {
    use std::os::unix::fs::MetadataExt;

    fs::symlink_metadata(path)
        .map(|metadata| metadata.nlink() > 1)
        .unwrap_or(true)
}

fn unquote(value: &str) -> &str {
    if value.len() >= 2 {
        let bytes = value.as_bytes();
        if matches!(bytes.first(), Some(b'\"') | Some(b'\'')) && bytes.first() == bytes.last() {
            return &value[1..value.len() - 1];
        }
    }
    value
}

#[cfg(test)]
mod tests {
    use std::io::Write;

    use tempfile::{tempdir, NamedTempFile};

    use super::*;

    #[test]
    fn reads_only_the_requested_nonempty_env_value() {
        let mut file = NamedTempFile::new().expect("temp file");
        writeln!(file, "OTHER_KEY=other-value-that-is-long").expect("write");
        writeln!(file, "MINIMAX_API_KEY='test-secret-value-1234'").expect("write");

        let root = file.path().parent().expect("temp directory");
        let value = read_env_value_inside(root, file.path(), "MINIMAX_API_KEY").expect("secret");

        assert_eq!(value.expose(), "test-secret-value-1234");
        assert_eq!(format!("{value:?}"), "SecretValue([REDACTED])");
    }

    #[test]
    fn rejects_short_and_oversized_secret_sources() {
        let mut short = NamedTempFile::new().expect("temp file");
        writeln!(short, "MINIMAX_API_KEY=short").expect("write");
        let short_root = short.path().parent().expect("temp directory");
        assert!(read_env_value_inside(short_root, short.path(), "MINIMAX_API_KEY").is_none());

        let mut oversized = NamedTempFile::new().expect("temp file");
        oversized
            .write_all(&vec![b'x'; MAX_SECRETS_BYTES as usize + 1])
            .expect("write");
        let oversized_root = oversized.path().parent().expect("temp directory");
        assert!(
            read_env_value_inside(oversized_root, oversized.path(), "MINIMAX_API_KEY").is_none()
        );
    }

    #[test]
    fn requires_the_legacy_workspace_marker() {
        let root = tempdir().expect("temp directory");
        assert!(!is_legacy_workspace(root.path()));

        fs::create_dir_all(root.path().join("config")).expect("config directory");
        fs::write(
            root.path().join("config").join("default.yaml"),
            "provider: test\n",
        )
        .expect("legacy marker");
        assert!(is_legacy_workspace(root.path()));
    }

    #[test]
    fn rejects_secret_sources_outside_the_selected_workspace() {
        let workspace = tempdir().expect("workspace");
        let outside = NamedTempFile::new().expect("outside file");

        assert!(
            read_env_value_inside(workspace.path(), outside.path(), "MINIMAX_API_KEY").is_none()
        );
    }

    #[test]
    fn rejects_hard_linked_secret_sources() {
        let workspace = tempdir().expect("workspace");
        let outside = NamedTempFile::new().expect("outside file");
        fs::write(outside.path(), "MINIMAX_API_KEY=test-secret-value-1234\n")
            .expect("secret source");
        let linked = workspace.path().join("secrets.env");
        fs::hard_link(outside.path(), &linked).expect("hard link");

        assert!(read_env_value_inside(workspace.path(), &linked, "MINIMAX_API_KEY").is_none());
    }
}
