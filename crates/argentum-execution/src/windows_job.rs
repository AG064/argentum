use crate::process::ManagedProcessLimits;

#[cfg(windows)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ManagedJobEvidence {
    pub(crate) active_process_limit: u32,
    pub(crate) process_memory_bytes: u64,
    pub(crate) job_memory_bytes: u64,
    pub(crate) cpu_rate: u32,
}

#[cfg(windows)]
pub(crate) struct ProcessContainment {
    job: Option<isize>,
    #[cfg(test)]
    injected_failure: Option<TestContainmentFailure>,
}

#[cfg(all(test, windows))]
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum TestContainmentFailure {
    Terminate,
    Close,
    AssignAfterMarker(std::path::PathBuf),
}

#[cfg(windows)]
impl ProcessContainment {
    pub(crate) fn attach(process_id: u32) -> Result<Self, std::io::Error> {
        let mut containment = Self::create()?;
        if let Err(error_value) = containment.apply_kill_on_close() {
            return Err(containment.setup_error(error_value));
        }
        if let Err(error_value) = containment.assign(process_id) {
            return Err(containment.setup_error(error_value));
        }
        Ok(containment)
    }

    #[cfg(test)]
    pub(crate) fn attach_with_failure(
        process_id: u32,
        failure: TestContainmentFailure,
    ) -> Result<Self, std::io::Error> {
        let mut containment = Self::create()?;
        containment.injected_failure = Some(failure);
        if let Err(error_value) = containment.apply_kill_on_close() {
            return Err(containment.setup_error(error_value));
        }
        if let Err(error_value) = containment.assign(process_id) {
            return Err(containment.setup_error(error_value));
        }
        Ok(containment)
    }

    pub(crate) fn attach_managed(
        process_id: u32,
        limits: ManagedProcessLimits,
    ) -> Result<(Self, ManagedJobEvidence), std::io::Error> {
        let mut containment = Self::create()?;
        if let Err(error_value) = containment.apply_managed_limits(limits) {
            return Err(containment.setup_error(error_value));
        }
        let evidence = match containment.query_managed_limits() {
            Ok(evidence) => evidence,
            Err(error_value) => return Err(containment.setup_error(error_value)),
        };
        if evidence.active_process_limit != limits.active_process_limit()
            || evidence.process_memory_bytes != limits.process_memory_bytes()
            || evidence.job_memory_bytes != limits.job_memory_bytes()
            || evidence.cpu_rate != limits.cpu_rate_percent().saturating_mul(100)
        {
            let error_value = std::io::Error::other("Windows Job Object limits did not round-trip");
            return Err(containment.setup_error(error_value));
        }
        if let Err(error_value) = containment.assign(process_id) {
            return Err(containment.setup_error(error_value));
        }
        Ok((containment, evidence))
    }

    fn create() -> Result<Self, std::io::Error> {
        use windows_sys::Win32::System::JobObjects::CreateJobObjectW;

        // SAFETY: null security attributes and name request an unnamed Job Object.
        let job = unsafe { CreateJobObjectW(std::ptr::null(), std::ptr::null()) };
        if job.is_null() {
            return Err(std::io::Error::last_os_error());
        }
        Ok(Self {
            job: Some(job as isize),
            #[cfg(test)]
            injected_failure: None,
        })
    }

    fn handle(&self) -> Result<isize, std::io::Error> {
        self.job
            .ok_or_else(|| std::io::Error::other("Windows Job Object is already closed"))
    }

    fn setup_error(&mut self, primary: std::io::Error) -> std::io::Error {
        match self.close() {
            Ok(()) => primary,
            Err(cleanup) => cleanup,
        }
    }

    fn apply_kill_on_close(&self) -> Result<(), std::io::Error> {
        use std::ffi::c_void;
        use windows_sys::Win32::System::JobObjects::{
            JobObjectExtendedLimitInformation, SetInformationJobObject,
            JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
        };

        let mut information = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
        information.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
        // SAFETY: `information` is initialized, remains live for the call, and the byte count matches.
        let result = unsafe {
            SetInformationJobObject(
                self.handle()? as _,
                JobObjectExtendedLimitInformation,
                (&raw const information).cast::<c_void>(),
                std::mem::size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
            )
        };
        if result == 0 {
            return Err(std::io::Error::last_os_error());
        }
        Ok(())
    }

    fn apply_managed_limits(&self, limits: ManagedProcessLimits) -> Result<(), std::io::Error> {
        use std::ffi::c_void;
        use windows_sys::Win32::System::JobObjects::{
            JobObjectCpuRateControlInformation, JobObjectExtendedLimitInformation,
            SetInformationJobObject, JOBOBJECT_CPU_RATE_CONTROL_INFORMATION,
            JOBOBJECT_CPU_RATE_CONTROL_INFORMATION_0, JOBOBJECT_EXTENDED_LIMIT_INFORMATION,
            JOB_OBJECT_CPU_RATE_CONTROL_ENABLE, JOB_OBJECT_CPU_RATE_CONTROL_HARD_CAP,
            JOB_OBJECT_LIMIT_ACTIVE_PROCESS, JOB_OBJECT_LIMIT_JOB_MEMORY,
            JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE, JOB_OBJECT_LIMIT_PROCESS_MEMORY,
        };

        let process_memory = usize::try_from(limits.process_memory_bytes())
            .map_err(|_| std::io::Error::other("process memory limit is not representable"))?;
        let job_memory = usize::try_from(limits.job_memory_bytes())
            .map_err(|_| std::io::Error::other("job memory limit is not representable"))?;
        let mut extended = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
        extended.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE
            | JOB_OBJECT_LIMIT_ACTIVE_PROCESS
            | JOB_OBJECT_LIMIT_PROCESS_MEMORY
            | JOB_OBJECT_LIMIT_JOB_MEMORY;
        extended.BasicLimitInformation.ActiveProcessLimit = limits.active_process_limit();
        extended.ProcessMemoryLimit = process_memory;
        extended.JobMemoryLimit = job_memory;
        // SAFETY: `extended` is initialized, remains live for the call, and the byte count matches.
        let extended_result = unsafe {
            SetInformationJobObject(
                self.handle()? as _,
                JobObjectExtendedLimitInformation,
                (&raw const extended).cast::<c_void>(),
                std::mem::size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
            )
        };
        if extended_result == 0 {
            return Err(std::io::Error::last_os_error());
        }

        let cpu = JOBOBJECT_CPU_RATE_CONTROL_INFORMATION {
            ControlFlags: JOB_OBJECT_CPU_RATE_CONTROL_ENABLE | JOB_OBJECT_CPU_RATE_CONTROL_HARD_CAP,
            Anonymous: JOBOBJECT_CPU_RATE_CONTROL_INFORMATION_0 {
                CpuRate: limits.cpu_rate_percent().saturating_mul(100),
            },
        };
        // SAFETY: `cpu` selects the documented `CpuRate` union member and remains live for the call.
        let cpu_result = unsafe {
            SetInformationJobObject(
                self.handle()? as _,
                JobObjectCpuRateControlInformation,
                (&raw const cpu).cast::<c_void>(),
                std::mem::size_of::<JOBOBJECT_CPU_RATE_CONTROL_INFORMATION>() as u32,
            )
        };
        if cpu_result == 0 {
            return Err(std::io::Error::last_os_error());
        }
        Ok(())
    }

    fn query_managed_limits(&self) -> Result<ManagedJobEvidence, std::io::Error> {
        use std::ffi::c_void;
        use windows_sys::Win32::System::JobObjects::{
            JobObjectCpuRateControlInformation, JobObjectExtendedLimitInformation,
            QueryInformationJobObject, JOBOBJECT_CPU_RATE_CONTROL_INFORMATION,
            JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JOB_OBJECT_CPU_RATE_CONTROL_ENABLE,
            JOB_OBJECT_CPU_RATE_CONTROL_HARD_CAP, JOB_OBJECT_LIMIT_ACTIVE_PROCESS,
            JOB_OBJECT_LIMIT_JOB_MEMORY, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
            JOB_OBJECT_LIMIT_PROCESS_MEMORY,
        };

        let mut extended = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
        // SAFETY: `extended` is a valid writable output buffer with the exact reported byte count.
        let extended_result = unsafe {
            QueryInformationJobObject(
                self.handle()? as _,
                JobObjectExtendedLimitInformation,
                (&raw mut extended).cast::<c_void>(),
                std::mem::size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
                std::ptr::null_mut(),
            )
        };
        if extended_result == 0 {
            return Err(std::io::Error::last_os_error());
        }
        let required = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE
            | JOB_OBJECT_LIMIT_ACTIVE_PROCESS
            | JOB_OBJECT_LIMIT_PROCESS_MEMORY
            | JOB_OBJECT_LIMIT_JOB_MEMORY;
        if extended.BasicLimitInformation.LimitFlags & required != required {
            return Err(std::io::Error::other(
                "Windows Job Object limit flags were not applied",
            ));
        }

        let mut cpu = JOBOBJECT_CPU_RATE_CONTROL_INFORMATION::default();
        // SAFETY: `cpu` is a valid writable output buffer with the exact reported byte count.
        let cpu_result = unsafe {
            QueryInformationJobObject(
                self.handle()? as _,
                JobObjectCpuRateControlInformation,
                (&raw mut cpu).cast::<c_void>(),
                std::mem::size_of::<JOBOBJECT_CPU_RATE_CONTROL_INFORMATION>() as u32,
                std::ptr::null_mut(),
            )
        };
        if cpu_result == 0 {
            return Err(std::io::Error::last_os_error());
        }
        let required_cpu =
            JOB_OBJECT_CPU_RATE_CONTROL_ENABLE | JOB_OBJECT_CPU_RATE_CONTROL_HARD_CAP;
        if cpu.ControlFlags & required_cpu != required_cpu {
            return Err(std::io::Error::other(
                "Windows Job Object CPU flags were not applied",
            ));
        }
        // SAFETY: ENABLE plus HARD_CAP selects the `CpuRate` union member by Windows API contract.
        let cpu_rate = unsafe { cpu.Anonymous.CpuRate };
        Ok(ManagedJobEvidence {
            active_process_limit: extended.BasicLimitInformation.ActiveProcessLimit,
            process_memory_bytes: extended.ProcessMemoryLimit as u64,
            job_memory_bytes: extended.JobMemoryLimit as u64,
            cpu_rate,
        })
    }

    fn assign(&mut self, process_id: u32) -> Result<(), std::io::Error> {
        use windows_sys::Win32::Foundation::CloseHandle;
        use windows_sys::Win32::System::JobObjects::AssignProcessToJobObject;
        use windows_sys::Win32::System::Threading::{
            OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_SET_QUOTA, PROCESS_TERMINATE,
        };

        #[cfg(test)]
        if let Some(TestContainmentFailure::AssignAfterMarker(marker)) =
            self.injected_failure.as_ref()
        {
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
            while !marker.exists() && std::time::Instant::now() < deadline {
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
            self.injected_failure = None;
            return Err(std::io::Error::other(
                "injected Windows Job Object assignment failure",
            ));
        }

        let job = self.handle()?;
        // SAFETY: requested access is limited to assigning and terminating the child identified by `process_id`.
        let process = unsafe {
            OpenProcess(
                PROCESS_SET_QUOTA | PROCESS_TERMINATE | PROCESS_QUERY_LIMITED_INFORMATION,
                0,
                process_id,
            )
        };
        if process.is_null() {
            return Err(std::io::Error::last_os_error());
        }
        // SAFETY: both handles are valid for this call and `process` is closed immediately afterwards.
        let assigned = unsafe { AssignProcessToJobObject(job as _, process) };
        let assign_error = (assigned == 0).then(std::io::Error::last_os_error);
        // SAFETY: `process` is a valid handle returned by `OpenProcess` above.
        let close_result = unsafe { CloseHandle(process) };
        if close_result == 0 {
            return Err(std::io::Error::last_os_error());
        }
        if let Some(error_value) = assign_error {
            return Err(error_value);
        }
        Ok(())
    }

    pub(crate) fn terminate(&mut self) -> Result<(), std::io::Error> {
        use windows_sys::Win32::System::JobObjects::TerminateJobObject;

        #[cfg(test)]
        if matches!(
            self.injected_failure.as_ref(),
            Some(TestContainmentFailure::Terminate)
        ) {
            self.injected_failure = None;
            return Err(std::io::Error::other(
                "injected Windows Job Object termination failure",
            ));
        }
        let job = self.handle()?;
        // SAFETY: `self.job` remains open for the lifetime of this containment object.
        let result = unsafe { TerminateJobObject(job as _, 1) };
        if result == 0 {
            return Err(std::io::Error::last_os_error());
        }
        Ok(())
    }

    pub(crate) fn close(&mut self) -> Result<(), std::io::Error> {
        use windows_sys::Win32::Foundation::CloseHandle;

        let Some(job) = self.job else {
            return Ok(());
        };
        #[cfg(test)]
        if matches!(
            self.injected_failure.as_ref(),
            Some(TestContainmentFailure::Close)
        ) {
            self.injected_failure = None;
            return Err(std::io::Error::other(
                "injected Windows Job Object close failure",
            ));
        }
        // SAFETY: `job` is owned by this containment object and remains open until this call succeeds.
        let result = unsafe { CloseHandle(job as _) };
        if result == 0 {
            return Err(std::io::Error::last_os_error());
        }
        self.job = None;
        Ok(())
    }

    #[cfg(test)]
    pub(crate) fn managed_evidence(&self) -> Result<ManagedJobEvidence, std::io::Error> {
        self.query_managed_limits()
    }

    #[cfg(test)]
    pub(crate) fn inject_failure(&mut self, failure: TestContainmentFailure) {
        self.injected_failure = Some(failure);
    }
}

#[cfg(windows)]
impl Drop for ProcessContainment {
    fn drop(&mut self) {
        use windows_sys::Win32::Foundation::CloseHandle;

        if let Some(job) = self.job.take() {
            // SAFETY: `job` is still owned because every successful explicit close clears the option.
            let _fallback_result = unsafe { CloseHandle(job as _) };
        }
    }
}

#[cfg(not(windows))]
pub(crate) struct ProcessContainment;

#[cfg(not(windows))]
impl ProcessContainment {
    pub(crate) fn attach(_process_id: u32) -> Result<Self, std::io::Error> {
        Err(std::io::Error::other(
            "process-group containment is not verified on this platform",
        ))
    }

    pub(crate) fn attach_managed(
        _process_id: u32,
        _limits: ManagedProcessLimits,
    ) -> Result<(Self, ()), std::io::Error> {
        Err(std::io::Error::other(
            "suspended managed processes are currently supported only on Windows",
        ))
    }

    pub(crate) fn terminate(&mut self) -> Result<(), std::io::Error> {
        Ok(())
    }

    pub(crate) fn close(&mut self) -> Result<(), std::io::Error> {
        Ok(())
    }
}
