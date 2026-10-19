//! Canonical structural contract shared by persistence and execution.
use crate::goal_supervision::{
    VerificationInputLimits, VerificationInputScope, VerificationInputSnapshot,
    VerificationProjectInputs,
};
use crate::Sha256Digest;
use std::path::{Path, PathBuf};

pub const PROJECT_VERIFICATION_METHOD: &str = "workspace_revision_project_inputs_v2";
pub const SOURCE_MAX_FILES: u64 = 16_384;
pub const SOURCE_MAX_TOTAL_BYTES: u64 = 64 * 1024 * 1024;
pub const SOURCE_MAX_FILE_BYTES: u64 = 16 * 1024 * 1024;
const MAX_ROLE_ROOTS: usize = 8;
const MAX_IDENTITY_FILES: u64 = 65_536;
const MAX_IDENTITY_BYTES: u64 = 1024 * 1024 * 1024;
const MAX_IDENTITY_FILE_BYTES: u64 = 256 * 1024 * 1024;
const MAX_INVENTORY_ENTRIES: u64 = 131_072;
const MAX_INVENTORY_DEPTH: u32 = 128;

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
#[error("{0}")]
pub struct VerificationInputContractError(pub &'static str);

fn portable_path(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}
fn valid_digest(value: &Sha256Digest) -> bool {
    value.0.len() == 64
        && value
            .0
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

pub fn validate_project_verification_inputs(
    inputs: &VerificationProjectInputs,
) -> Result<(), VerificationInputContractError> {
    if inputs.dependency_roots.len() > MAX_ROLE_ROOTS
        || inputs.generated_output_roots.len() > MAX_ROLE_ROOTS
        || (inputs.dependency_roots.is_empty() && inputs.generated_output_roots.is_empty())
    {
        return Err(VerificationInputContractError("verification role roots"));
    }
    validate_limits(&inputs.dependency_limits)?;
    validate_limits(&inputs.generated_limits)?;
    let valid_digest = |value: &Sha256Digest| {
        value.0.len() == 64
            && value
                .0
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    };
    if !valid_digest(&inputs.program.sha256)
        || inputs.program.bytes == 0
        || inputs.program.bytes > inputs.dependency_limits.max_file_bytes
        || inputs.program.bytes > inputs.dependency_limits.max_total_bytes
        || inputs.fixed_files.len() > 16
    {
        return Err(VerificationInputContractError(
            "verification environment identity",
        ));
    }
    let mut fixed_paths = std::collections::BTreeSet::new();
    for fixed in &inputs.fixed_files {
        if fixed.path.is_empty()
            || fixed.path.len() > 256
            || fixed
                .path
                .chars()
                .any(|c| c.is_control() || matches!(c, '\\' | ':' | '*' | '?' | '[' | ']'))
            || Path::new(&fixed.path).is_absolute()
            || fixed
                .path
                .split('/')
                .any(|part| matches!(part, "" | "." | ".." | ".git" | "target"))
            || !valid_digest(&fixed.sha256)
            || fixed.bytes > SOURCE_MAX_FILE_BYTES
            || !fixed_paths.insert(fixed.path.to_ascii_lowercase())
        {
            return Err(VerificationInputContractError(
                "fixed verifier input declaration",
            ));
        }
        for root in inputs
            .dependency_roots
            .iter()
            .chain(&inputs.generated_output_roots)
        {
            if Path::new(&fixed.path).starts_with(root) {
                return Err(VerificationInputContractError(
                    "fixed verifier files must remain source inputs",
                ));
            }
        }
    }
    if inputs.dependency_limits.max_total_bytes
        + inputs.generated_limits.max_total_bytes
        + SOURCE_MAX_TOTAL_BYTES
        > 2 * 1024 * 1024 * 1024
    {
        return Err(VerificationInputContractError(
            "aggregate verification identity bounds",
        ));
    }
    if inputs.inventory_limits.max_entries == 0
        || inputs.inventory_limits.max_entries > MAX_INVENTORY_ENTRIES
        || inputs.inventory_limits.max_depth == 0
        || inputs.inventory_limits.max_depth > MAX_INVENTORY_DEPTH
    {
        return Err(VerificationInputContractError(
            "verification inventory bounds",
        ));
    }
    let roots = inputs
        .dependency_roots
        .iter()
        .chain(&inputs.generated_output_roots)
        .map(|value| checked_project_input_root(value))
        .collect::<Result<Vec<_>, _>>()?;
    for (index, root) in roots.iter().enumerate() {
        for other in &roots[..index] {
            let root_key = portable_path(root).to_ascii_lowercase();
            let other_key = portable_path(other).to_ascii_lowercase();
            if root.starts_with(other)
                || other.starts_with(root)
                || root_key == other_key
                || root_key.starts_with(&(other_key.clone() + "/"))
                || other_key.starts_with(&(root_key + "/"))
            {
                return Err(VerificationInputContractError(
                    "overlapping verification role roots",
                ));
            }
        }
    }
    Ok(())
}

pub fn checked_project_input_root(value: &str) -> Result<PathBuf, VerificationInputContractError> {
    if value.is_empty()
        || value.len() > 256
        || value
            .chars()
            .any(|c| c.is_control() || matches!(c, '\\' | ':' | '*' | '?' | '[' | ']'))
        || value
            .split('/')
            .any(|part| part.is_empty() || matches!(part, "." | ".." | ".git" | "target"))
        || value.split('/').next() == Some(".agents")
        || Path::new(value).is_absolute()
    {
        return Err(VerificationInputContractError(
            "verification role path must be explicit and relative",
        ));
    }
    Ok(PathBuf::from(value))
}

fn validate_limits(limits: &VerificationInputLimits) -> Result<(), VerificationInputContractError> {
    if limits.max_files == 0
        || limits.max_files > MAX_IDENTITY_FILES
        || limits.max_total_bytes == 0
        || limits.max_total_bytes > MAX_IDENTITY_BYTES
        || limits.max_file_bytes == 0
        || limits.max_file_bytes > MAX_IDENTITY_FILE_BYTES
        || limits.max_file_bytes > limits.max_total_bytes
    {
        return Err(VerificationInputContractError(
            "verification identity bounds",
        ));
    }
    Ok(())
}

impl VerificationInputScope {
    pub fn validate(&self) -> Result<(), VerificationInputContractError> {
        if self.excluded_components != [".git", "target"]
            || self.max_files != SOURCE_MAX_FILES
            || self.max_total_bytes != SOURCE_MAX_TOTAL_BYTES
            || self.max_file_bytes != SOURCE_MAX_FILE_BYTES
        {
            return Err(VerificationInputContractError(
                "unsupported verification source bounds",
            ));
        }
        match &self.project_inputs {
            None if self.method == "workspace_revision_v1" && self.program_path.is_none() => Ok(()),
            Some(inputs) if self.method == PROJECT_VERIFICATION_METHOD => {
                validate_project_verification_inputs(inputs)?;
                let path = self
                    .program_path
                    .as_deref()
                    .filter(|p| !p.chars().any(char::is_control))
                    .ok_or(VerificationInputContractError(
                        "verifier executable binding missing",
                    ))?;
                if !Path::new(path).is_absolute() {
                    return Err(VerificationInputContractError(
                        "verifier executable must be absolute",
                    ));
                }
                Ok(())
            }
            _ => Err(VerificationInputContractError(
                "unsupported verification input method",
            )),
        }
    }
}

impl VerificationInputSnapshot {
    pub fn validate_for_scope(
        &self,
        scope: &VerificationInputScope,
    ) -> Result<(), VerificationInputContractError> {
        self.validate()?;
        scope.validate()?;
        if self.method != scope.method {
            return Err(VerificationInputContractError(
                "snapshot input method differs from scope",
            ));
        }
        if let Some(inputs) = &scope.project_inputs {
            if self.dependencies.len() != inputs.dependency_roots.len() + 1 {
                return Err(VerificationInputContractError(
                    "installed input coverage incomplete",
                ));
            }
            let mut bytes = 0_u64;
            let mut files = 0_u64;
            for (identity, expected) in self.dependencies.iter().zip(
                inputs
                    .dependency_roots
                    .iter()
                    .map(String::as_str)
                    .chain(std::iter::once("[verifier_program]")),
            ) {
                if identity.path != expected {
                    return Err(VerificationInputContractError(
                        "installed input identity order or root changed",
                    ));
                }
                bytes = bytes.saturating_add(identity.total_bytes);
                files = files.saturating_add(identity.file_count);
                if expected == "[verifier_program]"
                    && (identity.file_count != 1
                        || identity.total_bytes != inputs.program.bytes
                        || identity.digest != inputs.program.sha256)
                {
                    return Err(VerificationInputContractError(
                        "verifier program identity differs from declaration",
                    ));
                }
            }
            if files > inputs.dependency_limits.max_files
                || bytes > inputs.dependency_limits.max_total_bytes
            {
                return Err(VerificationInputContractError(
                    "installed input exceeds reviewed bounds",
                ));
            }
        }
        Ok(())
    }

    pub fn validate(&self) -> Result<(), VerificationInputContractError> {
        if !valid_digest(&self.digest)
            || self.file_count > SOURCE_MAX_FILES
            || self.total_bytes > SOURCE_MAX_TOTAL_BYTES
        {
            return Err(VerificationInputContractError(
                "invalid verification source snapshot",
            ));
        }
        match self.method.as_str() {
            "workspace_revision_v1" if self.dependencies.is_empty() => Ok(()),
            PROJECT_VERIFICATION_METHOD => {
                if self.dependencies.is_empty() || self.dependencies.len() > MAX_ROLE_ROOTS + 1 {
                    return Err(VerificationInputContractError(
                        "invalid installed input snapshot",
                    ));
                }
                let mut names = std::collections::BTreeSet::new();
                let mut bytes = 0_u64;
                let mut files = 0_u64;
                for item in &self.dependencies {
                    if !item.exists
                        || !valid_digest(&item.digest)
                        || !names.insert(&item.path)
                        || (item.path != "[verifier_program]"
                            && checked_project_input_root(&item.path).is_err())
                    {
                        return Err(VerificationInputContractError(
                            "invalid installed input identity",
                        ));
                    }
                    bytes = bytes.saturating_add(item.total_bytes);
                    files = files.saturating_add(item.file_count);
                }
                if bytes > MAX_IDENTITY_BYTES
                    || files > MAX_IDENTITY_FILES
                    || !names.contains(&"[verifier_program]".to_string())
                {
                    return Err(VerificationInputContractError(
                        "installed input snapshot bounds",
                    ));
                }
                Ok(())
            }
            _ => Err(VerificationInputContractError(
                "unsupported verification snapshot method",
            )),
        }
    }
}

impl crate::goal_supervision::VerificationGeneratedOutputs {
    pub fn validate_for_scope(
        &self,
        scope: &VerificationInputScope,
        required_after: bool,
    ) -> Result<(), VerificationInputContractError> {
        scope.validate()?;
        let inputs = scope
            .project_inputs
            .as_ref()
            .ok_or(VerificationInputContractError(
                "legacy contract cannot carry generated identities",
            ))?;
        for (rows, required) in [(&self.before, false), (&self.after, required_after)] {
            if rows.len() != inputs.generated_output_roots.len() {
                return Err(VerificationInputContractError(
                    "generated output coverage incomplete",
                ));
            }
            let mut files = 0_u64;
            let mut bytes = 0_u64;
            for (row, path) in rows.iter().zip(&inputs.generated_output_roots) {
                if &row.path != path
                    || !valid_digest(&row.digest)
                    || (required && !row.exists)
                    || (!row.exists && (row.file_count != 0 || row.total_bytes != 0))
                {
                    return Err(VerificationInputContractError(
                        "generated output identity invalid",
                    ));
                }
                files = files.saturating_add(row.file_count);
                bytes = bytes.saturating_add(row.total_bytes);
            }
            if files > inputs.generated_limits.max_files
                || bytes > inputs.generated_limits.max_total_bytes
            {
                return Err(VerificationInputContractError(
                    "generated output exceeds reviewed bounds",
                ));
            }
        }
        Ok(())
    }
}
