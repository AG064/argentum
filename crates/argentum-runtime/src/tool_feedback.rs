//! Fixed, non-authorizing feedback for failures proven to precede execution.
use argentum_tools::ToolError;
use argentum_workspaces::WorkspaceError;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum PreflightCode {
    InvalidArguments,
    MalformedJson,
    UnknownField,
    MissingField,
    WrongType,
    MixedSelectors,
    InvalidDigest,
    NoUniqueMatch,
    ExactMatchMissing,
    ExactMatchAmbiguous,
    LineEndingMismatch,
    PreserveModeUnsupported,
    InvalidSelector,
    InvalidQuery,
    CapabilityDisabled,
    ApprovalRequired,
    InvalidPath,
    NotFile,
    NotDirectory,
    StaleHash,
    NotFound,
    AccessDenied,
    InvalidText,
    LimitExceeded,
    DuplicatePath,
    NoChange,
}

impl PreflightCode {
    pub fn detail(self) -> &'static str {
        match self {
            Self::InvalidArguments => "Arguments do not match this tool's schema. Use its declared argument names and types. No operation was executed.",
            Self::MalformedJson => "The tool arguments were not valid JSON. Send one complete JSON object matching the declared schema. No operation was executed.",
            Self::UnknownField => "The tool arguments contain an unsupported property. Use only the properties declared for this tool. No operation was executed.",
            Self::MissingField => "A required tool property is missing. Send the complete declared property set for this operation. No operation was executed.",
            Self::WrongType => "A tool property has the wrong JSON type. Use the declared type and do not send null for required values. No operation was executed.",
            Self::MixedSelectors => "read_text must use byte selectors or the start_line/line_count pair, not both. No operation was executed.",
            Self::InvalidDigest => "Use the exact lowercase SHA-256 returned by read_text for the current file. Do not invent or omit the digest. No operation was executed.",
            Self::NoUniqueMatch => "An edit requires one unique exact old_text match in the digest-bound file. Re-read the file and submit one unambiguous edit. No part of the patch was applied.",
            Self::ExactMatchMissing => "The file digest matched, but old_text has zero exact matches. Copy old_text exactly from read_text, including whitespace and CRLF/LF, or use a shorter unique fragment. Do not reconstruct the source. No part of the patch was applied.",
            Self::ExactMatchAmbiguous => "The file digest matched, but old_text occurs more than once. Include surrounding unchanged text to make the match unique, keeping each text within 16 KiB. Do not shorten an ambiguous match. No part of the patch was applied.",
            Self::LineEndingMismatch => "The file digest matched and old_text matches only after CRLF/LF comparison. The exact-mode edit was not changed or applied. Re-read the file and either copy its exact line endings or submit a fresh explicit newline_mode=preserve edit.",
            Self::PreserveModeUnsupported => "newline_mode=preserve requires a uniformly LF or CRLF source. Mixed line endings, bare CR, and newline-introducing edits to a file with no newline are rejected without mutation; exact mode remains available.",
            Self::InvalidSelector => "The requested read selector is incomplete or out of bounds. Use either a valid byte range or both one-based start_line and line_count. No operation was executed.",
            Self::InvalidQuery => "The search query must be nonempty and within its declared byte limit. No operation was executed.",
            Self::CapabilityDisabled => "This tool is not enabled by the selected execution policy and was not advertised for this request. No operation was executed.",
            Self::ApprovalRequired => "This verification selection requires owner approval, which is unavailable in this headless attempt. No verification process was started. The native goal workflow can schedule its already authorized declared checks after a work-report handoff; do not retry this rejected selection. Additional runner authority requires explicit owner approval.",
            Self::InvalidPath => "Invalid path. For a workspace file use path=hello_world.txt or /workspace/hello_world.txt. Do not use /home, a drive-relative path, parent traversal, or control characters. No operation was executed.",
            Self::NotFile => "The target is not a regular file. Use a filename, not a directory or '.'. Inspect with file_metadata or list_directory; do not delete or replace a directory automatically. No part of the patch was applied.",
            Self::NotDirectory => "The target is not a directory. Inspect with file_metadata and choose a valid directory. No operation was executed.",
            Self::StaleHash => "The expected file hash does not match. Re-read the file with read_text and use its returned SHA-256 before submitting a new edit. Do not invent a hash. No part of the patch was applied.",
            Self::NotFound => "The target or its parent was not found during preparation. Check the path with list_directory. No part of the patch was applied.",
            Self::AccessDenied => "The target could not be inspected because access was denied. Full Access does not override operating-system permissions. No part of the patch was applied.",
            Self::InvalidText => "The text-edit request is invalid. Re-read the file and use a valid bounded edit. No part of the patch was applied.",
            Self::LimitExceeded => "The operation exceeds its file or byte limit. Use smaller bounded operations. No part of the patch was applied.",
            Self::DuplicatePath => "The patch contains the same target more than once. Combine those edits into one entry per file. No part of the patch was applied.",
            Self::NoChange => "This path already matches the requested state. No part of the patch was applied. Omit unchanged entries and submit any remaining required changes; otherwise continue with the task.",
        }
    }

    pub fn content(self, path: Option<&str>) -> String {
        self.content_with_issues(path, &[])
    }

    pub fn content_with_issues(self, path: Option<&str>, issues: &[FeedbackIssue]) -> String {
        let mut value = serde_json::json!({
            "outcome": if self == Self::NoChange { "no_change" } else { "preflight_rejected" },
            "executed": false, "code": self, "detail": self.detail(),
            "success": self == Self::NoChange,
        });
        if self != Self::NoChange {
            value["error"] = self.detail().into();
        }
        if let Some(path) = path {
            value["path"] = safe_path(path).into();
        }
        let bounded_issues = issues
            .iter()
            .take(4)
            .map(FeedbackIssue::value)
            .collect::<Vec<_>>();
        if !bounded_issues.is_empty() {
            value["issues"] = bounded_issues.into();
        }
        let encoded = value.to_string();
        if encoded.len() <= 2 * 1024 {
            encoded
        } else {
            // Keep typed correction metadata before optional long paths, and
            // never recurse with the same oversized multibyte path.
            value.as_object_mut().unwrap().remove("path");
            let without_path = value.to_string();
            if without_path.len() <= 2 * 1024 {
                without_path
            } else {
                self.content(None)
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct FeedbackIssue {
    pub category: &'static str,
    pub field: Option<&'static str>,
    pub expected: Option<&'static str>,
    pub actual: Option<&'static str>,
    pub decoded_bytes: Option<usize>,
    pub limit_bytes: Option<usize>,
    pub correction: &'static str,
}

impl FeedbackIssue {
    fn value(&self) -> serde_json::Value {
        let mut value = serde_json::json!({
            "category": self.category,
            "correction": self.correction,
        });
        if let Some(field) = self.field {
            value["field"] = field.into();
        }
        if let Some(expected) = self.expected {
            value["expected"] = expected.into();
        }
        if let Some(actual) = self.actual {
            value["actual"] = actual.into();
        }
        if let Some(decoded_bytes) = self.decoded_bytes {
            value["decoded_bytes"] = decoded_bytes.into();
        }
        if let Some(limit_bytes) = self.limit_bytes {
            value["limit_bytes"] = limit_bytes.into();
        }
        value
    }
}

pub(super) fn safe_path(path: &str) -> String {
    let lower = path.to_ascii_lowercase();
    if [
        "secret", "password", "api_key", "api-key", "bearer", "sk-", "sk_", "token=",
    ]
    .iter()
    .any(|s| lower.contains(s))
    {
        return "[sensitive path withheld]".into();
    }
    let mut value: String = path.chars().take(512).collect();
    if value.len() < path.len() {
        value.push_str(" [abbreviated]");
    }
    value
}

pub(super) fn rejected_code(content: &str) -> Option<PreflightCode> {
    let canonical = retained_feedback(content).or_else(|| retained_read_failure(content))?;
    let value: serde_json::Value = serde_json::from_str(&canonical).ok()?;
    let code: PreflightCode = serde_json::from_value(value["code"].clone()).ok()?;
    (code != PreflightCode::NoChange).then_some(code)
}

pub(super) fn rejection_signature(
    call: &argentum_providers::ModelToolCall,
    _content: &str,
    code: PreflightCode,
) -> (String, PreflightCode, Option<String>) {
    use sha2::{Digest, Sha256};
    // Displayed diagnostics may omit, abbreviate or redact the target. They
    // cannot identify the actual request. This private fingerprint grants no
    // authority and is never sent to a provider or persisted in feedback.
    let arguments = serde_json::from_str::<serde_json::Value>(&call.arguments).ok();
    let target = arguments.as_ref().and_then(|value| {
        ["path", "job_id"]
            .into_iter()
            .find_map(|field| value.get(field)?.as_str().map(|text| (field, text)))
    });
    let mut digest = Sha256::new();
    digest.update(b"argentum-private-rejection-target-v1\0");
    if let Some((field, target)) = target {
        digest.update(field.as_bytes());
        digest.update(b"\0");
        digest.update(target.as_bytes());
    } else {
        // Without an identifiable target, only identical inputs are grouped.
        // The existing hard model-round and request bounds still apply.
        digest.update(b"unidentifiable-input\0");
        digest.update(call.arguments.as_bytes());
    }
    (
        call.name.clone(),
        code,
        Some(format!("{:x}", digest.finalize())),
    )
}

fn read_detail(code: PreflightCode) -> Option<&'static str> {
    Some(match code {
    PreflightCode::StaleHash=>"The read guard refers to an older file revision. No current source bytes were returned. For a fresh observation omit expected_sha256; nonzero byte continuations first need the digest from that fresh read. This read changed no file.",
    PreflightCode::InvalidSelector=>"The admitted read failed because its selector is invalid. First obtain a bounded fresh observation, then select valid current lines or UTF-8 byte boundaries. Nonzero byte reads require the digest from that observation. This read changed no file.",
    PreflightCode::MixedSelectors=>"The admitted read mixed byte and line selectors. Choose one supported selector form after a fresh observation. This read changed no file.",
    PreflightCode::InvalidDigest=>"The admitted read supplied an invalid digest. Omit the guard for a fresh observation, or use its exact returned SHA-256 for a same-revision continuation. This read changed no file.",
    PreflightCode::NotFound=>"The requested file or directory was not found. Use an enabled bounded filename or directory discovery tool, then use the exact returned path. Do not guess aliases or widen scope. No file was changed.",
    PreflightCode::NotFile=>"The target is not a regular file. Choose a file, not a directory; no file was changed.",
    PreflightCode::NotDirectory=>"The target is not a directory. Choose a directory; no file was changed.",
    PreflightCode::AccessDenied=>"The read was denied by the operating system. No file was changed.",
    PreflightCode::LimitExceeded=>"The read exceeded its file or byte limit. Request a smaller bounded read; no file was changed.",
    PreflightCode::InvalidText=>"The text read is invalid or the file is not supported text. Use file metadata or a valid range; no file was changed.",
    _=>return None,
})
}
pub(super) fn read_failure(error: &ToolError, path: Option<&str>) -> Option<String> {
    let input = invalid_input_feedback(error, None)
        .and_then(|body| serde_json::from_str::<serde_json::Value>(&body).ok());
    let code = if let Some(input) = input.as_ref() {
        serde_json::from_value(input["code"].clone()).ok()?
    } else {
        preparation_failure(error)?.0
    };
    let detail = read_detail(code)?;
    let mut value = serde_json::json!({"success":false,"executed":true,"outcome":"tool_failed","code":code,"detail":detail,"error":detail});
    if let Some(issues) = input.as_ref().and_then(|input| input.get("issues")) {
        value["issues"] = issues.clone();
    }
    if let Some(path) = path {
        value["path"] = safe_path(path).into();
    }
    Some(value.to_string())
}
pub(super) fn retained_read_failure(content: &str) -> Option<String> {
    if content.len() > 8192 {
        return None;
    }
    let mut v: serde_json::Value = serde_json::from_str(content).ok()?;
    let code: PreflightCode = serde_json::from_value(v["code"].clone()).ok()?;
    let detail = read_detail(code)?;
    let legacy_not_found = "The requested file or directory was not found. Inspect list_directory and correct the path; no file was changed.";
    let known_detail = (v["detail"] == detail && v["error"] == detail)
        || (code == PreflightCode::NotFound
            && v["detail"] == legacy_not_found
            && v["error"] == legacy_not_found);
    if v["success"] != false
        || v["executed"] != true
        || v["outcome"] != "tool_failed"
        || !known_detail
        || v.as_object()?.keys().any(|k| {
            !matches!(
                k.as_str(),
                "success"
                    | "executed"
                    | "outcome"
                    | "code"
                    | "detail"
                    | "error"
                    | "path"
                    | "issues"
                    | "recovery_read"
                    | "recovery_notice"
            )
        })
    {
        return None;
    }
    v.as_object_mut()?.remove("path");
    v.as_object_mut()?.remove("issues");
    v.as_object_mut()?.remove("recovery_read");
    v.as_object_mut()?.remove("recovery_notice");
    Some(v.to_string())
}

/// A proposal for a separate observation, never a replacement write or a read.
/// The runtime checks disclosure/read enablement before calling this helper.
pub(super) fn with_observation_recovery(
    content: String,
    tool: &str,
    arguments: &serde_json::Value,
    path: &str,
) -> String {
    let Ok(mut feedback) = serde_json::from_str::<serde_json::Value>(&content) else {
        return content;
    };
    let Ok(code) = serde_json::from_value::<PreflightCode>(feedback["code"].clone()) else {
        return content;
    };
    let known_preflight = feedback["executed"] == false
        && feedback["outcome"] == "preflight_rejected"
        && retained_feedback(&content).is_some();
    let known_failed_read = tool == "read_text"
        && feedback["executed"] == true
        && feedback["outcome"] == "tool_failed"
        && retained_read_failure(&content).is_some();
    if !known_preflight && !known_failed_read {
        return content;
    }
    if !matches!(tool, "read_text" | "edit_text")
        || !matches!(
            code,
            PreflightCode::StaleHash
                | PreflightCode::InvalidSelector
                | PreflightCode::MixedSelectors
                | PreflightCode::InvalidDigest
                | PreflightCode::InvalidArguments
                | PreflightCode::MissingField
                | PreflightCode::WrongType
                | PreflightCode::UnknownField
                | PreflightCode::ExactMatchMissing
                | PreflightCode::ExactMatchAmbiguous
                | PreflightCode::LineEndingMismatch
                | PreflightCode::PreserveModeUnsupported
        )
        || path.len() > 256
        || safe_path(path) != path
    {
        return content;
    }
    let mut next = serde_json::json!({"path": path});
    let has_bytes = ["start_byte", "end_byte"]
        .iter()
        .any(|field| arguments.get(field).is_some_and(|value| !value.is_null()));
    // An old byte offset cannot be read without its guard. Begin a new bounded
    // observation instead of carrying that guard or guessing a current one.
    let invalid_coordinates = code == PreflightCode::InvalidSelector
        && !feedback
            .get("issues")
            .and_then(serde_json::Value::as_array)
            .is_some_and(|issues| {
                issues
                    .iter()
                    .any(|issue| issue["category"] == "selector_pair")
            });
    let old_line = (!has_bytes && code != PreflightCode::MixedSelectors && !invalid_coordinates)
        .then(|| {
            arguments
                .get("start_line")
                .and_then(serde_json::Value::as_u64)
        })
        .flatten()
        .filter(|line| {
            *line > 0 && *line <= argentum_domain::text_contract::MAX_TEXT_SNAPSHOT_BYTES as u64 + 1
        });
    if let Some(line) = old_line {
        next["start_line"] = line.into();
        let count = arguments
            .get("line_count")
            .and_then(serde_json::Value::as_u64)
            .filter(|count| {
                *count > 0
                    && *count <= u64::from(argentum_domain::text_contract::MAX_TEXT_LINE_COUNT)
            })
            .unwrap_or(16)
            .min(16);
        next["line_count"] = count.into();
    }
    feedback["recovery_read"] = serde_json::json!({
        "tool": "read_text", "arguments": next,
        "selection_role": "observation_window_only",
        "coordinate_basis": if old_line.is_some() {"previous_coordinates_unverified"} else {"fresh_orientation"}
    });
    feedback["recovery_notice"] = "Submit this as a separate read, not an edit. It omits the old digest. Old coordinates are only context to inspect; locate moved targets with a new bounded search. Select the intended replacement span from fresh observation, not the entire read window.".into();
    let encoded = feedback.to_string();
    if encoded.len() <= 2 * 1024 {
        encoded
    } else {
        content
    }
}

pub(super) fn preparation_failure(error: &ToolError) -> Option<(PreflightCode, Option<String>)> {
    use PreflightCode as C;
    let ToolError::Workspace(error) = error else {
        return None;
    };
    Some(match error {
        WorkspaceError::EmptyMutation(path) => (C::NoChange, Some(path.clone())),
        WorkspaceError::ExpectedDigestMismatch(path) => (C::StaleHash, Some(path.clone())),
        WorkspaceError::NotFile(path) => (C::NotFile, Some(path.to_string_lossy().into_owned())),
        WorkspaceError::NotDirectory(path) => {
            (C::NotDirectory, Some(path.to_string_lossy().into_owned()))
        }
        WorkspaceError::DuplicateMutationPath(path) => (C::DuplicatePath, Some(path.clone())),
        WorkspaceError::InvalidText("edit text was not found") => (C::ExactMatchMissing, None),
        WorkspaceError::InvalidText("edit text has multiple matches") => {
            (C::ExactMatchAmbiguous, None)
        }
        WorkspaceError::InvalidText("edit text line endings do not match") => {
            (C::LineEndingMismatch, None)
        }
        WorkspaceError::InvalidText(
            "preserve mode requires uniform LF or CRLF source"
            | "preserve mode does not support source text containing bare CR"
            | "preserve mode arguments must not contain bare CR"
            | "preserve mode text must not contain bare CR"
            | "preserve mode cannot infer line endings for a source without newlines",
        ) => (C::PreserveModeUnsupported, None),
        WorkspaceError::InvalidText("materialized new_text exceeds 16 KiB") => {
            (C::LimitExceeded, None)
        }
        WorkspaceError::InvalidText(_) => (C::InvalidText, None),
        WorkspaceError::EmptyPatch => (C::InvalidArguments, None),
        WorkspaceError::PatchLimitExceeded
        | WorkspaceError::FileTooLarge { .. }
        | WorkspaceError::LimitExceeded(_) => (C::LimitExceeded, None),
        WorkspaceError::Io { path, source }
        | WorkspaceError::Security(argentum_security::SecurityError::WorkspaceOperation {
            path,
            source,
        }) => {
            let code = match source.kind() {
                std::io::ErrorKind::NotFound => C::NotFound,
                std::io::ErrorKind::PermissionDenied => C::AccessDenied,
                _ => return None,
            };
            (code, Some(path.to_string_lossy().into_owned()))
        }
        // Never classify approval, policy, stale post-preparation writes,
        // rollback failures or uncertain effects as a harmless rejection.
        _ => return None,
    })
}

pub(super) fn preparation_feedback(
    error: &ToolError,
) -> Option<(PreflightCode, Option<String>, Option<FeedbackIssue>)> {
    let (code, path) = preparation_failure(error)?;
    let issue = match error {
        ToolError::Workspace(WorkspaceError::InvalidText(reason)) => {
            if *reason == "edit text line endings do not match" {
                Some(FeedbackIssue {
                    category: "newline_mode",
                    field: Some("newline_mode"),
                    expected: Some("exact_source_bytes_or_explicit_preserve"),
                    actual: Some("exact_newline_mismatch"),
                    decoded_bytes: None,
                    limit_bytes: None,
                    correction: "Submit a fresh edit using newline_mode=preserve for a multiline edit, or copy the exact physical line endings from read_text. The rejected call was not rewritten or applied.",
                })
            } else if reason.starts_with("preserve mode") {
                Some(FeedbackIssue {
                    category: "newline_style",
                    field: Some("newline_mode"),
                    expected: Some("uniform_lf_or_crlf_source"),
                    actual: Some("unsupported_source_or_argument_style"),
                    decoded_bytes: None,
                    limit_bytes: None,
                    correction: "Use exact mode with literal bytes, or first make a separate authorized exact edit that establishes an explicit line-ending convention.",
                })
            } else if reason.starts_with("line edit")
                || reason.starts_with("replacement line")
                || reason.starts_with("selected source lines")
                || reason.starts_with("materialized replacement")
            {
                Some(FeedbackIssue {
                    category: "line_edit_selection",
                    field: None,
                    expected: Some("complete_existing_uniform_line_span_and_bounded_bodies"),
                    actual: Some("unsupported_or_out_of_bounds"),
                    decoded_bytes: None,
                    limit_bytes: Some(argentum_domain::text_contract::MAX_TARGETED_EDIT_BYTES),
                    correction: "Re-read a complete eligible line span and use its edit_selection unchanged. Send 1 to 200 logical replacement line bodies without physical CR/LF; keep selected and materialized text within 16 KiB. Exact mode remains available.",
                })
            } else if reason.contains("multiple matches") || reason.contains("not found") {
                Some(FeedbackIssue {
                    category: "unique_exact_match",
                    field: Some("old_text"),
                    expected: Some("exactly_one_match"),
                    actual: Some(if reason.contains("multiple matches") {
                        "multiple_matches"
                    } else {
                        "zero_matches"
                    }),
                    decoded_bytes: None,
                    limit_bytes: None,
                    correction: if reason.contains("multiple matches") {
                        "Include surrounding unchanged text to select exactly one occurrence within the 16 KiB bound."
                    } else {
                        "Re-read the intended span and copy a shorter unique old_text exactly, including whitespace. For uniformly LF/CRLF source and a multiline payload with a different separator convention, submit an explicit newline_mode=preserve edit. The host never infers that mode or changes source escape backslashes."
                    },
                })
            } else if *reason == "old_text must be nonempty" {
                Some(FeedbackIssue {
                    category: "nonempty_text", field: Some("old_text"), expected: Some("nonempty"), actual: Some("empty"),
                    decoded_bytes: Some(0), limit_bytes: None,
                    correction: "Copy a nonempty unique old_text from read_text; edit_text does not insert without a match.",
                })
            } else if *reason == "new_text must differ from old_text" {
                Some(FeedbackIssue {
                    category: "changed_text", field: Some("new_text"), expected: Some("different_from_old_text"), actual: Some("identical"),
                    decoded_bytes: None, limit_bytes: None,
                    correction: "Provide the intended replacement in new_text, or omit the unchanged edit. The supplied digest was not the failing check.",
                })
            } else if *reason == "edit text must not contain NUL" {
                Some(FeedbackIssue {
                    category: "text_encoding",
                    field: None,
                    expected: Some("utf8_without_nul"),
                    actual: Some("contains_nul"),
                    decoded_bytes: None,
                    limit_bytes: None,
                    correction:
                        "Use decoded UTF-8 text without NUL characters in old_text and new_text.",
                })
            } else if reason.contains("SHA-256") || reason.contains("digest") {
                Some(FeedbackIssue {
                    category: "sha256",
                    field: Some("expected_sha256"),
                    expected: Some("64_lowercase_hex"),
                    actual: Some("invalid_or_missing"),
                    decoded_bytes: None,
                    limit_bytes: None,
                    correction:
                        "Use the exact digest returned by read_text for the same file revision.",
                })
            } else if reason.contains("16 KiB") {
                Some(FeedbackIssue {
                    category: "utf8_byte_limit",
                    field: None,
                    expected: Some("at_most_16k_utf8_bytes"),
                    actual: Some("oversized"),
                    decoded_bytes: None,
                    limit_bytes: Some(argentum_domain::text_contract::MAX_TARGETED_EDIT_BYTES),
                    correction: "Split the edit into smaller digest-bound operations; the limit is measured in UTF-8 bytes.",
                })
            } else {
                None
            }
        }
        _ => None,
    };
    Some((code, path, issue))
}

pub(super) fn invalid_input_feedback(error: &ToolError, path: Option<&str>) -> Option<String> {
    let ToolError::InvalidInput(reason) = error else {
        return None;
    };
    let (code, issue) = if reason.contains("either byte selectors") {
        (
            PreflightCode::MixedSelectors,
            Some(FeedbackIssue {
                category: "selector_mode",
                field: None,
                expected: Some("byte_or_line"),
                actual: Some("mixed"),
                decoded_bytes: None,
                limit_bytes: None,
                correction:
                    "Remove either both line fields or all byte selector fields, then retry.",
            }),
        )
    } else if reason.starts_with("edit_text requires either") {
        (
            PreflightCode::InvalidArguments,
            Some(FeedbackIssue {
                category: "edit_form",
                field: None,
                expected: Some("one_complete_edit_form"),
                actual: Some("incomplete_or_mixed"),
                decoded_bytes: None,
                limit_bytes: None,
                correction: "Use only old_text/new_text, or only start_line/line_count/replacement_lines from a complete read_text edit_selection.",
            }),
        )
    } else if reason.contains("line addressing requires") {
        (
            PreflightCode::InvalidSelector,
            Some(FeedbackIssue {
                category: "selector_pair",
                field: None,
                expected: Some("start_line_and_line_count"),
                actual: Some("incomplete"),
                decoded_bytes: None,
                limit_bytes: None,
                correction: "Provide both one-based start_line and line_count, or use byte selectors instead.",
            }),
        )
    } else if reason.contains("expected_sha256 must be") {
        (
            PreflightCode::InvalidDigest,
            Some(FeedbackIssue {
                category: "sha256",
                field: Some("expected_sha256"),
                expected: Some("64_lowercase_hex"),
                actual: Some("invalid"),
                decoded_bytes: None,
                limit_bytes: None,
                correction:
                    "Use the exact digest returned by read_text for the same file revision.",
            }),
        )
    } else if reason.contains("nonzero start_byte") {
        (
            PreflightCode::InvalidSelector,
            Some(FeedbackIssue {
                category: "byte_read_guard",
                field: Some("expected_sha256"),
                expected: Some("current_observed_digest_for_nonzero_byte_read"),
                actual: Some("missing"),
                decoded_bytes: None,
                limit_bytes: None,
                correction: "First read the path from byte zero without an expected digest, then submit the nonzero byte range with the digest returned by that fresh observation.",
            }),
        )
    } else if reason.contains("byte range") {
        (
            PreflightCode::InvalidSelector,
            Some(FeedbackIssue {
                category: "byte_selector",
                field: None,
                expected: Some("valid_utf8_byte_range"),
                actual: Some("invalid"),
                decoded_bytes: None,
                limit_bytes: None,
                correction: "Use zero-based UTF-8 byte offsets on character boundaries and include the current digest for continuation reads.",
            }),
        )
    } else if reason.contains("start_line") || reason.contains("line_count") {
        (
            PreflightCode::InvalidSelector,
            Some(FeedbackIssue {
                category: "line_selector",
                field: None,
                expected: Some("one_based_start_line_and_1_to_200_line_count"),
                actual: Some("invalid"),
                decoded_bytes: None,
                limit_bytes: None,
                correction: "Use a one-based start_line and a line_count between 1 and 200.",
            }),
        )
    } else if reason.contains("text search bounds") {
        (
            PreflightCode::InvalidQuery,
            Some(FeedbackIssue {
                category: "search_bounds",
                field: None,
                expected: Some("valid_query_and_bounded_selectors"),
                actual: Some("out_of_bounds"),
                decoded_bytes: None,
                limit_bytes: None,
                correction:
                    "Use a nonempty bounded query and max_results/context values from the schema.",
            }),
        )
    } else {
        return None;
    };
    Some(code.content_with_issues(path, issue.into_iter().collect::<Vec<_>>().as_slice()))
}

pub(super) fn retained_feedback(content: &str) -> Option<String> {
    if content.len() > 8192 {
        return None;
    }
    let value: serde_json::Value = serde_json::from_str(content).ok()?;
    let object = value.as_object()?;
    if object.keys().any(|key| {
        !matches!(
            key.as_str(),
            "outcome"
                | "executed"
                | "code"
                | "detail"
                | "path"
                | "success"
                | "error"
                | "issues"
                | "recovery_read"
                | "recovery_notice"
                | "correction_category"
                | "correction"
        )
    }) {
        return None;
    }
    let code: PreflightCode = serde_json::from_value(value.get("code")?.clone()).ok()?;
    let legacy_path_detail="Invalid path for this host or workspace scope. For workspace work use a relative path such as hello_world.txt. Do not invent /workspace or /home aliases. Absolute paths must be fully qualified and allowed by the current scope. No operation was executed.";
    if value["executed"] != false
        || (value["detail"] != code.detail()
            && !(code == PreflightCode::InvalidPath && value["detail"] == legacy_path_detail))
        || value
            .get("success")
            .is_some_and(|v| *v != (code == PreflightCode::NoChange))
        || value.get("error").is_some_and(|v| *v != code.detail())
        || value["outcome"]
            != if code == PreflightCode::NoChange {
                "no_change"
            } else {
                "preflight_rejected"
            }
    {
        return None;
    }
    // Preserve a closed correction category, not arbitrary issue text, values,
    // arguments, paths, OS errors, provider bodies or authority. Reconstruct the
    // message from host constants so this projection is private and idempotent.
    let correction = value
        .get("correction_category")
        .and_then(serde_json::Value::as_str)
        .and_then(|category| retained_correction(category, code))
        .or_else(|| {
            value
                .get("issues")
                .and_then(serde_json::Value::as_array)
                .and_then(|issues| {
                    issues
                        .iter()
                        .take(4)
                        .find_map(|issue| correction_from_issue(issue, code))
                })
        });
    let mut canonical: serde_json::Value = serde_json::from_str(&code.content(None)).ok()?;
    if let Some((category, message)) = correction {
        canonical["correction_category"] = category.into();
        canonical["correction"] = message.into();
    }
    Some(canonical.to_string())
}

fn retained_correction(
    category: &str,
    code: PreflightCode,
) -> Option<(&'static str, &'static str)> {
    Some(match (category, code) {
        ("mixed_edit_forms", PreflightCode::InvalidArguments) => ("mixed_edit_forms", "Resubmit ONE existing edit form with the current read's path and sha256: old_text + new_text (optional newline_mode), OR start_line + line_count + replacement_lines (no string-edit fields). No edit was applied."),
        ("missing_edit_form", PreflightCode::InvalidArguments) => ("missing_edit_form", "Include ONE complete existing edit form: old_text + new_text, OR start_line + line_count + replacement_lines, in addition to the current path and expected_sha256. No edit was applied."),
        ("search_mode", PreflightCode::InvalidArguments) => ("search_mode", "Use the exact mode literal or regex for search_text; omit mode for the literal default. Filename patterns belong to search_filenames. No search was executed."),
        ("search_result_bound", PreflightCode::InvalidArguments) => ("search_result_bound", "Send max_results as an integer from 1 through 1000. No search was executed."),
        ("search_context_bound", PreflightCode::InvalidArguments) => ("search_context_bound", "Send optional search_text before_context/after_context as integers from 0 through 8, or omit them for zero. No search was executed."),
        _ => return None,
    })
}

fn correction_from_issue(
    issue: &serde_json::Value,
    code: PreflightCode,
) -> Option<(&'static str, &'static str)> {
    let category = match (
        issue.get("category")?.as_str()?,
        issue.get("field").and_then(serde_json::Value::as_str),
        issue.get("actual")?.as_str()?,
    ) {
        ("edit_form", None, "mixed") => "mixed_edit_forms",
        ("edit_form", None, "missing") => "missing_edit_form",
        ("enum_value", Some("mode"), "unsupported") => "search_mode",
        ("bounded_property", Some("max_results"), "out_of_range") => "search_result_bound",
        ("bounded_property", Some("before_context" | "after_context"), "out_of_range") => {
            "search_context_bound"
        }
        _ => return None,
    };
    retained_correction(category, code)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recovery_read_is_bounded_observation_without_obsolete_mutation_fields() {
        for arguments in [
            serde_json::json!({"path":"file.rs","expected_sha256":"a".repeat(64),"start_byte":40,"end_byte":80}),
            serde_json::json!({"path":"file.rs","expected_sha256":"a".repeat(64),"start_line":4,"line_count":200,"replacement_lines":["not copied"]}),
        ] {
            let body = with_observation_recovery(
                PreflightCode::StaleHash.content(Some("file.rs")),
                "edit_text",
                &arguments,
                "file.rs",
            );
            assert!(body.len() <= 2048);
            let value: serde_json::Value = serde_json::from_str(&body).unwrap();
            let next = &value["recovery_read"]["arguments"];
            assert_eq!(next["path"], "file.rs");
            for field in [
                "expected_sha256",
                "old_text",
                "new_text",
                "replacement_lines",
                "start_byte",
                "end_byte",
            ] {
                assert!(next.get(field).is_none());
            }
            if arguments.get("start_line").is_some() {
                assert_eq!(next["line_count"], 16);
            } else {
                assert_eq!(next, &serde_json::json!({"path":"file.rs"}));
            }
            assert_eq!(
                value["recovery_read"]["selection_role"],
                "observation_window_only"
            );
            let retained = retained_feedback(&body).unwrap();
            assert!(!retained.contains("file.rs"));
            assert!(!retained.contains("recovery_read"));
        }
    }

    #[test]
    fn recovery_hints_do_not_reclassify_denials_absence_or_unknown_effects() {
        let arguments = serde_json::json!({"path":"file.rs","expected_sha256":"a".repeat(64)});
        for code in [
            PreflightCode::CapabilityDisabled,
            PreflightCode::InvalidPath,
            PreflightCode::AccessDenied,
            PreflightCode::NotFound,
            PreflightCode::NotFile,
            PreflightCode::LimitExceeded,
            PreflightCode::NoChange,
        ] {
            let original = code.content(Some("file.rs"));
            assert_eq!(
                with_observation_recovery(original.clone(), "edit_text", &arguments, "file.rs"),
                original
            );
        }
        let unknown =
            serde_json::json!({"executed":true,"code":"execution_failed","outcome":"tool_failed"})
                .to_string();
        assert_eq!(
            with_observation_recovery(unknown.clone(), "edit_text", &arguments, "file.rs"),
            unknown
        );
        let uncertain=serde_json::json!({"executed":true,"success":false,"code":"stale_hash","outcome":"effect_unknown"}).to_string();
        assert_eq!(
            with_observation_recovery(uncertain.clone(), "edit_text", &arguments, "file.rs"),
            uncertain
        );
        let sensitive = with_observation_recovery(
            PreflightCode::StaleHash.content(Some("secret.txt")),
            "edit_text",
            &arguments,
            "secret.txt",
        );
        assert!(!sensitive.contains("recovery_read"));
    }

    #[test]
    fn rejection_signature_distinguishes_omitted_and_redacted_targets() {
        for redacted in [false, true] {
            let paths = if redacted {
                ["secret-one.rs", "secret-two.rs"]
            } else {
                ["one.rs", "two.rs"]
            };
            let signatures: Vec<_> = paths
                .iter()
                .map(|path| {
                    let call = argentum_providers::ModelToolCall {
                        id: "synthetic-call".into(),
                        name: "edit_text".into(),
                        arguments: serde_json::json!({"path": path}).to_string(),
                    };
                    let content =
                        PreflightCode::InvalidArguments.content(redacted.then_some(*path));
                    rejection_signature(&call, &content, PreflightCode::InvalidArguments)
                })
                .collect();
            assert_ne!(signatures[0], signatures[1], "different input targets");
        }
    }

    #[test]
    fn rejection_signature_keeps_the_identical_failure_guard() {
        let call = argentum_providers::ModelToolCall {
            id: "synthetic-one".into(),
            name: "edit_text".into(),
            arguments: serde_json::json!({"path": "same.rs", "new_text": "one"}).to_string(),
        };
        let mut next = call.clone();
        next.id = "synthetic-two".into();
        next.arguments = serde_json::json!({"path": "same.rs", "new_text": "two"}).to_string();
        let content = PreflightCode::InvalidText.content(Some("same.rs"));
        assert_eq!(
            rejection_signature(&call, &content, PreflightCode::InvalidText),
            rejection_signature(&next, &content, PreflightCode::InvalidText)
        );
    }

    #[test]
    fn rejection_signature_unknown_inputs_and_job_targets_are_private_and_distinct() {
        for arguments in [
            ["{bad-one", "{bad-two"],
            [
                r#"{"job_id":"one","stdout_cursor":"wrong"}"#,
                r#"{"job_id":"two","stdout_cursor":"wrong"}"#,
            ],
        ] {
            let signatures: Vec<_> = arguments
                .iter()
                .map(|arguments| {
                    let call = argentum_providers::ModelToolCall {
                        id: "synthetic".into(),
                        name: "poll_process_job".into(),
                        arguments: (*arguments).into(),
                    };
                    rejection_signature(&call, "", PreflightCode::WrongType)
                })
                .collect();
            assert_ne!(signatures[0], signatures[1]);
            assert_eq!(signatures[0].2.as_ref().unwrap().len(), 64);
            assert!(!signatures[0].2.as_ref().unwrap().contains("bad-one"));
        }
    }

    #[test]
    fn retained_feedback_is_fixed_metadata_not_paths_or_arbitrary_text() {
        let content = PreflightCode::InvalidPath.content(Some("PRIVATE_PATH"));
        let retained = retained_feedback(&content).unwrap();
        assert!(!retained.contains("PRIVATE_PATH"));
        let mut forged: serde_json::Value = serde_json::from_str(&content).unwrap();
        forged["detail"] = "PRIVATE_BODY".into();
        assert!(retained_feedback(&forged.to_string()).is_none());
        forged = serde_json::from_str(&content).unwrap();
        forged["executed"] = true.into();
        assert!(retained_feedback(&forged.to_string()).is_none());
    }

    #[test]
    fn edit_feedback_identifies_the_actual_field_and_keeps_safe_failure_history() {
        for (reason, field) in [
            ("old_text must be nonempty", "old_text"),
            ("new_text must differ from old_text", "new_text"),
        ] {
            let error = ToolError::Workspace(WorkspaceError::InvalidText(reason));
            let (code, _, issue) = preparation_feedback(&error).unwrap();
            let issue = issue.unwrap();
            assert_eq!(issue.field, Some(field));
            assert_ne!(issue.category, "sha256");
            let content = code.content_with_issues(Some("PRIVATE_PATH"), &[issue]);
            let retained = retained_feedback(&content).unwrap();
            assert!(!retained.contains("PRIVATE_PATH"));
            assert!(!retained.contains("issues"));
            let value: serde_json::Value = serde_json::from_str(&retained).unwrap();
            assert_eq!(value["executed"], false);
            assert_eq!(value["success"], false);
        }
    }

    #[test]
    fn missing_and_ambiguous_matches_keep_distinct_actionable_history() {
        for (reason, expected_code, actual) in [
            (
                "edit text was not found",
                PreflightCode::ExactMatchMissing,
                "zero_matches",
            ),
            (
                "edit text has multiple matches",
                PreflightCode::ExactMatchAmbiguous,
                "multiple_matches",
            ),
        ] {
            let (code, _, issue) =
                preparation_feedback(&ToolError::Workspace(WorkspaceError::InvalidText(reason)))
                    .unwrap();
            assert_eq!(code, expected_code);
            let issue = issue.unwrap();
            assert_eq!(issue.actual, Some(actual));
            let retained =
                retained_feedback(&code.content_with_issues(Some("PRIVATE_PATH"), &[issue]))
                    .unwrap();
            assert!(retained.contains(code.detail()));
            assert!(!retained.contains("PRIVATE_PATH"));
            if code == PreflightCode::ExactMatchAmbiguous {
                assert!(issue.correction.contains("surrounding unchanged text"));
            }
        }
        let mismatch = ToolError::Workspace(WorkspaceError::InvalidText(
            "edit text line endings do not match",
        ));
        let (code, _, issue) = preparation_feedback(&mismatch).unwrap();
        assert_eq!(code, PreflightCode::LineEndingMismatch);
        assert!(issue.unwrap().correction.contains("newline_mode=preserve"));
        assert!(retained_feedback(&code.content(None))
            .unwrap()
            .contains("newline_mode=preserve"));

        let unsupported = ToolError::Workspace(WorkspaceError::InvalidText(
            "preserve mode requires uniform LF or CRLF source",
        ));
        let (code, _, issue) = preparation_feedback(&unsupported).unwrap();
        assert_eq!(code, PreflightCode::PreserveModeUnsupported);
        assert_eq!(issue.unwrap().field, Some("newline_mode"));
        assert!(retained_feedback(&code.content(None))
            .unwrap()
            .contains("uniformly LF or CRLF"));
    }

    #[test]
    fn post_preparation_staleness_and_recovery_errors_are_not_reclassified() {
        for error in [
            WorkspaceError::StaleMutation("file".into()),
            WorkspaceError::RollbackFailed("file".into()),
            WorkspaceError::Cancelled,
        ] {
            assert!(preparation_failure(&ToolError::Workspace(error)).is_none());
        }
        assert!(preparation_failure(&ToolError::Security(
            argentum_security::SecurityError::ApprovalRequired(
                argentum_domain::Capability::WriteFiles
            )
        ))
        .is_none());
    }
}
