//! Model-facing text protocol v1. Offsets are zero-based UTF-8 bytes, end-exclusive.
use serde::{Deserialize, Serialize};

use crate::Sha256Digest;

pub const MAX_TEXT_SNAPSHOT_BYTES: usize = 1024 * 1024;
pub const MAX_TEXT_RESULT_BYTES: usize = 64 * 1024;
pub const MAX_TARGETED_EDIT_BYTES: usize = 16 * 1024;
pub const MAX_TEXT_LINE_COUNT: u16 = 200;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TextNewlineMode {
    #[default]
    Exact,
    Preserve,
}

impl TextNewlineMode {
    pub fn is_exact(&self) -> bool {
        *self == Self::Exact
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TextLineEndingStyle {
    None,
    Lf,
    #[serde(rename = "crlf")]
    CrLf,
    Mixed,
    BareCr,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TextReadResult {
    pub schema_version: u8,
    pub position_unit: String,
    pub line_ending_style: TextLineEndingStyle,
    pub requested_start_byte: u64,
    pub requested_end_byte: u64,
    pub returned_start_byte: u64,
    pub returned_end_byte: u64,
    pub range_complete: bool,
    pub file_has_more: bool,
    pub is_complete_file: bool,
    pub next_start_byte: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub requested_start_line: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub requested_line_count: Option<u16>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub returned_start_line: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub returned_line_count: Option<u16>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub line_range_complete: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub edit_selection: Option<TextEditSelection>,
    pub file_size_bytes: u64,
    pub sha256: Sha256Digest,
    pub content: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TextEditSelection {
    pub expected_sha256: Sha256Digest,
    pub start_line: u64,
    pub line_count: u16,
}
