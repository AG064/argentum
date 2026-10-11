//! Bidirectional IPC on the same suspended/contained process launch boundary.
//! Job limits do not sandbox trusted code's filesystem or network access.
use std::time::Duration;

use tokio::io::AsyncRead;
use tokio::process::{Child, ChildStdin};

use crate::process::{ManagedProcessLimits, ProcessError, ProcessSpec};

pub struct ManagedIpcProcess {
    child: Child,
    containment: super::ProcessContainment,
    stderr: tokio::task::JoinHandle<Result<crate::process::StreamCapture, ProcessError>>,
    pub(crate) process_id: u32,
}

pub struct ManagedIpcStreams {
    pub read: Box<dyn AsyncRead + Unpin + Send>,
    pub write: ChildStdin,
}

impl ManagedIpcProcess {
    pub async fn spawn_checked<F>(
        spec: ProcessSpec,
        limits: ManagedProcessLimits,
        pre_resume_check: F,
    ) -> Result<(Self, ManagedIpcStreams), ProcessError>
    where
        F: FnOnce() -> bool + Send,
    {
        #[cfg(not(windows))]
        {
            let _ = (spec, limits, pre_resume_check);
            Err(ProcessError::Containment(std::io::Error::other(
                "managed extension IPC is currently Windows-only",
            )))
        }
        #[cfg(windows)]
        {
            if spec.launch_kind() != crate::process::ProcessLaunchKind::Argv {
                return Err(ProcessError::ManagedShellNotAllowed);
            }
            let mut command = super::configured_command(&spec, true);
            command.stdin(std::process::Stdio::piped());
            let mut child = command.spawn().map_err(ProcessError::Spawn)?;
            let pid = child.id().ok_or_else(|| {
                ProcessError::Containment(std::io::Error::other("IPC child has no PID"))
            })?;
            let (mut containment, _) = match super::ProcessContainment::attach_managed(pid, limits)
            {
                Ok(value) => value,
                Err(error) => {
                    super::kill_and_reap_child(&mut child).await?;
                    return Err(ProcessError::Containment(error));
                }
            };
            if !pre_resume_check() {
                super::terminate_and_close_containment(&mut containment)
                    .map_err(ProcessError::Containment)?;
                super::kill_and_reap_child(&mut child).await?;
                return Err(ProcessError::PreResumeCheckFailed);
            }
            if let Err(error) = super::resume_process_threads(pid) {
                let close = super::terminate_and_close_containment(&mut containment);
                let reap = super::kill_and_reap_child(&mut child).await;
                close.map_err(ProcessError::Containment)?;
                reap?;
                return Err(ProcessError::Containment(error));
            }
            let read = child.stdout.take().ok_or_else(|| {
                ProcessError::Output(std::io::Error::other("IPC stdout unavailable"))
            })?;
            let write = child.stdin.take().ok_or_else(|| {
                ProcessError::Output(std::io::Error::other("IPC stdin unavailable"))
            })?;
            let stderr = child.stderr.take().ok_or_else(|| {
                ProcessError::Output(std::io::Error::other("IPC stderr unavailable"))
            })?;
            let stderr = tokio::spawn(super::capture_managed_stream(
                stderr,
                crate::process::ProcessStream::Stderr,
                spec.output_limit(),
                limits.output_hash_limit(),
                None,
            ));
            Ok((
                Self {
                    child,
                    containment,
                    stderr,
                    process_id: pid,
                },
                ManagedIpcStreams {
                    read: Box::new(read),
                    write,
                },
            ))
        }
    }

    pub fn id(&self) -> u32 {
        self.process_id
    }

    pub async fn terminate_and_wait(mut self) -> Result<(), ProcessError> {
        let close = super::terminate_and_close_containment(&mut self.containment);
        let reap = super::kill_and_reap_child(&mut self.child).await;
        close.map_err(ProcessError::Containment)?;
        reap?;
        match tokio::time::timeout(Duration::from_secs(5), &mut self.stderr).await {
            Ok(result) => {
                result.map_err(|_| {
                    ProcessError::Output(std::io::Error::other("IPC stderr capture failed"))
                })??;
            }
            Err(_) => {
                self.stderr.abort();
                return Err(ProcessError::Output(std::io::Error::other(
                    "IPC stderr did not settle",
                )));
            }
        }
        Ok(())
    }
}
