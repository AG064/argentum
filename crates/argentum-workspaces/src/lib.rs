use std::fs;
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

    pub fn read_text(&self, path: impl AsRef<Path>) -> Result<String, WorkspaceError> {
        self.require(argentum_domain::Capability::ReadFiles)?;
        let path = self.broker.validate_path(path)?;
        fs::read_to_string(&path).map_err(|source| WorkspaceError::Io { path, source })
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
        self.require_with_grant(argentum_domain::Capability::WriteFiles, grant)?;
        let path = self.broker.validate_path(path)?;
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|source| WorkspaceError::Io {
                path: parent.to_path_buf(),
                source,
            })?;
        }
        fs::write(&path, content).map_err(|source| WorkspaceError::Io { path, source })
    }

    pub fn list_files(&self) -> Result<Vec<PathBuf>, WorkspaceError> {
        self.require(argentum_domain::Capability::ReadFiles)?;
        let mut files = Vec::new();
        collect_files(self.root(), &mut files).map_err(|source| WorkspaceError::Io {
            path: self.root().to_path_buf(),
            source,
        })?;
        files.sort();
        Ok(files)
    }

    fn require(&self, capability: argentum_domain::Capability) -> Result<(), WorkspaceError> {
        self.require_with_grant(capability, &ApprovalGrant::default())
    }

    fn require_with_grant(
        &self,
        capability: argentum_domain::Capability,
        grant: &ApprovalGrant,
    ) -> Result<(), WorkspaceError> {
        match self.broker.authorize_with_grant(capability, grant)? {
            Authorization::Allowed => Ok(()),
            Authorization::RequiresApproval => {
                Err(SecurityError::ApprovalRequired(capability).into())
            }
        }
    }
}

fn collect_files(root: &Path, files: &mut Vec<PathBuf>) -> std::io::Result<()> {
    for entry in fs::read_dir(root)? {
        let entry = entry?;
        let file_type = entry.file_type()?;
        let path = entry.path();
        if file_type.is_symlink() {
            continue;
        }
        if file_type.is_dir() {
            collect_files(&path, files)?;
        } else if file_type.is_file() {
            files.push(path);
        }
    }
    Ok(())
}
