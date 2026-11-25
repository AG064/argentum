// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 AG064

//! Finds what the owner set up in Claude Code, Codex and OpenCode, so they
//! can bring it into Argentum.
//!
//! [`scan`] only reads. It lists MCP servers, skills and custom commands from
//! the usual places, each as an [`ImportItem`] the owner can tick. Nothing is
//! imported until the owner picks items, and then:
//!
//! - an MCP server becomes a draft that is off ([`ImportedMcpServer`]), with
//!   the names of its environment variables and headers but never their
//!   values;
//! - a skill or a custom command becomes a project skill in
//!   `.agents/skills/<name>/SKILL.md`, with `allowed-tools` and other fields
//!   Argentum does not use dropped, and without the other files of its folder.
//!
//! An import never grants a permission, approves a tool or turns anything on.
//! What is read is untrusted text and is treated as data.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use argentum_domain::imports::{
    valid_imported_name, ImportItemView, ImportOrigin, ImportResultView, ImportedMcpServer,
    ImportedMcpTransport,
};
use argentum_security::credentials::looks_like_credential;
use argentum_workspaces::{check_skill_document, SKILL_ROOT_RELATIVE_PATH};
use serde::Serialize;
use thiserror::Error;

/// Largest settings file read. `~/.claude.json` keeps history and can be big.
const MAX_SETTINGS_BYTES: u64 = 8 * 1024 * 1024;
const MAX_DOCUMENT_BYTES: u64 = 256 * 1024;
const MAX_ITEMS: usize = 512;
const MAX_COMMAND_DEPTH: usize = 3;
const MAX_DESCRIPTION_CHARS: usize = 1_000;
const REMOVED_SECRET: &str = "<removed: looked like a secret>";

/// What one found item would become.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ImportKind {
    /// Project instructions that load by themselves, shown for information.
    Instructions {
        path: String,
    },
    McpServer {
        server: ImportedMcpServer,
    },
    Skill {
        skill: SkillDraft,
    },
    Command {
        skill: SkillDraft,
    },
}

/// A skill ready to be written as `SKILL.md`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SkillDraft {
    pub name: String,
    pub description: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub license: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub compatibility: Option<String>,
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub metadata: BTreeMap<String, String>,
    #[serde(skip)]
    pub body: String,
}

/// One thing found in another agent's settings.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ImportItem {
    /// Stable within one scan, such as `mcp:codex_user:github`.
    pub id: String,
    pub origin: ImportOrigin,
    pub source_path: String,
    #[serde(flatten)]
    pub kind: ImportKind,
    /// What was left out, such as `allowed-tools` or an argument that looked
    /// like a secret.
    pub dropped: Vec<String>,
    /// Why it cannot be imported as it is, such as a name already in use.
    pub conflict: Option<String>,
}

impl ImportItem {
    /// False for items shown only for information, and for conflicts.
    pub fn importable(&self) -> bool {
        self.conflict.is_none() && !matches!(self.kind, ImportKind::Instructions { .. })
    }

    /// The item's name, as the owner sees it.
    pub fn name(&self) -> &str {
        match &self.kind {
            ImportKind::Instructions { path } => path,
            ImportKind::McpServer { server } => &server.name,
            ImportKind::Skill { skill } | ImportKind::Command { skill } => &skill.name,
        }
    }

    /// Where an import would write, relative to the project, for skills and
    /// commands. Shown before anything is written.
    pub fn destination(&self) -> Option<String> {
        match &self.kind {
            ImportKind::Skill { skill } | ImportKind::Command { skill } => Some(format!(
                "{SKILL_ROOT_RELATIVE_PATH}/{}/SKILL.md",
                skill.name
            )),
            _ => None,
        }
    }

    /// A short line such as `MCP server github: npx -y server (stdio)`.
    pub fn summary(&self) -> String {
        match &self.kind {
            ImportKind::Instructions { path } => {
                format!("Instructions {path}: loaded with AGENTS.md, nothing to import")
            }
            ImportKind::McpServer { server } => {
                let reach = match &server.transport {
                    ImportedMcpTransport::Stdio { command, args } => {
                        let mut line = command.clone();
                        for arg in args.iter().take(6) {
                            line.push(' ');
                            line.push_str(arg);
                        }
                        if args.len() > 6 {
                            line.push_str(" ...");
                        }
                        line
                    }
                    ImportedMcpTransport::Http { url } => url.clone(),
                };
                format!("MCP server {}: {reach}", server.name)
            }
            ImportKind::Skill { skill } => format!("Skill {}: {}", skill.name, skill.description),
            ImportKind::Command { skill } => {
                format!("Command /{} as a skill: {}", skill.name, skill.description)
            }
        }
    }
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct ImportScan {
    pub items: Vec<ImportItem>,
    /// Files that were found but could not be read, in plain words.
    pub problems: Vec<String>,
}

impl ImportScan {
    pub fn importable(&self) -> impl Iterator<Item = &ImportItem> {
        self.items.iter().filter(|item| item.importable())
    }

    pub fn find(&self, id: &str) -> Option<&ImportItem> {
        self.items.iter().find(|item| item.id == id)
    }
}

#[derive(Debug, Error)]
pub enum ImportError {
    #[error("a skill named {0} is already in this project")]
    SkillExists(String),
    #[error("the project skill folder is a link or not a folder")]
    SkillFolderUnsafe,
    #[error("the skill would not load: {0}")]
    InvalidSkill(String),
    #[error("the skill could not be written: {0}")]
    Io(#[from] std::io::Error),
}

/// Lists what can be imported. `home` is the owner's home folder, or `None`
/// to look in the project only.
pub fn scan(home: Option<&Path>, workspace: &Path) -> ImportScan {
    let mut scan = Scanner {
        scan: ImportScan::default(),
        workspace: workspace.to_path_buf(),
        existing_skills: existing_skill_names(workspace),
        seen_skills: BTreeSet::new(),
        seen_servers: BTreeSet::new(),
    };
    for name in ["CLAUDE.md", "CLAUDE.local.md"] {
        if workspace.join(name).is_file() {
            scan.push(
                ImportOrigin::ClaudeCodeProject,
                workspace.join(name),
                "instructions",
                name,
                ImportKind::Instructions { path: name.into() },
                vec![],
            );
        }
    }
    // Project settings come before account settings, so a project's own
    // server or skill wins a name clash.
    scan.claude_mcp_file(&workspace.join(".mcp.json"));
    scan.opencode_config(ImportOrigin::OpencodeProject, workspace);
    scan.codex_config(
        ImportOrigin::CodexProject,
        &workspace.join(".codex/config.toml"),
    );
    if let Some(home) = home {
        scan.claude_user(home);
        scan.codex_config(ImportOrigin::CodexUser, &home.join(".codex/config.toml"));
        scan.opencode_config(ImportOrigin::OpencodeUser, &home.join(".config/opencode"));
    }
    scan.skills(
        ImportOrigin::ClaudeCodeProject,
        &workspace.join(".claude/skills"),
    );
    for folder in [".opencode/skill", ".opencode/skills"] {
        scan.skills(ImportOrigin::OpencodeProject, &workspace.join(folder));
    }
    if let Some(home) = home {
        scan.skills(ImportOrigin::ClaudeCodeUser, &home.join(".claude/skills"));
        scan.skills(ImportOrigin::CodexUser, &home.join(".codex/skills"));
        scan.skills(ImportOrigin::CodexUser, &home.join(".agents/skills"));
        for folder in [".config/opencode/skill", ".config/opencode/skills"] {
            scan.skills(ImportOrigin::OpencodeUser, &home.join(folder));
        }
    }
    scan.commands(
        ImportOrigin::ClaudeCodeProject,
        &workspace.join(".claude/commands"),
    );
    for folder in [".opencode/command", ".opencode/commands"] {
        scan.commands(ImportOrigin::OpencodeProject, &workspace.join(folder));
    }
    if let Some(home) = home {
        scan.commands(ImportOrigin::ClaudeCodeUser, &home.join(".claude/commands"));
        scan.commands(ImportOrigin::CodexUser, &home.join(".codex/prompts"));
        for folder in [".config/opencode/command", ".config/opencode/commands"] {
            scan.commands(ImportOrigin::OpencodeUser, &home.join(folder));
        }
    }
    scan.scan
}

struct Scanner {
    scan: ImportScan,
    workspace: PathBuf,
    existing_skills: BTreeSet<String>,
    seen_skills: BTreeSet<String>,
    seen_servers: BTreeSet<String>,
}

impl Scanner {
    fn push(
        &mut self,
        origin: ImportOrigin,
        source: PathBuf,
        prefix: &str,
        name: &str,
        kind: ImportKind,
        dropped: Vec<String>,
    ) {
        if self.scan.items.len() >= MAX_ITEMS {
            return;
        }
        let conflict = match &kind {
            ImportKind::Instructions { .. } => None,
            ImportKind::McpServer { .. } => (!self.seen_servers.insert(name.to_owned()))
                .then(|| "another server found earlier has this name".to_owned()),
            ImportKind::Skill { .. } | ImportKind::Command { .. } => {
                if self.existing_skills.contains(name) {
                    Some("a skill with this name is already in this project".to_owned())
                } else if !self.seen_skills.insert(name.to_owned()) {
                    Some("another skill found earlier has this name".to_owned())
                } else {
                    None
                }
            }
        };
        let id = format!("{prefix}:{}:{name}", origin.as_str());
        self.scan.items.push(ImportItem {
            id,
            origin,
            source_path: display_path(&source, &self.workspace),
            kind,
            dropped,
            conflict,
        });
    }

    fn problem(&mut self, path: &Path, what: &str) {
        let line = format!("{}: {what}", display_path(path, &self.workspace));
        if self.scan.problems.len() < 64 && !self.scan.problems.contains(&line) {
            self.scan.problems.push(line);
        }
    }

    fn read(&mut self, path: &Path, limit: u64) -> Option<String> {
        let metadata = fs::metadata(path).ok()?;
        if !metadata.is_file() {
            return None;
        }
        if metadata.len() > limit {
            self.problem(path, "too large to read");
            return None;
        }
        match fs::read(path).map(String::from_utf8) {
            Ok(Ok(text)) => Some(text),
            Ok(Err(_)) => {
                self.problem(path, "not UTF-8 text");
                None
            }
            Err(_) => {
                self.problem(path, "could not be read");
                None
            }
        }
    }

    fn claude_mcp_file(&mut self, path: &Path) {
        let Some(text) = self.read(path, MAX_SETTINGS_BYTES) else {
            return;
        };
        match serde_json::from_str::<serde_json::Value>(&text) {
            Ok(value) => {
                if let Some(servers) = value.get("mcpServers") {
                    self.claude_servers(ImportOrigin::ClaudeCodeProject, path, servers);
                }
            }
            Err(_) => self.problem(path, "not valid JSON"),
        }
    }

    fn claude_user(&mut self, home: &Path) {
        let path = home.join(".claude.json");
        let Some(text) = self.read(&path, MAX_SETTINGS_BYTES) else {
            return;
        };
        let Ok(value) = serde_json::from_str::<serde_json::Value>(&text) else {
            self.problem(&path, "not valid JSON");
            return;
        };
        // Servers Claude Code keeps for this project only come first.
        if let Some(projects) = value.get("projects").and_then(|p| p.as_object()) {
            let here = comparable_path(&self.workspace);
            for (key, project) in projects {
                if comparable_path(Path::new(key)) == here {
                    if let Some(servers) = project.get("mcpServers") {
                        self.claude_servers(ImportOrigin::ClaudeCodeProject, &path, servers);
                    }
                }
            }
        }
        if let Some(servers) = value.get("mcpServers") {
            self.claude_servers(ImportOrigin::ClaudeCodeUser, &path, servers);
        }
    }

    /// `{"name": {"command", "args", "env"} | {"type": "http", "url", "headers"}}`,
    /// shared by Claude Code's files.
    fn claude_servers(&mut self, origin: ImportOrigin, path: &Path, servers: &serde_json::Value) {
        let Some(servers) = servers.as_object() else {
            return;
        };
        for (name, entry) in servers {
            let command = entry.get("command").and_then(|v| v.as_str());
            let url = entry.get("url").and_then(|v| v.as_str());
            let args = string_list(entry.get("args"));
            let env = object_keys(entry.get("env"));
            let headers = object_keys(entry.get("headers"));
            self.server(
                origin,
                path,
                name,
                command.map(|c| (c, args)),
                url,
                env,
                headers,
            );
        }
    }

    fn codex_config(&mut self, origin: ImportOrigin, path: &Path) {
        let Some(text) = self.read(path, MAX_SETTINGS_BYTES) else {
            return;
        };
        let Ok(value) = toml::from_str::<toml::Table>(&text) else {
            self.problem(path, "not valid TOML");
            return;
        };
        let Some(servers) = value.get("mcp_servers").and_then(|v| v.as_table()) else {
            return;
        };
        for (name, entry) in servers {
            let command = entry.get("command").and_then(|v| v.as_str());
            let url = entry.get("url").and_then(|v| v.as_str());
            let args: Vec<String> = entry
                .get("args")
                .and_then(|v| v.as_array())
                .map(|items| {
                    items
                        .iter()
                        .filter_map(|item| item.as_str().map(str::to_owned))
                        .collect()
                })
                .unwrap_or_default();
            let keys = |key: &str| -> Vec<String> {
                entry
                    .get(key)
                    .and_then(|v| v.as_table())
                    .map(|table| table.keys().cloned().collect())
                    .unwrap_or_default()
            };
            let mut env = keys("env");
            if let Some(names) = entry.get("env_vars").and_then(|v| v.as_array()) {
                env.extend(names.iter().filter_map(|n| n.as_str().map(str::to_owned)));
            }
            let mut headers = keys("http_headers");
            headers.extend(keys("env_http_headers"));
            if entry.get("bearer_token_env_var").is_some() {
                headers.push("Authorization".into());
            }
            self.server(
                origin,
                path,
                name,
                command.map(|c| (c, args)),
                url,
                env,
                headers,
            );
        }
    }

    /// `opencode.json` or `opencode.jsonc` in `folder`, with
    /// `{"mcp": {"name": {"type": "local", "command": [...], "environment": {}}
    /// | {"type": "remote", "url", "headers"}}}`.
    fn opencode_config(&mut self, origin: ImportOrigin, folder: &Path) {
        for file in ["opencode.json", "opencode.jsonc"] {
            let path = folder.join(file);
            let Some(text) = self.read(&path, MAX_SETTINGS_BYTES) else {
                continue;
            };
            let Ok(value) = serde_json::from_str::<serde_json::Value>(&strip_json_comments(&text))
            else {
                self.problem(&path, "not valid JSON");
                continue;
            };
            let Some(servers) = value.get("mcp").and_then(|v| v.as_object()) else {
                continue;
            };
            for (name, entry) in servers {
                let mut command = string_list(entry.get("command"));
                let url = entry.get("url").and_then(|v| v.as_str());
                let env = object_keys(entry.get("environment"));
                let headers = object_keys(entry.get("headers"));
                let program = (!command.is_empty()).then(|| {
                    let program = command.remove(0);
                    (program, command)
                });
                self.server(
                    origin,
                    &path,
                    name,
                    program.as_ref().map(|(p, a)| (p.as_str(), a.clone())),
                    url,
                    env,
                    headers,
                );
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn server(
        &mut self,
        origin: ImportOrigin,
        path: &Path,
        raw_name: &str,
        command: Option<(&str, Vec<String>)>,
        url: Option<&str>,
        env_names: Vec<String>,
        header_names: Vec<String>,
    ) {
        let name = normalize_name(raw_name);
        if !valid_imported_name(&name) {
            self.problem(path, "an MCP server has a name that cannot be used");
            return;
        }
        let mut dropped = Vec::new();
        let transport = match (command, url) {
            (Some((command, args)), _) => {
                if looks_like_credential(command) {
                    self.problem(path, "an MCP server command looked like it holds a secret");
                    return;
                }
                let args = args
                    .into_iter()
                    .map(|arg| {
                        if looks_like_credential(&arg) {
                            dropped.push("an argument that looked like a secret".to_owned());
                            REMOVED_SECRET.to_owned()
                        } else {
                            arg
                        }
                    })
                    .collect();
                ImportedMcpTransport::Stdio {
                    command: command.to_owned(),
                    args,
                }
            }
            (None, Some(url)) => {
                let (url, cleaned) = clean_url(url);
                if cleaned {
                    dropped.push("credentials or a query in the address".to_owned());
                }
                ImportedMcpTransport::Http { url }
            }
            (None, None) => {
                self.problem(path, "an MCP server has neither a command nor an address");
                return;
            }
        };
        if !env_names.is_empty() {
            dropped.push(format!(
                "values of {}: enter them again before you turn it on",
                env_names.join(", ")
            ));
        }
        if !header_names.is_empty() {
            dropped.push(format!("values of headers {}", header_names.join(", ")));
        }
        let server = ImportedMcpServer {
            name: name.clone(),
            origin,
            source_path: display_path(path, &self.workspace),
            transport,
            env_names: env_names.into_iter().take(128).collect(),
            header_names: header_names.into_iter().take(128).collect(),
            enabled: false,
        };
        if server.validate().is_err() {
            self.problem(path, "an MCP server entry is not in a usable form");
            return;
        }
        self.push(
            origin,
            path.to_path_buf(),
            "mcp",
            &name,
            ImportKind::McpServer { server },
            dropped,
        );
    }

    /// `folder/<name>/SKILL.md`.
    fn skills(&mut self, origin: ImportOrigin, folder: &Path) {
        let Ok(entries) = fs::read_dir(folder) else {
            return;
        };
        let mut paths: Vec<_> = entries.filter_map(Result::ok).map(|e| e.path()).collect();
        paths.sort();
        for directory in paths.into_iter().take(MAX_ITEMS) {
            let file = directory.join("SKILL.md");
            if !directory.is_dir() || !file.is_file() {
                continue;
            }
            let Some(text) = self.read(&file, MAX_DOCUMENT_BYTES) else {
                continue;
            };
            let folder_name = directory
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or_default();
            let (fields, body) = split_frontmatter(&text);
            let mut dropped = Vec::new();
            let skill = draft_from(fields, body, folder_name, &mut dropped);
            let others = fs::read_dir(&directory)
                .map(|entries| {
                    entries
                        .filter_map(Result::ok)
                        .any(|entry| entry.file_name() != "SKILL.md")
                })
                .unwrap_or(false);
            if others {
                dropped.push("the other files in its folder".into());
            }
            let Some(skill) = skill else {
                self.problem(&file, "a skill without a usable name");
                continue;
            };
            let name = skill.name.clone();
            self.push(
                origin,
                file,
                "skill",
                &name,
                ImportKind::Skill { skill },
                dropped,
            );
        }
    }

    /// Markdown files under `folder`, with sub-folders joined to the name as
    /// `folder-name`, the way Claude Code names them.
    fn commands(&mut self, origin: ImportOrigin, folder: &Path) {
        let mut files = Vec::new();
        collect_markdown(folder, folder, 0, &mut files);
        files.sort();
        for (file, stem) in files.into_iter().take(MAX_ITEMS) {
            let Some(text) = self.read(&file, MAX_DOCUMENT_BYTES) else {
                continue;
            };
            let (fields, body) = split_frontmatter(&text);
            let mut dropped = Vec::new();
            let Some(mut skill) = draft_from(fields, body, &stem, &mut dropped) else {
                self.problem(&file, "a command without a usable name");
                continue;
            };
            skill.body = format!(
                "This skill was the custom command `/{}`. Where the text says $ARGUMENTS, use what the owner asked for.\n\n{}",
                skill.name, skill.body
            );
            let name = skill.name.clone();
            self.push(
                origin,
                file,
                "command",
                &name,
                ImportKind::Command { skill },
                dropped,
            );
        }
    }
}

fn collect_markdown(root: &Path, folder: &Path, depth: usize, out: &mut Vec<(PathBuf, String)>) {
    if depth > MAX_COMMAND_DEPTH || out.len() >= MAX_ITEMS {
        return;
    }
    let Ok(entries) = fs::read_dir(folder) else {
        return;
    };
    for entry in entries.filter_map(Result::ok) {
        let Ok(kind) = entry.file_type() else {
            continue;
        };
        let path = entry.path();
        if kind.is_dir() {
            collect_markdown(root, &path, depth + 1, out);
        } else if kind.is_file() && path.extension().is_some_and(|e| e == "md") {
            let stem = path
                .strip_prefix(root)
                .unwrap_or(&path)
                .with_extension("")
                .components()
                .filter_map(|c| c.as_os_str().to_str())
                .collect::<Vec<_>>()
                .join("-");
            out.push((path, stem));
        }
    }
}

/// Builds a draft from a frontmatter mapping and body. Fields Argentum does
/// not read are listed in `dropped`. `allowed-tools` is dropped on purpose:
/// in other agents it lets a skill run tools without asking.
fn draft_from(
    fields: Option<serde_yaml::Mapping>,
    body: &str,
    fallback_name: &str,
    dropped: &mut Vec<String>,
) -> Option<SkillDraft> {
    let fields = fields.unwrap_or_default();
    let text = |key: &str| {
        fields
            .get(key)
            .and_then(|value| value.as_str())
            .map(clean_line)
            .filter(|value| !value.is_empty())
    };
    let name = text("name")
        .map(|name| normalize_name(&name))
        .filter(|name| skill_name_ok(name))
        .unwrap_or_else(|| normalize_name(fallback_name));
    if !skill_name_ok(&name) {
        return None;
    }
    let description = text("description")
        .or_else(|| {
            body.lines()
                .map(|line| clean_line(line.trim_start_matches('#')))
                .find(|line| !line.is_empty())
        })
        .unwrap_or_else(|| format!("Imported {name}"));
    let description: String = description.chars().take(MAX_DESCRIPTION_CHARS).collect();
    let mut metadata = BTreeMap::new();
    if let Some(serde_yaml::Value::Mapping(values)) = fields.get("metadata") {
        for (key, value) in values.iter().take(16) {
            if let (Some(key), Some(value)) = (key.as_str(), scalar_text(value)) {
                if key.len() <= 64 && !key.chars().any(char::is_control) {
                    metadata.insert(key.to_owned(), value.chars().take(500).collect());
                }
            }
        }
    }
    for key in fields.keys().filter_map(|key| key.as_str()) {
        if !matches!(
            key,
            "name" | "description" | "license" | "compatibility" | "metadata"
        ) {
            dropped.push(format!("the {key} field"));
        }
    }
    Some(SkillDraft {
        name,
        description,
        license: text("license").map(|v| v.chars().take(256).collect()),
        compatibility: text("compatibility").map(|v| v.chars().take(500).collect()),
        metadata,
        body: body.trim().replace('\0', ""),
    })
}

fn scalar_text(value: &serde_yaml::Value) -> Option<String> {
    match value {
        serde_yaml::Value::String(text) => Some(clean_line(text)),
        serde_yaml::Value::Bool(flag) => Some(flag.to_string()),
        serde_yaml::Value::Number(number) => Some(number.to_string()),
        _ => None,
    }
}

/// Splits `---\n...\n---\n` frontmatter from the body. A file without
/// frontmatter, or with frontmatter that is not a YAML mapping, is all body.
fn split_frontmatter(text: &str) -> (Option<serde_yaml::Mapping>, &str) {
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    let Some(rest) = text
        .strip_prefix("---\n")
        .or_else(|| text.strip_prefix("---\r\n"))
    else {
        return (None, text);
    };
    let mut offset = 0;
    for line in rest.split_inclusive('\n') {
        if line.trim_end_matches(['\r', '\n']) == "---" {
            let yaml = &rest[..offset];
            let body = &rest[offset + line.len()..];
            return match serde_yaml::from_str::<serde_yaml::Mapping>(yaml) {
                Ok(fields) => (Some(fields), body),
                Err(_) => (None, body),
            };
        }
        offset += line.len();
    }
    (None, text)
}

fn clean_line(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Lower case, with anything other than letters, digits and `-` turned into
/// `-`, as Argentum's skill names require.
fn normalize_name(raw: &str) -> String {
    let mut name = String::new();
    for character in raw.trim().chars() {
        let character = character.to_ascii_lowercase();
        if character.is_ascii_lowercase() || character.is_ascii_digit() {
            name.push(character);
        } else if !name.ends_with('-') && !name.is_empty() {
            name.push('-');
        }
    }
    let name: String = name.chars().take(64).collect();
    name.trim_end_matches('-').to_owned()
}

fn skill_name_ok(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 64
        && name
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
        && !name.starts_with('-')
        && !name.ends_with('-')
        && !name.contains("--")
}

fn string_list(value: Option<&serde_json::Value>) -> Vec<String> {
    value
        .and_then(|v| v.as_array())
        .map(|items| {
            items
                .iter()
                .filter_map(|item| item.as_str().map(str::to_owned))
                .collect()
        })
        .unwrap_or_default()
}

fn object_keys(value: Option<&serde_json::Value>) -> Vec<String> {
    value
        .and_then(|v| v.as_object())
        .map(|object| object.keys().cloned().collect())
        .unwrap_or_default()
}

/// Removes user names, passwords and, when it looks like it holds a key, the
/// query from an address. Returns whether anything was removed.
fn clean_url(url: &str) -> (String, bool) {
    let mut cleaned = false;
    let mut url = url.trim().to_owned();
    if let Some(scheme_end) = url.find("://") {
        let authority_start = scheme_end + 3;
        let authority_end = url[authority_start..]
            .find(['/', '?', '#'])
            .map_or(url.len(), |end| authority_start + end);
        if let Some(at) = url[authority_start..authority_end].rfind('@') {
            url.replace_range(authority_start..authority_start + at + 1, "");
            cleaned = true;
        }
    }
    if let Some(query) = url.find('?') {
        if looks_like_credential(&url[query..])
            || url[query..].to_ascii_lowercase().contains("key")
            || url[query..].to_ascii_lowercase().contains("token")
        {
            url.truncate(query);
            cleaned = true;
        }
    }
    (url, cleaned)
}

/// Removes `//` and `/* */` comments outside strings, for `.jsonc` files.
fn strip_json_comments(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    let mut in_string = false;
    while let Some(character) = chars.next() {
        if in_string {
            out.push(character);
            if character == '\\' {
                if let Some(next) = chars.next() {
                    out.push(next);
                }
            } else if character == '"' {
                in_string = false;
            }
            continue;
        }
        match (character, chars.peek()) {
            ('"', _) => {
                in_string = true;
                out.push(character);
            }
            ('/', Some('/')) => {
                for next in chars.by_ref() {
                    if next == '\n' {
                        out.push('\n');
                        break;
                    }
                }
            }
            ('/', Some('*')) => {
                chars.next();
                let mut previous = ' ';
                for next in chars.by_ref() {
                    if previous == '*' && next == '/' {
                        break;
                    }
                    previous = next;
                }
            }
            _ => out.push(character),
        }
    }
    out
}

fn comparable_path(path: &Path) -> String {
    let path = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    let text = path.to_string_lossy().replace('\\', "/");
    let text = text.strip_prefix("//?/").unwrap_or(&text);
    let text = text.trim_end_matches('/');
    if cfg!(windows) {
        text.to_lowercase()
    } else {
        text.to_owned()
    }
}

fn display_path(path: &Path, workspace: &Path) -> String {
    match path.strip_prefix(workspace) {
        Ok(relative) => relative.to_string_lossy().replace('\\', "/"),
        Err(_) => path.to_string_lossy().into_owned(),
    }
}

fn existing_skill_names(workspace: &Path) -> BTreeSet<String> {
    fs::read_dir(workspace.join(SKILL_ROOT_RELATIVE_PATH))
        .map(|entries| {
            entries
                .filter_map(Result::ok)
                .filter_map(|entry| entry.file_name().to_str().map(str::to_owned))
                .collect()
        })
        .unwrap_or_default()
}

/// The `SKILL.md` text for a draft.
pub fn render_skill(skill: &SkillDraft) -> Result<String, ImportError> {
    let yaml =
        serde_yaml::to_string(skill).map_err(|e| ImportError::InvalidSkill(e.to_string()))?;
    let text = format!("---\n{yaml}---\n\n{}\n", skill.body);
    check_skill_document(&text).map_err(|e| ImportError::InvalidSkill(e.to_string()))?;
    Ok(text)
}

/// Writes a draft as `.agents/skills/<name>/SKILL.md` in the project. Never
/// replaces a skill that is already there, and refuses a skill folder that is
/// a link. Returns the path written, relative to the project.
pub fn write_skill(workspace: &Path, skill: &SkillDraft) -> Result<String, ImportError> {
    if !skill_name_ok(&skill.name) {
        return Err(ImportError::InvalidSkill("the name is not allowed".into()));
    }
    let text = render_skill(skill)?;
    let mut folder = workspace.to_path_buf();
    for part in SKILL_ROOT_RELATIVE_PATH.split('/') {
        folder.push(part);
        match fs::symlink_metadata(&folder) {
            Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_dir() => {
                return Err(ImportError::SkillFolderUnsafe)
            }
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => fs::create_dir(&folder)?,
            Err(error) => return Err(error.into()),
        }
    }
    let skill_folder = folder.join(&skill.name);
    match fs::create_dir(&skill_folder) {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
            return Err(ImportError::SkillExists(skill.name.clone()))
        }
        Err(error) => return Err(error.into()),
    }
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(skill_folder.join("SKILL.md"))?;
    file.write_all(text.as_bytes())?;
    Ok(format!(
        "{SKILL_ROOT_RELATIVE_PATH}/{}/SKILL.md",
        skill.name
    ))
}

/// Most items one import may tick.
pub const MAX_TICKED: usize = 256;

/// One found item, as the app and the CLI show it.
pub fn item_view(item: &ImportItem) -> ImportItemView {
    ImportItemView {
        id: item.id.clone(),
        kind: match item.kind {
            ImportKind::Instructions { .. } => "instructions",
            ImportKind::McpServer { .. } => "mcp_server",
            ImportKind::Skill { .. } => "skill",
            ImportKind::Command { .. } => "command",
        }
        .into(),
        name: item.name().to_owned(),
        origin: item.origin,
        summary: item.summary(),
        source_path: item.source_path.clone(),
        destination: item.destination(),
        dropped: item.dropped.clone(),
        conflict: item.conflict.clone(),
        importable: item.importable(),
    }
}

/// Imports the ticked items of `scan`, which must be a fresh scan: an id only
/// selects an item, it never carries content. Skills and commands are written
/// into the project; MCP servers are handed to `save_server`, which stores
/// them as drafts that are off. Returns one result per ticked id.
pub fn apply(
    scan: &ImportScan,
    workspace: &Path,
    ids: &[String],
    mut save_server: impl FnMut(&ImportedMcpServer) -> Result<(), String>,
) -> Vec<ImportResultView> {
    let mut results = Vec::new();
    let mut done = BTreeSet::new();
    for id in ids.iter().take(MAX_TICKED) {
        if !done.insert(id.as_str()) {
            continue;
        }
        let result = |imported: bool, detail: String| ImportResultView {
            id: id.clone(),
            imported,
            detail,
        };
        let Some(item) = scan.find(id) else {
            results.push(result(false, "not found any more".into()));
            continue;
        };
        if let Some(conflict) = &item.conflict {
            results.push(result(false, conflict.clone()));
            continue;
        }
        results.push(match &item.kind {
            ImportKind::Instructions { .. } => {
                result(false, "loads by itself, nothing to import".into())
            }
            ImportKind::McpServer { server } => match save_server(server) {
                Ok(()) => result(
                    true,
                    "saved as a draft that is off; it needs your review before it runs".into(),
                ),
                Err(error) => result(false, error),
            },
            ImportKind::Skill { skill } | ImportKind::Command { skill } => {
                match write_skill(workspace, skill) {
                    Ok(path) => result(true, format!("written to {path}")),
                    Err(error) => result(false, error.to_string()),
                }
            }
        });
    }
    results
}

#[cfg(test)]
mod tests;
