use std::collections::BTreeSet;
use std::fs;
use std::path::{Component, Path, PathBuf};
use std::sync::Arc;

use argentum_domain::{
    ApprovalGrantRecord, ApprovalMode, Capability, GrantId, GrantSubject, HarnessExecutionPolicy,
    ProjectId, RunId, SandboxScope, SessionId, Sha256Digest, ToolCallId, ToolRisk,
    CUSTOM_EXECUTION_PROFILE_ID,
};
use cap_std::fs::{Dir, File};
use thiserror::Error;
use time::OffsetDateTime;
use zeroize::{Zeroize, ZeroizeOnDrop};

pub mod authority;

pub use authority::{
    authorize_profile_transition, execution_policy_digest, execution_policy_is_elevation,
    host_authority_channel, AuthorityGeneration, LocalAuthorityAction, LocalAuthorityIssuer,
    LocalAuthorityTicket, LocalAuthorityVerifier, VerifiedLocalAuthority,
};

#[derive(Debug, Error)]
pub enum SecurityError {
    #[error("workspace root does not exist: {0}")]
    MissingWorkspace(PathBuf),
    #[error("path is outside the workspace boundary: {0}")]
    OutsideWorkspace(PathBuf),
    #[error("capability requires approval: {0:?}")]
    ApprovalRequired(Capability),
    #[error("capability is denied by policy: {0:?}")]
    CapabilityDenied(Capability),
    #[error("unable to resolve path: {0}")]
    PathResolution(PathBuf),
    #[error("workspace operation failed for {path}: {source}")]
    WorkspaceOperation {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("sandbox scope denies capability: {0:?}")]
    SandboxDenied(Capability),
    #[error("unrestricted sandbox scope requires the Full Access profile")]
    UnrestrictedProfileRequired,
    #[error("permission profile elevation requires local user authority")]
    ProfileElevationDenied,
    #[error("process argument is unsafe or exceeds its bound")]
    UnsafeProcessArgument,
    #[error("environment variable name is unsafe or secret-like")]
    UnsafeEnvironmentName,
    #[error("local authority generation is foreign to this issuer")]
    LocalAuthorityForeignGeneration,
    #[error("local authority issuer does not match this verifier")]
    LocalAuthorityIssuerMismatch,
    #[error("local authority action does not match")]
    LocalAuthorityActionMismatch,
    #[error("local authority generation does not match")]
    LocalAuthorityGenerationMismatch,
    #[error("local authority has expired")]
    LocalAuthorityExpired,
    #[error("local authority capability identifier is invalid")]
    LocalAuthorityInvalidCapability,
    #[error("local authority digest is invalid")]
    LocalAuthorityInvalidDigest,
    #[error("local authority approval identifier is invalid")]
    LocalAuthorityInvalidApprovalId,
    #[error("authority generation is exhausted")]
    AuthorityGenerationExhausted,
    #[error("local authority time bound overflowed")]
    LocalAuthorityTimeOverflow,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AuthorizationDecision {
    AllowedByPolicy,
    AllowedByGrants(Vec<GrantId>),
    Prompt,
    TryWithoutGrant,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthorizationRequest {
    pub policy: HarnessExecutionPolicy,
    pub capability: Capability,
    pub risk: ToolRisk,
    pub tool_id: String,
    pub target: String,
    pub binding: Sha256Digest,
    pub tool_call_id: ToolCallId,
    pub run_id: RunId,
    pub session_id: SessionId,
    pub project_id: ProjectId,
    pub at: OffsetDateTime,
}

pub struct ExecutionAuthorizer;

impl ExecutionAuthorizer {
    pub fn authorize(
        request: &AuthorizationRequest,
        grants: &[ApprovalGrantRecord],
    ) -> Result<AuthorizationDecision, SecurityError> {
        validate_sandbox(request)?;

        let mut grant_ids = grants
            .iter()
            .filter(|grant| grant_matches(grant, request))
            .map(|grant| grant.id)
            .collect::<Vec<_>>();
        grant_ids.sort_unstable();
        grant_ids.dedup();
        if !grant_ids.is_empty() {
            return Ok(AuthorizationDecision::AllowedByGrants(grant_ids));
        }

        // This is a separate explicit owner enablement, not inherited fixed-verifier authority.
        if request.capability == Capability::ExternalProcess
            && request.tool_id == "check_rust"
            && request
                .policy
                .capability_enabled
                .get("verification.supplemental-rust")
                == Some(&true)
            && (request.policy.profile_id == "verification"
                || (request.policy.profile_id == CUSTOM_EXECUTION_PROFILE_ID
                    && request.policy.source_profile_id.as_deref() == Some("verification")))
            && request
                .target
                .strip_prefix("isolated rust checks: ")
                .is_some_and(|hash| {
                    hash.len() == 64
                        && hash
                            .bytes()
                            .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
                })
            && request.risk == ToolRisk::Verification
            && request.policy.sandbox_scope == SandboxScope::WorkspaceWrite
            && request.policy.approval_mode == ApprovalMode::Never
        {
            return Ok(AuthorizationDecision::AllowedByPolicy);
        }

        // The Verification profile is an owner-selected exception for the
        // fixed verifier tools only. Direct argv and shell tools retain the
        // ordinary external-process approval boundary.
        if request.capability == Capability::ExternalProcess
            && (request.policy.profile_id == "verification"
                || (request.policy.profile_id == CUSTOM_EXECUTION_PROFILE_ID
                    && request.policy.source_profile_id.as_deref() == Some("verification")))
            && matches!(request.tool_id.as_str(), "verify" | "start_verification")
            && request
                .target
                .strip_prefix("fixed runners: ")
                .is_some_and(|runners| {
                    runners.split(", ").all(|runner| {
                        matches!(
                            runner,
                            "cargo-fmt-check"
                                | "cargo-check-locked"
                                | "cargo-test-locked"
                                | "cargo-clippy-strict"
                        )
                    })
                })
            && request.risk == ToolRisk::Verification
            && request.policy.sandbox_scope == SandboxScope::WorkspaceWrite
            && request.policy.approval_mode == ApprovalMode::Never
        {
            return Ok(AuthorizationDecision::AllowedByPolicy);
        }

        if request.capability == Capability::ExternalProcess
            && request.risk != ToolRisk::GitRead
            && request.policy.sandbox_scope != SandboxScope::Unrestricted
        {
            return Ok(AuthorizationDecision::Prompt);
        }

        let decision = match request.policy.approval_mode {
            ApprovalMode::Always if request.risk.requires_risk_approval() => {
                AuthorizationDecision::Prompt
            }
            ApprovalMode::OnRisk if request.risk.requires_risk_approval() => {
                AuthorizationDecision::Prompt
            }
            ApprovalMode::OnFailure if request.risk.requires_risk_approval() => {
                AuthorizationDecision::TryWithoutGrant
            }
            ApprovalMode::Always
            | ApprovalMode::OnRisk
            | ApprovalMode::OnFailure
            | ApprovalMode::Never => AuthorizationDecision::AllowedByPolicy,
        };
        Ok(decision)
    }
}

fn validate_sandbox(request: &AuthorizationRequest) -> Result<(), SecurityError> {
    match request.policy.sandbox_scope {
        SandboxScope::ReadOnly
            if !(matches!(
                request.capability,
                Capability::ReadFiles | Capability::GitRead
            ) || request.capability == Capability::ExternalProcess
                && request.risk == ToolRisk::GitRead) =>
        {
            Err(SecurityError::SandboxDenied(request.capability))
        }
        SandboxScope::Unrestricted if request.policy.profile_id != "full-access" => {
            Err(SecurityError::UnrestrictedProfileRequired)
        }
        SandboxScope::ReadOnly | SandboxScope::WorkspaceWrite | SandboxScope::Unrestricted => {
            Ok(())
        }
    }
}

fn grant_matches(grant: &ApprovalGrantRecord, request: &AuthorizationRequest) -> bool {
    if grant.capability != request.capability
        || grant.scope != grant.subject.scope()
        || grant.created_at > request.at
        || grant
            .expires_at
            .is_some_and(|expires_at| expires_at <= request.at)
        || grant
            .revoked_at
            .is_some_and(|revoked_at| revoked_at <= request.at)
    {
        return false;
    }

    match &grant.subject {
        GrantSubject::Action {
            tool_id,
            target,
            binding,
        } => {
            tool_id == &request.tool_id && target == &request.target && binding == &request.binding
        }
        GrantSubject::ToolCall { tool_call_id } => tool_call_id == &request.tool_call_id,
        GrantSubject::Run { run_id } => run_id == &request.run_id,
        GrantSubject::Session { session_id } => session_id == &request.session_id,
        GrantSubject::Workspace { project_id } => project_id == &request.project_id,
        GrantSubject::PersistentProfile { profile_id } => profile_id == &request.policy.profile_id,
    }
}

const MAX_PROCESS_ARGUMENT_BYTES: usize = 8 * 1024;
const MAX_ENVIRONMENT_NAME_BYTES: usize = 128;

pub fn validate_process_argument(argument: &str) -> Result<(), SecurityError> {
    if argument.len() > MAX_PROCESS_ARGUMENT_BYTES || argument.chars().any(char::is_control) {
        return Err(SecurityError::UnsafeProcessArgument);
    }
    if argument == "--api-key-file" {
        return Ok(());
    }
    let lower = argument.to_ascii_lowercase();
    if [
        "password=",
        "token=",
        "api_key=",
        "api-key=",
        "authorization:",
        "bearer ",
        "--password",
        "--token=",
        "--api-key",
    ]
    .iter()
    .any(|marker| lower.contains(marker))
    {
        return Err(SecurityError::UnsafeProcessArgument);
    }
    Ok(())
}

pub fn validate_environment_name(name: &str) -> Result<(), SecurityError> {
    let valid_shape = !name.is_empty()
        && name.len() <= MAX_ENVIRONMENT_NAME_BYTES
        && name
            .bytes()
            .all(|byte| byte.is_ascii_uppercase() || byte.is_ascii_digit() || byte == b'_');
    let secret_like = [
        "TOKEN",
        "SECRET",
        "PASSWORD",
        "CREDENTIAL",
        "API_KEY",
        "AUTH",
    ]
    .iter()
    .any(|marker| name.contains(marker));
    if valid_shape && !secret_like {
        Ok(())
    } else {
        Err(SecurityError::UnsafeEnvironmentName)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApprovalPolicy {
    allowed_without_approval: BTreeSet<Capability>,
    denied: BTreeSet<Capability>,
}

impl Default for ApprovalPolicy {
    fn default() -> Self {
        let mut allowed_without_approval = BTreeSet::new();
        allowed_without_approval.insert(Capability::ReadFiles);

        Self {
            allowed_without_approval,
            denied: BTreeSet::new(),
        }
    }
}

impl ApprovalPolicy {
    pub fn allow_without_approval(mut self, capability: Capability) -> Self {
        self.allowed_without_approval.insert(capability);
        self.denied.remove(&capability);
        self
    }

    pub fn deny(mut self, capability: Capability) -> Self {
        self.denied.insert(capability);
        self.allowed_without_approval.remove(&capability);
        self
    }

    pub fn check(&self, capability: Capability) -> Result<Authorization, SecurityError> {
        if self.denied.contains(&capability) {
            return Err(SecurityError::CapabilityDenied(capability));
        }
        if self.allowed_without_approval.contains(&capability) {
            Ok(Authorization::Allowed)
        } else {
            Ok(Authorization::RequiresApproval)
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Authorization {
    Allowed,
    RequiresApproval,
}

#[derive(Debug, Clone, Default)]
pub struct ApprovalGrant {
    capabilities: BTreeSet<Capability>,
}

impl ApprovalGrant {
    pub fn for_capabilities(capabilities: impl IntoIterator<Item = Capability>) -> Self {
        Self {
            capabilities: capabilities.into_iter().collect(),
        }
    }

    pub fn allows(&self, capability: Capability) -> bool {
        self.capabilities.contains(&capability)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ScopedPath {
    Workspace {
        relative: PathBuf,
        absolute: PathBuf,
    },
    Ambient(PathBuf),
}

#[derive(Debug, Clone)]
pub struct CapabilityBroker {
    root: PathBuf,
    directory: Arc<Dir>,
    policy: ApprovalPolicy,
}

impl CapabilityBroker {
    pub fn new(root: impl AsRef<Path>, policy: ApprovalPolicy) -> Result<Self, SecurityError> {
        let root = root.as_ref();
        if !root.exists() {
            return Err(SecurityError::MissingWorkspace(root.to_path_buf()));
        }
        let root = fs::canonicalize(root)
            .map_err(|_| SecurityError::PathResolution(root.to_path_buf()))?;
        let directory = Dir::open_ambient_dir(&root, cap_std::ambient_authority())
            .map_err(|_| SecurityError::PathResolution(root.clone()))?;
        Ok(Self {
            root,
            directory: Arc::new(directory),
            policy,
        })
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn policy(&self) -> &ApprovalPolicy {
        &self.policy
    }

    pub fn relative_path(&self, candidate: impl AsRef<Path>) -> Result<PathBuf, SecurityError> {
        let candidate = candidate.as_ref();
        let relative = if candidate.is_absolute() {
            candidate
                .strip_prefix(&self.root)
                .map_err(|_| SecurityError::OutsideWorkspace(candidate.to_path_buf()))?
        } else {
            candidate
        };

        if relative.components().any(|component| {
            matches!(
                component,
                Component::ParentDir | Component::RootDir | Component::Prefix(_)
            )
        }) {
            return Err(SecurityError::OutsideWorkspace(candidate.to_path_buf()));
        }

        Ok(relative.to_path_buf())
    }

    pub fn resolve_path_with_scope(
        &self,
        candidate: impl AsRef<Path>,
        scope: SandboxScope,
    ) -> Result<ScopedPath, SecurityError> {
        let candidate = candidate.as_ref();
        if !candidate.is_absolute() || scope != SandboxScope::Unrestricted {
            let relative = self.relative_path(candidate)?;
            return Ok(ScopedPath::Workspace {
                absolute: self.root.join(&relative),
                relative,
            });
        }

        if candidate
            .components()
            .any(|component| matches!(component, Component::ParentDir))
        {
            return Err(SecurityError::PathResolution(candidate.to_path_buf()));
        }
        let resolved = if candidate.exists() {
            fs::canonicalize(candidate)
                .map_err(|_| SecurityError::PathResolution(candidate.to_path_buf()))?
        } else {
            candidate.to_path_buf()
        };
        Ok(ScopedPath::Ambient(resolved))
    }

    pub fn open_file_for_read(
        &self,
        candidate: impl AsRef<Path>,
    ) -> Result<(PathBuf, File), SecurityError> {
        self.require_with_grant(Capability::ReadFiles, &ApprovalGrant::default())?;
        let relative = self.relative_path(candidate)?;
        let display_path = self.root.join(&relative);
        let file =
            self.directory
                .open(&relative)
                .map_err(|source| SecurityError::WorkspaceOperation {
                    path: display_path.clone(),
                    source,
                })?;
        Ok((display_path, file))
    }

    /// Optional contained regular-file read for automatic context discovery.
    /// Only NotFound is absence. Inspect each component without following links;
    /// the capability directory also contains the subsequent open during races.
    pub fn open_context_file(
        &self,
        candidate: impl AsRef<Path>,
    ) -> Result<Option<File>, SecurityError> {
        self.require_with_grant(Capability::ReadFiles, &ApprovalGrant::default())?;
        let relative = self.relative_path(candidate)?;
        let mut prefix = PathBuf::new();
        for component in relative.components() {
            prefix.push(component);
            let metadata = match self.directory.symlink_metadata(&prefix) {
                Ok(metadata) => metadata,
                Err(source) if source.kind() == std::io::ErrorKind::NotFound => return Ok(None),
                Err(source) => {
                    return Err(SecurityError::WorkspaceOperation {
                        path: self.root.join(&prefix),
                        source,
                    })
                }
            };
            if metadata.file_type().is_symlink()
                || (prefix != relative && !metadata.is_dir())
                || (prefix == relative && !metadata.is_file())
            {
                return Err(SecurityError::PathResolution(self.root.join(&prefix)));
            }
        }
        let file =
            self.directory
                .open(&relative)
                .map_err(|source| SecurityError::WorkspaceOperation {
                    path: self.root.join(&relative),
                    source,
                })?;
        let metadata = file
            .metadata()
            .map_err(|source| SecurityError::WorkspaceOperation {
                path: self.root.join(&relative),
                source,
            })?;
        if !metadata.is_file() {
            return Err(SecurityError::PathResolution(self.root.join(relative)));
        }
        Ok(Some(file))
    }

    pub fn context_path_is_directory(
        &self,
        candidate: impl AsRef<Path>,
    ) -> Result<Option<bool>, SecurityError> {
        self.require_with_grant(Capability::ReadFiles, &ApprovalGrant::default())?;
        let relative = self.relative_path(candidate)?;
        if relative.as_os_str().is_empty() {
            return Ok(Some(true));
        }
        match self.directory.symlink_metadata(&relative) {
            Ok(metadata) if !metadata.file_type().is_symlink() => Ok(Some(metadata.is_dir())),
            Ok(_) => Err(SecurityError::PathResolution(self.root.join(relative))),
            Err(source) if source.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(source) => Err(SecurityError::WorkspaceOperation {
                path: self.root.join(relative),
                source,
            }),
        }
    }

    pub fn write_file_with_grant(
        &self,
        candidate: impl AsRef<Path>,
        content: impl AsRef<[u8]>,
        grant: &ApprovalGrant,
    ) -> Result<(), SecurityError> {
        self.require_with_grant(Capability::WriteFiles, grant)?;
        let relative = self.relative_path(candidate)?;
        let display_path = self.root.join(&relative);
        if let Some(parent) = relative
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
        {
            self.directory.create_dir_all(parent).map_err(|source| {
                SecurityError::WorkspaceOperation {
                    path: self.root.join(parent),
                    source,
                }
            })?;
        }
        self.directory.write(&relative, content).map_err(|source| {
            SecurityError::WorkspaceOperation {
                path: display_path,
                source,
            }
        })
    }

    pub fn remove_file_with_grant(
        &self,
        candidate: impl AsRef<Path>,
        grant: &ApprovalGrant,
    ) -> Result<(), SecurityError> {
        self.require_with_grant(Capability::WriteFiles, grant)?;
        let relative = self.relative_path(candidate)?;
        let display_path = self.root.join(&relative);
        self.directory
            .remove_file(&relative)
            .map_err(|source| SecurityError::WorkspaceOperation {
                path: display_path,
                source,
            })
    }

    pub fn create_dir_all_with_grant(
        &self,
        candidate: impl AsRef<Path>,
        grant: &ApprovalGrant,
    ) -> Result<(), SecurityError> {
        self.require_with_grant(Capability::WriteFiles, grant)?;
        let relative = self.relative_path(candidate)?;
        let display_path = self.root.join(&relative);
        self.directory.create_dir_all(&relative).map_err(|source| {
            SecurityError::WorkspaceOperation {
                path: display_path,
                source,
            }
        })
    }

    pub fn list_workspace_files(&self) -> Result<Vec<PathBuf>, SecurityError> {
        self.require_with_grant(Capability::ReadFiles, &ApprovalGrant::default())?;
        let mut files = Vec::new();
        collect_files(&self.directory, Path::new(""), &self.root, &mut files).map_err(
            |source| SecurityError::WorkspaceOperation {
                path: self.root.clone(),
                source,
            },
        )?;
        files.sort();
        Ok(files)
    }

    pub fn authorize(&self, capability: Capability) -> Result<Authorization, SecurityError> {
        self.policy.check(capability)
    }

    pub fn authorize_with_grant(
        &self,
        capability: Capability,
        grant: &ApprovalGrant,
    ) -> Result<Authorization, SecurityError> {
        match self.authorize(capability)? {
            Authorization::Allowed => Ok(Authorization::Allowed),
            Authorization::RequiresApproval if grant.allows(capability) => {
                Ok(Authorization::Allowed)
            }
            Authorization::RequiresApproval => Ok(Authorization::RequiresApproval),
        }
    }

    fn require_with_grant(
        &self,
        capability: Capability,
        grant: &ApprovalGrant,
    ) -> Result<(), SecurityError> {
        match self.authorize_with_grant(capability, grant)? {
            Authorization::Allowed => Ok(()),
            Authorization::RequiresApproval => Err(SecurityError::ApprovalRequired(capability)),
        }
    }
}

fn collect_files(
    directory: &Dir,
    relative: &Path,
    root: &Path,
    files: &mut Vec<PathBuf>,
) -> std::io::Result<()> {
    for entry in directory.entries()? {
        let entry = entry?;
        let file_type = entry.file_type()?;
        let path = relative.join(entry.file_name());
        if file_type.is_symlink() {
            continue;
        }
        if file_type.is_dir() {
            collect_files(&entry.open_dir()?, &path, root, files)?;
        } else if file_type.is_file() {
            files.push(root.join(path));
        }
    }
    Ok(())
}

#[derive(Clone, Zeroize, ZeroizeOnDrop)]
pub struct SecretValue(String);

impl std::fmt::Debug for SecretValue {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("SecretValue([REDACTED])")
    }
}

impl SecretValue {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    pub fn expose(&self) -> &str {
        &self.0
    }
}

#[derive(Debug, Clone)]
pub struct SecretRedactor {
    secrets: Vec<String>,
    created_at: OffsetDateTime,
}

impl SecretRedactor {
    pub fn new(values: impl IntoIterator<Item = SecretValue>) -> Self {
        Self {
            secrets: values
                .into_iter()
                .map(|value| value.expose().to_owned())
                .collect(),
            created_at: OffsetDateTime::now_utc(),
        }
    }

    pub fn redact(&self, input: &str) -> String {
        let mut redacted = input.to_owned();
        for secret in &self.secrets {
            if !secret.is_empty() {
                redacted = redacted.replace(secret, "[REDACTED]");
            }
        }
        redacted
    }

    pub fn created_at(&self) -> OffsetDateTime {
        self.created_at
    }

    /// Count only; streaming disclosure must retain enough overlap to avoid
    /// exposing a registered secret split across capture chunks.
    pub fn max_secret_utf8_bytes(&self) -> usize {
        self.secrets.iter().map(String::len).max().unwrap_or(0)
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use argentum_domain::{GrantIssuer, GrantScope};

    use super::*;

    #[test]
    fn allows_reads_and_requires_approval_for_writes() {
        let temp = tempfile::tempdir().expect("temp directory");
        let broker = CapabilityBroker::new(temp.path(), ApprovalPolicy::default()).expect("broker");

        assert_eq!(
            broker.authorize(Capability::ReadFiles).unwrap(),
            Authorization::Allowed
        );
        assert_eq!(
            broker.authorize(Capability::WriteFiles).unwrap(),
            Authorization::RequiresApproval
        );
    }

    #[test]
    fn approval_grant_does_not_override_a_hard_policy_deny() {
        let temp = tempfile::tempdir().expect("temp directory");
        let broker = CapabilityBroker::new(
            temp.path(),
            ApprovalPolicy::default().deny(Capability::WriteFiles),
        )
        .expect("broker");
        let grant = ApprovalGrant::for_capabilities([Capability::WriteFiles]);

        assert!(matches!(
            broker.authorize_with_grant(Capability::WriteFiles, &grant),
            Err(SecurityError::CapabilityDenied(Capability::WriteFiles))
        ));
    }

    #[test]
    fn rejects_paths_outside_workspace() {
        let temp = tempfile::tempdir().expect("temp directory");
        let outside = temp.path().parent().expect("parent").join("outside.txt");
        let broker = CapabilityBroker::new(temp.path(), ApprovalPolicy::default()).expect("broker");

        assert!(matches!(
            broker.relative_path(&outside),
            Err(SecurityError::OutsideWorkspace(_))
        ));
        assert!(matches!(
            broker.relative_path("../outside.txt"),
            Err(SecurityError::OutsideWorkspace(_))
        ));
    }

    #[test]
    fn resolves_relative_paths_from_workspace_root() {
        let temp = tempfile::tempdir().expect("temp directory");
        fs::write(temp.path().join("note.txt"), "safe").expect("fixture");
        let broker = CapabilityBroker::new(temp.path(), ApprovalPolicy::default()).expect("broker");

        assert_eq!(
            broker.relative_path("note.txt").expect("relative path"),
            PathBuf::from("note.txt")
        );
    }

    #[test]
    fn redacts_registered_secrets() {
        let redactor = SecretRedactor::new([SecretValue::new("abc-secret")]);
        assert_eq!(redactor.redact("token=abc-secret"), "token=[REDACTED]");
    }

    #[test]
    fn secret_debug_output_never_contains_the_value() {
        let secret = SecretValue::new("do-not-print-this-value");

        let rendered = format!("{secret:?}");

        assert_eq!(rendered, "SecretValue([REDACTED])");
        assert!(!rendered.contains(secret.expose()));
    }

    fn phase4_request(
        policy: HarnessExecutionPolicy,
        capability: Capability,
        risk: ToolRisk,
    ) -> AuthorizationRequest {
        AuthorizationRequest {
            policy,
            capability,
            risk,
            tool_id: "execute_argv".into(),
            target: "cargo".into(),
            binding: Sha256Digest("1".repeat(64)),
            tool_call_id: uuid::Uuid::new_v4(),
            run_id: uuid::Uuid::new_v4(),
            session_id: uuid::Uuid::new_v4(),
            project_id: uuid::Uuid::new_v4(),
            at: OffsetDateTime::now_utc(),
        }
    }

    fn phase4_policy(
        profile_id: &str,
        sandbox_scope: SandboxScope,
        approval_mode: ApprovalMode,
    ) -> HarnessExecutionPolicy {
        HarnessExecutionPolicy {
            profile_id: profile_id.into(),
            source_profile_id: Some(profile_id.into()),
            sandbox_scope,
            approval_mode,
            capability_enabled: Default::default(),
        }
    }

    #[test]
    fn phase4_authorization_matrix_separates_sandbox_and_approval() {
        let guided = phase4_request(
            phase4_policy("guided", SandboxScope::WorkspaceWrite, ApprovalMode::Always),
            Capability::ExecuteCommands,
            ToolRisk::Process,
        );
        assert_eq!(
            ExecutionAuthorizer::authorize(&guided, &[]).expect("guided decision"),
            AuthorizationDecision::Prompt
        );

        let supervised = phase4_request(
            phase4_policy(
                "supervised",
                SandboxScope::WorkspaceWrite,
                ApprovalMode::OnRisk,
            ),
            Capability::GitRead,
            ToolRisk::GitRead,
        );
        assert_eq!(
            ExecutionAuthorizer::authorize(&supervised, &[]).expect("supervised decision"),
            AuthorizationDecision::AllowedByPolicy
        );

        let on_failure = phase4_request(
            phase4_policy(
                "custom",
                SandboxScope::WorkspaceWrite,
                ApprovalMode::OnFailure,
            ),
            Capability::WriteFiles,
            ToolRisk::Mutation,
        );
        assert_eq!(
            ExecutionAuthorizer::authorize(&on_failure, &[]).expect("on-failure decision"),
            AuthorizationDecision::TryWithoutGrant
        );

        let automatic = phase4_request(
            phase4_policy(
                "automatic",
                SandboxScope::WorkspaceWrite,
                ApprovalMode::Never,
            ),
            Capability::ExternalProcess,
            ToolRisk::Process,
        );
        assert_eq!(
            ExecutionAuthorizer::authorize(&automatic, &[]).expect("automatic decision"),
            AuthorizationDecision::Prompt
        );

        let on_failure_process = phase4_request(
            phase4_policy(
                "custom",
                SandboxScope::WorkspaceWrite,
                ApprovalMode::OnFailure,
            ),
            Capability::ExternalProcess,
            ToolRisk::Process,
        );
        assert_eq!(
            ExecutionAuthorizer::authorize(&on_failure_process, &[])
                .expect("on-failure process decision"),
            AuthorizationDecision::Prompt
        );

        let mut verified = phase4_request(
            phase4_policy(
                "verification",
                SandboxScope::WorkspaceWrite,
                ApprovalMode::Never,
            ),
            Capability::ExternalProcess,
            ToolRisk::Verification,
        );
        verified.tool_id = "verify".into();
        verified.target = "fixed runners: cargo-test-locked".into();
        assert_eq!(
            ExecutionAuthorizer::authorize(&verified, &[]).expect("fixed verifier decision"),
            AuthorizationDecision::AllowedByPolicy
        );

        let mut customized_verification = verified.clone();
        customized_verification.policy.profile_id = CUSTOM_EXECUTION_PROFILE_ID.into();
        assert_eq!(
            ExecutionAuthorizer::authorize(&customized_verification, &[])
                .expect("customized fixed verifier decision"),
            AuthorizationDecision::AllowedByPolicy
        );

        let mut unrelated_custom = customized_verification.clone();
        unrelated_custom.policy.source_profile_id = Some("automatic".into());
        assert_eq!(
            ExecutionAuthorizer::authorize(&unrelated_custom, &[])
                .expect("unrelated custom verifier decision"),
            AuthorizationDecision::Prompt
        );

        let mut unproven_custom = customized_verification.clone();
        unproven_custom.policy.source_profile_id = None;
        assert_eq!(
            ExecutionAuthorizer::authorize(&unproven_custom, &[])
                .expect("unproven custom verifier decision"),
            AuthorizationDecision::Prompt
        );

        let mut arbitrary_process = verified.clone();
        arbitrary_process.tool_id = "execute_argv".into();
        arbitrary_process.risk = ToolRisk::Process;
        arbitrary_process.target = "program powershell.exe".into();
        assert_eq!(
            ExecutionAuthorizer::authorize(&arbitrary_process, &[])
                .expect("arbitrary process decision"),
            AuthorizationDecision::Prompt
        );

        let mut restricted_verification = customized_verification.clone();
        restricted_verification
            .policy
            .capability_enabled
            .insert("execution.argv".into(), false);
        assert_eq!(
            ExecutionAuthorizer::authorize(&restricted_verification, &[])
                .expect("fixed verifier without generic argv"),
            AuthorizationDecision::AllowedByPolicy
        );
        restricted_verification.target = "fixed runners: project:next02-artifact".into();
        assert_eq!(
            ExecutionAuthorizer::authorize(&restricted_verification, &[])
                .expect("project verifier still needs exact approval"),
            AuthorizationDecision::Prompt
        );
    }

    #[test]
    fn isolated_rust_check_requires_its_separate_exact_owner_enablement() {
        let mut request = phase4_request(
            phase4_policy(
                "verification",
                SandboxScope::WorkspaceWrite,
                ApprovalMode::Never,
            ),
            Capability::ExternalProcess,
            ToolRisk::Verification,
        );
        request.tool_id = "check_rust".into();
        request.target = format!("isolated rust checks: {}", "a".repeat(64));
        assert_ne!(
            ExecutionAuthorizer::authorize(&request, &[]).unwrap(),
            AuthorizationDecision::AllowedByPolicy
        );
        request
            .policy
            .capability_enabled
            .insert("verification.supplemental-rust".into(), true);
        assert_eq!(
            ExecutionAuthorizer::authorize(&request, &[]).unwrap(),
            AuthorizationDecision::AllowedByPolicy
        );
        request.tool_id = "execute_argv".into();
        assert_ne!(
            ExecutionAuthorizer::authorize(&request, &[]).unwrap(),
            AuthorizationDecision::AllowedByPolicy
        );
        request.tool_id = "check_rust".into();
        request.policy.source_profile_id = Some("automatic".into());
        request.policy.profile_id = "custom".into();
        assert_ne!(
            ExecutionAuthorizer::authorize(&request, &[]).unwrap(),
            AuthorizationDecision::AllowedByPolicy
        );
    }

    #[test]
    fn phase4_read_only_and_full_access_rules_cannot_be_widened_by_grants() {
        let mut observe = phase4_request(
            phase4_policy("observe", SandboxScope::ReadOnly, ApprovalMode::Always),
            Capability::WriteFiles,
            ToolRisk::Mutation,
        );
        let grant = ApprovalGrantRecord {
            id: uuid::Uuid::new_v4(),
            audit_id: uuid::Uuid::new_v4(),
            capability: Capability::WriteFiles,
            scope: GrantScope::ToolCall,
            subject: GrantSubject::ToolCall {
                tool_call_id: observe.tool_call_id,
            },
            created_at: observe.at,
            expires_at: None,
            issuer: GrantIssuer::User,
            reason: "test grant".into(),
            revoked_at: None,
        };
        assert!(matches!(
            ExecutionAuthorizer::authorize(&observe, &[grant]),
            Err(SecurityError::SandboxDenied(Capability::WriteFiles))
        ));

        let git_process = phase4_request(
            phase4_policy("observe", SandboxScope::ReadOnly, ApprovalMode::Always),
            Capability::ExternalProcess,
            ToolRisk::GitRead,
        );
        assert_eq!(
            ExecutionAuthorizer::authorize(&git_process, &[]).expect("fixed Git process"),
            AuthorizationDecision::AllowedByPolicy
        );
        let general_process = phase4_request(
            phase4_policy("observe", SandboxScope::ReadOnly, ApprovalMode::Always),
            Capability::ExternalProcess,
            ToolRisk::Process,
        );
        assert!(matches!(
            ExecutionAuthorizer::authorize(&general_process, &[]),
            Err(SecurityError::SandboxDenied(Capability::ExternalProcess))
        ));

        observe.policy.sandbox_scope = SandboxScope::Unrestricted;
        assert!(matches!(
            ExecutionAuthorizer::authorize(&observe, &[]),
            Err(SecurityError::UnrestrictedProfileRequired)
        ));
    }

    #[test]
    fn phase4_grants_match_exact_subject_expiration_and_revocation() {
        let request = phase4_request(
            phase4_policy("guided", SandboxScope::WorkspaceWrite, ApprovalMode::Always),
            Capability::ExecuteCommands,
            ToolRisk::Process,
        );
        let mut grant = ApprovalGrantRecord {
            id: uuid::Uuid::new_v4(),
            audit_id: uuid::Uuid::new_v4(),
            capability: Capability::ExecuteCommands,
            scope: GrantScope::Run,
            subject: GrantSubject::Run {
                run_id: request.run_id,
            },
            created_at: request.at,
            expires_at: Some(request.at + time::Duration::minutes(1)),
            issuer: GrantIssuer::User,
            reason: "run tools".into(),
            revoked_at: None,
        };
        assert_eq!(
            ExecutionAuthorizer::authorize(&request, &[grant.clone()]).expect("grant decision"),
            AuthorizationDecision::AllowedByGrants(vec![grant.id])
        );

        grant.subject = GrantSubject::Run {
            run_id: uuid::Uuid::new_v4(),
        };
        assert_eq!(
            ExecutionAuthorizer::authorize(&request, &[grant.clone()]).expect("scope mismatch"),
            AuthorizationDecision::Prompt
        );
        grant.subject = GrantSubject::Run {
            run_id: request.run_id,
        };
        grant.expires_at = Some(request.at);
        assert_eq!(
            ExecutionAuthorizer::authorize(&request, &[grant.clone()]).expect("expired grant"),
            AuthorizationDecision::Prompt
        );
        grant.expires_at = None;
        grant.revoked_at = Some(request.at);
        assert_eq!(
            ExecutionAuthorizer::authorize(&request, &[grant]).expect("revoked grant"),
            AuthorizationDecision::Prompt
        );
    }

    #[test]
    fn phase4_action_grants_require_the_exact_request_binding() {
        let request = phase4_request(
            phase4_policy("guided", SandboxScope::WorkspaceWrite, ApprovalMode::Always),
            Capability::ExecuteCommands,
            ToolRisk::Process,
        );
        let mut grant = ApprovalGrantRecord {
            id: uuid::Uuid::new_v4(),
            audit_id: uuid::Uuid::new_v4(),
            capability: Capability::ExecuteCommands,
            scope: GrantScope::Action,
            subject: GrantSubject::Action {
                tool_id: request.tool_id.clone(),
                target: request.target.clone(),
                binding: request.binding.clone(),
            },
            created_at: request.at,
            expires_at: None,
            issuer: GrantIssuer::User,
            reason: "exact action".into(),
            revoked_at: None,
        };

        assert_eq!(
            ExecutionAuthorizer::authorize(&request, &[grant.clone()]).expect("exact action"),
            AuthorizationDecision::AllowedByGrants(vec![grant.id])
        );

        grant.subject = GrantSubject::Action {
            tool_id: request.tool_id.clone(),
            target: request.target.clone(),
            binding: Sha256Digest("2".repeat(64)),
        };
        assert_eq!(
            ExecutionAuthorizer::authorize(&request, &[grant]).expect("different payload"),
            AuthorizationDecision::Prompt
        );
    }

    #[test]
    fn phase4_scope_aware_path_resolution_requires_unrestricted_policy() {
        let temp = tempfile::tempdir().expect("temp directory");
        let outside = tempfile::tempdir().expect("outside directory");
        let broker = CapabilityBroker::new(temp.path(), ApprovalPolicy::default()).expect("broker");

        assert!(matches!(
            broker.resolve_path_with_scope(outside.path(), SandboxScope::WorkspaceWrite),
            Err(SecurityError::OutsideWorkspace(_))
        ));
        assert!(matches!(
            broker
                .resolve_path_with_scope(outside.path(), SandboxScope::Unrestricted)
                .expect("ambient path"),
            ScopedPath::Ambient(_)
        ));
        assert!(matches!(
            broker
                .resolve_path_with_scope("relative.txt", SandboxScope::Unrestricted)
                .expect("relative workspace path"),
            ScopedPath::Workspace { .. }
        ));
    }

    #[test]
    fn phase4_process_inputs_reject_secret_like_values() {
        for argument in [
            "--password=hunter2",
            "api_key=hidden",
            "Authorization: Bearer hidden",
        ] {
            assert!(matches!(
                validate_process_argument(argument),
                Err(SecurityError::UnsafeProcessArgument)
            ));
        }
        assert!(validate_process_argument("--locked").is_ok());
        assert!(validate_process_argument("--api-key-file").is_ok());
        assert!(matches!(
            validate_process_argument("--api-key=hidden"),
            Err(SecurityError::UnsafeProcessArgument)
        ));
        assert!(matches!(
            validate_process_argument("--api-key-file=hidden"),
            Err(SecurityError::UnsafeProcessArgument)
        ));
        assert!(validate_environment_name("RUST_LOG").is_ok());
        for name in ["OPENAI_API_KEY", "ACCESS_TOKEN", "PASSWORD"] {
            assert!(matches!(
                validate_environment_name(name),
                Err(SecurityError::UnsafeEnvironmentName)
            ));
        }
    }
}
