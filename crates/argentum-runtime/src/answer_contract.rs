use std::cell::RefCell;
use std::collections::BTreeSet;
use std::path::{Component, Path};

use argentum_domain::final_result::{
    AnswerContractEvaluation, AnswerContractSource, AnswerContractStatus,
};
use argentum_domain::{SandboxScope, Sha256Digest};
use argentum_security::ScopedPath;
use argentum_workspaces::WorkspaceManager;
use serde::de::{DeserializeSeed, Error as _, MapAccess, SeqAccess, Visitor};
use serde_json::{Map, Number, Value};
use sha2::{Digest, Sha256};
use thiserror::Error;

pub const ANSWER_SCHEMA_MAX_BYTES: usize = 32 * 1024;
pub const ANSWER_SCHEMA_MAX_DEPTH: usize = 32;
pub const ANSWER_SCHEMA_MAX_NODES: usize = 512;
pub const ANSWER_SCHEMA_MAX_PROPERTIES: usize = 128;
pub const ANSWER_MAX_BYTES: usize = 128 * 1024;
pub const ANSWER_VALIDATION_MAX_STEPS: usize = 4096;
pub const ANSWER_VALIDATION_MAX_ERRORS: usize = 8;
pub const ANSWER_CONTRACT_VALIDATOR_VERSION: u16 = 1;
pub const ANSWER_CONTRACT_SUBSET_VERSION: u16 = 1;
pub const JSON_SCHEMA_DIALECT: &str = "https://json-schema.org/draft/2020-12/schema";

#[derive(Debug, Error)]
pub enum AnswerContractError {
    #[error("answer_schema_invalid_path: use one workspace-relative regular file path")]
    InvalidPath,
    #[error("answer_schema_unavailable: {0}")]
    Unavailable(String),
    #[error(
        "answer_schema_digest_mismatch: the selected schema does not match the expected SHA-256"
    )]
    DigestMismatch,
    #[error("answer_schema_invalid_json: the schema must be one duplicate-free JSON document")]
    InvalidJson,
    #[error("answer_schema_unsupported: {0}")]
    Unsupported(&'static str),
    #[error("answer_schema_limit_exceeded: {0}")]
    Limit(&'static str),
    #[error("answer_schema_changed: the accepted schema source changed during the run")]
    Changed,
    #[error("answer_schema_provider_unsupported: schema-contracted answers require the selected compatible HTTP coding provider")]
    ProviderUnsupported,
    #[error("answer_contract_failed: the terminal model answer does not satisfy the accepted JSON schema")]
    OutputInvalid,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AcceptedAnswerContract {
    pub source: AnswerContractSource,
    schema_text: String,
    schema: Value,
}

impl AcceptedAnswerContract {
    pub fn load(
        workspace: &WorkspaceManager,
        relative_path: &str,
        expected_sha256: Option<&Sha256Digest>,
    ) -> Result<Self, AnswerContractError> {
        validate_relative_path(relative_path)?;
        match workspace
            .resolve_path_with_scope(relative_path, SandboxScope::ReadOnly)
            .map_err(|error| AnswerContractError::Unavailable(error.to_string()))?
        {
            ScopedPath::Workspace { .. } => {}
            ScopedPath::Ambient(_) => return Err(AnswerContractError::InvalidPath),
        }
        let schema_text = workspace
            .read_text_bounded(relative_path, ANSWER_SCHEMA_MAX_BYTES)
            .map_err(|error| AnswerContractError::Unavailable(error.to_string()))?;
        let digest = digest_bytes(schema_text.as_bytes());
        if expected_sha256.is_some_and(|expected| expected != &digest) {
            return Err(AnswerContractError::DigestMismatch);
        }
        let schema = parse_strict_json(&schema_text, ANSWER_SCHEMA_MAX_BYTES)
            .map_err(|_| AnswerContractError::InvalidJson)?;
        preflight_schema(&schema)?;
        Ok(Self {
            source: AnswerContractSource {
                relative_path: normalize_relative_path(relative_path),
                sha256: digest,
                dialect: JSON_SCHEMA_DIALECT.into(),
                subset_version: ANSWER_CONTRACT_SUBSET_VERSION,
            },
            schema_text,
            schema,
        })
    }

    pub fn verify_current(&self, workspace: &WorkspaceManager) -> Result<(), AnswerContractError> {
        let current = workspace
            .read_text_bounded(&self.source.relative_path, ANSWER_SCHEMA_MAX_BYTES)
            .map_err(|_| AnswerContractError::Changed)?;
        if digest_bytes(current.as_bytes()) == self.source.sha256 {
            Ok(())
        } else {
            Err(AnswerContractError::Changed)
        }
    }

    pub fn append_system_directive(&self, system: &mut String) {
        system.push_str("\n\nANSWER CONTRACT\nReturn the terminal answer as exactly one JSON document that satisfies the following untrusted workspace JSON Schema. Do not wrap it in Markdown or add prose. Tool calls and intermediate assistant text are not the terminal answer.\nBEGIN UNTRUSTED ANSWER SCHEMA\n");
        system.push_str(&self.schema_text);
        system.push_str("\nEND UNTRUSTED ANSWER SCHEMA\n");
    }

    pub fn evaluate(&self, answer: &str) -> AnswerContractEvaluation {
        let mut error_codes = Vec::new();
        let parsed = if answer.len() > ANSWER_MAX_BYTES {
            push_code(&mut error_codes, "answer_too_large");
            None
        } else {
            match parse_strict_json(answer, ANSWER_MAX_BYTES) {
                Ok(value) => Some(value),
                Err(StrictJsonError::DuplicateMember) => {
                    push_code(&mut error_codes, "duplicate_member");
                    None
                }
                Err(StrictJsonError::Limit) => {
                    push_code(&mut error_codes, "answer_limit_exceeded");
                    None
                }
                Err(StrictJsonError::Invalid) => {
                    push_code(&mut error_codes, "invalid_json");
                    None
                }
            }
        };
        if let Some(value) = parsed.as_ref() {
            let mut state = ValidationState::default();
            validate_instance(&self.schema, value, &mut state, &mut error_codes);
        }
        let status = if error_codes.is_empty() {
            AnswerContractStatus::Valid
        } else {
            AnswerContractStatus::Invalid
        };
        AnswerContractEvaluation {
            source: self.source.clone(),
            validator_version: ANSWER_CONTRACT_VALIDATOR_VERSION,
            status,
            error_count: u16::try_from(error_codes.len()).unwrap_or(u16::MAX),
            error_codes,
            parsed_value_sha256: parsed.as_ref().and_then(|value| {
                (status == AnswerContractStatus::Valid)
                    .then(|| serde_json::to_vec(value).ok())
                    .flatten()
                    .map(|bytes| digest_bytes(&bytes))
            }),
        }
    }
}

fn validate_relative_path(value: &str) -> Result<(), AnswerContractError> {
    let path = Path::new(value);
    if value.is_empty()
        || value.starts_with('~')
        || value.contains("://")
        || path.is_absolute()
        || path
            .components()
            .any(|component| !matches!(component, Component::Normal(_)))
    {
        return Err(AnswerContractError::InvalidPath);
    }
    Ok(())
}

fn normalize_relative_path(value: &str) -> String {
    Path::new(value)
        .components()
        .filter_map(|component| match component {
            Component::Normal(value) => Some(value.to_string_lossy()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("/")
}

fn digest_bytes(bytes: &[u8]) -> Sha256Digest {
    Sha256Digest(format!("{:x}", Sha256::digest(bytes)))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum StrictJsonError {
    Invalid,
    DuplicateMember,
    Limit,
}

#[derive(Default)]
struct ParseState {
    nodes: usize,
}

struct StrictSeed<'a> {
    state: &'a RefCell<ParseState>,
    depth: usize,
}

impl<'de> DeserializeSeed<'de> for StrictSeed<'_> {
    type Value = Value;

    fn deserialize<D>(self, deserializer: D) -> Result<Self::Value, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        if self.depth > ANSWER_SCHEMA_MAX_DEPTH {
            return Err(D::Error::custom("ARGENTUM_JSON_LIMIT"));
        }
        let mut state = self.state.borrow_mut();
        state.nodes = state.nodes.saturating_add(1);
        if state.nodes > ANSWER_SCHEMA_MAX_NODES {
            return Err(D::Error::custom("ARGENTUM_JSON_LIMIT"));
        }
        drop(state);
        deserializer.deserialize_any(StrictVisitor {
            state: self.state,
            depth: self.depth,
        })
    }
}

struct StrictVisitor<'a> {
    state: &'a RefCell<ParseState>,
    depth: usize,
}

impl<'de> Visitor<'de> for StrictVisitor<'_> {
    type Value = Value;

    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("a JSON value")
    }

    fn visit_bool<E>(self, value: bool) -> Result<Self::Value, E> {
        Ok(Value::Bool(value))
    }

    fn visit_i64<E>(self, value: i64) -> Result<Self::Value, E> {
        Ok(Value::Number(value.into()))
    }

    fn visit_u64<E>(self, value: u64) -> Result<Self::Value, E> {
        Ok(Value::Number(value.into()))
    }

    fn visit_f64<E: serde::de::Error>(self, value: f64) -> Result<Self::Value, E> {
        Number::from_f64(value)
            .map(Value::Number)
            .ok_or_else(|| E::custom("unsupported JSON number"))
    }

    fn visit_str<E>(self, value: &str) -> Result<Self::Value, E> {
        Ok(Value::String(value.into()))
    }

    fn visit_string<E>(self, value: String) -> Result<Self::Value, E> {
        Ok(Value::String(value))
    }

    fn visit_none<E>(self) -> Result<Self::Value, E> {
        Ok(Value::Null)
    }

    fn visit_unit<E>(self) -> Result<Self::Value, E> {
        Ok(Value::Null)
    }

    fn visit_seq<A>(self, mut sequence: A) -> Result<Self::Value, A::Error>
    where
        A: SeqAccess<'de>,
    {
        let mut values = Vec::new();
        while let Some(value) = sequence.next_element_seed(StrictSeed {
            state: self.state,
            depth: self.depth.saturating_add(1),
        })? {
            values.push(value);
        }
        Ok(Value::Array(values))
    }

    fn visit_map<A>(self, mut object: A) -> Result<Self::Value, A::Error>
    where
        A: MapAccess<'de>,
    {
        let mut values = Map::new();
        let mut names = BTreeSet::new();
        while let Some(name) = object.next_key::<String>()? {
            if !names.insert(name.clone()) {
                return Err(A::Error::custom("ARGENTUM_DUPLICATE_MEMBER"));
            }
            let value = object.next_value_seed(StrictSeed {
                state: self.state,
                depth: self.depth.saturating_add(1),
            })?;
            values.insert(name, value);
        }
        Ok(Value::Object(values))
    }
}

fn parse_strict_json(value: &str, byte_limit: usize) -> Result<Value, StrictJsonError> {
    if value.len() > byte_limit {
        return Err(StrictJsonError::Limit);
    }
    let state = RefCell::new(ParseState::default());
    let mut deserializer = serde_json::Deserializer::from_str(value);
    let parsed = StrictSeed {
        state: &state,
        depth: 0,
    }
    .deserialize(&mut deserializer)
    .map_err(|error| {
        let message = error.to_string();
        if message.contains("ARGENTUM_DUPLICATE_MEMBER") {
            StrictJsonError::DuplicateMember
        } else if message.contains("ARGENTUM_JSON_LIMIT") {
            StrictJsonError::Limit
        } else {
            StrictJsonError::Invalid
        }
    })?;
    deserializer.end().map_err(|_| StrictJsonError::Invalid)?;
    Ok(parsed)
}

fn preflight_schema(schema: &Value) -> Result<(), AnswerContractError> {
    if !schema.is_object() || schema.get("type").and_then(Value::as_str) != Some("object") {
        return Err(AnswerContractError::Unsupported("root_type_must_be_object"));
    }
    let mut state = SchemaState::default();
    inspect_schema_node(schema, 0, true, &mut state)
}

#[derive(Default)]
struct SchemaState {
    nodes: usize,
    properties: usize,
}

fn inspect_schema_node(
    schema: &Value,
    depth: usize,
    root: bool,
    state: &mut SchemaState,
) -> Result<(), AnswerContractError> {
    if depth > ANSWER_SCHEMA_MAX_DEPTH {
        return Err(AnswerContractError::Limit("schema_depth"));
    }
    state.nodes = state.nodes.saturating_add(1);
    if state.nodes > ANSWER_SCHEMA_MAX_NODES {
        return Err(AnswerContractError::Limit("schema_nodes"));
    }
    let object = schema.as_object().ok_or(AnswerContractError::Unsupported(
        "schema_node_must_be_object",
    ))?;
    const ALLOWED: &[&str] = &[
        "$schema",
        "title",
        "description",
        "type",
        "properties",
        "required",
        "additionalProperties",
        "items",
        "minLength",
        "maxLength",
        "minItems",
        "maxItems",
        "enum",
    ];
    if object.keys().any(|key| !ALLOWED.contains(&key.as_str())) {
        return Err(AnswerContractError::Unsupported("unsupported_keyword"));
    }
    if !root && object.contains_key("$schema") {
        return Err(AnswerContractError::Unsupported("nested_dialect"));
    }
    if let Some(dialect) = object.get("$schema") {
        if dialect.as_str() != Some(JSON_SCHEMA_DIALECT) {
            return Err(AnswerContractError::Unsupported("dialect"));
        }
    }
    for annotation in ["title", "description"] {
        if object
            .get(annotation)
            .is_some_and(|value| !value.is_string())
        {
            return Err(AnswerContractError::Unsupported("annotation_type"));
        }
    }
    let kind = object
        .get("type")
        .and_then(Value::as_str)
        .ok_or(AnswerContractError::Unsupported("declared_type_required"))?;
    if !matches!(
        kind,
        "object" | "array" | "string" | "number" | "integer" | "boolean" | "null"
    ) {
        return Err(AnswerContractError::Unsupported("declared_type"));
    }
    if let Some(values) = object.get("enum") {
        let values = values
            .as_array()
            .filter(|values| !values.is_empty())
            .ok_or(AnswerContractError::Unsupported("enum"))?;
        for value in values {
            if !instance_has_type(value, kind) {
                return Err(AnswerContractError::Unsupported("enum_type"));
            }
        }
    }
    match kind {
        "object" => {
            if object.keys().any(|key| {
                matches!(
                    key.as_str(),
                    "items" | "minLength" | "maxLength" | "minItems" | "maxItems"
                )
            }) {
                return Err(AnswerContractError::Unsupported("keyword_for_type"));
            }
            let properties = match object.get("properties") {
                Some(value) => value
                    .as_object()
                    .ok_or(AnswerContractError::Unsupported("properties"))?,
                None => return Err(AnswerContractError::Unsupported("properties_required")),
            };
            state.properties = state.properties.saturating_add(properties.len());
            if state.properties > ANSWER_SCHEMA_MAX_PROPERTIES {
                return Err(AnswerContractError::Limit("schema_properties"));
            }
            for child in properties.values() {
                inspect_schema_node(child, depth.saturating_add(1), false, state)?;
            }
            if let Some(required) = object.get("required") {
                let required = required
                    .as_array()
                    .ok_or(AnswerContractError::Unsupported("required"))?;
                let mut names = BTreeSet::new();
                for value in required {
                    let name = value
                        .as_str()
                        .ok_or(AnswerContractError::Unsupported("required"))?;
                    if !names.insert(name) {
                        return Err(AnswerContractError::Unsupported("duplicate_required"));
                    }
                }
            }
            if object
                .get("additionalProperties")
                .is_some_and(|value| !value.is_boolean())
            {
                return Err(AnswerContractError::Unsupported("additional_properties"));
            }
        }
        "array" => {
            if object.keys().any(|key| {
                matches!(
                    key.as_str(),
                    "properties" | "required" | "additionalProperties" | "minLength" | "maxLength"
                )
            }) {
                return Err(AnswerContractError::Unsupported("keyword_for_type"));
            }
            let items = object
                .get("items")
                .ok_or(AnswerContractError::Unsupported("items_required"))?;
            inspect_schema_node(items, depth.saturating_add(1), false, state)?;
            inspect_usize_pair(object, "minItems", "maxItems")?;
        }
        "string" => {
            if object.keys().any(|key| {
                matches!(
                    key.as_str(),
                    "properties"
                        | "required"
                        | "additionalProperties"
                        | "items"
                        | "minItems"
                        | "maxItems"
                )
            }) {
                return Err(AnswerContractError::Unsupported("keyword_for_type"));
            }
            inspect_usize_pair(object, "minLength", "maxLength")?;
        }
        _ => {
            if object.keys().any(|key| {
                matches!(
                    key.as_str(),
                    "properties"
                        | "required"
                        | "additionalProperties"
                        | "items"
                        | "minLength"
                        | "maxLength"
                        | "minItems"
                        | "maxItems"
                )
            }) {
                return Err(AnswerContractError::Unsupported("keyword_for_type"));
            }
        }
    }
    Ok(())
}

fn inspect_usize_pair(
    object: &Map<String, Value>,
    minimum: &str,
    maximum: &str,
) -> Result<(), AnswerContractError> {
    let min = object.get(minimum).map(parse_schema_usize).transpose()?;
    let max = object.get(maximum).map(parse_schema_usize).transpose()?;
    if min.zip(max).is_some_and(|(min, max)| min > max) {
        return Err(AnswerContractError::Unsupported("inverted_range"));
    }
    Ok(())
}

fn parse_schema_usize(value: &Value) -> Result<usize, AnswerContractError> {
    value
        .as_u64()
        .and_then(|value| usize::try_from(value).ok())
        .ok_or(AnswerContractError::Unsupported(
            "nonnegative_integer_limit",
        ))
}

#[derive(Default)]
struct ValidationState {
    steps: usize,
}

fn validate_instance(
    schema: &Value,
    value: &Value,
    state: &mut ValidationState,
    errors: &mut Vec<String>,
) {
    if errors.len() >= ANSWER_VALIDATION_MAX_ERRORS {
        return;
    }
    state.steps = state.steps.saturating_add(1);
    if state.steps > ANSWER_VALIDATION_MAX_STEPS {
        push_code(errors, "validation_limit_exceeded");
        return;
    }
    let object = schema
        .as_object()
        .expect("preflight accepted object schema");
    let kind = object["type"].as_str().expect("preflight accepted type");
    if !instance_has_type(value, kind) {
        push_code(errors, "type_mismatch");
        return;
    }
    if object
        .get("enum")
        .and_then(Value::as_array)
        .is_some_and(|allowed| !allowed.iter().any(|item| json_equal(item, value)))
    {
        push_code(errors, "enum_mismatch");
    }
    match (kind, value) {
        ("object", Value::Object(instance)) => {
            let properties = object["properties"]
                .as_object()
                .expect("preflight accepted properties");
            if let Some(required) = object.get("required").and_then(Value::as_array) {
                for name in required.iter().filter_map(Value::as_str) {
                    if !instance.contains_key(name) {
                        push_code(errors, "missing_required");
                    }
                }
            }
            if object.get("additionalProperties").and_then(Value::as_bool) == Some(false)
                && instance.keys().any(|name| !properties.contains_key(name))
            {
                push_code(errors, "additional_property");
            }
            for (name, child_schema) in properties {
                if let Some(child) = instance.get(name) {
                    validate_instance(child_schema, child, state, errors);
                }
            }
        }
        ("array", Value::Array(instance)) => {
            let len = instance.len();
            if object
                .get("minItems")
                .and_then(Value::as_u64)
                .is_some_and(|min| len < min as usize)
            {
                push_code(errors, "min_items");
            }
            if object
                .get("maxItems")
                .and_then(Value::as_u64)
                .is_some_and(|max| len > max as usize)
            {
                push_code(errors, "max_items");
            }
            for child in instance {
                validate_instance(&object["items"], child, state, errors);
            }
        }
        ("string", Value::String(instance)) => {
            let len = instance.chars().count();
            if object
                .get("minLength")
                .and_then(Value::as_u64)
                .is_some_and(|min| len < min as usize)
            {
                push_code(errors, "min_length");
            }
            if object
                .get("maxLength")
                .and_then(Value::as_u64)
                .is_some_and(|max| len > max as usize)
            {
                push_code(errors, "max_length");
            }
        }
        _ => {}
    }
}

fn push_code(errors: &mut Vec<String>, code: &str) {
    if errors.len() < ANSWER_VALIDATION_MAX_ERRORS {
        errors.push(code.into());
    }
}

fn instance_has_type(value: &Value, kind: &str) -> bool {
    match kind {
        "object" => value.is_object(),
        "array" => value.is_array(),
        "string" => value.is_string(),
        "number" => value.is_number(),
        "integer" => value
            .as_i64()
            .map(|_| true)
            .or_else(|| value.as_u64().map(|_| true))
            .unwrap_or_else(|| value.as_f64().is_some_and(|number| number.fract() == 0.0)),
        "boolean" => value.is_boolean(),
        "null" => value.is_null(),
        _ => false,
    }
}

fn json_equal(left: &Value, right: &Value) -> bool {
    match (left, right) {
        (Value::Number(left), Value::Number(right)) => left
            .as_f64()
            .zip(right.as_f64())
            .is_some_and(|(left, right)| left == right),
        _ => left == right,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use argentum_security::{ApprovalPolicy, CapabilityBroker};

    fn contract(schema: &str) -> (tempfile::TempDir, WorkspaceManager, AcceptedAnswerContract) {
        let directory = tempfile::tempdir().unwrap();
        std::fs::write(directory.path().join("answer.schema.json"), schema).unwrap();
        let workspace = WorkspaceManager::new(
            CapabilityBroker::new(directory.path(), ApprovalPolicy::default()).unwrap(),
        );
        let contract =
            AcceptedAnswerContract::load(&workspace, "answer.schema.json", None).unwrap();
        (directory, workspace, contract)
    }

    #[test]
    fn validates_nested_supported_subset_without_coercion_or_dropping() {
        let (_, _, contract) = contract(
            r#"{
          "$schema":"https://json-schema.org/draft/2020-12/schema",
          "type":"object",
          "properties":{
            "name":{"type":"string","minLength":2,"maxLength":8},
            "items":{"type":"array","minItems":1,"maxItems":2,"items":{"type":"integer","enum":[1,2]}}
          },
          "required":["name","items"],
          "additionalProperties":false
        }"#,
        );
        assert_eq!(
            contract.evaluate(r#"{"name":"ok","items":[1,2]}"#).status,
            AnswerContractStatus::Valid
        );
        let invalid = contract.evaluate(r#"{"name":"x","items":["1"],"extra":true}"#);
        assert_eq!(invalid.status, AnswerContractStatus::Invalid);
        assert!(invalid.error_codes.contains(&"min_length".into()));
        assert!(invalid.error_codes.contains(&"type_mismatch".into()));
        assert!(invalid.error_codes.contains(&"additional_property".into()));
    }

    #[test]
    fn rejects_duplicate_members_trailing_prose_and_unsupported_keywords() {
        let (_, _, contract) = contract(
            r#"{"type":"object","properties":{"ok":{"type":"boolean"}},"required":["ok"]}"#,
        );
        assert_eq!(
            contract.evaluate(r#"{"ok":true,"ok":false}"#).error_codes,
            ["duplicate_member"]
        );
        assert_eq!(
            contract.evaluate(r#"{"ok":true} trailing"#).error_codes,
            ["invalid_json"]
        );

        let directory = tempfile::tempdir().unwrap();
        std::fs::write(
            directory.path().join("bad.json"),
            r#"{"type":"object","properties":{},"allOf":[]}"#,
        )
        .unwrap();
        let workspace = WorkspaceManager::new(
            CapabilityBroker::new(directory.path(), ApprovalPolicy::default()).unwrap(),
        );
        assert!(matches!(
            AcceptedAnswerContract::load(&workspace, "bad.json", None),
            Err(AnswerContractError::Unsupported("unsupported_keyword"))
        ));
    }

    #[test]
    fn pins_source_digest_and_rejects_non_workspace_selection() {
        let (directory, workspace, contract) = contract(r#"{"type":"object","properties":{}}"#);
        std::fs::write(
            directory.path().join("answer.schema.json"),
            r#"{"type":"object","properties":{"x":{"type":"string"}}}"#,
        )
        .unwrap();
        assert!(matches!(
            contract.verify_current(&workspace),
            Err(AnswerContractError::Changed)
        ));
        assert!(matches!(
            AcceptedAnswerContract::load(&workspace, "../answer.schema.json", None),
            Err(AnswerContractError::InvalidPath)
        ));
    }
}
