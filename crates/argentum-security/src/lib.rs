use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

use argentum_domain::Capability;
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
        Ok(Self { root, policy })
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn policy(&self) -> &ApprovalPolicy {
        &self.policy
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

    pub fn validate_path(&self, candidate: impl AsRef<Path>) -> Result<PathBuf, SecurityError> {
        let candidate = candidate.as_ref();
        let candidate = if candidate.is_absolute() {
            candidate.to_path_buf()
        } else {
            self.root.join(candidate)
        };
        let resolved = canonicalize_with_existing_ancestor(&candidate)
            .ok_or_else(|| SecurityError::PathResolution(candidate.clone()))?;
        if resolved.starts_with(&self.root) {
            Ok(resolved)
        } else {
            Err(SecurityError::OutsideWorkspace(candidate))
        }
    }
}

fn canonicalize_with_existing_ancestor(path: &Path) -> Option<PathBuf> {
    if path.exists() {
        return fs::canonicalize(path).ok();
    }

    let mut missing = Vec::new();
    let mut current = path;
    while !current.exists() {
        missing.push(current.file_name()?.to_os_string());
        current = current.parent()?;
    }

    let mut resolved = fs::canonicalize(current).ok()?;
    for part in missing.iter().rev() {
        resolved.push(part);
    }
    Some(resolved)
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
        fs::write(&outside, "private").expect("outside fixture");
        let broker = CapabilityBroker::new(temp.path(), ApprovalPolicy::default()).expect("broker");

        assert!(matches!(
            broker.validate_path(&outside),
            Err(SecurityError::OutsideWorkspace(_))
        ));
    }

    #[test]
    fn resolves_relative_paths_from_workspace_root() {
        let temp = tempfile::tempdir().expect("temp directory");
        fs::write(temp.path().join("note.txt"), "safe").expect("fixture");
        let broker = CapabilityBroker::new(temp.path(), ApprovalPolicy::default()).expect("broker");

        assert_eq!(
            broker.validate_path("note.txt").expect("relative path"),
            fs::canonicalize(temp.path().join("note.txt")).expect("canonical path")
        );
    }

    #[cfg(unix)]
    #[test]
    fn rejects_symlink_escape() {
        use std::os::unix::fs::symlink;

        let workspace = tempfile::tempdir().expect("workspace");
        let outside = tempfile::tempdir().expect("outside");
        let outside_file = outside.path().join("secret.txt");
        fs::write(&outside_file, "private").expect("outside fixture");
        symlink(&outside_file, workspace.path().join("link.txt")).expect("symlink fixture");
        let broker =
            CapabilityBroker::new(workspace.path(), ApprovalPolicy::default()).expect("broker");

        assert!(matches!(
            broker.validate_path("link.txt"),
            Err(SecurityError::OutsideWorkspace(_))
        ));
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
