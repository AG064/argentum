use std::fs::File;
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

#[cfg(windows)]
use crate::private_path::OwnerOnlySecurityDescriptor;

const MAX_PRIVATE_FILE_BYTES: usize = 4 * 1024;

pub struct PrivateFile {
    file: Option<File>,
    path: PathBuf,
}

impl std::fmt::Debug for PrivateFile {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("PrivateFile")
            .field("path", &self.path)
            .field("content", &"[REDACTED]")
            .finish()
    }
}

impl PrivateFile {
    pub fn create_new(path: impl AsRef<Path>, content: &[u8]) -> Result<Self, std::io::Error> {
        let path = path.as_ref();
        if !path.is_absolute()
            || path.file_name().is_none()
            || content.is_empty()
            || content.len() > MAX_PRIVATE_FILE_BYTES
        {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "private file path or content is invalid",
            ));
        }
        reject_reparse_points(path.parent().unwrap_or(path))?;

        #[cfg(windows)]
        let mut file = create_private_windows_file(path)?;
        #[cfg(not(windows))]
        let mut file = create_private_portable_file(path)?;

        #[cfg(windows)]
        if let Err(error_value) = crate::ensure_owner_only_file(path) {
            drop(file);
            let _ = std::fs::remove_file(path);
            return Err(error_value);
        }

        if let Err(error_value) = file.write_all(content).and_then(|()| file.sync_all()) {
            drop(file);
            let _ = std::fs::remove_file(path);
            return Err(error_value);
        }
        Ok(Self {
            file: Some(file),
            path: path.to_path_buf(),
        })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for PrivateFile {
    fn drop(&mut self) {
        if let Some(file) = self.file.take() {
            drop(file);
        }
        let _ = std::fs::remove_file(&self.path);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileIdentity {
    Windows {
        volume_serial: u64,
        file_id: [u8; 16],
    },
    Unix {
        device: u64,
        inode: u64,
    },
    Unsupported,
}

pub struct VerifiedFileLease {
    file: File,
    path: PathBuf,
    identity: FileIdentity,
    size_bytes: u64,
    sha256: String,
    max_bytes: u64,
}

impl std::fmt::Debug for VerifiedFileLease {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("VerifiedFileLease")
            .field("path", &self.path)
            .field("identity", &self.identity)
            .field("size_bytes", &self.size_bytes)
            .field("sha256", &self.sha256)
            .finish()
    }
}

impl VerifiedFileLease {
    pub fn open(path: impl AsRef<Path>, max_bytes: u64) -> Result<Self, std::io::Error> {
        let path = path.as_ref();
        if !path.is_absolute() || path.file_name().is_none() || max_bytes == 0 {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "verified file path and maximum size are invalid",
            ));
        }
        #[cfg(windows)]
        let (canonical, mut file) = {
            let file =
                crate::private_path::open_verified_file_lease(path).map_err(|error_value| {
                    std::io::Error::new(
                        error_value.kind(),
                        format!("verified file lease open failed: {error_value}"),
                    )
                })?;
            let canonical = canonical_path_from_windows_handle(&file)?;
            (canonical, file)
        };
        #[cfg(not(windows))]
        let (canonical, mut file) = {
            reject_reparse_points(path).map_err(|error_value| {
                std::io::Error::new(
                    error_value.kind(),
                    format!("verified path symlink check failed: {error_value}"),
                )
            })?;
            let canonical = std::fs::canonicalize(path).map_err(|error_value| {
                std::io::Error::new(
                    error_value.kind(),
                    format!("verified path canonicalization failed: {error_value}"),
                )
            })?;
            reject_reparse_points(&canonical).map_err(|error_value| {
                std::io::Error::new(
                    error_value.kind(),
                    format!("canonical verified path symlink check failed: {error_value}"),
                )
            })?;
            let file = File::open(&canonical)?;
            (canonical, file)
        };

        let (identity, size_bytes) = file_identity_and_size(&file).map_err(|error_value| {
            std::io::Error::new(
                error_value.kind(),
                format!("verified file identity query failed: {error_value}"),
            )
        })?;
        if size_bytes > max_bytes {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "verified file exceeds its configured maximum size",
            ));
        }
        let sha256 = hash_open_file(&mut file, max_bytes).map_err(|error_value| {
            std::io::Error::new(
                error_value.kind(),
                format!("verified file hashing failed: {error_value}"),
            )
        })?;
        Ok(Self {
            file,
            path: canonical,
            identity,
            size_bytes,
            sha256,
            max_bytes,
        })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub const fn identity(&self) -> FileIdentity {
        self.identity
    }

    pub const fn size_bytes(&self) -> u64 {
        self.size_bytes
    }

    pub fn sha256(&self) -> &str {
        &self.sha256
    }

    pub fn verify_unchanged(&mut self) -> Result<(), std::io::Error> {
        self.verify_unchanged_with(|| Ok(()))
    }

    pub fn verify_unchanged_with<F>(&mut self, checkpoint: F) -> Result<(), std::io::Error>
    where
        F: FnMut() -> Result<(), std::io::Error>,
    {
        let (identity, size_bytes) = file_identity_and_size(&self.file)?;
        if size_bytes > self.max_bytes {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "verified file exceeds its configured maximum size",
            ));
        }
        let sha256 = hash_open_file_with(&mut self.file, self.max_bytes, checkpoint)?;
        if identity != self.identity || size_bytes != self.size_bytes || sha256 != self.sha256 {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "verified file changed while its lease was active",
            ));
        }
        Ok(())
    }

    pub fn verify_identity(&self) -> Result<(), std::io::Error> {
        let (identity, size_bytes) = file_identity_and_size(&self.file)?;
        #[cfg(windows)]
        let path_file = crate::private_path::open_verified_file_lease(&self.path)?;
        #[cfg(not(windows))]
        let path_file = File::open(&self.path)?;
        let (path_identity, path_size_bytes) = file_identity_and_size(&path_file)?;
        if identity != self.identity
            || size_bytes != self.size_bytes
            || path_identity != self.identity
            || path_size_bytes != self.size_bytes
        {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "verified file identity changed while its lease was active",
            ));
        }
        Ok(())
    }
}

pub struct ModelFileLease {
    lease: VerifiedFileLease,
}

impl std::fmt::Debug for ModelFileLease {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ModelFileLease")
            .field("lease", &self.lease)
            .finish()
    }
}

impl ModelFileLease {
    pub fn open(path: impl AsRef<Path>, max_bytes: u64) -> Result<Self, std::io::Error> {
        let path = path.as_ref();
        if !path
            .extension()
            .and_then(|value| value.to_str())
            .is_some_and(|value| value.eq_ignore_ascii_case("gguf"))
        {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "model path must be an absolute GGUF path",
            ));
        }
        Ok(Self {
            lease: VerifiedFileLease::open(path, max_bytes)?,
        })
    }

    pub fn path(&self) -> &Path {
        self.lease.path()
    }

    pub const fn identity(&self) -> FileIdentity {
        self.lease.identity()
    }

    pub const fn size_bytes(&self) -> u64 {
        self.lease.size_bytes()
    }

    pub fn sha256(&self) -> &str {
        self.lease.sha256()
    }

    pub fn verify_unchanged(&mut self) -> Result<(), std::io::Error> {
        self.lease.verify_unchanged()
    }

    pub fn verify_unchanged_with<F>(&mut self, checkpoint: F) -> Result<(), std::io::Error>
    where
        F: FnMut() -> Result<(), std::io::Error>,
    {
        self.lease.verify_unchanged_with(checkpoint)
    }

    pub fn verify_identity(&self) -> Result<(), std::io::Error> {
        self.lease.verify_identity()
    }
}

fn hash_open_file(file: &mut File, max_bytes: u64) -> Result<String, std::io::Error> {
    hash_open_file_with(file, max_bytes, || Ok(()))
}

fn hash_open_file_with<F>(
    file: &mut File,
    max_bytes: u64,
    mut checkpoint: F,
) -> Result<String, std::io::Error>
where
    F: FnMut() -> Result<(), std::io::Error>,
{
    file.seek(SeekFrom::Start(0))?;
    let result = (|| {
        let mut hasher = Sha256::new();
        let mut buffer = [0_u8; 64 * 1024];
        let mut total_bytes = 0_u64;
        loop {
            checkpoint()?;
            let read = file.read(&mut buffer)?;
            if read == 0 {
                break;
            }
            total_bytes = total_bytes.checked_add(read as u64).ok_or_else(|| {
                std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    "verified file size overflowed during hashing",
                )
            })?;
            if total_bytes > max_bytes {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    "verified file exceeds its configured maximum size",
                ));
            }
            hasher.update(&buffer[..read]);
        }
        Ok(format!("{:x}", hasher.finalize()))
    })();
    let reset = file.seek(SeekFrom::Start(0));
    match (result, reset) {
        (Ok(digest), Ok(_)) => Ok(digest),
        (Err(error), _) => Err(error),
        (Ok(_), Err(error)) => Err(error),
    }
}

#[cfg(windows)]
fn reject_reparse_points(path: &Path) -> Result<(), std::io::Error> {
    use std::os::windows::fs::MetadataExt;
    use windows_sys::Win32::Storage::FileSystem::FILE_ATTRIBUTE_REPARSE_POINT;

    let mut current = PathBuf::new();
    for component in path.components() {
        current.push(component);
        if matches!(component, std::path::Component::Prefix(_)) {
            continue;
        }
        match std::fs::symlink_metadata(&current) {
            Ok(metadata) if metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0 => {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidInput,
                    "reparse points are not allowed in protected file paths",
                ));
            }
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => break,
            Err(error) => return Err(error),
        }
    }
    Ok(())
}

#[cfg(not(windows))]
fn reject_reparse_points(path: &Path) -> Result<(), std::io::Error> {
    let mut current = PathBuf::new();
    for component in path.components() {
        current.push(component);
        match std::fs::symlink_metadata(&current) {
            Ok(metadata) if metadata.file_type().is_symlink() => {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidInput,
                    "symbolic links are not allowed in protected file paths",
                ));
            }
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => break,
            Err(error) => return Err(error),
        }
    }
    Ok(())
}

#[cfg(windows)]
fn wide_path(path: &Path) -> Vec<u16> {
    use std::os::windows::ffi::OsStrExt;

    path.as_os_str().encode_wide().chain(Some(0)).collect()
}

#[cfg(windows)]
fn canonical_path_from_windows_handle(file: &File) -> Result<PathBuf, std::io::Error> {
    use std::ffi::OsString;
    use std::os::windows::ffi::OsStringExt;
    use std::os::windows::io::AsRawHandle;
    use windows_sys::Win32::Storage::FileSystem::{
        GetFinalPathNameByHandleW, FILE_NAME_NORMALIZED, VOLUME_NAME_DOS,
    };

    let mut buffer = vec![0_u16; 512];
    loop {
        // SAFETY: the file handle remains open, the buffer points to writable UTF-16 storage of
        // the supplied length, and the flags request a normalized DOS path without extra output.
        let length = unsafe {
            GetFinalPathNameByHandleW(
                file.as_raw_handle(),
                buffer.as_mut_ptr(),
                buffer.len() as u32,
                FILE_NAME_NORMALIZED | VOLUME_NAME_DOS,
            )
        };
        if length == 0 {
            return Err(std::io::Error::last_os_error());
        }
        let length = length as usize;
        if length < buffer.len() {
            buffer.truncate(length);
            return Ok(PathBuf::from(OsString::from_wide(&buffer)));
        }
        if length > 32_768 {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "verified file canonical path is too long",
            ));
        }
        buffer.resize(length.saturating_add(1), 0);
    }
}

#[cfg(windows)]
fn create_private_windows_file(path: &Path) -> Result<File, std::io::Error> {
    use std::os::windows::io::FromRawHandle;
    use windows_sys::Win32::Foundation::{GENERIC_READ, GENERIC_WRITE, INVALID_HANDLE_VALUE};
    use windows_sys::Win32::Storage::FileSystem::{
        CreateFileW, CREATE_NEW, FILE_ATTRIBUTE_TEMPORARY, FILE_SHARE_READ,
    };

    let wide = wide_path(path);
    let mut descriptor = OwnerOnlySecurityDescriptor::owner_rights()?;
    let attributes = descriptor.attributes();
    // SAFETY: `wide` is NUL terminated, `attributes` references a live owner-only descriptor,
    // and a successful owned handle is transferred immediately into `File`.
    let handle = unsafe {
        CreateFileW(
            wide.as_ptr(),
            GENERIC_READ | GENERIC_WRITE,
            FILE_SHARE_READ,
            &attributes,
            CREATE_NEW,
            FILE_ATTRIBUTE_TEMPORARY,
            std::ptr::null_mut(),
        )
    };
    if handle == INVALID_HANDLE_VALUE {
        return Err(std::io::Error::last_os_error());
    }
    // SAFETY: `handle` is a newly owned valid Windows handle and `File` assumes its sole close
    // responsibility.
    Ok(unsafe { File::from_raw_handle(handle) })
}

#[cfg(not(windows))]
fn create_private_portable_file(path: &Path) -> Result<File, std::io::Error> {
    let file = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create_new(true)
        .open(path)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        file.set_permissions(std::fs::Permissions::from_mode(0o600))?;
    }
    Ok(file)
}

#[cfg(windows)]
fn file_identity_and_size(file: &File) -> Result<(FileIdentity, u64), std::io::Error> {
    use std::os::windows::io::AsRawHandle;
    use windows_sys::Win32::Storage::FileSystem::{
        FileIdInfo, FileStandardInfo, GetFileInformationByHandleEx, FILE_ID_INFO,
        FILE_STANDARD_INFO,
    };

    let mut identity = FILE_ID_INFO::default();
    // SAFETY: the file handle remains open, `FileIdInfo` requires exactly `FILE_ID_INFO`, and the
    // writable output buffer has that structure's exact size for the duration of the call.
    let identity_result = unsafe {
        GetFileInformationByHandleEx(
            file.as_raw_handle(),
            FileIdInfo,
            (&mut identity as *mut FILE_ID_INFO).cast(),
            std::mem::size_of::<FILE_ID_INFO>() as u32,
        )
    };
    if identity_result == 0 {
        return Err(std::io::Error::last_os_error());
    }

    let mut standard = FILE_STANDARD_INFO::default();
    // SAFETY: the file handle remains open, `FileStandardInfo` requires exactly
    // `FILE_STANDARD_INFO`, and the writable output buffer has that structure's exact size.
    let standard_result = unsafe {
        GetFileInformationByHandleEx(
            file.as_raw_handle(),
            FileStandardInfo,
            (&mut standard as *mut FILE_STANDARD_INFO).cast(),
            std::mem::size_of::<FILE_STANDARD_INFO>() as u32,
        )
    };
    if standard_result == 0 {
        return Err(std::io::Error::last_os_error());
    }
    if standard.Directory || standard.EndOfFile < 0 {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "verified file handle does not identify a regular file",
        ));
    }
    Ok((
        FileIdentity::Windows {
            volume_serial: identity.VolumeSerialNumber,
            file_id: identity.FileId.Identifier,
        },
        standard.EndOfFile as u64,
    ))
}

#[cfg(not(windows))]
fn file_identity_and_size(file: &File) -> Result<(FileIdentity, u64), std::io::Error> {
    let metadata = file.metadata()?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        return Ok((
            FileIdentity::Unix {
                device: metadata.dev(),
                inode: metadata.ino(),
            },
            metadata.len(),
        ));
    }
    #[allow(unreachable_code)]
    Ok((FileIdentity::Unsupported, metadata.len()))
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;

    #[cfg(windows)]
    #[test]
    fn private_file_is_create_new_readable_and_removed_on_drop() {
        let directory = tempfile::tempdir().expect("private file directory");
        let path = directory.path().join("api-key.txt");
        let private = PrivateFile::create_new(&path, b"private-value").expect("private file");
        assert_eq!(
            fs::read(&path).expect("read private file"),
            b"private-value"
        );
        let evidence = crate::owner_only_acl_evidence(&path).expect("private file ACL evidence");
        assert!(evidence.protected);
        assert_eq!(evidence.inherited_ace_count, 0);
        assert_eq!(evidence.non_owner_allow_ace_count, 0);
        assert!(evidence.owner_full_control);
        assert!(PrivateFile::create_new(&path, b"replacement").is_err());
        drop(private);
        assert!(!path.exists());
    }

    #[cfg(windows)]
    #[test]
    fn model_file_lease_blocks_writers_and_rechecks_identity_and_hash() {
        let directory = tempfile::tempdir().expect("model directory");
        let path = directory.path().join("model.gguf");
        fs::write(&path, b"GGUF-test-model").expect("model fixture");
        let mut lease = ModelFileLease::open(&path, 1024).expect("model file lease");
        assert_eq!(lease.size_bytes(), 15);
        assert_eq!(lease.sha256().len(), 64);
        assert!(fs::OpenOptions::new().write(true).open(&path).is_err());
        lease.verify_unchanged().expect("unchanged model");
        drop(lease);
        fs::OpenOptions::new()
            .write(true)
            .open(&path)
            .expect("writer allowed after lease");
    }

    #[test]
    fn verified_file_lease_records_canonical_identity_size_and_sha256() {
        let directory = tempfile::tempdir().expect("lease directory");
        let path = directory.path().join("launcher.exe");
        let bytes = b"verified launcher";
        fs::write(&path, bytes).expect("launcher fixture");

        let mut lease =
            VerifiedFileLease::open(&path, bytes.len() as u64).expect("verified launcher lease");

        assert_eq!(
            lease.path(),
            fs::canonicalize(&path).expect("canonical path")
        );
        #[cfg(windows)]
        assert!(matches!(
            lease.identity(),
            FileIdentity::Windows { file_id, .. } if file_id != [0; 16]
        ));
        #[cfg(unix)]
        assert!(matches!(
            lease.identity(),
            FileIdentity::Unix { device, inode } if device != 0 && inode != 0
        ));
        #[cfg(not(any(windows, unix)))]
        assert_eq!(lease.identity(), FileIdentity::Unsupported);
        assert_eq!(lease.size_bytes(), bytes.len() as u64);
        assert_eq!(lease.sha256(), format!("{:x}", Sha256::digest(bytes)));
        lease.verify_unchanged().expect("unchanged launcher");
        lease.verify_identity().expect("stable launcher identity");
    }

    #[test]
    fn verified_file_recheck_cooperatively_interrupts_between_bounded_chunks() {
        let directory = tempfile::tempdir().expect("lease directory");
        let path = directory.path().join("large-launcher.exe");
        let bytes = vec![0x5a; 128 * 1024 + 1];
        fs::write(&path, &bytes).expect("large launcher fixture");
        let mut lease =
            VerifiedFileLease::open(&path, bytes.len() as u64).expect("verified launcher lease");
        let mut checkpoints = 0;

        let error = lease
            .verify_unchanged_with(|| {
                checkpoints += 1;
                if checkpoints == 2 {
                    Err(std::io::Error::new(
                        std::io::ErrorKind::Interrupted,
                        "test verification cancellation",
                    ))
                } else {
                    Ok(())
                }
            })
            .expect_err("recheck must stop at the cooperative checkpoint");

        assert_eq!(error.kind(), std::io::ErrorKind::Interrupted);
        assert_eq!(checkpoints, 2);
        lease
            .verify_unchanged()
            .expect("interrupted recheck restored the file position");
    }

    #[test]
    fn verified_file_lease_rejects_files_over_the_configured_maximum() {
        let directory = tempfile::tempdir().expect("lease directory");
        let path = directory.path().join("archive.zip");
        fs::write(&path, b"oversized").expect("archive fixture");

        let error = VerifiedFileLease::open(&path, 8).expect_err("oversized lease");

        assert_eq!(error.kind(), std::io::ErrorKind::InvalidData);
    }

    #[cfg(windows)]
    #[test]
    fn verified_file_lease_blocks_write_and_delete_until_drop() {
        let directory = tempfile::tempdir().expect("lease directory");
        let path = directory.path().join("launcher.exe");
        fs::write(&path, b"verified launcher").expect("launcher fixture");

        let lease = VerifiedFileLease::open(&path, 1024).expect("verified launcher lease");

        assert!(fs::OpenOptions::new().write(true).open(&path).is_err());
        assert!(fs::remove_file(&path).is_err());
        drop(lease);
        fs::OpenOptions::new()
            .write(true)
            .open(&path)
            .expect("writer allowed after lease");
        fs::remove_file(&path).expect("delete allowed after lease");
    }

    #[cfg(not(windows))]
    #[test]
    fn verified_file_lease_detects_path_identity_replacement() {
        let directory = tempfile::tempdir().expect("lease directory");
        let path = directory.path().join("launcher");
        fs::write(&path, b"first launcher").expect("launcher fixture");
        let lease = VerifiedFileLease::open(&path, 1024).expect("verified launcher lease");

        fs::remove_file(&path).expect("remove leased path");
        fs::write(&path, b"replacement launcher").expect("replacement launcher");

        assert_eq!(
            lease
                .verify_identity()
                .expect_err("replacement identity must fail")
                .kind(),
            std::io::ErrorKind::InvalidData
        );
    }
}
