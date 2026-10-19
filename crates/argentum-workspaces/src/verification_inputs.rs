//! One contained, bounded identity path for reviewed project input roles.
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};

use argentum_domain::goal_supervision::{
    VerificationInputLimits, VerificationInputScope, VerificationInputSnapshot,
    VerificationProjectInputs, VerificationTreeIdentity,
};
use argentum_domain::Sha256Digest;
use sha2::{Digest, Sha256};

use crate::{normalize_display_path, revision_path_is_ignored, WorkspaceError, WorkspaceManager};

pub use argentum_domain::verification_inputs::PROJECT_VERIFICATION_METHOD;
use argentum_domain::verification_inputs::{
    SOURCE_MAX_FILES, SOURCE_MAX_FILE_BYTES, SOURCE_MAX_TOTAL_BYTES,
};

pub const fn default_verification_source_limits() -> crate::WorkspaceRevisionLimits {
    crate::WorkspaceRevisionLimits {
        max_files: SOURCE_MAX_FILES as usize,
        max_total_bytes: SOURCE_MAX_TOTAL_BYTES,
        max_file_bytes: SOURCE_MAX_FILE_BYTES,
    }
}

/// Bounded content identity for a file already opened by its authorized owner.
/// The path is diagnostic only; this function does not resolve or open it.
pub fn verification_file_identity(
    file: &mut fs::File,
    path: &Path,
    expected_bytes: u64,
    max_file_bytes: u64,
    cancelled: &mut impl FnMut() -> bool,
) -> Result<Sha256Digest, WorkspaceError> {
    check_cancelled(cancelled)?;
    let before = file.metadata().map_err(|source| WorkspaceError::Io {
        path: path.to_path_buf(),
        source,
    })?;
    if !before.is_file() {
        return Err(WorkspaceError::NotFile(path.to_path_buf()));
    }
    if before.len() != expected_bytes || expected_bytes > max_file_bytes {
        return Err(WorkspaceError::LimitExceeded(
            "verification file identity bytes",
        ));
    }
    let mut digest = Sha256::new();
    let mut observed = 0_u64;
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        check_cancelled(cancelled)?;
        let remaining = expected_bytes.saturating_sub(observed).saturating_add(1);
        let capacity = usize::try_from(remaining)
            .unwrap_or(usize::MAX)
            .min(buffer.len());
        let count = file
            .read(&mut buffer[..capacity])
            .map_err(|source| WorkspaceError::Io {
                path: path.to_path_buf(),
                source,
            })?;
        if count == 0 {
            break;
        }
        observed = observed.saturating_add(count as u64);
        if observed > expected_bytes {
            return Err(WorkspaceError::LimitExceeded(
                "verification input changed during capture",
            ));
        }
        digest.update(&buffer[..count]);
    }
    let after = file.metadata().map_err(|source| WorkspaceError::Io {
        path: path.to_path_buf(),
        source,
    })?;
    if observed != expected_bytes
        || before.len() != after.len()
        || before.modified().ok() != after.modified().ok()
    {
        return Err(WorkspaceError::InvalidText(
            "verification input changed during capture",
        ));
    }
    Ok(Sha256Digest(format!("{:x}", digest.finalize())))
}

pub fn validate_project_verification_inputs(
    inputs: &VerificationProjectInputs,
) -> Result<(), WorkspaceError> {
    argentum_domain::verification_inputs::validate_project_verification_inputs(inputs)
        .map_err(|error| WorkspaceError::InvalidText(error.0))
}

fn checked_root(value: &str) -> Result<PathBuf, WorkspaceError> {
    argentum_domain::verification_inputs::checked_project_input_root(value)
        .map_err(|error| WorkspaceError::InvalidText(error.0))
}

fn check_cancelled(cancelled: &mut impl FnMut() -> bool) -> Result<(), WorkspaceError> {
    if cancelled() {
        Err(WorkspaceError::VerificationCancelled)
    } else {
        Ok(())
    }
}

#[derive(Default)]
struct ObservedBudget {
    files: u64,
    bytes: u64,
}

impl WorkspaceManager {
    /// Capture all remaining source plus the actual declared installed bytes.
    /// No package-manager lockfile substitutes for reading current dependencies.
    pub fn capture_verification_inputs(
        &self,
        scope: &VerificationInputScope,
        cancelled: impl FnMut() -> bool,
    ) -> Result<VerificationInputSnapshot, WorkspaceError> {
        self.capture_verification_inputs_with_reserved(scope, 0, 0, cancelled)
    }

    /// Execution reserves its explicitly bound verifier program in the same
    /// installed-input budget before any dependency tree is read.
    pub fn capture_verification_inputs_with_reserved(
        &self,
        scope: &VerificationInputScope,
        reserved_files: u64,
        reserved_bytes: u64,
        mut cancelled: impl FnMut() -> bool,
    ) -> Result<VerificationInputSnapshot, WorkspaceError> {
        let Some(inputs) = &scope.project_inputs else {
            if scope.method != "workspace_revision_v1" {
                return Err(WorkspaceError::InvalidText(
                    "unsupported verification input method",
                ));
            }
            let revision = self.revision_digest(crate::WorkspaceRevisionLimits {
                max_files: scope
                    .max_files
                    .try_into()
                    .map_err(|_| WorkspaceError::LimitExceeded("revision files"))?,
                max_total_bytes: scope.max_total_bytes,
                max_file_bytes: scope.max_file_bytes,
            })?;
            return Ok(VerificationInputSnapshot {
                method: scope.method.clone(),
                digest: revision.digest,
                file_count: revision.file_count,
                total_bytes: revision.total_bytes,
                dependencies: Vec::new(),
            });
        };
        check_cancelled(&mut cancelled)?;
        self.validate_role_scope(scope, inputs)?;
        let dependencies = self.resolve_role_roots(&inputs.dependency_roots, false)?;
        let outputs = self.resolve_role_roots(&inputs.generated_output_roots, true)?;
        let roles = dependencies
            .iter()
            .chain(&outputs)
            .map(|(_, path, _)| path.clone())
            .collect::<Vec<_>>();
        let files = self.verification_inventory(inputs, &roles, &mut cancelled)?;
        let source_files = files
            .iter()
            .filter(|path| !roles.iter().any(|root| path.starts_with(root)))
            .cloned()
            .collect::<Vec<_>>();
        for fixed in &inputs.fixed_files {
            if !source_files.contains(&PathBuf::from(&fixed.path)) {
                return Err(WorkspaceError::InvalidText(
                    "fixed verifier input unavailable",
                ));
            }
        }
        let source_limits = VerificationInputLimits {
            max_files: scope.max_files,
            max_total_bytes: scope.max_total_bytes,
            max_file_bytes: scope.max_file_bytes,
        };
        let mut source_budget = ObservedBudget::default();
        let source = self.hash_verification_files(
            "",
            &source_files,
            true,
            &source_limits,
            &mut source_budget,
            &mut cancelled,
            &inputs.fixed_files,
        )?;
        if reserved_files > inputs.dependency_limits.max_files
            || reserved_bytes > inputs.dependency_limits.max_total_bytes
        {
            return Err(WorkspaceError::LimitExceeded(
                "verification environment budget",
            ));
        }
        let mut dependency_budget = ObservedBudget {
            files: reserved_files,
            bytes: reserved_bytes,
        };
        let mut identities = Vec::new();
        for (name, root, exists) in dependencies {
            let selected = files
                .iter()
                .filter(|path| path.starts_with(&root))
                .cloned()
                .collect::<Vec<_>>();
            identities.push(self.hash_verification_files(
                &name,
                &selected,
                exists,
                &inputs.dependency_limits,
                &mut dependency_budget,
                &mut cancelled,
                &[],
            )?);
        }
        let mut digest = Sha256::new();
        digest.update(b"argentum-project-verification-inputs-v2\0");
        digest.update(source.digest.0.as_bytes());
        for identity in &identities {
            digest.update((identity.path.len() as u64).to_le_bytes());
            digest.update(identity.path.as_bytes());
            digest.update(identity.digest.0.as_bytes());
            digest.update(identity.file_count.to_le_bytes());
            digest.update(identity.total_bytes.to_le_bytes());
        }
        Ok(VerificationInputSnapshot {
            method: scope.method.clone(),
            digest: Sha256Digest(format!("{:x}", digest.finalize())),
            file_count: source.file_count,
            total_bytes: source.total_bytes,
            dependencies: identities,
        })
    }

    /// Before observations retain existing outputs as existing inputs. After
    /// observations bind the actual tested build for later freshness checks.
    pub fn capture_verification_outputs(
        &self,
        scope: &VerificationInputScope,
        required: bool,
        mut cancelled: impl FnMut() -> bool,
    ) -> Result<Option<Vec<VerificationTreeIdentity>>, WorkspaceError> {
        let Some(inputs) = &scope.project_inputs else {
            return Ok(None);
        };
        check_cancelled(&mut cancelled)?;
        self.validate_role_scope(scope, inputs)?;
        let outputs = self.resolve_role_roots(&inputs.generated_output_roots, !required)?;
        let dependencies = self.resolve_role_roots(&inputs.dependency_roots, false)?;
        let roles = outputs
            .iter()
            .chain(&dependencies)
            .map(|(_, path, _)| path.clone())
            .collect::<Vec<_>>();
        let files = self.verification_inventory(inputs, &roles, &mut cancelled)?;
        let mut budget = ObservedBudget::default();
        let mut identities = Vec::new();
        for (name, root, exists) in outputs {
            let selected = files
                .iter()
                .filter(|path| path.starts_with(&root))
                .cloned()
                .collect::<Vec<_>>();
            identities.push(self.hash_verification_files(
                &name,
                &selected,
                exists,
                &inputs.generated_limits,
                &mut budget,
                &mut cancelled,
                &[],
            )?);
        }
        Ok(Some(identities))
    }

    fn validate_role_scope(
        &self,
        scope: &VerificationInputScope,
        inputs: &VerificationProjectInputs,
    ) -> Result<(), WorkspaceError> {
        let limits = default_verification_source_limits();
        if scope.method != PROJECT_VERIFICATION_METHOD
            || scope.excluded_components != [".git", "target"]
            || scope.max_files != limits.max_files as u64
            || scope.max_total_bytes != limits.max_total_bytes
            || scope.max_file_bytes != limits.max_file_bytes
        {
            return Err(WorkspaceError::InvalidText(
                "unsupported project verification scope",
            ));
        }
        validate_project_verification_inputs(inputs)
    }

    fn resolve_role_roots(
        &self,
        names: &[String],
        allow_missing: bool,
    ) -> Result<Vec<(String, PathBuf, bool)>, WorkspaceError> {
        let canonical_workspace =
            fs::canonicalize(self.root()).map_err(|source| WorkspaceError::Io {
                path: self.root().to_path_buf(),
                source,
            })?;
        names
            .iter()
            .map(|name| {
                let root = checked_root(name)?;
                let relative = self.broker.relative_path(&root)?;
                let exists = match self.broker.context_path_is_directory(&relative)? {
                    Some(true) => true,
                    Some(false) => {
                        return Err(WorkspaceError::NotDirectory(self.root().join(root)))
                    }
                    None if allow_missing => false,
                    None => {
                        return Err(WorkspaceError::Io {
                            path: self.root().join(root),
                            source: std::io::Error::new(
                                std::io::ErrorKind::NotFound,
                                "declared verification input root unavailable",
                            ),
                        })
                    }
                };
                if exists {
                    let canonical =
                        fs::canonicalize(self.root().join(&relative)).map_err(|source| {
                            WorkspaceError::Io {
                                path: self.root().join(&relative),
                                source,
                            }
                        })?;
                    let actual = canonical.strip_prefix(&canonical_workspace).map_err(|_| {
                        WorkspaceError::InvalidText("verification root escaped workspace")
                    })?;
                    if normalize_display_path(actual) != *name {
                        return Err(WorkspaceError::InvalidText(
                            "verification root must match canonical spelling",
                        ));
                    }
                }
                Ok((name.clone(), relative, exists))
            })
            .collect()
    }

    fn verification_inventory(
        &self,
        inputs: &VerificationProjectInputs,
        roles: &[PathBuf],
        cancelled: &mut impl FnMut() -> bool,
    ) -> Result<Vec<PathBuf>, WorkspaceError> {
        let files = self
            .broker
            .list_workspace_files_bounded(
                inputs.inventory_limits.max_entries as usize,
                inputs.inventory_limits.max_depth as usize,
                cancelled,
                |path| {
                    !roles.iter().any(|root| path.starts_with(root))
                        && revision_path_is_ignored(path)
                },
            )
            .map_err(|error| match error {
                argentum_security::SecurityError::WorkspaceOperation { source, .. }
                    if source.kind() == std::io::ErrorKind::Interrupted =>
                {
                    WorkspaceError::VerificationCancelled
                }
                other => WorkspaceError::from(other),
            })?;
        files
            .into_iter()
            .map(|absolute| {
                let relative = absolute
                    .strip_prefix(self.root())
                    .map_err(|_| {
                        WorkspaceError::InvalidText("verification inventory escaped workspace")
                    })?
                    .to_path_buf();
                if relative.to_str().is_none() {
                    return Err(WorkspaceError::InvalidText(
                        "verification input path is not Unicode",
                    ));
                }
                Ok(relative)
            })
            .collect()
    }

    // Keep the role's reviewed limits and the shared aggregate budget explicit.
    #[allow(clippy::too_many_arguments)]
    fn hash_verification_files(
        &self,
        root: &str,
        files: &[PathBuf],
        exists: bool,
        limits: &VerificationInputLimits,
        budget: &mut ObservedBudget,
        cancelled: &mut impl FnMut() -> bool,
        fixed_files: &[argentum_domain::goal_supervision::VerificationFixedFile],
    ) -> Result<VerificationTreeIdentity, WorkspaceError> {
        let mut paths = files.to_vec();
        paths.sort();
        let mut digest = Sha256::new();
        let mut bytes = 0_u64;
        for relative in &paths {
            check_cancelled(cancelled)?;
            budget.files = budget.files.saturating_add(1);
            if budget.files > limits.max_files {
                return Err(WorkspaceError::LimitExceeded("verification identity files"));
            }
            let (display_path, file) = self.broker.open_file_for_read(relative)?;
            // Keep the same contained OS handle; never reopen by its display path.
            let mut file = file.into_std();
            let before = file.metadata().map_err(|source| WorkspaceError::Io {
                path: display_path.clone(),
                source,
            })?;
            if !before.is_file() {
                return Err(WorkspaceError::NotFile(display_path));
            }
            if before.len() > limits.max_file_bytes {
                return Err(WorkspaceError::LimitExceeded(
                    "verification identity file bytes",
                ));
            }
            if budget.bytes.saturating_add(before.len()) > limits.max_total_bytes {
                return Err(WorkspaceError::LimitExceeded(
                    "verification identity total bytes",
                ));
            }
            let name = normalize_display_path(relative);
            digest.update((name.len() as u64).to_le_bytes());
            digest.update(name.as_bytes());
            digest.update(before.len().to_le_bytes());
            let content = verification_file_identity(
                &mut file,
                &display_path,
                before.len(),
                limits.max_file_bytes,
                cancelled,
            )?;
            if let Some(expected) = fixed_files.iter().find(|item| item.path == name) {
                if content != expected.sha256 || before.len() != expected.bytes {
                    return Err(WorkspaceError::InvalidText(
                        "fixed verifier input identity changed",
                    ));
                }
            }
            digest.update(content.0.as_bytes());
            budget.bytes = budget.bytes.saturating_add(before.len());
            bytes = bytes.saturating_add(before.len());
        }
        Ok(VerificationTreeIdentity {
            path: root.into(),
            exists,
            digest: Sha256Digest(format!("{:x}", digest.finalize())),
            file_count: paths.len() as u64,
            total_bytes: bytes,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use argentum_domain::goal_supervision::VerificationInventoryLimits;
    use argentum_security::{ApprovalPolicy, CapabilityBroker};
    use std::cell::Cell;
    use std::io::{Seek, Write};

    fn fixture() -> (tempfile::TempDir, WorkspaceManager, VerificationInputScope) {
        let temp = tempfile::tempdir().unwrap();
        fs::create_dir_all(temp.path().join("src")).unwrap();
        fs::create_dir_all(temp.path().join("node_modules")).unwrap();
        fs::write(temp.path().join("src/main.js"), "export const value = 1;\n").unwrap();
        fs::write(temp.path().join("package-lock.json"), "{\"version\":1}\n").unwrap();
        fs::File::create(temp.path().join("node_modules/native.dll"))
            .unwrap()
            .set_len(18 * 1024 * 1024)
            .unwrap();
        let manager = WorkspaceManager::new(
            CapabilityBroker::new(temp.path(), ApprovalPolicy::default()).unwrap(),
        );
        let inputs = VerificationProjectInputs {
            program: argentum_domain::goal_supervision::VerificationProgramIdentity {
                sha256: Sha256Digest("0".repeat(64)),
                bytes: 1,
            },
            fixed_files: Vec::new(),
            dependency_roots: vec!["node_modules".into()],
            generated_output_roots: vec!["build".into(), "dist".into()],
            dependency_limits: VerificationInputLimits {
                max_files: 16,
                max_total_bytes: 32 * 1024 * 1024,
                max_file_bytes: 24 * 1024 * 1024,
            },
            generated_limits: VerificationInputLimits {
                max_files: 16,
                max_total_bytes: 1024 * 1024,
                max_file_bytes: 1024 * 1024,
            },
            inventory_limits: VerificationInventoryLimits {
                max_entries: 128,
                max_depth: 16,
            },
        };
        let scope = VerificationInputScope {
            method: PROJECT_VERIFICATION_METHOD.into(),
            excluded_components: vec![".git".into(), "target".into()],
            max_files: 16_384,
            max_total_bytes: 64 * 1024 * 1024,
            max_file_bytes: 16 * 1024 * 1024,
            project_inputs: Some(inputs),
            program_path: None,
        };
        (temp, manager, scope)
    }

    #[test]
    fn project_inputs_large_installed_file_is_bound_without_changing_source_limits() {
        let (_temp, manager, scope) = fixture();
        assert!(manager
            .revision_digest(crate::WorkspaceRevisionLimits {
                max_files: 16_384,
                max_total_bytes: 64 * 1024 * 1024,
                max_file_bytes: 16 * 1024 * 1024
            })
            .is_err());
        let snapshot = manager
            .capture_verification_inputs(&scope, || false)
            .unwrap();
        assert_eq!(snapshot.file_count, 2);
        assert_eq!(snapshot.dependencies[0].file_count, 1);
        assert_eq!(snapshot.dependencies[0].total_bytes, 18 * 1024 * 1024);
        assert_eq!(scope.max_file_bytes, 16 * 1024 * 1024);
    }

    #[test]
    fn project_inputs_dependency_change_is_visible_with_unchanged_lockfile() {
        let (temp, manager, scope) = fixture();
        let before = manager
            .capture_verification_inputs(&scope, || false)
            .unwrap();
        let lock = fs::read(temp.path().join("package-lock.json")).unwrap();
        let mut file = fs::OpenOptions::new()
            .write(true)
            .open(temp.path().join("node_modules/native.dll"))
            .unwrap();
        file.seek(std::io::SeekFrom::Start(17)).unwrap();
        file.write_all(b"x").unwrap();
        drop(file);
        let after = manager
            .capture_verification_inputs(&scope, || false)
            .unwrap();
        assert_ne!(before.digest, after.digest);
        assert_ne!(before.dependencies, after.dependencies);
        assert_eq!(
            lock,
            fs::read(temp.path().join("package-lock.json")).unwrap()
        );
    }

    #[test]
    fn project_inputs_outputs_are_separate_but_new_source_is_never_ignored() {
        let (temp, manager, scope) = fixture();
        let source = manager
            .capture_verification_inputs(&scope, || false)
            .unwrap();
        let before = manager
            .capture_verification_outputs(&scope, false, || false)
            .unwrap()
            .unwrap();
        assert!(before.iter().all(|item| !item.exists));
        assert!(manager
            .capture_verification_outputs(&scope, true, || false)
            .is_err());
        for path in ["build", "dist"] {
            fs::create_dir_all(temp.path().join(path)).unwrap();
            fs::write(temp.path().join(path).join("app.js"), "built").unwrap();
        }
        let after = manager
            .capture_verification_outputs(&scope, true, || false)
            .unwrap()
            .unwrap();
        assert_ne!(before, after);
        assert_eq!(
            source,
            manager
                .capture_verification_inputs(&scope, || false)
                .unwrap()
        );
        fs::write(temp.path().join("dist/app.js"), "substituted").unwrap();
        assert_ne!(
            after,
            manager
                .capture_verification_outputs(&scope, true, || false)
                .unwrap()
                .unwrap()
        );
        fs::create_dir_all(temp.path().join("tests")).unwrap();
        fs::write(temp.path().join("tests/new.test.js"), "new check").unwrap();
        assert_ne!(
            source,
            manager
                .capture_verification_inputs(&scope, || false)
                .unwrap()
        );
    }

    #[test]
    fn project_inputs_missing_dependencies_bounds_and_mid_stream_cancellation_fail_closed() {
        let (temp, manager, mut scope) = fixture();
        let calls = Cell::new(0);
        let cancelled = manager.capture_verification_inputs(&scope, || {
            calls.set(calls.get() + 1);
            calls.get() > 25
        });
        assert!(matches!(
            cancelled,
            Err(WorkspaceError::VerificationCancelled)
        ));
        assert_eq!(calls.get(), 26);
        scope
            .project_inputs
            .as_mut()
            .unwrap()
            .dependency_limits
            .max_file_bytes = 17 * 1024 * 1024;
        assert!(manager
            .capture_verification_inputs(&scope, || false)
            .is_err());
        fs::remove_file(temp.path().join("node_modules/native.dll")).unwrap();
        fs::remove_dir(temp.path().join("node_modules")).unwrap();
        assert!(manager
            .capture_verification_inputs(&scope, || false)
            .is_err());
    }

    #[test]
    fn project_inputs_invalid_and_overlapping_roles_are_rejected() {
        let (_temp, manager, scope) = fixture();
        for root in [
            ".",
            "..",
            "../outside",
            "/absolute",
            ".agents",
            "target/cache",
            "node_modules/../src",
            "node_modules/*",
        ] {
            let mut contract = scope.project_inputs.clone().unwrap();
            contract.dependency_roots = vec![root.into()];
            assert!(
                validate_project_verification_inputs(&contract).is_err(),
                "{root}"
            );
        }
        let mut enlarged_source = scope.clone();
        enlarged_source.max_file_bytes *= 2;
        assert!(manager
            .capture_verification_inputs(&enlarged_source, || false)
            .is_err());
        let mut contract = scope.project_inputs.unwrap();
        contract.generated_output_roots = vec!["node_modules/nested".into()];
        assert!(validate_project_verification_inputs(&contract).is_err());
    }

    #[cfg(windows)]
    #[test]
    fn project_inputs_dependency_junction_is_unavailable_not_omitted() {
        let (temp, manager, scope) = fixture();
        fs::remove_file(temp.path().join("node_modules/native.dll")).unwrap();
        fs::remove_dir(temp.path().join("node_modules")).unwrap();
        let outside = tempfile::tempdir().unwrap();
        fs::write(outside.path().join("module.js"), "synthetic outside input").unwrap();
        let status = std::process::Command::new("cmd.exe")
            .args(["/d", "/c", "mklink", "/J"])
            .arg(temp.path().join("node_modules"))
            .arg(outside.path())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::piped())
            .output()
            .unwrap();
        assert!(
            status.status.success(),
            "junction control setup failed: {}",
            String::from_utf8_lossy(&status.stderr)
        );
        assert!(manager
            .capture_verification_inputs(&scope, || false)
            .is_err());
        assert_eq!(
            fs::read(outside.path().join("module.js")).unwrap(),
            b"synthetic outside input"
        );
    }
}
