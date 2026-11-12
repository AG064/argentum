// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 AG064

//! Session export from inside the app. It reads the store, removes anything
//! that looks like a credential, and hands the JSON to the platform, which asks
//! the owner where it goes: a Save dialog on Windows, the share sheet on
//! Android. Nothing is written or sent before the owner picks a destination.
//!
//! Provider keys live in the platform secret store, not in the database, but a
//! key can still reach a session as text: pasted into a message, echoed by a
//! tool, or quoted in an error. So the export removes, in this order:
//!
//! 1. every saved provider key, provider keys set in the environment, and the
//!    device identity key, by exact value;
//! 2. the values of fields named like credentials (`api_key`, `authorization`,
//!    `password` and similar);
//! 3. strings shaped like common keys and tokens (`sk-...`, `Bearer ...`,
//!    private key blocks).
//!
//! If a saved key could still be found in the output, the export stops instead
//! of producing a file.

use std::path::Path;

use argentum_domain::SessionId;
use argentum_platform::{provider_credential_key, SecretStore, DEVICE_IDENTITY_SECRET_KEY};
use argentum_store::Store;
use serde_json::Value;

/// What replaces a removed credential.
pub const REDACTED: &str = "[REDACTED]";

/// Field names whose values are always removed. Compared in lower case, with
/// `-` read as `_`.
const CREDENTIAL_FIELDS: &[&str] = &[
    "api_key",
    "apikey",
    "authorization",
    "proxy_authorization",
    "x_api_key",
    "password",
    "passphrase",
    "secret",
    "client_secret",
    "access_token",
    "refresh_token",
    "id_token",
    "bearer_token",
    "session_token",
    "private_key",
    "cookie",
    "set_cookie",
];

/// Prefixes of widely used provider and service keys. A match needs at least
/// [`MIN_TOKEN_BODY`] key characters after the prefix.
const KEY_PREFIXES: &[&str] = &[
    "sk-",
    "sk_",
    "xai-",
    "ghp_",
    "gho_",
    "ghu_",
    "ghs_",
    "github_pat_",
    "glpat-",
    "hf_",
    "AIza",
    "eyJ",
];

const MIN_TOKEN_BODY: usize = 16;

/// A finished export, ready for the platform to save or share.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionExport {
    /// A suggested file name, such as `argentum-session-1a2b3c4d.json`.
    pub file_name: String,
    pub json: String,
}

/// Why an export could not be made. Each message is shown to the owner as is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SessionExportError {
    NoDatabase,
    InvalidSession,
    Read,
    Keys,
    KeyRemained,
}

impl std::fmt::Display for SessionExportError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(match self {
            Self::NoDatabase => "This app keeps no saved sessions, so there is nothing to export.",
            Self::InvalidSession => "That session could not be found.",
            Self::Read => "Argentum could not read the session from its database.",
            Self::Keys => {
                "Argentum could not check the saved keys, so it did not export the session."
            }
            Self::KeyRemained => {
                "The export still contained a saved key, so Argentum did not export it."
            }
        })
    }
}

impl std::error::Error for SessionExportError {}

/// Reads one session from the database at `database` and returns it with
/// credentials removed.
pub fn export_session(
    database: Option<&Path>,
    secrets: &dyn SecretStore,
    session_id: &str,
) -> Result<SessionExport, SessionExportError> {
    let session_id: SessionId = session_id
        .trim()
        .parse()
        .map_err(|_| SessionExportError::InvalidSession)?;
    let database = database
        .filter(|path| path.exists())
        .ok_or(SessionExportError::NoDatabase)?;
    let store = Store::open_existing_read_only(database).map_err(|_| SessionExportError::Read)?;
    export_from_store(&store, secrets, session_id)
}

/// The same as [`export_session`], for a store that is already open.
pub fn export_from_store(
    store: &Store,
    secrets: &dyn SecretStore,
    session_id: SessionId,
) -> Result<SessionExport, SessionExportError> {
    let export = store
        .export_session(session_id)
        .map_err(|error| match error {
            argentum_store::StoreError::InvalidRecord(_) => SessionExportError::InvalidSession,
            _ => SessionExportError::Read,
        })?;
    let known = saved_secrets(store, secrets)?;
    let redacted = redact_value(export, &known);
    let json = serde_json::to_string_pretty(&redacted).map_err(|_| SessionExportError::Read)?;
    if contains_known_secret(&json, &known) {
        return Err(SessionExportError::KeyRemained);
    }
    Ok(SessionExport {
        file_name: export_file_name(session_id),
        json,
    })
}

/// The suggested file name for a session's export.
pub fn export_file_name(session_id: SessionId) -> String {
    let id = session_id.simple().to_string();
    format!("argentum-session-{}.json", &id[..8])
}

/// Every saved key that could appear in a session: each provider profile's
/// key, provider keys set in the environment, and the device identity key.
fn saved_secrets(
    store: &Store,
    secrets: &dyn SecretStore,
) -> Result<Vec<String>, SessionExportError> {
    let mut keys = vec![DEVICE_IDENTITY_SECRET_KEY.to_owned()];
    let mut values = Vec::new();
    let mut profile_ids = store
        .all_provider_profile_ids()
        .map_err(|_| SessionExportError::Read)?;
    // Keys for built-in providers can also come from environment variables,
    // and a built-in provider's key can be saved before its profile is.
    for (profile_id, variable) in argentum_providers::provider_credential_environment() {
        profile_ids.push(profile_id.to_owned());
        if let Ok(value) = std::env::var(variable) {
            let value = value.trim();
            if !value.is_empty() {
                values.push(value.to_owned());
            }
        }
    }
    profile_ids.sort();
    profile_ids.dedup();
    for profile_id in profile_ids {
        // A profile id the key scheme refuses can have no saved key.
        if let Ok(key) = provider_credential_key(&profile_id) {
            keys.push(key);
        }
    }
    for key in keys {
        // Android's Keystore bridge takes only known hosted providers and the
        // device key. A key it cannot hold has no saved value to remove.
        if !secrets.can_hold(&key) {
            continue;
        }
        match secrets.get(&key) {
            Ok(Some(value)) if !value.expose().is_empty() => {
                values.push(value.expose().to_owned());
            }
            Ok(_) => {}
            Err(_) => return Err(SessionExportError::Keys),
        }
    }
    // Longest first, so a key that contains another is removed whole.
    values.sort_by_key(|value| std::cmp::Reverse(value.len()));
    values.dedup();
    Ok(values)
}

fn contains_known_secret(text: &str, known: &[String]) -> bool {
    known.iter().any(|secret| {
        text.contains(secret.as_str())
            || serde_json::to_string(secret)
                .map(|escaped| text.contains(escaped.trim_matches('"')))
                .unwrap_or(false)
    })
}

/// Removes credentials from every string in `value`, and the whole value of
/// every field named like a credential.
pub fn redact_value(value: Value, known: &[String]) -> Value {
    match value {
        Value::String(text) => Value::String(redact_text(&text, known)),
        Value::Array(items) => Value::Array(
            items
                .into_iter()
                .map(|item| redact_value(item, known))
                .collect(),
        ),
        Value::Object(map) => Value::Object(
            map.into_iter()
                .map(|(field, item)| {
                    let item = if is_credential_field(&field) && !is_empty_value(&item) {
                        Value::String(REDACTED.into())
                    } else {
                        redact_value(item, known)
                    };
                    (redact_text(&field, known), item)
                })
                .collect(),
        ),
        other => other,
    }
}

fn is_empty_value(value: &Value) -> bool {
    match value {
        Value::Null => true,
        Value::String(text) => text.is_empty(),
        _ => false,
    }
}

fn is_credential_field(field: &str) -> bool {
    let normalized = field.to_ascii_lowercase().replace('-', "_");
    CREDENTIAL_FIELDS.contains(&normalized.as_str())
}

/// Removes saved keys and key-shaped strings from one piece of text.
pub fn redact_text(text: &str, known: &[String]) -> String {
    let mut output = text.to_owned();
    for secret in known {
        if !secret.is_empty() && output.contains(secret.as_str()) {
            output = output.replace(secret.as_str(), REDACTED);
        }
    }
    let output = redact_private_key_blocks(&output);
    let output = redact_bearer_tokens(&output);
    redact_prefixed_keys(&output)
}

fn is_token_char(character: char) -> bool {
    character.is_ascii_alphanumeric()
        || matches!(character, '-' | '_' | '.' | '~' | '+' | '/' | '=')
}

fn is_key_char(character: char) -> bool {
    character.is_ascii_alphanumeric() || matches!(character, '-' | '_')
}

fn redact_private_key_blocks(text: &str) -> String {
    const BEGIN: &str = "-----BEGIN ";
    const END: &str = "-----END ";
    let mut output = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(start) = rest.find(BEGIN) {
        let header_end = rest[start + BEGIN.len()..].find("-----");
        let is_private = header_end
            .map(|end| rest[start + BEGIN.len()..start + BEGIN.len() + end].contains("PRIVATE KEY"))
            .unwrap_or(false);
        if !is_private {
            output.push_str(&rest[..start + BEGIN.len()]);
            rest = &rest[start + BEGIN.len()..];
            continue;
        }
        output.push_str(&rest[..start]);
        output.push_str(REDACTED);
        // Drop everything to the end of the matching footer. A block with no
        // footer is dropped to the end of the text.
        let after = &rest[start..];
        match after.find(END) {
            Some(end) => {
                let footer = &after[end + END.len()..];
                let footer_end = footer
                    .find("-----")
                    .map(|index| index + 5)
                    .unwrap_or(footer.len());
                rest = &footer[footer_end..];
            }
            None => rest = "",
        }
    }
    output.push_str(rest);
    output
}

fn redact_bearer_tokens(text: &str) -> String {
    let mut output = String::with_capacity(text.len());
    let mut rest = text;
    loop {
        let found = ["Bearer ", "bearer ", "BEARER "]
            .iter()
            .filter_map(|word| rest.find(word).map(|index| (index, word.len())))
            .min();
        let Some((index, word_len)) = found else {
            break;
        };
        let token_start = index + word_len;
        let token_len = rest[token_start..]
            .char_indices()
            .find(|(_, character)| !is_token_char(*character))
            .map(|(offset, _)| offset)
            .unwrap_or(rest.len() - token_start);
        output.push_str(&rest[..token_start]);
        if token_len >= MIN_TOKEN_BODY {
            output.push_str(REDACTED);
        } else {
            output.push_str(&rest[token_start..token_start + token_len]);
        }
        rest = &rest[token_start + token_len..];
    }
    output.push_str(rest);
    output
}

fn redact_prefixed_keys(text: &str) -> String {
    let mut output = String::with_capacity(text.len());
    let mut index = 0;
    let bytes = text.as_bytes();
    while index < text.len() {
        let at_boundary = index == 0
            || !text[..index]
                .chars()
                .next_back()
                .map(is_key_char)
                .unwrap_or(false);
        if at_boundary {
            if let Some(prefix) = KEY_PREFIXES
                .iter()
                .find(|prefix| text[index..].starts_with(**prefix))
            {
                let body_start = index + prefix.len();
                let body_len = text[body_start..]
                    .char_indices()
                    .find(|(_, character)| !is_key_char(*character) && *character != '.')
                    .map(|(offset, _)| offset)
                    .unwrap_or(text.len() - body_start);
                if body_len >= MIN_TOKEN_BODY {
                    output.push_str(REDACTED);
                    index = body_start + body_len;
                    continue;
                }
            }
        }
        // Copy one whole character.
        let width = utf8_width(bytes[index]);
        output.push_str(&text[index..index + width]);
        index += width;
    }
    output
}

fn utf8_width(first: u8) -> usize {
    match first {
        0x00..=0x7f => 1,
        0xc0..=0xdf => 2,
        0xe0..=0xef => 3,
        _ => 4,
    }
}

/// What the platform did with an export.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExportDelivery {
    /// Written where the owner chose. The text says where.
    Saved(String),
    /// Handed to the system share sheet, where the owner picks a destination.
    Shared,
    /// The owner closed the dialog. Nothing was written.
    Cancelled,
}

/// The platform step that asks the owner where an export goes. It runs on the
/// UI thread. An error is shown to the owner as is.
pub type SessionExportSink = Box<dyn Fn(&SessionExport) -> Result<ExportDelivery, String>>;

/// Connects the Export actions in the interface to `sink`.
pub fn connect(
    ui: &argentum_ui::UiHandle,
    database: Option<std::path::PathBuf>,
    secrets: std::sync::Arc<dyn SecretStore>,
    sink: SessionExportSink,
) {
    use slint::ComponentHandle;

    let app = ui.window().global::<argentum_ui::App>();
    app.set_session_export_available(true);
    let weak = ui.weak_window();
    app.on_export_session(move |session_id| {
        let Some(window) = weak.upgrade() else {
            return;
        };
        let app = window.global::<argentum_ui::App>();
        let result = export_session(database.as_deref(), secrets.as_ref(), &session_id)
            .map_err(|error| error.to_string())
            .and_then(|export| sink(&export));
        let (notice, failed) = match result {
            Ok(ExportDelivery::Saved(location)) => (format!("Saved to {location}."), false),
            Ok(ExportDelivery::Shared) => (
                "Choose where to send the export. It stays on this device until you do.".to_owned(),
                false,
            ),
            Ok(ExportDelivery::Cancelled) => (String::new(), false),
            Err(message) => (message, true),
        };
        app.set_session_export_failed(failed);
        app.set_session_export_notice(notice.into());
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use argentum_domain::{
        AppEvent, ConversationMessage, ConversationMessageStatus, ConversationRole, ProviderKind,
        ProviderProfile, RunId,
    };
    use argentum_platform::PlatformError;
    use argentum_security::SecretValue;
    use argentum_store::EventScope;
    use std::collections::HashMap;
    use std::sync::Mutex;

    #[derive(Default)]
    struct MemorySecretStore(Mutex<HashMap<String, String>>);

    impl SecretStore for MemorySecretStore {
        fn get(&self, key: &str) -> Result<Option<SecretValue>, PlatformError> {
            Ok(self
                .0
                .lock()
                .expect("secrets")
                .get(key)
                .map(|value| SecretValue::new(value.as_str())))
        }
        fn set(&self, key: &str, value: SecretValue) -> Result<(), PlatformError> {
            self.0
                .lock()
                .expect("secrets")
                .insert(key.to_owned(), value.expose().to_owned());
            Ok(())
        }
        fn delete(&self, key: &str) -> Result<(), PlatformError> {
            self.0.lock().expect("secrets").remove(key);
            Ok(())
        }
    }

    const PROVIDER_KEY: &str = "sk-live-provider-key-0123456789abcdef";
    const SECOND_KEY: &str = "plain-saved-key-without-a-known-shape-42";
    const DEVICE_KEY: &str = "device-identity-secret-material-zzzz";

    struct Fixture {
        _directory: tempfile::TempDir,
        database: std::path::PathBuf,
        session_id: SessionId,
        secrets: MemorySecretStore,
    }

    fn fixture(message_text: &str, tool_text: &str) -> Fixture {
        let directory = tempfile::tempdir().expect("directory");
        let workspace = directory.path().join("workspace");
        std::fs::create_dir(&workspace).expect("workspace");
        let database = directory.path().join("argentum.db");
        let store = Store::open(&database).expect("store");
        let resolution = store.resolve_workspace(&workspace).expect("resolution");
        let project_id = resolution.snapshot.project.id;
        let session_id = resolution
            .snapshot
            .active_session_id
            .expect("active session");
        for (id, endpoint) in [
            ("hosted", "https://api.example.test/"),
            ("second", "https://two.example.test/"),
        ] {
            store
                .save_provider_profile(
                    project_id,
                    &ProviderProfile {
                        id: id.into(),
                        label: id.into(),
                        kind: ProviderKind::OpenAiCompatible,
                        endpoint: endpoint.into(),
                        model: "model".into(),
                        selected: id == "hosted",
                    },
                )
                .expect("profile");
        }
        let run_id = RunId::new_v4();
        let started = time::OffsetDateTime::now_utc();
        for (offset, (role, text)) in [
            (ConversationRole::User, message_text),
            (ConversationRole::Assistant, "Here is the answer."),
        ]
        .into_iter()
        .enumerate()
        {
            store
                .append_conversation_message(&ConversationMessage {
                    id: RunId::new_v4(),
                    project_id,
                    session_id,
                    run_id,
                    role,
                    text: text.into(),
                    reasoning: if role == ConversationRole::Assistant {
                        format!("The user pasted {PROVIDER_KEY} by mistake.")
                    } else {
                        String::new()
                    },
                    usage: (role == ConversationRole::Assistant).then_some(
                        argentum_domain::ModelUsage {
                            input_tokens: 10,
                            output_tokens: 5,
                            total_tokens: 15,
                            reasoning_tokens: Some(2),
                            cached_input_tokens: None,
                            context_window_tokens: Some(32_768),
                            turn_total: None,
                        },
                    ),
                    profile_id: if role == ConversationRole::Assistant {
                        "hosted".into()
                    } else {
                        String::new()
                    },
                    model: if role == ConversationRole::Assistant {
                        "model".into()
                    } else {
                        String::new()
                    },
                    status: ConversationMessageStatus::Complete,
                    // Distinct times, so the export order is the turn order.
                    created_at: started + time::Duration::seconds(offset as i64),
                })
                .expect("message");
        }
        store
            .append_event_scoped(
                &EventScope {
                    workspace_key: resolution.workspace_key.clone(),
                    project_id,
                    session_id: Some(session_id),
                    run_id: Some(run_id),
                },
                &AppEvent::ToolActivity {
                    run_id,
                    tool_call_id: uuid::Uuid::new_v4(),
                    text: tool_text.into(),
                },
            )
            .expect("event");
        drop(store);

        let secrets = MemorySecretStore::default();
        secrets
            .set("provider/hosted", SecretValue::new(PROVIDER_KEY))
            .expect("key");
        secrets
            .set("provider/second", SecretValue::new(SECOND_KEY))
            .expect("key");
        secrets
            .set(DEVICE_IDENTITY_SECRET_KEY, SecretValue::new(DEVICE_KEY))
            .expect("key");
        Fixture {
            _directory: directory,
            database,
            session_id,
            secrets,
        }
    }

    #[test]
    fn export_keeps_messages_reasoning_usage_and_events() {
        let fixture = fixture("Please list the files.", "listed 3 files");
        let export = export_session(
            Some(&fixture.database),
            &fixture.secrets,
            &fixture.session_id.to_string(),
        )
        .expect("export");
        let value: Value = serde_json::from_str(&export.json).expect("json");
        assert_eq!(value["format"], "argentum-session-export-v1");
        assert_eq!(value["session"]["id"], fixture.session_id.to_string());
        let messages = value["messages"].as_array().expect("messages");
        assert_eq!(messages.len(), 2);
        assert_eq!(messages[0]["role"], "user");
        assert_eq!(messages[0]["text"], "Please list the files.");
        assert_eq!(messages[1]["usage"]["total_tokens"], 15);
        assert_eq!(messages[1]["profile_id"], "hosted");
        let events = value["events"].as_array().expect("events");
        assert!(events
            .iter()
            .any(|event| event["event"].to_string().contains("listed 3 files")));
        assert!(export.file_name.starts_with("argentum-session-"));
        assert!(export.file_name.ends_with(".json"));
    }

    #[test]
    fn export_never_contains_saved_keys_or_key_shaped_text() {
        let fixture = fixture(
            &format!("My key is {SECOND_KEY} and the device key is {DEVICE_KEY}."),
            &format!(
                "curl -H 'Authorization: Bearer abcdefghijklmnopqrstuvwxyz012345' and {PROVIDER_KEY} \
                 and ghp_0123456789abcdefghijABCDEFGHIJ \
                 -----BEGIN OPENSSH PRIVATE KEY-----\nb3BlbnNzaC1rZXktdjEA\n-----END OPENSSH PRIVATE KEY----- done"
            ),
        );
        let export = export_session(
            Some(&fixture.database),
            &fixture.secrets,
            &fixture.session_id.to_string(),
        )
        .expect("export");
        // A failure names the kind of value only. The value itself is never
        // printed, even though these are test fixtures.
        for (kind, secret) in [
            ("saved provider key", PROVIDER_KEY),
            ("second saved provider key", SECOND_KEY),
            ("device identity key", DEVICE_KEY),
            ("bearer token", "abcdefghijklmnopqrstuvwxyz012345"),
            ("GitHub token", "ghp_0123456789abcdefghijABCDEFGHIJ"),
            ("private key block", "b3BlbnNzaC1rZXktdjEA"),
        ] {
            assert!(
                !export.json.contains(secret),
                "the export still contains the {kind}"
            );
        }
        assert!(export.json.contains(REDACTED));
        // The surrounding text stays readable.
        assert!(export.json.contains("My key is [REDACTED]"));
        assert!(export.json.contains(" done"));
    }

    #[test]
    fn credential_fields_are_removed_whatever_they_hold() {
        let value = serde_json::json!({
            "api_key": "short",
            "headers": { "Authorization": "token-of-any-shape", "X-Api-Key": "k" },
            "nested": [{ "password": "hunter2", "refresh_token": "r" }],
            "input_tokens": 12,
            "token_count": "7",
            "api_key_hint": "none",
        });
        let redacted = redact_value(value, &[]);
        assert_eq!(redacted["api_key"], REDACTED);
        assert_eq!(redacted["headers"]["Authorization"], REDACTED);
        assert_eq!(redacted["headers"]["X-Api-Key"], REDACTED);
        assert_eq!(redacted["nested"][0]["password"], REDACTED);
        assert_eq!(redacted["nested"][0]["refresh_token"], REDACTED);
        assert_eq!(redacted["input_tokens"], 12);
        assert_eq!(redacted["token_count"], "7");
        assert_eq!(redacted["api_key_hint"], "none");
    }

    #[test]
    fn ordinary_text_is_left_alone() {
        let text = "Tasks: sk-1 is short, risk-free, ask-me, Bearer short, and émoji-free ü text.";
        assert_eq!(redact_text(text, &[]), text);
        assert_eq!(
            redact_text(
                "-----BEGIN CERTIFICATE-----\nMIIB\n-----END CERTIFICATE-----",
                &[]
            ),
            "-----BEGIN CERTIFICATE-----\nMIIB\n-----END CERTIFICATE-----"
        );
    }

    #[test]
    fn an_unknown_session_or_missing_database_is_refused() {
        let fixture = fixture("hello", "tool");
        assert_eq!(
            export_session(
                Some(&fixture.database),
                &fixture.secrets,
                &SessionId::new_v4().to_string()
            ),
            Err(SessionExportError::InvalidSession)
        );
        assert_eq!(
            export_session(Some(&fixture.database), &fixture.secrets, "not-an-id"),
            Err(SessionExportError::InvalidSession)
        );
        assert_eq!(
            export_session(None, &fixture.secrets, &fixture.session_id.to_string()),
            Err(SessionExportError::NoDatabase)
        );
    }

    #[test]
    fn keys_a_store_cannot_hold_are_skipped_and_the_rest_still_removed() {
        // Like Android's Keystore bridge: only some key names are accepted,
        // and any other name fails the same way a broken store does.
        struct Limited(MemorySecretStore);
        impl SecretStore for Limited {
            fn get(&self, key: &str) -> Result<Option<SecretValue>, PlatformError> {
                if self.can_hold(key) {
                    self.0.get(key)
                } else {
                    Err(PlatformError::SecureStorage("refused".into()))
                }
            }
            fn set(&self, key: &str, value: SecretValue) -> Result<(), PlatformError> {
                self.0.set(key, value)
            }
            fn delete(&self, key: &str) -> Result<(), PlatformError> {
                self.0.delete(key)
            }
            fn can_hold(&self, key: &str) -> bool {
                key == "provider/hosted"
            }
        }
        let fixture = fixture("plain words", "tool");
        let store = Limited(fixture.secrets);
        let export = export_session(
            Some(&fixture.database),
            &store,
            &fixture.session_id.to_string(),
        )
        .expect("export");
        assert!(!export.json.contains(PROVIDER_KEY));
    }

    #[test]
    fn an_unreadable_secret_store_stops_the_export() {
        struct Broken;
        impl SecretStore for Broken {
            fn get(&self, _key: &str) -> Result<Option<SecretValue>, PlatformError> {
                Err(PlatformError::SecureStorageUnavailable)
            }
            fn set(&self, _key: &str, _value: SecretValue) -> Result<(), PlatformError> {
                Ok(())
            }
            fn delete(&self, _key: &str) -> Result<(), PlatformError> {
                Ok(())
            }
        }
        let fixture = fixture("hello", "tool");
        assert_eq!(
            export_session(
                Some(&fixture.database),
                &Broken,
                &fixture.session_id.to_string()
            ),
            Err(SessionExportError::Keys)
        );
    }
}
