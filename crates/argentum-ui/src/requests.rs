// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 AG064

//! Every model request in the running session: when, how long, where, and what filled
//! its context. Kept for this session only.

use std::collections::VecDeque;
use std::time::Instant;

use argentum_domain::context_budget::RequestAccounting;

/// Component keys that measure part of the system message; see the provider.
const WITHIN_SYSTEM_PREFIX: &str = "within_system:";
use argentum_domain::{RunId, ToolTrace};

const MAX_REQUESTS: usize = 200;

#[derive(Debug, Clone)]
pub(crate) struct RequestRecord {
    pub request_id: uuid::Uuid,
    pub run_id: RunId,
    pub purpose: String,
    pub endpoint: String,
    pub model: String,
    pub accounting: RequestAccounting,
    pub clock: String,
    started: Option<Instant>,
    pub duration_ms: Option<u64>,
    pub attempts: u8,
    pub outcome: String,
    pub status: Option<u16>,
    pub detail: String,
    pub input_tokens: Option<u64>,
    pub cached_tokens: Option<u64>,
    pub output_tokens: Option<u64>,
    pub tools: Vec<String>,
}

/// One part of a request's context, as a share of what the model can hold.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct ContextPart {
    pub label: &'static str,
    pub amount: u64,
    pub share: f32,
}

#[derive(Debug, Default)]
pub(crate) struct RequestLog {
    records: VecDeque<RequestRecord>,
}

fn clock_now() -> String {
    time::OffsetDateTime::now_utc()
        .format(&time::macros::format_description!(
            "[hour]:[minute]:[second] UTC"
        ))
        .unwrap_or_default()
}

/// What the request was sent to, as a person would read it.
fn request_target(accounting: &RequestAccounting) -> String {
    let base = accounting
        .provider_endpoint
        .trim_start_matches("https://")
        .trim_start_matches("http://")
        .trim_end_matches('/');
    match accounting.provider_kind {
        argentum_domain::ProviderKind::OpenAiCompatible => format!("POST {base}/chat/completions"),
        _ => base.to_owned(),
    }
}

impl RequestLog {
    pub fn accounted(
        &mut self,
        request_id: uuid::Uuid,
        run_id: RunId,
        purpose: &str,
        accounting: &RequestAccounting,
    ) {
        if let Some(record) = self.records.iter_mut().find(|r| r.request_id == request_id) {
            record.accounting = accounting.clone();
            return;
        }
        if self.records.len() >= MAX_REQUESTS {
            self.records.pop_front();
        }
        self.records.push_back(RequestRecord {
            request_id,
            run_id,
            purpose: purpose.replace('_', " "),
            endpoint: request_target(accounting),
            model: accounting.model.clone(),
            accounting: accounting.clone(),
            clock: clock_now(),
            started: None,
            duration_ms: None,
            attempts: 0,
            outcome: "waiting".into(),
            status: None,
            detail: String::new(),
            input_tokens: None,
            cached_tokens: None,
            output_tokens: None,
            tools: Vec::new(),
        });
    }

    #[allow(clippy::too_many_arguments)]
    pub fn attempt(
        &mut self,
        request_id: uuid::Uuid,
        attempt: u8,
        outcome: &str,
        status: Option<u16>,
        error_class: Option<&str>,
        stop_reason: Option<&str>,
        planned_delay_ms: Option<u64>,
    ) {
        let Some(record) = self.records.iter_mut().find(|r| r.request_id == request_id) else {
            return;
        };
        record.attempts = record.attempts.max(attempt);
        if outcome == "started" {
            record.started.get_or_insert_with(Instant::now);
            record.clock = clock_now();
            record.outcome = "running".into();
            return;
        }
        record.duration_ms = record
            .started
            .map(|started| u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX));
        record.outcome = outcome.replace('_', " ");
        record.status = status;
        record.detail = [
            stop_reason.map(|reason| reason.replace('_', " ")),
            error_class.map(|class| class.replace('_', " ")),
            planned_delay_ms.map(|delay| format!("retry in {delay} ms")),
        ]
        .into_iter()
        .flatten()
        .collect::<Vec<_>>()
        .join(", ");
    }

    /// Usage arrives per run, so it belongs to that run's latest request.
    pub fn usage(&mut self, run_id: RunId, input: u64, cached: Option<u64>, output: u64) {
        if let Some(record) = self.records.iter_mut().rev().find(|r| r.run_id == run_id) {
            record.input_tokens = Some(input);
            record.cached_tokens = cached;
            record.output_tokens = Some(output);
        }
    }

    pub fn tool(&mut self, trace: &ToolTrace) {
        if let Some(record) = self
            .records
            .iter_mut()
            .rev()
            .find(|r| r.run_id == trace.run_id)
        {
            let took = trace
                .duration_ms
                .map_or_else(String::new, |ms| format!(" {ms} ms"));
            record.tools.push(format!("{}{took}", trace.tool_id));
        }
    }

    /// Tokens per serialized byte for a model, from the newest request of
    /// that model whose input tokens the provider reported.
    pub fn scale_for(&self, model: &str) -> TokenScale {
        self.records
            .iter()
            .rev()
            .filter(|record| record.model == model)
            .find_map(|record| {
                let bytes = record.accounting.wire_bytes()?;
                let reported = record.input_tokens?;
                (bytes > 0 && reported > 0).then(|| reported as f64 / bytes as f64)
            })
            .map_or(TokenScale::Estimated, TokenScale::Measured)
    }

    pub fn records(&self) -> impl Iterator<Item = &RequestRecord> {
        self.records.iter().rev()
    }

    pub fn latest(&self) -> Option<&RequestRecord> {
        self.records.back()
    }
}

/// Share of the usable context at which compaction starts, in percent. It
/// matches the runtime, which shortens old tool output from here.
pub(crate) const COMPACTION_START_PERCENT: u64 = 85;

/// How serialized bytes become tokens for the breakdown.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum TokenScale {
    /// Tokens per byte, from the input tokens the provider reported for an
    /// earlier request of the same model and that request's size.
    Measured(f64),
    /// No reported usage yet: the runtime's own estimate.
    Estimated,
}

impl TokenScale {
    pub(crate) fn tokens(self, bytes: u64) -> u64 {
        match self {
            Self::Measured(per_byte) => (bytes as f64 * per_byte).round() as u64,
            Self::Estimated => {
                argentum_domain::context_budget::estimated_tokens_for_wire_bytes(bytes)
            }
        }
    }

    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Measured(_) => "Tokens measured from the usage the provider reported",
            Self::Estimated => "Estimated tokens, until the provider reports usage",
        }
    }
}

/// What filled the context of one request, in tokens, largest first, with
/// the free space last. Parts the runtime counts in tokens stay as they are.
pub(crate) fn context_parts(accounting: &RequestAccounting, scale: TokenScale) -> Vec<ContextPart> {
    let component = |key: &str| accounting.message_components.get(key).copied().unwrap_or(0);
    let known = [
        "current_task",
        "current_complete_exchanges",
        "historical_projection",
        "host_guidance_and_fresh_instructions",
        "message_array_overhead",
    ];
    let other: u64 = accounting
        .message_components
        .iter()
        .filter(|(key, _)| !known.contains(&key.as_str()) && !key.starts_with(WITHIN_SYSTEM_PREFIX))
        .map(|(_, amount)| *amount)
        .sum();
    let instructions = component("within_system:project_instructions");
    let skill = component("within_system:skill");
    let system = component("host_guidance_and_fresh_instructions")
        .saturating_sub(instructions)
        .saturating_sub(skill);
    let mut parts = vec![
        (
            "Tool calls and results this run",
            scale.tokens(component("current_complete_exchanges")),
        ),
        (
            "Earlier conversation",
            scale.tokens(component("historical_projection")),
        ),
        ("System prompt", scale.tokens(system)),
        ("Project instructions", scale.tokens(instructions)),
        ("Skill", scale.tokens(skill)),
        ("Your message", scale.tokens(component("current_task"))),
        (
            "Tool descriptions",
            scale.tokens(accounting.serialized_catalog_bytes),
        ),
        (
            "Reserved for the answer",
            u64::from(accounting.output_reserve),
        ),
        ("Safety margin", accounting.uncertainty_allowance),
        (
            "Other",
            scale.tokens(
                component("message_array_overhead") + accounting.serialized_envelope_bytes + other,
            ),
        ),
    ];
    parts.retain(|(_, amount)| *amount > 0);
    parts.sort_by_key(|part| std::cmp::Reverse(part.1));
    let used: u64 = parts.iter().map(|part| part.1).sum();
    let free = accounting.usable_context.saturating_sub(used);
    if free > 0 {
        parts.push(("Free space", free));
    }
    let usable = accounting.usable_context.max(1) as f32;
    parts
        .into_iter()
        .map(|(label, amount)| ContextPart {
            label,
            amount,
            share: (amount as f32 / usable).min(1.0),
        })
        .collect()
}

/// Tokens left before compaction starts, given what the request holds.
pub(crate) fn left_before_compaction(accounting: &RequestAccounting, input_tokens: u64) -> u64 {
    (accounting.usable_context * COMPACTION_START_PERCENT / 100).saturating_sub(input_tokens)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn accounting() -> RequestAccounting {
        RequestAccounting {
            method: "estimated".into(),
            capacity_source: "owner".into(),
            budget_revision: uuid::Uuid::nil(),
            profile_id: "deepseek".into(),
            model: "deepseek-flash".into(),
            provider_endpoint: "https://api.deepseek.com/".into(),
            provider_kind: argentum_domain::ProviderKind::OpenAiCompatible,
            accounted_input: 30_000,
            serialized_message_bytes: 20_000,
            message_components: [
                ("current_task".to_owned(), 1_000),
                ("current_complete_exchanges".to_owned(), 15_000),
                ("host_guidance_and_fresh_instructions".to_owned(), 4_000),
            ]
            .into_iter()
            .collect(),
            serialized_catalog_bytes: 9_000,
            serialized_envelope_bytes: 100,
            output_reserve: 8_000,
            uncertainty_allowance: 2_000,
            usable_context: 100_000,
            separate_input_limit: None,
            maximum_output: 8_000,
            bundled_model_context_limit: None,
            profile_catalog_context_limit: None,
            profile_catalog_source: None,
            fits: true,
        }
    }

    #[test]
    fn context_parts_are_largest_first_and_end_with_free_space() {
        let mut accounting = accounting();
        accounting
            .message_components
            .insert("within_system:project_instructions".into(), 1_000);
        // One token per byte keeps the arithmetic readable.
        let parts = context_parts(&accounting, TokenScale::Measured(1.0));
        assert_eq!(parts[0].label, "Tool calls and results this run");
        assert!((parts[0].share - 0.15).abs() < 0.001);
        assert!(parts.iter().all(|part| part.amount > 0));
        assert!(!parts
            .iter()
            .any(|part| part.label == "Earlier conversation"));
        let amount = |label: &str| parts.iter().find(|p| p.label == label).map(|p| p.amount);
        assert_eq!(amount("Project instructions"), Some(1_000));
        assert_eq!(amount("System prompt"), Some(3_000));
        assert_eq!(parts.last().unwrap().label, "Free space");
        let total: u64 = parts.iter().map(|part| part.amount).sum();
        assert_eq!(total, accounting.usable_context);
        assert_eq!(left_before_compaction(&accounting, 30_000), 55_000);
        assert_eq!(left_before_compaction(&accounting, 90_000), 0);
    }

    #[test]
    fn reported_usage_turns_bytes_into_measured_tokens() {
        let mut log = RequestLog::default();
        let run = RunId::new_v4();
        log.accounted(uuid::Uuid::new_v4(), run, "main_generation", &accounting());
        assert_eq!(log.scale_for("deepseek-flash"), TokenScale::Estimated);
        // 29,100 bytes on the wire, and the provider reported 9,700 tokens.
        log.usage(run, 9_700, None, 10);
        let TokenScale::Measured(per_byte) = log.scale_for("deepseek-flash") else {
            panic!("measured");
        };
        assert!((per_byte - 9_700.0 / 29_100.0).abs() < 1e-9);
        assert_eq!(log.scale_for("other-model"), TokenScale::Estimated);
        assert_eq!(TokenScale::Measured(per_byte).tokens(2_910), 970);
    }

    #[test]
    fn a_request_records_its_target_timing_and_usage() {
        let mut log = RequestLog::default();
        let (request, run) = (uuid::Uuid::new_v4(), RunId::new_v4());
        log.accounted(request, run, "main_generation", &accounting());
        log.attempt(request, 1, "started", None, None, None, None);
        log.attempt(
            request,
            1,
            "stopped",
            Some(200),
            None,
            Some("completed"),
            None,
        );
        log.usage(run, 7_000, Some(6_000), 300);
        let record = log.latest().expect("record");
        assert_eq!(record.endpoint, "POST api.deepseek.com/chat/completions");
        assert_eq!(record.purpose, "main generation");
        assert_eq!(record.outcome, "stopped");
        assert!(record.duration_ms.is_some());
        assert_eq!(
            (record.input_tokens, record.output_tokens),
            (Some(7_000), Some(300))
        );
    }
}
