// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 AG064

//! The model's bounded way to say where a goal stands.
//!
//! A goal-bound run is offered one extra tool, `report_goal_status`. Calling
//! it records a report on the goal: complete, partial, or needs input, with a
//! short model-authored summary. The report never changes the goal's
//! lifecycle and grants no authority. The owner confirms a completion (the
//! goal becomes Complete) or dismisses the report (the goal stays active).
//! Nothing completes a goal silently.

use argentum_domain::{
    GoalCompletionReport, GoalId, GoalLifecycle, GoalReportStatus, RunId, SessionId,
    MAX_GOAL_COMPLETION_REPORT_BYTES,
};
use argentum_providers::{ModelToolCall, ModelToolDefinition, ModelToolResult};
use serde::Deserialize;
use serde_json::json;

use super::{now, RuntimeError, RuntimeService};

pub(super) const TOOL_ID: &str = "report_goal_status";

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ReportInput {
    #[serde(default)]
    status: GoalReportStatus,
    #[serde(default)]
    summary: String,
}

/// Recorded when a model calls the tool without a summary. Some models send
/// empty arguments, and the owner still decides on the report.
const MISSING_SUMMARY: &str = "The agent reported the objective as achieved but gave no summary.";

pub(super) fn tool_definition() -> ModelToolDefinition {
    ModelToolDefinition {
        name: TOOL_ID.into(),
        description: "Report where the session goal stands. Call it once before you finish any run that works toward the goal. status is complete when the objective is fully achieved, partial when some of it is done and more is left, or needs_input when you cannot continue without the owner's answer. The summary says what was done and, for partial or needs_input, what is left or the exact question for the owner. This only informs the owner; the goal is not complete until the owner confirms it.".into(),
        parameters: json!({
            "type": "object",
            "properties": {
                "status": {
                    "type": "string",
                    "enum": ["complete", "partial", "needs_input"]
                },
                "summary": {
                    "type": "string",
                    "minLength": 1,
                    "maxLength": MAX_GOAL_COMPLETION_REPORT_BYTES
                }
            },
            "required": ["status", "summary"],
            "additionalProperties": false
        }),
    }
}

fn result(call_id: String, disposition: &'static str, detail: &str) -> ModelToolResult {
    ModelToolResult {
        images: Default::default(),
        call_id,
        content: json!({
            "version": 1,
            "disposition": disposition,
            "detail": detail,
            "goal_completed": false
        })
        .to_string(),
    }
}

/// The reported status, or why the arguments are not acceptable.
fn parse_status(arguments: &str) -> Result<GoalReportStatus, &'static str> {
    serde_json::from_str::<ReportInput>(arguments)
        .map(|input| input.status)
        .map_err(|_| "arguments must be {\"status\": \"complete\" | \"partial\" | \"needs_input\", \"summary\": string}")
}

/// Bounded, trimmed summary text, or why it is not acceptable.
fn parse_summary(arguments: &str) -> Result<String, &'static str> {
    let input: ReportInput = serde_json::from_str(arguments)
        .map_err(|_| "arguments must be {\"status\": \"complete\" | \"partial\" | \"needs_input\", \"summary\": string}")?;
    let summary = input.summary.trim().to_owned();
    if summary.is_empty() {
        return Ok(MISSING_SUMMARY.to_owned());
    }
    if summary.len() > MAX_GOAL_COMPLETION_REPORT_BYTES {
        return Err("summary is too long");
    }
    if summary
        .chars()
        .any(|character| character.is_control() && !matches!(character, '\n' | '\t'))
    {
        return Err("summary contains unsupported control characters");
    }
    Ok(summary)
}

impl RuntimeService {
    /// Tool definitions for a run, with the report tool added when the run is
    /// bound to a goal and already has tools.
    pub(super) fn with_goal_report_tool(
        &self,
        run: RunId,
        mut tools: Vec<ModelToolDefinition>,
    ) -> Result<Vec<ModelToolDefinition>, RuntimeError> {
        if !tools.is_empty() && self.goal_scope(run)?.is_some() {
            tools.push(tool_definition());
        }
        Ok(tools)
    }

    /// Records the model's completion report on the run's goal. Every refusal
    /// is returned to the model as a tool result, never as a run failure.
    pub(super) fn record_goal_completion_report(
        &self,
        run: RunId,
        call: &ModelToolCall,
    ) -> Result<ModelToolResult, RuntimeError> {
        let Some(scope) = self.goal_scope(run)? else {
            return Ok(result(
                call.id.clone(),
                "unavailable",
                "this run is not working toward a goal",
            ));
        };
        let summary = match parse_summary(&call.arguments) {
            Ok(summary) => summary,
            Err(reason) => return Ok(result(call.id.clone(), "rejected", reason)),
        };
        let status = match parse_status(&call.arguments) {
            Ok(status) => status,
            Err(reason) => return Ok(result(call.id.clone(), "rejected", reason)),
        };
        let Some(mut goal) = self.store.goal(self.project_id, scope.session)? else {
            return Ok(result(
                call.id.clone(),
                "unavailable",
                "the goal was cleared",
            ));
        };
        let revision = goal.accounting.as_ref().map(|account| account.revision);
        if goal.id != scope.binding.goal_id || revision != Some(scope.binding.revision) {
            return Ok(result(
                call.id.clone(),
                "unavailable",
                "the goal changed after this run started",
            ));
        }
        if goal.lifecycle != GoalLifecycle::Active {
            return Ok(result(
                call.id.clone(),
                "unavailable",
                "the goal is not active",
            ));
        }
        let expected = Some(goal.clone());
        goal.completion_report = Some(GoalCompletionReport {
            run_id: run,
            status,
            summary,
            reported_at: now(),
        });
        let (next_action, detail) = match status {
            GoalReportStatus::Complete => (
                "Confirm or dismiss the agent's completion report",
                "The owner has been asked to confirm. The goal is not complete until they do. Tell the owner what you did.",
            ),
            GoalReportStatus::Partial => (
                "Review what is left, then run the next task",
                "Recorded as partly done. The goal stays active. Tell the owner what you did and what is left.",
            ),
            GoalReportStatus::NeedsInput => (
                "Answer the agent's question",
                "Recorded as waiting for the owner. Ask the owner your question in your answer, then stop.",
            ),
        };
        goal.next_action = next_action.into();
        goal.updated_at = now();
        self.store.save_goal_if_current(&goal, &expected)?;
        self.publish_goal_snapshot(scope.session)?;
        Ok(result(call.id.clone(), "recorded", detail))
    }

    pub(super) fn confirm_goal_report(&self, goal_id: GoalId) -> Result<(), RuntimeError> {
        let session = self.session_id();
        self.settle_goal_report(session, goal_id, true)
    }

    pub(super) fn dismiss_goal_report(&self, goal_id: GoalId) -> Result<(), RuntimeError> {
        let session = self.session_id();
        self.settle_goal_report(session, goal_id, false)
    }

    fn settle_goal_report(
        &self,
        session: SessionId,
        goal_id: GoalId,
        confirmed: bool,
    ) -> Result<(), RuntimeError> {
        let mut goal = self
            .store
            .goal(self.project_id, session)?
            .ok_or(RuntimeError::GoalMissing)?;
        if goal.id != goal_id {
            return Err(RuntimeError::SubmissionTargetChanged);
        }
        let Some(report) = goal.completion_report.as_ref() else {
            return Err(RuntimeError::GoalReportMissing);
        };
        // Only a completion report can complete the goal.
        if confirmed && report.status != GoalReportStatus::Complete {
            return Err(RuntimeError::GoalReportMissing);
        }
        let expected = Some(goal.clone());
        if confirmed {
            goal.lifecycle = GoalLifecycle::Complete;
            goal.next_action = "Completed. Confirmed by the owner".into();
        } else {
            goal.completion_report = None;
            goal.next_action = "Run the next task".into();
        }
        goal.updated_at = now();
        self.store.save_goal_if_current(&goal, &expected)?;
        self.publish_goal_snapshot(session)
    }
}

#[cfg(test)]
mod tests {
    use super::{parse_status, parse_summary, MISSING_SUMMARY};
    use argentum_domain::GoalReportStatus;

    #[test]
    fn reports_say_where_the_goal_stands() {
        assert_eq!(
            parse_status(r#"{"status":"partial","summary":"Half."}"#).unwrap(),
            GoalReportStatus::Partial
        );
        assert_eq!(
            parse_status(r#"{"status":"needs_input","summary":"Which file?"}"#).unwrap(),
            GoalReportStatus::NeedsInput
        );
        // A report without a status, as older models send, is a completion.
        assert_eq!(
            parse_status(r#"{"summary":"Done."}"#).unwrap(),
            GoalReportStatus::Complete
        );
        assert!(parse_status(r#"{"status":"maybe","summary":"x"}"#).is_err());
    }

    #[test]
    fn summaries_are_bounded_and_plain() {
        assert_eq!(
            parse_summary(r#"{"summary":"  Counted to 100.  "}"#).unwrap(),
            "Counted to 100."
        );
        // A missing or blank summary still reaches the owner, with a stock note.
        assert_eq!(parse_summary("{}").unwrap(), MISSING_SUMMARY);
        assert_eq!(
            parse_summary(r#"{"summary":"   "}"#).unwrap(),
            MISSING_SUMMARY
        );
        assert!(parse_summary(r#"{"summary":"a","extra":1}"#).is_err());
        assert!(parse_summary(r#"{"summary":"bad\u0007bell"}"#).is_err());
        let long = format!(r#"{{"summary":"{}"}}"#, "x".repeat(2000));
        assert!(parse_summary(&long).is_err());
    }
}
