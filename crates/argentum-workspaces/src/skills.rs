use std::collections::BTreeMap;
use std::fs;
#[cfg(test)]
use std::path::Path;
use std::path::PathBuf;

use serde::Deserialize;
use sha2::{Digest, Sha256};
use thiserror::Error;

use crate::{WorkspaceError, WorkspaceManager};

pub const SKILL_ROOT_RELATIVE_PATH: &str = ".agents/skills";
pub const MAX_SKILL_DISCOVERY_ENTRIES: usize = 256;
pub const MAX_SKILL_FILE_BYTES: usize = 256 * 1024;
pub const MAX_SKILL_FRONTMATTER_BYTES: usize = 16 * 1024;
pub const MAX_SKILL_DESCRIPTION_CHARS: usize = 1_024;
pub const MAX_SKILL_BODY_BYTES: usize = 240 * 1024;
pub const MAX_SKILL_CATALOG_BYTES: usize = 64 * 1024;
const MAX_METADATA_FIELDS: usize = 16;
const MAX_METADATA_KEY_CHARS: usize = 64;
const MAX_METADATA_VALUE_CHARS: usize = 512;
const MAX_COMPATIBILITY_CHARS: usize = 500;
const MAX_LICENSE_CHARS: usize = 256;

#[derive(Debug, Error)]
pub enum SkillError {
    #[error("skill name is invalid: {0}")]
    InvalidName(String),
    #[error("skill is unavailable: {0}")]
    Unavailable(String),
    #[error("skill name collides with another skill: {0}")]
    NameCollision(String),
    #[error("skill discovery failed for {path}: {source}")]
    DiscoveryIo {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("skill file could not be read: {0}")]
    Workspace(#[from] WorkspaceError),
    #[error("skill file is invalid: {0}")]
    InvalidDocument(String),
    #[error("skill file exceeds the {limit_bytes} byte limit")]
    FileTooLarge { limit_bytes: usize },
    #[error("skill catalog exceeds the {limit_bytes} byte projection limit")]
    CatalogTooLarge { limit_bytes: usize },
    #[error("selected skill changed or became unavailable: {0}")]
    Stale(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkillSummary {
    pub name: Option<String>,
    pub description: Option<String>,
    pub relative_path: String,
    pub base_path: String,
    pub digest: Option<String>,
    pub valid: bool,
    pub error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkillCatalog {
    pub entries: Vec<SkillSummary>,
    pub complete: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkillInspection {
    pub summary: SkillSummary,
    pub body: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SelectedSkill {
    pub name: String,
    pub description: String,
    pub relative_path: String,
    pub base_path: String,
    pub digest: String,
    pub body: String,
}

impl SelectedSkill {
    pub fn verify_current(&self, workspace: &WorkspaceManager) -> Result<(), SkillError> {
        let current = read_document(workspace, &self.relative_path, &self.name)?;
        if current.summary.digest.as_deref() != Some(self.digest.as_str())
            || current.summary.name.as_deref() != Some(self.name.as_str())
            || current.body != self.body
        {
            return Err(SkillError::Stale(self.relative_path.clone()));
        }
        Ok(())
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct SkillFrontmatter {
    name: String,
    description: String,
    #[serde(default)]
    license: Option<String>,
    #[serde(default)]
    compatibility: Option<String>,
    #[serde(default)]
    metadata: BTreeMap<String, String>,
    #[serde(rename = "allowed-tools", default)]
    allowed_tools: Option<String>,
}

pub fn discover_skills(workspace: &WorkspaceManager) -> Result<SkillCatalog, SkillError> {
    let root = workspace.root().join(SKILL_ROOT_RELATIVE_PATH);
    let metadata = match fs::symlink_metadata(&root) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(SkillCatalog {
                entries: Vec::new(),
                complete: true,
            });
        }
        Err(source) => return Err(SkillError::DiscoveryIo { path: root, source }),
    };
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err(SkillError::Unavailable(format!(
            "{} is not a regular skill directory",
            SKILL_ROOT_RELATIVE_PATH
        )));
    }

    let entries = fs::read_dir(&root).map_err(|source| SkillError::DiscoveryIo {
        path: root.clone(),
        source,
    })?;
    let mut candidates = Vec::new();
    let mut complete = true;
    for entry in entries.take(MAX_SKILL_DISCOVERY_ENTRIES + 1) {
        let entry = entry.map_err(|source| SkillError::DiscoveryIo {
            path: root.clone(),
            source,
        })?;
        if candidates.len() == MAX_SKILL_DISCOVERY_ENTRIES {
            complete = false;
            break;
        }
        let entry_type = entry
            .file_type()
            .map_err(|source| SkillError::DiscoveryIo {
                path: entry.path(),
                source,
            })?;
        if entry_type.is_symlink() || !entry_type.is_dir() {
            continue;
        }
        let Some(directory_name) = entry.file_name().to_str().map(str::to_owned) else {
            candidates.push(SkillCandidate {
                directory_name: None,
                relative_path: format!("{SKILL_ROOT_RELATIVE_PATH}/<non-utf8>/SKILL.md"),
            });
            continue;
        };
        candidates.push(SkillCandidate {
            relative_path: format!("{SKILL_ROOT_RELATIVE_PATH}/{directory_name}/SKILL.md"),
            directory_name: Some(directory_name),
        });
    }
    candidates.sort_by(|left, right| left.relative_path.cmp(&right.relative_path));

    let mut summaries = Vec::with_capacity(candidates.len());
    for candidate in candidates {
        summaries.push(match candidate.directory_name {
            Some(directory_name) => {
                match read_document(workspace, &candidate.relative_path, &directory_name) {
                    Ok(document) => document.summary,
                    Err(error) => {
                        let digest = workspace
                            .capture_file(&candidate.relative_path, MAX_SKILL_FILE_BYTES)
                            .ok()
                            .and_then(|capture| capture.sha256);
                        invalid_summary(&candidate.relative_path, &directory_name, digest, error)
                    }
                }
            }
            None => SkillSummary {
                name: None,
                description: None,
                relative_path: candidate.relative_path.clone(),
                base_path: candidate
                    .relative_path
                    .rsplit_once('/')
                    .map(|(base, _)| base.to_owned())
                    .unwrap_or_default(),
                digest: None,
                valid: false,
                error: Some("skill directory name is not valid UTF-8".into()),
            },
        });
    }

    mark_collisions(&mut summaries);
    let mut projected_bytes = 0_usize;
    summaries.retain(|summary| {
        let entry_bytes = summary
            .name
            .as_ref()
            .map_or(0, String::len)
            .saturating_add(summary.description.as_ref().map_or(0, String::len))
            .saturating_add(summary.relative_path.len())
            .saturating_add(summary.base_path.len())
            .saturating_add(summary.digest.as_ref().map_or(0, String::len))
            .saturating_add(summary.error.as_ref().map_or(0, String::len))
            .saturating_add(64);
        if projected_bytes.saturating_add(entry_bytes) > MAX_SKILL_CATALOG_BYTES {
            complete = false;
            false
        } else {
            projected_bytes = projected_bytes.saturating_add(entry_bytes);
            true
        }
    });
    Ok(SkillCatalog {
        entries: summaries,
        complete,
    })
}

pub fn inspect_skill(
    workspace: &WorkspaceManager,
    name: &str,
) -> Result<SkillInspection, SkillError> {
    validate_skill_name(name)?;
    let relative_path = skill_file_path(name);
    read_document(workspace, &relative_path, name).map_err(|error| match error {
        SkillError::Workspace(WorkspaceError::Security(_)) => {
            SkillError::Unavailable(name.to_owned())
        }
        other => other,
    })
}

pub fn select_skill(workspace: &WorkspaceManager, name: &str) -> Result<SelectedSkill, SkillError> {
    let inspection = inspect_skill(workspace, name)?;
    if !inspection.summary.valid {
        return Err(SkillError::InvalidDocument(
            inspection
                .summary
                .error
                .unwrap_or_else(|| "invalid skill document".into()),
        ));
    }
    let summary = inspection.summary;
    Ok(SelectedSkill {
        name: summary.name.expect("validated skill name"),
        description: summary.description.expect("validated description"),
        relative_path: summary.relative_path,
        base_path: summary.base_path,
        digest: summary.digest.expect("validated digest"),
        body: inspection.body,
    })
}

#[derive(Debug)]
struct SkillCandidate {
    directory_name: Option<String>,
    relative_path: String,
}

fn read_document(
    workspace: &WorkspaceManager,
    relative_path: &str,
    directory_name: &str,
) -> Result<SkillInspection, SkillError> {
    let capture = workspace.capture_file(relative_path, MAX_SKILL_FILE_BYTES)?;
    if !capture.exists {
        return Err(SkillError::Unavailable(directory_name.to_owned()));
    }
    if capture.too_large || capture.bytes.is_none() {
        return Err(SkillError::FileTooLarge {
            limit_bytes: MAX_SKILL_FILE_BYTES,
        });
    }
    let digest = capture.sha256.unwrap_or_else(|| {
        let mut hasher = Sha256::new();
        hasher.update(capture.bytes.as_deref().unwrap_or_default());
        format!("{:x}", hasher.finalize())
    });
    let bytes = capture.bytes.expect("checked above");
    let text = String::from_utf8(bytes)
        .map_err(|_| SkillError::InvalidDocument("SKILL.md must be valid UTF-8".into()))?;
    let (frontmatter, body) = parse_document(&text)?;
    let summary = SkillSummary {
        name: Some(frontmatter.name.clone()),
        description: Some(frontmatter.description.clone()),
        relative_path: relative_path.to_owned(),
        base_path: relative_path
            .rsplit_once('/')
            .map(|(base, _)| base.to_owned())
            .unwrap_or_default(),
        digest: Some(digest),
        valid: true,
        error: None,
    };
    if frontmatter.name != directory_name {
        let mut summary = summary;
        summary.valid = false;
        summary.error = Some("frontmatter name must match its parent directory".into());
        return Ok(SkillInspection { summary, body });
    }
    Ok(SkillInspection { summary, body })
}

fn parse_document(text: &str) -> Result<(SkillFrontmatter, String), SkillError> {
    let mut lines = text.split_inclusive('\n');
    let opening = lines
        .next()
        .ok_or_else(|| SkillError::InvalidDocument("missing YAML frontmatter".into()))?;
    if opening.trim_end_matches(['\r', '\n']) != "---" {
        return Err(SkillError::InvalidDocument(
            "SKILL.md must start with YAML frontmatter".into(),
        ));
    }

    let mut frontmatter_text = String::new();
    let mut body_start = opening.len();
    let mut closing_found = false;
    for line in lines {
        let normalized = line.trim_end_matches(['\r', '\n']);
        if normalized == "---" {
            body_start += line.len();
            closing_found = true;
            break;
        }
        if frontmatter_text.len().saturating_add(line.len()) > MAX_SKILL_FRONTMATTER_BYTES {
            return Err(SkillError::InvalidDocument(format!(
                "frontmatter exceeds the {MAX_SKILL_FRONTMATTER_BYTES} byte limit"
            )));
        }
        frontmatter_text.push_str(line);
        body_start += line.len();
    }
    if !closing_found {
        return Err(SkillError::InvalidDocument(
            "missing closing YAML frontmatter delimiter".into(),
        ));
    }
    let frontmatter: SkillFrontmatter =
        serde_yaml::from_str(&frontmatter_text).map_err(|error| {
            SkillError::InvalidDocument(format!("invalid YAML frontmatter: {error}"))
        })?;
    validate_frontmatter(&frontmatter)?;
    let body = text
        .get(body_start..)
        .ok_or_else(|| SkillError::InvalidDocument("invalid frontmatter boundary".into()))?
        .trim()
        .to_owned();
    if body.len() > MAX_SKILL_BODY_BYTES {
        return Err(SkillError::InvalidDocument(format!(
            "skill body exceeds the {MAX_SKILL_BODY_BYTES} byte limit"
        )));
    }
    if body.contains('\0') {
        return Err(SkillError::InvalidDocument(
            "skill body contains a forbidden NUL character".into(),
        ));
    }
    Ok((frontmatter, body))
}

fn validate_frontmatter(frontmatter: &SkillFrontmatter) -> Result<(), SkillError> {
    validate_skill_name(&frontmatter.name)?;
    validate_text_limit(
        "description",
        &frontmatter.description,
        MAX_SKILL_DESCRIPTION_CHARS,
        true,
    )?;
    if let Some(value) = &frontmatter.license {
        validate_text_limit("license", value, MAX_LICENSE_CHARS, false)?;
    }
    if let Some(value) = &frontmatter.compatibility {
        validate_text_limit("compatibility", value, MAX_COMPATIBILITY_CHARS, false)?;
    }
    if frontmatter.metadata.len() > MAX_METADATA_FIELDS {
        return Err(SkillError::InvalidDocument(format!(
            "metadata exceeds the {MAX_METADATA_FIELDS} field limit"
        )));
    }
    for (key, value) in &frontmatter.metadata {
        validate_text_limit("metadata key", key, MAX_METADATA_KEY_CHARS, false)?;
        validate_text_limit("metadata value", value, MAX_METADATA_VALUE_CHARS, false)?;
    }
    if let Some(value) = &frontmatter.allowed_tools {
        validate_text_limit("allowed-tools", value, MAX_METADATA_VALUE_CHARS, false)?;
    }
    Ok(())
}

fn validate_text_limit(
    field: &str,
    value: &str,
    max_chars: usize,
    non_empty: bool,
) -> Result<(), SkillError> {
    if (non_empty && value.trim().is_empty()) || value.chars().count() > max_chars {
        return Err(SkillError::InvalidDocument(format!(
            "{field} must be non-empty and at most {max_chars} characters"
        )));
    }
    if value
        .chars()
        .any(|character| character == '\0' || character.is_control())
    {
        return Err(SkillError::InvalidDocument(format!(
            "{field} contains a forbidden control character"
        )));
    }
    Ok(())
}

fn validate_skill_name(name: &str) -> Result<(), SkillError> {
    let valid = !name.is_empty()
        && name.len() <= 64
        && name
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
        && !name.starts_with('-')
        && !name.ends_with('-')
        && !name.contains("--");
    if valid {
        Ok(())
    } else {
        Err(SkillError::InvalidName(name.to_owned()))
    }
}

fn skill_file_path(name: &str) -> String {
    format!("{SKILL_ROOT_RELATIVE_PATH}/{name}/SKILL.md")
}

fn invalid_summary(
    relative_path: &str,
    directory_name: &str,
    digest: Option<String>,
    error: SkillError,
) -> SkillSummary {
    SkillSummary {
        name: None,
        description: None,
        relative_path: relative_path.to_owned(),
        base_path: relative_path
            .rsplit_once('/')
            .map(|(base, _)| base.to_owned())
            .unwrap_or_default(),
        digest,
        valid: false,
        error: Some(format!("{directory_name}: {error}")),
    }
}

fn mark_collisions(entries: &mut [SkillSummary]) {
    let mut by_name = BTreeMap::<String, Vec<usize>>::new();
    for (index, entry) in entries.iter().enumerate() {
        if entry.valid {
            if let Some(name) = &entry.name {
                by_name
                    .entry(name.to_ascii_lowercase())
                    .or_default()
                    .push(index);
            }
        }
    }
    for indexes in by_name.values().filter(|indexes| indexes.len() > 1) {
        for index in indexes {
            entries[*index].valid = false;
            entries[*index].error =
                Some("skill name collides with another discovered skill".into());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use argentum_domain::Capability;
    use argentum_security::{ApprovalPolicy, CapabilityBroker};

    fn manager(path: &Path) -> WorkspaceManager {
        WorkspaceManager::new(
            CapabilityBroker::new(path, ApprovalPolicy::default()).expect("broker"),
        )
    }

    #[test]
    fn parses_crlf_and_ignores_allowed_tools_without_granting_authority() {
        let directory = tempfile::tempdir().expect("workspace");
        let skill = directory.path().join(".agents/skills/rust-fix-and-verify");
        fs::create_dir_all(&skill).expect("skill directory");
        fs::write(
            skill.join("SKILL.md"),
            "---\r\nname: rust-fix-and-verify\r\ndescription: Fix and verify Rust changes.\r\nallowed-tools: Bash(cargo:*)\r\nmetadata:\r\n  version: '1'\r\n---\r\nRead, edit, and verify in bounded steps.\r\n",
        )
        .expect("skill");

        let selected =
            select_skill(&manager(directory.path()), "rust-fix-and-verify").expect("selected");
        assert_eq!(selected.body, "Read, edit, and verify in bounded steps.");
        assert_eq!(selected.base_path, ".agents/skills/rust-fix-and-verify");
        assert!(manager(directory.path())
            .requires_approval(Capability::ExecuteCommands)
            .is_ok());
    }

    #[test]
    fn rejects_parent_name_mismatch_and_invalid_names() {
        let directory = tempfile::tempdir().expect("workspace");
        let skill = directory.path().join(".agents/skills/wrong-directory");
        fs::create_dir_all(&skill).expect("skill directory");
        fs::write(
            skill.join("SKILL.md"),
            "---\nname: valid-name\ndescription: Valid description.\n---\nBody\n",
        )
        .expect("skill");
        let catalog = discover_skills(&manager(directory.path())).expect("catalog");
        assert_eq!(catalog.entries.len(), 1);
        assert!(!catalog.entries[0].valid);
        assert!(matches!(
            select_skill(&manager(directory.path()), "../escape"),
            Err(SkillError::InvalidName(_))
        ));
    }

    #[test]
    fn stale_source_is_rejected() {
        let directory = tempfile::tempdir().expect("workspace");
        let skill = directory.path().join(".agents/skills/example");
        fs::create_dir_all(&skill).expect("skill directory");
        fs::write(
            skill.join("SKILL.md"),
            "---\nname: example\ndescription: Example skill.\n---\nOriginal\n",
        )
        .expect("skill");
        let workspace = manager(directory.path());
        let selected = select_skill(&workspace, "example").expect("selected");
        fs::write(
            skill.join("SKILL.md"),
            "---\nname: example\ndescription: Example skill.\n---\nChanged\n",
        )
        .expect("changed skill");
        assert!(matches!(
            selected.verify_current(&workspace),
            Err(SkillError::Stale(_))
        ));
    }

    #[test]
    fn duplicate_authority_fields_are_rejected() {
        let directory = tempfile::tempdir().expect("workspace");
        let skill = directory.path().join(".agents/skills/example");
        fs::create_dir_all(&skill).expect("skill directory");
        fs::write(
            skill.join("SKILL.md"),
            "---\nname: example\nname: other\ndescription: Example description.\n---\nBody\n",
        )
        .expect("skill");
        let catalog = discover_skills(&manager(directory.path())).expect("catalog");
        assert_eq!(catalog.entries.len(), 1);
        assert!(!catalog.entries[0].valid);
        assert!(catalog.entries[0]
            .error
            .as_deref()
            .is_some_and(|error| error.contains("duplicate")));
        assert!(matches!(
            select_skill(&manager(directory.path()), "example"),
            Err(SkillError::InvalidDocument(_))
        ));
    }
}
