use std::io;
use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OwnerOnlyAclEvidence {
    pub protected: bool,
    pub inherited_ace_count: u32,
    pub non_owner_allow_ace_count: u32,
    pub owner_full_control: bool,
}

#[cfg(windows)]
pub fn ensure_owner_only_directory(path: impl AsRef<Path>) -> io::Result<()> {
    windows::ensure_owner_only(path.as_ref(), windows::PathKind::Directory)
}

#[cfg(not(windows))]
pub fn ensure_owner_only_directory(_path: impl AsRef<Path>) -> io::Result<()> {
    Err(unsupported())
}

#[cfg(windows)]
pub fn ensure_owner_only_file(path: impl AsRef<Path>) -> io::Result<()> {
    windows::ensure_owner_only(path.as_ref(), windows::PathKind::File)
}

#[cfg(not(windows))]
pub fn ensure_owner_only_file(_path: impl AsRef<Path>) -> io::Result<()> {
    Err(unsupported())
}

#[cfg(windows)]
pub fn owner_only_acl_evidence(path: impl AsRef<Path>) -> io::Result<OwnerOnlyAclEvidence> {
    windows::owner_only_acl_evidence(path.as_ref())
}

#[cfg(all(test, windows))]
fn owner_and_dacl_fingerprint(path: impl AsRef<Path>) -> io::Result<Vec<u8>> {
    windows::owner_and_dacl_fingerprint(path.as_ref())
}

#[cfg(not(windows))]
pub fn owner_only_acl_evidence(_path: impl AsRef<Path>) -> io::Result<OwnerOnlyAclEvidence> {
    Err(unsupported())
}

#[cfg(not(windows))]
fn unsupported() -> io::Error {
    io::Error::new(
        io::ErrorKind::Unsupported,
        "owner-only path protection is available only on Windows",
    )
}

#[cfg(windows)]
pub(crate) use windows::{open_verified_file_lease, OwnerOnlySecurityDescriptor};

#[cfg(windows)]
mod windows {
    use std::ffi::OsStr;
    use std::fs::File;
    use std::io;
    use std::os::windows::ffi::OsStrExt;
    use std::os::windows::io::{AsRawHandle, FromRawHandle};
    use std::path::{Component, Path, PathBuf};

    use windows_sys::Win32::Foundation::GENERIC_READ;
    use windows_sys::Win32::Foundation::{LocalFree, ERROR_SUCCESS, INVALID_HANDLE_VALUE};
    use windows_sys::Win32::Security::Authorization::{
        ConvertSidToStringSidW, ConvertStringSecurityDescriptorToSecurityDescriptorW,
        GetSecurityInfo, SetSecurityInfo, SDDL_REVISION_1, SE_FILE_OBJECT,
    };
    use windows_sys::Win32::Security::{
        AclSizeInformation, EqualSid, GetAce, GetAclInformation, GetSecurityDescriptorControl,
        GetSecurityDescriptorDacl, GetSecurityDescriptorOwner, IsValidSid, ACCESS_ALLOWED_ACE, ACL,
        ACL_SIZE_INFORMATION, DACL_SECURITY_INFORMATION, INHERITED_ACE, OWNER_SECURITY_INFORMATION,
        PROTECTED_DACL_SECURITY_INFORMATION, PSECURITY_DESCRIPTOR, PSID, SECURITY_ATTRIBUTES,
        SE_DACL_PROTECTED,
    };
    #[cfg(test)]
    use windows_sys::Win32::Security::{GetSecurityDescriptorLength, SE_SELF_RELATIVE};
    use windows_sys::Win32::Storage::FileSystem::{
        CreateDirectoryW, CreateFileW, FileAttributeTagInfo, GetFileInformationByHandleEx,
        FILE_ALL_ACCESS, FILE_ATTRIBUTE_DIRECTORY, FILE_ATTRIBUTE_REPARSE_POINT,
        FILE_ATTRIBUTE_TAG_INFO, FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OPEN_REPARSE_POINT,
        FILE_READ_ATTRIBUTES, FILE_SHARE_READ, FILE_SHARE_WRITE, OPEN_EXISTING, READ_CONTROL,
        WRITE_DAC,
    };

    use super::OwnerOnlyAclEvidence;

    const ACCESS_ALLOWED_ACE_TYPE_VALUE: u8 = 0;
    const MAX_PROTECTED_PATH_CHARS: usize = 2_048;

    #[derive(Clone, Copy, PartialEq, Eq)]
    pub(crate) enum PathKind {
        Directory,
        File,
    }

    pub(crate) struct OwnerOnlySecurityDescriptor(PSECURITY_DESCRIPTOR);

    impl OwnerOnlySecurityDescriptor {
        pub(crate) fn owner_rights() -> io::Result<Self> {
            Self::from_sddl("D:P(A;;FA;;;OW)")
        }

        fn owner_rights_directory() -> io::Result<Self> {
            Self::from_sddl("D:P(A;OICI;FA;;;OW)")
        }

        fn for_owner(owner: PSID, kind: PathKind) -> io::Result<Self> {
            let owner = LocalWideString::from_sid(owner)?;
            let flags = if kind == PathKind::Directory {
                "OICI"
            } else {
                ""
            };
            let sddl = format!("D:P(A;{flags};FA;;;{})", owner.to_string()?);
            Self::from_sddl(&sddl)
        }

        fn from_sddl(sddl: &str) -> io::Result<Self> {
            let sddl = OsStr::new(sddl)
                .encode_wide()
                .chain(Some(0))
                .collect::<Vec<_>>();
            let mut descriptor = std::ptr::null_mut();
            // SAFETY: `sddl` is NUL terminated, the output pointer is valid, and ownership of the
            // returned descriptor is transferred to `OwnerOnlySecurityDescriptor` on success.
            let converted = unsafe {
                ConvertStringSecurityDescriptorToSecurityDescriptorW(
                    sddl.as_ptr(),
                    SDDL_REVISION_1,
                    &mut descriptor,
                    std::ptr::null_mut(),
                )
            };
            if converted == 0 {
                return Err(io::Error::last_os_error());
            }
            if descriptor.is_null() {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "Windows returned an empty security descriptor",
                ));
            }
            Ok(Self(descriptor))
        }

        pub(crate) fn attributes(&mut self) -> SECURITY_ATTRIBUTES {
            SECURITY_ATTRIBUTES {
                nLength: std::mem::size_of::<SECURITY_ATTRIBUTES>() as u32,
                lpSecurityDescriptor: self.0,
                bInheritHandle: 0,
            }
        }

        fn dacl(&self) -> io::Result<*mut ACL> {
            let mut present = 0;
            let mut defaulted = 0;
            let mut dacl = std::ptr::null_mut();
            // SAFETY: `self.0` is a valid descriptor allocated by the SDDL converter and all
            // output pointers refer to initialized local variables for the duration of the call.
            let result = unsafe {
                GetSecurityDescriptorDacl(self.0, &mut present, &mut dacl, &mut defaulted)
            };
            if result == 0 {
                return Err(io::Error::last_os_error());
            }
            if present == 0 || dacl.is_null() {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "owner-only descriptor has no DACL",
                ));
            }
            Ok(dacl)
        }
    }

    impl Drop for OwnerOnlySecurityDescriptor {
        fn drop(&mut self) {
            // SAFETY: the descriptor was allocated by LocalAlloc through the SDDL converter and
            // is freed exactly once when this owner is dropped.
            let result = unsafe { LocalFree(self.0) };
            debug_assert!(result.is_null(), "LocalFree rejected a security descriptor");
        }
    }

    struct LocalWideString(*mut u16);

    impl LocalWideString {
        fn from_sid(sid: PSID) -> io::Result<Self> {
            if sid.is_null() {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "owner SID is null",
                ));
            }
            let mut value = std::ptr::null_mut();
            // SAFETY: `sid` was returned from a live security descriptor, and `value` is a valid
            // output pointer that receives LocalAlloc-owned memory on success.
            let result = unsafe { ConvertSidToStringSidW(sid, &mut value) };
            if result == 0 {
                return Err(io::Error::last_os_error());
            }
            if value.is_null() {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "Windows returned an empty SID string",
                ));
            }
            Ok(Self(value))
        }

        fn to_string(&self) -> io::Result<String> {
            let mut length = 0_usize;
            // SAFETY: `self.0` points to a NUL-terminated string allocated by
            // `ConvertSidToStringSidW`; the loop reads only through its terminator.
            unsafe {
                while *self.0.add(length) != 0 {
                    length = length.checked_add(1).ok_or_else(|| {
                        io::Error::new(io::ErrorKind::InvalidData, "SID string is too long")
                    })?;
                    if length > 256 {
                        return Err(io::Error::new(
                            io::ErrorKind::InvalidData,
                            "SID string is too long",
                        ));
                    }
                }
                String::from_utf16(std::slice::from_raw_parts(self.0, length)).map_err(|_| {
                    io::Error::new(io::ErrorKind::InvalidData, "SID string is invalid UTF-16")
                })
            }
        }
    }

    impl Drop for LocalWideString {
        fn drop(&mut self) {
            // SAFETY: this string was allocated by LocalAlloc through `ConvertSidToStringSidW`
            // and is freed exactly once when this owner is dropped.
            let result = unsafe { LocalFree(self.0.cast()) };
            debug_assert!(result.is_null(), "LocalFree rejected a SID string");
        }
    }

    struct QueriedSecurityDescriptor(PSECURITY_DESCRIPTOR);

    impl QueriedSecurityDescriptor {
        fn from_handle(file: &File) -> io::Result<Self> {
            let mut descriptor = std::ptr::null_mut();
            // SAFETY: the file handle remains open for the call, all unrequested output pointers
            // are null, and `descriptor` receives LocalAlloc-owned memory on success.
            let result = unsafe {
                GetSecurityInfo(
                    file.as_raw_handle(),
                    SE_FILE_OBJECT,
                    OWNER_SECURITY_INFORMATION | DACL_SECURITY_INFORMATION,
                    std::ptr::null_mut(),
                    std::ptr::null_mut(),
                    std::ptr::null_mut(),
                    std::ptr::null_mut(),
                    &mut descriptor,
                )
            };
            if result != ERROR_SUCCESS {
                return Err(io::Error::from_raw_os_error(result as i32));
            }
            if descriptor.is_null() {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "Windows returned an empty queried security descriptor",
                ));
            }
            Ok(Self(descriptor))
        }

        fn owner(&self) -> io::Result<PSID> {
            let mut owner = std::ptr::null_mut();
            let mut defaulted = 0;
            // SAFETY: `self.0` is a live queried descriptor and both output pointers refer to
            // initialized local variables for the duration of the call.
            let result = unsafe { GetSecurityDescriptorOwner(self.0, &mut owner, &mut defaulted) };
            if result == 0 {
                return Err(io::Error::last_os_error());
            }
            // SAFETY: `owner` points inside the live descriptor and is only passed to the
            // validation routine while that descriptor remains allocated.
            if owner.is_null() || unsafe { IsValidSid(owner) } == 0 {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "owner SID is invalid",
                ));
            }
            Ok(owner)
        }

        fn evidence(&self) -> io::Result<OwnerOnlyAclEvidence> {
            let owner = self.owner()?;
            let mut control = 0_u16;
            let mut revision = 0_u32;
            // SAFETY: `self.0` is a live queried descriptor and the control and revision outputs
            // are valid writable locals.
            let control_result =
                unsafe { GetSecurityDescriptorControl(self.0, &mut control, &mut revision) };
            if control_result == 0 {
                return Err(io::Error::last_os_error());
            }

            let mut present = 0;
            let mut defaulted = 0;
            let mut dacl = std::ptr::null_mut();
            // SAFETY: `self.0` is a live queried descriptor and all DACL output pointers refer to
            // valid writable locals.
            let dacl_result = unsafe {
                GetSecurityDescriptorDacl(self.0, &mut present, &mut dacl, &mut defaulted)
            };
            if dacl_result == 0 {
                return Err(io::Error::last_os_error());
            }
            if present == 0 || dacl.is_null() {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "protected path has no DACL",
                ));
            }

            let mut size = ACL_SIZE_INFORMATION::default();
            // SAFETY: `dacl` points inside the live descriptor and `size` provides the exact
            // writable buffer and length required for `AclSizeInformation`.
            let acl_result = unsafe {
                GetAclInformation(
                    dacl,
                    (&mut size as *mut ACL_SIZE_INFORMATION).cast(),
                    std::mem::size_of::<ACL_SIZE_INFORMATION>() as u32,
                    AclSizeInformation,
                )
            };
            if acl_result == 0 {
                return Err(io::Error::last_os_error());
            }

            let mut inherited_ace_count = 0_u32;
            let mut non_owner_allow_ace_count = 0_u32;
            let mut owner_full_control = false;
            for index in 0..size.AceCount {
                let mut ace = std::ptr::null_mut();
                // SAFETY: `index` is below the ACE count reported for `dacl`, and `ace` is a valid
                // output pointer that receives an address inside the live descriptor.
                let ace_result = unsafe { GetAce(dacl, index, &mut ace) };
                if ace_result == 0 {
                    return Err(io::Error::last_os_error());
                }
                if ace.is_null() {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidData,
                        "Windows returned an empty ACE",
                    ));
                }
                // SAFETY: `GetAce` returned a pointer to an ACE in the live DACL. The header is
                // present for every ACE, and allow ACEs are checked for the required full size
                // before reading their mask and SID start fields.
                unsafe {
                    let header = &*(ace.cast::<windows_sys::Win32::Security::ACE_HEADER>());
                    if header.AceFlags & INHERITED_ACE as u8 != 0 {
                        inherited_ace_count = inherited_ace_count.saturating_add(1);
                    }
                    if header.AceType != ACCESS_ALLOWED_ACE_TYPE_VALUE {
                        continue;
                    }
                    if usize::from(header.AceSize) < std::mem::size_of::<ACCESS_ALLOWED_ACE>() {
                        return Err(io::Error::new(
                            io::ErrorKind::InvalidData,
                            "allow ACE is truncated",
                        ));
                    }
                    let allowed = &*(ace.cast::<ACCESS_ALLOWED_ACE>());
                    let sid = std::ptr::addr_of!(allowed.SidStart).cast_mut().cast();
                    if IsValidSid(sid) == 0 {
                        return Err(io::Error::new(
                            io::ErrorKind::InvalidData,
                            "allow ACE SID is invalid",
                        ));
                    }
                    if EqualSid(owner, sid) != 0 {
                        owner_full_control |= allowed.Mask & FILE_ALL_ACCESS == FILE_ALL_ACCESS;
                    } else {
                        non_owner_allow_ace_count = non_owner_allow_ace_count.saturating_add(1);
                    }
                }
            }

            Ok(OwnerOnlyAclEvidence {
                protected: control & SE_DACL_PROTECTED != 0,
                inherited_ace_count,
                non_owner_allow_ace_count,
                owner_full_control,
            })
        }

        #[cfg(test)]
        fn owner_and_dacl_fingerprint(&self) -> io::Result<Vec<u8>> {
            let mut control = 0_u16;
            let mut revision = 0_u32;
            // SAFETY: `self.0` is a live queried descriptor and the control and revision outputs
            // are valid writable locals.
            let control_result =
                unsafe { GetSecurityDescriptorControl(self.0, &mut control, &mut revision) };
            if control_result == 0 {
                return Err(io::Error::last_os_error());
            }
            if control & SE_SELF_RELATIVE == 0 {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "queried security descriptor is not self-relative",
                ));
            }

            // SAFETY: `self.0` is a valid self-relative descriptor returned by
            // `GetSecurityInfo` and remains allocated for this call.
            let length = unsafe { GetSecurityDescriptorLength(self.0) };
            if length == 0 {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "queried security descriptor has zero length",
                ));
            }
            let length = usize::try_from(length).map_err(|_| {
                io::Error::new(
                    io::ErrorKind::InvalidData,
                    "queried security descriptor length does not fit usize",
                )
            })?;
            // SAFETY: `GetSecurityDescriptorLength` returns the complete contiguous byte length
            // of the live self-relative descriptor. The slice is copied before `self` can drop.
            Ok(unsafe { std::slice::from_raw_parts(self.0.cast::<u8>(), length).to_vec() })
        }
    }

    impl Drop for QueriedSecurityDescriptor {
        fn drop(&mut self) {
            // SAFETY: the descriptor was allocated by LocalAlloc through `GetSecurityInfo` and
            // is freed exactly once when this owner is dropped.
            let result = unsafe { LocalFree(self.0) };
            debug_assert!(result.is_null(), "LocalFree rejected a queried descriptor");
        }
    }

    pub(crate) fn ensure_owner_only(path: &Path, kind: PathKind) -> io::Result<()> {
        validate_path(path)?;
        let _parent_handles = lock_existing_parents(path)?;
        let mut created = false;
        if kind == PathKind::Directory {
            match std::fs::symlink_metadata(path) {
                Ok(_) => {}
                Err(error) if error.kind() == io::ErrorKind::NotFound => {
                    create_owner_only_directory(path)?;
                    created = true;
                }
                Err(error) => return Err(error),
            }
        }

        let result = (|| {
            let file = open_no_follow(path, READ_CONTROL | WRITE_DAC | FILE_READ_ATTRIBUTES)?;
            validate_open_handle(&file, kind)?;
            apply_owner_only_acl(&file, kind)?;
            let evidence = evidence_from_handle(&file)?;
            if !evidence.protected
                || evidence.inherited_ace_count != 0
                || evidence.non_owner_allow_ace_count != 0
                || !evidence.owner_full_control
            {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "owner-only ACL verification failed",
                ));
            }
            Ok(())
        })();
        if result.is_err() && created {
            let _ = std::fs::remove_dir(path);
        }
        result
    }

    pub(crate) fn owner_only_acl_evidence(path: &Path) -> io::Result<OwnerOnlyAclEvidence> {
        validate_path(path)?;
        let _parent_handles = lock_existing_parents(path)?;
        let file = open_no_follow(path, READ_CONTROL | FILE_READ_ATTRIBUTES)?;
        validate_open_handle_exists(&file)?;
        evidence_from_handle(&file)
    }

    #[cfg(test)]
    pub(super) fn owner_and_dacl_fingerprint(path: &Path) -> io::Result<Vec<u8>> {
        validate_path(path)?;
        let _parent_handles = lock_existing_parents(path)?;
        let file = open_no_follow(path, READ_CONTROL | FILE_READ_ATTRIBUTES)?;
        validate_open_handle_exists(&file)?;
        QueriedSecurityDescriptor::from_handle(&file)?.owner_and_dacl_fingerprint()
    }

    pub(crate) fn open_verified_file_lease(path: &Path) -> io::Result<File> {
        validate_path(path)?;
        let _parent_handles = lock_existing_parents(path)?;
        let file = open_no_follow_with_share(path, GENERIC_READ, FILE_SHARE_READ)?;
        validate_open_handle(&file, PathKind::File)?;
        Ok(file)
    }

    fn apply_owner_only_acl(file: &File, kind: PathKind) -> io::Result<()> {
        let current = QueriedSecurityDescriptor::from_handle(file)?;
        let owner = current.owner()?;
        let descriptor = OwnerOnlySecurityDescriptor::for_owner(owner, kind)?;
        let dacl = descriptor.dacl()?;
        // SAFETY: the file handle remains open and validated, `dacl` belongs to the live local
        // descriptor, and null owner, group, and SACL pointers match the requested DACL-only update.
        let result = unsafe {
            SetSecurityInfo(
                file.as_raw_handle(),
                SE_FILE_OBJECT,
                DACL_SECURITY_INFORMATION | PROTECTED_DACL_SECURITY_INFORMATION,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                dacl,
                std::ptr::null_mut(),
            )
        };
        if result != ERROR_SUCCESS {
            return Err(io::Error::from_raw_os_error(result as i32));
        }
        Ok(())
    }

    fn evidence_from_handle(file: &File) -> io::Result<OwnerOnlyAclEvidence> {
        QueriedSecurityDescriptor::from_handle(file)?.evidence()
    }

    fn validate_path(path: &Path) -> io::Result<()> {
        let path_text = path.to_str().ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidInput,
                "protected path must be valid Unicode",
            )
        })?;
        if !path.is_absolute()
            || path.file_name().is_none()
            || path_text.encode_utf16().count() > MAX_PROTECTED_PATH_CHARS
            || path_text.contains('\0')
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "protected path must be a bounded absolute leaf path",
            ));
        }
        Ok(())
    }

    fn lock_existing_parents(path: &Path) -> io::Result<Vec<File>> {
        let parent = path
            .parent()
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "path has no parent"))?;
        let mut current = PathBuf::new();
        let mut handles = Vec::new();
        for component in parent.components() {
            current.push(component.as_os_str());
            if !matches!(component, Component::Normal(_)) {
                continue;
            }
            let file = open_no_follow(&current, FILE_READ_ATTRIBUTES)?;
            validate_open_handle(&file, PathKind::Directory)?;
            handles.push(file);
        }
        Ok(handles)
    }

    fn create_owner_only_directory(path: &Path) -> io::Result<()> {
        let wide = wide_path(path);
        let mut descriptor = OwnerOnlySecurityDescriptor::owner_rights_directory()?;
        let attributes = descriptor.attributes();
        // SAFETY: `wide` is NUL terminated and `attributes` points to a live owner-only security
        // descriptor for the duration of the directory creation call.
        let result = unsafe { CreateDirectoryW(wide.as_ptr(), &attributes) };
        if result == 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(())
    }

    fn open_no_follow(path: &Path, access: u32) -> io::Result<File> {
        open_no_follow_with_share(path, access, FILE_SHARE_READ | FILE_SHARE_WRITE)
    }

    fn open_no_follow_with_share(path: &Path, access: u32, share: u32) -> io::Result<File> {
        let wide = wide_path(path);
        // SAFETY: `wide` is NUL terminated, null security and template pointers are permitted,
        // and a successful owned handle is transferred immediately into `File`.
        let handle = unsafe {
            CreateFileW(
                wide.as_ptr(),
                access,
                share,
                std::ptr::null(),
                OPEN_EXISTING,
                FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT,
                std::ptr::null_mut(),
            )
        };
        if handle == INVALID_HANDLE_VALUE {
            return Err(io::Error::last_os_error());
        }
        // SAFETY: `handle` is a newly owned valid Windows handle and `File` assumes its sole
        // close responsibility.
        Ok(unsafe { File::from_raw_handle(handle) })
    }

    fn validate_open_handle(file: &File, kind: PathKind) -> io::Result<()> {
        let information = file_attribute_tag_info(file)?;
        let is_directory = information.FileAttributes & FILE_ATTRIBUTE_DIRECTORY != 0;
        if (kind == PathKind::Directory) != is_directory {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "protected path has the wrong file type",
            ));
        }
        Ok(())
    }

    fn validate_open_handle_exists(file: &File) -> io::Result<()> {
        let _ = file_attribute_tag_info(file)?;
        Ok(())
    }

    fn file_attribute_tag_info(file: &File) -> io::Result<FILE_ATTRIBUTE_TAG_INFO> {
        let mut information = FILE_ATTRIBUTE_TAG_INFO::default();
        // SAFETY: the file handle remains open, the information class matches the exact output
        // structure, and the buffer length is the structure's size.
        let result = unsafe {
            GetFileInformationByHandleEx(
                file.as_raw_handle(),
                FileAttributeTagInfo,
                (&mut information as *mut FILE_ATTRIBUTE_TAG_INFO).cast(),
                std::mem::size_of::<FILE_ATTRIBUTE_TAG_INFO>() as u32,
            )
        };
        if result == 0 {
            return Err(io::Error::last_os_error());
        }
        if information.FileAttributes & FILE_ATTRIBUTE_REPARSE_POINT != 0
            || information.ReparseTag != 0
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "reparse points are not allowed in owner-only paths",
            ));
        }
        Ok(information)
    }

    fn wide_path(path: &Path) -> Vec<u16> {
        path.as_os_str().encode_wide().chain(Some(0)).collect()
    }
}

#[cfg(test)]
mod tests {
    use std::fs;
    #[cfg(windows)]
    use std::io::{Read, Seek, SeekFrom, Write};
    #[cfg(windows)]
    use std::path::PathBuf;
    #[cfg(windows)]
    use std::process::{Child, Command, ExitStatus, Stdio};
    #[cfg(windows)]
    use std::time::{Duration, Instant};

    use super::*;

    #[cfg(windows)]
    const BOUNDED_COMMAND_SLEEP_ENV: &str = "ARGENTUM_BOUNDED_COMMAND_FIXTURE_SLEEP";
    #[cfg(windows)]
    const BOUNDED_COMMAND_OUTPUT_ENV: &str = "ARGENTUM_BOUNDED_COMMAND_FIXTURE_OUTPUT";
    #[cfg(windows)]
    const TEST_COMMAND_MAX_DIAGNOSTIC_BYTES: usize = 1_024;
    #[cfg(windows)]
    const TEST_COMMAND_POLL_INTERVAL: Duration = Duration::from_millis(10);
    #[cfg(windows)]
    const TEST_COMMAND_REAP_TIMEOUT: Duration = Duration::from_secs(2);

    #[cfg(windows)]
    fn poll_test_child_until(
        child: &mut Child,
        deadline: Instant,
    ) -> io::Result<Option<ExitStatus>> {
        loop {
            if let Some(status) = child.try_wait()? {
                return Ok(Some(status));
            }
            let now = Instant::now();
            if now >= deadline {
                return Ok(None);
            }
            std::thread::sleep(
                deadline
                    .saturating_duration_since(now)
                    .min(TEST_COMMAND_POLL_INTERVAL),
            );
        }
    }

    #[cfg(windows)]
    #[derive(Debug)]
    struct TestCommandOutput {
        status: ExitStatus,
        stdout: Vec<u8>,
        stderr: Vec<u8>,
        stdout_truncated: bool,
        stderr_truncated: bool,
    }

    #[cfg(windows)]
    fn read_bounded_test_command_capture(capture: &mut fs::File) -> io::Result<(Vec<u8>, bool)> {
        let read_limit = TEST_COMMAND_MAX_DIAGNOSTIC_BYTES
            .checked_add(1)
            .ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "test command diagnostic read limit overflow",
                )
            })?;
        let read_limit = u64::try_from(read_limit).map_err(|_| {
            io::Error::new(
                io::ErrorKind::InvalidInput,
                "test command diagnostic read limit does not fit u64",
            )
        })?;
        capture.seek(SeekFrom::Start(0))?;
        let mut bytes = Vec::with_capacity(TEST_COMMAND_MAX_DIAGNOSTIC_BYTES);
        Read::by_ref(capture)
            .take(read_limit)
            .read_to_end(&mut bytes)?;
        let truncated = bytes.len() > TEST_COMMAND_MAX_DIAGNOSTIC_BYTES;
        bytes.truncate(TEST_COMMAND_MAX_DIAGNOSTIC_BYTES);
        Ok((bytes, truncated))
    }

    #[cfg(windows)]
    fn collect_test_command_output(
        status: ExitStatus,
        stdout_capture: &mut fs::File,
        stderr_capture: &mut fs::File,
    ) -> io::Result<TestCommandOutput> {
        let (stdout, stdout_truncated) = read_bounded_test_command_capture(stdout_capture)?;
        let (stderr, stderr_truncated) = read_bounded_test_command_capture(stderr_capture)?;
        Ok(TestCommandOutput {
            status,
            stdout,
            stderr,
            stdout_truncated,
            stderr_truncated,
        })
    }

    #[cfg(windows)]
    fn format_test_command_diagnostic(bytes: &[u8], truncated: bool) -> String {
        let text = String::from_utf8_lossy(bytes);
        if truncated {
            format!("{text} [truncated after {TEST_COMMAND_MAX_DIAGNOSTIC_BYTES} bytes]")
        } else {
            text.into_owned()
        }
    }

    #[cfg(windows)]
    fn kill_and_reap_test_child(child: &mut Child) -> io::Result<(ExitStatus, String)> {
        let process_id = child.id();
        let kill_diagnostic = match child.kill() {
            Ok(()) => "kill succeeded".to_owned(),
            Err(kill_error) => match child.try_wait()? {
                Some(status) => {
                    return Ok((status, format!("kill raced with child exit: {kill_error}")));
                }
                None => {
                    return Err(io::Error::new(
                        kill_error.kind(),
                        format!("test command process {process_id}; kill failed: {kill_error}"),
                    ));
                }
            },
        };
        let reap_deadline = Instant::now()
            .checked_add(TEST_COMMAND_REAP_TIMEOUT)
            .ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "test command reap timeout overflow",
                )
            })?;
        let Some(status) = poll_test_child_until(child, reap_deadline)? else {
            return Err(io::Error::new(
                io::ErrorKind::TimedOut,
                format!(
                    "test command process {process_id}; {kill_diagnostic}; process was not reaped within {TEST_COMMAND_REAP_TIMEOUT:?}"
                ),
            ));
        };
        Ok((status, kill_diagnostic))
    }

    #[cfg(windows)]
    fn run_test_command_bounded(
        command: &mut Command,
        timeout: Duration,
    ) -> io::Result<TestCommandOutput> {
        let deadline = Instant::now().checked_add(timeout).ok_or_else(|| {
            io::Error::new(io::ErrorKind::InvalidInput, "test command timeout overflow")
        })?;
        let mut stdout_capture = tempfile::tempfile()?;
        let mut stderr_capture = tempfile::tempfile()?;
        command
            .stdout(Stdio::from(stdout_capture.try_clone()?))
            .stderr(Stdio::from(stderr_capture.try_clone()?));
        let mut child = command.spawn()?;
        let result = poll_test_child_until(&mut child, deadline);
        let (status, timed_out, kill_diagnostic) = match result {
            Ok(Some(status)) => (status, false, None),
            Ok(None) => {
                let (status, diagnostic) = kill_and_reap_test_child(&mut child)?;
                (status, true, Some(diagnostic))
            }
            Err(error) => {
                let (_, cleanup) = kill_and_reap_test_child(&mut child)?;
                return Err(io::Error::new(
                    error.kind(),
                    format!("test command polling failed: {error}; {cleanup}"),
                ));
            }
        };
        let output = collect_test_command_output(status, &mut stdout_capture, &mut stderr_capture)?;
        if !timed_out {
            return Ok(output);
        }
        let process_id = child.id();
        let kill_diagnostic = kill_diagnostic
            .ok_or_else(|| io::Error::other("timed-out test command has no kill diagnostic"))?;
        Err(io::Error::new(
            io::ErrorKind::TimedOut,
            format!(
                "test command process {process_id} exceeded {timeout:?}; {kill_diagnostic}; \
                 reaped with {}; stdout: {}; stderr: {}",
                output.status,
                format_test_command_diagnostic(&output.stdout, output.stdout_truncated),
                format_test_command_diagnostic(&output.stderr, output.stderr_truncated),
            ),
        ))
    }

    #[cfg(windows)]
    #[test]
    fn bounded_command_fixture_sleeps() {
        if std::env::var_os(BOUNDED_COMMAND_SLEEP_ENV).is_some() {
            std::thread::sleep(Duration::from_secs(1));
        }
    }

    #[cfg(windows)]
    #[test]
    fn bounded_test_command_stops_at_deadline() {
        let mut command = Command::new(std::env::current_exe().expect("current test executable"));
        command
            .args([
                "private_path::tests::bounded_command_fixture_sleeps",
                "--exact",
            ])
            .env(BOUNDED_COMMAND_SLEEP_ENV, "1");

        let error = run_test_command_bounded(&mut command, Duration::from_millis(50))
            .expect_err("sleeping test command must be stopped at its deadline");
        assert_eq!(error.kind(), io::ErrorKind::TimedOut);
        assert!(
            error.to_string().contains("reaped with"),
            "timed-out child must be reaped before the helper returns: {error}"
        );
    }

    #[cfg(windows)]
    #[test]
    fn bounded_test_command_rejects_unrepresentable_deadline_before_spawn() {
        let root = tempfile::tempdir().expect("command fixture root");
        let mut command = Command::new(root.path().join("missing-test-command.exe"));

        let error = run_test_command_bounded(&mut command, Duration::MAX)
            .expect_err("unrepresentable deadline must fail before spawning a command");
        assert_eq!(error.kind(), io::ErrorKind::InvalidInput);
        assert!(error.to_string().contains("test command timeout overflow"));
    }

    #[cfg(windows)]
    #[test]
    fn bounded_command_fixture_emits_large_completed_output() {
        if std::env::var_os(BOUNDED_COMMAND_OUTPUT_ENV).is_some() {
            let output = vec![b'o'; TEST_COMMAND_MAX_DIAGNOSTIC_BYTES * 8];
            std::io::stdout()
                .write_all(&output)
                .expect("write bounded-command stdout fixture");
            std::io::stdout()
                .flush()
                .expect("flush bounded-command stdout fixture");
            std::io::stderr()
                .write_all(&output)
                .expect("write bounded-command stderr fixture");
            std::io::stderr()
                .flush()
                .expect("flush bounded-command stderr fixture");
        }
    }

    #[cfg(windows)]
    #[test]
    fn bounded_test_command_truncates_completed_output_diagnostics() {
        let mut command = Command::new(std::env::current_exe().expect("current test executable"));
        command
            .args([
                "private_path::tests::bounded_command_fixture_emits_large_completed_output",
                "--exact",
            ])
            .env(BOUNDED_COMMAND_OUTPUT_ENV, "1");

        let output = run_test_command_bounded(&mut command, Duration::from_secs(5))
            .expect("output-producing test command must complete within its hard bound");
        assert!(output.status.success());
        assert_eq!(output.stdout.len(), TEST_COMMAND_MAX_DIAGNOSTIC_BYTES);
        assert_eq!(output.stderr.len(), TEST_COMMAND_MAX_DIAGNOSTIC_BYTES);
        assert!(output.stdout_truncated);
        assert!(output.stderr_truncated);
        let stdout_diagnostic =
            format_test_command_diagnostic(&output.stdout, output.stdout_truncated);
        let stderr_diagnostic =
            format_test_command_diagnostic(&output.stderr, output.stderr_truncated);
        assert!(stdout_diagnostic.contains("truncated"));
        assert!(stderr_diagnostic.contains("truncated"));
        assert!(
            stdout_diagnostic.len() < TEST_COMMAND_MAX_DIAGNOSTIC_BYTES * 2,
            "stdout diagnostic must stay bounded, got {} bytes",
            stdout_diagnostic.len()
        );
        assert!(
            stderr_diagnostic.len() < TEST_COMMAND_MAX_DIAGNOSTIC_BYTES * 2,
            "stderr diagnostic must stay bounded, got {} bytes",
            stderr_diagnostic.len()
        );
    }

    #[cfg(windows)]
    #[test]
    fn owner_only_directory_and_file_have_protected_owner_full_control_acls() {
        let root = tempfile::tempdir().expect("private path root");
        let private_dir = root.path().join("private");
        ensure_owner_only_directory(&private_dir).expect("owner-only directory");

        let private_file = private_dir.join("artifact.bin");
        fs::write(&private_file, b"private artifact").expect("private file fixture");
        ensure_owner_only_file(&private_file).expect("owner-only file");

        for evidence in [
            owner_only_acl_evidence(&private_dir).expect("directory ACL evidence"),
            owner_only_acl_evidence(&private_file).expect("file ACL evidence"),
        ] {
            assert!(evidence.protected);
            assert_eq!(evidence.inherited_ace_count, 0);
            assert_eq!(evidence.non_owner_allow_ace_count, 0);
            assert!(evidence.owner_full_control);
        }
    }

    #[cfg(windows)]
    #[test]
    fn owner_only_helpers_reject_relative_root_and_non_unicode_paths() {
        use std::ffi::OsString;
        use std::os::windows::ffi::{OsStrExt, OsStringExt};

        for result in [
            ensure_owner_only_directory(Path::new("relative")),
            ensure_owner_only_file(Path::new("relative.bin")),
            owner_only_acl_evidence(Path::new("relative")).map(|_| ()),
            ensure_owner_only_directory(Path::new(r"C:\")),
        ] {
            assert_eq!(
                result.expect_err("unscoped path must fail").kind(),
                io::ErrorKind::InvalidInput
            );
        }

        let root = tempfile::tempdir().expect("private path root");
        let mut invalid_wide: Vec<u16> = root.path().as_os_str().encode_wide().collect();
        invalid_wide.push(b'\\' as u16);
        invalid_wide.push(0xd800);
        let invalid_path = PathBuf::from(OsString::from_wide(&invalid_wide));
        assert_eq!(
            ensure_owner_only_directory(&invalid_path)
                .expect_err("non-Unicode path must fail before Win32 use")
                .kind(),
            io::ErrorKind::InvalidInput
        );
    }

    #[cfg(windows)]
    #[test]
    fn owner_only_directory_and_file_missing_path_behavior_is_fail_safe() {
        let root = tempfile::tempdir().expect("private path root");
        let missing_parent = root.path().join("missing-parent");
        let nested = missing_parent.join("nested");
        assert_eq!(
            ensure_owner_only_directory(&nested)
                .expect_err("missing parent must fail")
                .kind(),
            io::ErrorKind::NotFound
        );
        assert!(!missing_parent.exists());

        let missing_file = root.path().join("missing.bin");
        assert_eq!(
            ensure_owner_only_file(&missing_file)
                .expect_err("missing final file must fail")
                .kind(),
            io::ErrorKind::NotFound
        );
        assert!(!missing_file.exists());

        let created_directory = root.path().join("created-final");
        ensure_owner_only_directory(&created_directory).expect("create missing final directory");
        assert!(created_directory.is_dir());
        assert!(
            owner_only_acl_evidence(&created_directory)
                .expect("created directory ACL")
                .owner_full_control
        );
    }

    #[cfg(windows)]
    #[test]
    fn owner_only_helpers_reject_existing_wrong_types() {
        let root = tempfile::tempdir().expect("private path root");
        let file = root.path().join("artifact.bin");
        let directory = root.path().join("artifact-directory");
        fs::write(&file, b"artifact").expect("file fixture");
        fs::create_dir(&directory).expect("directory fixture");

        assert_eq!(
            ensure_owner_only_directory(&file)
                .expect_err("file cannot satisfy directory contract")
                .kind(),
            io::ErrorKind::InvalidInput
        );
        assert_eq!(
            ensure_owner_only_file(&directory)
                .expect_err("directory cannot satisfy file contract")
                .kind(),
            io::ErrorKind::InvalidInput
        );
    }

    #[cfg(windows)]
    #[test]
    fn owner_only_directory_supports_an_extended_length_prefix() {
        let root = tempfile::tempdir().expect("private path root");
        let root_text = root.path().to_str().expect("Unicode temporary path");
        let prefixed_root = if root_text.starts_with(r"\\?\") {
            PathBuf::from(root_text)
        } else {
            PathBuf::from(format!(r"\\?\{root_text}"))
        };
        let private = prefixed_root.join("prefixed-private");

        ensure_owner_only_directory(&private).expect("owner-only prefixed directory");
        assert!(
            owner_only_acl_evidence(&private)
                .expect("prefixed directory ACL")
                .owner_full_control
        );
    }

    #[cfg(windows)]
    #[test]
    fn owner_only_helpers_reject_final_and_parent_junctions_without_modifying_targets() {
        use std::os::windows::fs::MetadataExt;
        use std::os::windows::process::CommandExt;

        const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x0000_0400;

        struct JunctionFixture {
            link: PathBuf,
            target: PathBuf,
        }

        impl JunctionFixture {
            fn create(root: &Path, name: &str) -> Self {
                let target = root.join(format!("{name}-target"));
                let link = root.join(format!("{name}-junction"));
                fs::create_dir(&target).expect("junction target directory");
                let command = format!(r#"mklink /J "{}" "{}""#, link.display(), target.display());
                let mut process = Command::new("cmd.exe");
                process.args(["/D", "/C"]).raw_arg(&command);
                let output = run_test_command_bounded(&mut process, Duration::from_secs(5))
                    .expect("create directory junction without administrator access");
                assert!(
                    output.status.success(),
                    "mklink /J failed with {}; stdout: {}; stderr: {}",
                    output.status,
                    format_test_command_diagnostic(&output.stdout, output.stdout_truncated),
                    format_test_command_diagnostic(&output.stderr, output.stderr_truncated)
                );
                let metadata = fs::symlink_metadata(&link).expect("junction metadata");
                assert_ne!(
                    metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT,
                    0,
                    "mklink /J fixture was not a reparse point"
                );
                Self { link, target }
            }

            fn remove(self) {
                fs::remove_dir(&self.link).expect("remove junction without traversing its target");
                assert!(
                    self.target.is_dir(),
                    "junction cleanup removed the real target"
                );
            }
        }

        impl Drop for JunctionFixture {
            fn drop(&mut self) {
                let _ = fs::remove_dir(&self.link);
            }
        }

        let root = tempfile::tempdir().expect("private path root");
        let final_junction = JunctionFixture::create(root.path(), "final");
        let parent_junction = JunctionFixture::create(root.path(), "parent");
        let final_sentinel = final_junction.target.join("sentinel.txt");
        let parent_sentinel = parent_junction.target.join("sentinel.txt");
        fs::write(&final_sentinel, b"final target unchanged").expect("final sentinel");
        fs::write(&parent_sentinel, b"parent target unchanged").expect("parent sentinel");
        let final_acl_before =
            owner_only_acl_evidence(&final_junction.target).expect("final target ACL evidence");
        let parent_acl_before =
            owner_only_acl_evidence(&parent_junction.target).expect("parent target ACL evidence");
        let final_owner_and_dacl_before = owner_and_dacl_fingerprint(&final_junction.target)
            .expect("final target owner-and-DACL snapshot");
        let parent_owner_and_dacl_before = owner_and_dacl_fingerprint(&parent_junction.target)
            .expect("parent target owner-and-DACL snapshot");
        assert!(
            !final_owner_and_dacl_before.is_empty(),
            "final target owner-and-DACL snapshot must not be empty"
        );
        assert!(
            !parent_owner_and_dacl_before.is_empty(),
            "parent target owner-and-DACL snapshot must not be empty"
        );

        assert_eq!(
            ensure_owner_only_directory(&final_junction.link)
                .expect_err("final junction must fail")
                .kind(),
            io::ErrorKind::InvalidInput
        );
        assert_eq!(
            ensure_owner_only_directory(parent_junction.link.join("nested"))
                .expect_err("parent junction must fail")
                .kind(),
            io::ErrorKind::InvalidInput
        );
        assert_eq!(
            fs::read(&final_sentinel).expect("final sentinel after rejection"),
            b"final target unchanged"
        );
        assert_eq!(
            fs::read(&parent_sentinel).expect("parent sentinel after rejection"),
            b"parent target unchanged"
        );
        assert_eq!(
            owner_only_acl_evidence(&final_junction.target)
                .expect("final target ACL after rejection"),
            final_acl_before
        );
        assert_eq!(
            owner_only_acl_evidence(&parent_junction.target)
                .expect("parent target ACL after rejection"),
            parent_acl_before
        );
        assert_eq!(
            owner_and_dacl_fingerprint(&final_junction.target)
                .expect("final target owner-and-DACL snapshot after rejection"),
            final_owner_and_dacl_before
        );
        assert_eq!(
            owner_and_dacl_fingerprint(&parent_junction.target)
                .expect("parent target owner-and-DACL snapshot after rejection"),
            parent_owner_and_dacl_before
        );
        assert!(
            !parent_junction.target.join("nested").exists(),
            "parent junction allowed the helper to create a child in the real target"
        );

        let final_target = final_junction.target.clone();
        let parent_target = parent_junction.target.clone();
        final_junction.remove();
        assert!(final_target.is_dir(), "final target missing after cleanup");
        assert_eq!(
            fs::read(&final_sentinel).expect("final sentinel after junction cleanup"),
            b"final target unchanged"
        );
        assert_eq!(
            owner_and_dacl_fingerprint(&final_target)
                .expect("final target owner-and-DACL snapshot after junction cleanup"),
            final_owner_and_dacl_before
        );

        parent_junction.remove();
        assert!(
            parent_target.is_dir(),
            "parent target missing after cleanup"
        );
        assert_eq!(
            fs::read(&parent_sentinel).expect("parent sentinel after junction cleanup"),
            b"parent target unchanged"
        );
        assert!(
            !parent_target.join("nested").exists(),
            "junction cleanup left an unexpected child in the real parent target"
        );
        assert_eq!(
            owner_and_dacl_fingerprint(&parent_target)
                .expect("parent target owner-and-DACL snapshot after junction cleanup"),
            parent_owner_and_dacl_before
        );
    }

    #[cfg(not(windows))]
    #[test]
    fn owner_only_path_helpers_are_explicitly_unsupported() {
        let root = tempfile::tempdir().expect("private path root");
        let file = root.path().join("artifact.bin");
        fs::write(&file, b"private artifact").expect("private file fixture");

        for result in [
            ensure_owner_only_directory(root.path()),
            ensure_owner_only_file(&file),
        ] {
            assert_eq!(
                result
                    .expect_err("owner-only path operation must be unsupported")
                    .kind(),
                std::io::ErrorKind::Unsupported
            );
        }
        assert_eq!(
            owner_only_acl_evidence(&file)
                .expect_err("owner-only ACL evidence must be unsupported")
                .kind(),
            std::io::ErrorKind::Unsupported
        );
    }
}
