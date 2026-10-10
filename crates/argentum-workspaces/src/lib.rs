use std::collections::BTreeSet;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};

use argentum_domain::{Capability, PatchFile, SandboxScope, Sha256Digest};
use argentum_security::{
    ApprovalGrant, Authorization, CapabilityBroker, ScopedPath, SecurityError,
};
use sha2::{Digest, Sha256};
use thiserror::Error;

mod search;
mod skills;
mod text;

pub use skills::{
    discover_skills, inspect_skill, select_skill, SelectedSkill, SkillCatalog, SkillError,
    SkillInspection, SkillSummary,
};

pub use search::{
    WorkspaceSearchReport, WorkspaceSearchStopReason, MAX_IGNORE_CHAIN_DEPTH,
    MAX_IGNORE_FILE_BYTES, MAX_IGNORE_RULES, MAX_IGNORE_TOTAL_BYTES, MAX_SEARCH_CONTEXT_LINES,
    MAX_SEARCH_DEPTH, MAX_SEARCH_DIRECTORIES, MAX_SEARCH_ENTRIES, MAX_SEARCH_FILES,
    MAX_SEARCH_FILE_BYTES, MAX_SEARCH_LINE_BYTES, MAX_SEARCH_MATCHES, MAX_SEARCH_QUERY_BYTES,
    MAX_SEARCH_TOTAL_BYTES,
};

#[derive(Debug, Error)]
pub enum WorkspaceError {
    #[error("path is excluded from automatic workspace context")]
    ContextExcluded,
    #[error("text input is invalid: {0}")]
    InvalidText(&'static str),
    #[error(transparent)]
    Security(#[from] SecurityError),
    #[error("workspace operation failed for {path}: {source}")]
    Io {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("file exceeds the {limit_bytes} byte read limit: {path}")]
    FileTooLarge { path: PathBuf, limit_bytes: usize },
    #[error("workspace {0} limit exceeded")]
    LimitExceeded(&'static str),
    #[error("workspace path is a symbolic link: {0}")]
    SymlinkDenied(PathBuf),
    #[error("workspace path is not a regular file: {0}")]
    NotFile(PathBuf),
    #[error("workspace path is not a directory: {0}")]
    NotDirectory(PathBuf),
    #[error("workspace patch must contain at least one file")]
    EmptyPatch,
    #[error("workspace patch exceeds the file or byte limit")]
    PatchLimitExceeded,
    #[error("workspace patch contains duplicate path: {0}")]
    DuplicateMutationPath(String),
    #[error("workspace patch expected digest does not match: {0}")]
    ExpectedDigestMismatch(String),
    #[error("workspace mutation is stale: {0}")]
    StaleMutation(String),
    #[error("workspace mutation would not change a file: {0}")]
    EmptyMutation(String),
    #[error("workspace patch rollback failed for path: {0}")]
    RollbackFailed(String),
    #[error("workspace mutation failed: {0}")]
    MutationFailed(String),
    #[error("workspace mutation was cancelled and rolled back")]
    Cancelled,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkspaceEntryKind {
    File,
    Directory,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkspaceDirectoryEntry {
    pub path: String,
    pub name: String,
    pub kind: WorkspaceEntryKind,
    pub size_bytes: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkspaceMetadata {
    pub path: String,
    pub kind: WorkspaceEntryKind,
    pub size_bytes: u64,
    pub readonly: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkspaceBinaryMetadata {
    pub path: String,
    pub size_bytes: u64,
    pub sha256: Sha256Digest,
    pub binary: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkspaceTextMatch {
    pub path: String,
    pub line_number: u64,
    pub line: String,
    pub line_truncated: bool,
    pub read_selection: WorkspaceTextReadSelection,
    pub before_context_requested: u8,
    pub after_context_requested: u8,
    pub before_context: Vec<WorkspaceTextContextLine>,
    pub after_context: Vec<WorkspaceTextContextLine>,
    pub context_complete: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkspaceTextReadSelection {
    pub path: String,
    pub start_line: u64,
    pub line_count: u16,
    pub expected_sha256: Sha256Digest,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkspaceTextContextLine {
    pub line_number: u64,
    pub line: String,
    pub truncated: bool,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct WorkspaceTextSearchOptions {
    pub mode: argentum_domain::TextSearchMode,
    pub before_context: u8,
    pub after_context: u8,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkspaceFilenameMatch {
    pub path: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WorkspaceRevisionLimits {
    pub max_files: usize,
    pub max_total_bytes: u64,
    pub max_file_bytes: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkspaceRevision {
    pub digest: Sha256Digest,
    pub file_count: u64,
    pub total_bytes: u64,
}

pub const MAX_PATCH_FILES: usize = 64;
pub const MAX_PATCH_TOTAL_BYTES: usize = 4 * 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreparedWorkspaceMutation {
    pub relative_path: String,
    pub before: Option<Vec<u8>>,
    pub after: Option<Vec<u8>>,
    pub before_sha256: Option<Sha256Digest>,
    pub after_sha256: Option<Sha256Digest>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreparedWorkspacePatch {
    pub mutations: Vec<PreparedWorkspaceMutation>,
    pub total_bytes: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkspaceFileCapture {
    pub relative_path: String,
    pub exists: bool,
    pub size_bytes: u64,
    pub bytes: Option<Vec<u8>>,
    pub too_large: bool,
    pub sha256: Option<String>,
    pub binary: bool,
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

    pub fn resolve_path_with_scope(
        &self,
        path: impl AsRef<Path>,
        scope: SandboxScope,
    ) -> Result<ScopedPath, WorkspaceError> {
        self.broker
            .resolve_path_with_scope(path, scope)
            .map_err(Into::into)
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

    pub fn read_context_file(
        &self,
        path: impl AsRef<Path>,
        limit_bytes: usize,
    ) -> Result<Option<String>, WorkspaceError> {
        let path = path.as_ref();
        if Self::context_path_is_excluded(path) {
            return Err(WorkspaceError::ContextExcluded);
        }
        let Some(file) = self.broker.open_context_file(path)? else {
            return Ok(None);
        };
        let mut bytes = Vec::with_capacity(limit_bytes.min(4096));
        file.take((limit_bytes as u64).saturating_add(1))
            .read_to_end(&mut bytes)
            .map_err(|source| WorkspaceError::Io {
                path: path.into(),
                source,
            })?;
        if bytes.len() > limit_bytes {
            return Err(WorkspaceError::FileTooLarge {
                path: path.into(),
                limit_bytes,
            });
        }
        let text = String::from_utf8(bytes)
            .map_err(|_| WorkspaceError::InvalidText("instruction source must be UTF-8"))?;
        if text.contains('\0') {
            return Err(WorkspaceError::InvalidText(
                "NUL-containing instruction source is unsupported",
            ));
        }
        Ok(Some(text))
    }

    pub fn context_path_is_excluded(path: &Path) -> bool {
        revision_path_is_ignored(path)
    }

    pub fn context_path_is_directory(&self, path: &Path) -> Result<Option<bool>, WorkspaceError> {
        Ok(self.broker.context_path_is_directory(path)?)
    }

    pub fn read_text_bounded_with_scope(
        &self,
        path: impl AsRef<Path>,
        limit_bytes: usize,
        scope: SandboxScope,
    ) -> Result<String, WorkspaceError> {
        match self.resolve_path_with_scope(path, scope)? {
            ScopedPath::Workspace { relative, .. } => self.read_text_bounded(relative, limit_bytes),
            ScopedPath::Ambient(absolute) => {
                let capture = capture_ambient_file(&absolute, limit_bytes)?;
                if !capture.exists {
                    return Err(WorkspaceError::Io {
                        path: absolute,
                        source: std::io::Error::new(std::io::ErrorKind::NotFound, "file not found"),
                    });
                }
                if capture.too_large {
                    return Err(WorkspaceError::FileTooLarge {
                        path: absolute,
                        limit_bytes,
                    });
                }
                String::from_utf8(capture.bytes.unwrap_or_default()).map_err(|source| {
                    WorkspaceError::Io {
                        path: absolute,
                        source: std::io::Error::new(std::io::ErrorKind::InvalidData, source),
                    }
                })
            }
        }
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

    pub fn capture_file(
        &self,
        path: impl AsRef<Path>,
        limit_bytes: usize,
    ) -> Result<WorkspaceFileCapture, WorkspaceError> {
        let relative = self.broker.relative_path(path)?;
        let relative_path = relative
            .components()
            .filter_map(|component| match component {
                std::path::Component::Normal(value) => Some(value.to_string_lossy()),
                std::path::Component::CurDir => None,
                _ => None,
            })
            .collect::<Vec<_>>()
            .join("/");
        if relative_path.is_empty() {
            return Err(WorkspaceError::NotFile(self.root().to_path_buf()));
        }
        // Windows reports AccessDenied when opening a directory as a file.
        // Preserve the actual type error while retaining the contained open.
        if self.broker.context_path_is_directory(&relative)? == Some(true) {
            return Err(WorkspaceError::NotFile(self.root().join(&relative)));
        }
        let (display_path, file) = match self.broker.open_file_for_read(&relative) {
            Ok(opened) => opened,
            Err(SecurityError::WorkspaceOperation { source, .. })
                if source.kind() == std::io::ErrorKind::NotFound =>
            {
                return Ok(WorkspaceFileCapture {
                    relative_path,
                    exists: false,
                    size_bytes: 0,
                    bytes: None,
                    too_large: false,
                    sha256: None,
                    binary: false,
                });
            }
            Err(error_value) => return Err(error_value.into()),
        };
        let metadata = file.metadata().map_err(|source| WorkspaceError::Io {
            path: display_path.clone(),
            source,
        })?;
        if !metadata.is_file() {
            return Err(WorkspaceError::NotFile(display_path));
        }
        let size_bytes = metadata.len();
        let mut file = file;
        let mut hasher = Sha256::new();
        let mut bytes = Vec::with_capacity(limit_bytes.min(64 * 1024));
        let mut buffer = [0_u8; 64 * 1024];
        let mut actual_size = 0_u64;
        let mut too_large = size_bytes > limit_bytes.try_into().unwrap_or(u64::MAX);
        loop {
            let read = file
                .read(&mut buffer)
                .map_err(|source| WorkspaceError::Io {
                    path: display_path.clone(),
                    source,
                })?;
            if read == 0 {
                break;
            }
            actual_size = actual_size.saturating_add(read as u64);
            hasher.update(&buffer[..read]);
            if !too_large && bytes.len().saturating_add(read) <= limit_bytes {
                bytes.extend_from_slice(&buffer[..read]);
            } else {
                too_large = true;
                bytes.clear();
            }
        }
        let binary = !too_large && (std::str::from_utf8(&bytes).is_err() || bytes.contains(&0));
        Ok(WorkspaceFileCapture {
            relative_path,
            exists: true,
            size_bytes: actual_size,
            bytes: (!too_large).then_some(bytes),
            too_large,
            sha256: Some(format!("{:x}", hasher.finalize())),
            binary,
        })
    }

    pub fn capture_file_with_scope(
        &self,
        path: impl AsRef<Path>,
        limit_bytes: usize,
        scope: SandboxScope,
    ) -> Result<WorkspaceFileCapture, WorkspaceError> {
        match self.resolve_path_with_scope(path, scope)? {
            ScopedPath::Workspace { relative, .. } => self.capture_file(relative, limit_bytes),
            ScopedPath::Ambient(absolute) => capture_ambient_file(&absolute, limit_bytes),
        }
    }

    pub fn write_bytes_with_grant(
        &self,
        path: impl AsRef<Path>,
        content: &[u8],
        grant: &ApprovalGrant,
    ) -> Result<(), WorkspaceError> {
        self.broker
            .write_file_with_grant(path, content, grant)
            .map_err(Into::into)
    }

    pub fn write_bytes_with_scope_and_grant(
        &self,
        path: impl AsRef<Path>,
        content: &[u8],
        grant: &ApprovalGrant,
        scope: SandboxScope,
    ) -> Result<(), WorkspaceError> {
        self.apply_image(
            &path.as_ref().to_string_lossy(),
            Some(content),
            grant,
            scope,
        )
    }

    pub fn delete_file_with_grant(
        &self,
        path: impl AsRef<Path>,
        grant: &ApprovalGrant,
    ) -> Result<(), WorkspaceError> {
        self.broker
            .remove_file_with_grant(path, grant)
            .map_err(Into::into)
    }

    pub fn delete_file_with_scope_and_grant(
        &self,
        path: impl AsRef<Path>,
        grant: &ApprovalGrant,
        scope: SandboxScope,
    ) -> Result<(), WorkspaceError> {
        self.apply_image(&path.as_ref().to_string_lossy(), None, grant, scope)
    }

    pub fn create_directory_with_grant(
        &self,
        path: impl AsRef<Path>,
        grant: &ApprovalGrant,
    ) -> Result<(), WorkspaceError> {
        self.broker
            .create_dir_all_with_grant(path, grant)
            .map_err(Into::into)
    }

    pub fn create_directory_with_scope_and_grant(
        &self,
        path: impl AsRef<Path>,
        grant: &ApprovalGrant,
        scope: SandboxScope,
    ) -> Result<(), WorkspaceError> {
        self.create_directory_with_scope_and_grant_cancellable(path, grant, scope, || false)
    }

    pub fn create_directory_with_scope_and_grant_cancellable<F>(
        &self,
        path: impl AsRef<Path>,
        grant: &ApprovalGrant,
        scope: SandboxScope,
        mut is_cancelled: F,
    ) -> Result<(), WorkspaceError>
    where
        F: FnMut() -> bool,
    {
        if is_cancelled() {
            return Err(WorkspaceError::Cancelled);
        }
        let scoped = self.resolve_path_with_scope(path, scope)?;
        let absolute = match &scoped {
            ScopedPath::Workspace { absolute, .. } | ScopedPath::Ambient(absolute) => {
                absolute.clone()
            }
        };
        let mut created_directories = Vec::new();
        let mut candidate = absolute.as_path();
        while !candidate.exists() {
            created_directories.push(candidate.to_path_buf());
            let Some(parent) = candidate.parent() else {
                break;
            };
            candidate = parent;
        }
        match scoped {
            ScopedPath::Workspace { relative, .. } => {
                self.create_directory_with_grant(relative, grant)
            }
            ScopedPath::Ambient(absolute) => {
                if !grant.allows(Capability::WriteFiles) {
                    return Err(SecurityError::ApprovalRequired(Capability::WriteFiles).into());
                }
                fs::create_dir_all(&absolute).map_err(|source| WorkspaceError::Io {
                    path: absolute,
                    source,
                })
            }
        }?;
        if !is_cancelled() {
            return Ok(());
        }
        for directory in created_directories {
            match fs::remove_dir(&directory) {
                Ok(()) => {}
                Err(source) if source.kind() == std::io::ErrorKind::NotFound => {}
                Err(_) => {
                    return Err(WorkspaceError::RollbackFailed(
                        directory.to_string_lossy().into_owned(),
                    ));
                }
            }
        }
        Err(WorkspaceError::Cancelled)
    }

    pub fn prepare_patch(
        &self,
        files: &[PatchFile],
    ) -> Result<PreparedWorkspacePatch, WorkspaceError> {
        self.prepare_patch_with_scope(files, SandboxScope::WorkspaceWrite)
    }

    pub fn prepare_patch_with_scope(
        &self,
        files: &[PatchFile],
        scope: SandboxScope,
    ) -> Result<PreparedWorkspacePatch, WorkspaceError> {
        let images = files
            .iter()
            .map(|file| {
                (
                    file.path.clone(),
                    file.expected_sha256.clone(),
                    file.content
                        .as_ref()
                        .map(|content| content.as_bytes().to_vec()),
                )
            })
            .collect();
        self.prepare_patch_images_with_scope(images, scope)
    }

    fn prepare_patch_images_with_scope(
        &self,
        files: Vec<(String, Option<Sha256Digest>, Option<Vec<u8>>)>,
        scope: SandboxScope,
    ) -> Result<PreparedWorkspacePatch, WorkspaceError> {
        if files.is_empty() {
            return Err(WorkspaceError::EmptyPatch);
        }
        if files.len() > MAX_PATCH_FILES {
            return Err(WorkspaceError::PatchLimitExceeded);
        }

        let mut paths = BTreeSet::new();
        let mut mutations = Vec::with_capacity(files.len());
        let mut total_bytes = 0_usize;
        for (path, expected_sha256, after) in files {
            let relative_path = match self.resolve_path_with_scope(&path, scope)? {
                ScopedPath::Workspace { relative, .. } => normalize_display_path(&relative),
                ScopedPath::Ambient(absolute) => absolute.to_string_lossy().into_owned(),
            };
            if relative_path.is_empty() {
                return Err(SecurityError::OutsideWorkspace(path.into()).into());
            }
            if !paths.insert(path_identity(&relative_path)) {
                return Err(WorkspaceError::DuplicateMutationPath(relative_path));
            }
            let capture =
                self.capture_file_with_scope(&relative_path, MAX_PATCH_TOTAL_BYTES, scope)?;
            if capture.too_large {
                return Err(WorkspaceError::PatchLimitExceeded);
            }
            if expected_sha256.as_ref().map(|digest| digest.0.as_str())
                != expected_sha256
                    .as_ref()
                    .map(|_| capture.sha256.as_deref().unwrap_or_default())
            {
                return Err(WorkspaceError::ExpectedDigestMismatch(relative_path));
            }
            let before = capture.exists.then(|| capture.bytes.unwrap_or_default());
            if before == after {
                return Err(WorkspaceError::EmptyMutation(relative_path));
            }
            if before.is_none() && after.is_none() {
                return Err(WorkspaceError::EmptyMutation(relative_path));
            }
            total_bytes = total_bytes
                .saturating_add(before.as_ref().map_or(0, Vec::len))
                .saturating_add(after.as_ref().map_or(0, Vec::len));
            if total_bytes > MAX_PATCH_TOTAL_BYTES {
                return Err(WorkspaceError::PatchLimitExceeded);
            }
            mutations.push(PreparedWorkspaceMutation {
                relative_path,
                before_sha256: before.as_deref().map(sha256_digest),
                after_sha256: after.as_deref().map(sha256_digest),
                before,
                after,
            });
        }
        Ok(PreparedWorkspacePatch {
            mutations,
            total_bytes: total_bytes.try_into().unwrap_or(u64::MAX),
        })
    }

    pub fn prepare_copy_file(
        &self,
        from: impl AsRef<Path>,
        to: impl AsRef<Path>,
    ) -> Result<PreparedWorkspacePatch, WorkspaceError> {
        self.prepare_copy_file_with_scope(from, to, SandboxScope::WorkspaceWrite)
    }

    pub fn prepare_copy_file_with_scope(
        &self,
        from: impl AsRef<Path>,
        to: impl AsRef<Path>,
        scope: SandboxScope,
    ) -> Result<PreparedWorkspacePatch, WorkspaceError> {
        let source = self.capture_file_with_scope(from, MAX_PATCH_TOTAL_BYTES, scope)?;
        let bytes = source
            .exists
            .then_some(source.bytes)
            .flatten()
            .ok_or_else(|| WorkspaceError::NotFile(self.root().join(source.relative_path)))?;
        self.prepare_patch_images_with_scope(
            vec![(
                to.as_ref().to_string_lossy().into_owned(),
                None,
                Some(bytes),
            )],
            scope,
        )
    }

    pub fn prepare_move_file(
        &self,
        from: impl AsRef<Path>,
        to: impl AsRef<Path>,
    ) -> Result<PreparedWorkspacePatch, WorkspaceError> {
        self.prepare_move_file_with_scope(from, to, SandboxScope::WorkspaceWrite)
    }

    pub fn prepare_move_file_with_scope(
        &self,
        from: impl AsRef<Path>,
        to: impl AsRef<Path>,
        scope: SandboxScope,
    ) -> Result<PreparedWorkspacePatch, WorkspaceError> {
        let source_path = from.as_ref().to_string_lossy().into_owned();
        let source = self.capture_file_with_scope(&source_path, MAX_PATCH_TOTAL_BYTES, scope)?;
        let bytes = source
            .exists
            .then_some(source.bytes)
            .flatten()
            .ok_or_else(|| WorkspaceError::NotFile(self.root().join(source.relative_path)))?;
        self.prepare_patch_images_with_scope(
            vec![
                (
                    to.as_ref().to_string_lossy().into_owned(),
                    None,
                    Some(bytes),
                ),
                (source_path, None, None),
            ],
            scope,
        )
    }

    pub fn prepare_delete_file(
        &self,
        path: impl AsRef<Path>,
    ) -> Result<PreparedWorkspacePatch, WorkspaceError> {
        self.prepare_delete_file_with_scope(path, SandboxScope::WorkspaceWrite)
    }

    pub fn prepare_delete_file_with_scope(
        &self,
        path: impl AsRef<Path>,
        scope: SandboxScope,
    ) -> Result<PreparedWorkspacePatch, WorkspaceError> {
        self.prepare_patch_with_scope(
            &[PatchFile {
                path: path.as_ref().to_string_lossy().into_owned(),
                expected_sha256: None,
                content: None,
            }],
            scope,
        )
    }

    pub fn apply_prepared_patch_with_grant(
        &self,
        patch: &PreparedWorkspacePatch,
        grant: &ApprovalGrant,
    ) -> Result<(), WorkspaceError> {
        self.apply_prepared_patch_with_scope_and_grant(patch, grant, SandboxScope::WorkspaceWrite)
    }

    pub fn apply_prepared_patch_with_scope_and_grant(
        &self,
        patch: &PreparedWorkspacePatch,
        grant: &ApprovalGrant,
        scope: SandboxScope,
    ) -> Result<(), WorkspaceError> {
        self.apply_prepared_patch_internal(patch, grant, scope, None, &mut || false)
    }

    pub fn apply_prepared_patch_with_scope_and_grant_cancellable<F>(
        &self,
        patch: &PreparedWorkspacePatch,
        grant: &ApprovalGrant,
        scope: SandboxScope,
        mut is_cancelled: F,
    ) -> Result<(), WorkspaceError>
    where
        F: FnMut() -> bool,
    {
        self.apply_prepared_patch_internal(patch, grant, scope, None, &mut is_cancelled)
    }

    #[cfg(test)]
    fn apply_prepared_patch_with_test_failure(
        &self,
        patch: &PreparedWorkspacePatch,
        grant: &ApprovalGrant,
        fail_at: usize,
    ) -> Result<(), WorkspaceError> {
        self.apply_prepared_patch_internal(
            patch,
            grant,
            SandboxScope::WorkspaceWrite,
            Some(fail_at),
            &mut || false,
        )
    }

    fn apply_prepared_patch_internal(
        &self,
        patch: &PreparedWorkspacePatch,
        grant: &ApprovalGrant,
        scope: SandboxScope,
        fail_at: Option<usize>,
        is_cancelled: &mut dyn FnMut() -> bool,
    ) -> Result<(), WorkspaceError> {
        if patch.mutations.is_empty() || patch.mutations.len() > MAX_PATCH_FILES {
            return Err(WorkspaceError::EmptyPatch);
        }
        if is_cancelled() {
            return Err(WorkspaceError::Cancelled);
        }
        for mutation in &patch.mutations {
            self.ensure_mutation_current(mutation, scope)?;
        }

        let created_directories = self.missing_patch_parent_directories(patch);
        let mut applied = 0_usize;
        let mut failure = None;
        for (index, mutation) in patch.mutations.iter().enumerate() {
            if is_cancelled() {
                failure = Some(WorkspaceError::Cancelled);
                break;
            }
            if fail_at == Some(index) {
                failure = Some(WorkspaceError::MutationFailed(
                    "injected test failure".into(),
                ));
                break;
            }
            let result = self
                .apply_image(
                    &mutation.relative_path,
                    mutation.after.as_deref(),
                    grant,
                    scope,
                )
                .and_then(|()| self.ensure_mutation_after(mutation, scope));
            match result {
                Ok(()) => {
                    applied = applied.saturating_add(1);
                    if is_cancelled() {
                        failure = Some(WorkspaceError::Cancelled);
                        break;
                    }
                }
                Err(error_value) => {
                    failure = Some(error_value);
                    break;
                }
            }
        }
        let Some(failure) = failure else {
            return Ok(());
        };

        for mutation in patch.mutations[..applied].iter().rev() {
            if self
                .apply_image(
                    &mutation.relative_path,
                    mutation.before.as_deref(),
                    grant,
                    scope,
                )
                .and_then(|()| self.ensure_mutation_before(mutation, scope))
                .is_err()
            {
                return Err(WorkspaceError::RollbackFailed(
                    mutation.relative_path.clone(),
                ));
            }
        }
        for directory in created_directories {
            if directory.is_dir() {
                let _ = fs::remove_dir(&directory);
            }
        }
        Err(failure)
    }

    fn apply_image(
        &self,
        relative_path: &str,
        image: Option<&[u8]>,
        grant: &ApprovalGrant,
        scope: SandboxScope,
    ) -> Result<(), WorkspaceError> {
        match self.resolve_path_with_scope(relative_path, scope)? {
            ScopedPath::Workspace { relative, .. } => {
                if let Some(image) = image {
                    self.write_bytes_with_grant(relative, image, grant)
                } else {
                    self.delete_file_with_grant(relative, grant)
                }
            }
            ScopedPath::Ambient(absolute) => {
                if !grant.allows(Capability::WriteFiles) {
                    return Err(SecurityError::ApprovalRequired(Capability::WriteFiles).into());
                }
                if let Some(image) = image {
                    if let Some(parent) = absolute.parent() {
                        fs::create_dir_all(parent).map_err(|source| WorkspaceError::Io {
                            path: parent.to_path_buf(),
                            source,
                        })?;
                    }
                    fs::write(&absolute, image).map_err(|source| WorkspaceError::Io {
                        path: absolute,
                        source,
                    })
                } else {
                    fs::remove_file(&absolute).map_err(|source| WorkspaceError::Io {
                        path: absolute,
                        source,
                    })
                }
            }
        }
    }

    fn ensure_mutation_current(
        &self,
        mutation: &PreparedWorkspaceMutation,
        scope: SandboxScope,
    ) -> Result<(), WorkspaceError> {
        self.ensure_capture_matches(
            &mutation.relative_path,
            mutation.before.as_deref(),
            mutation.before_sha256.as_ref(),
            "current",
            scope,
        )
    }

    fn ensure_mutation_after(
        &self,
        mutation: &PreparedWorkspaceMutation,
        scope: SandboxScope,
    ) -> Result<(), WorkspaceError> {
        self.ensure_capture_matches(
            &mutation.relative_path,
            mutation.after.as_deref(),
            mutation.after_sha256.as_ref(),
            "after",
            scope,
        )
    }

    fn ensure_mutation_before(
        &self,
        mutation: &PreparedWorkspaceMutation,
        scope: SandboxScope,
    ) -> Result<(), WorkspaceError> {
        self.ensure_capture_matches(
            &mutation.relative_path,
            mutation.before.as_deref(),
            mutation.before_sha256.as_ref(),
            "rollback",
            scope,
        )
    }

    fn ensure_capture_matches(
        &self,
        relative_path: &str,
        expected: Option<&[u8]>,
        expected_sha256: Option<&Sha256Digest>,
        _stage: &'static str,
        scope: SandboxScope,
    ) -> Result<(), WorkspaceError> {
        let capture = self.capture_file_with_scope(relative_path, MAX_PATCH_TOTAL_BYTES, scope)?;
        let matches = match expected {
            Some(_) => {
                capture.exists
                    && capture
                        .sha256
                        .as_deref()
                        .zip(expected_sha256.map(|digest| digest.0.as_str()))
                        .is_some_and(|(actual, expected)| actual == expected)
            }
            None => !capture.exists,
        };
        if matches {
            Ok(())
        } else {
            Err(WorkspaceError::StaleMutation(relative_path.to_owned()))
        }
    }

    fn missing_patch_parent_directories(&self, patch: &PreparedWorkspacePatch) -> Vec<PathBuf> {
        let mut directories = BTreeSet::new();
        for mutation in &patch.mutations {
            if mutation.after.is_none() {
                continue;
            }
            let mut parent = Path::new(&mutation.relative_path).parent();
            while let Some(value) = parent.filter(|value| !value.as_os_str().is_empty()) {
                let absolute = if value.is_absolute() {
                    value.to_path_buf()
                } else {
                    self.root().join(value)
                };
                if !absolute.exists() {
                    directories.insert(absolute);
                }
                parent = value.parent();
            }
        }
        let mut directories = directories.into_iter().collect::<Vec<_>>();
        directories.sort_by_key(|path| std::cmp::Reverse(path.components().count()));
        directories
    }

    pub fn list_directory_bounded(
        &self,
        path: impl AsRef<Path>,
        max_entries: usize,
    ) -> Result<Vec<WorkspaceDirectoryEntry>, WorkspaceError> {
        if max_entries == 0 {
            return Err(WorkspaceError::LimitExceeded("directory entries"));
        }
        let relative = self.broker.relative_path(path)?;
        let absolute = self.root().join(&relative);
        let metadata = fs::symlink_metadata(&absolute).map_err(|source| WorkspaceError::Io {
            path: absolute.clone(),
            source,
        })?;
        if metadata.file_type().is_symlink() {
            return Err(WorkspaceError::SymlinkDenied(absolute));
        }
        if !metadata.is_dir() {
            return Err(WorkspaceError::NotDirectory(absolute));
        }

        let mut entries = Vec::new();
        for entry in fs::read_dir(&absolute).map_err(|source| WorkspaceError::Io {
            path: absolute.clone(),
            source,
        })? {
            let entry = entry.map_err(|source| WorkspaceError::Io {
                path: absolute.clone(),
                source,
            })?;
            let file_type = entry.file_type().map_err(|source| WorkspaceError::Io {
                path: entry.path(),
                source,
            })?;
            if file_type.is_symlink() {
                continue;
            }
            let kind = if file_type.is_dir() {
                WorkspaceEntryKind::Directory
            } else if file_type.is_file() {
                WorkspaceEntryKind::File
            } else {
                continue;
            };
            let metadata = entry.metadata().map_err(|source| WorkspaceError::Io {
                path: entry.path(),
                source,
            })?;
            let name = entry.file_name().to_string_lossy().into_owned();
            entries.push(WorkspaceDirectoryEntry {
                path: normalize_display_path(&relative.join(&name)),
                name,
                kind,
                size_bytes: metadata.len(),
            });
            if entries.len() > max_entries {
                return Err(WorkspaceError::LimitExceeded("directory entries"));
            }
        }
        entries.sort_by(|left, right| left.name.cmp(&right.name));
        Ok(entries)
    }

    pub fn list_directory_bounded_with_scope(
        &self,
        path: impl AsRef<Path>,
        max_entries: usize,
        scope: SandboxScope,
    ) -> Result<Vec<WorkspaceDirectoryEntry>, WorkspaceError> {
        match self.resolve_path_with_scope(path, scope)? {
            ScopedPath::Workspace { relative, .. } => {
                self.list_directory_bounded(relative, max_entries)
            }
            ScopedPath::Ambient(absolute) => list_ambient_directory(&absolute, max_entries),
        }
    }

    pub fn file_metadata(
        &self,
        path: impl AsRef<Path>,
    ) -> Result<WorkspaceMetadata, WorkspaceError> {
        let relative = self.broker.relative_path(path)?;
        let absolute = self.root().join(&relative);
        let metadata = fs::symlink_metadata(&absolute).map_err(|source| WorkspaceError::Io {
            path: absolute.clone(),
            source,
        })?;
        if metadata.file_type().is_symlink() {
            return Err(WorkspaceError::SymlinkDenied(absolute));
        }
        let kind = if metadata.is_file() {
            WorkspaceEntryKind::File
        } else if metadata.is_dir() {
            WorkspaceEntryKind::Directory
        } else {
            return Err(WorkspaceError::NotFile(absolute));
        };
        Ok(WorkspaceMetadata {
            path: normalize_display_path(&relative),
            kind,
            size_bytes: metadata.len(),
            readonly: metadata.permissions().readonly(),
        })
    }

    pub fn file_metadata_with_scope(
        &self,
        path: impl AsRef<Path>,
        scope: SandboxScope,
    ) -> Result<WorkspaceMetadata, WorkspaceError> {
        match self.resolve_path_with_scope(path, scope)? {
            ScopedPath::Workspace { relative, .. } => self.file_metadata(relative),
            ScopedPath::Ambient(absolute) => ambient_metadata(&absolute),
        }
    }

    pub fn binary_metadata_bounded(
        &self,
        path: impl AsRef<Path>,
        max_bytes: usize,
    ) -> Result<WorkspaceBinaryMetadata, WorkspaceError> {
        let capture = self.capture_file(path, max_bytes)?;
        if !capture.exists {
            return Err(WorkspaceError::Io {
                path: self.root().join(&capture.relative_path),
                source: std::io::Error::new(std::io::ErrorKind::NotFound, "file not found"),
            });
        }
        if capture.too_large {
            return Err(WorkspaceError::FileTooLarge {
                path: self.root().join(&capture.relative_path),
                limit_bytes: max_bytes,
            });
        }
        Ok(WorkspaceBinaryMetadata {
            path: capture.relative_path,
            size_bytes: capture.size_bytes,
            sha256: Sha256Digest(capture.sha256.unwrap_or_default()),
            binary: capture.binary,
        })
    }

    pub fn binary_metadata_bounded_with_scope(
        &self,
        path: impl AsRef<Path>,
        max_bytes: usize,
        scope: SandboxScope,
    ) -> Result<WorkspaceBinaryMetadata, WorkspaceError> {
        match self.resolve_path_with_scope(path, scope)? {
            ScopedPath::Workspace { relative, .. } => {
                self.binary_metadata_bounded(relative, max_bytes)
            }
            ScopedPath::Ambient(absolute) => {
                let capture = capture_ambient_file(&absolute, max_bytes)?;
                if !capture.exists {
                    return Err(WorkspaceError::Io {
                        path: absolute,
                        source: std::io::Error::new(std::io::ErrorKind::NotFound, "file not found"),
                    });
                }
                if capture.too_large {
                    return Err(WorkspaceError::FileTooLarge {
                        path: absolute,
                        limit_bytes: max_bytes,
                    });
                }
                Ok(WorkspaceBinaryMetadata {
                    path: capture.relative_path,
                    size_bytes: capture.size_bytes,
                    sha256: Sha256Digest(capture.sha256.unwrap_or_default()),
                    binary: capture.binary,
                })
            }
        }
    }

    pub fn search_text_bounded(
        &self,
        path: impl AsRef<Path>,
        query: &str,
        max_results: usize,
    ) -> Result<Vec<WorkspaceTextMatch>, WorkspaceError> {
        Ok(self
            .search_text_bounded_report(path, query, max_results)?
            .matches)
    }

    pub fn search_text_bounded_report(
        &self,
        path: impl AsRef<Path>,
        query: &str,
        max_results: usize,
    ) -> Result<WorkspaceSearchReport<WorkspaceTextMatch>, WorkspaceError> {
        self.search_text_bounded_report_with_cancellation(path, query, max_results, || false)
    }

    pub fn search_text_bounded_report_with_options(
        &self,
        path: impl AsRef<Path>,
        query: &str,
        max_results: usize,
        options: WorkspaceTextSearchOptions,
    ) -> Result<WorkspaceSearchReport<WorkspaceTextMatch>, WorkspaceError> {
        self.search_text_bounded_report_with_options_and_cancellation(
            path,
            query,
            max_results,
            options,
            || false,
        )
    }

    pub fn search_text_bounded_report_with_cancellation<F: FnMut() -> bool>(
        &self,
        path: impl AsRef<Path>,
        query: &str,
        max_results: usize,
        cancellation: F,
    ) -> Result<WorkspaceSearchReport<WorkspaceTextMatch>, WorkspaceError> {
        self.search_text_bounded_report_with_options_and_cancellation(
            path,
            query,
            max_results,
            WorkspaceTextSearchOptions::default(),
            cancellation,
        )
    }

    pub fn search_text_bounded_report_with_options_and_cancellation<F: FnMut() -> bool>(
        &self,
        path: impl AsRef<Path>,
        query: &str,
        max_results: usize,
        options: WorkspaceTextSearchOptions,
        cancellation: F,
    ) -> Result<WorkspaceSearchReport<WorkspaceTextMatch>, WorkspaceError> {
        match self
            .broker
            .resolve_path_with_scope(path, SandboxScope::WorkspaceWrite)?
        {
            ScopedPath::Workspace { relative, .. } => search::workspace_text(
                &self.broker,
                &relative,
                query,
                max_results,
                options,
                cancellation,
            ),
            ScopedPath::Ambient(_) => unreachable!("workspace-only search resolution"),
        }
    }

    pub fn search_text_bounded_with_scope(
        &self,
        path: impl AsRef<Path>,
        query: &str,
        max_results: usize,
        scope: SandboxScope,
    ) -> Result<Vec<WorkspaceTextMatch>, WorkspaceError> {
        Ok(self
            .search_text_bounded_report_with_scope(path, query, max_results, scope)?
            .matches)
    }

    pub fn search_text_bounded_report_with_scope(
        &self,
        path: impl AsRef<Path>,
        query: &str,
        max_results: usize,
        scope: SandboxScope,
    ) -> Result<WorkspaceSearchReport<WorkspaceTextMatch>, WorkspaceError> {
        self.search_text_bounded_report_with_scope_and_cancellation(
            path,
            query,
            max_results,
            scope,
            || false,
        )
    }

    pub fn search_text_bounded_report_with_scope_and_cancellation<F: FnMut() -> bool>(
        &self,
        path: impl AsRef<Path>,
        query: &str,
        max_results: usize,
        scope: SandboxScope,
        cancellation: F,
    ) -> Result<WorkspaceSearchReport<WorkspaceTextMatch>, WorkspaceError> {
        self.search_text_bounded_report_with_options_and_scope_and_cancellation(
            path,
            query,
            max_results,
            WorkspaceTextSearchOptions::default(),
            scope,
            cancellation,
        )
    }

    pub fn search_text_bounded_report_with_options_and_scope_and_cancellation<
        F: FnMut() -> bool,
    >(
        &self,
        path: impl AsRef<Path>,
        query: &str,
        max_results: usize,
        options: WorkspaceTextSearchOptions,
        scope: SandboxScope,
        cancellation: F,
    ) -> Result<WorkspaceSearchReport<WorkspaceTextMatch>, WorkspaceError> {
        match self.resolve_path_with_scope(path, scope)? {
            ScopedPath::Workspace { relative, .. } => search::workspace_text(
                &self.broker,
                &relative,
                query,
                max_results,
                options,
                cancellation,
            ),
            ScopedPath::Ambient(absolute) => {
                search::ambient_text(&absolute, query, max_results, options, cancellation)
            }
        }
    }

    pub fn search_filenames_bounded(
        &self,
        path: impl AsRef<Path>,
        pattern: &str,
        max_results: usize,
    ) -> Result<Vec<WorkspaceFilenameMatch>, WorkspaceError> {
        Ok(self
            .search_filenames_bounded_report(path, pattern, max_results)?
            .matches)
    }

    pub fn search_filenames_bounded_report(
        &self,
        path: impl AsRef<Path>,
        pattern: &str,
        max_results: usize,
    ) -> Result<WorkspaceSearchReport<WorkspaceFilenameMatch>, WorkspaceError> {
        self.search_filenames_bounded_report_with_cancellation(path, pattern, max_results, || false)
    }

    pub fn search_filenames_bounded_report_with_cancellation<F: FnMut() -> bool>(
        &self,
        path: impl AsRef<Path>,
        pattern: &str,
        max_results: usize,
        cancellation: F,
    ) -> Result<WorkspaceSearchReport<WorkspaceFilenameMatch>, WorkspaceError> {
        match self
            .broker
            .resolve_path_with_scope(path, SandboxScope::WorkspaceWrite)?
        {
            ScopedPath::Workspace { relative, .. } => search::workspace_filenames(
                &self.broker,
                &relative,
                pattern,
                max_results,
                cancellation,
            ),
            ScopedPath::Ambient(_) => unreachable!("workspace-only search resolution"),
        }
    }

    pub fn search_filenames_bounded_with_scope(
        &self,
        path: impl AsRef<Path>,
        pattern: &str,
        max_results: usize,
        scope: SandboxScope,
    ) -> Result<Vec<WorkspaceFilenameMatch>, WorkspaceError> {
        Ok(self
            .search_filenames_bounded_report_with_scope(path, pattern, max_results, scope)?
            .matches)
    }

    pub fn search_filenames_bounded_report_with_scope(
        &self,
        path: impl AsRef<Path>,
        pattern: &str,
        max_results: usize,
        scope: SandboxScope,
    ) -> Result<WorkspaceSearchReport<WorkspaceFilenameMatch>, WorkspaceError> {
        self.search_filenames_bounded_report_with_scope_and_cancellation(
            path,
            pattern,
            max_results,
            scope,
            || false,
        )
    }

    pub fn search_filenames_bounded_report_with_scope_and_cancellation<F: FnMut() -> bool>(
        &self,
        path: impl AsRef<Path>,
        pattern: &str,
        max_results: usize,
        scope: SandboxScope,
        cancellation: F,
    ) -> Result<WorkspaceSearchReport<WorkspaceFilenameMatch>, WorkspaceError> {
        match self.resolve_path_with_scope(path, scope)? {
            ScopedPath::Workspace { relative, .. } => search::workspace_filenames(
                &self.broker,
                &relative,
                pattern,
                max_results,
                cancellation,
            ),
            ScopedPath::Ambient(absolute) => {
                search::ambient_filenames(&absolute, pattern, max_results, cancellation)
            }
        }
    }

    pub fn revision_digest(
        &self,
        limits: WorkspaceRevisionLimits,
    ) -> Result<WorkspaceRevision, WorkspaceError> {
        if limits.max_files == 0 || limits.max_total_bytes == 0 || limits.max_file_bytes == 0 {
            return Err(WorkspaceError::LimitExceeded("revision bounds"));
        }
        let mut files = self
            .list_files()?
            .into_iter()
            .filter_map(|absolute| {
                let relative = absolute.strip_prefix(self.root()).ok()?.to_path_buf();
                (!revision_path_is_ignored(&relative)).then_some((relative, absolute))
            })
            .collect::<Vec<_>>();
        files.sort_by(|left, right| left.0.cmp(&right.0));
        if files.len() > limits.max_files {
            return Err(WorkspaceError::LimitExceeded("revision files"));
        }

        let mut hasher = Sha256::new();
        let mut total_bytes = 0_u64;
        for (relative, absolute) in &files {
            let metadata = fs::metadata(absolute).map_err(|source| WorkspaceError::Io {
                path: absolute.clone(),
                source,
            })?;
            if metadata.len() > limits.max_file_bytes {
                return Err(WorkspaceError::LimitExceeded("revision file bytes"));
            }
            total_bytes = total_bytes.saturating_add(metadata.len());
            if total_bytes > limits.max_total_bytes {
                return Err(WorkspaceError::LimitExceeded("revision total bytes"));
            }
            let max_bytes = usize::try_from(limits.max_file_bytes)
                .map_err(|_| WorkspaceError::LimitExceeded("revision file bytes"))?;
            let capture = self.capture_file(relative, max_bytes)?;
            let bytes = capture
                .bytes
                .ok_or(WorkspaceError::LimitExceeded("revision file bytes"))?;
            let display = normalize_display_path(relative);
            hasher.update((display.len() as u64).to_le_bytes());
            hasher.update(display.as_bytes());
            hasher.update(metadata.len().to_le_bytes());
            hasher.update(&bytes);
        }
        Ok(WorkspaceRevision {
            digest: Sha256Digest(format!("{:x}", hasher.finalize())),
            file_count: files.len().try_into().unwrap_or(u64::MAX),
            total_bytes,
        })
    }

    pub fn list_files(&self) -> Result<Vec<PathBuf>, WorkspaceError> {
        self.broker.list_workspace_files().map_err(Into::into)
    }
}

fn normalize_display_path(path: &Path) -> String {
    path.components()
        .filter_map(|component| match component {
            std::path::Component::Normal(value) => Some(value.to_string_lossy()),
            std::path::Component::CurDir => None,
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("/")
}

fn path_identity(path: &str) -> String {
    if cfg!(windows) {
        path.to_ascii_lowercase()
    } else {
        path.to_owned()
    }
}

fn capture_ambient_file(
    absolute: &Path,
    limit_bytes: usize,
) -> Result<WorkspaceFileCapture, WorkspaceError> {
    let display = absolute.to_string_lossy().into_owned();
    if fs::metadata(absolute).is_ok_and(|metadata| !metadata.is_file()) {
        return Err(WorkspaceError::NotFile(absolute.to_path_buf()));
    }
    let mut file = match fs::File::open(absolute) {
        Ok(file) => file,
        Err(source) if source.kind() == std::io::ErrorKind::NotFound => {
            return Ok(WorkspaceFileCapture {
                relative_path: display,
                exists: false,
                size_bytes: 0,
                bytes: None,
                too_large: false,
                sha256: None,
                binary: false,
            });
        }
        Err(source) => {
            return Err(WorkspaceError::Io {
                path: absolute.to_path_buf(),
                source,
            });
        }
    };
    let metadata = file.metadata().map_err(|source| WorkspaceError::Io {
        path: absolute.to_path_buf(),
        source,
    })?;
    if !metadata.is_file() {
        return Err(WorkspaceError::NotFile(absolute.to_path_buf()));
    }
    let mut hasher = Sha256::new();
    let mut bytes = Vec::with_capacity(limit_bytes.min(64 * 1024));
    let mut buffer = [0_u8; 64 * 1024];
    let mut actual_size = 0_u64;
    let mut too_large = metadata.len() > limit_bytes.try_into().unwrap_or(u64::MAX);
    loop {
        let read = file
            .read(&mut buffer)
            .map_err(|source| WorkspaceError::Io {
                path: absolute.to_path_buf(),
                source,
            })?;
        if read == 0 {
            break;
        }
        actual_size = actual_size.saturating_add(read as u64);
        hasher.update(&buffer[..read]);
        if !too_large && bytes.len().saturating_add(read) <= limit_bytes {
            bytes.extend_from_slice(&buffer[..read]);
        } else {
            too_large = true;
            bytes.clear();
        }
    }
    let binary = !too_large && (std::str::from_utf8(&bytes).is_err() || bytes.contains(&0));
    Ok(WorkspaceFileCapture {
        relative_path: display,
        exists: true,
        size_bytes: actual_size,
        bytes: (!too_large).then_some(bytes),
        too_large,
        sha256: Some(format!("{:x}", hasher.finalize())),
        binary,
    })
}

fn list_ambient_directory(
    absolute: &Path,
    max_entries: usize,
) -> Result<Vec<WorkspaceDirectoryEntry>, WorkspaceError> {
    if max_entries == 0 {
        return Err(WorkspaceError::LimitExceeded("directory entries"));
    }
    let metadata = fs::symlink_metadata(absolute).map_err(|source| WorkspaceError::Io {
        path: absolute.to_path_buf(),
        source,
    })?;
    if metadata.file_type().is_symlink() {
        return Err(WorkspaceError::SymlinkDenied(absolute.to_path_buf()));
    }
    if !metadata.is_dir() {
        return Err(WorkspaceError::NotDirectory(absolute.to_path_buf()));
    }

    let mut entries = Vec::new();
    for entry in fs::read_dir(absolute).map_err(|source| WorkspaceError::Io {
        path: absolute.to_path_buf(),
        source,
    })? {
        let entry = entry.map_err(|source| WorkspaceError::Io {
            path: absolute.to_path_buf(),
            source,
        })?;
        let entry_path = entry.path();
        let file_type = entry.file_type().map_err(|source| WorkspaceError::Io {
            path: entry_path.clone(),
            source,
        })?;
        if file_type.is_symlink() {
            continue;
        }
        let kind = if file_type.is_dir() {
            WorkspaceEntryKind::Directory
        } else if file_type.is_file() {
            WorkspaceEntryKind::File
        } else {
            continue;
        };
        let metadata = entry.metadata().map_err(|source| WorkspaceError::Io {
            path: entry_path.clone(),
            source,
        })?;
        entries.push(WorkspaceDirectoryEntry {
            path: entry_path.to_string_lossy().into_owned(),
            name: entry.file_name().to_string_lossy().into_owned(),
            kind,
            size_bytes: metadata.len(),
        });
        if entries.len() > max_entries {
            return Err(WorkspaceError::LimitExceeded("directory entries"));
        }
    }
    entries.sort_by(|left, right| left.name.cmp(&right.name));
    Ok(entries)
}

fn ambient_metadata(absolute: &Path) -> Result<WorkspaceMetadata, WorkspaceError> {
    let metadata = fs::symlink_metadata(absolute).map_err(|source| WorkspaceError::Io {
        path: absolute.to_path_buf(),
        source,
    })?;
    if metadata.file_type().is_symlink() {
        return Err(WorkspaceError::SymlinkDenied(absolute.to_path_buf()));
    }
    let kind = if metadata.is_file() {
        WorkspaceEntryKind::File
    } else if metadata.is_dir() {
        WorkspaceEntryKind::Directory
    } else {
        return Err(WorkspaceError::NotFile(absolute.to_path_buf()));
    };
    Ok(WorkspaceMetadata {
        path: absolute.to_string_lossy().into_owned(),
        kind,
        size_bytes: metadata.len(),
        readonly: metadata.permissions().readonly(),
    })
}

fn sha256_digest(bytes: &[u8]) -> Sha256Digest {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    Sha256Digest(format!("{:x}", hasher.finalize()))
}

fn revision_path_is_ignored(path: &Path) -> bool {
    path.components().any(|component| {
        matches!(component, std::path::Component::Normal(value) if value == ".git" || value == "target")
    })
}

fn truncate_utf8(value: &str, limit: usize) -> String {
    if value.len() <= limit {
        return value.to_owned();
    }
    let mut end = limit;
    while end > 0 && !value.is_char_boundary(end) {
        end -= 1;
    }
    value[..end].to_owned()
}

fn wildcard_matches(pattern: &str, value: &str) -> bool {
    let pattern = pattern.chars().collect::<Vec<_>>();
    let value = value.chars().collect::<Vec<_>>();
    let mut previous = vec![false; value.len().saturating_add(1)];
    previous[0] = true;
    for token in pattern {
        let mut current = vec![false; value.len().saturating_add(1)];
        if token == '*' {
            current[0] = previous[0];
        }
        for index in 1..current.len() {
            current[index] = if token == '*' {
                previous[index] || current[index - 1]
            } else {
                previous[index - 1] && (token == '?' || token == value[index - 1])
            };
        }
        previous = current;
    }
    previous[value.len()]
}

#[cfg(test)]
mod tests {
    use std::fs;

    use argentum_domain::PatchFile;
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

    #[test]
    fn bounded_capture_normalizes_paths_and_retains_exact_bytes() {
        let workspace = tempfile::tempdir().expect("workspace");
        fs::create_dir(workspace.path().join("nested")).expect("nested");
        fs::write(workspace.path().join("nested/note.txt"), b"safe bytes").expect("fixture");
        let broker =
            CapabilityBroker::new(workspace.path(), ApprovalPolicy::default()).expect("broker");
        let manager = WorkspaceManager::new(broker);

        let capture = manager
            .capture_file("nested/./note.txt", 32)
            .expect("bounded capture");

        assert_eq!(capture.relative_path, "nested/note.txt");
        assert!(capture.exists);
        assert_eq!(capture.bytes.as_deref(), Some(b"safe bytes".as_slice()));
        assert!(!capture.too_large);
        assert!(manager.capture_file("../outside.txt", 32).is_err());
    }

    #[test]
    fn bounded_capture_reports_large_files_without_returning_bytes() {
        let workspace = tempfile::tempdir().expect("workspace");
        fs::write(workspace.path().join("large.bin"), [7_u8; 33]).expect("fixture");
        let broker =
            CapabilityBroker::new(workspace.path(), ApprovalPolicy::default()).expect("broker");
        let manager = WorkspaceManager::new(broker);

        let capture = manager.capture_file("large.bin", 32).expect("capture");

        assert_eq!(capture.size_bytes, 33);
        assert!(capture.too_large);
        assert!(capture.bytes.is_none());
    }

    #[test]
    fn approved_write_readback_and_delete_use_the_same_normalized_path() {
        let workspace = tempfile::tempdir().expect("workspace");
        let broker =
            CapabilityBroker::new(workspace.path(), ApprovalPolicy::default()).expect("broker");
        let manager = WorkspaceManager::new(broker);
        let grant = ApprovalGrant::for_capabilities([Capability::WriteFiles]);

        manager
            .write_bytes_with_grant("nested/./note.bin", b"written", &grant)
            .expect("write");
        assert_eq!(
            manager
                .capture_file("nested/note.bin", 32)
                .expect("readback")
                .bytes
                .as_deref(),
            Some(b"written".as_slice())
        );
        manager
            .delete_file_with_grant("nested/note.bin", &grant)
            .expect("delete");
        assert!(
            !manager
                .capture_file("nested/note.bin", 32)
                .expect("missing capture")
                .exists
        );
    }

    #[test]
    fn phase4_inspection_lists_metadata_and_binary_digest_deterministically() {
        let workspace = tempfile::tempdir().expect("workspace");
        fs::create_dir_all(workspace.path().join("src/nested")).expect("directories");
        fs::write(workspace.path().join("src/zeta.txt"), "last").expect("text fixture");
        fs::write(workspace.path().join("src/alpha.bin"), [0_u8, 1, 2, 3]).expect("binary fixture");
        let broker =
            CapabilityBroker::new(workspace.path(), ApprovalPolicy::default()).expect("broker");
        let manager = WorkspaceManager::new(broker);

        let entries = manager
            .list_directory_bounded("src", 8)
            .expect("directory listing");
        assert_eq!(
            entries
                .iter()
                .map(|entry| entry.name.as_str())
                .collect::<Vec<_>>(),
            ["alpha.bin", "nested", "zeta.txt"]
        );
        assert_eq!(entries[1].kind, WorkspaceEntryKind::Directory);

        let metadata = manager
            .file_metadata("src/zeta.txt")
            .expect("file metadata");
        assert_eq!(metadata.kind, WorkspaceEntryKind::File);
        assert_eq!(metadata.size_bytes, 4);

        let binary = manager
            .binary_metadata_bounded("src/alpha.bin", 16)
            .expect("binary metadata");
        assert_eq!(binary.size_bytes, 4);
        assert!(binary.binary);
        assert_eq!(binary.sha256.0.len(), 64);
    }

    #[test]
    fn phase4_inspection_searches_text_and_filenames_with_hard_limits() {
        let workspace = tempfile::tempdir().expect("workspace");
        fs::create_dir_all(workspace.path().join("src/nested")).expect("directories");
        fs::write(
            workspace.path().join("src/lib.rs"),
            "first needle\nsecond needle\n",
        )
        .expect("first fixture");
        fs::write(workspace.path().join("src/nested/mod.rs"), "third needle\n")
            .expect("second fixture");
        fs::write(workspace.path().join("src/binary.bin"), [0_u8, 1, 2]).expect("binary fixture");
        let broker =
            CapabilityBroker::new(workspace.path(), ApprovalPolicy::default()).expect("broker");
        let manager = WorkspaceManager::new(broker);

        let matches = manager
            .search_text_bounded("src", "needle", 2)
            .expect("text search");
        assert_eq!(matches.len(), 2);
        assert!(matches.iter().all(|item| item.path == "src/lib.rs"));
        assert_eq!(matches[0].line_number, 1);

        let names = manager
            .search_filenames_bounded("src", "*.rs", 8)
            .expect("filename search");
        assert_eq!(
            names
                .iter()
                .map(|item| item.path.as_str())
                .collect::<Vec<_>>(),
            ["src/lib.rs", "src/nested/mod.rs"]
        );
        assert!(matches!(
            manager.list_directory_bounded("src", 2),
            Err(WorkspaceError::LimitExceeded("directory entries"))
        ));
    }

    #[test]
    fn phase4_full_access_inspects_absolute_paths_with_the_same_bounds() {
        let fixture = tempfile::tempdir().expect("fixture root");
        let workspace = fixture.path().join("workspace");
        let ambient = fixture.path().join("ambient");
        fs::create_dir_all(&workspace).expect("workspace directory");
        fs::create_dir_all(ambient.join("nested")).expect("ambient directory");
        fs::write(ambient.join("note.txt"), "first needle\nsecond needle\n").expect("text fixture");
        fs::write(ambient.join("nested/data.bin"), [0_u8, 1, 2, 3]).expect("binary fixture");
        let broker = CapabilityBroker::new(&workspace, ApprovalPolicy::default()).expect("broker");
        let manager = WorkspaceManager::new(broker);

        let entries = manager
            .list_directory_bounded_with_scope(&ambient, 8, SandboxScope::Unrestricted)
            .expect("ambient directory listing");
        assert_eq!(
            entries
                .iter()
                .map(|entry| entry.name.as_str())
                .collect::<Vec<_>>(),
            ["nested", "note.txt"]
        );
        assert!(entries
            .iter()
            .all(|entry| Path::new(&entry.path).is_absolute()));

        let metadata = manager
            .file_metadata_with_scope(ambient.join("note.txt"), SandboxScope::Unrestricted)
            .expect("ambient metadata");
        assert_eq!(metadata.kind, WorkspaceEntryKind::File);
        assert_eq!(metadata.size_bytes, 27);

        let text = manager
            .read_text_bounded_with_scope(ambient.join("note.txt"), 64, SandboxScope::Unrestricted)
            .expect("ambient text");
        assert!(text.contains("second needle"));

        let binary = manager
            .binary_metadata_bounded_with_scope(
                ambient.join("nested/data.bin"),
                16,
                SandboxScope::Unrestricted,
            )
            .expect("ambient binary metadata");
        assert!(binary.binary);
        assert!(Path::new(&binary.path).is_absolute());

        let matches = manager
            .search_text_bounded_with_scope(&ambient, "needle", 8, SandboxScope::Unrestricted)
            .expect("ambient text search");
        assert_eq!(matches.len(), 2);
        assert!(matches
            .iter()
            .all(|item| Path::new(&item.path).is_absolute()));

        let names = manager
            .search_filenames_bounded_with_scope(&ambient, "*.bin", 8, SandboxScope::Unrestricted)
            .expect("ambient filename search");
        assert_eq!(names.len(), 1);
        assert!(Path::new(&names[0].path).is_absolute());

        assert!(manager
            .read_text_bounded_with_scope(
                ambient.join("note.txt"),
                64,
                SandboxScope::WorkspaceWrite,
            )
            .is_err());
        assert!(matches!(
            manager.read_text_bounded_with_scope(
                ambient.join("missing.txt"),
                64,
                SandboxScope::Unrestricted,
            ),
            Err(WorkspaceError::Io { source, .. })
                if source.kind() == std::io::ErrorKind::NotFound
        ));
    }

    #[test]
    fn phase4_inspection_revision_digest_ignores_metadata_and_build_output() {
        let workspace = tempfile::tempdir().expect("workspace");
        fs::create_dir_all(workspace.path().join(".git")).expect("git directory");
        fs::create_dir_all(workspace.path().join("target")).expect("target directory");
        fs::create_dir_all(workspace.path().join("src")).expect("source directory");
        fs::write(workspace.path().join(".git/index"), "one").expect("git fixture");
        fs::write(workspace.path().join("target/output"), "one").expect("target fixture");
        fs::write(workspace.path().join("src/lib.rs"), "one").expect("source fixture");
        let broker =
            CapabilityBroker::new(workspace.path(), ApprovalPolicy::default()).expect("broker");
        let manager = WorkspaceManager::new(broker);
        let limits = WorkspaceRevisionLimits {
            max_files: 8,
            max_total_bytes: 1024,
            max_file_bytes: 512,
        };

        let first = manager.revision_digest(limits).expect("first digest");
        fs::write(workspace.path().join(".git/index"), "two").expect("git update");
        fs::write(workspace.path().join("target/output"), "two").expect("target update");
        let ignored = manager.revision_digest(limits).expect("ignored digest");
        assert_eq!(first, ignored);

        fs::write(workspace.path().join("src/lib.rs"), "two").expect("source update");
        let changed = manager.revision_digest(limits).expect("changed digest");
        assert_ne!(first.digest, changed.digest);
        assert_eq!(changed.file_count, 1);
    }

    #[test]
    fn phase4_inspection_revision_digest_fails_closed_on_bounds() {
        let workspace = tempfile::tempdir().expect("workspace");
        fs::write(workspace.path().join("one.txt"), "one").expect("first fixture");
        fs::write(workspace.path().join("two.txt"), "two").expect("second fixture");
        let broker =
            CapabilityBroker::new(workspace.path(), ApprovalPolicy::default()).expect("broker");
        let manager = WorkspaceManager::new(broker);

        assert!(matches!(
            manager.revision_digest(WorkspaceRevisionLimits {
                max_files: 1,
                max_total_bytes: 1024,
                max_file_bytes: 512,
            }),
            Err(WorkspaceError::LimitExceeded("revision files"))
        ));
    }

    #[test]
    fn phase4_mutation_patch_prepares_and_applies_exact_images() {
        let workspace = tempfile::tempdir().expect("workspace");
        fs::write(workspace.path().join("update.txt"), "before").expect("update fixture");
        fs::write(workspace.path().join("delete.txt"), "remove").expect("delete fixture");
        let broker =
            CapabilityBroker::new(workspace.path(), ApprovalPolicy::default()).expect("broker");
        let manager = WorkspaceManager::new(broker);
        let grant = ApprovalGrant::for_capabilities([Capability::WriteFiles]);
        let expected = manager
            .capture_file("update.txt", 1024)
            .expect("capture")
            .sha256
            .expect("digest");

        let patch = manager
            .prepare_patch(&[
                PatchFile {
                    path: "create.txt".into(),
                    expected_sha256: None,
                    content: Some("created".into()),
                },
                PatchFile {
                    path: "update.txt".into(),
                    expected_sha256: Some(Sha256Digest(expected)),
                    content: Some("after".into()),
                },
                PatchFile {
                    path: "delete.txt".into(),
                    expected_sha256: None,
                    content: None,
                },
            ])
            .expect("prepared patch");
        assert_eq!(patch.mutations.len(), 3);
        manager
            .apply_prepared_patch_with_grant(&patch, &grant)
            .expect("applied patch");

        assert_eq!(
            fs::read_to_string(workspace.path().join("create.txt")).expect("created read"),
            "created"
        );
        assert_eq!(
            fs::read_to_string(workspace.path().join("update.txt")).expect("updated read"),
            "after"
        );
        assert!(!workspace.path().join("delete.txt").exists());
    }

    #[test]
    fn phase4_mutation_patch_rejects_stale_and_duplicate_paths_before_writing() {
        let workspace = tempfile::tempdir().expect("workspace");
        fs::write(workspace.path().join("one.txt"), "before").expect("fixture");
        let broker =
            CapabilityBroker::new(workspace.path(), ApprovalPolicy::default()).expect("broker");
        let manager = WorkspaceManager::new(broker);
        let grant = ApprovalGrant::for_capabilities([Capability::WriteFiles]);
        let files = [PatchFile {
            path: "one.txt".into(),
            expected_sha256: None,
            content: Some("after".into()),
        }];
        let patch = manager.prepare_patch(&files).expect("prepared patch");
        fs::write(workspace.path().join("one.txt"), "concurrent").expect("concurrent write");

        assert!(matches!(
            manager.apply_prepared_patch_with_grant(&patch, &grant),
            Err(WorkspaceError::StaleMutation(_))
        ));
        assert_eq!(
            fs::read_to_string(workspace.path().join("one.txt")).expect("unchanged read"),
            "concurrent"
        );

        assert!(matches!(
            manager.prepare_patch(&[
                PatchFile {
                    path: "same.txt".into(),
                    expected_sha256: None,
                    content: Some("one".into()),
                },
                PatchFile {
                    path: "same.txt".into(),
                    expected_sha256: None,
                    content: Some("two".into()),
                },
            ]),
            Err(WorkspaceError::DuplicateMutationPath(_))
        ));
    }

    #[test]
    fn phase4_mutation_move_copy_delete_and_directory_use_prepared_changes() {
        let workspace = tempfile::tempdir().expect("workspace");
        fs::write(workspace.path().join("source.txt"), "source").expect("source fixture");
        let broker =
            CapabilityBroker::new(workspace.path(), ApprovalPolicy::default()).expect("broker");
        let manager = WorkspaceManager::new(broker);
        let grant = ApprovalGrant::for_capabilities([Capability::WriteFiles]);

        manager
            .create_directory_with_grant("nested", &grant)
            .expect("create directory");
        let copy = manager
            .prepare_copy_file("source.txt", "nested/copy.txt")
            .expect("copy plan");
        manager
            .apply_prepared_patch_with_grant(&copy, &grant)
            .expect("copy apply");
        let movement = manager
            .prepare_move_file("nested/copy.txt", "moved.txt")
            .expect("move plan");
        manager
            .apply_prepared_patch_with_grant(&movement, &grant)
            .expect("move apply");
        let deletion = manager
            .prepare_delete_file("moved.txt")
            .expect("delete plan");
        manager
            .apply_prepared_patch_with_grant(&deletion, &grant)
            .expect("delete apply");

        assert!(workspace.path().join("nested").is_dir());
        assert!(!workspace.path().join("nested/copy.txt").exists());
        assert!(!workspace.path().join("moved.txt").exists());
        assert_eq!(
            fs::read_to_string(workspace.path().join("source.txt")).expect("source read"),
            "source"
        );
    }

    #[test]
    fn phase4_mutation_binary_copy_and_move_preserve_exact_bytes() {
        let workspace = tempfile::tempdir().expect("workspace");
        let bytes = [0_u8, 1, 2, 127, 128, 254, 255];
        fs::write(workspace.path().join("source.bin"), bytes).expect("source fixture");
        let broker =
            CapabilityBroker::new(workspace.path(), ApprovalPolicy::default()).expect("broker");
        let manager = WorkspaceManager::new(broker);
        let grant = ApprovalGrant::for_capabilities([Capability::WriteFiles]);

        let copy = manager
            .prepare_copy_file("source.bin", "copy.bin")
            .expect("binary copy plan");
        manager
            .apply_prepared_patch_with_grant(&copy, &grant)
            .expect("binary copy");
        let movement = manager
            .prepare_move_file("copy.bin", "moved.bin")
            .expect("binary move plan");
        manager
            .apply_prepared_patch_with_grant(&movement, &grant)
            .expect("binary move");

        assert_eq!(
            fs::read(workspace.path().join("source.bin")).expect("source"),
            bytes
        );
        assert_eq!(
            fs::read(workspace.path().join("moved.bin")).expect("moved"),
            bytes
        );
        assert!(!workspace.path().join("copy.bin").exists());
    }

    #[test]
    fn phase4_mutation_patch_rolls_back_applied_images_in_reverse_order() {
        let workspace = tempfile::tempdir().expect("workspace");
        fs::write(workspace.path().join("one.txt"), "one-before").expect("first fixture");
        fs::write(workspace.path().join("two.txt"), "two-before").expect("second fixture");
        let broker =
            CapabilityBroker::new(workspace.path(), ApprovalPolicy::default()).expect("broker");
        let manager = WorkspaceManager::new(broker);
        let grant = ApprovalGrant::for_capabilities([Capability::WriteFiles]);
        let patch = manager
            .prepare_patch(&[
                PatchFile {
                    path: "one.txt".into(),
                    expected_sha256: None,
                    content: Some("one-after".into()),
                },
                PatchFile {
                    path: "two.txt".into(),
                    expected_sha256: None,
                    content: Some("two-after".into()),
                },
            ])
            .expect("patch");

        assert!(manager
            .apply_prepared_patch_with_test_failure(&patch, &grant, 1)
            .is_err());
        assert_eq!(
            fs::read_to_string(workspace.path().join("one.txt")).expect("first rollback"),
            "one-before"
        );
        assert_eq!(
            fs::read_to_string(workspace.path().join("two.txt")).expect("second unchanged"),
            "two-before"
        );
    }

    #[test]
    fn phase4_cancellation_between_file_effects_rolls_back_the_patch() {
        use std::cell::Cell;

        let workspace = tempfile::tempdir().expect("workspace");
        fs::write(workspace.path().join("one.txt"), "one-before").expect("first fixture");
        fs::write(workspace.path().join("two.txt"), "two-before").expect("second fixture");
        let broker =
            CapabilityBroker::new(workspace.path(), ApprovalPolicy::default()).expect("broker");
        let manager = WorkspaceManager::new(broker);
        let grant = ApprovalGrant::for_capabilities([Capability::WriteFiles]);
        let patch = manager
            .prepare_patch(&[
                PatchFile {
                    path: "one.txt".into(),
                    expected_sha256: None,
                    content: Some("one-after".into()),
                },
                PatchFile {
                    path: "two.txt".into(),
                    expected_sha256: None,
                    content: Some("two-after".into()),
                },
            ])
            .expect("patch");
        let checks = Cell::new(0_u8);

        let result = manager.apply_prepared_patch_with_scope_and_grant_cancellable(
            &patch,
            &grant,
            SandboxScope::WorkspaceWrite,
            || {
                let next = checks.get().saturating_add(1);
                checks.set(next);
                next >= 3
            },
        );

        assert!(matches!(result, Err(WorkspaceError::Cancelled)));
        assert_eq!(
            fs::read_to_string(workspace.path().join("one.txt")).expect("first rollback"),
            "one-before"
        );
        assert_eq!(
            fs::read_to_string(workspace.path().join("two.txt")).expect("second unchanged"),
            "two-before"
        );
    }

    #[test]
    fn phase4_directory_creation_rolls_back_when_cancelled_after_the_effect() {
        use std::cell::Cell;

        let workspace = tempfile::tempdir().expect("workspace");
        let broker =
            CapabilityBroker::new(workspace.path(), ApprovalPolicy::default()).expect("broker");
        let manager = WorkspaceManager::new(broker);
        let grant = ApprovalGrant::for_capabilities([Capability::WriteFiles]);
        let checks = Cell::new(0_u8);

        let result = manager.create_directory_with_scope_and_grant_cancellable(
            "created/nested",
            &grant,
            SandboxScope::WorkspaceWrite,
            || {
                let next = checks.get().saturating_add(1);
                checks.set(next);
                next >= 2
            },
        );

        assert!(matches!(result, Err(WorkspaceError::Cancelled)));
        assert!(!workspace.path().join("created").exists());
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
