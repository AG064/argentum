use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::{ExitStatus, Stdio};
use std::time::Duration;

use argentum_domain::Sha256Digest;
use argentum_security::{validate_environment_name, validate_process_argument, SecurityError};
use serde::Serialize;
use sha2::{Digest, Sha256};
use thiserror::Error;
use tokio::io::{AsyncRead, AsyncReadExt};
use tokio::process::Command;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use crate::executable::resolve_fixed_executable;
use crate::windows_job::ProcessContainment;

const DEFAULT_TIMEOUT: Duration = Duration::from_secs(120);
const MAX_TIMEOUT: Duration = Duration::from_secs(600);
const DEFAULT_OUTPUT_LIMIT: usize = 256 * 1024;
const MAX_OUTPUT_LIMIT: usize = 1024 * 1024;
const MAX_PROGRAM_BYTES: usize = 4096;
const MAX_ARGUMENTS: usize = 128;
const MAX_ARGUMENT_BYTES: usize = 64 * 1024;
const MAX_ENVIRONMENT_ENTRIES: usize = 32;
const MAX_ENVIRONMENT_VALUE_BYTES: usize = 8 * 1024;
const MAX_SHELL_COMMAND_BYTES: usize = 64 * 1024;
const CAPTURE_SETTLEMENT_TIMEOUT: Duration = Duration::from_secs(5);
const MIB: u64 = 1024 * 1024;
const GIB: u64 = 1024 * MIB;
const MIN_MANAGED_MEMORY_BYTES: u64 = 256 * MIB;
const MIN_LOCAL_MODEL_MEMORY_BYTES: u64 = 4 * GIB;
const MAX_LOCAL_MODEL_MEMORY_BYTES: u64 = 512 * GIB;

#[derive(Debug, Error)]
pub enum ProcessError {
    #[error(transparent)]
    Security(#[from] SecurityError),
    #[error("process program is empty, invalid, or exceeds its bound")]
    InvalidProgram,
    #[error("process argument count or total size exceeds its bound")]
    InvalidArguments,
    #[error("process working directory is unavailable: {0}")]
    InvalidWorkingDirectory(PathBuf),
    #[error("process timeout must be from 1 millisecond through 10 minutes")]
    InvalidTimeout,
    #[error("process output limit must be from 1 byte through 1 MiB per stream")]
    InvalidOutputLimit,
    #[error("process environment override is not permitted")]
    InvalidEnvironment,
    #[error("shell command is empty, invalid, or exceeds its bound")]
    InvalidShellCommand,
    #[error("trusted fixed executable is unavailable: {0}")]
    FixedExecutableUnavailable(String),
    #[error("process could not start: {0}")]
    Spawn(std::io::Error),
    #[error("process-tree containment could not be established: {0}")]
    Containment(std::io::Error),
    #[error("process wait failed: {0}")]
    Wait(std::io::Error),
    #[error("process output read failed: {0}")]
    Output(std::io::Error),
    #[error("process output capture did not settle before deadline")]
    OutputSettlementTimeout,
    #[error("process output task failed: {0}")]
    OutputTask(String),
    #[error("managed processes require an argument-vector launch")]
    ManagedShellNotAllowed,
    #[error("managed process pre-resume check failed")]
    PreResumeCheckFailed,
    #[error("managed process resource limits are invalid or cannot be represented")]
    InvalidManagedLimits,
    #[error("managed process resume failed: {0}")]
    Resume(std::io::Error),
    #[error("managed process listener ownership could not be verified: {0}")]
    ListenerOwnership(std::io::Error),
    #[error("managed process opened a listener outside literal IPv4 loopback")]
    UnsafeListenerAddress,
    #[error("managed process identity changed or is no longer available")]
    ProcessIdentityChanged,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProcessLaunchKind {
    Argv,
    Shell,
}

#[derive(Debug, Clone)]
pub struct ProcessSpec {
    launch_kind: ProcessLaunchKind,
    program: String,
    args: Vec<String>,
    cwd: PathBuf,
    timeout: Duration,
    output_limit: usize,
    environment: BTreeMap<String, String>,
    #[cfg(windows)]
    probe_boundary: Option<std::sync::Arc<crate::probe_sandbox::ProbeSandbox>>,
}

impl ProcessSpec {
    pub fn argv(
        program: impl Into<String>,
        args: impl IntoIterator<Item = String>,
        cwd: impl AsRef<Path>,
    ) -> Result<Self, ProcessError> {
        let program = program.into();
        if program.trim().is_empty()
            || program.len() > MAX_PROGRAM_BYTES
            || program.chars().any(char::is_control)
        {
            return Err(ProcessError::InvalidProgram);
        }
        validate_process_argument(&program)?;
        let args = args.into_iter().collect::<Vec<_>>();
        if args.len() > MAX_ARGUMENTS
            || args
                .iter()
                .map(String::len)
                .sum::<usize>()
                .saturating_add(args.len())
                > MAX_ARGUMENT_BYTES
        {
            return Err(ProcessError::InvalidArguments);
        }
        for argument in &args {
            validate_process_argument(argument)?;
        }
        let cwd = std::fs::canonicalize(cwd.as_ref())
            .map_err(|_| ProcessError::InvalidWorkingDirectory(cwd.as_ref().to_path_buf()))?;
        if !cwd.is_dir() {
            return Err(ProcessError::InvalidWorkingDirectory(cwd));
        }
        Ok(Self {
            launch_kind: ProcessLaunchKind::Argv,
            program,
            args,
            cwd,
            timeout: DEFAULT_TIMEOUT,
            output_limit: DEFAULT_OUTPUT_LIMIT,
            environment: BTreeMap::new(),
            #[cfg(windows)]
            probe_boundary: None,
        })
    }

    pub fn shell(command: impl Into<String>, cwd: impl AsRef<Path>) -> Result<Self, ProcessError> {
        let command = command.into();
        if command.trim().is_empty()
            || command.len() > MAX_SHELL_COMMAND_BYTES
            || command.contains('\0')
            || command
                .chars()
                .any(|character| character.is_control() && !matches!(character, '\t' | '\r' | '\n'))
        {
            return Err(ProcessError::InvalidShellCommand);
        }
        validate_process_argument(&command)?;
        let cwd = cwd.as_ref();
        #[cfg(windows)]
        let (program, args) = (
            resolve_fixed_executable("powershell.exe", &[cwd])
                .ok_or_else(|| ProcessError::FixedExecutableUnavailable("powershell.exe".into()))?
                .to_string_lossy()
                .into_owned(),
            vec![
                "-NoLogo".to_owned(),
                "-NoProfile".to_owned(),
                "-NonInteractive".to_owned(),
                "-Command".to_owned(),
                command,
            ],
        );
        #[cfg(not(windows))]
        let (program, args) = ("/bin/sh".to_owned(), vec!["-c".to_owned(), command]);
        let mut spec = Self::argv(program, args, cwd)?;
        spec.launch_kind = ProcessLaunchKind::Shell;
        Ok(spec)
    }

    pub fn with_timeout(mut self, timeout: Duration) -> Result<Self, ProcessError> {
        if timeout.is_zero() || timeout > MAX_TIMEOUT {
            return Err(ProcessError::InvalidTimeout);
        }
        self.timeout = timeout;
        Ok(self)
    }

    pub fn with_output_limit(mut self, output_limit: usize) -> Result<Self, ProcessError> {
        if output_limit == 0 || output_limit > MAX_OUTPUT_LIMIT {
            return Err(ProcessError::InvalidOutputLimit);
        }
        self.output_limit = output_limit;
        Ok(self)
    }

    pub fn with_environment(
        mut self,
        environment: BTreeMap<String, String>,
    ) -> Result<Self, ProcessError> {
        if environment.len() > MAX_ENVIRONMENT_ENTRIES {
            return Err(ProcessError::InvalidEnvironment);
        }
        for (name, value) in &environment {
            validate_environment_name(name)?;
            let allowed_name = name.starts_with("ARGENTUM_")
                || matches!(
                    name.as_str(),
                    "RUST_LOG"
                        | "NO_COLOR"
                        | "CARGO_TERM_COLOR"
                        | "GIT_TERMINAL_PROMPT"
                        | "GIT_PAGER"
                        | "GIT_CONFIG_NOSYSTEM"
                        | "LC_ALL"
                        | "LANG"
                );
            if !allowed_name || value.len() > MAX_ENVIRONMENT_VALUE_BYTES || value.contains('\0') {
                return Err(ProcessError::InvalidEnvironment);
            }
        }
        self.environment = environment;
        Ok(self)
    }

    pub fn launch_kind(&self) -> ProcessLaunchKind {
        self.launch_kind
    }

    pub fn program(&self) -> &str {
        &self.program
    }

    pub fn args(&self) -> &[String] {
        &self.args
    }

    pub fn cwd(&self) -> &Path {
        &self.cwd
    }

    pub fn timeout(&self) -> Duration {
        self.timeout
    }

    pub fn output_limit(&self) -> usize {
        self.output_limit
    }

    pub fn environment(&self) -> &BTreeMap<String, String> {
        &self.environment
    }

    pub fn environment_binding(&self) -> Sha256Digest {
        let mut hasher = Sha256::new();
        for name in inherited_environment_names() {
            if let Some(value) = std::env::var_os(name) {
                hasher.update(name.as_bytes());
                hasher.update([0]);
                hasher.update(value.to_string_lossy().as_bytes());
                hasher.update([0xff]);
            }
        }
        for (name, value) in &self.environment {
            hasher.update(name.as_bytes());
            hasher.update([0]);
            hasher.update(value.as_bytes());
            hasher.update([0xff]);
        }
        Sha256Digest(format!("{:x}", hasher.finalize()))
    }

    #[cfg(windows)]
    pub(crate) fn with_probe_boundary(
        mut self,
        boundary: std::sync::Arc<crate::probe_sandbox::ProbeSandbox>,
        environment: BTreeMap<String, String>,
    ) -> Self {
        // Only the internal fixed Rust-check builder can call this. No caller-supplied env map.
        self.environment = environment;
        self.probe_boundary = Some(boundary);
        self
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ProcessStream {
    Stdout,
    Stderr,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProcessOutputChunk {
    pub stream: ProcessStream,
    pub sequence: u64,
    pub text: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProcessState {
    Exited,
    TimedOut,
    Cancelled,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProcessResult {
    pub state: ProcessState,
    pub exit_code: Option<i32>,
    pub stdout: String,
    pub stderr: String,
    pub stdout_sha256: Sha256Digest,
    pub stderr_sha256: Sha256Digest,
    pub stdout_bytes: u64,
    pub stderr_bytes: u64,
    pub stdout_truncated: bool,
    pub stderr_truncated: bool,
    pub duration_ms: u64,
}

#[derive(Debug, Default, Clone, Copy)]
pub struct ProcessRunner;

#[cfg(test)]
tokio::task_local! {
    static CAPTURE_SETTLEMENT_TIMEOUT_OVERRIDE: Duration;
}

fn capture_settlement_timeout() -> Duration {
    #[cfg(test)]
    if let Ok(timeout) = CAPTURE_SETTLEMENT_TIMEOUT_OVERRIDE.try_with(|timeout| *timeout) {
        return timeout;
    }
    CAPTURE_SETTLEMENT_TIMEOUT
}

#[cfg(test)]
async fn with_capture_settlement_timeout_for_test<F>(timeout: Duration, future: F) -> F::Output
where
    F: std::future::Future,
{
    CAPTURE_SETTLEMENT_TIMEOUT_OVERRIDE
        .scope(timeout, future)
        .await
}

#[cfg(all(test, windows))]
#[derive(Clone)]
struct OrdinaryCaptureProbe {
    active_tasks: std::sync::Arc<std::sync::atomic::AtomicUsize>,
    release: CancellationToken,
}

#[cfg(all(test, windows))]
impl OrdinaryCaptureProbe {
    fn new() -> Self {
        Self {
            active_tasks: std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0)),
            release: CancellationToken::new(),
        }
    }

    fn active_tasks(&self) -> usize {
        self.active_tasks.load(std::sync::atomic::Ordering::SeqCst)
    }

    fn release(&self) {
        self.release.cancel();
    }
}

#[cfg(all(test, windows))]
struct OrdinaryCaptureGuard(OrdinaryCaptureProbe);

#[cfg(all(test, windows))]
impl OrdinaryCaptureGuard {
    fn new(probe: OrdinaryCaptureProbe) -> Self {
        probe
            .active_tasks
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        Self(probe)
    }

    async fn wait_for_release(&self) {
        self.0.release.cancelled().await;
    }
}

#[cfg(all(test, windows))]
impl Drop for OrdinaryCaptureGuard {
    fn drop(&mut self) {
        self.0
            .active_tasks
            .fetch_sub(1, std::sync::atomic::Ordering::SeqCst);
    }
}

#[cfg(all(test, windows))]
struct OrdinaryProcessTestHooks {
    containment_failure: std::cell::RefCell<Option<crate::windows_job::TestContainmentFailure>>,
    capture_probe: Option<OrdinaryCaptureProbe>,
    capture_failure: Option<ProcessStream>,
}

#[cfg(all(test, windows))]
tokio::task_local! {
    static ORDINARY_PROCESS_TEST_HOOKS: OrdinaryProcessTestHooks;
}

#[cfg(all(test, windows))]
async fn with_ordinary_process_test_hooks<F>(
    containment_failure: Option<crate::windows_job::TestContainmentFailure>,
    capture_probe: Option<OrdinaryCaptureProbe>,
    capture_failure: Option<ProcessStream>,
    future: F,
) -> F::Output
where
    F: std::future::Future,
{
    ORDINARY_PROCESS_TEST_HOOKS
        .scope(
            OrdinaryProcessTestHooks {
                containment_failure: std::cell::RefCell::new(containment_failure),
                capture_probe,
                capture_failure,
            },
            future,
        )
        .await
}

fn attach_ordinary_process(process_id: u32) -> Result<ProcessContainment, std::io::Error> {
    #[cfg(all(test, windows))]
    if let Some(failure) = ORDINARY_PROCESS_TEST_HOOKS
        .try_with(|hooks| hooks.containment_failure.borrow_mut().take())
        .ok()
        .flatten()
    {
        return ProcessContainment::attach_with_failure(process_id, failure);
    }
    ProcessContainment::attach(process_id)
}

fn spawn_ordinary_capture_task<R>(
    reader: R,
    stream: ProcessStream,
    limit: usize,
    output: Option<mpsc::UnboundedSender<ProcessOutputChunk>>,
) -> tokio::task::JoinHandle<Result<StreamCapture, ProcessError>>
where
    R: AsyncRead + Unpin + Send + 'static,
{
    #[cfg(all(test, windows))]
    let capture_probe = ORDINARY_PROCESS_TEST_HOOKS
        .try_with(|hooks| hooks.capture_probe.clone())
        .ok()
        .flatten();
    #[cfg(all(test, windows))]
    let inject_failure = ORDINARY_PROCESS_TEST_HOOKS
        .try_with(|hooks| hooks.capture_failure == Some(stream))
        .unwrap_or(false);
    tokio::spawn(async move {
        #[cfg(all(test, windows))]
        let capture_guard = capture_probe.map(OrdinaryCaptureGuard::new);
        #[cfg(all(test, windows))]
        let result = if inject_failure {
            Err(ProcessError::Output(std::io::Error::other(
                "injected ordinary capture failure",
            )))
        } else {
            capture_stream(reader, stream, limit, output).await
        };
        #[cfg(not(all(test, windows)))]
        let result = capture_stream(reader, stream, limit, output).await;
        #[cfg(all(test, windows))]
        if result.is_ok() {
            if let Some(capture_guard) = capture_guard.as_ref() {
                capture_guard.wait_for_release().await;
            }
        }
        result
    })
}

fn close_containment_for_completion(
    containment: &mut ProcessContainment,
) -> Result<(), std::io::Error> {
    match containment.close() {
        Ok(()) => Ok(()),
        Err(primary) => {
            let termination_error = containment.terminate().err();
            let retry_close_error = containment.close().err();
            if let Some(error_value) = retry_close_error {
                return Err(error_value);
            }
            if let Some(error_value) = termination_error {
                return Err(error_value);
            }
            Err(primary)
        }
    }
}

fn terminate_and_close_containment(
    containment: &mut ProcessContainment,
) -> Result<(), std::io::Error> {
    let termination_error = containment.terminate().err();
    let close_error = containment.close().err();
    let (retry_termination_error, retry_close_error) = if close_error.is_some() {
        (containment.terminate().err(), containment.close().err())
    } else {
        (None, None)
    };
    if let Some(error_value) = termination_error {
        return Err(error_value);
    }
    if let Some(error_value) = retry_termination_error {
        return Err(error_value);
    }
    if let Some(error_value) = retry_close_error {
        return Err(error_value);
    }
    if let Some(error_value) = close_error {
        return Err(error_value);
    }
    Ok(())
}

trait ReapChild {
    fn start_kill(&mut self) -> Result<(), std::io::Error>;
    fn wait(
        &mut self,
    ) -> impl std::future::Future<Output = Result<ExitStatus, std::io::Error>> + Send;
}
impl ReapChild for tokio::process::Child {
    fn start_kill(&mut self) -> Result<(), std::io::Error> {
        tokio::process::Child::start_kill(self)
    }
    fn wait(
        &mut self,
    ) -> impl std::future::Future<Output = Result<ExitStatus, std::io::Error>> + Send {
        tokio::process::Child::wait(self)
    }
}

enum ManagedChild {
    Ordinary(tokio::process::Child),
    #[cfg(windows)]
    Probe(crate::probe_sandbox::ProbeChild),
}
impl ManagedChild {
    fn id(&self) -> Option<u32> {
        match self {
            Self::Ordinary(child) => child.id(),
            #[cfg(windows)]
            Self::Probe(child) => Some(child.id()),
        }
    }
    fn try_wait(&mut self) -> Result<Option<ExitStatus>, std::io::Error> {
        match self {
            Self::Ordinary(child) => child.try_wait(),
            #[cfg(windows)]
            Self::Probe(child) => child.try_wait(),
        }
    }
    fn take_stdout(&mut self) -> Option<Box<dyn AsyncRead + Unpin + Send>> {
        match self {
            Self::Ordinary(child) => child.stdout.take().map(|reader| Box::new(reader) as _),
            #[cfg(windows)]
            Self::Probe(child) => child.stdout.take().map(|reader| Box::new(reader) as _),
        }
    }
    fn take_stderr(&mut self) -> Option<Box<dyn AsyncRead + Unpin + Send>> {
        match self {
            Self::Ordinary(child) => child.stderr.take().map(|reader| Box::new(reader) as _),
            #[cfg(windows)]
            Self::Probe(child) => child.stderr.take().map(|reader| Box::new(reader) as _),
        }
    }
}
impl ReapChild for ManagedChild {
    fn start_kill(&mut self) -> Result<(), std::io::Error> {
        match self {
            Self::Ordinary(child) => child.start_kill(),
            #[cfg(windows)]
            Self::Probe(child) => child.start_kill(),
        }
    }
    async fn wait(&mut self) -> Result<ExitStatus, std::io::Error> {
        match self {
            Self::Ordinary(child) => child.wait().await,
            #[cfg(windows)]
            Self::Probe(child) => child.wait().await,
        }
    }
}

async fn kill_and_reap_child(
    child: &mut (impl ReapChild + Send),
) -> Result<ExitStatus, ProcessError> {
    let kill_result = child.start_kill();
    let wait_result = tokio::time::timeout(Duration::from_secs(2), child.wait())
        .await
        .map_err(|_| {
            ProcessError::Wait(std::io::Error::new(
                std::io::ErrorKind::TimedOut,
                "process did not reap within two seconds",
            ))
        })?
        .map_err(ProcessError::Wait);
    match (kill_result, wait_result) {
        (_, Ok(status)) => Ok(status),
        (Err(kill_error), Err(_)) => Err(ProcessError::Wait(kill_error)),
        (Ok(()), Err(wait_error)) => Err(wait_error),
    }
}

async fn abort_capture_tasks(
    stdout_task: tokio::task::JoinHandle<Result<StreamCapture, ProcessError>>,
    stderr_task: tokio::task::JoinHandle<Result<StreamCapture, ProcessError>>,
) {
    stdout_task.abort();
    stderr_task.abort();
    let stdout_result = stdout_task.await;
    let stderr_result = stderr_task.await;
    let _both_cancelled = stdout_result
        .err()
        .is_some_and(|error_value| error_value.is_cancelled())
        && stderr_result
            .err()
            .is_some_and(|error_value| error_value.is_cancelled());
}

fn capture_task_result(
    result: Result<Result<StreamCapture, ProcessError>, tokio::task::JoinError>,
) -> Result<StreamCapture, ProcessError> {
    result.map_err(|error_value| ProcessError::OutputTask(error_value.to_string()))?
}

async fn settle_capture_tasks(
    stdout_task: tokio::task::JoinHandle<Result<StreamCapture, ProcessError>>,
    stderr_task: tokio::task::JoinHandle<Result<StreamCapture, ProcessError>>,
) -> Result<(StreamCapture, StreamCapture), ProcessError> {
    settle_capture_tasks_with_timeout(stdout_task, stderr_task, capture_settlement_timeout()).await
}

async fn settle_capture_tasks_with_timeout(
    mut stdout_task: tokio::task::JoinHandle<Result<StreamCapture, ProcessError>>,
    mut stderr_task: tokio::task::JoinHandle<Result<StreamCapture, ProcessError>>,
    timeout: Duration,
) -> Result<(StreamCapture, StreamCapture), ProcessError> {
    let deadline = tokio::time::sleep(timeout);
    tokio::pin!(deadline);
    let mut stdout_result = None;
    let mut stderr_result = None;
    while stdout_result.is_none() || stderr_result.is_none() {
        tokio::select! {
            result = &mut stdout_task, if stdout_result.is_none() => {
                stdout_result = Some(result);
            }
            result = &mut stderr_task, if stderr_result.is_none() => {
                stderr_result = Some(result);
            }
            _ = &mut deadline => {
                if stdout_result.is_none() {
                    stdout_task.abort();
                }
                if stderr_result.is_none() {
                    stderr_task.abort();
                }
                if stdout_result.is_none() {
                    let _ = stdout_task.await;
                }
                if stderr_result.is_none() {
                    let _ = stderr_task.await;
                }
                return Err(ProcessError::OutputSettlementTimeout);
            }
        }
    }
    let (Some(stdout_result), Some(stderr_result)) = (stdout_result, stderr_result) else {
        return Err(ProcessError::OutputSettlementTimeout);
    };
    let stdout_result = capture_task_result(stdout_result);
    let stderr_result = capture_task_result(stderr_result);
    match (stdout_result, stderr_result) {
        (Ok(stdout), Ok(stderr)) => Ok((stdout, stderr)),
        (Err(primary), _) => Err(primary),
        (Ok(_), Err(error_value)) => Err(error_value),
    }
}

impl ProcessRunner {
    pub async fn run(
        &self,
        spec: ProcessSpec,
        cancellation: CancellationToken,
    ) -> Result<ProcessResult, ProcessError> {
        self.run_with_output(spec, cancellation, None).await
    }

    pub async fn run_with_output(
        &self,
        spec: ProcessSpec,
        cancellation: CancellationToken,
        output: Option<mpsc::UnboundedSender<ProcessOutputChunk>>,
    ) -> Result<ProcessResult, ProcessError> {
        #[cfg(windows)]
        if spec.probe_boundary.is_some() {
            return Err(ProcessError::ManagedShellNotAllowed);
        }
        let started = std::time::Instant::now();
        if cancellation.is_cancelled() {
            let empty_digest = Sha256Digest(format!("{:x}", Sha256::digest([])));
            return Ok(ProcessResult {
                state: ProcessState::Cancelled,
                exit_code: None,
                stdout: String::new(),
                stderr: String::new(),
                stdout_sha256: empty_digest.clone(),
                stderr_sha256: empty_digest,
                stdout_bytes: 0,
                stderr_bytes: 0,
                stdout_truncated: false,
                stderr_truncated: false,
                duration_ms: started.elapsed().as_millis().min(u64::MAX as u128) as u64,
            });
        }
        let mut command = configured_command(&spec, true);

        let mut child = command.spawn().map_err(ProcessError::Spawn)?;
        let process_id = match child.id() {
            Some(process_id) => process_id,
            None => {
                let source = std::io::Error::other("suspended child process has no process ID");
                kill_and_reap_child(&mut child).await?;
                return Err(ProcessError::Containment(source));
            }
        };
        let mut containment = match attach_ordinary_process(process_id) {
            Ok(containment) => containment,
            Err(source) => {
                kill_and_reap_child(&mut child).await?;
                return Err(ProcessError::Containment(source));
            }
        };
        let stdout = match child.stdout.take() {
            Some(stdout) => stdout,
            None => {
                let containment_result = terminate_and_close_containment(&mut containment);
                let reap_result = kill_and_reap_child(&mut child).await;
                if let Err(error_value) = containment_result {
                    return Err(ProcessError::Containment(error_value));
                }
                reap_result?;
                return Err(ProcessError::Output(std::io::Error::other(
                    "stdout pipe is unavailable",
                )));
            }
        };
        let stderr = match child.stderr.take() {
            Some(stderr) => stderr,
            None => {
                let containment_result = terminate_and_close_containment(&mut containment);
                let reap_result = kill_and_reap_child(&mut child).await;
                if let Err(error_value) = containment_result {
                    return Err(ProcessError::Containment(error_value));
                }
                reap_result?;
                return Err(ProcessError::Output(std::io::Error::other(
                    "stderr pipe is unavailable",
                )));
            }
        };
        let stdout_task = spawn_ordinary_capture_task(
            stdout,
            ProcessStream::Stdout,
            spec.output_limit,
            output.clone(),
        );
        let stderr_task =
            spawn_ordinary_capture_task(stderr, ProcessStream::Stderr, spec.output_limit, output);

        if let Err(source) = resume_process_threads(process_id) {
            let containment_result = terminate_and_close_containment(&mut containment);
            let reap_result = kill_and_reap_child(&mut child).await;
            abort_capture_tasks(stdout_task, stderr_task).await;
            if let Err(error_value) = containment_result {
                return Err(ProcessError::Containment(error_value));
            }
            reap_result?;
            return Err(ProcessError::Containment(source));
        }

        let timeout = tokio::time::sleep(spec.timeout);
        tokio::pin!(timeout);
        enum OrdinaryProcessOutcome {
            Exited(Result<ExitStatus, std::io::Error>),
            Cancelled,
            TimedOut,
        }
        let outcome = tokio::select! {
            result = child.wait() => OrdinaryProcessOutcome::Exited(result),
            _ = cancellation.cancelled() => OrdinaryProcessOutcome::Cancelled,
            _ = &mut timeout => OrdinaryProcessOutcome::TimedOut,
        };
        let (state, status) = match outcome {
            OrdinaryProcessOutcome::Exited(Ok(status)) => (ProcessState::Exited, Some(status)),
            OrdinaryProcessOutcome::Exited(Err(source)) => {
                let containment_result = terminate_and_close_containment(&mut containment);
                let reap_result = kill_and_reap_child(&mut child).await;
                abort_capture_tasks(stdout_task, stderr_task).await;
                if let Err(error_value) = containment_result {
                    return Err(ProcessError::Containment(error_value));
                }
                reap_result?;
                return Err(ProcessError::Wait(source));
            }
            OrdinaryProcessOutcome::Cancelled => {
                let containment_result = terminate_and_close_containment(&mut containment);
                let reap_result = kill_and_reap_child(&mut child).await;
                if let Err(error_value) = containment_result {
                    abort_capture_tasks(stdout_task, stderr_task).await;
                    return Err(ProcessError::Containment(error_value));
                }
                if let Err(error_value) = reap_result {
                    abort_capture_tasks(stdout_task, stderr_task).await;
                    return Err(error_value);
                }
                (ProcessState::Cancelled, None)
            }
            OrdinaryProcessOutcome::TimedOut => {
                let containment_result = terminate_and_close_containment(&mut containment);
                let reap_result = kill_and_reap_child(&mut child).await;
                if let Err(error_value) = containment_result {
                    abort_capture_tasks(stdout_task, stderr_task).await;
                    return Err(ProcessError::Containment(error_value));
                }
                if let Err(error_value) = reap_result {
                    abort_capture_tasks(stdout_task, stderr_task).await;
                    return Err(error_value);
                }
                (ProcessState::TimedOut, None)
            }
        };
        if let Err(error_value) = close_containment_for_completion(&mut containment) {
            abort_capture_tasks(stdout_task, stderr_task).await;
            return Err(ProcessError::Containment(error_value));
        }
        let (stdout, stderr) = settle_capture_tasks(stdout_task, stderr_task).await?;
        Ok(ProcessResult {
            state,
            exit_code: status.and_then(|status| status.code()),
            stdout: String::from_utf8_lossy(&stdout.captured).into_owned(),
            stderr: String::from_utf8_lossy(&stderr.captured).into_owned(),
            stdout_sha256: Sha256Digest(stdout.sha256),
            stderr_sha256: Sha256Digest(stderr.sha256),
            stdout_bytes: stdout.total_bytes,
            stderr_bytes: stderr.total_bytes,
            stdout_truncated: stdout.truncated,
            stderr_truncated: stderr.truncated,
            duration_ms: started.elapsed().as_millis().min(u64::MAX as u128) as u64,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ManagedProcessState {
    Exited,
    Terminated,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ManagedProcessLimits {
    active_process_limit: u32,
    cpu_rate_percent: u32,
    output_hash_limit: usize,
    process_memory_bytes: u64,
    job_memory_bytes: u64,
}

impl ManagedProcessLimits {
    pub fn new(
        active_process_limit: u32,
        cpu_rate_percent: u32,
        output_hash_limit: usize,
        process_memory_bytes: u64,
        job_memory_bytes: u64,
    ) -> Result<Self, ProcessError> {
        if active_process_limit == 0
            || !(1..=100).contains(&cpu_rate_percent)
            || output_hash_limit == 0
            || process_memory_bytes < MIN_MANAGED_MEMORY_BYTES
            || job_memory_bytes < MIN_MANAGED_MEMORY_BYTES
            || job_memory_bytes < process_memory_bytes
        {
            return Err(ProcessError::InvalidManagedLimits);
        }
        Ok(Self {
            active_process_limit,
            cpu_rate_percent,
            output_hash_limit,
            process_memory_bytes,
            job_memory_bytes,
        })
    }

    pub fn for_local_model(model_size_bytes: u64) -> Result<Self, ProcessError> {
        let requested = model_size_bytes
            .checked_mul(2)
            .and_then(|value| value.checked_add(2 * GIB))
            .ok_or(ProcessError::InvalidManagedLimits)?;
        let memory_bytes =
            requested.clamp(MIN_LOCAL_MODEL_MEMORY_BYTES, MAX_LOCAL_MODEL_MEMORY_BYTES);
        Self::new(4, 95, 4 * 1024 * 1024, memory_bytes, memory_bytes)
    }

    pub(crate) fn for_fixed_rust_verification() -> Result<Self, ProcessError> {
        // Cargo's installed Windows proxy chain can include cargo, cargo-clippy,
        // another cargo invocation and compiler/driver proxies concurrently.
        // Keep a finite compiler-only process bound; local sidecars stay at four.
        Self::new(
            8,
            95,
            4 * 1024 * 1024,
            MIN_LOCAL_MODEL_MEMORY_BYTES,
            MIN_LOCAL_MODEL_MEMORY_BYTES,
        )
    }

    pub fn active_process_limit(&self) -> u32 {
        self.active_process_limit
    }

    pub fn cpu_rate_percent(&self) -> u32 {
        self.cpu_rate_percent
    }

    pub fn output_hash_limit(&self) -> usize {
        self.output_hash_limit
    }

    pub fn process_memory_bytes(&self) -> u64 {
        self.process_memory_bytes
    }

    pub fn job_memory_bytes(&self) -> u64 {
        self.job_memory_bytes
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ManagedProcessExit {
    pub state: ManagedProcessState,
    pub timed_out: bool,
    pub exit_code: Option<i32>,
    pub diagnostics: Vec<String>,
    pub stdout_sha256: Sha256Digest,
    pub stderr_sha256: Sha256Digest,
    pub stdout_hashed_bytes: u64,
    pub stderr_hashed_bytes: u64,
    pub stdout_hash_complete: bool,
    pub stderr_hash_complete: bool,
    pub stdout_bytes: u64,
    pub stderr_bytes: u64,
    pub stdout_truncated: bool,
    pub stderr_truncated: bool,
    pub duration_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoopbackListener {
    pub address: String,
    pub port: u16,
    pub pid: u32,
    pub process_started_at: u64,
}

pub struct ManagedProcess {
    containment: ProcessContainment,
    child: ManagedChild,
    stdout_task: tokio::task::JoinHandle<Result<StreamCapture, ProcessError>>,
    stderr_task: tokio::task::JoinHandle<Result<StreamCapture, ProcessError>>,
    process_id: u32,
    process_started_at: u64,
    started: std::time::Instant,
    #[cfg(all(test, windows))]
    injected_lifecycle_failure: Option<TestManagedLifecycleFailure>,
}

#[path = "managed_ipc.rs"]
mod managed_ipc;
pub use managed_ipc::{ManagedIpcProcess, ManagedIpcStreams};

struct SuspendedManagedProcess {
    containment: ProcessContainment,
    child: ManagedChild,
    stdout_task: tokio::task::JoinHandle<Result<StreamCapture, ProcessError>>,
    stderr_task: tokio::task::JoinHandle<Result<StreamCapture, ProcessError>>,
    process_id: u32,
    process_started_at: u64,
    started: std::time::Instant,
    #[cfg(all(test, windows))]
    injected_lifecycle_failure: Option<TestManagedLifecycleFailure>,
}

#[cfg(all(test, windows))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TestManagedLifecycleFailure {
    Wait,
    TryWait,
    GracefulWait,
    Reap,
}

impl ManagedProcess {
    pub async fn spawn(
        spec: ProcessSpec,
        limits: ManagedProcessLimits,
    ) -> Result<Self, ProcessError> {
        Self::spawn_suspended(spec, limits, None)
            .await?
            .resume()
            .await
    }

    pub async fn spawn_with_output(
        spec: ProcessSpec,
        limits: ManagedProcessLimits,
        output: mpsc::UnboundedSender<ProcessOutputChunk>,
    ) -> Result<Self, ProcessError> {
        Self::spawn_suspended(spec, limits, Some(output))
            .await?
            .resume()
            .await
    }

    pub async fn spawn_checked<F>(
        spec: ProcessSpec,
        limits: ManagedProcessLimits,
        pre_resume_check: F,
    ) -> Result<Self, ProcessError>
    where
        F: FnOnce() -> bool + Send,
    {
        let suspended = Self::spawn_suspended(spec, limits, None).await?;
        if !pre_resume_check() {
            suspended.reject().await?;
            return Err(ProcessError::PreResumeCheckFailed);
        }
        suspended.resume().await
    }

    async fn spawn_suspended(
        spec: ProcessSpec,
        limits: ManagedProcessLimits,
        output: Option<mpsc::UnboundedSender<ProcessOutputChunk>>,
    ) -> Result<SuspendedManagedProcess, ProcessError> {
        if spec.launch_kind != ProcessLaunchKind::Argv {
            return Err(ProcessError::ManagedShellNotAllowed);
        }
        #[cfg(not(windows))]
        {
            let _ = (spec, limits, output);
            return Err(ProcessError::Containment(std::io::Error::other(
                "suspended managed processes are currently supported only on Windows",
            )));
        }
        #[cfg(windows)]
        {
            let started = std::time::Instant::now();
            let output_limit = spec.output_limit;
            let mut child = if let Some(boundary) = &spec.probe_boundary {
                ManagedChild::Probe(boundary.spawn(&spec).map_err(ProcessError::Containment)?)
            } else {
                ManagedChild::Ordinary(
                    configured_command(&spec, true)
                        .spawn()
                        .map_err(ProcessError::Spawn)?,
                )
            };
            let process_id = child.id().ok_or_else(|| {
                ProcessError::Containment(std::io::Error::other("child process has no process ID"))
            })?;
            let mut containment = match ProcessContainment::attach_managed(process_id, limits) {
                Ok((containment, _evidence)) => containment,
                Err(source) => {
                    kill_and_reap_child(&mut child).await?;
                    return Err(ProcessError::Containment(source));
                }
            };
            let process_started_at = match process_start_identity(process_id) {
                Ok(value) => value,
                Err(source) => {
                    let containment_result = terminate_and_close_containment(&mut containment);
                    let reap_result = kill_and_reap_child(&mut child).await;
                    if let Err(cleanup_error) = containment_result {
                        return Err(ProcessError::Containment(cleanup_error));
                    }
                    reap_result?;
                    return Err(ProcessError::Containment(source));
                }
            };
            let stdout = child.take_stdout().ok_or_else(|| {
                ProcessError::Output(std::io::Error::other("stdout pipe is unavailable"))
            })?;
            let stderr = child.take_stderr().ok_or_else(|| {
                ProcessError::Output(std::io::Error::other("stderr pipe is unavailable"))
            })?;
            let stdout_task = tokio::spawn(capture_managed_stream(
                stdout,
                ProcessStream::Stdout,
                output_limit,
                limits.output_hash_limit(),
                output.clone(),
            ));
            let stderr_task = tokio::spawn(capture_managed_stream(
                stderr,
                ProcessStream::Stderr,
                output_limit,
                limits.output_hash_limit(),
                output,
            ));
            Ok(SuspendedManagedProcess {
                containment,
                child,
                stdout_task,
                stderr_task,
                process_id,
                process_started_at,
                started,
                #[cfg(all(test, windows))]
                injected_lifecycle_failure: None,
            })
        }
    }

    pub fn id(&self) -> u32 {
        self.process_id
    }

    pub fn process_started_at(&self) -> u64 {
        self.process_started_at
    }

    #[cfg(all(test, windows))]
    fn inject_containment_failure(&mut self, failure: crate::windows_job::TestContainmentFailure) {
        self.containment.inject_failure(failure);
    }

    #[cfg(all(test, windows))]
    fn inject_lifecycle_failure(&mut self, failure: TestManagedLifecycleFailure) {
        self.injected_lifecycle_failure = Some(failure);
    }

    #[cfg(all(test, windows))]
    fn take_lifecycle_failure(&mut self, failure: TestManagedLifecycleFailure) -> bool {
        if self.injected_lifecycle_failure == Some(failure) {
            self.injected_lifecycle_failure = None;
            true
        } else {
            false
        }
    }

    #[cfg(all(test, windows))]
    async fn replace_capture_tasks_for_test(
        &mut self,
        stdout_task: tokio::task::JoinHandle<Result<StreamCapture, ProcessError>>,
        stderr_task: tokio::task::JoinHandle<Result<StreamCapture, ProcessError>>,
    ) {
        let previous_stdout = std::mem::replace(&mut self.stdout_task, stdout_task);
        let previous_stderr = std::mem::replace(&mut self.stderr_task, stderr_task);
        abort_capture_tasks(previous_stdout, previous_stderr).await;
    }

    pub fn try_wait(&mut self) -> Result<Option<i32>, ProcessError> {
        self.child
            .try_wait()
            .map(|status| status.and_then(|status| status.code()))
            .map_err(ProcessError::Wait)
    }

    pub fn has_exited(&mut self) -> Result<bool, ProcessError> {
        self.child
            .try_wait()
            .map(|status| status.is_some())
            .map_err(ProcessError::Wait)
    }

    pub fn loopback_listeners(&self) -> Result<Vec<LoopbackListener>, ProcessError> {
        let current_identity =
            process_start_identity(self.process_id).map_err(ProcessError::ListenerOwnership)?;
        if current_identity != self.process_started_at {
            return Err(ProcessError::ProcessIdentityChanged);
        }
        owned_loopback_listeners(self.process_id, self.process_started_at)
    }

    pub async fn wait(mut self) -> Result<ManagedProcessExit, ProcessError> {
        #[cfg(all(test, windows))]
        let injected_wait_failure = self.take_lifecycle_failure(TestManagedLifecycleFailure::Wait);
        #[cfg(all(test, windows))]
        let wait_result = if injected_wait_failure {
            Err(std::io::Error::other("injected managed wait failure"))
        } else {
            self.child.wait().await
        };
        #[cfg(not(all(test, windows)))]
        let wait_result = self.child.wait().await;
        match wait_result {
            Ok(status) => self.finish(ManagedProcessState::Exited, Some(status)).await,
            Err(source) => self.cleanup_after_error(ProcessError::Wait(source)).await,
        }
    }

    pub async fn wait_with_cancellation(
        mut self,
        cancellation: CancellationToken,
        timeout: Duration,
    ) -> Result<ManagedProcessExit, ProcessError> {
        let deadline = tokio::time::sleep(timeout);
        tokio::pin!(deadline);
        enum Outcome {
            Exited(Result<ExitStatus, std::io::Error>),
            Cancelled,
            TimedOut,
        }
        let outcome = tokio::select! {
            result = self.child.wait() => Outcome::Exited(result),
            _ = cancellation.cancelled() => Outcome::Cancelled,
            _ = &mut deadline => Outcome::TimedOut,
        };
        match outcome {
            Outcome::Exited(Ok(status)) => {
                self.finish(ManagedProcessState::Exited, Some(status)).await
            }
            Outcome::Exited(Err(source)) => {
                self.cleanup_after_error(ProcessError::Wait(source)).await
            }
            Outcome::Cancelled => self.terminate_and_wait(Duration::from_millis(250)).await,
            Outcome::TimedOut => self
                .terminate_and_wait(Duration::from_millis(250))
                .await
                .map(|mut exit| {
                    exit.timed_out = true;
                    exit
                }),
        }
    }

    pub async fn terminate_and_wait(
        mut self,
        graceful_timeout: Duration,
    ) -> Result<ManagedProcessExit, ProcessError> {
        #[cfg(all(test, windows))]
        let injected_try_wait_failure =
            self.take_lifecycle_failure(TestManagedLifecycleFailure::TryWait);
        #[cfg(all(test, windows))]
        let try_wait_result = if injected_try_wait_failure {
            Err(std::io::Error::other("injected managed try-wait failure"))
        } else {
            self.child.try_wait()
        };
        #[cfg(not(all(test, windows)))]
        let try_wait_result = self.child.try_wait();
        match try_wait_result {
            Ok(Some(status)) => {
                return self.finish(ManagedProcessState::Exited, Some(status)).await;
            }
            Ok(None) => {}
            Err(source) => {
                return self.cleanup_after_error(ProcessError::Wait(source)).await;
            }
        }

        #[cfg(all(test, windows))]
        if self.take_lifecycle_failure(TestManagedLifecycleFailure::GracefulWait) {
            return self
                .cleanup_after_error(ProcessError::Wait(std::io::Error::other(
                    "injected managed graceful-wait failure",
                )))
                .await;
        }

        let graceful_timeout = if request_process_group_stop(self.process_id).is_ok() {
            graceful_timeout.min(Duration::from_secs(5))
        } else {
            Duration::ZERO
        };
        let graceful_status = if graceful_timeout.is_zero() {
            None
        } else {
            match tokio::time::timeout(graceful_timeout, self.child.wait()).await {
                Ok(Ok(status)) => Some(status),
                Ok(Err(source)) => {
                    return self.cleanup_after_error(ProcessError::Wait(source)).await;
                }
                Err(_) => None,
            }
        };

        let containment_result = terminate_and_close_containment(&mut self.containment);
        #[cfg(all(test, windows))]
        let injected_reap_failure = self.take_lifecycle_failure(TestManagedLifecycleFailure::Reap);
        #[cfg(all(test, windows))]
        let status_result = if injected_reap_failure {
            Err(ProcessError::Wait(std::io::Error::other(
                "injected managed reap failure",
            )))
        } else if let Some(status) = graceful_status {
            Ok(status)
        } else {
            kill_and_reap_child(&mut self.child).await
        };
        #[cfg(not(all(test, windows)))]
        let status_result = if let Some(status) = graceful_status {
            Ok(status)
        } else {
            kill_and_reap_child(&mut self.child).await
        };
        if let Err(error_value) = containment_result {
            abort_capture_tasks(self.stdout_task, self.stderr_task).await;
            return Err(ProcessError::Containment(error_value));
        }
        let status = match status_result {
            Ok(status) => status,
            Err(error_value) => {
                abort_capture_tasks(self.stdout_task, self.stderr_task).await;
                return Err(error_value);
            }
        };
        self.finish(ManagedProcessState::Terminated, Some(status))
            .await
    }

    async fn cleanup_after_error<T>(mut self, primary: ProcessError) -> Result<T, ProcessError> {
        let containment_result = terminate_and_close_containment(&mut self.containment);
        let reap_result = kill_and_reap_child(&mut self.child).await;
        abort_capture_tasks(self.stdout_task, self.stderr_task).await;
        if let Err(error_value) = containment_result {
            return Err(ProcessError::Containment(error_value));
        }
        reap_result?;
        Err(primary)
    }

    async fn finish(
        mut self,
        state: ManagedProcessState,
        status: Option<ExitStatus>,
    ) -> Result<ManagedProcessExit, ProcessError> {
        if let Err(error_value) = close_containment_for_completion(&mut self.containment) {
            abort_capture_tasks(self.stdout_task, self.stderr_task).await;
            return Err(ProcessError::Containment(error_value));
        }
        let (stdout, stderr) = settle_capture_tasks(self.stdout_task, self.stderr_task).await?;
        Ok(ManagedProcessExit {
            state,
            timed_out: false,
            exit_code: status.and_then(|status| status.code()),
            diagnostics: normalized_runtime_diagnostics(&stdout.captured, &stderr.captured),
            stdout_sha256: Sha256Digest(stdout.sha256),
            stderr_sha256: Sha256Digest(stderr.sha256),
            stdout_hashed_bytes: stdout.hashed_bytes,
            stderr_hashed_bytes: stderr.hashed_bytes,
            stdout_hash_complete: stdout.hash_complete,
            stderr_hash_complete: stderr.hash_complete,
            stdout_bytes: stdout.total_bytes,
            stderr_bytes: stderr.total_bytes,
            stdout_truncated: stdout.truncated,
            stderr_truncated: stderr.truncated,
            duration_ms: self.started.elapsed().as_millis().min(u64::MAX as u128) as u64,
        })
    }
}

impl SuspendedManagedProcess {
    #[cfg(all(test, windows))]
    fn managed_job_evidence(&self) -> Result<crate::windows_job::ManagedJobEvidence, ProcessError> {
        self.containment
            .managed_evidence()
            .map_err(ProcessError::Containment)
    }

    #[cfg(all(test, windows))]
    fn inject_lifecycle_failure(&mut self, failure: TestManagedLifecycleFailure) {
        self.injected_lifecycle_failure = Some(failure);
    }

    #[cfg(all(test, windows))]
    fn take_lifecycle_failure(&mut self, failure: TestManagedLifecycleFailure) -> bool {
        if self.injected_lifecycle_failure == Some(failure) {
            self.injected_lifecycle_failure = None;
            true
        } else {
            false
        }
    }

    #[cfg(all(test, windows))]
    async fn replace_capture_tasks_for_test(
        &mut self,
        stdout_task: tokio::task::JoinHandle<Result<StreamCapture, ProcessError>>,
        stderr_task: tokio::task::JoinHandle<Result<StreamCapture, ProcessError>>,
    ) {
        let previous_stdout = std::mem::replace(&mut self.stdout_task, stdout_task);
        let previous_stderr = std::mem::replace(&mut self.stderr_task, stderr_task);
        abort_capture_tasks(previous_stdout, previous_stderr).await;
    }

    async fn reject(mut self) -> Result<(), ProcessError> {
        let containment_result = terminate_and_close_containment(&mut self.containment);
        #[cfg(all(test, windows))]
        let reap_result = if self.take_lifecycle_failure(TestManagedLifecycleFailure::Reap) {
            Err(ProcessError::Wait(std::io::Error::other(
                "injected suspended managed reap failure",
            )))
        } else {
            kill_and_reap_child(&mut self.child).await
        };
        #[cfg(not(all(test, windows)))]
        let reap_result = kill_and_reap_child(&mut self.child).await;
        if let Err(error_value) = containment_result {
            abort_capture_tasks(self.stdout_task, self.stderr_task).await;
            return Err(ProcessError::Containment(error_value));
        }
        if let Err(error_value) = reap_result {
            abort_capture_tasks(self.stdout_task, self.stderr_task).await;
            return Err(error_value);
        }
        settle_capture_tasks(self.stdout_task, self.stderr_task).await?;
        Ok(())
    }

    async fn resume(mut self) -> Result<ManagedProcess, ProcessError> {
        if let Err(source) = resume_process_threads(self.process_id) {
            let containment_result = terminate_and_close_containment(&mut self.containment);
            let reap_result = kill_and_reap_child(&mut self.child).await;
            abort_capture_tasks(self.stdout_task, self.stderr_task).await;
            if let Err(error_value) = containment_result {
                return Err(ProcessError::Containment(error_value));
            }
            reap_result?;
            return Err(ProcessError::Resume(source));
        }
        Ok(ManagedProcess {
            containment: self.containment,
            child: self.child,
            stdout_task: self.stdout_task,
            stderr_task: self.stderr_task,
            process_id: self.process_id,
            process_started_at: self.process_started_at,
            started: self.started,
            #[cfg(all(test, windows))]
            injected_lifecycle_failure: self.injected_lifecycle_failure,
        })
    }
}

fn configured_command(spec: &ProcessSpec, managed: bool) -> Command {
    let mut command = Command::new(&spec.program);
    command
        .args(&spec.args)
        .current_dir(&spec.cwd)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .env_clear();
    for name in inherited_environment_names() {
        if let Some(value) = std::env::var_os(name) {
            command.env(name, value);
        }
    }
    for (name, value) in &spec.environment {
        command.env(name, value);
    }
    #[cfg(windows)]
    if managed {
        use windows_sys::Win32::System::Threading::{
            CREATE_NEW_PROCESS_GROUP, CREATE_NO_WINDOW, CREATE_SUSPENDED,
        };

        // These are owned background children with redirected standard I/O.
        // Do not create a console window or depend on an interactive console
        // being available on a Windows service/CI host.
        command.creation_flags(CREATE_NEW_PROCESS_GROUP | CREATE_SUSPENDED | CREATE_NO_WINDOW);
    }
    #[cfg(not(windows))]
    let _ = managed;
    command
}

fn normalized_runtime_diagnostics(stdout: &[u8], stderr: &[u8]) -> Vec<String> {
    let mut diagnostics = std::collections::BTreeSet::new();
    for captured in [stdout, stderr] {
        let lower = String::from_utf8_lossy(captured).to_ascii_lowercase();
        if lower.contains("server is listening") || lower.contains("listening at") {
            diagnostics.insert("runtime-listening".to_owned());
        }
        if lower.contains("failed to load model") || lower.contains("error loading model") {
            diagnostics.insert("runtime-model-load-failed".to_owned());
        }
        if lower.contains("address already in use") {
            diagnostics.insert("runtime-address-in-use".to_owned());
        }
        if lower.contains("out of memory") {
            diagnostics.insert("runtime-out-of-memory".to_owned());
        }
        if lower.contains("vulkan") && (lower.contains("not found") || lower.contains("failed")) {
            diagnostics.insert("runtime-vulkan-unavailable".to_owned());
        }
    }
    diagnostics.into_iter().collect()
}

#[cfg(windows)]
fn close_windows_handle(
    handle: windows_sys::Win32::Foundation::HANDLE,
) -> Result<(), std::io::Error> {
    use windows_sys::Win32::Foundation::CloseHandle;

    // SAFETY: callers pass an owned live Windows handle and stop using it after this call.
    let result = unsafe { CloseHandle(handle) };
    if result == 0 {
        return Err(std::io::Error::last_os_error());
    }
    Ok(())
}

#[cfg(windows)]
fn resume_process_threads(process_id: u32) -> Result<(), std::io::Error> {
    use windows_sys::Win32::Foundation::INVALID_HANDLE_VALUE;
    use windows_sys::Win32::System::Diagnostics::ToolHelp::{
        CreateToolhelp32Snapshot, Thread32First, Thread32Next, TH32CS_SNAPTHREAD, THREADENTRY32,
    };
    use windows_sys::Win32::System::Threading::{OpenThread, ResumeThread, THREAD_SUSPEND_RESUME};

    // SAFETY: the snapshot call receives a documented flag and no borrowed pointers.
    let snapshot = unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPTHREAD, 0) };
    if snapshot == INVALID_HANDLE_VALUE {
        return Err(std::io::Error::last_os_error());
    }
    let mut entry = THREADENTRY32 {
        dwSize: std::mem::size_of::<THREADENTRY32>() as u32,
        ..Default::default()
    };
    let mut found = false;
    // SAFETY: `entry` is initialized with the required size and remains writable for the call.
    let mut next = unsafe { Thread32First(snapshot, &mut entry) } != 0;
    while next {
        if entry.th32OwnerProcessID == process_id {
            // SAFETY: the thread ID came from the live snapshot and access is limited to resume.
            let thread = unsafe { OpenThread(THREAD_SUSPEND_RESUME, 0, entry.th32ThreadID) };
            if thread.is_null() {
                let primary = std::io::Error::last_os_error();
                return match close_windows_handle(snapshot) {
                    Ok(()) => Err(primary),
                    Err(cleanup) => Err(cleanup),
                };
            }
            // SAFETY: `thread` is a live handle opened with THREAD_SUSPEND_RESUME.
            let previous = unsafe { ResumeThread(thread) };
            let resume_error = (previous == u32::MAX).then(std::io::Error::last_os_error);
            let thread_close_error = close_windows_handle(thread).err();
            if resume_error.is_some() || thread_close_error.is_some() {
                let snapshot_close_error = close_windows_handle(snapshot).err();
                if let Some(error_value) = snapshot_close_error {
                    return Err(error_value);
                }
                if let Some(error_value) = thread_close_error {
                    return Err(error_value);
                }
                return Err(resume_error.expect("resume error was checked above"));
            }
            found = true;
        }
        // SAFETY: `snapshot` remains open and `entry` remains a correctly sized writable record.
        next = unsafe { Thread32Next(snapshot, &mut entry) } != 0;
    }
    close_windows_handle(snapshot)?;
    if found {
        Ok(())
    } else {
        Err(std::io::Error::other(
            "suspended process primary thread was not found",
        ))
    }
}

#[cfg(not(windows))]
fn resume_process_threads(_process_id: u32) -> Result<(), std::io::Error> {
    Err(std::io::Error::other(
        "suspended managed processes are currently supported only on Windows",
    ))
}

#[cfg(windows)]
fn process_start_identity(process_id: u32) -> Result<u64, std::io::Error> {
    use windows_sys::Win32::Foundation::FILETIME;
    use windows_sys::Win32::System::Threading::{
        GetProcessTimes, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION,
    };

    // SAFETY: requested access is read-only process metadata for the supplied process ID.
    let process = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, process_id) };
    if process.is_null() {
        return Err(std::io::Error::last_os_error());
    }
    let mut creation = FILETIME::default();
    let mut exit = FILETIME::default();
    let mut kernel = FILETIME::default();
    let mut user = FILETIME::default();
    // SAFETY: all FILETIME outputs are initialized writable records and `process` is live.
    let result =
        unsafe { GetProcessTimes(process, &mut creation, &mut exit, &mut kernel, &mut user) };
    let query_error = (result == 0).then(std::io::Error::last_os_error);
    let close_error = close_windows_handle(process).err();
    if let Some(error_value) = close_error {
        return Err(error_value);
    }
    if let Some(error_value) = query_error {
        return Err(error_value);
    }
    Ok((u64::from(creation.dwHighDateTime) << 32) | u64::from(creation.dwLowDateTime))
}

#[cfg(not(windows))]
fn process_start_identity(_process_id: u32) -> Result<u64, std::io::Error> {
    Err(std::io::Error::other(
        "process start identity is currently supported only on Windows",
    ))
}

#[cfg(windows)]
fn request_process_group_stop(process_id: u32) -> Result<(), std::io::Error> {
    use windows_sys::Win32::System::Console::{GenerateConsoleCtrlEvent, CTRL_BREAK_EVENT};

    // SAFETY: the process was created with CREATE_NEW_PROCESS_GROUP and the ID is its group ID.
    let result = unsafe { GenerateConsoleCtrlEvent(CTRL_BREAK_EVENT, process_id) };
    if result == 0 {
        return Err(std::io::Error::last_os_error());
    }
    Ok(())
}

#[cfg(not(windows))]
fn request_process_group_stop(_process_id: u32) -> Result<(), std::io::Error> {
    Ok(())
}

#[cfg(windows)]
fn owned_loopback_listeners(
    process_id: u32,
    process_started_at: u64,
) -> Result<Vec<LoopbackListener>, ProcessError> {
    use windows_sys::Win32::NetworkManagement::IpHelper::{
        MIB_TCP6ROW_OWNER_PID, MIB_TCPROW_OWNER_PID, MIB_TCP_STATE_LISTEN,
    };
    use windows_sys::Win32::Networking::WinSock::{AF_INET, AF_INET6};

    let ipv4 = tcp_owner_table(AF_INET.into()).map_err(ProcessError::ListenerOwnership)?;
    let row_size = std::mem::size_of::<MIB_TCPROW_OWNER_PID>();
    let count = table_row_count(&ipv4, row_size).map_err(ProcessError::ListenerOwnership)?;
    let mut listeners = Vec::new();
    for index in 0..count {
        let offset = std::mem::size_of::<u32>() + index * row_size;
        let row = unsafe {
            std::ptr::read_unaligned(ipv4.as_ptr().add(offset).cast::<MIB_TCPROW_OWNER_PID>())
        };
        if row.dwOwningPid != process_id || row.dwState != MIB_TCP_STATE_LISTEN as u32 {
            continue;
        }
        let address = std::net::Ipv4Addr::from(row.dwLocalAddr.to_ne_bytes());
        if address != std::net::Ipv4Addr::LOCALHOST {
            return Err(ProcessError::UnsafeListenerAddress);
        }
        listeners.push(LoopbackListener {
            address: address.to_string(),
            port: u16::from_be(row.dwLocalPort as u16),
            pid: process_id,
            process_started_at,
        });
    }

    let ipv6 = tcp_owner_table(AF_INET6.into()).map_err(ProcessError::ListenerOwnership)?;
    let row_size = std::mem::size_of::<MIB_TCP6ROW_OWNER_PID>();
    let count = table_row_count(&ipv6, row_size).map_err(ProcessError::ListenerOwnership)?;
    for index in 0..count {
        let offset = std::mem::size_of::<u32>() + index * row_size;
        let row = unsafe {
            std::ptr::read_unaligned(ipv6.as_ptr().add(offset).cast::<MIB_TCP6ROW_OWNER_PID>())
        };
        if row.dwOwningPid == process_id && row.dwState == MIB_TCP_STATE_LISTEN as u32 {
            return Err(ProcessError::UnsafeListenerAddress);
        }
    }
    listeners.sort_by_key(|entry| entry.port);
    Ok(listeners)
}

#[cfg(windows)]
fn tcp_owner_table(address_family: u32) -> Result<Vec<u8>, std::io::Error> {
    use windows_sys::Win32::Foundation::ERROR_INSUFFICIENT_BUFFER;
    use windows_sys::Win32::NetworkManagement::IpHelper::{
        GetExtendedTcpTable, TCP_TABLE_OWNER_PID_LISTENER,
    };

    const MAX_TABLE_BYTES: u32 = 16 * 1024 * 1024;
    unsafe {
        let mut size = 0_u32;
        let first = GetExtendedTcpTable(
            std::ptr::null_mut(),
            &mut size,
            0,
            address_family,
            TCP_TABLE_OWNER_PID_LISTENER,
            0,
        );
        if first != ERROR_INSUFFICIENT_BUFFER && first != 0 {
            return Err(std::io::Error::from_raw_os_error(first as i32));
        }
        if size < std::mem::size_of::<u32>() as u32 || size > MAX_TABLE_BYTES {
            return Err(std::io::Error::other("TCP owner table has an invalid size"));
        }
        let mut table = vec![0_u8; size as usize];
        let result = GetExtendedTcpTable(
            table.as_mut_ptr().cast(),
            &mut size,
            0,
            address_family,
            TCP_TABLE_OWNER_PID_LISTENER,
            0,
        );
        if result != 0 {
            return Err(std::io::Error::from_raw_os_error(result as i32));
        }
        table.truncate(size as usize);
        Ok(table)
    }
}

#[cfg(windows)]
fn table_row_count(table: &[u8], row_size: usize) -> Result<usize, std::io::Error> {
    if table.len() < std::mem::size_of::<u32>() || row_size == 0 {
        return Err(std::io::Error::other("TCP owner table is truncated"));
    }
    let count = unsafe { std::ptr::read_unaligned(table.as_ptr().cast::<u32>()) as usize };
    let available = (table.len() - std::mem::size_of::<u32>()) / row_size;
    if count > available {
        return Err(std::io::Error::other(
            "TCP owner table row count is invalid",
        ));
    }
    Ok(count)
}

#[cfg(not(windows))]
fn owned_loopback_listeners(
    _process_id: u32,
    _process_started_at: u64,
) -> Result<Vec<LoopbackListener>, ProcessError> {
    Err(ProcessError::ListenerOwnership(std::io::Error::other(
        "listener ownership is currently supported only on Windows",
    )))
}

struct StreamCapture {
    captured: Vec<u8>,
    sha256: String,
    total_bytes: u64,
    truncated: bool,
    hashed_bytes: u64,
    hash_complete: bool,
}

/// Incremental UTF-8 decoder for streamed output chunks.
///
/// A pipe read can split a valid UTF-8 character. Decoding each read in
/// isolation would corrupt such characters into replacement characters, so the
/// decoder buffers at most the three bytes that can still complete a multibyte
/// sequence and carries them into the next read. Bytes that cannot begin a
/// valid sequence, or that continue an invalid one, are replaced using the same
/// rules as [`String::from_utf8_lossy`], which keeps truly invalid input
/// identical to the retained-prefix decode. The pending suffix is finalized
/// once, so a truncated tail yields exactly the replacement characters a single
/// lossy decode of the same retained bytes would produce.
struct StreamUtf8Decoder {
    pending: [u8; 3],
    pending_len: usize,
}

impl StreamUtf8Decoder {
    fn new() -> Self {
        Self {
            pending: [0_u8; 3],
            pending_len: 0,
        }
    }

    /// Number of bytes currently buffered to complete a split character.
    fn pending_len(&self) -> usize {
        self.pending_len
    }

    /// Decodes `bytes`, returning text to emit now and buffering only the
    /// trailing bytes of an incomplete (but still valid) sequence.
    fn push(&mut self, bytes: &[u8]) -> String {
        // Prepend any bytes buffered from an earlier read so a character split
        // across reads is decoded exactly once, then decode the combined span
        // with the same rules as `String::from_utf8_lossy`. Only a trailing
        // incomplete (but still valid) sequence is buffered, so the carry stays
        // bounded to three bytes.
        let mut combined = Vec::with_capacity(self.pending_len + bytes.len());
        combined.extend_from_slice(&self.pending[..self.pending_len]);
        combined.extend_from_slice(bytes);
        self.pending_len = 0;

        let mut text = String::with_capacity(combined.len());
        let mut rest = &combined[..];
        loop {
            match std::str::from_utf8(rest) {
                Ok(valid) => {
                    text.push_str(valid);
                    return text;
                }
                Err(error) => {
                    let valid = error.valid_up_to();
                    // `valid_up_to()` bounds a prefix that is valid UTF-8 by
                    // construction, so decoding it cannot fail.
                    let prefix = std::str::from_utf8(&rest[..valid])
                        .expect("valid_up_to prefix is valid UTF-8");
                    text.push_str(prefix);
                    match error.error_len() {
                        Some(len) => {
                            // Truly invalid bytes: one replacement, then keep
                            // scanning so a following valid ASCII byte or a
                            // fresh lead byte is not swallowed with the bad run.
                            text.push('\u{FFFD}');
                            rest = &rest[valid + len..];
                        }
                        None => {
                            // Valid but incomplete tail: hold it for the next
                            // read (at most three bytes).
                            let tail = &rest[valid..];
                            self.pending_len = tail.len();
                            self.pending[..self.pending_len].copy_from_slice(tail);
                            return text;
                        }
                    }
                }
            }
        }
    }

    /// Emits any buffered tail exactly once at end of stream. A partial
    /// character at EOF is a single lossy replacement, matching what a standard
    /// lossy decode of the same retained bytes produces.
    fn finish(self) -> String {
        if self.pending_len > 0 {
            String::from('\u{FFFD}')
        } else {
            String::new()
        }
    }
}

async fn capture_stream<R>(
    mut reader: R,
    stream: ProcessStream,
    limit: usize,
    output: Option<mpsc::UnboundedSender<ProcessOutputChunk>>,
) -> Result<StreamCapture, ProcessError>
where
    R: AsyncRead + Unpin,
{
    let mut captured = Vec::with_capacity(limit.min(64 * 1024));
    let mut hasher = Sha256::new();
    let mut total_bytes = 0_u64;
    let mut sequence = 0_u64;
    let mut decoder = StreamUtf8Decoder::new();
    let mut buffer = [0_u8; 8192];
    loop {
        let read = reader
            .read(&mut buffer)
            .await
            .map_err(ProcessError::Output)?;
        if read == 0 {
            break;
        }
        total_bytes = total_bytes.saturating_add(read.try_into().unwrap_or(u64::MAX));
        hasher.update(&buffer[..read]);
        let remaining = limit.saturating_sub(captured.len());
        let retained = remaining.min(read);
        if retained > 0 {
            captured.extend_from_slice(&buffer[..retained]);
            if let Some(output) = &output {
                // Decode only retained bytes so streamed text matches the
                // retained raw prefix, and carry a split character across reads.
                let text = decoder.push(&buffer[..retained]);
                if !text.is_empty() {
                    let _ = output.send(ProcessOutputChunk {
                        stream,
                        sequence,
                        text,
                    });
                    sequence = sequence.saturating_add(1);
                }
            }
        }
    }
    if let Some(output) = &output {
        let tail = decoder.finish();
        if !tail.is_empty() {
            let _ = output.send(ProcessOutputChunk {
                stream,
                sequence,
                text: tail,
            });
        }
    }
    Ok(StreamCapture {
        truncated: total_bytes > captured.len().try_into().unwrap_or(u64::MAX),
        captured,
        sha256: format!("{:x}", hasher.finalize()),
        total_bytes,
        hashed_bytes: total_bytes,
        hash_complete: true,
    })
}

async fn capture_managed_stream<R>(
    mut reader: R,
    stream: ProcessStream,
    retained_limit: usize,
    hash_limit: usize,
    output: Option<mpsc::UnboundedSender<ProcessOutputChunk>>,
) -> Result<StreamCapture, ProcessError>
where
    R: AsyncRead + Unpin,
{
    let mut captured = Vec::with_capacity(retained_limit.min(64 * 1024));
    let mut hasher = Sha256::new();
    let mut total_bytes = 0_u64;
    let mut hashed_bytes = 0_u64;
    let mut decoder = StreamUtf8Decoder::new();
    let mut retained_bytes = 0_u64;
    let mut buffer = [0_u8; 8192];
    loop {
        let read = reader
            .read(&mut buffer)
            .await
            .map_err(ProcessError::Output)?;
        if read == 0 {
            break;
        }
        total_bytes = total_bytes.saturating_add(read.try_into().unwrap_or(u64::MAX));
        let remaining_hash =
            hash_limit.saturating_sub(hashed_bytes.try_into().unwrap_or(usize::MAX));
        let hashed = remaining_hash.min(read);
        if hashed > 0 {
            hasher.update(&buffer[..hashed]);
            hashed_bytes = hashed_bytes.saturating_add(hashed.try_into().unwrap_or(u64::MAX));
        }
        let remaining_retained = retained_limit.saturating_sub(captured.len());
        let retained = remaining_retained.min(read);
        if retained > 0 {
            captured.extend_from_slice(&buffer[..retained]);
            if let Some(output) = &output {
                // The sequence is the raw byte offset of this retained span, so
                // it stays a byte count; only the decoded text is incremental.
                // Decode retained bytes only, carrying a split character across
                // reads into the chunk that completes it.
                let sequence = retained_bytes;
                retained_bytes = retained_bytes.saturating_add(retained as u64);
                let text = decoder.push(&buffer[..retained]);
                if !text.is_empty() {
                    let _ = output.send(ProcessOutputChunk {
                        stream,
                        sequence,
                        text,
                    });
                }
            } else {
                retained_bytes = retained_bytes.saturating_add(retained as u64);
            }
        }
    }
    if let Some(output) = &output {
        let pending = decoder.pending_len();
        let tail = decoder.finish();
        if !tail.is_empty() {
            let _ = output.send(ProcessOutputChunk {
                stream,
                sequence: retained_bytes.saturating_sub(pending as u64),
                text: tail,
            });
        }
    }
    Ok(StreamCapture {
        truncated: total_bytes > captured.len().try_into().unwrap_or(u64::MAX),
        captured,
        sha256: format!("{:x}", hasher.finalize()),
        total_bytes,
        hashed_bytes,
        hash_complete: total_bytes == hashed_bytes,
    })
}

fn inherited_environment_names() -> &'static [&'static str] {
    &[
        "PATH",
        "SystemDrive",
        "PATHEXT",
        "SYSTEMROOT",
        "WINDIR",
        "COMSPEC",
        "TEMP",
        "TMP",
        "USERPROFILE",
        "LOCALAPPDATA",
        "APPDATA",
        "PROGRAMFILES",
        "PROGRAMFILES(X86)",
        "CARGO_HOME",
        "RUSTUP_HOME",
        "ARGENTUM_F13_GATE",
        "LANG",
        "LC_ALL",
    ]
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::fs;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;
    #[cfg(windows)]
    use std::sync::OnceLock;
    use std::time::Duration;

    use tokio_util::sync::CancellationToken;

    #[test]
    fn managed_process_limits_validate_and_derive_local_model_bounds() {
        let limits =
            ManagedProcessLimits::for_local_model(833_592_096).expect("Qwen sidecar limits");
        assert_eq!(limits.active_process_limit(), 4);
        assert_eq!(limits.cpu_rate_percent(), 95);
        assert_eq!(limits.output_hash_limit(), 4 * 1024 * 1024);
        assert!(limits.process_memory_bytes() >= 4 * 1024 * 1024 * 1024);
        assert_eq!(limits.job_memory_bytes(), limits.process_memory_bytes());

        assert!(matches!(
            ManagedProcessLimits::new(0, 95, 1024, 4 * 1024 * 1024 * 1024, 4 * 1024 * 1024 * 1024),
            Err(ProcessError::InvalidManagedLimits)
        ));
        assert!(matches!(
            ManagedProcessLimits::new(4, 0, 1024, 4 * 1024 * 1024 * 1024, 4 * 1024 * 1024 * 1024),
            Err(ProcessError::InvalidManagedLimits)
        ));
        assert!(matches!(
            ManagedProcessLimits::new(4, 101, 1024, 4 * 1024 * 1024 * 1024, 4 * 1024 * 1024 * 1024),
            Err(ProcessError::InvalidManagedLimits)
        ));
        assert!(matches!(
            ManagedProcessLimits::new(4, 95, 0, 4 * 1024 * 1024 * 1024, 4 * 1024 * 1024 * 1024),
            Err(ProcessError::InvalidManagedLimits)
        ));
        assert!(matches!(
            ManagedProcessLimits::new(4, 95, 1024, 256 * 1024 * 1024 - 1, 4 * 1024 * 1024 * 1024),
            Err(ProcessError::InvalidManagedLimits)
        ));
        assert!(matches!(
            ManagedProcessLimits::new(4, 95, 1024, 4 * 1024 * 1024 * 1024, 256 * 1024 * 1024 - 1),
            Err(ProcessError::InvalidManagedLimits)
        ));
        assert!(matches!(
            ManagedProcessLimits::new(4, 95, 1024, 5 * 1024 * 1024 * 1024, 4 * 1024 * 1024 * 1024),
            Err(ProcessError::InvalidManagedLimits)
        ));
        assert!(matches!(
            ManagedProcessLimits::for_local_model(u64::MAX),
            Err(ProcessError::InvalidManagedLimits)
        ));
    }

    #[tokio::test]
    async fn bounded_hash_managed_streams_drain_without_hashing_past_the_limit() {
        let bytes = vec![b'x'; 8 * 1024 * 1024];
        let (mut writer, reader) = tokio::io::duplex(128 * 1024);
        let writer_task = tokio::spawn(async move {
            tokio::io::AsyncWriteExt::write_all(&mut writer, &bytes)
                .await
                .expect("fixture write");
        });

        let capture = capture_managed_stream(reader, ProcessStream::Stdout, 64, 1024 * 1024, None)
            .await
            .expect("managed capture");
        writer_task.await.expect("fixture task");

        let mut expected = Sha256::new();
        expected.update(vec![b'x'; 1024 * 1024]);
        assert_eq!(capture.total_bytes, 8 * 1024 * 1024);
        assert_eq!(capture.captured.len(), 64);
        assert_eq!(capture.hashed_bytes, 1024 * 1024);
        assert!(!capture.hash_complete);
        assert_eq!(capture.sha256, format!("{:x}", expected.finalize()));
    }

    #[tokio::test]
    async fn bounded_hash_short_managed_stream_is_complete() {
        let (mut writer, reader) = tokio::io::duplex(128);
        let writer_task = tokio::spawn(async move {
            tokio::io::AsyncWriteExt::write_all(&mut writer, b"managed")
                .await
                .expect("fixture write");
        });

        let capture = capture_managed_stream(reader, ProcessStream::Stdout, 64, 1024, None)
            .await
            .expect("managed capture");
        writer_task.await.expect("fixture task");

        let mut expected = Sha256::new();
        expected.update(b"managed");
        assert_eq!(capture.hashed_bytes, 7);
        assert!(capture.hash_complete);
        assert_eq!(capture.sha256, format!("{:x}", expected.finalize()));
    }

    use super::*;

    #[derive(Clone)]
    struct CaptureSettlementProbe {
        active_tasks: Arc<AtomicUsize>,
        dropped_tasks: Arc<AtomicUsize>,
    }

    impl CaptureSettlementProbe {
        fn new() -> Self {
            Self {
                active_tasks: Arc::new(AtomicUsize::new(0)),
                dropped_tasks: Arc::new(AtomicUsize::new(0)),
            }
        }

        fn active_tasks(&self) -> usize {
            self.active_tasks.load(Ordering::SeqCst)
        }

        fn dropped_tasks(&self) -> usize {
            self.dropped_tasks.load(Ordering::SeqCst)
        }
    }

    struct CaptureSettlementGuard(CaptureSettlementProbe);

    impl CaptureSettlementGuard {
        fn new(probe: CaptureSettlementProbe) -> Self {
            probe.active_tasks.fetch_add(1, Ordering::SeqCst);
            Self(probe)
        }
    }

    impl Drop for CaptureSettlementGuard {
        fn drop(&mut self) {
            self.0.active_tasks.fetch_sub(1, Ordering::SeqCst);
            self.0.dropped_tasks.fetch_add(1, Ordering::SeqCst);
        }
    }

    fn pending_capture_task(
        probe: CaptureSettlementProbe,
        retained_canary: &'static str,
    ) -> tokio::task::JoinHandle<Result<StreamCapture, ProcessError>> {
        tokio::spawn(async move {
            let _guard = CaptureSettlementGuard::new(probe);
            let retained_canary = retained_canary.to_owned();
            let result = std::future::pending::<Result<StreamCapture, ProcessError>>().await;
            drop(retained_canary);
            result
        })
    }

    async fn wait_for_settlement_tasks(probe: &CaptureSettlementProbe, expected: usize) -> bool {
        let deadline = tokio::time::Instant::now() + Duration::from_secs(1);
        while probe.active_tasks() != expected && tokio::time::Instant::now() < deadline {
            tokio::task::yield_now().await;
        }
        probe.active_tasks() == expected
    }

    #[tokio::test(flavor = "current_thread")]
    async fn capture_settlement_timeout_aborts_and_joins_both_tasks() {
        let probe = CaptureSettlementProbe::new();
        let stdout_task = pending_capture_task(probe.clone(), "stdout-canary-secret");
        let stderr_task = pending_capture_task(probe.clone(), r"C:\private\stderr-canary");
        assert!(
            wait_for_settlement_tasks(&probe, 2).await,
            "capture tasks did not start"
        );

        let result =
            settle_capture_tasks_with_timeout(stdout_task, stderr_task, Duration::from_millis(25))
                .await;

        let error = match result {
            Err(error) => error,
            Ok(_) => panic!("blocked capture tasks must time out"),
        };
        assert!(matches!(&error, ProcessError::OutputSettlementTimeout));
        assert_eq!(probe.active_tasks(), 0);
        assert_eq!(probe.dropped_tasks(), 2);
        let rendered = error.to_string();
        for forbidden in ["stdout-canary-secret", "stderr-canary", r"C:\private"] {
            assert!(!rendered.contains(forbidden));
        }
        assert_eq!(
            rendered,
            "process output capture did not settle before deadline"
        );
    }

    #[tokio::test]
    async fn capture_settlement_preserves_normal_hash_semantics() {
        let (mut stdout_writer, stdout_reader) = tokio::io::duplex(128);
        let (mut stderr_writer, stderr_reader) = tokio::io::duplex(128);
        let stdout_task = tokio::spawn(capture_stream(
            stdout_reader,
            ProcessStream::Stdout,
            64,
            None,
        ));
        let stderr_task = tokio::spawn(capture_managed_stream(
            stderr_reader,
            ProcessStream::Stderr,
            64,
            4,
            None,
        ));
        tokio::io::AsyncWriteExt::write_all(&mut stdout_writer, b"ordinary")
            .await
            .expect("ordinary fixture write");
        tokio::io::AsyncWriteExt::write_all(&mut stderr_writer, b"managed")
            .await
            .expect("managed fixture write");
        drop(stdout_writer);
        drop(stderr_writer);

        let (stdout, stderr) =
            settle_capture_tasks_with_timeout(stdout_task, stderr_task, Duration::from_secs(1))
                .await
                .expect("short captures must settle");

        assert_eq!(stdout.captured, b"ordinary");
        assert_eq!(stdout.total_bytes, 8);
        assert_eq!(stdout.hashed_bytes, 8);
        assert!(stdout.hash_complete);
        assert_eq!(stdout.sha256, format!("{:x}", Sha256::digest(b"ordinary")));

        assert_eq!(stderr.captured, b"managed");
        assert_eq!(stderr.total_bytes, 7);
        assert_eq!(stderr.hashed_bytes, 4);
        assert!(!stderr.hash_complete);
        assert_eq!(stderr.sha256, format!("{:x}", Sha256::digest(b"mana")));
    }

    fn workspace() -> tempfile::TempDir {
        tempfile::tempdir().expect("workspace")
    }

    #[cfg(windows)]
    fn process_fixture_lock() -> &'static tokio::sync::Mutex<()> {
        static LOCK: OnceLock<tokio::sync::Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| tokio::sync::Mutex::new(()))
    }

    #[cfg(all(test, windows))]
    struct ParentOwnedHandle(Option<windows_sys::Win32::Foundation::HANDLE>);

    #[cfg(all(test, windows))]
    impl ParentOwnedHandle {
        fn new(handle: windows_sys::Win32::Foundation::HANDLE) -> Self {
            Self(Some(handle))
        }

        fn close_with<F>(&mut self, close: F) -> Result<(), std::io::Error>
        where
            F: FnOnce(windows_sys::Win32::Foundation::HANDLE) -> Result<(), std::io::Error>,
        {
            let Some(handle) = self.0.take() else {
                return Ok(());
            };
            match close(handle) {
                Ok(()) => Ok(()),
                Err(error_value) => {
                    self.0 = Some(handle);
                    Err(error_value)
                }
            }
        }

        fn close(&mut self) -> Result<(), std::io::Error> {
            self.close_with(|handle| {
                // SAFETY: The handle was returned by DuplicateHandle into this process and is
                // cleared from the owner before this explicit close.
                let close_result = unsafe { windows_sys::Win32::Foundation::CloseHandle(handle) };
                if close_result == 0 {
                    Err(std::io::Error::last_os_error())
                } else {
                    Ok(())
                }
            })
        }
    }

    #[cfg(all(test, windows))]
    impl Drop for ParentOwnedHandle {
        fn drop(&mut self) {
            if let Some(handle) = self.0.take() {
                // SAFETY: Any remaining handle is owned by this test wrapper and is closed once
                // during drop. An explicit close clears the option before reaching this path.
                unsafe {
                    let _ = windows_sys::Win32::Foundation::CloseHandle(handle);
                }
            }
        }
    }

    #[cfg(all(test, windows))]
    #[test]
    fn parent_owned_handle_close_with_restores_failed_ownership_and_clears_success() {
        use windows_sys::Win32::Foundation::HANDLE;

        let fake_handle = 1usize as HANDLE;
        let mut owner = ParentOwnedHandle::new(fake_handle);
        let error = std::io::Error::other("injected close failure");
        let expected_error = error.to_string();
        let failed_result = owner.close_with(|handle| {
            assert_eq!(handle, fake_handle);
            Err(error)
        });
        assert_eq!(
            failed_result
                .expect_err("injected close must fail")
                .to_string(),
            expected_error
        );
        assert_eq!(owner.0, Some(fake_handle));

        let mut api_called = false;
        owner
            .close_with(|handle| {
                assert_eq!(handle, fake_handle);
                api_called = true;
                Ok(())
            })
            .expect("injected close must succeed");
        assert!(api_called);
        assert!(owner.0.is_none());
    }

    #[test]
    fn process_spec_argv_and_shell_are_distinct_validated_launches() {
        let workspace = workspace();
        let argv = ProcessSpec::argv(
            "cargo",
            ["test".to_owned(), "--locked".to_owned()],
            workspace.path(),
        )
        .expect("argv spec");
        assert_eq!(argv.launch_kind(), ProcessLaunchKind::Argv);
        assert_eq!(argv.program(), "cargo");
        assert_eq!(argv.args(), ["test", "--locked"]);

        let shell = ProcessSpec::shell("cargo test --locked", workspace.path())
            .expect("explicit shell spec");
        assert_eq!(shell.launch_kind(), ProcessLaunchKind::Shell);
        assert!(Path::new(shell.program()).is_absolute());
        assert!(Path::new(shell.program()).is_file());
        assert!(!Path::new(shell.program()).starts_with(workspace.path()));
        assert!(shell
            .args()
            .iter()
            .any(|value| value.contains("cargo test")));
    }

    #[test]
    fn process_spec_rejects_secret_like_arguments_and_environment() {
        let workspace = workspace();
        assert!(matches!(
            ProcessSpec::argv("tool", ["--password=hidden".to_owned()], workspace.path()),
            Err(ProcessError::Security(_))
        ));

        let spec =
            ProcessSpec::argv("cargo", ["test".to_owned()], workspace.path()).expect("base spec");
        assert!(matches!(
            spec.clone()
                .with_environment(BTreeMap::from([("OPENAI_API_KEY".into(), "hidden".into())])),
            Err(ProcessError::Security(_))
        ));
        assert!(spec
            .with_environment(BTreeMap::from([(
                "ARGENTUM_TEST_VALUE".into(),
                "visible".into()
            )]))
            .is_ok());
    }

    #[test]
    fn process_spec_rejects_invalid_limits_and_working_directories() {
        let workspace = workspace();
        let missing = workspace.path().join("missing");
        assert!(matches!(
            ProcessSpec::argv("cargo", Vec::<String>::new(), &missing),
            Err(ProcessError::InvalidWorkingDirectory(_))
        ));
        let spec =
            ProcessSpec::argv("cargo", Vec::<String>::new(), workspace.path()).expect("base spec");
        assert!(spec.clone().with_timeout(Duration::ZERO).is_err());
        assert!(spec.with_output_limit(0).is_err());
    }

    #[cfg(windows)]
    fn powershell_spec(script: &str, workspace: &std::path::Path) -> ProcessSpec {
        ProcessSpec::argv(
            "powershell.exe",
            [
                "-NoLogo".to_owned(),
                "-NoProfile".to_owned(),
                "-NonInteractive".to_owned(),
                "-Command".to_owned(),
                script.to_owned(),
            ],
            workspace,
        )
        .expect("PowerShell process spec")
    }

    #[cfg(windows)]
    fn managed_limits() -> ManagedProcessLimits {
        ManagedProcessLimits::for_local_model(833_592_096).expect("managed limits")
    }

    #[cfg(windows)]
    #[tokio::test]
    async fn windows_background_shell_builtin_startup_diagnostic() {
        let _process_fixture_guard = process_fixture_lock().lock().await;
        let workspace = workspace();
        let marker = workspace.path().join("builtin-started.txt");
        let escaped = marker.to_string_lossy().replace('\'', "''");
        let script = format!("$ErrorActionPreference='Stop'; [Console]::Out.WriteLine('CONTROL_BEFORE_CMDLETS'); Get-Command Set-Content,Start-Sleep,Start-Process | ForEach-Object {{ [Console]::Out.WriteLine($_.Name + ':' + $_.Source) }}; Set-Content -LiteralPath '{escaped}' -Value started; [Console]::Out.WriteLine('CONTROL_AFTER_MARKER')");
        let result = ProcessRunner
            .run(
                powershell_spec(&script, workspace.path())
                    .with_timeout(Duration::from_secs(20))
                    .unwrap(),
                CancellationToken::new(),
            )
            .await
            .unwrap();
        assert!(
            result.exit_code == Some(0) && marker.exists(),
            "controlled shell failed: exit={:?}; stdout={}; stderr={}",
            result.exit_code,
            result.stdout,
            result.stderr
        );
    }

    #[cfg(windows)]
    #[tokio::test]
    async fn windows_managed_shell_listener_and_descendant_startup_diagnostic() {
        let _process_fixture_guard = process_fixture_lock().lock().await;
        let workspace = workspace();
        let marker = workspace.path().join("managed-started.txt");
        let escaped = marker.to_string_lossy().replace('\'', "''");
        let script = format!("$ErrorActionPreference='Stop'; [Console]::Out.WriteLine('CONTROL_MANAGED_BEFORE'); $listener=[Net.Sockets.TcpListener]::new([Net.IPAddress]::Loopback,0); $listener.Start(); [Console]::Out.WriteLine('CONTROL_LISTENER_STARTED'); $child=Start-Process -FilePath 'powershell.exe' -ArgumentList @('-NoLogo','-NoProfile','-NonInteractive','-Command','exit 0') -WindowStyle Hidden -PassThru; $child.WaitForExit(); [Console]::Out.WriteLine('CONTROL_CHILD_EXIT:'+ $child.ExitCode); Set-Content -LiteralPath '{escaped}' -Value started; $listener.Stop(); [Console]::Out.WriteLine('CONTROL_MANAGED_AFTER')");
        let (sender, mut receiver) = tokio::sync::mpsc::unbounded_channel();
        let process = ManagedProcess::spawn_with_output(
            powershell_spec(&script, workspace.path()),
            managed_limits(),
            sender,
        )
        .await
        .expect("managed diagnostic spawn");
        let exit = tokio::time::timeout(Duration::from_secs(25), process.wait())
            .await
            .expect("managed diagnostic deadline")
            .expect("managed diagnostic wait");
        let mut output = String::new();
        while let Ok(chunk) = receiver.try_recv() {
            if output.len() < 16 * 1024 {
                output.push_str(&chunk.text);
            }
        }
        assert!(
            exit.exit_code == Some(0) && marker.exists(),
            "managed shell diagnostic: exit={:?}; output={output}",
            exit.exit_code
        );
        println!("managed shell diagnostic completed: {output}");
    }

    #[cfg(windows)]
    async fn wait_for_capture_tasks(
        probe: &OrdinaryCaptureProbe,
        expected: usize,
        timeout: Duration,
    ) -> bool {
        let deadline = tokio::time::Instant::now() + timeout;
        while probe.active_tasks() != expected && tokio::time::Instant::now() < deadline {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        probe.active_tasks() == expected
    }

    #[cfg(windows)]
    fn blocked_capture_task(
        probe: OrdinaryCaptureProbe,
    ) -> tokio::task::JoinHandle<Result<StreamCapture, ProcessError>> {
        tokio::spawn(async move {
            let guard = OrdinaryCaptureGuard::new(probe);
            guard.wait_for_release().await;
            Ok(StreamCapture {
                captured: Vec::new(),
                sha256: format!("{:x}", Sha256::digest([])),
                total_bytes: 0,
                truncated: false,
                hashed_bytes: 0,
                hash_complete: true,
            })
        })
    }

    #[cfg(windows)]
    fn failed_capture_task(
        probe: OrdinaryCaptureProbe,
    ) -> tokio::task::JoinHandle<Result<StreamCapture, ProcessError>> {
        tokio::spawn(async move {
            let _guard = OrdinaryCaptureGuard::new(probe);
            Err(ProcessError::Output(std::io::Error::other(
                "injected capture task failure",
            )))
        })
    }

    #[cfg(windows)]
    #[tokio::test]
    async fn process_run_captures_streams_exit_code_and_chunks() {
        let _process_fixture_guard = process_fixture_lock().lock().await;
        let workspace = workspace();
        let spec = powershell_spec(
            "[Console]::Out.Write('out'); [Console]::Error.Write('err'); exit 7",
            workspace.path(),
        );
        let (sender, mut receiver) = tokio::sync::mpsc::unbounded_channel();
        let result = ProcessRunner
            .run_with_output(spec, CancellationToken::new(), Some(sender))
            .await
            .expect("process result");

        assert_eq!(result.state, ProcessState::Exited);
        assert_eq!(result.exit_code, Some(7));
        assert_eq!(result.stdout, "out");
        assert_eq!(result.stderr, "err");
        assert_eq!(result.stdout_sha256.0.len(), 64);
        assert_eq!(result.stderr_sha256.0.len(), 64);
        let mut streams = Vec::new();
        while let Ok(chunk) = receiver.try_recv() {
            streams.push(chunk.stream);
        }
        streams.sort();
        streams.dedup();
        assert_eq!(streams, [ProcessStream::Stdout, ProcessStream::Stderr]);
    }

    #[cfg(windows)]
    #[tokio::test]
    async fn process_run_caps_output_without_losing_total_byte_evidence() {
        let _process_fixture_guard = process_fixture_lock().lock().await;
        let workspace = workspace();
        let spec = powershell_spec("[Console]::Out.Write(('x' * 4096))", workspace.path())
            .with_output_limit(64)
            .expect("output limit");
        let result = ProcessRunner
            .run(spec, CancellationToken::new())
            .await
            .expect("process result");

        assert_eq!(result.stdout.len(), 64);
        assert_eq!(result.stdout_bytes, 4096);
        let mut expected = Sha256::new();
        expected.update(vec![b'x'; 4096]);
        assert_eq!(result.stdout_sha256.0, format!("{:x}", expected.finalize()));
        assert!(result.stdout_truncated);
    }

    #[cfg(windows)]
    #[tokio::test]
    async fn process_run_timeout_and_cancellation_are_distinct() {
        let _process_fixture_guard = process_fixture_lock().lock().await;
        let workspace = workspace();
        let timed = powershell_spec("Start-Sleep -Seconds 30", workspace.path())
            .with_timeout(Duration::from_millis(150))
            .expect("timeout");
        let timed_result = ProcessRunner
            .run(timed, CancellationToken::new())
            .await
            .expect("timeout result");
        assert_eq!(timed_result.state, ProcessState::TimedOut);

        let cancelled = powershell_spec("Start-Sleep -Seconds 30", workspace.path());
        let token = CancellationToken::new();
        let cancellation = token.clone();
        let task = tokio::spawn(async move { ProcessRunner.run(cancelled, token).await });
        tokio::time::sleep(Duration::from_millis(150)).await;
        cancellation.cancel();
        let cancelled_result = task.await.expect("task join").expect("cancel result");
        assert_eq!(cancelled_result.state, ProcessState::Cancelled);
    }

    #[cfg(windows)]
    #[test]
    fn child_duplicates_stdout_into_parent_helper() {
        let Ok(parent_pid) = std::env::var("ARGENTUM_TEST_PARENT_PID") else {
            return;
        };
        let Ok(marker_path) = std::env::var("ARGENTUM_TEST_MARKER_PATH") else {
            return;
        };
        let acknowledgment_path = std::env::var("ARGENTUM_TEST_ACKNOWLEDGMENT_PATH").ok();
        let parent_pid = parent_pid.parse::<u32>().expect("parent PID");

        use windows_sys::Win32::Foundation::{
            CloseHandle, DuplicateHandle, DUPLICATE_CLOSE_SOURCE, DUPLICATE_SAME_ACCESS, HANDLE,
            INVALID_HANDLE_VALUE,
        };
        use windows_sys::Win32::System::Console::{GetStdHandle, STD_OUTPUT_HANDLE};
        use windows_sys::Win32::System::Threading::{
            GetCurrentProcess, OpenProcess, PROCESS_DUP_HANDLE,
        };

        // SAFETY: The PID is supplied by the parent test process, and this requests only
        // PROCESS_DUP_HANDLE. The returned process handle is checked and closed below.
        let parent_process = unsafe { OpenProcess(PROCESS_DUP_HANDLE, 0, parent_pid) };
        assert!(
            !parent_process.is_null() && parent_process != INVALID_HANDLE_VALUE,
            "OpenProcess failed: {}",
            std::io::Error::last_os_error()
        );

        let operation_result = (|| -> Result<(), String> {
            // SAFETY: GetCurrentProcess returns this helper's pseudo-handle, which remains valid
            // for the duration of the helper and does not require CloseHandle.
            let current_process = unsafe { GetCurrentProcess() };
            if current_process.is_null() {
                return Err("GetCurrentProcess returned an invalid handle".to_owned());
            }

            let close_remote_handle = |duplicated_stdout: HANDLE| -> Result<(), String> {
                let mut recovered_stdout: HANDLE = std::ptr::null_mut();
                // SAFETY: parent_process owns duplicated_stdout, current_process is this helper's
                // valid pseudo-handle, and recovered_stdout points to writable local storage.
                let duplicate_result = unsafe {
                    DuplicateHandle(
                        parent_process,
                        duplicated_stdout,
                        current_process,
                        &mut recovered_stdout,
                        0,
                        0,
                        DUPLICATE_CLOSE_SOURCE | DUPLICATE_SAME_ACCESS,
                    )
                };
                if duplicate_result == 0 || recovered_stdout.is_null() {
                    return Err(format!(
                        "DuplicateHandle remote close failed: {}",
                        std::io::Error::last_os_error()
                    ));
                }

                // SAFETY: recovered_stdout was returned by DuplicateHandle into this helper.
                let close_result = unsafe { CloseHandle(recovered_stdout) };
                if close_result == 0 {
                    return Err(format!(
                        "CloseHandle recovered stdout failed: {}",
                        std::io::Error::last_os_error()
                    ));
                }
                Ok(())
            };

            // SAFETY: ProcessRunner launches this helper with stdout redirected to its capture
            // pipe. The returned standard handle is borrowed here and is not closed by the
            // helper.
            let stdout = unsafe { GetStdHandle(STD_OUTPUT_HANDLE) };
            if stdout.is_null() || stdout == INVALID_HANDLE_VALUE {
                return Err(format!(
                    "GetStdHandle failed: {}",
                    std::io::Error::last_os_error()
                ));
            }

            let mut duplicated_stdout: HANDLE = std::ptr::null_mut();
            // SAFETY: current_process and parent_process are valid process handles, stdout is the
            // helper's redirected write handle, and duplicated_stdout points to writable local
            // storage for the target-process handle value.
            let duplicate_result = unsafe {
                DuplicateHandle(
                    current_process,
                    stdout,
                    parent_process,
                    &mut duplicated_stdout,
                    0,
                    0,
                    DUPLICATE_SAME_ACCESS,
                )
            };
            if duplicate_result == 0 || duplicated_stdout.is_null() {
                return Err(format!(
                    "DuplicateHandle stdout failed: {}",
                    std::io::Error::last_os_error()
                ));
            }

            if let Err(error_value) =
                std::fs::write(&marker_path, (duplicated_stdout as usize).to_string())
            {
                close_remote_handle(duplicated_stdout).map_err(|cleanup_error| {
                    format!(
                        "marker write failed: {error_value}; remote cleanup failed: {cleanup_error}"
                    )
                })?;
                return Err(format!("marker write failed: {error_value}"));
            }

            let acknowledged = match acknowledgment_path.as_deref() {
                Some(path) => {
                    let deadline = std::time::Instant::now() + Duration::from_secs(1);
                    loop {
                        if std::fs::read(path).is_ok() {
                            break true;
                        }
                        if std::time::Instant::now() >= deadline {
                            break false;
                        }
                        std::thread::sleep(Duration::from_millis(10));
                    }
                }
                None => true,
            };
            if !acknowledged {
                close_remote_handle(duplicated_stdout)
                    .map_err(|error_value| format!("remote cleanup failed: {error_value}"))?;
            }
            Ok(())
        })();

        // SAFETY: parent_process was returned by OpenProcess above and remains open until this
        // final checked close, including while remote cleanup is in progress.
        let close_result = unsafe { CloseHandle(parent_process) };
        if close_result == 0 {
            panic!(
                "CloseHandle(parent process) failed: {}",
                std::io::Error::last_os_error()
            );
        }
        operation_result.expect("stdout escape helper operation failed");
    }

    #[cfg(windows)]
    #[tokio::test]
    async fn process_runner_returns_output_settlement_timeout_when_stdout_handle_escapes() {
        let _process_fixture_guard = process_fixture_lock().lock().await;
        let workspace = workspace();
        let marker_path = workspace.path().join("duplicated-stdout-handle.txt");
        let acknowledgment_path = workspace.path().join("acknowledged.txt");
        let current_exe = std::env::current_exe().expect("current test executable");
        let spec = ProcessSpec::argv(
            current_exe.to_string_lossy().into_owned(),
            [
                "--exact".to_owned(),
                "process::tests::child_duplicates_stdout_into_parent_helper".to_owned(),
                "--nocapture".to_owned(),
            ],
            workspace.path(),
        )
        .expect("helper process spec")
        .with_environment(BTreeMap::from([
            (
                "ARGENTUM_TEST_PARENT_PID".to_owned(),
                std::process::id().to_string(),
            ),
            (
                "ARGENTUM_TEST_MARKER_PATH".to_owned(),
                marker_path.to_string_lossy().into_owned(),
            ),
            (
                "ARGENTUM_TEST_ACKNOWLEDGMENT_PATH".to_owned(),
                acknowledgment_path.to_string_lossy().into_owned(),
            ),
        ]))
        .expect("helper test environment");
        let mut runner_task = tokio::spawn(with_capture_settlement_timeout_for_test(
            Duration::from_secs(1),
            ProcessRunner.run(spec, CancellationToken::new()),
        ));

        let marker_contents = match tokio::time::timeout(Duration::from_secs(5), async {
            loop {
                match fs::read_to_string(&marker_path) {
                    Ok(contents) if !contents.trim().is_empty() => break Ok(contents),
                    Ok(_) => {}
                    Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                    Err(error) => break Err(error),
                }
                tokio::time::sleep(Duration::from_millis(25)).await;
            }
        })
        .await
        {
            Ok(Ok(contents)) => contents,
            Ok(Err(error)) => {
                let runner_result =
                    tokio::time::timeout(Duration::from_secs(3), &mut runner_task).await;
                if runner_result.is_err() {
                    runner_task.abort();
                    let _ = runner_task.await;
                }
                panic!(
                    "read duplicated stdout handle marker: {error}; runner result: {runner_result:?}"
                );
            }
            Err(_) => {
                let runner_result =
                    tokio::time::timeout(Duration::from_secs(3), &mut runner_task).await;
                if runner_result.is_err() {
                    runner_task.abort();
                    let _ = runner_task.await;
                }
                panic!(
                    "helper did not write duplicated stdout handle marker within 5 seconds; runner result after 3-second settlement wait: {runner_result:?}"
                );
            }
        };
        let duplicated_stdout = marker_contents
            .trim()
            .parse::<usize>()
            .expect("duplicated stdout handle value");
        let mut duplicated_stdout_owner =
            ParentOwnedHandle::new(duplicated_stdout as windows_sys::Win32::Foundation::HANDLE);
        fs::write(&acknowledgment_path, "acknowledged").expect("write helper acknowledgment");
        let runner_result = tokio::time::timeout(Duration::from_secs(3), &mut runner_task)
            .await
            .expect("ProcessRunner must return within the outer test deadline")
            .expect("ProcessRunner task join");
        assert!(matches!(
            &runner_result,
            Err(ProcessError::OutputSettlementTimeout)
        ));
        assert_eq!(
            runner_result
                .as_ref()
                .expect_err("escaped stdout must fail output settlement")
                .to_string(),
            "process output capture did not settle before deadline"
        );
        duplicated_stdout_owner
            .close()
            .expect("CloseHandle(duplicated stdout) failed");
    }

    #[cfg(windows)]
    #[tokio::test]
    async fn escaped_stdout_helper_recovers_remote_handle_when_acknowledgment_is_withheld() {
        let _process_fixture_guard = process_fixture_lock().lock().await;
        let workspace = workspace();
        let marker_path = workspace.path().join("withheld-ack-handle.txt");
        let acknowledgment_path = workspace.path().join("withheld-ack.txt");
        let current_exe = std::env::current_exe().expect("current test executable");
        let spec = ProcessSpec::argv(
            current_exe.to_string_lossy().into_owned(),
            [
                "--exact".to_owned(),
                "process::tests::child_duplicates_stdout_into_parent_helper".to_owned(),
                "--nocapture".to_owned(),
            ],
            workspace.path(),
        )
        .expect("helper process spec")
        .with_environment(BTreeMap::from([
            (
                "ARGENTUM_TEST_PARENT_PID".to_owned(),
                std::process::id().to_string(),
            ),
            (
                "ARGENTUM_TEST_MARKER_PATH".to_owned(),
                marker_path.to_string_lossy().into_owned(),
            ),
            (
                "ARGENTUM_TEST_ACKNOWLEDGMENT_PATH".to_owned(),
                acknowledgment_path.to_string_lossy().into_owned(),
            ),
        ]))
        .expect("helper test environment");

        let runner_result = tokio::time::timeout(
            Duration::from_secs(4),
            with_capture_settlement_timeout_for_test(
                Duration::from_secs(1),
                ProcessRunner.run(spec, CancellationToken::new()),
            ),
        )
        .await
        .expect("helper cleanup must remain bounded");

        let result = runner_result.expect("withheld acknowledgment must recover the remote handle");
        assert_eq!(result.state, ProcessState::Exited);
        assert_eq!(result.exit_code, Some(0));
    }

    #[tokio::test]
    async fn pre_cancelled_process_returns_without_attempting_spawn() {
        let workspace = workspace();
        let spec = ProcessSpec::argv(
            "argentum-executable-that-must-not-be-spawned",
            Vec::new(),
            workspace.path(),
        )
        .expect("process spec");
        let token = CancellationToken::new();
        token.cancel();

        let result = ProcessRunner
            .run(spec, token)
            .await
            .expect("pre-cancelled result");

        assert_eq!(result.state, ProcessState::Cancelled);
        assert_eq!(result.exit_code, None);
        assert_eq!(result.stdout_bytes, 0);
        assert_eq!(result.stderr_bytes, 0);
    }

    #[cfg(windows)]
    #[tokio::test]
    async fn windows_job_object_cancellation_terminates_descendant_processes() {
        let _process_fixture_guard = process_fixture_lock().lock().await;
        let workspace = workspace();
        let child_script = workspace.path().join("child.ps1");
        let launched_marker = workspace.path().join("launched.txt");
        let descendant_marker = workspace.path().join("descendant.txt");
        let escaped_descendant = descendant_marker.to_string_lossy().replace('\'', "''");
        fs::write(
            &child_script,
            format!(
                "Start-Sleep -Milliseconds 1000\nSet-Content -LiteralPath '{escaped_descendant}' -Value child\n"
            ),
        )
        .expect("child script");
        let escaped_child = child_script.to_string_lossy().replace('\'', "''");
        let escaped_launched = launched_marker.to_string_lossy().replace('\'', "''");
        let parent = format!(
            "Start-Process -FilePath 'powershell.exe' -ArgumentList @('-NoLogo','-NoProfile','-NonInteractive','-ExecutionPolicy','Bypass','-File','{escaped_child}') -WindowStyle Hidden; Set-Content -LiteralPath '{escaped_launched}' -Value launched; Start-Sleep -Seconds 30"
        );
        let spec = powershell_spec(&parent, workspace.path());
        let token = CancellationToken::new();
        let cancellation = token.clone();
        let task = tokio::spawn(async move { ProcessRunner.run(spec, token).await });

        for _ in 0..200 {
            if launched_marker.exists() {
                break;
            }
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
        assert!(launched_marker.exists(), "parent did not launch its child");
        cancellation.cancel();
        let result = task.await.expect("task join").expect("cancel result");
        assert_eq!(result.state, ProcessState::Cancelled);
        tokio::time::sleep(Duration::from_millis(1500)).await;
        assert!(
            !descendant_marker.exists(),
            "descendant survived Job Object cancellation"
        );
    }

    #[cfg(windows)]
    #[tokio::test]
    async fn ordinary_attach_failure_never_runs_uncontained_child_or_descendant() {
        let _process_fixture_guard = process_fixture_lock().lock().await;
        let workspace = workspace();
        let child_script = workspace.path().join("ordinary-attach-child.ps1");
        let launched_marker = workspace.path().join("ordinary-attach-launched.txt");
        let descendant_marker = workspace.path().join("ordinary-attach-descendant.txt");
        let escaped_descendant = descendant_marker.to_string_lossy().replace('\'', "''");
        fs::write(
            &child_script,
            format!(
                "Start-Sleep -Milliseconds 750\nSet-Content -LiteralPath '{escaped_descendant}' -Value child\n"
            ),
        )
        .expect("child script");
        let escaped_child = child_script.to_string_lossy().replace('\'', "''");
        let escaped_launched = launched_marker.to_string_lossy().replace('\'', "''");
        let parent = format!(
            "Start-Process -FilePath 'powershell.exe' -ArgumentList @('-NoLogo','-NoProfile','-NonInteractive','-ExecutionPolicy','Bypass','-File','{escaped_child}') -NoNewWindow; Set-Content -LiteralPath '{escaped_launched}' -Value launched; Start-Sleep -Seconds 30"
        );
        let run = with_ordinary_process_test_hooks(
            Some(
                crate::windows_job::TestContainmentFailure::AssignAfterMarker(
                    launched_marker.clone(),
                ),
            ),
            None,
            None,
            ProcessRunner.run(
                powershell_spec(&parent, workspace.path()),
                CancellationToken::new(),
            ),
        );

        let result = tokio::time::timeout(Duration::from_secs(5), run)
            .await
            .expect("attach failure cleanup must be bounded");
        assert!(matches!(result, Err(ProcessError::Containment(_))));
        tokio::time::sleep(Duration::from_millis(1250)).await;
        assert!(
            !launched_marker.exists(),
            "ordinary child executed before containment"
        );
        assert!(
            !descendant_marker.exists(),
            "uncontained descendant survived ordinary attach failure"
        );
    }

    #[cfg(windows)]
    #[tokio::test]
    async fn ordinary_cancellation_reports_injected_job_termination_failure_and_settles_captures() {
        let _process_fixture_guard = process_fixture_lock().lock().await;
        let workspace = workspace();
        let child_script = workspace.path().join("ordinary-termination-child.ps1");
        let launched_marker = workspace.path().join("ordinary-termination-launched.txt");
        let descendant_marker = workspace.path().join("ordinary-termination-descendant.txt");
        let escaped_descendant = descendant_marker.to_string_lossy().replace('\'', "''");
        fs::write(
            &child_script,
            format!(
                "Start-Sleep -Milliseconds 1000\nSet-Content -LiteralPath '{escaped_descendant}' -Value child\nStart-Sleep -Seconds 30\n"
            ),
        )
        .expect("child script");
        let escaped_child = child_script.to_string_lossy().replace('\'', "''");
        let escaped_launched = launched_marker.to_string_lossy().replace('\'', "''");
        let parent = format!(
            "Start-Process -FilePath 'powershell.exe' -ArgumentList @('-NoLogo','-NoProfile','-NonInteractive','-ExecutionPolicy','Bypass','-File','{escaped_child}') -NoNewWindow; Set-Content -LiteralPath '{escaped_launched}' -Value launched; Start-Sleep -Seconds 30"
        );
        let token = CancellationToken::new();
        let cancellation = token.clone();
        let probe = OrdinaryCaptureProbe::new();
        let task = tokio::spawn(with_ordinary_process_test_hooks(
            Some(crate::windows_job::TestContainmentFailure::Terminate),
            Some(probe.clone()),
            None,
            ProcessRunner.run(powershell_spec(&parent, workspace.path()), token),
        ));

        let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
        while (!launched_marker.exists() || probe.active_tasks() != 2)
            && tokio::time::Instant::now() < deadline
        {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        assert!(launched_marker.exists(), "descendant was not started");
        assert_eq!(probe.active_tasks(), 2, "capture tasks were not running");
        cancellation.cancel();

        let result = tokio::time::timeout(Duration::from_secs(5), task)
            .await
            .expect("termination failure cleanup must be bounded")
            .expect("ordinary runner task");
        let active_at_return = probe.active_tasks();
        probe.release();
        assert!(
            wait_for_capture_tasks(&probe, 0, Duration::from_secs(1)).await,
            "test capture tasks did not release"
        );
        assert!(matches!(result, Err(ProcessError::Containment(_))));
        assert_eq!(
            active_at_return, 0,
            "ordinary termination failure returned with detached capture tasks"
        );
        tokio::time::sleep(Duration::from_millis(1250)).await;
        assert!(
            !descendant_marker.exists(),
            "descendant survived ordinary termination failure cleanup"
        );
    }

    #[cfg(windows)]
    #[tokio::test]
    async fn ordinary_natural_completion_reports_injected_job_close_failure_and_settles_captures() {
        let _process_fixture_guard = process_fixture_lock().lock().await;
        let workspace = workspace();
        let child_script = workspace.path().join("ordinary-close-child.ps1");
        let launched_marker = workspace.path().join("ordinary-close-launched.txt");
        let descendant_marker = workspace.path().join("ordinary-close-descendant.txt");
        let escaped_descendant = descendant_marker.to_string_lossy().replace('\'', "''");
        fs::write(
            &child_script,
            format!(
                "Start-Sleep -Milliseconds 1000\nSet-Content -LiteralPath '{escaped_descendant}' -Value child\nStart-Sleep -Seconds 30\n"
            ),
        )
        .expect("child script");
        let escaped_child = child_script.to_string_lossy().replace('\'', "''");
        let escaped_launched = launched_marker.to_string_lossy().replace('\'', "''");
        let parent = format!(
            "Start-Process -FilePath 'powershell.exe' -ArgumentList @('-NoLogo','-NoProfile','-NonInteractive','-ExecutionPolicy','Bypass','-File','{escaped_child}') -NoNewWindow; Set-Content -LiteralPath '{escaped_launched}' -Value launched"
        );
        let probe = OrdinaryCaptureProbe::new();
        let task = tokio::spawn(with_ordinary_process_test_hooks(
            Some(crate::windows_job::TestContainmentFailure::Close),
            Some(probe.clone()),
            None,
            ProcessRunner.run(
                powershell_spec(&parent, workspace.path()),
                CancellationToken::new(),
            ),
        ));

        let result = tokio::time::timeout(Duration::from_secs(5), task)
            .await
            .expect("close failure cleanup must be bounded")
            .expect("ordinary runner task");
        let active_at_return = probe.active_tasks();
        probe.release();
        assert!(
            wait_for_capture_tasks(&probe, 0, Duration::from_secs(1)).await,
            "test capture tasks did not release"
        );
        assert!(launched_marker.exists(), "descendant was not started");
        assert!(matches!(result, Err(ProcessError::Containment(_))));
        assert_eq!(
            active_at_return, 0,
            "ordinary close failure returned with detached capture tasks"
        );
        tokio::time::sleep(Duration::from_millis(1250)).await;
        assert!(
            !descendant_marker.exists(),
            "descendant survived ordinary close failure cleanup"
        );
    }

    #[cfg(windows)]
    #[tokio::test]
    async fn process_run_clears_environment_and_applies_safe_overrides() {
        let _process_fixture_guard = process_fixture_lock().lock().await;
        let workspace = workspace();
        let spec = powershell_spec(
            "[Console]::Out.Write($env:ARGENTUM_TEST_VALUE + '|' + $env:OPENAI_API_KEY)",
            workspace.path(),
        )
        .with_environment(BTreeMap::from([(
            "ARGENTUM_TEST_VALUE".into(),
            "visible".into(),
        )]))
        .expect("safe environment");
        let result = ProcessRunner
            .run(spec, CancellationToken::new())
            .await
            .expect("process result");
        assert_eq!(result.stdout, "visible|");
    }

    #[cfg(windows)]
    #[tokio::test]
    async fn managed_process_is_contained_before_resume() {
        let _process_fixture_guard = process_fixture_lock().lock().await;
        let workspace = workspace();
        let marker = workspace.path().join("resumed.txt");
        let escaped_marker = marker.to_string_lossy().replace('\'', "''");
        let spec = powershell_spec(
            &format!(
                "Set-Content -LiteralPath '{escaped_marker}' -Value resumed; Start-Sleep -Seconds 30"
            ),
            workspace.path(),
        );

        let suspended = ManagedProcess::spawn_suspended(spec, managed_limits(), None)
            .await
            .expect("suspended managed process");
        tokio::time::sleep(Duration::from_millis(200)).await;
        assert!(
            !marker.exists(),
            "suspended child executed before containment"
        );

        let process = suspended.resume().await.expect("resume contained child");
        for _ in 0..100 {
            if marker.exists() {
                break;
            }
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
        assert!(marker.exists(), "contained child did not resume");
        let exit = process
            .terminate_and_wait(Duration::from_millis(200))
            .await
            .expect("managed termination");
        assert_eq!(exit.state, ManagedProcessState::Terminated);
    }

    #[cfg(windows)]
    #[tokio::test]
    async fn managed_process_applies_and_queries_all_resource_limits_before_resume() {
        let _process_fixture_guard = process_fixture_lock().lock().await;
        let workspace = workspace();
        let limits = managed_limits();
        let suspended = ManagedProcess::spawn_suspended(
            powershell_spec("Start-Sleep -Seconds 30", workspace.path()),
            limits,
            None,
        )
        .await
        .expect("suspended managed process");
        let evidence = suspended
            .managed_job_evidence()
            .expect("Job Object evidence");
        assert_eq!(evidence.active_process_limit, limits.active_process_limit());
        assert_eq!(evidence.cpu_rate, limits.cpu_rate_percent() * 100);
        assert_eq!(evidence.process_memory_bytes, limits.process_memory_bytes());
        assert_eq!(evidence.job_memory_bytes, limits.job_memory_bytes());
        suspended.reject().await.expect("reject suspended process");
    }

    #[cfg(windows)]
    #[tokio::test]
    async fn checked_managed_process_fails_closed_before_the_suspended_child_resumes() {
        let _process_fixture_guard = process_fixture_lock().lock().await;
        let workspace = workspace();
        let marker = workspace.path().join("checked-resume.txt");
        let escaped_marker = marker.to_string_lossy().replace('\'', "''");
        let spec = powershell_spec(
            &format!("Set-Content -LiteralPath '{escaped_marker}' -Value resumed"),
            workspace.path(),
        );
        let checks = Arc::new(AtomicUsize::new(0));
        let observed_checks = Arc::clone(&checks);

        let result = ManagedProcess::spawn_checked(spec, managed_limits(), move || {
            observed_checks.fetch_add(1, Ordering::SeqCst);
            false
        })
        .await;

        assert!(matches!(result, Err(ProcessError::PreResumeCheckFailed)));
        assert_eq!(checks.load(Ordering::SeqCst), 1);
        assert!(!marker.exists(), "rejected suspended child was resumed");
    }

    #[cfg(windows)]
    #[tokio::test]
    async fn checked_managed_process_preserves_the_ordinary_success_path() {
        let _process_fixture_guard = process_fixture_lock().lock().await;
        let workspace = workspace();
        let spec = powershell_spec("[Console]::Out.Write('checked')", workspace.path());

        let process = ManagedProcess::spawn_checked(spec, managed_limits(), || true)
            .await
            .expect("checked managed process");
        let exit = process.wait().await.expect("checked process exit");

        assert_eq!(exit.state, ManagedProcessState::Exited);
        assert_eq!(exit.stdout_bytes, 7);
    }

    #[cfg(windows)]
    #[tokio::test]
    async fn managed_process_rejects_shell_launches() {
        let _process_fixture_guard = process_fixture_lock().lock().await;
        let workspace = workspace();
        let spec = ProcessSpec::shell("Write-Output forbidden", workspace.path())
            .expect("shell process spec");
        assert!(matches!(
            ManagedProcess::spawn(spec, managed_limits()).await,
            Err(ProcessError::ManagedShellNotAllowed)
        ));
    }

    #[cfg(windows)]
    #[tokio::test]
    async fn managed_process_returns_only_normalized_diagnostics() {
        let _process_fixture_guard = process_fixture_lock().lock().await;
        let workspace = workspace();
        let script = workspace.path().join("diagnostics.ps1");
        fs::write(
            &script,
            concat!(
                "[Console]::Out.Write('INFO model loa')\n",
                "Start-Sleep -Milliseconds 20\n",
                "[Console]::Out.Write(\"ded bearer-canary-value`n\")\n",
                "[Console]::Error.Write('ERROR out of')\n",
                "Start-Sleep -Milliseconds 20\n",
                "[Console]::Error.Write(\" memory prompt-canary-value`n\")\n",
                "Start-Sleep -Seconds 30\n"
            ),
        )
        .expect("diagnostic script");
        let spec = ProcessSpec::argv(
            "powershell.exe",
            [
                "-NoLogo".to_owned(),
                "-NoProfile".to_owned(),
                "-NonInteractive".to_owned(),
                "-File".to_owned(),
                script.to_string_lossy().into_owned(),
            ],
            workspace.path(),
        )
        .expect("diagnostic process spec");

        let process = ManagedProcess::spawn(spec, managed_limits())
            .await
            .expect("managed process");
        tokio::time::sleep(Duration::from_millis(200)).await;
        let exit = process
            .terminate_and_wait(Duration::from_millis(100))
            .await
            .expect("managed termination");
        let rendered = format!("{exit:?}");
        assert!(!rendered.contains("bearer-canary-value"));
        assert!(!rendered.contains("prompt-canary-value"));
        assert!(exit
            .diagnostics
            .iter()
            .all(|value| value.starts_with("runtime-")));
    }

    #[cfg(windows)]
    #[tokio::test]
    async fn managed_process_listener_evidence_is_bound_to_pid_and_start_identity() {
        let _process_fixture_guard = process_fixture_lock().lock().await;
        let workspace = workspace();
        let port_file = workspace.path().join("port.txt");
        let escaped_port_file = port_file.to_string_lossy().replace('\'', "''");
        let spec = powershell_spec(
            &format!(
                "$listener = [Net.Sockets.TcpListener]::new([Net.IPAddress]::Loopback, 0); $listener.Start(); $port = ([Net.IPEndPoint]$listener.LocalEndpoint).Port; Set-Content -LiteralPath '{escaped_port_file}' -Value $port; Start-Sleep -Seconds 30"
            ),
            workspace.path(),
        );
        let process = ManagedProcess::spawn(spec, managed_limits())
            .await
            .expect("managed process");
        let mut expected_port = None;
        for _ in 0..100 {
            if let Ok(value) = fs::read_to_string(&port_file) {
                expected_port = value.trim().parse::<u16>().ok();
                if expected_port.is_some() {
                    break;
                }
            }
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
        let expected_port = expected_port.expect("listener port");
        let mut evidence = Vec::new();
        for _ in 0..100 {
            evidence = process.loopback_listeners().expect("listener evidence");
            if evidence.iter().any(|entry| entry.port == expected_port) {
                break;
            }
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
        let entry = evidence
            .iter()
            .find(|entry| entry.port == expected_port)
            .expect("owned listener");
        assert_eq!(entry.pid, process.id());
        assert_eq!(entry.process_started_at, process.process_started_at());
        assert_eq!(entry.address, "127.0.0.1");
        process
            .terminate_and_wait(Duration::from_millis(100))
            .await
            .expect("managed termination");
    }

    #[cfg(windows)]
    #[tokio::test]
    async fn managed_process_job_close_terminates_descendants() {
        let _process_fixture_guard = process_fixture_lock().lock().await;
        let workspace = workspace();
        let child_script = workspace.path().join("managed-child.ps1");
        let launched_marker = workspace.path().join("managed-launched.txt");
        let descendant_marker = workspace.path().join("managed-descendant.txt");
        let escaped_descendant = descendant_marker.to_string_lossy().replace('\'', "''");
        fs::write(
            &child_script,
            format!(
                "Start-Sleep -Milliseconds 1000\nSet-Content -LiteralPath '{escaped_descendant}' -Value child\n"
            ),
        )
        .expect("child script");
        let escaped_child = child_script.to_string_lossy().replace('\'', "''");
        let escaped_launched = launched_marker.to_string_lossy().replace('\'', "''");
        let parent = format!(
            "Start-Process -FilePath 'powershell.exe' -ArgumentList @('-NoLogo','-NoProfile','-NonInteractive','-ExecutionPolicy','Bypass','-File','{escaped_child}') -WindowStyle Hidden; Set-Content -LiteralPath '{escaped_launched}' -Value launched; Start-Sleep -Seconds 30"
        );
        let process =
            ManagedProcess::spawn(powershell_spec(&parent, workspace.path()), managed_limits())
                .await
                .expect("managed process");
        for _ in 0..100 {
            if launched_marker.exists() {
                break;
            }
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
        assert!(launched_marker.exists(), "parent did not launch its child");
        process
            .terminate_and_wait(Duration::from_millis(100))
            .await
            .expect("managed termination");
        tokio::time::sleep(Duration::from_millis(1500)).await;
        assert!(!descendant_marker.exists(), "managed descendant survived");
    }

    #[cfg(windows)]
    #[tokio::test]
    async fn managed_process_active_limit_denies_the_fourth_descendant() {
        let _process_fixture_guard = process_fixture_lock().lock().await;
        let workspace = workspace();
        let mut child_arguments = Vec::new();
        let mut markers = Vec::new();
        for index in 1..=4 {
            let marker = workspace.path().join(format!("managed-limit-{index}.txt"));
            let escaped_marker = marker.to_string_lossy().replace('\'', "''");
            let script = workspace.path().join(format!("managed-limit-{index}.ps1"));
            fs::write(
                &script,
                format!(
                    "Set-Content -LiteralPath '{escaped_marker}' -Value child; Start-Sleep -Seconds 30"
                ),
            )
            .expect("child script");
            let escaped_script = script.to_string_lossy().replace('\'', "''");
            child_arguments.push(format!(
                "Start-Process -FilePath 'powershell.exe' -ArgumentList @('-NoLogo','-NoProfile','-NonInteractive','-ExecutionPolicy','Bypass','-File','{escaped_script}') -WindowStyle Hidden"
            ));
            markers.push(marker);
        }
        let parent = format!("{}; Start-Sleep -Seconds 30", child_arguments.join("; "));
        let process =
            ManagedProcess::spawn(powershell_spec(&parent, workspace.path()), managed_limits())
                .await
                .expect("managed process");

        let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
        while !markers[..3].iter().all(|marker| marker.exists())
            && tokio::time::Instant::now() < deadline
        {
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
        assert!(
            markers[..3].iter().all(|marker| marker.exists()),
            "the first three descendants did not start"
        );
        tokio::time::sleep(Duration::from_millis(200)).await;
        assert!(
            !markers[3].exists(),
            "the fourth descendant became a fifth active process"
        );
        process
            .terminate_and_wait(Duration::from_millis(100))
            .await
            .expect("terminate through Job Object");
    }

    #[cfg(windows)]
    #[tokio::test]
    async fn managed_shutdown_reports_injected_job_termination_failure_without_pipe_hang() {
        let _process_fixture_guard = process_fixture_lock().lock().await;
        let workspace = workspace();
        let child_script = workspace.path().join("termination-failure-child.ps1");
        let launched_marker = workspace.path().join("termination-failure-launched.txt");
        fs::write(&child_script, "Start-Sleep -Seconds 30\n").expect("child script");
        let escaped_child = child_script.to_string_lossy().replace('\'', "''");
        let escaped_launched = launched_marker.to_string_lossy().replace('\'', "''");
        let parent = format!(
            "Start-Process -FilePath 'powershell.exe' -ArgumentList @('-NoLogo','-NoProfile','-NonInteractive','-ExecutionPolicy','Bypass','-File','{escaped_child}') -NoNewWindow; Set-Content -LiteralPath '{escaped_launched}' -Value launched; Start-Sleep -Seconds 30"
        );
        let mut process =
            ManagedProcess::spawn(powershell_spec(&parent, workspace.path()), managed_limits())
                .await
                .expect("managed process");
        let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
        while !launched_marker.exists() && tokio::time::Instant::now() < deadline {
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
        assert!(launched_marker.exists(), "descendant was not started");
        process.inject_containment_failure(crate::windows_job::TestContainmentFailure::Terminate);

        let result = tokio::time::timeout(
            Duration::from_secs(5),
            process.terminate_and_wait(Duration::ZERO),
        )
        .await
        .expect("cleanup must not hang on descendant-owned pipes");
        assert!(matches!(result, Err(ProcessError::Containment(_))));
    }

    #[cfg(windows)]
    #[tokio::test]
    async fn managed_natural_completion_reports_injected_job_close_failure() {
        let _process_fixture_guard = process_fixture_lock().lock().await;
        let workspace = workspace();
        let child_script = workspace.path().join("close-failure-child.ps1");
        fs::write(&child_script, "Start-Sleep -Seconds 30\n").expect("child script");
        let escaped_child = child_script.to_string_lossy().replace('\'', "''");
        let parent = format!(
            "Start-Process -FilePath 'powershell.exe' -ArgumentList @('-NoLogo','-NoProfile','-NonInteractive','-ExecutionPolicy','Bypass','-File','{escaped_child}') -NoNewWindow"
        );
        let mut process =
            ManagedProcess::spawn(powershell_spec(&parent, workspace.path()), managed_limits())
                .await
                .expect("managed process");
        process.inject_containment_failure(crate::windows_job::TestContainmentFailure::Close);

        let result = tokio::time::timeout(Duration::from_secs(5), process.wait())
            .await
            .expect("cleanup must not hang on descendant-owned pipes");
        assert!(matches!(result, Err(ProcessError::Containment(_))));
    }

    #[cfg(windows)]
    #[tokio::test]
    async fn lifecycle_regression_normal_capture_failure_settles_sibling_for_ordinary_and_managed()
    {
        let _process_fixture_guard = process_fixture_lock().lock().await;
        let workspace = workspace();
        let mut detached = Vec::new();

        let ordinary_probe = OrdinaryCaptureProbe::new();
        let ordinary_run = with_ordinary_process_test_hooks(
            None,
            Some(ordinary_probe.clone()),
            Some(ProcessStream::Stdout),
            ProcessRunner.run(
                powershell_spec("exit 0", workspace.path()),
                CancellationToken::new(),
            ),
        );
        let mut ordinary_task = tokio::spawn(ordinary_run);
        let ordinary_early = tokio::time::timeout(Duration::from_secs(2), &mut ordinary_task).await;
        if ordinary_early.is_ok() && ordinary_probe.active_tasks() != 0 {
            detached.push("ordinary normal completion");
        }
        ordinary_probe.release();
        let ordinary_result = match ordinary_early {
            Ok(joined) => joined.expect("ordinary runner task"),
            Err(_) => ordinary_task.await.expect("ordinary runner task"),
        };
        assert!(matches!(ordinary_result, Err(ProcessError::Output(_))));
        assert!(
            wait_for_capture_tasks(&ordinary_probe, 0, Duration::from_secs(1)).await,
            "ordinary capture task did not settle"
        );

        let mut managed = ManagedProcess::spawn(
            powershell_spec("exit 0", workspace.path()),
            managed_limits(),
        )
        .await
        .expect("managed process");
        let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
        while !managed.has_exited().expect("managed status")
            && tokio::time::Instant::now() < deadline
        {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        assert!(managed.has_exited().expect("managed exit"));
        let managed_probe = OrdinaryCaptureProbe::new();
        managed
            .replace_capture_tasks_for_test(
                failed_capture_task(managed_probe.clone()),
                blocked_capture_task(managed_probe.clone()),
            )
            .await;
        assert!(
            wait_for_capture_tasks(&managed_probe, 1, Duration::from_secs(1)).await,
            "managed sibling capture task did not start"
        );
        let mut managed_task = tokio::spawn(managed.wait());
        let managed_early = tokio::time::timeout(Duration::from_secs(1), &mut managed_task).await;
        if managed_early.is_ok() && managed_probe.active_tasks() != 0 {
            detached.push("managed normal completion");
        }
        managed_probe.release();
        let managed_result = match managed_early {
            Ok(joined) => joined.expect("managed wait task"),
            Err(_) => managed_task.await.expect("managed wait task"),
        };
        assert!(matches!(managed_result, Err(ProcessError::Output(_))));
        assert!(
            wait_for_capture_tasks(&managed_probe, 0, Duration::from_secs(1)).await,
            "managed capture task did not settle"
        );

        assert!(
            detached.is_empty(),
            "capture failure detached sibling tasks: {detached:?}"
        );
    }

    #[cfg(windows)]
    #[tokio::test]
    async fn lifecycle_regression_managed_wait_failures_cleanup_containment_and_capture_tasks() {
        let _process_fixture_guard = process_fixture_lock().lock().await;
        let workspace = workspace();
        let mut failures = Vec::new();

        for failure in [
            TestManagedLifecycleFailure::Wait,
            TestManagedLifecycleFailure::TryWait,
            TestManagedLifecycleFailure::GracefulWait,
            TestManagedLifecycleFailure::Reap,
        ] {
            let mut process = ManagedProcess::spawn(
                powershell_spec("Start-Sleep -Seconds 30", workspace.path()),
                managed_limits(),
            )
            .await
            .expect("managed process");
            let probe = OrdinaryCaptureProbe::new();
            process
                .replace_capture_tasks_for_test(
                    blocked_capture_task(probe.clone()),
                    blocked_capture_task(probe.clone()),
                )
                .await;
            assert!(
                wait_for_capture_tasks(&probe, 2, Duration::from_secs(1)).await,
                "managed capture tasks did not start"
            );
            process.inject_lifecycle_failure(failure);
            if failure == TestManagedLifecycleFailure::Wait {
                process.inject_containment_failure(
                    crate::windows_job::TestContainmentFailure::Terminate,
                );
            }

            let result = tokio::time::timeout(Duration::from_secs(5), async move {
                match failure {
                    TestManagedLifecycleFailure::Wait => process.wait().await,
                    TestManagedLifecycleFailure::TryWait | TestManagedLifecycleFailure::Reap => {
                        process.terminate_and_wait(Duration::ZERO).await
                    }
                    TestManagedLifecycleFailure::GracefulWait => {
                        process.terminate_and_wait(Duration::from_millis(100)).await
                    }
                }
            })
            .await
            .expect("managed failure cleanup must be bounded");
            let active_at_return = probe.active_tasks();
            let expected_error = if failure == TestManagedLifecycleFailure::Wait {
                matches!(result, Err(ProcessError::Containment(_)))
            } else {
                matches!(result, Err(ProcessError::Wait(_)))
            };
            probe.release();
            assert!(
                wait_for_capture_tasks(&probe, 0, Duration::from_secs(1)).await,
                "managed capture tasks did not release"
            );
            if active_at_return != 0 || !expected_error {
                failures.push(format!(
                    "{failure:?}: active={active_at_return}, result={result:?}"
                ));
            }
        }

        assert!(
            failures.is_empty(),
            "managed wait failures skipped explicit cleanup: {failures:?}"
        );
    }

    #[cfg(windows)]
    #[tokio::test]
    async fn lifecycle_regression_suspended_reject_failure_settles_capture_tasks() {
        let _process_fixture_guard = process_fixture_lock().lock().await;
        let workspace = workspace();
        let mut suspended = ManagedProcess::spawn_suspended(
            powershell_spec("Start-Sleep -Seconds 30", workspace.path()),
            managed_limits(),
            None,
        )
        .await
        .expect("suspended managed process");
        let probe = OrdinaryCaptureProbe::new();
        suspended
            .replace_capture_tasks_for_test(
                blocked_capture_task(probe.clone()),
                blocked_capture_task(probe.clone()),
            )
            .await;
        assert!(
            wait_for_capture_tasks(&probe, 2, Duration::from_secs(1)).await,
            "suspended capture tasks did not start"
        );
        suspended.inject_lifecycle_failure(TestManagedLifecycleFailure::Reap);

        let result = tokio::time::timeout(Duration::from_secs(5), suspended.reject())
            .await
            .expect("suspended rejection cleanup must be bounded");
        let active_at_return = probe.active_tasks();
        probe.release();
        assert!(
            wait_for_capture_tasks(&probe, 0, Duration::from_secs(1)).await,
            "suspended capture tasks did not release"
        );
        assert!(matches!(result, Err(ProcessError::Wait(_))));
        assert_eq!(
            active_at_return, 0,
            "suspended reject failure detached capture tasks"
        );
    }
}
