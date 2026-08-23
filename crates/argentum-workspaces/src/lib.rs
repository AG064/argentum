use std::io::Read;
use std::path::{Path, PathBuf};

use argentum_domain::Capability;
use argentum_security::{ApprovalGrant, Authorization, CapabilityBroker, SecurityError};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum WorkspaceError {
    #[error(transparent)]
    Security(#[from] SecurityError),
    #[error("workspace operation failed for {path}: {source}")]
    Io {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("file exceeds the {limit_bytes} byte read limit: {path}")]
    FileTooLarge { path: PathBuf, limit_bytes: usize },
}

#[derive(Debug, Clone)]
pub struct WorkspaceManager {
    broker: CapabilityBroker,
}

impl WorkspaceManager {
    pub fn new(broker: CapabilityBroker) -> Self {
        Self { broker }
    }

    pub fn root(&self) -> &Path {
        self.broker.root()
    }

    pub fn requires_approval(&self, capability: Capability) -> Result<bool, WorkspaceError> {
        Ok(matches!(
            self.broker.authorize(capability)?,
            Authorization::RequiresApproval
        ))
    }

    pub fn read_text_bounded(
        &self,
        path: impl AsRef<Path>,
        limit_bytes: usize,
    ) -> Result<String, WorkspaceError> {
        let (display_path, file) = self.broker.open_file_for_read(path)?;
        let read_limit = u64::try_from(limit_bytes)
            .unwrap_or(u64::MAX)
            .saturating_add(1);
        let mut bytes = Vec::with_capacity(limit_bytes.min(64 * 1024));
        file.take(read_limit)
            .read_to_end(&mut bytes)
            .map_err(|source| WorkspaceError::Io {
                path: display_path.clone(),
                source,
            })?;
        if bytes.len() > limit_bytes {
            return Err(WorkspaceError::FileTooLarge {
                path: display_path,
                limit_bytes,
            });
        }
        String::from_utf8(bytes).map_err(|source| WorkspaceError::Io {
            path: display_path,
            source: std::io::Error::new(std::io::ErrorKind::InvalidData, source),
        })
    }

    pub fn write_text(&self, path: impl AsRef<Path>, content: &str) -> Result<(), WorkspaceError> {
        self.write_text_with_grant(path, content, &ApprovalGrant::default())
    }

    pub fn write_text_with_grant(
        &self,
        path: impl AsRef<Path>,
        content: &str,
        grant: &ApprovalGrant,
    ) -> Result<(), WorkspaceError> {
        self.broker
            .write_file_with_grant(path, content, grant)
            .map_err(Into::into)
    }

    pub fn list_files(&self) -> Result<Vec<PathBuf>, WorkspaceError> {
        self.broker.list_workspace_files().map_err(Into::into)
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    #[cfg(unix)]
    use argentum_security::ApprovalGrant;
    use argentum_security::{ApprovalPolicy, CapabilityBroker};

    use super::*;

    #[test]
    fn bounded_read_rejects_a_file_before_returning_oversized_content() {
        let workspace = tempfile::tempdir().expect("workspace");
        fs::write(workspace.path().join("large.txt"), b"eleven-byte").expect("fixture");
        let broker =
            CapabilityBroker::new(workspace.path(), ApprovalPolicy::default()).expect("broker");
        let manager = WorkspaceManager::new(broker);

        assert!(matches!(
            manager.read_text_bounded("large.txt", 10),
            Err(WorkspaceError::FileTooLarge {
                limit_bytes: 10,
                ..
            })
        ));
    }

    #[test]
    fn bounded_read_preserves_valid_utf8_at_the_limit() {
        let workspace = tempfile::tempdir().expect("workspace");
        fs::write(workspace.path().join("exact.txt"), "safe text").expect("fixture");
        let broker =
            CapabilityBroker::new(workspace.path(), ApprovalPolicy::default()).expect("broker");
        let manager = WorkspaceManager::new(broker);

        assert_eq!(
            manager
                .read_text_bounded("exact.txt", "safe text".len())
                .expect("bounded read"),
            "safe text"
        );
    }

    #[cfg(unix)]
    #[test]
    fn capability_reads_do_not_follow_symlinks_outside_the_workspace() {
        use std::os::unix::fs::symlink;

        let workspace = tempfile::tempdir().expect("workspace");
        let outside = tempfile::tempdir().expect("outside");
        let outside_file = outside.path().join("secret.txt");
        fs::write(&outside_file, "private").expect("outside fixture");
        symlink(&outside_file, workspace.path().join("link.txt")).expect("symlink fixture");
        let broker =
            CapabilityBroker::new(workspace.path(), ApprovalPolicy::default()).expect("broker");
        let manager = WorkspaceManager::new(broker);

        assert!(manager.read_text_bounded("link.txt", 1024).is_err());
    }

    #[cfg(unix)]
    #[test]
    fn capability_writes_do_not_follow_symlinks_outside_the_workspace() {
        use std::os::unix::fs::symlink;

        let workspace = tempfile::tempdir().expect("workspace");
        let outside = tempfile::tempdir().expect("outside");
        let outside_file = outside.path().join("secret.txt");
        fs::write(&outside_file, "private").expect("outside fixture");
        symlink(&outside_file, workspace.path().join("link.txt")).expect("symlink fixture");
        let broker =
            CapabilityBroker::new(workspace.path(), ApprovalPolicy::default()).expect("broker");
        let manager = WorkspaceManager::new(broker);
        let grant = ApprovalGrant::for_capabilities([Capability::WriteFiles]);

        assert!(manager
            .write_text_with_grant("link.txt", "overwrite", &grant)
            .is_err());
        assert_eq!(
            fs::read_to_string(outside_file).expect("outside read"),
            "private"
        );
    }
}
