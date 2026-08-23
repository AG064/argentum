use std::collections::BTreeSet;
use std::fs;
use std::path::{Component, Path, PathBuf};
use std::sync::Arc;

use argentum_domain::Capability;
use cap_std::fs::{Dir, File};
use thiserror::Error;
use time::OffsetDateTime;
use zeroize::{Zeroize, ZeroizeOnDrop};

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
        if grant.allows(capability) {
            Ok(Authorization::Allowed)
        } else {
            self.authorize(capability)
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
}

#[cfg(test)]
mod tests {
    use std::fs;

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
}
