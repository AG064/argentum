// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 AG064

//! Every model request in the running session: when, how long, where, and what filled
//! its context. Kept for this session only.

use std::collections::VecDeque;
use std::time::Instant;

use argentum_domain::context_budget::RequestAccounting;
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

    pub fn records(&self) -> impl Iterator<Item = &RequestRecord> {
        self.records.iter().rev()
    }

    pub fn latest(&self) -> Option<&RequestRecord> {
        self.records.back()
    }
}

/// What filled the context of one request, largest first.
pub(crate) fn context_parts(accounting: &RequestAccounting) -> Vec<ContextPart> {
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
        .filter(|(key, _)| !known.contains(&key.as_str()))
        .map(|(_, amount)| *amount)
        .sum();
    let mut parts = vec![
        (
            "Tool calls and results this run",
            component("current_complete_exchanges"),
        ),
        ("Earlier conversation", component("historical_projection")),
        (
            "Instructions",
            component("host_guidance_and_fresh_instructions"),
        ),
        ("Your message", component("current_task")),
        ("Tool definitions", accounting.serialized_catalog_bytes),
        (
            "Reserved for the answer",
            u64::from(accounting.output_reserve),
        ),
        ("Safety margin", accounting.uncertainty_allowance),
        (
            "Other",
            component("message_array_overhead") + accounting.serialized_envelope_bytes + other,
        ),
    ];
    parts.retain(|(_, amount)| *amount > 0);
    parts.sort_by_key(|part| std::cmp::Reverse(part.1));
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
    fn context_parts_are_largest_first_and_share_the_window() {
        let parts = context_parts(&accounting());
        assert_eq!(parts[0].label, "Tool calls and results this run");
        assert!((parts[0].share - 0.15).abs() < 0.001);
        assert!(parts.iter().all(|part| part.amount > 0));
        assert!(!parts
            .iter()
            .any(|part| part.label == "Earlier conversation"));
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
