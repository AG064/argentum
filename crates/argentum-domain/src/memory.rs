// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 AG064

//! Project memory: short notes kept for one project.
//!
//! Notes live with the workspace data, not in the repository. The agent can
//! add a note; the owner can see, edit and delete every note. Notes reach the
//! model as data and grant nothing.

use serde::{Deserialize, Serialize};

/// Longest note, in characters.
pub const MAX_MEMORY_NOTE_CHARS: usize = 500;
/// Most notes one project keeps.
pub const MAX_MEMORY_NOTES: usize = 100;
/// Most bytes of notes loaded into one request. Newer notes come first.
pub const MAX_MEMORY_CONTEXT_BYTES: usize = 4 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MemoryAuthor {
    Owner,
    Agent,
}

impl MemoryAuthor {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Owner => "owner",
            Self::Agent => "agent",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "owner" => Some(Self::Owner),
            "agent" => Some(Self::Agent),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MemoryNote {
    pub id: i64,
    pub text: String,
    pub author: MemoryAuthor,
    /// Unix seconds.
    pub created_at: i64,
    pub updated_at: i64,
}

/// Trims a note to one paragraph and checks its length. The credential check
/// is the runtime's, which has the shared detector.
pub fn normalize_memory_text(text: &str) -> Result<String, &'static str> {
    let text = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if text.is_empty() {
        return Err("a note cannot be empty");
    }
    if text.chars().count() > MAX_MEMORY_NOTE_CHARS {
        return Err("a note can be at most 500 characters");
    }
    if text.chars().any(char::is_control) {
        return Err("a note cannot contain control characters");
    }
    Ok(text)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn notes_are_one_trimmed_paragraph() {
        assert_eq!(
            normalize_memory_text("  Use  pnpm\n not npm. ").unwrap(),
            "Use pnpm not npm."
        );
        assert!(normalize_memory_text(" \n ").is_err());
        assert!(normalize_memory_text(&"x".repeat(501)).is_err());
        assert!(normalize_memory_text(&"x".repeat(500)).is_ok());
    }
}
