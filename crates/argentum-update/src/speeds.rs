// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 AG064

//! Reported model speeds for the marketplace, and the speed report a user can
//! choose to share.
//!
//! The Hugging Face Hub publishes no speed field, so speeds come from one place:
//! `community/local-model-speeds.json` in the Argentum repository. Every entry
//! arrives by a reviewed pull request. A copy is built into the app, so the
//! marketplace shows speeds without any request. "Refresh speeds" fetches the
//! current file from the repository, only when pressed.
//!
//! A speed report is never sent by Argentum. The owner measures on their own
//! machine, sees every field of the report, and presses a button that opens a
//! prefilled GitHub issue in their browser. They submit it there, or close it.
//! The report holds only the fields of [`SpeedReport`].

use std::time::Duration;

use serde::{Deserialize, Serialize};
use url::Url;

use crate::UpdateError;

/// The table built into this version of the app.
pub const BUILT_IN_TABLE: &str = include_str!("../../../community/local-model-speeds.json");
/// The current table, fetched only when the owner presses "Refresh speeds".
pub const TABLE_URL: &str =
    "https://raw.githubusercontent.com/AG064/argentum/development/community/local-model-speeds.json";
/// Where a report goes: a new issue on the project's own repository.
pub const REPORT_ISSUE_URL: &str = "https://github.com/AG064/argentum/issues/new";
const MAX_TABLE_BYTES: usize = 4 * 1024 * 1024;
const SCHEMA: u32 = 1;

/// One reported speed.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SpeedEntry {
    /// The Hugging Face repository, `owner/name`.
    pub repo: String,
    /// The quantization, such as `Q4_K_M`.
    pub quant: String,
    pub gpu: String,
    pub ram_gb: u32,
    pub tokens_per_second: f64,
    /// The pull request or issue the entry came from.
    pub source: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SpeedTable {
    pub schema: u32,
    pub entries: Vec<SpeedEntry>,
}

impl SpeedTable {
    /// Entries for one repository and quantization, fastest first.
    pub fn lookup(&self, repo: &str, quant: &str) -> Vec<&SpeedEntry> {
        let mut found: Vec<&SpeedEntry> = self
            .entries
            .iter()
            .filter(|entry| {
                entry.repo.eq_ignore_ascii_case(repo) && entry.quant.eq_ignore_ascii_case(quant)
            })
            .collect();
        found.sort_by(|left, right| right.tokens_per_second.total_cmp(&left.tokens_per_second));
        found
    }

    /// A one-line summary for the marketplace, or `None` without reports.
    pub fn summary(&self, repo: &str, quant: &str) -> Option<String> {
        let found = self.lookup(repo, quant);
        let best = found.first()?;
        let more = match found.len() {
            1 => String::new(),
            count => format!(", {} more reports", count - 1),
        };
        Some(format!(
            "Reported {:.1} tokens/s on {} with {} GB RAM{more}",
            best.tokens_per_second, best.gpu, best.ram_gb
        ))
    }
}

fn printable(value: &str, limit: usize) -> bool {
    !value.trim().is_empty()
        && value.chars().count() <= limit
        && value.chars().all(|character| !character.is_control())
}

/// Read and check a table. Entries that are not plausible are refused, so a
/// bad edit cannot put odd text or numbers in the interface.
pub fn parse_table(text: &str) -> Result<SpeedTable, UpdateError> {
    let table: SpeedTable = serde_json::from_str(text)
        .map_err(|error| UpdateError(format!("The speed table could not be read: {error}")))?;
    if table.schema != SCHEMA {
        return Err(UpdateError(
            "The speed table has a format this version does not read.".into(),
        ));
    }
    for entry in &table.entries {
        let valid = crate::hub::valid_repo_id(&entry.repo)
            && crate::hub::quant_of(&format!("x-{}.gguf", entry.quant))
                .is_some_and(|quant| quant.eq_ignore_ascii_case(&entry.quant))
            && printable(&entry.gpu, 80)
            && (1..=4096).contains(&entry.ram_gb)
            && entry.tokens_per_second.is_finite()
            && entry.tokens_per_second > 0.0
            && entry.tokens_per_second < 10_000.0
            && entry
                .source
                .starts_with("https://github.com/AG064/argentum/")
            && printable(&entry.source, 120);
        if !valid {
            return Err(UpdateError(format!(
                "The speed table has an entry Argentum does not accept, for {}.",
                entry.repo.chars().take(96).collect::<String>()
            )));
        }
    }
    Ok(table)
}

pub fn built_in_table() -> SpeedTable {
    parse_table(BUILT_IN_TABLE).unwrap_or(SpeedTable {
        schema: SCHEMA,
        entries: Vec::new(),
    })
}

/// Fetch the current table from the repository.
pub async fn fetch_table() -> Result<SpeedTable, UpdateError> {
    let client = reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(15))
        .user_agent(concat!("Argentum/", env!("CARGO_PKG_VERSION")))
        .https_only(true)
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(|_| UpdateError("The request could not be prepared.".into()))?;
    let mut response = client
        .get(TABLE_URL)
        .timeout(Duration::from_secs(30))
        .send()
        .await
        .map_err(|_| UpdateError("Argentum could not reach GitHub for the speed table.".into()))?;
    if !response.status().is_success() {
        return Err(UpdateError(format!(
            "GitHub answered with status {} for the speed table.",
            response.status().as_u16()
        )));
    }
    let mut body = Vec::new();
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|_| UpdateError("The speed table download stopped early.".into()))?
    {
        if body.len() + chunk.len() > MAX_TABLE_BYTES {
            return Err(UpdateError(
                "The speed table was larger than expected.".into(),
            ));
        }
        body.extend_from_slice(&chunk);
    }
    parse_table(&String::from_utf8_lossy(&body))
}

/// Everything a speed report contains, and nothing more.
#[derive(Debug, Clone, PartialEq)]
pub struct SpeedReport {
    pub repo: String,
    pub quant: String,
    pub gpu: String,
    pub ram_gb: u32,
    pub tokens_per_second: f64,
}

impl SpeedReport {
    /// The table entry the report proposes, as JSON. The source is left for
    /// the reviewer to fill with the issue's address.
    pub fn entry_json(&self) -> String {
        let entry = serde_json::json!({
            "repo": self.repo,
            "quant": self.quant,
            "gpu": self.gpu,
            "ram_gb": self.ram_gb,
            "tokens_per_second": (self.tokens_per_second * 10.0).round() / 10.0,
        });
        serde_json::to_string_pretty(&entry).unwrap_or_default()
    }

    pub fn title(&self) -> String {
        format!("Speed report: {} {}", self.repo, self.quant)
    }

    /// The issue text: the exact data, and how it was measured.
    pub fn body(&self) -> String {
        format!(
            "Measured with Argentum's \"Measure speed\" button: one fixed prompt, up to 128 output tokens, generation speed from the first token to the end.\n\n```json\n{}\n```\n\nThis issue contains only the fields above. Argentum opened it in the browser and did not send anything itself.",
            self.entry_json()
        )
    }

    /// The prefilled issue address the owner's browser opens.
    pub fn issue_url(&self) -> String {
        let mut url = Url::parse(REPORT_ISSUE_URL).expect("fixed issue address");
        url.query_pairs_mut()
            .append_pair("title", &self.title())
            .append_pair("body", &self.body());
        url.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry() -> SpeedEntry {
        SpeedEntry {
            repo: "unsloth/Qwen3.5-4B-GGUF".into(),
            quant: "Q4_K_M".into(),
            gpu: "NVIDIA GeForce RTX 4060".into(),
            ram_gb: 32,
            tokens_per_second: 41.5,
            source: "https://github.com/AG064/argentum/pull/1".into(),
        }
    }

    #[test]
    fn the_built_in_table_is_valid() {
        let table = parse_table(BUILT_IN_TABLE).expect("built-in table");
        assert_eq!(table.schema, SCHEMA);
    }

    #[test]
    fn lookups_match_repo_and_quant_fastest_first() {
        let mut slower = entry();
        slower.tokens_per_second = 12.0;
        slower.gpu = "AMD Radeon RX 6600".into();
        let table = SpeedTable {
            schema: 1,
            entries: vec![slower, entry()],
        };
        let found = table.lookup("unsloth/qwen3.5-4b-gguf", "q4_k_m");
        assert_eq!(found.len(), 2);
        assert_eq!(found[0].tokens_per_second, 41.5);
        assert_eq!(
            table
                .summary("unsloth/Qwen3.5-4B-GGUF", "Q4_K_M")
                .as_deref(),
            Some(
                "Reported 41.5 tokens/s on NVIDIA GeForce RTX 4060 with 32 GB RAM, 1 more reports"
            )
        );
        assert_eq!(table.summary("unsloth/Qwen3.5-4B-GGUF", "Q8_0"), None);
    }

    #[test]
    fn implausible_entries_are_refused() {
        let table = |entry: SpeedEntry| {
            serde_json::to_string(&SpeedTable {
                schema: 1,
                entries: vec![entry],
            })
            .expect("json")
        };
        assert!(parse_table(&table(entry())).is_ok());
        for bad in [
            SpeedEntry {
                repo: "../x".into(),
                ..entry()
            },
            SpeedEntry {
                quant: "Q99".into(),
                ..entry()
            },
            SpeedEntry {
                gpu: "line\nbreak".into(),
                ..entry()
            },
            SpeedEntry {
                ram_gb: 0,
                ..entry()
            },
            SpeedEntry {
                tokens_per_second: f64::INFINITY,
                ..entry()
            },
            SpeedEntry {
                tokens_per_second: -1.0,
                ..entry()
            },
            SpeedEntry {
                source: "https://example.com/x".into(),
                ..entry()
            },
        ] {
            assert!(parse_table(&table(bad.clone())).is_err(), "{bad:?}");
        }
        assert!(parse_table(r#"{"schema":2,"entries":[]}"#).is_err());
        assert!(parse_table(r#"{"schema":1,"entries":[],"extra":1}"#).is_err());
    }

    #[test]
    fn a_report_contains_only_its_five_fields() {
        let report = SpeedReport {
            repo: "unsloth/Qwen3.5-4B-GGUF".into(),
            quant: "Q4_K_M".into(),
            gpu: "NVIDIA GeForce RTX 4060".into(),
            ram_gb: 32,
            tokens_per_second: 41.4567,
        };
        let entry: serde_json::Value = serde_json::from_str(&report.entry_json()).expect("json");
        let keys: Vec<&str> = entry
            .as_object()
            .expect("object")
            .keys()
            .map(String::as_str)
            .collect();
        assert_eq!(keys.len(), 5, "{keys:?}");
        assert_eq!(entry["tokens_per_second"], 41.5);
        let url = Url::parse(&report.issue_url()).expect("url");
        assert_eq!(url.host_str(), Some("github.com"));
        assert_eq!(url.path(), "/AG064/argentum/issues/new");
        let body = url
            .query_pairs()
            .find(|(key, _)| key == "body")
            .map(|(_, value)| value.into_owned())
            .expect("body");
        assert!(body.contains("\"ram_gb\": 32"), "{body}");
        assert!(body.contains("did not send anything"), "{body}");
    }
}
