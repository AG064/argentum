//! The Windows execution boundary for the optional Rust check, not a generic executor.
//! Only explicitly owned copies receive package-SID access. No network capabilities.
//! Cargo needs the fixed read-only system-registry capability to initialize Winsock.
use std::collections::BTreeMap;
use std::ffi::{c_void, OsStr};
use std::fs::File;
use std::io;
use std::os::windows::ffi::OsStrExt;
use std::os::windows::fs::MetadataExt;
use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle};
use std::os::windows::process::ExitStatusExt;
use std::path::{Path, PathBuf};
use std::process::ExitStatus;
use std::sync::Arc;
use windows_sys::Win32::Foundation::{
    LocalFree, SetHandleInformation, HANDLE, HANDLE_FLAG_INHERIT, INVALID_HANDLE_VALUE,
    WAIT_OBJECT_0, WAIT_TIMEOUT,
};
use windows_sys::Win32::Security::Authorization::{
    ConvertSidToStringSidW, ConvertStringSecurityDescriptorToSecurityDescriptorW, SetSecurityInfo,
    SDDL_REVISION_1, SE_FILE_OBJECT,
};
use windows_sys::Win32::Security::Isolation::{
    CreateAppContainerProfile, DeleteAppContainerProfile, GetAppContainerFolderPath,
};
use windows_sys::Win32::Security::{
    AccessCheck, DeriveCapabilitySidsFromName, DuplicateToken, EqualSid, FreeSid,
    GetSecurityDescriptorDacl, GetSecurityDescriptorSacl, GetTokenInformation,
    SecurityImpersonation, TokenCapabilities, TokenIsAppContainer, DACL_SECURITY_INFORMATION,
    GENERIC_MAPPING, LABEL_SECURITY_INFORMATION, PROTECTED_DACL_SECURITY_INFORMATION,
    SECURITY_ATTRIBUTES, SECURITY_CAPABILITIES, SID_AND_ATTRIBUTES, TOKEN_DUPLICATE, TOKEN_GROUPS,
    TOKEN_QUERY,
};
use windows_sys::Win32::Storage::FileSystem::{
    CreateFileW, FILE_ATTRIBUTE_REPARSE_POINT, FILE_FLAG_BACKUP_SEMANTICS,
    FILE_FLAG_OPEN_REPARSE_POINT, FILE_READ_ATTRIBUTES, FILE_SHARE_READ, FILE_SHARE_WRITE,
    OPEN_EXISTING, READ_CONTROL, WRITE_DAC, WRITE_OWNER,
};
use windows_sys::Win32::System::Com::CoTaskMemFree;
use windows_sys::Win32::System::Pipes::CreatePipe;
use windows_sys::Win32::System::Threading::{
    CreateProcessW, DeleteProcThreadAttributeList, GetExitCodeProcess,
    InitializeProcThreadAttributeList, OpenProcessToken, TerminateProcess,
    UpdateProcThreadAttribute, WaitForSingleObject, CREATE_NEW_PROCESS_GROUP, CREATE_NO_WINDOW,
    CREATE_SUSPENDED, CREATE_UNICODE_ENVIRONMENT, EXTENDED_STARTUPINFO_PRESENT,
    PROCESS_INFORMATION, PROC_THREAD_ATTRIBUTE_ALL_APPLICATION_PACKAGES_POLICY,
    PROC_THREAD_ATTRIBUTE_HANDLE_LIST, PROC_THREAD_ATTRIBUTE_SECURITY_CAPABILITIES,
    STARTF_USESTDHANDLES, STARTUPINFOEXW,
};

fn wide(value: impl AsRef<OsStr>) -> Vec<u16> {
    value.as_ref().encode_wide().chain(Some(0)).collect()
}
fn checked(ok: i32) -> io::Result<()> {
    if ok == 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(())
    }
}
fn owned(handle: HANDLE) -> io::Result<OwnedHandle> {
    if handle.is_null() || handle == INVALID_HANDLE_VALUE {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: successful Win32 creation transfers this unique valid handle.
    Ok(unsafe { OwnedHandle::from_raw_handle(handle) })
}

pub(crate) struct ProbeSandbox {
    name: Vec<u16>,
    sid: usize,
    registry_sid: usize,
    sid_text: String,
    profile: PathBuf,
    owned_root: PathBuf,
    retired: std::sync::atomic::AtomicBool,
}
impl std::fmt::Debug for ProbeSandbox {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ProbeSandbox")
            .field("boundary", &"windows.lpac.no-network.v1")
            .finish()
    }
}
impl ProbeSandbox {
    pub(crate) fn create(owned_root: &Path) -> io::Result<Arc<Self>> {
        crate::ensure_owner_only_directory(owned_root)?;
        let owned_root = owned_root.canonicalize()?;
        let name = wide(format!("argentum.check.{}", uuid::Uuid::new_v4().simple()));
        let mut sid = std::ptr::null_mut();
        // SAFETY: bounded terminated strings and an initialized output pointer; no capabilities.
        let result = unsafe {
            CreateAppContainerProfile(
                name.as_ptr(),
                name.as_ptr(),
                name.as_ptr(),
                std::ptr::null(),
                0,
                &mut sid,
            )
        };
        if result < 0 {
            return Err(io::Error::other(
                "private check profile could not be created",
            ));
        }
        let mut value = Self {
            name,
            sid: sid as usize,
            registry_sid: 0,
            sid_text: String::new(),
            profile: PathBuf::new(),
            owned_root,
            retired: std::sync::atomic::AtomicBool::new(false),
        };
        let cap_name = wide("registryRead");
        let mut group_sids = std::ptr::null_mut();
        let mut capability_sids = std::ptr::null_mut();
        let mut groups = 0;
        let mut capabilities = 0;
        // SAFETY: the sole capability name is fixed by the host, never a request parameter.
        unsafe {
            checked(DeriveCapabilitySidsFromName(
                cap_name.as_ptr(),
                &mut group_sids,
                &mut groups,
                &mut capability_sids,
                &mut capabilities,
            ))?;
            for index in 0..groups {
                LocalFree((*group_sids.add(index as usize)).cast());
            }
            LocalFree(group_sids.cast());
            if capabilities != 1 {
                for index in 0..capabilities {
                    LocalFree((*capability_sids.add(index as usize)).cast());
                }
                LocalFree(capability_sids.cast());
                return Err(io::Error::other("fixed registry capability unavailable"));
            }
            value.registry_sid = *capability_sids as usize;
            LocalFree(capability_sids.cast());
        }
        let mut text = std::ptr::null_mut();
        // SAFETY: SID returned by the profile API is live; Win32 allocates the string.
        unsafe {
            checked(ConvertSidToStringSidW(sid, &mut text))?;
            let mut length = 0;
            while *text.add(length) != 0 {
                length += 1;
            }
            value.sid_text = String::from_utf16(std::slice::from_raw_parts(text, length))
                .map_err(io::Error::other)?;
            LocalFree(text.cast());
        }
        let sid_string = wide(&value.sid_text);
        let mut profile = std::ptr::null_mut();
        // SAFETY: valid terminated SID and output pointer; returned path is CoTaskMem-owned.
        unsafe {
            if GetAppContainerFolderPath(sid_string.as_ptr(), &mut profile) < 0 {
                return Err(io::Error::other(
                    "private check profile storage unavailable",
                ));
            }
            let mut length = 0;
            while *profile.add(length) != 0 {
                length += 1;
            }
            value.profile = PathBuf::from(
                String::from_utf16(std::slice::from_raw_parts(profile, length))
                    .map_err(io::Error::other)?,
            );
            CoTaskMemFree(profile.cast());
        }
        value.grant_tree(&value.owned_root, false)?;
        Ok(Arc::new(value))
    }

    pub(crate) fn profile(&self) -> &Path {
        &self.profile
    }
    pub(crate) fn retire(&self) -> io::Result<()> {
        if self.retired.load(std::sync::atomic::Ordering::Acquire) {
            return Ok(());
        }
        let status = unsafe { DeleteAppContainerProfile(self.name.as_ptr()) };
        if status < 0 {
            return Err(io::Error::other(
                "owned check profile cleanup did not settle",
            ));
        }
        self.retired
            .store(true, std::sync::atomic::Ordering::Release);
        Ok(())
    }

    pub(crate) fn grant_tree(&self, root: &Path, writable: bool) -> io::Result<()> {
        let root = root.canonicalize()?;
        if !root.starts_with(&self.owned_root) && root != self.profile.canonicalize()? {
            return Err(io::Error::other("check ACL target is not owned"));
        }
        let mut paths = vec![root];
        let mut count = 0usize;
        while let Some(path) = paths.pop() {
            count += 1;
            if count > 4096 {
                return Err(io::Error::other("check ACL inventory exceeds its bound"));
            }
            let metadata = std::fs::symlink_metadata(&path)?;
            if metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
                return Err(io::Error::other("check area contains a reparse point"));
            }
            self.grant_path(&path, writable)?;
            if metadata.is_dir() {
                for entry in std::fs::read_dir(path)? {
                    paths.push(entry?.path());
                }
            }
        }
        Ok(())
    }

    fn grant_path(&self, path: &Path, writable: bool) -> io::Result<()> {
        let path_wide = wide(path);
        // SAFETY: path belongs to the freshly owned tree, no child is running during ACL changes.
        let handle = owned(unsafe {
            CreateFileW(
                path_wide.as_ptr(),
                READ_CONTROL | WRITE_DAC | WRITE_OWNER | FILE_READ_ATTRIBUTES,
                FILE_SHARE_READ | FILE_SHARE_WRITE,
                std::ptr::null(),
                OPEN_EXISTING,
                FILE_FLAG_OPEN_REPARSE_POINT | FILE_FLAG_BACKUP_SEMANTICS,
                std::ptr::null_mut(),
            )
        })?;
        let rights = if writable { "0x1301bf" } else { "0x1200a9" };
        let sddl = wide(format!(
            "D:P(A;OICI;FA;;;OW)(A;OICI;{rights};;;{})S:(ML;OICI;NW;;;LW)",
            self.sid_text
        ));
        let mut sd = std::ptr::null_mut();
        // SAFETY: descriptor remains live for both SetSecurityInfo calls and is freed afterwards.
        unsafe {
            checked(ConvertStringSecurityDescriptorToSecurityDescriptorW(
                sddl.as_ptr(),
                SDDL_REVISION_1,
                &mut sd,
                std::ptr::null_mut(),
            ))?;
            let result = (|| {
                let mut present = 0;
                let mut defaulted = 0;
                let mut acl = std::ptr::null_mut();
                checked(GetSecurityDescriptorDacl(
                    sd,
                    &mut present,
                    &mut acl,
                    &mut defaulted,
                ))?;
                let code = SetSecurityInfo(
                    handle.as_raw_handle(),
                    SE_FILE_OBJECT,
                    DACL_SECURITY_INFORMATION | PROTECTED_DACL_SECURITY_INFORMATION,
                    std::ptr::null_mut(),
                    std::ptr::null_mut(),
                    acl,
                    std::ptr::null_mut(),
                );
                if code != 0 {
                    return Err(io::Error::from_raw_os_error(code as i32));
                }
                checked(GetSecurityDescriptorSacl(
                    sd,
                    &mut present,
                    &mut acl,
                    &mut defaulted,
                ))?;
                let code = SetSecurityInfo(
                    handle.as_raw_handle(),
                    SE_FILE_OBJECT,
                    LABEL_SECURITY_INFORMATION,
                    std::ptr::null_mut(),
                    std::ptr::null_mut(),
                    std::ptr::null_mut(),
                    acl,
                );
                if code != 0 {
                    return Err(io::Error::from_raw_os_error(code as i32));
                }
                Ok(())
            })();
            LocalFree(sd);
            result
        }
    }

    pub(crate) fn spawn(self: &Arc<Self>, spec: &crate::ProcessSpec) -> io::Result<ProbeChild> {
        self.spawn_inner(
            spec,
            #[cfg(test)]
            true,
        )
    }
    fn spawn_inner(
        self: &Arc<Self>,
        spec: &crate::ProcessSpec,
        #[cfg(test)] use_lpac: bool,
    ) -> io::Result<ProbeChild> {
        let program = Path::new(spec.program()).canonicalize()?;
        let cwd = spec.cwd().canonicalize()?;
        if !program.starts_with(&self.owned_root) || !cwd.starts_with(&self.owned_root) {
            return Err(io::Error::other(
                "check executable or working directory is outside its owned copy",
            ));
        }
        let (stdout_read, stdout_write) = pipe()?;
        let (stderr_read, stderr_write) = pipe()?;
        let sa = SECURITY_ATTRIBUTES {
            nLength: std::mem::size_of::<SECURITY_ATTRIBUTES>() as u32,
            lpSecurityDescriptor: std::ptr::null_mut(),
            bInheritHandle: 1,
        };
        let nul = wide("NUL");
        // SAFETY: inherited read-only NUL is the sole stdin; only the three listed handles inherit.
        let stdin = owned(unsafe {
            CreateFileW(
                nul.as_ptr(),
                0x80000000,
                FILE_SHARE_READ | FILE_SHARE_WRITE,
                &sa,
                OPEN_EXISTING,
                0,
                std::ptr::null_mut(),
            )
        })?;
        let attributes = AttributeList::new(3)?;
        let mut capability = SID_AND_ATTRIBUTES {
            Sid: self.registry_sid as _,
            Attributes: 0x4,
        }; // SE_GROUP_ENABLED
        let caps = SECURITY_CAPABILITIES {
            AppContainerSid: self.sid as _,
            Capabilities: &mut capability,
            CapabilityCount: 1,
            Reserved: 0,
        };
        #[cfg(not(test))]
        let lpac: u32 = 1;
        #[cfg(test)]
        let lpac: u32 = u32::from(use_lpac);
        let handles = [
            stdin.as_raw_handle(),
            stdout_write.as_raw_handle(),
            stderr_write.as_raw_handle(),
        ];
        attributes.set(PROC_THREAD_ATTRIBUTE_SECURITY_CAPABILITIES, &caps)?;
        attributes.set(PROC_THREAD_ATTRIBUTE_ALL_APPLICATION_PACKAGES_POLICY, &lpac)?;
        attributes.set_slice(PROC_THREAD_ATTRIBUTE_HANDLE_LIST, &handles)?;
        let mut startup = STARTUPINFOEXW::default();
        startup.StartupInfo.cb = std::mem::size_of::<STARTUPINFOEXW>() as u32;
        startup.StartupInfo.dwFlags = STARTF_USESTDHANDLES;
        startup.StartupInfo.hStdInput = stdin.as_raw_handle();
        startup.StartupInfo.hStdOutput = stdout_write.as_raw_handle();
        startup.StartupInfo.hStdError = stderr_write.as_raw_handle();
        startup.lpAttributeList = attributes.pointer();
        let mut command = wide(
            std::iter::once(spec.program())
                .chain(spec.args().iter().map(String::as_str))
                .map(quote)
                .collect::<Vec<_>>()
                .join(" "),
        );
        let program = wide(program);
        let cwd = wide(cwd);
        let mut environment: BTreeMap<String, String> = spec.environment().clone();
        // Windows appends Packages/<moniker>/AC for AppContainer known folders. Supplying AC
        // here causes a doubled package path in the native linker. The token still restricts
        // reads/writes to this unique profile, not the user's surrounding LocalAppData.
        let local_base = self
            .profile
            .parent()
            .and_then(Path::parent)
            .and_then(Path::parent)
            .ok_or_else(|| io::Error::other("private profile layout unavailable"))?;
        environment.insert(
            "LOCALAPPDATA".into(),
            local_base.to_string_lossy().into_owned(),
        );
        for name in ["SYSTEMROOT", "WINDIR"] {
            environment.insert(name.into(), std::env::var(name).map_err(io::Error::other)?);
        }
        let mut environment: Vec<u16> = environment
            .into_iter()
            .flat_map(|(k, v)| wide(format!("{k}={v}")))
            .collect();
        environment.push(0);
        let mut information = PROCESS_INFORMATION::default();
        // SAFETY: all attribute pointees, strings, exact handle list and environment live until return.
        unsafe {
            checked(CreateProcessW(
                program.as_ptr(),
                command.as_mut_ptr(),
                std::ptr::null(),
                std::ptr::null(),
                1,
                CREATE_SUSPENDED
                    | CREATE_NEW_PROCESS_GROUP
                    | CREATE_UNICODE_ENVIRONMENT
                    | CREATE_NO_WINDOW
                    | EXTENDED_STARTUPINFO_PRESENT,
                environment.as_ptr().cast(),
                cwd.as_ptr(),
                &startup.StartupInfo,
                &mut information,
            ))?;
        }
        let child = ProbeChild {
            process: owned(information.hProcess)?,
            _thread: owned(information.hThread)?,
            pid: information.dwProcessId,
            stdout: Some(tokio::fs::File::from_std(File::from(stdout_read))),
            stderr: Some(tokio::fs::File::from_std(File::from(stderr_read))),
            _sandbox: Arc::clone(self),
        };
        child.verify_token()?;
        Ok(child)
    }
}
impl Drop for ProbeSandbox {
    fn drop(&mut self) {
        // SAFETY: only this randomly named profile/SID is owned; no pre-existing profile is reused.
        unsafe {
            if !self.retired.load(std::sync::atomic::Ordering::Acquire) {
                DeleteAppContainerProfile(self.name.as_ptr());
            }
            FreeSid(self.sid as _);
            LocalFree(self.registry_sid as _);
        }
    }
}

struct AttributeList {
    storage: Vec<usize>,
}
impl AttributeList {
    fn new(count: u32) -> io::Result<Self> {
        let mut bytes = 0usize;
        // SAFETY: first call queries the required byte size; second uses aligned owned allocation.
        unsafe {
            InitializeProcThreadAttributeList(std::ptr::null_mut(), count, 0, &mut bytes);
            if bytes == 0 {
                return Err(io::Error::last_os_error());
            }
            let value = Self {
                storage: vec![0; bytes.div_ceil(std::mem::size_of::<usize>())],
            };
            checked(InitializeProcThreadAttributeList(
                value.pointer(),
                count,
                0,
                &mut bytes,
            ))?;
            Ok(value)
        }
    }
    fn pointer(&self) -> *mut c_void {
        self.storage.as_ptr().cast_mut().cast()
    }
    fn set<T>(&self, attribute: u32, value: &T) -> io::Result<()> {
        self.set_slice(attribute, std::slice::from_ref(value))
    }
    fn set_slice<T>(&self, attribute: u32, value: &[T]) -> io::Result<()> {
        // SAFETY: callers retain every pointee through CreateProcessW. No attribute is model-controlled.
        unsafe {
            checked(UpdateProcThreadAttribute(
                self.pointer(),
                0,
                attribute as usize,
                value.as_ptr().cast(),
                std::mem::size_of_val(value),
                std::ptr::null_mut(),
                std::ptr::null(),
            ))
        }
    }
}
impl Drop for AttributeList {
    fn drop(&mut self) {
        unsafe {
            DeleteProcThreadAttributeList(self.pointer());
        }
    }
}
fn pipe() -> io::Result<(OwnedHandle, OwnedHandle)> {
    let mut read = std::ptr::null_mut();
    let mut write = std::ptr::null_mut();
    let sa = SECURITY_ATTRIBUTES {
        nLength: std::mem::size_of::<SECURITY_ATTRIBUTES>() as u32,
        lpSecurityDescriptor: std::ptr::null_mut(),
        bInheritHandle: 1,
    };
    // SAFETY: valid output slots; read inheritance is disabled before the child is created.
    unsafe {
        checked(CreatePipe(&mut read, &mut write, &sa, 0))?;
        let read = owned(read)?;
        let write = owned(write)?;
        checked(SetHandleInformation(
            read.as_raw_handle(),
            HANDLE_FLAG_INHERIT,
            0,
        ))?;
        Ok((read, write))
    }
}
fn quote(value: &str) -> String {
    let mut result = String::from("\"");
    let mut backslashes = 0;
    for ch in value.chars() {
        if ch == '\\' {
            backslashes += 1;
            continue;
        }
        result.push_str(&"\\".repeat(if ch == '"' {
            backslashes * 2 + 1
        } else {
            backslashes
        }));
        backslashes = 0;
        result.push(ch);
    }
    result.push_str(&"\\".repeat(backslashes * 2));
    result.push('"');
    result
}

pub(crate) struct ProbeChild {
    process: OwnedHandle,
    _thread: OwnedHandle,
    pid: u32,
    pub(crate) stdout: Option<tokio::fs::File>,
    pub(crate) stderr: Option<tokio::fs::File>,
    _sandbox: Arc<ProbeSandbox>,
}
impl ProbeChild {
    pub(crate) fn id(&self) -> u32 {
        self.pid
    }
    fn verify_token(&self) -> io::Result<()> {
        let mut token = std::ptr::null_mut();
        // SAFETY: query the still-suspended child through its retained exact process handle.
        unsafe {
            checked(OpenProcessToken(
                self.process.as_raw_handle(),
                TOKEN_QUERY | TOKEN_DUPLICATE,
                &mut token,
            ))?;
            let token = owned(token)?;
            let mut identity = [0usize; 128];
            let mut identity_bytes = 0;
            checked(GetTokenInformation(
                token.as_raw_handle(),
                windows_sys::Win32::Security::TokenAppContainerSid,
                identity.as_mut_ptr().cast(),
                std::mem::size_of_val(&identity) as u32,
                &mut identity_bytes,
            ))?;
            let identity = &*identity
                .as_ptr()
                .cast::<windows_sys::Win32::Security::TOKEN_APPCONTAINER_INFORMATION>(
            );
            if identity.TokenAppContainer.is_null()
                || EqualSid(identity.TokenAppContainer, self._sandbox.sid as _) == 0
            {
                return Err(io::Error::other(
                    "check token does not match its owned package identity",
                ));
            }
            for class in [TokenIsAppContainer, TokenCapabilities] {
                let mut value = 0u32;
                let mut written = 0;
                // TOKEN_GROUPS with zero entries needs its header allocation, not merely a DWORD.
                let mut groups = [0usize; 128];
                let (pointer, bytes) = if class == TokenCapabilities {
                    (
                        groups.as_mut_ptr().cast(),
                        std::mem::size_of_val(&groups) as u32,
                    )
                } else {
                    ((&mut value as *mut u32).cast(), 4)
                };
                checked(GetTokenInformation(
                    token.as_raw_handle(),
                    class,
                    pointer,
                    bytes,
                    &mut written,
                ))?;
                if class == TokenCapabilities {
                    let actual = &*(groups.as_ptr().cast::<TOKEN_GROUPS>());
                    if actual.GroupCount != 1
                        || EqualSid(actual.Groups[0].Sid, self._sandbox.registry_sid as _) == 0
                    {
                        return Err(io::Error::other("check token capabilities differ from the fixed registry-only definition"));
                    }
                } else if value != 1 {
                    return Err(io::Error::other("check token is not the required LPAC"));
                }
            }
            // Class 46 is not a usable GetTokenInformation query on the supported test host.
            // Require an actual access-check denial to an Everyone + ALL_APPLICATION_PACKAGES
            // descriptor, in addition to the successfully installed opt-out creation attribute.
            // This checks effective restriction without pretending a failed query returned a flag.
            let mut impersonation = std::ptr::null_mut();
            checked(DuplicateToken(
                token.as_raw_handle(),
                SecurityImpersonation,
                &mut impersonation,
            ))?;
            let impersonation = owned(impersonation)?;
            let descriptor = wide("O:SYG:SYD:(A;;0x1;;;WD)(A;;0x1;;;AC)");
            let mut sd = std::ptr::null_mut();
            checked(ConvertStringSecurityDescriptorToSecurityDescriptorW(
                descriptor.as_ptr(),
                SDDL_REVISION_1,
                &mut sd,
                std::ptr::null_mut(),
            ))?;
            let mapping = GENERIC_MAPPING {
                GenericRead: 1,
                GenericWrite: 2,
                GenericExecute: 4,
                GenericAll: 7,
            };
            let mut privileges = [0usize; 128];
            let mut length = std::mem::size_of_val(&privileges) as u32;
            let mut granted = 0;
            let mut access = 0;
            let result = AccessCheck(
                sd,
                impersonation.as_raw_handle(),
                1,
                &mapping,
                privileges.as_mut_ptr().cast(),
                &mut length,
                &mut granted,
                &mut access,
            );
            LocalFree(sd);
            checked(result)?;
            if access != 0 || granted != 0 {
                return Err(io::Error::other(
                    "check token retains all-applications access",
                ));
            }
        }
        Ok(())
    }
    pub(crate) fn try_wait(&mut self) -> io::Result<Option<ExitStatus>> {
        // SAFETY: retained process handle, no PID-based lookup or guessed exit code.
        match unsafe { WaitForSingleObject(self.process.as_raw_handle(), 0) } {
            WAIT_TIMEOUT => Ok(None),
            WAIT_OBJECT_0 => {
                let mut code = 0;
                unsafe {
                    checked(GetExitCodeProcess(self.process.as_raw_handle(), &mut code))?;
                }
                Ok(Some(ExitStatus::from_raw(code)))
            }
            _ => Err(io::Error::last_os_error()),
        }
    }
    pub(crate) async fn wait(&mut self) -> io::Result<ExitStatus> {
        loop {
            if let Some(exit) = self.try_wait()? {
                return Ok(exit);
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
    }
    pub(crate) fn start_kill(&mut self) -> io::Result<()> {
        if self.try_wait()?.is_some() {
            return Ok(());
        }
        unsafe { checked(TerminateProcess(self.process.as_raw_handle(), 1)) }
    }
}
impl Drop for ProbeChild {
    fn drop(&mut self) {
        // A setup failure must never leave a suspended unassigned child behind.
        unsafe {
            TerminateProcess(self.process.as_raw_handle(), 1);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ManagedProcess, ManagedProcessLimits, ProcessSpec};

    #[test]
    fn isolated_child_control() {
        let Some(scratch) = std::env::var_os("ARGENTUM_PROBE_CONTROL_SCRATCH") else {
            return;
        };
        if std::env::var("ARGENTUM_PROBE_CONTROL_MODE").as_deref() == Ok("infinite") {
            loop {
                std::hint::spin_loop();
            }
        }
        let outside =
            std::env::var_os("ARGENTUM_PROBE_CONTROL_OUTSIDE").expect("synthetic outside marker");
        assert!(std::fs::read(&outside).is_err());
        assert!(std::fs::write(&outside, b"OUTSIDE_MUTATION").is_err());
        let readonly =
            std::env::var_os("ARGENTUM_PROBE_CONTROL_READONLY").expect("owned readonly input");
        assert_eq!(
            std::fs::read(&readonly).unwrap(),
            b"ADMITTED_READONLY_BYTES"
        );
        assert!(std::fs::write(&readonly, b"SOURCE_MUTATION").is_err());
        assert!(
            std::fs::hard_link(&outside, PathBuf::from(&scratch).join("outside-hardlink")).is_err()
        );
        let address = std::env::var("ARGENTUM_PROBE_CONTROL_NETWORK")
            .unwrap()
            .parse()
            .unwrap();
        assert!(std::net::TcpStream::connect_timeout(
            &address,
            std::time::Duration::from_millis(500)
        )
        .is_err());
        let private_key = wide(std::env::var("ARGENTUM_PROBE_CONTROL_PRIVATE_KEY").unwrap());
        let mut key = std::ptr::null_mut();
        let code = unsafe {
            windows_sys::Win32::System::Registry::RegOpenKeyExW(
                windows_sys::Win32::System::Registry::HKEY_CURRENT_USER,
                private_key.as_ptr(),
                0,
                windows_sys::Win32::System::Registry::KEY_READ,
                &mut key,
            )
        };
        if code == 0 {
            unsafe {
                windows_sys::Win32::System::Registry::RegCloseKey(key);
            }
        }
        assert_ne!(
            code, 0,
            "owner private registry key became available to the check"
        );
        let absolute_key = wide(std::env::var("ARGENTUM_PROBE_CONTROL_ABSOLUTE_KEY").unwrap());
        let code = unsafe {
            windows_sys::Win32::System::Registry::RegOpenKeyExW(
                windows_sys::Win32::System::Registry::HKEY_USERS,
                absolute_key.as_ptr(),
                0,
                windows_sys::Win32::System::Registry::KEY_READ,
                &mut key,
            )
        };
        if code == 0 {
            unsafe {
                windows_sys::Win32::System::Registry::RegCloseKey(key);
            }
        }
        assert_ne!(
            code, 0,
            "actual owner's HKU key became available to the check"
        );
        std::fs::write(PathBuf::from(scratch).join("owned.txt"), b"OWNED_ONLY")
            .expect("owned scratch write");
    }

    #[tokio::test]
    async fn lpac_managed_child_denies_outside_access() {
        let root = tempfile::tempdir().expect("owned check root");
        let outside = tempfile::tempdir().expect("synthetic private area");
        let marker = outside.path().join("marker.txt");
        std::fs::write(&marker, b"PRIVATE_SYNTHETIC_BYTES").unwrap();
        let exe = root.path().join("child.exe");
        std::fs::copy(std::env::current_exe().unwrap(), &exe).unwrap();
        let scratch = root.path().join("scratch");
        std::fs::create_dir(&scratch).unwrap();
        let readonly = root.path().join("readonly.txt");
        std::fs::write(&readonly, b"ADMITTED_READONLY_BYTES").unwrap();
        let network = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let private_key = format!(
            "Software\\ArgentumProbeControl\\{}",
            uuid::Uuid::new_v4().simple()
        );
        let private_key_wide = wide(&private_key);
        let mut key = std::ptr::null_mut();
        let code = unsafe {
            windows_sys::Win32::System::Registry::RegCreateKeyExW(
                windows_sys::Win32::System::Registry::HKEY_CURRENT_USER,
                private_key_wide.as_ptr(),
                0,
                std::ptr::null(),
                0,
                windows_sys::Win32::System::Registry::KEY_ALL_ACCESS,
                std::ptr::null(),
                &mut key,
                std::ptr::null_mut(),
            )
        };
        assert_eq!(code, 0);
        unsafe {
            windows_sys::Win32::System::Registry::RegCloseKey(key);
        }
        let boundary = ProbeSandbox::create(root.path()).expect("real Windows LPAC");
        boundary.grant_tree(&scratch, true).unwrap();
        let owner_sid = unsafe {
            let mut token = std::ptr::null_mut();
            checked(OpenProcessToken(
                windows_sys::Win32::System::Threading::GetCurrentProcess(),
                TOKEN_QUERY,
                &mut token,
            ))
            .unwrap();
            let token = owned(token).unwrap();
            let mut buffer = [0usize; 128];
            let mut written = 0;
            checked(GetTokenInformation(
                token.as_raw_handle(),
                windows_sys::Win32::Security::TokenUser,
                buffer.as_mut_ptr().cast(),
                std::mem::size_of_val(&buffer) as u32,
                &mut written,
            ))
            .unwrap();
            let user = &*buffer
                .as_ptr()
                .cast::<windows_sys::Win32::Security::TOKEN_USER>();
            let mut text = std::ptr::null_mut();
            checked(ConvertSidToStringSidW(user.User.Sid, &mut text)).unwrap();
            let mut len = 0;
            while *text.add(len) != 0 {
                len += 1;
            }
            let sid = String::from_utf16(std::slice::from_raw_parts(text, len)).unwrap();
            LocalFree(text.cast());
            sid
        };
        let environment = BTreeMap::from([
            (
                "ARGENTUM_PROBE_CONTROL_SCRATCH".into(),
                scratch.to_string_lossy().into_owned(),
            ),
            (
                "ARGENTUM_PROBE_CONTROL_OUTSIDE".into(),
                marker.to_string_lossy().into_owned(),
            ),
            (
                "ARGENTUM_PROBE_CONTROL_READONLY".into(),
                readonly.to_string_lossy().into_owned(),
            ),
            (
                "ARGENTUM_PROBE_CONTROL_NETWORK".into(),
                network.local_addr().unwrap().to_string(),
            ),
            ("TEMP".into(), scratch.to_string_lossy().into_owned()),
            (
                "ARGENTUM_PROBE_CONTROL_PRIVATE_KEY".into(),
                private_key.clone(),
            ),
            (
                "ARGENTUM_PROBE_CONTROL_ABSOLUTE_KEY".into(),
                format!("{owner_sid}\\{private_key}"),
            ),
            ("TMP".into(), scratch.to_string_lossy().into_owned()),
        ]);
        let spec = ProcessSpec::argv(
            exe.to_string_lossy(),
            [
                "--exact".into(),
                "probe_sandbox::tests::isolated_child_control".into(),
            ],
            &scratch,
        )
        .unwrap()
        .with_probe_boundary(Arc::clone(&boundary), environment);
        // The identical non-LPAC token must fail the pre-resume effective-access guard.
        assert!(boundary.spawn_inner(&spec, false).is_err());
        let process = ManagedProcess::spawn(
            spec,
            ManagedProcessLimits::for_fixed_rust_verification().unwrap(),
        )
        .await
        .unwrap();
        let exit = process.wait().await.unwrap();
        assert_eq!(exit.exit_code, Some(0));
        assert_eq!(std::fs::read(marker).unwrap(), b"PRIVATE_SYNTHETIC_BYTES");
        assert_eq!(std::fs::read(readonly).unwrap(), b"ADMITTED_READONLY_BYTES");
        let deleted = unsafe {
            windows_sys::Win32::System::Registry::RegDeleteTreeW(
                windows_sys::Win32::System::Registry::HKEY_CURRENT_USER,
                private_key_wide.as_ptr(),
            )
        };
        assert_eq!(deleted, 0);
        assert_eq!(
            std::fs::read(scratch.join("owned.txt")).unwrap(),
            b"OWNED_ONLY"
        );
    }
    #[tokio::test]
    async fn lpac_native_timeout_and_cancellation_drain_owned_child() {
        for cancelled in [false, true] {
            let root = tempfile::tempdir().unwrap();
            let exe = root.path().join("child.exe");
            std::fs::copy(std::env::current_exe().unwrap(), &exe).unwrap();
            let boundary = ProbeSandbox::create(root.path()).unwrap();
            let env = BTreeMap::from([
                ("ARGENTUM_PROBE_CONTROL_MODE".into(), "infinite".into()),
                (
                    "ARGENTUM_PROBE_CONTROL_SCRATCH".into(),
                    root.path().to_string_lossy().into_owned(),
                ),
            ]);
            let spec = ProcessSpec::argv(
                exe.to_string_lossy(),
                [
                    "--exact".into(),
                    "probe_sandbox::tests::isolated_child_control".into(),
                ],
                root.path(),
            )
            .unwrap()
            .with_probe_boundary(Arc::clone(&boundary), env);
            let process = ManagedProcess::spawn(
                spec,
                ManagedProcessLimits::for_fixed_rust_verification().unwrap(),
            )
            .await
            .unwrap();
            let token = tokio_util::sync::CancellationToken::new();
            let stop = token.clone();
            if cancelled {
                tokio::spawn(async move {
                    tokio::time::sleep(std::time::Duration::from_millis(150)).await;
                    stop.cancel();
                });
            }
            let exit = process
                .wait_with_cancellation(token, std::time::Duration::from_millis(400))
                .await
                .unwrap();
            assert!(exit.exit_code != Some(0));
            assert_eq!(exit.timed_out, !cancelled);
            boundary.retire().unwrap();
        }
    }
}
