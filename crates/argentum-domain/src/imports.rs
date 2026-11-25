// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 AG064

//! Settings imported from other coding agents.
//!
//! An import copies a description of what the owner had set up elsewhere. It
//! never grants a permission, approves a tool, or turns anything on. Secret
//! values, such as environment variables and request headers, are never
//! copied: only their names are kept, so the owner can enter them again.

use serde::{Deserialize, Serialize};

/// Where an imported item came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ImportOrigin {
    ClaudeCodeProject,
    ClaudeCodeUser,
    CodexProject,
    CodexUser,
    OpencodeProject,
    OpencodeUser,
}

impl ImportOrigin {
    pub const fn label(self) -> &'static str {
        match self {
            Self::ClaudeCodeProject => "Claude Code, this project",
            Self::ClaudeCodeUser => "Claude Code, your account",
            Self::CodexProject => "Codex, this project",
            Self::CodexUser => "Codex, your account",
            Self::OpencodeProject => "OpenCode, this project",
            Self::OpencodeUser => "OpenCode, your account",
        }
    }

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::ClaudeCodeProject => "claude_code_project",
            Self::ClaudeCodeUser => "claude_code_user",
            Self::CodexProject => "codex_project",
            Self::CodexUser => "codex_user",
            Self::OpencodeProject => "opencode_project",
            Self::OpencodeUser => "opencode_user",
        }
    }
}

/// How an imported MCP server is reached.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum ImportedMcpTransport {
    /// A local program that speaks MCP on standard input and output.
    Stdio { command: String, args: Vec<String> },
    /// A remote server, over streamable HTTP or server-sent events.
    Http { url: String },
}

/// An MCP server found in another agent's settings, kept as a draft.
///
/// The MCP module reads these drafts. Turning one on is that module's own
/// owner action, with its usual review and approvals.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ImportedMcpServer {
    pub name: String,
    pub origin: ImportOrigin,
    /// The file it was read from, for display only.
    pub source_path: String,
    pub transport: ImportedMcpTransport,
    /// Names of the environment variables it was given. Values are not kept.
    #[serde(default)]
    pub env_names: Vec<String>,
    /// Names of the request headers it was given. Values are not kept.
    #[serde(default)]
    pub header_names: Vec<String>,
    /// Always false on import.
    #[serde(default)]
    pub enabled: bool,
}

pub const MAX_IMPORTED_NAME_BYTES: usize = 64;

/// A name is 1 to 64 lower-case letters, digits, `-` or `_`, starting with a
/// letter or digit.
pub fn valid_imported_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= MAX_IMPORTED_NAME_BYTES
        && name
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-' || b == b'_')
        && name.as_bytes()[0].is_ascii_alphanumeric()
}

impl ImportedMcpServer {
    /// Checks the shape before the draft is stored.
    pub fn validate(&self) -> Result<(), &'static str> {
        if !valid_imported_name(&self.name) {
            return Err("server name is invalid");
        }
        if self.enabled {
            return Err("an imported server starts turned off");
        }
        match &self.transport {
            ImportedMcpTransport::Stdio { command, args } => {
                if command.trim().is_empty() || command.len() > 4096 {
                    return Err("server command is invalid");
                }
                if args.len() > 256 || args.iter().any(|arg| arg.len() > 4096) {
                    return Err("server arguments are too long");
                }
            }
            ImportedMcpTransport::Http { url } => {
                if !(url.starts_with("https://") || url.starts_with("http://")) || url.len() > 4096
                {
                    return Err("server address must be http or https");
                }
            }
        }
        if self.env_names.len() > 128 || self.header_names.len() > 128 {
            return Err("too many variable names");
        }
        Ok(())
    }
}

/// One found item, as the app and the CLI show it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ImportItemView {
    pub id: String,
    /// `instructions`, `mcp_server`, `skill` or `command`.
    pub kind: String,
    pub name: String,
    pub origin: ImportOrigin,
    pub summary: String,
    pub source_path: String,
    /// For skills and commands, the project file an import would create.
    pub destination: Option<String>,
    pub dropped: Vec<String>,
    pub conflict: Option<String>,
    pub importable: bool,
}

/// What happened to one ticked item.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ImportResultView {
    pub id: String,
    pub imported: bool,
    pub detail: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_are_checked() {
        assert!(valid_imported_name("github"));
        assert!(valid_imported_name("my_server-2"));
        assert!(!valid_imported_name("-x"));
        assert!(!valid_imported_name("Upper"));
        assert!(!valid_imported_name("../x"));
    }

    #[test]
    fn an_enabled_draft_is_refused() {
        let server = ImportedMcpServer {
            name: "x".into(),
            origin: ImportOrigin::CodexUser,
            source_path: "config.toml".into(),
            transport: ImportedMcpTransport::Http {
                url: "https://example.test/mcp".into(),
            },
            env_names: vec![],
            header_names: vec![],
            enabled: true,
        };
        assert!(server.validate().is_err());
    }
}
