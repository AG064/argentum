//! Bidirectional IPC on the same suspended/contained process launch boundary.
//! Job limits do not sandbox trusted code's filesystem or network access.
use std::time::Duration;

use tokio::io::AsyncRead;
use tokio::process::{Child, ChildStdin};

use crate::process::{ManagedProcessLimits, ProcessError, ProcessSpec};

pub struct ManagedIpcProcess {
    child: Child,
    containment: super::ProcessContainment,
    stderr: Option<tokio::task::JoinHandle<Result<crate::process::StreamCapture, ProcessError>>>,
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
            let Some(pid) = child.id() else {
                super::kill_and_reap_child(&mut child).await?;
                return Err(ProcessError::Containment(std::io::Error::other(
                    "IPC child has no PID",
                )));
            };
            // Validate all pipes while the process is still suspended. Every
            // setup error explicitly reaps the exact child, rather than relying
            // on an eventual asynchronous drop after it has been resumed.
            let pipes = (child.stdout.take(), child.stdin.take(), child.stderr.take());
            let (Some(read), Some(write), Some(stderr)) = pipes else {
                super::kill_and_reap_child(&mut child).await?;
                return Err(ProcessError::Output(std::io::Error::other(
                    "IPC standard stream unavailable",
                )));
            };
            let (mut containment, _) = match super::ProcessContainment::attach_managed(pid, limits)
            {
                Ok(value) => value,
                Err(error) => {
                    super::kill_and_reap_child(&mut child).await?;
                    return Err(ProcessError::Containment(error));
                }
            };
            if !pre_resume_check() {
                let close = super::terminate_and_close_containment(&mut containment);
                let reap = super::kill_and_reap_child(&mut child).await;
                close.map_err(ProcessError::Containment)?;
                reap?;
                return Err(ProcessError::PreResumeCheckFailed);
            }
            if let Err(error) = super::resume_process_threads(pid) {
                let close = super::terminate_and_close_containment(&mut containment);
                let reap = super::kill_and_reap_child(&mut child).await;
                close.map_err(ProcessError::Containment)?;
                reap?;
                return Err(ProcessError::Containment(error));
            }
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
                    stderr: Some(stderr),
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

    pub async fn terminate_and_wait(&mut self) -> Result<(), ProcessError> {
        let close = super::terminate_and_close_containment(&mut self.containment);
        let reap = super::kill_and_reap_child(&mut self.child).await;
        let capture = if let Some(mut stderr) = self.stderr.take() {
            match tokio::time::timeout(Duration::from_secs(5), &mut stderr).await {
                Ok(result) => result
                    .map_err(|_| {
                        ProcessError::Output(std::io::Error::other("IPC stderr capture failed"))
                    })
                    .and_then(|capture| capture.map(|_| ())),
                Err(_) => {
                    stderr.abort();
                    let _ = stderr.await;
                    Err(ProcessError::Output(std::io::Error::other(
                        "IPC stderr did not settle",
                    )))
                }
            }
        } else {
            Ok(())
        };
        close.map_err(ProcessError::Containment)?;
        reap?;
        capture
    }
}

impl Drop for ManagedIpcProcess {
    fn drop(&mut self) {
        // The owned Job Object and kill-on-drop child retain process-tree
        // containment. Do not detach a stderr capture on an error path.
        if let Some(stderr) = &self.stderr {
            stderr.abort();
        }
    }
}
