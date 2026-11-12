// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 AG064

//! Stable machine-readable error envelope for `--json` inspection commands.
//!
//! Success payloads are owned by each command and are not rewritten here. This
//! module only defines the failure envelope and the error classification used
//! when a command fails before it can print its own structured report.

use std::fmt;

use serde::Serialize;

pub(crate) const JSON_ERROR_SCHEMA_VERSION: u32 = 1;
const MAX_MESSAGE_CHARS: usize = 500;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct JsonErrorBody {
    pub code: String,
    pub message: String,
}

impl JsonErrorBody {
    pub(crate) fn new(code: &str, message: impl Into<String>) -> Self {
        Self {
            code: code.to_owned(),
            message: sanitize_message(&message.into()),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct JsonErrorCheck {
    pub id: String,
    pub required: bool,
    pub ok: bool,
    pub detail: String,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct JsonErrorEnvelope {
    pub schema_version: u32,
    pub ok: bool,
    pub command: String,
    pub error: JsonErrorBody,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub checks: Vec<JsonErrorCheck>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub action: Option<String>,
}

impl JsonErrorEnvelope {
    pub(crate) fn new(command: &str, code: &str, message: impl Into<String>) -> Self {
        Self {
            schema_version: JSON_ERROR_SCHEMA_VERSION,
            ok: false,
            command: command.to_owned(),
            error: JsonErrorBody::new(code, message),
            checks: Vec::new(),
            action: None,
        }
    }

    pub(crate) fn from_message(command: &str, message: &str) -> Self {
        Self::new(command, classify_error(message), message)
    }

    #[cfg(test)]
    pub(crate) fn with_checks(mut self, checks: Vec<JsonErrorCheck>) -> Self {
        self.checks = checks;
        self
    }

    #[cfg(test)]
    pub(crate) fn with_action(mut self, action: impl Into<String>) -> Self {
        self.action = Some(action.into());
        self
    }
}

/// Marker error returned after a command has already written its own structured
/// JSON failure object. The host must not append a second object.
#[derive(Debug)]
pub(crate) struct JsonEmitted;

impl fmt::Display for JsonEmitted {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("structured JSON failure was already emitted")
    }
}

impl std::error::Error for JsonEmitted {}

pub(crate) fn emit(envelope: &JsonErrorEnvelope) {
    match serde_json::to_string_pretty(envelope) {
        Ok(value) => println!("{value}"),
        Err(_) => println!(
            "{{\"schema_version\":{JSON_ERROR_SCHEMA_VERSION},\"ok\":false,\"command\":\"cli\",\"error\":{{\"code\":\"internal_error\",\"message\":\"the command failed\"}}}}"
        ),
    }
}

/// Map a human failure text to a stable code. New codes are added only with a
/// documented meaning; callers may also pass an explicit code.
pub(crate) fn classify_error(message: &str) -> &'static str {
    let value = message.to_ascii_lowercase();
    if (value.contains("--confirm") || value.contains("confirmation"))
        && (value.contains("match") || value.contains("mismatch"))
    {
        return "confirmation_mismatch";
    }
    if value.contains("not built with")
        || value.contains("rebuild with --features")
        || value.contains("omits optional")
        || value.contains("is unavailable")
        || value.contains("not compiled")
    {
        return "feature_unavailable";
    }
    if value.contains("browser-control.stdio is not installed")
        || (value.contains("browser definition") && value.contains("not installed"))
    {
        return "browser_definition_missing";
    }
    if value.contains("browser dependencies") && value.contains("not ready") {
        return "browser_dependencies_missing";
    }
    if value.contains("database error")
        || (value.contains("database")
            && (value.contains("does not exist")
                || value.contains("not found")
                || value.contains("missing")
                || value.contains("unable to open")
                || value.contains("could not open")
                || value.contains("could not resolve")
                || value.contains("no such file")
                || value.contains("required")))
    {
        return "database_unavailable";
    }
    if value.contains("required check") {
        return "required_check_failed";
    }
    if value.contains("interactive") && value.contains("terminal") {
        return "interactive_required";
    }
    if value.contains("unknown option")
        || value.contains("unknown command")
        || value.contains("require ")
        || value.contains("requires ")
        || value.contains("may be supplied once")
        || value.contains("cannot be combined")
        || value.contains("only accepts")
        || value.contains("is accepted only")
        || value.contains("must be")
        || value.contains("invalid ")
        || value.contains("unexpected ")
    {
        return "invalid_argument";
    }
    if value.contains("not available")
        || value.contains("not found")
        || value.contains("does not exist")
        || value.contains("no such")
        || value.contains("no active session")
    {
        return "not_found";
    }
    "command_failed"
}

/// Redact absolute paths and bound the length so errors never echo private
/// locations or unbounded provider text.
pub(crate) fn sanitize_message(message: &str) -> String {
    let flattened: String = message
        .chars()
        .map(|character| match character {
            '\r' | '\n' | '\t' => ' ',
            other => other,
        })
        .collect();
    let characters: Vec<char> = flattened.chars().collect();
    let mut output = String::new();
    let mut index = 0;
    let mut token_start = true;
    while index < characters.len() {
        if token_start && starts_url(&characters, index) {
            while index < characters.len()
                && !characters[index].is_whitespace()
                && !matches!(
                    characters[index],
                    '"' | '\'' | '`' | ')' | ']' | '}' | ',' | ';'
                )
            {
                index += 1;
            }
            output.push_str("<url>");
            token_start = false;
            continue;
        }
        if token_start && starts_absolute_path(&characters, index) {
            while index < characters.len()
                && !characters[index].is_whitespace()
                && !matches!(
                    characters[index],
                    '"' | '\'' | '`' | ')' | ']' | '}' | ',' | ';'
                )
            {
                index += 1;
            }
            output.push_str("<path>");
            token_start = false;
            continue;
        }
        let character = characters[index];
        output.push(character);
        token_start = character.is_whitespace()
            || matches!(character, '(' | '[' | '{' | '=' | ':' | '\'' | '"');
        index += 1;
    }
    let output = output.trim().to_owned();
    if output.chars().count() > MAX_MESSAGE_CHARS {
        let mut bounded = output.chars().take(MAX_MESSAGE_CHARS).collect::<String>();
        bounded.push_str("...");
        return bounded;
    }
    if output.is_empty() {
        "the command failed".into()
    } else {
        output
    }
}

fn starts_url(characters: &[char], index: usize) -> bool {
    let rest = &characters[index..];
    rest.starts_with(&['h', 't', 't', 'p', ':', '/', '/'])
        || rest.starts_with(&['h', 't', 't', 'p', 's', ':', '/', '/'])
}

fn starts_absolute_path(characters: &[char], index: usize) -> bool {
    let current = characters[index];
    if current.is_ascii_alphabetic()
        && characters.get(index + 1) == Some(&':')
        && matches!(characters.get(index + 2), Some('\\') | Some('/'))
    {
        return true;
    }
    if current == '\\' && characters.get(index + 1) == Some(&'\\') {
        return true;
    }
    current == '/'
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn envelope_has_the_stable_failure_shape() {
        let envelope =
            JsonErrorEnvelope::new("desktop inspect", "invalid_argument", "--hwnd is required");
        let value = serde_json::to_value(&envelope).expect("envelope");
        assert_eq!(value["schema_version"], 1);
        assert_eq!(value["ok"], false);
        assert_eq!(value["command"], "desktop inspect");
        assert_eq!(value["error"]["code"], "invalid_argument");
        assert_eq!(value["error"]["message"], "--hwnd is required");
        assert!(value.get("checks").is_none());
        assert!(value.get("action").is_none());
    }

    #[test]
    fn checks_and_action_are_optional_additions() {
        let envelope =
            JsonErrorEnvelope::new("doctor", "required_check_failed", "required checks failed")
                .with_checks(vec![JsonErrorCheck {
                    id: "database_exists".into(),
                    required: true,
                    ok: false,
                    detail: "database file does not exist".into(),
                }])
                .with_action("initialize the database");
        let value = serde_json::to_value(&envelope).expect("envelope");
        assert_eq!(value["checks"][0]["id"], "database_exists");
        assert_eq!(value["action"], "initialize the database");
    }

    #[test]
    fn representative_failures_get_stable_codes() {
        assert_eq!(
            classify_error("--confirm must exactly match --hwnd"),
            "confirmation_mismatch"
        );
        assert_eq!(
            classify_error(
                "desktop control is unavailable: rebuild with --features desktop-control"
            ),
            "feature_unavailable"
        );
        assert_eq!(
            classify_error("browser-control.stdio is not installed for this workspace"),
            "browser_definition_missing"
        );
        assert_eq!(
            classify_error("browser dependencies are not ready; run the printed install commands"),
            "browser_dependencies_missing"
        );
        assert_eq!(
            classify_error("the database does not exist; run a normal command to initialize it"),
            "database_unavailable"
        );
        assert_eq!(
            classify_error(
                "database error: The system cannot find the path specified. (os error 3)"
            ),
            "database_unavailable"
        );
        assert_eq!(
            classify_error("doctor found required check failures"),
            "required_check_failed"
        );
        assert_eq!(
            classify_error("--hwnd requires a hexadecimal window handle"),
            "invalid_argument"
        );
    }

    #[test]
    fn absolute_paths_are_redacted_and_messages_are_bounded() {
        let message = "unable to open C:\\Users\\owner\\private\\state.db and \\\\server\\share\\db from https://private.example/v1";
        let sanitized = sanitize_message(message);
        assert!(sanitized.contains("<path>"));
        assert!(sanitized.contains("<url>"));
        assert!(!sanitized.contains("C:\\Users"));
        assert!(!sanitized.contains("server\\share"));
        assert!(!sanitized.contains("private.example"));

        let long = "x".repeat(MAX_MESSAGE_CHARS + 50);
        assert!(sanitize_message(&long).chars().count() <= MAX_MESSAGE_CHARS + 3);
    }
}
