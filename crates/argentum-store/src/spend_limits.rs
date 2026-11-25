// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 AG064

//! A monthly token limit the owner sets for each provider profile.
//!
//! Tokens are what every provider reports, so the limit is in tokens, not
//! money. The month is the calendar month in UTC. What counts is the usage
//! the provider reported for replies and for compaction summaries in this
//! project; nothing is estimated.

use rusqlite::{params, Connection, OptionalExtension};

use super::{parse_stored_usage, AppEvent, ProjectId, Store, StoreError};

/// Share of the limit, in percent, from which the owner is warned.
pub const SPEND_WARNING_PERCENT: u64 = 80;

pub(super) fn migrate(connection: &Connection) -> Result<(), StoreError> {
    connection.execute_batch(
        "CREATE TABLE IF NOT EXISTS provider_spend_limits (
             project_id TEXT NOT NULL,
             profile_id TEXT NOT NULL,
             monthly_tokens INTEGER NOT NULL CHECK(monthly_tokens > 0),
             PRIMARY KEY(project_id, profile_id),
             FOREIGN KEY(project_id) REFERENCES projects(id) ON DELETE CASCADE
         );",
    )?;
    Ok(())
}

/// The first second of the current calendar month in UTC.
pub fn month_start(now: time::OffsetDateTime) -> i64 {
    now.date()
        .replace_day(1)
        .map_or(now.unix_timestamp(), |first| {
            first.midnight().assume_utc().unix_timestamp()
        })
}

impl Store {
    /// Sets or clears (with `None`) a profile's monthly token limit.
    pub fn set_provider_spend_limit(
        &self,
        project_id: ProjectId,
        profile_id: &str,
        monthly_tokens: Option<u64>,
    ) -> Result<(), StoreError> {
        if profile_id.is_empty() || profile_id.len() > 64 {
            return Err(StoreError::InvalidRecord("profile id is invalid".into()));
        }
        let connection = self
            .connection
            .lock()
            .map_err(|_| StoreError::LockPoisoned)?;
        match monthly_tokens {
            Some(0) => {
                return Err(StoreError::InvalidRecord(
                    "a spend limit must be at least one token".into(),
                ))
            }
            Some(tokens) => {
                let tokens = i64::try_from(tokens)
                    .map_err(|_| StoreError::InvalidRecord("spend limit is too large".into()))?;
                connection.execute(
                    "INSERT INTO provider_spend_limits (project_id, profile_id, monthly_tokens)
                     VALUES (?1, ?2, ?3)
                     ON CONFLICT(project_id, profile_id) DO UPDATE SET monthly_tokens = ?3",
                    params![project_id.to_string(), profile_id, tokens],
                )?;
            }
            None => {
                connection.execute(
                    "DELETE FROM provider_spend_limits WHERE project_id = ?1 AND profile_id = ?2",
                    params![project_id.to_string(), profile_id],
                )?;
            }
        }
        Ok(())
    }

    pub fn provider_spend_limit(
        &self,
        project_id: ProjectId,
        profile_id: &str,
    ) -> Result<Option<u64>, StoreError> {
        let connection = self
            .connection
            .lock()
            .map_err(|_| StoreError::LockPoisoned)?;
        Ok(connection
            .query_row(
                "SELECT monthly_tokens FROM provider_spend_limits
                 WHERE project_id = ?1 AND profile_id = ?2",
                params![project_id.to_string(), profile_id],
                |row| row.get::<_, i64>(0),
            )
            .optional()?
            .map(|tokens| u64::try_from(tokens).unwrap_or(0)))
    }

    /// Tokens a profile's provider reported in this project since `since`
    /// (a Unix time): replies and compaction summaries.
    pub fn provider_tokens_since(
        &self,
        project_id: ProjectId,
        profile_id: &str,
        since: i64,
    ) -> Result<u64, StoreError> {
        let connection = self
            .connection
            .lock()
            .map_err(|_| StoreError::LockPoisoned)?;
        let mut total = 0_u64;
        let mut statement = connection.prepare(
            "SELECT usage FROM conversation_messages
             WHERE project_id = ?1 AND profile_id = ?2 AND role = 'assistant'
               AND usage IS NOT NULL AND created_at >= ?3",
        )?;
        let mut rows = statement.query(params![project_id.to_string(), profile_id, since])?;
        while let Some(row) = rows.next()? {
            let usage: String = row.get(0)?;
            if let Some(usage) = parse_stored_usage(Some(&usage))? {
                let consumed = usage.consumed();
                total = total
                    .saturating_add(consumed.input_tokens)
                    .saturating_add(consumed.output_tokens);
            }
        }
        drop(rows);
        drop(statement);
        let mut statement = connection.prepare(
            "SELECT payload FROM event_log
             WHERE project_id = ?1 AND event_type = 'compaction_usage_recorded'
               AND created_at >= ?2",
        )?;
        let mut events = statement.query(params![project_id.to_string(), since])?;
        while let Some(row) = events.next()? {
            let payload: String = row.get(0)?;
            if let Ok(AppEvent::CompactionUsageRecorded {
                profile_id: recorded,
                usage,
                ..
            }) = serde_json::from_str::<AppEvent>(&payload)
            {
                if recorded == profile_id {
                    total = total.saturating_add(usage.total_tokens);
                }
            }
        }
        Ok(total)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use argentum_domain::{
        ConversationMessage, ConversationMessageStatus, ConversationRole, ModelUsage, RunId,
    };

    fn reply(
        store: &Store,
        project_id: ProjectId,
        session_id: argentum_domain::SessionId,
        profile: &str,
        tokens: u64,
    ) {
        store
            .append_conversation_message(&ConversationMessage {
                id: RunId::new_v4(),
                project_id,
                session_id,
                run_id: RunId::new_v4(),
                role: ConversationRole::Assistant,
                text: "answer".into(),
                reasoning: String::new(),
                usage: Some(ModelUsage {
                    input_tokens: tokens - 10,
                    output_tokens: 10,
                    total_tokens: tokens,
                    reasoning_tokens: None,
                    cached_input_tokens: None,
                    context_window_tokens: None,
                    turn_total: None,
                }),
                profile_id: profile.into(),
                model: "model".into(),
                status: ConversationMessageStatus::Complete,
                created_at: time::OffsetDateTime::now_utc(),
            })
            .expect("reply");
    }

    #[test]
    fn a_limit_is_kept_per_profile_and_counts_this_months_reported_tokens() {
        let workspace = tempfile::tempdir().expect("workspace");
        let store = Store::open_in_memory().expect("store");
        let resolution = store
            .resolve_workspace(workspace.path())
            .expect("resolution");
        let project_id = resolution.snapshot.project.id;
        let session_id = resolution.snapshot.active_session_id.expect("session");
        reply(&store, project_id, session_id, "deepseek", 300);
        reply(&store, project_id, session_id, "deepseek", 200);
        reply(&store, project_id, session_id, "other", 999);
        let since = month_start(time::OffsetDateTime::now_utc());
        assert_eq!(
            store
                .provider_tokens_since(project_id, "deepseek", since)
                .unwrap(),
            500
        );
        let tomorrow = time::OffsetDateTime::now_utc().unix_timestamp() + 86_400;
        assert_eq!(
            store
                .provider_tokens_since(project_id, "deepseek", tomorrow)
                .unwrap(),
            0
        );

        assert_eq!(
            store.provider_spend_limit(project_id, "deepseek").unwrap(),
            None
        );
        store
            .set_provider_spend_limit(project_id, "deepseek", Some(1_000))
            .unwrap();
        store
            .set_provider_spend_limit(project_id, "deepseek", Some(400))
            .unwrap();
        assert_eq!(
            store.provider_spend_limit(project_id, "deepseek").unwrap(),
            Some(400)
        );
        assert!(store
            .set_provider_spend_limit(project_id, "deepseek", Some(0))
            .is_err());
        store
            .set_provider_spend_limit(project_id, "unused", Some(50))
            .unwrap();

        let rows = store.provider_usage(project_id).unwrap();
        let row = |id: &str| {
            rows.iter()
                .find(|row| row.profile_id == id)
                .cloned()
                .unwrap()
        };
        assert_eq!(row("deepseek").month_tokens, 500);
        assert_eq!(row("deepseek").monthly_limit, Some(400));
        assert_eq!(row("other").monthly_limit, None);
        // A limit shows even before its provider is used.
        assert_eq!(row("unused").monthly_limit, Some(50));
        assert_eq!(row("unused").month_tokens, 0);

        store
            .set_provider_spend_limit(project_id, "deepseek", None)
            .unwrap();
        assert_eq!(
            store.provider_spend_limit(project_id, "deepseek").unwrap(),
            None
        );
    }

    #[test]
    fn the_month_starts_on_the_first_at_midnight_utc() {
        let moment = time::macros::datetime!(2026-11-25 17:30 UTC);
        assert_eq!(
            month_start(moment),
            time::macros::datetime!(2026-11-01 0:00 UTC).unix_timestamp()
        );
    }
}
