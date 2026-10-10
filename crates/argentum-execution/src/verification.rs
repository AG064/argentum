use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::time::Duration;

use argentum_domain::{
    goal_supervision::{VerificationInputScope, VerificationInputSnapshot},
    now, RunId, Sha256Digest, VerificationEvidence, VerificationRunnerId,
    VerificationRunnerInspection, VerificationState,
};
use argentum_workspaces::{WorkspaceError, WorkspaceManager, WorkspaceRevisionLimits};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use thiserror::Error;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use crate::executable::resolve_fixed_executable;
use crate::{ManagedProcessExit, ProcessError, ProcessRunner, ProcessSpec, ProcessState};

pub const VERIFICATION_REGISTRY_VERSION: u16 = 1;
const MAX_VERIFICATION_BATCH: usize = 4;
const MAX_PROJECT_CONFIG_BYTES: usize = 128 * 1024;
const MAX_PROJECT_RUNNERS: usize = 16;
const MAX_PROJECT_ID_BYTES: usize = 64;
const MAX_PROJECT_LABEL_BYTES: usize = 160;
const MAX_PROJECT_CWD_BYTES: usize = 256;
const MAX_PROJECT_TIMEOUT: Duration = Duration::from_secs(600);
const MAX_PROJECT_OUTPUT: usize = 1024 * 1024;
const PROJECT_CONFIG_RELATIVE_PATH: &str = ".agents/verification.yaml";

#[derive(Debug, Error)]
pub enum VerificationError {
    #[error("verification batch must contain at least one runner")]
    EmptyBatch,
    #[error("verification batch is limited to four runners")]
    BatchLimit,
    #[error("verification batch contains duplicate runner: {0}")]
    DuplicateRunner(String),
    #[error("verification runner is not registered: {0}")]
    UnknownRunner(String),
    #[error("project verification configuration is invalid: {0}")]
    Configuration(String),
    #[error("project verification requires an exact owner approval")]
    ProjectApprovalRequired,
    #[error(transparent)]
    Process(#[from] ProcessError),
    #[error(transparent)]
    Workspace(#[from] WorkspaceError),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VerificationRunnerOrigin {
    BuiltIn,
    Project,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerificationRunnerDefinition {
    pub id: VerificationRunnerId,
    pub label: String,
    pub program: String,
    pub args: Vec<String>,
    pub cwd: PathBuf,
    pub timeout: Duration,
    pub output_limit: usize,
    pub origin: VerificationRunnerOrigin,
    pub resolved_program: Option<PathBuf>,
    pub unavailable_reason: Option<String>,
}

#[derive(Debug, Clone)]
pub struct VerificationRegistry {
    version: u16,
    runners: BTreeMap<VerificationRunnerId, VerificationRunnerDefinition>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ProjectVerificationConfig {
    version: u16,
    runners: Vec<ProjectVerificationInput>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ProjectVerificationInput {
    id: String,
    label: String,
    program: String,
    #[serde(default)]
    args: Vec<String>,
    #[serde(default = "default_project_cwd")]
    cwd: String,
    timeout_ms: u64,
    output_limit: usize,
}

fn default_project_cwd() -> String {
    ".".into()
}

impl VerificationRegistry {
    pub fn built_in() -> Self {
        let runners = [
            definition(
                VerificationRunnerId::CargoFmtCheck,
                "Cargo format check",
                &["fmt", "--all", "--", "--check"],
            ),
            definition(
                VerificationRunnerId::CargoCheckLocked,
                "Cargo workspace check",
                &["check", "--workspace", "--locked", "--offline", "-j", "1"],
            ),
            definition(
                VerificationRunnerId::CargoTestLocked,
                "Cargo workspace tests",
                &["test", "--workspace", "--locked", "--offline", "-j", "1"],
            ),
            definition(
                VerificationRunnerId::CargoClippyStrict,
                "Cargo Clippy strict",
                &[
                    "clippy",
                    "--workspace",
                    "--all-targets",
                    "--locked",
                    "--offline",
                    "-j",
                    "1",
                    "--",
                    "-D",
                    "warnings",
                ],
            ),
        ]
        .into_iter()
        .map(|runner| (runner.id.clone(), runner))
        .collect();
        Self {
            version: VERIFICATION_REGISTRY_VERSION,
            runners,
        }
    }

    pub fn for_workspace(workspace: &Path) -> Result<Self, VerificationError> {
        let mut registry = Self::built_in();
        let path = workspace.join(PROJECT_CONFIG_RELATIVE_PATH);
        if !path.exists() {
            return Ok(registry);
        }
        let metadata = std::fs::symlink_metadata(&path)
            .map_err(|error| VerificationError::Configuration(error.to_string()))?;
        if metadata.file_type().is_symlink() || !metadata.is_file() {
            return Err(VerificationError::Configuration(
                "project verification configuration must be a regular file".into(),
            ));
        }
        if metadata.len() > MAX_PROJECT_CONFIG_BYTES as u64 {
            return Err(VerificationError::Configuration(
                "project verification configuration exceeds its byte limit".into(),
            ));
        }
        let bytes = std::fs::read(&path)
            .map_err(|error| VerificationError::Configuration(error.to_string()))?;
        let config: ProjectVerificationConfig = serde_yaml::from_slice(&bytes)
            .map_err(|error| VerificationError::Configuration(error.to_string()))?;
        if config.version != 1 {
            return Err(VerificationError::Configuration(
                "unsupported project verification configuration version".into(),
            ));
        }
        if config.runners.is_empty() || config.runners.len() > MAX_PROJECT_RUNNERS {
            return Err(VerificationError::Configuration(
                "project verification runner count is outside its bound".into(),
            ));
        }
        let root = std::fs::canonicalize(workspace)
            .map_err(|error| VerificationError::Configuration(error.to_string()))?;
        let mut ids = BTreeSet::new();
        for input in config.runners {
            validate_project_id(&input.id)?;
            if !ids.insert(input.id.clone()) {
                return Err(VerificationError::Configuration(
                    "project verification runner IDs must be unique".into(),
                ));
            }
            if is_builtin_name(&input.id) {
                return Err(VerificationError::Configuration(
                    "project verification runner cannot shadow a built-in ID".into(),
                ));
            }
            validate_project_text(&input.label, MAX_PROJECT_LABEL_BYTES, "label")?;
            if input.program.is_empty() || input.program.contains(['/', '\\']) {
                return Err(VerificationError::Configuration(
                    "project verification executable must be a bare name".into(),
                ));
            }
            if input.cwd.len() > MAX_PROJECT_CWD_BYTES
                || Path::new(&input.cwd).is_absolute()
                || input.cwd.split(['/', '\\']).any(|part| part == "..")
            {
                return Err(VerificationError::Configuration(
                    "project verification working directory is invalid".into(),
                ));
            }
            let cwd = std::fs::canonicalize(root.join(&input.cwd)).map_err(|_| {
                VerificationError::Configuration(format!(
                    "project verification working directory is unavailable: {}",
                    input.cwd
                ))
            })?;
            if !path_is_within(&cwd, &root) || !cwd.is_dir() {
                return Err(VerificationError::Configuration(
                    "project verification working directory escapes the workspace".into(),
                ));
            }
            if input.args.len() > 128
                || input.args.iter().map(String::len).sum::<usize>() > 64 * 1024
            {
                return Err(VerificationError::Configuration(
                    "project verification arguments exceed their bound".into(),
                ));
            }
            let timeout = Duration::from_millis(input.timeout_ms);
            if timeout.is_zero() || timeout > MAX_PROJECT_TIMEOUT {
                return Err(VerificationError::Configuration(
                    "project verification timeout is outside its bound".into(),
                ));
            }
            if input.output_limit == 0 || input.output_limit > MAX_PROJECT_OUTPUT {
                return Err(VerificationError::Configuration(
                    "project verification output limit is outside its bound".into(),
                ));
            }
            for argument in &input.args {
                argentum_security::validate_process_argument(argument).map_err(|_| {
                    VerificationError::Configuration(
                        "project verification argument is unsafe".into(),
                    )
                })?;
            }
            let resolved_program = resolve_fixed_executable(&input.program, &[&root]);
            let id = VerificationRunnerId::Project(input.id);
            registry.runners.insert(
                id.clone(),
                VerificationRunnerDefinition {
                    id,
                    label: input.label,
                    program: input.program.clone(),
                    args: input.args,
                    cwd,
                    timeout,
                    output_limit: input.output_limit,
                    origin: VerificationRunnerOrigin::Project,
                    unavailable_reason: resolved_program.is_none().then(|| {
                        format!(
                            "executable '{}' is unavailable on the trusted PATH",
                            input.program
                        )
                    }),
                    resolved_program,
                },
            );
        }
        Ok(registry)
    }

    #[cfg(test)]
    fn for_tests(runners: Vec<VerificationRunnerDefinition>) -> Self {
        Self {
            version: VERIFICATION_REGISTRY_VERSION,
            runners: runners
                .into_iter()
                .map(|runner| (runner.id.clone(), runner))
                .collect(),
        }
    }

    pub fn version(&self) -> u16 {
        self.version
    }

    pub fn runners(&self) -> Vec<VerificationRunnerDefinition> {
        self.runners.values().cloned().collect()
    }

    pub fn inspect(&self) -> Vec<VerificationRunnerInspection> {
        self.runners
            .values()
            .map(|definition| VerificationRunnerInspection {
                id: definition.id.id(),
                origin: match definition.origin {
                    VerificationRunnerOrigin::BuiltIn => "built_in".into(),
                    VerificationRunnerOrigin::Project => "project".into(),
                },
                available: definition.unavailable_reason.is_none(),
                approval_required: matches!(definition.origin, VerificationRunnerOrigin::Project),
                reason: definition.unavailable_reason.clone(),
                definition_sha256: self.definition_sha256(&definition.id).ok(),
            })
            .collect()
    }

    pub fn get(&self, id: &VerificationRunnerId) -> Option<&VerificationRunnerDefinition> {
        self.runners.get(id)
    }

    pub fn definition_sha256(
        &self,
        id: &VerificationRunnerId,
    ) -> Result<Sha256Digest, VerificationError> {
        let definition = self
            .get(id)
            .ok_or_else(|| VerificationError::UnknownRunner(id.id()))?;
        let scope = verification_input_scope();
        let mut digest = Sha256::new();
        digest.update(b"argentum-verification-definition-v1\0");
        hash_contract_field(&mut digest, &self.version.to_le_bytes());
        hash_contract_field(&mut digest, id.id().as_bytes());
        hash_contract_field(&mut digest, definition.program.as_bytes());
        hash_contract_field(&mut digest, definition.cwd.to_string_lossy().as_bytes());
        if let Some(resolved) = &definition.resolved_program {
            hash_contract_field(&mut digest, resolved.to_string_lossy().as_bytes());
        }
        hash_contract_field(
            &mut digest,
            &u64::try_from(definition.args.len())
                .unwrap_or(u64::MAX)
                .to_le_bytes(),
        );
        for argument in &definition.args {
            hash_contract_field(&mut digest, argument.as_bytes());
        }
        hash_contract_field(
            &mut digest,
            &u64::try_from(definition.timeout.as_millis())
                .unwrap_or(u64::MAX)
                .to_le_bytes(),
        );
        hash_contract_field(
            &mut digest,
            &u64::try_from(definition.output_limit)
                .unwrap_or(u64::MAX)
                .to_le_bytes(),
        );
        hash_contract_field(&mut digest, scope.method.as_bytes());
        for exclusion in &scope.excluded_components {
            hash_contract_field(&mut digest, exclusion.as_bytes());
        }
        for value in [scope.max_files, scope.max_total_bytes, scope.max_file_bytes] {
            hash_contract_field(&mut digest, &value.to_le_bytes());
        }
        if definition.origin == VerificationRunnerOrigin::BuiltIn {
            // Resource-profile changes invalidate old criteria and receipts just
            // like an argument or input-scope change. They do not grant authority.
            let limits = crate::jobs::verification_limits(id);
            hash_contract_field(&mut digest, b"fixed-rust-async-process-limits-v1");
            hash_contract_field(&mut digest, &limits.active_process_limit().to_le_bytes());
            hash_contract_field(&mut digest, &limits.cpu_rate_percent().to_le_bytes());
            hash_contract_field(
                &mut digest,
                &u64::try_from(limits.output_hash_limit())
                    .unwrap_or(u64::MAX)
                    .to_le_bytes(),
            );
            hash_contract_field(&mut digest, &limits.process_memory_bytes().to_le_bytes());
            hash_contract_field(&mut digest, &limits.job_memory_bytes().to_le_bytes());
        }
        Ok(Sha256Digest(format!("{:x}", digest.finalize())))
    }

    pub fn validate_batch(
        &self,
        runners: &[VerificationRunnerId],
    ) -> Result<(), VerificationError> {
        if runners.is_empty() {
            return Err(VerificationError::EmptyBatch);
        }
        if runners.len() > MAX_VERIFICATION_BATCH {
            return Err(VerificationError::BatchLimit);
        }
        let mut seen = BTreeSet::new();
        for runner in runners {
            if !self.runners.contains_key(runner) {
                return Err(VerificationError::UnknownRunner(runner.id()));
            }
            if !seen.insert(runner.clone()) {
                return Err(VerificationError::DuplicateRunner(runner.id()));
            }
        }
        Ok(())
    }
}

pub fn project_verification_binding(
    workspace: &WorkspaceManager,
    runners: &[VerificationRunnerId],
) -> Result<Sha256Digest, VerificationError> {
    let registry = if runners.iter().any(|runner| runner.project_id().is_some()) {
        VerificationRegistry::for_workspace(workspace.root())?
    } else {
        VerificationRegistry::built_in()
    };
    registry.validate_batch(runners)?;
    let mut digest = Sha256::new();
    digest.update(b"argentum-project-verification-action-v1\0");
    for runner in runners {
        hash_contract_field(&mut digest, runner.id().as_bytes());
        hash_contract_field(
            &mut digest,
            registry.definition_sha256(runner)?.0.as_bytes(),
        );
    }
    Ok(Sha256Digest(format!("{:x}", digest.finalize())))
}

impl Default for VerificationRegistry {
    fn default() -> Self {
        Self::built_in()
    }
}

fn definition(
    id: VerificationRunnerId,
    label: &str,
    args: &[&str],
) -> VerificationRunnerDefinition {
    VerificationRunnerDefinition {
        id,
        label: label.into(),
        program: "cargo".into(),
        args: args.iter().map(|value| (*value).to_owned()).collect(),
        cwd: PathBuf::from("."),
        timeout: Duration::from_secs(600),
        output_limit: 512 * 1024,
        origin: VerificationRunnerOrigin::BuiltIn,
        resolved_program: None,
        unavailable_reason: None,
    }
}

fn validate_project_id(id: &str) -> Result<(), VerificationError> {
    if id.is_empty()
        || id.len() > MAX_PROJECT_ID_BYTES
        || !id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
    {
        return Err(VerificationError::Configuration(
            "project verification runner ID is invalid".into(),
        ));
    }
    Ok(())
}

fn validate_project_text(
    value: &str,
    max_bytes: usize,
    field: &'static str,
) -> Result<(), VerificationError> {
    if value.is_empty() || value.len() > max_bytes || value.chars().any(char::is_control) {
        return Err(VerificationError::Configuration(format!(
            "project verification {field} is invalid"
        )));
    }
    Ok(())
}

fn is_builtin_name(id: &str) -> bool {
    matches!(
        id,
        "cargo-fmt-check" | "cargo-check-locked" | "cargo-test-locked" | "cargo-clippy-strict"
    )
}

fn path_is_within(path: &Path, root: &Path) -> bool {
    #[cfg(windows)]
    {
        path.to_string_lossy()
            .replace('/', "\\")
            .to_ascii_lowercase()
            .starts_with(&format!(
                "{}\\",
                root.to_string_lossy()
                    .replace('/', "\\")
                    .trim_end_matches('\\')
                    .to_ascii_lowercase()
            ))
            || path == root
    }
    #[cfg(not(windows))]
    {
        path == root || path.starts_with(root)
    }
}

#[derive(Debug, Clone)]
pub struct VerificationRunner {
    workspace: WorkspaceManager,
    registry: VerificationRegistry,
    process: ProcessRunner,
}

#[derive(Debug, Clone)]
pub struct VerificationJob {
    pub run_id: RunId,
    pub runner: VerificationRunnerId,
    pub registry_version: u16,
    pub program: String,
    pub args: Vec<String>,
    pub spec: ProcessSpec,
    pub revision_before: VerificationInputSnapshot,
    pub started_at: time::OffsetDateTime,
    pub workspace_key: String,
    pub definition_sha256: Sha256Digest,
    pub input_scope: VerificationInputScope,
}

/// Transient compiler output for the current authorized caller. Only `evidence`
/// is durable; these bodies never become verification records or debug text.
pub struct VerificationRunOutput {
    pub evidence: VerificationEvidence,
    pub stdout: Option<String>,
    pub stderr: Option<String>,
}

impl std::fmt::Debug for VerificationRunOutput {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("VerificationRunOutput")
            .field("id", &self.evidence.id)
            .field("runner", &self.evidence.runner)
            .field("state", &self.evidence.state)
            .field(
                "stdout_captured_bytes",
                &self.stdout.as_ref().map(String::len),
            )
            .field(
                "stderr_captured_bytes",
                &self.stderr.as_ref().map(String::len),
            )
            .finish()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VerificationJobTermination {
    Completed,
    Cancelled,
    TimedOut,
}

impl VerificationRunner {
    pub fn new(workspace: WorkspaceManager, registry: VerificationRegistry) -> Self {
        Self {
            workspace,
            registry,
            process: ProcessRunner,
        }
    }

    pub fn registry(&self) -> &VerificationRegistry {
        &self.registry
    }

    pub fn prepare_job(
        &self,
        run_id: RunId,
        runner_id: VerificationRunnerId,
    ) -> Result<VerificationJob, VerificationError> {
        let definition = self
            .registry
            .get(&runner_id)
            .ok_or_else(|| VerificationError::UnknownRunner(runner_id.id()))?;
        let revision = self.workspace.revision_digest(default_revision_limits())?;
        let program_path = definition
            .resolved_program
            .clone()
            .or_else(|| resolve_fixed_executable(&definition.program, &[self.workspace.root()]))
            .ok_or_else(|| ProcessError::FixedExecutableUnavailable(definition.program.clone()))?;
        let program = program_path.to_string_lossy().into_owned();
        let cwd = effective_working_directory(&self.workspace, definition);
        let spec = ProcessSpec::argv(program.clone(), definition.args.clone(), cwd)?
            .with_timeout(definition.timeout)?
            .with_output_limit(definition.output_limit)?;
        Ok(VerificationJob {
            run_id,
            runner: runner_id.clone(),
            registry_version: self.registry.version,
            program,
            args: definition.args.clone(),
            spec,
            revision_before: revision_snapshot(&revision),
            started_at: now(),
            workspace_key: workspace_key(self.workspace.root()),
            definition_sha256: self.registry.definition_sha256(&runner_id)?,
            input_scope: verification_input_scope(),
        })
    }

    pub fn settle_job(
        &self,
        job: &VerificationJob,
        exit: &ManagedProcessExit,
        termination: VerificationJobTermination,
    ) -> Result<VerificationEvidence, VerificationError> {
        let revision_after = self.workspace.revision_digest(default_revision_limits())?;
        let state = if job.revision_before.digest != revision_after.digest {
            VerificationState::Stale
        } else {
            match termination {
                VerificationJobTermination::Completed if exit.exit_code == Some(0) => {
                    VerificationState::Passed
                }
                VerificationJobTermination::Completed => VerificationState::Failed,
                VerificationJobTermination::Cancelled => VerificationState::Cancelled,
                VerificationJobTermination::TimedOut => VerificationState::TimedOut,
            }
        };
        Ok(VerificationEvidence {
            id: Uuid::new_v4(),
            run_id: job.run_id,
            runner: job.runner.clone(),
            registry_version: job.registry_version,
            program: job.program.clone(),
            args: job.args.clone(),
            state,
            started_at: job.started_at,
            duration_ms: exit.duration_ms,
            exit_code: exit.exit_code,
            revision_before: job.revision_before.digest.clone(),
            revision_after: revision_after.digest.clone(),
            stdout_sha256: exit.stdout_sha256.clone(),
            stderr_sha256: exit.stderr_sha256.clone(),
            stdout_bytes: exit.stdout_bytes,
            stderr_bytes: exit.stderr_bytes,
            stdout_truncated: exit.stdout_truncated,
            stderr_truncated: exit.stderr_truncated,
            workspace_key: job.workspace_key.clone(),
            definition_sha256: Some(job.definition_sha256.clone()),
            input_scope: Some(job.input_scope.clone()),
            input_before: Some(job.revision_before.clone()),
            input_after: Some(revision_snapshot(&revision_after)),
        })
    }

    pub async fn run_batch(
        &self,
        run_id: RunId,
        runners: &[VerificationRunnerId],
        cancellation: CancellationToken,
    ) -> Result<Vec<VerificationEvidence>, VerificationError> {
        Ok(self
            .run_batch_with_output(run_id, runners, cancellation)
            .await?
            .into_iter()
            .map(|result| result.evidence)
            .collect())
    }

    pub async fn run_batch_with_output(
        &self,
        run_id: RunId,
        runners: &[VerificationRunnerId],
        cancellation: CancellationToken,
    ) -> Result<Vec<VerificationRunOutput>, VerificationError> {
        self.registry.validate_batch(runners)?;
        let mut evidence = Vec::with_capacity(runners.len());
        for runner_id in runners {
            let definition = self
                .registry
                .get(runner_id)
                .ok_or_else(|| VerificationError::UnknownRunner(runner_id.id()))?;
            let revision_before = self.workspace.revision_digest(default_revision_limits())?;
            let started_at = now();
            let program_path = definition
                .resolved_program
                .clone()
                .or_else(|| resolve_fixed_executable(&definition.program, &[self.workspace.root()]))
                .ok_or_else(|| {
                    ProcessError::FixedExecutableUnavailable(definition.program.clone())
                })?;
            let program = program_path.to_string_lossy().into_owned();
            let cwd = effective_working_directory(&self.workspace, definition);
            let spec = ProcessSpec::argv(program.clone(), definition.args.clone(), cwd)?
                .with_timeout(definition.timeout)?
                .with_output_limit(definition.output_limit)?;
            let process_result = self.process.run(spec, cancellation.clone()).await;
            let revision_after = self.workspace.revision_digest(default_revision_limits())?;
            let workspace_key = workspace_key(self.workspace.root());
            let definition_sha256 = self.registry.definition_sha256(runner_id)?;
            let input_scope = verification_input_scope();
            let input_before = revision_snapshot(&revision_before);
            let input_after = revision_snapshot(&revision_after);
            let mut stdout = None;
            let mut stderr = None;
            let record = match process_result {
                Ok(result) => {
                    stdout = Some(result.stdout);
                    stderr = Some(result.stderr);
                    let state = if revision_before != revision_after {
                        VerificationState::Stale
                    } else {
                        match result.state {
                            ProcessState::Exited if result.exit_code == Some(0) => {
                                VerificationState::Passed
                            }
                            ProcessState::Exited => VerificationState::Failed,
                            ProcessState::TimedOut => VerificationState::TimedOut,
                            ProcessState::Cancelled => VerificationState::Cancelled,
                        }
                    };
                    VerificationEvidence {
                        id: Uuid::new_v4(),
                        run_id,
                        runner: runner_id.clone(),
                        registry_version: self.registry.version,
                        program: program.clone(),
                        args: definition.args.clone(),
                        state,
                        started_at,
                        duration_ms: result.duration_ms,
                        exit_code: result.exit_code,
                        revision_before: revision_before.digest.clone(),
                        revision_after: revision_after.digest.clone(),
                        stdout_sha256: result.stdout_sha256,
                        stderr_sha256: result.stderr_sha256,
                        stdout_bytes: result.stdout_bytes,
                        stderr_bytes: result.stderr_bytes,
                        stdout_truncated: result.stdout_truncated,
                        stderr_truncated: result.stderr_truncated,
                        workspace_key: workspace_key.clone(),
                        definition_sha256: Some(definition_sha256.clone()),
                        input_scope: Some(input_scope.clone()),
                        input_before: Some(input_before.clone()),
                        input_after: Some(input_after.clone()),
                    }
                }
                Err(ProcessError::Spawn(_) | ProcessError::Containment(_)) => {
                    VerificationEvidence {
                        id: Uuid::new_v4(),
                        run_id,
                        runner: runner_id.clone(),
                        registry_version: self.registry.version,
                        program: program.clone(),
                        args: definition.args.clone(),
                        state: VerificationState::SpawnFailed,
                        started_at,
                        duration_ms: 0,
                        exit_code: None,
                        revision_before: revision_before.digest.clone(),
                        revision_after: revision_after.digest.clone(),
                        stdout_sha256: empty_sha256(),
                        stderr_sha256: empty_sha256(),
                        stdout_bytes: 0,
                        stderr_bytes: 0,
                        stdout_truncated: false,
                        stderr_truncated: false,
                        workspace_key: workspace_key.clone(),
                        definition_sha256: Some(definition_sha256.clone()),
                        input_scope: Some(input_scope.clone()),
                        input_before: Some(input_before.clone()),
                        input_after: Some(input_after.clone()),
                    }
                }
                Err(error_value) => return Err(error_value.into()),
            };
            let terminal_cancellation = record.state == VerificationState::Cancelled;
            evidence.push(VerificationRunOutput {
                evidence: record,
                stdout,
                stderr,
            });
            if terminal_cancellation {
                break;
            }
        }
        Ok(evidence)
    }
}

fn effective_working_directory(
    workspace: &WorkspaceManager,
    definition: &VerificationRunnerDefinition,
) -> PathBuf {
    match definition.origin {
        VerificationRunnerOrigin::BuiltIn => workspace.root().to_path_buf(),
        VerificationRunnerOrigin::Project => definition.cwd.clone(),
    }
}

pub const fn default_revision_limits() -> WorkspaceRevisionLimits {
    WorkspaceRevisionLimits {
        max_files: 16_384,
        max_total_bytes: 64 * 1024 * 1024,
        max_file_bytes: 16 * 1024 * 1024,
    }
}

fn hash_contract_field(digest: &mut Sha256, value: &[u8]) {
    digest.update(u64::try_from(value.len()).unwrap_or(u64::MAX).to_le_bytes());
    digest.update(value);
}

pub fn verification_input_scope() -> VerificationInputScope {
    let limits = default_revision_limits();
    VerificationInputScope {
        method: "workspace_revision_v1".into(),
        excluded_components: vec![".git".into(), "target".into()],
        max_files: limits.max_files.try_into().unwrap_or(u64::MAX),
        max_total_bytes: limits.max_total_bytes,
        max_file_bytes: limits.max_file_bytes,
    }
}

fn revision_snapshot(
    revision: &argentum_workspaces::WorkspaceRevision,
) -> VerificationInputSnapshot {
    VerificationInputSnapshot {
        method: "workspace_revision_v1".into(),
        digest: revision.digest.clone(),
        file_count: revision.file_count,
        total_bytes: revision.total_bytes,
    }
}

fn workspace_key(path: &std::path::Path) -> String {
    let key = path.to_string_lossy().replace('\\', "/");
    #[cfg(target_os = "windows")]
    let key = key.to_ascii_lowercase();
    key
}

fn empty_sha256() -> Sha256Digest {
    Sha256Digest(format!("{:x}", Sha256::digest([])))
}

#[cfg(test)]
mod tests {
    use std::path::Path;
    use std::time::Duration;

    use argentum_domain::{
        process_job_contract::MAX_POLL_CURSOR, VerificationRunnerId, VerificationState,
    };
    use argentum_security::{ApprovalPolicy, CapabilityBroker};
    use argentum_workspaces::WorkspaceManager;
    use tokio_util::sync::CancellationToken;
    use uuid::Uuid;

    use crate::{
        ProcessJobError, ProcessJobManager, ProcessJobOwner, ProcessJobSnapshot, ProcessJobState,
    };

    use super::*;

    fn workspace() -> (tempfile::TempDir, WorkspaceManager) {
        let directory = tempfile::tempdir().expect("workspace");
        std::fs::write(directory.path().join("source.txt"), "stable").expect("source fixture");
        let broker = CapabilityBroker::new(directory.path(), ApprovalPolicy::default())
            .expect("capability broker");
        (directory, WorkspaceManager::new(broker))
    }

    #[cfg(windows)]
    #[tokio::test]
    async fn managed_cargo_verification_serializes_compilation_under_existing_job_limits() {
        for serial in [false, true] {
            let (directory, workspace) = workspace();
            std::fs::create_dir(directory.path().join("src")).unwrap();
            std::fs::create_dir(directory.path().join("tests")).unwrap();
            std::fs::write(directory.path().join("Cargo.toml"), "[package]\nname=\"bounded_verifier_fixture\"\nversion=\"0.1.0\"\nedition=\"2021\"\n").unwrap();
            std::fs::write(directory.path().join("Cargo.lock"), "version = 4\n\n[[package]]\nname = \"bounded_verifier_fixture\"\nversion = \"0.1.0\"\n").unwrap();
            std::fs::write(directory.path().join("src/lib.rs"), "pub fn answer() -> usize { 42 }\n#[test] fn unit() { assert_eq!(answer(), 42); }\n").unwrap();
            for n in 0..4 {
                std::fs::write(directory.path().join(format!("tests/case{n}.rs")), "#[test] fn external() { assert_eq!(bounded_verifier_fixture::answer(), 42); }\n").unwrap();
            }
            let mut registry = VerificationRegistry::built_in();
            let definition = registry
                .runners
                .get_mut(&VerificationRunnerId::CargoTestLocked)
                .unwrap();
            assert!(definition.args.windows(2).any(|a| a == ["-j", "1"]));
            if !serial {
                definition.args.truncate(definition.args.len() - 2);
            }
            let runner = VerificationRunner::new(workspace.clone(), registry);
            let manager = ProcessJobManager::new();
            let owner = job_owner(&manager, &workspace, Uuid::new_v4());
            let id = manager
                .start_verification(
                    owner.clone(),
                    runner,
                    VerificationRunnerId::CargoTestLocked,
                    CancellationToken::new(),
                )
                .unwrap();
            let deadline = tokio::time::Instant::now() + Duration::from_secs(30);
            let (mut out_cursor, mut err_cursor) = (0, 0);
            let mut stderr = String::new();
            let terminal = loop {
                assert!(
                    tokio::time::Instant::now() < deadline,
                    "managed Cargo did not settle"
                );
                let snapshot = manager
                    .poll(
                        &owner,
                        &id,
                        out_cursor,
                        err_cursor,
                        Duration::from_millis(200),
                    )
                    .await
                    .unwrap();
                stderr.push_str(&snapshot.stderr.text);
                if snapshot.state.is_terminal() {
                    break snapshot;
                }
                out_cursor = snapshot.stdout.next_cursor;
                err_cursor = snapshot.stderr.next_cursor;
                tokio::task::yield_now().await;
            };
            println!(
                "serial={serial} exit={:?} stderr={}",
                terminal.exit_code, stderr
            );
            manager.shutdown().await.unwrap();
            if serial {
                assert_eq!(terminal.exit_code, Some(0), "{stderr}");
                assert_eq!(
                    terminal.verification.unwrap().state,
                    VerificationState::Passed
                );
            }
        }
    }

    #[cfg(windows)]
    #[tokio::test]
    async fn managed_clippy_verification_preserves_process_bounds() {
        let (directory, workspace) = workspace();
        std::fs::create_dir(directory.path().join("src")).unwrap();
        std::fs::write(
            directory.path().join("Cargo.toml"),
            "[package]\nname=\"bounded_clippy_fixture\"\nversion=\"0.1.0\"\nedition=\"2021\"\n",
        )
        .unwrap();
        std::fs::write(
            directory.path().join("Cargo.lock"),
            "version = 4\n\n[[package]]\nname = \"bounded_clippy_fixture\"\nversion = \"0.1.0\"\n",
        )
        .unwrap();
        std::fs::write(
            directory.path().join("src/lib.rs"),
            "pub fn answer() -> usize { 42 }\n#[test] fn unit() { assert_eq!(answer(), 42); }\n",
        )
        .unwrap();
        let runner = VerificationRunner::new(workspace.clone(), VerificationRegistry::built_in());
        let manager = ProcessJobManager::new();
        let owner = job_owner(&manager, &workspace, Uuid::new_v4());
        let id = manager
            .start_verification(
                owner.clone(),
                runner,
                VerificationRunnerId::CargoClippyStrict,
                CancellationToken::new(),
            )
            .unwrap();
        let deadline = tokio::time::Instant::now() + Duration::from_secs(30);
        let (mut stdout_cursor, mut stderr_cursor) = (0, 0);
        let mut stderr = String::new();
        let terminal = loop {
            assert!(
                tokio::time::Instant::now() < deadline,
                "Clippy did not settle"
            );
            let snapshot = manager
                .poll(
                    &owner,
                    &id,
                    stdout_cursor,
                    stderr_cursor,
                    Duration::from_millis(200),
                )
                .await
                .unwrap();
            stderr.push_str(&snapshot.stderr.text);
            stdout_cursor = snapshot.stdout.next_cursor;
            stderr_cursor = snapshot.stderr.next_cursor;
            if snapshot.state.is_terminal() {
                break snapshot;
            }
        };
        manager.shutdown().await.unwrap();
        assert_eq!(terminal.exit_code, Some(0), "{stderr}");
        assert_eq!(terminal.state, ProcessJobState::Exited, "{stderr}");
        assert_eq!(
            terminal.verification.unwrap().state,
            VerificationState::Passed
        );
    }

    #[test]
    fn managed_clippy_verification_preserves_process_bounds_for_other_routes() {
        let sidecar = crate::ManagedProcessLimits::for_local_model(0).unwrap();
        let rust = crate::jobs::verification_limits(&VerificationRunnerId::CargoClippyStrict);
        let project =
            crate::jobs::verification_limits(&VerificationRunnerId::Project("custom".into()));
        assert_eq!(rust.active_process_limit(), 8);
        assert_eq!(sidecar.active_process_limit(), 4);
        assert_eq!(project.active_process_limit(), 4);
        for limits in [rust, project] {
            assert_eq!(limits.cpu_rate_percent(), sidecar.cpu_rate_percent());
            assert_eq!(
                limits.process_memory_bytes(),
                sidecar.process_memory_bytes()
            );
            assert_eq!(limits.job_memory_bytes(), sidecar.job_memory_bytes());
            assert_eq!(limits.output_hash_limit(), sidecar.output_hash_limit());
        }
    }

    #[cfg(windows)]
    fn job_owner(
        manager: &ProcessJobManager,
        workspace: &WorkspaceManager,
        run_id: Uuid,
    ) -> ProcessJobOwner {
        ProcessJobOwner {
            host_generation: manager.host_generation(),
            project_id: Uuid::new_v4(),
            workspace_key: workspace_key(workspace.root()),
            session_id: Uuid::new_v4(),
            run_id,
        }
    }

    #[cfg(windows)]
    async fn poll_until_terminal(
        manager: &ProcessJobManager,
        owner: &ProcessJobOwner,
        job_id: &str,
    ) -> ProcessJobSnapshot {
        let mut stdout_cursor = 0;
        let mut stderr_cursor = 0;
        for _ in 0..16 {
            let snapshot = manager
                .poll(
                    owner,
                    job_id,
                    stdout_cursor,
                    stderr_cursor,
                    Duration::from_secs(5),
                )
                .await
                .expect("poll owned verification job");
            if snapshot.state.is_terminal() {
                return snapshot;
            }
            stdout_cursor = snapshot.stdout.next_cursor;
            stderr_cursor = snapshot.stderr.next_cursor;
        }
        panic!("verification job did not settle within the test poll bound");
    }

    #[test]
    fn verification_registry_has_fixed_versioned_rust_commands() {
        let registry = VerificationRegistry::built_in();
        assert_eq!(registry.version(), 1);
        let runners = registry.runners();
        assert_eq!(
            runners
                .iter()
                .map(|runner| runner.id.clone())
                .collect::<Vec<_>>(),
            [
                VerificationRunnerId::CargoFmtCheck,
                VerificationRunnerId::CargoCheckLocked,
                VerificationRunnerId::CargoTestLocked,
                VerificationRunnerId::CargoClippyStrict,
            ]
        );
        assert_eq!(
            registry
                .get(&VerificationRunnerId::CargoFmtCheck)
                .expect("fmt runner")
                .args,
            ["fmt", "--all", "--", "--check"]
        );
        assert_eq!(
            registry
                .get(&VerificationRunnerId::CargoCheckLocked)
                .expect("check runner")
                .args,
            ["check", "--workspace", "--locked", "--offline", "-j", "1"]
        );
        assert_eq!(
            registry
                .get(&VerificationRunnerId::CargoTestLocked)
                .expect("test runner")
                .args,
            ["test", "--workspace", "--locked", "--offline", "-j", "1"]
        );
        assert_eq!(
            registry
                .get(&VerificationRunnerId::CargoClippyStrict)
                .expect("clippy runner")
                .args,
            [
                "clippy",
                "--workspace",
                "--all-targets",
                "--locked",
                "--offline",
                "-j",
                "1",
                "--",
                "-D",
                "warnings",
            ]
        );
    }

    #[cfg(windows)]
    #[test]
    fn built_in_verification_uses_the_selected_workspace_as_cwd() {
        let (_directory, workspace) = workspace();
        let runner = VerificationRunner::new(workspace.clone(), VerificationRegistry::built_in());
        let job = runner
            .prepare_job(Uuid::new_v4(), VerificationRunnerId::CargoTestLocked)
            .expect("built-in verification job");

        assert_eq!(job.spec.cwd(), workspace.root());
    }

    #[test]
    fn verification_registry_rejects_empty_duplicate_and_oversized_batches() {
        let registry = VerificationRegistry::built_in();
        assert!(matches!(
            registry.validate_batch(&[]),
            Err(VerificationError::EmptyBatch)
        ));
        assert!(matches!(
            registry.validate_batch(&[
                VerificationRunnerId::CargoFmtCheck,
                VerificationRunnerId::CargoFmtCheck,
            ]),
            Err(VerificationError::DuplicateRunner(_))
        ));
        assert!(matches!(
            registry.validate_batch(&[
                VerificationRunnerId::CargoFmtCheck,
                VerificationRunnerId::CargoCheckLocked,
                VerificationRunnerId::CargoTestLocked,
                VerificationRunnerId::CargoClippyStrict,
                VerificationRunnerId::CargoFmtCheck,
            ]),
            Err(VerificationError::BatchLimit)
        ));
    }

    #[test]
    fn project_registry_is_bounded_and_binds_the_resolved_executable() {
        let directory = tempfile::tempdir().expect("project workspace");
        std::fs::create_dir_all(directory.path().join(".agents")).expect("agents directory");
        std::fs::write(
            directory.path().join(PROJECT_CONFIG_RELATIVE_PATH),
            "version: 1\nrunners:\n  - id: unit-check\n    label: Project unit check\n    program: cargo\n    args: [check, --offline]\n    cwd: .\n    timeout_ms: 30000\n    output_limit: 65536\n",
        )
        .expect("project verification config");

        let registry = VerificationRegistry::for_workspace(directory.path())
            .expect("project verification registry");
        let id = VerificationRunnerId::Project("unit-check".into());
        let definition = registry.get(&id).expect("project runner");
        assert_eq!(definition.origin, VerificationRunnerOrigin::Project);
        assert!(definition.unavailable_reason.is_none());
        let first = registry
            .definition_sha256(&id)
            .expect("project definition digest");

        std::fs::write(
            directory.path().join(PROJECT_CONFIG_RELATIVE_PATH),
            "version: 1\nrunners:\n  - id: unit-check\n    label: Project unit check\n    program: cargo\n    args: [test, --offline]\n    cwd: .\n    timeout_ms: 30000\n    output_limit: 65536\n",
        )
        .expect("changed project verification config");
        let changed = VerificationRegistry::for_workspace(directory.path())
            .expect("changed project verification registry")
            .definition_sha256(&id)
            .expect("changed project definition digest");
        assert_ne!(first, changed);
    }

    #[test]
    fn project_registry_rejects_shadowing_and_workspace_escape() {
        let directory = tempfile::tempdir().expect("project workspace");
        std::fs::create_dir_all(directory.path().join(".agents")).expect("agents directory");
        let path = directory.path().join(PROJECT_CONFIG_RELATIVE_PATH);
        std::fs::write(
            &path,
            "version: 1\nrunners:\n  - id: cargo-test-locked\n    label: Shadow\n    program: cargo\n    args: [test]\n    cwd: .\n    timeout_ms: 1000\n    output_limit: 1024\n",
        )
        .expect("shadow config");
        assert!(matches!(
            VerificationRegistry::for_workspace(directory.path()),
            Err(VerificationError::Configuration(_))
        ));

        std::fs::write(
            &path,
            "version: 1\nrunners:\n  - id: escape\n    label: Escape\n    program: cargo\n    args: [check]\n    cwd: ../\n    timeout_ms: 1000\n    output_limit: 1024\n",
        )
        .expect("escape config");
        assert!(matches!(
            VerificationRegistry::for_workspace(directory.path()),
            Err(VerificationError::Configuration(_))
        ));
    }

    #[cfg(windows)]
    fn test_definition(id: VerificationRunnerId, script: &str) -> VerificationRunnerDefinition {
        VerificationRunnerDefinition {
            id,
            label: "Test verification".into(),
            program: "powershell.exe".into(),
            args: vec![
                "-NoLogo".into(),
                "-NoProfile".into(),
                "-NonInteractive".into(),
                "-Command".into(),
                script.into(),
            ],
            cwd: PathBuf::from("."),
            timeout: Duration::from_secs(5),
            output_limit: 1024,
            origin: VerificationRunnerOrigin::BuiltIn,
            resolved_program: None,
            unavailable_reason: None,
        }
    }

    #[cfg(windows)]
    #[tokio::test]
    async fn verification_run_records_pass_and_nonzero_failure_evidence() {
        let (_directory, workspace) = workspace();
        let registry = VerificationRegistry::for_tests(vec![
            test_definition(
                VerificationRunnerId::CargoFmtCheck,
                "[Console]::Out.Write('pass'); exit 0",
            ),
            test_definition(
                VerificationRunnerId::CargoCheckLocked,
                "[Console]::Error.Write('fail'); exit 9",
            ),
        ]);
        let runner = VerificationRunner::new(workspace, registry);
        let evidence = runner
            .run_batch(
                Uuid::new_v4(),
                &[
                    VerificationRunnerId::CargoFmtCheck,
                    VerificationRunnerId::CargoCheckLocked,
                ],
                CancellationToken::new(),
            )
            .await
            .expect("verification batch");

        assert_eq!(evidence.len(), 2);
        assert_eq!(evidence[0].state, VerificationState::Passed);
        assert_eq!(evidence[0].exit_code, Some(0));
        assert_eq!(evidence[1].state, VerificationState::Failed);
        assert_eq!(evidence[1].exit_code, Some(9));
        assert!(Path::new(&evidence[0].program).is_absolute());
        assert!(Path::new(&evidence[0].program).is_file());
        assert!(!evidence[0].args.is_empty());
        assert_eq!(evidence[0].revision_before, evidence[0].revision_after);
    }

    #[cfg(windows)]
    #[tokio::test]
    async fn verification_run_marks_success_stale_when_sources_change() {
        let (directory, workspace) = workspace();
        let changed_path = directory.path().join("source.txt");
        let script = format!(
            "[IO.File]::WriteAllText('{}', 'changed'); exit 0",
            changed_path.to_string_lossy().replace('\'', "''")
        );
        let registry = VerificationRegistry::for_tests(vec![test_definition(
            VerificationRunnerId::CargoFmtCheck,
            &script,
        )]);
        let runner = VerificationRunner::new(workspace.clone(), registry);
        let evidence = runner
            .run_batch(
                Uuid::new_v4(),
                &[VerificationRunnerId::CargoFmtCheck],
                CancellationToken::new(),
            )
            .await
            .expect("verification batch");

        assert_eq!(evidence[0].state, VerificationState::Stale);
        assert_ne!(evidence[0].revision_before, evidence[0].revision_after);
        let current = workspace
            .revision_digest(default_revision_limits())
            .expect("current revision");
        assert!(!evidence[0].is_fresh_for(&current.digest));
    }

    #[cfg(windows)]
    #[tokio::test]
    async fn verification_run_distinguishes_timeout_and_cancellation() {
        let (_directory, workspace) = workspace();
        let mut timeout = test_definition(
            VerificationRunnerId::CargoFmtCheck,
            "Start-Sleep -Seconds 30",
        );
        timeout.timeout = Duration::from_millis(150);
        let runner = VerificationRunner::new(
            workspace.clone(),
            VerificationRegistry::for_tests(vec![timeout]),
        );
        let evidence = runner
            .run_batch(
                Uuid::new_v4(),
                &[VerificationRunnerId::CargoFmtCheck],
                CancellationToken::new(),
            )
            .await
            .expect("timeout batch");
        assert_eq!(evidence[0].state, VerificationState::TimedOut);

        let runner = VerificationRunner::new(
            workspace,
            VerificationRegistry::for_tests(vec![test_definition(
                VerificationRunnerId::CargoFmtCheck,
                "Start-Sleep -Seconds 30",
            )]),
        );
        let token = CancellationToken::new();
        let cancellation = token.clone();
        let task = tokio::spawn(async move {
            runner
                .run_batch(
                    Uuid::new_v4(),
                    &[VerificationRunnerId::CargoFmtCheck],
                    token,
                )
                .await
        });
        tokio::time::sleep(Duration::from_millis(150)).await;
        cancellation.cancel();
        let evidence = task.await.expect("task join").expect("cancel batch");
        assert_eq!(evidence[0].state, VerificationState::Cancelled);
    }

    #[cfg(windows)]
    #[tokio::test]
    async fn process_job_poll_contract_preserves_owner_repeat_and_terminal_rules() {
        let (_directory, workspace) = workspace();
        let registry = VerificationRegistry::for_tests(vec![test_definition(
            VerificationRunnerId::CargoFmtCheck,
            "[Console]::Out.Write('JOB_OUTPUT'); exit 0",
        )]);
        let runner = VerificationRunner::new(workspace.clone(), registry);
        let manager = ProcessJobManager::new();
        let owner = job_owner(&manager, &workspace, Uuid::new_v4());
        let job_id = manager
            .start_verification(
                owner.clone(),
                runner,
                VerificationRunnerId::CargoFmtCheck,
                CancellationToken::new(),
            )
            .expect("start owned job");

        assert!(matches!(
            manager
                .poll(&owner, &job_id, MAX_POLL_CURSOR + 1, 0, Duration::ZERO,)
                .await,
            Err(ProcessJobError::InvalidCursor)
        ));
        let mut foreign = owner.clone();
        foreign.project_id = Uuid::new_v4();
        assert!(matches!(
            manager.poll(&foreign, &job_id, 0, 0, Duration::ZERO).await,
            Err(ProcessJobError::NotFound)
        ));
        let mut stale = owner.clone();
        stale.host_generation = Uuid::new_v4();
        assert!(matches!(
            manager.poll(&stale, &job_id, 0, 0, Duration::ZERO).await,
            Err(ProcessJobError::NotFound)
        ));

        let first = poll_until_terminal(&manager, &owner, &job_id).await;
        assert_eq!(first.state, ProcessJobState::Exited);
        assert_eq!(first.exit_code, Some(0));
        assert_eq!(
            first.verification.as_ref().map(|evidence| evidence.state),
            Some(VerificationState::Passed)
        );
        assert!(first.poll_selection().is_none());
        let repeated = manager
            .poll(
                &owner,
                &job_id,
                first.stdout.from_cursor,
                first.stderr.from_cursor,
                Duration::ZERO,
            )
            .await
            .expect("repeat terminal poll");
        assert_eq!(repeated, first);
        manager.shutdown().await.expect("drain completed job");
    }

    #[cfg(windows)]
    #[tokio::test]
    async fn process_job_poll_reports_real_failure_and_cancellation_without_restart() {
        let (_directory, workspace) = workspace();
        let registry = VerificationRegistry::for_tests(vec![
            test_definition(
                VerificationRunnerId::CargoFmtCheck,
                "[Console]::Error.Write('JOB_FAILURE'); exit 9",
            ),
            test_definition(
                VerificationRunnerId::CargoCheckLocked,
                "Start-Sleep -Seconds 30",
            ),
        ]);
        let runner = VerificationRunner::new(workspace.clone(), registry);
        let manager = ProcessJobManager::new();
        let failed_owner = job_owner(&manager, &workspace, Uuid::new_v4());
        let failed_job = manager
            .start_verification(
                failed_owner.clone(),
                runner.clone(),
                VerificationRunnerId::CargoFmtCheck,
                CancellationToken::new(),
            )
            .expect("start failing job");
        let failed = poll_until_terminal(&manager, &failed_owner, &failed_job).await;
        assert_eq!(failed.state, ProcessJobState::Exited);
        assert_eq!(failed.exit_code, Some(9));
        assert_eq!(
            failed.verification.as_ref().map(|evidence| evidence.state),
            Some(VerificationState::Failed)
        );
        assert!(failed.stdout.text.is_empty());
        assert!(failed.poll_selection().is_none());
        let failed_repeat = manager
            .poll(
                &failed_owner,
                &failed_job,
                failed.stdout.from_cursor,
                failed.stderr.from_cursor,
                Duration::ZERO,
            )
            .await
            .expect("repeat failed verification poll");
        assert_eq!(failed_repeat, failed);

        let cancel_owner = job_owner(&manager, &workspace, Uuid::new_v4());
        let cancel_job = manager
            .start_verification(
                cancel_owner.clone(),
                runner,
                VerificationRunnerId::CargoCheckLocked,
                CancellationToken::new(),
            )
            .expect("start cancellable job");
        let _ = manager
            .poll(&cancel_owner, &cancel_job, 0, 0, Duration::from_millis(200))
            .await
            .expect("observe cancellable job");
        let cancelling = manager
            .cancel(&cancel_owner, &cancel_job)
            .expect("cancel owned job");
        assert!(matches!(
            cancelling.state,
            ProcessJobState::Cancelling | ProcessJobState::Cancelled
        ));
        let cancelled = poll_until_terminal(&manager, &cancel_owner, &cancel_job).await;
        assert_eq!(cancelled.state, ProcessJobState::Cancelled);
        assert_eq!(
            cancelled
                .verification
                .as_ref()
                .map(|evidence| evidence.state),
            Some(VerificationState::Cancelled)
        );
        assert!(cancelled.poll_selection().is_none());
        manager
            .shutdown()
            .await
            .expect("drain failed and cancelled jobs");
    }
}
