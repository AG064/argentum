// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 AG064

//! One check for text that looks like it holds credentials.
//!
//! It is used where text would be kept beyond the turn that made it: kept
//! tool output, project memory notes, and settings imported from other
//! agents. It errs on the side of refusing: a false match only means a note
//! or an output is not kept.

/// Words that usually sit next to a secret value. Compared in lower case.
const MARKERS: &[&str] = &[
    "authorization:",
    "authorization\\\"",
    "bearer ",
    "private key-----",
    "api_key",
    "api-key",
    "api key",
    "password",
    "passwd",
    "client_secret",
    "client-secret",
    "client secret",
    "access_token",
    "access-token",
    "access token",
    "refresh_token",
    "refresh-token",
    "refresh token",
    "secret_access_key",
    "secret-access-key",
    "secret access key",
    "private_key",
    "private-key",
    "private key",
    "approval_ticket",
    "grant_material",
];

/// Prefixes of common provider and forge keys. A match needs at least
/// [`MIN_KEY_BODY`] key characters after the prefix.
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
    "xoxb-",
    "xoxp-",
    "AIza",
    "AKIA",
    "eyJ",
];
const MIN_KEY_BODY: usize = 16;

/// True when the text contains a word that usually labels a secret, such as
/// `password` or `api_key`. This is the check kept tool output has always used.
pub fn has_credential_marker(text: &str) -> bool {
    let lower = text.to_ascii_lowercase();
    MARKERS.iter().any(|marker| lower.contains(marker))
}

/// True when the text contains a key-shaped string, such as `sk-` or `ghp_`
/// followed by at least 16 key characters.
pub fn has_key_shaped_string(text: &str) -> bool {
    let is_key_char =
        |character: char| character.is_ascii_alphanumeric() || "-_.".contains(character);
    for (index, _) in text.char_indices() {
        let at_boundary = text[..index]
            .chars()
            .next_back()
            .is_none_or(|previous| !(previous.is_ascii_alphanumeric() || "-_".contains(previous)));
        if !at_boundary {
            continue;
        }
        for prefix in KEY_PREFIXES {
            if let Some(rest) = text[index..].strip_prefix(prefix) {
                if rest.chars().take_while(|c| is_key_char(*c)).count() >= MIN_KEY_BODY {
                    return true;
                }
            }
        }
    }
    false
}

/// True when the text looks like it holds a credential, by either check.
pub fn looks_like_credential(text: &str) -> bool {
    has_credential_marker(text) || has_key_shaped_string(text)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn labelled_secrets_are_found() {
        assert!(looks_like_credential("API_KEY=synthetic"));
        assert!(looks_like_credential("my Password is here"));
        assert!(looks_like_credential("Authorization: Bearer x"));
    }

    #[test]
    fn key_shaped_strings_are_found_only_with_a_long_body() {
        assert!(looks_like_credential("use sk-abcdefghijklmnopqrstuv now"));
        assert!(looks_like_credential("token ghp_0123456789abcdefABCDEF"));
        assert!(!looks_like_credential("run sk-learn tests"));
        assert!(!looks_like_credential("the task-abcdefghijklmnopqrstuv id"));
    }

    #[test]
    fn ordinary_notes_pass() {
        assert!(!looks_like_credential(
            "Run cargo test -p argentum-store before a commit."
        ));
        assert!(!looks_like_credential("The UI is written in Slint."));
    }
}
