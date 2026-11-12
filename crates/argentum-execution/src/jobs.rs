use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use argentum_domain::{
    process_job_contract::{PollSelection, MAX_POLL_CURSOR, POLL_WAIT_MS},
    ProjectId, RunId, SessionId, VerificationEvidence, VerificationRunnerId,
};
use argentum_security::SecretRedactor;
use serde::Serialize;
use thiserror::Error;
use tokio::sync::{mpsc, Notify};
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use crate::{
    ManagedProcess, ManagedProcessLimits, ProcessError, ProcessOutputChunk, ProcessStream,
    VerificationJobTermination, VerificationRunner,
};

pub const MAX_ACTIVE_JOBS_PER_RUN: usize = 4;
pub const MAX_ACTIVE_JOBS_PER_HOST: usize = 8;
pub const MAX_RETAINED_JOBS: usize = 64;
pub const MAX_RETAINED_OUTPUT_PER_STREAM: usize = 64 * 1024;
pub const MAX_POLL_WAIT: Duration = Duration::from_millis(POLL_WAIT_MS);

#[derive(Debug, Error)]
pub enum ProcessJobError {
    #[error("process job handle is not available to this owner")]
    NotFound,
    #[error("process job cursor is invalid")]
    InvalidCursor,
    #[error("process job capacity is exhausted")]
    Capacity,
    #[error("process job wait is outside its bound")]
    InvalidWait,
    #[error(transparent)]
    Verification(#[from] crate::VerificationError),
    #[error(transparent)]
    Process(#[from] ProcessError),
    #[error(transparent)]
    RustCheck(#[from] crate::RustCheckError),
    #[error("process job state is unavailable")]
    State,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProcessJobOwner {
    pub host_generation: Uuid,
    pub project_id: ProjectId,
    pub workspace_key: String,
    pub session_id: SessionId,
    pub run_id: RunId,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ProcessJobState {
    Starting,
    Running,
    Exited,
    Cancelling,
    Cancelled,
    TimedOut,
    Failed,
    Unresolved,
}

impl ProcessJobState {
    pub const fn label(self) -> &'static str {
        match self {
            Self::Starting => "starting",
            Self::Running => "running",
            Self::Exited => "exited",
            Self::Cancelling => "cancelling",
            Self::Cancelled => "cancelled",
            Self::TimedOut => "timed_out",
            Self::Failed => "failed",
            Self::Unresolved => "unresolved",
        }
    }

    pub const fn is_terminal(self) -> bool {
        matches!(
            self,
            Self::Exited | Self::Cancelled | Self::TimedOut | Self::Failed | Self::Unresolved
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProcessJobId(Uuid);

impl ProcessJobId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }

    pub fn as_str(&self) -> String {
        self.0.to_string()
    }
}

impl Default for ProcessJobId {
    fn default() -> Self {
        Self::new()
    }
}

impl std::str::FromStr for ProcessJobId {
    type Err = ProcessJobError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Uuid::parse_str(value)
            .map(Self)
            .map_err(|_| ProcessJobError::NotFound)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ProcessJobStream {
    pub stream: ProcessStream,
    pub from_cursor: u64,
    pub to_cursor: u64,
    pub earliest_cursor: u64,
    pub next_cursor: u64,
    pub available: bool,
    pub gap: bool,
    pub complete: bool,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ProcessJobSnapshot {
    pub job_id: String,
    pub state: ProcessJobState,
    pub runner: VerificationRunnerId,
    pub exit_code: Option<i32>,
    pub stdout: ProcessJobStream,
    pub stderr: ProcessJobStream,
    pub stdout_bytes: u64,
    pub stderr_bytes: u64,
    pub stdout_truncated: bool,
    pub stderr_truncated: bool,
    pub verification: Option<VerificationEvidence>,
    pub detail: Option<String>,
}

impl ProcessJobSnapshot {
    pub fn poll_selection(&self) -> Option<PollSelection> {
        (!self.state.is_terminal()).then(|| PollSelection {
            job_id: self.job_id.clone(),
            stdout_cursor: self.stdout.next_cursor,
            stderr_cursor: self.stderr.next_cursor,
            wait_ms: POLL_WAIT_MS,
        })
    }
}

#[derive(Debug, Default)]
struct StreamBuffer {
    base: u64,
    bytes: Vec<u8>,
    total: u64,
    truncated: bool,
    complete: bool,
}

impl StreamBuffer {
    fn append(&mut self, text: &str) {
        let bytes = text.as_bytes();
        self.total = self
            .total
            .saturating_add(bytes.len().try_into().unwrap_or(u64::MAX));
        self.bytes.extend_from_slice(bytes);
        if self.bytes.len() > MAX_RETAINED_OUTPUT_PER_STREAM {
            let drop_count = self.bytes.len() - MAX_RETAINED_OUTPUT_PER_STREAM;
            self.bytes.drain(..drop_count);
            self.base = self
                .base
                .saturating_add(drop_count.try_into().unwrap_or(u64::MAX));
            self.truncated = true;
        }
    }

    fn finish(&mut self) {
        self.complete = true;
    }

    fn read(
        &self,
        stream: ProcessStream,
        cursor: u64,
    ) -> Result<ProcessJobStream, ProcessJobError> {
        let next = self
            .base
            .saturating_add(self.bytes.len().try_into().unwrap_or(u64::MAX));
        if cursor > next {
            return Err(ProcessJobError::InvalidCursor);
        }
        let gap = cursor < self.base;
        let from = cursor.max(self.base);
        let start = usize::try_from(from.saturating_sub(self.base)).unwrap_or(self.bytes.len());
        let text = String::from_utf8_lossy(&self.bytes[start..]).into_owned();
        let to = next;
        Ok(ProcessJobStream {
            stream,
            from_cursor: from,
            to_cursor: to,
            earliest_cursor: self.base,
            next_cursor: next,
            available: from < next,
            gap,
            complete: self.complete && !gap,
            text,
        })
    }
}

#[derive(Debug)]
struct JobRecord {
    owner: ProcessJobOwner,
    id: ProcessJobId,
    runner: VerificationRunnerId,
    state: Mutex<ProcessJobState>,
    exit_code: Mutex<Option<i32>>,
    stdout: Mutex<StreamBuffer>,
    stderr: Mutex<StreamBuffer>,
    verification: Mutex<Option<VerificationEvidence>>,
    detail: Mutex<Option<String>>,
    cancellation: CancellationToken,
    changed: Notify,
}

#[derive(Clone)]
pub struct ProcessJobManager {
    host_generation: Uuid,
    jobs: Arc<Mutex<BTreeMap<Uuid, Arc<JobRecord>>>>,
    tasks: Arc<Mutex<Vec<tokio::task::JoinHandle<()>>>>,
    redactor: SecretRedactor,
    rust_checks: Arc<Mutex<BTreeMap<RunId, RustCheckRun>>>,
}

#[derive(Default)]
struct RustCheckRun {
    owner: Option<ProcessJobOwner>,
    admitted: usize,
    active: bool,
    admission_failed: bool,
    receipts: Vec<argentum_domain::supplemental::SupplementalCheckReceipt>,
}
struct RustCheckAdmission {
    runs: Arc<Mutex<BTreeMap<RunId, RustCheckRun>>>,
    run: RunId,
}
#[cfg(test)]
mod rust_check_admission_tests {
    use super::*;
    use argentum_domain::supplemental::{RustCheckProfile, RustCheckSource};
    #[tokio::test]
    async fn six_admissions_and_one_active_check_are_enforced_even_for_rejections() {
        let root = tempfile::tempdir().unwrap();
        std::fs::write(
            root.path().join("Cargo.toml"),
            "[package]\nname='actual'\nversion='0.1.0'\n",
        )
        .unwrap();
        let workspace = argentum_workspaces::WorkspaceManager::new(
            argentum_security::CapabilityBroker::new(
                root.path(),
                argentum_security::ApprovalPolicy::default(),
            )
            .unwrap(),
        );
        let manager = ProcessJobManager::new();
        let owner = ProcessJobOwner {
            host_generation: manager.host_generation(),
            project_id: Uuid::new_v4(),
            session_id: Uuid::new_v4(),
            run_id: Uuid::new_v4(),
            workspace_key: workspace
                .root()
                .to_string_lossy()
                .replace('\\', "/")
                .to_ascii_lowercase(),
        };
        let request = || crate::RustCheckRequest {
            package: "wrong".into(),
            profile: RustCheckProfile::Debug,
            source: RustCheckSource::new("#[test]fn check(){assert!(true);}".into()).unwrap(),
            replaces: None,
        };
        for _ in 0..6 {
            assert!(manager
                .run_rust_check(
                    owner.clone(),
                    workspace.clone(),
                    request(),
                    CancellationToken::new(),
                    Duration::from_secs(60)
                )
                .await
                .is_err());
        }
        assert!(manager.rust_check_admission_unsettled(&owner).unwrap());
        assert!(manager.rust_check_receipts(&owner).unwrap().is_empty());
        assert!(matches!(
            manager
                .run_rust_check(
                    owner.clone(),
                    workspace.clone(),
                    request(),
                    CancellationToken::new(),
                    Duration::from_secs(60)
                )
                .await,
            Err(ProcessJobError::Capacity)
        ));
        let next = ProcessJobOwner {
            run_id: Uuid::new_v4(),
            ..owner
        };
        manager.rust_checks.lock().unwrap().insert(
            next.run_id,
            RustCheckRun {
                owner: Some(next.clone()),
                active: true,
                ..RustCheckRun::default()
            },
        );
        assert!(matches!(
            manager
                .run_rust_check(
                    next,
                    workspace,
                    request(),
                    CancellationToken::new(),
                    Duration::from_secs(60)
                )
                .await,
            Err(ProcessJobError::Capacity)
        ));
    }
}
impl Drop for RustCheckAdmission {
    fn drop(&mut self) {
        if let Ok(mut runs) = self.runs.lock() {
            if let Some(run) = runs.get_mut(&self.run) {
                run.active = false;
            }
        }
    }
}

impl std::fmt::Debug for ProcessJobManager {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ProcessJobManager")
            .field("host_generation", &self.host_generation)
            .finish_non_exhaustive()
    }
}

impl Default for ProcessJobManager {
    fn default() -> Self {
        Self::new()
    }
}

impl ProcessJobManager {
    /// Apply the same in-run disclosure rules before diagnostic selection.
    /// This is not a reader and grants no access to a process or stream.
    pub fn disclose_process_text(&self, text: &str) -> String {
        redacted_process_text(&self.redactor, text)
    }

    /// A truncated capture can end inside a registered secret. Keep its unsafe
    /// overlap unavailable instead of selecting an unrecognizable tail fragment.
    pub fn disclose_captured_process_text(&self, text: &str, truncated: bool) -> String {
        let mut rendered = self.disclose_process_text(text);
        if truncated {
            let overlap = self.redactor.max_secret_utf8_bytes().max(64);
            let mut end = rendered.len().saturating_sub(overlap);
            while !rendered.is_char_boundary(end) {
                end -= 1;
            }
            rendered.truncate(end);
        }
        rendered
    }
    pub fn new() -> Self {
        Self::with_redactor(SecretRedactor::new(std::iter::empty()))
    }

    pub fn with_redactor(redactor: SecretRedactor) -> Self {
        Self {
            host_generation: Uuid::new_v4(),
            jobs: Arc::new(Mutex::new(BTreeMap::new())),
            tasks: Arc::new(Mutex::new(Vec::new())),
            redactor,
            rust_checks: Arc::new(Mutex::new(BTreeMap::new())),
        }
    }

    pub fn host_generation(&self) -> Uuid {
        self.host_generation
    }

    pub async fn run_rust_check(
        &self,
        owner: ProcessJobOwner,
        workspace: argentum_workspaces::WorkspaceManager,
        request: crate::RustCheckRequest,
        cancellation: CancellationToken,
        maximum: Duration,
    ) -> Result<crate::RustCheckOutput, ProcessJobError> {
        if owner.host_generation != self.host_generation
            || owner.workspace_key
                != workspace
                    .root()
                    .to_string_lossy()
                    .replace('\\', "/")
                    .to_ascii_lowercase()
        {
            return Err(ProcessJobError::NotFound);
        }
        {
            // Same lock order as fixed verification admission; do not enlarge host/run capacity.
            let jobs = self.jobs.lock().map_err(|_| ProcessJobError::State)?;
            let mut runs = self
                .rust_checks
                .lock()
                .map_err(|_| ProcessJobError::State)?;
            let active_jobs = jobs
                .values()
                .filter(|j| j.state.lock().map(|s| !s.is_terminal()).unwrap_or(true))
                .count();
            let active_checks = runs.values().filter(|r| r.active).count();
            if active_jobs + active_checks >= MAX_ACTIVE_JOBS_PER_HOST
                || (!runs.contains_key(&owner.run_id) && runs.len() >= MAX_RETAINED_JOBS)
            {
                return Err(ProcessJobError::Capacity);
            }
            if jobs
                .values()
                .filter(|j| {
                    j.owner.run_id == owner.run_id
                        && j.state.lock().map(|s| !s.is_terminal()).unwrap_or(true)
                })
                .count()
                >= MAX_ACTIVE_JOBS_PER_RUN
            {
                return Err(ProcessJobError::Capacity);
            }
            let run = runs.entry(owner.run_id).or_default();
            if run
                .owner
                .as_ref()
                .is_some_and(|previous| previous != &owner)
            {
                return Err(ProcessJobError::NotFound);
            }
            if run.active || run.admitted >= argentum_domain::supplemental::RUST_CHECK_MAX_BATCHES {
                return Err(ProcessJobError::Capacity);
            }
            if request
                .replaces
                .is_some_and(|id| !run.receipts.iter().any(|r| r.id == id))
            {
                return Err(ProcessJobError::NotFound);
            }
            run.owner = Some(owner.clone());
            run.admitted += 1;
            run.active = true;
            // A rejection after admission cannot disappear merely because no receipt was issued.
            run.admission_failed = true;
        }
        let _admission = RustCheckAdmission {
            runs: Arc::clone(&self.rust_checks),
            run: owner.run_id,
        };
        let mut output =
            crate::run_rust_check(workspace, owner.clone(), request, cancellation, maximum).await?;
        output.stdout = redacted_process_text(&self.redactor, &output.stdout);
        output.stderr = redacted_process_text(&self.redactor, &output.stderr);
        let mut runs = self
            .rust_checks
            .lock()
            .map_err(|_| ProcessJobError::State)?;
        let run = runs
            .get_mut(&owner.run_id)
            .ok_or(ProcessJobError::NotFound)?;
        run.admission_failed = false;
        run.receipts.push(output.receipt.clone());
        Ok(output)
    }

    pub fn rust_check_admission_unsettled(
        &self,
        owner: &ProcessJobOwner,
    ) -> Result<bool, ProcessJobError> {
        if owner.host_generation != self.host_generation {
            return Err(ProcessJobError::NotFound);
        }
        let runs = self
            .rust_checks
            .lock()
            .map_err(|_| ProcessJobError::State)?;
        match runs.get(&owner.run_id) {
            Some(run) if run.owner.as_ref() == Some(owner) => {
                Ok(run.active || run.admission_failed)
            }
            Some(_) => Err(ProcessJobError::NotFound),
            None => Ok(false),
        }
    }

    pub fn rust_check_receipts(
        &self,
        owner: &ProcessJobOwner,
    ) -> Result<Vec<argentum_domain::supplemental::SupplementalCheckReceipt>, ProcessJobError> {
        if owner.host_generation != self.host_generation {
            return Err(ProcessJobError::NotFound);
        }
        let runs = self
            .rust_checks
            .lock()
            .map_err(|_| ProcessJobError::State)?;
        match runs.get(&owner.run_id) {
            Some(run) if run.owner.as_ref() == Some(owner) => Ok(run.receipts.clone()),
            Some(_) => Err(ProcessJobError::NotFound),
            None => Ok(Vec::new()),
        }
    }

    pub fn start_verification(
        &self,
        owner: ProcessJobOwner,
        verifier: VerificationRunner,
        runner: VerificationRunnerId,
        parent_cancellation: CancellationToken,
    ) -> Result<String, ProcessJobError> {
        if owner.host_generation != self.host_generation {
            return Err(ProcessJobError::NotFound);
        }
        let capture_cancellation = parent_cancellation.child_token();
        let job = verifier.prepare_job_with_cancellation(
            owner.run_id,
            runner.clone(),
            capture_cancellation.clone(),
        )?;
        let id = ProcessJobId::new();
        let record = Arc::new(JobRecord {
            owner: owner.clone(),
            id: id.clone(),
            runner,
            state: Mutex::new(ProcessJobState::Starting),
            exit_code: Mutex::new(None),
            stdout: Mutex::new(StreamBuffer::default()),
            stderr: Mutex::new(StreamBuffer::default()),
            verification: Mutex::new(None),
            detail: Mutex::new(None),
            cancellation: capture_cancellation,
            changed: Notify::new(),
        });
        {
            let mut jobs = self.jobs.lock().map_err(|_| ProcessJobError::State)?;
            let active_host = jobs
                .values()
                .filter(|item| {
                    !item
                        .state
                        .lock()
                        .map(|state| state.is_terminal())
                        .unwrap_or(true)
                })
                .count();
            let active_run = jobs
                .values()
                .filter(|item| item.owner.run_id == owner.run_id)
                .filter(|item| {
                    !item
                        .state
                        .lock()
                        .map(|state| state.is_terminal())
                        .unwrap_or(true)
                })
                .count();
            let checks = self
                .rust_checks
                .lock()
                .map_err(|_| ProcessJobError::State)?;
            let check_host = checks.values().filter(|r| r.active).count();
            let check_run = checks
                .get(&owner.run_id)
                .map_or(0, |r| usize::from(r.active));
            if active_host + check_host >= MAX_ACTIVE_JOBS_PER_HOST
                || active_run + check_run >= MAX_ACTIVE_JOBS_PER_RUN
                || jobs.len() >= MAX_RETAINED_JOBS
            {
                return Err(ProcessJobError::Capacity);
            }
            jobs.insert(id.0, Arc::clone(&record));
        }
        let (output_tx, mut output_rx) = mpsc::unbounded_channel::<ProcessOutputChunk>();
        let manager = self.clone();
        let cancellation = record.cancellation.clone();
        let record_for_output = Arc::clone(&record);
        let redactor = self.redactor.clone();
        let output_task = tokio::spawn(async move {
            let mut tails = [String::new(), String::new()];
            let overlap = redactor.max_secret_utf8_bytes().max(64);
            let disclosure_allowed = overlap <= MAX_RETAINED_OUTPUT_PER_STREAM;
            let mut withheld = [false, false];
            while let Some(chunk) = output_rx.recv().await {
                let index = match chunk.stream {
                    ProcessStream::Stdout => 0,
                    ProcessStream::Stderr => 1,
                };
                if !disclosure_allowed {
                    if !withheld[index] {
                        if let Ok(mut buffer) = if index == 0 {
                            record_for_output.stdout.lock()
                        } else {
                            record_for_output.stderr.lock()
                        } {
                            buffer.append("[process output unavailable: disclosure overlap exceeds retained bound]");
                        }
                        withheld[index] = true;
                    }
                    continue;
                }
                // Redact before clipping: a complete secret spanning the split
                // must not become two unrecognizable fragments. An unterminated
                // key-value marker stays in the overlap with its redacted value.
                let safe = disclosed_chunk(&redactor, &mut tails[index], &chunk.text, overlap);
                if !safe.is_empty() {
                    if let Ok(mut buffer) = match chunk.stream {
                        ProcessStream::Stdout => record_for_output.stdout.lock(),
                        ProcessStream::Stderr => record_for_output.stderr.lock(),
                    } {
                        buffer.append(&safe);
                    }
                    record_for_output.changed.notify_waiters();
                }
            }
            for (index, tail) in tails.into_iter().enumerate() {
                if tail.is_empty() {
                    continue;
                }
                let rendered = redacted_process_text(&redactor, &tail);
                if let Ok(mut buffer) = if index == 0 {
                    record_for_output.stdout.lock()
                } else {
                    record_for_output.stderr.lock()
                } {
                    buffer.append(&rendered);
                }
            }
            if let Ok(mut stdout) = record_for_output.stdout.lock() {
                stdout.finish();
            }
            if let Ok(mut stderr) = record_for_output.stderr.lock() {
                stderr.finish();
            }
            record_for_output.changed.notify_waiters();
        });
        let record_for_task = Arc::clone(&record);
        let parent_for_task = parent_cancellation.clone();
        let task = tokio::spawn(async move {
            let process = ManagedProcess::spawn_with_output(
                job.spec.clone(),
                verification_limits(&job.runner),
                output_tx,
            )
            .await;
            let exit = match process {
                Ok(process) => {
                    set_state(&record_for_task, ProcessJobState::Running);
                    process
                        .wait_with_cancellation(cancellation.clone(), job.spec.timeout())
                        .await
                }
                Err(error) => {
                    set_failed(&record_for_task, ProcessJobState::Failed, error.to_string());
                    return;
                }
            };
            let termination = if exit.as_ref().is_ok_and(|value| value.timed_out) {
                VerificationJobTermination::TimedOut
            } else if cancellation.is_cancelled() || parent_for_task.is_cancelled() {
                VerificationJobTermination::Cancelled
            } else {
                VerificationJobTermination::Completed
            };
            let exit = match exit {
                Ok(exit) => exit,
                Err(error) => {
                    set_failed(
                        &record_for_task,
                        ProcessJobState::Unresolved,
                        error.to_string(),
                    );
                    return;
                }
            };
            let _ = output_task.await;
            if let Ok(mut output) = record_for_task.stdout.lock() {
                output.finish();
            }
            if let Ok(mut output) = record_for_task.stderr.lock() {
                output.finish();
            }
            if let Ok(mut code) = record_for_task.exit_code.lock() {
                *code = exit.exit_code;
            }
            let evidence = verifier.settle_job(&job, &exit, termination);
            match evidence {
                Ok(evidence) => {
                    let state = match termination {
                        VerificationJobTermination::Completed => ProcessJobState::Exited,
                        VerificationJobTermination::Cancelled => ProcessJobState::Cancelled,
                        VerificationJobTermination::TimedOut => ProcessJobState::TimedOut,
                    };
                    if let Ok(mut stored) = record_for_task.verification.lock() {
                        *stored = Some(evidence);
                    }
                    set_state(&record_for_task, state);
                }
                Err(crate::VerificationError::Workspace(
                    argentum_workspaces::WorkspaceError::VerificationCancelled,
                )) => {
                    // No complete post-run identity exists. Preserve cancellation
                    // without inventing a verification receipt or fresh digest.
                    set_failed(
                        &record_for_task,
                        ProcessJobState::Cancelled,
                        "Verification input capture cancelled; no complete receipt".into(),
                    );
                }
                Err(error) => set_failed(
                    &record_for_task,
                    ProcessJobState::Unresolved,
                    error.to_string(),
                ),
            }
            manager.reap_completed_tasks();
        });
        self.tasks
            .lock()
            .map_err(|_| ProcessJobError::State)?
            .push(task);
        let parent = parent_cancellation.clone();
        let record_for_parent = Arc::clone(&record);
        tokio::spawn(async move {
            loop {
                tokio::select! {
                    _ = parent.cancelled() => {
                        if let Ok(state) = record_for_parent.state.lock() {
                            if !state.is_terminal() {
                                record_for_parent.cancellation.cancel();
                                record_for_parent.changed.notify_waiters();
                            }
                        }
                        break;
                    }
                    _ = record_for_parent.changed.notified() => {
                        if record_for_parent.state.lock().map(|state| state.is_terminal()).unwrap_or(true) {
                            break;
                        }
                    }
                }
            }
        });
        Ok(id.as_str())
    }

    pub async fn poll(
        &self,
        owner: &ProcessJobOwner,
        job_id: &str,
        stdout_cursor: u64,
        stderr_cursor: u64,
        wait: Duration,
    ) -> Result<ProcessJobSnapshot, ProcessJobError> {
        if wait > MAX_POLL_WAIT {
            return Err(ProcessJobError::InvalidWait);
        }
        if stdout_cursor > MAX_POLL_CURSOR || stderr_cursor > MAX_POLL_CURSOR {
            return Err(ProcessJobError::InvalidCursor);
        }
        let record = self.lookup(owner, job_id)?;
        let deadline = tokio::time::sleep(wait);
        tokio::pin!(deadline);
        loop {
            let current = snapshot(&record, stdout_cursor, stderr_cursor)?;
            if current.stdout.available || current.stderr.available || current.state.is_terminal() {
                return Ok(current);
            }
            if wait.is_zero() {
                return Ok(current);
            }
            let notification = record.changed.notified();
            tokio::select! {
                _ = notification => {}
                _ = &mut deadline => return snapshot(&record, stdout_cursor, stderr_cursor),
            }
        }
    }

    pub fn cancel(
        &self,
        owner: &ProcessJobOwner,
        job_id: &str,
    ) -> Result<ProcessJobSnapshot, ProcessJobError> {
        let record = self.lookup(owner, job_id)?;
        if let Ok(mut state) = record.state.lock() {
            if !state.is_terminal() {
                *state = ProcessJobState::Cancelling;
                record.cancellation.cancel();
            }
        }
        record.changed.notify_waiters();
        let stdout_cursor = record
            .stdout
            .lock()
            .map_err(|_| ProcessJobError::State)?
            .base;
        let stderr_cursor = record
            .stderr
            .lock()
            .map_err(|_| ProcessJobError::State)?
            .base;
        snapshot(&record, stdout_cursor, stderr_cursor)
    }

    pub fn cancel_run(&self, run_id: RunId) -> Result<(), ProcessJobError> {
        let records = self
            .jobs
            .lock()
            .map_err(|_| ProcessJobError::State)?
            .values()
            .cloned()
            .collect::<Vec<_>>();
        for record in records
            .into_iter()
            .filter(|record| record.owner.run_id == run_id)
        {
            if let Ok(mut state) = record.state.lock() {
                if !state.is_terminal() {
                    *state = ProcessJobState::Cancelling;
                    record.cancellation.cancel();
                }
            }
            record.changed.notify_waiters();
        }
        Ok(())
    }

    pub fn has_live_run(&self, run_id: RunId) -> Result<bool, ProcessJobError> {
        let records = self.jobs.lock().map_err(|_| ProcessJobError::State)?;
        for record in records
            .values()
            .filter(|record| record.owner.run_id == run_id)
        {
            let state = record.state.lock().map_err(|_| ProcessJobError::State)?;
            if !state.is_terminal() {
                return Ok(true);
            }
        }
        Ok(false)
    }

    pub async fn settle_run(&self, run_id: RunId) -> Result<(), ProcessJobError> {
        self.cancel_run(run_id)?;
        self.drain_tasks().await
    }

    pub async fn shutdown(&self) -> Result<(), ProcessJobError> {
        let records = self
            .jobs
            .lock()
            .map_err(|_| ProcessJobError::State)?
            .values()
            .cloned()
            .collect::<Vec<_>>();
        for record in records {
            record.cancellation.cancel();
        }
        self.drain_tasks().await
    }

    async fn drain_tasks(&self) -> Result<(), ProcessJobError> {
        let tasks = self
            .tasks
            .lock()
            .map_err(|_| ProcessJobError::State)?
            .drain(..)
            .collect::<Vec<_>>();
        for task in tasks {
            task.await.map_err(|_| ProcessJobError::State)?;
        }
        Ok(())
    }

    fn lookup(
        &self,
        owner: &ProcessJobOwner,
        job_id: &str,
    ) -> Result<Arc<JobRecord>, ProcessJobError> {
        if owner.host_generation != self.host_generation {
            return Err(ProcessJobError::NotFound);
        }
        let id = job_id.parse::<ProcessJobId>()?;
        let record = self
            .jobs
            .lock()
            .map_err(|_| ProcessJobError::State)?
            .get(&id.0)
            .cloned()
            .ok_or(ProcessJobError::NotFound)?;
        if record.owner != *owner {
            return Err(ProcessJobError::NotFound);
        }
        Ok(record)
    }

    fn reap_completed_tasks(&self) {
        if let Ok(mut tasks) = self.tasks.lock() {
            tasks.retain(|task| !task.is_finished());
        }
    }
}

fn snapshot(
    record: &JobRecord,
    stdout_cursor: u64,
    stderr_cursor: u64,
) -> Result<ProcessJobSnapshot, ProcessJobError> {
    let state = *record.state.lock().map_err(|_| ProcessJobError::State)?;
    let stdout = record
        .stdout
        .lock()
        .map_err(|_| ProcessJobError::State)?
        .read(ProcessStream::Stdout, stdout_cursor)?;
    let stderr = record
        .stderr
        .lock()
        .map_err(|_| ProcessJobError::State)?
        .read(ProcessStream::Stderr, stderr_cursor)?;
    let exit_code = *record
        .exit_code
        .lock()
        .map_err(|_| ProcessJobError::State)?;
    let stdout_guard = record.stdout.lock().map_err(|_| ProcessJobError::State)?;
    let stdout_bytes = stdout_guard.total;
    let stdout_truncated = stdout_guard.truncated;
    drop(stdout_guard);
    let stderr_guard = record.stderr.lock().map_err(|_| ProcessJobError::State)?;
    let stderr_bytes = stderr_guard.total;
    let stderr_truncated = stderr_guard.truncated;
    drop(stderr_guard);
    let verification = record
        .verification
        .lock()
        .map_err(|_| ProcessJobError::State)?
        .clone();
    let detail = record
        .detail
        .lock()
        .map_err(|_| ProcessJobError::State)?
        .clone();
    Ok(ProcessJobSnapshot {
        job_id: record.id.as_str(),
        state,
        runner: record.runner.clone(),
        // The following fields intentionally expose counters only.
        // (The individual lock scopes keep polling non-destructive.)
        //
        exit_code,
        // Output is never copied into the audit store.
        stdout_bytes,
        stderr_bytes,
        stdout_truncated,
        stderr_truncated,
        verification,
        detail,
        stdout,
        stderr,
    })
}

fn set_state(record: &JobRecord, state: ProcessJobState) {
    if let Ok(mut current) = record.state.lock() {
        *current = state;
    }
    record.changed.notify_waiters();
}

fn set_failed(record: &JobRecord, state: ProcessJobState, detail: String) {
    if let Ok(mut current) = record.state.lock() {
        *current = state;
    }
    if let Ok(mut stored) = record.detail.lock() {
        *stored = Some("process job failed inside the active security boundary".into());
    }
    let _ = detail;
    record.changed.notify_waiters();
}

pub(crate) fn verification_limits(runner: &VerificationRunnerId) -> ManagedProcessLimits {
    if runner.project_id().is_none() {
        ManagedProcessLimits::for_fixed_rust_verification()
            .expect("fixed Rust verification limits are valid")
    } else {
        // Owner-approved project commands retain their previous narrower bound.
        ManagedProcessLimits::for_local_model(0).expect("project verification limits are valid")
    }
}

fn disclosed_chunk(
    redactor: &SecretRedactor,
    tail: &mut String,
    text: &str,
    overlap: usize,
) -> String {
    let combined = format!("{tail}{text}");
    let rendered = redacted_process_text(redactor, &combined);
    let safe_len = rendered.len().saturating_sub(overlap);
    let split = rendered
        .char_indices()
        .map(|(offset, _)| offset)
        .take_while(|offset| *offset <= safe_len)
        .last()
        .unwrap_or(0);
    let (safe, pending) = rendered.split_at(split);
    *tail = pending.to_owned();
    safe.to_owned()
}

fn redacted_process_text(redactor: &SecretRedactor, text: &str) -> String {
    let mut rendered = redactor.redact(text);
    for marker in ["token=", "secret=", "password=", "api_key="] {
        let mut offset = 0;
        while let Some(found) = rendered[offset..].find(marker) {
            let start = offset + found + marker.len();
            let end = rendered[start..]
                .find(|character: char| character.is_whitespace() || matches!(character, ',' | ';'))
                .map(|value| start + value)
                .unwrap_or(rendered.len());
            if end > start {
                rendered.replace_range(start..end, "[REDACTED]");
                offset = start + "[REDACTED]".len();
            } else {
                offset = end.saturating_add(1);
            }
            if offset >= rendered.len() {
                break;
            }
        }
    }
    rendered
}

#[cfg(test)]
mod tests {
    use argentum_security::SecretValue;

    use super::*;

    #[test]
    fn stream_buffer_is_repeatable_and_reports_eviction() {
        let mut buffer = StreamBuffer::default();
        buffer.append("first");
        let first = buffer
            .read(ProcessStream::Stdout, 0)
            .expect("first stream read");
        let repeated = buffer
            .read(ProcessStream::Stdout, 0)
            .expect("repeated stream read");
        assert_eq!(first, repeated);
        assert_eq!(first.text, "first");
        assert_eq!(first.next_cursor, 5);

        buffer.append(&"x".repeat(MAX_RETAINED_OUTPUT_PER_STREAM + 1));
        let evicted = buffer
            .read(ProcessStream::Stdout, 0)
            .expect("evicted stream read");
        assert!(evicted.gap);
        assert_eq!(evicted.from_cursor, evicted.earliest_cursor);
        assert!(evicted.available);
        assert!(evicted.text.len() <= MAX_RETAINED_OUTPUT_PER_STREAM);
        assert!(matches!(
            buffer.read(ProcessStream::Stdout, evicted.next_cursor + 1),
            Err(ProcessJobError::InvalidCursor)
        ));
    }

    #[test]
    fn process_text_redacts_secret_and_key_value_canaries() {
        let redactor = SecretRedactor::new([SecretValue::new("split-secret")]);
        let rendered = redacted_process_text(
            &redactor,
            "token=split-secret password=visible api_key=another-secret",
        );
        assert!(!rendered.contains("split-secret"));
        assert!(!rendered.contains("visible"));
        assert!(!rendered.contains("another-secret"));
        assert!(rendered.contains("token=[REDACTED]"));
        assert!(rendered.contains("password=[REDACTED]"));
        assert!(rendered.contains("api_key=[REDACTED]"));
    }

    #[test]
    fn disclosure_redacts_before_clip_across_chunks_and_long_values() {
        let secret = "REGISTERED_BOUNDARY_".repeat(480);
        let redactor = SecretRedactor::new([SecretValue::new(secret.clone())]);
        let overlap = redactor.max_secret_utf8_bytes().max(64);
        let source = format!(
            "{}\nerror: private output {}\napi_key={}\npublic end",
            "padding".repeat(1600),
            secret,
            "UNREGISTERED_VALUE_".repeat(1200)
        );
        let mut tail = String::new();
        let mut shown = String::new();
        for chunk in source.as_bytes().chunks(733) {
            shown.push_str(&disclosed_chunk(
                &redactor,
                &mut tail,
                std::str::from_utf8(chunk).unwrap(),
                overlap,
            ));
            assert!(tail.len() <= overlap + 4);
        }
        shown.push_str(&redacted_process_text(&redactor, &tail));
        assert!(!shown.contains("REGISTERED_BOUNDARY_"));
        assert!(!shown.contains("UNREGISTERED_VALUE_"));
        assert!(shown.contains("[REDACTED]"));
        assert!(shown.contains("public end"));
        assert_eq!(shown, redacted_process_text(&redactor, &source));
        let empty = SecretRedactor::new(std::iter::empty());
        for split in 1..8 {
            let mut tail = String::new();
            let mut shown = String::new();
            let text = format!("{}api_key=BOUNDARY_CANARY\n", "x".repeat(200));
            for chunk in text.as_bytes().chunks(split) {
                shown.push_str(&disclosed_chunk(
                    &empty,
                    &mut tail,
                    std::str::from_utf8(chunk).unwrap(),
                    64,
                ));
            }
            shown.push_str(&redacted_process_text(&empty, &tail));
            assert!(!shown.contains("BOUNDARY_CANARY"));
        }
        let manager = ProcessJobManager::with_redactor(SecretRedactor::new([SecretValue::new(
            "CAPTURE_END_SECRET_COMPLETE",
        )]));
        let captured = "ordinary output\nCAPTURE_END_SECRET_";
        let shown = manager.disclose_captured_process_text(captured, true);
        assert!(!shown.contains("CAPTURE_END_SECRET_"));
    }

    #[test]
    fn poll_selection_tracks_cursors_and_is_omitted_for_terminal_jobs() {
        let stream = |next_cursor| ProcessJobStream {
            stream: ProcessStream::Stdout,
            from_cursor: 0,
            to_cursor: next_cursor,
            earliest_cursor: 0,
            next_cursor,
            available: next_cursor > 0,
            gap: false,
            complete: false,
            text: String::new(),
        };
        let mut snapshot = ProcessJobSnapshot {
            job_id: "job-id".into(),
            state: ProcessJobState::Running,
            runner: argentum_domain::VerificationRunnerId::CargoTestLocked,
            exit_code: None,
            stdout: stream(12),
            stderr: stream(7),
            stdout_bytes: 12,
            stderr_bytes: 7,
            stdout_truncated: false,
            stderr_truncated: false,
            verification: None,
            detail: None,
        };
        assert_eq!(
            snapshot.poll_selection(),
            Some(PollSelection {
                job_id: "job-id".into(),
                stdout_cursor: 12,
                stderr_cursor: 7,
                wait_ms: POLL_WAIT_MS,
            })
        );
        snapshot.state = ProcessJobState::Exited;
        assert_eq!(snapshot.poll_selection(), None);
    }
}
