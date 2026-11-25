// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 AG064

//! A full-text index of conversation messages, so a search finds a sentence
//! in any past session and stays fast as history grows.
//!
//! The index is an SQLite FTS5 table over `conversation_messages`. It keeps no
//! copy of the text: it reads the message table by row id. Triggers keep it in
//! step with every insert, text change and delete, including the deletes a
//! removed session cascades to. Messages written before the index existed are
//! indexed once, when the table is created.

use rusqlite::{params, Connection, OptionalExtension};

use super::{ProjectId, SessionId, StoreError};

/// Session ids whose messages match `?` (an FTS5 expression from
/// [`match_expression`]) in project `?`.
pub(super) const MATCHING_SESSIONS_SQL: &str = "SELECT m.session_id FROM message_search \
     JOIN conversation_messages m ON m.rowid = message_search.rowid \
     WHERE message_search MATCH ? AND m.project_id = ?";

/// The longest piece of a message shown under a matching session.
const MAX_SNIPPET_CHARS: usize = 160;

pub(super) fn migrate(connection: &Connection) -> Result<(), StoreError> {
    let existed = connection
        .query_row(
            "SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = 'message_search'",
            [],
            |_| Ok(()),
        )
        .optional()?
        .is_some();
    let transaction = connection.unchecked_transaction()?;
    transaction.execute_batch(
        "CREATE VIRTUAL TABLE IF NOT EXISTS message_search USING fts5(
             text,
             content = 'conversation_messages',
             content_rowid = 'rowid',
             tokenize = 'unicode61 remove_diacritics 2'
         );
         CREATE TRIGGER IF NOT EXISTS message_search_insert
         AFTER INSERT ON conversation_messages BEGIN
             INSERT INTO message_search(rowid, text) VALUES (new.rowid, new.text);
         END;
         CREATE TRIGGER IF NOT EXISTS message_search_delete
         AFTER DELETE ON conversation_messages BEGIN
             INSERT INTO message_search(message_search, rowid, text)
             VALUES ('delete', old.rowid, old.text);
         END;
         CREATE TRIGGER IF NOT EXISTS message_search_update
         AFTER UPDATE OF text ON conversation_messages BEGIN
             INSERT INTO message_search(message_search, rowid, text)
             VALUES ('delete', old.rowid, old.text);
             INSERT INTO message_search(rowid, text) VALUES (new.rowid, new.text);
         END;",
    )?;
    if !existed {
        transaction.execute(
            "INSERT INTO message_search(message_search) VALUES ('rebuild')",
            [],
        )?;
    }
    transaction.commit()?;
    Ok(())
}

/// An FTS5 expression for what the owner typed: every word must appear, and
/// the last one may be the start of a word. Quoting each word keeps FTS5
/// syntax in the query from being interpreted. `None` when nothing in the
/// query is searchable.
pub(super) fn match_expression(query: &str) -> Option<String> {
    let words = query
        .split(|character: char| !character.is_alphanumeric())
        .filter(|word| !word.is_empty())
        .collect::<Vec<_>>();
    let searchable: usize = words.iter().map(|word| word.chars().count()).sum();
    if searchable < 2 {
        return None;
    }
    let mut expression = String::new();
    for (index, word) in words.iter().enumerate() {
        if index > 0 {
            expression.push(' ');
        }
        expression.push('"');
        expression.push_str(&word.replace('"', "\"\""));
        expression.push('"');
        if index + 1 == words.len() {
            expression.push('*');
        }
    }
    Some(expression)
}

/// The newest matching piece of a message in one session, on one line.
pub(super) fn snippet(
    connection: &Connection,
    expression: &str,
    project_id: ProjectId,
    session_id: SessionId,
) -> Result<Option<String>, StoreError> {
    let text = connection
        .query_row(
            "SELECT snippet(message_search, 0, '', '', '...', 16) FROM message_search
             JOIN conversation_messages m ON m.rowid = message_search.rowid
             WHERE message_search MATCH ?1 AND m.project_id = ?2 AND m.session_id = ?3
             ORDER BY m.created_at DESC LIMIT 1",
            params![expression, project_id.to_string(), session_id.to_string()],
            |row| row.get::<_, String>(0),
        )
        .optional()?;
    Ok(text.map(|text| one_line(&text)))
}

fn one_line(text: &str) -> String {
    let mut line = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if line.chars().count() > MAX_SNIPPET_CHARS {
        line = line.chars().take(MAX_SNIPPET_CHARS - 3).collect::<String>() + "...";
    }
    line
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_query_becomes_quoted_words_with_a_prefix_on_the_last() {
        assert_eq!(
            match_expression("compaction bud").as_deref(),
            Some("\"compaction\" \"bud\"*")
        );
        // FTS5 operators and quotes are only words here.
        assert_eq!(
            match_expression("a\" OR NEAR(x").as_deref(),
            Some("\"a\" \"OR\" \"NEAR\" \"x\"*")
        );
        assert_eq!(match_expression("  ").as_deref(), None);
        assert_eq!(match_expression("x").as_deref(), None);
        assert_eq!(match_expression("***").as_deref(), None);
    }

    #[test]
    fn a_long_snippet_is_cut_to_one_short_line() {
        let text = "word\n".repeat(100);
        let line = one_line(&text);
        assert!(!line.contains('\n'));
        assert_eq!(line.chars().count(), MAX_SNIPPET_CHARS);
        assert!(line.ends_with("..."));
    }
}
